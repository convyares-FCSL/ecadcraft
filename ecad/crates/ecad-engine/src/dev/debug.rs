//! Read-only debugger snapshots (persistence stage 2b).
//!
//! The developer debugger follows one trail: **selected drawing objects → ECAD IDs → model objects
//! → the transactions that touched them, with before/after values → whether each transaction is
//! saved in the file, and its stored record.** This module supplies the data; the panel that
//! shows it is in the desktop app app.
//!
//! # Semantics
//! - **Coherent and short.** [`crate::ProjectHandle::debug_snapshot`] copies what the panel shows out of
//!   the project under one `try_lock` and releases it before anything is drawn. A snapshot is
//!   labelled with the transaction it shows ([`DebugSnapshot::label`]). It is rebuilt only when
//!   its [`SnapshotKey`] changes (head, saved position, runtime events, active page, selection), so
//!   an idle panel costs one key comparison per frame. A busy project (`BUSY`) is not waited for:
//!   the panel keeps showing its previous snapshot, marked stale.
//! - **History is copied once.** Recorded transactions never change, so the panel keeps its own
//!   copy and a snapshot carries only the transactions after the copy's end ([`TxCursor`]).
//! - **Pending versus saved.** [`DebugSnapshot::record_state`] says, per transaction, whether it is
//!   only in memory (pending), written but not yet confirmed durable, or saved durably
//!   ([`RecordState`]).
//! - **Expensive checks only on request.** A snapshot never replays the log, reprojects a page or
//!   reads the file. [`super::checks::verify_replay`], [`super::checks::verify_projection`] (inputs from
//!   [`crate::ProjectHandle::debug_projection_inputs`]) and [`super::checks::verify_store`] do, when asked, on copies and
//!   outside the lock. [`super::checks::read_row`] reads one transaction's stored record, capped.
//! - **History never touches the live project.** [`super::checks::state_at`] rebuilds the document after
//!   transaction `n` from the panel's copy of the log.
//!
//! Nothing here changes a project: the only project access is through `try_lock` reads.

use std::sync::Arc;

use crate::history::event_log::Event;
use cadcraft_engine::doc::Handle;
use ecad_model::{
    CableId, ConnectionPointId, DefinitionId, DeviceId, Document, ItemId, LayerId, PageId, ProjectId, RepresentationId, RouteId, Subject,
    Transaction, TxId, WireId,
};
use serde_json::{Value, json};

use super::checks::{PageInputs, ProjectionInputs};
use crate::access::PagesRef;
use crate::project::{Mapped, Persistence, Project};

/// Selected handles described per snapshot (the rest are counted).
pub(crate) const MAX_SELECTION: usize = 500;
/// Runtime events carried by a snapshot (the most recent).
pub(crate) const MAX_EVENTS: usize = 500;
/// Record bytes shown by [`super::checks::read_row`]; longer records are checked in full but shown cut.
pub(crate) const MAX_RECORD_SHOWN: usize = 16 * 1024;

/// Any ECAD object, for following IDs across the model and the history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObjectId {
    Page(PageId),
    Layer(LayerId),
    Item(ItemId),
    Device(DeviceId),
    ConnectionPoint(ConnectionPointId),
    Representation(RepresentationId),
    Wire(WireId),
    Cable(CableId),
    /// A route.
    Route(RouteId),
    /// A drawing aid.
    Aid(ecad_model::AidId),
    /// A loose part.
    LoosePart(ecad_model::LoosePartId),
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
            ObjectId::Device(_) => "component",
            ObjectId::ConnectionPoint(_) => "connection_point",
            ObjectId::Representation(_) => "representation",
            ObjectId::Wire(_) => "conductor",
            ObjectId::Cable(_) => "cable",
            ObjectId::Route(_) => "route",
            ObjectId::Aid(_) => "aid",
            ObjectId::LoosePart(_) => "loose part",
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
            ObjectId::Device(i) => i.short(),
            ObjectId::ConnectionPoint(i) => i.short(),
            ObjectId::Representation(i) => i.short(),
            ObjectId::Wire(i) => i.short(),
            ObjectId::Cable(i) => i.short(),
            ObjectId::Route(i) => i.short(),
            ObjectId::Aid(i) => i.short(),
            ObjectId::LoosePart(i) => i.short(),
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
            ObjectId::Device(i) => i.hex(),
            ObjectId::ConnectionPoint(i) => i.hex(),
            ObjectId::Representation(i) => i.hex(),
            ObjectId::Wire(i) => i.hex(),
            ObjectId::Cable(i) => i.hex(),
            ObjectId::Route(i) => i.hex(),
            ObjectId::Aid(i) => i.hex(),
            ObjectId::LoosePart(i) => i.hex(),
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
            Subject::Device(c) => ObjectId::Device(c),
            Subject::Wire(c) => ObjectId::Wire(c),
            Subject::Cable(c) => ObjectId::Cable(c),
            Subject::Route(c) => ObjectId::Route(c),
            Subject::Aid(c) => ObjectId::Aid(c),
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
    /// The file's head: the last transaction written to it (equals the ledger's saved position
    /// once acknowledged).
    pub writer_persisted: Option<u64>,
    pub read_only: Option<String>,
    /// The last Save As replaced its target without confirming durability.
    pub unconfirmed: bool,
    /// What opening found, when the project was opened from a file.
    pub open_report: Option<String>,
    /// The file's facts (the debugger's Storage view).
    pub storage: Option<ecad_file::binding::StorageFacts>,
}

impl FileFacts {
    /// The facts as JSON, for exported inspector snapshots (`ecad-inspect/2`).
    pub fn to_json(&self) -> Value {
        json!({
            "persistence": self.persistence.map(|p| format!("{p:?}")),
            "path": self.path,
            "lock_file": self.lock_file,
            "in_memory": self.in_memory,
            "writer_persisted": self.writer_persisted,
            "read_only": self.read_only,
            "unconfirmed": self.unconfirmed,
            "open_report": self.open_report,
            "storage": self.storage.as_ref().map(|s| json!({
                "branch": s.branch, "head": s.head, "snapshot_seq": s.snapshot_seq,
                "snapshot_every": s.snapshot_every, "writable": s.writable,
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
    /// Saved: acknowledged durable, in the file.
    Saved,
    /// In the file, written by a Save As whose durability could not be confirmed; Save confirms
    /// it.
    Unconfirmed,
    /// Only in memory.
    Pending,
    /// The project has no persistence: nothing is ever saved.
    NotSaved,
}

impl RecordState {
    /// A short label for listings.
    pub fn label(self) -> &'static str {
        match self {
            RecordState::Saved => "saved",
            RecordState::Unconfirmed => "written, unconfirmed",
            RecordState::Pending => "pending",
            RecordState::NotSaved => "not saved",
        }
    }

    /// Whether the transaction is durably saved.
    pub fn durable(self) -> bool {
        matches!(self, RecordState::Saved)
    }

    /// Whether the transaction's record is in the file.
    pub fn in_file(self) -> bool {
        matches!(self, RecordState::Saved | RecordState::Unconfirmed)
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
    /// The project file's database (on disk, or the test store's in memory), for on-request reads
    /// (records, Verify file, the Storage and Branches views). Reads only; weak, so a copy never keeps a closed project's
    /// file open.
    pub store: Option<std::sync::Weak<ecad_file::Store>>,
}

impl DebugSnapshot {
    /// The bound file's database, while the project still has it open.
    pub fn store_arc(&self) -> Option<Arc<ecad_file::Store>> {
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
        let written = self.file.writer_persisted.unwrap_or(0).clamp(saved, self.head);
        StateCounts { saved, unconfirmed: written - saved, pending: self.head - written }
    }
}

/// Where transaction `seq` stands, given the file facts and the durable position.
pub fn record_state(file: &FileFacts, persisted: u64, seq: u64) -> RecordState {
    if file.persistence == Some(Persistence::Unavailable) {
        return RecordState::NotSaved;
    }
    if seq <= persisted {
        RecordState::Saved
    } else if file.writer_persisted.is_some_and(|w| seq <= w) {
        RecordState::Unconfirmed
    } else {
        RecordState::Pending
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
            persistence: Some(self.storage.persistence),
            open_report: self.storage.open_report.as_ref().map(ecad_file::Report::describe),
            ..FileFacts::default()
        };
        match self.storage.persistence {
            Persistence::TestStore => {
                f.in_memory = true;
                f.storage = self.storage.store.project().map(ecad_file::binding::StorageFacts::of);
                f.read_only = self.storage.store.project().and_then(|p| p.read_only()).map(str::to_string);
            }
            Persistence::File => {
                let st = self.storage.file.status();
                f.path = st.path;
                f.read_only = st.read_only;
                f.unconfirmed = st.unconfirmed;
                f.lock_file = self.storage.file.lock_file();
                f.storage = self.storage.file.storage();
            }
            Persistence::Unavailable => {}
        }
        f.writer_persisted = f.storage.as_ref().map(|s| s.head);
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
            .page_order()
            .iter()
            .map(|p| {
                let view = self.pages.get(p);
                let open = view.and_then(|v| v.uid).filter(|u| pg.doc(*u).is_some());
                PageRow {
                    id: *p,
                    title: doc.pages().get(p).map(|x| x.title.to_string()).unwrap_or_default(),
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
            store: match self.storage.persistence {
                Persistence::TestStore => self.storage.store.project().map(|p| Arc::downgrade(p.store())),
                _ => self.storage.file.store().as_ref().map(Arc::downgrade),
            },
        }
    }

    /// Copies for [`super::checks::verify_projection`].
    pub(crate) fn projection_inputs(&self, pg: &PagesRef<'_>) -> ProjectionInputs {
        let pages = self
            .pages
            .iter()
            .filter_map(|(id, v)| {
                let uid = v.uid?;
                let drawing = pg.doc(uid)?.doc.clone();
                Some(PageInputs { id: *id, drawing, map: v.map.clone(), sheet: self.sheet_index(*id) })
            })
            .collect();
        ProjectionInputs {
            head: self.ledger.head(),
            doc: self.ledger.document().clone(),
            pages,
            views: self.display.views.clone(),
            tolerance: self.tolerance,
            hidden_text: self.display.show_hidden_text,
            pending: self.frame.pending,
            colours: self.display.sheet,
        }
    }
}
