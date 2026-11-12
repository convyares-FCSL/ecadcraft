//! Transactions and the append-only history (proposal §6).
//!
//! A transaction applies in place with a rollback journal and is checked only where it touched
//! the document ([`crate::invariant`]), so its cost follows the size of the change: on every
//! commit, undo and redo, and per transaction when a file is replayed on open.

use std::collections::BTreeMap;
use std::sync::Arc;

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
    /// A merge of another branch into this one (brief 020b, FQ1): ordinary ops, and where they
    /// came from. On the undo stack it is an edit; replay applies its ops and reads nothing else.
    Merge {
        /// The source branch (the file's 128-bit branch ID).
        source: [u8; 16],
        /// The source's head that was merged.
        source_head: StepPoint,
        /// The common ancestor the merge compared against.
        ancestor: BranchPoint,
    },
}

impl TxKind {
    /// Whether the step is a new change on the undo stack (an edit or a merge).
    pub fn is_edit(self) -> bool {
        matches!(self, TxKind::Edit | TxKind::Merge { .. })
    }
}

/// A step of a branch: its `seq` and chain value (the file's hash chain, brief 021).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepPoint {
    pub seq: u64,
    pub chain: [u8; 32],
}

/// A step of a named branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchPoint {
    pub branch: [u8; 16],
    pub seq: u64,
    pub chain: [u8; 32],
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
///   turns on; `ECAD_VERIFY_INVARIANTS=1`; [`Ledger::replay_verified`]; or [`Check::Full`]) also runs the full
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
/// ledger.commit(Author::User, "New page", vec![Op::AddPage { id: page, page: Page::new("1"), index: 0 }]).unwrap();
/// assert!(ledger.is_unsaved());
/// ledger.undo(Author::User).unwrap();
/// assert!(ledger.document().pages.is_empty());
/// assert_eq!(ledger.head(), 2); // the undo was appended
/// ```
#[derive(Debug, Clone)]
pub struct Ledger {
    doc: Document,
    /// The transactions after [`Ledger::base`], oldest first: `log[i].seq == base + i + 1`.
    log: Vec<Transaction>,
    /// The sequence number the in-memory log starts after: 0 for a ledger replayed from the
    /// project root, a snapshot's `seq` for one opened from a snapshot (brief 021).
    base: u64,
    /// Where transactions before `base` are read on demand (`None` when `base` is 0).
    history: Option<Arc<dyn History>>,
    /// Transactions before `base` read so far: always the tops of the undo and redo stacks when
    /// they lie before `base` (read again after every step that exposes a new top).
    older: BTreeMap<u64, Transaction>,
    persisted: u64,
    ids: IdAllocator,
    /// The undo/redo stacks derived from `log`, kept current.
    stacks: Stacks,
    /// Verify mode for this ledger, on top of the build and environment switches.
    verify: bool,
    /// Rollback failures (never expected) and whether they left the document unusable.
    health: Health,
    /// How often the whole undo and redo stacks were read from the history (at most once).
    stack_rebuilds: u32,
    /// Stack entries read from the history at a time ([`Base::stack_cap`]).
    stack_read: u64,
}

/// The derived undo/redo stacks: the `seq` and ID of each entry, so that admitting a step never
/// needs the transaction itself (which may lie before the ledger's base).
#[derive(Debug, Clone, Default)]
struct Stacks {
    undo: Vec<(u64, TxId)>,
    redo: Vec<(u64, TxId)>,
    /// Entries below the visible ones that a snapshot did not record (brief 021: a snapshot
    /// keeps the top of each stack and its depth). They are read once, from the history, when
    /// undo or redo first reaches them ([`Ledger::rebuild_stacks`]).
    undo_hidden: u64,
    redo_hidden: u64,
    /// How many entries of the base's stacks (from their tops) have been read so far.
    undo_known: u64,
    redo_known: u64,
}

/// How many stack entries are read from the history at a time, when undo or redo reaches below
/// what is held (the cap a snapshot records, brief 021).
pub const STACK_READ: u64 = 1_024;

impl Stacks {
    /// Whether `kind` would pop a stack whose visible part is empty but which has hidden entries.
    fn needs_full(&self, kind: TxKind) -> bool {
        match kind {
            TxKind::Edit | TxKind::Merge { .. } => false,
            TxKind::Undo { .. } => self.undo.is_empty() && self.undo_hidden > 0,
            TxKind::Redo { .. } => self.redo.is_empty() && self.redo_hidden > 0,
        }
    }

    /// Whether either stack's visible part is empty while it has hidden entries.
    fn top_hidden(&self) -> bool {
        (self.undo.is_empty() && self.undo_hidden > 0) || (self.redo.is_empty() && self.redo_hidden > 0)
    }
    /// Whether transaction `id`, of `kind`, may follow the entries already pushed.
    fn admit_kind(&self, kind: TxKind, id: impl std::fmt::Display) -> Result<()> {
        let top = |stack: &Vec<(u64, TxId)>| stack.last().map(|(_, t)| *t);
        match kind {
            TxKind::Edit | TxKind::Merge { .. } => Ok(()),
            TxKind::Undo { of } if top(&self.undo) != Some(of) => Err(ModelError::CorruptLog(format!("undo {id} does not undo the latest step"))),
            TxKind::Redo { of } if top(&self.redo) != Some(of) => Err(ModelError::CorruptLog(format!("redo {id} does not redo the latest undo"))),
            TxKind::Undo { .. } | TxKind::Redo { .. } => Ok(()),
        }
    }

    /// Whether `tx` may follow the entries already pushed.
    fn admit(&self, tx: &Transaction) -> Result<()> {
        self.admit_kind(tx.kind, tx.id)
    }

    /// Account for `tx`, after [`Stacks::admit`] accepted it.
    fn push(&mut self, tx: &Transaction) {
        match tx.kind {
            TxKind::Edit | TxKind::Merge { .. } => {
                self.undo.push((tx.seq, tx.id));
                self.redo.clear();
                self.redo_hidden = 0;
            }
            TxKind::Undo { .. } => {
                self.undo.pop();
                self.redo.push((tx.seq, tx.id));
            }
            TxKind::Redo { .. } => {
                self.redo.pop();
                self.undo.push((tx.seq, tx.id));
            }
        }
    }
}

fn stacks(log: &[Transaction]) -> Result<Stacks> {
    let mut s = Stacks::default();
    for tx in log {
        s.admit(tx)?;
        s.push(tx);
    }
    Ok(s)
}

/// Where a ledger opened from a snapshot reads the history before its base (brief 021): a
/// project file's store. Reads only; never changes the file.
pub trait History: std::fmt::Debug + Send + Sync {
    /// Transaction `seq` (before the ledger's base).
    ///
    /// # Errors
    /// The store could not be read, or holds no such transaction.
    fn transaction(&self, seq: u64) -> Result<Transaction>;

    /// The document after transaction `seq` (at or before the ledger's base), rebuilt from the
    /// nearest snapshot before it and the transactions after that snapshot.
    ///
    /// # Errors
    /// As [`History::transaction`], and replay failures.
    fn state_at(&self, seq: u64) -> Result<Document>;

    /// Transactions `from..=to` (all before the ledger's base), in order.
    ///
    /// # Errors
    /// As [`History::transaction`].
    fn transactions(&self, from: u64, to: u64) -> Result<Vec<Transaction>> {
        (from..=to).map(|s| self.transaction(s)).collect()
    }

    /// The whole undo and redo stacks after transaction `seq` (at or before the ledger's base),
    /// derived from the transactions' kinds: reads every transaction up to `seq`, so it is used
    /// once per session at most, when undo or redo first goes below what a snapshot recorded.
    ///
    /// # Errors
    /// As [`History::transaction`]; an inconsistent undo/redo structure.
    fn stacks_at(&self, seq: u64) -> Result<(Vec<(u64, TxId)>, Vec<(u64, TxId)>)> {
        stacks_from(self, seq)
    }

    /// Entries `skip .. skip + count` counted from the top of the undo (or redo) stack after
    /// transaction `seq`, oldest first (fewer when the stack is shallower). A store walks the
    /// history backwards from `seq` only as far as those entries go.
    ///
    /// # Errors
    /// As [`History::stacks_at`].
    fn stack_slice(&self, seq: u64, undo: bool, skip: u64, count: u64) -> Result<Vec<(u64, TxId)>> {
        let (u, r) = self.stacks_at(seq)?;
        let s = if undo { u } else { r };
        let end = s.len().saturating_sub(usize::try_from(skip).unwrap_or(usize::MAX));
        let start = end.saturating_sub(usize::try_from(count).unwrap_or(usize::MAX));
        Ok(s.get(start..end).unwrap_or_default().to_vec())
    }
}

/// The whole undo and redo stacks after transaction `seq`, from the kinds of every transaction
/// up to it.
fn stacks_from<H: History + ?Sized>(h: &H, seq: u64) -> Result<(Vec<(u64, TxId)>, Vec<(u64, TxId)>)> {
    {
        let mut s = Stacks::default();
        let mut from = 1;
        while from <= seq {
            let to = seq.min(from.saturating_add(4_095));
            for tx in h.transactions(from, to)? {
                s.admit(&tx)?;
                s.push(&tx);
            }
            let Some(next) = to.checked_add(1) else { break };
            from = next;
        }
        Ok((s.undo, s.redo))
    }
}

/// A snapshot a ledger starts from instead of the project root (brief 021): the state after
/// transaction `seq`, the undo and redo stacks there (as `(seq, id)` pairs, oldest first), and
/// where the history before it is read.
#[derive(Debug, Clone)]
pub struct Base {
    pub seq: u64,
    pub doc: Document,
    /// The top of each stack (all of it, or only the newest entries a snapshot recorded).
    pub undo: Vec<(u64, TxId)>,
    pub redo: Vec<(u64, TxId)>,
    /// How many entries lie below `undo` and `redo` (0 when they are whole).
    pub undo_hidden: u64,
    pub redo_hidden: u64,
    /// `doc`'s derived index is already built (it is built here otherwise).
    pub indexed: bool,
    /// The cap the snapshot recorded its stacks with: entries below them are read from the
    /// history this many at a time (0: the stacks are whole; [`STACK_READ`] is then used).
    pub stack_cap: u64,
    pub history: Arc<dyn History>,
}

pub use crate::dev::verify_mode;

/// How much a replay checks after each transaction ([`Ledger::replay_checked_from`]), whatever
/// the build's verify mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// Exactly what [`Ledger::commit`] checks outside verify mode: the objects the transaction
    /// touched. The cost follows the size of each transaction. The file's **Verify** uses this.
    Commit,
    /// The full [`Document::check`] and the index check as well, after every transaction (brief
    /// 013's verify mode): the cost follows the size of the document, per transaction.
    Full,
}

/// What [`validate`] checks: the build's choice (the scoped check, plus the full one in verify
/// mode or when `true`), or exactly one [`Check`].
#[derive(Debug, Clone, Copy)]
enum Level {
    Build(bool),
    Exactly(Check),
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

/// The committed state a failed rollback is repaired from: the state at `base` (read from
/// `history` when `base` is not 0) followed by `log`.
#[derive(Clone, Copy)]
struct Committed<'a> {
    log: &'a [Transaction],
    base: u64,
    history: Option<&'a Arc<dyn History>>,
}

impl<'a> Committed<'a> {
    fn root(log: &'a [Transaction]) -> Self {
        Self { log, base: 0, history: None }
    }
}

/// The document after replaying `history` (already committed transactions), without checks.
fn restore(history: Committed<'_>) -> Option<Document> {
    let mut doc = match (history.base, history.history) {
        (0, _) => Document::default(),
        (base, Some(h)) => h.state_at(base).ok()?,
        (_, None) => return None,
    };
    for tx in history.log {
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
    /// Borrowed, unless a numbering pass extended the transaction (brief 022).
    ops: std::borrow::Cow<'a, [Op]>,
    applied: usize,
    /// The committed transactions whose replay gives the state before this one; `None` when the
    /// caller discards the document on error.
    history: Option<Committed<'a>>,
    health: &'a mut Health,
    open: bool,
}

impl<'a> Journal<'a> {
    fn new(doc: &'a mut Document, ops: &'a [Op], history: Option<Committed<'a>>, health: &'a mut Health) -> Self {
        Self { doc, ops: std::borrow::Cow::Borrowed(ops), applied: 0, history, health, open: true }
    }

    /// Apply every op in order; what they touched.
    fn apply(&mut self) -> Result<Touched> {
        let mut touched = Touched::default();
        self.apply_rest(&mut touched)?;
        Ok(touched)
    }

    /// Append `more` ops to the transaction (the numbering pass, brief 022) and apply them.
    fn extend(&mut self, more: Vec<Op>, touched: &mut Touched) -> Result<()> {
        if more.is_empty() {
            return Ok(());
        }
        self.ops.to_mut().extend(more);
        self.apply_rest(touched)
    }

    /// Apply the ops not applied yet, in order, adding what they touch to `touched`; the nets are
    /// settled once at the end (brief 022).
    fn apply_rest(&mut self, touched: &mut Touched) -> Result<()> {
        // Disjoint borrows: the ops stay in place, so a panic part-way still rolls them back.
        let Journal { doc, ops, applied, .. } = self;
        for op in ops.get(*applied..).unwrap_or(&[]) {
            #[cfg(test)]
            hooks::maybe_panic(*applied);
            if let Err(e) = op.apply_unsettled(doc, true) {
                crate::op::settle(doc);
                return Err(e);
            }
            touched.add(op);
            *applied += 1;
        }
        crate::op::settle(doc);
        #[cfg(test)]
        hooks::maybe_panic(*applied);
        Ok(())
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
        let ops = std::mem::take(&mut self.ops);
        let applied = ops.get(..self.applied).unwrap_or(&[]);
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
        crate::op::settle(self.doc);
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
    validate_at(doc, touched, Level::Build(verify))
}

/// [`validate`] at an explicit [`Level`].
fn validate_at(doc: &Document, touched: &Touched, level: Level) -> (Result<()>, Option<String>) {
    let scoped = doc.check_touched(touched);
    let full = match level {
        Level::Build(verify) => crate::dev::verifying(verify),
        Level::Exactly(Check::Full) => true,
        Level::Exactly(Check::Commit) => false,
    };
    if full {
        let full = doc.check().and_then(|()| doc.check_index());
        let disagreement = (scoped.is_ok() != full.is_ok()).then(|| format!("scoped check {scoped:?} disagrees with the full check {full:?}"));
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
    doc: &mut Document, ops: &[Op], verify: bool, history: Option<Committed<'_>>, health: &mut Health, then: impl FnOnce() -> Result<()>,
) -> Result<()> {
    apply_tx_at(doc, ops, Level::Build(verify), history, health, then)
}

/// [`apply_tx`] at an explicit [`Level`].
fn apply_tx_at(
    doc: &mut Document, ops: &[Op], level: Level, history: Option<Committed<'_>>, health: &mut Health, then: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let mut journal = Journal::new(doc, ops, history, health);
    let (result, disagreement) = match journal.apply() {
        Ok(touched) => validate_at(journal.doc, &touched, level),
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

/// [`apply_tx`] with a numbering pass (brief 022): after `ops`, `extend` sees the document and
/// returns more ops for the same transaction, applied before the end state is checked. Returns
/// those extra ops.
fn apply_tx_extended(
    doc: &mut Document, ops: &[Op], verify: bool, history: Option<Committed<'_>>, health: &mut Health,
    extend: impl FnOnce(&Document, &[Op]) -> Vec<Op>, then: impl FnOnce() -> Result<()>,
) -> Result<Vec<Op>> {
    let mut journal = Journal::new(doc, ops, history, health);
    let mut touched = Touched::default();
    let mut more = Vec::new();
    let applied = journal.apply_rest(&mut touched).and_then(|()| {
        more = extend(journal.doc, ops);
        journal.extend(more.clone(), &mut touched)
    });
    let (result, disagreement) = match applied {
        Ok(()) => validate(journal.doc, &touched, verify),
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
    debug_assert!(rollback_failure.is_none(), "{rollback_failure:?}");
    debug_assert!(disagreement.is_none(), "{disagreement:?}");
    result.map(|()| more)
}

/// Apply `ops` to `doc` in place, check the end state as [`apply_tx`] does, run `read` on it, and
/// always roll back (also when `read` panics: the [`Journal`] reverts on drop). Returns `read`'s
/// result and, apart from the result, any rollback failure (the document has then been restored
/// from `history` when possible).
fn applied_then_reverted<R>(
    doc: &mut Document, ops: &[Op], verify: bool, history: Option<Committed<'_>>, health: &mut Health, read: impl FnOnce(&Document) -> R,
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
    /// let add = vec![Op::AddPage { id: PageId(1), page: Page::new("1"), index: 0 }];
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
        if let Op::NameVersion { name, at, .. } = op
            && *at >= seq
        {
            return Err(ModelError::Invariant(format!("version {name:?} names transaction {at}, which does not precede transaction {seq}")));
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

    /// Rebuild from a log by applying every transaction to an empty document (FILE-02).
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
    /// open a damaged history read-only at its last good transaction (proposal §20.6).
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

    /// Start from a snapshot instead of the project root (brief 021, EEP-0005 §5): the document,
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
        let Base { seq, mut doc, undo, redo, undo_hidden, redo_hidden, indexed, stack_cap, history } = base;
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
        if !indexed {
            doc.rebuild_index();
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
    fn extend_prefix(&mut self, log: Vec<Transaction>) -> Option<(usize, ModelError)> {
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
    fn rebuild_stacks(&mut self) -> Result<()> {
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
    fn load_tops(&mut self) -> Result<()> {
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
    fn tx(&self, seq: u64) -> Option<&Transaction> {
        if seq > self.base { self.log.get(usize::try_from(seq - self.base - 1).ok()?) } else { self.older.get(&seq) }
    }

    /// Rebuild from a checkpoint holding the state after the first `at` transactions, applying
    /// the rest of `log`. A checkpoint is only a cache (FILE-04): the full log is still required.
    ///
    /// # Errors
    /// As [`Ledger::replay`].
    pub fn from_checkpoint(doc: Document, at: u64, log: Vec<Transaction>, seed: u64) -> Result<Self> {
        Self::rebuild(doc, at, log, seed, false)
    }

    pub(crate) fn rebuild(doc: Document, at: u64, log: Vec<Transaction>, seed: u64, verify: bool) -> Result<Self> {
        Self::rebuild_at(doc, at, log, seed, Level::Build(verify), |_, _| {})
    }

    fn rebuild_at(
        doc: Document, at: u64, log: Vec<Transaction>, seed: u64, level: Level, mut visit: impl FnMut(&Transaction, &Document),
    ) -> Result<Self> {
        let verify = matches!(level, Level::Build(true));
        let mut doc = doc;
        let mut health = Health::default();
        // The checkpoint may have been built without ops; make its index match its objects.
        doc.rebuild_index();
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

    /// The transactions held in memory, oldest first: the whole log when [`Ledger::base`] is 0,
    /// otherwise only those after the base (`log()[i].seq == base() + i + 1`).
    pub fn log(&self) -> &[Transaction] {
        &self.log
    }

    /// The whole history, oldest first: the transactions before the base read from the history
    /// (reads everything before the base: for checks and tools, never on an ordinary path), then
    /// the in-memory log.
    ///
    /// # Errors
    /// The history could not be read.
    pub fn full_log(&self) -> Result<Vec<Transaction>> {
        let mut all = match (&self.history, self.base) {
            (_, 0) => Vec::new(),
            (Some(h), base) => h.transactions(1, base)?,
            (None, base) => {
                return Err(ModelError::CorruptLog(format!("transactions 1 to {base} cannot be read: no history")));
            }
        };
        all.extend(self.log.iter().cloned());
        Ok(all)
    }

    /// The `seq` the in-memory log starts after: 0, or the snapshot this ledger was opened from.
    pub fn base(&self) -> u64 {
        self.base
    }

    /// Where transactions before [`Ledger::base`] are read, when it is not 0.
    pub fn history(&self) -> Option<&Arc<dyn History>> {
        self.history.as_ref()
    }

    /// Read the history before the base from `history` from now on: the same history in another
    /// file (Save As copies it). Transactions already read are kept.
    pub fn set_history(&mut self, history: Arc<dyn History>) {
        if self.base > 0 {
            self.history = Some(history);
        }
    }

    /// The undo and redo stacks as `(seq, id)` pairs, oldest first (recorded with snapshots).
    pub fn stacks(&self) -> (Vec<(u64, TxId)>, Vec<(u64, TxId)>) {
        (self.stacks.undo.clone(), self.stacks.redo.clone())
    }

    /// Transaction `seq` if it is held in memory (after the base, or read from before it).
    pub fn transaction(&self, seq: u64) -> Option<&Transaction> {
        self.tx(seq)
    }

    /// The sequence number the next transaction gets.
    fn next_seq(&self) -> Result<u64> {
        self.head().checked_add(1).ok_or_else(|| ModelError::CorruptLog("the history has no room for another transaction".into()))
    }

    /// Sequence number of the last applied transaction (0 when empty).
    pub fn head(&self) -> u64 {
        self.base.saturating_add(self.log.len() as u64)
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
        if ops.is_empty() {
            return Err(ModelError::Empty);
        }
        self.append(author, label.to_string(), TxKind::Edit, ops, time_ms)
    }

    /// [`Ledger::commit_at`] with a pass that extends the transaction (brief 022's numbering
    /// pass): after `ops` are applied, `extend` sees the document as they left it (no copy) and
    /// returns more ops, recorded in the same transaction after `ops`, so one undo undoes both.
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

    /// Commit a merge (brief 020b, FQ1): `ops` as one transaction of kind [`TxKind::Merge`],
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

    fn append(&mut self, author: Author, label: String, kind: TxKind, ops: Vec<Op>, time_ms: Option<u64>) -> Result<TxId> {
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
    fn after_append(&mut self) {
        let read = if self.stacks.top_hidden() { self.rebuild_stacks() } else { self.load_tops() };
        if let Err(e) = read {
            self.health.diagnostics.push(format!("could not read an older step for undo or redo: {e}"));
        }
    }

    /// The transaction on top of `stack`, if held in memory.
    fn top(&self, stack: &[(u64, TxId)]) -> Option<&Transaction> {
        stack.last().and_then(|(seq, _)| self.tx(*seq))
    }

    /// The step [`Ledger::undo`] would undo, when held in memory (brief 020b: undoing a merge
    /// asks first).
    pub fn undo_top(&self) -> Option<&Transaction> {
        self.top(&self.stacks.undo)
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
    fn ensure_top(&mut self, undo: bool, nothing: &'static str) -> Result<&Transaction> {
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
        // starts from its parent's state, which its history gives (brief 020).
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
                Op::AddPage { id: PageId(1), page: Page::new("1"), index: 0 },
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
            Op::RemoveCableMember { cable: CableId(7), index: 0, member: CableMember { conductor: ConductorId(6), core: None } },
            Op::AddPage { id: PageId(8), page: Page::new("2"), index: 1 },
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
            Op::AddPage { id: PageId(8), page: Page::new("2"), index: 1 },
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
            Op::RemoveTerminal { id: TerminalId(5), terminal: Terminal { component: ComponentId(3), label: None, key: None } },
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
        assert!(crate::dev::switch_on(Some(OsStr::new("1"))));
        for v in ["0", "", "true", "yes", " 1"] {
            assert!(!crate::dev::switch_on(Some(OsStr::new(v))), "{v:?}");
        }
        assert!(!crate::dev::switch_on(None));
    }

    #[test]
    fn the_test_build_verifies() {
        assert!(crate::dev::verifying(false), "every cargo test runs in verify mode");
    }

    #[test]
    fn with_applied_shows_the_changed_document_and_reverts_it() {
        let mut l = ledger();
        let (doc, head) = (json(l.document()), l.head());
        let seen =
            l.with_applied(&three_ops(), |d| (d.ends_at(TerminalId(5)), d.cables_of(ConductorId(6)), d.pages.len(), d.check().is_ok())).unwrap();
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
            Op::AddPage { id: PageId(8), page: Page::new("2"), index: 1 },
            Op::AttachEnd { conductor: ConductorId(6), end: End::B, before: None, after: Some(TerminalId(99)) },
        ];
        let bad_before = vec![Op::AttachEnd { conductor: ConductorId(6), end: End::A, before: Some(TerminalId(5)), after: None }];
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
        let r = catch_unwind(AssertUnwindSafe(|| d.with_applied(&three_ops(), |_| std::panic::resume_unwind(Box::new("boom")))));
        assert!(r.is_err());
        assert_eq!(json(&d), doc);
        d.check_index().unwrap();
        // The ledger carries on.
        l.commit(Author::User, "edit", three_ops()).unwrap();
    }

    #[test]
    fn with_applied_reports_a_rollback_it_could_not_repair_at_once() {
        // A log that no longer replays, so a failed revert cannot be repaired from it.
        let mut l = ledger();
        let mut unreplayable = l.log.first().cloned().unwrap();
        unreplayable.seq = 2;
        unreplayable.ops = vec![Op::RemoveLayer { id: LayerId(99), layer: Layer { name: "gone".into() } }];
        l.log.push(unreplayable);
        hooks::FAIL_REVERT.with(|c| c.set(true));
        let r = catch_unwind(AssertUnwindSafe(|| l.with_applied(&three_ops(), |_| ())));
        hooks::FAIL_REVERT.with(|c| c.set(false));
        match r {
            Ok(r) => assert!(matches!(r, Err(ModelError::CorruptLog(_))), "{r:?}"),
            Err(_) => assert!(std::hint::black_box(cfg!(debug_assertions)), "only the debug assertion may panic"),
        }
        assert!(l.health.broken);
        assert!(matches!(l.with_applied(&three_ops(), |_| ()), Err(ModelError::CorruptLog(_))));
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
