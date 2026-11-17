//! [`FileError`]: what is wrong with a stored row, chunk or record, or with a value that cannot be
//! stored (brief 070).
//!
//! Each variant's `Display` is the sentence the crate wrote as text before, word for word. The
//! errors of whole operations ([`crate::OpenErrorV2`], [`crate::SaveErrorV2`],
//! [`crate::BranchError`], [`crate::StoreError`], [`crate::binding::BindingError`]) carry it where
//! the problem lies in the file.

use ecad_model::ModelError;

use crate::canonical::CanonicalError;
use crate::chunks::META_KEY;
use crate::pack::MAX_BODY;
use crate::record::{DecodeError, MAX_ARRAY, MAX_DEPTH, MAX_OBJECT, MAX_STRING};
use crate::records::OP_VERSION;
use crate::schema::SNAPSHOT_EVERY_RANGE;
use crate::store::StoreError;

/// Why stored data does not read, or a value cannot be written.
///
/// # Semantics
/// Nothing was changed by the operation that returned it. The variants name the problem; the
/// `Display` text is what a person reads (in a damage report, a refusal or Verify).
///
/// # Example
/// ```
/// use ecad_file::FileError;
/// assert_eq!(FileError::TxNotFollowing(7).to_string(), "transaction 7 does not follow the one before");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FileError {
    // The database.
    /// The database could not be read or written.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// A JSON value did not encode or decode (serde's own words).
    #[error("{0}")]
    Json(String),
    /// The model refused (replaying ops, a ledger built on a snapshot).
    #[error(transparent)]
    Model(#[from] ModelError),
    /// A save's own error, where a read shares the save's code (the merge index).
    #[error(transparent)]
    Save(Box<crate::SaveErrorV2>),
    /// A position that does not fit this platform's integers.
    #[error(transparent)]
    Int(#[from] std::num::TryFromIntError),

    // Compressed bodies (`pack`).
    /// A Zstandard frame that does not decode.
    #[error("damaged compressed body: {0}")]
    Compressed(String),
    /// A compressed body that expands past [`MAX_BODY`].
    #[error("a compressed body expands to more than {MAX_BODY} bytes")]
    Expands,
    /// No Zstandard decompression context could be made.
    #[error("no decompression context")]
    NoContext,
    /// A stored uncompressed body over [`MAX_BODY`].
    #[error("a stored body is larger than {MAX_BODY} bytes")]
    StoredTooLarge,
    /// A compression flag this build does not know.
    #[error("unknown compression flag {0}")]
    Flag(u8),

    // Rows and values (`schema`).
    /// A recorded chain value that is not 64 hex digits.
    #[error("{0:?} is not a chain value")]
    ChainValue(String),
    /// A `transactions` row shorter than its fixed part.
    #[error("a transaction row of {0} bytes is too short")]
    TxRowShort(usize),
    /// A media type too long to store.
    #[error("a media type is too long")]
    MediaTooLong,
    /// An `assets` row shorter than its media type.
    #[error("an asset row is too short")]
    AssetRowShort,
    /// An `assets` row whose media type is over 255 bytes.
    #[error("an asset's media type is longer than 255 bytes")]
    AssetMediaLong,
    /// An `assets` row whose media type is not UTF-8.
    #[error("an asset's media type is not UTF-8")]
    AssetMediaUtf8,

    // Records (`record`).
    /// An op that did not encode as an object.
    #[error("op did not encode as an object")]
    OpNotObject,
    /// An op that did not encode as a tagged object.
    #[error("op did not encode as a tagged object")]
    OpNotTagged,
    /// A transaction whose `seq` or time a double cannot hold exactly.
    #[error("transaction {0}: seq or time exceeds 2^53 - 1")]
    TxLimits(u64),
    /// An integer field a double cannot hold exactly.
    #[error("integer {0} exceeds 2^53 - 1")]
    Integer(u64),
    /// A value nested deeper than [`MAX_DEPTH`].
    #[error("nesting deeper than {MAX_DEPTH}")]
    Depth,
    /// A string longer than [`MAX_STRING`] bytes.
    #[error("a string longer than {MAX_STRING} bytes")]
    LongString,
    /// An array longer than [`MAX_ARRAY`].
    #[error("an array longer than {MAX_ARRAY}")]
    LongArray,
    /// An object with more than [`MAX_OBJECT`] entries.
    #[error("an object with more than {MAX_OBJECT} entries")]
    BigObject,
    /// An object key longer than [`MAX_STRING`] bytes.
    #[error("a key longer than {MAX_STRING} bytes")]
    LongKey,
    /// An op stored in a version newer than this build reads.
    #[error("op {name} version {version} is newer than this build reads ({OP_VERSION})")]
    OpNewer { name: String, version: u64 },
    /// An op stored in an older version with no upcast.
    #[error("op {name} version {version} has no upcast to version {OP_VERSION}")]
    OpNoUpcast { name: String, version: u64 },
    /// An op of an older version without a field its upcast needs.
    #[error("{0}")]
    OpUpcastShape(&'static str),
    /// A value without canonical form.
    #[error(transparent)]
    Canonical(#[from] CanonicalError),

    // Snapshot chunks (`chunks`).
    /// A chunk's recorded hash that is not a hash.
    #[error("chunk {key}: {hash:?} is not a hash")]
    ChunkHash { key: String, hash: String },
    /// A model chunk key without its bucket number.
    #[error("chunk key {0:?} has no bucket number")]
    ChunkBucket(String),
    /// A `model:` key of no model kind, or past the bucket limit.
    #[error("chunk key {0:?} is not a model chunk")]
    NotModelChunk(String),
    /// A chunk key this build does not know.
    #[error("unknown chunk key {0:?}")]
    ChunkKey(String),
    /// A page chunk key that names no page.
    #[error("chunk key {0:?} names no page")]
    ChunkPage(String),
    /// A model kind with a bucket count that is not a power of two.
    #[error("{kind} has {count} chunks, not a power of two")]
    NotPowerOfTwo { kind: String, count: u32 },
    /// A model kind this build does not know, while building chunks.
    #[error("unknown model kind {0}")]
    ModelKind(String),
    /// A model chunk whose kind the chunk set has no bucket count for.
    #[error("chunk {0}: unknown kind")]
    ChunkKindUnknown(String),
    /// A model chunk of a kind this build does not know.
    #[error("chunk {key}: unknown kind {kind}")]
    ChunkKind { key: String, kind: String },
    /// A chunk whose JSON does not decode.
    #[error("chunk {key}: {error}")]
    ChunkJson { key: String, error: String },
    /// An object in two chunks (`what` is its kind: "page", "item", …).
    #[error("{what} {id} is in two chunks")]
    InTwoChunks { what: &'static str, id: String },
    /// An object in a bucket it does not belong in.
    #[error("{id} does not belong in chunk {key}")]
    WrongBucket { id: String, key: String },
    /// An item or representation in another page's chunk (`what`: "item" or "representation").
    #[error("{what} {id} is in the chunk of page {page} but on another page")]
    OnAnotherPage { what: &'static str, id: String, page: String },
    /// A key that is not a component chunk, where one was asked for.
    #[error("{0} is not a component chunk")]
    NotComponentChunk(String),
    /// A chunk whose body does not match its hash.
    #[error("chunk {0} does not match its hash")]
    ChunkMismatch(String),
    /// A chunk named by a snapshot that is not stored.
    #[error("chunk {0} is missing")]
    ChunkMissing(String),
    /// A worker thread for reading chunks could not start.
    #[error("a chunk could not be read: a worker thread did not start; open the project again")]
    ChunkThread,
    /// A snapshot without its project chunk.
    #[error("the snapshot has no project chunk")]
    NoProjectChunk,
    /// A snapshot whose page order and page chunks differ.
    #[error("the snapshot's page order does not match its page chunks")]
    PageOrder,
    /// The `model:meta` chunk does not decode.
    #[error("chunk {META_KEY}: {0}")]
    MetaChunk(String),

    // Transactions and the history chain (`project`).
    /// A record that does not match its stored chain value.
    #[error("the record does not match its chain value")]
    RecordChain,
    /// A row whose body is not a JSON record.
    #[error("not a record: {0}")]
    NotARecord(String),
    /// A row holding another transaction than its key says.
    #[error("the row of {seq} holds transaction {held:?}")]
    RowHolds { seq: u64, held: Option<u64> },
    /// A record whose `prev` is not the previous row's chain value.
    #[error("prev does not link to the previous transaction")]
    PrevLink,
    /// A record that does not decode (shown as the decoder's own form).
    #[error("{0:?}")]
    Decode(DecodeError),
    /// A row of transaction `seq` that does not decode.
    #[error("transaction {seq}: {error:?}")]
    TxDecode { seq: u64, error: DecodeError },
    /// A row of transaction `seq` whose JSON does not read.
    #[error("transaction {seq}: {error}")]
    TxJson { seq: u64, error: String },
    /// Damage found at a transaction while reading rows.
    #[error("transaction {seq:?}: {detail}")]
    AtTx { seq: Option<u64>, detail: String },
    /// A row that does not match its chain value.
    #[error("transaction {0} does not match its chain value")]
    TxChain(u64),
    /// A row whose `prev` is not a chain value.
    #[error("transaction {0}: bad prev")]
    TxPrev(u64),
    /// A row out of order, or not linked to the one before.
    #[error("transaction {0} does not follow the one before")]
    TxNotFollowing(u64),
    /// A range of rows with gaps.
    #[error("transactions {from} to {to} are not all stored")]
    NotAllStored { from: u64, to: u64 },
    /// A transaction that is not stored.
    #[error("transaction {0} is not stored")]
    TxNotStored(u64),
    /// A step that is not stored.
    #[error("step {0} is not stored")]
    StepNotStored(u64),
    /// An op that does not replay on the state before it.
    #[error("replaying {seq}: {error}")]
    Replay { seq: u64, error: ModelError },

    // Undo and redo stacks.
    /// A stack record with hidden entries and no cap.
    #[error("the stack record has entries below it but no cap")]
    StackNoCap,
    /// A stack record holding more than its cap.
    #[error("the stack record holds more than its cap of {0}")]
    StackOverCap(u64),
    /// A stack entry that is not a transaction ID.
    #[error("{0:?} is not a transaction ID")]
    NotATxId(String),
    /// The history holds fewer undo steps than a snapshot recorded.
    #[error("the history holds fewer undo steps than recorded")]
    FewerUndo,
    /// The undo history does not add up.
    #[error("the undo history is inconsistent")]
    UndoInconsistent,

    // Branches and their lineage.
    /// Branch parents that loop, or go deeper than the limit.
    #[error("the branches' parents form a loop or are too deep")]
    LineageLoop,
    /// A branch row that is not stored (its hex ID).
    #[error("branch {0} is missing")]
    BranchMissing(String),
    /// The file has no `main` branch.
    #[error("the file has no main branch")]
    NoMain,
    /// The `main` branch's row breaks its rules.
    #[error("the main branch's record is inconsistent")]
    MainInconsistent,
    /// A branch other than `main` without a parent or fork point (its name).
    #[error("branch {0:?} has no parent or fork point")]
    NoForkPoint(String),
    /// A lineage index past its end.
    #[error("a lineage index is out of range")]
    LineageIndex,
    /// A lineage with no branch.
    #[error("an empty lineage")]
    EmptyLineage,
    /// A fork point that is not the parent's stored row.
    #[error("the fork point of branch {branch} does not match its parent's transaction {seq}")]
    ForkMismatch { branch: String, seq: u64 },
    /// A merge row without its source branch.
    #[error("a merge row names no source branch")]
    MergeNoSource,
    /// A merge row without its ancestor branch.
    #[error("a merge row names no ancestor branch")]
    MergeNoAncestor,
    /// A merge row whose ancestor has no chain value.
    #[error("a merge row's ancestor has no chain value")]
    MergeAncestorChain,

    // Snapshots, the container facts and saving.
    /// A snapshot whose point is not its stored row.
    #[error("the snapshot's point does not match its transaction")]
    SnapshotPoint,
    /// A snapshot on which the ledger could not be built.
    #[error("the snapshot at {seq}: {error}")]
    SnapshotAt { seq: u64, error: ModelError },
    /// A snapshot interval outside [`SNAPSHOT_EVERY_RANGE`].
    #[error("the snapshot interval must be between {} and {}", SNAPSHOT_EVERY_RANGE.0, SNAPSHOT_EVERY_RANGE.1)]
    SnapshotEvery,
    /// A `meta` row that does not decode.
    #[error("meta {name}: {error}")]
    Meta { name: &'static str, error: String },
    /// `meta.file_id` missing or not a project ID.
    #[error("meta file_id is missing or malformed")]
    FileId,
    /// `meta.cache` with a snapshot interval out of range.
    #[error("meta cache: snapshot interval {0} is out of range")]
    CacheEvery(u64),
    /// A new file written from a ledger that does not start at the project root.
    #[error("a new file needs the whole history in memory")]
    WholeHistory,
    /// A ledger that does not continue the file's head.
    #[error("the project's history (base {base}, head {head}) does not continue the file's head {file}")]
    NotContinuing { base: u64, head: u64, file: u64 },
    /// A revision name longer than a version name may be.
    #[error("a revision name is too long")]
    RevisionName,
}

impl FileError {
    /// A JSON value that did not encode or decode, in serde's words.
    pub fn json(e: &serde_json::Error) -> Self {
        FileError::Json(e.to_string())
    }
}
