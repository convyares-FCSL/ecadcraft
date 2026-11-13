//! Brief 044 timings (design 044 §11.3), through the app's project on a copy of a version 2 file.
//! The connectors are added on the copy here, not by the generator, so every earlier row stays
//! comparable with earlier baselines.
//!
//! # Semantics
//! - The copy gains 200 connectors (fewer pages: as many as fit): 100 plain 4-pin pairs as rows,
//!   50 D-sub 25 connectors with the socket on one page and the plug on the next, and 50 M12 5-pin
//!   connectors drawn pin by pin (pin pairs) across 5 wires. It is saved and opened again, so the
//!   measured project starts clean.
//! - **connplace**: a D-sub 25 pair placed as rows (50 terminals), committed and updated.
//! - **connpins**: a pin line across 8 wires (split, mate), committed and updated.
//! - **connside**: the project's `connector.side.socket` changed (every connector inherits it),
//!   committed, updated, the page on screen redrawn.
//! - **pinlabel**: one pin renamed on both halves, committed and updated.
//!
//! Each edit is undone after it is measured.

use std::path::Path;
use std::time::Instant;

use cadcraft_engine::Session;
use ecad_cadcraft::ProjectHandle;
use ecad_model::{ComponentId, PageId};
use serde_json::json;

use crate::measure::{median, path_text, remove};

const OPEN_SEED: u64 = 0x0044_0000;
/// Where the bench's connectors and wires sit: right of an A3 sheet, clear of the pattern.
const X0: f64 = 700.0;

/// What brief 044 measured, in milliseconds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeasuredConnectors {
    pub connplace_ms: f64,
    pub connpins_ms: f64,
    pub connside_ms: f64,
    pub pinlabel_ms: f64,
    /// Connectors in the project.
    pub connectors: usize,
    pub pages: usize,
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn run(s: &mut Session, cmd: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
    s.execute(cmd, &params).map_err(|e| format!("{cmd} {params}: {e}"))
}

fn hex<T: From<u128>>(v: &serde_json::Value, key: &str) -> Result<T, String> {
    v[key].as_str().and_then(|t| u128::from_str_radix(t, 16).ok()).map(T::from).ok_or_else(|| format!("no `{key}` in {v}"))
}

/// `n` vertical wires on the active page at x `X0 + 10 i`, from y `y0` to `y0 - 60`.
fn wires(s: &mut Session, n: usize, y0: f64) -> Result<(), String> {
    for i in 0..n {
        let x = X0 + 10.0 * i as f64;
        run(s, "wire", json!({"points": [[x, y0], [x, y0 - 60.0]]}))?;
    }
    Ok(())
}

/// Add the 200 connectors (design 044 §11.3's population).
fn populate(s: &mut Session, pages: &[PageId]) -> Result<usize, String> {
    let mut n = 0;
    let page = |i: usize| pages.get(i % pages.len().max(1)).copied();
    for i in 0..100 {
        let Some(p) = page(i) else { break };
        run(
            s,
            "connector",
            json!({"halves": "both", "family": "plain", "pins": 4, "layout": "row", "at": [X0, 380.0 - 20.0 * (i / pages.len()) as f64], "page": p.hex()}),
        )?;
        n += 1;
    }
    for i in 0..50 {
        let (Some(a), Some(b)) = (page(2 * i), page(2 * i + 1)) else { break };
        let v = run(s, "connector", json!({"halves": "socket", "family": "dsub", "pins": 25, "layout": "row", "at": [X0, 320.0], "page": a.hex()}))?;
        let c: ComponentId = hex(&v, "component")?;
        run(s, "connector", json!({"halves": "plug", "device": c.hex(), "layout": "row", "at": [X0, 300.0], "page": b.hex()}))?;
        n += 1;
    }
    for i in 0..50 {
        let Some(p) = page(i) else { break };
        run(s, "openpage", json!({"page": p.hex()}))?;
        let y0 = 260.0 - 70.0 * (i / pages.len()) as f64;
        wires(s, 5, y0)?;
        run(
            s,
            "connector",
            json!({"halves": "both", "family": "m12", "pins": 5, "layout": "pins", "line": [[X0 - 5.0, y0 - 30.0], [X0 + 45.0, y0 - 30.0]]}),
        )?;
        n += 1;
    }
    Ok(n)
}

/// Measure brief 044 on a copy of `path` (in `scratch`), `runs` times each.
///
/// # Errors
/// Opening, a refused command, or a project with no pages.
pub fn measure(path: &Path, runs: usize, scratch: &Path) -> Result<MeasuredConnectors, String> {
    let _ = ecad_cadcraft::install();
    let copy = scratch.join("connectors.ecad");
    remove(&copy);
    std::fs::copy(path, &copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        let mut out = MeasuredConnectors::default();
        {
            let mut s = Session::empty();
            let (project, _) = ProjectHandle::open_file_in(&mut s, path_text(&copy)?, OPEN_SEED, ecad_cadcraft::views::Settings::none())?;
            let pages: Vec<PageId> = project.read(&mut s, |p, _| p.ledger.document().page_order.clone())?;
            let first = *pages.first().ok_or("the project has no pages")?;
            run(&mut s, "openpage", json!({"page": first.hex()}))?;
            out.connectors = populate(&mut s, &pages)?;
            out.pages = pages.len();
            drop(project);
        }
        crate::measure::release_lock_file(&copy);
        let mut s = Session::empty();
        let (project, _) = ProjectHandle::open_file_in(&mut s, path_text(&copy)?, OPEN_SEED + 1, ecad_cadcraft::views::Settings::none())?;
        let page1 = project.read(&mut s, |p, _| p.ledger.document().page_order.first().copied())?.ok_or("the project has no pages")?;
        run(&mut s, "openpage", json!({"page": page1.hex()}))?;
        // The report models built first, as on first show (untimed).
        project.configure(&mut s, |p| {
            p.outputs();
        })?;
        let mut place = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            run(&mut s, "connector", json!({"halves": "both", "family": "dsub", "pins": 25, "layout": "row", "at": [X0, 120.0]}))?;
            place.push(ms(t));
            run(&mut s, "undo", json!({}))?;
        }
        out.connplace_ms = median(&mut place);
        wires(&mut s, 8, 100.0)?;
        let mut pins = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            run(
                &mut s,
                "connector",
                json!({"halves": "both", "family": "plain", "pins": 8, "layout": "pins", "line": [[X0 - 5.0, 70.0], [X0 + 75.0, 70.0]]}),
            )?;
            pins.push(ms(t));
            run(&mut s, "undo", json!({}))?;
        }
        out.connpins_ms = median(&mut pins);
        let mut side = Vec::new();
        for i in 0..runs {
            let t = Instant::now();
            run(&mut s, "setdefault", json!({"key": "connector.side.socket", "value": if i % 2 == 0 { "F" } else { "B" }}))?;
            side.push(ms(t));
            run(&mut s, "undo", json!({}))?;
        }
        out.connside_ms = median(&mut side);
        let c = project
            .read(&mut s, |p, _| {
                let doc = p.ledger.document();
                doc.components.keys().copied().find(|c| ecad_cadcraft::connectors::is_connector(doc, *c))
            })?
            .ok_or("no connector was added")?;
        let mut label = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            run(&mut s, "pinlabel", json!({"device": c.hex(), "pin": "1", "to": "PE"}))?;
            label.push(ms(t));
            run(&mut s, "undo", json!({}))?;
        }
        out.pinlabel_ms = median(&mut label);
        Ok(out)
    })();
    remove(&copy);
    result
}
