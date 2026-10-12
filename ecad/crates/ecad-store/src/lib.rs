//! ecad-store
//!
//! A **provisional, test-only** project store for the bounded integration experiment
//! (proposal §17.1). It writes one header line and then one JSON line per transaction, appends on
//! save, flushes, and advances the ledger's persisted position only after the flush succeeds
//! (SAVE-04). A failing medium can be injected to test save failure.
//!
//! This crate intentionally is **not** the `.ecad` encoding: framing, checksums, CBOR/MessagePack,
//! durable checkpoints and recovery journals are out of scope until the format is designed.
//! Checkpoints here are a disposable in-memory cache (FILE-04).
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::io;

use ecad_model::{Document, Ledger, Transaction};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Version of this test store's layout. Independent of product and schema versions (§8.1).
pub const TEST_STORE_FORMAT: u32 = 1;

/// Store errors.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("write failed: {0}")]
    Io(#[from] io::Error),
    #[error("unreadable store: {0}")]
    Format(String),
    #[error(transparent)]
    Model(#[from] ecad_model::ModelError),
}

/// Result alias.
pub type Result<T> = std::result::Result<T, StoreError>;

/// Byte storage the store appends to.
pub trait Medium {
    /// Current length in bytes.
    fn len(&self) -> u64;
    /// Whether the medium holds nothing.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Append bytes (possibly partially, on failure).
    fn append(&mut self, bytes: &[u8]) -> io::Result<()>;
    /// Make appended bytes durable.
    fn flush(&mut self) -> io::Result<()>;
    /// Cut the medium back to `len` bytes.
    fn truncate(&mut self, len: u64) -> io::Result<()>;
    /// Everything stored.
    fn read_all(&self) -> io::Result<Vec<u8>>;
}

/// An in-memory medium with injectable failures.
#[derive(Debug, Default, Clone)]
pub struct MemoryMedium {
    bytes: Vec<u8>,
    /// The next append writes only half its bytes and then fails (a torn write).
    pub fail_next_append: bool,
    /// The next flush fails.
    pub fail_next_flush: bool,
}

impl Medium for MemoryMedium {
    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        if std::mem::take(&mut self.fail_next_append) {
            let half = bytes.get(..bytes.len() / 2).unwrap_or_default();
            self.bytes.extend_from_slice(half);
            return Err(io::Error::other("injected write failure"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn flush(&mut self) -> io::Result<()> {
        if std::mem::take(&mut self.fail_next_flush) {
            return Err(io::Error::other("injected flush failure"));
        }
        Ok(())
    }
    fn truncate(&mut self, len: u64) -> io::Result<()> {
        self.bytes.truncate(usize::try_from(len).map_err(io::Error::other)?);
        Ok(())
    }
    fn read_all(&self) -> io::Result<Vec<u8>> {
        Ok(self.bytes.clone())
    }
}

#[derive(Serialize, Deserialize)]
struct Header {
    test_store_format: u32,
    schema_version: u32,
}

/// What the medium holds, counting complete (newline-terminated) records only.
struct Scan {
    /// Byte offset just after the last complete record.
    good_len: u64,
    header: Option<Header>,
    log: Vec<Transaction>,
}

/// Read complete records. Bytes after the last newline are a torn write and are ignored (FILE-03):
/// a record only counts once its terminating newline is durable.
fn scan(bytes: &[u8]) -> Result<Scan> {
    let complete_end = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    let complete = bytes.get(..complete_end).unwrap_or_default();
    let mut lines = complete.split(|b| *b == b'\n').filter(|l| !l.is_empty());
    let header: Header = match lines.next() {
        Some(l) => serde_json::from_slice(l).map_err(|e| StoreError::Format(format!("header: {e}")))?,
        None => return Ok(Scan { good_len: 0, header: None, log: Vec::new() }),
    };
    if header.test_store_format != TEST_STORE_FORMAT {
        return Err(StoreError::Format(format!("test store format {} is not supported", header.test_store_format)));
    }
    if header.schema_version != ecad_model::SCHEMA_VERSION {
        return Err(StoreError::Format(format!(
            "schema version {} is not supported (this build reads {})",
            header.schema_version,
            ecad_model::SCHEMA_VERSION
        )));
    }
    let mut log = Vec::new();
    for (i, l) in lines.enumerate() {
        log.push(
            serde_json::from_slice::<Transaction>(l)
                .map_err(|e| StoreError::Format(format!("transaction line {}: {e}", i + 1)))?,
        );
    }
    Ok(Scan { good_len: complete_end as u64, header: Some(header), log })
}

/// The test-only store.
///
/// # Semantics
/// - The store tracks what **it** holds: the offset after its last complete record and the
///   transactions in it. [`TestStore::save`] appends exactly the ledger's transactions this medium
///   does not yet hold, so saving to a new, empty medium (Save As) writes the whole history. It
///   refuses a ledger whose history does not continue the stored one (another project).
/// - Before appending, any torn bytes after the last complete record are cut off, including bytes
///   left by a process that died mid-write before this store was opened.
/// - Only after the flush succeeds is [`Ledger::mark_persisted`] called. On failure the medium is
///   cut back, the ledger stays unsaved and the error is reported (SAVE-04).
/// - [`TestStore::open`] replays the complete records (FILE-02); the checkpoint is an optional
///   in-memory cache that must give the same state (FILE-04).
#[derive(Debug, Default)]
pub struct TestStore<M: Medium> {
    pub medium: M,
    good_len: u64,
    /// Number of transactions durably stored, and the last one's ID.
    stored: usize,
    last_stored: Option<ecad_model::TxId>,
    /// Why the medium cannot be appended to, if it is unreadable.
    unreadable: Option<String>,
    checkpoint: Option<(u64, String)>,
}

impl<M: Medium> TestStore<M> {
    pub fn new(medium: M) -> Self {
        let scanned = medium.read_all().map_err(StoreError::Io).and_then(|b| scan(&b));
        let (good_len, stored, last_stored, unreadable) = match scanned {
            Ok(sc) => (sc.good_len, sc.log.len(), sc.log.last().map(|t| t.id), None),
            Err(e) => (0, 0, None, Some(e.to_string())),
        };
        Self { medium, good_len, stored, last_stored, unreadable, checkpoint: None }
    }

    /// Append the transactions this medium does not hold yet and mark them persisted.
    ///
    /// # Errors
    /// [`StoreError::Io`] when appending or flushing fails (the ledger stays unsaved);
    /// [`StoreError::Format`] when the medium is unreadable or holds another project's history.
    pub fn save(&mut self, ledger: &mut Ledger) -> Result<u64> {
        if let Some(why) = &self.unreadable {
            return Err(StoreError::Format(format!("the store cannot be appended to: {why}")));
        }
        let continues = self.stored <= ledger.log().len()
            && (self.stored == 0 || ledger.log().get(self.stored - 1).map(|t| t.id) == self.last_stored);
        if !continues {
            return Err(StoreError::Format("the store holds a different project's history".into()));
        }
        let mut out = Vec::new();
        if self.good_len == 0 {
            let header = Header { test_store_format: TEST_STORE_FORMAT, schema_version: ecad_model::SCHEMA_VERSION };
            out.extend(serde_json::to_vec(&header).map_err(|e| StoreError::Format(e.to_string()))?);
            out.push(b'\n');
        }
        for tx in ledger.log().iter().skip(self.stored) {
            out.extend(serde_json::to_vec(tx).map_err(|e| StoreError::Format(e.to_string()))?);
            out.push(b'\n');
        }
        // Cut off torn bytes (an earlier failure, or a crash before this store was opened).
        if self.medium.len() != self.good_len {
            self.medium.truncate(self.good_len)?;
        }
        let result = self.medium.append(&out).and_then(|()| self.medium.flush());
        match result {
            Ok(()) => {
                self.good_len = self.medium.len();
                self.stored = ledger.log().len();
                self.last_stored = ledger.log().last().map(|t| t.id);
                ledger.mark_persisted(ledger.head())?;
                Ok(ledger.head())
            }
            Err(e) => {
                let _ = self.medium.truncate(self.good_len);
                Err(StoreError::Io(e))
            }
        }
    }

    fn read_log(&self) -> Result<Vec<Transaction>> {
        let bytes = self.medium.read_all()?;
        let sc = scan(&bytes)?;
        let _ = sc.header;
        Ok(sc.log)
    }

    /// Open by replaying the whole log.
    ///
    /// # Errors
    /// Unreadable data or a log that does not replay.
    pub fn open(&self, seed: u64) -> Result<Ledger> {
        Ok(Ledger::replay(self.read_log()?, seed)?)
    }

    /// Cache the document at the ledger's head (disposable, FILE-04).
    ///
    /// # Errors
    /// Serialisation failure.
    pub fn write_checkpoint(&mut self, ledger: &Ledger) -> Result<()> {
        let json = serde_json::to_string(ledger.document()).map_err(|e| StoreError::Format(e.to_string()))?;
        self.checkpoint = Some((ledger.head(), json));
        Ok(())
    }

    /// Delete the cache.
    pub fn drop_checkpoint(&mut self) {
        self.checkpoint = None;
    }

    /// Open from the checkpoint plus the log tail, or the whole log when there is no checkpoint.
    ///
    /// # Errors
    /// As [`TestStore::open`].
    pub fn open_with_checkpoint(&self, seed: u64) -> Result<Ledger> {
        let log = self.read_log()?;
        match &self.checkpoint {
            Some((at, json)) if *at <= log.len() as u64 => {
                let doc: Document = serde_json::from_str(json).map_err(|e| StoreError::Format(e.to_string()))?;
                Ok(Ledger::from_checkpoint(doc, *at, log, seed)?)
            }
            _ => Ok(Ledger::replay(log, seed)?),
        }
    }
}
