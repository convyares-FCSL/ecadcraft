//! ecad-file
//!
//! ECAD project files (`.ecad`) as specified in the architecture proposal, Part E, stage 1:
//!
//! - **Semantic records** ([`v1`], [`record`]): schema version 1, separate from `ecad-model` and
//!   converted explicitly, so a model change never silently changes the format. Ops are written
//!   flat, `{"op": name, "v": n, …}`, and older op versions are upcast after integrity checks.
//! - **Codec** ([`codec`]): JSON Lines, one framed record per line,
//!   `{"crc":"<8 hex>","rec":<payload>}`, with CRC-32C over exactly the payload bytes.
//! - **Integrity** ([`canonical`], [`chain`]): RFC 8785 canonical form (also the bytes written),
//!   and an unkeyed SHA-256 history chain with domain-separated seeds for projects and
//!   libraries. It detects accidental damage, reordering and divergence; it does not resist
//!   deliberate rewriting or detect truncation at a record boundary.
//! - **Reading** ([`read`]): the verified prefix, and a report classifying what follows it:
//!   clean, an incomplete trailing write (writable), or a complete corrupt record at any position
//!   (read-only, source untouched).
//! - **Writing** ([`write`]): first write and Save with retry over an `ecad_store::Medium`,
//!   distinguishing our own failed bytes from external changes.
//! - **Files on disk** ([`disk`], native only): open with the writer lock, Save, and Save As by
//!   temporary file, rename and folder flush; interrupted trailing bytes preserved in a sidecar.
//!
//! Excluded until later stages: library files beyond their header and chain seed (stage 3), checkpoints (disabled; their
//! trust contract is unresolved), and other codecs.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod canonical;
pub mod chain;
pub mod codec;
mod crc;
#[cfg(not(target_arch = "wasm32"))]
pub mod disk;
pub mod read;
pub mod record;
pub mod v1;
pub mod write;

pub use crc::crc32c;
#[cfg(not(target_arch = "wasm32"))]
pub use disk::{DiskError, DiskProject, Durability, OpenedDisk, ReadOnly, StepFaults};
pub use read::{Access, Check, Corruption, OpenError, OpenReport, Opened, Outcome, RecordSpan, open_project};
pub use record::{CODEC, CONTAINER_VERSION, Header, SCHEMA_VERSION, definition_hash};
pub use write::{
    Attempt, AttemptStage, AttemptSummary, ProjectFile, SaveError, SaveOutcome, WriterState, WriterSummary,
};
