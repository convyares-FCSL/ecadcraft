//! ecad-store
//!
//! The **storage layer** of ECAD persistence (proposal §20.1, layer 3): byte media that project
//! and library files are written to, and nothing else. What the bytes mean, and how records are
//! framed and checked, belongs to `ecad-file`.
//!
//! - [`Medium`]: append, flush, truncate and read, the operations the writer's durability and
//!   retry rules (§20.5) are built from.
//! - [`MemoryMedium`]: an in-memory medium with injectable failures (partial append, failed
//!   flush, failed truncate) and direct access for simulating external changes, so every
//!   failure point can be tested without a disk.
//!
//! Excluded until stage 2 of the persistence plan: files on disk, atomic replacement, folder
//! flushes and writer locks.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::io;

/// Byte storage a file writer appends to.
///
/// # Semantics
/// `append` may write a prefix of its bytes and then fail; `flush` is the durability point; after
/// a failed `flush` nothing is known about whether the data is durable.
pub trait Medium {
    /// Current length in bytes.
    fn len(&self) -> u64;
    /// Whether the medium holds nothing.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Append bytes (possibly partially, on failure).
    ///
    /// # Errors
    /// The underlying write failed; a prefix of `bytes` may have been written.
    fn append(&mut self, bytes: &[u8]) -> io::Result<()>;
    /// Make appended bytes durable.
    ///
    /// # Errors
    /// Durability could not be confirmed.
    fn flush(&mut self) -> io::Result<()>;
    /// Cut the medium back to `len` bytes.
    ///
    /// # Errors
    /// The medium could not be truncated; its contents are unchanged.
    fn truncate(&mut self, len: u64) -> io::Result<()>;
    /// Everything stored.
    ///
    /// # Errors
    /// The medium could not be read.
    fn read_all(&self) -> io::Result<Vec<u8>>;
    /// `len` bytes from `offset` (fewer at the end of the medium).
    ///
    /// # Errors
    /// The medium could not be read.
    fn read_range(&self, offset: u64, len: u64) -> io::Result<Vec<u8>> {
        let all = self.read_all()?;
        let start = usize::try_from(offset).map_err(io::Error::other)?.min(all.len());
        let end = start.saturating_add(usize::try_from(len).map_err(io::Error::other)?).min(all.len());
        Ok(all.get(start..end).map(<[u8]>::to_vec).unwrap_or_default())
    }
}

/// An in-memory medium with injectable failures.
///
/// # Example
/// ```rust
/// use ecad_store::{Medium, MemoryMedium};
/// let mut m = MemoryMedium::default();
/// m.partial_append = Some(3);
/// assert!(m.append(b"abcdef").is_err());
/// assert_eq!(m.read_all().unwrap(), b"abc");
/// ```
#[derive(Debug, Default, Clone)]
pub struct MemoryMedium {
    bytes: Vec<u8>,
    /// The next append writes half of its bytes and then fails (a torn write).
    pub fail_next_append: bool,
    /// The next append writes this many bytes (at most) and then fails.
    pub partial_append: Option<usize>,
    /// The next flush fails.
    pub fail_next_flush: bool,
    /// The next truncate fails and changes nothing.
    pub fail_next_truncate: bool,
    /// Flushes that succeeded, for tests.
    pub flushes: u32,
}

impl MemoryMedium {
    /// A medium holding `bytes`.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes, ..Self::default() }
    }

    /// Direct access to the stored bytes, for simulating changes made by someone else.
    pub fn external(&mut self) -> &mut Vec<u8> {
        &mut self.bytes
    }
}

impl Medium for MemoryMedium {
    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        let cut = match (self.partial_append.take(), std::mem::take(&mut self.fail_next_append)) {
            (Some(n), _) => Some(n.min(bytes.len())),
            (None, true) => Some(bytes.len() / 2),
            (None, false) => None,
        };
        match cut {
            Some(n) => {
                self.bytes.extend_from_slice(bytes.get(..n).unwrap_or_default());
                Err(io::Error::other("injected write failure"))
            }
            None => {
                self.bytes.extend_from_slice(bytes);
                Ok(())
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        if std::mem::take(&mut self.fail_next_flush) {
            return Err(io::Error::other("injected flush failure"));
        }
        self.flushes += 1;
        Ok(())
    }
    fn truncate(&mut self, len: u64) -> io::Result<()> {
        if std::mem::take(&mut self.fail_next_truncate) {
            return Err(io::Error::other("injected truncate failure"));
        }
        self.bytes.truncate(usize::try_from(len).map_err(io::Error::other)?);
        Ok(())
    }
    fn read_all(&self) -> io::Result<Vec<u8>> {
        Ok(self.bytes.clone())
    }
}
