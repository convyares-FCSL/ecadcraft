//! A project in a version 2 file: opening from the latest snapshot and its tail, and saving
//! (EEP-0005 §5–§6).
//!
//! - **Open** reads `meta`, the branch row and its head transaction, the latest snapshot's chunks
//!   (phase 1: the whole active snapshot, overseer 2026-11-08) and the transactions after it,
//!   each checked against its chain value and replayed with every check. It never reads other
//!   branches, older versions, transactions before the snapshot, or assets. Older transactions are
//!   read on demand through [`StoreHistory`] (undo past the snapshot, `state_at`).
//! - **Save** writes the pending transactions, the snapshots of any versions they name and of an
//!   automatic snapshot every `cache.every` transactions, the branch head and `meta` in one redb
//!   write transaction. Chunks are built incrementally from the last snapshot ([`chunks::Dirty`]).
//!   Nothing stored is read back to check it; its cost follows what is written.

use std::collections::BTreeMap;
use std::sync::Arc;

use ecad_model::{Base, Document, History, Ledger, ModelError, ObjectRef, Op, ProjectId, Transaction, TxId, TxKind};
use serde_json::{Value, json};

use super::chunks::{self, ChunkSet, Dirty, ModelMeta};
use super::pack;
use super::schema::{
    BranchId, BranchRecord, BranchStatus, CODEC, CONTAINER_VERSION, Cache, FORMAT_PROJECT, LinkV1, MAX_NAME,
    META_NAMES, Point, Release, SNAPSHOT_EVERY_RANGE, STACK_CAP, SnapshotRef, VersionKind, VersionRecord, auto_name,
};
use super::store::{Reader, Store, StoreError};
use crate::chain::{self, ChainKind, Hash};
use crate::record::{self, SCHEMA_VERSION};
use ecad_model::tag::HighWater;

/// Why a version 2 file could not be opened at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpenErrorV2 {
    #[error("{0}")]
    Store(#[from] StoreError),
    #[error("{0}")]
    Unsupported(String),
    /// The container facts or the branch are unusable.
    #[error("the file is damaged: {0}")]
    Damaged(String),
}

/// Why a save did not complete. The file then holds exactly what it held before, and the
/// project is still unsaved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SaveErrorV2 {
    #[error("the file was opened read-only: {0}")]
    ReadOnly(String),
    #[error("the file was changed by something else ({0}); it was not modified; use Save As")]
    ExternalChange(String),
    #[error("{0}")]
    Store(#[from] StoreError),
    #[error("could not encode: {0}")]
    Encode(String),
}

/// What a successful save wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SavedV2 {
    pub first_seq: u64,
    pub last_seq: u64,
    /// Bytes of new rows and chunks (before redb's own overhead).
    pub bytes: u64,
    pub chunks_written: usize,
    /// Snapshots written (versions and automatic).
    pub snapshots: usize,
}

/// What a check found wrong while opening (EEP-0005 §5.8): the project opens read-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DamageV2 {
    /// The transaction (or snapshot point) that failed, when known.
    pub seq: Option<u64>,
    pub detail: String,
}

/// What opening found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportV2 {
    /// The branch's recorded head.
    pub head: u64,
    /// The last transaction that verified (equal to `head` unless damaged).
    pub verified_seq: u64,
    /// The snapshot the project was opened from (0: the project root).
    pub snapshot_seq: u64,
    /// Transactions replayed after the snapshot.
    pub tail: u64,
    /// redb repaired the file while opening it (it was not closed cleanly).
    pub repaired: bool,
    pub damage: Option<DamageV2>,
    /// Converted from a version 1 file.
    pub link_v1: Option<LinkV1>,
}

impl ReportV2 {
    /// One paragraph for the user.
    pub fn describe(&self) -> String {
        let mut s = format!(
            "Opened: {} transactions (from the snapshot at {}, then {} replayed).",
            self.verified_seq, self.snapshot_seq, self.tail
        );
        if self.repaired {
            s.push_str(" The file was not closed cleanly last time and was recovered to its last save.");
        }
        if let Some(d) = &self.damage {
            s.push_str(&format!(
                " Opened read-only: {}{}. {} of {} transactions verified; use Save As to keep them.",
                d.seq.map_or(String::new(), |q| format!("at transaction {q}, ")),
                d.detail,
                self.verified_seq,
                self.head
            ));
        }
        s
    }
}

/// The project ID and cache of a file, after checking its container facts.
pub(crate) fn read_facts_pub(r: &Reader) -> Result<(ProjectId, Cache), OpenErrorV2> {
    read_facts(r).map(|f| (f.project, f.cache))
}

pub(crate) use decode_row as decode_row_pub;

/// A transaction row decoded and checked: its record against its stored chain value, its `seq`,
/// and its `prev` against the previous row's chain value.
pub(crate) fn decode_row(
    seq: u64,
    flag: u8,
    stored_chain: &Hash,
    body: &[u8],
    prev: &Hash,
) -> Result<Transaction, String> {
    let record = pack::unpack(flag, body)?;
    if chain::tx_hash(&record) != *stored_chain {
        return Err("the record does not match its chain value".into());
    }
    let value: Value = serde_json::from_slice(&record).map_err(|e| format!("not a record: {e}"))?;
    record::check_limits(&value)?;
    let (rseq, rprev) = record::seq_and_prev(&value);
    if rseq != Some(seq) {
        return Err(format!("the row of {seq} holds transaction {rseq:?}"));
    }
    if rprev.and_then(chain::parse_hex) != Some(*prev) {
        return Err("prev does not link to the previous transaction".into());
    }
    record::decode_tx(&value).map_err(|e| format!("{e:?}"))
}

/// The canonical record and chain value of `tx` after `prev`.
fn encode_tx(tx: &Transaction, prev: &Hash) -> Result<(Vec<u8>, Hash), String> {
    let value = record::tx_value(tx, prev)?;
    let canonical = crate::canonical::canonical(&value).map_err(|e| e.to_string())?;
    let hash = chain::tx_hash(&canonical);
    Ok((canonical, hash))
}

/// The undo and redo stacks at a point: their tops, and how many entries lie below what is held
/// (a snapshot records only the top [`STACK_CAP`] of each, brief 021).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct StackState {
    undo: Vec<(u64, TxId)>,
    redo: Vec<(u64, TxId)>,
    undo_hidden: u64,
    redo_hidden: u64,
}

impl StackState {
    /// Advance over `txs` (only kinds and IDs are read). `false` when a step pops below what is
    /// held: the whole stacks must be read first.
    fn advance(&mut self, txs: &[Transaction]) -> bool {
        for tx in txs {
            match tx.kind {
                TxKind::Edit => {
                    self.undo.push((tx.seq, tx.id));
                    self.redo.clear();
                    self.redo_hidden = 0;
                }
                TxKind::Undo { .. } => {
                    if self.undo.pop().is_none() && self.undo_hidden > 0 {
                        return false;
                    }
                    self.redo.push((tx.seq, tx.id));
                }
                TxKind::Redo { .. } => {
                    if self.redo.pop().is_none() && self.redo_hidden > 0 {
                        return false;
                    }
                    self.undo.push((tx.seq, tx.id));
                }
            }
        }
        true
    }

    /// The record's form: the top [`STACK_CAP`] entries of each stack and the counts below them.
    fn record(&self) -> (Vec<(u64, String)>, Vec<(u64, String)>, u64, u64) {
        let cap = usize::try_from(STACK_CAP).unwrap_or(usize::MAX);
        let top = |s: &[(u64, TxId)], hidden: u64| {
            let cut = s.len().saturating_sub(cap);
            let kept = s.get(cut..).unwrap_or_default().iter().map(|(q, id)| (*q, format!("{:032x}", id.0))).collect();
            (kept, hidden + cut as u64)
        };
        let ((undo, uh), (redo, rh)) = (top(&self.undo, self.undo_hidden), top(&self.redo, self.redo_hidden));
        (undo, redo, uh, rh)
    }

    /// From a version row.
    fn from_record(v: &VersionRecord) -> Result<Self, String> {
        Ok(Self {
            undo: stacks_from_record(&v.undo)?,
            redo: stacks_from_record(&v.redo)?,
            undo_hidden: v.undo_hidden,
            redo_hidden: v.redo_hidden,
        })
    }
}

fn stacks_from_record(s: &[(u64, String)]) -> Result<Vec<(u64, TxId)>, String> {
    s.iter()
        .map(|(q, id)| TxId::parse(id).map(|t| (*q, t)).ok_or_else(|| format!("{id:?} is not a transaction ID")))
        .collect()
}

/// The tag high-water mark of rows `1..=upto` of `branch`, read from the file: for a snapshot
/// written without `model:meta`, and for a point before the ledger's base.
fn scan_tag_high(r: &Reader, branch: BranchId, upto: u64, seed: Hash) -> Result<HighWater, String> {
    let mut high = HighWater::new();
    let mut from = 1;
    let mut prev = seed;
    while from <= upto {
        let to = upto.min(from + 4_095);
        let (txs, last, damage) = read_rows(r, branch, from, to, prev).map_err(|e| e.to_string())?;
        if let Some(d) = damage {
            return Err(format!("transaction {:?}: {}", d.seq, d.detail));
        }
        for tx in &txs {
            ecad_model::tag::note_ops(&mut high, &tx.ops);
        }
        prev = last;
        from = to + 1;
    }
    Ok(high)
}

/// Read a snapshot's document and its `model:meta`: every chunk, each checked against its hash.
fn load_snapshot(r: &Reader, v: &VersionRecord) -> Result<(Document, Option<ModelMeta>), String> {
    let set = ChunkSet::from_record(&v.chunks)?;
    let mut stored = Vec::with_capacity(set.chunks.len());
    for (key, hash) in &set.chunks {
        let (flag, body) = r
            .chunk(hash)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("chunk {} is missing", chain::hex(hash)))?;
        stored.push((key.clone(), *hash, flag, body));
    }
    chunks::assemble_stored(&set, stored)
}

/// Rows `from..=to` of `branch`, decoded and chain-checked from `prev`. Stops at the first bad
/// row: the good prefix, and the failure.
fn read_rows(
    r: &Reader,
    branch: BranchId,
    from: u64,
    to: u64,
    mut prev: Hash,
) -> Result<(Vec<Transaction>, Hash, Option<DamageV2>), StoreError> {
    let rows = r.tx_range(branch, from..=to)?;
    let mut out = Vec::with_capacity(rows.len());
    let mut expected = from;
    for (seq, flag, stored, body) in rows {
        if seq != expected {
            return Ok((
                out,
                prev,
                Some(DamageV2 { seq: Some(expected), detail: "the transaction is missing".into() }),
            ));
        }
        match decode_row(seq, flag, &stored, &body, &prev) {
            Ok(tx) => {
                out.push(tx);
                prev = stored;
            }
            Err(detail) => return Ok((out, prev, Some(DamageV2 { seq: Some(seq), detail }))),
        }
        expected += 1;
    }
    if expected <= to {
        return Ok((out, prev, Some(DamageV2 { seq: Some(expected), detail: "the transaction is missing".into() })));
    }
    Ok((out, prev, None))
}

/// Reads the history before a ledger's base from the file, on demand (never on open). It does
/// not keep the file open: once the project lets go of it (closed, or bound to another file by
/// Save As, which rebinds the ledger), reads report that the file is closed.
#[derive(Debug)]
pub struct StoreHistory {
    store: std::sync::Weak<Store>,
    branch: BranchId,
    seed: Hash,
}

impl StoreHistory {
    fn read(&self) -> ecad_model::Result<Reader> {
        let store =
            self.store.upgrade().ok_or_else(|| ModelError::CorruptLog("the project's file is closed".into()))?;
        store.reader().map_err(|e| ModelError::CorruptLog(e.to_string()))
    }

    /// The chain value before `seq` (the seed before 1).
    fn prev_of(&self, r: &Reader, seq: u64) -> ecad_model::Result<Hash> {
        if seq <= 1 {
            return Ok(self.seed);
        }
        r.tx(self.branch, seq - 1)
            .map_err(|e| ModelError::CorruptLog(e.to_string()))?
            .map(|(_, c, _)| c)
            .ok_or_else(|| ModelError::CorruptLog(format!("transaction {} is missing", seq - 1)))
    }
}

impl History for StoreHistory {
    fn transaction(&self, seq: u64) -> ecad_model::Result<Transaction> {
        let r = self.read()?;
        let prev = self.prev_of(&r, seq)?;
        let (flag, chain, body) = r
            .tx(self.branch, seq)
            .map_err(|e| ModelError::CorruptLog(e.to_string()))?
            .ok_or_else(|| ModelError::CorruptLog(format!("transaction {seq} is missing")))?;
        decode_row(seq, flag, &chain, &body, &prev)
            .map_err(|e| ModelError::CorruptLog(format!("transaction {seq}: {e}")))
    }

    fn transactions(&self, from: u64, to: u64) -> ecad_model::Result<Vec<Transaction>> {
        let r = self.read()?;
        let prev = self.prev_of(&r, from)?;
        let (txs, _, damage) =
            read_rows(&r, self.branch, from, to, prev).map_err(|e| ModelError::CorruptLog(e.to_string()))?;
        match damage {
            Some(d) => Err(ModelError::CorruptLog(format!("transaction {:?}: {}", d.seq, d.detail))),
            None => Ok(txs),
        }
    }

    /// Walks the history backwards from `seq`, only as far as the wanted entries go: an undo pops
    /// the newest live entry of the undo stack and an edit or redo pushes one; a redo pops the
    /// redo stack, an undo pushes one, and an edit empties it. Each row is checked against its
    /// chain value and the row before it.
    fn stack_slice(&self, seq: u64, undo: bool, skip: u64, count: u64) -> ecad_model::Result<Vec<(u64, TxId)>> {
        #[derive(serde::Deserialize)]
        struct Head {
            seq: u64,
            id: TxId,
            kind: crate::v1::TxKind,
            prev: String,
        }
        let bad = |m: String| ModelError::CorruptLog(m);
        let r = self.read()?;
        let mut out = Vec::new();
        let (mut debt, mut skipped) = (0u64, 0u64);
        // The `prev` of the lowest row read so far: the next (older) range must end with it.
        let mut expect_last: Option<Hash> = None;
        let mut hi = seq;
        'walk: while hi >= 1 && (out.len() as u64) < count {
            let lo = hi.saturating_sub(4_095).max(1);
            let rows = r.tx_range(self.branch, lo..=hi).map_err(|e| bad(e.to_string()))?;
            if rows.len() as u64 != hi - lo + 1 {
                return Err(bad(format!("transactions {lo} to {hi} are not all stored")));
            }
            let mut heads = Vec::with_capacity(rows.len());
            let mut last_chain = None;
            for (q, flag, chain_value, body) in rows {
                let record = pack::unpack(flag, &body).map_err(bad)?;
                if chain::tx_hash(&record) != chain_value {
                    return Err(bad(format!("transaction {q} does not match its chain value")));
                }
                let h: Head = serde_json::from_slice(&record).map_err(|e| bad(format!("transaction {q}: {e}")))?;
                let prev = chain::parse_hex(&h.prev).ok_or_else(|| bad(format!("transaction {q}: bad prev")))?;
                if h.seq != q || last_chain.is_some_and(|c| c != prev) {
                    return Err(bad(format!("transaction {q} does not follow the one before")));
                }
                last_chain = Some(chain_value);
                heads.push((h, prev));
            }
            if let (Some(want), Some(have)) = (expect_last, last_chain)
                && want != have
            {
                return Err(bad("the history's chain is broken between two ranges".into()));
            }
            expect_last = heads.first().map(|(_, p)| *p);
            for (h, _) in heads.into_iter().rev() {
                use crate::v1::TxKind as K;
                let (push, pop, clears) = match (h.kind, undo) {
                    (K::Edit, true) | (K::Redo(_), true) => (true, false, false),
                    (K::Undo(_), true) => (false, true, false),
                    (K::Undo(_), false) => (true, false, false),
                    (K::Redo(_), false) => (false, true, false),
                    (K::Edit, false) => (false, false, true),
                };
                if clears {
                    break 'walk;
                }
                if pop {
                    debt += 1;
                } else if push {
                    if debt > 0 {
                        debt -= 1;
                    } else if skipped < skip {
                        skipped += 1;
                    } else {
                        out.push((h.seq, h.id));
                        if out.len() as u64 == count {
                            break 'walk;
                        }
                    }
                }
            }
            if lo == 1 {
                break;
            }
            hi = lo - 1;
        }
        out.reverse();
        Ok(out)
    }

    /// Only each row's `seq`, `id`, `kind` and `prev` are decoded (its record still checked
    /// against its chain value): the ops are skipped. Rows are read and decoded in parallel, in
    /// waves of up to eight ranges of 4 096, then folded into the stacks in order, so a long
    /// history is read once quickly and memory stays bounded.
    fn stacks_at(&self, seq: u64) -> ecad_model::Result<(Vec<(u64, TxId)>, Vec<(u64, TxId)>)> {
        #[derive(serde::Deserialize)]
        struct Head {
            seq: u64,
            id: TxId,
            kind: crate::v1::TxKind,
            prev: String,
        }
        /// A range's rows, checked on their own: each row against its chain value and the one
        /// before it; the first row's `prev` and the last row's chain value link the ranges.
        struct Range {
            first_prev: Hash,
            last: Hash,
            rows: Vec<(u64, TxId, crate::v1::TxKind)>,
        }
        let bad = |m: String| ModelError::CorruptLog(m);
        let store = self.store.upgrade().ok_or_else(|| bad("the project's file is closed".into()))?;
        let branch = self.branch;
        let read_range = |(from, to): (u64, u64)| -> Result<Range, String> {
            let r = store.reader().map_err(|e| e.to_string())?;
            let stored = r.tx_range(branch, from..=to).map_err(|e| e.to_string())?;
            if stored.len() as u64 != to - from + 1 {
                return Err(format!("transactions {from} to {to} are not all stored"));
            }
            let mut out = Range { first_prev: [0; 32], last: [0; 32], rows: Vec::with_capacity(stored.len()) };
            for (i, (q, flag, chain_value, body)) in stored.into_iter().enumerate() {
                let record = pack::unpack(flag, &body)?;
                if chain::tx_hash(&record) != chain_value {
                    return Err(format!("transaction {q} does not match its chain value"));
                }
                let h: Head = serde_json::from_slice(&record).map_err(|e| format!("transaction {q}: {e}"))?;
                let prev = chain::parse_hex(&h.prev).ok_or_else(|| format!("transaction {q}: bad prev"))?;
                if h.seq != q || (i > 0 && prev != out.last) {
                    return Err(format!("transaction {q} does not follow the one before"));
                }
                if i == 0 {
                    out.first_prev = prev;
                }
                out.last = chain_value;
                out.rows.push((q, h.id, h.kind));
            }
            Ok(out)
        };
        let (mut undo, mut redo): (Vec<(u64, TxId)>, Vec<(u64, TxId)>) = (Vec::new(), Vec::new());
        let mut prev = self.seed;
        let mut from = 1;
        while from <= seq {
            let mut ranges = Vec::new();
            while from <= seq && ranges.len() < 8 {
                let to = seq.min(from + 4_095);
                ranges.push((from, to));
                from = to + 1;
            }
            let wanted = ranges.len();
            let read = chunks::par_map(ranges, read_range);
            if read.len() != wanted {
                return Err(bad("the history could not be read: a worker thread did not start".into()));
            }
            for range in read {
                let range = range.map_err(bad)?;
                if range.first_prev != prev {
                    return Err(bad("the history's chain is broken between two ranges".into()));
                }
                prev = range.last;
                for (q, id, kind) in range.rows {
                    match kind {
                        crate::v1::TxKind::Edit => {
                            undo.push((q, id));
                            redo.clear();
                        }
                        crate::v1::TxKind::Undo(of) => {
                            if undo.pop().map(|t| t.1) != Some(of) {
                                return Err(bad(format!("undo {q} does not undo the latest step")));
                            }
                            redo.push((q, id));
                        }
                        crate::v1::TxKind::Redo(of) => {
                            if redo.pop().map(|t| t.1) != Some(of) {
                                return Err(bad(format!("redo {q} does not redo the latest undo")));
                            }
                            undo.push((q, id));
                        }
                    }
                }
            }
        }
        Ok((undo, redo))
    }

    fn state_at(&self, seq: u64) -> ecad_model::Result<Document> {
        let r = self.read()?;
        let err = |e: String| ModelError::CorruptLog(e);
        // The nearest snapshot at or before `seq`: an automatic one, or a named version.
        let mut best: Option<VersionRecord> = r.auto_at_or_before(self.branch, seq).map_err(|e| err(e.to_string()))?;
        for kind in [VersionKind::User, VersionKind::Revision] {
            for v in r.versions(self.branch, kind).map_err(|e| err(e.to_string()))? {
                if v.point.seq <= seq && best.as_ref().is_none_or(|b| b.point.seq < v.point.seq) {
                    best = Some(v);
                }
            }
        }
        let (mut doc, from, prev) = match &best {
            Some(v) => (load_snapshot(&r, v).map_err(err)?.0, v.point.seq, v.point.hash().map_err(err)?),
            None => (Document::default(), 0, self.seed),
        };
        let (txs, _, damage) = read_rows(&r, self.branch, from + 1, seq, prev).map_err(|e| err(e.to_string()))?;
        if let Some(d) = damage {
            return Err(err(format!("transaction {:?}: {}", d.seq, d.detail)));
        }
        doc.rebuild_index();
        for tx in &txs {
            for op in &tx.ops {
                op.apply(&mut doc)?;
            }
        }
        doc.check()?;
        Ok(doc)
    }
}

/// The in-memory snapshot the next one is built from.
#[derive(Debug, Clone)]
struct Snap {
    seq: u64,
    set: ChunkSet,
}

/// A project bound to a version 2 database.
#[derive(Debug)]
pub struct ProjectV2 {
    store: Arc<Store>,
    project: ProjectId,
    branch: BranchId,
    /// The branch head in the file: `seq` and chain value.
    head: (u64, Hash),
    cache: Cache,
    snap: Snap,
    /// The tag high-water mark at `base_seq`.
    base_tag_high: HighWater,
    /// The `versions` row of the snapshot opened from or last written (`None`: the root).
    snap_ref: Option<SnapshotRef>,
    /// The undo and redo stacks at the ledger's base (the snapshot opened from).
    base_stacks: StackState,
    base_seq: u64,
    writer: String,
    read_only: Option<String>,
    link_v1: Option<LinkV1>,
    created_ms: Option<u64>,
    last_save: Option<SavedV2>,
    /// `meta.writer` names this build already (written by the first save of a session).
    wrote_writer: bool,
}

/// A project opened from a version 2 database.
#[derive(Debug)]
pub struct OpenedV2 {
    pub project: ProjectV2,
    pub ledger: Ledger,
    pub report: ReportV2,
}

fn meta_json(r: &Reader, name: &str) -> Result<Option<Value>, OpenErrorV2> {
    match r.meta(name)? {
        None => Ok(None),
        Some(b) => serde_json::from_slice(&b).map(Some).map_err(|e| OpenErrorV2::Damaged(format!("meta {name}: {e}"))),
    }
}

/// The container facts, checked as E2 §5 checks a header.
struct Facts {
    project: ProjectId,
    cache: Cache,
    link_v1: Option<LinkV1>,
    created_ms: Option<u64>,
}

fn read_facts(r: &Reader) -> Result<Facts, OpenErrorV2> {
    for (name, _) in r.meta_all()? {
        if !META_NAMES.contains(&name.as_str()) {
            return Err(OpenErrorV2::Unsupported(format!("meta {name:?} is not something this build reads")));
        }
    }
    let format = meta_json(r, "format")?;
    if format.as_ref().and_then(Value::as_str) != Some(FORMAT_PROJECT) {
        return Err(OpenErrorV2::Unsupported(format!("not an ECAD project file (format {format:?})")));
    }
    let cv = meta_json(r, "container_version")?.and_then(|v| v.as_u64());
    if cv != Some(CONTAINER_VERSION) {
        return Err(OpenErrorV2::Unsupported(format!(
            "container version {cv:?} is not supported (this build reads {CONTAINER_VERSION})"
        )));
    }
    let sv = meta_json(r, "schema_version")?.and_then(|v| v.as_u64()).unwrap_or(0);
    if sv == 0 || sv > SCHEMA_VERSION {
        return Err(OpenErrorV2::Unsupported(format!(
            "schema version {sv} is not supported (this build reads 1 to {SCHEMA_VERSION})"
        )));
    }
    let codec = meta_json(r, "codec")?;
    if codec.as_ref().and_then(Value::as_str) != Some(CODEC) {
        return Err(OpenErrorV2::Unsupported(format!("codec {codec:?} is not supported (this build reads {CODEC:?})")));
    }
    let project = meta_json(r, "file_id")?
        .and_then(|v| v.as_str().and_then(ProjectId::parse))
        .ok_or_else(|| OpenErrorV2::Damaged("meta file_id is missing or malformed".into()))?;
    let cache: Cache = match meta_json(r, "cache")? {
        Some(v) => serde_json::from_value(v).map_err(|e| OpenErrorV2::Damaged(format!("meta cache: {e}")))?,
        None => Cache::default(),
    };
    if !(SNAPSHOT_EVERY_RANGE.0..=SNAPSHOT_EVERY_RANGE.1).contains(&cache.every) {
        return Err(OpenErrorV2::Damaged(format!("meta cache: snapshot interval {} is out of range", cache.every)));
    }
    let link_v1 = match meta_json(r, "link_v1")? {
        Some(v) => Some(serde_json::from_value(v).map_err(|e| OpenErrorV2::Damaged(format!("meta link_v1: {e}")))?),
        None => None,
    };
    let created_ms = meta_json(r, "created_ms")?.and_then(|v| v.as_u64());
    Ok(Facts { project, cache, link_v1, created_ms })
}

/// The original branch, `main`.
fn main_branch(r: &Reader, project: ProjectId) -> Result<(BranchId, BranchRecord), OpenErrorV2> {
    let id = BranchId::main_of(project.0);
    let rec = r.branch(id)?.ok_or_else(|| OpenErrorV2::Damaged("the file has no main branch".into()))?;
    if rec.name != "main" || rec.parent.is_some() || rec.fork_point.is_some() || rec.status != BranchStatus::Open {
        return Err(OpenErrorV2::Damaged("the main branch's record is inconsistent".into()));
    }
    Ok((id, rec))
}

impl ProjectV2 {
    /// Open the project in `store` (EEP-0005 §5). `seed` seeds IDs allocated after opening.
    /// Damage found while loading opens read-only, at the last state that verifies, with a report.
    ///
    /// # Errors
    /// [`OpenErrorV2`] when the container facts or the branch row are unusable.
    pub fn open(store: Store, seed: u64, writer: &str) -> Result<OpenedV2, OpenErrorV2> {
        let store = Arc::new(store);
        let r = store.reader()?;
        let facts = read_facts(&r)?;
        let (branch, rec) = main_branch(&r, facts.project)?;
        let seed_chain = chain::seed(ChainKind::Project, facts.project.0);
        let head = rec.head.seq;
        let head_chain = rec.head.hash().map_err(OpenErrorV2::Damaged)?;
        let mut damage = None;
        // The head must be the branch's last row, with the recorded chain value.
        let last = r.last_seq(branch)?.unwrap_or(0);
        if last != head {
            damage = Some(DamageV2 {
                seq: Some(head),
                detail: format!("the head is {head} but the last stored transaction is {last}"),
            });
        } else if head > 0 && r.tx(branch, head)?.map(|(_, c, _)| c) != Some(head_chain) {
            damage = Some(DamageV2 {
                seq: Some(head),
                detail: "the head transaction does not match the branch head".into(),
            });
        }
        // The latest snapshot at or before the head.
        let latest = facts.cache.latest.get(&branch.hex()).filter(|s| s.seq <= head);
        let mut loaded = None;
        let mut snap_ref = None;
        if let Some(s) = latest {
            match r.version(branch, s.kind, &s.name)? {
                Some(v) if v.point.seq == s.seq => match load_snapshot(&r, &v) {
                    Ok((mut doc, meta)) => {
                        // Every invariant of the snapshot is checked in debug builds and after a
                        // crash repair (overseer, 2026-11-09: about 38 ms on the large project, so
                        // not on every normal release open). Its chunks were checked against their
                        // hashes in any case, and Verify checks everything on request.
                        doc.rebuild_index();
                        let checked = if cfg!(debug_assertions) || store.repaired() { doc.check() } else { Ok(()) };
                        match checked {
                            Ok(()) => {
                                loaded = Some((v, doc, meta));
                                snap_ref = Some(s.clone());
                            }
                            Err(e) => {
                                damage = damage.or(Some(DamageV2 {
                                    seq: Some(s.seq),
                                    detail: format!("snapshot breaks an invariant: {e}"),
                                }))
                            }
                        }
                    }
                    Err(e) => damage = damage.or(Some(DamageV2 { seq: Some(s.seq), detail: format!("snapshot: {e}") })),
                },
                _ => {
                    damage =
                        damage.or(Some(DamageV2 { seq: Some(s.seq), detail: "the latest snapshot is missing".into() }))
                }
            }
        }
        let mut snap_meta = None;
        let (snap_seq, snap_chain, doc, stacks, set) = match loaded {
            Some((v, doc, meta)) => {
                snap_meta = meta;
                let chain = v.point.hash().map_err(OpenErrorV2::Damaged)?;
                let stacks = StackState::from_record(&v).map_err(OpenErrorV2::Damaged)?;
                let set = ChunkSet::from_record(&v.chunks).map_err(OpenErrorV2::Damaged)?;
                (v.point.seq, chain, doc, stacks, set)
            }
            None => (0, seed_chain, Document::default(), StackState::default(), ChunkSet::default()),
        };
        // The snapshot's point must be the stored row's chain value.
        if snap_seq > 0 && r.tx(branch, snap_seq)?.map(|(_, c, _)| c) != Some(snap_chain) {
            damage = damage.or(Some(DamageV2 {
                seq: Some(snap_seq),
                detail: "the snapshot's point does not match its transaction".into(),
            }));
        }
        // The tag high-water mark at the snapshot: carried by `model:meta`; a snapshot written
        // without it is scanned once (a cache, rebuilt from the history).
        let base_tag_high = match snap_meta {
            Some(m) => m.tag_high,
            None if snap_seq > 0 => scan_tag_high(&r, branch, snap_seq, seed_chain).unwrap_or_default(),
            None => HighWater::new(),
        };
        let to = head.min(last);
        let (tail, verified_chain, row_damage) = read_rows(&r, branch, snap_seq + 1, to, snap_chain)?;
        drop(r);
        let damage = damage.or(row_damage);
        let history: Arc<dyn History> =
            Arc::new(StoreHistory { store: Arc::downgrade(&store), branch, seed: seed_chain });
        let base = Base {
            seq: snap_seq,
            doc,
            undo: stacks.undo.clone(),
            redo: stacks.redo.clone(),
            undo_hidden: stacks.undo_hidden,
            redo_hidden: stacks.redo_hidden,
            indexed: true,
            history,
        };
        let tail_len = tail.len() as u64;
        let (ledger, failure) = Ledger::from_base(base, tail, seed)
            .map_err(|e| OpenErrorV2::Damaged(format!("the snapshot at {snap_seq}: {e}")))?;
        let damage =
            damage.or(failure.map(|(i, e)| DamageV2 { seq: Some(snap_seq + i as u64 + 1), detail: e.to_string() }));
        let verified = ledger.head();
        let verified_chain = if verified == snap_seq + tail_len {
            verified_chain
        } else {
            // Replay stopped early: the chain value of the last good row.
            store.reader()?.tx(branch, verified)?.map(|(_, c, _)| c).unwrap_or(snap_chain)
        };
        let read_only = damage.as_ref().map(|d| format!("the file is damaged ({})", d.detail));
        let read_only = read_only.or_else(|| (!store.writable()).then(|| "it is open in another session".to_string()));
        let report = ReportV2 {
            head,
            verified_seq: verified,
            snapshot_seq: snap_seq,
            tail: verified - snap_seq,
            repaired: store.repaired(),
            damage,
            link_v1: facts.link_v1.clone(),
        };
        let project = ProjectV2 {
            store,
            project: facts.project,
            branch,
            head: (verified, verified_chain),
            cache: facts.cache,
            snap: Snap { seq: snap_seq, set },
            base_tag_high,
            snap_ref,
            base_stacks: stacks,
            base_seq: snap_seq,
            writer: writer.to_string(),
            read_only,
            link_v1: facts.link_v1,
            created_ms: facts.created_ms,
            last_save: None,
            wrote_writer: false,
        };
        Ok(OpenedV2 { project, ledger, report })
    }

    /// Write `ledger`'s whole history into the empty database `store` as a new project file:
    /// `main` from the project root, a snapshot for every named version, and a snapshot at the
    /// head. `link` names the version 1 history it continues, when converted; `records` are then
    /// the version 1 records exactly as stored (canonical), so the chain is version 1's.
    ///
    /// # Errors
    /// [`SaveErrorV2`]; nothing is committed.
    pub fn create(
        store: Store,
        project: ProjectId,
        ledger: &Ledger,
        records: Option<&[Vec<u8>]>,
        link: Option<LinkV1>,
        created_ms: Option<u64>,
        writer: &str,
        every: u64,
    ) -> Result<(ProjectV2, SavedV2), SaveErrorV2> {
        if ledger.base() != 0 {
            return Err(SaveErrorV2::Encode("a new file needs the whole history in memory".into()));
        }
        let store = Arc::new(store);
        let branch = BranchId::main_of(project.0);
        let seed = chain::seed(ChainKind::Project, project.0);
        let mut w = store.writer()?;
        let mut saved = SavedV2 { first_seq: 1, last_seq: ledger.head(), ..SavedV2::default() };
        // Rows, and the chain value of every point.
        let mut chains = Vec::with_capacity(ledger.log().len() + 1);
        chains.push(seed);
        let mut prev = seed;
        for (i, tx) in ledger.log().iter().enumerate() {
            let (bytes, hash) = match records.and_then(|r| r.get(i)) {
                Some(b) => (b.clone(), chain::tx_hash(b)),
                None => encode_tx(tx, &prev).map_err(SaveErrorV2::Encode)?,
            };
            saved.bytes += w.append_tx(branch, tx.seq, &hash, &bytes)?;
            chains.push(hash);
            prev = hash;
        }
        // Version snapshots, then the head: one forward pass over the log.
        let doc_end = ledger.document();
        let mut points: Vec<(u64, VersionKind, String)> = Vec::new();
        for (name, at) in &doc_end.versions {
            let kind = if revision_of(doc_end, name).is_some() { VersionKind::Revision } else { VersionKind::User };
            points.push((*at, kind, name.clone()));
        }
        points.push((ledger.head(), VersionKind::Auto, auto_name(ledger.head())));
        points.sort();
        let mut doc = Document::default();
        let mut stacks = StackState::default();
        let mut done = 0u64;
        let mut snap = Snap { seq: 0, set: ChunkSet::default() };
        let mut first = true;
        let mut latest: Option<SnapshotRef> = None;
        let mut high = HighWater::new();
        for (at, kind, name) in points {
            let range = ledger.log().get(done as usize..at as usize).unwrap_or_default();
            let mut dirty = Dirty::default();
            for tx in range {
                for op in &tx.ops {
                    op.apply(&mut doc).map_err(|e| SaveErrorV2::Encode(format!("replaying {}: {e}", tx.seq)))?;
                }
                dirty.add_ops(&tx.ops);
                ecad_model::tag::note_ops(&mut high, &tx.ops);
            }
            stacks.advance(range);
            done = at;
            let meta = ModelMeta { tag_high: high.clone() };
            let (set, new) = if first {
                chunks::build_full(&doc, &meta)
            } else {
                chunks::build_incremental(&doc, &snap.set, &dirty, &meta)
            }
            .map_err(SaveErrorV2::Encode)?;
            first = false;
            for p in &new {
                let n = w.put_chunk(&p.hash, &p.body)?;
                saved.bytes += n;
                saved.chunks_written += usize::from(n > 0);
            }
            let chain = chains.get(at as usize).copied().unwrap_or(seed);
            let release = (kind == VersionKind::Revision).then(|| revision_of(doc_end, &name)).flatten();
            let rec = stacks.record();
            w.put_version(
                branch,
                &VersionRecord {
                    kind,
                    name: name.clone(),
                    point: Point::new(at, &chain),
                    chunks: set.to_record(),
                    release,
                    undo: rec.0,
                    redo: rec.1,
                    undo_hidden: rec.2,
                    redo_hidden: rec.3,
                    stack_cap: STACK_CAP,
                },
            )?;
            saved.snapshots += 1;
            latest = Some(SnapshotRef { kind, name, seq: at });
            snap = Snap { seq: at, set };
        }
        let mut cache = Cache { every, ..Cache::default() };
        if let Some(l) = &latest {
            cache.latest.insert(branch.hex(), l.clone());
        }
        let head = (ledger.head(), prev);
        w.put_branch(
            branch,
            &BranchRecord {
                name: "main".into(),
                status: BranchStatus::Open,
                parent: None,
                fork_point: None,
                head: Point::new(head.0, &head.1),
                release_line: true,
            },
        )?;
        w.set_meta("format", &json!(FORMAT_PROJECT))?;
        w.set_meta("container_version", &json!(CONTAINER_VERSION))?;
        w.set_meta("schema_version", &json!(SCHEMA_VERSION))?;
        w.set_meta("codec", &json!(CODEC))?;
        w.set_meta("file_id", &json!(project.hex()))?;
        w.set_meta("writer", &json!(writer))?;
        w.set_meta("created_ms", &json!(created_ms))?;
        w.set_meta("cache", &serde_json::to_value(&cache).map_err(|e| SaveErrorV2::Encode(e.to_string()))?)?;
        if let Some(l) = &link {
            w.set_meta("link_v1", &serde_json::to_value(l).map_err(|e| SaveErrorV2::Encode(e.to_string()))?)?;
        }
        w.commit()?;
        let me = ProjectV2 {
            store,
            project,
            branch,
            head,
            cache,
            snap: snap.clone(),
            base_tag_high: HighWater::new(),
            snap_ref: latest,
            base_stacks: StackState::default(),
            base_seq: 0,
            writer: writer.to_string(),
            read_only: None,
            link_v1: link,
            created_ms,
            last_save: Some(saved.clone()),
            wrote_writer: true,
        };
        Ok((me, saved))
    }

    /// Save the transactions after the file's head (EEP-0005 §6), with the snapshots of any
    /// versions they name and an automatic snapshot when due, in one write transaction. On
    /// success the ledger is marked persisted.
    ///
    /// # Errors
    /// [`SaveErrorV2`]; the file and the ledger are then unchanged.
    pub fn save(&mut self, ledger: &mut Ledger) -> Result<SavedV2, SaveErrorV2> {
        if let Some(r) = &self.read_only {
            return Err(SaveErrorV2::ReadOnly(r.clone()));
        }
        let (head, mut prev) = self.head;
        if ledger.head() == head {
            if ledger.persisted() != head {
                ledger.mark_persisted(head).map_err(|e| SaveErrorV2::Encode(e.to_string()))?;
            }
            return Ok(SavedV2 { first_seq: head + 1, last_seq: head, ..SavedV2::default() });
        }
        if ledger.base() > head || ledger.head() < head {
            return Err(SaveErrorV2::Encode(format!(
                "the project's history (base {}, head {}) does not continue the file's head {head}",
                ledger.base(),
                ledger.head()
            )));
        }
        let start = usize::try_from(head - ledger.base()).map_err(|e| SaveErrorV2::Encode(e.to_string()))?;
        let pending: Vec<Transaction> = ledger.log().get(start..).unwrap_or_default().to_vec();
        let mut w = self.store.writer()?;
        // Another writer cannot exist (locks); check anyway, inside this write transaction: one
        // row read.
        {
            match w.branch(self.branch)? {
                Some(b) if b.head.seq == head && b.head.hash().ok() == Some(prev) => {}
                Some(b) => return Err(SaveErrorV2::ExternalChange(format!("the branch head is now {}", b.head.seq))),
                None => return Err(SaveErrorV2::ExternalChange("the branch is gone".into())),
            }
        }
        let mut saved = SavedV2 { first_seq: head + 1, last_seq: ledger.head(), ..SavedV2::default() };
        let mut chains: BTreeMap<u64, Hash> = BTreeMap::new();
        chains.insert(head, prev);
        for tx in &pending {
            let (bytes, hash) = encode_tx(tx, &prev).map_err(SaveErrorV2::Encode)?;
            saved.bytes += w.append_tx(self.branch, tx.seq, &hash, &bytes)?;
            chains.insert(tx.seq, hash);
            prev = hash;
        }
        // Versions named or unnamed by these transactions, in order; then an automatic snapshot.
        let mut snap = self.snap.clone();
        let mut cache = self.cache.clone();
        for tx in &pending {
            for op in &tx.ops {
                match op {
                    Op::NameVersion { name, at } => {
                        let kind = if issues(tx, name) { VersionKind::Revision } else { VersionKind::User };
                        // Replaced if it existed under the other kind.
                        for k in [VersionKind::User, VersionKind::Revision] {
                            w.remove_version(self.branch, k, name)?;
                        }
                        let release = if kind == VersionKind::Revision {
                            Some(release_at(ledger, tx, name).map_err(SaveErrorV2::Encode)?)
                        } else {
                            None
                        };
                        let chain = self.chain_at(*at, &chains)?;
                        self.write_snapshot(
                            &mut w, ledger, &mut snap, &mut cache, *at, chain, kind, name, release, &mut saved,
                        )?;
                    }
                    Op::UnnameVersion { name, .. } => {
                        for k in [VersionKind::User, VersionKind::Revision] {
                            w.remove_version(self.branch, k, name)?;
                        }
                    }
                    _ => {}
                }
            }
        }
        let end = ledger.head();
        if end.saturating_sub(snap.seq) >= cache.every {
            let chain = prev;
            self.write_snapshot(
                &mut w,
                ledger,
                &mut snap,
                &mut cache,
                end,
                chain,
                VersionKind::Auto,
                &auto_name(end),
                None,
                &mut saved,
            )?;
        }
        w.put_branch(
            self.branch,
            &BranchRecord {
                name: "main".into(),
                status: BranchStatus::Open,
                parent: None,
                fork_point: None,
                head: Point::new(end, &prev),
                release_line: true,
            },
        )?;
        if !self.wrote_writer {
            w.set_meta("writer", &json!(self.writer))?;
        }
        if cache != self.cache {
            w.set_meta("cache", &serde_json::to_value(&cache).map_err(|e| SaveErrorV2::Encode(e.to_string()))?)?;
        }
        w.commit()?;
        self.head = (end, prev);
        self.snap = snap;
        self.snap_ref = cache.latest.get(&self.branch.hex()).cloned();
        self.cache = cache;
        ledger.mark_persisted(end).map_err(|e| SaveErrorV2::Encode(e.to_string()))?;
        self.last_save = Some(saved.clone());
        self.wrote_writer = true;
        Ok(saved)
    }

    /// The chain value at `seq`: from this save's rows, or the stored row.
    fn chain_at(&self, seq: u64, chains: &BTreeMap<u64, Hash>) -> Result<Hash, SaveErrorV2> {
        if let Some(c) = chains.get(&seq) {
            return Ok(*c);
        }
        if seq == 0 {
            return Ok(chain::seed(ChainKind::Project, self.project.0));
        }
        let r = self.store.reader()?;
        r.tx(self.branch, seq)?
            .map(|(_, c, _)| c)
            .ok_or_else(|| SaveErrorV2::Encode(format!("transaction {seq} is not stored")))
    }

    /// Build and write the snapshot at `at` (a point at or before the ledger's head), relative to
    /// `snap` when the transactions between them are in memory.
    #[allow(clippy::too_many_arguments)]
    fn write_snapshot(
        &mut self,
        w: &mut super::store::Writer,
        ledger: &mut Ledger,
        snap: &mut Snap,
        cache: &mut Cache,
        at: u64,
        chain: Hash,
        kind: VersionKind,
        name: &str,
        release: Option<Release>,
        saved: &mut SavedV2,
    ) -> Result<(), SaveErrorV2> {
        let enc = SaveErrorV2::Encode;
        let base = ledger.base();
        let meta = ModelMeta { tag_high: self.tag_high_at(ledger, at)? };
        let (lo, hi) = (at.min(snap.seq), at.max(snap.seq));
        let in_memory = lo >= base;
        let built = if in_memory {
            let mut dirty = Dirty::default();
            for seq in lo + 1..=hi {
                if let Some(tx) = ledger.transaction(seq) {
                    dirty.add_ops(&tx.ops);
                }
            }
            let undo_after: Vec<Op> = ledger
                .log()
                .iter()
                .rev()
                .take_while(|t| t.seq > at)
                .flat_map(|t| t.ops.iter().rev().map(Op::inverse))
                .collect();
            let prev = snap.set.clone();
            if undo_after.is_empty() {
                chunks::build_incremental(ledger.document(), &prev, &dirty, &meta)
            } else {
                ledger
                    .with_applied(&undo_after, |d| chunks::build_incremental(d, &prev, &dirty, &meta))
                    .map_err(|e| enc(e.to_string()))?
            }
        } else {
            let doc = ledger.state_at(at).map_err(|e| enc(e.to_string()))?;
            chunks::build_full(&doc, &meta)
        }
        .map_err(enc)?;
        let (set, new) = built;
        for p in &new {
            let n = w.put_chunk(&p.hash, &p.body)?;
            saved.bytes += n;
            saved.chunks_written += usize::from(n > 0);
        }
        let (undo, redo, undo_hidden, redo_hidden) = self.stacks_at(ledger, at)?.record();
        w.put_version(
            self.branch,
            &VersionRecord {
                kind,
                name: name.to_string(),
                point: Point::new(at, &chain),
                chunks: set.to_record(),
                release,
                undo,
                redo,
                undo_hidden,
                redo_hidden,
                stack_cap: STACK_CAP,
            },
        )?;
        saved.snapshots += 1;
        // The latest snapshot (the one opening uses) only moves forward.
        let latest = cache.latest.get(&self.branch.hex()).map_or(0, |s| s.seq);
        if at >= latest {
            cache.latest.insert(self.branch.hex(), SnapshotRef { kind, name: name.to_string(), seq: at });
        }
        if in_memory {
            *snap = Snap { seq: at, set };
        }
        Ok(())
    }

    /// After a rescue copy (Save As of a damaged file, rows after the verified head left out):
    /// set the branch head to the verified one, drop versions after it, and point the cache at
    /// the snapshot this project opened from (none: the root).
    ///
    /// # Errors
    /// The engine failed.
    pub fn write_rescue(&self, w: &mut super::store::Writer, r: &Reader) -> Result<(), SaveErrorV2> {
        let (seq, chain) = self.head;
        w.put_branch(
            self.branch,
            &BranchRecord {
                name: "main".into(),
                status: BranchStatus::Open,
                parent: None,
                fork_point: None,
                head: Point::new(seq, &chain),
                release_line: true,
            },
        )?;
        for kind in [VersionKind::User, VersionKind::Revision, VersionKind::Auto] {
            for v in r.versions(self.branch, kind)? {
                if v.point.seq > seq {
                    w.remove_version(self.branch, kind, &v.name)?;
                }
            }
        }
        let mut cache = self.cache.clone();
        cache.latest.remove(&self.branch.hex());
        if let Some(s) = self.snap_ref.as_ref().filter(|s| s.seq <= seq) {
            cache.latest.insert(self.branch.hex(), s.clone());
        }
        w.set_meta("cache", &serde_json::to_value(&cache).map_err(|e| SaveErrorV2::Encode(e.to_string()))?)?;
        Ok(())
    }

    /// The undo and redo stacks at `at`: from those at the snapshot this project was opened from
    /// and the transactions after it in memory. When that pops below what is held, the next
    /// entries of the snapshot's stacks are read from the file ([`STACK_CAP`] at a time, walking
    /// back only as far as they go) and kept. A point before the snapshot is read from the file.
    fn stacks_at(&mut self, ledger: &Ledger, at: u64) -> Result<StackState, SaveErrorV2> {
        let enc = |e: ModelError| SaveErrorV2::Encode(e.to_string());
        if at >= self.base_seq && self.base_seq >= ledger.base() {
            let from = usize::try_from(self.base_seq - ledger.base()).unwrap_or(usize::MAX);
            let to = usize::try_from(at - ledger.base()).unwrap_or(usize::MAX);
            let txs = ledger.log().get(from..to).unwrap_or_default();
            loop {
                let mut s = self.base_stacks.clone();
                if s.advance(txs) {
                    return Ok(s);
                }
                let history = self.history();
                let base = &mut self.base_stacks;
                let mut grew = false;
                for undo in [true, false] {
                    let (stack, hidden) = if undo {
                        (&mut base.undo, &mut base.undo_hidden)
                    } else {
                        (&mut base.redo, &mut base.redo_hidden)
                    };
                    if *hidden == 0 {
                        continue;
                    }
                    let n = (*hidden).min(STACK_CAP);
                    let more = history.stack_slice(self.base_seq, undo, stack.len() as u64, n).map_err(enc)?;
                    if more.len() as u64 != n {
                        return Err(SaveErrorV2::Encode("the history holds fewer undo steps than recorded".into()));
                    }
                    let mut all = more;
                    all.append(stack);
                    *stack = all;
                    *hidden -= n;
                    grew = true;
                }
                if !grew {
                    return Err(SaveErrorV2::Encode("the undo history is inconsistent".into()));
                }
            }
        }
        let (undo, redo) = self.history().stacks_at(at).map_err(enc)?;
        Ok(StackState { undo, redo, undo_hidden: 0, redo_hidden: 0 })
    }

    /// The tag high-water mark at `at`: from the one at the snapshot this project was opened from
    /// and the transactions after it in memory, or (for an earlier point) read from the file.
    fn tag_high_at(&self, ledger: &Ledger, at: u64) -> Result<HighWater, SaveErrorV2> {
        if at >= self.base_seq && self.base_seq >= ledger.base() {
            let mut high = self.base_tag_high.clone();
            for seq in self.base_seq + 1..=at {
                if let Some(tx) = ledger.transaction(seq) {
                    ecad_model::tag::note_ops(&mut high, &tx.ops);
                }
            }
            return Ok(high);
        }
        let r = self.store.reader()?;
        scan_tag_high(&r, self.branch, at, chain::seed(ChainKind::Project, self.project.0)).map_err(SaveErrorV2::Encode)
    }

    /// The tag high-water mark at the snapshot this project was opened from (brief 018: the next
    /// automatic number is the highest + 1); the transactions after it add to it.
    pub fn base_tag_high(&self) -> &HighWater {
        &self.base_tag_high
    }

    /// A reader of this file's history, for a ledger based on one of its snapshots.
    pub fn history(&self) -> Arc<dyn History> {
        Arc::new(StoreHistory {
            store: Arc::downgrade(&self.store),
            branch: self.branch,
            seed: chain::seed(ChainKind::Project, self.project.0),
        })
    }

    /// The database.
    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    /// The project ID.
    pub fn project_id(&self) -> ProjectId {
        self.project
    }

    /// The branch this project edits (`main`).
    pub fn branch(&self) -> BranchId {
        self.branch
    }

    /// The head in the file: `seq` and chain value.
    pub fn head(&self) -> (u64, Hash) {
        self.head
    }

    /// The `seq` of the latest snapshot.
    pub fn snapshot_seq(&self) -> u64 {
        self.snap.seq
    }

    /// Transactions between automatic snapshots.
    pub fn snapshot_every(&self) -> u64 {
        self.cache.every
    }

    /// Set the automatic snapshot interval (written with the next save).
    ///
    /// # Errors
    /// Out of range.
    pub fn set_snapshot_every(&mut self, every: u64) -> Result<(), String> {
        if !(SNAPSHOT_EVERY_RANGE.0..=SNAPSHOT_EVERY_RANGE.1).contains(&every) {
            return Err(format!(
                "the snapshot interval must be between {} and {}",
                SNAPSHOT_EVERY_RANGE.0, SNAPSHOT_EVERY_RANGE.1
            ));
        }
        self.cache.every = every;
        Ok(())
    }

    /// Why the project cannot be saved to this file, if it cannot.
    pub fn read_only(&self) -> Option<&str> {
        self.read_only.as_deref()
    }

    /// Make the project read-only (another session holds the file's lock).
    pub fn set_read_only(&mut self, why: String) {
        self.read_only = Some(why);
    }

    /// The version 1 history this file continues, if converted.
    pub fn link_v1(&self) -> Option<&LinkV1> {
        self.link_v1.as_ref()
    }

    /// When the file was created (informational).
    pub fn created_ms(&self) -> Option<u64> {
        self.created_ms
    }

    /// What the last save wrote.
    pub fn last_save(&self) -> Option<&SavedV2> {
        self.last_save.as_ref()
    }
}

/// Whether `tx` issues revision `name`: it sets some `rev.<n>.issued` to `name` (brief 019).
fn issues(tx: &Transaction, name: &str) -> bool {
    tx.ops.iter().any(|op| {
        matches!(op, Op::SetProperty { object: ObjectRef::Project, key, after: Some(v), .. }
            if key.starts_with("rev.") && key.ends_with(".issued") && v == name)
    })
}

/// The revision row (`rev.<n>.*`) whose `issued` is `name`, in `doc`, as release data.
fn revision_of(doc: &Document, name: &str) -> Option<Release> {
    let key = doc.properties.iter().find(|(k, v)| k.starts_with("rev.") && k.ends_with(".issued") && *v == name)?.0;
    let prefix = key.strip_suffix("issued")?;
    let fields = doc
        .properties
        .range(prefix.to_string()..)
        .take_while(|(k, _)| k.starts_with(prefix))
        .filter_map(|(k, v)| k.strip_prefix(prefix).filter(|f| *f != "issued").map(|f| (f.to_string(), v.clone())))
        .collect();
    Some(Release { fields, assets: Vec::new() })
}

/// The release data of revision `name` as issued by `tx`: the revision row's fields once `tx`
/// applied (fields are never changed after issuing, so the document after `tx` is read).
fn release_at(ledger: &mut Ledger, tx: &Transaction, name: &str) -> Result<Release, String> {
    if name.len() > MAX_NAME {
        return Err("a revision name is too long".into());
    }
    let undo_after: Vec<Op> = ledger
        .log()
        .iter()
        .rev()
        .take_while(|t| t.seq > tx.seq)
        .flat_map(|t| t.ops.iter().rev().map(Op::inverse))
        .collect();
    let release = if undo_after.is_empty() {
        revision_of(ledger.document(), name)
    } else {
        ledger.with_applied(&undo_after, |d| revision_of(d, name)).map_err(|e| e.to_string())?
    };
    Ok(release.unwrap_or_default())
}
