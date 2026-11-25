//! Reading a ledger: the document, the log, the stacks, what is saved.

use std::sync::Arc;

use crate::Document;
use crate::error::{ModelError, Result};
use crate::naming::id::TxId;

use super::*;

impl Ledger {
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
    pub(super) fn next_seq(&self) -> Result<u64> {
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

    /// Whether the project has transactions that are not yet saved.
    pub fn is_unsaved(&self) -> bool {
        self.head() != self.persisted
    }

    /// Record that transactions up to `seq` are durably saved. Called by a store only after its
    /// flush succeeded.
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
}
