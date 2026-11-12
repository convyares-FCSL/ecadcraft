//! Brief 027 timings (design 027 §11.2), through the app's project on a copy of the version 2
//! file, with the Checks and report models kept first (as the Release dialog keeps them).
//!
//! # Semantics
//! - **prerelease**: the Release dialog's check of the pending revision (`prerelease {}`), the
//!   kept lists warm. Target ≤ 100 ms on large.
//! - **orphanupd**: one cable reference end erased (its partner becomes an orphan), committed with
//!   the Checks kept, then undone; **orphanwork**: of that, the orphan rows re-derived for the
//!   cable (`checks::reference_checks`), the only work 027 adds to the edit (target: under 1 ms
//!   over the same edit without it).
//! - **release**: `release` of the pending revision: the whole book and the lists to a staging
//!   folder, the issue, the save, the manifest and the rename (once; reported).
//! - **rerelease**: `release {rev}` of that revision again, its state and report models built on
//!   request (once; reported).

use std::path::Path;
use std::time::Instant;

use cadcraft_engine::Session;
use ecad_cadcraft::ProjectHandle;
use ecad_model::Subject;
use serde_json::json;

use crate::measure::{median, path_text, remove};

const OPEN_SEED: u64 = 0x0027_0000;

/// What brief 027 measured: milliseconds and seconds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeasuredRelease {
    pub prerelease_ms: f64,
    pub orphanupd_ms: f64,
    pub orphanwork_ms: f64,
    pub release_s: f64,
    pub rerelease_s: f64,
    pub sheets: usize,
    pub files: usize,
    /// Rows in the check.
    pub rows: usize,
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn run(s: &mut Session, cmd: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
    s.execute(cmd, &params).map_err(|e| format!("{cmd} {params}: {e}"))
}

/// Measure brief 027 on a copy of `path` (in `scratch`), `runs` times each (the release once).
///
/// # Errors
/// Opening, a refused command, or nothing to measure.
pub fn measure(path: &Path, runs: usize, scratch: &Path) -> Result<MeasuredRelease, String> {
    let dir = scratch.join("release-bench");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let copy = dir.join("release.ecad");
    std::fs::copy(path, &copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        ecad_cadcraft::install();
        ecad_cadcraft::printbook::install_io();
        let mut s = Session::empty();
        let (project, _) = ProjectHandle::open_file_in(&mut s, path_text(&copy)?, OPEN_SEED, ecad_cadcraft::views::Settings::none())?;
        let mut out = MeasuredRelease::default();
        run(&mut s, "revision", json!({"description": "Bench", "drawn": "BEN"}))?;
        // The kept lists, as on the dialog's first show.
        project.configure(&mut s, |p| {
            p.checks();
            p.outputs();
        })?;
        let mut pre = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            let v = run(&mut s, "prerelease", json!({}))?;
            pre.push(ms(t));
            out.rows = v["groups"].as_array().map_or(0, |g| g.iter().filter_map(|x| x["count"].as_u64()).sum::<u64>() as usize);
        }
        out.prerelease_ms = median(&mut pre);
        // A reference end of a continued cable, on its page.
        let found = project.read(&mut s, |p, _| {
            let doc = p.ledger.document();
            doc.representations.values().find_map(|r| match r.subject {
                Subject::Cable(k) if ecad_cadcraft::references::is_reference(r) => Some((k, r.page, *r.items.first()?)),
                _ => None,
            })
        })?;
        let (cable, page, item) = found.ok_or("the project has no cable reference end")?;
        run(&mut s, "openpage", json!({"page": page.hex()}))?;
        let mut upd = Vec::new();
        let mut work = Vec::new();
        for _ in 0..runs {
            let h = project.read(&mut s, |p, _| p.pages.get(&page).and_then(|v| v.map.item(item)).map(|h| h.hex()))?.ok_or("no handle")?;
            let t = Instant::now();
            run(&mut s, "erase", json!({"handles": [h]}))?;
            upd.push(ms(t));
            work.push(project.read(&mut s, |p, _| {
                let t = Instant::now();
                std::hint::black_box(ecad_cadcraft::checks::reference_checks(p.ledger.document(), cable));
                ms(t)
            })?);
            run(&mut s, "undo", json!({}))?;
        }
        out.orphanupd_ms = median(&mut upd);
        out.orphanwork_ms = median(&mut work);
        let t = Instant::now();
        let v = run(&mut s, "release", json!({"accept": "errors", "provisional": "issue"}))?;
        out.release_s = t.elapsed().as_secs_f64();
        let files = v["files"].as_array().cloned().unwrap_or_default();
        out.files = files.len();
        out.sheets = files.iter().filter_map(|f| f["sheets"].as_u64()).sum::<u64>() as usize;
        let rev = v["rev"].as_str().unwrap_or("A").to_string();
        let t = Instant::now();
        run(&mut s, "release", json!({"rev": rev, "existing": "keep-both"}))?;
        out.rerelease_s = t.elapsed().as_secs_f64();
        drop(project);
        drop(s);
        Ok(out)
    })();
    crate::measure::release_lock_file(&copy);
    remove(&copy);
    let _ = std::fs::remove_dir_all(&dir);
    result
}
