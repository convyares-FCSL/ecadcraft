//! Brief 050 timings (design 050 §11), through the app's project on a copy of a version 2 file.
//!
//! # Semantics
//! - **style_tag**: `style {catalogue: text, id: tag, fields: {height}}` committed with the densest
//!   page on screen. Target: within 10 % of **textstyle** on the same project (031's command, the
//!   same change; since 050a it writes the same record).
//! - **textstyle**: `textstyle {kind: tag, height}`, the same way.
//! - **propredraw**: the baseline of 031, an ordinary device property on that page.
//! - **standard_remove**: `standard {remove}` of a standard holding 50 values and 20 styles. Target:
//!   under 100 ms.
//! - **open** / **open_styled**: opening the file, and the same project saved with a standard of
//!   50 values and 50 styles. Target: within 5 %.

use std::path::Path;
use std::time::Instant;

use cadcraft_engine::Session;
use ecad_cadcraft::ProjectHandle;
use ecad_model::{PageId, Subject};
use serde_json::json;

use crate::measure::{median, path_text, remove};

const OPEN_SEED: u64 = 0x0050_0000;

/// What brief 050 measured, in milliseconds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeasuredStyles {
    pub style_tag_ms: f64,
    pub textstyle_ms: f64,
    pub propredraw_ms: f64,
    pub standard_remove_ms: f64,
    pub open_ms: f64,
    pub open_styled_ms: f64,
    /// 050b: `style` on the `dc24` wire category's colour, and `category` on one net.
    pub wire_category_ms: f64,
    pub category_net_ms: f64,
    pub potential_net_ms: f64,
}

/// The densest page and a device on it.
fn densest(project: &ProjectHandle, s: &mut Session) -> Result<(PageId, Option<ecad_model::ComponentId>), String> {
    project.read(s, |p, _| {
        let doc = p.ledger.document();
        let page = doc.page_order.iter().copied().max_by_key(|pg| doc.representations_on(*pg).count()).unwrap_or(PageId(0));
        let device = doc.representations_on(page).find_map(|(_, r)| match r.subject {
            Subject::Component(c) => Some(c),
            _ => None,
        });
        (page, device)
    })
}

fn exec(s: &mut Session, cmd: &str, params: serde_json::Value) -> Result<f64, String> {
    let t = Instant::now();
    s.execute(cmd, &params).map_err(|e| format!("{cmd}: {e}"))?;
    Ok(t.elapsed().as_secs_f64() * 1e3)
}

fn time_open(file: &Path, runs: usize) -> Result<f64, String> {
    let mut v = Vec::new();
    for _ in 0..runs.max(1) {
        let mut s = Session::empty();
        let t = Instant::now();
        let (p, _) = ProjectHandle::open_file_in(&mut s, path_text(file)?, OPEN_SEED, ecad_cadcraft::views::Settings::none())?;
        v.push(t.elapsed().as_secs_f64() * 1e3);
        drop(p);
        crate::measure::release_lock_file(file);
    }
    Ok(median(&mut v))
}

/// Measure brief 050's rows on copies of `path`.
///
/// # Errors
/// A copy cannot be made or opened, or a command fails.
pub fn measure(path: &Path, runs: usize, scratch: &Path) -> Result<MeasuredStyles, String> {
    let _ = ecad_cadcraft::install();
    let copy = scratch.join("styles.ecad");
    let styled = scratch.join("styled.ecad");
    remove(&copy);
    remove(&styled);
    std::fs::copy(path, &copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let mut out = MeasuredStyles::default();
    let result = (|| {
        out.open_ms = time_open(&copy, runs)?;
        let mut s = Session::empty();
        let (project, _) = ProjectHandle::open_file_in(&mut s, path_text(&copy)?, OPEN_SEED, ecad_cadcraft::views::Settings::none())?;
        let (page, device) = densest(&project, &mut s)?;
        project.open_page(&mut s, page)?;
        let (mut style, mut text, mut prop) = (Vec::new(), Vec::new(), Vec::new());
        for i in 0..runs.max(1) {
            let h = if i % 2 == 0 { 3.0 } else { 2.5 };
            style.push(exec(&mut s, "setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"height": h + 0.5}}))?);
            text.push(exec(&mut s, "textstyle", json!({"kind": "tag", "height": h}))?);
            if let Some(c) = device {
                prop.push(exec(&mut s, "setprop", json!({"component": c.hex(), "key": "maker", "value": format!("M{i}")}))?);
            }
        }
        out.style_tag_ms = median(&mut style);
        out.textstyle_ms = median(&mut text);
        out.propredraw_ms = median(&mut prop);
        // 050b's rows, when the build has wire categories.
        if ecad_cadcraft::styles::catalogue("wire").is_some() {
            let (mut cat, mut pot) = (Vec::new(), Vec::new());
            let mut wire = Vec::new();
            for i in 0..runs.max(1) {
                let colour = if i % 2 == 0 { "RD" } else { "BU" };
                wire.push(exec(&mut s, "setstyle", json!({"catalogue": "wire", "id": "dc24", "fields": {"colour": colour}}))?);
            }
            out.wire_category_ms = median(&mut wire);
            let net = project.read(&mut s, |p, _| {
                let doc = p.ledger.document();
                doc.representations_on(page).find_map(|(_, r)| match r.subject {
                    Subject::Conductor(c) => Some(c),
                    _ => None,
                })
            })?;
            if let Some(w) = net {
                for i in 0..runs.max(1) {
                    let v = if i % 2 == 0 { "signal" } else { "analogue" };
                    cat.push(exec(&mut s, "category", json!({"net": w.hex(), "value": v}))?);
                    let p = if i % 2 == 0 { "L1" } else { "L2" };
                    pot.push(exec(&mut s, "potential", json!({"conductor": w.hex(), "value": p}))?);
                }
                out.category_net_ms = median(&mut cat);
                out.potential_net_ms = median(&mut pot);
            }
        }
        // A standard with 50 values and 20 styles, then back to IEC.
        exec(&mut s, "standard", json!({"new": "Bench"}))?;
        let keys: Vec<&'static str> =
            ecad_cadcraft::defaults::all().filter(|e| e.allows(ecad_cadcraft::defaults::Level::Standard)).map(|e| e.key).take(50).collect();
        project.read(&mut s, |_, _| ())?;
        let mut values = 0;
        for k in &keys {
            let e = ecad_cadcraft::defaults::find(k).ok_or("a registry key")?;
            let v = ecad_cadcraft::defaults::Resolved { value: e.builtin, from: ecad_cadcraft::defaults::Source::Builtin }.text();
            if s.execute("setdefault", &json!({"key": k, "level": "standard", "value": v})).is_ok() {
                values += 1;
            }
        }
        for i in 0..20 {
            exec(&mut s, "setstyle", json!({"catalogue": "text", "new": true, "name": format!("Bench {i}"), "layer": "standard"}))?;
        }
        out.standard_remove_ms = exec(&mut s, "standard", json!({"remove": true}))?;
        let _ = values;
        // The project saved with a standard of 50 values and 50 styles, for the open row.
        exec(&mut s, "standard", json!({"new": "Bench"}))?;
        for k in &keys {
            if let Some(e) = ecad_cadcraft::defaults::find(k) {
                let v = ecad_cadcraft::defaults::Resolved { value: e.builtin, from: ecad_cadcraft::defaults::Source::Builtin }.text();
                let _ = s.execute("setdefault", &json!({"key": k, "level": "standard", "value": v}));
            }
        }
        for i in 0..50 {
            exec(&mut s, "setstyle", json!({"catalogue": "text", "new": true, "name": format!("Open {i}"), "layer": "standard"}))?;
        }
        s.execute("saveas", &json!({"path": path_text(&styled)?})).map_err(|e| e.to_string())?;
        drop(project);
        drop(s);
        crate::measure::release_lock_file(&styled);
        out.open_styled_ms = time_open(&styled, runs)?;
        Ok(out.clone())
    })();
    crate::measure::release_lock_file(&copy);
    remove(&copy);
    remove(&styled);
    result
}
