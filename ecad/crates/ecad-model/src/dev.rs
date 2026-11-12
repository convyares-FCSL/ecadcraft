//! Developer and test hooks of `ecad-model` (design 029 §12): **verify mode** (brief 013).
//!
//! Verify mode runs the full invariant and index check after every transaction, beside the scoped
//! check, and treats a disagreement as a bug. It is on in every `cargo test` (the
//! `verify-invariants` feature, which dependent crates enable through their dev-dependencies),
//! with `ECAD_VERIFY_INVARIANTS=1` at run time, and in [`Ledger::replay_verified`] (the
//! debugger's **Verify replay**). App builds, debug or release, take the fast path. The file's
//! Verify never uses it: it replays with [`crate::ledger::Check::Commit`].
//!
//! This module is the only place in the crate where the `verify-invariants` feature is read
//! (`cargo xtask layers` enforces it).

use std::sync::OnceLock;

use crate::doc::Document;
use crate::error::Result;
use crate::ledger::{Ledger, Transaction};

/// Whether `ECAD_VERIFY_INVARIANTS=1` is set (read once per process).
fn verify_env() -> bool {
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| switch_on(std::env::var_os("ECAD_VERIFY_INVARIANTS").as_deref()))
}

/// Only the value `1` turns verify mode on.
pub(crate) fn switch_on(value: Option<&std::ffi::OsStr>) -> bool {
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
    local || cfg!(feature = "verify-invariants") || verify_env()
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
