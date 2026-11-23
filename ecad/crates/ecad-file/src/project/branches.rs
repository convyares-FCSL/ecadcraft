//! Branches: their names, creating, renaming, closing and reopening one, switching to one, and
//! reading another branch's transactions and states.

use ecad_model::{Document, Ledger, Transaction};
use serde_json::{Value, json};

use super::Project;
use super::lineage::{Lineage, MAX_DEPTH, line_state, lineage, nearest_snapshot};
use super::open::load_branch;
use super::report::{BranchError, Report};
use crate::FileError;
use crate::chain::{self, ChainKind};
use crate::pack;
use crate::record;
use crate::schema::{BranchId, BranchRecord, BranchStatus, Point};
use crate::store::StoreError;

/// Longest branch name, in characters.
pub(crate) const MAX_BRANCH_NAME: usize = 60;

/// A branch name checked: trimmed, 1–60 characters, no control characters,
/// not `main`.
///
/// # Errors
/// [`BranchError::Refused`] in words.
pub fn check_branch_name(name: &str) -> Result<String, BranchError> {
    let n = name.trim();
    let len = n.chars().count();
    if len == 0 || len > MAX_BRANCH_NAME {
        return Err(BranchError::Refused(format!("A branch name is 1 to {MAX_BRANCH_NAME} characters.")));
    }
    if n.chars().any(char::is_control) {
        return Err(BranchError::Refused("A branch name cannot hold control characters.".into()));
    }
    if n.eq_ignore_ascii_case("main") {
        return Err(BranchError::Refused("\"main\" is the release line's name; choose another.".into()));
    }
    Ok(n.to_string())
}

impl Project {
    /// The row of the branch shown, as last read or written.
    pub fn branch_record(&self) -> &BranchRecord {
        &self.rec
    }

    /// The branch shown and its ancestry up to `main`.
    pub fn lineage(&self) -> &Lineage {
        &self.lineage
    }

    /// Every branch row of the file (`main` first, then by creation time and name).
    ///
    /// # Errors
    /// The engine failed.
    pub fn branches(&self) -> Result<Vec<(BranchId, BranchRecord)>, StoreError> {
        let main = BranchId::main_of(self.project.0);
        let mut all = self.store.reader()?.branches()?;
        all.sort_by(|(a, x), (b, y)| (*a != main, x.created_ms, &x.name).cmp(&(*b != main, y.created_ms, &y.name)));
        Ok(all)
    }

    /// Why branch actions cannot write to the file, if they cannot.
    pub(super) fn check_writable(&self) -> Result<(), BranchError> {
        match &self.read_only {
            Some(r) => Err(BranchError::ReadOnly(r.clone())),
            None => Ok(()),
        }
    }

    /// Create a branch named `name` forking from `from` at its transaction `seq` (a random
    /// ID; written at once, in a write transaction of its own; no snapshot is written).
    /// A fork at a branch's `seq` 0 is its parent's fork point, so the new branch's parent is the
    /// branch that holds the transaction. The caller saves pending transactions first.
    ///
    /// # Errors
    /// [`BranchError`]: the file is read-only, the name is refused, the point does not exist, or
    /// the engine failed.
    pub fn create_branch(
        &mut self, name: &str, from: BranchId, seq: u64, created_ms: Option<u64>, created_by: Option<String>,
    ) -> Result<(BranchId, BranchRecord), BranchError> {
        self.check_writable()?;
        let name = check_branch_name(name)?;
        let r = self.store.reader()?;
        let all = r.branches()?;
        if let Some((_, b)) = all.iter().find(|(_, b)| b.status == BranchStatus::Open && b.name.to_lowercase() == name.to_lowercase()) {
            return Err(BranchError::Refused(format!("There is already a branch \"{}\".", b.name)));
        }
        // Walk a fork at `seq` 0 up to the branch holding the transaction.
        let (mut at, mut seq) = (from, seq);
        for _ in 0..MAX_DEPTH {
            if seq > 0 {
                break;
            }
            match r.branch(at)?.and_then(|b| b.parent.zip(b.fork_point.map(|p| p.seq))) {
                Some((parent, fseq)) => (at, seq) = (parent, fseq),
                None => break,
            }
        }
        let holder = r.branch(at)?.ok_or_else(|| BranchError::Refused("That branch does not exist.".into()))?;
        // An unsaved point of the branch shown counts as its saved head (the caller saved first).
        if seq > holder.head.seq {
            return Err(BranchError::Refused(format!("Branch \"{}\" has no step {seq}.", holder.name)));
        }
        let fork_chain = if seq == 0 {
            chain::seed(ChainKind::Project, self.project.0)
        } else {
            r.tx(at, seq)?.map(|(_, c, _)| c).ok_or_else(|| BranchError::Refused(format!("Step {seq} is not stored.")))?
        };
        drop(r);
        let id = BranchId::from_seed(ecad_store::random_u128());
        let seed = chain::branch_seed(self.project.0, &id.0, &fork_chain);
        let rec = BranchRecord {
            name: name.clone(),
            status: BranchStatus::Open,
            parent: Some(at),
            fork_point: Some(Point::new(seq, &fork_chain)),
            head: Point::new(0, &seed),
            release_line: false,
            merged: None,
            created_ms,
            created_by,
        };
        let mut w = self.store.writer()?;
        if w.branch(id)?.is_some() {
            return Err(BranchError::Refused("A new branch ID was already used; try again.".into()));
        }
        w.put_branch(id, &rec)?;
        w.commit()?;
        Ok((id, rec))
    }

    /// Rename `branch` (not `main`), written at once.
    ///
    /// # Errors
    /// As [`Project::create_branch`].
    pub fn rename_branch(&mut self, branch: BranchId, name: &str) -> Result<BranchRecord, BranchError> {
        self.check_writable()?;
        let name = check_branch_name(name)?;
        if branch == BranchId::main_of(self.project.0) {
            return Err(BranchError::Refused("main is the release line and keeps its name.".into()));
        }
        let all = self.store.reader()?.branches()?;
        if let Some((_, b)) =
            all.iter().find(|(id, b)| *id != branch && b.status == BranchStatus::Open && b.name.to_lowercase() == name.to_lowercase())
        {
            return Err(BranchError::Refused(format!("There is already a branch \"{}\".", b.name)));
        }
        self.update_branch(branch, |b| b.name = name.clone())
    }

    /// Archive `branch` (`archive`) or reopen it, written at once. `main` cannot be archived;
    /// the branch shown cannot be archived (switch away first). Reopening checks the name is
    /// unique among open branches again.
    ///
    /// # Errors
    /// As [`Project::create_branch`].
    pub fn set_branch_open(&mut self, branch: BranchId, open: bool) -> Result<BranchRecord, BranchError> {
        self.check_writable()?;
        if branch == BranchId::main_of(self.project.0) {
            return Err(BranchError::Refused("main is the release line and cannot be archived.".into()));
        }
        if !open && branch == self.branch {
            return Err(BranchError::Refused("Switch to another branch before archiving this one.".into()));
        }
        if open {
            let all = self.store.reader()?.branches()?;
            let name = all.iter().find(|(id, _)| *id == branch).map(|(_, b)| b.name.to_lowercase()).unwrap_or_default();
            if let Some((_, b)) = all.iter().find(|(id, b)| *id != branch && b.status == BranchStatus::Open && b.name.to_lowercase() == name) {
                return Err(BranchError::NameTaken(b.name.clone()));
            }
        }
        self.update_branch(branch, |b| {
            b.status = if open { BranchStatus::Open } else { BranchStatus::Archived };
            if open {
                b.merged = None;
            }
        })
    }

    /// Reopen `branch` under `name` in one write (a reopen whose name another open
    /// branch took is never a dead end).
    ///
    /// # Errors
    /// As [`Project::set_branch_open`]; the new name is checked as a rename's.
    pub fn reopen_as(&mut self, branch: BranchId, name: &str) -> Result<BranchRecord, BranchError> {
        self.check_writable()?;
        let name = check_branch_name(name)?;
        if branch == BranchId::main_of(self.project.0) {
            return Err(BranchError::Refused("main is the release line and is always open.".into()));
        }
        let all = self.store.reader()?.branches()?;
        if let Some((_, b)) =
            all.iter().find(|(id, b)| *id != branch && b.status == BranchStatus::Open && b.name.to_lowercase() == name.to_lowercase())
        {
            return Err(BranchError::NameTaken(b.name.clone()));
        }
        self.update_branch(branch, |b| {
            b.name = name.clone();
            b.status = BranchStatus::Open;
            b.merged = None;
        })
    }

    pub(super) fn update_branch(&mut self, branch: BranchId, change: impl FnOnce(&mut BranchRecord)) -> Result<BranchRecord, BranchError> {
        let mut w = self.store.writer()?;
        let mut rec = w.branch(branch)?.ok_or_else(|| BranchError::Refused("That branch does not exist.".into()))?;
        change(&mut rec);
        w.put_branch(branch, &rec)?;
        w.commit()?;
        if branch == self.branch {
            self.rec = rec.clone();
        }
        Ok(rec)
    }

    /// Show `branch` instead: load it as opening does (its latest snapshot and
    /// tail, or its parent's state at the fork) and, when the file is writable, write
    /// `meta.active` at once. The caller saves the branch shown first: switching with
    /// unsaved transactions is refused. A read-only file switches and writes nothing.
    ///
    /// # Errors
    /// [`BranchError`]; on an error the project still shows the previous branch.
    pub fn switch_to(&mut self, ledger: &Ledger, branch: BranchId, seed: u64) -> Result<(Ledger, Report), BranchError> {
        // Unsaved transactions are never switched away from, read-only or not.
        if ledger.head() != self.head.0 {
            return Err(BranchError::Refused("Save first: the branch shown has unsaved changes.".into()));
        }
        let rec = self.store.reader()?.branch(branch)?.ok_or_else(|| BranchError::Refused("That branch does not exist.".into()))?;
        if rec.status != BranchStatus::Open {
            return Err(BranchError::Refused(format!("Branch \"{}\" is not open; reopen it first.", rec.name)));
        }
        let l = load_branch(&self.store, self.project, &self.cache, branch, &rec, seed).map_err(|e| BranchError::Refused(e.to_string()))?;
        let writable = self.file_read_only.is_none();
        if writable {
            let mut w = self.store.writer()?;
            w.set_meta("active", &json!(branch.hex()))?;
            w.commit()?;
        }
        self.branch = branch;
        self.rec = rec;
        self.lineage = l.lineage;
        self.head = l.head;
        self.snap = l.snap;
        self.base_tag_high = l.base_tag_high;
        self.snap_ref = l.snap_ref;
        self.base_stacks = l.base_stacks;
        self.base_seq = l.report.snapshot_seq;
        self.read_only = l.damage.or_else(|| self.file_read_only.clone());
        Ok((l.ledger, l.report))
    }

    /// Transactions `from..=to` of `branch` as stored, for display (the History tab's times,
    /// authors and labels): each row is decoded and checked against its own
    /// chain value, not linked to its neighbours (Verify does that).
    ///
    /// # Errors
    /// The engine failed, or a row does not decode.
    pub fn tx_display(&self, branch: BranchId, from: u64, to: u64) -> Result<Vec<Transaction>, FileError> {
        if from > to {
            return Ok(Vec::new());
        }
        let r = self.store.reader()?;
        let mut out = Vec::new();
        for (seq, flag, stored, body) in r.tx_range(branch, from..=to)? {
            let record = pack::unpack(flag, &body)?;
            if chain::tx_hash(&record) != stored {
                return Err(FileError::TxChain(seq));
            }
            let value: Value = serde_json::from_slice(&record).map_err(|e| FileError::TxJson { seq, error: e.to_string() })?;
            out.push(record::decode_tx(&value).map_err(|error| FileError::TxDecode { seq, error })?);
        }
        Ok(out)
    }

    /// How many transactions [`Project::point_state`] replays for `seq` of `branch`: from the
    /// nearest snapshot at or before it on its line (never more than the snapshot interval past a
    /// snapshot, wherever the point is).
    ///
    /// # Errors
    /// The branch does not exist, or the file cannot be read.
    pub fn point_replay(&self, branch: BranchId, seq: u64) -> Result<u64, FileError> {
        let r = self.store.reader()?;
        let line = lineage(&r, self.project, branch)?;
        let (mut i, mut at, mut steps) = (0usize, seq, 0u64);
        loop {
            let l = line.get(i).ok_or(FileError::LineageIndex)?;
            if let Some(v) = nearest_snapshot(&r, l.branch, at)? {
                return Ok(steps + at - v.point.seq);
            }
            steps += at;
            match l.fork {
                Some((pi, fseq)) => (i, at) = (pi, fseq),
                None => return Ok(steps),
            }
        }
    }

    /// The document at `seq` of `branch` (any branch of the file): from its nearest snapshot, or
    /// its ancestry's, and at most the transactions after it (on a copy).
    ///
    /// # Errors
    /// The branch or point does not exist, or the file cannot be read.
    pub fn point_state(&self, branch: BranchId, seq: u64) -> Result<Document, FileError> {
        let r = self.store.reader()?;
        let line = lineage(&r, self.project, branch)?;
        let (doc, _) = line_state(&r, &line, 0, seq)?;
        Ok(doc)
    }
}
