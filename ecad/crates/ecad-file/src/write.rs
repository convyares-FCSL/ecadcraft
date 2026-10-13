//! Writing a project file: first write, and Save with retry (proposal §20.5).
//!
//! The writer keeps the **verified prefix** (`verified_len`), the **last acknowledged durable
//! position** (`persisted`), the chain head, a running SHA-256 of the verified prefix (to confirm
//! the whole prefix is still ours) and the **attempt** of a failed save. Every save starts by
//! re-reading the verified prefix and checking its digest, removing only bytes that our own failed attempt (or the incomplete
//! trailing write reported at open) left after the verified prefix, and refusing any other
//! change. A failed flush is never retried on the same bytes: the batch is truncated and
//! rewritten. `persisted` advances only after a successful flush.
//!
//! Stage 1 runs over [`ecad_store::Medium`] (memory). Save As with replacement, folder flushes
//! and writer locks are stage 2.

use std::io;

use ecad_model::{Ledger, ProjectId, TxId};
use ecad_store::Medium;
use sha2::{Digest, Sha256};

use crate::chain::{self, Hash};
use crate::codec::{Codec, JsonLines};
use crate::read::{self, Access, OpenError, OpenReport, RecordSpan};
use crate::record::{self, Header};

/// How far a failed save got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptStage {
    /// The append failed; a prefix of the batch may be in the medium.
    AppendFailed,
    /// The whole batch was appended but the flush failed; durability is unknown.
    FlushFailed,
}

/// A save that did not complete: its bytes identify what it may have left in the medium.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    /// Where the batch starts (the verified length at the time).
    pub offset: u64,
    pub bytes: Vec<u8>,
    pub first_seq: u64,
    pub last_seq: u64,
    pub stage: AttemptStage,
}

/// The writer's state, for reports and the future debugger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterState {
    pub access: Access,
    pub verified_len: u64,
    pub persisted: u64,
    pub attempt: Option<Attempt>,
    /// An incomplete trailing write reported at open, still in the medium.
    pub pending_fragment: Option<u64>,
}

/// What a successful save did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    Saved {
        first_seq: u64,
        last_seq: u64,
        bytes: u64,
    },
    /// Nothing was pending (any leftover bytes of our own were removed).
    NothingToSave,
}

/// Why a save did not complete. In every case the in-memory project is unchanged and still
/// unsaved, and `persisted` has not moved.
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("the file was opened read-only because it is damaged; use Save As")]
    ReadOnly,
    #[error("the file was changed by something else ({0}); it was not modified; use Save As")]
    ExternalChange(String),
    #[error("this file holds a different history ({0})")]
    NotThisHistory(String),
    #[error("could not read the file: {0}")]
    Read(io::Error),
    #[error("could not remove the bytes of the earlier failed save: {0}")]
    Truncate(io::Error),
    #[error("writing failed: {0}")]
    Append(io::Error),
    #[error("could not confirm the data is durable: {0}")]
    Flush(io::Error),
    #[error("a new file's target is not empty")]
    NotEmpty,
    #[error("could not encode: {0}")]
    Encode(String),
}

/// A project file over a medium.
#[derive(Debug)]
pub struct ProjectFile<M: Medium> {
    medium: M,
    header: Header,
    access: Access,
    verified_len: u64,
    persisted: u64,
    last_tx: Option<TxId>,
    chain_head: Hash,
    /// SHA-256 state over the verified prefix's bytes.
    prefix_digest: Sha256,
    fragment: Option<Vec<u8>>,
    attempt: Option<Attempt>,
    spans: Vec<RecordSpan>,
}

/// Encode transactions `from..` of `ledger`, chained from `prev`: the bytes, the spans relative
/// to the batch start and the last hash.
fn encode_batch(ledger: &Ledger, from: usize, prev: Hash) -> Result<(Vec<u8>, Vec<RecordSpan>, Hash), String> {
    let codec = JsonLines;
    let mut bytes = Vec::new();
    let mut spans = Vec::new();
    let mut prev = prev;
    for tx in ledger.log().iter().skip(from) {
        let value = record::tx_value(tx, &prev)?;
        let canonical = crate::canonical::canonical(&value).map_err(|e| e.to_string())?;
        prev = chain::tx_hash(&canonical);
        let line = codec.encode(&value).map_err(|e| e.to_string())?;
        spans.push(RecordSpan { seq: tx.seq, offset: bytes.len() as u64, len: line.len() as u64 });
        bytes.extend_from_slice(&line);
    }
    Ok((bytes, spans, prev))
}

impl<M: Medium> ProjectFile<M> {
    /// Write a new file holding the whole history (first save, or the write phase of Save As).
    /// The medium must be empty. On success every transaction is acknowledged durable.
    ///
    /// # Errors
    /// [`SaveError`]; the medium is then discarded by the caller (it is the temporary target).
    pub fn create(
        mut medium: M,
        project: ProjectId,
        ledger: &mut Ledger,
        created_ms: Option<u64>,
        writer: &str,
    ) -> Result<Self, SaveError> {
        if !medium.is_empty() {
            return Err(SaveError::NotEmpty);
        }
        let header = Header::project(project, created_ms, writer);
        let seed = header.seed().map_err(SaveError::Encode)?;
        let header_value = serde_json::to_value(&header).map_err(|e| SaveError::Encode(e.to_string()))?;
        let header_line = JsonLines.encode(&header_value).map_err(|e| SaveError::Encode(e.to_string()))?;
        let (batch, spans, head_hash) = encode_batch(ledger, 0, seed).map_err(SaveError::Encode)?;
        let mut all = header_line.clone();
        all.extend_from_slice(&batch);
        medium.append(&all).map_err(SaveError::Append)?;
        medium.flush().map_err(SaveError::Flush)?;
        let head = ledger.head();
        ledger.mark_persisted(head).map_err(|e| SaveError::Encode(e.to_string()))?;
        let base = header_line.len() as u64;
        Ok(Self {
            medium,
            header,
            access: Access::Writable,
            verified_len: all.len() as u64,
            persisted: head,
            last_tx: ledger.log().last().map(|t| t.id),
            chain_head: head_hash,
            prefix_digest: Sha256::new().chain_update(&all),
            fragment: None,
            attempt: None,
            spans: spans.into_iter().map(|s| RecordSpan { offset: s.offset + base, ..s }).collect(),
        })
    }

    /// Open the file in `medium`: the ledger of its verified prefix and the report (§20.6).
    ///
    /// # Errors
    /// [`OpenError`] when there is no valid, supported header; [`OpenError::Header`] also when
    /// the medium cannot be read.
    pub fn open(medium: M, seed: u64) -> Result<(Self, Ledger, OpenReport), OpenError> {
        let bytes = medium.read_all().map_err(|e| OpenError::Header(format!("could not read: {e}")))?;
        let opened = read::open_project(&bytes, seed)?;
        let verified =
            usize::try_from(opened.report.verified_len).ok().and_then(|n| bytes.get(..n)).unwrap_or_default();
        let file = Self {
            medium,
            header: opened.header,
            access: opened.report.access,
            verified_len: opened.report.verified_len,
            persisted: opened.report.verified_seq,
            last_tx: opened.last_tx,
            chain_head: opened.chain_head,
            prefix_digest: Sha256::new().chain_update(verified),
            fragment: opened.fragment,
            attempt: None,
            spans: opened.spans,
        };
        Ok((file, opened.ledger, opened.report))
    }

    /// Append the pending transactions and flush (§20.5).
    ///
    /// # Errors
    /// [`SaveError`]; the in-memory project is unchanged and still unsaved.
    pub fn save(&mut self, ledger: &mut Ledger) -> Result<SaveOutcome, SaveError> {
        if self.access == Access::ReadOnly {
            return Err(SaveError::ReadOnly);
        }
        let head = ledger.head();
        if head < self.persisted
            || (self.persisted > 0 && ledger.log().get(self.persisted as usize - 1).map(|t| t.id) != self.last_tx)
        {
            return Err(SaveError::NotThisHistory("its saved transactions are not this project's".into()));
        }
        // 1. The whole verified prefix must still be ours: it is re-read and its digest compared,
        //    so a change anywhere in it (not only at its end) refuses the save.
        let len = self.medium.len();
        if len < self.verified_len {
            return Err(SaveError::ExternalChange("it is shorter than the saved history".into()));
        }
        if self.prefix_hash()? != self.prefix_digest.clone().finalize().as_slice() {
            return Err(SaveError::ExternalChange("the saved history in it has changed".into()));
        }
        // 2. Classify bytes beyond the verified prefix.
        let extra_len = len - self.verified_len;
        if extra_len > 0 {
            let known = self
                .attempt
                .as_ref()
                .map_or(0, |a| a.bytes.len() as u64)
                .max(self.fragment.as_ref().map_or(0, |f| f.len() as u64));
            if extra_len > known {
                return Err(SaveError::ExternalChange(format!(
                    "{extra_len} unexpected bytes follow the saved history"
                )));
            }
            let extra = self.medium.read_range(self.verified_len, extra_len).map_err(SaveError::Read)?;
            let ours = self.attempt.as_ref().is_some_and(|a| a.bytes.starts_with(&extra))
                || self.fragment.as_ref().is_some_and(|f| *f == extra);
            if !ours {
                return Err(SaveError::ExternalChange(
                    "the bytes after the saved history are not this writer's".into(),
                ));
            }
            self.medium.truncate(self.verified_len).map_err(SaveError::Truncate)?;
        }
        self.fragment = None;
        if head == self.persisted {
            self.attempt = None;
            return Ok(SaveOutcome::NothingToSave);
        }
        // 3. Write the batch afresh (it includes edits made since any failed attempt).
        let (bytes, spans, head_hash) =
            encode_batch(ledger, self.persisted as usize, self.chain_head).map_err(SaveError::Encode)?;
        let first_seq = self.persisted + 1;
        if let Err(e) = self.medium.append(&bytes) {
            self.attempt = Some(Attempt {
                offset: self.verified_len,
                bytes,
                first_seq,
                last_seq: head,
                stage: AttemptStage::AppendFailed,
            });
            return Err(SaveError::Append(e));
        }
        // 4. Flush; a failure is never retried on the same bytes.
        if let Err(e) = self.medium.flush() {
            self.attempt = Some(Attempt {
                offset: self.verified_len,
                bytes,
                first_seq,
                last_seq: head,
                stage: AttemptStage::FlushFailed,
            });
            return Err(SaveError::Flush(e));
        }
        // 5. Acknowledge.
        ledger.mark_persisted(head).map_err(|e| SaveError::Encode(e.to_string()))?;
        let base = self.verified_len;
        self.spans.extend(spans.into_iter().map(|s| RecordSpan { offset: s.offset + base, ..s }));
        self.verified_len += bytes.len() as u64;
        self.persisted = head;
        self.last_tx = ledger.log().last().map(|t| t.id);
        self.chain_head = head_hash;
        self.prefix_digest.update(&bytes);
        self.attempt = None;
        Ok(SaveOutcome::Saved { first_seq, last_seq: head, bytes: bytes.len() as u64 })
    }

    /// SHA-256 of the medium's first `verified_len` bytes, read in chunks.
    fn prefix_hash(&self) -> Result<Vec<u8>, SaveError> {
        const CHUNK: u64 = 1 << 20;
        let mut digest = Sha256::new();
        let mut at = 0;
        while at < self.verified_len {
            let n = CHUNK.min(self.verified_len - at);
            let part = self.medium.read_range(at, n).map_err(SaveError::Read)?;
            if part.len() as u64 != n {
                return Err(SaveError::ExternalChange("it is shorter than the saved history".into()));
            }
            digest.update(&part);
            at += n;
        }
        Ok(digest.finalize().to_vec())
    }

    /// The writer's state.
    pub fn state(&self) -> WriterState {
        WriterState {
            access: self.access,
            verified_len: self.verified_len,
            persisted: self.persisted,
            attempt: self.attempt.clone(),
            pending_fragment: self.fragment.as_ref().map(|f| f.len() as u64),
        }
    }

    /// The file's header.
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Where each acknowledged transaction's record sits.
    pub fn spans(&self) -> &[RecordSpan] {
        &self.spans
    }

    /// The medium.
    pub fn medium(&self) -> &M {
        &self.medium
    }

    /// The medium, for injecting faults or simulating external changes in tests.
    pub fn medium_mut(&mut self) -> &mut M {
        &mut self.medium
    }

    /// Take the medium back.
    pub fn into_medium(self) -> M {
        self.medium
    }
}
