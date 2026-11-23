//! Saving: the pending transactions, their merges, snapshots and stacks, in one write transaction.

use std::collections::BTreeMap;

use ecad_model::{Document, History, Ledger, ModelError, Op, PointName, Rev, Transaction, TxId, TxKind};
use serde_json::json;

use super::lineage::{StackState, encode_tx, line_high, lineage};
use super::report::{BranchError, SaveError, Saved};
use super::{MergeFollow, MergeIndex, Project, Snap};
use crate::FileError;
use crate::chain::Hash;
use crate::chunks::{self, Dirty, ModelMeta};
use crate::schema::{
    BranchId, BranchRecord, BranchStatus, Cache, MergeRow, Merged, Point, Release, RowKind, STACK_CAP, SnapshotRef, VersionRecord, auto_name,
};
use crate::store::Reader;
use ecad_model::naming::tag::HighWater;

impl Project {
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
    fn follow_merges(&self, ledger: &Ledger, steps: &[Transaction], w: Option<&crate::store::Writer>) -> Result<MergeFollow, SaveError> {
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
        &mut self, w: &mut crate::store::Writer, ledger: &mut Ledger, snap: &mut Snap, cache: &mut Cache, at: u64, chain: Hash, kind: RowKind,
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
    pub(crate) fn write_rescue(&self, w: &mut crate::store::Writer, r: &Reader) -> Result<(), SaveError> {
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
pub(super) fn revision_of(doc: &Document, rev: Rev) -> Option<Release> {
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
