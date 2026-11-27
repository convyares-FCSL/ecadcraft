//! Per-edit cost as landed wires accumulate: the product path (release, verify off) of `setstyle`, `wire` and
//! `category`, as a person drawing wires with their own wire types makes them.
//!
//! # Semantics
//! On a copy of a written project (or a new empty one), pages are added at the end and filled with landed
//! wires, [`PER_PAGE`] to a page: per wire, two coils are placed (not timed), then a new wire type is made
//! (`setstyle`), a wire drawn from the first coil's A1 to the second's (`wire`, it lands on both) and given
//! that type (`category`). Each command is timed. At each count asked for, the mean of the last
//! [`WINDOW`] edits of each command is reported, so a cost that grows with the wires already drawn shows as
//! a growing row.

use std::path::Path;
use std::time::Instant;

use cadcraft_engine::Session;
use ecad_engine::ProjectHandle;
use ecad_model::{ConnectionPointOwner, DeviceId, PageId, Pt};
use serde_json::{Value, json};

use crate::measure::{path_text, remove};

/// Landed wires on one page, as a real drawing spreads them.
pub const PER_PAGE: usize = 40;
/// Edits averaged at each count.
pub const WINDOW: usize = 10;
const SEED: u64 = 0x0099_0000;

/// The mean per-edit cost of each command over the [`WINDOW`] edits before `wires` landed wires, in ms.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PerEdit {
    pub wires: usize,
    pub setstyle_ms: f64,
    pub wire_ms: f64,
    pub category_ms: f64,
}

fn run(s: &mut Session, cmd: &str, params: &Value) -> Result<Value, String> {
    s.execute(cmd, params).map_err(|e| format!("{cmd} {params}: {e}"))
}

fn timed(s: &mut Session, cmd: &str, params: &Value) -> Result<(f64, Value), String> {
    let t = Instant::now();
    let v = run(s, cmd, params)?;
    Ok((t.elapsed().as_secs_f64() * 1000.0, v))
}

fn id_of(v: &Value, key: &str) -> Result<String, String> {
    v.get(key).and_then(Value::as_str).map(str::to_string).ok_or_else(|| format!("no `{key}` in {v}"))
}

/// Where connection point `label` of device `device` lands on `page`.
fn point(project: &ProjectHandle, s: &mut Session, page: PageId, device: &str, label: &str) -> Result<Pt, String> {
    let device = DeviceId::parse(device).ok_or_else(|| format!("not a device id: {device}"))?;
    project
        .read(s, |p, _| {
            let doc = p.ledger.document();
            doc.connection_points()
                .iter()
                .find(|(_, c)| c.owner == ConnectionPointOwner::Device(device) && c.label.as_deref() == Some(label))
                .and_then(|(id, _)| ecad_model::connectivity::derive::connection_point_position(doc, page, *id))
        })?
        .ok_or_else(|| format!("{label} of {} is not on the page", device.hex()))
}

/// Measure on a copy of `path` (`None`: a new empty project), reporting at each of `counts` landed wires.
///
/// # Errors
/// A command refused, or the copy not opened.
pub fn measure(path: Option<&Path>, counts: &[usize], scratch: &Path) -> Result<Vec<PerEdit>, String> {
    let max = counts.iter().copied().max().unwrap_or(0);
    let copy = scratch.join("per-edit.ecad");
    remove(&copy);
    ecad_engine::install();
    let mut s = Session::empty();
    let project = match path {
        Some(p) => {
            std::fs::copy(p, &copy).map_err(|e| format!("copying {} to {}: {e}", p.display(), copy.display()))?;
            ProjectHandle::open_file_in(&mut s, path_text(&copy)?, SEED, ecad_engine::cables::views::Settings::none())?.0
        }
        None => ecad_engine::Project::create_with(&mut s, "per-edit", SEED, ecad_engine::Persistence::File)?,
    };
    let mut last = project.read(&mut s, |p, _| p.ledger.document().page_order().last().copied())?.ok_or("the project has no page")?;
    let (mut styles, mut wires, mut cats) = (Vec::new(), Vec::new(), Vec::new());
    let mut out = Vec::new();
    let mut page = last;
    for i in 0..max {
        let k = i % PER_PAGE;
        if k == 0 {
            let v = run(&mut s, "newpage", &json!({"type": "schematic", "after": last.hex()}))?;
            page = PageId::parse(&id_of(&v, "page")?).ok_or("newpage gave no page id")?;
            last = page;
            project.open_page(&mut s, page)?;
        }
        // 8 wires a row, 45 mm apart, rows 50 mm apart: 40 spread over the drawing area.
        let (x, y) = ((40 + (k % 8) * 45) as f64, (40 + (k / 8) * 50) as f64);
        let a = id_of(&run(&mut s, "place", &json!({"symbol": "coil", "at": [x, y]}))?, "component")?;
        let b = id_of(&run(&mut s, "place", &json!({"symbol": "coil", "at": [x + 20.0, y]}))?, "component")?;
        let (pa, pb) = (point(&project, &mut s, page, &a, "A1")?, point(&project, &mut s, page, &b, "A1")?);
        let fields = json!({"colours": "BU", "size": format!("{}.5", i + 1)});
        let (t, style) =
            timed(&mut s, "setstyle", &json!({"catalogue": "wire", "new": true, "name": format!("P{i}"), "copy_of": "dc24", "fields": fields}))?;
        styles.push(t);
        let (t, w) = timed(&mut s, "wire", &json!({"points": [[pa.x, pa.y], [pb.x, pb.y]]}))?;
        wires.push(t);
        let (t, set) = timed(&mut s, "category", &json!({"conductor": id_of(&w, "conductor")?, "value": id_of(&style, "id")?}))?;
        cats.push(t);
        if set.get("changed") != Some(&Value::Bool(true)) {
            return Err(format!("wire {i} did not take its type: {set}"));
        }
        if counts.contains(&(i + 1)) {
            let mean = |v: &[f64]| {
                let tail = &v[v.len().saturating_sub(WINDOW)..];
                tail.iter().sum::<f64>() / tail.len().max(1) as f64
            };
            out.push(PerEdit { wires: i + 1, setstyle_ms: mean(&styles), wire_ms: mean(&wires), category_ms: mean(&cats) });
        }
    }
    drop(project);
    drop(s);
    remove(&copy);
    Ok(out)
}

/// Devices given each location and function by [`refusal_snapshot`].
pub const DESIGNATION_GROUP: usize = 200;

/// What [`refusal_snapshot`] measured: the median time to copy what a generic command keeps to put back on a
/// refusal (the id allocator and the tag high-water marks, copied before every generic command), and what a
/// copy of the designation-uses index would cost (it is not copied: it journals its changes and undoes them),
/// on a project of `devices` devices.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RefusalSnapshot {
    pub devices: usize,
    pub ids_ms: f64,
    pub tag_high_ms: f64,
    pub designation_uses_ms: f64,
}

/// Time copying the refusal snapshot of the project at `path` (opened on a copy), median of `runs`: the ids and
/// high-water copies are what every generic command pays before it runs; the designation-uses copy is the cost
/// its journal saves. The devices are first given a location and a function,
/// [`DESIGNATION_GROUP`] to each, as a real project has them (the generated projects have none).
///
/// # Errors
/// The copy not opened.
pub fn refusal_snapshot(path: &Path, runs: usize, scratch: &Path) -> Result<RefusalSnapshot, String> {
    let copy = scratch.join("refusal.ecad");
    remove(&copy);
    std::fs::copy(path, &copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    ecad_engine::install();
    let mut s = Session::empty();
    let project = ProjectHandle::open_file_in(&mut s, path_text(&copy)?, SEED, ecad_engine::cables::views::Settings::none())?.0;
    let devices: Vec<String> = project.read(&mut s, |p, _| p.ledger.document().devices().keys().map(|d| d.hex()).collect())?;
    for (k, group) in devices.chunks(DESIGNATION_GROUP).enumerate() {
        run(&mut s, "setlocation", &json!({"devices": group, "value": format!("+CP{k}")}))?;
        run(&mut s, "setfunction", &json!({"devices": group, "value": format!("=F{k}")}))?;
    }
    let r = project.read(&mut s, |p, _| {
        let doc = p.ledger.document();
        let uses = ecad_electrical::locations::designations::Uses::of(doc);
        let mut tag_high = ecad_electrical::devices::tags::TagHigh::starting_at(ecad_model::naming::tag::HighWater::new());
        tag_high.catch_up(&p.ledger);
        let time = |f: &dyn Fn()| {
            let mut v: Vec<f64> = (0..runs.max(1))
                .map(|_| {
                    let t = Instant::now();
                    f();
                    t.elapsed().as_secs_f64() * 1000.0
                })
                .collect();
            crate::measure::median(&mut v)
        };
        RefusalSnapshot {
            devices: doc.devices().len(),
            ids_ms: time(&|| {
                std::hint::black_box(p.ids.clone());
            }),
            tag_high_ms: time(&|| drop(std::hint::black_box(tag_high.clone()))),
            designation_uses_ms: time(&|| drop(std::hint::black_box(uses.clone()))),
        }
    });
    drop(project);
    drop(s);
    remove(&copy);
    r
}

/// Free wires on the page before [`stacked`] times more.
pub const STACK: usize = 40;

/// The mean `wire` cost, in ms, of drawing [`WINDOW`] more free horizontal wires on a page that already has
/// [`STACK`] of them, each `spacing` mm below the one before: 0.01 mm stacks their ends on top of each other
/// (the near-ends check finds every one of them), 5 mm spreads them as a normal page does. A new project.
///
/// # Errors
/// A command refused.
pub fn stacked(spacing: f64) -> Result<f64, String> {
    ecad_engine::install();
    let mut s = Session::empty();
    let project = ecad_engine::Project::create_with(&mut s, "stacked", SEED, ecad_engine::Persistence::File)?;
    let last = project.read(&mut s, |p, _| p.ledger.document().page_order().last().copied())?.ok_or("the project has no page")?;
    let v = run(&mut s, "newpage", &json!({"type": "schematic", "after": last.hex()}))?;
    let page = PageId::parse(&id_of(&v, "page")?).ok_or("newpage gave no page id")?;
    project.open_page(&mut s, page)?;
    let mut times = Vec::new();
    for k in 0..STACK + WINDOW {
        let y = 50.0 + k as f64 * spacing;
        let (t, _) = timed(&mut s, "wire", &json!({"points": [[40.0, y], [200.0, y]]}))?;
        if k >= STACK {
            times.push(t);
        }
    }
    drop(project);
    drop(s);
    Ok(times.iter().sum::<f64>() / times.len().max(1) as f64)
}
