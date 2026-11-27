//! The project database behind ECAD's append-only API. Opening, transactions and
//! reopening after an I/O error are ecad-store's generic redb handle ([`ecad_store::db`]); this
//! module adds the project tables and the rules about what may be written.
//!
//! The only write paths are those of [`Writer`]: append a `transactions` row directly after a
//! branch's last one, insert a chunk or asset that is not stored yet, and put or remove
//! `branches`, `versions` and `meta` rows (only the names `meta` allows). No code path updates or
//! deletes a `transactions` row. Every write of one save happens in one redb write transaction
//! with quick-repair on, committed durably before [`Writer::commit`] returns.
//!
//! Every read and write runs under the database's halt flag ([`ecad_store::Halt`]): a damaged page that makes
//! redb panic is [`StoreError::Halted`], and the database is halted, so every later call returns that error
//! without reading the file again.

use std::ops::RangeInclusive;

use ecad_store::Halt;
use ecad_store::db::{Database, DbError};
use redb::{ReadTransaction, ReadableTable, ReadableTableMetadata, StorageBackend, TableDefinition, TableHandle, WriteTransaction};

use super::pack;
use super::schema::{
    ASSETS, BRANCHES, BranchId, BranchRecord, CHUNKS, META, META_NAMES, RowKind, TRANSACTIONS, VERSIONS, VersionRecord, asset_hash, asset_value,
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
    /// A damaged page stopped the storage engine (redb panicked on it), now or earlier through this database. The
    /// database is halted: nothing more is read or written through it.
    #[error("the file is damaged and cannot be read: {0}")]
    Halted(String),
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

/// Every table, in schema order, by name.
pub const TABLES: [&str; 6] = ["meta", "branches", "transactions", "versions", "chunks", "assets"];

/// An open project database: ecad-store's generic redb handle ([`ecad_store::db::Database`]),
/// with the project tables and ECAD's append-only API on top.
pub struct Store {
    db: Database,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").field("writable", &self.writable()).field("repaired", &self.repaired()).finish()
    }
}

impl From<DbError> for StoreError {
    fn from(e: DbError) -> Self {
        match e {
            DbError::Engine(e) => StoreError::Engine(e),
            DbError::Busy => StoreError::Busy,
            e @ DbError::Exists(_) => StoreError::Refused(e.to_string()),
            DbError::PreviousIo => StoreError::PreviousIo,
            DbError::ReadOnly => StoreError::ReadOnly,
            DbError::Halted(e) => StoreError::Halted(e),
        }
    }
}

impl From<ecad_store::Halted> for StoreError {
    fn from(h: ecad_store::Halted) -> Self {
        StoreError::Halted(h.0)
    }
}

impl Store {
    /// Create a new, empty project database at `path`, with create-new semantics: an existing
    /// file is never replaced.
    ///
    /// # Errors
    /// [`StoreError::Refused`] when `path` exists; the file could not be created (it is then
    /// removed).
    pub fn create(path: &std::path::Path) -> Result<Self, StoreError> {
        let me = Self { db: Database::create(path)? };
        // The file is ours from here: a failure removes it.
        if let Err(e) = me.init() {
            drop(me);
            let _ = ecad_store::disk::remove_file(path);
            return Err(e);
        }
        Ok(me)
    }

    /// Open the project database at `path`, for writing or read-only.
    ///
    /// # Errors
    /// The file could not be opened, is not a redb database, or [`StoreError::Busy`].
    pub fn open(path: &std::path::Path, writable: bool) -> Result<Self, StoreError> {
        Ok(Self { db: Database::open(path, writable)? })
    }

    /// Open (or create, when empty) a project database over `backend`: memory, or a
    /// fault-injecting backend in tests. The backend is cloned to reopen it after an I/O error.
    ///
    /// # Errors
    /// As [`Store::open`].
    pub fn with_backend<B: StorageBackend + Clone>(backend: B) -> Result<Self, StoreError> {
        let me = Self { db: Database::with_backend(backend)? };
        me.init()?;
        Ok(me)
    }

    /// Open a project database over `backend` read-only; see [`Database::with_backend_read_only`].
    ///
    /// # Errors
    /// As [`Store::open`].
    pub fn with_backend_read_only<B: StorageBackend>(backend: B) -> Result<Self, StoreError> {
        Ok(Self { db: Database::with_backend_read_only(backend)? })
    }

    /// Open a project database from its bytes, held in memory; see
    /// [`Database::from_bytes`].
    ///
    /// # Errors
    /// The bytes are not a redb database.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, StoreError> {
        let me = Self { db: Database::from_bytes(bytes)? };
        me.init()?;
        Ok(me)
    }

    /// Make sure every table exists (an empty database has none).
    fn init(&self) -> Result<(), StoreError> {
        if !self.db.writable() {
            return Ok(());
        }
        let txn = self.db.write()?;
        txn.open_table(META).map_err(engine)?;
        txn.open_table(BRANCHES).map_err(engine)?;
        txn.open_table(TRANSACTIONS).map_err(engine)?;
        txn.open_table(VERSIONS).map_err(engine)?;
        txn.open_table(CHUNKS).map_err(engine)?;
        txn.open_table(ASSETS).map_err(engine)?;
        txn.commit().map_err(engine)
    }

    /// Whether this handle may write.
    pub fn writable(&self) -> bool {
        self.db.writable()
    }

    /// Whether redb repaired the file while opening it.
    pub fn repaired(&self) -> bool {
        self.db.repaired()
    }

    /// The error every call returns once a damaged page stopped the storage engine ([`StoreError::Halted`]).
    pub fn halted(&self) -> Option<StoreError> {
        self.db.halt().halted().map(|m| StoreError::Halted(m.to_string()))
    }

    /// A consistent read view.
    ///
    /// # Errors
    /// The engine could not start a read.
    pub fn reader(&self) -> Result<Reader, StoreError> {
        Ok(Reader { txn: Some(self.db.read()?), halt: self.db.halt().clone() })
    }

    /// Start a save: one redb write transaction, with quick-repair.
    ///
    /// # Errors
    /// [`StoreError::ReadOnly`], or the engine could not start a write.
    pub fn writer(&self) -> Result<Writer, StoreError> {
        Ok(Writer { txn: Some(self.db.write()?), halt: self.db.halt().clone() })
    }

    /// Engine compaction: returns free pages; drops nothing ECAD stored.
    ///
    /// # Errors
    /// [`StoreError::ReadOnly`], or the engine failed.
    pub fn compact(&mut self) -> Result<bool, StoreError> {
        Ok(self.db.compact()?)
    }

    /// redb's own integrity check (reads every page; repairs the allocator state).
    ///
    /// # Errors
    /// [`StoreError::ReadOnly`], or the engine failed.
    pub fn check_integrity(&mut self) -> Result<bool, StoreError> {
        Ok(self.db.check_integrity()?)
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

/// A consistent read view of the database. Every read runs under the database's halt flag: a damaged page that
/// makes redb panic returns [`StoreError::Halted`] and halts the database.
pub struct Reader {
    /// `None` only while it is dropped.
    txn: Option<ReadTransaction>,
    halt: Halt,
}

impl Drop for Reader {
    fn drop(&mut self) {
        self.halt.drop_guarded(self.txn.take());
    }
}

/// The transaction a view holds; `None` only while it is dropped or committed.
fn live<T>(txn: &Option<T>) -> Result<&T, StoreError> {
    txn.as_ref().ok_or_else(|| StoreError::Engine("the transaction has ended".into()))
}

/// Run `f`, which reads or writes through redb, under `halt` ([`Halt::run`]): a panic inside redb, now or earlier
/// on the same database, is [`StoreError::Halted`].
fn guard<T>(halt: &Halt, f: impl FnOnce() -> Result<T, StoreError>) -> Result<T, StoreError> {
    halt.run(f)?
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
        let halt = self.halt.clone();
        guard(&halt, || {
            let t = live(&self.txn)?.open_table(META).map_err(engine)?;
            Ok(t.get(name).map_err(engine)?.map(|v| v.value().to_vec()))
        })
    }

    /// Every `meta` row.
    ///
    /// # Errors
    /// The engine failed.
    pub fn meta_all(&self) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let t = live(&self.txn)?.open_table(META).map_err(engine)?;
            let mut out = Vec::new();
            for row in t.iter().map_err(engine)? {
                let (k, v) = row.map_err(engine)?;
                out.push((k.value().to_string(), v.value().to_vec()));
            }
            Ok(out)
        })
    }

    /// Every branch.
    ///
    /// # Errors
    /// The engine failed, or a row does not decode.
    pub fn branches(&self) -> Result<Vec<(BranchId, BranchRecord)>, StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let t = live(&self.txn)?.open_table(BRANCHES).map_err(engine)?;
            let mut out = Vec::new();
            for row in t.iter().map_err(engine)? {
                let (k, v) = row.map_err(engine)?;
                let rec: BranchRecord = serde_json::from_slice(v.value()).map_err(|e| StoreError::Damaged(format!("branch row: {e}")))?;
                out.push((BranchId(k.value()), rec));
            }
            Ok(out)
        })
    }

    /// One branch.
    ///
    /// # Errors
    /// The engine failed, or the row does not decode.
    pub fn branch(&self, id: BranchId) -> Result<Option<BranchRecord>, StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let t = live(&self.txn)?.open_table(BRANCHES).map_err(engine)?;
            let Some(v) = t.get(id.0).map_err(engine)? else { return Ok(None) };
            serde_json::from_slice(v.value()).map(Some).map_err(|e| StoreError::Damaged(format!("branch row: {e}")))
        })
    }

    /// The stored `transactions` value of `seq` on `branch`: flag, chain value and body.
    ///
    /// # Errors
    /// The engine failed, or the row is too short.
    pub fn tx(&self, branch: BranchId, seq: u64) -> Result<Option<(u8, Hash, Vec<u8>)>, StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let t = live(&self.txn)?.open_table(TRANSACTIONS).map_err(engine)?;
            let Some(v) = t.get(tx_key(branch, seq)).map_err(engine)? else { return Ok(None) };
            let (flag, chain, body) = split_tx_value(v.value()).map_err(|e| StoreError::Damaged(e.to_string()))?;
            Ok(Some((flag, chain, body.to_vec())))
        })
    }

    /// The stored rows of `seqs` on `branch`, in order (a range read).
    ///
    /// # Errors
    /// As [`Reader::tx`].
    pub fn tx_range(&self, branch: BranchId, seqs: RangeInclusive<u64>) -> Result<Vec<(u64, u8, Hash, Vec<u8>)>, StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let t = live(&self.txn)?.open_table(TRANSACTIONS).map_err(engine)?;
            let mut out = Vec::new();
            if seqs.is_empty() {
                return Ok(out);
            }
            for row in t.range(tx_key(branch, *seqs.start())..=tx_key(branch, *seqs.end())).map_err(engine)? {
                let (k, v) = row.map_err(engine)?;
                let (flag, chain, body) = split_tx_value(v.value()).map_err(|e| StoreError::Damaged(e.to_string()))?;
                out.push((key_seq(&k.value()), flag, chain, body.to_vec()));
            }
            Ok(out)
        })
    }

    /// The highest `seq` stored for `branch`.
    ///
    /// # Errors
    /// The engine failed.
    pub fn last_seq(&self, branch: BranchId) -> Result<Option<u64>, StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let t = live(&self.txn)?.open_table(TRANSACTIONS).map_err(engine)?;
            let mut r = t.range(tx_key(branch, 0)..=tx_key(branch, u64::MAX)).map_err(engine)?;
            match r.next_back() {
                Some(row) => Ok(Some(key_seq(&row.map_err(engine)?.0.value()))),
                None => Ok(None),
            }
        })
    }

    /// The number of rows stored for `branch`.
    ///
    /// # Errors
    /// The engine failed.
    pub fn tx_count(&self, branch: BranchId) -> Result<u64, StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let t = live(&self.txn)?.open_table(TRANSACTIONS).map_err(engine)?;
            let r = t.range(tx_key(branch, 0)..=tx_key(branch, u64::MAX)).map_err(engine)?;
            Ok(r.count() as u64)
        })
    }

    /// A version row.
    ///
    /// # Errors
    /// The engine failed, or the row does not decode.
    pub fn version(&self, branch: BranchId, kind: RowKind, name: &str) -> Result<Option<VersionRecord>, StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let t = live(&self.txn)?.open_table(VERSIONS).map_err(engine)?;
            let Some(v) = t.get(version_key(branch, kind, name).as_slice()).map_err(engine)? else { return Ok(None) };
            serde_json::from_slice(v.value()).map(Some).map_err(|e| StoreError::Damaged(format!("version row: {e}")))
        })
    }

    /// The version rows of `kind` on `branch`, in key order (for automatic snapshots, `seq`
    /// order).
    ///
    /// # Errors
    /// As [`Reader::version`].
    pub fn versions(&self, branch: BranchId, kind: RowKind) -> Result<Vec<VersionRecord>, StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let t = live(&self.txn)?.open_table(VERSIONS).map_err(engine)?;
            let lo = version_key(branch, kind, "");
            let mut hi = branch.0.to_vec();
            hi.push(kind.byte() + 1);
            let mut out = Vec::new();
            for row in t.range(lo.as_slice()..hi.as_slice()).map_err(engine)? {
                let (_, v) = row.map_err(engine)?;
                out.push(serde_json::from_slice(v.value()).map_err(|e| StoreError::Damaged(format!("version row: {e}")))?);
            }
            Ok(out)
        })
    }

    /// The automatic snapshot with the highest `seq` at or before `seq` on `branch`.
    ///
    /// # Errors
    /// As [`Reader::version`].
    pub fn auto_at_or_before(&self, branch: BranchId, seq: u64) -> Result<Option<VersionRecord>, StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let t = live(&self.txn)?.open_table(VERSIONS).map_err(engine)?;
            let lo = version_key(branch, RowKind::Auto, "");
            let hi = version_key(branch, RowKind::Auto, &super::schema::auto_name(seq));
            let mut r = t.range(lo.as_slice()..=hi.as_slice()).map_err(engine)?;
            match r.next_back() {
                Some(row) => {
                    let (_, v) = row.map_err(engine)?;
                    serde_json::from_slice(v.value()).map(Some).map_err(|e| StoreError::Damaged(format!("version row: {e}")))
                }
                None => Ok(None),
            }
        })
    }

    /// A stored chunk: flag and body.
    ///
    /// # Errors
    /// The engine failed, or the row is empty.
    pub fn chunk(&self, hash: &Hash) -> Result<Option<(u8, Vec<u8>)>, StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let t = live(&self.txn)?.open_table(CHUNKS).map_err(engine)?;
            let Some(v) = t.get(*hash).map_err(engine)? else { return Ok(None) };
            let v = v.value();
            let flag = *v.first().ok_or_else(|| StoreError::Damaged("an empty chunk row".into()))?;
            Ok(Some((flag, v.get(1..).unwrap_or_default().to_vec())))
        })
    }

    /// A chunk's uncompressed body, checked against its hash.
    ///
    /// # Errors
    /// Missing, damaged, or not matching its hash.
    pub fn chunk_body(&self, hash: &Hash) -> Result<Vec<u8>, StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let (flag, body) = self.chunk(hash)?.ok_or_else(|| StoreError::Damaged(format!("chunk {} is missing", crate::chain::hex(hash))))?;
            let body = pack::unpack(flag, &body).map_err(|e| StoreError::Damaged(e.to_string()))?;
            if super::schema::chunk_hash(&body) != *hash {
                return Err(StoreError::Damaged(format!("chunk {} does not match its hash", crate::chain::hex(hash))));
            }
            Ok(body)
        })
    }

    /// Every chunk's hash, flag and stored length.
    ///
    /// # Errors
    /// The engine failed.
    pub fn chunk_list(&self) -> Result<Vec<(Hash, u8, u64)>, StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let t = live(&self.txn)?.open_table(CHUNKS).map_err(engine)?;
            let mut out = Vec::new();
            for row in t.iter().map_err(engine)? {
                let (k, v) = row.map_err(engine)?;
                let v = v.value();
                out.push((k.value(), v.first().copied().unwrap_or(0), v.len().saturating_sub(1) as u64));
            }
            Ok(out)
        })
    }

    /// An asset: media type and bytes.
    ///
    /// # Errors
    /// The engine failed, or the row does not decode.
    pub fn asset(&self, hash: &Hash) -> Result<Option<(String, Vec<u8>)>, StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let t = live(&self.txn)?.open_table(ASSETS).map_err(engine)?;
            let Some(v) = t.get(*hash).map_err(engine)? else { return Ok(None) };
            let (media, bytes) = super::schema::split_asset_value(v.value()).map_err(|e| StoreError::Damaged(e.to_string()))?;
            Ok(Some((media, bytes.to_vec())))
        })
    }

    /// Row counts and sizes of every table.
    ///
    /// # Errors
    /// The engine failed.
    pub fn stats(&self) -> Result<Vec<TableStat>, StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let txn = live(&self.txn)?;
            Ok(vec![
                table_stat(txn, META)?,
                table_stat(txn, BRANCHES)?,
                table_stat(txn, TRANSACTIONS)?,
                table_stat(txn, VERSIONS)?,
                table_stat(txn, CHUNKS)?,
                table_stat(txn, ASSETS)?,
            ])
        })
    }

    /// Every row of every table as (table, key, stored value), in schema order, for exports.
    ///
    /// # Errors
    /// The engine failed.
    pub fn raw_rows(&self, mut each: impl FnMut(&'static str, &[u8], &[u8]) -> Result<(), StoreError>) -> Result<(), StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            {
                let t = live(&self.txn)?.open_table(META).map_err(engine)?;
                for row in t.iter().map_err(engine)? {
                    let (k, v) = row.map_err(engine)?;
                    each("meta", k.value().as_bytes(), v.value())?;
                }
            }
            macro_rules! fixed {
                ($def:expr, $name:literal) => {{
                    let t = live(&self.txn)?.open_table($def).map_err(engine)?;
                    for row in t.iter().map_err(engine)? {
                        let (k, v) = row.map_err(engine)?;
                        each($name, &k.value(), v.value())?;
                    }
                }};
            }
            fixed!(BRANCHES, "branches");
            fixed!(TRANSACTIONS, "transactions");
            {
                let t = live(&self.txn)?.open_table(VERSIONS).map_err(engine)?;
                for row in t.iter().map_err(engine)? {
                    let (k, v) = row.map_err(engine)?;
                    each("versions", k.value(), v.value())?;
                }
            }
            fixed!(CHUNKS, "chunks");
            fixed!(ASSETS, "assets");
            Ok(())
        })
    }
}

/// One save: a redb write transaction behind the append-only API. Dropping it without
/// [`Writer::commit`] writes nothing. Every write runs under the database's halt flag, as a [`Reader`]'s reads do.
pub struct Writer {
    /// `None` only after [`Writer::commit`] or while it is dropped.
    txn: Option<WriteTransaction>,
    halt: Halt,
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.halt.drop_guarded(self.txn.take());
    }
}

impl Writer {
    /// Put a `meta` row. Only the names the container format defines are accepted.
    ///
    /// # Errors
    /// [`StoreError::Refused`] for another name; the engine failed.
    pub fn set_meta(&mut self, name: &str, value: &serde_json::Value) -> Result<(), StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            if !META_NAMES.contains(&name) {
                return Err(StoreError::Refused(format!("meta name {name:?} is not part of the format")));
            }
            let bytes = serde_json::to_vec(value).map_err(engine)?;
            let mut t = live(&self.txn)?.open_table(META).map_err(engine)?;
            t.insert(name, bytes.as_slice()).map_err(engine)?;
            Ok(())
        })
    }

    /// A branch row, as this transaction sees it.
    ///
    /// # Errors
    /// The engine failed, or the row does not decode.
    pub fn branch(&self, id: BranchId) -> Result<Option<BranchRecord>, StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let t = live(&self.txn)?.open_table(BRANCHES).map_err(engine)?;
            let Some(v) = t.get(id.0).map_err(engine)? else { return Ok(None) };
            serde_json::from_slice(v.value()).map(Some).map_err(|e| StoreError::Damaged(format!("branch row: {e}")))
        })
    }

    /// Put a branch row (a new branch, or its new head or status).
    ///
    /// # Errors
    /// The engine failed.
    pub fn put_branch(&mut self, id: BranchId, rec: &BranchRecord) -> Result<(), StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let bytes = serde_json::to_vec(rec).map_err(engine)?;
            let mut t = live(&self.txn)?.open_table(BRANCHES).map_err(engine)?;
            t.insert(id.0, bytes.as_slice()).map_err(engine)?;
            Ok(())
        })
    }

    /// Append transaction `seq` to `branch`: its chain value and its uncompressed record.
    ///
    /// # Errors
    /// [`StoreError::Refused`] unless `seq` comes directly after the branch's last stored row
    /// (append-only: a stored row is never replaced); the engine failed.
    pub fn append_tx(&mut self, branch: BranchId, seq: u64, chain: &Hash, record: &[u8]) -> Result<u64, StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let mut t = live(&self.txn)?.open_table(TRANSACTIONS).map_err(engine)?;
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
        })
    }

    /// Insert a chunk unless one with that hash is stored. Returns the bytes written (0 when it
    /// was already stored).
    ///
    /// # Errors
    /// [`StoreError::Refused`] when `hash` is not the hash of `body`; the engine failed.
    pub fn put_chunk(&mut self, hash: &Hash, body: &[u8]) -> Result<u64, StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            if super::schema::chunk_hash(body) != *hash {
                return Err(StoreError::Refused("a chunk's hash does not match its body".into()));
            }
            let mut t = live(&self.txn)?.open_table(CHUNKS).map_err(engine)?;
            if t.get(*hash).map_err(engine)?.is_some() {
                return Ok(0);
            }
            let (flag, packed) = pack::pack(body);
            let mut value = Vec::with_capacity(1 + packed.len());
            value.push(flag);
            value.extend_from_slice(&packed);
            t.insert(*hash, value.as_slice()).map_err(engine)?;
            Ok(value.len() as u64)
        })
    }

    /// Insert an asset unless it is stored (assets are stored as they are). Returns its hash.
    ///
    /// # Errors
    /// A media type over 255 bytes; the engine failed.
    pub fn put_asset(&mut self, media: &str, bytes: &[u8]) -> Result<Hash, StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let hash = asset_hash(bytes);
            let mut t = live(&self.txn)?.open_table(ASSETS).map_err(engine)?;
            if t.get(hash).map_err(engine)?.is_none() {
                let value = asset_value(media, bytes).map_err(|e| StoreError::Refused(e.to_string()))?;
                t.insert(hash, value.as_slice()).map_err(engine)?;
            }
            Ok(hash)
        })
    }

    /// Put a version row.
    ///
    /// # Errors
    /// The engine failed.
    pub fn put_version(&mut self, branch: BranchId, rec: &VersionRecord) -> Result<(), StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let bytes = serde_json::to_vec(rec).map_err(engine)?;
            let mut t = live(&self.txn)?.open_table(VERSIONS).map_err(engine)?;
            t.insert(version_key(branch, rec.kind, &rec.name).as_slice(), bytes.as_slice()).map_err(engine)?;
            Ok(())
        })
    }

    /// The version rows of `kind` on `branch` as this transaction sees them, in key order.
    ///
    /// # Errors
    /// The engine failed, or a row does not decode.
    pub fn versions(&self, branch: BranchId, kind: RowKind) -> Result<Vec<VersionRecord>, StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let t = live(&self.txn)?.open_table(VERSIONS).map_err(engine)?;
            let lo = version_key(branch, kind, "");
            let mut hi = branch.0.to_vec();
            hi.push(kind.byte() + 1);
            let mut out = Vec::new();
            for row in t.range(lo.as_slice()..hi.as_slice()).map_err(engine)? {
                let (_, v) = row.map_err(engine)?;
                out.push(serde_json::from_slice(v.value()).map_err(|e| StoreError::Damaged(format!("version row: {e}")))?);
            }
            Ok(out)
        })
    }

    /// Set the description of a `versions` row this write transaction holds.
    ///
    /// # Errors
    /// No such row; the engine failed.
    pub(crate) fn describe_version(&mut self, branch: BranchId, kind: RowKind, name: &str, description: &str) -> Result<(), StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let key = version_key(branch, kind, name);
            let rec = {
                let t = live(&self.txn)?.open_table(VERSIONS).map_err(engine)?;
                let v = t.get(key.as_slice()).map_err(engine)?;
                match v {
                    Some(v) => serde_json::from_slice::<VersionRecord>(v.value()).map_err(|e| StoreError::Damaged(e.to_string()))?,
                    None => return Err(StoreError::Refused(format!("version {name:?} has no row to describe"))),
                }
            };
            self.put_version(branch, &VersionRecord { description: Some(description.to_string()), ..rec })
        })
    }

    /// Remove a version row (a version unnamed by `UnnameVersion`, or a rescue dropping rows past
    /// the verified head). Its chunks stay until compaction.
    ///
    /// # Errors
    /// The engine failed.
    pub(crate) fn remove_version(&mut self, branch: BranchId, kind: RowKind, name: &str) -> Result<(), StoreError> {
        let halt = self.halt.clone();
        guard(&halt, || {
            let mut t = live(&self.txn)?.open_table(VERSIONS).map_err(engine)?;
            t.remove(version_key(branch, kind, name).as_slice()).map_err(engine)?;
            Ok(())
        })
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
        let halt = self.halt.clone();
        guard(&halt, || {
            {
                let t = live(&self.txn)?.open_table(TRANSACTIONS).map_err(engine)?;
                if !t.is_empty().map_err(engine)? {
                    return Err(StoreError::Refused("Save As copies only into an empty file".into()));
                }
            }
            let mut bytes = 0u64;
            let mut last: Option<([u8; 16], u64)> = None;
            let mut meta = live(&self.txn)?.open_table(META).map_err(engine)?;
            let mut branches = live(&self.txn)?.open_table(BRANCHES).map_err(engine)?;
            let mut txs = live(&self.txn)?.open_table(TRANSACTIONS).map_err(engine)?;
            let mut versions = live(&self.txn)?.open_table(VERSIONS).map_err(engine)?;
            let mut chunks = live(&self.txn)?.open_table(CHUNKS).map_err(engine)?;
            let mut assets = live(&self.txn)?.open_table(ASSETS).map_err(engine)?;
            let fixed = |k: &[u8], n: usize| -> Result<Vec<u8>, StoreError> {
                if k.len() == n { Ok(k.to_vec()) } else { Err(StoreError::Engine(format!("a key of {} bytes where {n} were expected", k.len()))) }
            };
            from.raw_rows(|table, k, v| {
                bytes += (k.len() + v.len()) as u64;
                let e = |e: redb::StorageError| StoreError::Engine(e.to_string());
                match table {
                    "meta" => {
                        let name = std::str::from_utf8(k).map_err(|e| StoreError::Engine(e.to_string()))?;
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
                            return Err(StoreError::Engine(format!("transaction {seq} follows {} on its branch", expected - 1)));
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
                    other => return Err(StoreError::Engine(format!("unknown table {other}"))),
                }
                Ok(())
            })
            .map_err(|e| match e {
                StoreError::Engine(m) => StoreError::Refused(m),
                e => e,
            })?;
            Ok(bytes)
        })
    }

    /// Commit durably: after a crash, all of this save is in the file or none of it.
    ///
    /// # Errors
    /// The engine failed; nothing of this save is in the file.
    pub fn commit(mut self) -> Result<(), StoreError> {
        let halt = self.halt.clone();
        let txn = self.txn.take().ok_or_else(|| StoreError::Engine("the transaction has ended".into()))?;
        guard(&halt, || txn.commit().map_err(engine))
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
