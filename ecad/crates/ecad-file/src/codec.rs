//! Codecs: how one record becomes bytes and back (proposal §20.1, layer 2).
//!
//! JSON Lines (`jsonl/1`) is the only codec. Each line is exactly
//! `{"crc":"<8 lower-case hex>","rec":<payload>}\n`: a 24-byte fixed prefix, the payload (the
//! record's canonical JSON), and the suffix `}\n`. The CRC-32C covers exactly the payload bytes,
//! so checking it never depends on removing or re-encoding a field.

use serde_json::Value;

use crate::canonical::{CanonicalError, canonical};
use crate::crc::crc32c;
use crate::record::MAX_LINE;

/// A record codec. Later codecs (§20.1) implement the same contract; a conformance suite will
/// require identical records and history hashes from each.
pub trait Codec {
    /// The `codec` value written in the header.
    fn id(&self) -> &'static str;
    /// One framed record, terminator included.
    ///
    /// # Errors
    /// The record has no canonical form.
    fn encode(&self, record: &Value) -> Result<Vec<u8>, CanonicalError>;
    /// The payload of one framed record (terminator excluded), after checking its frame.
    ///
    /// # Errors
    /// [`FrameError`] naming the problem.
    fn payload<'a>(&self, frame: &'a [u8]) -> Result<&'a [u8], FrameError>;
}

/// Why a frame is not valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// Not the `jsonl/1` frame shape, or too long.
    Shape(String),
    /// The CRC does not match the payload.
    Crc { stored: u32, computed: u32 },
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::Shape(m) => f.write_str(m),
            FrameError::Crc { stored, computed } => {
                write!(f, "frame checksum {stored:08x} does not match the payload ({computed:08x})")
            }
        }
    }
}

/// The `jsonl/1` codec.
#[derive(Debug, Clone, Copy, Default)]
pub struct JsonLines;

const PREFIX_OPEN: &[u8] = b"{\"crc\":\"";
const PREFIX_CLOSE: &[u8] = b"\",\"rec\":";
/// Length of the fixed frame prefix: `{"crc":"` + 8 hex digits + `","rec":`.
pub const PREFIX_LEN: usize = 24;

impl Codec for JsonLines {
    fn id(&self) -> &'static str {
        crate::record::CODEC
    }

    fn encode(&self, record: &Value) -> Result<Vec<u8>, CanonicalError> {
        let payload = canonical(record)?;
        let mut out = Vec::with_capacity(payload.len() + PREFIX_LEN + 2);
        out.extend_from_slice(PREFIX_OPEN);
        out.extend_from_slice(format!("{:08x}", crc32c(&payload)).as_bytes());
        out.extend_from_slice(PREFIX_CLOSE);
        out.extend_from_slice(&payload);
        out.extend_from_slice(b"}\n");
        Ok(out)
    }

    fn payload<'a>(&self, frame: &'a [u8]) -> Result<&'a [u8], FrameError> {
        if frame.len() > MAX_LINE {
            return Err(FrameError::Shape(format!("record longer than {MAX_LINE} bytes")));
        }
        let shape = || FrameError::Shape("not a jsonl/1 frame".into());
        let crc_hex = frame.get(PREFIX_OPEN.len()..PREFIX_OPEN.len() + 8).ok_or_else(shape)?;
        if !frame.starts_with(PREFIX_OPEN)
            || frame.get(PREFIX_OPEN.len() + 8..PREFIX_LEN) != Some(PREFIX_CLOSE)
            || frame.last() != Some(&b'}')
            || frame.len() < PREFIX_LEN + 1
            || !crc_hex.iter().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c))
        {
            return Err(shape());
        }
        let stored =
            std::str::from_utf8(crc_hex).ok().and_then(|t| u32::from_str_radix(t, 16).ok()).ok_or_else(shape)?;
        let payload = frame.get(PREFIX_LEN..frame.len() - 1).ok_or_else(shape)?;
        let computed = crc32c(payload);
        if stored != computed {
            return Err(FrameError::Crc { stored, computed });
        }
        Ok(payload)
    }
}
