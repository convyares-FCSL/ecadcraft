//! [`crate::Variant::Rich`]: the lean project of a tier plus the domain's other
//! objects, so the views that hold them (connectors, PLC, terminal strips, the cover, ratings,
//! cross-references with contacts) are measured on data and not on an empty corpus.
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

/// Why a rich project (or a view timing) could not be made.
#[derive(Debug, thiserror::Error)]
pub enum CorpusError {
    /// The lean base, the coils or the file container failed.
    #[error("rich corpus, base project: {0}")]
    Base(String),
    /// A scratch folder, the open or a read of the project failed.
    #[error("rich corpus, project session: {0}")]
    Project(String),
    /// A command of the enrichment was refused.
    #[error("rich corpus, command {command} {params}: {reason}")]
    Command { command: String, params: String, reason: String },
    /// One of the bench parts' population steps failed.
    #[error("rich corpus, {part} population: {reason}")]
    Populate { part: &'static str, reason: String },
    /// The project has no page to put things on.
    #[error("rich corpus: the project has no pages")]
    NoPages,
    /// A command's answer lacked the device it made.
    #[error("rich corpus: no component in the answer {0}")]
    NoComponent(String),
}

fn run(s: &mut Session, cmd: &str, params: serde_json::Value) -> Result<serde_json::Value, CorpusError> {
    s.execute(cmd, &params).map_err(|e| CorpusError::Command { command: cmd.into(), params: params.to_string(), reason: e.to_string() })
}

/// Generate the rich project of `tier`.
///
/// # Errors
/// The lean base fails to generate, or any command of the enrichment is refused.
pub fn generate_rich(tier: Tier) -> Result<Generated, CorpusError> {
    let mut g = generate(tier, Variant::Lean).map_err(CorpusError::Base)?;
    // The coils with contacts go straight on the ledger, as the references bench does.
    crate::measure::add_coils(&mut g.ledger).map_err(CorpusError::Base)?;
    // A folder of its own: two projects generated in one process (tests) never share a file.
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = scratch_dir(&format!("rich-{}-{}", tier.name, N.fetch_add(1, std::sync::atomic::Ordering::Relaxed))).map_err(CorpusError::Project)?;
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
pub fn write_rich(tier: Tier, path: &std::path::Path) -> Result<(), CorpusError> {
    let mut g = generate(tier, Variant::Lean).map_err(CorpusError::Base)?;
    crate::measure::add_coils(&mut g.ledger).map_err(CorpusError::Base)?;
    remove(path);
    let result = enrich(&g, path).map(drop);
    release_lock_file(path);
    result
}

fn enrich(g: &Generated, path: &std::path::Path) -> Result<ecad_model::Ledger, CorpusError> {
    crate::container::write(g, path).map_err(CorpusError::Base)?;
    let _ = ecad_engine::install();
    let mut s = Session::empty();
    let (project, _) =
        ProjectHandle::open_file_in(&mut s, path_text(path).map_err(CorpusError::Project)?, OPEN_SEED, ecad_engine::cables::views::Settings::none())
            .map_err(CorpusError::Project)?;
    let pages: Vec<PageId> = project.read(&mut s, |p, _| p.ledger.document().page_order().to_vec()).map_err(CorpusError::Project)?;
    let page1 = *pages.first().ok_or(CorpusError::NoPages)?;
    run(&mut s, "openpage", json!({"page": page1.hex()}))?;
    // Terminal strips (with levels, accessories and jumper bars) and PLC racks go straight on the
    // ledger, as their bench parts do.
    crate::strips::populate(&project, &mut s).map_err(|reason| CorpusError::Populate { part: "strips", reason })?;
    crate::plc::populate(&project, &mut s).map_err(|reason| CorpusError::Populate { part: "PLC", reason })?;
    run(&mut s, "openpage", json!({"page": page1.hex()}))?;
    crate::connectors::populate(&mut s, &pages).map(drop).map_err(|reason| CorpusError::Populate { part: "connectors", reason })?;
    levels(&mut s, &pages)?;
    run(&mut s, "newpage", json!({"type": "front", "generated": "cover"}))?;
    ratings(&project, &mut s, &pages)?;
    run(&mut s, "openpage", json!({"page": page1.hex()}))?;
    run(&mut s, "qsave", json!({}))?;
    project.read(&mut s, |p, _| p.ledger.clone()).map_err(CorpusError::Project)
}

/// A level strip (`LNPE`) on page 2 or 1: Place Terminals across new wires.
fn levels(s: &mut Session, pages: &[PageId]) -> Result<(), CorpusError> {
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
fn ratings(project: &ProjectHandle, s: &mut Session, pages: &[PageId]) -> Result<(), CorpusError> {
    for cat in ["l", "l1", "l2", "l3", "n"] {
        run(s, "setstyle", json!({"catalogue": "wire", "id": cat, "fields": {"rated_current": "27"}}))?;
    }
    run(s, "setdefault", json!({"key": "rating.bundling", "level": "project", "value": "0.8"}))?;
    for (i, page) in pages.iter().take(PROTECTIVE).enumerate() {
        run(s, "openpage", json!({"page": page.hex()}))?;
        let symbol = if i % 2 == 0 { "mcb1" } else { "fuse" };
        let v = run(s, "place", json!({"symbol": symbol, "at": [900.0, 120.0 + 30.0 * (i / pages.len().max(1)) as f64]}))?;
        let c = v["component"].as_str().ok_or_else(|| CorpusError::NoComponent(v.to_string()))?.to_string();
        let value = if i % 2 == 0 { "C16" } else { "10 A" };
        run(s, "setprop", json!({"component": c, "key": "rating", "value": value}))?;
    }
    let (wires, cables): (Vec<WireId>, Vec<CableId>) = project
        .read(s, |p, _| {
            let doc = p.ledger.document();
            (doc.wires().keys().copied().collect(), doc.cables().keys().copied().collect())
        })
        .map_err(CorpusError::Project)?;
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
