//! The timings. Each is the median of the runs asked for; generating the project
//! is never timed.
//!
//! # Semantics
//! - **open** and **first page** each open the file afresh in every run (from a warm file cache,
//!   taking and releasing the writer lock as the app does).
//! - **edit** and **save** run on one copy of the file, opened once and not timed: each run
//!   commits one add-wire transaction (timed as edit), then saves it (timed as save).
//! - **xref** (cable references) times `ecad_model::connectivity::derive::cable_references` on the
//!   document the open runs replayed to, as **derive** times the nets; see [`cable_references_ms`].
//! - **draw edit** runs on another copy, opened once (not timed) in a headless
//!   CADCraft session with page 1 open, as the desktop app opens it. Each run makes one edit the way a
//!   person makes it, through the engine: **move** moves a component that has wires attached
//!   (MOVE on its symbol; the commit goes through the translation and the movement policy, and
//!   the page is projected again), alternately down and back up; **move with wires** moves the
//!   same component together with every wire attached to it, as a window around them selects
//!   them, so each wire's far end leaves its terminal; **wire** draws one wire between
//!   two terminals with the `wire` command, as the mouse wire tool does.

use std::path::{Path, PathBuf};
use std::time::Instant;

use ecad_model::{Author, Document, IdAllocator};

/// Runs per measurement unless `--runs` says otherwise.
pub const RUNS: usize = 5;
/// Most runs `--runs` accepts.
pub const MAX_RUNS: usize = 1_000;
/// Session seed used when opening (any fixed value).
const OPEN_SEED: u64 = 0xBE_4C11;
/// Seed of the IDs the edit timing allocates: a stream the generator never uses.
const EDIT_SEED: u64 = 0xED17_0012;
/// The writer the lock file names while the benchmark holds a project.
pub const HOLDER: &str = "ecad-bench";

/// What one project measured, times in milliseconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Measured {
    pub bytes: u64,
    pub transactions: u64,
    /// Open the version 2 file to its document (`DiskProject::open`, no CADCraft).
    pub open_ms: f64,
    /// Nets on the opened document (`ecad_model::connectivity::derive::nets`).
    pub derive_ms: f64,
    /// Cable references on the opened document (`ecad_model::connectivity::derive::cable_references`).
    pub reference_ms: Option<f64>,
    /// Open the file in a headless CADCraft session and project page 1 (`ecad-engine`).
    pub first_page_ms: f64,
    /// Commit one add-wire transaction to the loaded project (`Ledger::commit`).
    pub edit_ms: f64,
    /// Write that transaction to the file, durable before it returns (`DiskProject::save`).
    pub save_ms: f64,
    /// Move a component with wires attached on page 1, in a headless CADCraft session.
    pub draw_move_ms: f64,
    /// Move that component together with every wire attached to it, as a window selection picks
    /// them: the far ends leave their connection points.
    pub draw_window_ms: f64,
    /// Draw one wire between two terminals on page 1, in a headless CADCraft session.
    pub draw_wire_ms: f64,
    /// Switch the active page: `openpage` in a headless session
    /// with one page active, until that page is the active page; median over up to
    /// [`SWITCH_PAGES`] pages spread over the project and back to page 1 after each.
    pub switch_ms: f64,
    /// Draw the read-only preview of a page: project it, build and rasterise it at 480 x 320
    /// (`ecad_engine::pages::preview`), median over the same pages.
    pub preview_ms: f64,
    /// Rebuild the Project panel's rows after one `renamepage` (`Project::page_rows`),
    /// median over the same pages.
    pub panel_ms: f64,
    /// Rebuild the Devices tab's list (`device_list::device_entries`, unfiltered), median over the
    /// same pages.
    pub devices_ms: f64,
    /// Go to a device symbol on another page from the Devices tab: `openpage` of its page, then its
    /// handles selected (`device_list::symbol_target`), median over the same pages.
    pub goto_ms: f64,
    /// Bring the Devices tab's list up to date after one device property edit (`setprop`):
    /// `device_list::DeviceList::sync` refreshing only that device, median over the same pages.
    pub devupd_ms: f64,
    /// Build the nets from scratch, as on open (`Connectivity::build`).
    pub nets_ms: f64,
    /// One wire attached at both ends committed with the numbering pass, the Checks list and page
    /// 1's labels brought up to date, then undone the same way.
    pub netupd_ms: f64,
    /// Delete one conductor of the largest net with the numbering pass and the Checks update
    /// (the worst case).
    pub netcut_ms: f64,
    /// How many conductors the net cut by `netcut` had.
    pub netcut_size: usize,
    /// The worst case on a synthetic net: one conductor of a star of
    /// [`RAIL_WIRES`] conductors round one connection point (a 0V rail) deleted, with the
    /// numbering pass and the Checks update. The generated tiers have no net that large.
    pub rail_ms: f64,
    /// Place a starter coil as a new device on page 1 (`place {symbol}`).
    pub place_ms: f64,
    /// Add a NO contact to that device (`place {symbol, device}`).
    pub join_ms: f64,
    /// Rename that device's tag (`setprop` tag).
    pub tag_ms: f64,
    /// Renumber page 2 (`pageprops {number}`): the commit, the redraw of the page shown
    /// and the reference texts naming page 2 (its cables' other ends).
    pub pnum_ms: f64,
    /// Work out one page's frame and title block from the document (`frame::FrameInput::of` and
    /// `frame::parts`): what the first view of a page adds to `switch`.
    pub frame_ms: f64,
    /// Change the project title (`projectprops {title}`): the commit and the title block redrawn.
    pub tbupd_ms: f64,
    /// Change the project's sheet (`projectprops {sheet}`, A3 to A2 and back): the one change that
    /// may rewrite every reference text.
    pub gridchange_ms: f64,
    /// Open an issued revision read-only (`openrevision`): the project rebuilt as at Rev A.
    pub revopen_ms: f64,
    /// On a copy with [`REFERENCE_COILS`] coils added: the reference index
    /// built from scratch, as on open.
    pub xbuild_ms: f64,
    /// Move such a coil (its contacts on other pages): commit, reference update and redraw.
    pub xmove_ms: f64,
    /// Renumber a page with 40 contact targets: the commit and the rows naming it.
    pub xrenum_ms: f64,
    /// Bring a page with 10 contact tables to the front (`openpage`, projected with its tables).
    pub xmirror_ms: f64,
    /// The numbering timings.
    pub numbering: NumberingTimes,
}

/// Renumbering on a copy in a headless session, medians in ms.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NumberingTimes {
    /// `renumber {kind: devices, scope: all, preview}`: the plan, nothing written.
    pub renumprev_ms: f64,
    /// `renumber {kind: devices, scope: all, start: 2}`: every device tag changes, one transaction.
    pub renum_ms: f64,
    /// `renumber {kind: wires, scope: all, start: 2}`: every automatic wire number changes.
    pub renumw_ms: f64,
    /// `u` of the device renumber.
    pub renumu_ms: f64,
    /// `setprop` tag with `conflict: swap` between two devices.
    pub swap_ms: f64,
    /// `finalise` of 50 provisional devices.
    pub finalise_ms: f64,
    /// How many device tags `renum` changed.
    pub renum_count: usize,
}

/// Most pages the switch-page timings visit, spread evenly over the page order.
pub const SWITCH_PAGES: usize = 10;

/// The median of `samples` (0 for none).
pub fn median(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples.get(samples.len() / 2).copied().unwrap_or(0.0)
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

pub(crate) fn path_text(path: &Path) -> Result<&str, String> {
    path.to_str().ok_or_else(|| format!("{} is not valid Unicode", path.display()))
}

/// Open `path` as the app's file layer does; refuse a damaged or read-only open.
fn open(path: &Path) -> Result<ecad_file::disk::OpenedDisk, String> {
    crate::container::open(path)
}

/// The time to derive the cable references of `doc` with
/// `ecad_model::connectivity::derive::cable_references`, in milliseconds. Always `Some` in this revision; the
/// `Option` keeps the report's `—` for a revision without the derivation.
pub fn cable_references_ms(doc: &Document) -> Option<f64> {
    let start = Instant::now();
    let references = ecad_model::connectivity::derive::cable_references(doc);
    let took = ms(start);
    std::hint::black_box(references);
    Some(took)
}

/// Measure the project file at `path` (version 2, the format the app opens), `runs` times each;
/// `scratch` is a folder for the copies the edit and save timings write to. Returns the
/// measurements and the document opening replayed to.
///
/// # Errors
/// The file does not open cleanly, the page cannot be projected, or the edit or save fails.
pub fn measure(path: &Path, runs: usize, scratch: &Path) -> Result<(Measured, Document), String> {
    let app = path;
    let runs = runs.clamp(1, MAX_RUNS);
    let bytes = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();

    let mut open_ms = Vec::with_capacity(runs);
    let mut opened = None;
    for _ in 0..runs {
        // Release the previous open's lock and memory before the next one.
        drop(opened.take());
        let t = Instant::now();
        let o = open(path)?;
        open_ms.push(ms(t));
        opened = Some(o.ledger);
    }
    let ledger = opened.ok_or("nothing was opened")?;
    let transactions = ledger.head();
    let doc = ledger.document().clone();
    drop(ledger);

    let mut derive_ms = Vec::with_capacity(runs);
    let mut reference_ms = Vec::with_capacity(runs);
    for _ in 0..runs {
        let t = Instant::now();
        let nets = ecad_model::connectivity::derive::nets(&doc);
        derive_ms.push(ms(t));
        std::hint::black_box(nets);
        if let Some(x) = cable_references_ms(&doc) {
            reference_ms.push(x);
        }
    }

    let mut first_page_ms = Vec::with_capacity(runs);
    let text = path_text(app)?;
    for _ in 0..runs {
        first_page_ms.push(first_page(text)?);
    }
    release_lock_file(app);

    let (mut edit_ms, mut save_ms) = edit_and_save(path, &scratch.join("edit.ecad"), runs)?;
    let (mut draw_move_ms, mut draw_window_ms, mut draw_wire_ms) = draw_edits(app, &scratch.join("draw.ecad"), runs)?;
    let (mut switch_ms, mut preview_ms, mut panel_ms, mut devices_ms, mut goto_ms, mut devupd_ms) = switch_pages(app, &scratch.join("switch.ecad"))?;
    let (mut place_ms, mut join_ms, mut tag_ms) = device_edits(app, &scratch.join("devices.ecad"), runs)?;
    let mut nets_ms = Vec::with_capacity(runs);
    for _ in 0..runs {
        let t = Instant::now();
        let c = ecad_model::Connectivity::build(&doc);
        nets_ms.push(ms(t));
        std::hint::black_box(c);
    }
    let (mut netupd_ms, mut netcut_ms, netcut_size) = net_edits(path, &scratch.join("nets.ecad"), runs)?;
    // A debug build (the tests) uses a small rail: the release benchmark measures the real one.
    let mut rail_ms = rail_cut(runs, if cfg!(debug_assertions) { 200 } else { RAIL_WIRES })?;
    let mut sheet = sheet_edits(app, &scratch.join("sheets.ecad"), runs)?;
    let numbering = numbering_edits(app, &scratch.join("numbering.ecad"), runs)?;
    let mut xr = device_reference_edits(path, &scratch.join("references.ecad"), runs)?;

    let m = Measured {
        bytes,
        transactions,
        open_ms: median(&mut open_ms),
        derive_ms: median(&mut derive_ms),
        reference_ms: (!reference_ms.is_empty()).then(|| median(&mut reference_ms)),
        first_page_ms: median(&mut first_page_ms),
        edit_ms: median(&mut edit_ms),
        save_ms: median(&mut save_ms),
        draw_move_ms: median(&mut draw_move_ms),
        draw_window_ms: median(&mut draw_window_ms),
        draw_wire_ms: median(&mut draw_wire_ms),
        switch_ms: median(&mut switch_ms),
        preview_ms: median(&mut preview_ms),
        panel_ms: median(&mut panel_ms),
        devices_ms: median(&mut devices_ms),
        goto_ms: median(&mut goto_ms),
        devupd_ms: median(&mut devupd_ms),
        place_ms: median(&mut place_ms),
        join_ms: median(&mut join_ms),
        tag_ms: median(&mut tag_ms),
        nets_ms: median(&mut nets_ms),
        netupd_ms: median(&mut netupd_ms),
        netcut_ms: median(&mut netcut_ms),
        netcut_size,
        rail_ms: median(&mut rail_ms),
        pnum_ms: median(&mut sheet.pnum),
        frame_ms: median(&mut sheet.frame),
        tbupd_ms: median(&mut sheet.tbupd),
        gridchange_ms: median(&mut sheet.gridchange),
        revopen_ms: median(&mut sheet.revopen),
        numbering,
        xbuild_ms: median(&mut xr.build),
        xmove_ms: median(&mut xr.moves),
        xrenum_ms: median(&mut xr.renum),
        xmirror_ms: median(&mut xr.mirror),
    };
    Ok((m, doc))
}

/// Time from nothing to page 1 projected in a fresh headless session.
fn first_page(path: &str) -> Result<f64, String> {
    let mut s = cadcraft_engine::Session::empty();
    let t = Instant::now();
    let (project, _) =
        ecad_engine::ProjectHandle::open_file_in(&mut s, path, OPEN_SEED, ecad_engine::cables::views::Settings::none()).map_err(|e| e.to_string())?;
    let elapsed = ms(t);
    let (pages, read_only) = project.read(&mut s, |p, _| (p.open_pages(), p.file_status().read_only))?;
    if pages != 1 {
        return Err(format!("{path}: expected page 1 shown, found {pages} page windows"));
    }
    if let Some(r) = read_only {
        return Err(format!("{path} opened read-only in the session: {r}"));
    }
    // Dropping the session and the handle releases the project and its writer lock.
    drop(project);
    drop(s);
    Ok(elapsed)
}

/// Copy `path` to `copy` and open it once (not timed). Then per run: commit one add-wire
/// transaction (timed), and save it (timed separately).
fn edit_and_save(path: &Path, copy: &Path, runs: usize) -> Result<(Vec<f64>, Vec<f64>), String> {
    remove(copy);
    std::fs::copy(path, copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        let mut o = open(copy)?;
        let mut ids = IdAllocator::seeded(EDIT_SEED);
        let (mut edit, mut save) = (Vec::with_capacity(runs), Vec::with_capacity(runs));
        for _ in 0..runs {
            let ops = crate::generate::added_wire(o.ledger.document(), &mut ids)?;
            let t = Instant::now();
            o.ledger.commit(Author::User, "Wire", ops).map_err(|e| e.to_string())?;
            edit.push(ms(t));
            let t = Instant::now();
            o.project.save(&mut o.ledger).map_err(|e| e.to_string())?;
            save.push(ms(t));
            if o.ledger.is_unsaved() {
                return Err(format!("{}: the change is still unsaved after saving", copy.display()));
            }
        }
        Ok((edit, save))
    })();
    remove(copy);
    result
}

/// The kept Checks list as the project keeps it: the rating results and the list, built whole and
/// updated by their own views for each commit, noted on the state before and after it (the other
/// views are not kept here, so only the two stages of the Checks list are timed).
struct KeptChecks {
    rating: ecad_electrical::views::RatingView,
    checks: ecad_electrical::views::ChecksView,
    engine: ecad_electrical::views::EngineInputs,
}

impl KeptChecks {
    /// The list of `doc`, built whole (not timed by the callers).
    fn new(doc: &Document, tol: f64) -> Self {
        let engine = ecad_engine::project::engine_views().inputs().clone();
        let rating = ecad_electrical::views::RatingView::build(doc);
        let checks = ecad_electrical::views::ChecksView::build(doc, tol, &*engine.look, engine.registry, rating.kept());
        KeptChecks { rating, checks, engine }
    }

    /// The Before phase: what `ops` name on `before`, the state before them.
    fn before(&self, before: &Document, ops: &[ecad_model::Op]) -> (ecad_electrical::views::RatingScope, ecad_electrical::views::ChecksScope) {
        use ecad_electrical::views::{Inputs, note_view};
        let (mut rating, mut checks) = Default::default();
        let effects = ecad_model::document::effect::Effects::of(ops);
        let cx = ecad_model::document::view::NoteCx { doc: before, phase: ecad_model::document::view::Phase::Before, effects: &effects };
        note_view::<_, Inputs<'_>>(&self.rating, &cx, &mut rating);
        self.rating.close_before(&mut rating);
        note_view::<_, Inputs<'_>>(&self.checks, &cx, &mut checks);
        self.checks.close_before(&mut checks);
        (rating, checks)
    }

    /// The After phase on the committed transaction's ops on `after`, then the updates.
    fn after(
        &mut self, after: &Document, tx_ops: &[ecad_model::Op], scope: (ecad_electrical::views::RatingScope, ecad_electrical::views::ChecksScope),
    ) {
        use ecad_electrical::views::{Inputs, note_view};
        use ecad_model::document::view::DerivedView;
        let (mut rating, mut checks) = scope;
        let effects = ecad_model::document::effect::Effects::of(tx_ops);
        let cx = ecad_model::document::view::NoteCx { doc: after, phase: ecad_model::document::view::Phase::After, effects: &effects };
        note_view::<_, Inputs<'_>>(&self.rating, &cx, &mut rating);
        note_view::<_, Inputs<'_>>(&self.checks, &cx, &mut checks);
        let inputs = Inputs { engine: &self.engine, frame_page: None, rating: None };
        self.rating.update(after, rating, &inputs);
        self.checks.update(after, checks, &Inputs { rating: Some(&self.rating), ..inputs });
    }
}

/// Conductors of the synthetic rail net of [`Measured::rail_ms`].
pub const RAIL_WIRES: usize = 25_000;

/// [`Measured::rail_ms`]: a star net of `wires` conductors ([`RAIL_WIRES`] in the benchmark) from one hub point to their
/// own points, built and numbered once (not timed); per run one leaf conductor is deleted (timed:
/// the commit with the numbering pass and the Checks update) and the deletion undone (not timed).
fn rail_cut(runs: usize, wires: usize) -> Result<Vec<f64>, String> {
    use ecad_model::{ConductorId, ConnectionPoint, ConnectionPointId, Device, DeviceId, Ledger, NetId, Op, Page, PageId, Wire, WireId};
    let mut ids = IdAllocator::seeded(EDIT_SEED ^ 0x0FA1);
    let mut ledger = Ledger::new(7);
    let page: PageId = ids.fresh();
    let comp: DeviceId = ids.fresh();
    let hub: ConnectionPointId = ids.fresh();
    let mut ops = vec![
        Op::AddPage { id: page, page: Page::titled("1").map_err(|e| e.to_string())?, index: 0 },
        Op::CreateDevice { id: comp, device: Device::default() },
        Op::AddConnectionPoint {
            id: hub,
            connection_point: ConnectionPoint { owner: comp.into(), label: None, key: None, accepts: Default::default(), protective_earth: false },
        },
    ];
    let mut leaves = Vec::with_capacity(wires);
    let net: NetId = ids.fresh();
    for i in 0..wires {
        let (t, c, k): (ConnectionPointId, WireId, ConductorId) = (ids.fresh(), ids.fresh(), ids.fresh());
        ops.push(Op::AddConnectionPoint {
            id: t,
            connection_point: ConnectionPoint { owner: comp.into(), label: None, key: None, accepts: Default::default(), protective_earth: false },
        });
        let order = u16::try_from(i + 1).unwrap_or(u16::MAX);
        let wire = Wire { potential: ecad_model::naming::text::Text::new("0V").ok(), net_order: order, ..Wire::new(net, k, hub, t) };
        ops.push(Op::CreateWire { id: c, wire });
        leaves.push(c);
    }
    ledger.commit_extended(Author::User, "Rail", ops, None, ecad_model::connectivity::wiring::assign).map_err(|e| e.to_string())?;
    let tol = 1e-6;
    let mut checks = KeptChecks::new(ledger.document(), tol);
    let mut out = Vec::with_capacity(runs);
    for run in 0..runs {
        let victim = *leaves.get(run * 997 % wires.max(1)).ok_or("no leaf")?;
        let wire = ledger.document().wires().get(&victim).cloned().ok_or("leaf missing")?;
        let ops = vec![Op::DeleteWire { id: victim, wire }];
        let t = Instant::now();
        let scope = checks.before(ledger.document(), &ops);
        ledger.commit_extended(Author::User, "Delete", ops, None, ecad_model::connectivity::wiring::assign).map_err(|e| e.to_string())?;
        let last = ledger.log().last().map(|t| t.ops.clone()).unwrap_or_default();
        checks.after(ledger.document(), &last, scope);
        out.push(ms(t));
        let undo = ledger.undo_ops().unwrap_or_default();
        let scope = checks.before(ledger.document(), &undo);
        ledger.undo(Author::User).map_err(|e| e.to_string())?;
        checks.after(ledger.document(), &undo, scope);
    }
    Ok(out)
}

/// Incremental net updates on a copy opened once (not timed). Per run:
/// - **netupd**: commit one wire attached at both ends with the numbering pass, bring the Checks
///   list (built once, not timed) and page 1's labels up to date, then undo it the same way;
/// - **netcut**: delete one conductor of the largest net (its route and drawing; the commit's
///   rewrite pass then brings the net's wires in line) with the numbering pass and the Checks update; the undo after it is not timed.
///
/// Returns the netupd and netcut samples and the size (conductors) of the net cut.
fn net_edits(path: &Path, copy: &Path, runs: usize) -> Result<(Vec<f64>, Vec<f64>, usize), String> {
    use ecad_model::RouteEnd;
    remove(copy);
    std::fs::copy(path, copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        let mut o = open(copy)?;
        let mut ids = IdAllocator::seeded(EDIT_SEED ^ 0x22);
        let tol = 1e-6;
        let mut checks = KeptChecks::new(o.ledger.document(), tol);
        let page = *o.ledger.document().page_order().first().ok_or("no page")?;
        let (mut upd, mut cut) = (Vec::with_capacity(runs), Vec::with_capacity(runs));
        // The largest net with a wire in no cable, and that wire.
        let (victim, size) = {
            let doc = o.ledger.document();
            let conn = doc.connectivity();
            let free = |n: &ecad_model::NetData| n.wires.iter().copied().find(|c| doc.cables_of(*c).is_empty());
            let (c, net) = conn
                .nets()
                .filter_map(|(_, n)| free(n).map(|c| (c, n)))
                .max_by_key(|(_, n)| n.wires.len())
                .ok_or("no net with a wire outside a cable")?;
            (c, net.wires.len())
        };
        for _ in 0..runs {
            let ops = crate::generate::added_wire(o.ledger.document(), &mut ids)?;
            let t = Instant::now();
            let scope = checks.before(o.ledger.document(), &ops);
            o.ledger.commit_extended(Author::User, "Wire", ops, None, ecad_model::connectivity::wiring::assign).map_err(|e| e.to_string())?;
            let last = o.ledger.log().last().map(|t| t.ops.clone()).unwrap_or_default();
            checks.after(o.ledger.document(), &last, scope);
            std::hint::black_box(ecad_engine::wires::draw::labels_on_page(o.ledger.document(), page));
            let undo = o.ledger.undo_ops().unwrap_or_default();
            let scope = checks.before(o.ledger.document(), &undo);
            o.ledger.undo(Author::User).map_err(|e| e.to_string())?;
            checks.after(o.ledger.document(), &undo, scope);
            upd.push(ms(t));

            // The victim's route goes (with its drawing); the commit's rewrite deletes its wire.
            let doc = o.ledger.document();
            let wire = doc.wires().get(&victim).cloned().ok_or("victim missing")?;
            let [a, b] = wire.ends;
            let route = doc
                .routes_at(a)
                .find(|r| doc.routes().get(r).is_some_and(|x| x.ends.contains(&RouteEnd::Point(b))))
                .ok_or("the victim's route is missing")?;
            let ops = ecad_electrical::wires::routes::remove_route_ops(doc, route);
            let t = Instant::now();
            let scope = checks.before(o.ledger.document(), &ops);
            o.ledger.commit_extended(Author::User, "Delete", ops, None, ecad_model::connectivity::wiring::assign).map_err(|e| e.to_string())?;
            let last = o.ledger.log().last().map(|t| t.ops.clone()).unwrap_or_default();
            checks.after(o.ledger.document(), &last, scope);
            cut.push(ms(t));
            let undo = o.ledger.undo_ops().unwrap_or_default();
            let scope = checks.before(o.ledger.document(), &undo);
            o.ledger.undo(Author::User).map_err(|e| e.to_string())?;
            checks.after(o.ledger.document(), &undo, scope);
        }
        Ok((upd, cut, size))
    })();
    remove(copy);
    result
}

/// What a draw-edit run reads from the session before it edits.
struct DrawTarget {
    head: u64,
    /// The symbol of the first component on page 1 with routes landing on its connection points. By
    /// routes, not wire ends: a move with wires leaves the far ends free, so the
    /// component keeps its routes but not its wires until the next run's move back re-lands them,
    /// and every run must pick the same component.
    symbol: cadcraft_engine::doc::Handle,
    /// That symbol and the routes on page 1 of every wire attached to the component, as a window
    /// around the component and its wiring selects them.
    with_wires: Vec<cadcraft_engine::doc::Handle>,
    /// Two terminal positions on page 1, of other components.
    from: ecad_model::Pt,
    to: ecad_model::Pt,
}

fn draw_target(project: &ecad_engine::ProjectHandle, s: &mut cadcraft_engine::Session) -> Result<DrawTarget, String> {
    use ecad_model::{Form, ItemKind, Subject};
    project
        .read(s, |p, _| -> Result<DrawTarget, &'static str> {
            let doc = p.ledger.document();
            let page = doc.page_order().first().copied().ok_or("the project has no page")?;
            let (component, item) = doc
                .representations()
                .values()
                .find_map(|r| {
                    let Subject::Device(c) = r.subject else { return None };
                    let item = *r.items.first()?;
                    let wired = doc.connection_points_of(c).any(|(t, _)| doc.routes_at(t).next().is_some());
                    (r.page == page
                        && r.form == Form::Symbol
                        && wired
                        && matches!(doc.items().get(&item).map(|i| &i.kind), Some(ItemKind::Symbol(_))))
                    .then_some((c, item))
                })
                .ok_or("page 1 has no component with routes landing on it")?;
            let map = &p.pages.get(&page).ok_or("page 1 is not open")?.map;
            let symbol = map.item(item).ok_or("the wired component's symbol is not drawn on page 1")?;
            let mut with_wires = vec![symbol];
            // The routes landing on its connection points, by their drawing.
            for (t, _) in doc.connection_points_of(component) {
                for r in doc.routes_at(t) {
                    for (_, rep) in doc.representations_of(Subject::Route(r)).filter(|(_, rep)| rep.page == page) {
                        if let Some(h) = rep.items.first().and_then(|i| map.item(*i))
                            && !with_wires.contains(&h)
                        {
                            with_wires.push(h);
                        }
                    }
                }
            }
            let shown = ecad_model::connectivity::derive::connection_point_positions(doc, page);
            let mut others = shown.iter().filter(|(t, _, _)| doc.connection_points().get(t).is_some_and(|x| x.device() != Some(component)));
            let (_, _, from) = *others.next().ok_or("page 1 has no connection point of another component")?;
            let (_, _, to) = *others.find(|(_, _, at)| !at.near(from, 1e-6)).ok_or("page 1 has no second connection point of another component")?;
            Ok(DrawTarget { head: p.ledger.head(), symbol, with_wires, from, to })
        })?
        .map_err(String::from)
}

/// Draw-edit timings: move, move with wires, wire.
type DrawTimes = (Vec<f64>, Vec<f64>, Vec<f64>);

/// Copy `path` to `copy` and open it once in a headless CADCraft session (not timed). Then per
/// run: move a component that has wires attached (timed), then the same component with its wires
/// (timed), and separately draw one wire between two terminals (timed). Every edit must be
/// recorded as exactly one transaction.
fn draw_edits(path: &Path, copy: &Path, runs: usize) -> Result<DrawTimes, String> {
    use serde_json::json;

    remove(copy);
    std::fs::copy(path, copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        ecad_engine::install();
        let mut s = cadcraft_engine::Session::empty();
        let (project, _) =
            ecad_engine::ProjectHandle::open_file_in(&mut s, path_text(copy)?, OPEN_SEED, ecad_engine::cables::views::Settings::none())
                .map_err(|e| e.to_string())?;
        let one_tx = |before: u64, after: u64, what: &str| {
            if after == before + 1 { Ok(()) } else { Err(format!("the {what} was not recorded as one transaction (head {before} → {after})")) }
        };
        let (mut moves, mut windows, mut wires) = (Vec::with_capacity(runs), Vec::with_capacity(runs), Vec::with_capacity(runs));
        for run in 0..runs {
            let target = draw_target(&project, &mut s).map_err(|e| format!("move {}: {e}", run + 1))?;
            let dy = if run % 2 == 0 { -5.0 } else { 5.0 };
            let params = json!({"handles": [target.symbol.hex()], "delta": [0.0, dy]});
            let t = Instant::now();
            s.execute("move", &params).map_err(|e| format!("move {}: {e}", run + 1))?;
            moves.push(ms(t));
            one_tx(target.head, draw_target(&project, &mut s).map_err(|e| format!("after move {}: {e}", run + 1))?.head, "move")?;
        }
        for run in 0..runs {
            let target = draw_target(&project, &mut s).map_err(|e| format!("move with wires {}: {e}", run + 1))?;
            let dy = if run % 2 == 0 { -5.0 } else { 5.0 };
            let handles: Vec<String> = target.with_wires.iter().map(|h| h.hex()).collect();
            let params = json!({"handles": handles, "delta": [0.0, dy]});
            let t = Instant::now();
            s.execute("move", &params).map_err(|e| format!("move with wires {}: {e}", run + 1))?;
            windows.push(ms(t));
            one_tx(
                target.head,
                draw_target(&project, &mut s).map_err(|e| format!("after move with wires {}: {e}", run + 1))?.head,
                "move with wires",
            )?;
        }
        for run in 0..runs {
            let target = draw_target(&project, &mut s).map_err(|e| format!("wire {}: {e}", run + 1))?;
            let (a, b) = (target.from, target.to);
            let params = json!({ "points": [[a.x, a.y], [a.x, b.y], [b.x, b.y]] });
            let t = Instant::now();
            s.execute("wire", &params).map_err(|e| format!("wire {}: {e}", run + 1))?;
            wires.push(ms(t));
            one_tx(target.head, draw_target(&project, &mut s).map_err(|e| format!("after wire {}: {e}", run + 1))?.head, "wire")?;
        }
        // Dropping the session and the handle releases the project and its writer lock.
        drop(project);
        drop(s);
        Ok((moves, windows, wires))
    })();
    remove(copy);
    result
}

/// Copy `path` to `copy` and open it once in a headless CADCraft session (not timed). Then per run:
/// place a starter coil as a new device (timed), add a NO contact to it (timed) and
/// rename its tag (timed). Every edit must be recorded as exactly one transaction.
fn device_edits(path: &Path, copy: &Path, runs: usize) -> Result<DrawTimes, String> {
    use serde_json::json;

    remove(copy);
    std::fs::copy(path, copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        ecad_engine::install();
        let mut s = cadcraft_engine::Session::empty();
        let (project, _) =
            ecad_engine::ProjectHandle::open_file_in(&mut s, path_text(copy)?, OPEN_SEED, ecad_engine::cables::views::Settings::none())
                .map_err(|e| e.to_string())?;
        let head = |s: &mut cadcraft_engine::Session| project.read(s, |p, _| p.ledger.head());
        let timed = |s: &mut cadcraft_engine::Session, cmd: &str, params: serde_json::Value| {
            let before = head(s)?;
            let t = Instant::now();
            let r = s.execute(cmd, &params).map_err(|e| format!("{cmd}: {e}"))?;
            let took = ms(t);
            if head(s)? != before + 1 {
                return Err(format!("the {cmd} was not recorded as one transaction"));
            }
            Ok((took, r))
        };
        let (mut places, mut joins, mut tags) = (Vec::with_capacity(runs), Vec::with_capacity(runs), Vec::with_capacity(runs));
        for run in 0..runs {
            // Far from the generated drawing, so nothing else is touched.
            let x = -1000.0 - 50.0 * run as f64;
            let (t, r) = timed(&mut s, "place", json!({"symbol": "coil", "at": [x, -1000.0]}))?;
            places.push(t);
            let device = r["component"].clone();
            let (t, _) = timed(&mut s, "place", json!({"symbol": "no", "at": [x, -1050.0], "device": device}))?;
            joins.push(t);
            let tag = format!("-KB{:02}", run + 1);
            let (t, _) = timed(&mut s, "setprop", json!({"component": device, "key": "tag", "value": tag}))?;
            tags.push(t);
        }
        drop(project);
        drop(s);
        Ok((places, joins, tags))
    })();
    remove(copy);
    result
}

/// On a copy opened once in a headless session, per run: the device
/// renumber's preview, the renumber itself (start 2, so every tag changes), its undo, the wire
/// renumber (undone untimed), a swap of two tags, and Finalise numbering of 50 provisional devices
/// (placed untimed, undone untimed), after the nets were numbered (untimed). Every write must be
/// one transaction.
fn numbering_edits(path: &Path, copy: &Path, runs: usize) -> Result<NumberingTimes, String> {
    use serde_json::json;

    remove(copy);
    std::fs::copy(path, copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        ecad_engine::install();
        let mut s = cadcraft_engine::Session::empty();
        let (project, _) =
            ecad_engine::ProjectHandle::open_file_in(&mut s, path_text(copy)?, OPEN_SEED, ecad_engine::cables::views::Settings::none())
                .map_err(|e| e.to_string())?;
        let head = |s: &mut cadcraft_engine::Session| project.read(s, |p, _| p.ledger.head());
        let run = |s: &mut cadcraft_engine::Session, cmd: &str, params: serde_json::Value| -> Result<(f64, serde_json::Value), String> {
            let before = head(s)?;
            let t = Instant::now();
            let r = s.execute(cmd, &params).map_err(|e| format!("{cmd}: {e}"))?;
            let took = ms(t);
            let after = head(s)?;
            if params.get("preview").is_none() && after != before + 1 {
                return Err(format!("the {cmd} was not recorded as one transaction"));
            }
            Ok((took, r))
        };
        // The generated tiers' nets carry no wire numbers: number them first (not timed), so that
        // Finalise measures the 50 provisional devices, not every net of the project.
        s.execute("renumberwires", &json!({"scope": "unnumbered"})).map_err(|e| format!("renumberwires: {e}"))?;
        let mut t = NumberingTimes::default();
        let (mut prev, mut renum, mut undo, mut wires, mut swap, mut fin) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
        // Two tagged devices to swap.
        let pair: Vec<String> = project.read(&mut s, |p, _| p.ledger.document().tagged().take(2).map(|(_, c)| c.hex()).collect())?;
        let (a, b) = match pair.as_slice() {
            [a, b] => (a.clone(), b.clone()),
            _ => return Err("the project has fewer than two tagged devices".into()),
        };
        for r in 0..runs {
            let (x, _) = run(&mut s, "renumber", json!({"kind": "devices", "scope": "all", "start": 2, "preview": true}))?;
            prev.push(x);
            let (x, v) = run(&mut s, "renumber", json!({"kind": "devices", "scope": "all", "start": 2}))?;
            renum.push(x);
            t.renum_count = v["devices"].as_u64().unwrap_or(0) as usize;
            let (x, _) = run(&mut s, "u", json!({}))?;
            undo.push(x);
            let (x, _) = run(&mut s, "renumber", json!({"kind": "wires", "scope": "all", "start": 2}))?;
            wires.push(x);
            run(&mut s, "u", json!({}))?;
            let b_tag: String = project.read(&mut s, |p, _| {
                ecad_model::DeviceId::parse(&b)
                    .and_then(|id| p.ledger.document().devices().get(&id))
                    .and_then(|c| c.tag().map(str::to_string))
                    .unwrap_or_default()
            })?;
            let (x, _) = run(&mut s, "setprop", json!({"component": a, "key": "tag", "value": b_tag, "conflict": "swap"}))?;
            swap.push(x);
            run(&mut s, "u", json!({}))?;
            run(&mut s, "numbering", json!({"provisional": true}))?;
            let base = head(&mut s)?;
            for i in 0..50 {
                let at = [-3000.0 - 30.0 * i as f64, -3000.0 - 400.0 * r as f64];
                s.execute("place", &json!({"symbol": "coil", "at": at})).map_err(|e| format!("place: {e}"))?;
            }
            let (x, _) = run(&mut s, "finalise", json!({}))?;
            fin.push(x);
            for _ in 0..(head(&mut s)? - base) {
                s.execute("u", &json!({})).map_err(|e| format!("u: {e}"))?;
            }
            run(&mut s, "numbering", json!({"provisional": false}))?;
        }
        t.renumprev_ms = median(&mut prev);
        t.renum_ms = median(&mut renum);
        t.renumu_ms = median(&mut undo);
        t.renumw_ms = median(&mut wires);
        t.swap_ms = median(&mut swap);
        t.finalise_ms = median(&mut fin);
        drop(project);
        drop(s);
        Ok(t)
    })();
    remove(copy);
    result
}

/// Coils the cross-reference timings add to their copy of the project.
pub const REFERENCE_COILS: usize = 1_000;

/// The device reference timings, one sample per run each.
#[derive(Debug, Default)]
struct DeviceReferenceTimes {
    build: Vec<f64>,
    moves: Vec<f64>,
    renum: Vec<f64>,
    mirror: Vec<f64>,
}

/// The device reference timings on a copy: [`REFERENCE_COILS`] coils are
/// added and saved (not timed; then opened once in a headless session), each with a `power3` on its own page and a NO and an NC
/// on the next page, half of them with an offer (`3P+1NO+1NC`); the first 20 coils' contacts are
/// all on page 2 (40 targets) and coils 20 to 29 on page 3 (10 tables). Then per run:
/// - **xbuild**: the references view (its index) built from scratch, as `Views::rebuild` does on open;
/// - **xmove**: a coil moved on its page (MOVE on its symbol: the commit, the reference update and
///   the redraw), alternately down and back up;
/// - **xrenum**: page 2 renumbered (its 40 contacts' rows on other pages), then back (not timed);
/// - **xmirror**: page 3 brought to the front (projected with its 10 tables), then page 1 again
///   (not timed).
fn device_reference_edits(path: &Path, copy: &Path, runs: usize) -> Result<DeviceReferenceTimes, String> {
    use serde_json::json;

    remove(copy);
    std::fs::copy(path, copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        // The coils, committed to the copy and saved (not timed).
        let (coils, order) = {
            let mut o = open(copy)?;
            let r = add_coils(&mut o.ledger)?;
            o.project.save(&mut o.ledger).map_err(|e| e.to_string())?;
            r
        };
        release_lock_file(copy);
        ecad_engine::install();
        let mut s = cadcraft_engine::Session::empty();
        let (project, _) =
            ecad_engine::ProjectHandle::open_file_in(&mut s, path_text(copy)?, OPEN_SEED, ecad_engine::cables::views::Settings::none())
                .map_err(|e| e.to_string())?;
        let head = |s: &mut cadcraft_engine::Session| project.read(s, |p, _| p.ledger.head());
        let mut out = DeviceReferenceTimes::default();
        for _ in 0..runs {
            let t = project.read(&mut s, |p, _| {
                let t = Instant::now();
                let idx = ecad_electrical::views::ReferencesView::build(p.ledger.document());
                let took = ms(t);
                std::hint::black_box(idx);
                took
            })?;
            out.build.push(t);
        }
        // A coil on page 4 (none of the special pages), its page in front.
        let coil = *coils.get(35).or(coils.last()).ok_or("no coil")?;
        let page = project.read(&mut s, |p, _| p.ledger.document().representations().get(&coil).map(|r| r.page))?.ok_or("coil missing")?;
        s.execute("openpage", &json!({"page": page.hex()})).map_err(|e| e.to_string())?;
        for run in 0..runs {
            let h = project
                .read(&mut s, |p, _| {
                    let item = p.ledger.document().representations().get(&coil)?.items.first().copied()?;
                    p.pages.get(&page)?.map.item(item)
                })?
                .ok_or("the coil is not drawn")?;
            let dy = if run % 2 == 0 { -5.0 } else { 5.0 };
            let before = head(&mut s)?;
            let t = Instant::now();
            s.execute("move", &json!({"handles": [h.hex()], "delta": [0.0, dy]})).map_err(|e| format!("move: {e}"))?;
            out.moves.push(ms(t));
            if head(&mut s)? != before + 1 {
                return Err("the coil move was not recorded as one transaction".into());
            }
        }
        let p2 = order[1 % order.len()];
        let number = project.read(&mut s, |p, _| ecad_model::document::book::page_number(p.ledger.document(), p2))?;
        for _ in 0..runs {
            let t = Instant::now();
            s.execute("pageprops", &json!({"page": p2.hex(), "number": "9002"})).map_err(|e| e.to_string())?;
            out.renum.push(ms(t));
            s.execute("pageprops", &json!({"page": p2.hex(), "number": number})).map_err(|e| e.to_string())?;
        }
        let (p1, p3) = (order[0], order[2 % order.len()]);
        for _ in 0..runs {
            s.execute("openpage", &json!({"page": p1.hex()})).map_err(|e| e.to_string())?;
            let t = Instant::now();
            s.execute("openpage", &json!({"page": p3.hex()})).map_err(|e| e.to_string())?;
            out.mirror.push(ms(t));
        }
        drop(project);
        drop(s);
        Ok(out)
    })();
    release_lock_file(copy);
    remove(copy);
    result
}

/// The coils of [`device_reference_edits`], committed to `ledger` one placement at a time: their head
/// representations and the page order.
pub(crate) fn add_coils(ledger: &mut ecad_model::Ledger) -> Result<(Vec<ecad_model::RepresentationId>, Vec<ecad_model::PageId>), String> {
    use ecad_model::{Color, Op, Placement, Pt, Style};
    use {ecad_electrical::devices::placing::Target, ecad_electrical::devices::placing::plan_place, ecad_electrical::devices::tags::TagHigh};
    let order = ledger.document().page_order().to_vec();
    let n = order.len();
    if n == 0 {
        return Err("the cross-reference timings need a page".into());
    }
    let layer = ledger.document().layers().iter().find(|(_, l)| l.name == "0").map(|(id, _)| *id).ok_or("no layer 0")?;
    // A debug build (the tests) adds fewer coils: the release benchmark adds them all.
    let wanted = if cfg!(debug_assertions) { 40 } else { REFERENCE_COILS };
    let at = |k: usize| order[k % n];
    let style = Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
    let mut ids = IdAllocator::seeded(EDIT_SEED ^ 0x025);
    let mut high = TagHigh::default();
    let starter = |k: &str| ecad_electrical::symbols::catalogue::by_key(k).ok_or_else(|| format!("no starter {k}"));
    let place =
        |pt: (f64, f64)| Placement { origin: Pt::new(pt.0, pt.1), rotation: ecad_model::geometry::units::Radians(0.0), mirror: false, scale: 1.0 };
    let mut coils = Vec::with_capacity(wanted);
    for i in 0..wanted {
        let ka = match i {
            20..=29 => 2,
            _ => (i % n.saturating_sub(1).max(1)) + 1,
        };
        let (a, b) = (at(ka), if i < 20 { at(1) } else { at(ka + 1) });
        let x = 30.0 + 35.0 * (i % 10) as f64;
        let mut step = |ledger: &mut ecad_model::Ledger, page, key: &str, pt, target| -> Result<ecad_electrical::devices::placing::Placed, String> {
            let plan = plan_place(ledger, &mut ids, &mut high, page, style.clone(), starter(key)?, place(pt), target)?;
            ledger.commit(Author::User, "Bench coils", plan.ops.clone()).map_err(|e| e.to_string())?;
            Ok(plan)
        };
        let coil = step(
            ledger,
            a,
            "coil",
            (x, 260.0),
            Target::New(Some(ecad_model::naming::tag::Entry::Tag(ecad_model::naming::tag::Given::new(
                format!("-KX{:04}", i + 1),
                ecad_model::naming::tag::TagState::Fixed,
            )))),
        )?;
        let c = coil.component;
        coils.push(coil.representation);
        step(ledger, a, "power3", (x + 15.0, 230.0), Target::Join(c))?;
        step(ledger, b, "no", (x, 60.0), Target::Join(c))?;
        step(ledger, b, "nc", (x + 10.0, 60.0), Target::Join(c))?;
        if i % 2 == 0 {
            let before = ledger.document().devices().get(&c).cloned().ok_or("the bench coil is missing")?;
            let after = ecad_electrical::references::devices::parse::with_contacts(&before, "the bench coil", Some("3P+1NO+1NC"))
                .map_err(|e| e.to_string())?;
            let op = Op::ModifyDevice { id: c, before, after };
            ledger.commit(Author::User, "Bench coils", vec![op]).map_err(|e| e.to_string())?;
        }
    }
    Ok((coils, order))
}

/// The numbering timings, one sample per run each.
#[derive(Debug, Default)]
struct SheetTimes {
    pnum: Vec<f64>,
    frame: Vec<f64>,
    tbupd: Vec<f64>,
    gridchange: Vec<f64>,
    revopen: Vec<f64>,
}

/// Renumber a page, work out a frame, change the project title, change
/// the grid, open an issued revision; each a recorded command in a headless session on a copy.
fn sheet_edits(path: &Path, copy: &Path, runs: usize) -> Result<SheetTimes, String> {
    use serde_json::json;

    remove(copy);
    std::fs::copy(path, copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        ecad_engine::install();
        let mut s = cadcraft_engine::Session::empty();
        let (project, _) =
            ecad_engine::ProjectHandle::open_file_in(&mut s, path_text(copy)?, OPEN_SEED, ecad_engine::cables::views::Settings::none())
                .map_err(|e| e.to_string())?;
        let run = |s: &mut cadcraft_engine::Session, cmd: &str, params: serde_json::Value| {
            let t = Instant::now();
            s.execute(cmd, &params).map_err(|e| format!("{cmd}: {e}"))?;
            Ok::<f64, String>(ms(t))
        };
        let page2 = project.read(&mut s, |p, _| p.ledger.document().page_order().get(1).copied())?;
        let mut out = SheetTimes::default();
        for i in 0..runs {
            if let Some(p2) = page2 {
                out.pnum.push(run(&mut s, "pageprops", json!({"page": p2.hex(), "number": format!("{}", 9000 + i)}))?);
            }
            let f = project.read(&mut s, |p, _| {
                let doc = p.ledger.document();
                let page = doc.page_order().first().copied();
                let t = Instant::now();
                let n = page.map(|pg| {
                    let input = ecad_engine::pages::frame::FrameInput::of(doc, pg, &ecad_model::document::book::Book::of(doc));
                    ecad_engine::pages::frame::parts(&input).len()
                });
                std::hint::black_box(n);
                ms(t)
            })?;
            out.frame.push(f);
            out.tbupd.push(run(&mut s, "projectprops", json!({"title": format!("Bench {i}")}))?);
            let size = if i % 2 == 0 { "A2" } else { "A3" };
            out.gridchange.push(run(&mut s, "projectprops", json!({"sheet": size}))?);
        }
        run(&mut s, "revision", json!({"description": "Bench issue", "drawn": "BN"}))?;
        run(&mut s, "issuerevision", json!({"date": "2026-11-08"}))?;
        for _ in 0..runs {
            out.revopen.push(run(&mut s, "openrevision", json!({"rev": "A"}))?);
            run(&mut s, "closerevision", json!({}))?;
        }
        drop(project);
        drop(s);
        Ok(out)
    })();
    remove(copy);
    result
}

/// The pages after page 1 to visit: up to [`SWITCH_PAGES`] of them, spread evenly over the order.
fn spread(order: &[ecad_model::PageId]) -> Vec<ecad_model::PageId> {
    let rest = order.get(1..).unwrap_or_default();
    let n = rest.len().min(SWITCH_PAGES);
    (0..n).filter_map(|i| rest.get(i * rest.len() / n.max(1)).copied()).collect()
}

/// Copy `path` to `copy` and open it in a headless CADCraft session with page 1 shown (not timed).
/// Then for each page of [`spread`]: make it the active page with `openpage` (timed as a switch),
/// draw its preview at 480 x 320 (timed), rename it and rebuild the panel's rows (the rebuild
/// timed), and switch back to page 1 (timed as a switch). Switches go through the engine and
/// `Project::open_page`, as the Project panel, `openpage` and Go to Reference do; every switch
/// must leave the asked page active in the one page window.
/// Switch, preview, panel, devices and go-to timings.
type Switches = (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>);

fn switch_pages(path: &Path, copy: &Path) -> Result<Switches, String> {
    use serde_json::json;

    remove(copy);
    std::fs::copy(path, copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        ecad_engine::install();
        let mut s = cadcraft_engine::Session::empty();
        let (project, _) =
            ecad_engine::ProjectHandle::open_file_in(&mut s, path_text(copy)?, OPEN_SEED, ecad_engine::cables::views::Settings::none())
                .map_err(|e| e.to_string())?;
        let order = project.read(&mut s, |p, _| p.ledger.document().page_order().to_vec())?;
        let first = *order.first().ok_or("the project has no pages")?;
        let pages = spread(&order);
        if pages.is_empty() {
            return Err("the project has only one page; nothing to switch to".into());
        }
        let shown = |project: &ecad_engine::ProjectHandle, s: &mut cadcraft_engine::Session| {
            let uid = s.state().map(|st| st.uid).ok();
            project.read(s, |p, _| (uid.and_then(|u| p.page_of(u)), p.open_pages()))
        };
        let n = pages.len();
        let (mut switch, mut preview, mut panel) = (Vec::with_capacity(2 * n), Vec::with_capacity(n), Vec::with_capacity(n));
        let (mut devices, mut goto, mut devupd) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
        // The Devices tab's list, built once as when the tab is shown.
        let mut list = ecad_engine::reports::device_list::DeviceList::default();
        project.read(&mut s, |p, _| list.sync(1, p))?;
        let (w, h) = (480, 320);
        for page in pages {
            let t = Instant::now();
            s.execute("openpage", &json!({"page": page.hex()})).map_err(|e| format!("openpage {page}: {e}"))?;
            switch.push(ms(t));
            if shown(&project, &mut s)? != (Some(page), 1) {
                return Err(format!("openpage {page} did not make the page active in the one page window"));
            }
            // The preview of page 1 from this page: projected under the lock, drawn after it.
            let t = Instant::now();
            let drawing = project.read(&mut s, |p, _| ecad_engine::pages::preview::preview_drawing(p, first))??;
            let view = ecad_engine::pages::preview::PreviewView::extents(&drawing, w, h);
            let image = ecad_engine::pages::preview::rasterise(&drawing, view, w, h, [33, 40, 48], 0.0)?;
            preview.push(ms(t));
            if image.rgba.len() != (w * h * 4) as usize {
                return Err("the preview image has the wrong size".into());
            }
            s.execute("renamepage", &json!({"page": page.hex(), "title": format!("renamed {page}")}))
                .map_err(|e| format!("renamepage {page}: {e}"))?;
            let t = Instant::now();
            let rows = project.read(&mut s, |p, _| p.page_rows())?;
            panel.push(ms(t));
            if rows.len() != order.len() {
                return Err("the panel's rows do not list every page".into());
            }
            // The Devices tab's list, rebuilt.
            let t = Instant::now();
            let listed = project.read(&mut s, |p, _| ecad_electrical::devices::device_list::device_entries(p.ledger.document(), "").len())?;
            devices.push(ms(t));
            if listed == 0 {
                return Err("the Devices list is empty".into());
            }
            // A symbol of a device on this page, to go to from page 1.
            let found = project.read(&mut s, |p, _| {
                p.ledger.document().representations_on(page).find_map(|(id, r)| match r.subject {
                    ecad_model::Subject::Device(c) => Some((id, c)),
                    _ => None,
                })
            })?;
            let rep = found.map(|(r, _)| r);
            // One device property edit, then the list brought up to date (only that device).
            if let Some((_, c)) = found {
                s.execute("setprop", &json!({"component": c.hex(), "key": "description", "value": format!("bench {page}")}))
                    .map_err(|e| format!("setprop {c}: {e}"))?;
                let t = Instant::now();
                let changed = project.read(&mut s, |p, _| list.sync(1, p))?;
                devupd.push(ms(t));
                if !changed || list.rebuilds != 1 {
                    return Err("the Devices list was not refreshed by device".into());
                }
            }
            let t = Instant::now();
            s.execute("openpage", &json!({"page": first.hex()})).map_err(|e| format!("openpage {first}: {e}"))?;
            switch.push(ms(t));
            if shown(&project, &mut s)? != (Some(first), 1) {
                return Err("switching back did not make page 1 active".into());
            }
            if let Some(rep) = rep {
                let t = Instant::now();
                s.execute("openpage", &json!({"page": page.hex()})).map_err(|e| format!("openpage {page}: {e}"))?;
                let handles = project.read(&mut s, |p, _| ecad_engine::reports::device_list::symbol_target(p, rep).map(|x| x.2))?.unwrap_or_default();
                s.set_selection(handles.clone());
                goto.push(ms(t));
                if handles.is_empty() {
                    return Err("the go to selected nothing".into());
                }
                s.execute("openpage", &json!({"page": first.hex()})).map_err(|e| format!("openpage {first}: {e}"))?;
            }
        }
        // Dropping the session and the handle releases the project and its writer lock.
        drop(project);
        drop(s);
        Ok((switch, preview, panel, devices, goto, devupd))
    })();
    remove(copy);
    result
}

/// Remove `path` and its lock file, if present. Only for the benchmark's own scratch files.
pub fn remove(path: &Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(ecad_store::disk::lock_path(path));
}

/// Make `path` free for a new generated project: refuse when another session holds its writer
/// lock (a project open in the desktop app), otherwise remove any earlier file there.
///
/// # Errors
/// Another session holds the lock, or the lock cannot be checked.
pub fn claim(path: &Path) -> Result<(), String> {
    use ecad_store::disk::{LockAttempt, WriterLock};
    match WriterLock::try_acquire(path, HOLDER) {
        Ok(LockAttempt::Acquired { lock, .. }) => {
            let _ = std::fs::remove_file(path);
            // Dropping the lock releases it.
            drop(lock);
            Ok(())
        }
        Ok(LockAttempt::HeldByOther { holder }) => Err(format!(
            "{} is open in another session ({}); close it there first, or choose another --keep folder",
            path.display(),
            holder.as_deref().unwrap_or("holder not shown")
        )),
        Err(e) => Err(format!("{}: cannot check its writer lock: {e}", path.display())),
    }
}

/// Remove the (empty) lock file the benchmark's own opens left beside `path`, once nothing holds
/// it; a lock held by another session is left alone.
pub fn release_lock_file(path: &Path) {
    use ecad_store::disk::{LockAttempt, WriterLock};
    if let Ok(LockAttempt::Acquired { lock, .. }) = WriterLock::try_acquire(path, HOLDER) {
        let lock_file = lock.path().to_path_buf();
        // Release (and close) it first, then remove the file.
        drop(lock);
        let _ = std::fs::remove_file(lock_file);
    }
}

/// A fresh scratch folder under the system temporary folder.
///
/// # Errors
/// It cannot be created.
pub fn scratch_dir(tag: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("ecad-bench-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}
