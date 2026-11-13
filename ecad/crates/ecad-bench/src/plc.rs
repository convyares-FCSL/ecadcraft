//! Brief 045 timings (design 045 §11.2), through the app's project on a copy of the version 2
//! file. The racks and cards are added on the copy here, not by the generator, so every earlier row
//! stays comparable with earlier baselines.
//!
//! # Semantics
//! - The copy gains 10 racks (a `plccpu` each) with 12 cards each (DI, DQ, AI, RTD and IOL bodies in
//!   turn) in slots 1–12, spread over its first 30 pages (fewer when it has fewer); it is saved and
//!   opened again, so the measured project starts clean.
//! - **plcbuild**: the PLC model built from scratch (the report models already built), as on first
//!   show. Target < 30 ms.
//! - **slotmove**: a card moved from slot 2 to 20 (the cards after it change address), committed
//!   and updated. Target < 16 ms.
//! - **chfence**: Place Channels across 16 wires (a new DI card), committed and updated. < 16 ms.
//! - **plcstyle**: one rack's address style changed (`plcaddress`), committed and updated. < 16 ms.
//! - **iolist**: every I/O row made (the models built). < 50 ms. **iolistupd**: one channel's
//!   text set (`plcchannel`), committed, updated and its card's rows made again. < 5 ms.
//! - **work**: how many racks and cards a slot move recomputed (never all of them).
//!
//! Each edit is undone after it is measured.

use std::path::Path;
use std::time::Instant;

use cadcraft_engine::Session;
use ecad_cadcraft::ProjectHandle;
use ecad_model::{Author, ComponentId, PageId, Placement, Pt};
use serde_json::json;

use crate::measure::{median, path_text, remove};

const OPEN_SEED: u64 = 0x0045_0000;
const RACKS: usize = 10;
const CARDS: usize = 12;
const PAGES: usize = 30;
const BODIES: [&str; 5] = ["plcdi", "plcdq", "plcai", "plcrtd", "plciol"];

/// What brief 045 measured, in milliseconds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeasuredPlc {
    pub plcbuild_ms: f64,
    pub racks: usize,
    pub cards: usize,
    /// Racks and cards one slot move recomputed.
    pub work: usize,
    pub slotmove_ms: f64,
    /// Brief 045b: Place Channels across 16 wires (a new DI card), committed and updated.
    pub chfence_ms: f64,
    /// One rack's address style changed, committed and updated.
    pub plcstyle_ms: f64,
    /// Brief 045c: every I/O row made from scratch, and one channel text edited (committed,
    /// updated, its card's rows made again).
    pub iolist_ms: f64,
    pub iolistupd_ms: f64,
    pub rows: usize,
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn run(s: &mut Session, cmd: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
    s.execute(cmd, &params).map_err(|e| format!("{cmd} {params}: {e}"))
}

/// Add the racks and cards (one transaction per rack, straight on the ledger).
fn populate(project: &ProjectHandle, s: &mut Session) -> Result<(), String> {
    project.configure(s, |p| -> Result<(), String> {
        let pages: Vec<PageId> = p.ledger.document().page_order.iter().take(PAGES).copied().collect();
        if pages.is_empty() {
            return Err("the project has no pages".into());
        }
        let style = ecad_model::Style { layer: p.default_layer, color: ecad_model::Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
        let cpu = ecad_cadcraft::symbols::by_key("plccpu").ok_or("no plccpu")?;
        // Tags from the project's history (the project's own counter is private to it).
        let mut high = ecad_cadcraft::devices::TagHigh::default();
        for r in 0..RACKS {
            let page = pages[(r * 3) % pages.len()];
            let mut ops = Vec::new();
            let at =
                |i: usize| Placement { origin: Pt::new(600.0 + 40.0 * i as f64, 600.0 + 60.0 * r as f64), rotation: 0.0, mirror: false, scale: 1.0 };
            let plan = ecad_cadcraft::devices::plan_place(
                &p.ledger,
                &mut p.ids,
                &mut high,
                page,
                style.clone(),
                cpu,
                at(0),
                ecad_cadcraft::devices::Target::New(None),
            )?;
            let rack: ComponentId = plan.component;
            ops.extend(plan.ops);
            p.ledger.commit(Author::User, "bench rack", std::mem::take(&mut ops)).map_err(|e| e.to_string())?;
            for i in 0..CARDS {
                let page = pages[(r * 3 + i / 4) % pages.len()];
                let body = ecad_cadcraft::symbols::by_key(BODIES[i % BODIES.len()]).ok_or("no card body")?;
                let plan = ecad_cadcraft::devices::plan_place(
                    &p.ledger,
                    &mut p.ids,
                    &mut high,
                    page,
                    style.clone(),
                    body,
                    at(i + 1),
                    ecad_cadcraft::devices::Target::New(None),
                )?;
                let card = plan.component;
                ops.extend(plan.ops);
                let slot = u32::try_from(i + 1).map_err(|e| e.to_string())?;
                ops.extend(ecad_cadcraft::plc::rack::new_card_ops(card, rack, slot));
                p.ledger.commit(Author::User, "bench card", std::mem::take(&mut ops)).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    })??;
    run(s, "qsave", json!({}))?;
    Ok(())
}

/// Measure brief 045 on a copy of `path` (in `scratch`), `runs` times each.
///
/// # Errors
/// Opening, a refused command, or a missing page.
pub fn measure(path: &Path, runs: usize, scratch: &Path) -> Result<MeasuredPlc, String> {
    let copy = scratch.join("plc.ecad");
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
        let mut out = MeasuredPlc::default();
        let mut build = Vec::new();
        for _ in 0..runs {
            build.push(project.configure(&mut s, |p| {
                p.outputs();
                let doc = p.ledger.document();
                let t = Instant::now();
                let built = ecad_cadcraft::plc::Plc::build(doc);
                let took = ms(t);
                std::hint::black_box(&built);
                took
            })?);
        }
        out.plcbuild_ms = median(&mut build);
        let (racks, cards, first) = project.read(&mut s, |p, _| {
            let o = p.outputs.as_ref();
            let racks = o.map_or(0, |o| o.plc.racks.len());
            let cards = o.map_or(0, |o| o.plc.cards.len());
            let first =
                o.and_then(|o| o.plc.racks_by_tag().first().map(|r| r.id)).and_then(|r| o.and_then(|o| o.plc.cards_of(r).get(1).map(|c| c.id)));
            (racks, cards, first)
        })?;
        (out.racks, out.cards) = (racks, cards);
        let card = first.ok_or("no card was added")?;
        // slotmove: a card from slot 2 to the free slot 20, committed and updated, then undone.
        let mut moves = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            run(&mut s, "plcslot", json!({"card": card.hex(), "slot": 20}))?;
            moves.push(ms(t));
            out.work = out.work.max(project.read(&mut s, |p, _| p.outputs.as_ref().map_or(0, |o| o.plc.last_work))?);
            run(&mut s, "undo", json!({}))?;
        }
        out.slotmove_ms = median(&mut moves);
        // 16 wires on the page shown, for the fence (setup, not timed).
        for i in 0..16 {
            let x = 900.0 + 10.0 * f64::from(i);
            run(&mut s, "wire", json!({"points": [[x, 250.0], [x, 150.0]]}))?;
        }
        let mut fences = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            run(&mut s, "plcchannels", json!({"type": "DI", "from": [895.0, 200.0], "to": [1055.0, 200.0]}))?;
            fences.push(ms(t));
            run(&mut s, "undo", json!({}))?;
        }
        out.chfence_ms = median(&mut fences);
        let rack = project.read(&mut s, |p, _| p.outputs.as_ref().and_then(|o| o.plc.racks_by_tag().first().map(|r| r.id)))?.ok_or("no rack")?;
        let mut styles = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            run(&mut s, "plcaddress", json!({"rack": rack.hex(), "style": "iec"}))?;
            styles.push(ms(t));
            run(&mut s, "undo", json!({}))?;
        }
        out.plcstyle_ms = median(&mut styles);
        let mut lists = Vec::new();
        for _ in 0..runs {
            lists.push(project.configure(&mut s, |p| {
                p.outputs();
                let doc = p.ledger.document();
                let Some(o) = p.outputs.as_ref() else { return (0.0, 0) };
                let t = Instant::now();
                let n: usize = ecad_cadcraft::plc::iolist::cards_in_order(&o.plc)
                    .into_iter()
                    .map(|c| ecad_cadcraft::plc::iolist::rows_of(doc, &o.plc, c).len())
                    .sum();
                (ms(t), n)
            })?);
        }
        out.rows = lists.first().map_or(0, |x| x.1);
        let mut times: Vec<f64> = lists.into_iter().map(|x| x.0).collect();
        out.iolist_ms = median(&mut times);
        let mut upd = Vec::new();
        for k in 0..runs {
            let t = Instant::now();
            run(&mut s, "plcchannel", json!({"card": card.hex(), "channel": 0, "text": format!("Bench {k}")}))?;
            project.configure(&mut s, |p| {
                p.outputs();
                let doc = p.ledger.document();
                if let Some(o) = p.outputs.as_ref() {
                    for c in &o.plc.rows_changed {
                        if let Some(cd) = o.plc.cards.get(c) {
                            std::hint::black_box(ecad_cadcraft::plc::iolist::rows_of(doc, &o.plc, cd));
                        }
                    }
                }
            })?;
            upd.push(ms(t));
            run(&mut s, "undo", json!({}))?;
        }
        out.iolistupd_ms = median(&mut upd);
        Ok(out)
    })();
    crate::measure::release_lock_file(&copy);
    remove(&copy);
    result
}
