//! A project in a version 2 file: opening from the latest snapshot and its tail, and saving.
//!
//! - **Open** reads `meta`, the branch row and its head transaction, the latest snapshot's chunks
//!   (the whole active snapshot) and the transactions after it, each checked against its chain value and
//!   replayed with every check. It never reads other
//!   branches, older versions, transactions before the snapshot, or assets. Older transactions are
//!   read on demand through [`StoreHistory`] (undo past the snapshot, `state_at`).
//! - **Save** writes the pending transactions, the snapshots of any versions they name and of an
//!   automatic snapshot every `cache.every` transactions, the branch head and `meta` in one redb
//!   write transaction. Chunks are built incrementally from the last snapshot ([`chunks::Dirty`]).
//!   Nothing stored is read back to check it; its cost follows what is written.

use std::collections::BTreeMap;
use std::sync::Arc;

use ecad_model::{Base, Document, History, Ledger, ModelError, Op, PointName, ProjectId, Rev, Transaction, TxId, TxKind};
use rayon::prelude::*;
use serde_json::{Value, json};

use super::chunks::{self, ChunkSet, Dirty, ModelMeta};
use super::pack;
use super::schema::{
    BranchId, BranchRecord, BranchStatus, CODEC, CONTAINER_VERSION, Cache, FORMAT_PROJECT, MergeRow, Merged, Point, Release, RowKind,
    SNAPSHOT_EVERY_RANGE, STACK_CAP, SnapshotRef, VersionRecord, auto_name,
};
use super::store::{Reader, Store, StoreError};
use crate::FileError;
use crate::chain::{self, ChainKind, Hash};
use crate::record::{self, SCHEMA_VERSION};
use ecad_model::naming::tag::HighWater;

/// Why a version 2 file could not be opened at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpenError {
    #[error("{0}")]
    Store(#[from] StoreError),
    #[error("{0}")]
    Unsupported(String),
    /// The container facts or the branch are unusable.
    #[error("the file is damaged: {0}")]
    Damaged(FileError),
}

/// Why a save did not complete. The file then holds exactly what it held before, and the
/// project is still unsaved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SaveError {
    #[error("the file was opened read-only: {0}")]
    ReadOnly(String),
    #[error("the file was changed by something else ({0}); it was not modified; use Save As")]
    ExternalChange(String),
    #[error("{0}")]
    Store(#[from] StoreError),
    #[error("could not encode: {0}")]
    Encode(FileError),
}

/// Why a branch action (create, rename, archive, reopen, switch) did not happen. The file is then
/// unchanged.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BranchError {
    #[error("the file was opened read-only: {0}")]
    ReadOnly(String),
    /// Refused, in words for the user.
    #[error("{0}")]
    Refused(String),
    /// Reopening a branch whose name another open branch now has: rename it first.
    #[error("Another open branch is called \"{0}\"; give this one a new name.")]
    NameTaken(String),
    #[error("{0}")]
    Store(#[from] StoreError),
}

/// Longest branch name, in characters.
pub(crate) const MAX_BRANCH_NAME: usize = 60;

/// A branch name checked: trimmed, 1–60 characters, no control characters,
/// not `main`.
///
/// # Errors
/// [`BranchError::Refused`] in words.
pub fn check_branch_name(name: &str) -> Result<String, BranchError> {
    let n = name.trim();
    let len = n.chars().count();
    if len == 0 || len > MAX_BRANCH_NAME {
        return Err(BranchError::Refused(format!("A branch name is 1 to {MAX_BRANCH_NAME} characters.")));
    }
    if n.chars().any(char::is_control) {
        return Err(BranchError::Refused("A branch name cannot hold control characters.".into()));
    }
    if n.eq_ignore_ascii_case("main") {
        return Err(BranchError::Refused("\"main\" is the release line's name; choose another.".into()));
    }
    Ok(n.to_string())
}

/// What a successful save wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Saved {
    pub first_seq: u64,
    pub last_seq: u64,
    /// Bytes of new rows and chunks (before redb's own overhead).
    pub bytes: u64,
    pub chunks_written: usize,
    /// Snapshots written (versions and automatic).
    pub snapshots: usize,
}

/// What a check found wrong while opening: the project opens read-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Damage {
    /// The transaction (or snapshot point) that failed, when known.
    pub seq: Option<u64>,
    pub detail: String,
}

/// What opening found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
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
    pub damage: Option<Damage>,
    /// The file's `schema_version` (see [`format_note`]).
    pub schema_version: u64,
}

impl Report {
    /// One paragraph for the user.
    pub fn describe(&self) -> String {
        let mut s = format!("Opened: {} transactions (from the snapshot at {}, then {} replayed).", self.verified_seq, self.snapshot_seq, self.tail);
        if self.repaired {
            s.push_str(" The file was not closed cleanly last time and was recovered to its last save.");
        }
        if let Some(n) = format_note(self.schema_version) {
            s.push_str(&format!(" Opened read-only: {n}; it is kept exactly as it is. Use Save As to keep a copy."));
        } else if let Some(d) = &self.damage {
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
pub(crate) fn read_facts_pub(r: &Reader) -> Result<(ProjectId, Cache), OpenError> {
    read_facts(r).map(|f| (f.project, f.cache))
}

/// The project ID of a file, after checking its container facts as opening does.
///
/// # Errors
/// Not a version 2 project file this build reads, or its facts are damaged.
pub fn project_id(r: &Reader) -> Result<ProjectId, OpenError> {
    read_facts(r).map(|f| f.project)
}

pub(crate) use decode_row as decode_row_pub;

/// A transaction row decoded and checked: its record against its stored chain value, its `seq`,
/// and its `prev` against the previous row's chain value.
pub(crate) fn decode_row(seq: u64, flag: u8, stored_chain: &Hash, body: &[u8], prev: &Hash) -> Result<Transaction, FileError> {
    let record = pack::unpack(flag, body)?;
    if chain::tx_hash(&record) != *stored_chain {
        return Err(FileError::RecordChain);
    }
    let value: Value = serde_json::from_slice(&record).map_err(|e| FileError::NotARecord(e.to_string()))?;
    record::check_limits(&value)?;
    let (rseq, rprev) = record::seq_and_prev(&value);
    if rseq != Some(seq) {
        return Err(FileError::RowHolds { seq, held: rseq });
    }
    if rprev.and_then(chain::parse_hex) != Some(*prev) {
        return Err(FileError::PrevLink);
    }
    record::decode_tx(&value).map_err(FileError::Decode)
}

/// The canonical record and chain value of `tx` after `prev`.
fn encode_tx(tx: &Transaction, prev: &Hash) -> Result<(Vec<u8>, Hash), FileError> {
    let value = record::tx_value(tx, prev)?;
    let canonical = crate::canonical::canonical(&value)?;
    let hash = chain::tx_hash(&canonical);
    Ok((canonical, hash))
}

/// The undo and redo stacks at a point: their tops, and how many entries lie below what is held
/// (a snapshot records only the top [`STACK_CAP`] of each).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct StackState {
    undo: Vec<(u64, TxId)>,
    redo: Vec<(u64, TxId)>,
    undo_hidden: u64,
    redo_hidden: u64,
    /// The cap the record was written with (0: the stacks are whole): entries below are read
    /// this many at a time.
    cap: u64,
}

impl StackState {
    /// Advance over `txs` (only kinds and IDs are read). `false` when a step pops below what is
    /// held: the whole stacks must be read first.
    fn advance(&mut self, txs: &[Transaction]) -> bool {
        for tx in txs {
            match tx.kind {
                TxKind::Edit | TxKind::Merge { .. } => {
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
            (kept, hidden.saturating_add(cut as u64))
        };
        let ((undo, uh), (redo, rh)) = (top(&self.undo, self.undo_hidden), top(&self.redo, self.redo_hidden));
        (undo, redo, uh, rh)
    }

    /// From a version row.
    /// The cap is the record's own `stack_cap`, never this build's [`STACK_CAP`]: a record holds
    /// at most that many entries of each stack, and entries below them only with a cap.
    fn from_record(v: &VersionRecord) -> Result<Self, FileError> {
        let cap = v.stack_cap;
        if cap == 0 && (v.undo_hidden > 0 || v.redo_hidden > 0) {
            return Err(FileError::StackNoCap);
        }
        if cap > 0 && (v.undo.len() as u64 > cap || v.redo.len() as u64 > cap) {
            return Err(FileError::StackOverCap(cap));
        }
        Ok(Self {
            undo: stacks_from_record(&v.undo)?,
            redo: stacks_from_record(&v.redo)?,
            undo_hidden: v.undo_hidden,
            redo_hidden: v.redo_hidden,
            cap,
        })
    }
}

fn stacks_from_record(s: &[(u64, String)]) -> Result<Vec<(u64, TxId)>, FileError> {
    s.iter().map(|(q, id)| TxId::parse(id).map(|t| (*q, t)).ok_or_else(|| FileError::NotATxId(id.clone()))).collect()
}

/// One branch of a lineage: the branch, its chain seed (its chain value at `seq` 0) and, for a
/// branch other than `main`, the index of its parent in the lineage and the parent's `seq` it
/// forks after.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub branch: BranchId,
    pub seed: Hash,
    pub fork: Option<(usize, u64)>,
}

/// A branch and its ancestry up to `main`: element 0 is the branch itself, the last is `main`.
pub(crate) type Lineage = Arc<Vec<Line>>;

/// The most levels of branches on branches read (a crafted file cannot make it loop).
const MAX_DEPTH: usize = 64;

/// The lineage of `branch`, from its own row up through its parents to `main`.
///
/// # Errors
/// A missing or inconsistent row, a cycle, or more than [`MAX_DEPTH`] levels.
pub fn lineage(r: &Reader, project: ProjectId, branch: BranchId) -> Result<Lineage, FileError> {
    let main = BranchId::main_of(project.0);
    let mut out: Vec<Line> = Vec::new();
    let mut at = branch;
    loop {
        if out.len() >= MAX_DEPTH || out.iter().any(|l| l.branch == at) {
            return Err(FileError::LineageLoop);
        }
        let rec = r.branch(at)?.ok_or_else(|| FileError::BranchMissing(at.hex()))?;
        if at == main {
            if rec.parent.is_some() || rec.fork_point.is_some() {
                return Err(FileError::MainInconsistent);
            }
            out.push(Line { branch: at, seed: chain::seed(ChainKind::Project, project.0), fork: None });
            break;
        }
        let (Some(parent), Some(fork)) = (rec.parent, rec.fork_point.as_ref()) else {
            return Err(FileError::NoForkPoint(rec.name));
        };
        let fork_chain = fork.hash()?;
        let i = out.len();
        out.push(Line { branch: at, seed: chain::branch_seed(project.0, &at.0, &fork_chain), fork: Some((i + 1, fork.seq)) });
        at = parent;
    }
    // Each fork point must be the parent's stored row (its chain value), or the parent's seed.
    for i in 0..out.len() {
        if let Some(Line { fork: Some((pi, seq)), branch, .. }) = out.get(i).cloned() {
            let parent = out.get(pi).ok_or(FileError::LineageIndex)?;
            let want = r.branch(branch)?.and_then(|b| b.fork_point).map(|p| p.chain).unwrap_or_default();
            let have = if seq == 0 { Some(parent.seed) } else { r.tx(parent.branch, seq)?.map(|(_, c, _)| c) };
            if have.map(|h| chain::hex(&h)) != Some(want) {
                return Err(FileError::ForkMismatch { branch: branch.hex(), seq });
            }
        }
    }
    Ok(Arc::new(out))
}

/// The nearest snapshot of `branch` at or before `seq` (an automatic one or a named version).
fn nearest_snapshot(r: &Reader, branch: BranchId, seq: u64) -> Result<Option<VersionRecord>, FileError> {
    let mut best: Option<VersionRecord> = r.auto_at_or_before(branch, seq)?;
    for kind in [RowKind::VERSION, RowKind::REVISION] {
        for v in r.versions(branch, kind)? {
            if v.point.seq <= seq && best.as_ref().is_none_or(|b| b.point.seq < v.point.seq) {
                best = Some(v);
            }
        }
    }
    Ok(best)
}

/// The document and tag high-water mark of lineage entry `i` after its transaction `seq`: from
/// its nearest snapshot at or before `seq`, or else from its parent's state at the fork point (or
/// the empty project for `main`), then its own rows, each checked against its chain value.
///
/// # Errors
/// Missing or damaged rows or chunks, and replay failures.
pub fn line_state(r: &Reader, lineage: &[Line], i: usize, seq: u64) -> Result<(Document, HighWater), FileError> {
    let line = lineage.get(i).ok_or(FileError::LineageIndex)?;
    let (mut doc, mut high, from, prev) = match nearest_snapshot(r, line.branch, seq)? {
        Some(v) => {
            let (doc, meta) = load_snapshot(r, &v)?;
            // Cached marks of an older rule are recomputed from the history.
            let high = match meta {
                Some(m) if v.tag_marks_current() => m.tag_high,
                _ => line_high(r, lineage, i, v.point.seq)?,
            };
            (doc, high, v.point.seq, v.point.hash()?)
        }
        None => match line.fork {
            Some((pi, fseq)) => {
                let (doc, high) = line_state(r, lineage, pi, fseq)?;
                (doc, high, 0, line.seed)
            }
            None => (Document::default(), HighWater::new(), 0, line.seed),
        },
    };
    let mut from = from.saturating_add(1);
    let mut prev = prev;
    while from <= seq {
        let to = seq.min(from.saturating_add(4_095));
        let (txs, last, damage) = read_rows(r, line.branch, from, to, prev)?;
        if let Some(d) = damage {
            return Err(FileError::AtTx { seq: d.seq, detail: d.detail });
        }
        for tx in &txs {
            for op in &tx.ops {
                op.apply(&mut doc).map_err(|error| FileError::Replay { seq: tx.seq, error })?;
            }
            ecad_model::naming::tag::note_ops(&mut high, &tx.ops);
        }
        prev = last;
        let Some(next) = to.checked_add(1) else { break };
        from = next;
    }
    Ok((doc, high))
}

/// The tag high-water mark of lineage entry `i` after its transaction `seq`, read from the file
/// without rebuilding documents: the nearest snapshot's `model:meta` (or the parent's mark at the
/// fork), then the rows after it.
fn line_high(r: &Reader, lineage: &[Line], i: usize, seq: u64) -> Result<HighWater, FileError> {
    let line = lineage.get(i).ok_or(FileError::LineageIndex)?;
    let snap = nearest_snapshot(r, line.branch, seq)?;
    let meta = match &snap {
        // Marks of an older rule are not used: the rows from the branch's start are scanned.
        Some(v) if !v.tag_marks_current() => None,
        Some(v) => {
            let set = ChunkSet::from_record(&v.chunks)?;
            match set.chunks.get(chunks::META_KEY) {
                Some(h) => {
                    let body = r.chunk_body(h)?;
                    let m: ModelMeta = serde_json::from_slice(&body).map_err(|e| FileError::MetaChunk(e.to_string()))?;
                    Some((m.tag_high, v.point.seq, v.point.hash()?))
                }
                None => None,
            }
        }
        None => None,
    };
    let (high, from, prev) = match meta {
        Some(found) => found,
        None => match line.fork {
            Some((pi, fseq)) => (line_high(r, lineage, pi, fseq)?, 0, line.seed),
            None => (HighWater::new(), 0, line.seed),
        },
    };
    scan_tag_high(r, line.branch, high, from, prev, seq)
}

/// `high` advanced over rows `after + 1 ..= upto` of `branch` (`prev`: the chain value at `after`).
fn scan_tag_high(r: &Reader, branch: BranchId, mut high: HighWater, after: u64, seed: Hash, upto: u64) -> Result<HighWater, FileError> {
    let mut from = after.saturating_add(1);
    let mut prev = seed;
    while from <= upto {
        let to = upto.min(from.saturating_add(4_095));
        let (txs, last, damage) = read_rows(r, branch, from, to, prev)?;
        if let Some(d) = damage {
            return Err(FileError::AtTx { seq: d.seq, detail: d.detail });
        }
        for tx in &txs {
            ecad_model::naming::tag::note_ops(&mut high, &tx.ops);
        }
        prev = last;
        let Some(next) = to.checked_add(1) else { break };
        from = next;
    }
    Ok(high)
}

/// Read a snapshot's document and its `model:meta`: every chunk, each checked against its hash.
fn load_snapshot(r: &Reader, v: &VersionRecord) -> Result<(Document, Option<ModelMeta>), FileError> {
    let set = ChunkSet::from_record(&v.chunks)?;
    let mut stored = Vec::with_capacity(set.chunks.len());
    for (key, hash) in &set.chunks {
        let (flag, body) = r.chunk(hash)?.ok_or_else(|| FileError::ChunkMissing(chain::hex(hash)))?;
        stored.push((key.clone(), *hash, flag, body));
    }
    chunks::assemble_stored(&set, stored)
}

/// Rows `from..=to` of `branch`, decoded and chain-checked from `prev`. Stops at the first bad
/// row: the good prefix, and the failure.
fn read_rows(r: &Reader, branch: BranchId, from: u64, to: u64, mut prev: Hash) -> Result<(Vec<Transaction>, Hash, Option<Damage>), StoreError> {
    let rows = r.tx_range(branch, from..=to)?;
    let mut out = Vec::with_capacity(rows.len());
    // `None` once past u64::MAX: no row can follow.
    let mut expected = Some(from);
    for (seq, flag, stored, body) in rows {
        if Some(seq) != expected {
            return Ok((out, prev, Some(Damage { seq: expected, detail: "the transaction is missing".into() })));
        }
        match decode_row(seq, flag, &stored, &body, &prev) {
            Ok(tx) => {
                out.push(tx);
                prev = stored;
            }
            Err(e) => return Ok((out, prev, Some(Damage { seq: Some(seq), detail: e.to_string() }))),
        }
        expected = seq.checked_add(1);
    }
    if let Some(e) = expected
        && e <= to
    {
        return Ok((out, prev, Some(Damage { seq: Some(e), detail: "the transaction is missing".into() })));
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
    /// The branch and its ancestry: a state before the branch's own first snapshot starts from
    /// its parent's state at the fork point.
    lineage: Lineage,
}

impl StoreHistory {
    fn read(&self) -> ecad_model::Result<Reader> {
        let store = self.store.upgrade().ok_or_else(|| ModelError::CorruptLog("the project's file is closed".into()))?;
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
        decode_row(seq, flag, &chain, &body, &prev).map_err(|e| ModelError::CorruptLog(format!("transaction {seq}: {e}")))
    }

    fn transactions(&self, from: u64, to: u64) -> ecad_model::Result<Vec<Transaction>> {
        let r = self.read()?;
        let prev = self.prev_of(&r, from)?;
        let (txs, _, damage) = read_rows(&r, self.branch, from, to, prev).map_err(|e| ModelError::CorruptLog(e.to_string()))?;
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
            kind: crate::records::TxKind,
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
            if rows.len() as u64 != (hi - lo).saturating_add(1) {
                return Err(bad(format!("transactions {lo} to {hi} are not all stored")));
            }
            let mut heads = Vec::with_capacity(rows.len());
            let mut last_chain = None;
            for (q, flag, chain_value, body) in rows {
                let record = pack::unpack(flag, &body).map_err(|e| bad(e.to_string()))?;
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
                use crate::records::TxKind as K;
                let (push, pop, clears) = match (h.kind, undo) {
                    (K::Edit | K::Merge(_), true) | (K::Redo(_), true) => (true, false, false),
                    (K::Undo(_), true) => (false, true, false),
                    (K::Undo(_), false) => (true, false, false),
                    (K::Redo(_), false) => (false, true, false),
                    (K::Edit | K::Merge(_), false) => (false, false, true),
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
            kind: crate::records::TxKind,
            prev: String,
        }
        /// A range's rows, checked on their own: each row against its chain value and the one
        /// before it; the first row's `prev` and the last row's chain value link the ranges.
        struct Range {
            first_prev: Hash,
            last: Hash,
            rows: Vec<(u64, TxId, crate::records::TxKind)>,
        }
        let bad = |m: String| ModelError::CorruptLog(m);
        let store = self.store.upgrade().ok_or_else(|| bad("the project's file is closed".into()))?;
        let branch = self.branch;
        let read_range = |(from, to): (u64, u64)| -> Result<Range, FileError> {
            let r = store.reader()?;
            let stored = r.tx_range(branch, from..=to)?;
            if stored.len() as u64 != to.saturating_sub(from).saturating_add(1) {
                return Err(FileError::NotAllStored { from, to });
            }
            let mut out = Range { first_prev: [0; 32], last: [0; 32], rows: Vec::with_capacity(stored.len()) };
            for (i, (q, flag, chain_value, body)) in stored.into_iter().enumerate() {
                let record = pack::unpack(flag, &body)?;
                if chain::tx_hash(&record) != chain_value {
                    return Err(FileError::TxChain(q));
                }
                let h: Head = serde_json::from_slice(&record).map_err(|e| FileError::TxJson { seq: q, error: e.to_string() })?;
                let prev = chain::parse_hex(&h.prev).ok_or(FileError::TxPrev(q))?;
                if h.seq != q || (i > 0 && prev != out.last) {
                    return Err(FileError::TxNotFollowing(q));
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
                let to = seq.min(from.saturating_add(4_095));
                ranges.push((from, to));
                let Some(next) = to.checked_add(1) else { break };
                from = next;
            }
            // One rayon task per range; the results keep the ranges' order.
            let read: Vec<Result<Range, FileError>> = ranges.into_par_iter().map(read_range).collect();
            for range in read {
                let range = range.map_err(|e| bad(e.to_string()))?;
                if range.first_prev != prev {
                    return Err(bad("the history's chain is broken between two ranges".into()));
                }
                prev = range.last;
                for (q, id, kind) in range.rows {
                    match kind {
                        crate::records::TxKind::Edit | crate::records::TxKind::Merge(_) => {
                            undo.push((q, id));
                            redo.clear();
                        }
                        crate::records::TxKind::Undo(of) => {
                            if undo.pop().map(|t| t.1) != Some(of) {
                                return Err(bad(format!("undo {q} does not undo the latest step")));
                            }
                            redo.push((q, id));
                        }
                        crate::records::TxKind::Redo(of) => {
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
        let (doc, _) = line_state(&r, &self.lineage, 0, seq).map_err(|e| ModelError::CorruptLog(e.to_string()))?;
        doc.check()?;
        Ok(doc)
    }
}

/// The in-memory snapshot the next one is built from. `set` is `None` for a branch opened from
/// its parent's state (it has no snapshot of its own yet): its first snapshot is built whole.
#[derive(Debug, Clone)]
struct Snap {
    seq: u64,
    set: Option<ChunkSet>,
}

/// A project bound to a version 2 database.
#[derive(Debug)]
pub struct Project {
    store: Arc<Store>,
    project: ProjectId,
    branch: BranchId,
    /// The branch's row as last read or written (its head is written by each save).
    rec: BranchRecord,
    /// The branch and its ancestry.
    lineage: Lineage,
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
    /// Why the file is read-only, apart from damage on the branch shown (kept across a switch).
    file_read_only: Option<String>,
    created_ms: Option<u64>,
    last_save: Option<Saved>,
    /// `meta.writer` names this build already (written by the first save of a session).
    wrote_writer: bool,
    /// Merges committed this session that close their source branch when saved: the merge's ID and
    /// the source.
    closing: BTreeMap<TxId, BranchId>,
    /// Sources reopened by an undo of a merge, by the branch and the undo's `seq`: a redo of
    /// that undo closes them again. Kept for the session.
    reopened: BTreeMap<(BranchId, u64), BranchId>,
}

/// A live merge into a branch (the merge index): its step and what it recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeIndex {
    /// The step that made it live: the merge, or the redo of its undo.
    pub seq: u64,
    pub source: BranchId,
    pub source_head: (u64, Hash),
    pub ancestor: (BranchId, u64, Hash),
}

impl MergeIndex {
    fn of(seq: u64, row: &MergeRow) -> Result<Self, FileError> {
        Ok(Self {
            seq,
            source: row.source_id().ok_or(FileError::MergeNoSource)?,
            source_head: (row.source_head.seq, row.source_head.hash()?),
            ancestor: (
                row.ancestor_id().ok_or(FileError::MergeNoAncestor)?,
                row.ancestor.seq,
                chain::parse_hex(&row.ancestor.chain).ok_or(FileError::MergeAncestorChain)?,
            ),
        })
    }
}

/// What following merges, their undos and redos over some steps gives (shared by Save, which
/// writes it, and [`Project::merge_index`], which only reads).
#[derive(Debug, Default)]
struct MergeFollow {
    /// Rows to write (`Some`) or remove (`None`), by `seq`.
    rows: BTreeMap<u64, Option<MergeRow>>,
    /// Source branches to close (merged into this branch at `seq`) or reopen (`None`).
    status: Vec<(BranchId, Option<u64>)>,
    /// Sources reopened by an undo, by the undo's `seq`.
    reopened: BTreeMap<u64, BranchId>,
}

/// A project opened from a version 2 database.
#[derive(Debug)]
pub struct Opened {
    pub project: Project,
    pub ledger: Ledger,
    pub report: Report,
    /// Words this build does not read in the file's container facts: each `meta` name not in
    /// [`META_NAMES`](crate::schema::META_NAMES). When there are any the project opens read-only
    /// (pre-v1 strict reading) and the file is kept as it is.
    pub unknown_words: Vec<ecad_model::document::strict::UnknownWord>,
}

fn meta_json(r: &Reader, name: &'static str) -> Result<Option<Value>, OpenError> {
    match r.meta(name)? {
        None => Ok(None),
        Some(b) => serde_json::from_slice(&b).map(Some).map_err(|e| OpenError::Damaged(FileError::Meta { name, error: e.to_string() })),
    }
}

/// The container facts, checked as a container header is checked.
struct Facts {
    project: ProjectId,
    cache: Cache,
    created_ms: Option<u64>,
    /// `meta.active`: the branch the project was last shown on (`None`: not recorded or not read).
    active: Option<BranchId>,
    /// Why `meta.active` does not read, when it is recorded but unreadable.
    active_damage: Option<String>,
    /// The `meta` names this build does not read.
    unknown: Vec<ecad_model::document::strict::UnknownWord>,
    /// The file's `schema_version`; not [`SCHEMA_VERSION`] means it opens read-only
    /// ([`format_note`]).
    schema_version: u64,
}

/// `older format version (file 1, this build 2)` / `newer ...`, when `file` is not the build's
/// [`SCHEMA_VERSION`]. Such a file is opened read-only whatever replay found, and never called
/// damaged: its records may mean something else here.
pub(crate) fn format_note(file: u64) -> Option<String> {
    match file.cmp(&SCHEMA_VERSION) {
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Less => Some(format!("older format version (file {file}, this build {SCHEMA_VERSION})")),
        std::cmp::Ordering::Greater => Some(format!("newer format version (file {file}, this build {SCHEMA_VERSION})")),
    }
}

fn read_facts(r: &Reader) -> Result<Facts, OpenError> {
    // Pre-v1 there is no forward compatibility: a `meta` name this build does not
    // read is an unknown word, so the project opens read-only and names it. Every name this build
    // needs is checked below.
    let unknown = r
        .meta_all()?
        .into_iter()
        .filter(|(name, _)| !crate::schema::META_NAMES.contains(&name.as_str()))
        .map(|(name, _)| ecad_model::document::strict::UnknownWord::MetaName { name })
        .collect();
    let format = meta_json(r, "format")?;
    if format.as_ref().and_then(Value::as_str) != Some(FORMAT_PROJECT) {
        return Err(OpenError::Unsupported(format!("not an ECAD project file (format {format:?})")));
    }
    let cv = meta_json(r, "container_version")?.and_then(|v| v.as_u64());
    if cv != Some(CONTAINER_VERSION) {
        return Err(OpenError::Unsupported(format!("container version {cv:?} is not supported (this build reads {CONTAINER_VERSION})")));
    }
    let sv = meta_json(r, "schema_version")?.and_then(|v| v.as_u64()).unwrap_or(0);
    if sv == 0 {
        return Err(OpenError::Unsupported("the file has no schema version".into()));
    }
    let codec = meta_json(r, "codec")?;
    if codec.as_ref().and_then(Value::as_str) != Some(CODEC) {
        return Err(OpenError::Unsupported(format!("codec {codec:?} is not supported (this build reads {CODEC:?})")));
    }
    let project = meta_json(r, "file_id")?.and_then(|v| v.as_str().and_then(ProjectId::parse)).ok_or(OpenError::Damaged(FileError::FileId))?;
    let cache: Cache = match meta_json(r, "cache")? {
        Some(v) => serde_json::from_value(v).map_err(|e| OpenError::Damaged(FileError::Meta { name: "cache", error: e.to_string() }))?,
        None => Cache::default(),
    };
    if !(SNAPSHOT_EVERY_RANGE.0..=SNAPSHOT_EVERY_RANGE.1).contains(&cache.every) {
        return Err(OpenError::Damaged(FileError::CacheEvery(cache.every)));
    }
    let created_ms = meta_json(r, "created_ms")?.and_then(|v| v.as_u64());
    // An `active` that does not read is damage, reported when the project opens (on `main`).
    let (active, active_damage) = match meta_json(r, "active") {
        Ok(None) => (None, None),
        Ok(Some(v)) => match v.as_str().and_then(BranchId::parse) {
            Some(b) => (Some(b), None),
            None => (None, Some(format!("`meta.active` is not a branch ID ({v})"))),
        },
        Err(e) => (None, Some(format!("`meta.active` does not read ({e})"))),
    };
    Ok(Facts { project, cache, created_ms, active, active_damage, unknown, schema_version: sv })
}

/// [`main_branch`], for Verify: the check that opening applies.
pub(crate) fn main_branch_pub(r: &Reader, project: ProjectId) -> Result<(), OpenError> {
    main_branch(r, project).map(|_| ())
}

/// The original branch, `main`.
fn main_branch(r: &Reader, project: ProjectId) -> Result<(BranchId, BranchRecord), OpenError> {
    let id = BranchId::main_of(project.0);
    let rec = r.branch(id)?.ok_or(OpenError::Damaged(FileError::NoMain))?;
    if rec.name != "main" || rec.parent.is_some() || rec.fork_point.is_some() || rec.status != BranchStatus::Open {
        return Err(OpenError::Damaged(FileError::MainInconsistent));
    }
    Ok((id, rec))
}

/// What loading one branch gave.
struct Loaded {
    ledger: Ledger,
    report: Report,
    lineage: Lineage,
    head: (u64, Hash),
    snap: Snap,
    base_tag_high: HighWater,
    snap_ref: Option<SnapshotRef>,
    base_stacks: StackState,
    damage: Option<String>,
}

/// Load `branch`: its latest snapshot and the transactions after it; a
/// branch with no snapshot of its own starts from its parent's state at the fork point. Damage
/// found loads the last state that verifies, with a report.
fn load_branch(store: &Arc<Store>, project: ProjectId, cache: &Cache, branch: BranchId, rec: &BranchRecord, seed: u64) -> Result<Loaded, OpenError> {
    let r = store.reader()?;
    let lineage = lineage(&r, project, branch).map_err(OpenError::Damaged)?;
    let own = lineage.first().cloned().ok_or(OpenError::Damaged(FileError::EmptyLineage))?;
    let seed_chain = own.seed;
    let head = rec.head.seq;
    let head_chain = rec.head.hash().map_err(OpenError::Damaged)?;
    let mut damage = None;
    // The head must be the branch's last row, with the recorded chain value.
    let last = r.last_seq(branch)?.unwrap_or(0);
    if last != head {
        damage = Some(Damage { seq: Some(head), detail: format!("the head is {head} but the last stored transaction is {last}") });
    } else if head > 0 && r.tx(branch, head)?.map(|(_, c, _)| c) != Some(head_chain) {
        damage = Some(Damage { seq: Some(head), detail: "the head transaction does not match the branch head".into() });
    } else if head == 0 && head_chain != seed_chain {
        damage = Some(Damage { seq: Some(0), detail: "the branch head does not match its fork point".into() });
    }
    // The latest snapshot at or before the head.
    let latest = cache.latest.get(&branch.hex()).filter(|s| s.seq <= head);
    let mut loaded = None;
    let mut snap_ref = None;
    if let Some(s) = latest {
        match r.version(branch, s.kind, &s.name)? {
            Some(v) if v.point.seq == s.seq => match load_snapshot(&r, &v) {
                Ok((doc, meta)) => {
                    // Every invariant of the snapshot is checked in debug builds and after a
                    // crash repair (about 38 ms on the large project, so not on every normal
                    // release open). Its chunks were checked against their hashes in any case,
                    // and Verify checks everything on request.
                    let checked = if cfg!(debug_assertions) || store.repaired() { doc.check() } else { Ok(()) };
                    match checked {
                        Ok(()) => {
                            loaded = Some((v, doc, meta));
                            snap_ref = Some(s.clone());
                        }
                        Err(e) => damage = damage.or(Some(Damage { seq: Some(s.seq), detail: format!("snapshot breaks an invariant: {e}") })),
                    }
                }
                Err(e) => damage = damage.or(Some(Damage { seq: Some(s.seq), detail: format!("snapshot: {e}") })),
            },
            _ => damage = damage.or(Some(Damage { seq: Some(s.seq), detail: "the latest snapshot is missing".into() })),
        }
    }
    // Start from the snapshot; if anything about it fails (its point row, its stack record,
    // the rows under it), open from the branch's start instead (the project root, or the
    // parent's state at the fork point), read-only with the damage reported.
    let to = head.min(last);
    let (ledger, failure, snap_seq, snap_chain, stacks, set, base_tag_high, tail_len, verified_chain, row_damage) = loop {
        let using = loaded.take();
        let from_snapshot = using.as_ref().map(|(v, _, _)| v.point.seq);
        let attempt = (|| -> Result<_, FileError> {
            let (snap_seq, snap_chain, doc, stacks, set, high) = match using {
                Some((v, doc, meta)) => {
                    let chain = v.point.hash()?;
                    let stacks = StackState::from_record(&v)?;
                    let set = ChunkSet::from_record(&v.chunks)?;
                    // The snapshot's point must be the stored row's chain value.
                    if r.tx(branch, v.point.seq)?.map(|(_, c, _)| c) != Some(chain) {
                        return Err(FileError::SnapshotPoint);
                    }
                    // The tag high-water mark at the snapshot: carried by `model:meta`; a
                    // snapshot written without it is scanned once (a cache, rebuilt from the
                    // history).
                    // Marks of an older rule (written before cable tags were counted) are
                    // recomputed once, the same way.
                    let high = match meta {
                        Some(m) if v.tag_marks_current() => m.tag_high,
                        _ => line_high(&r, &lineage, 0, v.point.seq)?,
                    };
                    (v.point.seq, chain, doc, stacks, Some(set), high)
                }
                None => match own.fork {
                    // A branch without a snapshot of its own: its parent's state at the fork.
                    Some((pi, fseq)) => {
                        let (doc, high) = line_state(&r, &lineage, pi, fseq)?;
                        (0, seed_chain, doc, StackState::default(), None, high)
                    }
                    None => (0, seed_chain, Document::default(), StackState::default(), Some(ChunkSet::default()), HighWater::new()),
                },
            };
            let (tail, verified_chain, row_damage) = read_rows(&r, branch, snap_seq.saturating_add(1), to, snap_chain)?;
            let history: Arc<dyn History> =
                Arc::new(StoreHistory { store: Arc::downgrade(store), branch, seed: seed_chain, lineage: lineage.clone() });
            let base = Base {
                seq: snap_seq,
                doc,
                undo: stacks.undo.clone(),
                redo: stacks.redo.clone(),
                undo_hidden: stacks.undo_hidden,
                redo_hidden: stacks.redo_hidden,
                stack_cap: stacks.cap,
                history,
            };
            let tail_len = tail.len() as u64;
            let (ledger, failure) = Ledger::from_base(base, tail, seed).map_err(|error| FileError::SnapshotAt { seq: snap_seq, error })?;
            Ok((ledger, failure, snap_seq, snap_chain, stacks, set, high, tail_len, verified_chain, row_damage))
        })();
        match (attempt, from_snapshot) {
            (Ok(done), _) => break done,
            (Err(e), Some(seq)) => {
                damage = damage.or(Some(Damage { seq: Some(seq), detail: format!("snapshot: {e}") }));
                snap_ref = None;
            }
            (Err(e), None) => return Err(OpenError::Damaged(e)),
        }
    };
    drop(r);
    let damage = damage.or(row_damage);
    let damage =
        damage.or(failure.map(|(i, e)| Damage { seq: snap_seq.checked_add(i as u64).and_then(|n| n.checked_add(1)), detail: e.to_string() }));
    let verified = ledger.head();
    let verified_chain = if verified == snap_seq + tail_len {
        verified_chain
    } else {
        // Replay stopped early: the chain value of the last good row.
        store.reader()?.tx(branch, verified)?.map(|(_, c, _)| c).unwrap_or(snap_chain)
    };
    let damage_text = damage.as_ref().map(|d| format!("the file is damaged ({})", d.detail));
    let report = Report {
        head,
        verified_seq: verified,
        snapshot_seq: snap_seq,
        tail: verified - snap_seq,
        repaired: store.repaired(),
        damage,
        schema_version: 0,
    };
    Ok(Loaded {
        ledger,
        report,
        lineage,
        head: (verified, verified_chain),
        snap: Snap { seq: snap_seq, set },
        base_tag_high,
        snap_ref,
        base_stacks: stacks,
        damage: damage_text,
    })
}

impl Project {
    /// Open the project in `store` on the branch it was last shown on
    /// (`meta.active`), or `main` when that is absent, unknown or not open. `seed` seeds IDs
    /// allocated after opening. Damage found while loading opens read-only, at the last state that
    /// verifies, with a report; so does a `meta.active` that does not read (on `main`).
    ///
    /// # Errors
    /// [`OpenError`] when the container facts or the branch row are unusable.
    pub fn open(store: Store, seed: u64, writer: &str) -> Result<Opened, OpenError> {
        let store = Arc::new(store);
        let r = store.reader()?;
        let facts = read_facts(&r)?;
        let (main, main_rec) = main_branch(&r, facts.project)?;
        let (branch, rec) = match facts.active.filter(|a| *a != main) {
            Some(a) => match r.branch(a)? {
                Some(rec) if rec.status == BranchStatus::Open => (a, rec),
                _ => (main, main_rec),
            },
            None => (main, main_rec),
        };
        drop(r);
        let note = format_note(facts.schema_version);
        let mut l = match load_branch(&store, facts.project, &facts.cache, branch, &rec, seed) {
            Ok(l) => l,
            // A file of another format that cannot be read at all is refused in the same words.
            Err(e) => match &note {
                Some(n) => return Err(OpenError::Unsupported(format!("{n}: {e}"))),
                None => return Err(e),
            },
        };
        l.report.schema_version = facts.schema_version;
        if let Some(detail) = facts.active_damage
            && l.damage.is_none()
        {
            l.damage = Some(format!("the file is damaged ({detail})"));
            l.report.damage = Some(Damage { seq: None, detail });
        }
        // Unknown `meta` names keep the file exactly as it is: nothing writes to it, not even a
        // branch switch's `meta.active`.
        let unknown_words = facts.unknown;
        let unknown_read_only = (!unknown_words.is_empty()).then(|| crate::disk::ReadOnly::UnknownWords(unknown_words.clone()).to_string());
        let file_read_only = (!store.writable()).then(|| "it is open in another session".to_string()).or(unknown_read_only);
        // Another format version is read-only whatever replay found: the version comes first, and
        // nothing writes to the file, not even a branch switch's `meta.active`.
        let file_read_only = note.clone().or(file_read_only);
        let read_only = note.or_else(|| l.damage.clone()).or_else(|| file_read_only.clone());
        let project = Project {
            store,
            project: facts.project,
            branch,
            rec,
            lineage: l.lineage,
            head: l.head,
            cache: facts.cache,
            snap: l.snap,
            base_tag_high: l.base_tag_high,
            snap_ref: l.snap_ref,
            base_stacks: l.base_stacks,
            base_seq: l.report.snapshot_seq,
            writer: writer.to_string(),
            read_only,
            file_read_only,
            created_ms: facts.created_ms,
            last_save: None,
            wrote_writer: false,
            closing: BTreeMap::new(),
            reopened: BTreeMap::new(),
        };
        Ok(Opened { project, ledger: l.ledger, report: l.report, unknown_words })
    }

    /// Write `ledger`'s whole history into the empty database `store` as a new project file:
    /// `main` from the project root, a snapshot for every named version, and a snapshot at the
    /// head.
    ///
    /// # Errors
    /// [`SaveError`]; nothing is committed. `every` outside [`SNAPSHOT_EVERY_RANGE`] is refused
    /// (an open would refuse the file).
    pub fn create(
        store: Store, project: ProjectId, ledger: &Ledger, created_ms: Option<u64>, writer: &str, every: u64,
    ) -> Result<(Project, Saved), SaveError> {
        if ledger.base() != 0 {
            return Err(SaveError::Encode(FileError::WholeHistory));
        }
        if !(SNAPSHOT_EVERY_RANGE.0..=SNAPSHOT_EVERY_RANGE.1).contains(&every) {
            return Err(SaveError::Encode(FileError::SnapshotEvery));
        }
        let store = Arc::new(store);
        let branch = BranchId::main_of(project.0);
        let seed = chain::seed(ChainKind::Project, project.0);
        let mut w = store.writer()?;
        let mut saved = Saved { first_seq: 1, last_seq: ledger.head(), ..Saved::default() };
        // Rows, and the chain value of every point.
        let mut chains = Vec::with_capacity(ledger.log().len() + 1);
        chains.push(seed);
        let mut prev = seed;
        for tx in ledger.log() {
            let (bytes, hash) = encode_tx(tx, &prev).map_err(SaveError::Encode)?;
            saved.bytes += w.append_tx(branch, tx.seq, &hash, &bytes)?;
            chains.push(hash);
            prev = hash;
        }
        // Version snapshots, then the head: one forward pass over the log.
        let doc_end = ledger.document();
        let mut points: Vec<(u64, RowKind, String)> = Vec::new();
        for (name, at) in doc_end.versions() {
            points.push((*at, RowKind::of(name), name.key()));
        }
        points.push((ledger.head(), RowKind::Auto, auto_name(ledger.head())));
        points.sort();
        let mut doc = Document::default();
        let mut stacks = StackState::default();
        let mut done = 0u64;
        let mut snap = (0u64, ChunkSet::default());
        let mut first = true;
        let mut latest: Option<SnapshotRef> = None;
        let mut high = HighWater::new();
        for (at, kind, name) in points {
            let range = ledger.log().get(done as usize..at as usize).unwrap_or_default();
            let mut dirty = Dirty::default();
            for tx in range {
                for op in &tx.ops {
                    op.apply(&mut doc).map_err(|error| SaveError::Encode(FileError::Replay { seq: tx.seq, error }))?;
                }
                dirty.add_ops(&tx.ops);
                ecad_model::naming::tag::note_ops(&mut high, &tx.ops);
            }
            stacks.advance(range);
            done = at;
            let meta = ModelMeta { tag_high: high.clone() };
            let (set, new) = if first { chunks::build_full(&doc, &meta) } else { chunks::build_incremental(&doc, &snap.1, &dirty, &meta) }
                .map_err(SaveError::Encode)?;
            first = false;
            for p in &new {
                let n = w.put_chunk(&p.hash, &p.body)?;
                saved.bytes += n;
                saved.chunks_written += usize::from(n > 0);
            }
            let chain = chains.get(at as usize).copied().unwrap_or(seed);
            let release = Rev::from_key(&name).filter(|_| kind == RowKind::REVISION).and_then(|rev| revision_of(doc_end, rev));
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
                    description: None,
                    merge: None,
                    tag_rule: Some(ecad_model::naming::tag::TAG_RULE),
                },
            )?;
            saved.snapshots += 1;
            latest = Some(SnapshotRef { kind, name, seq: at });
            snap = (at, set);
        }
        let mut cache = Cache { every, ..Cache::default() };
        if let Some(l) = &latest {
            cache.latest.insert(branch.hex(), l.clone());
        }
        let head = (ledger.head(), prev);
        let rec = BranchRecord::main(Point::new(head.0, &head.1));
        w.put_branch(branch, &rec)?;
        w.set_meta("format", &json!(FORMAT_PROJECT))?;
        w.set_meta("container_version", &json!(CONTAINER_VERSION))?;
        w.set_meta("schema_version", &json!(SCHEMA_VERSION))?;
        w.set_meta("codec", &json!(CODEC))?;
        w.set_meta("file_id", &json!(project.hex()))?;
        w.set_meta("writer", &json!(writer))?;
        w.set_meta("created_ms", &json!(created_ms))?;
        w.set_meta("cache", &serde_json::to_value(&cache).map_err(|e| SaveError::Encode(FileError::json(&e)))?)?;
        w.commit()?;
        let lineage = Arc::new(vec![Line { branch, seed, fork: None }]);
        let me = Project {
            store,
            project,
            branch,
            rec,
            lineage,
            head,
            cache,
            snap: Snap { seq: snap.0, set: Some(snap.1) },
            base_tag_high: HighWater::new(),
            snap_ref: latest,
            base_stacks: StackState::default(),
            base_seq: 0,
            writer: writer.to_string(),
            read_only: None,
            file_read_only: None,
            created_ms,
            last_save: Some(saved.clone()),
            wrote_writer: true,
            closing: BTreeMap::new(),
            reopened: BTreeMap::new(),
        };
        Ok((me, saved))
    }

    /// Save the transactions after the file's head, with the snapshots of any
    /// versions they name and an automatic snapshot when due, in one write transaction. On
    /// success the ledger is marked persisted.
    ///
    /// # Errors
    /// [`SaveError`]; the file and the ledger are then unchanged.
    pub fn save(&mut self, ledger: &mut Ledger) -> Result<Saved, SaveError> {
        if let Some(r) = &self.read_only {
            return Err(SaveError::ReadOnly(r.clone()));
        }
        let (head, mut prev) = self.head;
        if ledger.head() == head {
            if ledger.persisted() != head {
                ledger.mark_persisted(head).map_err(|e| SaveError::Encode(e.into()))?;
            }
            return Ok(Saved { first_seq: head.saturating_add(1), last_seq: head, ..Saved::default() });
        }
        if ledger.base() > head || ledger.head() < head {
            return Err(SaveError::Encode(FileError::NotContinuing { base: ledger.base(), head: ledger.head(), file: head }));
        }
        let start = usize::try_from(head - ledger.base()).map_err(|e| SaveError::Encode(e.into()))?;
        let pending: Vec<Transaction> = ledger.log().get(start..).unwrap_or_default().to_vec();
        let mut w = self.store.writer()?;
        // Another writer cannot exist (locks); check anyway, inside this write transaction: one
        // row read.
        {
            match w.branch(self.branch)? {
                Some(b) if b.head.seq == head && b.head.hash().ok() == Some(prev) => {}
                Some(b) => return Err(SaveError::ExternalChange(format!("the branch head is now {}", b.head.seq))),
                None => return Err(SaveError::ExternalChange("the branch is gone".into())),
            }
        }
        let mut saved = Saved { first_seq: head.saturating_add(1), last_seq: ledger.head(), ..Saved::default() };
        let mut chains: BTreeMap<u64, Hash> = BTreeMap::new();
        chains.insert(head, prev);
        for tx in &pending {
            let (bytes, hash) = encode_tx(tx, &prev).map_err(SaveError::Encode)?;
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
                    Op::NameVersion { name, at, description } => {
                        let (kind, key) = (RowKind::of(name), name.key());
                        let release = match name {
                            PointName::Revision(rev) => Some(release_at(ledger, tx, *rev).map_err(SaveError::Encode)?),
                            PointName::Version(_) => None,
                        };
                        let chain = self.chain_at(*at, &chains)?;
                        self.write_snapshot(&mut w, ledger, &mut snap, &mut cache, *at, chain, kind, &key, release, &mut saved)?;
                        if let Some(d) = description {
                            w.describe_version(self.branch, kind, &key, d)?;
                        }
                    }
                    Op::UnnameVersion { name, .. } => {
                        w.remove_version(self.branch, RowKind::of(name), &name.key())?;
                    }
                    _ => {}
                }
            }
        }
        // The merge index and the sources' statuses follow merges, their undos and redos.
        let follow = self.follow_merges(ledger, &pending, Some(&w))?;
        for (seq, row) in &follow.rows {
            match row {
                Some(row) => w.put_version(self.branch, &merge_record(*seq, self.chain_at(*seq, &chains)?, row))?,
                None => w.remove_version(self.branch, RowKind::Merge, &auto_name(*seq))?,
            }
        }
        for (source, merged) in &follow.status {
            // The release line (or any branch without a parent) is never closed by a merge.
            if let Some(mut b) = w.branch(*source)?.filter(|b| !b.release_line && b.parent.is_some()) {
                match merged {
                    Some(seq) if b.status == BranchStatus::Open => {
                        b.status = BranchStatus::Merged;
                        b.merged = Some(Merged { into: self.branch.hex(), seq: *seq });
                    }
                    None if b.status == BranchStatus::Merged => {
                        b.status = BranchStatus::Open;
                        b.merged = None;
                    }
                    _ => continue,
                }
                w.put_branch(*source, &b)?;
            }
        }
        let end = ledger.head();
        // A branch's first save writes its first snapshot: until then it opens from its
        // parent's state at the fork, which costs more than an open.
        if end.saturating_sub(snap.seq) >= cache.every || (snap.set.is_none() && end > snap.seq) {
            let chain = prev;
            self.write_snapshot(&mut w, ledger, &mut snap, &mut cache, end, chain, RowKind::Auto, &auto_name(end), None, &mut saved)?;
        }
        // The row as stored now (a rename or archive may have changed it since it was read), with
        // the new head.
        let mut rec = w.branch(self.branch)?.unwrap_or_else(|| self.rec.clone());
        rec.head = Point::new(end, &prev);
        w.put_branch(self.branch, &rec)?;
        if !self.wrote_writer {
            w.set_meta("writer", &json!(self.writer))?;
        }
        if cache != self.cache {
            w.set_meta("cache", &serde_json::to_value(&cache).map_err(|e| SaveError::Encode(FileError::json(&e)))?)?;
        }
        w.commit()?;
        self.rec = rec;
        self.head = (end, prev);
        self.snap = snap;
        self.snap_ref = cache.latest.get(&self.branch.hex()).cloned();
        self.cache = cache;
        ledger.mark_persisted(end).map_err(|e| SaveError::Encode(e.into()))?;
        for tx in &pending {
            self.closing.remove(&tx.id);
        }
        for (u, source) in follow.reopened {
            self.reopened.insert((self.branch, u), source);
        }
        self.last_save = Some(saved.clone());
        self.wrote_writer = true;
        Ok(saved)
    }

    /// Follow merges, their undos and redos over `steps` (the branch shown's steps after the
    /// file's head): which merge index rows to write or remove, and which sources to close or
    /// reopen. Only the steps' kinds are read, plus the undo and redo stacks when the branch has
    /// merges at all (in the file, or in `steps`) and a step is an undo or redo. `w`: read the
    /// stored rows inside Save's write transaction.
    fn follow_merges(&self, ledger: &Ledger, steps: &[Transaction], w: Option<&super::store::Writer>) -> Result<MergeFollow, SaveError> {
        let mut out = MergeFollow::default();
        let any_merge = steps.iter().any(|t| matches!(t.kind, TxKind::Merge { .. }));
        let any_step = steps.iter().any(|t| matches!(t.kind, TxKind::Undo { .. } | TxKind::Redo { .. }));
        if !any_merge && !any_step {
            return Ok(out);
        }
        let rows = match w {
            Some(w) => w.versions(self.branch, RowKind::Merge)?,
            None => self.store.reader()?.versions(self.branch, RowKind::Merge)?,
        };
        let mut live: BTreeMap<u64, MergeRow> = rows.into_iter().filter_map(|v| v.merge.map(|m| (v.point.seq, m))).collect();
        let reopened_before: BTreeMap<u64, BranchId> =
            self.reopened.iter().filter(|((b, _), _)| *b == self.branch).map(|((_, u), s)| (*u, *s)).collect();
        if !any_merge && live.is_empty() && reopened_before.is_empty() && !self.has_undone_merges(ledger, steps) {
            return Ok(out);
        }
        let Some(first) = steps.first() else { return Ok(out) };
        let mut stacks = self.stacks_at_ro(ledger, first.seq.saturating_sub(1))?;
        // The live merges that closed their source: only those reopen it
        // when undone, and only a redo of such an undo closes it again. A stored merge closed its
        // source when the source's row says it was merged into this branch at that step.
        let mut closed: BTreeMap<u64, BranchId> = BTreeMap::new();
        for (x, row) in &live {
            let Some(source) = row.source_id() else { continue };
            let rec = match w {
                Some(w) => w.branch(source)?,
                None => self.store.reader()?.branch(source)?,
            };
            if rec.is_some_and(|b| b.status == BranchStatus::Merged && b.merged.as_ref().is_some_and(|m| m.into == self.branch.hex() && m.seq == *x))
            {
                closed.insert(*x, source);
            }
        }
        // Merges undone in this pass, by the undo's `seq`.
        let mut undone: BTreeMap<u64, MergeRow> = BTreeMap::new();
        for tx in steps {
            match tx.kind {
                TxKind::Merge { .. } => {
                    if let Some(row) = MergeRow::of(&tx.kind) {
                        out.rows.insert(tx.seq, Some(row.clone()));
                        if let Some(source) = self.closing.get(&tx.id) {
                            out.status.push((*source, Some(tx.seq)));
                            closed.insert(tx.seq, *source);
                        }
                        live.insert(tx.seq, row);
                    }
                }
                TxKind::Undo { .. } => {
                    if stacks.undo.is_empty() && stacks.undo_hidden > 0 {
                        stacks = self.stacks_at_ro(ledger, tx.seq.saturating_sub(1))?;
                    }
                    if let Some((x, _)) = stacks.undo.last().copied()
                        && let Some(row) = live.remove(&x)
                    {
                        out.rows.insert(x, None);
                        if let Some(source) = closed.remove(&x) {
                            out.status.push((source, None));
                            out.reopened.insert(tx.seq, source);
                        }
                        undone.insert(tx.seq, row);
                    }
                }
                TxKind::Redo { .. } => {
                    if stacks.redo.is_empty() && stacks.redo_hidden > 0 {
                        stacks = self.stacks_at_ro(ledger, tx.seq.saturating_sub(1))?;
                    }
                    if let Some((u, _)) = stacks.redo.last().copied() {
                        let row = match undone.remove(&u) {
                            Some(r) => Some(r),
                            None => self.undone_merge(ledger, u),
                        };
                        if let Some(row) = row {
                            out.rows.insert(tx.seq, Some(row.clone()));
                            if let Some(source) = out.reopened.remove(&u).or_else(|| reopened_before.get(&u).copied()) {
                                out.status.push((source, Some(tx.seq)));
                                closed.insert(tx.seq, source);
                            }
                            live.insert(tx.seq, row);
                        }
                    }
                }
                TxKind::Edit => {}
            }
            if !stacks.advance(std::slice::from_ref(tx)) {
                stacks = self.stacks_at_ro(ledger, tx.seq)?;
            }
        }
        Ok(out)
    }

    /// Whether a redo in `steps` may redo a merge undone before them: a redo whose undo's label
    /// is a merge's (a cheap filter; [`Project::undone_merge`] then reads the stacks).
    fn has_undone_merges(&self, ledger: &Ledger, steps: &[Transaction]) -> bool {
        let _ = ledger;
        steps.iter().any(|t| matches!(t.kind, TxKind::Redo { .. }) && (t.label.starts_with("Merge ") || t.label.starts_with("Update from ")))
    }

    /// The merge an undo at `u` undid, when the undo is of a merge (or of a redo of one): read
    /// from the undo's own stack position. Used for a redo whose undo was saved before.
    fn undone_merge(&self, ledger: &Ledger, u: u64) -> Option<MergeRow> {
        let mut at = u;
        for _ in 0..64 {
            // The step the undo at `at` undid: the undo stack's top just before it.
            let (x, _) = *self.stacks_at_ro(ledger, at.checked_sub(1)?).ok()?.undo.last()?;
            let tx = match ledger.transaction(x) {
                Some(t) => t.clone(),
                None => self.history().transaction(x).ok()?,
            };
            match tx.kind {
                TxKind::Merge { .. } => return MergeRow::of(&tx.kind),
                // A redo: the undo it redid is the redo stack's top just before it.
                TxKind::Redo { .. } => at = self.stacks_at_ro(ledger, x.checked_sub(1)?).ok()?.redo.last()?.0,
                TxKind::Edit | TxKind::Undo { .. } => return None,
            }
        }
        None
    }

    /// The live merges into the branch shown (the merge index), its unsaved steps
    /// included, oldest first.
    ///
    /// # Errors
    /// The file could not be read.
    pub fn merge_index(&self, ledger: &Ledger) -> Result<Vec<MergeIndex>, FileError> {
        let mut rows: BTreeMap<u64, MergeRow> =
            self.store.reader()?.versions(self.branch, RowKind::Merge)?.into_iter().filter_map(|v| v.merge.map(|m| (v.point.seq, m))).collect();
        if ledger.head() > self.head.0 && ledger.base() <= self.head.0 {
            let start = usize::try_from(self.head.0 - ledger.base()).unwrap_or(usize::MAX);
            let pending: Vec<Transaction> = ledger.log().get(start..).unwrap_or_default().to_vec();
            let follow = self.follow_merges(ledger, &pending, None).map_err(|e| FileError::Save(Box::new(e)))?;
            for (seq, row) in follow.rows {
                match row {
                    Some(r) => rows.insert(seq, r),
                    None => rows.remove(&seq),
                };
            }
        }
        rows.iter().map(|(seq, r)| MergeIndex::of(*seq, r)).collect()
    }

    /// The live merges into `branch` as saved (the merge index), oldest first.
    ///
    /// # Errors
    /// The file could not be read.
    pub fn merge_rows(&self, branch: BranchId) -> Result<Vec<MergeIndex>, FileError> {
        let r = self.store.reader()?;
        let rows = r.versions(branch, RowKind::Merge)?;
        let mut out = Vec::new();
        for v in &rows {
            if let Some(m) = &v.merge {
                out.push(MergeIndex::of(v.point.seq, m)?);
            }
        }
        out.sort_by_key(|m| m.seq);
        Ok(out)
    }

    /// Close `source` when the merge `tx` is saved (the merge dialog's Close branch
    /// option). Forgotten if the merge is never saved.
    pub fn close_on_save(&mut self, tx: TxId, source: BranchId) {
        self.closing.insert(tx, source);
    }

    /// The chain value of `branch` at its stored step `seq` (its seed at 0).
    ///
    /// # Errors
    /// The step is not stored, or the branch's lineage cannot be read.
    pub fn chain_of(&self, branch: BranchId, seq: u64) -> Result<Hash, FileError> {
        let r = self.store.reader()?;
        if seq == 0 {
            let line = lineage(&r, self.project, branch)?;
            return line.first().map(|l| l.seed).ok_or(FileError::EmptyLineage);
        }
        r.tx(branch, seq)?.map(|(_, c, _)| c).ok_or(FileError::StepNotStored(seq))
    }

    /// Mark `source` merged into `into` at its step `seq`, written at once (a merge with nothing
    /// to merge, closed from the merge dialog).
    ///
    /// # Errors
    /// As [`Project::create_branch`].
    pub fn mark_merged(&mut self, source: BranchId, into: BranchId, seq: u64) -> Result<BranchRecord, BranchError> {
        self.check_writable()?;
        if source == BranchId::main_of(self.project.0) {
            return Err(BranchError::Refused("main is the release line and is always open.".into()));
        }
        self.update_branch(source, |b| {
            b.status = BranchStatus::Merged;
            b.merged = Some(Merged { into: into.hex(), seq });
        })
    }

    /// Whether the merge `tx` (unsaved) will close its source when saved.
    pub fn closes_on_save(&self, tx: TxId) -> Option<BranchId> {
        self.closing.get(&tx).copied()
    }

    /// The chain value at `seq`: from this save's rows, or the stored row.
    fn chain_at(&self, seq: u64, chains: &BTreeMap<u64, Hash>) -> Result<Hash, SaveError> {
        if let Some(c) = chains.get(&seq) {
            return Ok(*c);
        }
        if seq == 0 {
            return Ok(self.seed());
        }
        let r = self.store.reader()?;
        r.tx(self.branch, seq)?.map(|(_, c, _)| c).ok_or(SaveError::Encode(FileError::TxNotStored(seq)))
    }

    /// Build and write the snapshot at `at` (a point at or before the ledger's head), relative to
    /// `snap` when the transactions between them are in memory.
    #[allow(clippy::too_many_arguments)]
    fn write_snapshot(
        &mut self, w: &mut super::store::Writer, ledger: &mut Ledger, snap: &mut Snap, cache: &mut Cache, at: u64, chain: Hash, kind: RowKind,
        name: &str, release: Option<Release>, saved: &mut Saved,
    ) -> Result<(), SaveError> {
        let enc = SaveError::Encode;
        let base = ledger.base();
        let meta = ModelMeta { tag_high: self.tag_high_at(ledger, at)? };
        let (lo, hi) = (at.min(snap.seq), at.max(snap.seq));
        let in_memory = lo >= base;
        let built = if let (true, Some(prev)) = (in_memory, snap.set.clone()) {
            let mut dirty = Dirty::default();
            for seq in lo.saturating_add(1)..=hi {
                if let Some(tx) = ledger.transaction(seq) {
                    dirty.add_ops(&tx.ops);
                }
            }
            let undo_after: Vec<Op> =
                ledger.log().iter().rev().take_while(|t| t.seq > at).flat_map(|t| t.ops.iter().rev().map(Op::inverse)).collect();
            if undo_after.is_empty() {
                chunks::build_incremental(ledger.document(), &prev, &dirty, &meta)
            } else {
                ledger.with_applied(&undo_after, |d| chunks::build_incremental(d, &prev, &dirty, &meta)).map_err(|e| enc(e.into()))?
            }
        } else {
            let doc = ledger.state_at(at).map_err(|e| enc(e.into()))?;
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
                description: None,
                merge: None,
                tag_rule: Some(ecad_model::naming::tag::TAG_RULE),
            },
        )?;
        saved.snapshots += 1;
        // The latest snapshot (the one opening uses) only moves forward.
        let latest = cache.latest.get(&self.branch.hex()).map_or(0, |s| s.seq);
        if at >= latest {
            cache.latest.insert(self.branch.hex(), SnapshotRef { kind, name: name.to_string(), seq: at });
        }
        if in_memory {
            *snap = Snap { seq: at, set: Some(set) };
        }
        Ok(())
    }

    /// After a rescue copy (Save As of a damaged file, rows after the verified head left out):
    /// set the branch head to the verified one, drop versions after it, and point the cache at
    /// the snapshot this project opened from (none: the root).
    ///
    /// # Errors
    /// The engine failed.
    pub(crate) fn write_rescue(&self, w: &mut super::store::Writer, r: &Reader) -> Result<(), SaveError> {
        let (seq, chain) = self.head;
        let mut rec = self.rec.clone();
        rec.head = Point::new(seq, &chain);
        w.put_branch(self.branch, &rec)?;
        for kind in [RowKind::VERSION, RowKind::REVISION, RowKind::Auto, RowKind::Merge] {
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
        w.set_meta("cache", &serde_json::to_value(&cache).map_err(|e| SaveError::Encode(FileError::json(&e)))?)?;
        Ok(())
    }

    /// The undo and redo stacks at `at`: from those at the snapshot this project was opened from
    /// and the transactions after it in memory. When that pops below what is held, the next
    /// entries of the snapshot's stacks are read from the file (the record's cap at a time, walking
    /// back only as far as they go) and kept. A point before the snapshot is read from the file.
    fn stacks_at(&mut self, ledger: &Ledger, at: u64) -> Result<StackState, SaveError> {
        let history = self.history();
        let mut base = std::mem::take(&mut self.base_stacks);
        let r = stacks_from(&*history, &mut base, self.base_seq, ledger, at);
        self.base_stacks = base;
        r
    }

    /// [`Project::stacks_at`] without keeping what it read (for reads that do not save).
    fn stacks_at_ro(&self, ledger: &Ledger, at: u64) -> Result<StackState, SaveError> {
        let mut base = self.base_stacks.clone();
        stacks_from(&*self.history(), &mut base, self.base_seq, ledger, at)
    }

    /// The tag high-water mark at `at`: from the one at the snapshot this project was opened from
    /// and the transactions after it in memory, or (for an earlier point) read from the file.
    fn tag_high_at(&self, ledger: &Ledger, at: u64) -> Result<HighWater, SaveError> {
        if at >= self.base_seq && self.base_seq >= ledger.base() {
            let mut high = self.base_tag_high.clone();
            for seq in self.base_seq.saturating_add(1)..=at {
                if let Some(tx) = ledger.transaction(seq) {
                    ecad_model::naming::tag::note_ops(&mut high, &tx.ops);
                }
            }
            return Ok(high);
        }
        let r = self.store.reader()?;
        line_high(&r, &self.lineage, 0, at).map_err(SaveError::Encode)
    }

    /// This branch's chain value at `seq` 0: the project seed on `main`, the branch seed
    /// elsewhere.
    fn seed(&self) -> Hash {
        self.lineage.first().map_or_else(|| chain::seed(ChainKind::Project, self.project.0), |l| l.seed)
    }

    /// The row of the branch shown, as last read or written.
    pub fn branch_record(&self) -> &BranchRecord {
        &self.rec
    }

    /// The branch shown and its ancestry up to `main`.
    pub fn lineage(&self) -> &Lineage {
        &self.lineage
    }

    /// Every branch row of the file (`main` first, then by creation time and name).
    ///
    /// # Errors
    /// The engine failed.
    pub fn branches(&self) -> Result<Vec<(BranchId, BranchRecord)>, StoreError> {
        let main = BranchId::main_of(self.project.0);
        let mut all = self.store.reader()?.branches()?;
        all.sort_by(|(a, x), (b, y)| (*a != main, x.created_ms, &x.name).cmp(&(*b != main, y.created_ms, &y.name)));
        Ok(all)
    }

    /// Why branch actions cannot write to the file, if they cannot.
    fn check_writable(&self) -> Result<(), BranchError> {
        match &self.read_only {
            Some(r) => Err(BranchError::ReadOnly(r.clone())),
            None => Ok(()),
        }
    }

    /// Create a branch named `name` forking from `from` at its transaction `seq` (a random
    /// ID; written at once, in a write transaction of its own; no snapshot is written).
    /// A fork at a branch's `seq` 0 is its parent's fork point, so the new branch's parent is the
    /// branch that holds the transaction. The caller saves pending transactions first.
    ///
    /// # Errors
    /// [`BranchError`]: the file is read-only, the name is refused, the point does not exist, or
    /// the engine failed.
    pub fn create_branch(
        &mut self, name: &str, from: BranchId, seq: u64, created_ms: Option<u64>, created_by: Option<String>,
    ) -> Result<(BranchId, BranchRecord), BranchError> {
        self.check_writable()?;
        let name = check_branch_name(name)?;
        let r = self.store.reader()?;
        let all = r.branches()?;
        if let Some((_, b)) = all.iter().find(|(_, b)| b.status == BranchStatus::Open && b.name.to_lowercase() == name.to_lowercase()) {
            return Err(BranchError::Refused(format!("There is already a branch \"{}\".", b.name)));
        }
        // Walk a fork at `seq` 0 up to the branch holding the transaction.
        let (mut at, mut seq) = (from, seq);
        for _ in 0..MAX_DEPTH {
            if seq > 0 {
                break;
            }
            match r.branch(at)?.and_then(|b| b.parent.zip(b.fork_point.map(|p| p.seq))) {
                Some((parent, fseq)) => (at, seq) = (parent, fseq),
                None => break,
            }
        }
        let holder = r.branch(at)?.ok_or_else(|| BranchError::Refused("That branch does not exist.".into()))?;
        // An unsaved point of the branch shown counts as its saved head (the caller saved first).
        if seq > holder.head.seq {
            return Err(BranchError::Refused(format!("Branch \"{}\" has no step {seq}.", holder.name)));
        }
        let fork_chain = if seq == 0 {
            chain::seed(ChainKind::Project, self.project.0)
        } else {
            r.tx(at, seq)?.map(|(_, c, _)| c).ok_or_else(|| BranchError::Refused(format!("Step {seq} is not stored.")))?
        };
        drop(r);
        let id = BranchId::from_seed(ecad_store::random_u128());
        let seed = chain::branch_seed(self.project.0, &id.0, &fork_chain);
        let rec = BranchRecord {
            name: name.clone(),
            status: BranchStatus::Open,
            parent: Some(at),
            fork_point: Some(Point::new(seq, &fork_chain)),
            head: Point::new(0, &seed),
            release_line: false,
            merged: None,
            created_ms,
            created_by,
        };
        let mut w = self.store.writer()?;
        if w.branch(id)?.is_some() {
            return Err(BranchError::Refused("A new branch ID was already used; try again.".into()));
        }
        w.put_branch(id, &rec)?;
        w.commit()?;
        Ok((id, rec))
    }

    /// Rename `branch` (not `main`), written at once.
    ///
    /// # Errors
    /// As [`Project::create_branch`].
    pub fn rename_branch(&mut self, branch: BranchId, name: &str) -> Result<BranchRecord, BranchError> {
        self.check_writable()?;
        let name = check_branch_name(name)?;
        if branch == BranchId::main_of(self.project.0) {
            return Err(BranchError::Refused("main is the release line and keeps its name.".into()));
        }
        let all = self.store.reader()?.branches()?;
        if let Some((_, b)) =
            all.iter().find(|(id, b)| *id != branch && b.status == BranchStatus::Open && b.name.to_lowercase() == name.to_lowercase())
        {
            return Err(BranchError::Refused(format!("There is already a branch \"{}\".", b.name)));
        }
        self.update_branch(branch, |b| b.name = name.clone())
    }

    /// Archive `branch` (`archive`) or reopen it, written at once. `main` cannot be archived;
    /// the branch shown cannot be archived (switch away first). Reopening checks the name is
    /// unique among open branches again.
    ///
    /// # Errors
    /// As [`Project::create_branch`].
    pub fn set_branch_open(&mut self, branch: BranchId, open: bool) -> Result<BranchRecord, BranchError> {
        self.check_writable()?;
        if branch == BranchId::main_of(self.project.0) {
            return Err(BranchError::Refused("main is the release line and cannot be archived.".into()));
        }
        if !open && branch == self.branch {
            return Err(BranchError::Refused("Switch to another branch before archiving this one.".into()));
        }
        if open {
            let all = self.store.reader()?.branches()?;
            let name = all.iter().find(|(id, _)| *id == branch).map(|(_, b)| b.name.to_lowercase()).unwrap_or_default();
            if let Some((_, b)) = all.iter().find(|(id, b)| *id != branch && b.status == BranchStatus::Open && b.name.to_lowercase() == name) {
                return Err(BranchError::NameTaken(b.name.clone()));
            }
        }
        self.update_branch(branch, |b| {
            b.status = if open { BranchStatus::Open } else { BranchStatus::Archived };
            if open {
                b.merged = None;
            }
        })
    }

    /// Reopen `branch` under `name` in one write (a reopen whose name another open
    /// branch took is never a dead end).
    ///
    /// # Errors
    /// As [`Project::set_branch_open`]; the new name is checked as a rename's.
    pub fn reopen_as(&mut self, branch: BranchId, name: &str) -> Result<BranchRecord, BranchError> {
        self.check_writable()?;
        let name = check_branch_name(name)?;
        if branch == BranchId::main_of(self.project.0) {
            return Err(BranchError::Refused("main is the release line and is always open.".into()));
        }
        let all = self.store.reader()?.branches()?;
        if let Some((_, b)) =
            all.iter().find(|(id, b)| *id != branch && b.status == BranchStatus::Open && b.name.to_lowercase() == name.to_lowercase())
        {
            return Err(BranchError::NameTaken(b.name.clone()));
        }
        self.update_branch(branch, |b| {
            b.name = name.clone();
            b.status = BranchStatus::Open;
            b.merged = None;
        })
    }

    fn update_branch(&mut self, branch: BranchId, change: impl FnOnce(&mut BranchRecord)) -> Result<BranchRecord, BranchError> {
        let mut w = self.store.writer()?;
        let mut rec = w.branch(branch)?.ok_or_else(|| BranchError::Refused("That branch does not exist.".into()))?;
        change(&mut rec);
        w.put_branch(branch, &rec)?;
        w.commit()?;
        if branch == self.branch {
            self.rec = rec.clone();
        }
        Ok(rec)
    }

    /// Show `branch` instead: load it as opening does (its latest snapshot and
    /// tail, or its parent's state at the fork) and, when the file is writable, write
    /// `meta.active` at once. The caller saves the branch shown first: switching with
    /// unsaved transactions is refused. A read-only file switches and writes nothing.
    ///
    /// # Errors
    /// [`BranchError`]; on an error the project still shows the previous branch.
    pub fn switch_to(&mut self, ledger: &Ledger, branch: BranchId, seed: u64) -> Result<(Ledger, Report), BranchError> {
        // Unsaved transactions are never switched away from, read-only or not.
        if ledger.head() != self.head.0 {
            return Err(BranchError::Refused("Save first: the branch shown has unsaved changes.".into()));
        }
        let rec = self.store.reader()?.branch(branch)?.ok_or_else(|| BranchError::Refused("That branch does not exist.".into()))?;
        if rec.status != BranchStatus::Open {
            return Err(BranchError::Refused(format!("Branch \"{}\" is not open; reopen it first.", rec.name)));
        }
        let l = load_branch(&self.store, self.project, &self.cache, branch, &rec, seed).map_err(|e| BranchError::Refused(e.to_string()))?;
        let writable = self.file_read_only.is_none();
        if writable {
            let mut w = self.store.writer()?;
            w.set_meta("active", &json!(branch.hex()))?;
            w.commit()?;
        }
        self.branch = branch;
        self.rec = rec;
        self.lineage = l.lineage;
        self.head = l.head;
        self.snap = l.snap;
        self.base_tag_high = l.base_tag_high;
        self.snap_ref = l.snap_ref;
        self.base_stacks = l.base_stacks;
        self.base_seq = l.report.snapshot_seq;
        self.read_only = l.damage.or_else(|| self.file_read_only.clone());
        Ok((l.ledger, l.report))
    }

    /// Transactions `from..=to` of `branch` as stored, for display (the History tab's times,
    /// authors and labels): each row is decoded and checked against its own
    /// chain value, not linked to its neighbours (Verify does that).
    ///
    /// # Errors
    /// The engine failed, or a row does not decode.
    pub fn tx_display(&self, branch: BranchId, from: u64, to: u64) -> Result<Vec<Transaction>, FileError> {
        if from > to {
            return Ok(Vec::new());
        }
        let r = self.store.reader()?;
        let mut out = Vec::new();
        for (seq, flag, stored, body) in r.tx_range(branch, from..=to)? {
            let record = pack::unpack(flag, &body)?;
            if chain::tx_hash(&record) != stored {
                return Err(FileError::TxChain(seq));
            }
            let value: Value = serde_json::from_slice(&record).map_err(|e| FileError::TxJson { seq, error: e.to_string() })?;
            out.push(record::decode_tx(&value).map_err(|error| FileError::TxDecode { seq, error })?);
        }
        Ok(out)
    }

    /// How many transactions [`Project::point_state`] replays for `seq` of `branch`: from the
    /// nearest snapshot at or before it on its line (never more than the snapshot interval past a
    /// snapshot, wherever the point is).
    ///
    /// # Errors
    /// The branch does not exist, or the file cannot be read.
    pub fn point_replay(&self, branch: BranchId, seq: u64) -> Result<u64, FileError> {
        let r = self.store.reader()?;
        let line = lineage(&r, self.project, branch)?;
        let (mut i, mut at, mut steps) = (0usize, seq, 0u64);
        loop {
            let l = line.get(i).ok_or(FileError::LineageIndex)?;
            if let Some(v) = nearest_snapshot(&r, l.branch, at)? {
                return Ok(steps + at - v.point.seq);
            }
            steps += at;
            match l.fork {
                Some((pi, fseq)) => (i, at) = (pi, fseq),
                None => return Ok(steps),
            }
        }
    }

    /// The document at `seq` of `branch` (any branch of the file): from its nearest snapshot, or
    /// its ancestry's, and at most the transactions after it (on a copy).
    ///
    /// # Errors
    /// The branch or point does not exist, or the file cannot be read.
    pub fn point_state(&self, branch: BranchId, seq: u64) -> Result<Document, FileError> {
        let r = self.store.reader()?;
        let line = lineage(&r, self.project, branch)?;
        let (doc, _) = line_state(&r, &line, 0, seq)?;
        Ok(doc)
    }

    /// The tag high-water mark at the snapshot this project was opened from (the next
    /// automatic number is the highest + 1); the transactions after it add to it.
    pub fn base_tag_high(&self) -> &HighWater {
        &self.base_tag_high
    }

    /// A reader of this file's history, for a ledger based on one of its snapshots.
    pub fn history(&self) -> Arc<dyn History> {
        Arc::new(StoreHistory { store: Arc::downgrade(&self.store), branch: self.branch, seed: self.seed(), lineage: self.lineage.clone() })
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
    pub fn set_snapshot_every(&mut self, every: u64) -> Result<(), FileError> {
        if !(SNAPSHOT_EVERY_RANGE.0..=SNAPSHOT_EVERY_RANGE.1).contains(&every) {
            return Err(FileError::SnapshotEvery);
        }
        self.cache.every = every;
        Ok(())
    }

    /// Why the project cannot be saved to this file, if it cannot.
    pub fn read_only(&self) -> Option<&str> {
        self.read_only.as_deref()
    }

    /// Make the project read-only (another session holds the file's lock).
    pub(crate) fn set_read_only(&mut self, why: String) {
        self.read_only = Some(why);
    }

    /// When the file was created (informational).
    pub fn created_ms(&self) -> Option<u64> {
        self.created_ms
    }

    /// What the last save wrote.
    pub fn last_save(&self) -> Option<&Saved> {
        self.last_save.as_ref()
    }
}

/// The undo and redo stacks at `at`: from `base` (those at `base_seq`, the snapshot opened from)
/// and the transactions after it in memory. When that pops below what `base` holds, its next
/// entries are read from `history` (the record's cap at a time, walking back only as far as they
/// go) and kept in `base`. A point before `base_seq` is read from the history.
fn stacks_from(history: &dyn History, base: &mut StackState, base_seq: u64, ledger: &Ledger, at: u64) -> Result<StackState, SaveError> {
    let enc = |e: ModelError| SaveError::Encode(e.into());
    if at >= base_seq && base_seq >= ledger.base() {
        let from = usize::try_from(base_seq - ledger.base()).unwrap_or(usize::MAX);
        let to = usize::try_from(at - ledger.base()).unwrap_or(usize::MAX);
        let txs = ledger.log().get(from..to).unwrap_or_default();
        loop {
            let mut s = base.clone();
            if s.advance(txs) {
                return Ok(s);
            }
            let cap = base.cap.max(1);
            let mut grew = false;
            for undo in [true, false] {
                let (stack, hidden) = if undo { (&mut base.undo, &mut base.undo_hidden) } else { (&mut base.redo, &mut base.redo_hidden) };
                if *hidden == 0 {
                    continue;
                }
                let n = (*hidden).min(cap);
                let more = history.stack_slice(base_seq, undo, stack.len() as u64, n).map_err(enc)?;
                if more.len() as u64 != n {
                    return Err(SaveError::Encode(FileError::FewerUndo));
                }
                let mut all = more;
                all.append(stack);
                *stack = all;
                *hidden -= n;
                grew = true;
            }
            if !grew {
                return Err(SaveError::Encode(FileError::UndoInconsistent));
            }
        }
    }
    let (undo, redo) = history.stacks_at(at).map_err(enc)?;
    Ok(StackState { undo, redo, undo_hidden: 0, redo_hidden: 0, cap: 0 })
}

/// A merge index row for the step at `seq` with chain value `chain`.
fn merge_record(seq: u64, chain: Hash, row: &MergeRow) -> VersionRecord {
    VersionRecord {
        kind: RowKind::Merge,
        name: auto_name(seq),
        point: Point::new(seq, &chain),
        chunks: BTreeMap::new(),
        release: None,
        undo: Vec::new(),
        redo: Vec::new(),
        undo_hidden: 0,
        redo_hidden: 0,
        stack_cap: 0,
        description: None,
        merge: Some(row.clone()),
        tag_rule: None,
    }
}

/// The issued revision row `rev` of `doc`, as release data.
fn revision_of(doc: &Document, rev: Rev) -> Option<Release> {
    let row = doc.project().revisions.iter().find(|r| r.issued && r.rev == Some(rev))?;
    Some(Release { revision: row.clone(), assets: Vec::new() })
}

/// The release data of revision `rev` as issued by `tx`: the revision row once `tx`
/// applied (fields are never changed after issuing, so the document after `tx` is read).
fn release_at(ledger: &mut Ledger, tx: &Transaction, rev: Rev) -> Result<Release, FileError> {
    let undo_after: Vec<Op> = ledger.log().iter().rev().take_while(|t| t.seq > tx.seq).flat_map(|t| t.ops.iter().rev().map(Op::inverse)).collect();
    let release =
        if undo_after.is_empty() { revision_of(ledger.document(), rev) } else { ledger.with_applied(&undo_after, |d| revision_of(d, rev))? };
    Ok(release.unwrap_or_default())
}
