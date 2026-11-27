//! What opening and saving report: their errors, a save's result and the damage found.

use super::open::format_note;
use crate::FileError;
use crate::store::StoreError;

/// Why a version 2 file could not be opened at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpenError {
    #[error("{0}")]
    Store(#[from] StoreError),
    #[error("{0}")]
    Unsupported(String),
    /// The container facts or the branch are unusable.
    #[error("the file is damaged: {0}")]
    Damaged(FileError),
}

/// Why a save did not complete. The file then holds exactly what it held before, and the
/// project is still unsaved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SaveError {
    #[error("the file was opened read-only: {0}")]
    ReadOnly(String),
    #[error("the file was changed by something else ({0}); it was not modified; use Save As")]
    ExternalChange(String),
    #[error("{0}")]
    Store(#[from] StoreError),
    #[error("could not encode: {0}")]
    Encode(FileError),
}

/// Why a branch action (create, rename, archive, reopen, switch) did not happen. The file is then
/// unchanged.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BranchError {
    #[error("the file was opened read-only: {0}")]
    ReadOnly(String),
    /// Refused, in words for the user.
    #[error("{0}")]
    Refused(String),
    /// Reopening a branch whose name another open branch now has: rename it first.
    #[error("Another open branch is called \"{0}\"; give this one a new name.")]
    NameTaken(String),
    #[error("{0}")]
    Store(#[from] StoreError),
}

/// What a successful save wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Saved {
    pub first_seq: u64,
    pub last_seq: u64,
    /// Bytes of new rows and chunks (before redb's own overhead).
    pub bytes: u64,
    pub chunks_written: usize,
    /// Snapshots written (versions and automatic).
    pub snapshots: usize,
}

/// What a check found wrong while opening: the project opens read-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Damage {
    /// The transaction (or snapshot point) that failed, when known.
    pub seq: Option<u64>,
    pub detail: String,
}

/// What opening found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The branch's recorded head.
    pub head: u64,
    /// The last transaction that verified (equal to `head` unless damaged).
    pub verified_seq: u64,
    /// The snapshot the project was opened from (0: the project root).
    pub snapshot_seq: u64,
    /// Transactions replayed after the snapshot.
    pub tail: u64,
    /// redb repaired the file while opening it (it was not closed cleanly).
    pub repaired: bool,
    pub damage: Option<Damage>,
    /// The file's `schema_version` (see [`format_note`]).
    pub schema_version: u64,
}

impl Report {
    /// One paragraph for the user.
    pub fn describe(&self) -> String {
        let mut s = format!("Opened: {} transactions (from the snapshot at {}, then {} replayed).", self.verified_seq, self.snapshot_seq, self.tail);
        if self.repaired {
            s.push_str(" The file was not closed cleanly last time and was recovered to its last save.");
        }
        // Both facts are said: the format note never hides damage. A file of another format gets no Save As advice
        // once it is damaged too.
        let note = format_note(self.schema_version);
        if let Some(n) = &note {
            s.push_str(&format!(" Opened read-only: {n}; it is kept exactly as it is."));
            if self.damage.is_none() {
                s.push_str(" Use Save As to keep a copy.");
            }
        }
        if let Some(d) = &self.damage {
            s.push_str(&format!(
                " {}: {}{}. {} of {} transactions verified",
                if note.is_some() { "It is also damaged" } else { "Opened read-only" },
                d.seq.map_or(String::new(), |q| format!("at transaction {q}, ")),
                d.detail,
                self.verified_seq,
                self.head
            ));
            s.push_str(if note.is_some() { "." } else { "; use Save As to keep them." });
        }
        s
    }
}
