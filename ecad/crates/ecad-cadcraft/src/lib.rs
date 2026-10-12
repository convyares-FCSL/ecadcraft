//! ecad-cadcraft
//!
//! The adapter between the authoritative ECAD document ([`ecad_model`]) and the CADCraft engine.
//! It is the only ECAD crate that depends on CADCraft (principle P10), and everything in it is
//! **provisional** (proposal Part B): it exists to prove or disprove hypotheses H1-H5 in the
//! bounded integration experiment (§17).
//!
//! - [`project::Project`] is a CADCraft document owner (`cadcraft_engine::ext`): it projects each
//!   page into an engine drawing, keeps the session-only handle map, translates CADCraft commits
//!   into transactions ([`translate`]), owns undo/redo/save, and restores consistency by
//!   reprojecting after a failure (PROJ-02).
//! - [`commands`] adds the ECAD commands (JSON forms) to the engine's command registry.
//!
//! This crate intentionally contains **no** UI, no file encoding (the store is test-only) and
//! no ECAD semantics inside upstream crates.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod commands;
pub mod convert;
pub mod project;
mod translate;

pub use project::{Faults, HandleMap, Mapped, PageView, Project, project_page, session_seed};

/// The incorporated CADCraft commit (see `ecad/UPSTREAM.md`).
pub const UPSTREAM_COMMIT: &str = "00c7613f1af3c73f7157fa91d9fe2635c6f23c06";

/// Install the ECAD commands into the engine's registry. Call once, before the engine's command
/// list is first used. Returns `false` if it was too late or already done.
pub fn install() -> bool {
    cadcraft_engine::cmd::set_extension_commands(commands::specs)
}

/// Fault injection for the experiment (`Faults::panic_in_commit`): a deliberate panic inside an
/// owner callback, to prove the engine's guard and the owner's recovery.
#[allow(clippy::panic)]
pub(crate) fn fault_panic() -> ! {
    panic!("injected owner failure (experiment fault injection)")
}
