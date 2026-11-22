//! Applying a transaction in place with a rollback journal, and the check after it.

use std::sync::Arc;

use crate::Document;
use crate::document::invariant::Touched;
use crate::document::op::Op;
use crate::error::{ModelError, Result};

use super::*;

/// What [`validate`] checks: the build's choice (the scoped check, plus the full one in verify
/// mode or when `true`), or exactly one [`Check`].
#[derive(Debug, Clone, Copy)]
pub(super) enum Level {
    Build(bool),
    Exactly(Check),
}

/// What went wrong with rollbacks. Empty in any correct build.
#[derive(Debug, Clone, Default)]
pub(super) struct Health {
    /// One line per rollback that could not revert an op.
    pub(super) diagnostics: Vec<String>,
    /// A failed rollback could not be repaired by replaying the log, so the document may be
    /// corrupt: the ledger refuses further changes.
    pub(super) broken: bool,
}

/// The committed state a failed rollback is repaired from: the state at `base` (read from
/// `history` when `base` is not 0) followed by `log`.
#[derive(Clone, Copy)]
pub(super) struct Committed<'a> {
    pub(super) log: &'a [Transaction],
    pub(super) base: u64,
    pub(super) history: Option<&'a Arc<dyn History>>,
}

impl<'a> Committed<'a> {
    pub(super) fn root(log: &'a [Transaction]) -> Self {
        Self { log, base: 0, history: None }
    }
}

/// The document after replaying `history` (already committed transactions), without checks.
pub(super) fn restore(history: Committed<'_>) -> Option<Document> {
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
pub(super) mod hooks {
    use std::cell::Cell;

    thread_local! {
        /// Panic when this many ops of a transaction have been applied.
        pub(crate) static PANIC_AT: Cell<Option<usize>> = const { Cell::new(None) };
        /// Treat every revert as failed.
        pub(crate) static FAIL_REVERT: Cell<bool> = const { Cell::new(false) };
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
pub(super) struct Journal<'a> {
    pub(super) doc: &'a mut Document,
    /// Borrowed, unless a numbering pass extended the transaction (brief 022).
    pub(super) ops: std::borrow::Cow<'a, [Op]>,
    pub(super) applied: usize,
    /// The committed transactions whose replay gives the state before this one; `None` when the
    /// caller discards the document on error.
    pub(super) history: Option<Committed<'a>>,
    pub(super) health: &'a mut Health,
    pub(super) open: bool,
}

impl<'a> Journal<'a> {
    pub(super) fn new(doc: &'a mut Document, ops: &'a [Op], history: Option<Committed<'a>>, health: &'a mut Health) -> Self {
        Self { doc, ops: std::borrow::Cow::Borrowed(ops), applied: 0, history, health, open: true }
    }

    /// Apply every op in order; what they touched.
    pub(super) fn apply(&mut self) -> Result<Touched> {
        let mut touched = Touched::default();
        self.apply_rest(&mut touched)?;
        Ok(touched)
    }

    /// Append `more` ops to the transaction (the numbering pass, brief 022) and apply them.
    pub(super) fn extend(&mut self, more: Vec<Op>, touched: &mut Touched) -> Result<()> {
        if more.is_empty() {
            return Ok(());
        }
        self.ops.to_mut().extend(more);
        self.apply_rest(touched)
    }

    /// Apply the ops not applied yet, in order, adding what they touch to `touched`; the nets are
    /// settled once at the end (brief 022).
    pub(super) fn apply_rest(&mut self, touched: &mut Touched) -> Result<()> {
        // Disjoint borrows: the ops stay in place, so a panic part-way still rolls them back.
        let Journal { doc, ops, applied, .. } = self;
        for op in ops.get(*applied..).unwrap_or(&[]) {
            #[cfg(test)]
            hooks::maybe_panic(*applied);
            if let Err(e) = op.apply_unsettled(doc, true) {
                crate::document::op::settle(doc);
                return Err(e);
            }
            touched.add(op);
            *applied += 1;
        }
        crate::document::op::settle(doc);
        #[cfg(test)]
        hooks::maybe_panic(*applied);
        Ok(())
    }

    /// Keep the applied ops.
    pub(super) fn commit(mut self) {
        self.open = false;
    }

    /// Revert the applied ops. Returns the failure, if a revert failed (the document has then
    /// been restored from the history when possible).
    pub(super) fn roll_back(&mut self) -> Option<String> {
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
        crate::document::op::settle(self.doc);
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
pub(super) fn validate(doc: &Document, touched: &Touched, verify: bool) -> (Result<()>, Option<String>) {
    validate_at(doc, touched, Level::Build(verify))
}

/// [`validate`] at an explicit [`Level`].
pub(super) fn validate_at(doc: &Document, touched: &Touched, level: Level) -> (Result<()>, Option<String>) {
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
pub(super) fn apply_tx(
    doc: &mut Document, ops: &[Op], verify: bool, history: Option<Committed<'_>>, health: &mut Health, then: impl FnOnce() -> Result<()>,
) -> Result<()> {
    apply_tx_at(doc, ops, Level::Build(verify), history, health, then)
}

/// [`apply_tx`] at an explicit [`Level`].
pub(super) fn apply_tx_at(
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
pub(super) fn apply_tx_extended(
    doc: &mut Document, ops: &[Op], verify: bool, history: Option<Committed<'_>>, health: &mut Health,
    extend: impl FnOnce(&Document, &[Op]) -> Vec<Op>, then: impl FnOnce() -> Result<()>,
) -> Result<Vec<Op>> {
    let mut journal = Journal::new(doc, ops, history, health);
    let mut touched = Touched::default();
    let mut more = Vec::new();
    let applied = journal.apply_rest(&mut touched).and_then(|()| {
        // Design 072 §4.4: the wires of every net whose points changed, in the same transaction.
        let mut wires = crate::connectivity::wire_order::rewrite(journal.doc, ops);
        if !wires.is_empty() {
            journal.extend(wires.clone(), &mut touched)?;
        }
        // The cores on the routes changed move with them (owner ruling, 2026-11-18).
        let cores = crate::connectivity::wire_order::cores(journal.doc, ops);
        if !cores.is_empty() {
            journal.extend(cores.clone(), &mut touched)?;
            wires.extend(cores);
        }
        // A deleted device's `protected_by`s are cleared (overseer ruling, 2026-11-18).
        let unprotected = crate::connectivity::wire_order::unprotect(journal.doc, ops);
        if !unprotected.is_empty() {
            journal.extend(unprotected.clone(), &mut touched)?;
            wires.extend(unprotected);
        }
        let extra = if wires.is_empty() {
            extend(journal.doc, ops)
        } else {
            let all: Vec<Op> = ops.iter().cloned().chain(wires.iter().cloned()).collect();
            extend(journal.doc, &all)
        };
        more = wires;
        more.extend(extra.iter().cloned());
        journal.extend(extra, &mut touched)
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
pub(super) fn applied_then_reverted<R>(
    doc: &mut Document, ops: &[Op], verify: bool, history: Option<Committed<'_>>, health: &mut Health, read: impl FnOnce(&Document) -> R,
) -> (Result<R>, Option<String>) {
    let mut journal = Journal::new(doc, ops, history, health);
    let mut touched = Touched::default();
    // As a commit does (design 072 §4.4): the wires of the nets whose points changed follow.
    let applied = journal.apply_rest(&mut touched).and_then(|()| {
        let wires = crate::connectivity::wire_order::rewrite(journal.doc, ops);
        journal.extend(wires, &mut touched)?;
        let cores = crate::connectivity::wire_order::cores(journal.doc, ops);
        journal.extend(cores, &mut touched)
    });
    let (checked, disagreement) = match applied {
        Ok(()) => validate(journal.doc, &touched, verify),
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
    /// let add = vec![Op::AddPage { id: PageId(1), page: Page::titled("1").unwrap(), index: 0 }];
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
pub(super) fn apply_scratch(doc: &mut Document, ops: &[Op], verify: bool) -> Result<()> {
    apply_tx(doc, ops, verify, None, &mut Health::default(), || Ok(()))
}

/// A named version must name a transaction before the one naming it (0 is the empty project), so
/// [`Ledger::state_at`] can always resolve it.
pub(super) fn check_bookmarks(ops: &[Op], seq: u64) -> Result<()> {
    for op in ops {
        if let Op::NameVersion { name, at, .. } = op
            && *at >= seq
        {
            return Err(ModelError::Invariant(format!("version {name:?} names transaction {at}, which does not precede transaction {seq}")));
        }
    }
    Ok(())
}

pub(super) fn inverse_ops(ops: &[Op]) -> Vec<Op> {
    ops.iter().rev().map(Op::inverse).collect()
}
