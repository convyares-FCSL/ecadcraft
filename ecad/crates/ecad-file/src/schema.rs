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

use ecad_model::{PointKind, PointName};

use crate::FileError;
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
pub(crate) const ASSETS: TableDefinition<[u8; 32], &[u8]> = TableDefinition::new("assets");

/// `meta.format` of a project file.
pub(crate) const FORMAT_PROJECT: &str = "ecad-project";
/// `meta.container_version`.
pub const CONTAINER_VERSION: u64 = 2;
/// `meta.codec`: rows and chunks are JSON, compressed (overseer, 2026-11-08; `bin/1` later).
pub const CODEC: &str = "json/1";
/// The `meta` names this build reads and writes. Any other name is an unknown word (pre-v1, no
/// forward compatibility; owner, 0.2.3): the project opens read-only, names it and keeps the file
/// as it is. A name this build needs and cannot read is refused.
pub const META_NAMES: [&str; 9] = ["format", "container_version", "schema_version", "codec", "file_id", "writer", "created_ms", "cache", "active"];
/// The default number of transactions between automatic snapshots (overseer, 2026-11-08).
pub const DEFAULT_SNAPSHOT_EVERY: u64 = 2_000;
/// Smallest and largest snapshot intervals accepted from a file.
pub const SNAPSHOT_EVERY_RANGE: (u64, u64) = (16, 1_000_000);
/// The most entries of each undo and redo stack a snapshot records (overseer, 2026-11-09).
pub const STACK_CAP: u64 = 1_024;
/// Longest version name accepted from a file.
pub const MAX_NAME: usize = 4_096;

/// A branch ID: 16 bytes. In JSON it is its 32 lower-case hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BranchId(pub [u8; 16]);

impl Serialize for BranchId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.hex())
    }
}

impl<'de> Deserialize<'de> for BranchId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        BranchId::parse(&text).ok_or_else(|| serde::de::Error::custom(format!("{text:?} is not a branch ID (32 hex digits)")))
    }
}

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

    /// From 32 hex digits.
    pub fn parse(text: &str) -> Option<Self> {
        let t = text.trim();
        if t.len() != 32 || !t.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let n = u128::from_str_radix(t, 16).ok()?;
        Some(BranchId(n.to_be_bytes()))
    }

    /// The branch ID made from `seed` (overseer FQ7: random 128-bit). The caller passes the
    /// randomness, from `ecad_store::random_u128` (design 065 §3.1: ecad-file reads no OS
    /// randomness or clock itself).
    pub(crate) fn from_seed(seed: u128) -> Self {
        BranchId(seed.to_be_bytes())
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
pub(crate) fn key_seq(key: &[u8; 24]) -> u64 {
    let mut s = [0u8; 8];
    s.copy_from_slice(key.get(16..).unwrap_or(&[0u8; 8]));
    u64::from_be_bytes(s)
}

/// What a `versions` row is (overseer, 2026-11-08: an explicit kind, never a name prefix; S4: the
/// two named kinds are [`PointKind`]s, the same type the document keys its points by).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowKind {
    /// A named point: a user's version or an issued revision (brief 019: with release data).
    Point(PointKind),
    /// An automatic snapshot, a loading accelerator only: never shown as a version.
    Auto,
    /// The merge index (brief 020b, FQ2): one row per live merge into the branch, named by its
    /// `seq` as [`auto_name`]; no chunks.
    Merge,
}

impl VersionRecord {
    /// The named point this row is: its kind and key say which, never the spelling. `None` for an
    /// automatic snapshot, a merge row, or a key that is no point's.
    pub fn named_point(&self) -> Option<PointName> {
        match self.kind {
            RowKind::Point(k) => PointName::from_row(k, &self.name),
            RowKind::Auto | RowKind::Merge => None,
        }
    }
}

impl RowKind {
    /// A user's named version.
    pub const VERSION: RowKind = RowKind::Point(PointKind::Version);
    /// An issued revision.
    pub const REVISION: RowKind = RowKind::Point(PointKind::Revision);

    /// The kind byte in a `versions` key.
    pub fn byte(self) -> u8 {
        match self {
            RowKind::Point(PointKind::Version) => 1,
            RowKind::Point(PointKind::Revision) => 2,
            RowKind::Auto => 3,
            RowKind::Merge => 4,
        }
    }

    /// The kind in words, as the inspector shows it.
    pub fn name(self) -> &'static str {
        match self {
            RowKind::Point(PointKind::Version) => "Version",
            RowKind::Point(PointKind::Revision) => "Revision",
            RowKind::Auto => "Auto",
            RowKind::Merge => "Merge",
        }
    }

    /// The row kind of a named point.
    pub fn of(point: &PointName) -> RowKind {
        RowKind::Point(point.kind())
    }
}

/// The `versions` key of `name` of `kind` on `branch`. Automatic snapshots are named by their
/// `seq` as 20 decimal digits, so key order is `seq` order.
pub fn version_key(branch: BranchId, kind: RowKind, name: &str) -> Vec<u8> {
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
    pub fn hash(&self) -> Result<Hash, FileError> {
        chain::parse_hex(&self.chain).ok_or_else(|| FileError::ChainValue(self.chain.clone()))
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

/// Where a merged branch went (FQ3; written by merges, brief 020b).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Merged {
    /// The target branch ID (hex).
    pub into: String,
    /// The merge transaction's `seq` on the target.
    pub seq: u64,
}

/// A `branches` row. Unknown fields are refused (pre-v1: a newer file is refused by its format version).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BranchRecord {
    pub name: String,
    pub status: BranchStatus,
    /// Absent for the original branch.
    pub parent: Option<BranchId>,
    /// Absent for the original branch: the parent transaction it starts after.
    pub fork_point: Option<Point>,
    /// The last `seq` and its chain value.
    pub head: Point,
    pub release_line: bool,
    /// Where a `merged` branch was merged (brief 020b).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged: Option<Merged>,
    /// When the branch was created (milliseconds since 1970; for display).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_ms: Option<u64>,
    /// Who created it (an author's display text; for display).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

impl BranchRecord {
    /// Whether this is the original branch: the only one with no parent.
    pub fn is_main(&self) -> bool {
        self.parent.is_none()
    }

    /// The original branch's row at `head`.
    pub fn main(head: Point) -> Self {
        Self {
            name: "main".into(),
            status: BranchStatus::Open,
            parent: None,
            fork_point: None,
            head,
            release_line: true,
            merged: None,
            created_ms: None,
            created_by: None,
        }
    }
}

/// Release data of an issued revision (EEP-0005 §3.4). Unknown fields are refused.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    /// The revision record as issued (rev, date, description, drawn, checked, approved, ecr and
    /// the project's own columns), typed as in the project.
    pub revision: ecad_model::Revision,
    /// Asset hashes of the frozen outputs; none exist yet.
    pub assets: Vec<String>,
}

/// A `versions` row: a point, the hashes of its chunks, and what opening there needs. Unknown
/// fields are refused (pre-v1: a newer file is refused by its format version).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionRecord {
    pub kind: RowKind,
    pub name: String,
    pub point: Point,
    /// Chunk key → chunk hash (64 hex digits) of the state at `point`.
    pub chunks: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<Release>,
    /// The top of the undo and redo stacks at `point`: `[seq, transaction ID]`, oldest first,
    /// at most `stack_cap` entries each (EEP-0005 §5.5 as built, overseer 2026-11-09).
    pub undo: Vec<(u64, String)>,
    pub redo: Vec<(u64, String)>,
    /// How many entries lie below `undo` and `redo`: read from the history, once a session, when
    /// undo or redo first reaches them.
    #[serde(default)]
    pub undo_hidden: u64,
    #[serde(default)]
    pub redo_hidden: u64,
    /// The cap this row was written with.
    #[serde(default)]
    pub stack_cap: u64,
    /// The version's description, when it was given one (FQ5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A merge index row's facts (kind `merge` only, FQ2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge: Option<MergeRow>,
    /// The rule ([`ecad_model::naming::tag::TAG_RULE`]) this snapshot's `model:meta` tag marks were
    /// computed with (brief 029, FP1). Absent in snapshots written before it: rule 1. Marks of an
    /// older rule are recomputed from the history on open. Here, not in `model:meta` (which
    /// refuses unknown fields), so builds before it still open files that carry it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_rule: Option<u32>,
}

impl VersionRecord {
    /// The rule its cached tag marks were computed with (1 when it does not say).
    pub fn tag_rule(&self) -> u32 {
        self.tag_rule.unwrap_or(1)
    }

    /// Whether its cached tag marks can be used as they are: computed with this build's rule (or
    /// a newer one, which counts at least as much).
    pub(crate) fn tag_marks_current(&self) -> bool {
        self.tag_rule() >= ecad_model::naming::tag::TAG_RULE
    }
}

/// What a merge index row records (FQ2): the merge's source branch, the source head it merged and
/// the common ancestor it compared against. The row's `point` is the merge step itself (or the
/// redo that made it live again).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeRow {
    pub source: String,
    pub source_head: Point,
    pub ancestor: AncestorPoint,
}

/// A step of a named branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AncestorPoint {
    pub branch: String,
    pub seq: u64,
    pub chain: String,
}

impl MergeRow {
    /// The row of a merge transaction's kind (`None` for any other kind).
    pub fn of(kind: &ecad_model::TxKind) -> Option<Self> {
        match kind {
            ecad_model::TxKind::Merge { source, source_head, ancestor } => Some(Self {
                source: BranchId(*source).hex(),
                source_head: Point::new(source_head.seq, &source_head.chain),
                ancestor: AncestorPoint { branch: BranchId(ancestor.branch).hex(), seq: ancestor.seq, chain: chain::hex(&ancestor.chain) },
            }),
            _ => None,
        }
    }

    /// The source branch's ID.
    pub(crate) fn source_id(&self) -> Option<BranchId> {
        BranchId::parse(&self.source)
    }

    /// The ancestor's branch ID.
    pub(crate) fn ancestor_id(&self) -> Option<BranchId> {
        BranchId::parse(&self.ancestor.branch)
    }
}

/// `meta.cache`: the snapshot policy and, per branch, the latest snapshot (EEP-0005 §3.1).
/// Unknown fields are refused (pre-v1: a newer file is refused by its format version).
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
/// Unknown fields are refused (pre-v1: a newer file is refused by its format version).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotRef {
    pub kind: RowKind,
    pub name: String,
    pub seq: u64,
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
pub fn split_tx_value(v: &[u8]) -> Result<(u8, Hash, &[u8]), FileError> {
    let (Some(flag), Some(chain), Some(body)) = (v.first(), v.get(1..33), v.get(33..)) else {
        return Err(FileError::TxRowShort(v.len()));
    };
    let mut c = [0u8; 32];
    c.copy_from_slice(chain);
    Ok((*flag, c, body))
}

/// `media type length ‖ media type ‖ bytes`.
///
/// # Errors
/// A media type longer than 255 bytes.
pub(crate) fn asset_value(media: &str, bytes: &[u8]) -> Result<Vec<u8>, FileError> {
    let len = u16::try_from(media.len()).ok().filter(|l| *l <= 255).ok_or(FileError::MediaTooLong)?;
    let mut v = Vec::with_capacity(2 + media.len() + bytes.len());
    v.extend_from_slice(&len.to_be_bytes());
    v.extend_from_slice(media.as_bytes());
    v.extend_from_slice(bytes);
    Ok(v)
}

/// The media type and bytes of an `assets` value.
///
/// # Errors
/// A malformed value, or a media type over 255 bytes.
pub fn split_asset_value(v: &[u8]) -> Result<(String, &[u8]), FileError> {
    let len = v.get(..2).map(|b| usize::from(u16::from_be_bytes([b[0], b[1]]))).ok_or(FileError::AssetRowShort)?;
    // EEP-0005 read limits: a media type is at most 255 bytes.
    if len > 255 {
        return Err(FileError::AssetMediaLong);
    }
    let media = v.get(2..2 + len).ok_or(FileError::AssetRowShort)?;
    let media = std::str::from_utf8(media).map_err(|_| FileError::AssetMediaUtf8)?;
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
        assert!(version_key(b, RowKind::Auto, &auto_name(9)) < version_key(b, RowKind::Auto, &auto_name(10)));
        let v = tx_value(1, &[7; 32], b"body");
        assert_eq!(split_tx_value(&v).unwrap(), (1, [7; 32], &b"body"[..]));
        assert!(split_tx_value(&v[..20]).is_err());
        let a = asset_value("image/png", b"\x89PNG").unwrap();
        assert_eq!(split_asset_value(&a).unwrap(), ("image/png".to_string(), &b"\x89PNG"[..]));
        assert!(split_asset_value(&a[..3]).is_err());
    }
}
