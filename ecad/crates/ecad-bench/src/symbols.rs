//! Brief 048 timings (design 048 §11.3), through the app's project on a copy of a version 2 file.
//! The project symbols and their copies are added on the copy here, not by the generator, so every
//! earlier row stays comparable with earlier baselines.
//!
//! # Semantics
//! - The copy gains one small project symbol placed 500 times (spread over the pages, at most 10
//!   per page) and 49 more project symbols, so 50 lineages.
//! - **symbolnew**: a symbol of 200 shapes and 40 points saved (then undone).
//! - **symbolimpact**: the impact of moving one point on the lineage with 500 copies (read only).
//! - **symboledit**: that edit saved with `update: all` (then undone).
//! - **wiredraw**: an ordinary wire drawn with the 50 lineages in the project (then undone).

use std::path::Path;
use std::time::Instant;

use cadcraft_engine::Session;
use ecad_cadcraft::ProjectHandle;
use ecad_model::PageId;
use serde_json::{Value, json};

use crate::measure::{median, path_text, remove};

const OPEN_SEED: u64 = 0x0048_0000;
/// Where the bench's symbols sit: right of an A3 sheet, clear of the pattern.
const X0: f64 = 700.0;

/// What brief 048 measured, in milliseconds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeasuredSymbols {
    pub symbolnew_ms: f64,
    pub symbolimpact_ms: f64,
    pub symboledit_ms: f64,
    pub wiredraw_ms: f64,
    /// Placed copies of the measured lineage, and the pages they are on.
    pub copies: usize,
    pub pages: usize,
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn run(s: &mut Session, cmd: &str, params: Value) -> Result<Value, String> {
    s.execute(cmd, &params).map_err(|e| format!("{cmd}: {e}"))
}

/// A symbol `key` with `shapes` lines and `points` points on the 5 mm grid; point 2 at
/// (10, `second_y`).
fn content(key: &str, shapes: usize, points: usize, second_y: f64) -> Value {
    let graphics: Vec<Value> = (0..shapes).map(|i| json!({"kind": "line", "a": [0, i as f64 * 0.5], "b": [10, i as f64 * 0.5]})).collect();
    let mut pts: Vec<Value> = (0..points).map(|i| json!({"key": (i + 1).to_string(), "at": [0, -5.0 * (i as f64 + 1.0)]})).collect();
    if let Some(p) = pts.get_mut(1) {
        p["at"] = json!([10, second_y]);
    }
    json!({"name": format!("Bench {key}"), "key": key, "graphics": graphics, "points": pts,
        "slots": [{"text": "tag", "at": [-5, 0], "h": "right"}], "defaults": {"prefix": "A"}})
}

/// Measure brief 048 on a copy of `path` (in `scratch`), `runs` times each.
///
/// # Errors
/// Opening, a refused command, or a project with no pages.
pub fn measure(path: &Path, runs: usize, scratch: &Path) -> Result<MeasuredSymbols, String> {
    let _ = ecad_cadcraft::install();
    let copy = scratch.join("symbols.ecad");
    remove(&copy);
    std::fs::copy(path, &copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        let mut out = MeasuredSymbols::default();
        let mut s = Session::empty();
        let (project, _) = ProjectHandle::open_file_in(&mut s, path_text(&copy)?, OPEN_SEED, ecad_cadcraft::views::Settings::none())?;
        let pages: Vec<PageId> = project.read(&mut s, |p, _| p.ledger.document().page_order.clone())?;
        let page1 = *pages.first().ok_or("the project has no pages")?;
        // The measured lineage, placed 500 times: at most 10 per page.
        let v = run(&mut s, "symbolnew", content("bench0", 4, 2, -5.0))?;
        let def = v["definition"].as_str().ok_or("no definition")?.to_string();
        let mut used = std::collections::BTreeSet::new();
        let n = pages.len().max(1);
        for i in 0..500usize {
            let Some(p) = pages.get(i / 10 % n) else { break };
            if i % 10 == 0 {
                run(&mut s, "openpage", json!({"page": p.hex()}))?;
            }
            let row = (i / 10 / n) * 10 + i % 10;
            run(&mut s, "place", json!({"symbol": "bench0", "at": [X0 + 30.0 * (row % 5) as f64, 380.0 - 30.0 * (row / 5) as f64]}))?;
            used.insert(*p);
        }
        out.copies = 500;
        out.pages = used.len();
        for k in 1..50 {
            run(&mut s, "symbolnew", content(&format!("bench{k}"), 4, 2, -5.0))?;
        }
        run(&mut s, "openpage", json!({"page": page1.hex()}))?;
        project.configure(&mut s, |p| {
            p.outputs();
        })?;
        let mut v = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            run(&mut s, "symbolnew", content("big", 200, 40, -5.0))?;
            v.push(ms(t));
            run(&mut s, "undo", json!({}))?;
        }
        out.symbolnew_ms = median(&mut v);
        let mut edit = content("bench0", 4, 2, -10.0);
        edit["definition"] = json!(def);
        let mut v = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            let r = run(&mut s, "symbolimpact", edit.clone())?;
            v.push(ms(t));
            if r["copies"] != json!(500) {
                return Err(format!("the impact lists {} copies, not 500", r["copies"]));
            }
        }
        out.symbolimpact_ms = median(&mut v);
        edit["update"] = json!("all");
        let mut v = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            run(&mut s, "symboledit", edit.clone())?;
            v.push(ms(t));
            run(&mut s, "undo", json!({}))?;
        }
        out.symboledit_ms = median(&mut v);
        let mut v = Vec::new();
        for i in 0..runs {
            let x = X0 + 200.0 + i as f64 * 5.0;
            let t = Instant::now();
            run(&mut s, "wire", json!({"points": [[x, 50.0], [x, 10.0]]}))?;
            v.push(ms(t));
            run(&mut s, "undo", json!({}))?;
        }
        out.wiredraw_ms = median(&mut v);
        Ok(out)
    })();
    remove(&copy);
    result
}
