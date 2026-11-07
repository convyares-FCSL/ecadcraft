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
/// - **Verify mode** (the `verify-invariants` cargo feature, which every `cargo test`
///   turns on; `ECAD_VERIFY_INVARIANTS=1`; or [`Ledger::replay_verified`]) also runs the full
///   [`Document::check`] and the index check after every transaction and treats any
///   disagreement with the scoped check as a bug: a debug assertion once the transaction is
///   settled, and the full check's verdict wins.
/// - **Panics:** the rollback journal is a drop guard, so a panic in the middle of a transaction
///   (caught further up, for example by `catch_unwind`) still leaves the document and log as they
///   were. A revert that fails (a bug) is repaired by replaying the log and reported in
///   [`Ledger::diagnostics`]; if even that fails, the ledger refuses further changes.
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
    /// Rollback failures (never expected) and whether they left the document unusable.
    health: Health,
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
        self.admit_kind(log, tx.kind, tx.id)
    }

    /// Whether transaction `id`, of `kind`, may follow the entries already pushed.
    fn admit_kind(&self, log: &[Transaction], kind: TxKind, id: impl std::fmt::Display) -> Result<()> {
        let top = |stack: &Vec<usize>| stack.last().and_then(|j| log.get(*j)).map(|t| t.id);
        match kind {
            TxKind::Edit => Ok(()),
            TxKind::Undo { of } if top(&self.undo) != Some(of) => {
                Err(ModelError::CorruptLog(format!("undo {id} does not undo the latest step")))
            }
            TxKind::Redo { of } if top(&self.redo) != Some(of) => {
                Err(ModelError::CorruptLog(format!("redo {id} does not redo the latest undo")))
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
    *FLAG.get_or_init(|| switch_on(std::env::var_os("ECAD_VERIFY_INVARIANTS").as_deref()))
}

/// Only the value `1` turns verify mode on.
fn switch_on(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|v| v == "1")
}

/// Whether to run the full check alongside the scoped one.
fn verifying(local: bool) -> bool {
    local || cfg!(feature = "verify-invariants") || verify_env()
}

/// What went wrong with rollbacks. Empty in any correct build.
#[derive(Debug, Clone, Default)]
struct Health {
    /// One line per rollback that could not revert an op.
    diagnostics: Vec<String>,
    /// A failed rollback could not be repaired by replaying the log, so the document may be
    /// corrupt: the ledger refuses further changes.
    broken: bool,
}

/// The document after replaying `history` (already committed transactions), without checks.
fn restore(history: &[Transaction]) -> Option<Document> {
    let mut doc = Document::default();
    for tx in history {
        for op in &tx.ops {
            op.apply(&mut doc).ok()?;
        }
    }
    Some(doc)
}

/// Test-only fault injection.
#[cfg(test)]
mod hooks {
    use std::cell::Cell;

    thread_local! {
        /// Panic when this many ops of a transaction have been applied.
        pub(super) static PANIC_AT: Cell<Option<usize>> = const { Cell::new(None) };
        /// Treat every revert as failed.
        pub(super) static FAIL_REVERT: Cell<bool> = const { Cell::new(false) };
    }

    pub(super) fn maybe_panic(applied: usize) {
        if PANIC_AT.with(Cell::get) == Some(applied) {
            std::panic::resume_unwind(Box::new("injected panic"));
        }
    }

    pub(super) fn fail_revert() -> bool {
        FAIL_REVERT.with(Cell::get)
    }
}

/// The rollback journal of one transaction: the ops applied to the live document so far.
///
/// # Semantics
/// Unless [`Journal::commit`] is called, dropping the journal reverts the applied ops in reverse
/// order. That includes unwinding from a panic in the middle of a transaction, so the document
/// never keeps part of a transaction. If a revert fails (it cannot, by construction), the
/// document is rebuilt by replaying `history`, the committed log it reflects; if that is not
/// possible either, `health.broken` is set. Nothing here panics, as it may run during unwinding.
struct Journal<'a> {
    doc: &'a mut Document,
    ops: &'a [Op],
    applied: usize,
    /// The committed transactions whose replay gives the state before this one; `None` when the
    /// caller discards the document on error.
    history: Option<&'a [Transaction]>,
    health: &'a mut Health,
    open: bool,
}

impl<'a> Journal<'a> {
    fn new(doc: &'a mut Document, ops: &'a [Op], history: Option<&'a [Transaction]>, health: &'a mut Health) -> Self {
        Self { doc, ops, applied: 0, history, health, open: true }
    }

    /// Apply every op in order; what they touched.
    fn apply(&mut self) -> Result<Touched> {
        let mut touched = Touched::default();
        for op in self.ops {
            #[cfg(test)]
            hooks::maybe_panic(self.applied);
            op.apply(self.doc)?;
            touched.add(op);
            self.applied += 1;
        }
        #[cfg(test)]
        hooks::maybe_panic(self.applied);
        Ok(touched)
    }

    /// Keep the applied ops.
    fn commit(mut self) {
        self.open = false;
    }

    /// Revert the applied ops. Returns the failure, if a revert failed (the document has then
    /// been restored from the history when possible).
    fn roll_back(&mut self) -> Option<String> {
        if !self.open {
            return None;
        }
        self.open = false;
        let applied = self.ops.get(..self.applied).unwrap_or(&[]);
        self.applied = 0;
        let mut failed = None;
        for op in applied.iter().rev() {
            #[cfg(test)]
            if hooks::fail_revert() {
                failed = Some(format!("rollback of {} failed: injected", op.name()));
                break;
            }
            if let Err(e) = op.revert(self.doc) {
                failed = Some(format!("rollback of {} failed: {e}", op.name()));
                break;
            }
        }
        let failed = failed?;
        let repaired = match self.history.map(restore) {
            Some(Some(doc)) => {
                *self.doc = doc;
                format!("{failed}; the document was rebuilt from the log")
            }
            Some(None) => {
                self.health.broken = true;
                format!("{failed}; rebuilding the document from the log failed too")
            }
            None => format!("{failed}; the document was discarded"),
        };
        self.health.diagnostics.push(repaired.clone());
        Some(repaired)
    }
}

impl Drop for Journal<'_> {
    fn drop(&mut self) {
        self.roll_back();
    }
}

/// Check the end state of a transaction that touched `touched`. Also returns a disagreement
/// between the scoped and the full check, found in verify mode, for the caller to assert on once
/// the transaction is settled.
fn validate(doc: &Document, touched: &Touched, verify: bool) -> (Result<()>, Option<String>) {
    let scoped = doc.check_touched(touched);
    if verifying(verify) {
        let full = doc.check().and_then(|()| doc.check_index());
        let disagreement = (scoped.is_ok() != full.is_ok())
            .then(|| format!("scoped check {scoped:?} disagrees with the full check {full:?}"));
        return (full.and(scoped), disagreement);
    }
    // On failure, report the full check's first violation, so messages do not depend on which
    // objects were touched.
    (scoped.map_err(|e| doc.check().err().unwrap_or(e)), None)
}

/// Apply `ops` to `doc` in place as one transaction, then run `then`: all or nothing.
///
/// # Errors
/// The first op's precondition failure, an invariant failure of the end state, or `then`'s
/// error. On error, or if anything panics, `doc` is exactly as it was (see [`Journal`]).
fn apply_tx(
    doc: &mut Document,
    ops: &[Op],
    verify: bool,
    history: Option<&[Transaction]>,
    health: &mut Health,
    then: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let mut journal = Journal::new(doc, ops, history, health);
    let (result, disagreement) = match journal.apply() {
        Ok(touched) => validate(journal.doc, &touched, verify),
        Err(e) => (Err(e), None),
    };
    let result = result.and_then(|()| then());
    let rollback_failure = match result {
        Ok(()) => {
            journal.commit();
            None
        }
        Err(_) => journal.roll_back(),
    };
    // Assertions only now, with the document settled: a panic here loses nothing.
    debug_assert!(rollback_failure.is_none(), "{rollback_failure:?}");
    debug_assert!(disagreement.is_none(), "{disagreement:?}");
    result
}

/// Apply `ops` to `doc` in place, check the end state as [`apply_tx`] does, run `read` on it, and
/// always roll back (also when `read` panics: the [`Journal`] reverts on drop). Returns `read`'s
/// result and, apart from the result, any rollback failure (the document has then been restored
/// from `history` when possible).
fn applied_then_reverted<R>(
    doc: &mut Document,
    ops: &[Op],
    verify: bool,
    history: Option<&[Transaction]>,
    health: &mut Health,
    read: impl FnOnce(&Document) -> R,
) -> (Result<R>, Option<String>) {
    let mut journal = Journal::new(doc, ops, history, health);
    let (checked, disagreement) = match journal.apply() {
        Ok(touched) => validate(journal.doc, &touched, verify),
        Err(e) => (Err(e), None),
    };
    let result = checked.map(|()| read(journal.doc));
    let rollback_failure = journal.roll_back();
    drop(journal);
    // Assertions only now, with the document settled: a panic here loses nothing.
    debug_assert!(rollback_failure.is_none(), "{rollback_failure:?}");
    debug_assert!(disagreement.is_none(), "{disagreement:?}");
    (result, rollback_failure)
}

impl Document {
    /// Run `read` on the document as it would be after `ops`, without copying it and without
    /// keeping the change.
    ///
    /// # Semantics
    /// The ops are applied in place, in order, and the end state is checked exactly as
    /// [`Ledger::commit`] checks it (only where the ops touched the document; in verify mode also
    /// in full). Only if that succeeds does `read` run, on the changed document. Afterwards the
    /// ops are always reverted in reverse order, leaving the document, its indexes and
    /// everything `==` compares exactly as they were, also when `read` panics (the rollback
    /// journal reverts on unwinding). The cost follows the size of `ops`, not of the document.
    ///
    /// # Errors
    /// The first op's precondition failure or the end state's invariant failure, with the same
    /// error [`Ledger::commit`] would report; `read` has then not run. In the never expected case
    /// that reverting an op fails, [`ModelError::Invariant`] says so: the document may then no
    /// longer be valid and should be discarded.
    ///
    /// # Example
    /// ```rust
    /// use ecad_model::{Document, Op, Page, PageId};
    /// let mut doc = Document::default();
    /// let add = vec![Op::AddPage { id: PageId(1), page: Page { title: "1".into() }, index: 0 }];
    /// let pages = doc.with_applied(&add, |d| d.pages.len()).unwrap();
    /// assert_eq!(pages, 1);
    /// assert!(doc.pages.is_empty()); // reverted
    /// ```
    pub fn with_applied<R>(&mut self, ops: &[Op], read: impl FnOnce(&Document) -> R) -> Result<R> {
        let (result, failure) = applied_then_reverted(self, ops, false, None, &mut Health::default(), read);
        match failure {
            Some(f) => Err(ModelError::Invariant(format!("{f}: the document may no longer be valid"))),
            None => result,
        }
    }
}

/// [`apply_tx`] for a scratch document that the caller discards on error.
fn apply_scratch(doc: &mut Document, ops: &[Op], verify: bool) -> Result<()> {
    apply_tx(doc, ops, verify, None, &mut Health::default(), || Ok(()))
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
            health: Health::default(),
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
        let mut health = Health::default();
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
                check_bookmarks(&tx.ops, tx.seq).and_then(|()| {
                    let admit = || stacks.admit(&log, i);
                    apply_tx(&mut doc, &tx.ops, false, log.get(..i), &mut health, admit)
                })
            };
            if health.broken {
                // Never expected: a rollback failed and the good prefix did not replay either.
                // Open nothing rather than a document that may be corrupt.
                doc = Document::default();
                stacks = Stacks::default();
                good = 0;
                let why = health.diagnostics.last().cloned().unwrap_or_default();
                failure = Some((i, ModelError::CorruptLog(why)));
                break;
            }
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
        (Self { doc, log, persisted, ids, stacks, verify: false, health }, failure)
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
        let mut health = Health::default();
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
                    .and_then(|()| apply_tx(&mut doc, &tx.ops, verify, log.get(..i), &mut health, || Ok(())))
                    .map_err(|e| ModelError::CorruptLog(format!("transaction {}: {e}", tx.seq)))?;
            }
        }
        let stacks = stacks(&log)?;
        let persisted = log.len() as u64;
        let last = log.last().map_or(0, |t| t.id.0);
        Ok(Self { doc, log, persisted, ids: IdAllocator::seeded_after(seed, last), stacks, verify, health })
    }

    /// Rollback failures since this ledger was built: one line each, naming the op and how the
    /// document was repaired. Always empty unless there is a bug; the debugger or a store may
    /// report it.
    pub fn diagnostics(&self) -> &[String] {
        &self.health.diagnostics
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
    /// `read` has not run); the ledger refuses, as it refuses commits, after a rollback it could
    /// not repair.
    pub fn with_applied<R>(&mut self, ops: &[Op], read: impl FnOnce(&Document) -> R) -> Result<R> {
        if self.health.broken {
            return Err(ModelError::CorruptLog(
                "the document could not be restored after a failed rollback; reopen the project".into(),
            ));
        }
        applied_then_reverted(&mut self.doc, ops, self.verify, Some(&self.log), &mut self.health, read).0
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
        if self.health.broken {
            return Err(ModelError::CorruptLog(
                "the document could not be restored after a failed rollback; reopen the project".into(),
            ));
        }
        let seq = self.head() + 1;
        check_bookmarks(&ops, seq)?;
        // Not refused through undo/redo (their target is the stack top); kept so a bad kind can
        // never be recorded.
        let (stacks, log) = (&self.stacks, &self.log);
        let admit = || stacks.admit_kind(log, kind, "the new transaction");
        apply_tx(&mut self.doc, &ops, self.verify, Some(log), &mut self.health, admit)?;
        let id: TxId = self.ids.fresh();
        self.log.push(Transaction { id, seq, author, label, kind, time_ms, ops });
        self.stacks.push(&self.log, self.log.len() - 1);
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
            apply_scratch(&mut doc, &tx.ops, self.verify)?;
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::*;
    use crate::doc::{Cable, CableMember, Component, Conductor, End, Layer, Page, Terminal};
    use crate::id::{CableId, ComponentId, ConductorId, LayerId, PageId, TerminalId};

    fn json(doc: &Document) -> String {
        serde_json::to_string(doc).unwrap()
    }

    /// A ledger with a component, two terminals, a conductor on the first and a cable holding it.
    fn ledger() -> Ledger {
        let mut l = Ledger::new(3);
        let (c, w) = (ComponentId(3), ConductorId(6));
        l.commit(
            Author::User,
            "setup",
            vec![
                Op::AddPage { id: PageId(1), page: Page { title: "1".into() }, index: 0 },
                Op::AddLayer { id: LayerId(2), layer: Layer { name: "0".into() } },
                Op::CreateComponent { id: c, component: Component::default() },
                Op::AddTerminal { id: TerminalId(4), terminal: Terminal { component: c, label: None, key: None } },
                Op::AddTerminal { id: TerminalId(5), terminal: Terminal { component: c, label: None, key: None } },
                Op::CreateConductor { id: w, conductor: Conductor::default() },
                Op::AttachEnd { conductor: w, end: End::A, before: None, after: Some(TerminalId(4)) },
                Op::CreateCable { id: CableId(7), cable: Cable::default() },
                Op::AddCableMember { cable: CableId(7), index: 0, member: CableMember { conductor: w, core: None } },
            ],
        )
        .unwrap();
        l
    }

    /// A valid three-op transaction touching indexes.
    fn three_ops() -> Vec<Op> {
        vec![
            Op::AttachEnd { conductor: ConductorId(6), end: End::B, before: None, after: Some(TerminalId(5)) },
            Op::RemoveCableMember {
                cable: CableId(7),
                index: 0,
                member: CableMember { conductor: ConductorId(6), core: None },
            },
            Op::AddPage { id: PageId(8), page: Page { title: "2".into() }, index: 1 },
        ]
    }

    fn assert_unchanged(l: &Ledger, doc: &str, head: u64) {
        assert_eq!(json(l.document()), doc);
        l.document().check_index().unwrap();
        assert_eq!(l.head(), head);
        assert_eq!(l.state_at(head).unwrap(), *l.document());
        assert_eq!(l.document().ends_at(TerminalId(4)), vec![(ConductorId(6), End::A)]);
        assert!(l.document().ends_at(TerminalId(5)).is_empty());
        assert_eq!(l.document().cables_of(ConductorId(6)), vec![CableId(7)]);
    }

    #[test]
    fn a_panic_in_the_middle_of_a_transaction_leaves_document_indexes_and_log_as_they_were() {
        // Panic before op 1, between ops, and after the last op (in the end check's place).
        for at in 0..=3 {
            let mut l = ledger();
            let (doc, head) = (json(l.document()), l.head());
            hooks::PANIC_AT.with(|c| c.set(Some(at)));
            let r = catch_unwind(AssertUnwindSafe(|| l.commit(Author::User, "boom", three_ops())));
            hooks::PANIC_AT.with(|c| c.set(None));
            assert!(r.is_err(), "panicked at {at}");
            assert_unchanged(&l, &doc, head);
            assert!(l.diagnostics().is_empty());
            // The ledger carries on: the same transaction now commits, replays and undoes.
            l.commit(Author::User, "again", three_ops()).unwrap();
            assert_eq!(Ledger::replay(l.log().to_vec(), 0).unwrap().document(), l.document());
            l.undo(Author::User).unwrap();
            assert_unchanged(&l, &doc, head + 2);
        }
    }

    #[test]
    fn a_panic_during_undo_also_rolls_back() {
        let mut l = ledger();
        l.commit(Author::User, "edit", three_ops()).unwrap();
        let (doc, head) = (json(l.document()), l.head());
        hooks::PANIC_AT.with(|c| c.set(Some(2)));
        let r = catch_unwind(AssertUnwindSafe(|| l.undo(Author::User)));
        hooks::PANIC_AT.with(|c| c.set(None));
        assert!(r.is_err());
        assert_eq!(json(l.document()), doc);
        assert_eq!(l.head(), head);
        l.document().check_index().unwrap();
        assert_eq!(l.undo_label(), Some("edit"));
    }

    #[test]
    fn a_failed_revert_rebuilds_the_document_from_the_log() {
        let mut l = ledger();
        let (doc, head) = (json(l.document()), l.head());
        // Ops that apply, then an end state that fails: a conductor end on a missing terminal.
        let ops = vec![
            Op::AddPage { id: PageId(8), page: Page { title: "2".into() }, index: 1 },
            Op::AttachEnd { conductor: ConductorId(6), end: End::B, before: None, after: Some(TerminalId(99)) },
        ];
        hooks::FAIL_REVERT.with(|c| c.set(true));
        // The debug assertion fires only after the document has been rebuilt.
        let r = catch_unwind(AssertUnwindSafe(|| l.commit(Author::User, "bad", ops)));
        hooks::FAIL_REVERT.with(|c| c.set(false));
        match r {
            Ok(r) => assert!(matches!(r, Err(ModelError::Invariant(_))), "{r:?}"),
            Err(_) => assert!(std::hint::black_box(cfg!(debug_assertions)), "only the debug assertion may panic"),
        }
        assert_unchanged(&l, &doc, head);
        assert_eq!(l.diagnostics().len(), 1, "{:?}", l.diagnostics());
        assert!(l.diagnostics().iter().all(|d| d.contains("rebuilt from the log")), "{:?}", l.diagnostics());
        l.commit(Author::User, "fine", three_ops()).unwrap();
    }

    #[test]
    fn a_failed_revert_on_a_damaged_file_opens_at_the_last_good_transaction() {
        let mut l = ledger();
        l.commit(Author::User, "edit", three_ops()).unwrap();
        let good = l.document().clone();
        let mut log = l.log().to_vec();
        let mut bad = log.get(1).cloned().unwrap();
        bad.seq = 3;
        bad.ops = vec![
            Op::AddLayer { id: LayerId(50), layer: Layer { name: "x".into() } },
            Op::RemoveTerminal {
                id: TerminalId(5),
                terminal: Terminal { component: ComponentId(3), label: None, key: None },
            },
        ];
        log.push(bad);
        hooks::FAIL_REVERT.with(|c| c.set(true));
        let r = catch_unwind(AssertUnwindSafe(|| Ledger::replay_prefix(log.clone(), 0)));
        hooks::FAIL_REVERT.with(|c| c.set(false));
        match r {
            Ok((opened, failure)) => {
                assert_eq!(opened.document(), &good);
                opened.document().check_index().unwrap();
                assert!(matches!(failure, Some((2, ModelError::Invariant(_)))), "{failure:?}");
                assert_eq!(opened.diagnostics().len(), 1);
            }
            Err(_) => assert!(std::hint::black_box(cfg!(debug_assertions)), "only the debug assertion may panic"),
        }
    }

    #[test]
    fn only_the_value_1_turns_the_switch_on() {
        use std::ffi::OsStr;
        assert!(switch_on(Some(OsStr::new("1"))));
        for v in ["0", "", "true", "yes", " 1"] {
            assert!(!switch_on(Some(OsStr::new(v))), "{v:?}");
        }
        assert!(!switch_on(None));
    }

    #[test]
    fn the_test_build_verifies() {
        assert!(verifying(false), "every cargo test runs in verify mode");
    }

    #[test]
    fn with_applied_shows_the_changed_document_and_reverts_it() {
        let mut l = ledger();
        let (doc, head) = (json(l.document()), l.head());
        let seen = l
            .with_applied(&three_ops(), |d| {
                (d.ends_at(TerminalId(5)), d.cables_of(ConductorId(6)), d.pages.len(), d.check().is_ok())
            })
            .unwrap();
        assert_eq!(seen, (vec![(ConductorId(6), End::B)], vec![], 2, true));
        assert_unchanged(&l, &doc, head);
        // The plain document form does the same.
        let mut d = l.document().clone();
        assert_eq!(d.with_applied(&three_ops(), |d| d.pages.len()).unwrap(), 2);
        assert_eq!(json(&d), doc);
        d.check_index().unwrap();
    }

    #[test]
    fn with_applied_refuses_as_commit_does_without_running_the_closure() {
        let mut l = ledger();
        let (doc, head) = (json(l.document()), l.head());
        let bad_end = vec![
            Op::AddPage { id: PageId(8), page: Page { title: "2".into() }, index: 1 },
            Op::AttachEnd { conductor: ConductorId(6), end: End::B, before: None, after: Some(TerminalId(99)) },
        ];
        let bad_before =
            vec![Op::AttachEnd { conductor: ConductorId(6), end: End::A, before: Some(TerminalId(5)), after: None }];
        for ops in [bad_end, bad_before] {
            let mut ran = false;
            let previewed = l.with_applied(&ops, |_| ran = true).map_err(|e| e.to_string());
            assert!(!ran, "the closure runs only on a valid end state");
            let committed = l.clone().commit(Author::User, "x", ops).map(|_| ()).map_err(|e| e.to_string());
            assert!(previewed.is_err());
            assert_eq!(previewed, committed, "the same error as a commit");
            assert_unchanged(&l, &doc, head);
        }
    }

    #[test]
    fn a_panic_inside_the_with_applied_closure_leaves_document_indexes_and_log_as_they_were() {
        let mut l = ledger();
        let (doc, head) = (json(l.document()), l.head());
        let r = catch_unwind(AssertUnwindSafe(|| {
            l.with_applied(&three_ops(), |d| {
                assert_eq!(d.pages.len(), 2, "the closure sees the change");
                std::panic::resume_unwind(Box::new("panic in the closure"));
            })
        }));
        assert!(r.is_err());
        assert_unchanged(&l, &doc, head);
        assert!(l.diagnostics().is_empty());
        // A panic while the ops are being applied is reverted too.
        for at in 0..=3 {
            hooks::PANIC_AT.with(|c| c.set(Some(at)));
            let r = catch_unwind(AssertUnwindSafe(|| l.with_applied(&three_ops(), |_| ())));
            hooks::PANIC_AT.with(|c| c.set(None));
            assert!(r.is_err(), "panicked at {at}");
            assert_unchanged(&l, &doc, head);
        }
        // And the plain document form.
        let mut d = l.document().clone();
        let r = catch_unwind(AssertUnwindSafe(|| {
            d.with_applied(&three_ops(), |_| std::panic::resume_unwind(Box::new("boom")))
        }));
        assert!(r.is_err());
        assert_eq!(json(&d), doc);
        d.check_index().unwrap();
        // The ledger carries on.
        l.commit(Author::User, "edit", three_ops()).unwrap();
    }

    #[test]
    fn with_applied_runs_the_full_check_in_verify_mode() {
        // A terminal written straight into the map: the full check passes, the index check does
        // not, and the scoped check of `three_ops` never looks at it. Only verify mode notices.
        let mut l = ledger();
        l.doc.terminals.insert(TerminalId(50), Terminal { component: ComponentId(3), label: None, key: None });
        let doc = json(l.document());
        let r = catch_unwind(AssertUnwindSafe(|| l.with_applied(&three_ops(), |_| ())));
        match r {
            Ok(r) => assert!(matches!(r, Err(ModelError::Invariant(_))), "{r:?}"),
            // The disagreement between the scoped and the full check is asserted in debug builds,
            // once the document is settled.
            Err(_) => assert!(std::hint::black_box(cfg!(debug_assertions)), "only the debug assertion may panic"),
        }
        assert_eq!(json(l.document()), doc);
    }
}
