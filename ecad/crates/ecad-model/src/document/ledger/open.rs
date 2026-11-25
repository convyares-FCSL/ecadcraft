//! Opening a ledger: new, replay, from a base or a checkpoint.

use std::collections::BTreeMap;

use crate::Document;
use crate::error::{ModelError, Result};
use crate::naming::id::IdAllocator;

use super::*;

impl Ledger {
    /// An empty project. `seed` seeds transaction IDs.
    pub fn new(seed: u64) -> Self {
        Self {
            doc: Document::default(),
            log: Vec::new(),
            base: 0,
            history: None,
            older: BTreeMap::new(),
            persisted: 0,
            ids: IdAllocator::seeded(seed),
            stacks: Stacks::default(),
            verify: false,
            health: Health::default(),
            stack_rebuilds: 0,
            stack_read: STACK_READ,
        }
    }

    /// Rebuild from a log by applying every transaction to an empty document.
    ///
    /// # Errors
    /// [`ModelError::CorruptLog`] when a transaction's preconditions or the invariants fail, or
    /// the undo/redo structure is inconsistent.
    pub fn replay(log: Vec<Transaction>, seed: u64) -> Result<Self> {
        Self::from_checkpoint(Document::default(), 0, log, seed)
    }

    /// Replay `log` on `doc` (the empty project, or a branch's parent state at its fork point)
    /// checking each transaction at exactly `check`, whatever the build's verify mode, so the
    /// cost and the verdict are the same in tests and in release. The resulting ledger is not in
    /// verify mode.
    ///
    /// # Errors
    /// As [`Ledger::replay`].
    pub fn replay_checked_from(doc: Document, log: Vec<Transaction>, seed: u64, check: Check) -> Result<Self> {
        Self::replay_checked_visit(doc, log, seed, check, |_, _| {})
    }

    /// [`Ledger::replay_checked_from`], calling `visit` with each transaction and the document
    /// just after it (checked), in order. The file's Verify compares snapshots and keeps fork
    /// states this way, in the same pass.
    ///
    /// # Errors
    /// As [`Ledger::replay`]. `visit` has then been called for every transaction before the
    /// failing one.
    pub fn replay_checked_visit(
        doc: Document, log: Vec<Transaction>, seed: u64, check: Check, visit: impl FnMut(&Transaction, &Document),
    ) -> Result<Self> {
        Self::rebuild_at(doc, 0, log, seed, Level::Exactly(check), visit)
    }

    /// Replay as much of `log` as is valid: the ledger of the longest valid prefix and, when a
    /// transaction fails, its position in `log` (0-based) and the error. Used by file readers to
    /// open a damaged history read-only at its last good transaction.
    ///
    /// # Semantics
    /// Every transaction is checked as it is applied, exactly as when it was committed; a failed
    /// one is rolled back, so the document is the state after the last good transaction.
    pub fn replay_prefix(log: Vec<Transaction>, seed: u64) -> (Self, Option<(usize, ModelError)>) {
        let mut me = Self::new(seed);
        let failure = me.extend_prefix(log);
        let last = me.log.last().map_or(0, |t| t.id.0);
        me.ids = IdAllocator::seeded_after(seed, last);
        (me, failure)
    }

    /// Start from a snapshot instead of the project root: the document,
    /// undo and redo stacks after transaction `base.seq`, then `tail` (the transactions after it,
    /// `seq` `base.seq + 1` on) replayed with every check. Transactions before the base are read
    /// from `base.history` only when undo, redo or [`Ledger::state_at`] reach them.
    ///
    /// # Semantics
    /// Like [`Ledger::replay_prefix`], the longest valid prefix of `tail` is kept, and the first
    /// failure is returned with its position in `tail` (0-based).
    ///
    /// # Errors
    /// [`ModelError::CorruptLog`] when the base's stacks are inconsistent (an entry at 0 or after
    /// the base, or out of order) or their tops cannot be read from the history.
    pub fn from_base(base: Base, tail: Vec<Transaction>, seed: u64) -> Result<(Self, Option<(usize, ModelError)>)> {
        let Base { seq, doc, undo, redo, undo_hidden, redo_hidden, stack_cap, history } = base;
        // Each transaction up to the base is on at most one stack: more entries than that (a
        // crafted file) is damage, never an allocation or a loop.
        let depth = [undo.len() as u64, undo_hidden, redo.len() as u64, redo_hidden].into_iter().try_fold(0u64, u64::checked_add);
        if depth.is_none_or(|n| n > seq) {
            return Err(ModelError::CorruptLog(format!("the undo and redo stacks recorded at {seq} are deeper than the history")));
        }
        if stack_cap == 0 && (undo_hidden > 0 || redo_hidden > 0) {
            return Err(ModelError::CorruptLog(format!("the stacks recorded at {seq} have entries below them but no cap")));
        }
        for stack in [&undo, &redo] {
            let bad_entry = stack.iter().any(|(s, _)| *s == 0 || *s > seq);
            let unordered = stack.windows(2).any(|w| matches!(w, [a, b] if a.0 >= b.0));
            if bad_entry || unordered {
                return Err(ModelError::CorruptLog(format!("the undo or redo stack recorded at {seq} is inconsistent")));
            }
        }
        let mut me = Self {
            doc,
            log: Vec::new(),
            base: seq,
            history: Some(history),
            older: BTreeMap::new(),
            persisted: seq,
            ids: IdAllocator::seeded(seed),
            stacks: Stacks { undo_known: undo.len() as u64, redo_known: redo.len() as u64, undo, redo, undo_hidden, redo_hidden },
            verify: false,
            health: Health::default(),
            stack_rebuilds: 0,
            stack_read: if stack_cap == 0 { STACK_READ } else { stack_cap },
        };
        me.load_tops()?;
        let failure = me.extend_prefix(tail);
        let last = me.log.last().map_or(0, |t| t.id.0);
        me.ids = IdAllocator::seeded_after(seed, last);
        Ok((me, failure))
    }

    /// Append as much of `log` (the transactions after the head, in order) as is valid, checking
    /// each as when it was committed; the first failure and its position in `log`. Everything
    /// appended counts as persisted (it was read from a file).
    pub(super) fn extend_prefix(&mut self, log: Vec<Transaction>) -> Option<(usize, ModelError)> {
        let mut failure = None;
        let start = self.head();
        for (i, tx) in log.into_iter().enumerate() {
            let expected = start.checked_add(i as u64).and_then(|n| n.checked_add(1));
            // A step that pops below what the snapshot recorded reads the stacks whole first.
            let full = if self.stacks.needs_full(tx.kind) { self.rebuild_stacks() } else { Ok(()) };
            let step = if let Err(e) = full {
                Err(e)
            } else if Some(tx.seq) != expected {
                Err(ModelError::CorruptLog(format!("transaction {} has sequence {} at position {:?}", tx.id, tx.seq, expected)))
            } else {
                let stacks = &self.stacks;
                let committed = Committed { log: &self.log, base: self.base, history: self.history.as_ref() };
                check_bookmarks(&tx.ops, tx.seq)
                    .and_then(|()| apply_tx(&mut self.doc, &tx.ops, false, Some(committed), &mut self.health, || stacks.admit(&tx)))
            };
            if self.health.broken {
                // Never expected: a rollback failed and the good prefix did not replay either.
                // Open nothing rather than a document that may be corrupt.
                self.doc = Document::default();
                self.stacks = Stacks::default();
                self.log.clear();
                self.base = 0;
                self.history = None;
                self.older.clear();
                let why = self.health.diagnostics.last().cloned().unwrap_or_default();
                failure = Some((i, ModelError::CorruptLog(why)));
                break;
            }
            match step {
                Ok(()) => {
                    self.stacks.push(&tx);
                    self.log.push(tx);
                    if let Err(e) = self.load_tops() {
                        failure = Some((i + 1, e));
                        break;
                    }
                }
                Err(e) => {
                    failure = Some((i, e));
                    break;
                }
            }
        }
        self.persisted = self.head();
        failure
    }

    /// Read the next [`Base::stack_cap`] entries below an emptied stack from the history: the base's
    /// stack entries just below those already read (the transactions after the base never reach
    /// below what is held without this). Kept for the session; each further read goes deeper.
    ///
    /// # Errors
    /// The history could not be read, or holds fewer entries than the depth recorded.
    pub(super) fn rebuild_stacks(&mut self) -> Result<()> {
        for undo in [true, false] {
            let (held, hidden, known) = if undo {
                (self.stacks.undo.len(), self.stacks.undo_hidden, self.stacks.undo_known)
            } else {
                (self.stacks.redo.len(), self.stacks.redo_hidden, self.stacks.redo_known)
            };
            if held > 0 || hidden == 0 {
                continue;
            }
            let h =
                self.history.as_ref().ok_or_else(|| ModelError::CorruptLog("the undo history before the base cannot be read: no history".into()))?;
            let n = hidden.min(self.stack_read);
            let more = h.stack_slice(self.base, undo, known, n)?;
            if more.len() as u64 != n || more.iter().any(|(s, _)| *s > self.base) {
                return Err(ModelError::CorruptLog("the history does not agree with the recorded undo stacks".into()));
            }
            if undo {
                self.stacks.undo = more;
                self.stacks.undo_hidden = hidden.saturating_sub(n);
                self.stacks.undo_known = known.saturating_add(n);
            } else {
                self.stacks.redo = more;
                self.stacks.redo_hidden = hidden.saturating_sub(n);
                self.stacks.redo_known = known.saturating_add(n);
            }
            self.stack_rebuilds += 1;
        }
        self.load_tops()
    }

    /// How often stack entries below what was held were read from the history.
    pub fn stack_rebuilds(&self) -> u32 {
        self.stack_rebuilds
    }

    /// The depths of the undo and redo stacks, entries not read yet included.
    pub fn stack_depths(&self) -> (u64, u64) {
        (self.stacks.undo.len() as u64 + self.stacks.undo_hidden, self.stacks.redo.len() as u64 + self.stacks.redo_hidden)
    }

    /// Read the tops of the undo and redo stacks that lie before the base, if not read yet.
    pub(super) fn load_tops(&mut self) -> Result<()> {
        let tops = [self.stacks.undo.last().copied(), self.stacks.redo.last().copied()];
        for (seq, id) in tops.into_iter().flatten() {
            if seq > self.base || self.older.contains_key(&seq) {
                continue;
            }
            let h = self
                .history
                .as_ref()
                .ok_or_else(|| ModelError::CorruptLog(format!("transaction {seq} is before the base and there is no history")))?;
            let tx = h.transaction(seq)?;
            if tx.seq != seq || tx.id != id {
                return Err(ModelError::CorruptLog(format!(
                    "the history holds {} at {}, where the undo or redo stack names {id} at {seq}",
                    tx.id, tx.seq
                )));
            }
            self.older.insert(seq, tx);
        }
        Ok(())
    }

    /// Transaction `seq`, from the log or the transactions read from before the base.
    pub(super) fn tx(&self, seq: u64) -> Option<&Transaction> {
        if seq > self.base { self.log.get(usize::try_from(seq - self.base - 1).ok()?) } else { self.older.get(&seq) }
    }

    /// Rebuild from a checkpoint holding the state after the first `at` transactions, applying
    /// the rest of `log`. A checkpoint is only a cache: the full log is still required.
    ///
    /// # Errors
    /// As [`Ledger::replay`].
    pub fn from_checkpoint(doc: Document, at: u64, log: Vec<Transaction>, seed: u64) -> Result<Self> {
        Self::rebuild(doc, at, log, seed, false)
    }

    pub(crate) fn rebuild(doc: Document, at: u64, log: Vec<Transaction>, seed: u64, verify: bool) -> Result<Self> {
        Self::rebuild_at(doc, at, log, seed, Level::Build(verify), |_, _| {})
    }

    pub(super) fn rebuild_at(
        doc: Document, at: u64, log: Vec<Transaction>, seed: u64, level: Level, mut visit: impl FnMut(&Transaction, &Document),
    ) -> Result<Self> {
        let verify = matches!(level, Level::Build(true));
        let mut doc = doc;
        let mut health = Health::default();
        for (i, tx) in log.iter().enumerate() {
            if tx.seq != i as u64 + 1 {
                return Err(ModelError::CorruptLog(format!("transaction {} has sequence {} at position {}", tx.id, tx.seq, i + 1)));
            }
            if tx.seq > at {
                let history = log.get(..i).map(Committed::root);
                check_bookmarks(&tx.ops, tx.seq)
                    .and_then(|()| apply_tx_at(&mut doc, &tx.ops, level, history, &mut health, || Ok(())))
                    .map_err(|e| ModelError::CorruptLog(format!("transaction {}: {e}", tx.seq)))?;
                visit(tx, &doc);
            }
        }
        let stacks = stacks(&log)?;
        let persisted = log.len() as u64;
        let last = log.last().map_or(0, |t| t.id.0);
        Ok(Self {
            doc,
            log,
            base: 0,
            history: None,
            older: BTreeMap::new(),
            persisted,
            ids: IdAllocator::seeded_after(seed, last),
            stacks,
            verify,
            health,
            stack_rebuilds: 0,
            stack_read: STACK_READ,
        })
    }
}
