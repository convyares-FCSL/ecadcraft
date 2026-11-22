//! Brief 059 timings (design 059 §12), through the app's project on a copy of a version 2 file,
//! with the Checks list and the reports kept (as when Project ▸ Cables is shown).
//!
//! # Semantics
//! - **cablenew**: `cablenew {}`, until the list row and Checks are current. Budget (large): under
//!   2 ms of update after the commit.
//! - **cableedit**: `cableedit {set: {length}}` on one cable. Budget (large): under 2 ms.
//! - **cableassign**: `cableslash` of 7 fresh wires onto a cable not drawn (`7G`). Budget (large):
//!   at most **cableslash** (the same slash making a new cable) plus 1 ms.
//!
//! The rows measure the whole command (commit and the kept views' updates), so they are upper
//! bounds of the update alone.

use std::path::Path;
use std::time::Instant;

use cadcraft_engine::Session;
use ecad_engine::ProjectHandle;
use serde_json::{Value, json};

use crate::measure::{median, path_text, remove};

const OPEN_SEED: u64 = 0x0059_0000;
/// Where the fresh wires go: beyond every tier's drawing.
const X0: f64 = 20_000.0;

/// What brief 059 measured.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeasuredCables {
    pub cables: usize,
    pub cablenew_ms: f64,
    pub cableedit_ms: f64,
    pub cableassign_ms: f64,
    pub cableslash_ms: f64,
}

fn exec(s: &mut Session, cmd: &str, params: Value) -> Result<(f64, Value), String> {
    let t = Instant::now();
    let v = s.execute(cmd, &params).map_err(|e| format!("{cmd}: {e}"))?;
    Ok((t.elapsed().as_secs_f64() * 1e3, v))
}

/// Seven fresh vertical wires at `x0`, and the slash across them.
fn seven(s: &mut Session, x0: f64) -> Result<Value, String> {
    for i in 0..7 {
        let x = x0 + 5.0 * f64::from(i);
        exec(s, "wire", json!({"points": [[x, 0.0], [x, 40.0]]}))?;
    }
    Ok(json!({"from": [x0 - 2.0, 20.0], "to": [x0 + 32.0, 20.0]}))
}

/// Measure brief 059's rows on a copy of `path`.
///
/// # Errors
/// A copy cannot be made or opened, or a command fails.
pub fn measure(path: &Path, runs: usize, scratch: &Path) -> Result<MeasuredCables, String> {
    let _ = ecad_engine::install();
    let copy = scratch.join("cables.ecad");
    remove(&copy);
    std::fs::copy(path, &copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let mut out = MeasuredCables::default();
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file_in(&mut s, path_text(&copy)?, OPEN_SEED, ecad_engine::cables::views::Settings::none())?;
    if let Some(p) = project.read(&mut s, |p, _| p.ledger.document().page_order.first().copied())? {
        project.open_page(&mut s, p)?;
    }
    out.cables = project.configure(&mut s, |p| {
        p.checks();
        p.outputs();
        p.ledger.document().cables.len()
    })?;
    let (mut n, mut e, mut a, mut sl) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for i in 0..runs.max(1) {
        let (ms, v) = exec(&mut s, "cablenew", json!({}))?;
        n.push(ms);
        let k = v["cable"].clone();
        e.push(exec(&mut s, "cableedit", json!({"cable": k, "set": {"length": 10 + i}}))?.0);
        let x = X0 + 100.0 * i as f64;
        let mut slash = seven(&mut s, x)?;
        let (_, v) = exec(&mut s, "cablenew", json!({"cores": "7G"}))?;
        slash["cable"] = v["cable"].clone();
        a.push(exec(&mut s, "cableslash", slash)?.0);
        let slash = seven(&mut s, x + 50.0)?;
        sl.push(exec(&mut s, "cableslash", slash)?.0);
    }
    out.cablenew_ms = median(&mut n);
    out.cableedit_ms = median(&mut e);
    out.cableassign_ms = median(&mut a);
    out.cableslash_ms = median(&mut sl);
    drop(project);
    crate::measure::release_lock_file(&copy);
    Ok(out)
}
