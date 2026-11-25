//! [`Database`]: a redb database as generic storage, the handle a file format builds on.
//!
//! - **Open**: a new file with create-new semantics, an existing file for writing or read-only,
//!   any storage backend (memory, or a fault-injecting one in tests), or a copy of a database's
//!   bytes held in memory.
//! - **Transactions**: a consistent read view, and one write transaction with quick-repair on.
//! - **The handle**: after an I/O error redb refuses every further use of a handle; the next read
//!   or write reopens the database once, recovering to its last commit. Nothing acknowledged is
//!   lost: a commit that failed was never acknowledged.
//!
//! Excluded: what the tables hold and which exist (the file format, `ecad-file`, opens them), and
//! every rule about what may be written.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use redb::{Builder, ConcurrencyMode, ReadOnlyDatabase, ReadTransaction, ReadableDatabase, StorageBackend, WriteTransaction};

/// Why the database could not be opened, read or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DbError {
    /// The engine reported a failure (I/O, a damaged page, a lock).
    #[error("{0}")]
    Engine(String),
    /// Another process has the database open for writing.
    #[error("the file is open for writing in another process")]
    Busy,
    /// A new database was asked for where a file already exists; it is never overwritten.
    #[error("{0} already exists; it is never overwritten")]
    Exists(String),
    /// An earlier write failed and the engine refuses this handle until it is reopened (done
    /// once, automatically).
    #[error("an earlier write to the file failed; the file was reopened")]
    PreviousIo,
    /// The database was opened read-only.
    #[error("the file was opened read-only")]
    ReadOnly,
}

/// An engine failure, in words.
pub fn engine(e: impl std::fmt::Display) -> DbError {
    DbError::Engine(e.to_string())
}

fn open_err(e: redb::DatabaseError) -> DbError {
    match e {
        redb::DatabaseError::DatabaseAlreadyOpen => DbError::Busy,
        e => engine(e),
    }
}

/// A transaction could not start: redb's "an earlier I/O error" is told apart, so the handle can
/// be reopened.
fn begin_err(e: redb::TransactionError) -> DbError {
    match e {
        redb::TransactionError::Storage(redb::StorageError::PreviousIo) => DbError::PreviousIo,
        e => engine(e),
    }
}

fn poisoned() -> DbError {
    DbError::Engine("the database handle's lock is poisoned".into())
}

enum Db {
    Writable(redb::Database),
    /// A file opened read-only. Only [`Database::open`] makes one.
    ReadOnly(ReadOnlyDatabase),
}

/// How a handle opens its database again after an I/O error.
type Reopen = Box<dyn Fn() -> Result<Db, DbError> + Send + Sync>;

/// The builder every handle uses: one writer, read-only openers sharing the file and following its
/// commits (`SingleWriter`); a repair callback that notes a repair.
fn builder(flag: &Arc<AtomicBool>) -> Builder {
    let mut b = redb::Database::builder();
    b.set_concurrency_mode(ConcurrencyMode::SingleWriter);
    let seen = flag.clone();
    b.set_repair_callback(move |_| seen.store(true, Ordering::SeqCst));
    b
}

/// redb's read-only open (`Builder::open_read_only`, which opens the file for reading and hands it
/// to a `FileBackend`), on a file this crate opened.
fn open_read_only(b: &Builder, path: &std::path::Path) -> Result<ReadOnlyDatabase, redb::DatabaseError> {
    let file = crate::disk::open_read_only(path)?;
    b.open_read_only_with_backend(redb::backends::FileBackend::new(file)?)
}

/// An open redb database.
pub struct Database {
    /// `None` only while it is being reopened, or after a reopen failed.
    db: RwLock<Option<Db>>,
    reopen: Reopen,
    /// redb repaired the file while opening it (it was not closed cleanly).
    repaired: bool,
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database").field("writable", &self.writable()).field("repaired", &self.repaired).finish()
    }
}

impl Database {
    fn new(db: Db, reopen: Reopen, repaired: bool) -> Self {
        Self { db: RwLock::new(Some(db)), reopen, repaired }
    }

    /// Create a new, empty database at `path`, with create-new semantics: an existing file is
    /// never replaced. A failure after the file was made removes it.
    ///
    /// # Errors
    /// [`DbError::Exists`] when `path` exists; the file could not be created.
    pub fn create(path: &std::path::Path) -> Result<Self, DbError> {
        let file = crate::disk::create_new(path)
            .map_err(|e| if e.kind() == std::io::ErrorKind::AlreadyExists { DbError::Exists(path.display().to_string()) } else { engine(e) })?;
        let made = (|| {
            let flag = Arc::new(AtomicBool::new(false));
            let backend = redb::backends::FileBackend::new(file).map_err(engine)?;
            let db = builder(&flag).create_with_backend(backend).map_err(open_err)?;
            Ok(Self::new(Db::Writable(db), Self::reopen_path(path, true), false))
        })();
        if made.is_err() {
            let _ = crate::disk::remove_file(path);
        }
        made
    }

    /// The writable open is redb's path-based `Builder::open`: redb 4.5 has no public
    /// open-existing over a backend, and `create_with_backend` would initialise an empty file as a
    /// new database where `open` refuses it.
    fn reopen_path(path: &std::path::Path, writable: bool) -> Reopen {
        let path = path.to_path_buf();
        Box::new(move || {
            let b = builder(&Arc::new(AtomicBool::new(false)));
            Ok(if writable { Db::Writable(b.open(&path).map_err(open_err)?) } else { Db::ReadOnly(open_read_only(&b, &path).map_err(open_err)?) })
        })
    }

    /// Open the database at `path`, for writing or read-only.
    ///
    /// # Errors
    /// The file could not be opened, is not a redb database, or [`DbError::Busy`].
    pub fn open(path: &std::path::Path, writable: bool) -> Result<Self, DbError> {
        let flag = Arc::new(AtomicBool::new(false));
        let b = builder(&flag);
        let db = if writable { Db::Writable(b.open(path).map_err(open_err)?) } else { Db::ReadOnly(open_read_only(&b, path).map_err(open_err)?) };
        Ok(Self::new(db, Self::reopen_path(path, writable), flag.load(Ordering::SeqCst)))
    }

    /// Open (or create, when empty) a database over `backend`: memory, or a fault-injecting
    /// backend in tests. The backend is cloned to reopen it after an I/O error.
    ///
    /// # Errors
    /// The bytes are not a redb database, or the engine failed.
    pub fn with_backend<B: StorageBackend + Clone>(backend: B) -> Result<Self, DbError> {
        let flag = Arc::new(AtomicBool::new(false));
        let db = builder(&flag).create_with_backend(backend.clone()).map_err(open_err)?;
        let reopen: Reopen = Box::new(move || {
            let b = builder(&Arc::new(AtomicBool::new(false)));
            Ok(Db::Writable(b.create_with_backend(backend.clone()).map_err(open_err)?))
        });
        Ok(Self::new(Db::Writable(db), reopen, flag.load(Ordering::SeqCst)))
    }

    /// Open a database from its bytes, held in memory by redb's own in-memory backend. The bytes
    /// are moved into the backend once and the caller's buffer is released; nothing else keeps a
    /// copy. Writes go to the memory copy only, and cannot fail, so the handle is never reopened.
    ///
    /// # Errors
    /// The bytes are not a redb database.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, DbError> {
        let backend = redb::backends::InMemoryBackend::new();
        backend.set_len(bytes.len() as u64).map_err(engine)?;
        backend.write(0, &bytes).map_err(engine)?;
        drop(bytes);
        // redb's default mode: `SingleWriter` coordinates processes through file locks, which a
        // buffer in memory has none of (and needs none: one handle owns it).
        let flag = Arc::new(AtomicBool::new(false));
        let mut b = redb::Database::builder();
        let seen = flag.clone();
        b.set_repair_callback(move |_| seen.store(true, Ordering::SeqCst));
        let db = b.create_with_backend(backend).map_err(open_err)?;
        let reopen: Reopen = Box::new(|| Err(DbError::Engine("a database held in memory cannot be reopened".into())));
        Ok(Self::new(Db::Writable(db), reopen, flag.load(Ordering::SeqCst)))
    }

    /// Run `f` on the database; after redb's "an earlier I/O error", reopen it once, recovering to
    /// its last commit, and run `f` again.
    fn with_db<T>(&self, f: impl Fn(&Db) -> Result<T, DbError>) -> Result<T, DbError> {
        let first = {
            let g = self.db.read().map_err(|_| poisoned())?;
            match g.as_ref() {
                Some(db) => f(db),
                None => Err(DbError::PreviousIo),
            }
        };
        match first {
            Err(DbError::PreviousIo) => {
                {
                    let mut g = self.db.write().map_err(|_| poisoned())?;
                    // The failed handle goes first: the file allows one writer.
                    drop(g.take());
                    *g = Some((self.reopen)()?);
                }
                let g = self.db.read().map_err(|_| poisoned())?;
                match g.as_ref() {
                    Some(db) => f(db),
                    None => Err(DbError::PreviousIo),
                }
            }
            other => other,
        }
    }

    /// Whether this handle may write.
    pub fn writable(&self) -> bool {
        self.db.read().is_ok_and(|g| matches!(g.as_ref(), Some(Db::Writable(_))))
    }

    /// Whether redb repaired the file while opening it.
    pub fn repaired(&self) -> bool {
        self.repaired
    }

    /// A consistent read view.
    ///
    /// # Errors
    /// The engine could not start a read.
    pub fn read(&self) -> Result<ReadTransaction, DbError> {
        self.with_db(|db| {
            match db {
                Db::Writable(db) => db.begin_read(),
                Db::ReadOnly(db) => db.begin_read(),
            }
            .map_err(begin_err)
        })
    }

    /// One write transaction, with quick-repair on: committed durably before its commit returns.
    ///
    /// # Errors
    /// [`DbError::ReadOnly`], or the engine could not start a write.
    pub fn write(&self) -> Result<WriteTransaction, DbError> {
        self.with_db(|db| {
            let Db::Writable(db) = db else { return Err(DbError::ReadOnly) };
            let mut txn = db.begin_write().map_err(begin_err)?;
            txn.set_quick_repair(true);
            Ok(txn)
        })
    }

    /// Engine compaction: returns free pages; drops nothing stored.
    ///
    /// # Errors
    /// [`DbError::ReadOnly`], or the engine failed.
    pub fn compact(&mut self) -> Result<bool, DbError> {
        let g = self.db.get_mut().map_err(|_| poisoned())?;
        let Some(Db::Writable(db)) = g.as_mut() else { return Err(DbError::ReadOnly) };
        db.compact().map_err(engine)
    }

    /// redb's own integrity check (reads every page; repairs the allocator state).
    ///
    /// # Errors
    /// [`DbError::ReadOnly`], or the engine failed.
    pub fn check_integrity(&mut self) -> Result<bool, DbError> {
        let g = self.db.get_mut().map_err(|_| poisoned())?;
        let Some(Db::Writable(db)) = g.as_mut() else { return Err(DbError::ReadOnly) };
        db.check_integrity().map_err(engine)
    }
}
