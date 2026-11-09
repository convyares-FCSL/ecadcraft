//! The logical schema of container version 2 (EEP-0005 §3): the redb tables, their keys and the
//! records stored in them.
//!
//! - Integers in keys are big-endian, so key order is numeric order.
//! - `meta`, `branches` and `versions` values are JSON (small, read by eye in `ecadinspect`).
//! - A `transactions` value is `flag ‖ chain value (32 bytes) ‖ body`, the body being the
//!   transaction's E2 canonical JSON record (codec `json/1`), compressed when smaller (flag, see
//!   [`super::pack`]). The chain value is E2's `H(record)`, so it is also the next record's `prev`.
//! - A `chunks` value is `flag ‖ body`, keyed by [`chunk_hash`] of the uncompressed body.
//! - An `assets` value is `media type length (u16 BE) ‖ media type ‖ bytes`, keyed by SHA-256 of
//!   the bytes, stored as they are.

use std::collections::BTreeMap;

use redb::TableDefinition;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::chain::{self, Hash};

/// `meta`: name → JSON value.
pub const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");
/// `branches`: branch ID → [`BranchRecord`] as JSON.
pub const BRANCHES: TableDefinition<[u8; 16], &[u8]> = TableDefinition::new("branches");
/// `transactions`: branch ID ‖ `seq` (big-endian) → flag ‖ chain value ‖ body.
pub const TRANSACTIONS: TableDefinition<[u8; 24], &[u8]> = TableDefinition::new("transactions");
/// `versions`: branch ID ‖ kind byte ‖ name (UTF-8) → [`VersionRecord`] as JSON.
pub const VERSIONS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("versions");
/// `chunks`: chunk hash → flag ‖ body.
pub const CHUNKS: TableDefinition<[u8; 32], &[u8]> = TableDefinition::new("chunks");
/// `assets`: SHA-256 of the bytes → media type and bytes.
pub const ASSETS: TableDefinition<[u8; 32], &[u8]> = TableDefinition::new("assets");

/// `meta.format` of a project file.
pub const FORMAT_PROJECT: &str = "ecad-project";
/// `meta.container_version`.
pub const CONTAINER_VERSION: u64 = 2;
/// `meta.codec`: rows and chunks are JSON, compressed (overseer, 2026-11-08; `bin/1` later).
pub const CODEC: &str = "json/1";
/// The `meta` names this build reads; any other is refused (EEP-0005 §3.1, strict reading).
pub const META_NAMES: [&str; 9] =
    ["format", "container_version", "schema_version", "codec", "file_id", "writer", "created_ms", "cache", "link_v1"];
/// The default number of transactions between automatic snapshots (overseer, 2026-11-08).
pub const DEFAULT_SNAPSHOT_EVERY: u64 = 2_000;
/// Smallest and largest snapshot intervals accepted from a file.
pub const SNAPSHOT_EVERY_RANGE: (u64, u64) = (16, 1_000_000);
/// Longest version name accepted from a file.
pub const MAX_NAME: usize = 4_096;

/// A branch ID: 16 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BranchId(pub [u8; 16]);

impl BranchId {
    /// The original branch, `main`, of project `project`: derived from the project ID, so every
    /// file of a project names it alike.
    pub fn main_of(project: u128) -> Self {
        let mut h = Sha256::new();
        h.update(b"ecad-branch/main\0");
        h.update(project.to_be_bytes());
        let d: Hash = h.finalize().into();
        let mut id = [0u8; 16];
        id.copy_from_slice(d.get(..16).unwrap_or(&[0u8; 16]));
        BranchId(id)
    }

    /// Lower-case hex.
    pub fn hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// The `transactions` key of `seq` on `branch`.
pub fn tx_key(branch: BranchId, seq: u64) -> [u8; 24] {
    let mut k = [0u8; 24];
    for (slot, b) in k.iter_mut().zip(branch.0.iter().chain(seq.to_be_bytes().iter())) {
        *slot = *b;
    }
    k
}

/// The `seq` of a `transactions` key.
pub fn key_seq(key: &[u8; 24]) -> u64 {
    let mut s = [0u8; 8];
    s.copy_from_slice(key.get(16..).unwrap_or(&[0u8; 8]));
    u64::from_be_bytes(s)
}

/// What a version row is (overseer, 2026-11-08: an explicit kind, never a name prefix).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionKind {
    /// A named version (`NameVersion`).
    User,
    /// An issued revision (brief 019): a named version with release data.
    Revision,
    /// An automatic snapshot, a loading accelerator only: never shown as a version.
    Auto,
}

impl VersionKind {
    /// The kind byte in a `versions` key.
    pub fn byte(self) -> u8 {
        match self {
            VersionKind::User => 1,
            VersionKind::Revision => 2,
            VersionKind::Auto => 3,
        }
    }
}

/// The `versions` key of `name` of `kind` on `branch`. Automatic snapshots are named by their
/// `seq` as 20 decimal digits, so key order is `seq` order.
pub fn version_key(branch: BranchId, kind: VersionKind, name: &str) -> Vec<u8> {
    let mut k = branch.0.to_vec();
    k.push(kind.byte());
    k.extend_from_slice(name.as_bytes());
    k
}

/// The name of the automatic snapshot at `seq`.
pub fn auto_name(seq: u64) -> String {
    format!("{seq:020}")
}

/// A point in a branch's history: a `seq` and its chain value (the seed at 0).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub seq: u64,
    /// 64 hex digits.
    pub chain: String,
}

impl Point {
    pub fn new(seq: u64, chain: &Hash) -> Self {
        Self { seq, chain: chain::hex(chain) }
    }

    /// The chain value.
    ///
    /// # Errors
    /// Not 64 hex digits.
    pub fn hash(&self) -> Result<Hash, String> {
        chain::parse_hex(&self.chain).ok_or_else(|| format!("{:?} is not a chain value", self.chain))
    }
}

/// A branch's status (EEP-0005 §3.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchStatus {
    Open,
    Merged,
    Archived,
}

/// A `branches` row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BranchRecord {
    pub name: String,
    pub status: BranchStatus,
    /// Absent for the original branch.
    pub parent: Option<String>,
    /// Absent for the original branch: the parent transaction it starts after.
    pub fork_point: Option<Point>,
    /// The last `seq` and its chain value.
    pub head: Point,
    pub release_line: bool,
}

/// Release data of an issued revision (EEP-0005 §3.4).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    /// The revision's fields as issued (`rev`, `date`, `description`, `drawn`, `checked`,
    /// `approved`), from the project's `rev.<n>.*` properties.
    pub fields: BTreeMap<String, String>,
    /// Asset hashes of the frozen outputs; none exist yet.
    pub assets: Vec<String>,
}

/// A `versions` row: a point, the hashes of its chunks, and what opening there needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionRecord {
    pub kind: VersionKind,
    pub name: String,
    pub point: Point,
    /// Chunk key → chunk hash (64 hex digits) of the state at `point`.
    pub chunks: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<Release>,
    /// The undo and redo stacks at `point`: `[seq, transaction ID]`, oldest first
    /// (EEP-0005 §5.5, option a).
    pub undo: Vec<(u64, String)>,
    pub redo: Vec<(u64, String)>,
}

/// `meta.cache`: the snapshot policy and, per branch, the latest snapshot (EEP-0005 §3.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cache {
    /// Transactions between automatic snapshots.
    pub every: u64,
    /// Branch ID (hex) → the latest snapshot on it.
    pub latest: BTreeMap<String, SnapshotRef>,
}

impl Default for Cache {
    fn default() -> Self {
        Self { every: DEFAULT_SNAPSHOT_EVERY, latest: BTreeMap::new() }
    }
}

/// Which `versions` row holds a snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotRef {
    pub kind: VersionKind,
    pub name: String,
    pub seq: u64,
}

/// `meta.link_v1`: the version 1 history a converted file continues (EEP-0005 §10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkV1 {
    /// The version 1 file's final chain hash.
    pub hash: String,
    pub transactions: u64,
    pub source_container: u64,
    pub source_schema: u64,
}

/// The content address of a chunk: `SHA-256("ecad-chunk/1\0" ‖ uncompressed body)`.
pub fn chunk_hash(body: &[u8]) -> Hash {
    let mut h = Sha256::new();
    h.update(b"ecad-chunk/1\0");
    h.update(body);
    h.finalize().into()
}

/// The content address of an asset: SHA-256 of its bytes.
pub fn asset_hash(bytes: &[u8]) -> Hash {
    Sha256::digest(bytes).into()
}

/// `flag ‖ chain ‖ body`.
pub fn tx_value(flag: u8, chain: &Hash, body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(33 + body.len());
    v.push(flag);
    v.extend_from_slice(chain);
    v.extend_from_slice(body);
    v
}

/// The chunks of a `transactions` value.
///
/// # Errors
/// Shorter than its fixed chunk.
pub fn split_tx_value(v: &[u8]) -> Result<(u8, Hash, &[u8]), String> {
    let (Some(flag), Some(chain), Some(body)) = (v.first(), v.get(1..33), v.get(33..)) else {
        return Err(format!("a transaction row of {} bytes is too short", v.len()));
    };
    let mut c = [0u8; 32];
    c.copy_from_slice(chain);
    Ok((*flag, c, body))
}

/// `media type length ‖ media type ‖ bytes`.
///
/// # Errors
/// A media type longer than 255 bytes.
pub fn asset_value(media: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
    let len = u16::try_from(media.len()).ok().filter(|l| *l <= 255).ok_or("a media type is too long")?;
    let mut v = Vec::with_capacity(2 + media.len() + bytes.len());
    v.extend_from_slice(&len.to_be_bytes());
    v.extend_from_slice(media.as_bytes());
    v.extend_from_slice(bytes);
    Ok(v)
}

/// The media type and bytes of an `assets` value.
///
/// # Errors
/// A malformed value.
pub fn split_asset_value(v: &[u8]) -> Result<(String, &[u8]), String> {
    let len = v.get(..2).map(|b| usize::from(u16::from_be_bytes([b[0], b[1]]))).ok_or("an asset row is too short")?;
    let media = v.get(2..2 + len).ok_or("an asset row is too short")?;
    let media = std::str::from_utf8(media).map_err(|_| "an asset's media type is not UTF-8")?;
    Ok((media.to_string(), v.get(2 + len..).unwrap_or_default()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn keys_order_numerically_and_round_trip() {
        let b = BranchId::main_of(42);
        assert_eq!(b, BranchId::main_of(42));
        assert_ne!(b, BranchId::main_of(43));
        let keys: Vec<_> = [1u64, 2, 255, 256, 70_000, u64::MAX].iter().map(|s| tx_key(b, *s)).collect();
        assert!(keys.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(key_seq(&tx_key(b, 70_000)), 70_000);
        assert!(version_key(b, VersionKind::Auto, &auto_name(9)) < version_key(b, VersionKind::Auto, &auto_name(10)));
        let v = tx_value(1, &[7; 32], b"body");
        assert_eq!(split_tx_value(&v).unwrap(), (1, [7; 32], &b"body"[..]));
        assert!(split_tx_value(&v[..20]).is_err());
        let a = asset_value("image/png", b"\x89PNG").unwrap();
        assert_eq!(split_asset_value(&a).unwrap(), ("image/png".to_string(), &b"\x89PNG"[..]));
        assert!(split_asset_value(&a[..3]).is_err());
    }
}
