//! Page timings, through the app's project on a copy of a version 2 file.
//!
//! # Semantics
//! - **newpage**: `newpage {type: "network", after}` after the densest page.
//! - **renumber**: `renumberpages {}` with a folder of start 100 holding half the pages (on
//!   request; reported, no limit).
//! - **typesheet**: `setstyle {catalogue: "page", id: "schematic", fields: {sheet}}` with the
//!   densest page on screen (every schematic page's sheet and the references to items on them).
//! - The cover rows: the cover is in the book but another page is on screen (one page window); a
//!   change it shows marks it stale, and the cost measured is its update map and its wire-category
//!   index.
//! - **coverupd**: the second copy of a symbol placed on another page
//!   (the first changed the legend; the second must not redraw it). **coverfirst** is the first.
//! - **wirecat**: a wire drawn on another page and given the `dc24`
//!   category (the first of that category changes the legend).

use std::path::Path;
use std::time::Instant;

use cadcraft_engine::Session;
use ecad_engine::ProjectHandle;
use ecad_model::PageId;
use serde_json::json;

use crate::measure::{median, path_text, remove};

const SEED: u64 = 0x0054_0000;

/// What the page timings measured, in milliseconds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeasuredPages {
    pub pages: usize,
    pub newpage_ms: f64,
    pub renumber_ms: f64,
    pub typesheet_ms: f64,
    pub coverfirst_ms: f64,
    pub coverupd_ms: f64,
    pub wirecat_ms: f64,
}

fn exec(s: &mut Session, cmd: &str, params: serde_json::Value) -> Result<(f64, serde_json::Value), String> {
    let t = Instant::now();
    let v = s.execute(cmd, &params).map_err(|e| format!("{cmd}: {e}"))?;
    Ok((t.elapsed().as_secs_f64() * 1e3, v))
}

/// Measure the page rows on a copy of `path`.
///
/// # Errors
/// The copy cannot be made or opened, or a command fails.
pub fn measure(path: &Path, runs: usize, scratch: &Path) -> Result<MeasuredPages, String> {
    let _ = ecad_engine::install();
    let copy = scratch.join("pages.ecad");
    remove(&copy);
    std::fs::copy(path, &copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let mut out = MeasuredPages::default();
    let result = (|| {
        let mut s = Session::empty();
        let (project, _) =
            ProjectHandle::open_file_in(&mut s, path_text(&copy)?, SEED, ecad_engine::cables::views::Settings::none()).map_err(|e| e.to_string())?;
        let (pages, densest) = project.read(&mut s, |p, _| {
            let doc = p.ledger.document();
            let d = doc.page_order().iter().copied().max_by_key(|pg| doc.representations_on(*pg).count()).unwrap_or(PageId(0));
            (doc.page_order().to_vec(), d)
        })?;
        out.pages = pages.len();
        project.open_page(&mut s, densest)?;
        // newpage after the densest page, undone each time.
        let mut v = Vec::new();
        for _ in 0..runs.max(1) {
            v.push(exec(&mut s, "newpage", json!({"type": "network", "after": densest.hex()}))?.0);
            s.execute("u", &json!({})).map_err(|e| e.to_string())?;
            project.open_page(&mut s, densest)?;
        }
        out.newpage_ms = median(&mut v);
        // typesheet with the densest page on screen.
        let mut v = Vec::new();
        for i in 0..runs.max(1) {
            let sheet = if i % 2 == 0 { "A2" } else { "A3" };
            v.push(exec(&mut s, "setstyle", json!({"catalogue": "page", "id": "schematic", "fields": {"sheet": sheet}}))?.0);
        }
        out.typesheet_ms = median(&mut v);
        // renumber: half the pages in a folder starting at 100.
        let (_, f) = exec(&mut s, "newfolder", json!({"name": "Bench"}))?;
        let folder = f.get("folder").and_then(|x| x.as_str()).unwrap_or_default().to_string();
        exec(&mut s, "folderprops", json!({"folder": folder, "start": "100"}))?;
        for p in pages.iter().take(pages.len() / 2) {
            exec(&mut s, "movepage", json!({"page": p.hex(), "folder": folder}))?;
        }
        let mut v = Vec::new();
        for _ in 0..runs.max(1) {
            v.push(exec(&mut s, "renumberpages", json!({}))?.0);
            s.execute("u", &json!({})).map_err(|e| e.to_string())?;
        }
        out.renumber_ms = median(&mut v);
        // The cover on screen; symbols and wires go on the densest page without showing it.
        let (_, c) = exec(&mut s, "newpage", json!({"type": "front", "generated": "cover"}))?;
        let _ = c;
        let (mut first, mut second, mut wire) = (Vec::new(), Vec::new(), Vec::new());
        for i in 0..runs.max(1) {
            project.open_page(&mut s, densest)?;
            let x = 30.0 + 20.0 * i as f64;
            first.push(exec(&mut s, "place", json!({"symbol": "sel", "at": [x, 260.0]}))?.0);
            second.push(exec(&mut s, "place", json!({"symbol": "sel", "at": [x + 10.0, 260.0]}))?.0);
            let (_, w) = exec(&mut s, "wire", json!({"points": [[x, 250.0], [x + 15.0, 250.0]]}))?;
            let c = w.get("conductor").and_then(|x| x.as_str()).unwrap_or_default().to_string();
            wire.push(exec(&mut s, "category", json!({"conductor": c, "value": "dc24"}))?.0);
            for _ in 0..4 {
                s.execute("u", &json!({})).map_err(|e| e.to_string())?;
            }
        }
        out.coverfirst_ms = median(&mut first);
        out.coverupd_ms = median(&mut second);
        out.wirecat_ms = median(&mut wire);
        drop(project);
        drop(s);
        Ok(out.clone())
    })();
    crate::measure::release_lock_file(&copy);
    remove(&copy);
    result
}
