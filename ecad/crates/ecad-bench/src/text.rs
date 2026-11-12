//! Brief 031 timings (design 031 §12), through the app's project on a copy of the version 2 file.
//!
//! # Semantics
//! - **textplace**: every derived text of the densest page (most symbols) worked out, placement
//!   included (`devices::all_texts_on_page`), as each draw of that page does. Target: medium no
//!   more than 15 % over `dev`'s `texts_on_page` on the same page; large at most 25 ms.
//! - **textstyle**: `textstyle {kind: tag, height}` committed with that page on screen (the commit
//!   redraws it). Target (large): at most 1.2 × **propredraw**.
//! - **propredraw**: the baseline, an ordinary property of a device on that page (`setprop maker`)
//!   committed with the page on screen.

use std::path::Path;
use std::time::Instant;

use cadcraft_engine::Session;
use ecad_cadcraft::ProjectHandle;
use ecad_model::{PageId, Subject};
use serde_json::json;

use crate::measure::{median, path_text, remove};

const OPEN_SEED: u64 = 0x0031_0000;

/// What brief 031 measured, in milliseconds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeasuredText {
    pub textplace_ms: f64,
    pub textstyle_ms: f64,
    pub propredraw_ms: f64,
    /// Symbols and derived texts on the densest page.
    pub symbols: usize,
    pub texts: usize,
}

/// Measure brief 031's rows on a copy of `path`.
///
/// # Errors
/// The copy cannot be made or opened, or a command fails.
pub fn measure(path: &Path, runs: usize, scratch: &Path) -> Result<MeasuredText, String> {
    let copy = scratch.join("text.ecad");
    remove(&copy);
    std::fs::copy(path, &copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        let mut s = Session::empty();
        let (project, _) = ProjectHandle::open_file_in(&mut s, path_text(&copy)?, OPEN_SEED, ecad_cadcraft::views::Settings::none())?;
        let (page, symbols, device) = project.read(&mut s, |p, _| {
            let doc = p.ledger.document();
            let page: Option<PageId> = doc.page_order.iter().copied().max_by_key(|pg| doc.representations_on(*pg).count());
            let page = page.unwrap_or(PageId(0));
            let device = doc.representations_on(page).find_map(|(_, r)| match r.subject {
                Subject::Component(c) => Some(c),
                _ => None,
            });
            (page, doc.representations_on(page).count(), device)
        })?;
        let mut place = Vec::new();
        let mut texts = 0;
        for _ in 0..runs.max(1) {
            let ms = project.read(&mut s, |p, _| {
                let t = Instant::now();
                let n = ecad_cadcraft::devices::all_texts_on_page(p.ledger.document(), page).len();
                (t.elapsed().as_secs_f64() * 1e3, n)
            })?;
            texts = ms.1;
            place.push(ms.0);
        }
        project.open_page(&mut s, page)?;
        let mut style = Vec::new();
        let mut prop = Vec::new();
        for i in 0..runs.max(1) {
            let h = if i % 2 == 0 { 3.0 } else { 2.5 };
            let t = Instant::now();
            s.execute("textstyle", &json!({"kind": "tag", "height": h})).map_err(|e| e.to_string())?;
            style.push(t.elapsed().as_secs_f64() * 1e3);
            if let Some(c) = device {
                let t = Instant::now();
                s.execute("setprop", &json!({"component": c.hex(), "key": "maker", "value": format!("M{i}")})).map_err(|e| e.to_string())?;
                prop.push(t.elapsed().as_secs_f64() * 1e3);
            }
        }
        Ok(MeasuredText { textplace_ms: median(&mut place), textstyle_ms: median(&mut style), propredraw_ms: median(&mut prop), symbols, texts })
    })();
    crate::measure::release_lock_file(&copy);
    remove(&copy);
    result
}
