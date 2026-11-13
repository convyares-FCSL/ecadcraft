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
//! - Brief 030 (design 030 §11.3): the copy also gets project part defaults for every accessory,
//!   end stops and end plates at both ends of every strip, a partition after every 5th terminal
//!   on 50 strips, markers on every position of 50 others, and 2 links (`1-4` and the skip link
//!   `7, 9, 11`) on every strip. **accset**: a partition after 4 (`accessory`); **accmany**:
//!   markers on 20 positions in one call; **accproj**: `strip_end_plate_part_number` changed (every
//!   strip inherits it); **linkset**: a 10-position link made with `jumper` (the numbering pass in
//!   its transaction, the page redrawn); **linkpage**: the link lines of page 1 worked out. Each
//!   committed and updated, then undone.
//!
//! - Brief 036 (design 036 §11.3), on page 1's 20 bench wires: **lvlfence**: Place Terminals
//!   `LNPE` level `L` across them (a new strip); **lvlplace**: a `termlnpe.2` level placed on a wire
//!   with that strip (the free-level search, the split); **lvlsep**: the project's
//!   `terminal.level.separator` changed (only strips with multi-level positions inherit it);
//!   **toposition**: a fuse terminal device turned into a fuse position; **lvltrenum**: the L/N
//!   strip (20 positions, links on levels 1 and 2) renumbered. Each undone (the strip kept).
//!   The design's large-tier population (50 L/N/PE strips, 50 sensor strips, 25 fuse strips) is
//!   not added, so 028's and 030's rows stay comparable with their baselines.
//!
//! - Brief 046 (design 046 §10): **spage**: the first sheet of a one-strip graphic page drawn (the
//!   bench strip: 20 positions, 10 two-core cables, links and accessories); **spageupd**: with
//!   that page shown, a wire number set on a wire of that strip (the strip's drawing changes; a
//!   draw command cannot run on a generated page), committed, updated, redrawn; **spageoff**: the
//!   same on another strip (no projection, counted in `spageoff_projections`); **spagetype**: one
//!   of the strip's cables gets another type with the page shown (`spagetype_work` strips marked).
//!   **stripbuild** now includes the graphic pages' kept data (`StripPages::build`), as first show
//!   does.
//!
//! Each edit is undone after it is measured.

use std::path::Path;
use std::time::Instant;

use cadcraft_engine::Session;
use ecad_cadcraft::ProjectHandle;
use ecad_model::{
    Author, CableId, CableMember, ComponentId, ConductorId, End, Item, ItemKind, ObjectRef, Op, PageId, Primitive, Pt, Representation, Subject,
};
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
    /// Brief 030.
    pub accset_ms: f64,
    pub accmany_ms: f64,
    pub accproj_ms: f64,
    pub accproj_work: usize,
    pub linkset_ms: f64,
    pub linkpage_ms: f64,
    /// Brief 036.
    pub lvlfence_ms: f64,
    pub lvlplace_ms: f64,
    pub lvlsep_ms: f64,
    pub lvlsep_work: usize,
    pub toposition_ms: f64,
    pub lvltrenum_ms: f64,
    /// Brief 046.
    pub spage_ms: f64,
    pub spageupd_ms: f64,
    pub spageoff_ms: f64,
    pub spageoff_projections: u64,
    pub spagetype_ms: f64,
    pub spagetype_work: usize,
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
        let mut made: Vec<ComponentId> = Vec::new();
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
            made.push(f.strip);
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
        // Brief 030: accessories, parts and links (design 030 §11.3), one transaction.
        let mut ops = Vec::new();
        let doc = p.ledger.document();
        let set = |ops: &mut Vec<Op>, object: ObjectRef, key: String, value: String| {
            ops.push(Op::SetProperty { object, key, before: None, after: Some(value) });
        };
        for item in ecad_cadcraft::accessories::PART_ITEMS {
            if !doc.properties.contains_key(&format!("strip_{item}_part_number")) {
                set(&mut ops, ObjectRef::Project, format!("strip_{item}_part_number"), format!("{}-1", item.to_ascii_uppercase()));
            }
        }
        set(&mut ops, ObjectRef::Project, "strip_marker_per_card".into(), "10".into());
        for (i, c) in made.iter().enumerate() {
            let o = ObjectRef::Component(*c);
            set(&mut ops, o, "acc.start".into(), "end_stop".into());
            set(&mut ops, o, "acc.end".into(), "end_plate,end_stop".into());
            if i < 50 {
                for n in (5..=TERMINALS).step_by(5) {
                    set(&mut ops, o, format!("acc.after.{n}"), "partition".into());
                }
            } else if i < 100 {
                for n in 1..=TERMINALS {
                    set(&mut ops, o, format!("acc.on.{n}"), "marker".into());
                }
            }
            set(&mut ops, o, "link.1".into(), "1-4".into());
            set(&mut ops, o, "link.2".into(), "7, 9, 11".into());
        }
        p.ledger.commit(Author::User, "bench accessories", ops).map_err(|e| e.to_string())?;
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
                // Brief 046: the graphic pages' kept data, built with the strips.
                std::hint::black_box(ecad_cadcraft::strip_page::StripPages::build(doc, &built));
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
            // Brief 032: no joining at the free end, in both rows (the baseline's free end lies on
            // a cable core, where a join is refused), so the two stay comparable.
            run(&mut s, "wire", json!({"points": [[last + 30.0, 130.0], [last + 5.0, 100.0], [last, 100.0]], "join": false}))?;
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
                run(&mut s, "wire", json!({"points": [[at.x + 30.0, at.y + 30.0], [at.x + 5.0, at.y], [at.x, at.y]], "join": false}))?;
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
        // Brief 030 (design 030 §11.3), each undone.
        let timed = |s: &mut Session, cmd: &str, params: serde_json::Value| -> Result<f64, String> {
            let mut v = Vec::new();
            for _ in 0..runs {
                let t = Instant::now();
                run(s, cmd, params.clone())?;
                v.push(ms(t));
                run(s, "undo", json!({}))?;
            }
            Ok(median(&mut v))
        };
        out.accset_ms = timed(&mut s, "accessory", json!({"strip": strip.hex(), "at": "3", "add": ["partition"]}))?;
        out.accmany_ms = timed(&mut s, "accessory", json!({"strip": strip.hex(), "at": "all", "on": true, "items": ["marker"]}))?;
        out.linkset_ms = timed(&mut s, "jumper", json!({"strip": strip.hex(), "positions": "11-20"}))?;
        let mut proj = Vec::new();
        for i in 0..runs {
            let t = Instant::now();
            run(&mut s, "projectprops", json!({"props": {"strip_end_plate_part_number": format!("EP-{}", i + 2)}}))?;
            proj.push(ms(t));
            out.accproj_work = out.accproj_work.max(work(&project, &mut s));
            run(&mut s, "undo", json!({}))?;
        }
        out.accproj_ms = median(&mut proj);
        let mut lp = Vec::new();
        for _ in 0..runs {
            lp.push(project.read(&mut s, |p, _| {
                let t = Instant::now();
                std::hint::black_box(ecad_cadcraft::link_lines::parts_on_page(p.ledger.document(), page1));
                ms(t)
            })?);
        }
        out.linkpage_ms = median(&mut lp);
        // Brief 036 (design 036 §11.3).
        let lv_line = |y: f64| json!({"type": "LNPE", "level": "L", "from": [X0 - 5.0, y], "to": [X0 + PITCH * TERMINALS as f64, y]});
        out.lvlfence_ms = timed(&mut s, "terminals", lv_line(120.0))?;
        let lv = run(&mut s, "terminals", lv_line(120.0))?;
        let lv: ComponentId = hex(&lv, "strip")?;
        out.lvlplace_ms = timed(&mut s, "place", json!({"symbol": "termlnpe.2", "at": [X0 + PITCH, 80.0], "device": lv.hex()}))?;
        let mut sep = Vec::new();
        for i in 0..runs {
            let t = Instant::now();
            run(&mut s, "projectprops", json!({"props": {"terminal.level.separator": if i % 2 == 0 { "" } else { "-" }}}))?;
            sep.push(ms(t));
            out.lvlsep_work = out.lvlsep_work.max(work(&project, &mut s));
            run(&mut s, "undo", json!({}))?;
        }
        out.lvlsep_ms = median(&mut sep);
        let mut topo = Vec::new();
        for _ in 0..runs {
            let f = run(&mut s, "place", json!({"symbol": "fuseterm", "at": [X0 - 40.0, 150.0]}))?;
            let f: ComponentId = hex(&f, "component")?;
            let t = Instant::now();
            run(&mut s, "toposition", json!({"device": f.hex(), "strip": lv.hex()}))?;
            topo.push(ms(t));
            run(&mut s, "undo", json!({}))?;
            run(&mut s, "undo", json!({}))?;
        }
        out.toposition_ms = median(&mut topo);
        run(
            &mut s,
            "terminals",
            json!({"strip": lv.hex(), "type": "LNPE", "level": "N", "from": [X0 - 5.0, 60.0], "to": [X0 + PITCH * TERMINALS as f64, 60.0]}),
        )?;
        run(&mut s, "setprop", json!({"component": lv.hex(), "key": "link.1", "value": "1-10"}))?;
        run(&mut s, "setprop", json!({"component": lv.hex(), "key": "link.2", "value": "1.2-10.2"}))?;
        let mut lt = Vec::new();
        for i in 0..runs {
            let t = Instant::now();
            run(&mut s, "renumber", json!({"kind": "terminals", "scope": "selection", "strips": [lv.hex()], "start": 201 + i}))?;
            lt.push(ms(t));
            run(&mut s, "undo", json!({}))?;
        }
        out.lvltrenum_ms = median(&mut lt);
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
        // Brief 046 (design 046 §10): a graphic page of the same strip.
        let sp = run(&mut s, "newpage", json!({"generated": "terminals", "style": "graphic", "strip": strip.hex()}))?;
        let sp_page: PageId = hex(&sp, "page")?;
        let mut spage = Vec::new();
        for _ in 0..runs {
            spage.push(project.configure(&mut s, |p| {
                p.outputs();
                let doc = p.ledger.document();
                let t = Instant::now();
                let (input, total) = ecad_cadcraft::generated::input_of(doc, sp_page, p.outputs.as_ref());
                std::hint::black_box(ecad_cadcraft::generated::page_parts(doc, sp_page, &input, p.outputs.as_ref(), total));
                ms(t)
            })?);
        }
        out.spage_ms = median(&mut spage);
        // A wire of this strip and one of another strip; a cable of this strip.
        let (mine, other, cable) = project.read(&mut s, |p, _| {
            let doc = p.ledger.document();
            let wire_of = |c: ComponentId| doc.terminals_of(c).find_map(|(t, _)| doc.ends_at(t).into_iter().next().map(|(w, _)| w));
            let other = p.outputs.as_ref().and_then(|o| o.strips.strips.keys().copied().find(|c| *c != strip && *c != lv));
            let cable = wire_of(strip)
                .and_then(|_| doc.terminals_of(strip).flat_map(|(t, _)| doc.ends_at(t)).find_map(|(w, _)| doc.cables_of(w).into_iter().next()));
            (wire_of(strip), other.and_then(wire_of), cable)
        })?;
        run(&mut s, "openpage", json!({"page": sp_page.hex()}))?;
        let projections = |s: &mut Session| project.read(s, |p, _| p.projections).unwrap_or(0);
        if let Some(w) = mine {
            let mut v = Vec::new();
            for i in 0..runs {
                let t = Instant::now();
                run(&mut s, "wirenumber", json!({"conductor": w.hex(), "value": format!("SP-{i}")}))?;
                v.push(ms(t));
                run(&mut s, "undo", json!({}))?;
            }
            out.spageupd_ms = median(&mut v);
        }
        if let Some(w) = other {
            let mut v = Vec::new();
            for i in 0..runs {
                let before = projections(&mut s);
                let t = Instant::now();
                run(&mut s, "wirenumber", json!({"conductor": w.hex(), "value": format!("SO-{i}")}))?;
                v.push(ms(t));
                out.spageoff_projections = out.spageoff_projections.max(projections(&mut s).saturating_sub(before));
                run(&mut s, "undo", json!({}))?;
            }
            out.spageoff_ms = median(&mut v);
        }
        if let Some(k) = cable {
            let mut v = Vec::new();
            for i in 0..runs {
                let t = Instant::now();
                run(&mut s, "setprop", json!({"cable": k.hex(), "key": "type", "value": format!("{}G1.5", 3 + i % 5)}))?;
                v.push(ms(t));
                out.spagetype_work = out
                    .spagetype_work
                    .max(project.read(&mut s, |p, _| p.outputs.as_ref().map_or(0, |o| o.strip_pages.last_changed.len())).unwrap_or(0));
                run(&mut s, "undo", json!({}))?;
            }
            out.spagetype_ms = median(&mut v);
        }
        drop(project);
        drop(s);
        Ok(out)
    })();
    crate::measure::release_lock_file(&copy);
    remove(&copy);
    result
}
