//! Reading stored history: the transaction row codec, a branch's lineage and its state at a point,
//! the undo and redo stacks a version records, and [`StoreHistory`] for a ledger's reads past its base.

use std::sync::Arc;

use ecad_model::{Document, History, ModelError, ProjectId, Transaction, TxId, TxKind};
use rayon::prelude::*;
use serde_json::Value;

use super::report::Damage;
use crate::FileError;
use crate::chain::{self, ChainKind, Hash};
use crate::chunks::{self, ChunkSet, ModelMeta};
use crate::pack;
use crate::record;
use crate::schema::{BranchId, RowKind, STACK_CAP, VersionRecord};
use crate::store::{Reader, Store, StoreError};
use ecad_model::naming::tag::HighWater;

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
pub(super) fn encode_tx(tx: &Transaction, prev: &Hash) -> Result<(Vec<u8>, Hash), FileError> {
    let value = record::tx_value(tx, prev)?;
    let canonical = crate::canonical::canonical(&value)?;
    let hash = chain::tx_hash(&canonical);
    Ok((canonical, hash))
}

/// The undo and redo stacks at a point: their tops, and how many entries lie below what is held
/// (a snapshot records only the top [`STACK_CAP`] of each).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct StackState {
    pub(super) undo: Vec<(u64, TxId)>,
    pub(super) redo: Vec<(u64, TxId)>,
    pub(super) undo_hidden: u64,
    pub(super) redo_hidden: u64,
    /// The cap the record was written with (0: the stacks are whole): entries below are read
    /// this many at a time.
    pub(super) cap: u64,
}

impl StackState {
    /// Advance over `txs` (only kinds and IDs are read). `false` when a step pops below what is
    /// held: the whole stacks must be read first.
    pub(super) fn advance(&mut self, txs: &[Transaction]) -> bool {
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
    pub(super) fn record(&self) -> (Vec<(u64, String)>, Vec<(u64, String)>, u64, u64) {
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
    pub(super) fn from_record(v: &VersionRecord) -> Result<Self, FileError> {
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
pub(super) const MAX_DEPTH: usize = 64;

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
pub(super) fn nearest_snapshot(r: &Reader, branch: BranchId, seq: u64) -> Result<Option<VersionRecord>, FileError> {
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
pub(super) fn line_high(r: &Reader, lineage: &[Line], i: usize, seq: u64) -> Result<HighWater, FileError> {
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
pub(super) fn load_snapshot(r: &Reader, v: &VersionRecord) -> Result<(Document, Option<ModelMeta>), FileError> {
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
pub(super) fn read_rows(
    r: &Reader, branch: BranchId, from: u64, to: u64, mut prev: Hash,
) -> Result<(Vec<Transaction>, Hash, Option<Damage>), StoreError> {
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
    pub(super) store: std::sync::Weak<Store>,
    pub(super) branch: BranchId,
    pub(super) seed: Hash,
    /// The branch and its ancestry: a state before the branch's own first snapshot starts from
    /// its parent's state at the fork point.
    pub(super) lineage: Lineage,
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
