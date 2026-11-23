//! Files on disk (native only): the file medium, the writer lock and the steps of an atomic
//! replacement. What is written, and in which order, is decided by `ecad-file`;
//! this module only performs each step and reports its result.
//!
//! Guarantees are those of the operating system and file system: they are relied on, and tested,
//! on a Linux file system (ext4, including the Linux home directory under WSL) and on NTFS under
//! native Windows. Other file systems are not verified: FAT and exFAT drives, network shares
//! (SMB), ReFS (including Windows 11 Dev Drives, where a 64-bit file index is not guaranteed
//! unique, so file identity is unverified there), and Windows drives mounted under WSL
//! (`/mnt/c`).
//!
//! Where Windows differs:
//! - A file's identity is its volume serial number and file index (Unix: device and inode).
//! - A folder is flushed by opening it with backup semantics and flushing that handle.
//! - Renaming a temporary file over a target and removing a file are retried briefly when another
//!   program (a virus scanner, the search indexer, a sync client) has the file open
//!   ([`retry_transient`]).
//! - The writer lock is mandatory, so a refused writer cannot read who holds it
//!   ([`HOLDER_SHOWN`]), and the operating system may release a killed process's lock a moment
//!   after it ends.

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
    /// The handle cannot be duplicated or its identity cannot be read.
    pub fn identity(&self) -> io::Result<FileIdentity> {
        Ok(FileIdentity(same_file::Handle::from_file(self.file.try_clone()?)?))
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

/// Which file a path or handle refers to, to notice a file replaced behind the writer's back:
/// device and inode on Unix, volume serial number and file index on Windows (through
/// `same-file`, without `unsafe`).
///
/// # Semantics
/// An identity keeps its file open. Windows only guarantees a file index while a handle to the
/// file is open, so identities are compared while both are held; a deleted file's index cannot
/// be reused by a new file while its identity lives.
#[derive(Debug, PartialEq, Eq)]
pub struct FileIdentity(same_file::Handle);

impl FileIdentity {
    /// The identity of the file now at `path`.
    ///
    /// # Errors
    /// Nothing is at `path`, or it cannot be opened to read its identity.
    pub fn at(path: &Path) -> io::Result<Self> {
        #[cfg(windows)]
        {
            // Open for attributes only: a handle with read access to a file just written makes
            // Windows' malware scanner read the whole file when it closes (measured 24 ms on a
            // 16 MB project, more on larger ones, on every save).
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_READ_ATTRIBUTES: u32 = 0x80;
            const FILE_SHARE_ALL: u32 = 0x1 | 0x2 | 0x4;
            const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
            let file = OpenOptions::new()
                .access_mode(FILE_READ_ATTRIBUTES)
                .share_mode(FILE_SHARE_ALL)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
                .open(path)?;
            Ok(Self(same_file::Handle::from_file(file)?))
        }
        #[cfg(not(windows))]
        Ok(Self(same_file::Handle::from_path(path)?))
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
        /// What the holder recorded about itself, when it can be read; always `None` where the
        /// record cannot be shown ([`HOLDER_SHOWN`]).
        holder: Option<String>,
    },
}

/// Whether a refused writer is told what the lock's holder recorded about itself. Not on
/// Windows: there the lock (`LockFileEx`) is mandatory and stops every other handle reading the
/// lock file, and no unlocked copy of the record is kept beside it.
pub const HOLDER_SHOWN: bool = cfg!(not(windows));

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
                // Best effort where it is shown at all.
                let holder = HOLDER_SHOWN.then(|| file.read_to_string(&mut s).ok().map(|_| s.trim().to_string())).flatten().filter(|s| !s.is_empty());
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

/// Rename `from` over `to`, replacing it (atomic within one file system). Retried briefly on
/// Windows while another program has either file open ([`retry_transient`]).
///
/// # Errors
/// The rename failed; `to` is unchanged.
pub fn replace(from: &Path, to: &Path) -> io::Result<()> {
    retry_transient(|| std::fs::rename(from, to))
}

/// Remove a file, retried briefly on Windows while another program has it open
/// ([`retry_transient`]).
///
/// # Errors
/// The file could not be removed (`NotFound` if nothing is there).
pub fn remove_file(path: &Path) -> io::Result<()> {
    retry_transient(|| std::fs::remove_file(path))
}

/// How many times [`retry_transient`] tries an operation on Windows.
pub const TRANSIENT_ATTEMPTS: u32 = 5;

/// The pause between tries in [`retry_transient`] (so at most about 0.4 s is spent waiting).
pub const TRANSIENT_PAUSE: std::time::Duration = std::time::Duration::from_millis(100);

/// Run `op`, and on Windows try it again (up to [`TRANSIENT_ATTEMPTS`] times in all, with
/// [`TRANSIENT_PAUSE`] between tries) while it fails because another program briefly has the file
/// open: a virus scanner, the search indexer or a sync client. Elsewhere `op` runs once.
///
/// # Errors
/// The last failure of `op`.
pub fn retry_transient<T>(mut op: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let mut tries = 1;
    loop {
        match op() {
            Err(e) if is_transient(&e) && tries < TRANSIENT_ATTEMPTS => {
                tries += 1;
                std::thread::sleep(TRANSIENT_PAUSE);
            }
            r => return r,
        }
    }
}

/// Whether `e` is a failure that another program holding a file open can cause for a moment.
/// On Windows: access denied (also given while a deletion is pending), a sharing violation or a
/// lock violation. Never elsewhere, where opening a file does not stop it being renamed or
/// removed.
pub fn is_transient(e: &io::Error) -> bool {
    // ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION.
    cfg!(windows) && matches!(e.raw_os_error(), Some(5 | 32 | 33))
}

/// Make a folder's entries (a rename or a new file) durable: on Unix the folder is opened and
/// flushed; on Windows it is opened with backup semantics, for writing (which flushing needs),
/// and flushed. Elsewhere it is not available and fails with [`io::ErrorKind::Unsupported`], so
/// that nothing is reported durable without a flush.
///
/// # Errors
/// The folder could not be opened or flushed, or flushing a folder is not available.
pub fn sync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(dir)?.sync_all()
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_FLAG_BACKUP_SEMANTICS: without it a folder cannot be opened.
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        OpenOptions::new().read(true).write(true).custom_flags(FILE_FLAG_BACKUP_SEMANTICS).open(dir)?.sync_all()
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = dir;
        Err(folder_flush_unavailable())
    }
}

/// The failure [`sync_dir`] gives where flushing a folder is not available.
pub fn folder_flush_unavailable() -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, "flushing a folder is not available on this system")
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
        let _ = remove_file(path);
    }
    r
}

impl FileMedium {
    /// The open file, giving up the medium (for a caller that hands the handle to another
    /// storage engine, such as redb's `FileBackend`).
    pub fn into_file(self) -> File {
        self.file
    }
}

/// Seconds since 1970 by this computer's clock (0 if the clock is before 1970). The one clock
/// read of ECAD's file layer: `ecad-file` stamps lock records with it.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// What a writer records in the lock file about itself: `writer`, this process's id and
/// `now_secs` (seconds since 1970, passed in) as a UTC date and time.
pub fn holder_record(writer: &str, now_secs: u64) -> String {
    format!("{writer}, process {}, since {}", std::process::id(), utc(now_secs))
}

/// `secs` since 1970 as a UTC date and time, `YYYY-MM-DD hh:mm:ss UTC`.
pub fn utc(secs: u64) -> String {
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Whether `a` and `b` name the same file or folder: their canonical paths when both exist,
/// otherwise their spelling ([`lexical`], either separator, case ignored on Windows and macOS).
/// Two paths that do not exist yet can match. This is the one path comparison: Save As, the
/// default projects folder and the release folder checks all use it.
pub fn same_path(a: &Path, b: &Path) -> bool {
    let (x, y) = compared(a, b);
    x == y
}

/// Whether `a` is `b` or lies inside it, compared as [`same_path`] compares.
pub fn same_or_below(a: &Path, b: &Path) -> bool {
    let (x, y) = compared(a, b);
    x.len() >= y.len() && x.iter().zip(&y).all(|(p, q)| p == q)
}

/// `path` with `.` dropped and `..` taken back, without touching the disk.
pub fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The parts of `a` and `b` to compare: of their canonical paths when both exist, else of their
/// [`lexical`] spelling.
fn compared(a: &Path, b: &Path) -> (Vec<String>, Vec<String>) {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => (parts(&x), parts(&y)),
        _ => (parts(&lexical(a)), parts(&lexical(b))),
    }
}

/// A path's parts, case-folded where the file system ignores case (Windows, macOS).
fn parts(path: &Path) -> Vec<String> {
    let fold = cfg!(any(target_os = "windows", target_os = "macos"));
    path.components()
        .map(|c| {
            let t = c.as_os_str().to_string_lossy().replace('\\', "/");
            if fold { t.to_lowercase() } else { t }
        })
        .collect()
}

/// Whether something is at `path`.
pub fn exists(path: &Path) -> bool {
    path.exists()
}

/// The first bytes of the file at `path`: up to `n`, fewer if the file is shorter or one read
/// returns fewer.
///
/// # Errors
/// The file cannot be opened or read.
pub fn read_head(path: &Path, n: usize) -> io::Result<Vec<u8>> {
    let mut head = vec![0u8; n];
    let got = File::open(path)?.read(&mut head)?;
    head.truncate(got);
    Ok(head)
}

/// The whole file at `path`, refused if it is larger than `cap` bytes.
///
/// # Errors
/// The file cannot be read, or it is larger than `cap`.
pub fn read_all(path: &Path, cap: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(cap.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > cap {
        return Err(io::Error::other(format!("the file is larger than {cap} bytes")));
    }
    Ok(bytes)
}

/// The length of the file at `path`, in bytes.
///
/// # Errors
/// Its metadata cannot be read.
pub fn len(path: &Path) -> io::Result<u64> {
    Ok(std::fs::metadata(path)?.len())
}
