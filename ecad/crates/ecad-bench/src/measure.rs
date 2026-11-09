//! The timings (brief 012 §3). Each is the median of the runs asked for; generating the project
//! is never timed.
//!
//! # Semantics
//! - **open** and **first page** each open the file afresh in every run (from a warm file cache,
//!   taking and releasing the writer lock as the app does).
//! - **edit** and **save** run on one copy of the file, opened once and not timed: each run
//!   commits one add-wire transaction (timed as edit), then saves it (timed as save).
//! - **xref** (cable references) times brief 010's `ecad_model::derive::cable_references` on the
//!   document the open runs replayed to, as **derive** times the nets; see [`cable_references_ms`].
//! - **draw edit** (brief 014) runs on another copy, opened once (not timed) in a headless
//!   CADCraft session with page 1 open, as the trial opens it. Each run makes one edit the way a
//!   person makes it, through the engine: **move** moves a component that has wires attached
//!   (MOVE on its symbol; the commit goes through the translation and the movement policy, and
//!   the page is projected again), alternately down and back up; **move with wires** moves the
//!   same component together with every wire attached to it, as a window around them selects
//!   them, so each wire's far end leaves its terminal (M3); **wire** draws one wire between
//!   two terminals with the `wire` command, as the mouse wire tool does.

use std::path::{Path, PathBuf};
use std::time::Instant;

use ecad_file::{DiskProject, Outcome};
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
    /// Read from disk, verify every record, replay to the document (`ecad-file`, no CADCraft).
    pub open_ms: f64,
    /// Nets on the opened document (`ecad_model::derive::nets`).
    pub derive_ms: f64,
    /// Cable references on the opened document (`ecad_model::derive::cable_references`, brief 010).
    pub xref_ms: Option<f64>,
    /// Open the file in a headless CADCraft session and project page 1 (`ecad-cadcraft`).
    pub first_page_ms: f64,
    /// Commit one add-wire transaction to the loaded project (`Ledger::commit`).
    pub edit_ms: f64,
    /// Append that transaction to the file and flush (`DiskProject::save`).
    pub save_ms: f64,
    /// Move a component with wires attached on page 1, in a headless CADCraft session (brief 014).
    pub draw_move_ms: f64,
    /// Move that component together with every wire attached to it, as a window selection picks
    /// them: the far ends leave their terminals (brief 014 fix round).
    pub draw_window_ms: f64,
    /// Draw one wire between two terminals on page 1, in a headless CADCraft session (brief 014).
    pub draw_wire_ms: f64,
    /// Switch the active page (brief 016, design 016 section 6): `openpage` in a headless session
    /// with one page active, until that page is the active page; median over up to
    /// [`SWITCH_PAGES`] pages spread over the project and back to page 1 after each.
    pub switch_ms: f64,
    /// Draw the read-only preview of a page: project it, build and rasterise it at 480 x 320
    /// (`ecad_cadcraft::preview`), median over the same pages.
    pub preview_ms: f64,
    /// Rebuild the Project panel's rows after one `renamepage` (`Project::page_rows`, brief 016
    /// AC4), median over the same pages.
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
    /// Build the nets from scratch, as on open (`Connectivity::build`, brief 022).
    pub nets_ms: f64,
    /// One wire attached at both ends committed with the numbering pass, the Checks list and page
    /// 1's labels brought up to date, then undone the same way (brief 022, design 022 §4).
    pub netupd_ms: f64,
    /// Delete one conductor of the largest net with the numbering pass and the Checks update
    /// (brief 022's worst cut).
    pub netcut_ms: f64,
    /// How many conductors the net cut by `netcut` had.
    pub netcut_size: usize,
    /// The worst cut of design 022 §4 on a synthetic net: one conductor of a star of
    /// [`RAIL_WIRES`] conductors round one connection point (a 0V rail) deleted, with the
    /// numbering pass and the Checks update. The generated tiers have no net that large.
    pub rail_ms: f64,
    /// Place a starter coil as a new device on page 1 (`place {symbol}`, brief 018).
    pub place_ms: f64,
    /// Add a NO contact to that device (`place {symbol, device}`, brief 018).
    pub join_ms: f64,
    /// Rename that device's tag (`setprop` tag, brief 018).
    pub tag_ms: f64,
    /// Renumber page 2 (`pageprops {number}`, brief 019): the commit, the redraw of the page shown
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

fn path_text(path: &Path) -> Result<&str, String> {
    path.to_str().ok_or_else(|| format!("{} is not valid Unicode", path.display()))
}

/// Open `path` as the app's file layer does; refuse anything but a clean, writable open.
fn open(path: &Path) -> Result<ecad_file::OpenedDisk, String> {
    let o = DiskProject::open(path, OPEN_SEED, HOLDER).map_err(|e| e.to_string())?;
    if o.report.outcome != Outcome::Clean {
        return Err(format!("{} did not open cleanly: {}", path.display(), o.report.describe()));
    }
    if let Some(r) = o.project.read_only() {
        return Err(format!("{} opened read-only: {r}", path.display()));
    }
    Ok(o)
}

/// The time to derive the cable references of `doc` (brief 010's
/// `ecad_model::derive::cable_references`), in milliseconds. Always `Some` in this revision; the
/// `Option` keeps the report's `—` for a revision without the derivation.
pub fn cable_references_ms(doc: &Document) -> Option<f64> {
    let start = Instant::now();
    let refs = ecad_model::derive::cable_references(doc);
    let took = ms(start);
    std::hint::black_box(refs);
    Some(took)
}

/// Measure the project file at `path`, `runs` times each; `scratch` is a folder for the copy the
/// edit and save timings write to. Returns the measurements and the document opening replayed to.
///
/// # Errors
/// The file does not open cleanly, the page cannot be projected, or the edit or save fails.
pub fn measure(path: &Path, runs: usize, scratch: &Path) -> Result<(Measured, Document), String> {
    let app = scratch.join("measure-app.v2.ecad");
    crate::v2::convert(path, &app)?;
    let r = measure_with(path, &app, runs, scratch);
    release_lock_file(&app);
    remove(&app);
    r
}

/// [`measure`] with the version 2 file the app timings open (since brief 021 the app opens only
/// version 2): `path` (version 1) for open, edit and save; `app` for every timing through the app.
///
/// # Errors
/// As [`measure`].
pub fn measure_with(path: &Path, app: &Path, runs: usize, scratch: &Path) -> Result<(Measured, Document), String> {
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
    let mut xref_ms = Vec::with_capacity(runs);
    for _ in 0..runs {
        let t = Instant::now();
        let nets = ecad_model::derive::nets(&doc);
        derive_ms.push(ms(t));
        std::hint::black_box(nets);
        if let Some(x) = cable_references_ms(&doc) {
            xref_ms.push(x);
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
    let (mut switch_ms, mut preview_ms, mut panel_ms, mut devices_ms, mut goto_ms, mut devupd_ms) =
        switch_pages(app, &scratch.join("switch.ecad"))?;
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

    let m = Measured {
        bytes,
        transactions,
        open_ms: median(&mut open_ms),
        derive_ms: median(&mut derive_ms),
        xref_ms: (!xref_ms.is_empty()).then(|| median(&mut xref_ms)),
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
    };
    Ok((m, doc))
}

/// Time from nothing to page 1 projected in a fresh headless session.
fn first_page(path: &str) -> Result<f64, String> {
    let mut s = cadcraft_engine::Session::empty();
    let t = Instant::now();
    let (project, _) =
        ecad_cadcraft::ProjectHandle::open_file_in(&mut s, path, OPEN_SEED, ecad_cadcraft::views::Settings::none())?;
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
        for run in 0..runs {
            let ops = crate::generate::added_wire(o.ledger.document(), &mut ids, &format!("W-bench-{}", run + 1))?;
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

/// Conductors of the synthetic rail net of [`Measured::rail_ms`].
pub const RAIL_WIRES: usize = 25_000;

/// [`Measured::rail_ms`]: a star net of `wires` conductors ([`RAIL_WIRES`] in the benchmark) from one hub point to their
/// own points, built and numbered once (not timed); per run one leaf conductor is deleted (timed:
/// the commit with the numbering pass and the Checks update) and the deletion undone (not timed).
fn rail_cut(runs: usize, wires: usize) -> Result<Vec<f64>, String> {
    use ecad_model::{Component, ComponentId, Conductor, ConductorId, Ledger, Op, Page, PageId, Terminal, TerminalId};
    let mut ids = IdAllocator::seeded(EDIT_SEED ^ 0x0FA1);
    let mut ledger = Ledger::new(7);
    let page: PageId = ids.fresh();
    let comp: ComponentId = ids.fresh();
    let hub: TerminalId = ids.fresh();
    let mut ops = vec![
        Op::AddPage { id: page, page: Page::new("1"), index: 0 },
        Op::CreateComponent { id: comp, component: Component::default() },
        Op::AddTerminal { id: hub, terminal: Terminal { component: comp, label: None, key: None } },
    ];
    let mut leaves = Vec::with_capacity(wires);
    for _ in 0..wires {
        let (t, c): (TerminalId, ConductorId) = (ids.fresh(), ids.fresh());
        ops.push(Op::AddTerminal { id: t, terminal: Terminal { component: comp, label: None, key: None } });
        let mut conductor = Conductor { ends: [Some(hub), Some(t)], ..Conductor::default() };
        conductor.properties.insert(ecad_model::wiring::POTENTIAL_KEY.into(), "0V".into());
        ops.push(Op::CreateConductor { id: c, conductor });
        leaves.push(c);
    }
    ledger.commit_extended(Author::User, "Rail", ops, None, ecad_model::wiring::assign).map_err(|e| e.to_string())?;
    let tol = 1e-6;
    let mut checks = ecad_cadcraft::checks::Checks::build(ledger.document(), tol);
    let mut out = Vec::with_capacity(runs);
    for run in 0..runs {
        let victim = *leaves.get(run * 997 % wires.max(1)).ok_or("no leaf")?;
        let cond = ledger.document().conductors.get(&victim).cloned().ok_or("leaf missing")?;
        let mut ops = Vec::new();
        for end in ecad_model::End::BOTH {
            if let Some(t) = cond.end(end) {
                ops.push(Op::AttachEnd { conductor: victim, end, before: Some(t), after: None });
            }
        }
        let mut bare = cond.clone();
        bare.ends = [None, None];
        ops.push(Op::DeleteConductor { id: victim, conductor: bare });
        let t = Instant::now();
        let scope = checks.scope(ledger.document(), &ops);
        ledger
            .commit_extended(Author::User, "Delete", ops, None, ecad_model::wiring::assign)
            .map_err(|e| e.to_string())?;
        let last = ledger.log().last().map(|t| t.ops.clone()).unwrap_or_default();
        checks.update(ledger.document(), &last, scope, tol);
        out.push(ms(t));
        let undo = ledger.undo_ops().unwrap_or_default();
        let scope = checks.scope(ledger.document(), &undo);
        ledger.undo(Author::User).map_err(|e| e.to_string())?;
        checks.update(ledger.document(), &undo, scope, tol);
    }
    Ok(out)
}

/// Brief 022's incremental net updates on a copy opened once (not timed). Per run:
/// - **netupd**: commit one wire attached at both ends with the numbering pass, bring the Checks
///   list (built once, not timed) and page 1's labels up to date, then undo it the same way;
/// - **netcut**: delete one conductor of the largest net (its route, its attachments, the
///   conductor) with the numbering pass and the Checks update; the undo after it is not timed.
///
/// Returns the netupd and netcut samples and the size (conductors) of the net cut.
fn net_edits(path: &Path, copy: &Path, runs: usize) -> Result<(Vec<f64>, Vec<f64>, usize), String> {
    use ecad_model::{Op, Subject};
    remove(copy);
    std::fs::copy(path, copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        let mut o = open(copy)?;
        let mut ids = IdAllocator::seeded(EDIT_SEED ^ 0x22);
        let tol = 1e-6;
        let mut checks = ecad_cadcraft::checks::Checks::build(o.ledger.document(), tol);
        let page = *o.ledger.document().page_order.first().ok_or("no page")?;
        let (mut upd, mut cut) = (Vec::with_capacity(runs), Vec::with_capacity(runs));
        // The largest net with a wire in no cable, and that wire.
        let (victim, size) = {
            let doc = o.ledger.document();
            let conn = doc.connectivity();
            let free = |n: &ecad_model::NetData| n.conductors.iter().copied().find(|c| doc.cables_of(*c).is_empty());
            let (c, net) = conn
                .nets()
                .filter_map(|(_, n)| free(n).map(|c| (c, n)))
                .max_by_key(|(_, n)| n.conductors.len())
                .ok_or("no net with a wire outside a cable")?;
            (c, net.conductors.len())
        };
        for run in 0..runs {
            let ops = crate::generate::added_wire(o.ledger.document(), &mut ids, &format!("W-net-{}", run + 1))?;
            let t = Instant::now();
            let scope = checks.scope(o.ledger.document(), &ops);
            o.ledger
                .commit_extended(Author::User, "Wire", ops, None, ecad_model::wiring::assign)
                .map_err(|e| e.to_string())?;
            let last = o.ledger.log().last().map(|t| t.ops.clone()).unwrap_or_default();
            checks.update(o.ledger.document(), &last, scope, tol);
            std::hint::black_box(ecad_cadcraft::wires::labels_on_page(o.ledger.document(), page));
            let undo = o.ledger.undo_ops().unwrap_or_default();
            let scope = checks.scope(o.ledger.document(), &undo);
            o.ledger.undo(Author::User).map_err(|e| e.to_string())?;
            checks.update(o.ledger.document(), &undo, scope, tol);
            upd.push(ms(t));

            let doc = o.ledger.document();
            let mut ops = Vec::new();
            for (rid, r) in doc.representations_of(Subject::Conductor(victim)) {
                ops.push(Op::UnlinkRepresentation { id: rid, rep: r.clone() });
                for i in &r.items {
                    let item = doc.items.get(i).cloned().ok_or("route item missing")?;
                    ops.push(Op::RemoveItem { id: *i, item });
                }
            }
            let cond = doc.conductors.get(&victim).cloned().ok_or("victim missing")?;
            for end in ecad_model::End::BOTH {
                if let Some(t) = cond.end(end) {
                    ops.push(Op::AttachEnd { conductor: victim, end, before: Some(t), after: None });
                }
            }
            let mut bare = cond.clone();
            bare.ends = [None, None];
            ops.push(Op::DeleteConductor { id: victim, conductor: bare });
            let t = Instant::now();
            let scope = checks.scope(o.ledger.document(), &ops);
            o.ledger
                .commit_extended(Author::User, "Delete", ops, None, ecad_model::wiring::assign)
                .map_err(|e| e.to_string())?;
            let last = o.ledger.log().last().map(|t| t.ops.clone()).unwrap_or_default();
            checks.update(o.ledger.document(), &last, scope, tol);
            cut.push(ms(t));
            let undo = o.ledger.undo_ops().unwrap_or_default();
            let scope = checks.scope(o.ledger.document(), &undo);
            o.ledger.undo(Author::User).map_err(|e| e.to_string())?;
            checks.update(o.ledger.document(), &undo, scope, tol);
        }
        Ok((upd, cut, size))
    })();
    remove(copy);
    result
}

/// What a draw-edit run reads from the session before it edits.
struct DrawTarget {
    head: u64,
    /// The symbol of a component on page 1 that has wires attached.
    symbol: cadcraft_engine::doc::Handle,
    /// That symbol and the routes on page 1 of every wire attached to the component, as a window
    /// around the component and its wiring selects them.
    with_wires: Vec<cadcraft_engine::doc::Handle>,
    /// Two terminal positions on page 1, of other components.
    from: ecad_model::Pt,
    to: ecad_model::Pt,
}

fn draw_target(project: &ecad_cadcraft::ProjectHandle, s: &mut cadcraft_engine::Session) -> Result<DrawTarget, String> {
    use ecad_model::{Form, ItemKind, Subject};
    let found = project.read(s, |p, _| {
        let doc = p.ledger.document();
        let page = doc.page_order.first().copied()?;
        let (component, item) = doc.representations.values().find_map(|r| {
            let Subject::Component(c) = r.subject else { return None };
            let item = *r.items.first()?;
            let wired = doc.terminals_of(c).any(|(t, _)| !doc.ends_at(t).is_empty());
            (r.page == page
                && r.form == Form::Symbol
                && wired
                && matches!(doc.items.get(&item).map(|i| &i.kind), Some(ItemKind::Symbol(_))))
            .then_some((c, item))
        })?;
        let map = &p.pages.get(&page)?.map;
        let symbol = map.item(item)?;
        let mut with_wires = vec![symbol];
        for (t, _) in doc.terminals_of(component) {
            for (c, _) in doc.ends_at(t) {
                if let Some(h) = ecad_model::derive::route_on_page(doc, c, page).and_then(|(i, _)| map.item(i))
                    && !with_wires.contains(&h)
                {
                    with_wires.push(h);
                }
            }
        }
        let shown = ecad_model::derive::terminal_positions(doc, page);
        let mut others =
            shown.iter().filter(|(t, _, _)| doc.terminals.get(t).is_some_and(|x| x.component != component));
        let (_, _, from) = *others.next()?;
        let (_, _, to) = *others.find(|(_, _, at)| !at.near(from, 1e-6))?;
        Some(DrawTarget { head: p.ledger.head(), symbol, with_wires, from, to })
    })?;
    found.ok_or_else(|| "page 1 has no wired component and two other terminals to edit".into())
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
        ecad_cadcraft::install();
        let mut s = cadcraft_engine::Session::empty();
        let (project, _) = ecad_cadcraft::ProjectHandle::open_file_in(
            &mut s,
            path_text(copy)?,
            OPEN_SEED,
            ecad_cadcraft::views::Settings::none(),
        )?;
        let one_tx = |before: u64, after: u64, what: &str| {
            if after == before + 1 {
                Ok(())
            } else {
                Err(format!("the {what} was not recorded as one transaction (head {before} → {after})"))
            }
        };
        let (mut moves, mut windows, mut wires) =
            (Vec::with_capacity(runs), Vec::with_capacity(runs), Vec::with_capacity(runs));
        for run in 0..runs {
            let target = draw_target(&project, &mut s)?;
            let dy = if run % 2 == 0 { -5.0 } else { 5.0 };
            let params = json!({"handles": [target.symbol.hex()], "delta": [0.0, dy]});
            let t = Instant::now();
            s.execute("move", &params).map_err(|e| format!("move {}: {e}", run + 1))?;
            moves.push(ms(t));
            one_tx(target.head, draw_target(&project, &mut s)?.head, "move")?;
        }
        for run in 0..runs {
            let target = draw_target(&project, &mut s)?;
            let dy = if run % 2 == 0 { -5.0 } else { 5.0 };
            let handles: Vec<String> = target.with_wires.iter().map(|h| h.hex()).collect();
            let params = json!({"handles": handles, "delta": [0.0, dy]});
            let t = Instant::now();
            s.execute("move", &params).map_err(|e| format!("move with wires {}: {e}", run + 1))?;
            windows.push(ms(t));
            one_tx(target.head, draw_target(&project, &mut s)?.head, "move with wires")?;
        }
        for run in 0..runs {
            let target = draw_target(&project, &mut s)?;
            let (a, b) = (target.from, target.to);
            let params = json!({ "points": [[a.x, a.y], [a.x, b.y], [b.x, b.y]] });
            let t = Instant::now();
            s.execute("wire", &params).map_err(|e| format!("wire {}: {e}", run + 1))?;
            wires.push(ms(t));
            one_tx(target.head, draw_target(&project, &mut s)?.head, "wire")?;
        }
        // Dropping the session and the handle releases the project and its writer lock.
        drop(project);
        drop(s);
        Ok((moves, windows, wires))
    })();
    remove(copy);
    result
}

/// Copy `path` to `copy` and open it once in a headless CADCraft session (not timed). Then per run
/// (brief 018): place a starter coil as a new device (timed), add a NO contact to it (timed) and
/// rename its tag (timed). Every edit must be recorded as exactly one transaction.
fn device_edits(path: &Path, copy: &Path, runs: usize) -> Result<DrawTimes, String> {
    use serde_json::json;

    remove(copy);
    std::fs::copy(path, copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        ecad_cadcraft::install();
        let mut s = cadcraft_engine::Session::empty();
        let (project, _) = ecad_cadcraft::ProjectHandle::open_file_in(
            &mut s,
            path_text(copy)?,
            OPEN_SEED,
            ecad_cadcraft::views::Settings::none(),
        )?;
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
        let (mut places, mut joins, mut tags) =
            (Vec::with_capacity(runs), Vec::with_capacity(runs), Vec::with_capacity(runs));
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

/// Brief 019's timings, one sample per run each.
#[derive(Debug, Default)]
struct SheetTimes {
    pnum: Vec<f64>,
    frame: Vec<f64>,
    tbupd: Vec<f64>,
    gridchange: Vec<f64>,
    revopen: Vec<f64>,
}

/// Brief 019 (design 019 §9): renumber a page, work out a frame, change the project title, change
/// the grid, open an issued revision; each a recorded command in a headless session on a copy.
fn sheet_edits(path: &Path, copy: &Path, runs: usize) -> Result<SheetTimes, String> {
    use serde_json::json;

    remove(copy);
    std::fs::copy(path, copy).map_err(|e| format!("copying {} to {}: {e}", path.display(), copy.display()))?;
    let result = (|| {
        ecad_cadcraft::install();
        let mut s = cadcraft_engine::Session::empty();
        let (project, _) = ecad_cadcraft::ProjectHandle::open_file_in(
            &mut s,
            path_text(copy)?,
            OPEN_SEED,
            ecad_cadcraft::views::Settings::none(),
        )?;
        let run = |s: &mut cadcraft_engine::Session, cmd: &str, params: serde_json::Value| {
            let t = Instant::now();
            s.execute(cmd, &params).map_err(|e| format!("{cmd}: {e}"))?;
            Ok::<f64, String>(ms(t))
        };
        let page2 = project.read(&mut s, |p, _| p.ledger.document().page_order.get(1).copied())?;
        let mut out = SheetTimes::default();
        for i in 0..runs {
            if let Some(p2) = page2 {
                out.pnum.push(run(&mut s, "pageprops", json!({"page": p2.hex(), "number": format!("{}", 9000 + i)}))?);
            }
            let f = project.read(&mut s, |p, _| {
                let doc = p.ledger.document();
                let page = doc.page_order.first().copied();
                let t = Instant::now();
                let n = page.map(|pg| {
                    let input = ecad_cadcraft::frame::FrameInput::of(doc, pg, &ecad_model::book::Book::of(doc));
                    ecad_cadcraft::frame::parts(&input).len()
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
        ecad_cadcraft::install();
        let mut s = cadcraft_engine::Session::empty();
        let (project, _) = ecad_cadcraft::ProjectHandle::open_file_in(
            &mut s,
            path_text(copy)?,
            OPEN_SEED,
            ecad_cadcraft::views::Settings::none(),
        )?;
        let order = project.read(&mut s, |p, _| p.ledger.document().page_order.clone())?;
        let first = *order.first().ok_or("the project has no pages")?;
        let pages = spread(&order);
        if pages.is_empty() {
            return Err("the project has only one page; nothing to switch to".into());
        }
        let shown = |project: &ecad_cadcraft::ProjectHandle, s: &mut cadcraft_engine::Session| {
            let uid = s.state().map(|st| st.uid).ok();
            project.read(s, |p, _| (uid.and_then(|u| p.page_of(u)), p.open_pages()))
        };
        let n = pages.len();
        let (mut switch, mut preview, mut panel) =
            (Vec::with_capacity(2 * n), Vec::with_capacity(n), Vec::with_capacity(n));
        let (mut devices, mut goto, mut devupd) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
        // The Devices tab's list, built once as when the tab is shown.
        let mut list = ecad_cadcraft::device_list::DeviceList::default();
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
            let drawing = project.read(&mut s, |p, _| ecad_cadcraft::preview::preview_drawing(p, first))??;
            let view = ecad_cadcraft::preview::PreviewView::extents(&drawing, w, h);
            let image = ecad_cadcraft::preview::rasterise(&drawing, view, w, h, [33, 40, 48], 0.0)?;
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
            let listed = project
                .read(&mut s, |p, _| ecad_cadcraft::device_list::device_entries(p.ledger.document(), "").len())?;
            devices.push(ms(t));
            if listed == 0 {
                return Err("the Devices list is empty".into());
            }
            // A symbol of a device on this page, to go to from page 1.
            let found = project.read(&mut s, |p, _| {
                p.ledger.document().representations_on(page).find_map(|(id, r)| match r.subject {
                    ecad_model::Subject::Component(c) => Some((id, c)),
                    _ => None,
                })
            })?;
            let rep = found.map(|(r, _)| r);
            // One device property edit, then the list brought up to date (only that device).
            if let Some((_, c)) = found {
                s.execute(
                    "setprop",
                    &json!({"component": c.hex(), "key": "description", "value": format!("bench {page}")}),
                )
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
                let handles = project
                    .read(&mut s, |p, _| ecad_cadcraft::device_list::symbol_target(p, rep).map(|x| x.2))?
                    .unwrap_or_default();
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
/// lock (a project open in the trial), otherwise remove any earlier file there.
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
