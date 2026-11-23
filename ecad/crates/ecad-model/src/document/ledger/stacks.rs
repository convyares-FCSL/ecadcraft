//! The undo and redo stacks derived from the log.

use crate::error::{ModelError, Result};
use crate::naming::id::TxId;

use super::*;

/// The derived undo/redo stacks: the `seq` and ID of each entry, so that admitting a step never
/// needs the transaction itself (which may lie before the ledger's base).
#[derive(Debug, Clone, Default)]
pub(super) struct Stacks {
    pub(super) undo: Vec<(u64, TxId)>,
    pub(super) redo: Vec<(u64, TxId)>,
    /// Entries below the visible ones that a snapshot did not record (a snapshot
    /// keeps the top of each stack and its depth). They are read once, from the history, when
    /// undo or redo first reaches them ([`Ledger::rebuild_stacks`]).
    pub(super) undo_hidden: u64,
    pub(super) redo_hidden: u64,
    /// How many entries of the base's stacks (from their tops) have been read so far.
    pub(super) undo_known: u64,
    pub(super) redo_known: u64,
}

/// How many stack entries are read from the history at a time, when undo or redo reaches below
/// what is held (the cap a snapshot records).
pub(super) const STACK_READ: u64 = 1_024;

impl Stacks {
    /// Whether `kind` would pop a stack whose visible part is empty but which has hidden entries.
    pub(super) fn needs_full(&self, kind: TxKind) -> bool {
        match kind {
            TxKind::Edit | TxKind::Merge { .. } => false,
            TxKind::Undo { .. } => self.undo.is_empty() && self.undo_hidden > 0,
            TxKind::Redo { .. } => self.redo.is_empty() && self.redo_hidden > 0,
        }
    }

    /// Whether either stack's visible part is empty while it has hidden entries.
    pub(super) fn top_hidden(&self) -> bool {
        (self.undo.is_empty() && self.undo_hidden > 0) || (self.redo.is_empty() && self.redo_hidden > 0)
    }
    /// Whether transaction `id`, of `kind`, may follow the entries already pushed.
    pub(super) fn admit_kind(&self, kind: TxKind, id: impl std::fmt::Display) -> Result<()> {
        let top = |stack: &Vec<(u64, TxId)>| stack.last().map(|(_, t)| *t);
        match kind {
            TxKind::Edit | TxKind::Merge { .. } => Ok(()),
            TxKind::Undo { of } if top(&self.undo) != Some(of) => Err(ModelError::CorruptLog(format!("undo {id} does not undo the latest step"))),
            TxKind::Redo { of } if top(&self.redo) != Some(of) => Err(ModelError::CorruptLog(format!("redo {id} does not redo the latest undo"))),
            TxKind::Undo { .. } | TxKind::Redo { .. } => Ok(()),
        }
    }

    /// Whether `tx` may follow the entries already pushed.
    pub(super) fn admit(&self, tx: &Transaction) -> Result<()> {
        self.admit_kind(tx.kind, tx.id)
    }

    /// Account for `tx`, after [`Stacks::admit`] accepted it.
    pub(super) fn push(&mut self, tx: &Transaction) {
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

pub(super) fn stacks(log: &[Transaction]) -> Result<Stacks> {
    let mut s = Stacks::default();
    for tx in log {
        s.admit(tx)?;
        s.push(tx);
    }
    Ok(s)
}

/// The whole undo and redo stacks after transaction `seq`, from the kinds of every transaction
/// up to it.
pub(super) fn stacks_from<H: History + ?Sized>(h: &H, seq: u64) -> Result<(Vec<(u64, TxId)>, Vec<(u64, TxId)>)> {
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
