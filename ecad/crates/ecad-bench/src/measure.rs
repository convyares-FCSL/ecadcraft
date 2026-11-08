//! The timings (brief 012 §3). Each is the median of the runs asked for; generating the project
//! is never timed.
//!
//! # Semantics
//! - **open** and **first page** each open the file afresh in every run (from a warm file cache,
//!   taking and releasing the writer lock as the app does).
//! - **edit** and **save** run on one copy of the file, opened once and not timed: each run
//!   commits one add-wire transaction (timed as edit), then saves it (timed as save).
//! - **xref** (cable references) times brief 010's `ecad_model::derive::cable_references` on the
//!   document the open runs replayed to, as **derive** times the nets; see [`cable_references_ms`].

use std::path::{Path, PathBuf};
use std::time::Instant;

use ecad_file::{DiskProject, Outcome};
use ecad_model::{Author, Document, IdAllocator};

/// Runs per measurement unless `--runs` says otherwise.
pub const RUNS: usize = 5;
/// Most runs `--runs` accepts.
pub const MAX_RUNS: usize = 1_000;
/// Session seed used when opening (any fixed value).
const OPEN_SEED: u64 = 0xBE_4C11;
/// Seed of the IDs the edit timing allocates: a stream the generator never uses.
const EDIT_SEED: u64 = 0xED17_0012;
/// The writer the lock file names while the benchmark holds a project.
pub const HOLDER: &str = "ecad-bench";

/// What one project measured, times in milliseconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Measured {
    pub bytes: u64,
    pub transactions: u64,
    /// Read from disk, verify every record, replay to the document (`ecad-file`, no CADCraft).
    pub open_ms: f64,
    /// Nets on the opened document (`ecad_model::derive::nets`).
    pub derive_ms: f64,
    /// Cable references on the opened document (`ecad_model::derive::cable_references`, brief 010).
    pub xref_ms: Option<f64>,
    /// Open the file in a headless CADCraft session and project page 1 (`ecad-cadcraft`).
    pub first_page_ms: f64,
    /// Commit one add-wire transaction to the loaded project (`Ledger::commit`).
    pub edit_ms: f64,
    /// Append that transaction to the file and flush (`DiskProject::save`).
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

/// The time to derive the cable references of `doc` (brief 010's
/// `ecad_model::derive::cable_references`), in milliseconds. Always `Some` in this revision; the
/// `Option` keeps the report's `—` for a revision without the derivation.
pub fn cable_references_ms(doc: &Document) -> Option<f64> {
    let start = Instant::now();
    let refs = ecad_model::derive::cable_references(doc);
    let took = ms(start);
    std::hint::black_box(refs);
    Some(took)
}

/// Measure the project file at `path`, `runs` times each; `scratch` is a folder for the copy the
/// edit and save timings write to. Returns the measurements and the document opening replayed to.
///
/// # Errors
/// The file does not open cleanly, the page cannot be projected, or the edit or save fails.
pub fn measure(path: &Path, runs: usize, scratch: &Path) -> Result<(Measured, Document), String> {
    let runs = runs.clamp(1, MAX_RUNS);
    let bytes = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();

    let mut open_ms = Vec::with_capacity(runs);
    let mut opened = None;
    for _ in 0..runs {
        // Release the previous open's lock and memory before the next one.
        drop(opened.take());
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
    let mut xref_ms = Vec::with_capacity(runs);
    for _ in 0..runs {
        let t = Instant::now();
        let nets = ecad_model::derive::nets(&doc);
        derive_ms.push(ms(t));
        std::hint::black_box(nets);
        if let Some(x) = cable_references_ms(&doc) {
            xref_ms.push(x);
        }
    }

    let mut first_page_ms = Vec::with_capacity(runs);
    let text = path_text(path)?;
    for _ in 0..runs {
        first_page_ms.push(first_page(text)?);
    }

    let (mut edit_ms, mut save_ms) = edit_and_save(path, &scratch.join("edit.ecad"), runs)?;

    let m = Measured {
        bytes,
        transactions,
        open_ms: median(&mut open_ms),
        derive_ms: median(&mut derive_ms),
        xref_ms: (!xref_ms.is_empty()).then(|| median(&mut xref_ms)),
        first_page_ms: median(&mut first_page_ms),
        edit_ms: median(&mut edit_ms),
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

/// Copy `path` to `copy` and open it once (not timed). Then per run: commit one add-wire
/// transaction (timed), and save it (timed separately).
fn edit_and_save(path: &Path, copy: &Path, runs: usize) -> Result<(Vec<f64>, Vec<f64>), String> {
    remove(copy);
    std::fs::copy(path, copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        let mut o = open(copy)?;
        let mut ids = IdAllocator::seeded(EDIT_SEED);
        let (mut edit, mut save) = (Vec::with_capacity(runs), Vec::with_capacity(runs));
        for run in 0..runs {
            let ops = crate::generate::added_wire(o.ledger.document(), &mut ids, &format!("W-bench-{}", run + 1))?;
            let t = Instant::now();
            o.ledger.commit(Author::User, "Wire", ops).map_err(|e| e.to_string())?;
            edit.push(ms(t));
            let t = Instant::now();
            o.project.save(&mut o.ledger).map_err(|e| e.to_string())?;
            save.push(ms(t));
            if o.ledger.is_unsaved() {
                return Err(format!("{}: the change is still unsaved after saving", copy.display()));
            }
        }
        Ok((edit, save))
    })();
    remove(copy);
    result
}

/// Remove `path` and its lock file, if present. Only for the benchmark's own scratch files.
pub fn remove(path: &Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(ecad_store::disk::lock_path(path));
}

/// Make `path` free for a new generated project: refuse when another session holds its writer
/// lock (a project open in the trial), otherwise remove any earlier file there.
///
/// # Errors
/// Another session holds the lock, or the lock cannot be checked.
pub fn claim(path: &Path) -> Result<(), String> {
    use ecad_store::disk::{LockAttempt, WriterLock};
    match WriterLock::try_acquire(path, HOLDER) {
        Ok(LockAttempt::Acquired { lock, .. }) => {
            let _ = std::fs::remove_file(path);
            // Dropping the lock releases it.
            drop(lock);
            Ok(())
        }
        Ok(LockAttempt::HeldByOther { holder }) => Err(format!(
            "{} is open in another session ({}); close it there first, or choose another --keep folder",
            path.display(),
            holder.as_deref().unwrap_or("holder not shown")
        )),
        Err(e) => Err(format!("{}: cannot check its writer lock: {e}", path.display())),
    }
}

/// Remove the (empty) lock file the benchmark's own opens left beside `path`, once nothing holds
/// it; a lock held by another session is left alone.
pub fn release_lock_file(path: &Path) {
    use ecad_store::disk::{LockAttempt, WriterLock};
    if let Ok(LockAttempt::Acquired { lock, .. }) = WriterLock::try_acquire(path, HOLDER) {
        let lock_file = lock.path().to_path_buf();
        // Release (and close) it first, then remove the file.
        drop(lock);
        let _ = std::fs::remove_file(lock_file);
    }
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
