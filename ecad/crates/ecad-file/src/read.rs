//! Opening a project file (proposal §20.6).
//!
//! The bytes are split into complete records (ending in `\n`) and at most one unterminated
//! trailing fragment. Each complete record is checked in order: frame and CRC, parse, limits,
//! record type, sequence, hash chain (on the stored form), op names and versions, upcast,
//! embedded content hashes, then
//! replay preconditions and document invariants. The verified prefix ends at the last record that
//! passes every check, and what follows it decides the outcome:
//!
//! - nothing: **clean**, writable;
//! - only an unterminated fragment: an **incomplete trailing write**, writable, fragment reported;
//! - a complete record that fails any check, at any position including the last: **corrupt**,
//!   read-only, the source untouched, the omitted tail reported.

use ecad_model::{Ledger, ModelError, TxId};
use serde_json::Value;

use crate::canonical::MAX_SAFE_INTEGER;
use crate::chain::{self, Hash};
use crate::codec::{Codec, FrameError, JsonLines};
use crate::record::{self, DecodeError, Header, MAX_RECORDS};

/// Whether the opened project may be saved back to the same file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Writable,
    /// Damage was found: the file must not be written; Save As can rescue the verified history.
    ReadOnly,
}

/// The check a corrupt record failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// Not the `jsonl/1` frame shape, or too long.
    Frame,
    /// The frame checksum does not match the payload.
    Crc,
    /// The payload is not valid JSON or not a valid record.
    Parse,
    /// A size or integer limit.
    Limits,
    /// Not a transaction record (including the reserved `checkpoint`).
    RecordType,
    /// `seq` is not the next number.
    Sequence,
    /// `prev` does not link to the previous record.
    Chain,
    /// An op this build does not know.
    UnknownOp,
    /// An op version that is newer, or older with no upcast.
    Upcast,
    /// An embedded definition's recorded content hash does not match its content.
    ContentHash,
    /// Replay found the document did not hold an op's recorded `before` state.
    Precondition,
    /// Replay would break a document invariant.
    Invariant,
    /// The undo/redo structure of the history is inconsistent.
    History,
}

/// A complete record that failed a check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corruption {
    /// Byte offset of the failing record.
    pub offset: u64,
    /// Its `seq`, when it could be read.
    pub seq: Option<u64>,
    pub check: Check,
    pub detail: String,
    /// Complete records omitted, the failing one included.
    pub omitted_records: u64,
    /// Bytes omitted from the failing record to the end of the file.
    pub omitted_bytes: u64,
}

/// What follows the verified prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Clean,
    /// An unterminated fragment after the last complete record: an interrupted append.
    IncompleteTail {
        offset: u64,
        len: u64,
        /// The first bytes, escaped for display.
        preview: String,
    },
    Corrupt(Corruption),
}

/// Where a verified transaction's record sits in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordSpan {
    pub seq: u64,
    pub offset: u64,
    pub len: u64,
}

/// What opening found. Always shown to the user when it is not [`Outcome::Clean`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenReport {
    pub access: Access,
    /// The last verified transaction.
    pub verified_seq: u64,
    /// Bytes of the verified prefix (header included).
    pub verified_len: u64,
    /// Bytes in the file.
    pub file_len: u64,
    pub outcome: Outcome,
}

impl OpenReport {
    /// One paragraph for the user.
    pub fn describe(&self) -> String {
        match &self.outcome {
            Outcome::Clean => {
                format!("Opened: {} transactions verified ({} bytes).", self.verified_seq, self.verified_len)
            }
            Outcome::IncompleteTail { offset, len, preview } => format!(
                "Recovered from an interrupted save: {} transactions verified ({} bytes). An incomplete trailing write \
                 of {len} bytes at offset {offset} was omitted: {preview}",
                self.verified_seq, self.verified_len
            ),
            Outcome::Corrupt(c) => format!(
                "Opened read-only: the record at offset {} ({}) failed the {:?} check: {}. {} transactions are verified \
                 ({} bytes); {} complete records ({} bytes) after them were omitted. The file was not changed; use \
                 Save As to keep the verified history.",
                c.offset,
                c.seq.map_or("sequence unknown".into(), |s| format!("transaction {s}")),
                c.check,
                c.detail,
                self.verified_seq,
                self.verified_len,
                c.omitted_records,
                c.omitted_bytes
            ),
        }
    }
}

/// Why a file could not be opened at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpenError {
    #[error("the file is empty")]
    Empty,
    #[error("the file has no valid header: {0}")]
    Header(String),
    #[error("{0}")]
    Unsupported(String),
}

/// A project read from a file.
#[derive(Debug, Clone)]
pub struct Opened {
    pub ledger: Ledger,
    pub header: Header,
    pub report: OpenReport,
    /// Where each verified transaction's record sits.
    pub spans: Vec<RecordSpan>,
    /// History hash of the last verified transaction (the seed when there is none).
    pub chain_head: Hash,
    /// ID of the last verified transaction.
    pub last_tx: Option<TxId>,
    /// The unterminated fragment, when [`Outcome::IncompleteTail`].
    pub fragment: Option<Vec<u8>>,
}

fn preview(bytes: &[u8]) -> String {
    let shown: String = bytes.iter().take(48).flat_map(|b| std::ascii::escape_default(*b)).map(char::from).collect();
    if bytes.len() > 48 { format!("\"{shown}…\"") } else { format!("\"{shown}\"") }
}

struct Line {
    offset: usize,
    /// Without the terminator.
    end: usize,
}

/// Open a project file's bytes. `seed` seeds IDs allocated after opening.
///
/// # Errors
/// [`OpenError`] when there is no valid, supported header. Damage after the header never
/// fails: it opens the verified prefix and reports.
pub fn open_project(bytes: &[u8], seed: u64) -> Result<Opened, OpenError> {
    if bytes.is_empty() {
        return Err(OpenError::Empty);
    }
    let codec = JsonLines;
    let mut lines = Vec::new();
    let mut start = 0;
    let mut too_many = false;
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'\n' {
            if lines.len() > MAX_RECORDS {
                too_many = true;
                break;
            }
            lines.push(Line { offset: start, end: i });
            start = i + 1;
        }
    }
    let fragment_start = start;
    let Some(first) = lines.first() else {
        return Err(OpenError::Header("the header record is incomplete".into()));
    };
    let header_bytes = bytes.get(first.offset..first.end).unwrap_or_default();
    let payload = codec.payload(header_bytes).map_err(|e| OpenError::Header(e.to_string()))?;
    let value: Value = serde_json::from_slice(payload).map_err(|e| OpenError::Header(e.to_string()))?;
    if record::record_type(&value) != Some("header") {
        return Err(OpenError::Header("the first record is not a header".into()));
    }
    // The header gets the same hostile-input limits as every other record.
    record::check_limits(&value).map_err(|e| OpenError::Header(format!("header: {e}")))?;
    let header: Header = serde_json::from_value(value).map_err(|e| OpenError::Header(e.to_string()))?;
    if header.created_ms.is_some_and(|t| t > MAX_SAFE_INTEGER) {
        return Err(OpenError::Header("header: created_ms exceeds 2^53 - 1".into()));
    }
    let (kind, _) = header.validate().map_err(OpenError::Unsupported)?;
    if kind != chain::ChainKind::Project {
        return Err(OpenError::Unsupported("this is a library file; libraries are opened in stage 3".into()));
    }
    let seed_hash = header.seed().map_err(OpenError::Unsupported)?;

    // Records: frame, parse, limits, type, sequence, chain (stored form), ops, upcast.
    let mut txs = Vec::new();
    let mut hashes = Vec::new();
    let mut spans = Vec::new();
    let mut chain_head = seed_hash;
    let mut failure: Option<(usize, Option<u64>, Check, String)> = None;
    for (n, line) in lines.iter().enumerate().skip(1) {
        let expected = n as u64;
        let frame = bytes.get(line.offset..line.end).unwrap_or_default();
        let fail = |check, detail: String, seq| Some((n, seq, check, detail));
        let payload = match codec.payload(frame) {
            Ok(p) => p,
            Err(FrameError::Crc { stored, computed }) => {
                failure = fail(Check::Crc, FrameError::Crc { stored, computed }.to_string(), None);
                break;
            }
            Err(e) => {
                failure = fail(Check::Frame, e.to_string(), None);
                break;
            }
        };
        let value: Value = match serde_json::from_slice(payload) {
            Ok(v) => v,
            Err(e) => {
                failure = fail(Check::Parse, e.to_string(), None);
                break;
            }
        };
        if let Err(e) = record::check_limits(&value) {
            failure = fail(Check::Limits, e, None);
            break;
        }
        if record::record_type(&value) != Some("tx") {
            let what = record::record_type(&value).map_or("record has no type".to_string(), |t| match t {
                "checkpoint" => "checkpoint records are not supported (checkpoints are disabled, §20.8)".into(),
                t => format!("unexpected record type {t:?}"),
            });
            failure = fail(Check::RecordType, what, None);
            break;
        }
        let (seq, prev) = record::seq_and_prev(&value);
        if seq != Some(expected) {
            failure = fail(Check::Sequence, format!("expected transaction {expected}, found {seq:?}"), seq);
            break;
        }
        if prev.and_then(chain::parse_hex) != Some(chain_head) {
            failure = fail(Check::Chain, "prev does not link to the previous record".into(), seq);
            break;
        }
        let canonical = match crate::canonical::canonical(&value) {
            Ok(c) => c,
            Err(e) => {
                failure = fail(Check::Limits, e.to_string(), seq);
                break;
            }
        };
        let hash = chain::tx_hash(&canonical);
        let tx = match record::decode_tx(&value) {
            Ok(tx) => tx,
            Err(e) => {
                let (check, detail) = match e {
                    DecodeError::RecordType(d) => (Check::RecordType, d),
                    DecodeError::Parse(d) => (Check::Parse, d),
                    DecodeError::UnknownOp(d) => (Check::UnknownOp, d),
                    DecodeError::Upcast(d) => (Check::Upcast, d),
                    DecodeError::Limits(d) => (Check::Limits, d),
                    DecodeError::ContentHash(d) => (Check::ContentHash, d),
                };
                failure = fail(check, detail, seq);
                break;
            }
        };
        chain_head = hash;
        hashes.push(hash);
        spans.push(RecordSpan { seq: expected, offset: line.offset as u64, len: (line.end + 1 - line.offset) as u64 });
        txs.push(tx);
    }

    // Replay: preconditions, invariants and the undo/redo structure.
    if too_many && failure.is_none() {
        failure = Some((lines.len(), None, Check::Limits, format!("more than {MAX_RECORDS} records")));
    }
    let (ledger, replay_failure) = Ledger::replay_prefix(txs, seed);
    if let Some((k, e)) = replay_failure {
        let check = match e {
            ModelError::Precondition(_) => Check::Precondition,
            ModelError::Invariant(_) => Check::Invariant,
            _ => Check::History,
        };
        failure = Some((k + 1, Some(k as u64 + 1), check, e.to_string()));
    }
    let good = ledger.log().len();
    let last_line = lines.get(good).map_or(first, |l| l);
    let verified_len = (last_line.end + 1) as u64;
    let chain_head = if good == 0 { seed_hash } else { hashes.get(good - 1).copied().unwrap_or(seed_hash) };
    let file_len = bytes.len() as u64;
    let (outcome, fragment) = match failure {
        Some((n, seq, check, detail)) => {
            let offset = lines.get(n).map_or(fragment_start as u64, |l| l.offset as u64);
            let omitted_records = (lines.len() - n.min(lines.len())) as u64;
            (
                Outcome::Corrupt(Corruption {
                    offset,
                    seq,
                    check,
                    detail,
                    omitted_records,
                    omitted_bytes: file_len.saturating_sub(offset),
                }),
                None,
            )
        }
        None if fragment_start < bytes.len() => {
            let frag = bytes.get(fragment_start..).unwrap_or_default();
            (
                Outcome::IncompleteTail {
                    offset: fragment_start as u64,
                    len: frag.len() as u64,
                    preview: preview(frag),
                },
                Some(frag.to_vec()),
            )
        }
        None => (Outcome::Clean, None),
    };
    let access = if matches!(outcome, Outcome::Corrupt(_)) { Access::ReadOnly } else { Access::Writable };
    spans.truncate(good);
    let last_tx = ledger.log().last().map(|t| t.id);
    Ok(Opened {
        report: OpenReport { access, verified_seq: good as u64, verified_len, file_len, outcome },
        ledger,
        header,
        spans,
        chain_head,
        last_tx,
        fragment,
    })
}
