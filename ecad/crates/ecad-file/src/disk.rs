//! Project files on disk (native only; proposal §20.5, persistence stage 2).
//!
//! [`DiskProject`] binds a project to a `.ecad` file and holds its writer lock:
//!
//! - **Open** takes the writer lock; if another session holds it, the project opens read-only
//!   and says who holds it. A lock file left by a session that crashed is reclaimed and reported:
//!   the operating system releases a dead process's lock, so a crash never locks a project out.
//! - **Save** appends with the retry rules of [`ProjectFile::save`], after checking that the file
//!   at the path is still the one opened. Interrupted trailing bytes found at open are first
//!   copied to a uniquely named sidecar (`<name>.tail-<offset>-<unique>`); if that copy fails,
//!   the file is left untouched and the failure reported (E13).
//! - **Save As** (and the first save) writes a temporary file beside the target, flushes it,
//!   renames it over the target and flushes the folder. A failure before the rename leaves any
//!   existing target unchanged. A failed folder flush after it means durability is unconfirmed:
//!   the file is ours, but nothing is acknowledged until a later Save confirms it. The target's
//!   lock is taken first (refused if held by another session), and the previous path's lock is
//!   released only after a successful replacement. The project keeps its ID (E9).
//!
//! Guarantees are those of the file system: relied on, and tested, on a Linux file system
//! (including the Linux home directory under WSL). Others, including Windows drives under WSL,
//! are unverified.

use std::path::{Path, PathBuf};

use ecad_model::{Ledger, ProjectId};
use ecad_store::disk::{self, LockAttempt, WriterLock};
use ecad_store::{FileIdentity, FileMedium, Medium};

use crate::read::{Access, OpenError, OpenReport};
use crate::write::{ProjectFile, SaveError, SaveOutcome};

/// Faults injected into the steps of Save and Save As (tests). Each is used once.
#[derive(Debug, Default, Clone)]
pub struct StepFaults {
    /// Creating the temporary file fails.
    pub fail_temp_create: bool,
    /// Writing the temporary file stops after this many bytes and fails.
    pub temp_partial_append: Option<usize>,
    /// Flushing the temporary file fails.
    pub fail_temp_flush: bool,
    /// Renaming the temporary file over the target fails.
    pub fail_rename: bool,
    /// Flushing the folder after the rename (or when confirming a pending one) fails.
    pub fail_dir_sync: bool,
    /// Writing the sidecar that preserves interrupted trailing bytes fails.
    pub fail_sidecar: bool,
}

/// Why a project cannot be saved to its own file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOnly {
    /// Damage was found on open; the file is preserved as it is (see the open report).
    Damaged,
    /// Another session is editing the file (it holds the writer lock).
    Locked {
        /// What that session recorded about itself.
        holder: Option<String>,
    },
}

impl std::fmt::Display for ReadOnly {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadOnly::Damaged => f.write_str("the file is damaged and is kept exactly as it is"),
            ReadOnly::Locked { holder: Some(h) } => write!(f, "another session is editing it ({h})"),
            ReadOnly::Locked { holder: None } => f.write_str("another session is editing it"),
        }
    }
}

/// Whether a file written by Save As is known to be durable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Durability {
    /// Flushed and its folder entry flushed: acknowledged.
    Confirmed,
    /// It replaced its target, but the folder flush failed: after a crash the folder could show
    /// the old or the new file. Nothing is acknowledged; the next Save confirms it first.
    Unconfirmed(String),
}

/// Why opening or saving on disk did not complete. In every case the in-memory project is
/// unchanged and nothing more is acknowledged than before.
#[derive(Debug, thiserror::Error)]
pub enum DiskError {
    #[error("not saved: {path} is being edited in another session{holder}; nothing was written")]
    TargetLocked { path: String, holder: String },
    #[error("not saved: {step} failed ({error}); the existing file is unchanged{cleanup}")]
    BeforeReplacement { step: &'static str, error: String, cleanup: String },
    #[error("written, but the system could not confirm the change is durable ({0}); save again")]
    Unconfirmed(String),
    #[error("not saved: this project was opened read-only because {0}; use Save As to save a copy")]
    ReadOnly(ReadOnly),
    #[error(
        "not saved: the interrupted bytes at the end of the file could not be preserved in {path} ({error}); \
the file was not changed"
    )]
    PreserveFailed { path: String, error: String },
    #[error(
        "not saved: the file at {0} was replaced, moved or removed by something else; nothing was written; use Save As"
    )]
    Replaced(String),
    #[error("not saved: {0}")]
    Save(#[from] SaveError),
    #[error("could not open {path}: {error}")]
    Open { path: String, error: String },
}

/// A project opened from disk.
#[derive(Debug)]
pub struct OpenedDisk {
    pub project: DiskProject,
    pub ledger: Ledger,
    pub report: OpenReport,
    /// A lock file left by a session that ended without releasing it, reclaimed on open: what
    /// that session recorded.
    pub reclaimed_lock: Option<String>,
}

/// A project file on disk, with its writer lock.
#[derive(Debug)]
pub struct DiskProject {
    path: PathBuf,
    file: ProjectFile<FileMedium>,
    lock: Option<WriterLock>,
    read_only: Option<ReadOnly>,
    identity: Option<FileIdentity>,
    /// Written and renamed into place, but the folder flush has not been confirmed.
    unconfirmed: bool,
    /// The sidecar holding the interrupted trailing bytes, once preserved.
    preserved: Option<PathBuf>,
    /// Faults to inject (tests).
    pub faults: StepFaults,
}

/// What a writer records in the lock file about itself.
pub fn holder_record(writer: &str) -> String {
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    format!("{writer}; process {}; since {since} (seconds since 1970)", std::process::id())
}

fn holder_text(holder: Option<&str>) -> String {
    holder.map(|h| format!(" ({h})")).unwrap_or_default()
}

fn show(p: &Path) -> String {
    p.display().to_string()
}

/// Remove a temporary file after a failed Save As; the text of a failure to do so.
fn clean_up(temp: &Path) -> String {
    match std::fs::remove_file(temp) {
        Ok(()) => String::new(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => format!("; the temporary file {} could not be removed ({e})", show(temp)),
    }
}

fn injected(what: &str) -> std::io::Error {
    std::io::Error::other(format!("injected {what} failure"))
}

/// Whether `a` and `b` name the same existing file.
fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

impl DiskProject {
    /// Write the project to `path` as a new file (the first save, or Save As): take the target's
    /// lock, write and flush a temporary file, rename it over `path`, flush the folder. Returns the
    /// bound project and whether durability is confirmed; only a confirmed write is acknowledged
    /// in `ledger`.
    ///
    /// # Errors
    /// [`DiskError::TargetLocked`] when another session holds `path`'s lock;
    /// [`DiskError::BeforeReplacement`] when a step before the rename failed (any existing file at
    /// `path` is unchanged and the temporary file is removed).
    pub fn create(
        path: &Path,
        project: ProjectId,
        ledger: &mut Ledger,
        created_ms: Option<u64>,
        writer: &str,
        mut faults: StepFaults,
    ) -> Result<(Self, Durability), DiskError> {
        Self::create_with(path, project, ledger, created_ms, writer, &mut faults).map(|(mut me, d)| {
            me.faults = faults;
            (me, d)
        })
    }

    /// [`DiskProject::create`] using (and consuming) `faults`.
    fn create_with(
        path: &Path,
        project: ProjectId,
        ledger: &mut Ledger,
        created_ms: Option<u64>,
        writer: &str,
        faults: &mut StepFaults,
    ) -> Result<(Self, Durability), DiskError> {
        let lock = match WriterLock::try_acquire(path, &holder_record(writer)) {
            Ok(LockAttempt::Acquired { lock, .. }) => lock,
            Ok(LockAttempt::HeldByOther { holder }) => {
                return Err(DiskError::TargetLocked { path: show(path), holder: holder_text(holder.as_deref()) });
            }
            Err(e) => {
                return Err(DiskError::BeforeReplacement {
                    step: "taking the file's writer lock",
                    error: e.to_string(),
                    cleanup: String::new(),
                });
            }
        };
        let (file, durability) = write_and_replace(path, project, ledger, created_ms, writer, faults)?;
        if durability == Durability::Confirmed {
            file.acknowledge(ledger)?;
        }
        let identity = file.medium().identity().ok().flatten();
        let unconfirmed = durability != Durability::Confirmed;
        let me = Self {
            path: path.to_path_buf(),
            file,
            lock: Some(lock),
            read_only: None,
            identity,
            unconfirmed,
            preserved: None,
            faults: StepFaults::default(),
        };
        Ok((me, durability))
    }

    /// Open the project file at `path`: take its writer lock (read-only if another session holds
    /// it) and read it (read-only if damaged, §20.6). `seed` seeds transaction IDs.
    ///
    /// # Errors
    /// [`DiskError::Open`]: the file cannot be read, or has no valid, supported header.
    pub fn open(path: &Path, seed: u64, writer: &str) -> Result<OpenedDisk, DiskError> {
        let open_err = |e: String| DiskError::Open { path: show(path), error: e };
        if !path.is_file() {
            return Err(open_err("there is no file there".into()));
        }
        let (lock, read_only, reclaimed_lock) = match WriterLock::try_acquire(path, &holder_record(writer)) {
            Ok(LockAttempt::Acquired { lock, leftover }) => (Some(lock), None, leftover),
            Ok(LockAttempt::HeldByOther { holder }) => (None, Some(ReadOnly::Locked { holder }), None),
            // A folder we cannot create a lock file in: the file can still be read, not written.
            Err(e) => (None, Some(ReadOnly::Locked { holder: Some(format!("no lock could be taken: {e}")) }), None),
        };
        let medium = FileMedium::open(path, lock.is_some()).map_err(|e| open_err(e.to_string()))?;
        let identity = medium.identity().ok().flatten();
        let (file, ledger, report) = ProjectFile::open(medium, seed).map_err(|e: OpenError| open_err(e.to_string()))?;
        let read_only = read_only.or((report.access == Access::ReadOnly).then_some(ReadOnly::Damaged));
        let project = Self {
            path: path.to_path_buf(),
            file,
            lock,
            read_only,
            identity,
            unconfirmed: false,
            preserved: None,
            faults: StepFaults::default(),
        };
        Ok(OpenedDisk { project, ledger, report, reclaimed_lock })
    }

    /// Save the transactions after the last acknowledged one (§20.5).
    ///
    /// # Errors
    /// [`DiskError`]; the in-memory project is unchanged and still unsaved.
    pub fn save(&mut self, ledger: &mut Ledger) -> Result<SaveOutcome, DiskError> {
        if let Some(r) = &self.read_only {
            return Err(DiskError::ReadOnly(r.clone()));
        }
        // The file at the path must still be the one this project wrote or opened.
        match FileIdentity::at(&self.path) {
            Ok(id) if id == self.identity => {}
            _ => return Err(DiskError::Replaced(show(&self.path))),
        }
        if self.unconfirmed {
            self.confirm(ledger)?;
        }
        self.preserve_fragment()?;
        Ok(self.file.save(ledger)?)
    }

    /// Save As: write the project to `path` as a new file and bind to it (the project keeps its
    /// ID). Saving as the file already bound is a plain [`DiskProject::save`]. A read-only
    /// project can be saved as another file, which is how damaged or locked work is rescued.
    ///
    /// # Errors
    /// As [`DiskProject::create`]; the project then stays bound to its previous file and lock.
    pub fn save_as(
        &mut self,
        path: &Path,
        ledger: &mut Ledger,
        created_ms: Option<u64>,
        writer: &str,
    ) -> Result<Durability, DiskError> {
        if same_file(path, &self.path) {
            self.save(ledger)?;
            return Ok(if self.unconfirmed {
                Durability::Unconfirmed("not yet confirmed".into())
            } else {
                Durability::Confirmed
            });
        }
        let id = self
            .project_id()
            .ok_or_else(|| DiskError::Open { path: show(&self.path), error: "the file has no project ID".into() })?;
        let (mut next, durability) = Self::create_with(path, id, ledger, created_ms, writer, &mut self.faults)?;
        next.faults = std::mem::take(&mut self.faults);
        // Replacing `self` releases the previous file's lock, only now.
        *self = next;
        Ok(durability)
    }

    /// Confirm a replacement whose folder flush failed: flush the file and its folder again,
    /// then acknowledge it.
    fn confirm(&mut self, ledger: &mut Ledger) -> Result<(), DiskError> {
        let r = self.file.medium_mut().flush().and_then(|()| {
            if std::mem::take(&mut self.faults.fail_dir_sync) {
                return Err(injected("folder flush"));
            }
            disk::sync_dir(&disk::parent_dir(&self.path))
        });
        r.map_err(|e| DiskError::Unconfirmed(e.to_string()))?;
        self.file.acknowledge(ledger)?;
        self.unconfirmed = false;
        Ok(())
    }

    /// Copy interrupted trailing bytes found at open to a new sidecar file before a save removes
    /// them (E13). A failure leaves the project file untouched.
    fn preserve_fragment(&mut self) -> Result<(), DiskError> {
        if self.preserved.is_some() {
            return Ok(());
        }
        let Some((offset, bytes)) = self.file.fragment() else { return Ok(()) };
        let sidecar = disk::sibling(&self.path, &format!(".tail-{offset}-{}", disk::unique()));
        let r = if std::mem::take(&mut self.faults.fail_sidecar) {
            Err(injected("sidecar write"))
        } else {
            disk::write_new_file(&sidecar, bytes).and_then(|()| disk::sync_dir(&disk::parent_dir(&self.path)))
        };
        match r {
            Ok(()) => {
                self.preserved = Some(sidecar);
                Ok(())
            }
            Err(e) => Err(DiskError::PreserveFailed { path: show(&sidecar), error: e.to_string() }),
        }
    }

    /// The file's path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The project ID recorded in the file's header.
    pub fn project_id(&self) -> Option<ProjectId> {
        self.file.header().project_id
    }

    /// Why the project cannot be saved to its own file, if it cannot.
    pub fn read_only(&self) -> Option<&ReadOnly> {
        self.read_only.as_ref()
    }

    /// Whether the last Save As replaced its target without confirming durability.
    pub fn unconfirmed(&self) -> bool {
        self.unconfirmed
    }

    /// The sidecar holding interrupted trailing bytes, once preserved.
    pub fn preserved_fragment(&self) -> Option<&Path> {
        self.preserved.as_deref()
    }

    /// The lock file, while this project holds the lock.
    pub fn lock_file(&self) -> Option<&Path> {
        self.lock.as_ref().map(WriterLock::path)
    }

    /// The underlying file writer.
    pub fn file(&self) -> &ProjectFile<FileMedium> {
        &self.file
    }

    /// The underlying file writer, to inject medium faults in tests.
    pub fn file_mut(&mut self) -> &mut ProjectFile<FileMedium> {
        &mut self.file
    }
}

/// Steps (b)-(f) of Save As (§20.5): temporary file, write and flush, rename, folder flush.
fn write_and_replace(
    path: &Path,
    project: ProjectId,
    ledger: &Ledger,
    created_ms: Option<u64>,
    writer: &str,
    faults: &mut StepFaults,
) -> Result<(ProjectFile<FileMedium>, Durability), DiskError> {
    let before =
        |step: &'static str, error: String, cleanup: String| DiskError::BeforeReplacement { step, error, cleanup };
    let temp = disk::temp_path(path);
    let created = if std::mem::take(&mut faults.fail_temp_create) {
        Err(injected("temporary file creation"))
    } else {
        FileMedium::create_new(&temp)
    };
    let mut medium = created.map_err(|e| before("creating the temporary file", e.to_string(), String::new()))?;
    medium.faults.partial_append = faults.temp_partial_append.take();
    medium.faults.fail_next_flush = std::mem::take(&mut faults.fail_temp_flush);
    let mut file = match ProjectFile::write_new(medium, project, ledger, created_ms, writer) {
        Ok(f) => f,
        Err(e) => {
            let step = match e {
                SaveError::Flush(_) => "flushing the temporary file",
                _ => "writing the temporary file",
            };
            return Err(before(step, e.to_string(), clean_up(&temp)));
        }
    };
    let renamed =
        if std::mem::take(&mut faults.fail_rename) { Err(injected("rename")) } else { disk::replace(&temp, path) };
    if let Err(e) = renamed {
        drop(file);
        return Err(before("replacing the file", e.to_string(), clean_up(&temp)));
    }
    file.medium_mut().renamed_to(path);
    let synced = if std::mem::take(&mut faults.fail_dir_sync) {
        Err(injected("folder flush"))
    } else {
        disk::sync_dir(&disk::parent_dir(path))
    };
    let durability = match synced {
        Ok(()) => Durability::Confirmed,
        Err(e) => Durability::Unconfirmed(e.to_string()),
    };
    Ok((file, durability))
}
