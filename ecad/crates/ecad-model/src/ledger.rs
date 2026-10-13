//! Transactions and the append-only history (proposal §6).

use serde::{Deserialize, Serialize};

use crate::doc::Document;
use crate::error::{ModelError, Result};
use crate::id::{IdAllocator, TxId};
use crate::op::Op;

/// Who made a transaction (FILE-07).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Author {
    User,
    Agent(String),
}

/// What kind of history step a transaction is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TxKind {
    Edit,
    /// Inverts transaction `of` (an edit or a redo).
    Undo {
        of: TxId,
    },
    /// Inverts transaction `of` (an undo).
    Redo {
        of: TxId,
    },
}

/// One atomic, recorded change to the document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transaction {
    pub id: TxId,
    /// Position in the log, starting at 1. A snapshot "at seq n" holds transactions 1..=n.
    pub seq: u64,
    pub author: Author,
    pub label: String,
    pub kind: TxKind,
    /// Wall-clock milliseconds when known; informational, never used by replay.
    pub time_ms: Option<u64>,
    pub ops: Vec<Op>,
}

/// The authoritative document and its history.
///
/// # Semantics
/// - [`Ledger::commit`] applies a transaction to a copy, checks every invariant and only then
///   appends it (TX-03).
/// - [`Ledger::undo`] and [`Ledger::redo`] append transactions holding concrete inverse ops
///   (UNDO-01). The undo/redo position is derived from the log (UNDO-02), never stored.
/// - The project is unsaved exactly when [`Ledger::head`] differs from
///   [`Ledger::persisted`] (SAVE-01).
///
/// # Example
/// ```rust
/// use ecad_model::{Author, Ledger, Op, Page, PageId};
/// let mut ledger = Ledger::new(1);
/// let page = PageId(7);
/// ledger.commit(Author::User, "New page", vec![Op::AddPage { id: page, page: Page { title: "1".into() }, index: 0 }]).unwrap();
/// assert!(ledger.is_unsaved());
/// ledger.undo(Author::User).unwrap();
/// assert!(ledger.document().pages.is_empty());
/// assert_eq!(ledger.head(), 2); // the undo was appended
/// ```
#[derive(Debug, Clone)]
pub struct Ledger {
    doc: Document,
    log: Vec<Transaction>,
    persisted: u64,
    ids: IdAllocator,
}

/// The derived undo/redo stacks: log indices.
struct Stacks {
    undo: Vec<usize>,
    redo: Vec<usize>,
}

fn stacks(log: &[Transaction]) -> Result<Stacks> {
    let mut s = Stacks { undo: Vec::new(), redo: Vec::new() };
    for (i, tx) in log.iter().enumerate() {
        match tx.kind {
            TxKind::Edit => {
                s.undo.push(i);
                s.redo.clear();
            }
            TxKind::Undo { of } => {
                let top = s.undo.pop().and_then(|j| log.get(j));
                if top.map(|t| t.id) != Some(of) {
                    return Err(ModelError::CorruptLog(format!("undo {} does not undo the latest step", tx.id)));
                }
                s.redo.push(i);
            }
            TxKind::Redo { of } => {
                let top = s.redo.pop().and_then(|j| log.get(j));
                if top.map(|t| t.id) != Some(of) {
                    return Err(ModelError::CorruptLog(format!("redo {} does not redo the latest undo", tx.id)));
                }
                s.undo.push(i);
            }
        }
    }
    Ok(s)
}

fn apply_all(doc: &Document, ops: &[Op]) -> Result<Document> {
    let mut next = doc.clone();
    for op in ops {
        op.apply(&mut next)?;
    }
    next.check()?;
    Ok(next)
}

fn inverse_ops(ops: &[Op]) -> Vec<Op> {
    ops.iter().rev().map(Op::inverse).collect()
}

impl Ledger {
    /// An empty project. `seed` seeds transaction IDs.
    pub fn new(seed: u64) -> Self {
        Self { doc: Document::default(), log: Vec::new(), persisted: 0, ids: IdAllocator::seeded(seed) }
    }

    /// Rebuild from a log by applying every transaction to an empty document (FILE-02).
    ///
    /// # Errors
    /// [`ModelError::CorruptLog`] when a transaction's preconditions or the invariants fail, or
    /// the undo/redo structure is inconsistent.
    pub fn replay(log: Vec<Transaction>, seed: u64) -> Result<Self> {
        Self::from_checkpoint(Document::default(), 0, log, seed)
    }

    /// Rebuild from a checkpoint holding the state after the first `at` transactions, applying
    /// the rest of `log`. A checkpoint is only a cache (FILE-04): the full log is still required.
    ///
    /// # Errors
    /// As [`Ledger::replay`].
    pub fn from_checkpoint(doc: Document, at: u64, log: Vec<Transaction>, seed: u64) -> Result<Self> {
        let mut doc = doc;
        for (i, tx) in log.iter().enumerate() {
            if tx.seq != i as u64 + 1 {
                return Err(ModelError::CorruptLog(format!(
                    "transaction {} has sequence {} at position {}",
                    tx.id,
                    tx.seq,
                    i + 1
                )));
            }
            if tx.seq > at {
                doc = apply_all(&doc, &tx.ops)
                    .map_err(|e| ModelError::CorruptLog(format!("transaction {}: {e}", tx.seq)))?;
            }
        }
        stacks(&log)?;
        let persisted = log.len() as u64;
        let last = log.last().map_or(0, |t| t.id.0);
        Ok(Self { doc, log, persisted, ids: IdAllocator::seeded_after(seed, last) })
    }

    /// The current document.
    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// The whole log, oldest first.
    pub fn log(&self) -> &[Transaction] {
        &self.log
    }

    /// Sequence number of the last applied transaction (0 when empty).
    pub fn head(&self) -> u64 {
        self.log.len() as u64
    }

    /// Sequence number of the last transaction known to be durably saved.
    pub fn persisted(&self) -> u64 {
        self.persisted
    }

    /// Whether the project has transactions that are not yet saved (SAVE-01).
    pub fn is_unsaved(&self) -> bool {
        self.head() != self.persisted
    }

    /// Record that transactions up to `seq` are durably saved. Called by a store only after its
    /// flush succeeded (SAVE-04).
    ///
    /// # Errors
    /// [`ModelError::Precondition`] when `seq` is past the head.
    pub fn mark_persisted(&mut self, seq: u64) -> Result<()> {
        if seq > self.head() {
            return Err(ModelError::Precondition(format!("persisted position {seq} is past the head {}", self.head())));
        }
        self.persisted = seq;
        Ok(())
    }

    /// The document after `ops`, without committing anything.
    ///
    /// # Errors
    /// Precondition or invariant failures, exactly as [`Ledger::commit`] would report them.
    pub fn preview(&self, ops: &[Op]) -> Result<Document> {
        apply_all(&self.doc, ops)
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
        if ops.is_empty() {
            return Err(ModelError::Empty);
        }
        self.append(author, label.to_string(), TxKind::Edit, ops, time_ms)
    }

    fn append(
        &mut self,
        author: Author,
        label: String,
        kind: TxKind,
        ops: Vec<Op>,
        time_ms: Option<u64>,
    ) -> Result<TxId> {
        let next = apply_all(&self.doc, &ops)?;
        let id: TxId = self.ids.fresh();
        self.log.push(Transaction { id, seq: self.head() + 1, author, label, kind, time_ms, ops });
        self.doc = next;
        Ok(id)
    }

    /// Label of the step [`Ledger::undo`] would undo.
    pub fn undo_label(&self) -> Option<&str> {
        let s = stacks(&self.log).ok()?;
        s.undo.last().and_then(|i| self.log.get(*i)).map(|t| t.label.as_str())
    }

    /// Label of the step [`Ledger::redo`] would redo.
    pub fn redo_label(&self) -> Option<&str> {
        let s = stacks(&self.log).ok()?;
        s.redo.last().and_then(|i| self.log.get(*i)).map(|t| t.label.as_str())
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
    /// As [`Ledger::undo`].
    pub fn undo_at(&mut self, author: Author, time_ms: Option<u64>) -> Result<TxId> {
        let s = stacks(&self.log)?;
        let target = s.undo.last().and_then(|i| self.log.get(*i)).ok_or(ModelError::NothingTo("nothing to undo"))?;
        let (of, label, ops) = (target.id, target.label.clone(), inverse_ops(&target.ops));
        self.append(author, label, TxKind::Undo { of }, ops, time_ms)
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
        let s = stacks(&self.log)?;
        let target = s.redo.last().and_then(|i| self.log.get(*i)).ok_or(ModelError::NothingTo("nothing to redo"))?;
        let (of, label, ops) = (target.id, target.label.clone(), inverse_ops(&target.ops));
        self.append(author, label, TxKind::Redo { of }, ops, time_ms)
    }

    /// Reconstruct the document as it was after transaction `seq`, separately from the live
    /// state (which is not touched).
    ///
    /// # Errors
    /// [`ModelError::Precondition`] when `seq` is past the head; replay failures.
    pub fn state_at(&self, seq: u64) -> Result<Document> {
        if seq > self.head() {
            return Err(ModelError::Precondition(format!("sequence {seq} is past the head {}", self.head())));
        }
        let mut doc = Document::default();
        for tx in self.log.iter().take(seq as usize) {
            doc = apply_all(&doc, &tx.ops)?;
        }
        Ok(doc)
    }

    /// Replay the whole log into a fresh document and compare it with the live one.
    ///
    /// # Errors
    /// Replay failures.
    pub fn replay_matches(&self) -> Result<bool> {
        Ok(self.state_at(self.head())? == self.doc)
    }
}
