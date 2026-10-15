//! The developer debugger panel (persistence stage 2b, proposal §21): read-only, opened from the
//! banner, never shown by default.
//!
//! It follows one trail: **selected drawing objects → ECAD IDs → model objects → the transactions
//! that touched them, with before/after values → the saved record's offset and bytes**, and keeps
//! pending changes (in memory only) visibly apart from durably saved history.
//!
//! Everything shown comes from a [`DebugSnapshot`] copied under one short lock
//! (`ProjectHandle::debug_snapshot`), released before anything is drawn. The snapshot is rebuilt
//! only when the project, its saved position, its runtime events or the selection change, or on
//! Refresh; a busy project is never waited for (the previous snapshot stays, marked stale).
//! Replay, projection and file checks run only when their button is pressed, on copies. History
//! at an earlier transaction is rebuilt from the panel's own copy of the log.

use std::collections::BTreeSet;
use std::path::Path;

use cadcraft_engine::Session;
use ecad_cadcraft::ProjectHandle;
use ecad_cadcraft::debug::{
    self, Check, DebugSnapshot, ObjectId, RecordBytes, RecordState, Resolved, TxCursor, op_view, resolve,
};
use ecad_model::{Author, Document, Transaction};
use eframe::egui::{self, Color32, RichText};

/// The banner button's hover text.
pub const HOVER: &str = "Read-only developer debugger: selection → ECAD IDs → model → transactions → saved records";

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
    Diagnostics,
    Checks,
}

impl View {
    pub const ALL: [View; 7] = [
        View::Selection,
        View::Model,
        View::Definitions,
        View::History,
        View::Persistence,
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
}

/// What the panel asks the trial to do after drawing (things that need the project).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Refresh,
    Close,
    Check(CheckKind),
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
    at_text: String,
    historical: Option<Historical>,
    checks: Vec<Ran>,
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
            at_text: String::new(),
            historical: None,
            checks: Vec::new(),
        }
    }
}

fn now() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    ecad_file::disk::utc(secs)
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
    }

    /// Install a new snapshot.
    pub fn take(&mut self, mut snap: DebugSnapshot) {
        if snap.log_from == 0 || self.snap.as_ref().is_some_and(|old| old.project != snap.project) {
            self.log.clear();
        }
        self.log.truncate(usize::try_from(snap.log_from).unwrap_or(usize::MAX));
        self.log.append(&mut snap.new_transactions);
        if self.tx.is_some_and(|t| t > snap.head) {
            self.tx = None;
            self.record = None;
        }
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
        self.log.iter().filter(|t| debug::touches(t, &ids)).map(|t| t.seq).collect()
    }

    /// Read the trail transaction's record from the file (no project lock).
    pub fn read_record(&mut self) {
        let (Some(snap), Some(seq)) = (&self.snap, self.tx) else { return };
        let span = match snap.record_state(seq) {
            RecordState::Saved(Some(s)) | RecordState::Unconfirmed(s) => s,
            other => {
                self.record = Some(Err(format!("transaction {seq} has no record in the file ({})", other.label())));
                return;
            }
        };
        let Some(path) = &snap.file.path else {
            self.record = Some(Err("the test store keeps its file in memory".into()));
            return;
        };
        let expected = self.log.get(usize::try_from(seq).unwrap_or(usize::MAX).wrapping_sub(1));
        self.record = Some(debug::read_record(Path::new(path), span, expected));
    }

    /// Rebuild the document after transaction `seq` from the panel's copy of the history.
    pub fn show_at(&mut self, seq: u64) {
        let doc = debug::state_at(&self.log, seq);
        self.historical = Some(Historical { seq, doc, focus: None });
    }

    /// Run a check now (only ever on request).
    pub fn run_check(&mut self, kind: CheckKind, project: &ProjectHandle, s: &mut Session) {
        let check = match (kind, &self.snap) {
            (_, None) => return,
            (CheckKind::Replay, Some(snap)) => debug::verify_replay(&self.log, &snap.doc),
            (CheckKind::Projection, Some(snap)) => match project.debug_projection_inputs(s) {
                Ok(inputs) => debug::verify_projection(&inputs),
                Err(e) => Check { what: "projection", at_seq: snap.head, ok: false, lines: vec![e] },
            },
            (CheckKind::File, Some(snap)) => match &snap.file.path {
                Some(p) => debug::verify_file(Path::new(p), &self.log, &snap.file, snap.persisted),
                None => Check {
                    what: "file",
                    at_seq: snap.head,
                    ok: false,
                    lines: vec!["the project has no file yet".into()],
                },
            },
        };
        self.checks.push(Ran { check, time: now() });
    }

    /// Apply a panel action that needs the project.
    pub fn apply(&mut self, a: Action, project: &ProjectHandle, s: &mut Session) {
        match a {
            Action::Refresh => self.refresh(project, s, true),
            Action::Close => self.open = false,
            Action::Check(k) => self.run_check(k, project, s),
        }
    }

    /// Draw the panel. Returns what needs the project.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new("ECAD debugger (read-only)").strong());
            if ui.button("Refresh").on_hover_text("Take a new snapshot now").clicked() {
                action = Some(Action::Refresh);
            }
            if ui.button("Close").clicked() {
                action = Some(Action::Close);
            }
        });
        let warn = ui.visuals().warn_fg_color;
        let Some(snap) = &self.snap else {
            ui.label(match &self.stale {
                Some(e) => format!("No snapshot yet: {e}"),
                None => "No snapshot yet.".to_string(),
            });
            return action;
        };
        ui.label(format!("Snapshot: {} of \"{}\", taken {} (#{}).", snap.label(), snap.name, self.taken, self.builds));
        ui.label(saved_summary(snap));
        if let Some(e) = &self.stale {
            ui.label(RichText::new(format!("Stale: {e}. Showing the snapshot above.")).color(warn));
        }
        ui.horizontal_wrapped(|ui| {
            for v in View::ALL {
                if ui.selectable_label(self.view == v, v.title()).clicked() {
                    self.view = v;
                }
            }
        });
        ui.separator();
        let trail_open = !self.focus.is_empty() || self.tx.is_some() || !self.focus_from.is_empty();
        let list_height = if trail_open { ui.available_height() * 0.45 } else { ui.available_height() };
        egui::ScrollArea::vertical().id_salt("dbg_view").max_height(list_height).auto_shrink([false, true]).show(
            ui,
            |ui| match self.view {
                View::Selection => self.selection_view(ui),
                View::Model => self.model_view(ui, false),
                View::Definitions => self.model_view(ui, true),
                View::History => self.history_view(ui),
                View::Persistence => self.persistence_view(ui),
                View::Diagnostics => self.diagnostics_view(ui),
                View::Checks => {
                    if let Some(k) = self.checks_view(ui) {
                        action = Some(Action::Check(k));
                    }
                }
            },
        );
        if trail_open {
            ui.separator();
            egui::ScrollArea::vertical().id_salt("dbg_trail").auto_shrink([false, false]).show(ui, |ui| {
                self.trail(ui);
            });
        }
        action
    }

    fn selection_view(&mut self, ui: &mut egui::Ui) {
        let Some(snap) = &self.snap else { return };
        if snap.active_page.is_none() {
            ui.label("The active drawing is not a page of the ECAD project.");
            return;
        }
        if snap.selection.is_empty() {
            ui.label("Nothing is selected. Click objects in the drawing; they are listed here with their ECAD IDs.");
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
            if ui.selectable_label(false, text).on_hover_text("Follow it into the model and the history").clicked() {
                follow = Some(i);
            }
        }
        if snap.selection_total > snap.selection.len() {
            ui.label(format!("… and {} more selected.", snap.selection_total - snap.selection.len()));
        }
        if let Some(i) = follow {
            self.follow_selection(i);
        }
    }

    fn model_view(&mut self, ui: &mut egui::Ui, definitions: bool) {
        let Some(snap) = &self.snap else { return };
        let counts = debug::counts(&snap.doc);
        ui.label(counts.iter().map(|(k, n)| format!("{n} {k}")).collect::<Vec<_>>().join(", "));
        ui.horizontal(|ui| {
            ui.label("Search:");
            if ui.text_edit_singleline(&mut self.query).changed() {
                self.list_page = 0;
            }
        });
        let q = self.query.trim().to_lowercase();
        let rows: Vec<(ObjectId, String)> = debug::objects(&snap.doc)
            .into_iter()
            .filter(|id| matches!(id, ObjectId::Definition(_)) == definitions)
            .map(|id| (id, debug::object_label(&snap.doc, id)))
            .filter(|(id, label)| {
                q.is_empty()
                    || label.to_lowercase().contains(&q)
                    || id.short().contains(&q)
                    || id.hex().contains(&q)
                    || id.kind().contains(&q)
            })
            .collect();
        let mut follow = None;
        pager(ui, "objects", rows.len(), &mut self.list_page);
        for (id, label) in rows.iter().skip(self.list_page * PAGE).take(PAGE) {
            let chosen = self.focus.first() == Some(id);
            if ui.selectable_label(chosen, format!("{} {}  {label}", id.kind(), id.short())).clicked() {
                follow = Some(*id);
            }
        }
        if rows.is_empty() {
            ui.label(if definitions { "No embedded definitions." } else { "Nothing matches." });
        }
        if let Some(id) = follow {
            self.follow_object(id);
        }
    }

    fn history_view(&mut self, ui: &mut egui::Ui) {
        let Some(snap) = &self.snap else { return };
        let warn = ui.visuals().warn_fg_color;
        let head = snap.head;
        let where_ = snap.file.path.clone().unwrap_or_else(|| "no file yet".into());
        ui.label(
            RichText::new(format!("Saved durably ({where_}): transactions 1–{}.", snap.persisted)).color(ok_color()),
        );
        let c = snap.state_counts();
        if c.unconfirmed > 0 {
            ui.label(
                RichText::new(format!(
                    "Written but not confirmed durable (Save confirms them): transactions {}–{}.",
                    c.saved + 1,
                    c.saved + c.unconfirmed
                ))
                .color(warn),
            );
        }
        if c.pending > 0 {
            ui.label(
                RichText::new(format!(
                    "Pending (in memory only; lost unless saved): transactions {}–{head}.",
                    head - c.pending + 1
                ))
                .color(warn),
            );
        } else {
            ui.label("Nothing pending.");
        }
        ui.horizontal(|ui| {
            ui.label("State at transaction");
            ui.add(egui::TextEdit::singleline(&mut self.at_text).desired_width(60.0));
            if ui
                .button("Rebuild")
                .on_hover_text("Rebuilt from a copy of the history; the live project is unchanged")
                .clicked()
            {
                let seq = self.at_text.trim().parse::<u64>().unwrap_or(head);
                self.show_at(seq);
            }
            if self.historical.is_some() && ui.button("Hide").clicked() {
                self.historical = None;
            }
        });
        self.historical_view(ui);
        ui.separator();
        let snap = match &self.snap {
            Some(s) => s,
            None => return,
        };
        let mut pick = None;
        pager(ui, "transactions", self.log.len(), &mut self.history_page);
        for tx in self.log.iter().rev().skip(self.history_page * PAGE).take(PAGE) {
            let state = snap.record_state(tx.seq);
            let text = RichText::new(format!(
                "#{} [{}] {} ({}, {} ops)",
                tx.seq,
                state.label(),
                tx.label,
                debug::tx_kind(tx, &self.log),
                tx.ops.len()
            ));
            let chosen = self.tx == Some(tx.seq);
            let text = if chosen { text } else { text.color(state_color(state, warn)) };
            if ui.selectable_label(chosen, text).clicked() {
                pick = Some(tx.seq);
            }
        }
        if let Some(seq) = pick {
            self.select_tx(Some(seq));
        }
    }

    fn historical_view(&mut self, ui: &mut egui::Ui) {
        let Some(h) = &mut self.historical else { return };
        let frame = egui::Frame::group(ui.style());
        frame.show(ui, |ui| match &h.doc {
            Err(e) => {
                ui.label(format!("History at seq {}: {e}", h.seq));
            }
            Ok(doc) => {
                ui.label(
                    RichText::new(format!(
                        "History at seq {} (rebuilt from a copy of the log; the live project is unchanged)",
                        h.seq
                    ))
                    .strong(),
                );
                let counts = debug::counts(doc);
                ui.label(counts.iter().map(|(k, n)| format!("{n} {k}")).collect::<Vec<_>>().join(", "));
                let mut pick = None;
                for id in debug::objects(doc).into_iter().filter(|i| !matches!(i, ObjectId::Item(_))).take(PAGE) {
                    if ui
                        .selectable_label(
                            h.focus == Some(id),
                            format!("{} {}  {}", id.kind(), id.short(), debug::object_label(doc, id)),
                        )
                        .clicked()
                    {
                        pick = Some(id);
                    }
                }
                if let Some(id) = pick {
                    h.focus = Some(id);
                }
                if let Some(v) = h.focus.and_then(|id| debug::object_json(doc, id)) {
                    ui.monospace(json_text(&v));
                }
            }
        });
    }

    fn persistence_view(&mut self, ui: &mut egui::Ui) {
        let Some(snap) = &self.snap else { return };
        let f = &snap.file;
        let warn = ui.visuals().warn_fg_color;
        egui::Grid::new("dbg_persistence").num_columns(2).striped(true).show(ui, |ui| {
            let mut row = |k: &str, v: String| {
                ui.label(k);
                // Long paths wrap instead of widening the panel.
                ui.add(egui::Label::new(v).wrap());
                ui.end_row();
            };
            row("Persistence", f.persistence.map_or_else(String::new, |p| format!("{p:?}")));
            row("File", f.path.clone().unwrap_or_else(|| "none yet (Save As writes one)".into()));
            row("Writer lock", f.lock_file.clone().unwrap_or_else(|| "not held".into()));
            if let Some(h) = &f.header {
                row(
                    "Header",
                    format!(
                        "{} v{}, schema {}, codec {}, project {}",
                        h.container,
                        h.container_version,
                        h.schema_version,
                        h.codec,
                        h.project_id.map_or_else(String::new, |p| p.short())
                    ),
                );
                row("Written by", h.writer.clone());
            }
            row("Verified bytes", f.verified_len.map_or_else(String::new, |v| v.to_string()));
            row(
                "Saved through (writer / project)",
                format!("{} / {}", f.writer_persisted.map_or_else(|| "-".into(), |v| v.to_string()), snap.persisted),
            );
            row("Records in the file", f.spans.len().to_string());
            if let Some(last) = f.spans.last() {
                row("Last record", format!("transaction {} at offset {} ({} bytes)", last.seq, last.offset, last.len));
            }
            row(
                "Opening found",
                f.open_report.clone().unwrap_or_else(|| "(created in this session, not opened from a file)".into()),
            );
            row("Preserved tail", f.preserved_tail.clone().unwrap_or_else(|| "none".into()));
        });
        if let Some(r) = &f.read_only {
            ui.label(RichText::new(format!("Read-only: {r}")).color(warn));
        }
        if f.unconfirmed {
            ui.label(RichText::new("The last Save As is not confirmed durable; Save confirms it.").color(warn));
        }
        if let Some(a) = &f.attempt {
            ui.label(
                RichText::new(format!(
                    "A failed save ({:?}) of transactions {}–{} may have left {} bytes at offset {}; the next Save removes them first.",
                    a.stage, a.first_seq, a.last_seq, a.len, a.offset
                ))
                .color(warn),
            );
        }
        if let Some(n) = f.pending_fragment {
            ui.label(
                RichText::new(format!("An incomplete trailing write of {n} bytes is still in the file.")).color(warn),
            );
        }
    }

    fn diagnostics_view(&mut self, ui: &mut egui::Ui) {
        let Some(snap) = &self.snap else { return };
        ui.label(RichText::new("Pages").strong());
        for p in &snap.pages {
            ui.label(format!(
                "{} {}: {}{}",
                p.title,
                p.id.short(),
                match (p.uid, p.mapped, p.map_consistent) {
                    (Some(_), Some(n), Some(true)) => format!("open, {n} mapped handles"),
                    (Some(_), Some(n), _) => format!("open, {n} mapped handles, handle map NOT one-to-one"),
                    _ => "closed".to_string(),
                },
                if p.unavailable { ", UNAVAILABLE (reopen it)" } else { "" }
            ));
        }
        ui.separator();
        ui.label(RichText::new("Runtime events (diagnostics; never saved in the project)").strong());
        pager(ui, "events", snap.events.len(), &mut self.events_page);
        for e in snap.events.iter().rev().skip(self.events_page * PAGE).take(PAGE) {
            ui.label(format!(
                "#{} {:?}{} at seq {}: {}",
                e.n,
                e.kind,
                e.command.as_deref().map_or_else(String::new, |c| format!(" [{c}]")),
                e.seq,
                e.message
            ));
        }
    }

    fn checks_view(&mut self, ui: &mut egui::Ui) -> Option<CheckKind> {
        let mut run = None;
        ui.label("Checks run only when asked, on copies, and never change the project.");
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Verify replay")
                .on_hover_text("Replay the history and compare with the live document")
                .clicked()
            {
                run = Some(CheckKind::Replay);
            }
            if ui
                .button("Verify projection")
                .on_hover_text("Reproject the open pages and compare with their drawings")
                .clicked()
            {
                run = Some(CheckKind::Projection);
            }
            if ui
                .button("Verify file")
                .on_hover_text("Re-read the file: frames, checksums, hash chain, history")
                .clicked()
            {
                run = Some(CheckKind::File);
            }
        });
        let warn = ui.visuals().warn_fg_color;
        for r in self.checks.iter().rev() {
            let head = format!(
                "{} {} at seq {}, {}",
                if r.check.ok { "PASS" } else { "FAIL" },
                r.check.what,
                r.check.at_seq,
                r.time
            );
            ui.label(RichText::new(head).strong().color(if r.check.ok { ok_color() } else { warn }));
            for l in &r.check.lines {
                ui.label(format!("  {l}"));
            }
        }
        run
    }

    fn trail(&mut self, ui: &mut egui::Ui) {
        let Some(snap) = &self.snap else { return };
        let warn = ui.visuals().warn_fg_color;
        ui.label(RichText::new(format!("Trail from {}", self.focus_from)).strong());
        // ECAD IDs → model objects (live values).
        for id in &self.focus {
            let label = debug::object_label(&snap.doc, *id);
            egui::CollapsingHeader::new(format!("{} {} {label}", id.kind(), id.short()))
                .id_salt(("dbg_obj", id.hex(), id.kind()))
                .show(ui, |ui| {
                    ui.monospace(format!("ID {}", id.hex()));
                    let users = debug::used_by(&snap.doc, *id);
                    if !users.is_empty() {
                        let shown: Vec<String> =
                            users.iter().take(PAGE).map(|u| format!("{} {}", u.kind(), u.short())).collect();
                        ui.label(format!(
                            "Used by: {}{}",
                            shown.join(", "),
                            if users.len() > PAGE { ", …" } else { "" }
                        ));
                    }
                    match debug::object_json(&snap.doc, *id) {
                        Some(v) => ui.monospace(json_text(&v)),
                        None => ui.label("Not in the live model (deleted or never created)."),
                    };
                });
        }
        // → the transactions that touched them.
        let touching = self.touching();
        if !self.focus.is_empty() {
            ui.label(format!("Transactions that touched them: {}", touching.len()));
        }
        let mut pick = None;
        for seq in touching.iter().rev().take(PAGE) {
            let Some(tx) = self.log.get(usize::try_from(*seq).unwrap_or(usize::MAX).wrapping_sub(1)) else { continue };
            let state = snap.record_state(*seq);
            let chosen = self.tx == Some(*seq);
            let text = RichText::new(format!("#{} [{}] {}", seq, state.label(), tx.label));
            let text = if chosen { text } else { text.color(state_color(state, warn)) };
            if ui.selectable_label(chosen, text).clicked() {
                pick = Some(*seq);
            }
        }
        if let Some(seq) = pick {
            self.tx = Some(seq);
            self.record = None;
        }
        // → the transaction, its ops with before/after values, and its record.
        let Some(seq) = self.tx else { return };
        let Some(tx) = self.log.get(usize::try_from(seq).unwrap_or(usize::MAX).wrapping_sub(1)) else { return };
        let state = snap.record_state(seq);
        ui.separator();
        ui.label(RichText::new(format!("Transaction #{seq}: {}", tx.label)).strong().color(state_color(state, warn)));
        ui.label(format!(
            "{}, by {}, {}, ID {}",
            debug::tx_kind(tx, &self.log),
            author(&tx.author),
            tx.time_ms.map_or_else(|| "time unknown".into(), |ms| ecad_file::disk::utc(ms / 1000)),
            tx.id.short()
        ));
        let command = snap.events.iter().find(|e| e.seq == seq && e.command.is_some());
        if let Some(e) = command {
            ui.label(format!(
                "Recorded by command {} (event #{} {:?})",
                e.command.as_deref().unwrap_or(""),
                e.n,
                e.kind
            ));
        }
        let mut read = false;
        match state {
            RecordState::Saved(Some(s)) | RecordState::Unconfirmed(s) => {
                let what = if state.durable() { "Saved" } else { "Written but NOT confirmed durable" };
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("{what}: offset {} in the file, {} bytes.", s.offset, s.len));
                    read = ui.button("Read bytes").clicked();
                });
            }
            RecordState::Saved(None) => {
                ui.label("Saved (the writer does not know the record's position).");
            }
            RecordState::Pending { attempt } => {
                ui.label(
                    RichText::new(match attempt {
                        None => "Pending: only in memory, not in the file. Save writes it.".to_string(),
                        Some(st) => format!("Pending: a save of it failed ({st:?}); only in memory. Save retries it."),
                    })
                    .color(warn),
                );
            }
            RecordState::NotSaved => {
                ui.label("This project has no persistence; nothing is saved.");
            }
        }
        match &self.record {
            Some(Ok(r)) => {
                match &r.check {
                    Ok(()) => ui.label(
                        RichText::new("The bytes are a valid record holding exactly this transaction.")
                            .color(ok_color()),
                    ),
                    Err(e) => ui.label(RichText::new(format!("The bytes do NOT match: {e}")).color(warn)),
                };
                ui.monospace(debug::printable(&r.shown));
                if r.truncated {
                    ui.label(format!("… cut at {} bytes", r.shown.len()));
                }
            }
            Some(Err(e)) => {
                ui.label(RichText::new(e.as_str()).color(warn));
            }
            None => {}
        }
        ui.label(format!("{} ops, with the values they replaced (before) and set (after):", tx.ops.len()));
        for (i, op) in tx.ops.iter().take(MAX_OPS).enumerate() {
            let v = op_view(op);
            let targets = v.targets.iter().map(|t| t.short()).collect::<Vec<_>>().join(", ");
            egui::CollapsingHeader::new(format!("{} {}  {targets}", i + 1, v.name)).id_salt(("dbg_op", seq, i)).show(
                ui,
                |ui| {
                    ui.label("before:");
                    ui.monospace(v.before.as_ref().map_or_else(|| "(none)".into(), json_text));
                    ui.label("after:");
                    ui.monospace(v.after.as_ref().map_or_else(|| "(none)".into(), json_text));
                },
            );
        }
        if tx.ops.len() > MAX_OPS {
            ui.label(format!("… {} more ops", tx.ops.len() - MAX_OPS));
        }
        if read {
            self.read_record();
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
            " The banner counts {}: a new project's own setup is not counted as a change until the project has a file.",
            snap.unsaved
        ));
    }
    s
}

fn ok_color() -> Color32 {
    Color32::from_rgb(0x2e, 0x9e, 0x4f)
}

fn state_color(state: RecordState, warn: Color32) -> Color32 {
    if state.durable() { ok_color() } else { warn }
}

/// A resolved selection in words.
fn describe(doc: &Document, r: &Resolved) -> String {
    let mut parts = Vec::new();
    if let Some(i) = r.item {
        parts.push(format!("item {}", i.short()));
    }
    if let Some(t) = r.terminal {
        parts.push(format!("terminal {} {}", t.short(), debug::object_label(doc, ObjectId::Terminal(t))));
    }
    if let Some(rep) = r.representation {
        parts.push(format!("{} {}", rep.short(), debug::object_label(doc, ObjectId::Representation(rep))));
    }
    if let Some(s) = r.subject {
        let id = ObjectId::from(s);
        parts.push(format!("{} {} {}", id.kind(), id.short(), debug::object_label(doc, id)));
    }
    if parts.is_empty() { "not found in the model".into() } else { parts.join(" → ") }
}

/// Previous/next buttons for a list of `n` rows.
fn pager(ui: &mut egui::Ui, what: &str, n: usize, page: &mut usize) {
    let pages = n.div_ceil(PAGE).max(1);
    *page = (*page).min(pages - 1);
    if pages <= 1 {
        return;
    }
    ui.horizontal(|ui| {
        if ui.add_enabled(*page > 0, egui::Button::new("◀")).clicked() {
            *page -= 1;
        }
        ui.label(format!("{what} page {} of {pages} ({n})", *page + 1));
        if ui.add_enabled(*page + 1 < pages, egui::Button::new("▶")).clicked() {
            *page += 1;
        }
    });
}
