//! The history hash chain (proposal §20.10).
//!
//! Unkeyed SHA-256: it detects accidental damage, reordering, missing or substituted records
//! within a history, records from another project or library, and diverging copies. It does
//! **not** protect against deliberate rewriting, and it cannot detect truncation at a record
//! boundary (a valid prefix is a valid history).

use sha2::{Digest, Sha256};

/// A 32-byte SHA-256 value.
pub type Hash = [u8; 32];

/// Which kind of file a chain belongs to (domain separation of the seed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainKind {
    Project,
    Library,
}

/// The value transaction 1's `prev` must hold: `SHA-256("ecad-chain/1\0" ‖ kind ‖ "\0" ‖ id)`,
/// with the 128-bit ID as 16 big-endian bytes. Independent of codec, writer and time.
pub fn seed(kind: ChainKind, id: u128) -> Hash {
    let mut h = Sha256::new();
    h.update(b"ecad-chain/1\0");
    h.update(match kind {
        ChainKind::Project => &b"project"[..],
        ChainKind::Library => &b"library"[..],
    });
    h.update(b"\0");
    h.update(id.to_be_bytes());
    h.finalize().into()
}

/// `H(tx) = SHA-256("ecad-tx/1\0" ‖ canonical(tx))`, over the transaction record as stored
/// (including its own `prev`), before any upcast.
pub fn tx_hash(canonical_tx: &[u8]) -> Hash {
    let mut h = Sha256::new();
    h.update(b"ecad-tx/1\0");
    h.update(canonical_tx);
    h.finalize().into()
}

/// SHA-256 of a definition record's canonical form: the content hash of §20.9.
pub fn content_hash(canonical_definition: &[u8]) -> Hash {
    let mut h = Sha256::new();
    h.update(b"ecad-definition/1\0");
    h.update(canonical_definition);
    h.finalize().into()
}

/// Lower-case hex.
pub fn hex(h: &Hash) -> String {
    h.iter().map(|b| format!("{b:02x}")).collect()
}

/// Parse 64 hex digits.
pub fn parse_hex(text: &str) -> Option<Hash> {
    ecad_model::ContentHash::parse(text).map(|c| c.0)
}
