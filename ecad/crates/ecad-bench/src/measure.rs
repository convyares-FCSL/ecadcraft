//! The timings (brief 012 §3). Each is the median of [`RUNS`] runs; generating the project is
//! never timed.

use std::path::{Path, PathBuf};
use std::time::Instant;

use ecad_file::{DiskProject, Outcome};
use ecad_model::{Author, Document, ObjectRef, Op};

/// Runs per measurement.
pub const RUNS: usize = 5;
/// Session seed used when opening (any fixed value).
const OPEN_SEED: u64 = 0xBE_4C11;
/// The writer the lock file names while the benchmark holds a project.
const HOLDER: &str = "ecad-bench";

/// What one project measured, times in milliseconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Measured {
    pub bytes: u64,
    pub transactions: u64,
    /// Read from disk, verify every record, replay to the document (`ecad-file`, no CADCraft).
    pub open_ms: f64,
    /// Nets on the opened document (`ecad_model::derive::nets`).
    pub derive_ms: f64,
    /// Open the file in a headless CADCraft session and project page 1 (`ecad-cadcraft`).
    pub first_page_ms: f64,
    /// Append one small transaction to the open project's file and flush (`DiskProject::save`).
    pub save_ms: f64,
}

/// The median of `samples` (0 for none).
pub fn median(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples.get(samples.len() / 2).copied().unwrap_or(0.0)
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

fn path_text(path: &Path) -> Result<&str, String> {
    path.to_str().ok_or_else(|| format!("{} is not valid Unicode", path.display()))
}

/// Open `path` as the app's file layer does; refuse anything but a clean, writable open.
fn open(path: &Path) -> Result<ecad_file::OpenedDisk, String> {
    let o = DiskProject::open(path, OPEN_SEED, HOLDER).map_err(|e| e.to_string())?;
    if o.report.outcome != Outcome::Clean {
        return Err(format!("{} did not open cleanly: {}", path.display(), o.report.describe()));
    }
    if let Some(r) = o.project.read_only() {
        return Err(format!("{} opened read-only: {r}", path.display()));
    }
    Ok(o)
}

/// Measure the project file at `path`, `runs` times each; `scratch` is a folder for the copies
/// the save timing writes to. Returns the measurements and the document opening replayed to.
///
/// # Errors
/// The file does not open cleanly, the page cannot be projected or the save fails.
pub fn measure(path: &Path, runs: usize, scratch: &Path) -> Result<(Measured, Document), String> {
    let runs = runs.max(1);
    let bytes = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();

    let mut open_ms = Vec::with_capacity(runs);
    let mut opened = None;
    for _ in 0..runs {
        let t = Instant::now();
        let o = open(path)?;
        open_ms.push(ms(t));
        opened = Some(o.ledger);
    }
    let ledger = opened.ok_or("nothing was opened")?;
    let transactions = ledger.head();
    let doc = ledger.document().clone();
    drop(ledger);

    let mut derive_ms = Vec::with_capacity(runs);
    for _ in 0..runs {
        let t = Instant::now();
        let nets = ecad_model::derive::nets(&doc);
        derive_ms.push(ms(t));
        std::hint::black_box(nets);
    }

    let mut first_page_ms = Vec::with_capacity(runs);
    let text = path_text(path)?;
    for _ in 0..runs {
        first_page_ms.push(first_page(text)?);
    }

    let mut save_ms = Vec::with_capacity(runs);
    for run in 0..runs {
        save_ms.push(save_one_change(path, &scratch.join(format!("save-{run}.ecad")))?);
    }

    let m = Measured {
        bytes,
        transactions,
        open_ms: median(&mut open_ms),
        derive_ms: median(&mut derive_ms),
        first_page_ms: median(&mut first_page_ms),
        save_ms: median(&mut save_ms),
    };
    Ok((m, doc))
}

/// Time from nothing to page 1 projected in a fresh headless session.
fn first_page(path: &str) -> Result<f64, String> {
    let mut s = cadcraft_engine::Session::empty();
    let t = Instant::now();
    let (project, _) =
        ecad_cadcraft::ProjectHandle::open_file_in(&mut s, path, OPEN_SEED, ecad_cadcraft::views::Settings::none())?;
    let elapsed = ms(t);
    let (pages, read_only) = project.read(&mut s, |p, _| (p.open_pages(), p.file_status().read_only))?;
    if pages != 1 {
        return Err(format!("{path}: expected page 1 open, found {pages} open pages"));
    }
    if let Some(r) = read_only {
        return Err(format!("{path} opened read-only in the session: {r}"));
    }
    // Dropping the session and the handle releases the project and its writer lock.
    drop(project);
    drop(s);
    Ok(elapsed)
}

/// Copy `path` to `copy`, open it (not timed), rename one component (not timed), then time the
/// save that appends that one transaction and flushes.
fn save_one_change(path: &Path, copy: &Path) -> Result<f64, String> {
    remove(copy);
    std::fs::copy(path, copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        let mut o = open(copy)?;
        let doc = o.ledger.document();
        let (id, c) = doc.components.iter().next().ok_or("the project has no component to rename")?;
        let before = c.properties.get("tag").cloned();
        let after = Some(format!("{} (saved)", before.as_deref().unwrap_or("")));
        let op = Op::SetProperty { object: ObjectRef::Component(*id), key: "tag".into(), before, after };
        o.ledger.commit(Author::User, "Rename", vec![op]).map_err(|e| e.to_string())?;
        let t = Instant::now();
        o.project.save(&mut o.ledger).map_err(|e| e.to_string())?;
        let elapsed = ms(t);
        if o.ledger.is_unsaved() {
            return Err(format!("{}: the change is still unsaved after saving", copy.display()));
        }
        Ok(elapsed)
    })();
    remove(copy);
    result
}

/// Remove `path` and its lock file, if present.
pub fn remove(path: &Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(ecad_store::disk::lock_path(path));
}

/// A fresh scratch folder under the system temporary folder.
///
/// # Errors
/// It cannot be created.
pub fn scratch_dir(tag: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("ecad-bench-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}
