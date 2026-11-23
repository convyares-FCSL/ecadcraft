//! The developer debugger panel: read-only, opened from the
//! ECAD Dev tool set (Tool Sets), never shown by default.
//!
//! It follows one trail: **selected drawing objects → ECAD IDs → model objects → the transactions
//! that touched them, with before/after values → the saved record's offset and bytes**, and keeps
//! pending changes (in memory only) visibly apart from durably saved history.
//!
//! Everything shown comes from a [`DebugSnapshot`] copied under one short lock
//! (`ProjectHandle::debug_snapshot`), released before anything is drawn. The snapshot is rebuilt
//! only when the project, its saved position, its runtime events or the selection change, or on
//! Refresh; a busy project is never waited for (the previous snapshot stays, marked stale).
//! Replay, projection, file and index checks run only when their button is pressed: their inputs
//! are copied (the project locked only for the copy), the check runs on the background pool
//! ([`ecad_engine::background`]) and its result is taken on a later frame, or dropped if the project's
//! head moved meanwhile. History at an earlier transaction is rebuilt from the panel's own copy of
//! the log.

use std::collections::BTreeSet;
use std::path::Path;

use ecad_engine::ProjectHandle;
use ecad_engine::background::{At, Jobs};
use ecad_engine::cad::Session;
use ecad_engine::dev::checks::{self, Check, RecordBytes};
use ecad_engine::dev::debug::{DebugSnapshot, ObjectId, RecordState, TxCursor};
use ecad_engine::dev::objects::{self, Resolved, op_view, resolve};
use ecad_model::{Author, Document, ProjectId, Transaction};
use ecad_widgets::control::{Select, TextEntry};
use ecad_widgets::prim::{self, Button, IconButton, Kind, ListRow, SectionHeader, Tone};
use ecad_widgets::tokens::{radius, size, space};
use eframe::egui::{self, RichText};
use {ecad_widgets::Icon, ecad_widgets::SHELL, ecad_widgets::color};

/// The ECAD Dev Debugger button's hover text.
pub(crate) const HOVER: &str = "Read-only developer debugger: selection → ECAD IDs → model → transactions → saved records";

/// Rows per page in long lists.
pub const PAGE: usize = 100;
/// Longest JSON text shown for one value.
const MAX_JSON: usize = 20_000;
/// Ops shown for one transaction.
const MAX_OPS: usize = 200;

/// The panel's views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Selection,
    Model,
    Definitions,
    History,
    Persistence,
    /// The version 2 file's tables and snapshots.
    Storage,
    /// The version 2 file's branches.
    Branches,
    Diagnostics,
    Checks,
}

impl View {
    pub const ALL: [View; 9] = [
        View::Selection,
        View::Model,
        View::Definitions,
        View::History,
        View::Persistence,
        View::Storage,
        View::Branches,
        View::Diagnostics,
        View::Checks,
    ];

    fn title(self) -> &'static str {
        match self {
            View::Selection => "Selection",
            View::Model => "Model",
            View::Definitions => "Definitions",
            View::History => "History",
            View::Persistence => "Persistence",
            View::Storage => "Storage",
            View::Branches => "Branches",
            View::Diagnostics => "Diagnostics",
            View::Checks => "Checks",
        }
    }
}

/// An explicit check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckKind {
    /// Replay the history and compare with the live document.
    Replay,
    /// Reproject the open pages and compare with their drawings.
    Projection,
    /// Re-read the file and check frames, checksums, hash chain and history.
    File,
    /// Rebuild the reference index and compare it with the kept one.
    Index,
}

/// What the panel asks the app to do after drawing (things that need the project).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Refresh,
    Close,
    Check(CheckKind),
    /// Read the file's table sizes and version rows (on request: it walks the tables).
    ReadStorage,
}

/// A check that ran, and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ran {
    pub check: Check,
    pub time: String,
}

/// The state rebuilt for "History at seq n".
#[derive(Debug, Clone)]
pub struct Historical {
    pub seq: u64,
    pub doc: Result<Document, String>,
    pub focus: Option<ObjectId>,
}

/// The debugger panel's state. Nothing in it refers to the live project: it holds copies.
pub struct Debugger {
    pub open: bool,
    pub view: View,
    /// The panel's copy of the history (transactions never change once recorded).
    log: Vec<Transaction>,
    snap: Option<DebugSnapshot>,
    /// When the shown snapshot was taken.
    taken: String,
    /// Why the shown snapshot may be out of date (the project was busy at the last attempt).
    stale: Option<String>,
    /// How many snapshots were taken (for tests and the heading).
    pub builds: u64,
    /// The objects the trail follows, and what they were reached from.
    focus: Vec<ObjectId>,
    focus_from: String,
    /// The transaction the trail shows.
    tx: Option<u64>,
    record: Option<Result<RecordBytes, String>>,
    query: String,
    list_page: usize,
    history_page: usize,
    events_page: usize,
    /// The `Cable references` diagnostics, derived once per snapshot.
    ref_lines: Vec<String>,
    references_page: usize,
    at_text: String,
    historical: Option<Historical>,
    checks: Vec<Ran>,
    /// Checks running on the background pool.
    jobs: Jobs<Check>,
    /// The Storage view's file read, on request.
    storage: Option<(Result<checks::StorageRead, String>, String)>,
    /// The Branches view's rows, read after each save (a few rows), and the saved position they
    /// were read at.
    branches: Option<Result<Vec<checks::BranchRow>, String>>,
    branches_at: Option<(ProjectId, u64)>,
}

impl Default for Debugger {
    fn default() -> Self {
        Self {
            open: false,
            view: View::Selection,
            log: Vec::new(),
            snap: None,
            taken: String::new(),
            stale: None,
            builds: 0,
            focus: Vec::new(),
            focus_from: String::new(),
            tx: None,
            record: None,
            query: String::new(),
            list_page: 0,
            history_page: 0,
            events_page: 0,
            ref_lines: Vec::new(),
            references_page: 0,
            at_text: String::new(),
            historical: None,
            checks: Vec::new(),
            jobs: Jobs::default(),
            storage: None,
            branches: None,
            branches_at: None,
        }
    }
}

fn now() -> String {
    let secs = (crate::shell::host::host().now_ms)() / 1000;
    ecad_file::utc(secs)
}

fn json_text(v: &serde_json::Value) -> String {
    let mut s = serde_json::to_string_pretty(v).unwrap_or_else(|e| format!("(cannot show: {e})"));
    if s.len() > MAX_JSON {
        let mut cut = MAX_JSON;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
        s.push_str("\n… (cut)");
    }
    s
}

fn author(a: &Author) -> String {
    match a {
        Author::User => "user".into(),
        Author::Agent(n) => format!("agent {n}"),
    }
}

impl Debugger {
    /// Forget everything (another project was opened).
    pub fn reset(&mut self) {
        let open = self.open;
        *self = Self { open, ..Self::default() };
    }

    /// The shown snapshot.
    pub fn snapshot(&self) -> Option<&DebugSnapshot> {
        self.snap.as_ref()
    }

    /// The panel's copy of the history.
    pub fn log(&self) -> &[Transaction] {
        &self.log
    }

    /// Why the shown snapshot may be out of date.
    pub fn stale(&self) -> Option<&str> {
        self.stale.as_deref()
    }

    /// The objects the trail follows.
    pub fn focus(&self) -> &[ObjectId] {
        &self.focus
    }

    /// The transaction the trail shows.
    pub fn selected_tx(&self) -> Option<u64> {
        self.tx
    }

    /// The record bytes read for the trail's transaction.
    pub fn record(&self) -> Option<&Result<RecordBytes, String>> {
        self.record.as_ref()
    }

    /// Checks run so far, oldest first.
    pub fn checks(&self) -> &[Ran] {
        &self.checks
    }

    /// The rebuilt earlier state, if one is shown.
    pub fn historical(&self) -> Option<&Historical> {
        self.historical.as_ref()
    }

    /// Take a new snapshot if anything changed (always, with `force`). The project is locked only
    /// while it is copied; a busy project keeps the previous snapshot, marked stale.
    pub fn refresh(&mut self, project: &ProjectHandle, s: &mut Session, force: bool) {
        let known = if force { None } else { self.snap.as_ref().map(|x| x.key.clone()) };
        let have = TxCursor { len: self.log.len() as u64, last: self.log.last().map(|t| t.id) };
        match project.debug_snapshot(s, known.as_ref(), have) {
            Ok(None) => self.stale = None,
            Ok(Some(snap)) => self.take(snap),
            Err(e) => self.stale = Some(e),
        }
        self.collect();
    }

    /// Take the checks that finished at the state shown; a result computed at an earlier head is
    /// dropped. Nothing is taken while the snapshot is stale (the head may have moved unseen).
    pub fn collect(&mut self) {
        let Some(snap) = self.snap.as_ref().filter(|_| self.stale.is_none()) else { return };
        let at = At { project: snap.project, head: snap.head };
        for r in self.jobs.take(at) {
            let check = r.unwrap_or_else(|e| Check { what: "check", at_seq: at.head, ok: false, lines: vec![e.to_string()] });
            self.checks.push(Ran { check, time: now() });
        }
    }

    /// Checks started and not yet taken.
    pub fn running_checks(&self) -> usize {
        self.jobs.running()
    }

    /// Check results dropped because the project's head moved while they ran.
    pub fn dropped_checks(&self) -> u64 {
        self.jobs.dropped()
    }

    /// Wait for the running checks, at most `timeout` (tests and headless runs; the app never
    /// waits); their results are taken by the next [`Debugger::refresh`]. Whether all finished.
    pub fn wait_for_checks(&mut self, timeout: std::time::Duration) -> bool {
        self.jobs.wait(timeout)
    }

    /// Install a new snapshot.
    pub fn take(&mut self, mut snap: DebugSnapshot) {
        let other = self.snap.as_ref().is_some_and(|old| old.project != snap.project || old.base != snap.base);
        if snap.log_from == 0 || other {
            self.log.clear();
        }
        if other {
            self.storage = None;
        }
        // The Branches view follows saves: its few rows are read again when the saved position
        // moves (outside the project lock: the snapshot is already a copy).
        let at = (snap.project, snap.persisted);
        if self.branches_at != Some(at) {
            self.branches = snap.store_arc().map(|s| checks::read_branches(&s));
            self.branches_at = Some(at);
        }
        self.log.truncate(usize::try_from(snap.log_from).unwrap_or(usize::MAX));
        self.log.append(&mut snap.new_transactions);
        if self.tx.is_some_and(|t| t > snap.head) {
            self.tx = None;
            self.record = None;
        }
        self.ref_lines = ecad_electrical::references::cables::diagnostics(&snap.doc);
        self.snap = Some(snap);
        self.taken = now();
        self.stale = None;
        self.builds += 1;
    }

    /// Follow selected object `i` of the snapshot into the model.
    pub fn follow_selection(&mut self, i: usize) {
        let Some(snap) = &self.snap else { return };
        let Some(sel) = snap.selection.get(i) else { return };
        match sel.mapped {
            Some(m) => {
                let r = resolve(&snap.doc, m);
                self.focus = r.ids;
                self.focus_from = format!("handle {} ({})", sel.handle.hex(), sel.entity);
            }
            None => {
                self.focus.clear();
                self.focus_from = format!("handle {} ({}) is not an ECAD object", sel.handle.hex(), sel.entity);
            }
        }
        self.select_tx(None);
    }

    /// Follow one model object.
    pub fn follow_object(&mut self, id: ObjectId) {
        self.focus = vec![id];
        self.focus_from = format!("{} {}", id.kind(), id.short());
        self.select_tx(None);
    }

    /// Show transaction `seq` in the trail.
    pub fn select_tx(&mut self, seq: Option<u64>) {
        self.tx = seq;
        self.record = None;
    }

    /// The transactions that touched the followed objects, oldest first.
    pub fn touching(&self) -> Vec<u64> {
        let ids: BTreeSet<ObjectId> = self.focus.iter().copied().collect();
        if ids.is_empty() {
            return Vec::new();
        }
        self.log.iter().filter(|t| objects::touches(t, &ids)).map(|t| t.seq).collect()
    }

    /// Read the trail transaction's row from the project file (no project lock).
    pub fn read_row(&mut self) {
        let (Some(snap), Some(seq)) = (&self.snap, self.tx) else { return };
        let Some(store) = snap.store_arc() else {
            self.record = Some(Err("the project has no file yet".into()));
            return;
        };
        if !snap.record_state(seq).in_file() {
            self.record = Some(Err(format!("transaction {seq} is not in the file yet")));
            return;
        }
        self.record = Some(checks::read_row(&store, seq, snap.tx_in(&self.log, seq)));
    }

    /// Rebuild the document after transaction `seq` from the panel's copy of the history (and,
    /// before the snapshot the project was opened from, from the file).
    pub fn show_at(&mut self, seq: u64) {
        let Some(snap) = &self.snap else { return };
        let doc = checks::state_at_from(snap.base, snap.history.as_ref(), &self.log, seq);
        self.historical = Some(Historical { seq, doc, focus: None });
    }

    /// Read the Storage view's table sizes and version rows (on request).
    pub fn read_storage(&mut self) {
        let Some(snap) = &self.snap else { return };
        let read = match snap.store_arc() {
            Some(st) => checks::read_storage(&st, snap.file.path.as_deref().map(Path::new)),
            None => Err("the project has no file yet".into()),
        };
        self.storage = Some((read, now()));
    }

    /// Start a check (only ever on request): its inputs are copied now, under one short lock where
    /// the snapshot does not hold them, and it runs on the background pool.
    pub fn run_check(&mut self, kind: CheckKind, project: &ProjectHandle, s: &mut Session) {
        let Some(snap) = &self.snap else { return };
        let at = At { project: snap.project, head: snap.head };
        let failed = |what: &'static str, e: String| Check { what, at_seq: at.head, ok: false, lines: vec![e] };
        match kind {
            CheckKind::Replay => {
                let (base, history, log, doc) = (snap.base, snap.history.clone(), self.log.clone(), snap.doc.clone());
                self.jobs.spawn(at, move || checks::verify_replay_from(base, history.as_ref(), &log, &doc));
            }
            CheckKind::Projection => match project.debug_projection_inputs(s) {
                Ok(inputs) => {
                    let at = At { head: inputs.head, ..at };
                    self.jobs.spawn(at, move || checks::verify_projection(&inputs));
                }
                Err(e) => self.checks.push(Ran { check: failed("projection", e), time: now() }),
            },
            CheckKind::File => match snap.store_arc() {
                Some(st) => {
                    let (base, log, persisted) = (snap.base, self.log.clone(), snap.persisted);
                    self.jobs.spawn(at, move || checks::verify_store(&st, base, &log, persisted));
                }
                None => self.checks.push(Ran { check: failed("file", "the project has no file yet".into()), time: now() }),
            },
            CheckKind::Index => match project.read(s, |p, _| (p.ledger.document().clone(), p.reference().clone(), p.ledger.head())) {
                Ok((doc, idx, head)) => self.jobs.spawn(At { head, ..at }, move || checks::verify_index(&doc, &idx, head)),
                Err(e) => self.checks.push(Ran { check: failed("reference index", e), time: now() }),
            },
        }
    }

    /// Apply a panel action that needs the project.
    pub fn apply(&mut self, a: Action, project: &ProjectHandle, s: &mut Session) {
        match a {
            Action::Refresh => self.refresh(project, s, true),
            Action::Close => self.open = false,
            Action::Check(k) => self.run_check(k, project, s),
            Action::ReadStorage => self.read_storage(),
        }
    }

    /// Draw the panel. Returns what needs the project.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        ui.horizontal(|ui| {
            prim::body(ui, "ECAD debugger (read-only)", false);
            if ui.add(Button::new("Refresh").small()).on_hover_text("Take a new snapshot now").clicked() {
                action = Some(Action::Refresh);
            }
            if ui.add(Button::new("Close").kind(Kind::Quiet).small()).clicked() {
                action = Some(Action::Close);
            }
        });
        let Some(snap) = &self.snap else {
            let words = match &self.stale {
                Some(e) => format!("No snapshot yet: {e}"),
                None => "No snapshot yet.".to_string(),
            };
            prim::body(ui, &words, true);
            return action;
        };
        prim::body(ui, &format!("Snapshot: {} of \"{}\", taken {} (#{}).", snap.label(), snap.name, self.taken, self.builds), false);
        prim::body(ui, &saved_summary(snap), false);
        if let Some(e) = &self.stale {
            prim::note(ui, &format!("Stale: {e}. Showing the snapshot above."), Tone::Warning);
        }
        // Nine views: a selector rather than a segmented choice (more than about five).
        let titles = View::ALL.map(View::title);
        let mut chosen = View::ALL.iter().position(|v| *v == self.view).unwrap_or(0);
        prim::property_row(ui, "View", |ui| Select::new(&mut chosen, &titles, "ecad.debug.view").width(size::LABEL_COLUMN * 2.0).show(ui));
        if let Some(v) = View::ALL.get(chosen) {
            self.view = *v;
        }
        rule(ui);
        let trail_open = !self.focus.is_empty() || self.tx.is_some() || !self.focus_from.is_empty();
        let list_height = if trail_open { ui.available_height() * 0.45 } else { ui.available_height() };
        egui::ScrollArea::vertical().id_salt("dbg_view").max_height(list_height).auto_shrink([false, true]).show(ui, |ui| match self.view {
            View::Selection => self.selection_view(ui),
            View::Model => self.model_view(ui, false),
            View::Definitions => self.model_view(ui, true),
            View::History => self.history_view(ui),
            View::Persistence => self.persistence_view(ui),
            View::Storage => {
                if self.storage_view(ui) {
                    action = Some(Action::ReadStorage);
                }
            }
            View::Branches => self.branches_view(ui),
            View::Diagnostics => self.diagnostics_view(ui),
            View::Checks => {
                if let Some(k) = self.checks_view(ui) {
                    action = Some(Action::Check(k));
                }
            }
        });
        if trail_open {
            rule(ui);
            egui::ScrollArea::vertical().id_salt("dbg_trail").auto_shrink([false, false]).show(ui, |ui| {
                self.trail(ui);
            });
        }
        action
    }

    fn selection_view(&mut self, ui: &mut egui::Ui) {
        let Some(snap) = &self.snap else { return };
        if snap.active_page.is_none() {
            prim::body(ui, "The active drawing is not a page of the ECAD project.", true);
            return;
        }
        if snap.selection.is_empty() {
            prim::body(ui, "Nothing is selected. Click objects in the drawing; they are listed here with their ECAD IDs.", true);
            return;
        }
        let mut follow = None;
        for (i, sel) in snap.selection.iter().enumerate() {
            let text = match sel.mapped {
                Some(m) => {
                    let r = resolve(&snap.doc, m);
                    format!("{} {} → {}", sel.entity, sel.handle.hex(), describe(&snap.doc, &r))
                }
                None => format!("{} {} → not an ECAD object", sel.entity, sel.handle.hex()),
            };
            if ListRow::new(&text).show(ui).on_hover_text("Follow it into the model and the history").clicked() {
                follow = Some(i);
            }
        }
        if snap.selection_total > snap.selection.len() {
            prim::body(ui, &format!("… and {} more selected.", snap.selection_total - snap.selection.len()), true);
        }
        if let Some(i) = follow {
            self.follow_selection(i);
        }
    }

    fn model_view(&mut self, ui: &mut egui::Ui, definitions: bool) {
        let Some(snap) = &self.snap else { return };
        let counts = objects::counts(&snap.doc);
        prim::body(ui, &counts.iter().map(|(k, n)| format!("{n} {k}")).collect::<Vec<_>>().join(", "), true);
        if prim::SearchField::new(&mut self.query, "ecad.debug.search").show(ui).changed() {
            self.list_page = 0;
        }
        let q = self.query.trim().to_lowercase();
        let rows: Vec<(ObjectId, String)> = objects::objects(&snap.doc)
            .into_iter()
            .filter(|id| matches!(id, ObjectId::Definition(_)) == definitions)
            .map(|id| (id, objects::object_label(&snap.doc, id)))
            .filter(|(id, label)| {
                q.is_empty() || label.to_lowercase().contains(&q) || id.short().contains(&q) || id.hex().contains(&q) || id.kind().contains(&q)
            })
            .collect();
        let mut follow = None;
        pager(ui, "objects", rows.len(), &mut self.list_page);
        for (id, label) in rows.iter().skip(self.list_page * PAGE).take(PAGE) {
            let chosen = self.focus.first() == Some(id);
            if ListRow::new(&format!("{} {}  {label}", id.kind(), id.short())).selected(chosen).show(ui).clicked() {
                follow = Some(*id);
            }
        }
        if rows.is_empty() {
            prim::body(ui, if definitions { "No embedded definitions." } else { "Nothing matches." }, true);
        }
        if let Some(id) = follow {
            self.follow_object(id);
        }
    }

    fn history_view(&mut self, ui: &mut egui::Ui) {
        let Some(snap) = &self.snap else { return };
        let head = snap.head;
        let where_ = match (&snap.file.path, snap.file.in_memory && snap.file.storage.is_some()) {
            (Some(p), _) => p.clone(),
            (None, true) => "the test store, in memory".into(),
            (None, false) => "no file yet".into(),
        };
        prim::note(ui, &format!("Saved durably ({where_}): transactions 1–{}.", snap.persisted), Tone::Ok);
        let c = snap.state_counts();
        if c.unconfirmed > 0 {
            prim::note(
                ui,
                &format!("Written but not confirmed durable (Save confirms them): transactions {}–{}.", c.saved + 1, c.saved + c.unconfirmed),
                Tone::Warning,
            );
        }
        if c.pending > 0 {
            prim::note(ui, &format!("Pending (in memory only; lost unless saved): transactions {}–{head}.", head - c.pending + 1), Tone::Warning);
        } else {
            prim::body(ui, "Nothing pending.", true);
        }
        ui.horizontal(|ui| {
            prim::body(ui, "State at transaction", true);
            TextEntry::new(&mut self.at_text, "ecad.debug.at").width(size::LABEL_COLUMN * 0.75).show(ui);
            if ui.add(Button::new("Rebuild").small()).on_hover_text("Rebuilt from a copy of the history; the live project is unchanged").clicked() {
                let seq = self.at_text.trim().parse::<u64>().unwrap_or(head);
                self.show_at(seq);
            }
            if self.historical.is_some() && ui.add(Button::new("Hide").kind(Kind::Quiet).small()).clicked() {
                self.historical = None;
            }
        });
        self.historical_view(ui);
        rule(ui);
        let snap = match &self.snap {
            Some(s) => s,
            None => return,
        };
        let mut pick = None;
        pager(ui, "transactions", self.log.len(), &mut self.history_page);
        for tx in self.log.iter().rev().skip(self.history_page * PAGE).take(PAGE) {
            let state = snap.record_state(tx.seq);
            let text = format!("#{} {} ({}, {} ops)", tx.seq, tx.label, ecad_file::history::tx_kind(tx, &self.log), tx.ops.len());
            if state_row(ui, &text, state, self.tx == Some(tx.seq)).clicked() {
                pick = Some(tx.seq);
            }
        }
        if let Some(seq) = pick {
            self.select_tx(Some(seq));
        }
    }

    fn historical_view(&mut self, ui: &mut egui::Ui) {
        let Some(h) = &mut self.historical else { return };
        card().show(ui, |ui| match &h.doc {
            Err(e) => {
                prim::note(ui, &format!("History at seq {}: {e}", h.seq), Tone::Warning);
            }
            Ok(doc) => {
                prim::caption(ui, &format!("History at seq {} (rebuilt from a copy of the log; the live project is unchanged)", h.seq));
                let counts = objects::counts(doc);
                prim::body(ui, &counts.iter().map(|(k, n)| format!("{n} {k}")).collect::<Vec<_>>().join(", "), true);
                let mut pick = None;
                for id in objects::objects(doc).into_iter().filter(|i| !matches!(i, ObjectId::Item(_))).take(PAGE) {
                    let text = format!("{} {}  {}", id.kind(), id.short(), objects::object_label(doc, id));
                    if ListRow::new(&text).selected(h.focus == Some(id)).show(ui).clicked() {
                        pick = Some(id);
                    }
                }
                if let Some(id) = pick {
                    h.focus = Some(id);
                }
                if let Some(v) = h.focus.and_then(|id| objects::object_json(doc, id)) {
                    mono(ui, json_text(&v));
                }
            }
        });
    }

    fn persistence_view(&mut self, ui: &mut egui::Ui) {
        let Some(snap) = &self.snap else { return };
        let f = &snap.file;
        // A grid rather than property rows: these labels are longer than the label column.
        egui::Grid::new("dbg_persistence").num_columns(2).striped(true).show(ui, |ui| {
            let mut row = |k: &str, v: String| {
                prim::body(ui, k, true);
                // Long paths wrap instead of widening the panel.
                prim::body(ui, &v, false);
                ui.end_row();
            };
            row("Persistence", f.persistence.map_or_else(String::new, |p| format!("{p:?}")));
            let file = match (&f.path, f.in_memory) {
                (Some(p), _) => p.clone(),
                (None, true) if f.storage.is_some() => "in memory (the test store)".into(),
                (None, _) => "none yet (Save As writes one)".into(),
            };
            row("File", file);
            row("Writer lock", f.lock_file.clone().unwrap_or_else(|| "not held".into()));
            row(
                "Saved through (file / project)",
                format!("{} / {}", f.writer_persisted.map_or_else(|| "-".into(), |v| v.to_string()), snap.persisted),
            );
            row("Opening found", f.open_report.clone().unwrap_or_else(|| "(created in this session, not opened from a file)".into()));
        });
        if let Some(r) = &f.read_only {
            prim::note(ui, &format!("Read-only: {r}"), Tone::Warning);
        }
        if f.unconfirmed {
            prim::note(ui, "The last Save As is not confirmed durable; Save confirms it.", Tone::Warning);
        }
    }

    /// The project file's facts (live, from the snapshot) and, on request, its table sizes and
    /// version rows. Returns whether the read was asked for.
    fn storage_view(&mut self, ui: &mut egui::Ui) -> bool {
        let Some(snap) = &self.snap else { return false };
        let Some(st) = &snap.file.storage else {
            prim::body(ui, "The project has no file yet (Save As writes one).", true);
            return false;
        };
        // A grid rather than property rows: these labels are longer than the label column.
        egui::Grid::new("dbg_storage").num_columns(2).striped(true).show(ui, |ui| {
            let mut row = |k: &str, v: String| {
                prim::body(ui, k, true);
                prim::body(ui, &v, false);
                ui.end_row();
            };
            row("Container", "version 2 (redb), codec json/1, Zstandard".into());
            row("Branch", st.branch.clone());
            row("Head in the file", st.head.to_string());
            row("Opened from (base)", format!("the snapshot at {}", snap.base));
            row("Latest snapshot", format!("{} (every {} transactions)", st.snapshot_seq, st.snapshot_every));
            row(
                "Last save",
                st.last_save.as_ref().map_or_else(
                    || "none in this session".into(),
                    |l| {
                        format!(
                            "transactions {}–{}, {} bytes, {} new chunks, {} snapshots",
                            l.first_seq, l.last_seq, l.bytes, l.chunks_written, l.snapshots
                        )
                    },
                ),
            );
            row("Writable", if st.writable { "yes".into() } else { "no (read-only)".into() });
        });
        let ask = ui
            .add(Button::new("Read table sizes").small())
            .on_hover_text("Reads the file's tables and version rows (on request; the project is not locked)")
            .clicked();
        if let Some((read, when)) = &self.storage {
            prim::body(ui, &format!("Read {when}:"), true);
            match read {
                Ok(r) => {
                    prim::body(ui, &format!("File: {} bytes", r.file_bytes), false);
                    egui::Grid::new("dbg_tables").num_columns(5).striped(true).show(ui, |ui| {
                        for h in ["table", "rows", "stored bytes", "metadata", "fragmented"] {
                            prim::caption(ui, h);
                        }
                        ui.end_row();
                        for t in &r.tables {
                            prim::body(ui, t.name, false);
                            prim::body(ui, &t.rows.to_string(), false);
                            prim::body(ui, &t.stored_bytes.to_string(), false);
                            prim::body(ui, &t.metadata_bytes.to_string(), false);
                            prim::body(ui, &t.fragmented_bytes.to_string(), false);
                            ui.end_row();
                        }
                    });
                    prim::body(ui, &format!("{} version rows:", r.versions.len()), false);
                    for (b, kind, name, seq, chunks) in r.versions.iter().rev().take(PAGE) {
                        prim::body(ui, &format!("{b}: {kind} {name:?} at {seq} ({chunks} chunks)"), true);
                    }
                }
                Err(e) => {
                    prim::note(ui, e, Tone::Warning);
                }
            }
        }
        ask
    }

    /// The version 2 file's branches: name, status, head, rows, fork point.
    fn branches_view(&mut self, ui: &mut egui::Ui) {
        match &self.branches {
            None => {
                prim::body(ui, "The project has no version 2 file yet (Save As writes one).", true);
            }
            Some(Err(e)) => {
                prim::note(ui, e, Tone::Warning);
            }
            Some(Ok(rows)) => {
                prim::body(ui, "Each branch is its own append-only log; a branch starts after a transaction of its parent.", true);
                for b in rows {
                    let from = match &b.fork {
                        Some((p, seq)) => format!("forked from {p} after transaction {seq}"),
                        None => "from the project root".into(),
                    };
                    card().show(ui, |ui| {
                        prim::body(ui, &format!("{} ({})", b.name, b.status), false);
                        prim::body(
                            ui,
                            &format!("head {}, {} rows, {from}{}", b.head, b.rows, if b.release_line { ", release line" } else { "" }),
                            true,
                        );
                        mono(ui, format!("ID {}  head chain {}", b.id, b.head_chain));
                    });
                    ui.add_space(space::XS);
                }
            }
        }
    }

    fn diagnostics_view(&mut self, ui: &mut egui::Ui) {
        let Some(snap) = &self.snap else { return };
        prim::caption(ui, "Pages");
        for p in &snap.pages {
            let line = format!(
                "{} {}: {}{}",
                p.title,
                p.id.short(),
                match (p.uid, p.mapped, p.map_consistent) {
                    (Some(_), Some(n), Some(true)) => format!("open, {n} mapped handles"),
                    (Some(_), Some(n), _) => format!("open, {n} mapped handles, handle map NOT one-to-one"),
                    _ => "closed".to_string(),
                },
                if p.unavailable { ", UNAVAILABLE (reopen it)" } else { "" }
            );
            prim::body(ui, &line, false);
        }
        rule(ui);
        // Derived once per snapshot from its copy of the document, never saved; paged.
        prim::caption(ui, "Cable references");
        pager(ui, "references", self.ref_lines.len(), &mut self.references_page);
        for line in self.ref_lines.iter().skip(self.references_page * PAGE).take(PAGE) {
            prim::body(ui, line, false);
        }
        rule(ui);
        prim::caption(ui, "Runtime events (diagnostics; never saved in the project)");
        pager(ui, "events", snap.events.len(), &mut self.events_page);
        for e in snap.events.iter().rev().skip(self.events_page * PAGE).take(PAGE) {
            let line = format!(
                "#{} {:?}{} at seq {}: {}",
                e.n,
                e.kind,
                e.command.as_deref().map_or_else(String::new, |c| format!(" [{c}]")),
                e.seq,
                e.message
            );
            prim::body(ui, &line, false);
        }
    }

    fn checks_view(&mut self, ui: &mut egui::Ui) -> Option<CheckKind> {
        let mut run = None;
        prim::body(ui, "Checks run only when asked, on copies, and never change the project.", true);
        ui.horizontal_wrapped(|ui| {
            if ui.add(Button::new("Verify replay").small()).on_hover_text("Replay the history and compare with the live document").clicked() {
                run = Some(CheckKind::Replay);
            }
            if ui.add(Button::new("Verify projection").small()).on_hover_text("Reproject the open pages and compare with their drawings").clicked() {
                run = Some(CheckKind::Projection);
            }
            if ui.add(Button::new("Verify file").small()).on_hover_text("Re-read the file: rows, hash chain, history").clicked() {
                run = Some(CheckKind::File);
            }
            if ui
                .add(Button::new("Check index").small())
                .on_hover_text("Rebuild the reference index from a copy and compare it with the kept one")
                .accessible_name("ecad.debug.referencecheck")
                .clicked()
            {
                run = Some(CheckKind::Index);
            }
        });
        let running = self.jobs.running();
        if running > 0 {
            prim::note(ui, &format!("Running {running} check{}…", ecad_widgets::plural(running)), Tone::Neutral);
            // The result arrives on the background pool's channel: look again shortly.
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        let dropped = self.jobs.dropped();
        if dropped > 0 {
            prim::body(ui, &format!("{dropped} result{} dropped: the project changed while the check ran.", ecad_widgets::plural(dropped)), true);
        }
        for r in self.checks.iter().rev() {
            let head = format!("{} {} at seq {}, {}", if r.check.ok { "PASS" } else { "FAIL" }, r.check.what, r.check.at_seq, r.time);
            prim::note(ui, &head, if r.check.ok { Tone::Ok } else { Tone::Warning });
            for l in &r.check.lines {
                prim::body(ui, &format!("  {l}"), true);
            }
        }
        run
    }

    fn trail(&mut self, ui: &mut egui::Ui) {
        let Some(snap) = &self.snap else { return };
        prim::caption(ui, &format!("Trail from {}", self.focus_from));
        // ECAD IDs → model objects (live values).
        for id in &self.focus {
            let label = objects::object_label(&snap.doc, *id);
            fold(ui, ui.scope_id().with(("dbg_obj", id.hex(), id.kind())), &format!("{} {} {label}", id.kind(), id.short()), |ui| {
                mono(ui, format!("ID {}", id.hex()));
                let users = objects::used_by(&snap.doc, *id);
                if !users.is_empty() {
                    let shown: Vec<String> = users.iter().take(PAGE).map(|u| format!("{} {}", u.kind(), u.short())).collect();
                    prim::body(ui, &format!("Used by: {}{}", shown.join(", "), if users.len() > PAGE { ", …" } else { "" }), false);
                }
                match objects::object_json(&snap.doc, *id) {
                    Some(v) => mono(ui, json_text(&v)),
                    None => prim::body(ui, "Not in the live model (deleted or never created).", true),
                };
            });
        }
        // → the transactions that touched them.
        let touching = self.touching();
        if !self.focus.is_empty() {
            prim::body(ui, &format!("Transactions that touched them: {}", touching.len()), false);
        }
        let mut pick = None;
        for seq in touching.iter().rev().take(PAGE) {
            let Some(tx) = snap.tx_in(&self.log, *seq) else { continue };
            let state = snap.record_state(*seq);
            if state_row(ui, &format!("#{} {}", seq, tx.label), state, self.tx == Some(*seq)).clicked() {
                pick = Some(*seq);
            }
        }
        if let Some(seq) = pick {
            self.tx = Some(seq);
            self.record = None;
        }
        // → the transaction, its ops with before/after values, and its record.
        let Some(seq) = self.tx else { return };
        let Some(tx) = snap.tx_in(&self.log, seq) else { return };
        let state = snap.record_state(seq);
        rule(ui);
        prim::note(ui, &format!("Transaction #{seq}: {}", tx.label), state_tone(state));
        prim::body(
            ui,
            &format!(
                "{}, by {}, {}, ID {}",
                ecad_file::history::tx_kind(tx, &self.log),
                author(&tx.author),
                tx.time_ms.map_or_else(|| "time unknown".into(), |ms| ecad_file::utc(ms / 1000)),
                tx.id.short()
            ),
            true,
        );
        let command = snap.events.iter().find(|e| e.seq == seq && e.command.is_some());
        if let Some(e) = command {
            prim::body(ui, &format!("Recorded by command {} (event #{} {:?})", e.command.as_deref().unwrap_or(""), e.n, e.kind), true);
        }
        let mut read_row = false;
        match state {
            RecordState::Saved | RecordState::Unconfirmed => {
                let what = if state.durable() { "Saved" } else { "Written but NOT confirmed durable" };
                ui.horizontal_wrapped(|ui| {
                    prim::body(ui, &format!("{what}: a row of the file's transactions table."), false);
                    read_row = ui.add(Button::new("Read row").small()).clicked();
                });
            }
            RecordState::Pending => {
                prim::note(ui, "Pending: only in memory, not in the file. Save writes it.", Tone::Warning);
            }
            RecordState::NotSaved => {
                prim::body(ui, "This project has no persistence; nothing is saved.", false);
            }
        }
        match &self.record {
            Some(Ok(r)) => {
                match &r.check {
                    Ok(()) => prim::note(ui, &format!("The row ({} bytes stored) holds exactly this transaction.", r.stored), Tone::Ok),
                    Err(e) => prim::note(ui, &format!("The bytes do NOT match: {e}"), Tone::Warning),
                };
                mono(ui, objects::printable(&r.shown));
                if r.truncated {
                    prim::body(ui, &format!("… cut at {} bytes", r.shown.len()), true);
                }
            }
            Some(Err(e)) => {
                prim::note(ui, e, Tone::Warning);
            }
            None => {}
        }
        prim::body(ui, &format!("{} ops, with the values they replaced (before) and set (after):", tx.ops.len()), false);
        for (i, op) in tx.ops.iter().take(MAX_OPS).enumerate() {
            let v = op_view(op);
            let targets = v.targets.iter().map(|t| t.short()).collect::<Vec<_>>().join(", ");
            fold(ui, ui.scope_id().with(("dbg_op", seq, i)), &format!("{} {}  {targets}", i + 1, v.name), |ui| {
                prim::caption(ui, "before:");
                mono(ui, v.before.as_ref().map_or_else(|| "(none)".into(), json_text));
                prim::caption(ui, "after:");
                mono(ui, v.after.as_ref().map_or_else(|| "(none)".into(), json_text));
            });
        }
        if tx.ops.len() > MAX_OPS {
            prim::body(ui, &format!("… {} more ops", tx.ops.len() - MAX_OPS), true);
        }
        if read_row {
            self.read_row();
        }
    }
}

/// The heading's line on saved, unconfirmed and pending transactions.
pub fn saved_summary(snap: &DebugSnapshot) -> String {
    let c = snap.state_counts();
    let mut s = format!("Saved through transaction {}", c.saved);
    if c.unconfirmed > 0 {
        s.push_str(&format!("; {} written but not confirmed durable (Save confirms them)", c.unconfirmed));
    }
    s.push_str(&format!("; {} pending (in memory only).", c.pending));
    let pending = c.unconfirmed + c.pending;
    if pending != snap.unsaved {
        s.push_str(&format!(
            " The Project panel counts {}: a new project's own setup is not counted as a change until the project has a file.",
            snap.unsaved
        ));
    }
    s
}

/// Durable records in the ok tone, everything else (pending, unconfirmed) as a warning.
fn state_tone(state: RecordState) -> Tone {
    if state.durable() { Tone::Ok } else { Tone::Warning }
}

/// A transaction's list row: its text, and its record state at the right in the state's tone.
fn state_row(ui: &mut egui::Ui, text: &str, state: RecordState, chosen: bool) -> egui::Response {
    ListRow::new(text).selected(chosen).show_with(ui, |ui| {
        prim::note(ui, &format!("[{}]", state.label()), state_tone(state));
    })
}

/// A data line (JSON, record bytes, IDs) in the code font.
fn mono(ui: &mut egui::Ui, text: String) -> egui::Response {
    ui.label(RichText::new(text).monospace().color(color(SHELL.text)))
}

/// A hairline between parts of the panel, across its width.
fn rule(ui: &mut egui::Ui) {
    let full = ui.max_rect().x_range();
    ecad_widgets::panel::rule(ui, full);
}

/// The inset card a rebuilt state or a branch is shown in.
fn card() -> egui::Frame {
    egui::Frame::new().fill(color(SHELL.raised)).corner_radius(ecad_widgets::style::corner(radius::INSET_CARD)).inner_margin(space::M)
}

/// A folding section (closed at first), its open state kept under `id`; `add` draws its body.
fn fold(ui: &mut egui::Ui, id: egui::Id, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    let mut open = ui.data_mut(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    SectionHeader::new(&mut open, title).show(ui);
    ui.data_mut(|d| d.insert_temp(id, open));
    if open {
        ui.indent(id, add);
    }
}

/// A resolved selection in words.
fn describe(doc: &Document, r: &Resolved) -> String {
    let mut chunks = Vec::new();
    if let Some(i) = r.item {
        chunks.push(format!("item {}", i.short()));
    }
    if let Some(t) = r.connection_point {
        chunks.push(format!("connection point {} {}", t.short(), objects::object_label(doc, ObjectId::ConnectionPoint(t))));
    }
    if let Some(rep) = r.representation {
        chunks.push(format!("{} {}", rep.short(), objects::object_label(doc, ObjectId::Representation(rep))));
    }
    if let Some(s) = r.subject {
        let id = ObjectId::from(s);
        chunks.push(format!("{} {} {}", id.kind(), id.short(), objects::object_label(doc, id)));
    }
    if chunks.is_empty() { "not found in the model".into() } else { chunks.join(" → ") }
}

/// Previous/next buttons for a list of `n` rows.
fn pager(ui: &mut egui::Ui, what: &str, n: usize, page: &mut usize) {
    let pages = n.div_ceil(PAGE).max(1);
    *page = (*page).min(pages - 1);
    if pages <= 1 {
        return;
    }
    ui.horizontal(|ui| {
        if ui.add_enabled(*page > 0, IconButton::new(Icon::ChevronLeft, "Previous page")).clicked() {
            *page -= 1;
        }
        prim::body(ui, &format!("{what} page {} of {pages} ({n})", *page + 1), true);
        if ui.add_enabled(*page + 1 < pages, IconButton::new(Icon::ChevronRight, "Next page")).clicked() {
            *page += 1;
        }
    });
}
