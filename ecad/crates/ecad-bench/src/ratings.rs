//! Brief 057 timings (design 057 §10), through the app's project on a copy of a version 2 file,
//! with the Checks list kept (as when it is shown) and every power wire rated.
//!
//! # Semantics
//! - **device_rating**: `setprop rating` on a protective device. Budget: under 2 ms of re-check
//!   (the counters say how many subjects were re-checked and re-traced: `protects[device]` only).
//! - **wire_on_net**: one wire drawn from a protective device's load-side pin. Budget: under 2 ms
//!   of re-check.
//! - **panel_default**: `setdefault rating.panel_temperature`. Budget: under **checks_build**, the
//!   whole Checks list built from scratch on the same project.
//!
//! The rating values are synthetic (made up for the bench; none is from a standard).

use std::path::Path;
use std::time::Instant;

use cadcraft_engine::Session;
use ecad_cadcraft::ProjectHandle;
use serde_json::json;

use crate::measure::{median, path_text, remove};

const OPEN_SEED: u64 = 0x0057_0000;

/// What brief 057 measured.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeasuredRatings {
    pub rated: usize,
    pub device_rating_ms: f64,
    pub device_evaluated: usize,
    pub device_retraced: usize,
    pub wire_on_net_ms: f64,
    pub panel_default_ms: f64,
    pub checks_build_ms: f64,
}

fn exec(s: &mut Session, cmd: &str, params: serde_json::Value) -> Result<f64, String> {
    let t = Instant::now();
    s.execute(cmd, &params).map_err(|e| format!("{cmd}: {e}"))?;
    Ok(t.elapsed().as_secs_f64() * 1e3)
}

/// Measure brief 057's rows on a copy of `path`.
///
/// # Errors
/// A copy cannot be made or opened, or a command fails.
pub fn measure(path: &Path, runs: usize, scratch: &Path) -> Result<MeasuredRatings, String> {
    let _ = ecad_cadcraft::install();
    let copy = scratch.join("ratings.ecad");
    remove(&copy);
    std::fs::copy(path, &copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let mut out = MeasuredRatings::default();
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file_in(&mut s, path_text(&copy)?, OPEN_SEED, ecad_cadcraft::views::Settings::none())?;
    if let Some(p) = project.read(&mut s, |p, _| p.ledger.document().page_order.first().copied())? {
        project.open_page(&mut s, p)?;
    }
    // Every power category rated (synthetic), a bundling factor, the Checks list kept.
    for cat in ["l", "l1", "l2", "l3", "n"] {
        exec(&mut s, "setstyle", json!({"catalogue": "wire", "id": cat, "fields": {"rated_current": "27"}}))?;
    }
    exec(&mut s, "setdefault", json!({"key": "rating.bundling", "level": "project", "value": "0.8"}))?;
    // A protective device and its load-side pin.
    let found = project.configure(&mut s, |p| {
        let t = Instant::now();
        let _ = ecad_cadcraft::checks::Checks::build(p.ledger.document(), p.tolerance);
        let build = t.elapsed().as_secs_f64() * 1e3;
        let rated = p.checks().rating.results.len();
        let doc = p.ledger.document();
        let dev = doc.components.keys().copied().find(|c| ecad_cadcraft::rating::trace::is_protective_component(doc, *c));
        let pin = dev.and_then(|c| {
            doc.terminals_of(c).find_map(|(t, x)| {
                let label = x.label.clone().unwrap_or_default();
                let on = doc.representations_of(ecad_model::Subject::Component(c)).next().map(|(_, r)| r.page)?;
                let at = ecad_model::derive::terminal_position(doc, on, t)?;
                (label.parse::<u32>().is_ok_and(|n| n % 2 == 0)).then_some((on, at))
            })
        });
        (build, rated, dev, pin)
    })?;
    let (build, rated, dev, pin) = found;
    out.checks_build_ms = build;
    out.rated = rated;
    let dev = dev.ok_or("no protective device in the project")?;
    let (mut d, mut w, mut pd) = (Vec::new(), Vec::new(), Vec::new());
    for i in 0..runs.max(1) {
        let value = if i % 2 == 0 { "20 A" } else { "4A C" };
        d.push(exec(&mut s, "setprop", json!({"component": dev.hex(), "key": "rating", "value": value}))?);
        let (e, r) = project.configure(&mut s, |p| (p.checks().rating.evaluated, p.checks().rating.retraced))?;
        out.device_evaluated = e;
        out.device_retraced = r;
        let temp = if i % 2 == 0 { "45" } else { "40" };
        pd.push(exec(&mut s, "setdefault", json!({"key": "rating.panel_temperature", "level": "project", "value": temp}))?);
    }
    if let Some((page, at)) = pin {
        project.open_page(&mut s, page)?;
        for i in 0..runs.max(1) {
            let y = at.y - 5.0 - i as f64;
            w.push(exec(&mut s, "wire", json!({"points": [[at.x, at.y], [at.x, y]]}))?);
        }
    }
    out.device_rating_ms = median(&mut d);
    out.wire_on_net_ms = median(&mut w);
    out.panel_default_ms = median(&mut pd);
    drop(project);
    crate::measure::release_lock_file(&copy);
    Ok(out)
}
