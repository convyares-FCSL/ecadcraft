//! Changing a ledger: preview, commit, undo, redo, and the states in its past.

use crate::Document;
use crate::document::op::Op;
use crate::error::{ModelError, Result};
use crate::naming::id::TxId;

use super::*;

impl Ledger {
    /// The document after `ops`, without committing anything.
    ///
    /// # Semantics
    /// Works on a copy of the document (a preview is a second document by nature); the live
    /// document is not touched.
    ///
    /// # Errors
    /// Precondition or invariant failures, exactly as [`Ledger::commit`] would report them.
    pub fn preview(&self, ops: &[Op]) -> Result<Document> {
        let mut next = self.doc.clone();
        apply_scratch(&mut next, ops, self.verify)?;
        Ok(next)
    }

    /// Run `read` on the document as it would be after `ops`, without copying it and without
    /// committing anything: [`Document::with_applied`] on the live document, in this ledger's
    /// verify mode.
    ///
    /// # Semantics
    /// The document, its indexes and the log are left exactly as they were, also when `read`
    /// panics. In the never expected case that reverting an op fails, the document is rebuilt by
    /// replaying the log, as for a failed commit, and [`Ledger::diagnostics`] records it.
    ///
    /// # Errors
    /// Precondition or invariant failures, exactly as [`Ledger::commit`] would report them (then
    /// `read` has not run). [`ModelError::CorruptLog`] when a revert failed and replaying the log
    /// could not restore the document either, from this call on, as commits are refused then.
    pub fn with_applied<R>(&mut self, ops: &[Op], read: impl FnOnce(&Document) -> R) -> Result<R> {
        if self.health.broken {
            return Err(ModelError::CorruptLog("the document could not be restored after a failed rollback; reopen the project".into()));
        }
        let committed = Committed { log: &self.log, base: self.base, history: self.history.as_ref() };
        let (result, failure) = applied_then_reverted(&mut self.doc, ops, self.verify, Some(committed), &mut self.health, read);
        match failure {
            // The revert failed and replaying the log could not restore the document either.
            Some(f) if self.health.broken => Err(ModelError::CorruptLog(format!("{f}; reopen the project"))),
            _ => result,
        }
    }

    /// Validate and append an edit.
    ///
    /// # Errors
    /// [`ModelError::Empty`] for no ops; precondition or invariant failures. On error nothing
    /// changes.
    pub fn commit(&mut self, author: Author, label: &str, ops: Vec<Op>) -> Result<TxId> {
        self.commit_at(author, label, ops, None)
    }

    /// [`Ledger::commit`] with a wall-clock time.
    ///
    /// # Errors
    /// As [`Ledger::commit`].
    pub fn commit_at(&mut self, author: Author, label: &str, ops: Vec<Op>, time_ms: Option<u64>) -> Result<TxId> {
        self.commit_extended(author, label, ops, time_ms, |_, _| Vec::new())
    }

    /// [`Ledger::commit_at`] with a pass that extends the transaction (the numbering
    /// pass): after `ops` are applied, the wire rewrite ([`crate::connectivity::wire_order::rewrite`], every
    /// edit) and then `extend` see the document as they left it (no copy) and return more ops,
    /// recorded in the same transaction after `ops`, so one undo undoes all.
    ///
    /// # Errors
    /// As [`Ledger::commit`]; the extra ops are checked like any other.
    pub fn commit_extended(
        &mut self, author: Author, label: &str, ops: Vec<Op>, time_ms: Option<u64>, extend: impl FnOnce(&Document, &[Op]) -> Vec<Op>,
    ) -> Result<TxId> {
        if ops.is_empty() {
            return Err(ModelError::Empty);
        }
        if self.health.broken {
            return Err(ModelError::CorruptLog("the document could not be restored after a failed rollback; reopen the project".into()));
        }
        let seq = self.next_seq()?;
        check_bookmarks(&ops, seq)?;
        let kind = TxKind::Edit;
        let stacks = &self.stacks;
        let committed = Committed { log: &self.log, base: self.base, history: self.history.as_ref() };
        let admit = || stacks.admit_kind(kind, "the new transaction");
        let more = apply_tx_extended(&mut self.doc, &ops, self.verify, Some(committed), &mut self.health, extend, admit)?;
        let mut ops = ops;
        ops.extend(more);
        let id: TxId = self.ids.fresh();
        let tx = Transaction { id, seq, author, label: label.to_string(), kind, time_ms, ops };
        self.stacks.push(&tx);
        self.log.push(tx);
        self.after_append();
        Ok(id)
    }

    /// Commit a merge: `ops` as one transaction of kind [`TxKind::Merge`],
    /// checked like any edit. A merge may have no ops (the target's side kept for every
    /// conflict): it is still recorded, so the next merge compares against it.
    ///
    /// # Errors
    /// As [`Ledger::commit`].
    pub fn commit_merge(&mut self, author: Author, label: &str, kind: TxKind, ops: Vec<Op>, time_ms: Option<u64>) -> Result<TxId> {
        if !matches!(kind, TxKind::Merge { .. }) {
            return Err(ModelError::Precondition("a merge must be recorded as a merge".into()));
        }
        self.append(author, label.to_string(), kind, ops, time_ms)
    }

    pub(super) fn append(&mut self, author: Author, label: String, kind: TxKind, ops: Vec<Op>, time_ms: Option<u64>) -> Result<TxId> {
        if self.health.broken {
            return Err(ModelError::CorruptLog("the document could not be restored after a failed rollback; reopen the project".into()));
        }
        let seq = self.next_seq()?;
        check_bookmarks(&ops, seq)?;
        // Not refused through undo/redo (their target is the stack top); kept so a bad kind can
        // never be recorded.
        let stacks = &self.stacks;
        let committed = Committed { log: &self.log, base: self.base, history: self.history.as_ref() };
        let admit = || stacks.admit_kind(kind, "the new transaction");
        apply_tx(&mut self.doc, &ops, self.verify, Some(committed), &mut self.health, admit)?;
        let id: TxId = self.ids.fresh();
        let tx = Transaction { id, seq, author, label, kind, time_ms, ops };
        self.stacks.push(&tx);
        self.log.push(tx);
        self.after_append();
        Ok(id)
    }

    /// After a transaction is appended: undo or redo may have exposed a top before the base, or
    /// emptied what the snapshot recorded. Read it now, so labels and the next step never wait on
    /// the store. A failure is reported here, and retried by the step.
    pub(super) fn after_append(&mut self) {
        let read = if self.stacks.top_hidden() { self.rebuild_stacks() } else { self.load_tops() };
        if let Err(e) = read {
            self.health.diagnostics.push(format!("could not read an older step for undo or redo: {e}"));
        }
    }

    /// The transaction on top of `stack`, if held in memory.
    pub(super) fn top(&self, stack: &[(u64, TxId)]) -> Option<&Transaction> {
        stack.last().and_then(|(seq, _)| self.tx(*seq))
    }

    /// The step [`Ledger::undo`] would undo, when held in memory (undoing a merge
    /// asks first).
    pub fn undo_top(&self) -> Option<&Transaction> {
        self.top(&self.stacks.undo)
    }

    /// The undo held by the step [`Ledger::redo`] would redo, when held in memory (an
    /// agent redoes only its own undos).
    pub fn redo_top(&self) -> Option<&Transaction> {
        self.top(&self.stacks.redo)
    }

    /// The steps [`Ledger::undo`] would take back, oldest first (`(seq, id)`; the last is the next
    /// one). Only the entries known in memory: a snapshot's older entries may not be read yet
    /// (Undo session looks only at an agent session's recent steps).
    pub fn undo_stack(&self) -> &[(u64, TxId)] {
        &self.stacks.undo
    }

    /// Label of the step [`Ledger::undo`] would undo.
    pub fn undo_label(&self) -> Option<&str> {
        self.top(&self.stacks.undo).map(|t| t.label.as_str())
    }

    /// Label of the step [`Ledger::redo`] would redo.
    pub fn redo_label(&self) -> Option<&str> {
        self.top(&self.stacks.redo).map(|t| t.label.as_str())
    }

    /// The top of the undo (or redo) stack, read from the history first if it lies before the
    /// base and was not read yet (a failed earlier read is retried here).
    pub(super) fn ensure_top(&mut self, undo: bool, nothing: &'static str) -> Result<&Transaction> {
        if self.stacks.top_hidden() {
            self.rebuild_stacks()?;
        }
        let empty = if undo { self.stacks.undo.is_empty() } else { self.stacks.redo.is_empty() };
        if empty {
            return Err(ModelError::NothingTo(nothing));
        }
        self.load_tops()?;
        let stack = if undo { &self.stacks.undo } else { &self.stacks.redo };
        self.top(stack).ok_or(ModelError::NothingTo(nothing))
    }

    /// Append the inverse of the latest effective step.
    ///
    /// # Errors
    /// [`ModelError::NothingTo`] when there is nothing to undo.
    pub fn undo(&mut self, author: Author) -> Result<TxId> {
        self.undo_at(author, None)
    }

    /// [`Ledger::undo`] with a wall-clock time.
    ///
    /// # Errors
    /// As [`Ledger::undo`]; [`ModelError::CorruptLog`] when the step lies before the base and
    /// could not be read.
    pub fn undo_at(&mut self, author: Author, time_ms: Option<u64>) -> Result<TxId> {
        let target = self.ensure_top(true, "nothing to undo")?;
        let (of, label, ops) = (target.id, target.label.clone(), inverse_ops(&target.ops));
        self.append(author, label, TxKind::Undo { of }, ops, time_ms)
    }

    /// The ops [`Ledger::undo_at`] would apply now (the inverse of the transaction it undoes), or
    /// `None` when there is nothing to undo. Reads only.
    pub fn undo_ops(&self) -> Option<Vec<Op>> {
        self.top(&self.stacks.undo).map(|t| inverse_ops(&t.ops))
    }

    /// The ops [`Ledger::redo_at`] would apply now, or `None` when there is nothing to redo.
    pub fn redo_ops(&self) -> Option<Vec<Op>> {
        self.top(&self.stacks.redo).map(|t| inverse_ops(&t.ops))
    }

    /// Append the inverse of the latest undo.
    ///
    /// # Errors
    /// [`ModelError::NothingTo`] when there is nothing to redo.
    pub fn redo(&mut self, author: Author) -> Result<TxId> {
        self.redo_at(author, None)
    }

    /// [`Ledger::redo`] with a wall-clock time.
    ///
    /// # Errors
    /// As [`Ledger::redo`].
    pub fn redo_at(&mut self, author: Author, time_ms: Option<u64>) -> Result<TxId> {
        let target = self.ensure_top(false, "nothing to redo")?;
        let (of, label, ops) = (target.id, target.label.clone(), inverse_ops(&target.ops));
        self.append(author, label, TxKind::Redo { of }, ops, time_ms)
    }

    /// Reconstruct the document as it was after transaction `seq`, separately from the live
    /// state (which is not touched).
    ///
    /// # Semantics
    /// From the root, replaying the log, when the base is 0. Otherwise a point at or after the
    /// base is reached by undoing the later transactions on a copy of the live document, and a
    /// point before it is read from the history (its nearest snapshot and the rows after it).
    ///
    /// # Errors
    /// [`ModelError::Precondition`] when `seq` is past the head; replay or store failures.
    pub fn state_at(&self, seq: u64) -> Result<Document> {
        if seq > self.head() {
            return Err(ModelError::Precondition(format!("sequence {seq} is past the head {}", self.head())));
        }
        // From the root only when nothing precedes the log: a branch's ledger based at its `seq` 0
        // starts from its parent's state, which its history gives.
        if self.base == 0 && self.history.is_none() {
            let mut doc = Document::default();
            for tx in self.log.iter().take(usize::try_from(seq).unwrap_or(usize::MAX)) {
                apply_scratch(&mut doc, &tx.ops, self.verify)?;
            }
            return Ok(doc);
        }
        if seq < self.base {
            let h = self
                .history
                .as_ref()
                .ok_or_else(|| ModelError::CorruptLog(format!("transaction {seq} is before the base and there is no history")))?;
            return h.state_at(seq);
        }
        let mut doc = self.doc.clone();
        for tx in self.log.iter().rev().take_while(|t| t.seq > seq) {
            apply_scratch(&mut doc, &inverse_ops(&tx.ops), self.verify)?;
        }
        Ok(doc)
    }

    /// Replay the log from the root (or, for a ledger with a base, from the state the history
    /// gives at the base) into a fresh document and compare it with the live one.
    ///
    /// # Errors
    /// Replay failures.
    pub fn replay_matches(&self) -> Result<bool> {
        let mut doc = match (&self.history, self.base) {
            (Some(h), base) => h.state_at(base)?,
            (None, 0) => Document::default(),
            (None, base) => {
                return Err(ModelError::CorruptLog(format!("the state at {base} cannot be read: no history")));
            }
        };
        for tx in &self.log {
            apply_scratch(&mut doc, &tx.ops, self.verify)?;
        }
        Ok(doc == self.doc)
    }
}
