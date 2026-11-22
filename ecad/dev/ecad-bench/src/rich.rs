//! [`Variant::Rich`](crate::Variant::Rich): the lean project of a tier plus the domain's other
//! objects, so the views that hold them (connectors, PLC, terminal strips, the cover, ratings,
//! cross-references with contacts) are measured on data and not on an empty corpus (#57).
//!
//! # Semantics
//! - The base is the **lean** project of the tier, unchanged.
//! - On top, only public commands and model plans are used (the ones the bench parts already
//!   use on their own copies), in this order: coils with contacts and offers (cross-references);
//!   terminal strips with terminal blocks, levels (`LNPE`), accessories and jumper bars; PLC racks
//!   with cards and channels; connectors (plain rows, D-sub pairs mated across pages, M12 pins
//!   drawn pin by pin); a cover page; protective devices with ratings, rated wires and cables.
//! - Deterministic: every ID comes from a fixed seed and no clock enters the document (a
//!   transaction's time is not part of it). The result's `nets` and `continuations` are those of
//!   the lean base; the added objects are described by the documents' own derivations.
//! - Counts follow the tier: the bench parts' populations are capped by the pages the tier has.

use cadcraft_engine::Session;
use ecad_engine::ProjectHandle;
use ecad_model::{CableId, PageId, WireId};
use serde_json::json;

use crate::generate::{Generated, Tier, Variant, generate};
use crate::measure::{path_text, release_lock_file, remove, scratch_dir};

const OPEN_SEED: u64 = 0x0057_0D01;
/// Protective devices placed (one per page, as far as the project has pages).
const PROTECTIVE: usize = 60;
/// Largest list one `rating {many}` call takes.
const MANY: usize = 500;

fn run(s: &mut Session, cmd: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
    s.execute(cmd, &params).map_err(|e| format!("{cmd} {params}: {e}"))
}

/// Generate the rich project of `tier`.
///
/// # Errors
/// The lean base fails to generate, or any command of the enrichment is refused.
pub fn generate_rich(tier: Tier) -> Result<Generated, String> {
    let mut g = generate(tier, Variant::Lean)?;
    // The coils with contacts go straight on the ledger, as the references bench does.
    crate::measure::add_coils(&mut g.ledger)?;
    // A folder of its own: two projects generated in one process (tests) never share a file.
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = scratch_dir(&format!("rich-{}-{}", tier.name, N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)))?;
    let path = dir.join("rich.ecad");
    let result = enrich(&g, &path);
    release_lock_file(&path);
    remove(&path);
    let _ = std::fs::remove_dir_all(&dir);
    g.ledger = result?;
    g.variant = Variant::Rich;
    Ok(g)
}

/// Generate the rich project of `tier` and write it to `path` as the app saves it: the enriched
/// session's own file (its history is a snapshot and the commands since, which is not the whole
/// history [`crate::container::write`] needs).
///
/// # Errors
/// As [`generate_rich`], or the file cannot be written.
pub fn write_rich(tier: Tier, path: &std::path::Path) -> Result<(), String> {
    let mut g = generate(tier, Variant::Lean)?;
    crate::measure::add_coils(&mut g.ledger)?;
    remove(path);
    let result = enrich(&g, path).map(drop);
    release_lock_file(path);
    result
}

fn enrich(g: &Generated, path: &std::path::Path) -> Result<ecad_model::Ledger, String> {
    crate::container::write(g, path)?;
    let _ = ecad_engine::install();
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file_in(&mut s, path_text(path)?, OPEN_SEED, ecad_engine::cables::views::Settings::none())?;
    let pages: Vec<PageId> = project.read(&mut s, |p, _| p.ledger.document().page_order.clone())?;
    let page1 = *pages.first().ok_or("the project has no pages")?;
    run(&mut s, "openpage", json!({"page": page1.hex()}))?;
    // Terminal strips (with levels, accessories and jumper bars) and PLC racks go straight on the
    // ledger, as their bench parts do.
    crate::strips::populate(&project, &mut s)?;
    crate::plc::populate(&project, &mut s)?;
    run(&mut s, "openpage", json!({"page": page1.hex()}))?;
    crate::connectors::populate(&mut s, &pages)?;
    levels(&mut s, &pages)?;
    run(&mut s, "newpage", json!({"type": "front", "generated": "cover"}))?;
    ratings(&project, &mut s, &pages)?;
    run(&mut s, "openpage", json!({"page": page1.hex()}))?;
    run(&mut s, "qsave", json!({}))?;
    project.read(&mut s, |p, _| p.ledger.clone())
}

/// A level strip (`LNPE`) on page 2 or 1: Place Terminals across new wires.
fn levels(s: &mut Session, pages: &[PageId]) -> Result<(), String> {
    for (i, page) in pages.iter().take(10).enumerate() {
        run(s, "openpage", json!({"page": page.hex()}))?;
        let y0 = 900.0 + 80.0 * i as f64;
        for k in 0..6 {
            let x = 500.0 + 10.0 * k as f64;
            run(s, "wire", json!({"points": [[x, y0], [x, y0 - 60.0]], "join": false}))?;
        }
        run(s, "terminalblocks", json!({"from": [495.0, y0 - 30.0], "to": [560.0, y0 - 30.0], "type": "LNPE", "level": "L"}))?;
    }
    Ok(())
}

/// Ratings: every power wire category rated, protective devices with ratings, wires and cables
/// with their own.
fn ratings(project: &ProjectHandle, s: &mut Session, pages: &[PageId]) -> Result<(), String> {
    for cat in ["l", "l1", "l2", "l3", "n"] {
        run(s, "setstyle", json!({"catalogue": "wire", "id": cat, "fields": {"rated_current": "27"}}))?;
    }
    run(s, "setdefault", json!({"key": "rating.bundling", "level": "project", "value": "0.8"}))?;
    for (i, page) in pages.iter().take(PROTECTIVE).enumerate() {
        run(s, "openpage", json!({"page": page.hex()}))?;
        let symbol = if i % 2 == 0 { "mcb1" } else { "fuse" };
        let v = run(s, "place", json!({"symbol": symbol, "at": [900.0, 120.0 + 30.0 * (i / pages.len().max(1)) as f64]}))?;
        let c = v["component"].as_str().ok_or_else(|| format!("no component in {v}"))?.to_string();
        let value = if i % 2 == 0 { "C16" } else { "10 A" };
        run(s, "setprop", json!({"component": c, "key": "rating", "value": value}))?;
    }
    let (wires, cables): (Vec<WireId>, Vec<CableId>) = project.read(s, |p, _| {
        let doc = p.ledger.document();
        (doc.wires.keys().copied().collect(), doc.cables.keys().copied().collect())
    })?;
    for chunk in cables.chunks(MANY) {
        let many: Vec<_> = chunk.iter().map(|k| json!({"cable": k.hex(), "manual": "16 A", "ambient": "40"})).collect();
        run(s, "rating", json!({"many": many}))?;
    }
    for chunk in wires.iter().step_by(4).copied().collect::<Vec<_>>().chunks(MANY) {
        let many: Vec<_> = chunk.iter().map(|w| json!({"wire": w.hex(), "basis": "manual", "manual": "10 A"})).collect();
        run(s, "rating", json!({"many": many}))?;
    }
    Ok(())
}
