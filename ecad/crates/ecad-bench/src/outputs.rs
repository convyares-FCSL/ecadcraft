//! Brief 024 timings (design 024 §9), through the app's project on a copy of the version 2 file,
//! with the report models built first (as the Reports window or a BOM page does) and a BOM page
//! added, so every edit goes through their update map.
//!
//! # Semantics
//! - **reportfirst**: the report models built and each tab's table made, as on the window's first
//!   show (a full build is allowed there).
//! - **locset**: the location of 50 devices set in one `setprop` (one transaction).
//! - **boxmove**: a 40-vertex location box on page 1 holding about 20 symbols moved by 1 mm and
//!   back, each committed with the page refreshed; **plainmove**: the same move of a plain
//!   40-vertex polyline (the box's should cost at most about 1 ms more).
//! - **symin**: one symbol moved into the box and out again.
//! - **projloc**: the project's `location` changed (the one project-wide case: devices whose
//!   location comes from the project).
//! - **bomupd**: a part-number edit with the models kept (Reports and a BOM page open).
//! - **wireupd**: a wire added between two connection points (then undone).
//! - **print**: Print Book of the first 50 pages to memory (on request; reported, no limit).
//! - **work**: the most rows any ordinary edit above recomputed (never the whole project).

use std::path::Path;
use std::time::Instant;

use cadcraft_engine::Session;
use ecad_cadcraft::ProjectHandle;
use ecad_model::{PageId, Pt, Subject};
use serde_json::json;

use crate::measure::{median, path_text, remove};

const OPEN_SEED: u64 = 0x0024_0000;

/// What brief 024 measured, in milliseconds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeasuredOutputs {
    pub reportfirst_ms: f64,
    pub locset_ms: f64,
    pub boxmove_ms: f64,
    pub plainmove_ms: f64,
    pub symin_ms: f64,
    pub projloc_ms: f64,
    pub bomupd_ms: f64,
    pub wireupd_ms: f64,
    pub print_ms: f64,
    pub print_sheets: usize,
    /// The most rows an ordinary edit recomputed, and the devices in the project.
    pub work: usize,
    pub devices: usize,
    /// Symbols inside the box.
    pub inside: usize,
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn run(s: &mut Session, cmd: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
    s.execute(cmd, &params).map_err(|e| format!("{cmd} {params}: {e}"))
}

fn timed(s: &mut Session, cmd: &str, params: serde_json::Value) -> Result<f64, String> {
    let t = Instant::now();
    run(s, cmd, params)?;
    Ok(ms(t))
}

/// A 40-vertex polygon round the box `lo`–`hi` (an ellipse through its corners' margin).
fn ring(lo: Pt, hi: Pt) -> Vec<[f64; 2]> {
    let (cx, cy) = ((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0);
    let (rx, ry) = ((hi.x - lo.x).max(20.0) * 0.75, (hi.y - lo.y).max(20.0) * 0.75);
    (0..40)
        .map(|i| {
            let a = std::f64::consts::TAU * f64::from(i) / 40.0;
            [cx + rx * a.cos(), cy + ry * a.sin()]
        })
        .collect()
}

fn work(p: &ProjectHandle, s: &mut Session) -> usize {
    p.read(s, |p, _| p.outputs.as_ref().map_or(0, |o| o.last_work)).unwrap_or(0)
}

/// Measure brief 024 on a copy of `path` (in `scratch`), `runs` times each.
///
/// # Errors
/// Opening, a refused command, or a missing page or device.
pub fn measure(path: &Path, runs: usize, scratch: &Path) -> Result<MeasuredOutputs, String> {
    let copy = scratch.join("outputs.ecad");
    remove(&copy);
    std::fs::copy(path, &copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        ecad_cadcraft::install();
        ecad_cadcraft::printbook::install_io();
        let mut s = Session::empty();
        let (project, _) = ProjectHandle::open_file_in(&mut s, path_text(&copy)?, OPEN_SEED, ecad_cadcraft::views::Settings::none())?;
        let mut out = MeasuredOutputs::default();
        // First show of the reports (built whole, every table made).
        let mut first = Vec::new();
        for _ in 0..runs {
            first.push(project.configure(&mut s, |p| {
                p.outputs = None;
                let t = Instant::now();
                let o = p.outputs();
                std::hint::black_box((o.bom_table(ecad_cadcraft::outputs::Group::Location), o.wire_table(), o.cable_table()));
                ms(t)
            })?);
        }
        out.reportfirst_ms = median(&mut first);
        out.devices = project.read(&mut s, |p, _| p.outputs.as_ref().map_or(0, ecad_cadcraft::outputs::Outputs::device_count))?;
        // A BOM page, so its sheets and Total follow the models too.
        run(&mut s, "newpage", json!({"generated": "bom-location"}))?;
        let (page1, devices) = project.read(&mut s, |p, _| {
            let doc = p.ledger.document();
            (doc.page_order.first().copied(), ecad_cadcraft::devices::devices(doc, None))
        })?;
        let page1: PageId = page1.ok_or("the project has no pages")?;
        run(&mut s, "openpage", json!({"page": page1.hex()}))?;
        let fifty: Vec<String> = devices.iter().take(50).map(|c| c.hex()).collect();
        let mut locset = Vec::new();
        for i in 0..runs {
            locset.push(timed(&mut s, "setprop", json!({"components": fifty, "key": "location", "value": format!("+B{i}")}))?);
            out.work = out.work.max(work(&project, &mut s));
        }
        out.locset_ms = median(&mut locset);
        // The page's symbol anchors: the box goes round them.
        let anchors: Vec<Pt> = project.read(&mut s, |p, _| {
            let doc = p.ledger.document();
            doc.representations_on(page1)
                .filter(|(_, r)| matches!(r.subject, Subject::Component(c) if ecad_cadcraft::locations::is_device(doc, c)))
                .filter_map(|(_, r)| ecad_cadcraft::locations::anchor(doc, r))
                .collect()
        })?;
        let (lo, hi) = ecad_cadcraft::locations::bounds(&anchors);
        let (lo, hi) = if anchors.is_empty() { (Pt::new(50.0, 80.0), Pt::new(150.0, 180.0)) } else { (lo, hi) };
        let outline = ring(lo, hi);
        let b = run(&mut s, "locationbox", json!({"location": "+BENCH", "points": outline}))?;
        out.inside = b["inside"].as_u64().unwrap_or(0) as usize;
        let box_item: ecad_model::ItemId =
            b["item"].as_str().and_then(|t| u128::from_str_radix(t, 16).ok()).map(ecad_model::ItemId::from).ok_or("no box item")?;
        // A plain polyline of the same shape, off to the side.
        let plain: Vec<[f64; 2]> = outline.iter().map(|[x, y]| [x + 2000.0, *y]).collect();
        let before: std::collections::BTreeSet<ecad_model::ItemId> =
            project.read(&mut s, |p, _| p.ledger.document().items.keys().copied().collect())?;
        run(&mut s, "pline", json!({"vertices": plain, "closed": true}))?;
        let handle_of =
            |s: &mut Session, item: ecad_model::ItemId| project.read(s, |p, _| p.pages.get(&page1).and_then(|v| v.map.item(item)).map(|h| h.hex()));
        let plain_item =
            project.read(&mut s, |p, _| p.ledger.document().items.keys().copied().find(|i| !before.contains(i)))?.ok_or("no plain item")?;
        let mut boxmove = Vec::new();
        let mut plainmove = Vec::new();
        for _ in 0..runs {
            for (item, into) in [(box_item, &mut boxmove), (plain_item, &mut plainmove)] {
                let h = handle_of(&mut s, item)?.ok_or("no handle")?;
                let t = Instant::now();
                run(&mut s, "move", json!({"handles": [h], "delta": [0, 1]}))?;
                let h = handle_of(&mut s, item)?.ok_or("no handle")?;
                run(&mut s, "move", json!({"handles": [h], "delta": [0, -1]}))?;
                into.push(ms(t) / 2.0);
            }
            out.work = out.work.max(work(&project, &mut s));
        }
        out.boxmove_ms = median(&mut boxmove);
        out.plainmove_ms = median(&mut plainmove);
        // One symbol moved into the box and out again: one outside it on page 1, else any.
        let sym = project.read(&mut s, |p, _| {
            let doc = p.ledger.document();
            doc.representations_on(page1).find_map(|(_, r)| {
                let Subject::Component(c) = r.subject else { return None };
                if !ecad_cadcraft::locations::is_device(doc, c) {
                    return None;
                }
                let item = *r.items.first()?;
                Some((item, ecad_cadcraft::locations::anchor(doc, r)?))
            })
        })?;
        if let Some((item, at)) = sym {
            let centre = Pt::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0);
            let (dx, dy) = (centre.x - at.x + 1.0, centre.y - at.y + 1.0);
            let mut symin = Vec::new();
            for _ in 0..runs {
                let h = handle_of(&mut s, item)?.ok_or("no handle")?;
                let t = Instant::now();
                run(&mut s, "move", json!({"handles": [h], "delta": [dx, dy]}))?;
                symin.push(ms(t));
                out.work = out.work.max(work(&project, &mut s));
                let h = handle_of(&mut s, item)?.ok_or("no handle")?;
                run(&mut s, "move", json!({"handles": [h], "delta": [-dx, -dy]}))?;
            }
            out.symin_ms = median(&mut symin);
        }
        let mut projloc = Vec::new();
        let mut bomupd = Vec::new();
        for i in 0..runs {
            projloc.push(timed(&mut s, "projectprops", json!({"location": format!("+P{i}")}))?);
            if let Some(c) = devices.get(100).or(devices.first()) {
                bomupd.push(timed(&mut s, "setprop", json!({"component": c.hex(), "key": "part_number", "value": format!("PN-{i}")}))?);
                out.work = out.work.max(work(&project, &mut s));
            }
        }
        out.projloc_ms = median(&mut projloc);
        out.bomupd_ms = median(&mut bomupd);
        // A wire between two free points (then undone).
        let mut wireupd = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            run(&mut s, "wire", json!({"points": [[3000, 3000], [3050, 3000]]}))?;
            wireupd.push(ms(t));
            out.work = out.work.max(work(&project, &mut s));
            run(&mut s, "undo", json!({}))?;
        }
        out.wireupd_ms = median(&mut wireupd);
        // Print Book of the first 50 pages, to memory.
        let range = project.read(&mut s, |p, _| {
            let doc = p.ledger.document();
            let n = |i: usize| doc.page_order.get(i).map(|pg| ecad_model::book::page_number(doc, *pg)).unwrap_or_default();
            format!("{}-{}", n(0), n(doc.page_order.len().min(50).saturating_sub(1)))
        })?;
        let t = Instant::now();
        let v = run(&mut s, "printbook", json!({"scope": {"pages": range}}))?;
        out.print_ms = ms(t);
        out.print_sheets = v["sheets"].as_u64().unwrap_or(0) as usize;
        drop(project);
        drop(s);
        Ok(out)
    })();
    remove(&copy);
    result
}
