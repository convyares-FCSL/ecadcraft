//! Read-only debugger snapshots (persistence stage 2b, proposal §21).
//!
//! The developer debugger follows one trail: **selected drawing objects → ECAD IDs → model objects
//! → the transactions that touched them, with before/after values → where each transaction's
//! record sits in the saved file, and its bytes.** This module supplies the data; the panel that
//! shows it is in the trial app.
//!
//! # Semantics
//! - **Coherent and short.** [`ProjectHandle::debug_snapshot`] copies what the panel shows out of
//!   the project under one `try_lock` and releases it before anything is drawn. A snapshot is
//!   labelled with the transaction it shows ([`DebugSnapshot::label`]). It is rebuilt only when
//!   its [`SnapshotKey`] changes (head, saved position, runtime events, active page, selection), so
//!   an idle panel costs one key comparison per frame. A busy project (`BUSY`) is not waited for:
//!   the panel keeps showing its previous snapshot, marked stale.
//! - **History is copied once.** Recorded transactions never change, so the panel keeps its own
//!   copy and a snapshot carries only the transactions after the copy's end ([`TxCursor`]).
//! - **Pending versus saved.** [`DebugSnapshot::record_state`] says, per transaction, whether it is
//!   only in memory (pending), written but not yet confirmed durable, or saved durably at a file
//!   offset ([`RecordState`]).
//! - **Expensive checks only on request.** A snapshot never replays the log, reprojects a page or
//!   reads the file. [`verify_replay`], [`verify_projection`] (inputs from
//!   [`ProjectHandle::debug_projection_inputs`]) and [`verify_file`] do, when asked, on copies and
//!   outside the lock. [`read_record`] reads one record's bytes, capped.
//! - **History never touches the live project.** [`state_at`] rebuilds the document after
//!   transaction `n` from the panel's copy of the log.
//!
//! Nothing here changes a project: the only project access is through `try_lock` reads.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;

use cadcraft_engine::doc::{Drawing, Handle};
use ecad_file::codec::{Codec, JsonLines};
use ecad_file::{AttemptStage, AttemptSummary, Header, RecordSpan};
use ecad_inspect::Event;
use ecad_model::{
    CableId, ComponentId, ConductorId, DefinitionId, Document, ItemId, ItemKind, LayerId, Ledger, Op, PageId, ProjectId, RepresentationId, Subject,
    TerminalId, Transaction, TxId, TxKind,
};
use serde_json::{Value, json};

use crate::access::PagesRef;
use crate::project::{HandleMap, Mapped, Persistence, Project};

/// Selected handles described per snapshot (the rest are counted).
pub const MAX_SELECTION: usize = 500;
/// Runtime events carried by a snapshot (the most recent).
pub const MAX_EVENTS: usize = 500;
/// Record bytes shown by [`read_record`]; longer records are checked in full but shown cut.
pub const MAX_RECORD_SHOWN: usize = 16 * 1024;
/// Largest file [`verify_file`] reads.
pub const MAX_VERIFY_FILE: u64 = 1 << 30;

/// Any ECAD object, for following IDs across the model and the history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObjectId {
    Page(PageId),
    Layer(LayerId),
    Item(ItemId),
    Component(ComponentId),
    Terminal(TerminalId),
    Representation(RepresentationId),
    Conductor(ConductorId),
    Cable(CableId),
    Definition(DefinitionId),
    /// The project settings (one per project).
    Settings,
}

impl ObjectId {
    /// The object kind in words.
    pub fn kind(self) -> &'static str {
        match self {
            ObjectId::Page(_) => "page",
            ObjectId::Layer(_) => "layer",
            ObjectId::Item(_) => "item",
            ObjectId::Component(_) => "component",
            ObjectId::Terminal(_) => "terminal",
            ObjectId::Representation(_) => "representation",
            ObjectId::Conductor(_) => "conductor",
            ObjectId::Cable(_) => "cable",
            ObjectId::Definition(_) => "definition",
            ObjectId::Settings => "settings",
        }
    }

    /// The short form (`cmp-1a2b3c4d`).
    pub fn short(self) -> String {
        match self {
            ObjectId::Page(i) => i.short(),
            ObjectId::Layer(i) => i.short(),
            ObjectId::Item(i) => i.short(),
            ObjectId::Component(i) => i.short(),
            ObjectId::Terminal(i) => i.short(),
            ObjectId::Representation(i) => i.short(),
            ObjectId::Conductor(i) => i.short(),
            ObjectId::Cable(i) => i.short(),
            ObjectId::Definition(i) => i.short(),
            ObjectId::Settings => "settings".into(),
        }
    }

    /// The full 32-hex-digit form, as saved in the file.
    pub fn hex(self) -> String {
        match self {
            ObjectId::Page(i) => i.hex(),
            ObjectId::Layer(i) => i.hex(),
            ObjectId::Item(i) => i.hex(),
            ObjectId::Component(i) => i.hex(),
            ObjectId::Terminal(i) => i.hex(),
            ObjectId::Representation(i) => i.hex(),
            ObjectId::Conductor(i) => i.hex(),
            ObjectId::Cable(i) => i.hex(),
            ObjectId::Definition(i) => i.hex(),
            ObjectId::Settings => String::new(),
        }
    }
}

impl std::fmt::Display for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.short())
    }
}

impl From<Subject> for ObjectId {
    fn from(s: Subject) -> Self {
        match s {
            Subject::Component(c) => ObjectId::Component(c),
            Subject::Conductor(c) => ObjectId::Conductor(c),
            Subject::Cable(c) => ObjectId::Cable(c),
        }
    }
}

/// What a snapshot was built from. The panel rebuilds only when this changes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SnapshotKey {
    pub project: Option<ProjectId>,
    pub head: u64,
    pub persisted: u64,
    /// Number of the last runtime event (saves, failures and recoveries all record one).
    pub last_event: u64,
    /// The active engine document, when it is one of the project's pages.
    pub active: Option<u64>,
    pub selection: Vec<Handle>,
}

/// The end of the caller's copy of the history: how many transactions it holds and the last one's
/// ID, so a snapshot only carries what is new (and everything when the copy does not match).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TxCursor {
    pub len: u64,
    pub last: Option<TxId>,
}

/// A selected drawing object and the ECAD ID its handle maps to.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectedObject {
    pub handle: Handle,
    /// CADCraft's type name for the entity (`Line`, `Insert`, …).
    pub entity: String,
    /// What the handle stands for; `None` for drawing objects that are not ECAD's.
    pub mapped: Option<Mapped>,
}

/// One page of the project.
#[derive(Debug, Clone, PartialEq)]
pub struct PageRow {
    pub id: PageId,
    pub title: String,
    /// The engine document showing it, when open.
    pub uid: Option<u64>,
    /// Handles in its handle map (open pages).
    pub mapped: Option<usize>,
    /// Whether the handle map is one-to-one (open pages).
    pub map_consistent: Option<bool>,
    /// Projection and rebuild both failed; the page must be reopened.
    pub unavailable: bool,
}

/// Where the project's file stands.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FileFacts {
    /// How the project saves (`None` before a project exists).
    pub persistence: Option<Persistence>,
    /// The file on disk, once there is one.
    pub path: Option<String>,
    /// Its writer lock file, while held.
    pub lock_file: Option<String>,
    /// The test store keeps its file in memory (automated tests only).
    pub in_memory: bool,
    pub header: Option<Header>,
    /// Bytes of the verified prefix (header included).
    pub verified_len: Option<u64>,
    /// The writer's acknowledged durable position (equals the ledger's when acknowledged).
    pub writer_persisted: Option<u64>,
    pub attempt: Option<AttemptSummary>,
    /// An incomplete trailing write reported at open, still in the file.
    pub pending_fragment: Option<u64>,
    /// Where each written transaction's record sits.
    pub spans: Vec<RecordSpan>,
    pub read_only: Option<String>,
    /// The last Save As replaced its target without confirming durability.
    pub unconfirmed: bool,
    /// What opening found, when the project was opened from a file.
    pub open_report: Option<String>,
    /// A version 2 file's facts (the debugger's Storage view).
    pub storage: Option<crate::file::StorageFacts>,
}

impl FileFacts {
    /// The facts as JSON, for exported inspector snapshots (`ecad-inspect/2`).
    pub fn to_json(&self) -> Value {
        json!({
            "persistence": self.persistence.map(|p| format!("{p:?}")),
            "path": self.path,
            "lock_file": self.lock_file,
            "in_memory": self.in_memory,
            "header": self.header,
            "verified_len": self.verified_len,
            "writer_persisted": self.writer_persisted,
            "attempt": self.attempt.as_ref().map(|a| json!({
                "offset": a.offset, "len": a.len, "first_seq": a.first_seq, "last_seq": a.last_seq,
                "stage": format!("{:?}", a.stage),
            })),
            "pending_fragment": self.pending_fragment,
            "records": self.spans.iter().map(|s| json!({"seq": s.seq, "offset": s.offset, "len": s.len})).collect::<Vec<_>>(),
            "read_only": self.read_only,
            "unconfirmed": self.unconfirmed,
            "open_report": self.open_report,
            "storage": self.storage.as_ref().map(|s| json!({
                "branch": s.branch, "head": s.head, "snapshot_seq": s.snapshot_seq,
                "snapshot_every": s.snapshot_every, "writable": s.writable,
                "link_v1": s.link_v1.as_ref().map(|(h, n)| json!({"hash": h, "transactions": n})),
                "last_save": s.last_save.as_ref().map(|l| json!({
                    "first_seq": l.first_seq, "last_seq": l.last_seq, "bytes": l.bytes,
                    "chunks_written": l.chunks_written, "snapshots": l.snapshots,
                })),
            })),
        })
    }
}

/// Transactions per state; see [`DebugSnapshot::state_counts`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StateCounts {
    pub saved: u64,
    pub unconfirmed: u64,
    pub pending: u64,
}

/// Where one transaction stands with respect to the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordState {
    /// Saved: acknowledged durable, in the file at `span` (when the writer knows the span).
    Saved(Option<RecordSpan>),
    /// Written by a Save As whose durability could not be confirmed; Save confirms it.
    Unconfirmed(RecordSpan),
    /// Only in memory. `attempt`: a failed save tried to write it and may have left bytes.
    Pending { attempt: Option<AttemptStage> },
    /// The project has no persistence: nothing is ever saved.
    NotSaved,
}

impl RecordState {
    /// A short label for listings.
    pub fn label(self) -> &'static str {
        match self {
            RecordState::Saved(_) => "saved",
            RecordState::Unconfirmed(_) => "written, unconfirmed",
            RecordState::Pending { attempt: None } => "pending",
            RecordState::Pending { attempt: Some(_) } => "pending (save failed)",
            RecordState::NotSaved => "not saved",
        }
    }

    /// Whether the transaction is durably saved.
    pub fn durable(self) -> bool {
        matches!(self, RecordState::Saved(_))
    }
}

/// Everything the debugger panel shows, copied from the project at one moment.
#[derive(Debug, Clone)]
pub struct DebugSnapshot {
    pub key: SnapshotKey,
    pub name: String,
    pub project: ProjectId,
    pub head: u64,
    /// The last durably saved transaction.
    pub persisted: u64,
    /// Transactions the banner counts as unsaved.
    pub unsaved: u64,
    /// The live document, copied.
    pub doc: Document,
    /// `new_transactions` follow the caller's copy of the history from here (0: a full copy).
    pub log_from: u64,
    pub new_transactions: Vec<Transaction>,
    /// The active page of the project, when one of its pages is the active document.
    pub active_page: Option<PageId>,
    /// The active page's selection, mapped (at most [`MAX_SELECTION`]).
    pub selection: Vec<SelectedObject>,
    /// Selected handles in all (may exceed `selection.len()`).
    pub selection_total: usize,
    pub pages: Vec<PageRow>,
    /// The most recent runtime events (at most [`MAX_EVENTS`]); never part of the saved history.
    pub events: Vec<Event>,
    pub file: FileFacts,
    /// The `seq` the copied history starts after (a project opened from a version 2 snapshot);
    /// `new_transactions[i].seq == base + log_from + i + 1`.
    pub base: u64,
    /// Where the history before `base` is read, on request only.
    pub history: Option<Arc<dyn ecad_model::History>>,
    /// The bound version 2 file's database, for on-request reads (records, Verify file, the
    /// Storage and Branches views). Reads only; weak, so a copy never keeps a closed project's
    /// file open.
    pub store: Option<std::sync::Weak<ecad_file::v2::Store>>,
}

impl DebugSnapshot {
    /// The bound file's database, while the project still has it open.
    pub fn store_arc(&self) -> Option<Arc<ecad_file::v2::Store>> {
        self.store.as_ref().and_then(std::sync::Weak::upgrade)
    }

    /// The copied history's transaction `seq`, from a copy that starts after [`Self::base`].
    pub fn tx_in<'a>(&self, log: &'a [Transaction], seq: u64) -> Option<&'a Transaction> {
        seq.checked_sub(self.base + 1).and_then(|i| usize::try_from(i).ok()).and_then(|i| log.get(i))
    }
    /// What the snapshot shows, for its heading.
    pub fn label(&self) -> String {
        format!("live at seq {}", self.head)
    }

    /// Where transaction `seq` stands with respect to the file.
    pub fn record_state(&self, seq: u64) -> RecordState {
        record_state(&self.file, self.persisted, seq)
    }

    /// How many transactions are saved durably, written but not confirmed durable, and only in
    /// memory (in that order: they are consecutive ranges of the history).
    pub fn state_counts(&self) -> StateCounts {
        if self.file.persistence == Some(Persistence::Unavailable) {
            return StateCounts { saved: 0, unconfirmed: 0, pending: self.head };
        }
        let saved = self.persisted.min(self.head);
        let spans = &self.file.spans;
        let written_after = spans.len().saturating_sub(spans.partition_point(|s| s.seq <= saved)) as u64;
        let unconfirmed = written_after.min(self.head - saved);
        StateCounts { saved, unconfirmed, pending: self.head - saved - unconfirmed }
    }

    /// The record span of transaction `seq`, when it has been written.
    pub fn span(&self, seq: u64) -> Option<RecordSpan> {
        span_of(&self.file.spans, seq)
    }
}

fn span_of(spans: &[RecordSpan], seq: u64) -> Option<RecordSpan> {
    // Spans are in sequence order, but not necessarily from seq 1 (none for a memory test store
    // before its first save), so search.
    spans.binary_search_by_key(&seq, |s| s.seq).ok().and_then(|i| spans.get(i).copied())
}

/// Where transaction `seq` stands, given the file facts and the durable position.
pub fn record_state(file: &FileFacts, persisted: u64, seq: u64) -> RecordState {
    if file.persistence == Some(Persistence::Unavailable) {
        return RecordState::NotSaved;
    }
    let span = span_of(&file.spans, seq);
    if seq <= persisted {
        return RecordState::Saved(span);
    }
    match span {
        Some(s) => RecordState::Unconfirmed(s),
        None => RecordState::Pending { attempt: file.attempt.as_ref().filter(|a| (a.first_seq..=a.last_seq).contains(&seq)).map(|a| a.stage) },
    }
}

impl Project {
    fn front_page(&self, pg: &PagesRef<'_>) -> (Option<u64>, Option<PageId>, Vec<Handle>) {
        match pg.active() {
            Some((uid, sel)) => match self.page_of(uid) {
                Some(page) => (Some(uid), Some(page), sel),
                None => (None, None, Vec::new()),
            },
            None => (None, None, Vec::new()),
        }
    }

    /// The key of the snapshot that would be built now.
    pub(crate) fn debug_key(&self, pg: &PagesRef<'_>) -> SnapshotKey {
        let (active, _, selection) = self.front_page(pg);
        SnapshotKey {
            project: Some(self.id),
            head: self.ledger.head(),
            persisted: self.ledger.persisted(),
            last_event: self.events.last().map_or(0, |e| e.n),
            active,
            selection,
        }
    }

    /// The file facts (cheap: no file access).
    pub fn file_facts(&self) -> FileFacts {
        let mut f = FileFacts {
            persistence: Some(self.persistence),
            open_report: self
                .open_report
                .as_ref()
                .map(ecad_file::OpenReport::describe)
                .or_else(|| self.open_report_v2.as_ref().map(ecad_file::v2::ReportV2::describe)),
            ..FileFacts::default()
        };
        let writer = match self.persistence {
            Persistence::TestStore => {
                f.in_memory = true;
                self.store.file().map(|w| (w.header().clone(), w.summary(), w.spans().to_vec()))
            }
            Persistence::File => {
                let st = self.file.status();
                f.path = st.path;
                f.read_only = st.read_only;
                f.unconfirmed = st.unconfirmed;
                f.lock_file = self.file.lock_file();
                f.storage = self.file.storage();
                f.writer_persisted = f.storage.as_ref().map(|s| s.head);
                None
            }
            Persistence::Unavailable => None,
        };
        if let Some((header, state, spans)) = writer {
            f.header = Some(header);
            f.verified_len = Some(state.verified_len);
            f.writer_persisted = Some(state.persisted);
            f.attempt = state.attempt;
            f.pending_fragment = state.pending_fragment;
            f.spans = spans;
        }
        f
    }

    /// Build a snapshot (under the caller's lock; copies only).
    pub(crate) fn debug_snapshot(&self, pg: &PagesRef<'_>, key: SnapshotKey, have: TxCursor) -> DebugSnapshot {
        let (_, active_page, selection) = self.front_page(pg);
        let log = self.ledger.log();
        // Continue the caller's copy only when its last transaction is ours.
        let continues = match (have.len, have.last) {
            (0, _) => false,
            (n, Some(id)) => usize::try_from(n - 1).ok().and_then(|i| log.get(i)).is_some_and(|t| t.id == id),
            (_, None) => false,
        };
        let log_from = if continues { have.len } else { 0 };
        let new_transactions = log.iter().skip(usize::try_from(log_from).unwrap_or(usize::MAX)).cloned().collect();
        let map = active_page.and_then(|p| self.pages.get(&p)).map(|v| &v.map);
        let drawing = active_page.and_then(|p| pg.drawing_of(self, p));
        let selected = selection
            .iter()
            .take(MAX_SELECTION)
            .map(|h| SelectedObject {
                handle: *h,
                entity: drawing.as_ref().and_then(|d| d.entity(*h)).map_or_else(|| "missing".to_string(), |e| e.kind.type_name().to_string()),
                mapped: map.and_then(|m| m.get(*h)),
            })
            .collect();
        let doc = self.ledger.document();
        let pages = doc
            .page_order
            .iter()
            .map(|p| {
                let view = self.pages.get(p);
                let open = view.and_then(|v| v.uid).filter(|u| pg.doc(*u).is_some());
                PageRow {
                    id: *p,
                    title: doc.pages.get(p).map(|x| x.title.clone()).unwrap_or_default(),
                    uid: open,
                    mapped: view.filter(|_| open.is_some()).map(|v| v.map.len()),
                    map_consistent: view.filter(|_| open.is_some()).map(|v| v.map.is_consistent()),
                    unavailable: view.is_some_and(|v| v.unavailable),
                }
            })
            .collect();
        let events: Vec<Event> = self.events.events().skip(self.events.len().saturating_sub(MAX_EVENTS)).cloned().collect();
        DebugSnapshot {
            key,
            name: self.name.clone(),
            project: self.id,
            head: self.ledger.head(),
            persisted: self.ledger.persisted(),
            unsaved: self.unsaved_transactions(),
            doc: doc.clone(),
            log_from,
            new_transactions,
            active_page,
            selection: selected,
            selection_total: selection.len(),
            pages,
            events,
            file: self.file_facts(),
            base: self.ledger.base(),
            history: self.ledger.history().cloned(),
            store: self.file.store().as_ref().map(Arc::downgrade),
        }
    }

    /// Copies for [`verify_projection`].
    pub(crate) fn projection_inputs(&self, pg: &PagesRef<'_>) -> ProjectionInputs {
        let pages = self
            .pages
            .iter()
            .filter_map(|(id, v)| {
                let uid = v.uid?;
                let drawing = pg.doc(uid)?.doc.clone();
                Some(PageInputs { id: *id, drawing, map: v.map.clone() })
            })
            .collect();
        ProjectionInputs {
            head: self.ledger.head(),
            doc: self.ledger.document().clone(),
            pages,
            views: self.views.clone(),
            tolerance: self.tolerance,
            hidden_text: self.show_hidden_text,
            pending: self.pending,
        }
    }
}

/// What [`verify_projection`] needs, copied under the lock.
#[derive(Debug, Clone)]
pub struct ProjectionInputs {
    pub head: u64,
    pub doc: Document,
    pub pages: Vec<PageInputs>,
    /// Cables shown in fan view: the fresh projection uses the same views (design 005 §8).
    pub views: std::collections::BTreeSet<ecad_model::CableId>,
    pub tolerance: f64,
    /// Show Hidden Text (brief 031): the fresh projection draws hidden texts the same way.
    pub hidden_text: bool,
    /// Brief 033: the project's pending flag, so the fresh frame shows the same Rev box.
    pub pending: bool,
}

/// One open page's drawing and handle map.
#[derive(Debug, Clone)]
pub struct PageInputs {
    pub id: PageId,
    pub drawing: Arc<Drawing>,
    pub map: HandleMap,
}

/// The result of an explicit check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub what: &'static str,
    /// The head the check ran against.
    pub at_seq: u64,
    pub ok: bool,
    pub lines: Vec<String>,
}

/// Rebuild the reference index (brief 025) from a copy of the document and compare it with a copy
/// of the kept one (the debugger's **Check index**, design 025 §3.3).
pub fn verify_index(doc: &Document, kept: &crate::xref::RefIndex, head: u64) -> Check {
    let (members, groups, edges) = kept.sizes();
    match kept.verify(doc) {
        Ok(()) => Check {
            what: "reference index",
            at_seq: head,
            ok: true,
            lines: vec![format!("{members} members in {groups} groups, {edges} edges: the same as a rebuild.")],
        },
        Err(e) => Check { what: "reference index", at_seq: head, ok: false, lines: vec![e] },
    }
}

/// Replay the history into a fresh document and compare it with the live one (copies of both),
/// then check the document's invariants.
///
/// The replay runs in verify mode: every transaction is checked in full as well as by the scoped
/// check, and any disagreement fails it.
pub fn verify_replay(log: &[Transaction], live: &Document) -> Check {
    let at_seq = log.len() as u64;
    let mut lines = Vec::new();
    let ok = match Ledger::replay_verified(log.to_vec(), 0) {
        Ok(l) if l.document() == live => {
            lines.push(format!("replaying {at_seq} transactions gives the live document"));
            true
        }
        Ok(_) => {
            lines.push(format!("replaying {at_seq} transactions gives a DIFFERENT document"));
            false
        }
        Err(e) => {
            lines.push(format!("replay failed: {e}"));
            false
        }
    };
    let invariants = live.check();
    match &invariants {
        Ok(()) => lines.push("document invariants hold".into()),
        Err(e) => lines.push(format!("document invariants fail: {e}")),
    }
    Check { what: "replay", at_seq, ok: ok && invariants.is_ok(), lines }
}

/// [`verify_replay`] for a copy of the history that starts after `base` (a project opened from a
/// version 2 snapshot): the state at `base` is read from `history`, then the copy is applied.
pub fn verify_replay_from(base: u64, history: Option<&Arc<dyn ecad_model::History>>, log: &[Transaction], live: &Document) -> Check {
    if base == 0 {
        return verify_replay(log, live);
    }
    let at_seq = base + log.len() as u64;
    let mut lines = Vec::new();
    let replayed = history
        .ok_or_else(|| "no history to read the snapshot from".to_string())
        .and_then(|h| h.state_at(base).map_err(|e| e.to_string()))
        .and_then(|mut doc| {
            lines.push(format!("the state at the snapshot ({base}) was rebuilt from the file"));
            for tx in log {
                for op in &tx.ops {
                    op.apply(&mut doc).map_err(|e| format!("transaction {}: {e}", tx.seq))?;
                }
            }
            Ok(doc)
        });
    let ok = match replayed {
        Ok(d) if &d == live => {
            lines.push(format!("replaying the {} transactions after it gives the live document", log.len()));
            true
        }
        Ok(_) => {
            lines.push(format!("replaying the {} transactions after it gives a DIFFERENT document", log.len()));
            false
        }
        Err(e) => {
            lines.push(format!("replay failed: {e}"));
            false
        }
    };
    let invariants = live.check();
    match &invariants {
        Ok(()) => lines.push("document invariants hold".into()),
        Err(e) => lines.push(format!("document invariants fail: {e}")),
    }
    Check { what: "replay", at_seq, ok: ok && invariants.is_ok(), lines }
}

/// **Verify file** for a version 2 file (EEP-0005 §5.8): replay the branch from the root with
/// every check and check every snapshot against the replay, then compare the file's rows with the
/// copied history (after `base`) up to the saved position. Reads only, on request.
pub fn verify_store(store: &ecad_file::v2::Store, base: u64, log: &[Transaction], persisted: u64) -> Check {
    let at_seq = base + log.len() as u64;
    let v = ecad_file::v2::verify::verify(store);
    let mut ok = v.ok;
    let mut lines = v.lines;
    let rows = store.reader().map_err(|e| e.to_string()).and_then(|r| {
        let branch =
            r.branches().map_err(|e| e.to_string())?.into_iter().find(|(_, b)| b.name == "main").map(|(id, _)| id).ok_or("no main branch")?;
        let head = r.last_seq(branch).map_err(|e| e.to_string())?.unwrap_or(0);
        Ok((r, branch, head))
    });
    match rows {
        Ok((r, branch, head)) => {
            if head < persisted {
                ok = false;
                lines.push(format!("the file ends at transaction {head}, before the saved position {persisted}"));
            }
            let mut differ = None;
            for tx in log.iter().filter(|t| t.seq <= head.min(persisted)) {
                let row = r.tx(branch, tx.seq).ok().flatten();
                let same = row.and_then(|(flag, _, body)| ecad_file::v2::pack::unpack(flag, &body).ok()).is_some_and(|rec| {
                    serde_json::from_slice::<Value>(&rec).ok().and_then(|v| ecad_file::record::decode_tx(&v).ok()).is_some_and(|t| &t == tx)
                });
                if !same {
                    differ = Some(tx.seq);
                    break;
                }
            }
            match differ {
                Some(seq) => {
                    ok = false;
                    lines.push(format!("transaction {seq} in the file DIFFERS from the project's"));
                }
                None => lines.push(format!(
                    "the file's rows match the project's history; saved through {persisted}, {} pending in memory",
                    at_seq.saturating_sub(persisted)
                )),
            }
        }
        Err(e) => {
            ok = false;
            lines.push(format!("the file's rows could not be read: {e}"));
        }
    }
    Check { what: "file", at_seq, ok, lines }
}

/// Read transaction `seq`'s row from a version 2 file (no project lock) and check that it decodes
/// to exactly `expected`. `shown` is the record, decompressed; the span gives its stored size.
///
/// # Errors
/// The file cannot be read, or holds no such row.
pub fn read_row(store: &ecad_file::v2::Store, seq: u64, expected: Option<&Transaction>) -> Result<RecordBytes, String> {
    let r = store.reader().map_err(|e| e.to_string())?;
    let branch = r.branches().map_err(|e| e.to_string())?.into_iter().find(|(_, b)| b.name == "main").map(|(id, _)| id).ok_or("no main branch")?;
    let (flag, chain, body) = r.tx(branch, seq).map_err(|e| e.to_string())?.ok_or(format!("no row for {seq}"))?;
    let record = ecad_file::v2::pack::unpack(flag, &body)?;
    let check = (|| {
        if ecad_file::chain::tx_hash(&record) != chain {
            return Err("the record does not match its chain value".to_string());
        }
        let v: Value = serde_json::from_slice(&record).map_err(|e| format!("not JSON: {e}"))?;
        let tx = ecad_file::record::decode_tx(&v).map_err(|e| format!("not a transaction record: {e:?}"))?;
        match expected {
            Some(e) if *e != tx => Err(format!("transaction {seq} in the file differs from the one in memory")),
            _ => Ok(()),
        }
    })();
    let span = RecordSpan { seq, offset: 0, len: (body.len() + 33) as u64 };
    let mut shown = record;
    let truncated = shown.len() > MAX_RECORD_SHOWN;
    shown.truncate(MAX_RECORD_SHOWN);
    Ok(RecordBytes { span, shown, truncated, check })
}

/// What the Storage and Branches views read from a version 2 file, on request: table sizes, the
/// file's size, branches and version rows (without their chunk maps).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StorageRead {
    pub file_bytes: u64,
    pub tables: Vec<ecad_file::v2::TableStat>,
    /// Name, ID, status, head, rows, parent and fork point.
    pub branches: Vec<BranchRow>,
    /// Branch name, kind, name, point and chunk count.
    pub versions: Vec<(String, String, String, u64, usize)>,
}

/// One branch, for the Branches view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRow {
    pub name: String,
    pub id: String,
    pub status: String,
    pub head: u64,
    pub head_chain: String,
    pub rows: u64,
    pub release_line: bool,
    /// Parent branch and fork point, for a branch that is not the original.
    pub fork: Option<(String, u64)>,
}

/// The branches of a version 2 file (a few rows and one key read each; read after each save, so
/// its cost never depends on the history's length).
///
/// # Errors
/// The file cannot be read.
pub fn read_branches(store: &ecad_file::v2::Store) -> Result<Vec<BranchRow>, String> {
    let r = store.reader().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (id, b) in r.branches().map_err(|e| e.to_string())? {
        out.push(BranchRow {
            name: b.name.clone(),
            id: id.hex(),
            status: format!("{:?}", b.status),
            head: b.head.seq,
            head_chain: b.head.chain.clone(),
            // The last stored `seq`, one key read: a branch's rows run 1..=last without gaps, so
            // it is the row count. Counting the rows would read the whole log after every save.
            rows: r.last_seq(id).map_err(|e| e.to_string())?.unwrap_or(0),
            release_line: b.release_line,
            fork: b.parent.clone().zip(b.fork_point.as_ref().map(|f| f.seq)),
        });
    }
    Ok(out)
}

/// Read the Storage view's facts (on request: table statistics walk the tables).
///
/// # Errors
/// The file cannot be read.
pub fn read_storage(store: &ecad_file::v2::Store, path: Option<&Path>) -> Result<StorageRead, String> {
    let r = store.reader().map_err(|e| e.to_string())?;
    let tables = r.stats().map_err(|e| e.to_string())?;
    let mut versions = Vec::new();
    for (id, b) in r.branches().map_err(|e| e.to_string())? {
        for kind in [ecad_file::v2::schema::VersionKind::User, ecad_file::v2::schema::VersionKind::Revision, ecad_file::v2::schema::VersionKind::Auto]
        {
            for v in r.versions(id, kind).map_err(|e| e.to_string())? {
                versions.push((b.name.clone(), format!("{kind:?}"), v.name.clone(), v.point.seq, v.chunks.len()));
            }
        }
    }
    drop(r);
    Ok(StorageRead {
        file_bytes: path.and_then(|p| std::fs::metadata(p).ok()).map_or(0, |m| m.len()),
        tables,
        branches: read_branches(store)?,
        versions,
    })
}

/// Reproject every open page from the document and compare (same handles), then check each page
/// without relying on handle numbers ([`verify_page`]). Runs on copies, outside the lock.
pub fn verify_projection(inputs: &ProjectionInputs) -> Check {
    let mut lines = Vec::new();
    let mut ok = true;
    if inputs.pages.is_empty() {
        lines.push("no page is open".into());
    }
    for p in &inputs.pages {
        let title = inputs.doc.pages.get(&p.id).map_or_else(|| p.id.short(), |x| x.title.clone());
        let same = crate::project::project_page_pending(
            &inputs.doc,
            p.id,
            &p.drawing,
            &p.map,
            &inputs.views,
            inputs.tolerance,
            inputs.hidden_text,
            inputs.pending,
        )
        .map(|(d, m)| d.model == p.drawing.model && m == p.map && p.map.is_consistent());
        match same {
            Ok(true) => lines.push(format!("page {title}: drawing equals a fresh projection ({} handles)", p.map.len())),
            Ok(false) => {
                ok = false;
                lines.push(format!("page {title}: drawing DIFFERS from a fresh projection"));
            }
            Err(e) => {
                ok = false;
                lines.push(format!("page {title}: projection failed: {e}"));
            }
        }
        let v = crate::verify::verify_page_pending(
            &inputs.doc,
            p.id,
            &p.drawing,
            &p.map,
            &inputs.views,
            inputs.tolerance,
            inputs.hidden_text,
            inputs.pending,
        );
        ok &= v.ok();
        lines.push(format!("page {title}: {}", v.summary()));
    }
    Check { what: "projection", at_seq: inputs.head, ok, lines }
}

/// Re-read the project file from disk and check it end to end (frames, checksums, hash chain,
/// sequence, replay), then compare its history and record positions with the snapshot's. Reads
/// only; the file and the project are unchanged.
pub fn verify_file(path: &Path, log: &[Transaction], file: &FileFacts, persisted: u64) -> Check {
    let at_seq = log.len() as u64;
    let fail = |m: String| Check { what: "file", at_seq, ok: false, lines: vec![m] };
    let bytes = match read_capped(path, MAX_VERIFY_FILE) {
        Ok(b) => b,
        Err(e) => return fail(format!("could not read {}: {e}", path.display())),
    };
    let opened = match ecad_file::open_project(&bytes, 0) {
        Ok(o) => o,
        Err(e) => return fail(format!("{} does not open: {e}", path.display())),
    };
    let mut lines = vec![format!("{} ({} bytes): {}", path.display(), bytes.len(), opened.report.describe())];
    let mut ok = opened.report.outcome == ecad_file::Outcome::Clean;
    let in_file = opened.ledger.log();
    let n = in_file.len();
    if log.get(..n) == Some(in_file) {
        lines.push(format!("the file's {n} transactions are the first {n} of the project's history"));
    } else {
        ok = false;
        lines.push(format!("the file's {n} transactions DIFFER from the project's history"));
    }
    if (n as u64) < persisted {
        ok = false;
        lines.push(format!("the file ends at transaction {n}, before the saved position {persisted}"));
    } else {
        lines.push(format!("saved through transaction {persisted}; {} pending in memory", at_seq.saturating_sub(n as u64)));
    }
    if opened.spans == file.spans {
        lines.push(format!("all {} record offsets match the writer's", opened.spans.len()));
    } else {
        ok = false;
        lines.push("record offsets DIFFER from the writer's".into());
    }
    if file.header.as_ref().is_some_and(|h| *h != opened.header) {
        ok = false;
        lines.push("the header DIFFERS from the writer's".into());
    }
    Check { what: "file", at_seq, ok, lines }
}

fn read_capped(path: &Path, cap: u64) -> Result<Vec<u8>, String> {
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let len = f.metadata().map_err(|e| e.to_string())?.len();
    if len > cap {
        return Err(format!("it is {len} bytes, more than the debugger reads ({cap})"));
    }
    let mut bytes = Vec::new();
    f.take(cap).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// One record's bytes, read from the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordBytes {
    pub span: RecordSpan,
    /// The first [`MAX_RECORD_SHOWN`] bytes.
    pub shown: Vec<u8>,
    pub truncated: bool,
    /// Whether the bytes are a valid frame decoding to exactly the expected transaction.
    pub check: Result<(), String>,
}

/// Read the record at `span` from `path` (outside any lock) and check that it is a valid frame
/// holding exactly `expected`.
///
/// # Errors
/// The file cannot be read at that position.
pub fn read_record(path: &Path, span: RecordSpan, expected: Option<&Transaction>) -> Result<RecordBytes, String> {
    let len = usize::try_from(span.len).ok().filter(|l| *l <= ecad_file::record::MAX_LINE + 64);
    let len = len.ok_or_else(|| format!("record of {} bytes is longer than any valid record", span.len))?;
    let mut f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    f.seek(SeekFrom::Start(span.offset)).map_err(|e| e.to_string())?;
    let mut bytes = vec![0; len];
    f.read_exact(&mut bytes).map_err(|e| format!("reading {} bytes at offset {}: {e}", span.len, span.offset))?;
    let check = check_record(&bytes, span.seq, expected);
    let truncated = bytes.len() > MAX_RECORD_SHOWN;
    bytes.truncate(MAX_RECORD_SHOWN);
    Ok(RecordBytes { span, shown: bytes, truncated, check })
}

fn check_record(bytes: &[u8], seq: u64, expected: Option<&Transaction>) -> Result<(), String> {
    let frame = bytes.strip_suffix(b"\n").ok_or("the record does not end with a line break")?;
    let payload = JsonLines.payload(frame).map_err(|e| e.to_string())?;
    let value: Value = serde_json::from_slice(payload).map_err(|e| format!("not JSON: {e}"))?;
    let tx = ecad_file::record::decode_tx(&value).map_err(|e| format!("not a transaction record: {e:?}"))?;
    if tx.seq != seq {
        return Err(format!("it holds transaction {}, not {seq}", tx.seq));
    }
    match expected {
        Some(e) if *e != tx => Err(format!("transaction {seq} in the file differs from the one in memory")),
        _ => Ok(()),
    }
}

/// The document after transaction `seq`, rebuilt from a copy of the history. The live project is
/// not involved.
///
/// # Errors
/// `seq` past the end of `log`, or replay failure.
pub fn state_at_from(base: u64, history: Option<&Arc<dyn ecad_model::History>>, log: &[Transaction], seq: u64) -> Result<Document, String> {
    if base == 0 {
        return state_at(log, seq);
    }
    let h = history.ok_or("no history to read older states from")?;
    if seq <= base {
        return h.state_at(seq).map_err(|e| e.to_string());
    }
    let n = usize::try_from(seq - base).map_err(|e| e.to_string())?;
    let tail = log.get(..n).ok_or_else(|| format!("transaction {seq} is past the head {}", base + log.len() as u64))?;
    let mut doc = h.state_at(base).map_err(|e| e.to_string())?;
    for tx in tail {
        for op in &tx.ops {
            op.apply(&mut doc).map_err(|e| format!("transaction {}: {e}", tx.seq))?;
        }
    }
    Ok(doc)
}

/// The document after transaction `seq`, replayed from a copy of the history from the root.
///
/// # Errors
/// Past the copy's end, or replay failed.
pub fn state_at(log: &[Transaction], seq: u64) -> Result<Document, String> {
    let n = usize::try_from(seq).map_err(|e| e.to_string())?;
    let prefix = log.get(..n).ok_or_else(|| format!("transaction {seq} is past the head {}", log.len()))?;
    Ledger::replay(prefix.to_vec(), 0).map(|l| l.document().clone()).map_err(|e| e.to_string())
}

/// A transaction's kind in words.
pub fn tx_kind(tx: &Transaction, log: &[Transaction]) -> String {
    let seq_of = |id: TxId| log.iter().find(|t| t.id == id).map_or_else(|| id.short(), |t| format!("#{}", t.seq));
    match tx.kind {
        TxKind::Edit => "edit".into(),
        TxKind::Undo { of } => format!("undo of {}", seq_of(of)),
        TxKind::Redo { of } => format!("redo of {}", seq_of(of)),
        TxKind::Merge { source, source_head, .. } => {
            format!("merge of {}… at {}", source.iter().take(4).map(|b| format!("{b:02x}")).collect::<String>(), source_head.seq)
        }
    }
}

/// One op's target, before and after values (JSON of the model's own serialisation).
#[derive(Debug, Clone, PartialEq)]
pub struct OpView {
    pub name: &'static str,
    /// Every object the op names: its target first, then the objects it refers to.
    pub targets: Vec<ObjectId>,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

fn val<T: serde::Serialize>(v: &T) -> Option<Value> {
    Some(serde_json::to_value(v).unwrap_or_else(|e| Value::String(format!("(cannot show: {e})"))))
}

/// An op's view.
pub fn op_view(op: &Op) -> OpView {
    let (before, after) = match op {
        Op::AddPage { page, index, .. } => (None, val(&json!({"page": page, "index": index}))),
        Op::RemovePage { page, index, .. } => (val(&json!({"page": page, "index": index})), None),
        Op::AddLayer { layer, .. } => (None, val(layer)),
        Op::RemoveLayer { layer, .. } => (val(layer), None),
        Op::AddItem { item, .. } => (None, val(item)),
        Op::ModifyItem { before, after, .. } => (val(before), val(after)),
        Op::RemoveItem { item, .. } => (val(item), None),
        Op::CreateComponent { component, .. } => (None, val(component)),
        Op::DeleteComponent { component, .. } => (val(component), None),
        Op::SetComponentDefinition { before, after, .. } => (val(before), val(after)),
        Op::AddTerminal { terminal, .. } => (None, val(terminal)),
        Op::RemoveTerminal { terminal, .. } => (val(terminal), None),
        Op::LinkRepresentation { rep, .. } => (None, val(rep)),
        Op::ModifyRepresentation { before, after, .. } => (val(before), val(after)),
        Op::UnlinkRepresentation { rep, .. } => (val(rep), None),
        Op::CreateConductor { conductor, .. } => (None, val(conductor)),
        Op::DeleteConductor { conductor, .. } => (val(conductor), None),
        Op::AttachEnd { end, before, after, .. } => (val(&json!({"end": end, "terminal": before})), val(&json!({"end": end, "terminal": after}))),
        Op::CreateCable { cable, .. } => (None, val(cable)),
        Op::DeleteCable { cable, .. } => (val(cable), None),
        Op::AddCableMember { index, member, .. } => (None, val(&json!({"index": index, "member": member}))),
        Op::RemoveCableMember { index, member, .. } => (val(&json!({"index": index, "member": member})), None),
        Op::SetProperty { key, before, after, .. } => (val(&json!({ key.as_str(): before })), val(&json!({ key.as_str(): after }))),
        Op::EmbedDefinition { definition, .. } => (None, val(definition)),
        Op::RemoveDefinition { definition, .. } => (val(definition), None),
        Op::SetProjectSettings { before, after } => (val(before), val(after)),
        Op::NameVersion { name, at, .. } => (None, val(&json!({"name": name, "at": at}))),
        Op::UnnameVersion { name, at, .. } => (val(&json!({"name": name, "at": at})), None),
        Op::AddFolder { folder, index, .. } => (None, val(&json!({"folder": folder, "index": index}))),
        Op::RemoveFolder { folder, index, .. } => (val(&json!({"folder": folder, "index": index})), None),
        Op::ModifyFolder { before, after, .. } => (val(before), val(after)),
        Op::SetStyle { layer, catalogue, id, before, after } => (
            val(&json!({"layer": layer, "catalogue": catalogue, "id": id, "record": before})),
            val(&json!({"layer": layer, "catalogue": catalogue, "id": id, "record": after})),
        ),
        Op::SetStandard { before, after } => (val(before), val(after)),
    };
    OpView { name: op.name(), targets: op_targets(op), before, after }
}

fn item_refs(item: &ecad_model::Item, out: &mut Vec<ObjectId>) {
    out.push(ObjectId::Page(item.page));
    out.push(ObjectId::Layer(item.style.layer));
    if let ItemKind::Symbol(s) = &item.kind {
        out.push(ObjectId::Definition(s.definition));
    }
}

fn rep_refs(rep: &ecad_model::Representation, out: &mut Vec<ObjectId>) {
    out.push(rep.subject.into());
    out.push(ObjectId::Page(rep.page));
    out.extend(rep.items.iter().map(|i| ObjectId::Item(*i)));
    out.extend(rep.connection_points.keys().map(|t| ObjectId::Terminal(*t)));
}

/// Every object an op names: its target first.
pub fn op_targets(op: &Op) -> Vec<ObjectId> {
    let mut out = Vec::new();
    match op {
        Op::AddPage { id, .. } | Op::RemovePage { id, .. } => out.push(ObjectId::Page(*id)),
        Op::AddLayer { id, .. } | Op::RemoveLayer { id, .. } => out.push(ObjectId::Layer(*id)),
        Op::AddItem { id, item } | Op::RemoveItem { id, item } => {
            out.push(ObjectId::Item(*id));
            item_refs(item, &mut out);
        }
        Op::ModifyItem { id, before, after } => {
            out.push(ObjectId::Item(*id));
            item_refs(before, &mut out);
            item_refs(after, &mut out);
        }
        Op::CreateComponent { id, component } | Op::DeleteComponent { id, component } => {
            out.push(ObjectId::Component(*id));
            out.extend(component.definition.map(ObjectId::Definition));
        }
        Op::SetComponentDefinition { id, before, after } => {
            out.push(ObjectId::Component(*id));
            out.extend(before.iter().chain(after).copied().map(ObjectId::Definition));
        }
        Op::AddTerminal { id, terminal } | Op::RemoveTerminal { id, terminal } => {
            out.push(ObjectId::Terminal(*id));
            out.push(ObjectId::Component(terminal.component));
        }
        Op::LinkRepresentation { id, rep } | Op::UnlinkRepresentation { id, rep } => {
            out.push(ObjectId::Representation(*id));
            rep_refs(rep, &mut out);
        }
        Op::ModifyRepresentation { id, before, after } => {
            out.push(ObjectId::Representation(*id));
            rep_refs(before, &mut out);
            rep_refs(after, &mut out);
        }
        Op::CreateConductor { id, conductor } | Op::DeleteConductor { id, conductor } => {
            out.push(ObjectId::Conductor(*id));
            out.extend(conductor.ends.iter().flatten().map(|t| ObjectId::Terminal(*t)));
        }
        Op::AttachEnd { conductor, before, after, .. } => {
            out.push(ObjectId::Conductor(*conductor));
            out.extend(before.iter().chain(after.iter()).map(|t| ObjectId::Terminal(*t)));
        }
        Op::CreateCable { id, cable } | Op::DeleteCable { id, cable } => {
            out.push(ObjectId::Cable(*id));
            out.extend(cable.members.iter().map(|m| ObjectId::Conductor(m.conductor)));
        }
        Op::AddCableMember { cable, member, .. } | Op::RemoveCableMember { cable, member, .. } => {
            out.push(ObjectId::Cable(*cable));
            out.push(ObjectId::Conductor(member.conductor));
        }
        Op::SetProperty { object, .. } => out.push(match object {
            ecad_model::ObjectRef::Component(c) => ObjectId::Component(*c),
            ecad_model::ObjectRef::Conductor(c) => ObjectId::Conductor(*c),
            ecad_model::ObjectRef::Cable(c) => ObjectId::Cable(*c),
            ecad_model::ObjectRef::Page(p) => ObjectId::Page(*p),
            ecad_model::ObjectRef::Project | ecad_model::ObjectRef::Standard => ObjectId::Settings,
        }),
        Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => out.push(ObjectId::Definition(*id)),
        Op::SetProjectSettings { .. } => out.push(ObjectId::Settings),
        Op::NameVersion { .. } | Op::UnnameVersion { .. } => {}
        // Folders (brief 019) are not among the debugger's objects yet.
        Op::AddFolder { .. } | Op::RemoveFolder { .. } | Op::ModifyFolder { .. } => {}
        // The standard and style records (brief 050a) are project-wide, like the settings.
        Op::SetStyle { .. } | Op::SetStandard { .. } => out.push(ObjectId::Settings),
    }
    let mut seen = BTreeSet::new();
    out.retain(|i| seen.insert(*i));
    out
}

/// Whether `tx` names any of `ids`.
pub fn touches(tx: &Transaction, ids: &BTreeSet<ObjectId>) -> bool {
    tx.ops.iter().any(|op| op_targets(op).iter().any(|i| ids.contains(i)))
}

/// What a selected object stands for in the model, and the IDs the trail follows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Resolved {
    pub item: Option<ItemId>,
    pub representation: Option<RepresentationId>,
    pub subject: Option<Subject>,
    /// A terminal marker's terminal.
    pub terminal: Option<TerminalId>,
    /// The subject's terminals (a component) or attached terminals (a conductor).
    pub terminals: Vec<TerminalId>,
    /// The objects whose history the trail follows: item, representation, subject, terminals.
    pub ids: Vec<ObjectId>,
}

/// Follow a mapped handle into the model.
pub fn resolve(doc: &Document, mapped: Mapped) -> Resolved {
    let mut r = Resolved::default();
    match mapped {
        Mapped::Item(i) => {
            r.item = Some(i);
            r.representation = doc.representations.iter().find(|(_, rep)| rep.items.contains(&i)).map(|(id, _)| *id);
        }
        Mapped::Marker(rep, t) => {
            r.representation = Some(rep);
            r.terminal = Some(t);
        }
        // A fan view line (brief 005) stands for its cable; it is no item.
        Mapped::Fan(_) => {}
        // A reference's derived text (brief 010) stands for its reference representation.
        Mapped::RefText(rep) => r.representation = Some(rep),
        // A device's tag text or terminal label (brief 018) stands for its symbol representation.
        Mapped::Tag(rep) => r.representation = Some(rep),
        // A conductor label (brief 022) stands for its conductor.
        Mapped::Label(_) => {}
        // A line or text of the sheet frame (brief 019) stands for nothing stored.
        Mapped::Frame(_) => {}
        // A device cross-reference (brief 025) stands for its contact or head representation.
        Mapped::Xref(rep) => r.representation = Some(rep),
        // A link line (brief 030) stands for its strip.
        Mapped::Link(..) => {}
    }
    r.subject = match mapped {
        Mapped::Fan(k) => Some(Subject::Cable(k)),
        Mapped::Label(c) => Some(Subject::Conductor(c)),
        Mapped::Link(c, _) => Some(Subject::Component(c)),
        _ => r.representation.and_then(|id| doc.representations.get(&id)).map(|rep| rep.subject),
    };
    r.terminals = match r.subject {
        Some(Subject::Component(c)) => doc.terminals.iter().filter(|(_, t)| t.component == c).map(|(id, _)| *id).collect(),
        Some(Subject::Conductor(c)) => doc.conductors.get(&c).map(|x| x.ends.iter().flatten().copied().collect()).unwrap_or_default(),
        _ => Vec::new(),
    };
    r.ids.extend(r.item.map(ObjectId::Item));
    r.ids.extend(r.representation.map(ObjectId::Representation));
    r.ids.extend(r.subject.map(ObjectId::from));
    r.ids.extend(r.terminal.map(ObjectId::Terminal));
    for t in &r.terminals {
        if r.terminal != Some(*t) {
            r.ids.push(ObjectId::Terminal(*t));
        }
    }
    r
}

/// An object as the model holds it (`None` when it does not exist in `doc`).
pub fn object_json(doc: &Document, id: ObjectId) -> Option<Value> {
    match id {
        ObjectId::Page(i) => doc.pages.get(&i).and_then(val),
        ObjectId::Layer(i) => doc.layers.get(&i).and_then(val),
        ObjectId::Item(i) => doc.items.get(&i).and_then(val),
        ObjectId::Component(i) => doc.components.get(&i).and_then(val),
        ObjectId::Terminal(i) => doc.terminals.get(&i).and_then(val),
        ObjectId::Representation(i) => doc.representations.get(&i).and_then(val),
        ObjectId::Conductor(i) => doc.conductors.get(&i).and_then(val),
        ObjectId::Cable(i) => doc.cables.get(&i).and_then(val),
        ObjectId::Definition(i) => doc.definitions.get(&i).and_then(val),
        ObjectId::Settings => val(&doc.settings),
    }
}

/// The objects that refer to `id`: a definition's components and symbol instances, a component's
/// terminals and representations, a terminal's attached conductors, a conductor's cables and
/// representations, a cable's members (in member order) and then its representations (design 004
/// §9: slash → cable → conductors). Representations of a conductor or a cable are listed by page,
/// in page order (design 010 §8: a cable drawn on several pages, a conductor's route on each), and
/// a cable reference's are its cable and its partner reference.
pub fn used_by(doc: &Document, id: ObjectId) -> Vec<ObjectId> {
    let page_index: BTreeMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    let reps_of = |s: Subject| {
        let mut v: Vec<(usize, RepresentationId)> = doc
            .representations
            .iter()
            .filter(|(_, r)| r.subject == s)
            .map(|(i, r)| (page_index.get(&r.page).copied().unwrap_or(usize::MAX), *i))
            .collect();
        v.sort();
        v.into_iter().map(|(_, i)| ObjectId::Representation(i))
    };
    match id {
        ObjectId::Definition(d) => doc
            .components
            .iter()
            .filter(|(_, c)| c.definition == Some(d))
            .map(|(i, _)| ObjectId::Component(*i))
            .chain(doc.items.iter().filter(|(_, it)| matches!(&it.kind, ItemKind::Symbol(s) if s.definition == d)).map(|(i, _)| ObjectId::Item(*i)))
            .collect(),
        ObjectId::Component(c) => doc
            .terminals
            .iter()
            .filter(|(_, t)| t.component == c)
            .map(|(i, _)| ObjectId::Terminal(*i))
            .chain(reps_of(Subject::Component(c)))
            .collect(),
        ObjectId::Terminal(t) => doc.conductors.iter().filter(|(_, c)| c.ends.contains(&Some(t))).map(|(i, _)| ObjectId::Conductor(*i)).collect(),
        ObjectId::Conductor(c) => doc
            .cables
            .iter()
            .filter(|(_, k)| k.members.iter().any(|m| m.conductor == c))
            .map(|(i, _)| ObjectId::Cable(*i))
            .chain(reps_of(Subject::Conductor(c)))
            .collect(),
        ObjectId::Cable(c) => doc
            .cables
            .get(&c)
            .into_iter()
            .flat_map(|k| k.members.iter().map(|m| ObjectId::Conductor(m.conductor)))
            .chain(reps_of(Subject::Cable(c)))
            .collect(),
        ObjectId::Page(p) => doc.representations.iter().filter(|(_, r)| r.page == p).map(|(i, _)| ObjectId::Representation(*i)).collect(),
        ObjectId::Representation(r) => match doc.representations.get(&r) {
            Some(rep) if crate::references::is_reference(rep) => {
                let mut out: Vec<ObjectId> = vec![ObjectId::from(rep.subject)];
                if let Some(x) = crate::references::all(doc).into_iter().find(|x| x.reference == r) {
                    out.extend(x.partners.into_iter().map(ObjectId::Representation));
                }
                out
            }
            _ => Vec::new(),
        },
        ObjectId::Layer(_) | ObjectId::Item(_) | ObjectId::Settings => Vec::new(),
    }
}

fn tag(p: &ecad_model::Properties) -> Option<String> {
    p.get("tag").cloned()
}

fn subject_label(doc: &Document, s: Subject) -> String {
    let t = match s {
        Subject::Component(c) => doc.components.get(&c).and_then(|x| tag(&x.properties)),
        Subject::Conductor(c) => doc.conductors.get(&c).and_then(|x| tag(&x.properties)),
        Subject::Cable(c) => doc.cables.get(&c).and_then(|x| tag(&x.properties)),
    };
    t.unwrap_or_else(|| format!("untagged {}", ObjectId::from(s).kind()))
}

/// A readable label for an object.
pub fn object_label(doc: &Document, id: ObjectId) -> String {
    match id {
        ObjectId::Page(i) => doc.pages.get(&i).map_or_else(String::new, |p| format!("page {}", p.title)),
        ObjectId::Layer(i) => doc.layers.get(&i).map_or_else(String::new, |l| format!("layer {}", l.name)),
        ObjectId::Item(i) => doc.items.get(&i).map_or_else(String::new, |it| match &it.kind {
            ItemKind::Primitive(p) => format!("{p:?}").split([' ', '{', '(']).next().unwrap_or("item").to_lowercase(),
            ItemKind::Symbol(_) => "symbol".into(),
        }),
        ObjectId::Component(c) => subject_label(doc, Subject::Component(c)),
        // A member of a cable shows its core (design 009 §8): `W3 · core 1`.
        ObjectId::Conductor(c) => {
            let core = doc.cables_of(c).first().and_then(|k| doc.cables.get(k)).and_then(|k| {
                k.members
                    .iter()
                    .find(|m| m.conductor == c)
                    .and_then(|m| m.core.as_deref().filter(|x| !x.trim().is_empty()).map(|x| crate::look::shorten(x, crate::look::MAX_CORE_CHARS)))
            });
            match core {
                Some(core) => format!("{} · core {core}", subject_label(doc, Subject::Conductor(c))),
                None => subject_label(doc, Subject::Conductor(c)),
            }
        }
        ObjectId::Cable(c) => subject_label(doc, Subject::Cable(c)),
        ObjectId::Terminal(i) => doc.terminals.get(&i).map_or_else(String::new, |t| {
            let name = t.label.clone().or_else(|| t.key.clone()).unwrap_or_else(|| i.short());
            format!("{}:{name}", subject_label(doc, Subject::Component(t.component)))
        }),
        ObjectId::Representation(i) => doc.representations.get(&i).map_or_else(String::new, |r| {
            // A cable's reference end shows the text it draws (design 009 §8).
            if r.role == crate::look::REFERENCE_ROLE && matches!(r.subject, Subject::Cable(_)) {
                format!("{} of {} \"{}\"", r.role, subject_label(doc, r.subject), crate::look::reference_text(doc, i))
            } else {
                format!("{} of {}", r.role, subject_label(doc, r.subject))
            }
        }),
        ObjectId::Definition(i) => doc.definitions.get(&i).map_or_else(String::new, |d| d.name.clone()),
        ObjectId::Settings => format!("length unit {:?}", doc.settings.length_unit),
    }
}

/// Every object of `doc`, in kind order.
pub fn objects(doc: &Document) -> Vec<ObjectId> {
    let mut out: Vec<ObjectId> = Vec::new();
    out.extend(doc.page_order.iter().map(|i| ObjectId::Page(*i)));
    out.extend(doc.layers.keys().map(|i| ObjectId::Layer(*i)));
    out.extend(doc.components.keys().map(|i| ObjectId::Component(*i)));
    out.extend(doc.terminals.keys().map(|i| ObjectId::Terminal(*i)));
    out.extend(doc.conductors.keys().map(|i| ObjectId::Conductor(*i)));
    out.extend(doc.cables.keys().map(|i| ObjectId::Cable(*i)));
    out.extend(doc.representations.keys().map(|i| ObjectId::Representation(*i)));
    out.extend(doc.items.keys().map(|i| ObjectId::Item(*i)));
    out.extend(doc.definitions.keys().map(|i| ObjectId::Definition(*i)));
    out.push(ObjectId::Settings);
    out
}

/// Objects of `doc` per kind, for a summary line.
pub fn counts(doc: &Document) -> BTreeMap<&'static str, usize> {
    let mut m = BTreeMap::new();
    for id in objects(doc) {
        *m.entry(id.kind()).or_insert(0) += 1;
    }
    m
}

/// Bytes for display: text as it is (records are UTF-8 JSON lines), with control characters
/// (the line break included) and invalid UTF-8 escaped.
pub fn printable(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.utf8_chunks() {
        for c in chunk.valid().chars() {
            if c.is_control() {
                out.extend(c.escape_default());
            } else {
                out.push(c);
            }
        }
        out.extend(chunk.invalid().iter().map(|b| format!("\\x{b:02x}")));
    }
    out
}
