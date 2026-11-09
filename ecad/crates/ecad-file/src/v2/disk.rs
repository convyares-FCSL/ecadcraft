//! Version 2 project files on disk (native only; EEP-0005 §6.4, §7).
//!
//! - **Open** takes the writer lock (`<file>.lock`, as version 1); redb's own lock is taken too.
//!   If another session holds the lock, the file opens read-only and follows nothing: a second
//!   opener reads the last committed state. A lock left by a session that crashed is reclaimed
//!   and reported. redb's quick-repair makes reopening after a crash fast.
//! - **Save** checks that the file at the path is still the one opened (its identity), then
//!   writes one redb transaction ([`ProjectV2::save`]).
//! - **Save As** (and the first save) writes a temporary file beside the target (a copy of the
//!   current file's rows, then the pending transactions), compacts it, renames it over the
//!   target and flushes the folder; the previous file's lock is released only after that.

use std::path::{Path, PathBuf};

use ecad_model::{Ledger, ProjectId};
use ecad_store::FileIdentity;
use ecad_store::disk::{self, LockAttempt, WriterLock};

use super::project::{OpenedV2, ProjectV2, ReportV2, SaveErrorV2, SavedV2};
use super::schema::DEFAULT_SNAPSHOT_EVERY;
use super::store::{Store, StoreError};
use crate::disk::{JUST_ENDED, ReadOnly, holder_record};

/// Why opening or saving on disk did not complete. The in-memory project is then unchanged and
/// nothing more is acknowledged than before.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DiskErrorV2 {
    #[error("could not open {path}: {error}")]
    Open { path: String, error: String },
    #[error("not saved: {path} is being edited in another session; nothing was written ({JUST_ENDED})")]
    TargetLocked { path: String },
    #[error("not saved: {step} failed ({error}); the existing file is unchanged")]
    BeforeReplacement { step: &'static str, error: String },
    #[error("written, but the system could not confirm the change is durable ({0}); save again")]
    Unconfirmed(String),
    #[error("not saved: the file at {0} was replaced, moved or removed by something else; nothing was written; use Save As")]
    Replaced(String),
    #[error("not saved: could not check that {path} is still the file this project opened ({error}); try again")]
    CannotCheck { path: String, error: String },
    #[error("not saved: this project was opened read-only because {0}; use Save As to save a copy")]
    ReadOnly(String),
    #[error("not saved: {0}")]
    Save(#[from] SaveErrorV2),
}

fn show(p: &Path) -> String {
    p.display().to_string()
}

/// Whether `a` and `b` name the same existing file.
pub fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// A version 2 project opened from disk.
#[derive(Debug)]
pub struct OpenedDiskV2 {
    pub project: DiskProjectV2,
    pub ledger: Ledger,
    pub report: ReportV2,
    /// A lock file left by a session that ended without releasing it, reclaimed on open.
    pub reclaimed_lock: Option<String>,
}

/// A version 2 project file on disk, with its writer lock.
#[derive(Debug)]
pub struct DiskProjectV2 {
    path: PathBuf,
    project: ProjectV2,
    /// Dropped after `project`, so redb's handle closes before the lock is released.
    lock: Option<WriterLock>,
    identity: Option<FileIdentity>,
    read_only: Option<ReadOnly>,
    /// Save As renamed the file into place, but the folder flush failed: nothing it wrote is
    /// acknowledged until a later Save confirms it.
    unconfirmed: bool,
}

fn holder_text(holder: Option<&str>) -> String {
    match holder {
        Some(h) => format!(" ({h})"),
        None if !disk::HOLDER_SHOWN => " (Windows does not say which session)".into(),
        None => String::new(),
    }
}

impl DiskProjectV2 {
    /// Open the version 2 project file at `path`. `seed` seeds IDs allocated after opening.
    ///
    /// # Errors
    /// [`DiskErrorV2::Open`]: the file cannot be read or is not a version 2 project.
    pub fn open(path: &Path, seed: u64, writer: &str) -> Result<OpenedDiskV2, DiskErrorV2> {
        Self::open_as(path, seed, writer, false)
    }

    /// [`DiskProjectV2::open`] without taking the writer lock or opening the database for
    /// writing: the file is never changed, not even redb's own bookkeeping (an agent with writes
    /// off). A file that needs recovery after a crash is refused until a writer opens it.
    ///
    /// # Errors
    /// As [`DiskProjectV2::open`].
    pub fn open_read_only(path: &Path, seed: u64, writer: &str) -> Result<OpenedDiskV2, DiskErrorV2> {
        Self::open_as(path, seed, writer, true)
    }

    fn open_as(path: &Path, seed: u64, writer: &str, reading: bool) -> Result<OpenedDiskV2, DiskErrorV2> {
        let open_err = |e: String| DiskErrorV2::Open { path: show(path), error: e };
        if !path.is_file() {
            return Err(open_err("there is no file there".into()));
        }
        let attempt = if reading { Ok(LockAttempt::HeldByOther { holder: None }) } else { WriterLock::try_acquire(path, &holder_record(writer)) };
        let (mut lock, mut read_only, reclaimed) = match attempt {
            Ok(LockAttempt::Acquired { lock, leftover }) => (Some(lock), None, leftover),
            Ok(LockAttempt::HeldByOther { .. }) if reading => (None, Some(ReadOnly::NotWritable("it was opened for reading only".into())), None),
            Ok(LockAttempt::HeldByOther { holder }) => (None, Some(ReadOnly::Locked { holder }), None),
            Err(e) => (None, Some(ReadOnly::NoLock(e.to_string())), None),
        };
        let store = match Store::open(path, lock.is_some()) {
            Ok(s) => s,
            Err(StoreError::Busy) if lock.is_some() => {
                // redb's lock is held by a process without ours (an older build, or a tool).
                lock = None;
                read_only = Some(ReadOnly::Locked { holder: None });
                Store::open(path, false).map_err(|e| open_err(e.to_string()))?
            }
            Err(e) if lock.is_some() && e.to_string().contains("denied") => {
                lock = None;
                read_only = Some(ReadOnly::NotWritable(e.to_string()));
                Store::open(path, false).map_err(|e| open_err(e.to_string()))?
            }
            Err(e) => return Err(open_err(e.to_string())),
        };
        let identity = match disk::retry_transient(|| FileIdentity::at(path)) {
            Ok(id) => Some(id),
            Err(e) => {
                read_only =
                    read_only.or(Some(ReadOnly::NotWritable(format!("its identity could not be read ({e}), so a replacement could not be noticed"))));
                None
            }
        };
        let OpenedV2 { mut project, ledger, report } = ProjectV2::open(store, seed, writer).map_err(|e| open_err(e.to_string()))?;
        if report.damage.is_some() {
            read_only = read_only.or(Some(ReadOnly::Damaged));
        }
        if let Some(r) = &read_only {
            project.set_read_only(r.to_string());
        }
        let me = Self { path: path.to_path_buf(), project, lock, identity, read_only, unconfirmed: false };
        Ok(OpenedDiskV2 { project: me, ledger, report, reclaimed_lock: reclaimed })
    }

    /// Write a project to `path` as a new file (the first save, or Save As): take the target's
    /// lock, write a temporary file (`from`'s rows, if given, then `ledger`'s transactions after
    /// them; otherwise `ledger`'s whole history), compact it, rename it over `path`, flush the
    /// folder, and open it. Only a confirmed write is acknowledged in `ledger`.
    ///
    /// # Errors
    /// [`DiskErrorV2::TargetLocked`]; [`DiskErrorV2::BeforeReplacement`] when a step before the
    /// rename failed (an existing file at `path` is unchanged; the temporary file is removed).
    pub fn create(
        path: &Path, project: ProjectId, ledger: &mut Ledger, from: Option<&ProjectV2>, created_ms: Option<u64>, writer: &str,
    ) -> Result<(Self, Option<String>), DiskErrorV2> {
        let before = |step: &'static str, error: String| DiskErrorV2::BeforeReplacement { step, error };
        let lock = match WriterLock::try_acquire(path, &holder_record(writer)) {
            Ok(LockAttempt::Acquired { lock, .. }) => lock,
            Ok(LockAttempt::HeldByOther { holder }) => {
                let _ = holder_text(holder.as_deref());
                return Err(DiskErrorV2::TargetLocked { path: show(path) });
            }
            Err(e) => return Err(before("taking the file's writer lock", e.to_string())),
        };
        let temp = disk::temp_path(path);
        let every = from.map_or(DEFAULT_SNAPSHOT_EVERY, ProjectV2::snapshot_every);
        let written = (|| -> Result<(), DiskErrorV2> {
            let mut store = Store::create(&temp).map_err(|e| before("creating the temporary file", e.to_string()))?;
            match from {
                Some(src) => {
                    let r = src.store().reader().map_err(|e| before("reading the current file", e.to_string()))?;
                    let mut w = store.writer().map_err(|e| before("writing the temporary file", e.to_string()))?;
                    let rescue = src.read_only().is_some();
                    let cut = rescue.then(|| (src.branch(), src.head().0));
                    w.copy_all(&r, cut).map_err(|e| before("copying the history", e.to_string()))?;
                    if rescue {
                        src.write_rescue(&mut w, &r).map_err(|e| before("copying the history", e.to_string()))?;
                    }
                    w.commit().map_err(|e| before("writing the temporary file", e.to_string()))?;
                    store.compact().map_err(|e| before("compacting the temporary file", e.to_string()))?;
                }
                None => {
                    let (p, _) = ProjectV2::create(store, project, ledger, None, None, created_ms, writer, every)
                        .map_err(|e| before("writing the temporary file", e.to_string()))?;
                    drop(p);
                    store = Store::open(&temp, true).map_err(|e| before("reopening the temporary file", e.to_string()))?;
                    store.compact().map_err(|e| before("compacting the temporary file", e.to_string()))?;
                }
            }
            drop(store);
            disk::replace(&temp, path).map_err(|e| before("replacing the file", e.to_string()))
        })();
        if let Err(e) = written {
            let _ = disk::remove_file(&temp);
            return Err(e);
        }
        let synced = disk::sync_dir(&disk::parent_dir(path));
        let store = Store::open(path, true).map_err(|e| DiskErrorV2::Unconfirmed(e.to_string()))?;
        let OpenedV2 { project: opened, .. } = ProjectV2::open(store, 0, writer).map_err(|e| DiskErrorV2::Unconfirmed(e.to_string()))?;
        let identity = disk::retry_transient(|| FileIdentity::at(path)).ok();
        // The history before the ledger's base is now read from the new file.
        if from.is_some() {
            ledger.set_history(opened.history());
        }
        let mut me = Self { path: path.to_path_buf(), project: opened, lock: Some(lock), identity, read_only: None, unconfirmed: false };
        if let Err(e) = synced {
            // Bound to the new file, but nothing is acknowledged: the next Save confirms it.
            me.unconfirmed = true;
            return Ok((me, Some(e.to_string())));
        }
        // The pending transactions (Save As of a project opened from a file).
        if from.is_some() {
            me.project.save(ledger)?;
        } else {
            ledger.mark_persisted(ledger.head()).map_err(|e| DiskErrorV2::Unconfirmed(e.to_string()))?;
        }
        Ok((me, None))
    }

    /// Whether the last Save As is still not confirmed durable.
    pub fn unconfirmed(&self) -> bool {
        self.unconfirmed
    }

    /// Save the transactions after the file's head.
    ///
    /// # Errors
    /// [`DiskErrorV2`]; the in-memory project is unchanged and still unsaved.
    pub fn save(&mut self, ledger: &mut Ledger) -> Result<SavedV2, DiskErrorV2> {
        if let Some(r) = &self.read_only {
            return Err(DiskErrorV2::ReadOnly(r.to_string()));
        }
        match (&self.identity, disk::retry_transient(|| FileIdentity::at(&self.path))) {
            (Some(ours), Ok(now)) if *ours == now => {}
            (_, Err(e)) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(DiskErrorV2::CannotCheck { path: show(&self.path), error: e.to_string() });
            }
            _ => return Err(DiskErrorV2::Replaced(show(&self.path))),
        }
        if self.unconfirmed {
            disk::sync_dir(&disk::parent_dir(&self.path)).map_err(|e| {
                DiskErrorV2::Unconfirmed(format!("the earlier Save As is still not confirmed durable ({e}); the new changes were not written"))
            })?;
            self.unconfirmed = false;
            let head = self.project.head().0;
            if head > ledger.persisted() {
                ledger.mark_persisted(head).map_err(|e| DiskErrorV2::Unconfirmed(e.to_string()))?;
            }
        }
        Ok(self.project.save(ledger)?)
    }

    /// Save As `path`: copy this file's history and the pending transactions to a new file and
    /// bind to it. Saving as the file already bound is a plain save. A read-only project can be
    /// saved as another file (how damaged or locked work is rescued).
    ///
    /// # Errors
    /// As [`DiskProjectV2::create`]; the project then stays bound to its previous file.
    pub fn save_as(&mut self, path: &Path, ledger: &mut Ledger, created_ms: Option<u64>, writer: &str) -> Result<Option<String>, DiskErrorV2> {
        if same_file(path, &self.path) {
            self.save(ledger)?;
            return Ok(None);
        }
        let (next, unconfirmed) = Self::create(path, self.project.project_id(), ledger, Some(&self.project), created_ms, writer)?;
        *self = next;
        Ok(unconfirmed)
    }

    /// The file's path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The project in the file.
    pub fn project(&self) -> &ProjectV2 {
        &self.project
    }

    /// The project in the file, to change its settings (the snapshot interval).
    pub fn project_mut(&mut self) -> &mut ProjectV2 {
        &mut self.project
    }

    /// Why the project cannot be saved to its own file, if it cannot.
    pub fn read_only(&self) -> Option<&ReadOnly> {
        self.read_only.as_ref()
    }

    /// The lock file, while this project holds the lock.
    pub fn lock_file(&self) -> Option<&Path> {
        self.lock.as_ref().map(WriterLock::path)
    }
}
