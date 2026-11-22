//! ecad-engine
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
//!   every draw, never stored ([`cables::markers_on_page`] is the one source of that text).
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
pub mod draw;
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
pub mod wires;
// Design 065 §3, cut 12: the agent gate, shared by ecad-mcp and the window's live bridge.
/// A builder for the small projects tests start from (brief 062); enabled only by tests.

// The electrical rules live in `ecad-electrical` (design 065 §5 step 6). Each module stays reachable under
// its old name; a module with an engine half in `draw` is a facade over both halves.
/// `ecad_electrical::checks` and its engine half, [`draw::checks`].
pub mod checks {
    pub use crate::checks::{all, all_with, handles_of};
    pub use ecad_electrical::checks::*;
}
/// `ecad_electrical::continuations` and its engine half, [`draw::continuations`].
pub mod continuations {
    pub use crate::continuations::draw::*;
    pub use ecad_electrical::continuations::*;
}
/// `ecad_electrical::devices::device_list` and its engine half, [`draw::device_list`].
pub mod device_list {
    pub use crate::reports::device_list::{DeviceList, symbol_target};
    pub use ecad_electrical::devices::device_list::*;
}
/// `ecad_electrical::devices` and its engine half, [`draw::devices`].
pub mod devices {
    pub use crate::devices::draw::*;
    pub use ecad_electrical::devices::*;
}
/// `ecad_electrical::pages::front` and its engine half, [`draw::front`].
pub mod front {
    pub use crate::pages::front::cover;
    pub use ecad_electrical::pages::front::*;
}
/// `ecad_electrical::pages::generated` and its engine half, [`draw::generated`].
pub mod generated {
    pub use crate::pages::generated::{Draw, input_of, missing_mark, page_parts, parts_of, parts_of_pending};
    pub use ecad_electrical::pages::generated::*;
}
/// `ecad_electrical::locations` and its engine half, [`draw::locations`].
pub mod locations {
    pub use crate::locations::draw::*;
    pub use ecad_electrical::locations::*;
}
/// `ecad_electrical::reports`.
pub mod outputs {
    pub use ecad_electrical::reports::*;
}
/// `ecad_electrical::plc` and its engine half, [`draw::plc`].
pub mod plc {
    pub use crate::plc::draw::Plc;
    pub use ecad_electrical::plc::*;
    /// `ecad_electrical::plc::draw` and its engine half, [`crate::plc::draw::card`].
    pub mod look {
        pub use crate::plc::draw::card::parts_on_page;
        pub use ecad_electrical::plc::draw::*;
    }
}
/// `ecad_electrical::references::cables::cables` and its engine half, [`draw::references`].
pub mod references {
    pub use crate::references::draw::cables::reference_at;
    pub use ecad_electrical::references::cables::cables::*;
}
/// `ecad_electrical::styles::text_style` and its engine half, [`draw::text_style`].
pub mod text_style {
    pub use crate::styles::draw::*;
    pub use ecad_electrical::styles::text_style::*;
}
/// `ecad_electrical::title_blocks` and its engine half, [`draw::title_block`].
pub mod title_block {
    pub use crate::title_blocks::{dxf, layout};
    pub use ecad_electrical::title_blocks::*;
}
/// `ecad_electrical::wires` and its engine half, [`draw::wires`].
pub mod wires {
    pub use crate::wires::draw::*;
    pub use ecad_electrical::wires::*;
}
/// `ecad_electrical::references::cables::devices` and its engine half, [`draw::xref`].
pub mod xref {
    pub use crate::references::draw::devices::*;
    pub use ecad_electrical::references::cables::devices::*;
}

// The commands' paths before step 11 of lane 067, used outside the crate.

// Brief 035: a per-user settings file and a window's port. Its
// path before cut 12.
pub use crate::access::{BUSY, NOT_ATTACHED, PagesRef, ProjectHandle};
pub use crate::file::{FileStatus, HOME_UNKNOWN, NO_FILE_YET, SAVE_AS_NEEDS_PATH, expand_home, home, normalize, normalize_with, same_path};
pub use crate::project::{
    Change, Faults, HandleMap, Mapped, PageRow, PageView, Persistence, Project, SAVE_UNAVAILABLE, TreeRow, View, project_page, project_page_framed,
    project_page_with, session_seed, tree_rows, writer,
};

/// The incorporated CADCraft commit (see `ecad/UPSTREAM.md`).
pub const UPSTREAM_COMMIT: &str = "227e3c385b20129cf525775e3d9cbe78b722b3bb";

/// Install the ECAD commands into the engine's registry. Call once, before the engine's command
/// list is first used. Returns `false` if it was too late or already done.
pub fn install() -> bool {
    // Brief 040: the sheet font for every page drawn or printed, in the window or headless.
    draw::register_sheet_font();
    cadcraft_engine::cmd::set_extension_commands(cmd::specs)
}

/// Fault injection for the experiment (`Faults::panic_in_commit`): a deliberate panic inside an
/// owner callback, to prove the engine's guard and the owner's recovery. Compiled only with the
/// `fault-injection` feature, so normal builds contain no panic path.
#[cfg(feature = "fault-injection")]
#[allow(clippy::panic)]
pub(crate) fn fault_panic() -> ! {
    panic!("injected owner failure (experiment fault injection)")
}
