//! Transactions and the append-only history (proposal §6).
//!
//! A transaction applies in place with a rollback journal and is checked only where it touched
//! the document ([`crate::invariant`]), so its cost follows the size of the change: on every
//! commit, undo and redo, and per transaction when a file is replayed on open.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::doc::Document;
use crate::error::{ModelError, Result};
use crate::id::{IdAllocator, TxId};
use crate::invariant::Touched;
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
/// - [`Ledger::commit`] applies a transaction's ops in place, in order, then checks the
///   invariants the transaction could have affected ([`Document::check_touched`]) and only then
///   appends it (TX-03). If an op or the check fails, the applied ops are undone with their
///   inverses in reverse order (the rollback journal), leaving the document exactly as it was.
/// - States between the ops of one transaction may break invariants; only the end state is
///   checked. A later op sees what the earlier ops created, changed or removed.
/// - [`Ledger::undo`] and [`Ledger::redo`] append transactions holding concrete inverse ops
///   (UNDO-01). The undo/redo position is derived from the log (UNDO-02): the ledger keeps it in
///   memory, rebuilt on load and extended on every append, and never stores it in the file.
/// - The project is unsaved exactly when [`Ledger::head`] differs from
///   [`Ledger::persisted`] (SAVE-01).
/// - **Verify mode** (debug builds, `ECAD_VERIFY_INVARIANTS=1`, or [`Ledger::replay_verified`])
///   also runs the full [`Document::check`] and the index check after every transaction and
///   treats any disagreement with the scoped check as a bug: a debug assertion, and in release the
///   full check's verdict wins.
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
    /// The undo/redo stacks derived from `log`, kept current.
    stacks: Stacks,
    /// Verify mode for this ledger, on top of the build and environment switches.
    verify: bool,
}

/// The derived undo/redo stacks: log indices.
#[derive(Debug, Clone, Default)]
struct Stacks {
    undo: Vec<usize>,
    redo: Vec<usize>,
}

impl Stacks {
    /// Whether `log[i]` may follow the entries already pushed.
    fn admit(&self, log: &[Transaction], i: usize) -> Result<()> {
        let Some(tx) = log.get(i) else { return Ok(()) };
        let top = |stack: &Vec<usize>| stack.last().and_then(|j| log.get(*j)).map(|t| t.id);
        match tx.kind {
            TxKind::Edit => Ok(()),
            TxKind::Undo { of } if top(&self.undo) != Some(of) => {
                Err(ModelError::CorruptLog(format!("undo {} does not undo the latest step", tx.id)))
            }
            TxKind::Redo { of } if top(&self.redo) != Some(of) => {
                Err(ModelError::CorruptLog(format!("redo {} does not redo the latest undo", tx.id)))
            }
            TxKind::Undo { .. } | TxKind::Redo { .. } => Ok(()),
        }
    }

    /// Account for `log[i]`, after [`Stacks::admit`] accepted it.
    fn push(&mut self, log: &[Transaction], i: usize) {
        let Some(tx) = log.get(i) else { return };
        match tx.kind {
            TxKind::Edit => {
                self.undo.push(i);
                self.redo.clear();
            }
            TxKind::Undo { .. } => {
                self.undo.pop();
                self.redo.push(i);
            }
            TxKind::Redo { .. } => {
                self.redo.pop();
                self.undo.push(i);
            }
        }
    }
}

fn stacks(log: &[Transaction]) -> Result<Stacks> {
    let mut s = Stacks::default();
    for i in 0..log.len() {
        s.admit(log, i)?;
        s.push(log, i);
    }
    Ok(s)
}

/// Whether `ECAD_VERIFY_INVARIANTS=1` is set (read once per process).
fn verify_env() -> bool {
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| std::env::var_os("ECAD_VERIFY_INVARIANTS").is_some_and(|v| v == "1"))
}

/// Whether to run the full check alongside the scoped one.
fn verifying(local: bool) -> bool {
    local || cfg!(debug_assertions) || verify_env()
}

/// Undo `applied` (ops that were applied to `doc`, in order) by applying their inverses in
/// reverse ([`Op::revert`]). Infallible by construction: each inverse meets the state its op
/// produced, and it does not compare recorded values, which could hold NaN.
fn rollback(doc: &mut Document, applied: &[Op]) {
    for op in applied.iter().rev() {
        let undone = op.revert(doc);
        debug_assert!(undone.is_ok(), "rollback of {} failed: {undone:?}", op.name());
    }
}

/// Check the end state of a transaction that touched `touched`.
fn validate(doc: &Document, touched: &Touched, verify: bool) -> Result<()> {
    let scoped = doc.check_touched(touched);
    if verifying(verify) {
        let full = doc.check().and_then(|()| doc.check_index());
        debug_assert_eq!(
            scoped.is_ok(),
            full.is_ok(),
            "scoped check {scoped:?} disagrees with the full check {full:?}"
        );
        return full.and(scoped);
    }
    // On failure, report the full check's first violation, so messages do not depend on which
    // objects were touched.
    scoped.map_err(|e| doc.check().err().unwrap_or(e))
}

/// Apply `ops` to `doc` in place as one transaction: all or nothing.
///
/// # Errors
/// The first op's precondition failure, or an invariant failure of the end state. On error `doc`
/// is exactly as it was.
fn apply_tx(doc: &mut Document, ops: &[Op], verify: bool) -> Result<()> {
    let mut touched = Touched::default();
    let mut applied = 0;
    let mut result = Ok(());
    for op in ops {
        if let Err(e) = op.apply(doc) {
            result = Err(e);
            break;
        }
        touched.add(op);
        applied += 1;
    }
    if result.is_ok() {
        result = validate(doc, &touched, verify);
    }
    if result.is_err() {
        rollback(doc, ops.get(..applied).unwrap_or(&[]));
    }
    result
}

/// A named version must name a transaction before the one naming it (0 is the empty project), so
/// [`Ledger::state_at`] can always resolve it.
fn check_bookmarks(ops: &[Op], seq: u64) -> Result<()> {
    for op in ops {
        if let Op::NameVersion { name, at } = op
            && *at >= seq
        {
            return Err(ModelError::Invariant(format!(
                "version {name:?} names transaction {at}, which does not precede transaction {seq}"
            )));
        }
    }
    Ok(())
}

fn inverse_ops(ops: &[Op]) -> Vec<Op> {
    ops.iter().rev().map(Op::inverse).collect()
}

impl Ledger {
    /// An empty project. `seed` seeds transaction IDs.
    pub fn new(seed: u64) -> Self {
        Self {
            doc: Document::default(),
            log: Vec::new(),
            persisted: 0,
            ids: IdAllocator::seeded(seed),
            stacks: Stacks::default(),
            verify: false,
        }
    }

    /// Rebuild from a log by applying every transaction to an empty document (FILE-02).
    ///
    /// # Errors
    /// [`ModelError::CorruptLog`] when a transaction's preconditions or the invariants fail, or
    /// the undo/redo structure is inconsistent.
    pub fn replay(log: Vec<Transaction>, seed: u64) -> Result<Self> {
        Self::from_checkpoint(Document::default(), 0, log, seed)
    }

    /// [`Ledger::replay`] in verify mode: every transaction is also checked in full, and the
    /// resulting ledger stays in verify mode. Used by the debugger's **Verify replay**.
    ///
    /// # Errors
    /// As [`Ledger::replay`].
    pub fn replay_verified(log: Vec<Transaction>, seed: u64) -> Result<Self> {
        Self::rebuild(Document::default(), 0, log, seed, true)
    }

    /// Replay as much of `log` as is valid: the ledger of the longest valid prefix and, when a
    /// transaction fails, its position in `log` (0-based) and the error. Used by file readers to
    /// open a damaged history read-only at its last good transaction (proposal §20.6).
    ///
    /// # Semantics
    /// Every transaction is checked as it is applied, exactly as when it was committed; a failed
    /// one is rolled back, so the document is the state after the last good transaction.
    pub fn replay_prefix(log: Vec<Transaction>, seed: u64) -> (Self, Option<(usize, ModelError)>) {
        let mut doc = Document::default();
        let mut stacks = Stacks::default();
        let mut failure = None;
        let mut good = 0;
        for (i, tx) in log.iter().enumerate() {
            let step = if tx.seq != i as u64 + 1 {
                Err(ModelError::CorruptLog(format!(
                    "transaction {} has sequence {} at position {}",
                    tx.id,
                    tx.seq,
                    i + 1
                )))
            } else {
                check_bookmarks(&tx.ops, tx.seq)
                    .and_then(|()| apply_tx(&mut doc, &tx.ops, false))
                    .and_then(|()| stacks.admit(&log, i).inspect_err(|_| rollback(&mut doc, &tx.ops)))
            };
            match step {
                Ok(()) => {
                    stacks.push(&log, i);
                    good = i + 1;
                }
                Err(e) => {
                    failure = Some((i, e));
                    break;
                }
            }
        }
        let mut log = log;
        log.truncate(good);
        let persisted = log.len() as u64;
        let last = log.last().map_or(0, |t| t.id.0);
        let ids = IdAllocator::seeded_after(seed, last);
        (Self { doc, log, persisted, ids, stacks, verify: false }, failure)
    }

    /// Rebuild from a checkpoint holding the state after the first `at` transactions, applying
    /// the rest of `log`. A checkpoint is only a cache (FILE-04): the full log is still required.
    ///
    /// # Errors
    /// As [`Ledger::replay`].
    pub fn from_checkpoint(doc: Document, at: u64, log: Vec<Transaction>, seed: u64) -> Result<Self> {
        Self::rebuild(doc, at, log, seed, false)
    }

    fn rebuild(doc: Document, at: u64, log: Vec<Transaction>, seed: u64, verify: bool) -> Result<Self> {
        let mut doc = doc;
        // The checkpoint may have been built without ops; make its index match its objects.
        doc.rebuild_index();
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
                check_bookmarks(&tx.ops, tx.seq)
                    .and_then(|()| apply_tx(&mut doc, &tx.ops, verify))
                    .map_err(|e| ModelError::CorruptLog(format!("transaction {}: {e}", tx.seq)))?;
            }
        }
        let stacks = stacks(&log)?;
        let persisted = log.len() as u64;
        let last = log.last().map_or(0, |t| t.id.0);
        Ok(Self { doc, log, persisted, ids: IdAllocator::seeded_after(seed, last), stacks, verify })
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
    /// # Semantics
    /// Works on a copy of the document (a preview is a second document by nature); the live
    /// document is not touched.
    ///
    /// # Errors
    /// Precondition or invariant failures, exactly as [`Ledger::commit`] would report them.
    pub fn preview(&self, ops: &[Op]) -> Result<Document> {
        let mut next = self.doc.clone();
        apply_tx(&mut next, ops, self.verify)?;
        Ok(next)
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
        let seq = self.head() + 1;
        check_bookmarks(&ops, seq)?;
        apply_tx(&mut self.doc, &ops, self.verify)?;
        let id: TxId = self.ids.fresh();
        self.log.push(Transaction { id, seq, author, label, kind, time_ms, ops });
        let i = self.log.len() - 1;
        if let Err(e) = self.stacks.admit(&self.log, i) {
            // Not reachable through undo/redo (their target is the stack top); kept so a bad
            // kind can never be recorded.
            if let Some(tx) = self.log.pop() {
                rollback(&mut self.doc, &tx.ops);
            }
            return Err(e);
        }
        self.stacks.push(&self.log, i);
        Ok(id)
    }

    /// Label of the step [`Ledger::undo`] would undo.
    pub fn undo_label(&self) -> Option<&str> {
        self.stacks.undo.last().and_then(|i| self.log.get(*i)).map(|t| t.label.as_str())
    }

    /// Label of the step [`Ledger::redo`] would redo.
    pub fn redo_label(&self) -> Option<&str> {
        self.stacks.redo.last().and_then(|i| self.log.get(*i)).map(|t| t.label.as_str())
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
        let target =
            self.stacks.undo.last().and_then(|i| self.log.get(*i)).ok_or(ModelError::NothingTo("nothing to undo"))?;
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
        let target =
            self.stacks.redo.last().and_then(|i| self.log.get(*i)).ok_or(ModelError::NothingTo("nothing to redo"))?;
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
            apply_tx(&mut doc, &tx.ops, self.verify)?;
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
