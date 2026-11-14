//! Brief 047 timings (design 047 §10), run inside [`crate::outputs::measure`] with the report
//! models kept and a BOM-by-location page in the book, so every edit goes through the BOM's
//! update map.
//!
//! # Semantics
//! - **bomline**: `partline add` on one device (then the line removed, untimed).
//! - **bomkit**: the same aux-block line on 50 devices in one transaction (then undone).
//! - **bomitem**: an item added (then undone).
//! - **bomart**: `article` on a part used by 200 entries.
//! - **bomcols**: `bom.column.article` toggled with the BOM page on screen.
//! - **verify**: after the steps the kept BOM equals a rebuild in every layout (a difference fails
//!   the bench).

use std::time::Instant;

use cadcraft_engine::Session;
use ecad_cadcraft::ProjectHandle;
use ecad_cadcraft::outputs::{BomView, Group, Outputs};
use ecad_model::{ComponentId, PageId};
use serde_json::json;

use crate::measure::median;

/// What brief 047 measured, in milliseconds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeasuredBom {
    pub bomline_ms: f64,
    pub bomkit_ms: f64,
    pub bomitem_ms: f64,
    pub bomart_ms: f64,
    pub bomcols_ms: f64,
    /// Entries `article` set, and the BOM page's sheets.
    pub art_entries: usize,
    pub sheets: usize,
}

fn run(s: &mut Session, cmd: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
    s.execute(cmd, &params).map_err(|e| format!("{cmd} {params}: {e}"))
}

fn timed(s: &mut Session, cmd: &str, params: serde_json::Value) -> Result<f64, String> {
    let t = Instant::now();
    run(s, cmd, params)?;
    Ok(t.elapsed().as_secs_f64() * 1000.0)
}

/// Measure brief 047 on the open `project` (models kept), `devices` its devices, `bom_page` its
/// BOM page, `runs` times each.
///
/// # Errors
/// A refused command, too few devices, or the kept BOM differs from a rebuild.
pub fn measure(s: &mut Session, project: &ProjectHandle, devices: &[ComponentId], bom_page: PageId, runs: usize) -> Result<MeasuredBom, String> {
    let mut out = MeasuredBom::default();
    let one = devices.get(100).or(devices.first()).ok_or("no device")?.hex();
    let fifty: Vec<String> = devices.iter().take(50).map(|c| c.hex()).collect();
    let line = json!({"part_number": "AB-1NO", "maker": "Generic", "description": "Aux block 1NO"});
    let mut v = Vec::new();
    for _ in 0..runs {
        v.push(timed(s, "partline", json!({"component": one, "add": line}))?);
        run(s, "undo", json!({}))?;
    }
    out.bomline_ms = median(&mut v);
    let mut v = Vec::new();
    for _ in 0..runs {
        v.push(timed(s, "partline", json!({"components": fifty, "add": line}))?);
        run(s, "undo", json!({}))?;
    }
    out.bomkit_ms = median(&mut v);
    let mut v = Vec::new();
    for i in 0..runs {
        v.push(timed(s, "bomitem", json!({"description": "Slotted trunking 50x75", "qty": format!("{}.5", i + 1), "unit": "m"}))?);
        run(s, "undo", json!({}))?;
    }
    out.bomitem_ms = median(&mut v);
    // A part used by 200 entries: the line on 200 devices (in batches), then `article`.
    for chunk in devices.iter().take(200).collect::<Vec<_>>().chunks(50) {
        let cs: Vec<String> = chunk.iter().map(|c| c.hex()).collect();
        run(s, "partline", json!({"components": cs, "add": line}))?;
    }
    let mut v = Vec::new();
    for i in 0..runs {
        let t = Instant::now();
        let r = run(s, "article", json!({"part_number": "AB-1NO", "article_number": format!("A-{i:04}")}))?;
        v.push(t.elapsed().as_secs_f64() * 1000.0);
        out.art_entries = usize::try_from(r["parts"].as_u64().unwrap_or(0)).unwrap_or(0);
    }
    out.bomart_ms = median(&mut v);
    run(s, "openpage", json!({"page": bom_page.hex()}))?;
    out.sheets = project.read(s, |p, _| p.outputs.as_ref().map_or(0, Outputs::bom_sheets))?;
    let mut v = Vec::new();
    for i in 0..runs {
        let value = if i % 2 == 0 { "on" } else { "off" };
        v.push(timed(s, "setdefault", json!({"key": "bom.column.article", "level": "project", "value": value}))?);
    }
    out.bomcols_ms = median(&mut v);
    // Verify mode: the kept model against a rebuild, every layout.
    let same = project.configure(s, |p| {
        let kept = p.outputs().clone();
        let built = Outputs::build(p.ledger.document());
        [false, true].iter().all(|a| {
            [false, true].iter().all(|n| {
                let v = BomView { article: *a, nested: *n };
                [Group::Location, Group::Part].iter().all(|g| kept.bom_table_with(*g, v, true) == built.bom_table_with(*g, v, true))
            })
        }) && kept.check_counts() == built.check_counts()
            && kept.bom_sheets() == built.bom_sheets()
    })?;
    if !same {
        return Err("brief 047: the kept BOM differs from a rebuild".into());
    }
    Ok(out)
}
