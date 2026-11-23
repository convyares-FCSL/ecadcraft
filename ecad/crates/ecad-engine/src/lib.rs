//! ecad-engine: the link between an ECAD project and CADCraft's drawing engine.
//!
//! It draws each page from the model, turns CADCraft edits into ECAD transactions, owns undo, redo and save, and
//! runs the ECAD commands. It is the only ECAD crate that depends on CADCraft; the rules themselves live in
//! `ecad-electrical`, and the engine holds only what needs the session (drawing, cursor, selection, entities).
//!
//! The project and its boundary:
//! - [`project`]: [`Project`], the CADCraft document owner: page drawings, the handle map, undo, redo and save,
//!   and redrawing every page after a failed commit so the drawing always matches the document.
//! - [`ProjectHandle`] (the `access` module): the only way to reach a project from outside, under its lock.
//! - [`cad`]: the CADCraft types and functions the app and the programs may name.
//! - [`commands`]: the command registry and the shared helpers of session commands.
//! - [`interact`]: click-based forms of the commands, the marks drawn over pages and the hover text.
//! - [`pointer`]: what is under the cursor and the running tool's intent.
//! - [`convert`] and each feature's `draw` module: building CADCraft entities, text styles and fonts from
//!   what the rules decide.
//! - [`file`]: the project's `.ecad` file on disk, the home folder and path rules.
//! - [`history`]: the History tab: versions, points, restore, branches and merging.
//! - [`agent`]: what an agent may run, its caps, its author name and its read tools.
//! - [`background`], [`warm`]: work off the UI thread, and the first builds of the kept views.
//! - [`export`]: export formats (Print Book PDF).
//! - [`dev`]: the debugger, verify, the snapshot and test support.
//!
//! Feature areas, each the engine half of the same area in `ecad-electrical` (its commands, drawing and click
//! forms): [`cables`], [`checks`], [`connectors`], [`continuations`], [`devices`], [`locations`], [`pages`],
//! [`plc`], [`properties`], [`references`], [`releases`], [`reports`], [`styles`], [`symbols`],
//! [`terminal_strips`], [`title_blocks`], [`wires`].
//!
//! This crate contains **no** UI, no file encoding of its own (that is `ecad-file`) and no ECAD meaning inside
//! upstream crates.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod access;
pub mod agent;
pub mod background;
pub mod cables;
pub mod cad;
pub mod checks;
pub mod commands;
pub mod connectors;
pub mod continuations;
pub mod convert;
pub mod dev;
pub mod devices;
pub mod export;
pub mod file;
pub(crate) mod generic;
pub mod history;
pub mod interact;
pub mod locations;
pub mod pages;
pub mod plc;
pub mod pointer;
pub mod project;
pub mod properties;
pub mod references;
pub mod releases;
pub mod reports;
pub mod styles;
pub mod symbols;
pub mod terminal_strips;
pub mod title_blocks;
mod translate;
pub mod warm;
pub mod wires;
pub use crate::access::{BUSY, NOT_ATTACHED, PagesRef, ProjectHandle};
pub use crate::file::{SaveAsError, SaveAsRefused, ensure_default_folder, home, normalize, normalize_resolved, save_project_as, without_extension};
pub use crate::project::{
    Change, Faults, HandleMap, Mapped, PageRow, PageView, Persistence, Project, SAVE_UNAVAILABLE, TreeRow, View, project_page, project_page_with,
    session_seed, tree_rows, writer,
};

/// The incorporated CADCraft commit (see `ecad/UPSTREAM.md`).
pub const UPSTREAM_COMMIT: &str = "227e3c385b20129cf525775e3d9cbe78b722b3bb";

/// Install the ECAD commands into the engine's registry. Call once, before the engine's command
/// list is first used. Returns `false` if it was too late or already done.
pub fn install() -> bool {
    // The sheet font for every page drawn or printed, in the window or headless.
    styles::draw::register_sheet_font();
    cadcraft_engine::cmd::set_extension_commands(commands::specs)
}

/// Fault injection (`Faults::panic_in_commit`): a deliberate panic inside a document owner callback,
/// to prove the engine's guard and the project's recovery. Compiled only with the
/// `fault-injection` feature, so normal builds contain no panic path.
#[cfg(feature = "fault-injection")]
#[allow(clippy::panic)]
pub(crate) fn fault_panic() -> ! {
    panic!("injected owner failure (experiment fault injection)")
}
