//! The redb database of a version 2 file, behind ECAD's append-only API (EEP-0005 §3).
//!
//! The only write paths are those of [`Writer`]: append a `transactions` row directly after a
//! branch's last one, insert a chunk or asset that is not stored yet, and put or remove
//! `branches`, `versions` and `meta` rows (only the names `meta` allows). No code path updates or
//! deletes a `transactions` row. Every write of one save happens in one redb write transaction
//! with quick-repair on (EEP-0005 §5.7), committed durably before [`Writer::commit`] returns.

use std::ops::RangeInclusive;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use redb::{
    Builder, ConcurrencyMode, Database, ReadOnlyDatabase, ReadTransaction, ReadableDatabase, ReadableTable, ReadableTableMetadata, StorageBackend,
    TableDefinition, TableHandle, WriteTransaction,
};

use super::pack;
use super::schema::{
    ASSETS, BRANCHES, BranchId, BranchRecord, CHUNKS, META, META_NAMES, TRANSACTIONS, VERSIONS, VersionKind, VersionRecord, asset_hash, asset_value,
    key_seq, split_tx_value, tx_key, tx_value, version_key,
};
use crate::chain::Hash;

/// Why the database could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    /// The engine reported a failure (I/O, a damaged page, a lock).
    #[error("{0}")]
    Engine(String),
    /// Another process has the database open for writing.
    #[error("the file is open for writing in another process")]
    Busy,
    /// A write the append-only API refuses.
    #[error("refused: {0}")]
    Refused(String),
    /// A stored value that does not decode.
    #[error("damaged: {0}")]
    Damaged(String),
    /// An earlier write failed and the engine refuses this handle until it is reopened (done
    /// once, automatically).
    #[error("an earlier write to the file failed; the file was reopened")]
    PreviousIo,
    /// The database was opened read-only.
    #[error("the file was opened read-only")]
    ReadOnly,
}

fn engine(e: impl std::fmt::Display) -> StoreError {
    StoreError::Engine(e.to_string())
}

fn open_err(e: redb::DatabaseError) -> StoreError {
    match e {
        redb::DatabaseError::DatabaseAlreadyOpen => StoreError::Busy,
        e => engine(e),
    }
}

/// Every table, in schema order, by name.
pub const TABLES: [&str; 6] = ["meta", "branches", "transactions", "versions", "chunks", "assets"];

enum Db {
    Writable(Database),
    ReadOnly(ReadOnlyDatabase),
}

/// How a handle opens its database again (after an I/O error, redb refuses every further use of
/// the handle it had: the file is reopened, recovering to its last commit).
type Reopen = Box<dyn Fn() -> Result<Db, StoreError> + Send + Sync>;

/// An open version 2 database.
pub struct Store {
    /// `None` only while it is being reopened, or after a reopen failed.
    db: RwLock<Option<Db>>,
    reopen: Reopen,
    /// redb repaired the file while opening it (it was not closed cleanly).
    repaired: bool,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").field("writable", &self.writable()).field("repaired", &self.repaired).finish()
    }
}

/// The builder every handle uses: one writer, read-only openers sharing the file and following its
/// commits (`SingleWriter`); a repair callback that notes a repair.
fn builder(flag: &Arc<AtomicBool>) -> Builder {
    let mut b = Database::builder();
    b.set_concurrency_mode(ConcurrencyMode::SingleWriter);
    let seen = flag.clone();
    b.set_repair_callback(move |_| seen.store(true, Ordering::SeqCst));
    b
}

fn poisoned() -> StoreError {
    StoreError::Engine("the database handle's lock is poisoned".into())
}

/// A transaction could not start: redb's "an earlier I/O error" is told apart, so the handle can
/// be reopened.
fn begin_err(e: redb::TransactionError) -> StoreError {
    match e {
        redb::TransactionError::Storage(redb::StorageError::PreviousIo) => StoreError::PreviousIo,
        e => engine(e),
    }
}

impl Store {
    fn new(db: Db, reopen: Reopen, repaired: bool) -> Self {
        Self { db: RwLock::new(Some(db)), reopen, repaired }
    }

    /// Create a new, empty database at `path`, with create-new semantics: an existing file is
    /// never replaced.
    ///
    /// # Errors
    /// [`StoreError::Refused`] when `path` exists; the file could not be created (it is then
    /// removed).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn create(path: &std::path::Path) -> Result<Self, StoreError> {
        let file = std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                StoreError::Refused(format!("{} already exists; it is never overwritten", path.display()))
            } else {
                engine(e)
            }
        })?;
        // The file is ours from here: a failure removes it.
        let made = (|| {
            let flag = Arc::new(AtomicBool::new(false));
            let backend = redb::backends::FileBackend::new(file).map_err(engine)?;
            let db = builder(&flag).create_with_backend(backend).map_err(open_err)?;
            let me = Self::new(Db::Writable(db), Self::reopen_path(path, true), false);
            me.init()?;
            Ok(me)
        })();
        if made.is_err() {
            let _ = std::fs::remove_file(path);
        }
        made
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn reopen_path(path: &std::path::Path, writable: bool) -> Reopen {
        let path = path.to_path_buf();
        Box::new(move || {
            let b = builder(&Arc::new(AtomicBool::new(false)));
            Ok(if writable { Db::Writable(b.open(&path).map_err(open_err)?) } else { Db::ReadOnly(b.open_read_only(&path).map_err(open_err)?) })
        })
    }

    /// Open the database at `path`, for writing or read-only.
    ///
    /// # Errors
    /// The file could not be opened, is not a redb database, or [`StoreError::Busy`].
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open(path: &std::path::Path, writable: bool) -> Result<Self, StoreError> {
        let flag = Arc::new(AtomicBool::new(false));
        let b = builder(&flag);
        let db = if writable { Db::Writable(b.open(path).map_err(open_err)?) } else { Db::ReadOnly(b.open_read_only(path).map_err(open_err)?) };
        Ok(Self::new(db, Self::reopen_path(path, writable), flag.load(Ordering::SeqCst)))
    }

    /// Open (or create, when empty) a database over `backend`: memory, or a fault-injecting
    /// medium in tests. The backend is cloned to reopen it after an I/O error.
    ///
    /// # Errors
    /// As [`Store::open`].
    pub fn with_backend<B: StorageBackend + Clone>(backend: B) -> Result<Self, StoreError> {
        let flag = Arc::new(AtomicBool::new(false));
        let db = builder(&flag).create_with_backend(backend.clone()).map_err(open_err)?;
        let reopen: Reopen = Box::new(move || {
            let b = builder(&Arc::new(AtomicBool::new(false)));
            Ok(Db::Writable(b.create_with_backend(backend.clone()).map_err(open_err)?))
        });
        let me = Self::new(Db::Writable(db), reopen, flag.load(Ordering::SeqCst));
        me.init()?;
        Ok(me)
    }

    /// Open a database from its bytes, held in memory by redb's own in-memory backend (EEP-0005
    /// §11: what a wasm build reads). The bytes are moved into the backend once and the caller's
    /// buffer is released; nothing else keeps a copy. Writes go to the memory copy only, and
    /// cannot fail, so the handle is never reopened.
    ///
    /// # Errors
    /// The bytes are not a redb database.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, StoreError> {
        let backend = redb::backends::InMemoryBackend::new();
        backend.set_len(bytes.len() as u64).map_err(engine)?;
        backend.write(0, &bytes).map_err(engine)?;
        drop(bytes);
        // redb's default mode: `SingleWriter` coordinates processes through file locks, which a
        // buffer in memory has none of (and needs none: one handle owns it).
        let flag = Arc::new(AtomicBool::new(false));
        let mut b = Database::builder();
        let seen = flag.clone();
        b.set_repair_callback(move |_| seen.store(true, Ordering::SeqCst));
        let db = b.create_with_backend(backend).map_err(open_err)?;
        let reopen: Reopen = Box::new(|| Err(StoreError::Engine("a database held in memory cannot be reopened".into())));
        let me = Self::new(Db::Writable(db), reopen, flag.load(Ordering::SeqCst));
        me.init()?;
        Ok(me)
    }

    /// Run `f` on the database; after redb's "an earlier I/O error" (it refuses a handle once a
    /// write failed), reopen it once, recovering to its last commit, and run `f` again. Nothing
    /// acknowledged is lost: a commit that failed was never acknowledged.
    fn with_db<T>(&self, f: impl Fn(&Db) -> Result<T, StoreError>) -> Result<T, StoreError> {
        let first = {
            let g = self.db.read().map_err(|_| poisoned())?;
            match g.as_ref() {
                Some(db) => f(db),
                None => Err(StoreError::PreviousIo),
            }
        };
        match first {
            Err(StoreError::PreviousIo) => {
                {
                    let mut g = self.db.write().map_err(|_| poisoned())?;
                    // The failed handle goes first: the file allows one writer.
                    drop(g.take());
                    *g = Some((self.reopen)()?);
                }
                let g = self.db.read().map_err(|_| poisoned())?;
                match g.as_ref() {
                    Some(db) => f(db),
                    None => Err(StoreError::PreviousIo),
                }
            }
            other => other,
        }
    }

    /// Make sure every table exists (an empty database has none).
    fn init(&self) -> Result<(), StoreError> {
        self.with_db(|db| {
            let Db::Writable(db) = db else { return Ok(()) };
            let txn = db.begin_write().map_err(begin_err)?;
            txn.open_table(META).map_err(engine)?;
            txn.open_table(BRANCHES).map_err(engine)?;
            txn.open_table(TRANSACTIONS).map_err(engine)?;
            txn.open_table(VERSIONS).map_err(engine)?;
            txn.open_table(CHUNKS).map_err(engine)?;
            txn.open_table(ASSETS).map_err(engine)?;
            txn.commit().map_err(engine)
        })
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
    pub fn reader(&self) -> Result<Reader, StoreError> {
        self.with_db(|db| {
            let txn = match db {
                Db::Writable(db) => db.begin_read(),
                Db::ReadOnly(db) => db.begin_read(),
            }
            .map_err(begin_err)?;
            Ok(Reader { txn })
        })
    }

    /// Start a save: one redb write transaction, with quick-repair.
    ///
    /// # Errors
    /// [`StoreError::ReadOnly`], or the engine could not start a write.
    pub fn writer(&self) -> Result<Writer, StoreError> {
        self.with_db(|db| {
            let Db::Writable(db) = db else { return Err(StoreError::ReadOnly) };
            let mut txn = db.begin_write().map_err(begin_err)?;
            txn.set_quick_repair(true);
            Ok(Writer { txn })
        })
    }

    /// Engine compaction (EEP-0005 §9.1): returns free pages; drops nothing ECAD stored.
    ///
    /// # Errors
    /// [`StoreError::ReadOnly`], or the engine failed.
    pub fn compact(&mut self) -> Result<bool, StoreError> {
        let g = self.db.get_mut().map_err(|_| poisoned())?;
        let Some(Db::Writable(db)) = g.as_mut() else { return Err(StoreError::ReadOnly) };
        db.compact().map_err(engine)
    }

    /// redb's own integrity check (reads every page; repairs the allocator state).
    ///
    /// # Errors
    /// [`StoreError::ReadOnly`], or the engine failed.
    pub fn check_integrity(&mut self) -> Result<bool, StoreError> {
        let g = self.db.get_mut().map_err(|_| poisoned())?;
        let Some(Db::Writable(db)) = g.as_mut() else { return Err(StoreError::ReadOnly) };
        db.check_integrity().map_err(engine)
    }
}

/// A table's row count and the bytes its rows take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableStat {
    pub name: &'static str,
    pub rows: u64,
    pub stored_bytes: u64,
    pub metadata_bytes: u64,
    pub fragmented_bytes: u64,
}

/// A consistent read view of the database.
pub struct Reader {
    txn: ReadTransaction,
}

fn table_stat<K: redb::Key + 'static, V: redb::Value + 'static>(txn: &ReadTransaction, def: TableDefinition<K, V>) -> Result<TableStat, StoreError> {
    let name = TABLES.into_iter().find(|n| *n == def.name()).unwrap_or("?");
    let t = txn.open_table(def).map_err(engine)?;
    let s = t.stats().map_err(engine)?;
    Ok(TableStat {
        name,
        rows: t.len().map_err(engine)?,
        stored_bytes: s.stored_bytes(),
        metadata_bytes: s.metadata_bytes(),
        fragmented_bytes: s.fragmented_bytes(),
    })
}

impl Reader {
    /// A `meta` value, as stored (JSON).
    ///
    /// # Errors
    /// The engine failed.
    pub fn meta(&self, name: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let t = self.txn.open_table(META).map_err(engine)?;
        Ok(t.get(name).map_err(engine)?.map(|v| v.value().to_vec()))
    }

    /// Every `meta` row.
    ///
    /// # Errors
    /// The engine failed.
    pub fn meta_all(&self) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
        let t = self.txn.open_table(META).map_err(engine)?;
        let mut out = Vec::new();
        for row in t.iter().map_err(engine)? {
            let (k, v) = row.map_err(engine)?;
            out.push((k.value().to_string(), v.value().to_vec()));
        }
        Ok(out)
    }

    /// Every branch.
    ///
    /// # Errors
    /// The engine failed, or a row does not decode.
    pub fn branches(&self) -> Result<Vec<(BranchId, BranchRecord)>, StoreError> {
        let t = self.txn.open_table(BRANCHES).map_err(engine)?;
        let mut out = Vec::new();
        for row in t.iter().map_err(engine)? {
            let (k, v) = row.map_err(engine)?;
            let rec: BranchRecord = serde_json::from_slice(v.value()).map_err(|e| StoreError::Damaged(format!("branch row: {e}")))?;
            out.push((BranchId(k.value()), rec));
        }
        Ok(out)
    }

    /// One branch.
    ///
    /// # Errors
    /// The engine failed, or the row does not decode.
    pub fn branch(&self, id: BranchId) -> Result<Option<BranchRecord>, StoreError> {
        let t = self.txn.open_table(BRANCHES).map_err(engine)?;
        let Some(v) = t.get(id.0).map_err(engine)? else { return Ok(None) };
        serde_json::from_slice(v.value()).map(Some).map_err(|e| StoreError::Damaged(format!("branch row: {e}")))
    }

    /// The stored `transactions` value of `seq` on `branch`: flag, chain value and body.
    ///
    /// # Errors
    /// The engine failed, or the row is too short.
    pub fn tx(&self, branch: BranchId, seq: u64) -> Result<Option<(u8, Hash, Vec<u8>)>, StoreError> {
        let t = self.txn.open_table(TRANSACTIONS).map_err(engine)?;
        let Some(v) = t.get(tx_key(branch, seq)).map_err(engine)? else { return Ok(None) };
        let (flag, chain, body) = split_tx_value(v.value()).map_err(StoreError::Damaged)?;
        Ok(Some((flag, chain, body.to_vec())))
    }

    /// The stored rows of `seqs` on `branch`, in order (a range read).
    ///
    /// # Errors
    /// As [`Reader::tx`].
    pub fn tx_range(&self, branch: BranchId, seqs: RangeInclusive<u64>) -> Result<Vec<(u64, u8, Hash, Vec<u8>)>, StoreError> {
        let t = self.txn.open_table(TRANSACTIONS).map_err(engine)?;
        let mut out = Vec::new();
        if seqs.is_empty() {
            return Ok(out);
        }
        for row in t.range(tx_key(branch, *seqs.start())..=tx_key(branch, *seqs.end())).map_err(engine)? {
            let (k, v) = row.map_err(engine)?;
            let (flag, chain, body) = split_tx_value(v.value()).map_err(StoreError::Damaged)?;
            out.push((key_seq(&k.value()), flag, chain, body.to_vec()));
        }
        Ok(out)
    }

    /// The highest `seq` stored for `branch`.
    ///
    /// # Errors
    /// The engine failed.
    pub fn last_seq(&self, branch: BranchId) -> Result<Option<u64>, StoreError> {
        let t = self.txn.open_table(TRANSACTIONS).map_err(engine)?;
        let mut r = t.range(tx_key(branch, 0)..=tx_key(branch, u64::MAX)).map_err(engine)?;
        match r.next_back() {
            Some(row) => Ok(Some(key_seq(&row.map_err(engine)?.0.value()))),
            None => Ok(None),
        }
    }

    /// The number of rows stored for `branch`.
    ///
    /// # Errors
    /// The engine failed.
    pub fn tx_count(&self, branch: BranchId) -> Result<u64, StoreError> {
        let t = self.txn.open_table(TRANSACTIONS).map_err(engine)?;
        let r = t.range(tx_key(branch, 0)..=tx_key(branch, u64::MAX)).map_err(engine)?;
        Ok(r.count() as u64)
    }

    /// A version row.
    ///
    /// # Errors
    /// The engine failed, or the row does not decode.
    pub fn version(&self, branch: BranchId, kind: VersionKind, name: &str) -> Result<Option<VersionRecord>, StoreError> {
        let t = self.txn.open_table(VERSIONS).map_err(engine)?;
        let Some(v) = t.get(version_key(branch, kind, name).as_slice()).map_err(engine)? else { return Ok(None) };
        serde_json::from_slice(v.value()).map(Some).map_err(|e| StoreError::Damaged(format!("version row: {e}")))
    }

    /// The version rows of `kind` on `branch`, in key order (for automatic snapshots, `seq`
    /// order).
    ///
    /// # Errors
    /// As [`Reader::version`].
    pub fn versions(&self, branch: BranchId, kind: VersionKind) -> Result<Vec<VersionRecord>, StoreError> {
        let t = self.txn.open_table(VERSIONS).map_err(engine)?;
        let lo = version_key(branch, kind, "");
        let mut hi = branch.0.to_vec();
        hi.push(kind.byte() + 1);
        let mut out = Vec::new();
        for row in t.range(lo.as_slice()..hi.as_slice()).map_err(engine)? {
            let (_, v) = row.map_err(engine)?;
            out.push(serde_json::from_slice(v.value()).map_err(|e| StoreError::Damaged(format!("version row: {e}")))?);
        }
        Ok(out)
    }

    /// The automatic snapshot with the highest `seq` at or before `seq` on `branch`.
    ///
    /// # Errors
    /// As [`Reader::version`].
    pub fn auto_at_or_before(&self, branch: BranchId, seq: u64) -> Result<Option<VersionRecord>, StoreError> {
        let t = self.txn.open_table(VERSIONS).map_err(engine)?;
        let lo = version_key(branch, VersionKind::Auto, "");
        let hi = version_key(branch, VersionKind::Auto, &super::schema::auto_name(seq));
        let mut r = t.range(lo.as_slice()..=hi.as_slice()).map_err(engine)?;
        match r.next_back() {
            Some(row) => {
                let (_, v) = row.map_err(engine)?;
                serde_json::from_slice(v.value()).map(Some).map_err(|e| StoreError::Damaged(format!("version row: {e}")))
            }
            None => Ok(None),
        }
    }

    /// A stored chunk: flag and body.
    ///
    /// # Errors
    /// The engine failed, or the row is empty.
    pub fn chunk(&self, hash: &Hash) -> Result<Option<(u8, Vec<u8>)>, StoreError> {
        let t = self.txn.open_table(CHUNKS).map_err(engine)?;
        let Some(v) = t.get(*hash).map_err(engine)? else { return Ok(None) };
        let v = v.value();
        let flag = *v.first().ok_or_else(|| StoreError::Damaged("an empty chunk row".into()))?;
        Ok(Some((flag, v.get(1..).unwrap_or_default().to_vec())))
    }

    /// A chunk's uncompressed body, checked against its hash.
    ///
    /// # Errors
    /// Missing, damaged, or not matching its hash.
    pub fn chunk_body(&self, hash: &Hash) -> Result<Vec<u8>, StoreError> {
        let (flag, body) = self.chunk(hash)?.ok_or_else(|| StoreError::Damaged(format!("chunk {} is missing", crate::chain::hex(hash))))?;
        let body = pack::unpack(flag, &body).map_err(StoreError::Damaged)?;
        if super::schema::chunk_hash(&body) != *hash {
            return Err(StoreError::Damaged(format!("chunk {} does not match its hash", crate::chain::hex(hash))));
        }
        Ok(body)
    }

    /// Every chunk's hash, flag and stored length.
    ///
    /// # Errors
    /// The engine failed.
    pub fn chunk_list(&self) -> Result<Vec<(Hash, u8, u64)>, StoreError> {
        let t = self.txn.open_table(CHUNKS).map_err(engine)?;
        let mut out = Vec::new();
        for row in t.iter().map_err(engine)? {
            let (k, v) = row.map_err(engine)?;
            let v = v.value();
            out.push((k.value(), v.first().copied().unwrap_or(0), v.len().saturating_sub(1) as u64));
        }
        Ok(out)
    }

    /// An asset: media type and bytes.
    ///
    /// # Errors
    /// The engine failed, or the row does not decode.
    pub fn asset(&self, hash: &Hash) -> Result<Option<(String, Vec<u8>)>, StoreError> {
        let t = self.txn.open_table(ASSETS).map_err(engine)?;
        let Some(v) = t.get(*hash).map_err(engine)? else { return Ok(None) };
        let (media, bytes) = super::schema::split_asset_value(v.value()).map_err(StoreError::Damaged)?;
        Ok(Some((media, bytes.to_vec())))
    }

    /// Row counts and sizes of every table.
    ///
    /// # Errors
    /// The engine failed.
    pub fn stats(&self) -> Result<Vec<TableStat>, StoreError> {
        Ok(vec![
            table_stat(&self.txn, META)?,
            table_stat(&self.txn, BRANCHES)?,
            table_stat(&self.txn, TRANSACTIONS)?,
            table_stat(&self.txn, VERSIONS)?,
            table_stat(&self.txn, CHUNKS)?,
            table_stat(&self.txn, ASSETS)?,
        ])
    }

    /// Every row of every table as (table, key, stored value), in schema order, for exports.
    ///
    /// # Errors
    /// The engine failed.
    pub fn raw_rows(&self, mut each: impl FnMut(&'static str, &[u8], &[u8]) -> Result<(), String>) -> Result<(), StoreError> {
        let err = StoreError::Engine;
        {
            let t = self.txn.open_table(META).map_err(engine)?;
            for row in t.iter().map_err(engine)? {
                let (k, v) = row.map_err(engine)?;
                each("meta", k.value().as_bytes(), v.value()).map_err(err)?;
            }
        }
        macro_rules! fixed {
            ($def:expr, $name:literal) => {{
                let t = self.txn.open_table($def).map_err(engine)?;
                for row in t.iter().map_err(engine)? {
                    let (k, v) = row.map_err(engine)?;
                    each($name, &k.value(), v.value()).map_err(err)?;
                }
            }};
        }
        fixed!(BRANCHES, "branches");
        fixed!(TRANSACTIONS, "transactions");
        {
            let t = self.txn.open_table(VERSIONS).map_err(engine)?;
            for row in t.iter().map_err(engine)? {
                let (k, v) = row.map_err(engine)?;
                each("versions", k.value(), v.value()).map_err(err)?;
            }
        }
        fixed!(CHUNKS, "chunks");
        fixed!(ASSETS, "assets");
        Ok(())
    }
}

/// One save: a redb write transaction behind the append-only API. Dropping it without
/// [`Writer::commit`] writes nothing.
pub struct Writer {
    txn: WriteTransaction,
}

impl Writer {
    /// Put a `meta` row. Only the names EEP-0005 §3.1 lists (and `link_v1`) are accepted.
    ///
    /// # Errors
    /// [`StoreError::Refused`] for another name; the engine failed.
    pub fn set_meta(&mut self, name: &str, value: &serde_json::Value) -> Result<(), StoreError> {
        if !META_NAMES.contains(&name) {
            return Err(StoreError::Refused(format!("meta name {name:?} is not part of the format")));
        }
        let bytes = serde_json::to_vec(value).map_err(engine)?;
        let mut t = self.txn.open_table(META).map_err(engine)?;
        t.insert(name, bytes.as_slice()).map_err(engine)?;
        Ok(())
    }

    /// A branch row, as this transaction sees it.
    ///
    /// # Errors
    /// The engine failed, or the row does not decode.
    pub fn branch(&self, id: BranchId) -> Result<Option<BranchRecord>, StoreError> {
        let t = self.txn.open_table(BRANCHES).map_err(engine)?;
        let Some(v) = t.get(id.0).map_err(engine)? else { return Ok(None) };
        serde_json::from_slice(v.value()).map(Some).map_err(|e| StoreError::Damaged(format!("branch row: {e}")))
    }

    /// Put a branch row (a new branch, or its new head or status).
    ///
    /// # Errors
    /// The engine failed.
    pub fn put_branch(&mut self, id: BranchId, rec: &BranchRecord) -> Result<(), StoreError> {
        let bytes = serde_json::to_vec(rec).map_err(engine)?;
        let mut t = self.txn.open_table(BRANCHES).map_err(engine)?;
        t.insert(id.0, bytes.as_slice()).map_err(engine)?;
        Ok(())
    }

    /// Append transaction `seq` to `branch`: its chain value and its uncompressed record.
    ///
    /// # Errors
    /// [`StoreError::Refused`] unless `seq` comes directly after the branch's last stored row
    /// (append-only: a stored row is never replaced); the engine failed.
    pub fn append_tx(&mut self, branch: BranchId, seq: u64, chain: &Hash, record: &[u8]) -> Result<u64, StoreError> {
        let mut t = self.txn.open_table(TRANSACTIONS).map_err(engine)?;
        let last = {
            let mut r = t.range(tx_key(branch, 0)..=tx_key(branch, u64::MAX)).map_err(engine)?;
            match r.next_back() {
                Some(row) => key_seq(&row.map_err(engine)?.0.value()),
                None => 0,
            }
        };
        if last.checked_add(1) != Some(seq) {
            return Err(StoreError::Refused(format!("transaction {seq} does not follow the branch's last stored transaction {last}")));
        }
        let (flag, body) = pack::pack(record);
        let value = tx_value(flag, chain, &body);
        t.insert(tx_key(branch, seq), value.as_slice()).map_err(engine)?;
        Ok(value.len() as u64)
    }

    /// Insert a chunk unless one with that hash is stored. Returns the bytes written (0 when it
    /// was already stored).
    ///
    /// # Errors
    /// [`StoreError::Refused`] when `hash` is not the hash of `body`; the engine failed.
    pub fn put_chunk(&mut self, hash: &Hash, body: &[u8]) -> Result<u64, StoreError> {
        if super::schema::chunk_hash(body) != *hash {
            return Err(StoreError::Refused("a chunk's hash does not match its body".into()));
        }
        let mut t = self.txn.open_table(CHUNKS).map_err(engine)?;
        if t.get(*hash).map_err(engine)?.is_some() {
            return Ok(0);
        }
        let (flag, packed) = pack::pack(body);
        let mut value = Vec::with_capacity(1 + packed.len());
        value.push(flag);
        value.extend_from_slice(&packed);
        t.insert(*hash, value.as_slice()).map_err(engine)?;
        Ok(value.len() as u64)
    }

    /// Insert an asset unless it is stored (EEP-0005 §3.6: stored as it is). Returns its hash.
    ///
    /// # Errors
    /// A media type over 255 bytes; the engine failed.
    pub fn put_asset(&mut self, media: &str, bytes: &[u8]) -> Result<Hash, StoreError> {
        let hash = asset_hash(bytes);
        let mut t = self.txn.open_table(ASSETS).map_err(engine)?;
        if t.get(hash).map_err(engine)?.is_none() {
            let value = asset_value(media, bytes).map_err(StoreError::Refused)?;
            t.insert(hash, value.as_slice()).map_err(engine)?;
        }
        Ok(hash)
    }

    /// Put a version row.
    ///
    /// # Errors
    /// The engine failed.
    pub fn put_version(&mut self, branch: BranchId, rec: &VersionRecord) -> Result<(), StoreError> {
        let bytes = serde_json::to_vec(rec).map_err(engine)?;
        let mut t = self.txn.open_table(VERSIONS).map_err(engine)?;
        t.insert(version_key(branch, rec.kind, &rec.name).as_slice(), bytes.as_slice()).map_err(engine)?;
        Ok(())
    }

    /// Set the description of a `versions` row this write transaction holds (FQ5).
    ///
    /// # Errors
    /// No such row; the engine failed.
    pub fn describe_version(&mut self, branch: BranchId, kind: VersionKind, name: &str, description: &str) -> Result<(), StoreError> {
        let key = version_key(branch, kind, name);
        let rec = {
            let t = self.txn.open_table(VERSIONS).map_err(engine)?;
            let v = t.get(key.as_slice()).map_err(engine)?;
            match v {
                Some(v) => serde_json::from_slice::<VersionRecord>(v.value()).map_err(|e| StoreError::Damaged(e.to_string()))?,
                None => return Err(StoreError::Refused(format!("version {name:?} has no row to describe"))),
            }
        };
        self.put_version(branch, &VersionRecord { description: Some(description.to_string()), ..rec })
    }

    /// Remove a version row (a version unnamed by `UnnameVersion`, or a rescue dropping rows past
    /// the verified head). Its chunks stay until compaction.
    ///
    /// # Errors
    /// The engine failed.
    pub fn remove_version(&mut self, branch: BranchId, kind: VersionKind, name: &str) -> Result<(), StoreError> {
        let mut t = self.txn.open_table(VERSIONS).map_err(engine)?;
        t.remove(version_key(branch, kind, name).as_slice()).map_err(engine)?;
        Ok(())
    }

    /// Copy every row of `from` into this new, empty file, keeping the stored bytes: Save As.
    /// Each branch's transactions are copied in `seq` order and must have no gaps; with `cut`,
    /// that branch's rows after that `seq` are left out (a rescue of what verified). Returns the
    /// bytes copied.
    ///
    /// # Errors
    /// [`StoreError::Refused`] when this file is not empty or a branch's rows have a gap; the
    /// engine failed.
    pub fn copy_all(&mut self, from: &Reader, cut: Option<(BranchId, u64)>) -> Result<u64, StoreError> {
        {
            let t = self.txn.open_table(TRANSACTIONS).map_err(engine)?;
            if !t.is_empty().map_err(engine)? {
                return Err(StoreError::Refused("Save As copies only into an empty file".into()));
            }
        }
        let mut bytes = 0u64;
        let mut last: Option<([u8; 16], u64)> = None;
        let mut meta = self.txn.open_table(META).map_err(engine)?;
        let mut branches = self.txn.open_table(BRANCHES).map_err(engine)?;
        let mut txs = self.txn.open_table(TRANSACTIONS).map_err(engine)?;
        let mut versions = self.txn.open_table(VERSIONS).map_err(engine)?;
        let mut chunks = self.txn.open_table(CHUNKS).map_err(engine)?;
        let mut assets = self.txn.open_table(ASSETS).map_err(engine)?;
        let fixed = |k: &[u8], n: usize| -> Result<Vec<u8>, String> {
            if k.len() == n { Ok(k.to_vec()) } else { Err(format!("a key of {} bytes where {n} were expected", k.len())) }
        };
        from.raw_rows(|table, k, v| {
            bytes += (k.len() + v.len()) as u64;
            let e = |e: redb::StorageError| e.to_string();
            match table {
                "meta" => {
                    let name = std::str::from_utf8(k).map_err(|e| e.to_string())?;
                    meta.insert(name, v).map_err(e)?;
                }
                "branches" => {
                    let mut key = [0u8; 16];
                    key.copy_from_slice(&fixed(k, 16)?);
                    branches.insert(key, v).map_err(e)?;
                }
                "transactions" => {
                    let mut key = [0u8; 24];
                    key.copy_from_slice(&fixed(k, 24)?);
                    let mut b = [0u8; 16];
                    b.copy_from_slice(key.get(..16).unwrap_or(&[0u8; 16]));
                    let seq = key_seq(&key);
                    let expected = match last {
                        Some((lb, ls)) if lb == b => ls.saturating_add(1),
                        _ => 1,
                    };
                    if cut.is_some_and(|(cb, cs)| cb.0 == b && seq > cs) {
                        // A rescue copies only what verified.
                        return Ok(());
                    }
                    if seq != expected {
                        return Err(format!("transaction {seq} follows {} on its branch", expected - 1));
                    }
                    last = Some((b, seq));
                    txs.insert(key, v).map_err(e)?;
                }
                "versions" => {
                    versions.insert(k, v).map_err(e)?;
                }
                "chunks" => {
                    let mut key = [0u8; 32];
                    key.copy_from_slice(&fixed(k, 32)?);
                    chunks.insert(key, v).map_err(e)?;
                }
                "assets" => {
                    let mut key = [0u8; 32];
                    key.copy_from_slice(&fixed(k, 32)?);
                    assets.insert(key, v).map_err(e)?;
                }
                other => return Err(format!("unknown table {other}")),
            }
            Ok(())
        })
        .map_err(|e| match e {
            StoreError::Engine(m) => StoreError::Refused(m),
            e => e,
        })?;
        Ok(bytes)
    }

    /// Commit durably: after a crash, all of this save is in the file or none of it.
    ///
    /// # Errors
    /// The engine failed; nothing of this save is in the file.
    pub fn commit(self) -> Result<(), StoreError> {
        self.txn.commit().map_err(engine)
    }
}

/// The table names in schema order, with a short description each (for inspection).
pub fn table_descriptions() -> [(&'static str, &'static str); 6] {
    [
        ("meta", "container facts"),
        ("branches", "branch ID -> name, status, parent, fork point, head"),
        ("transactions", "(branch, seq) -> flag, chain value, record"),
        ("versions", "(branch, kind, name) -> point, chunks, release"),
        ("chunks", "chunk hash -> flag, body"),
        ("assets", "asset hash -> media type, bytes"),
    ]
}
