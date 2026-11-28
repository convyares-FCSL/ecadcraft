//! ecad-file: ECAD project files (`.ecad`), one format: a redb database.
//!
//! - **Records** ([`records`], [`record`]): the transaction records every file stores, at the schema version
//!   [`record::SCHEMA_VERSION`], separate from `ecad-model` and converted explicitly. The exceptions are stored
//!   as the model writes them: a device's type (`DeviceType`, with its ratings), tag (`TagSlot`), location and
//!   function (`Place`) and user values (`UserValue`), whose words and fields are strict already; a second copy
//!   of them would only have to be kept in step. Ops are written flat, `{"op": name, "v": n, ...}`; any op
//!   version but the current one is refused (no upcasts before v1).
//! - **What the schema version pins**: two hashes of the golden fixture (`tests/all/common`), checked by
//!   `the_schema_version_is_bumped_with_every_format_change`: the head of its transaction records (record
//!   fields, op names and versions, the canonical form, the chain), and its snapshot (every chunk key and the
//!   hash of every chunk's bytes; chunks are serde's form of the model types, see [`chunks`]). A change to
//!   either fails that test until [`record::SCHEMA_VERSION`] is bumped. The fixture holds one device of each
//!   type with every field written out, so a field added to a device type does not compile until the fixture
//!   names it. Only what the fixture holds is pinned: a model field it leaves at a value serde skips can change
//!   shape unseen.
//! - **Integrity** ([`canonical`], [`chain`]): RFC 8785 canonical form (the bytes stored and hashed), and an
//!   unkeyed SHA-256 history chain with domain-separated seeds for projects and libraries. It detects
//!   accidental damage, reordering and divergence; it does not resist deliberate rewriting.
//! - **The container**: [`schema`] (the tables, keys and records), [`store`] (the database behind ECAD's
//!   append-only API), [`pack`] (Zstandard compression of rows and chunks), [`chunks`] (snapshot chunks),
//!   [`project`] (opening from the latest snapshot, saving, branches), [`verify`] (Verify) and [`disk`] (files
//!   on disk with the writer lock). The format version is the number the file records
//!   (`meta.container_version`), not a module name. Opening the database, its transactions and reopening after
//!   an I/O error are `ecad_store::db`'s generic handle; this crate defines the tables and reads and writes
//!   them.
//! - **History** ([`history`]): the version and branch graph the History tab shows (branch and graph rows,
//!   lanes, named points, session groups), built from the rows above.
//! - **Binding** ([`binding`]): a project bound to its file (open, Save, Save As, status) and the checks on a
//!   path typed for one; the caller passes the home folder.
//!
//! Not here: library files and checkpoints (neither exists yet; a checkpoint record is refused), and other
//! codecs.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
// Every byte read here may come from a hostile file: no indexing or slicing that can panic.
#![deny(clippy::indexing_slicing)]

pub mod binding;
pub mod canonical;
pub mod chain;
pub mod chunks;
mod crc;
pub mod disk;
mod error;
pub mod history;
pub mod pack;
pub mod project;
pub mod record;
pub mod records;
pub mod schema;
pub mod store;
pub mod verify;

pub use crc::crc32c;
pub use disk::{JUST_ENDED, ReadOnly};
pub use error::FileError;
pub use project::{BranchError, Damage, OpenError, Opened, Project, Report, SaveError, Saved, StoreHistory};
pub use record::{SCHEMA_VERSION, definition_hash};
pub use store::{Reader, Store, StoreError, TableStat, Writer};
