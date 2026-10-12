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

/// The test-only store.
///
/// # Semantics
/// - [`TestStore::save`] appends every transaction after the ledger's persisted position, flushes,
///   and only then calls [`Ledger::mark_persisted`]. On any failure it truncates the medium back
///   to the last good length, leaves the ledger's persisted position unchanged and reports the
///   error (SAVE-04).
/// - [`TestStore::open`] reads the header and complete lines; a torn trailing line is ignored
///   (FILE-03) and the log is replayed (FILE-02).
/// - The checkpoint is an optional in-memory cache: [`TestStore::open_with_checkpoint`] must give
///   the same state as [`TestStore::open`] (FILE-04).
#[derive(Debug, Default)]
pub struct TestStore<M: Medium> {
    pub medium: M,
    good_len: u64,
    checkpoint: Option<(u64, String)>,
}

impl<M: Medium> TestStore<M> {
    pub fn new(medium: M) -> Self {
        let good_len = medium.len();
        Self { medium, good_len, checkpoint: None }
    }

    /// Append unsaved transactions and mark them persisted.
    ///
    /// # Errors
    /// [`StoreError::Io`] when appending or flushing fails; the ledger stays unsaved.
    pub fn save(&mut self, ledger: &mut Ledger) -> Result<u64> {
        let mut out = Vec::new();
        if self.good_len == 0 {
            let header = Header { test_store_format: TEST_STORE_FORMAT, schema_version: ecad_model::SCHEMA_VERSION };
            out.extend(serde_json::to_vec(&header).map_err(|e| StoreError::Format(e.to_string()))?);
            out.push(b'\n');
        }
        let from = ledger.persisted() as usize;
        for tx in ledger.log().iter().skip(from) {
            out.extend(serde_json::to_vec(tx).map_err(|e| StoreError::Format(e.to_string()))?);
            out.push(b'\n');
        }
        // Remove any torn bytes left by an earlier failure before appending.
        if self.medium.len() != self.good_len {
            self.medium.truncate(self.good_len)?;
        }
        let result = self.medium.append(&out).and_then(|()| self.medium.flush());
        match result {
            Ok(()) => {
                self.good_len = self.medium.len();
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
        let mut lines = bytes.split(|b| *b == b'\n');
        let header: Header = match lines.next() {
            Some(l) if !l.is_empty() => serde_json::from_slice(l).map_err(|e| StoreError::Format(e.to_string()))?,
            _ => return Ok(Vec::new()),
        };
        if header.test_store_format != TEST_STORE_FORMAT {
            return Err(StoreError::Format(format!("test store format {} is not supported", header.test_store_format)));
        }
        let complete = bytes.ends_with(b"\n");
        let all: Vec<&[u8]> = lines.filter(|l| !l.is_empty()).collect();
        let mut log = Vec::new();
        for (i, l) in all.iter().enumerate() {
            match serde_json::from_slice::<Transaction>(l) {
                Ok(tx) => log.push(tx),
                // A torn trailing line is ignored (FILE-03); anything else is corruption.
                Err(_) if i + 1 == all.len() && !complete => break,
                Err(e) => return Err(StoreError::Format(format!("transaction line {}: {e}", i + 1))),
            }
        }
        Ok(log)
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
