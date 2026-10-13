//! Files on disk (native only): the file medium, the writer lock and the steps of an atomic
//! replacement (proposal §20.5). What is written, and in which order, is decided by `ecad-file`;
//! this module only performs each step and reports its result.
//!
//! Guarantees are those of the operating system and file system: they are relied on, and tested,
//! on a Linux file system (ext4, including the Linux home directory under WSL). Other file
//! systems, including Windows drives mounted under WSL (`/mnt/c`), are not verified.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::Medium;

/// The largest file read whole into memory. Larger files are refused rather than read
/// (hostile-input cap; a project of this size is far beyond anything expected).
pub const MAX_FILE: u64 = 1 << 30;

/// Faults injected into a [`FileMedium`] (tests). Each is used once.
#[derive(Debug, Default, Clone)]
pub struct MediumFaults {
    /// The next append writes this many bytes (at most) and then fails.
    pub partial_append: Option<usize>,
    /// The next flush fails (nothing is flushed).
    pub fail_next_flush: bool,
    /// The next truncate fails and changes nothing.
    pub fail_next_truncate: bool,
}

/// A file on disk as a [`Medium`]. Appends go to the end of the file; `flush` is `sync_data`.
#[derive(Debug)]
pub struct FileMedium {
    file: File,
    path: PathBuf,
    writable: bool,
    /// Faults to inject (tests).
    pub faults: MediumFaults,
}

impl FileMedium {
    /// Open an existing file, for reading and appending when `writable`, else read-only.
    ///
    /// # Errors
    /// The file does not exist or cannot be opened.
    pub fn open(path: &Path, writable: bool) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(writable).open(path)?;
        Ok(Self { file, path: path.to_path_buf(), writable, faults: MediumFaults::default() })
    }

    /// Create a new, empty file; fails if `path` exists.
    ///
    /// # Errors
    /// The file exists or cannot be created.
    pub fn create_new(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).create_new(true).open(path)?;
        Ok(Self { file, path: path.to_path_buf(), writable: true, faults: MediumFaults::default() })
    }

    /// The path the file was opened at (it may have been renamed since).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Record that the file now lives at `path` (after this process renamed it).
    pub fn renamed_to(&mut self, path: &Path) {
        self.path = path.to_path_buf();
    }

    /// Whether appends are allowed.
    pub fn writable(&self) -> bool {
        self.writable
    }

    /// The identity of the open file (not of whatever is at its path now).
    ///
    /// # Errors
    /// The file's metadata cannot be read.
    pub fn identity(&self) -> io::Result<Option<FileIdentity>> {
        Ok(FileIdentity::of(&self.file.metadata()?))
    }

    fn read_only() -> io::Error {
        io::Error::new(io::ErrorKind::PermissionDenied, "the file is open read-only")
    }
}

impl Medium for FileMedium {
    fn len(&self) -> u64 {
        // An unreadable length reads as 0, which the writer treats as an external change.
        self.file.metadata().map_or(0, |m| m.len())
    }

    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        if !self.writable {
            return Err(Self::read_only());
        }
        self.file.seek(SeekFrom::End(0))?;
        if let Some(n) = self.faults.partial_append.take() {
            self.file.write_all(bytes.get(..n.min(bytes.len())).unwrap_or_default())?;
            return Err(io::Error::other("injected write failure"));
        }
        self.file.write_all(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        if std::mem::take(&mut self.faults.fail_next_flush) {
            return Err(io::Error::other("injected flush failure"));
        }
        self.file.sync_data()
    }

    fn truncate(&mut self, len: u64) -> io::Result<()> {
        if !self.writable {
            return Err(Self::read_only());
        }
        if std::mem::take(&mut self.faults.fail_next_truncate) {
            return Err(io::Error::other("injected truncate failure"));
        }
        self.file.set_len(len)
    }

    fn read_all(&self) -> io::Result<Vec<u8>> {
        let len = self.len();
        if len > MAX_FILE {
            return Err(io::Error::other(format!("the file is larger than {MAX_FILE} bytes")));
        }
        self.read_range(0, len)
    }

    fn read_range(&self, offset: u64, len: u64) -> io::Result<Vec<u8>> {
        if len > MAX_FILE {
            return Err(io::Error::other(format!("a read larger than {MAX_FILE} bytes")));
        }
        let mut f = &self.file;
        f.seek(SeekFrom::Start(offset))?;
        let mut out = Vec::with_capacity(usize::try_from(len.min(1 << 20)).unwrap_or(0));
        f.take(len).read_to_end(&mut out)?;
        Ok(out)
    }
}

/// Which file a path or handle refers to (device and inode), to notice a file replaced behind
/// the writer's back. Only available on Unix; elsewhere this check is not made (unverified).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileIdentity {
    dev: u64,
    ino: u64,
}

impl FileIdentity {
    #[cfg(unix)]
    fn of(m: &std::fs::Metadata) -> Option<Self> {
        use std::os::unix::fs::MetadataExt;
        Some(Self { dev: m.dev(), ino: m.ino() })
    }

    #[cfg(not(unix))]
    fn of(_m: &std::fs::Metadata) -> Option<Self> {
        None
    }

    /// The identity of the file now at `path`; `Ok(None)` where identities are not available.
    ///
    /// # Errors
    /// Nothing is at `path`, or its metadata cannot be read.
    pub fn at(path: &Path) -> io::Result<Option<Self>> {
        Ok(Self::of(&std::fs::metadata(path)?))
    }
}

/// The lock file guarding writers of `project`: `<file name>.lock` beside it.
pub fn lock_path(project: &Path) -> PathBuf {
    sibling(project, ".lock")
}

/// `project`'s path with `suffix` appended to its file name.
pub fn sibling(project: &Path, suffix: &str) -> PathBuf {
    let mut name = project.file_name().map(std::ffi::OsStr::to_os_string).unwrap_or_default();
    name.push(suffix);
    project.with_file_name(name)
}

/// The outcome of trying to take a project's writer lock.
#[derive(Debug)]
pub enum LockAttempt {
    /// This process is now the writer.
    Acquired {
        lock: WriterLock,
        /// A lock file left by a session that ended without releasing it (a crash), with what
        /// that session recorded. Its lock had already been released by the operating system,
        /// so it was reclaimed.
        leftover: Option<String>,
    },
    /// Another session holds the lock: open read-only.
    HeldByOther {
        /// What the holder recorded about itself, when it can be read.
        holder: Option<String>,
    },
}

/// An exclusive writer lock: an operating-system lock (`flock` on Unix, `LockFileEx` on Windows,
/// through `std::fs::File::try_lock`) on the project's lock file, held for as long as this value
/// lives.
///
/// # Semantics
/// - The operating system releases the lock when the process ends, including by a crash, so no
///   lock outlives its process and a crash never leaves a permanent lockout.
/// - The lock file itself is never deleted (deleting it could let two writers lock different
///   files). On a clean release its contents are cleared; a lock file that still holds a record
///   but has no holder was left by a session that ended without releasing it, and is reported as
///   such when it is reclaimed.
/// - A lock is per open file: a second attempt in the same process is refused too.
#[derive(Debug)]
pub struct WriterLock {
    file: File,
    path: PathBuf,
}

impl WriterLock {
    /// Try to take the writer lock of `project`, recording `holder` (who and since when) in the
    /// lock file for anyone refused later.
    ///
    /// # Errors
    /// The lock file cannot be created, opened or written.
    pub fn try_acquire(project: &Path, holder: &str) -> io::Result<LockAttempt> {
        let path = lock_path(project);
        let mut file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                let mut s = String::new();
                // Best effort: some systems refuse reads of a locked file.
                let holder = file.read_to_string(&mut s).ok().map(|_| s.trim().to_string()).filter(|s| !s.is_empty());
                return Ok(LockAttempt::HeldByOther { holder });
            }
            Err(TryLockError::Error(e)) => return Err(e),
        }
        let mut previous = String::new();
        file.read_to_string(&mut previous)?;
        let leftover = Some(previous.trim().to_string()).filter(|s| !s.is_empty());
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(holder.as_bytes())?;
        file.sync_data()?;
        Ok(LockAttempt::Acquired { lock: WriterLock { file, path }, leftover })
    }

    /// The lock file's path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for WriterLock {
    fn drop(&mut self) {
        // A clean release clears the record, then the lock goes with the handle. Errors here only
        // mean a later opener reports a leftover lock file; they are harmless.
        let _ = self.file.set_len(0);
        let _ = self.file.unlock();
    }
}

/// A unique name for a temporary file beside `target` (same folder, so a rename is atomic).
pub fn temp_path(target: &Path) -> PathBuf {
    sibling(target, &format!(".tmp-{}", unique()))
}

/// A unique suffix: process id, clock and a per-process counter.
pub fn unique() -> String {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    format!("{}-{t}-{n}", std::process::id())
}

/// Rename `from` over `to`, replacing it (atomic within one file system).
///
/// # Errors
/// The rename failed; `to` is unchanged.
pub fn replace(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::rename(from, to)
}

/// Make a folder's entries (a rename or a new file) durable. On Unix this flushes the folder; on
/// other systems it is not available and succeeds without doing anything (unverified).
///
/// # Errors
/// The folder could not be flushed.
pub fn sync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(dir)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        Ok(())
    }
}

/// The folder holding `path` (`.` for a bare file name).
pub fn parent_dir(path: &Path) -> PathBuf {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Write `bytes` to a new file at `path` (refusing to overwrite anything) and flush it.
///
/// # Errors
/// The file exists, or creating, writing or flushing failed (a partial file is then removed,
/// best effort).
pub fn write_new_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut f = OpenOptions::new().write(true).create_new(true).open(path)?;
    let r = f.write_all(bytes).and_then(|()| f.sync_all());
    if r.is_err() {
        drop(f);
        let _ = std::fs::remove_file(path);
    }
    r
}
