//! Container version 2 (EEP-0005, brief 021): a project file is one **redb** database.
//!
//! - [`schema`]: the tables, keys and records (EEP-0005 §3).
//! - [`store`]: the database behind ECAD's append-only API.
//! - [`pack`]: Zstandard compression of rows and parts (EEP-0005 §4).
//! - [`parts`]: snapshot parts built from and assembled into a document (EEP-0005 §3.5).
//! - [`project`]: opening from the latest snapshot and saving (EEP-0005 §5–§6).
//!
//! Excluded in this brief: creating, switching and merging branches (the tables and fork
//! records are in place), per-page lazy loading (phase 2), user-triggered history compaction,
//! and the `bin/1` codec.

#[cfg(not(target_arch = "wasm32"))]
pub mod convert;
#[cfg(not(target_arch = "wasm32"))]
pub mod disk;
pub mod pack;
pub mod parts;
pub mod project;
pub mod schema;
pub mod store;

pub use project::{DamageV2, OpenErrorV2, OpenedV2, ProjectV2, ReportV2, SaveErrorV2, SavedV2, StoreHistory};
pub use store::{Reader, Store, StoreError, TableStat, Writer};
