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
//! - [`ProjectHandle`] is the only way to reach a project from outside the crate. It enforces the
//!   lock and re-entry contract (see the `access` module): nothing can call the session while it
//!   holds the project.
//! - [`commands`] adds the ECAD commands (JSON forms) to the engine's command registry, and
//!   [`interact`] their click-based interactive forms, the terminal and wire-end marks drawn over
//!   pages, and the hover description. [`slash`] decides what a cable slash crosses (brief 004).
//! - [`views`] holds each cable's traditional / fan view (brief 005): presentation only, drawn by
//!   the projection, remembered per user outside the `.ecad` file.
//! - [`references`] holds cable references across pages (brief 010): a cable drawn on several
//!   pages ends at reference ends whose text (`/<page>.<column>` of the other end) is derived on
//!   every draw, never stored ([`references::markers_on_page`] is the one source of that text).
//!
//! - [`file`] binds a project to its `.ecad` file on disk (through `ecad-file`), holding the
//!   writer lock while it is open.
//! - [`debug`] copies what the developer debugger shows out of a project under one short lock,
//!   and runs its explicit checks (replay, projection, file) on copies.
//!
//! This crate intentionally contains **no** UI, no file encoding of its own (that is `ecad-file`)
//! and no ECAD semantics inside upstream crates.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod access;
pub mod checks;
pub mod commands;
pub mod convert;
pub mod debug;
mod device_cmds;
pub mod device_list;
pub mod devices;
pub mod file;
pub mod frame;
pub mod history;
pub mod interact;
pub mod look;
pub mod merging;
pub mod oops;
pub mod preview;
pub mod project;
pub mod props;
pub mod references;
pub mod revisions;
pub mod sheet_cmds;
pub mod slash;
pub mod symbols;
mod translate;
/// The pre-brief-014 translation, compared with the current one on every edit (test oracle).
#[cfg(feature = "translate-oracle")]
pub mod translate_oracle;
pub mod verify;
pub mod views;
mod wire_cmds;
pub mod wires;

pub use access::{BUSY, NOT_ATTACHED, PagesRef, ProjectHandle};
pub use file::{FileStatus, HOME_UNKNOWN, NO_FILE_YET, SAVE_AS_NEEDS_PATH, expand_home, home, normalize, normalize_with, same_path};
pub use project::{
    Change, Faults, HandleMap, Mapped, PageRow, PageView, Persistence, Project, SAVE_UNAVAILABLE, TreeRow, View, project_page, project_page_framed,
    project_page_with, session_seed, tree_rows, writer,
};

/// The incorporated CADCraft commit (see `ecad/UPSTREAM.md`).
pub const UPSTREAM_COMMIT: &str = "00c7613f1af3c73f7157fa91d9fe2635c6f23c06";

/// Install the ECAD commands into the engine's registry. Call once, before the engine's command
/// list is first used. Returns `false` if it was too late or already done.
pub fn install() -> bool {
    cadcraft_engine::cmd::set_extension_commands(commands::specs)
}

/// Fault injection for the experiment (`Faults::panic_in_commit`): a deliberate panic inside an
/// owner callback, to prove the engine's guard and the owner's recovery. Compiled only with the
/// `fault-injection` feature, so normal builds contain no panic path.
#[cfg(feature = "fault-injection")]
#[allow(clippy::panic)]
pub(crate) fn fault_panic() -> ! {
    panic!("injected owner failure (experiment fault injection)")
}
