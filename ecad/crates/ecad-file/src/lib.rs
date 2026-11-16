//! ecad-file
//!
//! ECAD project files (`.ecad`): one format, a redb database (EEP-0005; architecture proposal,
//! Part E).
//!
//! - **Records** ([`records`], [`record`]): the transaction records every file stores, schema
//!   version 1, separate from `ecad-model` and converted explicitly, so a model change never
//!   silently changes the format. Ops are written flat, `{"op": name, "v": n, …}`, and older op
//!   versions are upcast after integrity checks.
//! - **Integrity** ([`canonical`], [`chain`]): RFC 8785 canonical form (the bytes stored and
//!   hashed), and an unkeyed SHA-256 history chain with domain-separated seeds for projects and
//!   libraries. It detects accidental damage, reordering and divergence; it does not resist
//!   deliberate rewriting.
//! - **The container** (EEP-0005): [`schema`] (the tables, keys and records), [`store`] (the
//!   database behind ECAD's append-only API), [`pack`] (Zstandard compression of rows and
//!   chunks), [`chunks`] (snapshot chunks), [`project`] (opening from the latest snapshot, saving,
//!   branches), [`verify`] (Verify) and [`disk`] (files on disk with the writer lock). The format
//!   version is the number the file records (`meta.container_version`), not a module name.
//! - **Binding** ([`binding`]): a project bound to its file (open, Save, Save As, status) and
//!   the checks on a path typed for one; the caller passes the home folder.
//!
//! Excluded until later stages: library files (stage 3), checkpoints (disabled; their trust
//! contract is unresolved), and other codecs.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod binding;
pub mod canonical;
pub mod chain;
pub mod chunks;
mod crc;
#[cfg(not(target_arch = "wasm32"))]
pub mod disk;
pub mod pack;
pub mod project;
pub mod record;
pub mod records;
pub mod schema;
pub mod store;
pub mod verify;

pub use crc::crc32c;
#[cfg(not(target_arch = "wasm32"))]
pub use disk::{JUST_ENDED, ReadOnly, utc};
pub use project::{BranchError, DamageV2, OpenErrorV2, OpenedV2, ProjectV2, ReportV2, SaveErrorV2, SavedV2, StoreHistory};
pub use record::{SCHEMA_VERSION, definition_hash};
pub use store::{Reader, Store, StoreError, TableStat, Writer};
