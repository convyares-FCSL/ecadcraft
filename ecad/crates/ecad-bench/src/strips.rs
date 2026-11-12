//! Brief 028 timings (design 028 §11.4), through the app's project on a copy of the version 2
//! file. The strips are added on the copy here, not by the generator, so every earlier row stays
//! comparable with earlier baselines (as brief 025 added its coils).
//!
//! # Semantics
//! - The copy gains, on its first 250 pages (fewer when it has fewer), one strip of 20 terminals
//!   each (`terminals` planned on 20 wires right of the sheet), bridges on every fifth pair, and
//!   half of each strip's outside halves as cores of cables of type `7G1.5` (two cores each); it
//!   is saved and opened again, so the measured project starts clean.
//! - **stripbuild**: the strips and every cable's core count built from scratch (the report
//!   models already built), as on first show.
//! - **tplace**: a terminal placed on a wire (split), committed and the kept models updated.
//! - **tfence**: Place Terminals across 20 wires, committed and updated (a new strip).
//! - **stripupd**: a wire drawn onto a terminal (attached), committed and updated.
//! - **relocate**: the far device of one terminal's wire gets another location (`setprop`), so
//!   that row's sides flip (the design moves it into a box; a location change takes the same
//!   path of the update map).
//! - **tplan**: the first sheet of a one-strip terminal plan page drawn (frame and table parts).
//! - **trenum**: one 20-terminal strip renumbered from another start (`renumber {kind:
//!   "terminals", scope: "selection", strips}`), committed and updated, then undone.
//! - **work**: the most strips one ordinary edit above recomputed (never all of them).
//!
//! Each edit is undone after it is measured.

use std::path::Path;
use std::time::Instant;

use cadcraft_engine::Session;
use ecad_cadcraft::ProjectHandle;
use ecad_model::{Author, CableId, CableMember, ComponentId, ConductorId, End, Item, ItemKind, Op, PageId, Primitive, Pt, Representation, Subject};
use serde_json::json;

use crate::measure::{median, path_text, remove};

const OPEN_SEED: u64 = 0x0028_0000;
/// Strips added (one per page), terminals per strip.
const STRIPS: usize = 250;
const TERMINALS: usize = 20;
/// Where the bench's wires sit: right of an A3 sheet, clear of the generated pattern.
const X0: f64 = 500.0;
const PITCH: f64 = 10.0;

/// What brief 028 measured, in milliseconds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeasuredStrips {
    pub stripbuild_ms: f64,
    pub tplace_ms: f64,
    pub tfence_ms: f64,
    pub stripupd_ms: f64,
    /// The same wire drawn onto a coil's connection point instead: the baseline of `stripupd`.
    pub attach_ms: f64,
    pub relocate_ms: f64,
    pub tplan_ms: f64,
    pub trenum_ms: f64,
    /// Strips, terminals and cables in the project.
    pub strips: usize,
    pub terminals: usize,
    pub cables: usize,
    /// The most strips an ordinary edit recomputed.
    pub work: usize,
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

fn work(p: &ProjectHandle, s: &mut Session) -> usize {
    p.read(s, |p, _| p.outputs.as_ref().map_or(0, |o| o.strips.last_work)).unwrap_or(0)
}

/// Add the strips, bridges and cables to the open project (one transaction per strip, straight
/// on the ledger: no page is open on those objects yet).
fn populate(project: &ProjectHandle, s: &mut Session) -> Result<(), String> {
    project.configure(s, |p| -> Result<(), String> {
        let pages: Vec<PageId> = p.ledger.document().page_order.iter().take(STRIPS).copied().collect();
        let style = ecad_model::Style { layer: p.default_layer, color: ecad_model::Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
        // The wires.
        let mut ops = Vec::new();
        for page in &pages {
            for i in 0..TERMINALS {
                let x = X0 + PITCH * i as f64;
                let (w, it, r): (ConductorId, ecad_model::ItemId, ecad_model::RepresentationId) = (p.ids.fresh(), p.ids.fresh(), p.ids.fresh());
                ops.push(Op::CreateConductor { id: w, conductor: Default::default() });
                ops.push(Op::AddItem {
                    id: it,
                    item: Item {
                        page: *page,
                        style: style.clone(),
                        kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(x, 200.0), b: Pt::new(x, 0.0) }),
                    },
                });
                ops.push(Op::LinkRepresentation {
                    id: r,
                    rep: Representation {
                        subject: Subject::Conductor(w),
                        page: *page,
                        form: ecad_model::Form::Drawn,
                        role: "route".into(),
                        items: vec![it],
                        connection_points: Default::default(),
                    },
                });
            }
        }
        p.ledger.commit(Author::User, "bench wires", ops).map_err(|e| e.to_string())?;
        // A strip on each page, with bridges and cables (tags -W90001…, clear of the generator's).
        let mut serial = 0usize;
        for page in &pages {
            let f = ecad_cadcraft::terminal_cmds::plan_fence_in(
                p,
                *page,
                style.clone(),
                None,
                false,
                Pt::new(X0 - 5.0, 100.0),
                Pt::new(X0 + PITCH * TERMINALS as f64, 100.0),
            )?;
            let mut ops = f.ops;
            for pair in f.placed.chunks(5) {
                let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else { continue };
                let (w, it, r): (ConductorId, ecad_model::ItemId, ecad_model::RepresentationId) = (p.ids.fresh(), p.ids.fresh(), p.ids.fresh());
                let ia = f.placed.iter().position(|x| x.0 == a.0).unwrap_or(0) as f64;
                let (xa, xb) = (X0 + PITCH * ia, X0 + PITCH * (ia + 1.0));
                let route = ecad_cadcraft::terminal_cmds::staple(Pt::new(xa, 100.0), Pt::new(xb, 100.0));
                ops.push(Op::CreateConductor { id: w, conductor: Default::default() });
                ops.push(Op::AddItem {
                    id: it,
                    item: Item {
                        page: *page,
                        style: style.clone(),
                        kind: ItemKind::Primitive(Primitive::Polyline { vertices: route, closed: false }),
                    },
                });
                ops.push(Op::LinkRepresentation {
                    id: r,
                    rep: Representation {
                        subject: Subject::Conductor(w),
                        page: *page,
                        form: ecad_model::Form::Drawn,
                        role: "route".into(),
                        items: vec![it],
                        connection_points: Default::default(),
                    },
                });
                ops.push(Op::AttachEnd { conductor: w, end: End::A, before: None, after: Some(a.0) });
                ops.push(Op::AttachEnd { conductor: w, end: End::B, before: None, after: Some(b.0) });
            }
            // Half the outside halves (the second halves) as two-core cables.
            let halves: Vec<ConductorId> = f.placed.iter().take(TERMINALS / 2).map(|x| x.3).collect();
            for two in halves.chunks(2) {
                let k: CableId = p.ids.fresh();
                let mut cable = ecad_model::Cable::default();
                cable.properties.insert("type".into(), "YSLY-JZ 7G1.5".into());
                serial += 1;
                cable.properties.insert("tag".into(), format!("-W9{serial:04}"));
                ops.push(Op::CreateCable { id: k, cable });
                for (index, w) in two.iter().enumerate() {
                    ops.push(Op::AddCableMember { cable: k, index, member: CableMember { conductor: *w, core: None } });
                }
            }
            p.ledger.commit(Author::User, "bench strip", ops).map_err(|e| e.to_string())?;
        }
        Ok(())
    })??;
    run(s, "qsave", json!({}))?;
    Ok(())
}

/// Measure brief 028 on a copy of `path` (in `scratch`), `runs` times each.
///
/// # Errors
/// Opening, a refused command, or a missing page or strip.
pub fn measure(path: &Path, runs: usize, scratch: &Path) -> Result<MeasuredStrips, String> {
    let copy = scratch.join("strips.ecad");
    remove(&copy);
    std::fs::copy(path, &copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        ecad_cadcraft::install();
        {
            let mut s = Session::empty();
            let (project, _) = ProjectHandle::open_file_in(&mut s, path_text(&copy)?, OPEN_SEED, ecad_cadcraft::views::Settings::none())?;
            populate(&project, &mut s)?;
            drop(project);
        }
        crate::measure::release_lock_file(&copy);
        let mut s = Session::empty();
        let (project, _) = ProjectHandle::open_file_in(&mut s, path_text(&copy)?, OPEN_SEED + 1, ecad_cadcraft::views::Settings::none())?;
        let mut out = MeasuredStrips::default();
        // First build of the strips and core counts (the report models built, untimed, first).
        let mut build = Vec::new();
        for _ in 0..runs {
            build.push(project.configure(&mut s, |p| {
                p.outputs();
                let doc = p.ledger.document();
                let Some(o) = p.outputs.as_ref() else { return 0.0 };
                let t = Instant::now();
                let cx = ecad_cadcraft::outputs::PageCx::of(doc);
                let built = ecad_cadcraft::strips::Strips::build(doc, o, &cx);
                for k in doc.cables.keys() {
                    std::hint::black_box(ecad_cadcraft::strips::cable_cores(doc, *k));
                }
                std::hint::black_box(&built);
                ms(t)
            })?);
        }
        out.stripbuild_ms = median(&mut build);
        let (page1, first_strip, strips, terminals, cables) = project.read(&mut s, |p, _| {
            let doc = p.ledger.document();
            let o = p.outputs.as_ref();
            let strips = o.map_or(0, |o| o.strips.len());
            let terminals = o.map_or(0, |o| o.strips.strips.values().map(|x| x.drawn).sum());
            let first = o.and_then(|o| o.strips.sorted().first().map(|x| x.component));
            (doc.page_order.first().copied(), first, strips, terminals, doc.cables.len())
        })?;
        (out.strips, out.terminals, out.cables) = (strips, terminals, cables);
        let page1: PageId = page1.ok_or("the project has no pages")?;
        let strip: ComponentId = first_strip.ok_or("no strip was added")?;
        run(&mut s, "openpage", json!({"page": page1.hex()}))?;
        // A terminal on a wire (its lower half), then undone.
        let mut tplace = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            run(&mut s, "place", json!({"symbol": "terminal", "at": [X0 + PITCH, 40.0], "device": strip.hex()}))?;
            tplace.push(ms(t));
            out.work = out.work.max(work(&project, &mut s));
            run(&mut s, "undo", json!({}))?;
        }
        out.tplace_ms = median(&mut tplace);
        // Place Terminals across the 20 wires (a new strip), then undone.
        let mut tfence = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            run(&mut s, "terminals", json!({"from": [X0 - 5.0, 160.0], "to": [X0 + PITCH * TERMINALS as f64, 160.0]}))?;
            tfence.push(ms(t));
            out.work = out.work.max(work(&project, &mut s));
            run(&mut s, "undo", json!({}))?;
        }
        out.tfence_ms = median(&mut tfence);
        // A wire drawn onto terminal 20 (attached), then undone.
        let last = X0 + PITCH * (TERMINALS as f64 - 1.0);
        let mut upd = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            run(&mut s, "wire", json!({"points": [[last + 30.0, 130.0], [last + 5.0, 100.0], [last, 100.0]]}))?;
            upd.push(ms(t));
            out.work = out.work.max(work(&project, &mut s));
            run(&mut s, "undo", json!({}))?;
        }
        out.stripupd_ms = median(&mut upd);
        // The baseline: the same wire onto a device's connection point on page 1 (no strip).
        let point = project.read(&mut s, |p, _| {
            let doc = p.ledger.document();
            doc.representations_on(page1).find_map(|(_, r)| {
                let Subject::Component(c) = r.subject else { return None };
                if ecad_cadcraft::strips::is_strip(doc, c) || !ecad_cadcraft::locations::is_device(doc, c) {
                    return None;
                }
                let t = doc.terminals_of(c).map(|(t, _)| t).next()?;
                ecad_model::derive::terminal_position(doc, page1, t)
            })
        })?;
        if let Some(at) = point {
            let mut base = Vec::new();
            for _ in 0..runs {
                let t = Instant::now();
                run(&mut s, "wire", json!({"points": [[at.x + 30.0, at.y + 30.0], [at.x + 5.0, at.y], [at.x, at.y]]}))?;
                base.push(ms(t));
                run(&mut s, "undo", json!({}))?;
            }
            out.attach_ms = median(&mut base);
        }
        // One strip renumbered (every label changes), then undone.
        let mut trenum = Vec::new();
        for i in 0..runs {
            let t = Instant::now();
            run(&mut s, "renumber", json!({"kind": "terminals", "scope": "selection", "strips": [strip.hex()], "start": 101 + i}))?;
            trenum.push(ms(t));
            out.work = out.work.max(work(&project, &mut s));
            run(&mut s, "undo", json!({}))?;
        }
        out.trenum_ms = median(&mut trenum);
        // A far device: a lamp at the top end of the first wire, connected, in +FIELD; the strip
        // in +CP1. Its location then flips to +CP1 and back.
        run(&mut s, "setprop", json!({"component": strip.hex(), "key": "location", "value": "+CP1"}))?;
        let lamp = run(&mut s, "place", json!({"symbol": "lamp", "at": [X0, 210.0]}))?;
        let lamp_c: ComponentId = hex(&lamp, "component")?;
        let x2 = lamp["terminals"].as_array().and_then(|a| a.iter().find(|t| t["key"] == "X2")).cloned().ok_or("no lamp X2")?;
        let x2: ecad_model::TerminalId = hex(&x2, "terminal")?;
        let top = project
            .read(&mut s, |p, _| {
                let doc = p.ledger.document();
                doc.representations_on(page1).find_map(|(_, r)| {
                    let Subject::Conductor(w) = r.subject else { return None };
                    let (a, _) = ecad_model::derive::route_ends_on(doc, w, page1)?;
                    (a.near(Pt::new(X0, 200.0), 1e-6) && doc.conductors.get(&w)?.end(End::A).is_none()).then_some(w)
                })
            })?
            .ok_or("no wire end at the lamp")?;
        run(&mut s, "connect", json!({"conductor": top.hex(), "end": "A", "terminal": x2.hex()}))?;
        run(&mut s, "setprop", json!({"component": lamp_c.hex(), "key": "location", "value": "+FIELD"}))?;
        let mut relocate = Vec::new();
        for i in 0..runs {
            let to = if i % 2 == 0 { "+CP1" } else { "+FIELD" };
            let t = Instant::now();
            run(&mut s, "setprop", json!({"component": lamp_c.hex(), "key": "location", "value": to}))?;
            relocate.push(ms(t));
            out.work = out.work.max(work(&project, &mut s));
        }
        out.relocate_ms = median(&mut relocate);
        // The first sheet of a one-strip terminal plan page.
        let plan = run(&mut s, "newpage", json!({"generated": "terminals", "strip": strip.hex()}))?;
        let plan_page: PageId = hex(&plan, "page")?;
        let mut tplan = Vec::new();
        for _ in 0..runs {
            tplan.push(project.configure(&mut s, |p| {
                p.outputs();
                let doc = p.ledger.document();
                let t = Instant::now();
                let (input, total) = ecad_cadcraft::generated::input_of(doc, plan_page, p.outputs.as_ref());
                std::hint::black_box(ecad_cadcraft::generated::page_parts(doc, plan_page, &input, p.outputs.as_ref(), total));
                ms(t)
            })?);
        }
        out.tplan_ms = median(&mut tplan);
        drop(project);
        drop(s);
        Ok(out)
    })();
    crate::measure::release_lock_file(&copy);
    remove(&copy);
    result
}
