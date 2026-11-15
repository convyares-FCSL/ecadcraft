//! Developer and test hooks of `ecad-model` (design 029 §12): **verify mode** (brief 013).
//!
//! Verify mode runs the full invariant and index check after every transaction, beside the scoped
//! check, and treats a disagreement as a bug. It is on in every `cargo test` (the
//! `verify-invariants` feature, which dependent crates enable through their dev-dependencies),
//! with `ECAD_VERIFY_INVARIANTS=1` at run time (read by each program at start-up, which passes it
//! in through [`set_verify_invariants`]: Core reads no environment, design 065 §3.1), and in
//! [`Ledger::replay_verified`] (the
//! debugger's **Verify replay**). App builds, debug or release, take the fast path. The file's
//! Verify never uses it: it replays with [`crate::ledger::Check::Commit`].
//!
//! This module is the only place in the crate where the `verify-invariants` feature is read
//! (`cargo xtask layers` enforces it).

use std::sync::atomic::{AtomicBool, Ordering};

use crate::doc::Document;
use crate::error::Result;
use crate::ledger::{Ledger, Transaction};

/// The run-time switch, set by the program ([`set_verify_invariants`]).
static SWITCH: AtomicBool = AtomicBool::new(false);

/// Turn verify mode on or off for this process. A program calls it once at start-up with
/// [`switch_on`] of its `ECAD_VERIFY_INVARIANTS` environment variable (design 065 §3.1: the model
/// reads no environment).
pub fn set_verify_invariants(on: bool) {
    SWITCH.store(on, Ordering::Relaxed);
}

/// Whether the value of `ECAD_VERIFY_INVARIANTS` turns verify mode on: only the value `1` does.
///
/// # Example
/// ```
/// use std::ffi::OsStr;
/// assert!(ecad_model::ledger::verify_switch_on(Some(OsStr::new("1"))));
/// assert!(!ecad_model::ledger::verify_switch_on(Some(OsStr::new("0"))));
/// ```
pub fn switch_on(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|v| v == "1")
}

/// Whether verify mode is on for this build or process (the `verify-invariants` feature, which
/// every `cargo test` turns on, or `ECAD_VERIFY_INVARIANTS=1`): derived indexes kept outside the
/// model (brief 025's reference index) compare themselves with a rebuild after every change.
pub fn verify_mode() -> bool {
    verifying(false)
}

/// Whether to run the full check alongside the scoped one (`local`: this ledger is in verify
/// mode).
pub(crate) fn verifying(local: bool) -> bool {
    local || cfg!(feature = "verify-invariants") || SWITCH.load(Ordering::Relaxed)
}

impl Ledger {
    /// [`Ledger::replay`] in verify mode: every transaction is also checked in full, and the
    /// resulting ledger stays in verify mode. Used by the debugger's **Verify replay** and by
    /// tests, never by the file's Verify.
    ///
    /// # Errors
    /// As [`Ledger::replay`].
    pub fn replay_verified(log: Vec<Transaction>, seed: u64) -> Result<Self> {
        Self::rebuild(Document::default(), 0, log, seed, true)
    }

    /// [`Ledger::replay_verified`] from `doc` instead of the empty project: a branch's history
    /// replayed on its parent's state at the fork point (brief 020).
    ///
    /// # Errors
    /// As [`Ledger::replay`].
    pub fn replay_verified_from(doc: Document, log: Vec<Transaction>, seed: u64) -> Result<Self> {
        Self::rebuild(doc, 0, log, seed, true)
    }
}
