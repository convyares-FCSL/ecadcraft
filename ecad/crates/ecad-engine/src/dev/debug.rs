//! Read-only debugger snapshots (persistence stage 2b, proposal §21).
//!
//! The developer debugger follows one trail: **selected drawing objects → ECAD IDs → model objects
//! → the transactions that touched them, with before/after values → whether each transaction is
//! saved in the file, and its stored record.** This module supplies the data; the panel that
//! shows it is in the desktop app app.
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
//!   only in memory (pending), written but not yet confirmed durable, or saved durably
//!   ([`RecordState`]).
//! - **Expensive checks only on request.** A snapshot never replays the log, reprojects a page or
//!   reads the file. [`verify_replay`], [`verify_projection`] (inputs from
//!   [`ProjectHandle::debug_projection_inputs`]) and [`verify_store`] do, when asked, on copies and
//!   outside the lock. [`read_row`] reads one transaction's stored record, capped.
//! - **History never touches the live project.** [`state_at`] rebuilds the document after
//!   transaction `n` from the panel's copy of the log.
//!
//! Nothing here changes a project: the only project access is through `try_lock` reads.

use ecad_model::naming::keys::PropsExt;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use crate::history::event_log::Event;
use cadcraft_engine::doc::{Drawing, Handle};
use ecad_model::{
    CableId, ConnectionPointId, DefinitionId, DeviceId, Document, ItemId, ItemKind, LayerId, Ledger, Op, PageId, ProjectId, RepresentationId,
    RouteId, Subject, Transaction, TxId, WireId,
};
use serde_json::{Value, json};

use crate::access::PagesRef;
use crate::project::{HandleMap, Mapped, Persistence, Project};

/// Selected handles described per snapshot (the rest are counted).
pub const MAX_SELECTION: usize = 500;
/// Runtime events carried by a snapshot (the most recent).
pub const MAX_EVENTS: usize = 500;
/// Record bytes shown by [`read_row`]; longer records are checked in full but shown cut.
pub const MAX_RECORD_SHOWN: usize = 16 * 1024;

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
    /// A route (design 072 F31).
    Route(RouteId),
    /// A drawing aid (design 072 §6.2).
    Aid(ecad_model::AidId),
    /// A loose part (design 072 §6.4).
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
            persistence: Some(self.persistence),
            open_report: self.open_report.as_ref().map(ecad_file::Report::describe),
            ..FileFacts::default()
        };
        match self.persistence {
            Persistence::TestStore => {
                f.in_memory = true;
                f.storage = self.store.project().map(crate::file::StorageFacts::of);
                f.read_only = self.store.project().and_then(|p| p.read_only()).map(str::to_string);
            }
            Persistence::File => {
                let st = self.file.status();
                f.path = st.path;
                f.read_only = st.read_only;
                f.unconfirmed = st.unconfirmed;
                f.lock_file = self.file.lock_file();
                f.storage = self.file.storage();
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
            .page_order
            .iter()
            .map(|p| {
                let view = self.pages.get(p);
                let open = view.and_then(|v| v.uid).filter(|u| pg.doc(*u).is_some());
                PageRow {
                    id: *p,
                    title: doc.pages.get(p).map(|x| x.title.to_string()).unwrap_or_default(),
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
            store: match self.persistence {
                Persistence::TestStore => self.store.project().map(|p| Arc::downgrade(p.store())),
                _ => self.file.store().as_ref().map(Arc::downgrade),
            },
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
pub fn verify_index(doc: &Document, kept: &ecad_electrical::references::devices::RefIndex, head: u64) -> Check {
    let (members, groups, edges) = kept.sizes();
    match kept.verify(doc) {
        Ok(()) => Check {
            what: "reference index",
            at_seq: head,
            ok: true,
            lines: vec![format!("{members} members in {groups} groups, {edges} edges: the same as a rebuild.")],
        },
        Err(e) => Check { what: "reference index", at_seq: head, ok: false, lines: vec![e.to_string()] },
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
pub fn verify_store(store: &ecad_file::Store, base: u64, log: &[Transaction], persisted: u64) -> Check {
    let at_seq = base + log.len() as u64;
    let v = ecad_file::verify::verify(store);
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
                let same = row.and_then(|(flag, _, body)| ecad_file::pack::unpack(flag, &body).ok()).is_some_and(|rec| {
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

/// Read transaction `seq`'s row from the project file (no project lock) and check that it decodes
/// to exactly `expected`. `shown` is the record, decompressed; `stored` is its size in the file.
///
/// # Errors
/// The file cannot be read, or holds no such row.
pub fn read_row(store: &ecad_file::Store, seq: u64, expected: Option<&Transaction>) -> Result<RecordBytes, String> {
    let r = store.reader().map_err(|e| e.to_string())?;
    let branch = r.branches().map_err(|e| e.to_string())?.into_iter().find(|(_, b)| b.name == "main").map(|(id, _)| id).ok_or("no main branch")?;
    let (flag, chain, body) = r.tx(branch, seq).map_err(|e| e.to_string())?.ok_or(format!("no row for {seq}"))?;
    let record = ecad_file::pack::unpack(flag, &body).map_err(|e| e.to_string())?;
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
    let stored = (body.len() + 33) as u64;
    let mut shown = record;
    let truncated = shown.len() > MAX_RECORD_SHOWN;
    shown.truncate(MAX_RECORD_SHOWN);
    Ok(RecordBytes { seq, stored, shown, truncated, check })
}

/// What the Storage and Branches views read from a version 2 file, on request: table sizes, the
/// file's size, branches and version rows (without their chunk maps).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StorageRead {
    pub file_bytes: u64,
    pub tables: Vec<ecad_file::TableStat>,
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
pub fn read_branches(store: &ecad_file::Store) -> Result<Vec<BranchRow>, String> {
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
pub fn read_storage(store: &ecad_file::Store, path: Option<&Path>) -> Result<StorageRead, String> {
    let r = store.reader().map_err(|e| e.to_string())?;
    let tables = r.stats().map_err(|e| e.to_string())?;
    let mut versions = Vec::new();
    for (id, b) in r.branches().map_err(|e| e.to_string())? {
        for kind in [ecad_file::schema::VersionKind::User, ecad_file::schema::VersionKind::Revision, ecad_file::schema::VersionKind::Auto] {
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
    // The report models of this copy, worked out once for every page.
    let reports = crate::project::engine_views().report_models(&inputs.doc);
    for p in &inputs.pages {
        let title = inputs.doc.pages.get(&p.id).map_or_else(|| p.id.short(), |x| x.title.to_string());
        let same = crate::project::project_page_pending(
            &inputs.doc,
            p.id,
            &p.drawing,
            &p.map,
            &inputs.views,
            inputs.tolerance,
            inputs.hidden_text,
            inputs.pending,
            reports.as_ref(),
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
        let v = crate::dev::verify::verify_page_pending(
            &inputs.doc,
            p.id,
            &p.drawing,
            &p.map,
            &inputs.views,
            inputs.tolerance,
            inputs.hidden_text,
            inputs.pending,
            reports.as_ref(),
        );
        ok &= v.ok();
        lines.push(format!("page {title}: {}", v.summary()));
    }
    Check { what: "projection", at_seq: inputs.head, ok, lines }
}

/// One transaction's stored record, read from the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordBytes {
    pub seq: u64,
    /// Bytes the row takes in the file (compressed, with its flag and chain value).
    pub stored: u64,
    /// The first [`MAX_RECORD_SHOWN`] bytes of the record.
    pub shown: Vec<u8>,
    pub truncated: bool,
    /// Whether the record matches its chain value and decodes to exactly the expected transaction.
    pub check: Result<(), String>,
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
        Op::ModifyPage { before, after, .. } => (val(before), val(after)),
        Op::AddLayer { layer, .. } => (None, val(layer)),
        Op::RemoveLayer { layer, .. } => (val(layer), None),
        Op::AddItem { item, .. } => (None, val(item)),
        Op::ModifyItem { before, after, .. } => (val(before), val(after)),
        Op::RemoveItem { item, .. } => (val(item), None),
        Op::CreateDevice { device, .. } => (None, val(device)),
        Op::DeleteDevice { device, .. } => (val(device), None),
        Op::ModifyDevice { before, after, .. } => (val(before), val(after)),
        Op::SetDeviceDefinition { before, after, .. } => (val(before), val(after)),
        Op::AddConnectionPoint { connection_point, .. } => (None, val(connection_point)),
        Op::RemoveConnectionPoint { connection_point, .. } => (val(connection_point), None),
        Op::LinkRepresentation { rep, .. } => (None, val(rep)),
        Op::ModifyRepresentation { before, after, .. } => (val(before), val(after)),
        Op::UnlinkRepresentation { rep, .. } => (val(rep), None),
        Op::CreateWire { wire, .. } => (None, val(wire)),
        Op::DeleteWire { wire, .. } => (val(wire), None),
        Op::ModifyWire { before, after, .. } => (val(before), val(after)),
        Op::AddRoute { route, .. } => (None, val(route)),
        Op::RemoveRoute { route, .. } => (val(route), None),
        Op::ModifyRoute { before, after, .. } => (val(before), val(after)),
        Op::CreateAid { aid, .. } => (None, val(aid)),
        Op::DeleteAid { aid, .. } => (val(aid), None),
        Op::ModifyAid { before, after, .. } => (val(before), val(after)),
        Op::CreateLoosePart { part, .. } => (None, val(part)),
        Op::DeleteLoosePart { part, .. } => (val(part), None),
        Op::ModifyLoosePart { before, after, .. } => (val(before), val(after)),
        Op::CreateCable { cable, .. } => (None, val(cable)),
        Op::DeleteCable { cable, .. } => (val(cable), None),
        Op::ModifyCable { before, after, .. } => (val(before), val(after)),
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
        Op::ModifyProject { before, after } => (val(before), val(after)),
        Op::AddDeviceLink { link, .. } => (None, val(link)),
        Op::RemoveDeviceLink { link, .. } => (val(link), None),
    };
    OpView { name: op.name(), targets: op_targets(op), before, after }
}

fn item_references(item: &ecad_model::Item, out: &mut Vec<ObjectId>) {
    out.push(ObjectId::Page(item.page));
    out.push(ObjectId::Layer(item.style.layer));
    if let ItemKind::Symbol(s) = &item.kind {
        out.push(ObjectId::Definition(s.definition));
    }
}

fn rep_references(rep: &ecad_model::Representation, out: &mut Vec<ObjectId>) {
    out.push(rep.subject.into());
    out.push(ObjectId::Page(rep.page));
    out.extend(rep.items.iter().map(|i| ObjectId::Item(*i)));
    out.extend(rep.connection_points.keys().map(|t| ObjectId::ConnectionPoint(*t)));
}

/// Every object an op names: its target first.
pub fn op_targets(op: &Op) -> Vec<ObjectId> {
    let mut out = Vec::new();
    match op {
        Op::AddPage { id, page, .. } | Op::RemovePage { id, page, .. } => {
            out.push(ObjectId::Page(*id));
            out.extend(page.kind.strip().map(ObjectId::Device));
        }
        Op::ModifyPage { id, before, .. } => {
            out.push(ObjectId::Page(*id));
            out.extend(before.kind.strip().map(ObjectId::Device));
        }
        Op::AddLayer { id, .. } | Op::RemoveLayer { id, .. } => out.push(ObjectId::Layer(*id)),
        Op::AddItem { id, item } | Op::RemoveItem { id, item } => {
            out.push(ObjectId::Item(*id));
            item_references(item, &mut out);
        }
        Op::ModifyItem { id, before, after } => {
            out.push(ObjectId::Item(*id));
            item_references(before, &mut out);
            item_references(after, &mut out);
        }
        Op::CreateDevice { id, device } | Op::DeleteDevice { id, device } => {
            out.push(ObjectId::Device(*id));
            out.extend(device.definition.map(ObjectId::Definition));
        }
        Op::ModifyDevice { id, before, after } => {
            out.push(ObjectId::Device(*id));
            out.extend(before.definition.iter().chain(&after.definition).copied().map(ObjectId::Definition));
        }
        Op::SetDeviceDefinition { id, before, after } => {
            out.push(ObjectId::Device(*id));
            out.extend(before.iter().chain(after).copied().map(ObjectId::Definition));
        }
        Op::AddConnectionPoint { id, connection_point } | Op::RemoveConnectionPoint { id, connection_point } => {
            out.push(ObjectId::ConnectionPoint(*id));
            out.push(match connection_point.owner {
                ecad_model::ConnectionPointOwner::Device(c) => ObjectId::Device(c),
                ecad_model::ConnectionPointOwner::Aid(a) => ObjectId::Aid(a),
            });
        }
        Op::LinkRepresentation { id, rep } | Op::UnlinkRepresentation { id, rep } => {
            out.push(ObjectId::Representation(*id));
            rep_references(rep, &mut out);
        }
        Op::ModifyRepresentation { id, before, after } => {
            out.push(ObjectId::Representation(*id));
            rep_references(before, &mut out);
            rep_references(after, &mut out);
        }
        Op::CreateWire { id, wire } | Op::DeleteWire { id, wire } => {
            out.push(ObjectId::Wire(*id));
            out.extend(wire.ends.iter().map(|t| ObjectId::ConnectionPoint(*t)));
        }
        Op::ModifyWire { id, .. } => out.push(ObjectId::Wire(*id)),
        Op::AddRoute { id, route } | Op::RemoveRoute { id, route } => {
            out.push(ObjectId::Route(*id));
            out.push(ObjectId::Page(route.page));
            out.extend(route.points_landed().map(ObjectId::ConnectionPoint));
        }
        Op::ModifyRoute { id, before, after } => {
            out.push(ObjectId::Route(*id));
            out.extend(before.points_landed().chain(after.points_landed()).map(ObjectId::ConnectionPoint));
        }
        Op::CreateAid { id, .. } | Op::DeleteAid { id, .. } | Op::ModifyAid { id, .. } => out.push(ObjectId::Aid(*id)),
        Op::CreateLoosePart { id, .. } | Op::DeleteLoosePart { id, .. } | Op::ModifyLoosePart { id, .. } => out.push(ObjectId::LoosePart(*id)),
        Op::CreateCable { id, cable } | Op::DeleteCable { id, cable } => {
            out.push(ObjectId::Cable(*id));
            out.extend(cable.cores.iter().filter_map(member_object));
        }
        Op::ModifyCable { id, before, after } => {
            out.push(ObjectId::Cable(*id));
            out.extend(before.cores.iter().chain(&after.cores).filter_map(member_object));
        }
        Op::SetProperty { object, .. } => out.push(match object {
            ecad_model::ObjectRef::Device(c) => ObjectId::Device(*c),
            ecad_model::ObjectRef::Wire(c) => ObjectId::Wire(*c),
            ecad_model::ObjectRef::Cable(c) => ObjectId::Cable(*c),
            ecad_model::ObjectRef::Page(p) => ObjectId::Page(*p),
            ecad_model::ObjectRef::Project | ecad_model::ObjectRef::Standard => ObjectId::Settings,
        }),
        Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => out.push(ObjectId::Definition(*id)),
        Op::SetProjectSettings { .. } | Op::ModifyProject { .. } => out.push(ObjectId::Settings),
        // A device link (design 072 §2.6) shows in the history of both devices it joins.
        Op::AddDeviceLink { link, .. } | Op::RemoveDeviceLink { link, .. } => {
            out.push(ObjectId::Device(link.a));
            out.push(ObjectId::Device(link.b));
        }
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

/// The object a cable member stands on: its wire, or its loose line while it has no wire.
fn member_object(m: &ecad_model::Core) -> Option<ObjectId> {
    m.wire().map(ObjectId::Wire).or_else(|| m.route().map(ObjectId::Route))
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
    pub connection_point: Option<ConnectionPointId>,
    /// The subject's terminals (a component) or attached terminals (a conductor).
    pub connection_points: Vec<ConnectionPointId>,
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
            r.connection_point = Some(t);
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
        Mapped::DeviceReference(rep) => r.representation = Some(rep),
        // A link line (brief 030) stands for its strip.
        Mapped::JumperBar(..) => {}
        // A T's dot (lane 072 D) stands for the route that ends there.
        Mapped::Tee(_) => {}
    }
    r.subject = match mapped {
        Mapped::Fan(k) => Some(Subject::Cable(k)),
        Mapped::Label(c) => Some(Subject::Wire(c)),
        Mapped::JumperBar(c, _) => Some(Subject::Device(c)),
        Mapped::Tee(r) => Some(Subject::Route(r)),
        _ => r.representation.and_then(|id| doc.representations.get(&id)).map(|rep| rep.subject),
    };
    r.connection_points = match r.subject {
        Some(Subject::Device(c)) => doc.connection_points.iter().filter(|(_, t)| t.device() == Some(c)).map(|(id, _)| *id).collect(),
        Some(Subject::Wire(c)) => doc.wires.get(&c).map(|x| x.ends.to_vec()).unwrap_or_default(),
        Some(Subject::Route(x)) => doc.routes.get(&x).map(|r| r.points_landed().collect()).unwrap_or_default(),
        _ => Vec::new(),
    };
    r.ids.extend(r.item.map(ObjectId::Item));
    r.ids.extend(r.representation.map(ObjectId::Representation));
    r.ids.extend(r.subject.map(ObjectId::from));
    // A route's line: the wire it carries follows it (design 072 §4: routes draw, wires connect).
    if let Some(Subject::Route(x)) = r.subject {
        r.ids.extend(ecad_electrical::wires::wire_of_route(doc, x).map(ObjectId::Wire));
    }
    r.ids.extend(r.connection_point.map(ObjectId::ConnectionPoint));
    for t in &r.connection_points {
        if r.connection_point != Some(*t) {
            r.ids.push(ObjectId::ConnectionPoint(*t));
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
        ObjectId::Device(i) => doc.devices.get(&i).and_then(val),
        ObjectId::ConnectionPoint(i) => doc.connection_points.get(&i).and_then(val),
        ObjectId::Representation(i) => doc.representations.get(&i).and_then(val),
        ObjectId::Wire(i) => doc.wires.get(&i).and_then(val),
        ObjectId::Cable(i) => doc.cables.get(&i).and_then(val),
        ObjectId::Route(i) => doc.routes.get(&i).and_then(val),
        ObjectId::Aid(i) => doc.aids.get(&i).and_then(val),
        ObjectId::LoosePart(i) => doc.loose_parts.get(&i).and_then(val),
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
            .devices
            .iter()
            .filter(|(_, c)| c.definition == Some(d))
            .map(|(i, _)| ObjectId::Device(*i))
            .chain(doc.items.iter().filter(|(_, it)| matches!(&it.kind, ItemKind::Symbol(s) if s.definition == d)).map(|(i, _)| ObjectId::Item(*i)))
            .collect(),
        ObjectId::Device(c) => doc
            .connection_points
            .iter()
            .filter(|(_, t)| t.device() == Some(c))
            .map(|(i, _)| ObjectId::ConnectionPoint(*i))
            .chain(reps_of(Subject::Device(c)))
            .collect(),
        ObjectId::ConnectionPoint(t) => doc
            .wires
            .iter()
            .filter(|(_, c)| c.ends.contains(&t))
            .map(|(i, _)| ObjectId::Wire(*i))
            .chain(doc.routes.iter().filter(|(_, r)| r.points_landed().any(|p| p == t)).map(|(i, _)| ObjectId::Route(*i)))
            .collect(),
        ObjectId::Wire(c) => doc
            .cables
            .iter()
            .filter(|(_, k)| k.cores.iter().any(|m| m.wire() == Some(c)))
            .map(|(i, _)| ObjectId::Cable(*i))
            .chain(reps_of(Subject::Wire(c)))
            .chain(ecad_electrical::wires::routes_of_wire(doc, c).into_iter().map(ObjectId::Route))
            .collect(),
        ObjectId::Cable(c) => {
            doc.cables.get(&c).into_iter().flat_map(|k| k.cores.iter().filter_map(member_object)).chain(reps_of(Subject::Cable(c))).collect()
        }
        ObjectId::Aid(a) => reps_of(Subject::Aid(a)).collect(),
        ObjectId::Route(r) => doc
            .routes
            .iter()
            .filter(|(_, x)| x.ends.iter().any(|e| e.route() == Some(r)))
            .map(|(i, _)| ObjectId::Route(*i))
            .chain(reps_of(Subject::Route(r)))
            .chain(ecad_electrical::wires::wire_of_route(doc, r).map(ObjectId::Wire))
            .collect(),
        ObjectId::Page(p) => doc
            .representations
            .iter()
            .filter(|(_, r)| r.page == p)
            .map(|(i, _)| ObjectId::Representation(*i))
            .chain(doc.routes.iter().filter(|(_, r)| r.page == p).map(|(i, _)| ObjectId::Route(*i)))
            .collect(),
        ObjectId::Representation(r) => match doc.representations.get(&r) {
            Some(rep) if ecad_electrical::references::cables::is_reference(rep) => {
                let mut out: Vec<ObjectId> = vec![ObjectId::from(rep.subject)];
                if let Some(x) = ecad_electrical::references::cables::all(doc).into_iter().find(|x| x.reference == r) {
                    out.extend(x.partners.into_iter().map(ObjectId::Representation));
                }
                out
            }
            _ => Vec::new(),
        },
        ObjectId::Layer(_) | ObjectId::Item(_) | ObjectId::LoosePart(_) | ObjectId::Settings => Vec::new(),
    }
}

fn tag(p: &ecad_model::Properties) -> Option<String> {
    p.text(ecad_model::naming::keys::TAG).map(str::to_string)
}

fn subject_label(doc: &Document, s: Subject) -> String {
    let t = match s {
        Subject::Device(c) => doc.devices.get(&c).and_then(|x| x.tag().map(str::to_string)),
        Subject::Wire(c) => doc.wires.get(&c).and_then(|x| x.number_text()).map(str::to_string),
        Subject::Cable(c) => doc.cables.get(&c).and_then(|x| tag(&x.properties)),
        Subject::Route(r) => doc.routes.get(&r).map(|x| format!("line on {}", x.page.short())),
        Subject::Aid(a) => doc.aids.get(&a).map(|x| match x {
            ecad_model::Aid::Continuation { name: Some(n), .. } => format!("continuation {n}"),
            ecad_model::Aid::Continuation { name: None, .. } => "unnamed continuation".into(),
            ecad_model::Aid::Box(_) => "location box".into(),
        }),
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
        ObjectId::Device(c) => subject_label(doc, Subject::Device(c)),
        // A member of a cable shows its core (design 009 §8): `W3 · core 1`.
        ObjectId::Wire(c) => {
            let core = doc.cables_of(c).first().and_then(|k| doc.cables.get(k)).and_then(|k| {
                k.cores.iter().find(|m| m.wire() == Some(c)).and_then(|m| {
                    m.label_str()
                        .filter(|x| !x.trim().is_empty())
                        .map(|x| ecad_electrical::cables::words::shorten(x, ecad_electrical::cables::words::MAX_CORE_CHARS))
                })
            });
            match core {
                Some(core) => format!("{} · core {core}", subject_label(doc, Subject::Wire(c))),
                None => subject_label(doc, Subject::Wire(c)),
            }
        }
        ObjectId::Cable(c) => subject_label(doc, Subject::Cable(c)),
        ObjectId::Route(r) => subject_label(doc, Subject::Route(r)),
        ObjectId::Aid(a) => subject_label(doc, Subject::Aid(a)),
        ObjectId::LoosePart(i) => doc.loose_parts.get(&i).map_or_else(String::new, |p| format!("loose part {}", p.description)),
        ObjectId::ConnectionPoint(i) => doc.connection_points.get(&i).map_or_else(String::new, |t| {
            let name = t.label.clone().or_else(|| t.key.as_ref().map(ToString::to_string)).unwrap_or_else(|| i.short());
            format!("{}:{name}", subject_label(doc, t.owner.subject()))
        }),
        ObjectId::Representation(i) => doc.representations.get(&i).map_or_else(String::new, |r| {
            // A cable's reference end shows the text it draws (design 009 §8).
            if r.role == ecad_model::Role::Reference && matches!(r.subject, Subject::Cable(_)) {
                format!("{} of {} \"{}\"", r.role, subject_label(doc, r.subject), ecad_electrical::cables::layout::reference_text(doc, i))
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
    out.extend(doc.devices.keys().map(|i| ObjectId::Device(*i)));
    out.extend(doc.connection_points.keys().map(|i| ObjectId::ConnectionPoint(*i)));
    out.extend(doc.wires.keys().map(|i| ObjectId::Wire(*i)));
    out.extend(doc.cables.keys().map(|i| ObjectId::Cable(*i)));
    out.extend(doc.routes.keys().map(|i| ObjectId::Route(*i)));
    out.extend(doc.aids.keys().map(|i| ObjectId::Aid(*i)));
    out.extend(doc.loose_parts.keys().map(|i| ObjectId::LoosePart(*i)));
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

/// Bytes for display: text as it is (records are UTF-8 JSON), with control characters
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
