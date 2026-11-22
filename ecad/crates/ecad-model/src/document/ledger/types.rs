//! The records of the history: authors, kinds, points, transactions, the `History` trait, `Base`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::Document;
use crate::document::op::Op;
use crate::error::Result;
use crate::naming::id::TxId;

use super::*;

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
