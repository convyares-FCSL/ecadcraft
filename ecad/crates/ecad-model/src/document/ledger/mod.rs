//! Transactions and the append-only history.
//!
//! A transaction applies in place with a rollback journal and is checked only where it touched
//! the document ([`crate::document::invariant`]), so its cost follows the size of the change: on every
//! commit, undo and redo, and per transaction when a file is replayed on open.

mod edit;
mod journal;
mod open;
mod read;
mod stacks;
#[cfg(test)]
mod tests;
pub mod types;

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::Document;
use crate::naming::id::IdAllocator;

use journal::*;
use stacks::*;
use types::*;

/// The authoritative document and its history.
///
/// # Semantics
/// - [`Ledger::commit`] applies a transaction's ops in place, in order, then checks the
///   invariants the transaction could have affected ([`Document::check_touched`]) and only then
///   appends it. If an op or the check fails, the applied ops are undone with their
///   inverses in reverse order (the rollback journal), leaving the document exactly as it was.
/// - States between the ops of one transaction may break invariants; only the end state is
///   checked. A later op sees what the earlier ops created, changed or removed.
/// - [`Ledger::undo`] and [`Ledger::redo`] append transactions holding concrete inverse ops.
///   The undo/redo position is derived from the log: the ledger keeps it in memory, rebuilt on load and extended on every append, and never stores it in the file.
/// - The project is unsaved exactly when [`Ledger::head`] differs from
///   [`Ledger::persisted`].
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
/// ledger.commit(Author::User, "New page", vec![Op::AddPage { id: page, page: Page::titled("1").unwrap(), index: 0 }]).unwrap();
/// assert!(ledger.is_unsaved());
/// ledger.undo(Author::User).unwrap();
/// assert!(ledger.document().pages().is_empty());
/// assert_eq!(ledger.head(), 2); // the undo was appended
/// ```
#[derive(Debug, Clone)]
pub struct Ledger {
    doc: Document,
    /// The transactions after [`Ledger::base`], oldest first: `log[i].seq == base + i + 1`.
    log: Vec<Transaction>,
    /// The sequence number the in-memory log starts after: 0 for a ledger replayed from the
    /// project root, a snapshot's `seq` for one opened from a snapshot.
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

pub use crate::dev::{set_verify_invariants, switch_on as verify_switch_on, verify_mode};
