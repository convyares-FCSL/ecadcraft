//! Terminal strip timings, through the app's project on a copy of the version 2
//! file. The strips are added on the copy here, not by the generator, so every earlier row stays
//! comparable with earlier baselines (as the coils were added for the device references).
//!
//! # Semantics
//! - The copy gains, on its first 250 pages (fewer when it has fewer), one strip of 20 terminals
//!   each (`connection_points` planned on 20 wires right of the sheet), bridges on every fifth pair, and
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
//! - Accessories: the copy also gets project part defaults for every accessory,
//!   end stops and end plates at both ends of every strip, a partition after every 5th terminal
//!   on 50 strips, markers on every position of 50 others, and 2 links (`1-4` and the skip link
//!   `7, 9, 11`) on every strip. **accset**: a partition after 4 (`accessory`); **accmany**:
//!   markers on 20 positions in one call; **accproj**: `strip_end_plate_part_number` changed (every
//!   strip inherits it); **linkset**: a 10-position link made with `jumper` (the numbering pass in
//!   its transaction, the page redrawn); **linkpage**: the link lines of page 1 worked out. Each
//!   committed and updated, then undone.
//!
//! - Multi-level terminal blocks, on page 1's 20 bench wires: **lvlfence**: Place Terminals
//!   `LNPE` level `L` across them (a new strip); **lvlplace**: a `termlnpe.2` level placed on a wire
//!   with that strip (the free-level search, the split); **lvlsep**: the project's
//!   `terminal_block.level.separator` changed (only strips with multi-level positions inherit it);
//!   **toposition**: a fuse terminal device turned into a fuse position; **lvltrenum**: the L/N
//!   strip (20 positions, links on levels 1 and 2) renumbered. Each undone (the strip kept).
//!   The large-tier population (50 L/N/PE strips, 50 sensor strips, 25 fuse strips) is
//!   not added, so the earlier rows stay comparable with their baselines.
//!
//! - Strip pages: **spage**: the first sheet of a one-strip graphic page drawn (the
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
use ecad_engine::ProjectHandle;
use ecad_model::{Author, CableId, ConnectionPointId, Core, CorePosition, DeviceId, ObjectRef, Op, PageId, Pt, RouteEnd, RouteId, Subject};
use serde_json::json;

use crate::measure::{median, path_text, remove};

const OPEN_SEED: u64 = 0x0028_0000;
/// Strips added (one per page), terminals per strip.
const STRIPS: usize = 250;
const TERMINAL_BLOCKS: usize = 20;
/// Where the bench's wires sit: right of an A3 sheet, clear of the generated pattern.
const X0: f64 = 500.0;
const PITCH: f64 = 10.0;

/// What the terminal strip timings measured, in milliseconds.
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
    /// Accessories.
    pub accset_ms: f64,
    pub accmany_ms: f64,
    pub accproj_ms: f64,
    pub accproj_work: usize,
    pub linkset_ms: f64,
    pub linkpage_ms: f64,
    /// Multi-level terminal blocks.
    pub lvlfence_ms: f64,
    pub lvlplace_ms: f64,
    pub lvlsep_ms: f64,
    pub lvlsep_work: usize,
    pub toposition_ms: f64,
    pub lvltrenum_ms: f64,
    /// Strip pages.
    pub spage_ms: f64,
    pub spageupd_ms: f64,
    pub spageoff_ms: f64,
    pub spageoff_projections: u64,
    pub spagetype_ms: f64,
    pub spagetype_work: usize,
    /// The `StripPages::build` share of `stripbuild`.
    pub spbuild_ms: f64,
    /// Strips, terminals and cables in the project.
    pub strips: usize,
    pub connection_points: usize,
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
    p.read(s, |p, _| p.kept_reports().map_or(0, |o| o.strips.last_work)).unwrap_or(0)
}

/// Add the strips, bridges and cables to the open project (one transaction per strip, straight
/// on the ledger: no page is open on those objects yet).
pub(crate) fn populate(project: &ProjectHandle, s: &mut Session) -> Result<(), String> {
    project.configure(s, |p| -> Result<(), String> {
        let pages: Vec<PageId> = p.ledger.document().page_order().iter().take(STRIPS).copied().collect();
        let style = ecad_model::Style { layer: p.default_layer, color: ecad_model::Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
        // The lines: one loose route each (no wire until both ends land on connection points).
        let mut ops = Vec::new();
        for page in &pages {
            for i in 0..TERMINAL_BLOCKS {
                let x = X0 + PITCH * i as f64;
                let (_, _, add) = ecad_electrical::wires::routes::add_route(
                    &mut p.ids,
                    *page,
                    style.clone(),
                    vec![Pt::new(x, 200.0), Pt::new(x, 0.0)],
                    [RouteEnd::Free, RouteEnd::Free],
                );
                ops.extend(add);
            }
        }
        p.ledger.commit(Author::User, "bench wires", ops).map_err(|e| e.to_string())?;
        // A strip on each page, with bridges and cables (tags -W90001…, clear of the generator's).
        let mut serial = 0usize;
        let mut made: Vec<DeviceId> = Vec::new();
        for page in &pages {
            let f = ecad_engine::terminal_strips::commands::plan_fence_in(
                p,
                *page,
                style.clone(),
                None,
                false,
                Pt::new(X0 - 5.0, 100.0),
                Pt::new(X0 + PITCH * TERMINAL_BLOCKS as f64, 100.0),
            )
            .map_err(String::from)?;
            made.push(f.strip);
            let mut ops = f.ops;
            for pair in f.placed.chunks(5) {
                let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else { continue };
                let ia = f.placed.iter().position(|x| x.0 == a.0).unwrap_or(0) as f64;
                let (xa, xb) = (X0 + PITCH * ia, X0 + PITCH * (ia + 1.0));
                let route = ecad_electrical::terminal_strips::commands::terminal_strips::staple(Pt::new(xa, 100.0), Pt::new(xb, 100.0));
                // A route from terminal to terminal: the commit's rewrite makes its wire.
                let (_, _, add) =
                    ecad_electrical::wires::routes::add_route(&mut p.ids, *page, style.clone(), route, [RouteEnd::Point(a.0), RouteEnd::Point(b.0)]);
                ops.extend(add);
            }
            // Half the outside halves (the second halves) as two-core cables.
            // Each a loose line (landed on its terminal, the other end free): the core sits on it.
            let halves: Vec<(RouteId, ConnectionPointId)> = f.placed.iter().take(TERMINAL_BLOCKS / 2).map(|x| (x.3, x.0)).collect();
            for two in halves.chunks(2) {
                let k: CableId = p.ids.fresh();
                let mut cable = ecad_model::Cable::default();
                cable.properties.insert("type".into(), "YSLY-JZ 7G1.5".into());
                serial += 1;
                cable.tag = ecad_model::naming::tag::TagSlot::of_text(Some(&format!("-W9{serial:04}")), Default::default());
                cable.cores = two
                    .iter()
                    .map(|(route, landed)| Core { position: CorePosition::Route { route: *route, landed: Some(*landed) }, ..Core::unplaced(None) })
                    .collect();
                ops.push(Op::CreateCable { id: k, cable });
            }
            p.ledger.commit(Author::User, "bench strip", ops).map_err(|e| e.to_string())?;
        }
        // Accessories, parts and links, one transaction.
        let mut ops = Vec::new();
        let doc = p.ledger.document();
        let set = |ops: &mut Vec<Op>, object: ObjectRef, key: String, value: String| {
            ops.push(Op::SetProperty { object, key, before: None, after: Some(value) });
        };
        for item in ecad_electrical::terminal_strips::accessories::PART_ITEMS {
            if !doc.display().contains_key(&format!("strip_{item}_part_number")) {
                set(&mut ops, ObjectRef::Project, format!("strip_{item}_part_number"), format!("{}-1", item.to_ascii_uppercase()));
            }
        }
        set(&mut ops, ObjectRef::Project, "strip_marker_per_card".into(), "10".into());
        for (i, c) in made.iter().enumerate() {
            let o = ObjectRef::Device(*c);
            set(&mut ops, o, "acc.start".into(), "end_stop".into());
            set(&mut ops, o, "acc.end".into(), "end_plate,end_stop".into());
            if i < 50 {
                for n in (5..=TERMINAL_BLOCKS).step_by(5) {
                    set(&mut ops, o, format!("acc.after.{n}"), "partition".into());
                }
            } else if i < 100 {
                for n in 1..=TERMINAL_BLOCKS {
                    set(&mut ops, o, format!("acc.on.{n}"), "marker".into());
                }
            }
            set(&mut ops, o, "jumper_bar.1".into(), "1-4".into());
            set(&mut ops, o, "jumper_bar.2".into(), "7, 9, 11".into());
        }
        p.ledger.commit(Author::User, "bench accessories", ops).map_err(|e| e.to_string())?;
        Ok(())
    })??;
    run(s, "qsave", json!({}))?;
    Ok(())
}

/// Measure the terminal strips on a copy of `path` (in `scratch`), `runs` times each.
///
/// # Errors
/// Opening, a refused command, or a missing page or strip.
pub fn measure(path: &Path, runs: usize, scratch: &Path) -> Result<MeasuredStrips, String> {
    let copy = scratch.join("strips.ecad");
    remove(&copy);
    std::fs::copy(path, &copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        ecad_engine::install();
        {
            let mut s = Session::empty();
            let (project, _) = ProjectHandle::open_file_in(&mut s, path_text(&copy)?, OPEN_SEED, ecad_engine::cables::views::Settings::none())?;
            populate(&project, &mut s)?;
            drop(project);
        }
        crate::measure::release_lock_file(&copy);
        let mut s = Session::empty();
        let (project, _) = ProjectHandle::open_file_in(&mut s, path_text(&copy)?, OPEN_SEED + 1, ecad_engine::cables::views::Settings::none())?;
        let mut out = MeasuredStrips::default();
        // First build of the strips and core counts (the report models built, untimed, first).
        let mut build = Vec::new();
        let mut spbuild = Vec::new();
        for _ in 0..runs {
            let sp = &mut spbuild;
            build.push(project.configure(&mut s, |p| {
                p.reports();
                let doc = p.ledger.document();
                let Some(o) = p.kept_reports() else { return 0.0 };
                let t = Instant::now();
                let cx = ecad_electrical::reports::PageCx::of(doc);
                let built = ecad_electrical::terminal_strips::strip_model::Strips::build(doc, o, &cx);
                // The graphic pages' kept data, built with the strips.
                let t2 = Instant::now();
                std::hint::black_box(ecad_electrical::terminal_strips::pages::StripPages::build(doc, &built));
                sp.push(ms(t2));
                for k in doc.cables().keys() {
                    std::hint::black_box(ecad_electrical::terminal_strips::cores::cable_cores(doc, *k));
                }
                std::hint::black_box(&built);
                ms(t)
            })?);
        }
        out.stripbuild_ms = median(&mut build);
        out.spbuild_ms = median(&mut spbuild);
        let (page1, first_strip, strips, connection_points, cables) = project.read(&mut s, |p, _| {
            let doc = p.ledger.document();
            let o = p.kept_reports();
            let strips = o.map_or(0, |o| o.strips.len());
            let connection_points = o.map_or(0, |o| o.strips.strips.values().map(|x| x.drawn).sum());
            let first = o.and_then(|o| o.strips.sorted().first().map(|x| x.component));
            (doc.page_order().first().copied(), first, strips, connection_points, doc.cables().len())
        })?;
        (out.strips, out.connection_points, out.cables) = (strips, connection_points, cables);
        let page1: PageId = page1.ok_or("the project has no pages")?;
        let strip: DeviceId = first_strip.ok_or("no strip was added")?;
        run(&mut s, "openpage", json!({"page": page1.hex()}))?;
        // A terminal on a wire (its lower half), then undone.
        let mut tplace = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            run(&mut s, "place", json!({"symbol": "terminal_block", "at": [X0 + PITCH, 40.0], "device": strip.hex()}))?;
            tplace.push(ms(t));
            out.work = out.work.max(work(&project, &mut s));
            run(&mut s, "undo", json!({}))?;
        }
        out.tplace_ms = median(&mut tplace);
        // Place Terminals across the 20 wires (a new strip), then undone.
        let mut tfence = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            run(&mut s, "terminalblocks", json!({"from": [X0 - 5.0, 160.0], "to": [X0 + PITCH * TERMINAL_BLOCKS as f64, 160.0]}))?;
            tfence.push(ms(t));
            out.work = out.work.max(work(&project, &mut s));
            run(&mut s, "undo", json!({}))?;
        }
        out.tfence_ms = median(&mut tfence);
        // A wire drawn onto terminal 20 (attached), then undone.
        let last = X0 + PITCH * (TERMINAL_BLOCKS as f64 - 1.0);
        let mut upd = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            // No joining at the free end, in both rows (the baseline's free end lies on
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
                let Subject::Device(c) = r.subject else { return None };
                if ecad_electrical::terminal_strips::strip_of(doc, c).is_some() || !ecad_electrical::locations::is_device(doc, c) {
                    return None;
                }
                let t = doc.connection_points_of(c).map(|(t, _)| t).next()?;
                ecad_model::connectivity::derive::connection_point_position(doc, page1, t)
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
            run(&mut s, "renumber", json!({"kind": "terminal_blocks", "scope": "selection", "strips": [strip.hex()], "start": 101 + i}))?;
            trenum.push(ms(t));
            out.work = out.work.max(work(&project, &mut s));
            run(&mut s, "undo", json!({}))?;
        }
        out.trenum_ms = median(&mut trenum);
        // Accessories, each undone.
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
                std::hint::black_box(ecad_electrical::terminal_strips::jumper_bars::parts_on_page(p.ledger.document(), page1));
                ms(t)
            })?);
        }
        out.linkpage_ms = median(&mut lp);
        // Multi-level terminal blocks.
        let lv_line = |y: f64| json!({"type": "LNPE", "level": "L", "from": [X0 - 5.0, y], "to": [X0 + PITCH * TERMINAL_BLOCKS as f64, y]});
        out.lvlfence_ms = timed(&mut s, "terminalblocks", lv_line(120.0))?;
        let lv = run(&mut s, "terminalblocks", lv_line(120.0))?;
        let lv: DeviceId = hex(&lv, "strip")?;
        out.lvlplace_ms = timed(&mut s, "place", json!({"symbol": "termlnpe.2", "at": [X0 + PITCH, 80.0], "device": lv.hex()}))?;
        let mut sep = Vec::new();
        for i in 0..runs {
            let t = Instant::now();
            run(&mut s, "projectprops", json!({"props": {"terminal_block.level.separator": if i % 2 == 0 { "" } else { "-" }}}))?;
            sep.push(ms(t));
            out.lvlsep_work = out.lvlsep_work.max(work(&project, &mut s));
            run(&mut s, "undo", json!({}))?;
        }
        out.lvlsep_ms = median(&mut sep);
        let mut topo = Vec::new();
        for _ in 0..runs {
            let f = run(&mut s, "place", json!({"symbol": "fuseterminalblock", "at": [X0 - 40.0, 150.0]}))?;
            let f: DeviceId = hex(&f, "component")?;
            let t = Instant::now();
            run(&mut s, "toposition", json!({"device": f.hex(), "strip": lv.hex()}))?;
            topo.push(ms(t));
            run(&mut s, "undo", json!({}))?;
            run(&mut s, "undo", json!({}))?;
        }
        out.toposition_ms = median(&mut topo);
        run(
            &mut s,
            "terminalblocks",
            json!({"strip": lv.hex(), "type": "LNPE", "level": "N", "from": [X0 - 5.0, 60.0], "to": [X0 + PITCH * TERMINAL_BLOCKS as f64, 60.0]}),
        )?;
        run(&mut s, "setprop", json!({"component": lv.hex(), "key": "jumper_bar.1", "value": "1-10"}))?;
        run(&mut s, "setprop", json!({"component": lv.hex(), "key": "jumper_bar.2", "value": "1.2-10.2"}))?;
        let mut lt = Vec::new();
        for i in 0..runs {
            let t = Instant::now();
            run(&mut s, "renumber", json!({"kind": "terminal_blocks", "scope": "selection", "strips": [lv.hex()], "start": 201 + i}))?;
            lt.push(ms(t));
            run(&mut s, "undo", json!({}))?;
        }
        out.lvltrenum_ms = median(&mut lt);
        // A far device: a lamp at the top end of the first wire, connected, in +FIELD; the strip
        // in +CP1. Its location then flips to +CP1 and back.
        run(&mut s, "setprop", json!({"component": strip.hex(), "key": "location", "value": "+CP1"}))?;
        let lamp = run(&mut s, "place", json!({"symbol": "lamp", "at": [X0, 210.0]}))?;
        let lamp_c: DeviceId = hex(&lamp, "component")?;
        let x2 = lamp["connection_points"].as_array().and_then(|a| a.iter().find(|t| t["key"] == "X2")).cloned().ok_or("no lamp X2")?;
        let x2: ecad_model::ConnectionPointId = hex(&x2, "connection_point")?;
        let top = project
            .read(&mut s, |p, _| {
                let doc = p.ledger.document();
                doc.routes().iter().find_map(|(id, r)| {
                    let first = *r.points.first()?;
                    (r.page == page1 && first.near(Pt::new(X0, 200.0), 1e-6) && r.ends[0] == RouteEnd::Free).then_some(*id)
                })
            })?
            .ok_or("no wire end at the lamp")?;
        run(&mut s, "connect", json!({"route": top.hex(), "end": "A", "connection_point": x2.hex()}))?;
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
        let plan = run(&mut s, "newpage", json!({"generated": "terminal_blocks", "strip": strip.hex()}))?;
        let plan_page: PageId = hex(&plan, "page")?;
        let mut tplan = Vec::new();
        for _ in 0..runs {
            tplan.push(project.configure(&mut s, |p| {
                p.reports();
                let doc = p.ledger.document();
                let t = Instant::now();
                let (input, total) = ecad_engine::pages::generated::input_of(doc, plan_page, p.kept_reports());
                std::hint::black_box(ecad_engine::pages::generated::page_parts(doc, plan_page, &input, p.kept_reports(), total));
                ms(t)
            })?);
        }
        out.tplan_ms = median(&mut tplan);
        // A graphic page of the same strip.
        let sp = run(&mut s, "newpage", json!({"generated": "terminal_blocks", "style": "graphic", "strip": strip.hex()}))?;
        let sp_page: PageId = hex(&sp, "page")?;
        let mut spage = Vec::new();
        for _ in 0..runs {
            spage.push(project.configure(&mut s, |p| {
                p.reports();
                let doc = p.ledger.document();
                let t = Instant::now();
                let (input, total) = ecad_engine::pages::generated::input_of(doc, sp_page, p.kept_reports());
                std::hint::black_box(ecad_engine::pages::generated::page_parts(doc, sp_page, &input, p.kept_reports(), total));
                ms(t)
            })?);
        }
        out.spage_ms = median(&mut spage);
        // A wire of this strip and one of another strip; a cable of this strip.
        let (mine, other, cable) = project.read(&mut s, |p, _| {
            let doc = p.ledger.document();
            let wire_of = |c: DeviceId| doc.connection_points_of(c).find_map(|(t, _)| doc.ends_at(t).into_iter().next().map(|(w, _)| w));
            let other = p.kept_reports().and_then(|o| o.strips.strips.keys().copied().find(|c| *c != strip && *c != lv));
            let cable = wire_of(strip).and_then(|_| {
                doc.connection_points_of(strip).flat_map(|(t, _)| doc.ends_at(t)).find_map(|(w, _)| doc.cables_of(w).into_iter().next())
            });
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
                    .max(project.read(&mut s, |p, _| p.kept_reports().map_or(0, |o| o.strip_pages.last_changed.len())).unwrap_or(0));
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
