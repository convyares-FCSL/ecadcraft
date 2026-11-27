//! A project in a version 2 file: opening from the latest snapshot and its tail, and saving.
//!
//! - **Open** reads `meta`, the branch row and its head transaction, the latest snapshot's chunks
//!   (the whole active snapshot) and the transactions after it, each checked against its chain value and
//!   replayed with every check. It never reads other
//!   branches, older versions, transactions before the snapshot, or assets. Older transactions are
//!   read on demand through [`StoreHistory`] (undo past the snapshot, `state_at`).
//! - **Save** writes the pending transactions, the snapshots of any versions they name and of an
//!   automatic snapshot every `cache.every` transactions, the branch head and `meta` in one redb
//!   write transaction. Chunks are built incrementally from the last snapshot ([`crate::chunks::Dirty`]).
//!   Nothing stored is read back to check it; its cost follows what is written.

mod branches;
mod lineage;
mod open;
mod report;
mod save;

use std::collections::BTreeMap;
use std::sync::Arc;

use ecad_model::{History, Ledger, ProjectId, TxId};

use crate::FileError;
use crate::chain::{self, ChainKind, Hash};
use crate::chunks::ChunkSet;
use crate::schema::{BranchId, BranchRecord, Cache, MergeRow, SNAPSHOT_EVERY_RANGE, SnapshotRef};
use crate::store::Store;
use ecad_model::naming::tag::HighWater;
use lineage::StackState;

pub use branches::check_branch_name;
pub use lineage::{Line, StoreHistory, line_state, lineage};
pub(crate) use lineage::{Lineage, decode_row};
pub use open::project_id;
pub(crate) use open::{format_note, main_branch_pub, read_facts_pub};
pub use report::{BranchError, Damage, OpenError, Report, SaveError, Saved};

/// The in-memory snapshot the next one is built from. `set` is `None` for a branch opened from
/// its parent's state (it has no snapshot of its own yet): its first snapshot is built whole.
#[derive(Debug, Clone)]
struct Snap {
    seq: u64,
    set: Option<ChunkSet>,
}

/// A project bound to a version 2 database.
#[derive(Debug)]
pub struct Project {
    store: Arc<Store>,
    project: ProjectId,
    branch: BranchId,
    /// The branch's row as last read or written (its head is written by each save).
    rec: BranchRecord,
    /// The branch and its ancestry.
    lineage: Lineage,
    /// The branch head in the file: `seq` and chain value.
    head: (u64, Hash),
    cache: Cache,
    snap: Snap,
    /// The tag high-water mark at `base_seq`.
    base_tag_high: HighWater,
    /// The `versions` row of the snapshot opened from or last written (`None`: the root).
    snap_ref: Option<SnapshotRef>,
    /// The undo and redo stacks at the ledger's base (the snapshot opened from).
    base_stacks: StackState,
    base_seq: u64,
    writer: String,
    read_only: Option<String>,
    /// Why the file is read-only, apart from damage on the branch shown (kept across a switch).
    file_read_only: Option<String>,
    created_ms: Option<u64>,
    last_save: Option<Saved>,
    /// `meta.writer` names this build already (written by the first save of a session).
    wrote_writer: bool,
    /// Merges committed this session that close their source branch when saved: the merge's ID and
    /// the source.
    closing: BTreeMap<TxId, BranchId>,
    /// Sources reopened by an undo of a merge, by the branch and the undo's `seq`: a redo of
    /// that undo closes them again. Kept for the session.
    reopened: BTreeMap<(BranchId, u64), BranchId>,
}

/// A live merge into a branch (the merge index): its step and what it recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeIndex {
    /// The step that made it live: the merge, or the redo of its undo.
    pub seq: u64,
    pub source: BranchId,
    pub source_head: (u64, Hash),
    pub ancestor: (BranchId, u64, Hash),
}

impl MergeIndex {
    fn of(seq: u64, row: &MergeRow) -> Result<Self, FileError> {
        Ok(Self {
            seq,
            source: row.source_id().ok_or(FileError::MergeNoSource)?,
            source_head: (row.source_head.seq, row.source_head.hash()?),
            ancestor: (
                row.ancestor_id().ok_or(FileError::MergeNoAncestor)?,
                row.ancestor.seq,
                chain::parse_hex(&row.ancestor.chain).ok_or(FileError::MergeAncestorChain)?,
            ),
        })
    }
}

/// What following merges, their undos and redos over some steps gives (shared by Save, which
/// writes it, and [`Project::merge_index`], which only reads).
#[derive(Debug, Default)]
struct MergeFollow {
    /// Rows to write (`Some`) or remove (`None`), by `seq`.
    rows: BTreeMap<u64, Option<MergeRow>>,
    /// Source branches to close (merged into this branch at `seq`) or reopen (`None`).
    status: Vec<(BranchId, Option<u64>)>,
    /// Sources reopened by an undo, by the undo's `seq`.
    reopened: BTreeMap<u64, BranchId>,
}

/// A project opened from a version 2 database.
#[derive(Debug)]
pub struct Opened {
    pub project: Project,
    pub ledger: Ledger,
    pub report: Report,
    /// Words this build does not read in the file's container facts: each `meta` name not in
    /// [`META_NAMES`](crate::schema::META_NAMES). When there are any the project opens read-only
    /// (pre-v1 strict reading) and the file is kept as it is.
    pub unknown_words: Vec<ecad_model::document::strict::UnknownWord>,
}

impl Project {
    /// This branch's chain value at `seq` 0: the project seed on `main`, the branch seed
    /// elsewhere.
    fn seed(&self) -> Hash {
        self.lineage.first().map_or_else(|| chain::seed(ChainKind::Project, self.project.0), |l| l.seed)
    }

    /// The tag high-water mark at the snapshot this project was opened from (the next
    /// automatic number is the highest + 1); the transactions after it add to it.
    pub fn base_tag_high(&self) -> &HighWater {
        &self.base_tag_high
    }

    /// A reader of this file's history, for a ledger based on one of its snapshots.
    pub fn history(&self) -> Arc<dyn History> {
        Arc::new(StoreHistory { store: Arc::downgrade(&self.store), branch: self.branch, seed: self.seed(), lineage: self.lineage.clone() })
    }

    /// The database.
    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    /// The project ID.
    pub fn project_id(&self) -> ProjectId {
        self.project
    }

    /// The branch this project edits (`main`).
    pub fn branch(&self) -> BranchId {
        self.branch
    }

    /// The head in the file: `seq` and chain value.
    pub fn head(&self) -> (u64, Hash) {
        self.head
    }

    /// The `seq` of the latest snapshot.
    pub fn snapshot_seq(&self) -> u64 {
        self.snap.seq
    }

    /// Transactions between automatic snapshots.
    pub fn snapshot_every(&self) -> u64 {
        self.cache.every
    }

    /// Set the automatic snapshot interval (written with the next save).
    ///
    /// # Errors
    /// Out of range.
    pub fn set_snapshot_every(&mut self, every: u64) -> Result<(), FileError> {
        if !(SNAPSHOT_EVERY_RANGE.0..=SNAPSHOT_EVERY_RANGE.1).contains(&every) {
            return Err(FileError::SnapshotEvery);
        }
        self.cache.every = every;
        Ok(())
    }

    /// Why the project cannot be saved to this file, if it cannot.
    pub fn read_only(&self) -> Option<&str> {
        self.read_only.as_deref()
    }

    /// Make the project read-only (another session holds the file's lock).
    pub(crate) fn set_read_only(&mut self, why: String) {
        self.read_only = Some(why);
    }

    /// When the file was created (informational).
    pub fn created_ms(&self) -> Option<u64> {
        self.created_ms
    }

    /// What the last save wrote.
    pub fn last_save(&self) -> Option<&Saved> {
        self.last_save.as_ref()
    }
}
