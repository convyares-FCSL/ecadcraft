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
// Design 065 §3, cut 12: the agent gate, shared by ecad-mcp and the window's live bridge.
pub mod agent;
pub mod background;
pub mod cad;
pub mod cmd;
pub mod convert;
pub mod debug;
pub mod draw;
pub mod file;
pub mod frame;
pub mod history;
pub mod interact;
mod lanes;
pub mod look;
pub mod merging;
pub mod oops;
pub mod preview;
pub mod printbook;
pub mod project;
pub mod props;
pub mod release;
pub mod strip_page;
/// A builder for the small projects tests start from (brief 062); enabled only by tests.
#[cfg(feature = "test-support")]
pub mod test_support;
mod translate;
pub mod verify;
pub mod views;

// The electrical rules live in `ecad-electrical` (design 065 §5 step 6). Each module stays reachable under
// its old name; a module with an engine half in `draw` is a facade over both halves.
pub use ecad_electrical::accessories;
pub use ecad_electrical::bom;
pub use ecad_electrical::cables;
pub use ecad_electrical::connectors;
pub use ecad_electrical::defaults;
pub use ecad_electrical::derived;
pub use ecad_electrical::designations;
pub use ecad_electrical::frames;
pub use ecad_electrical::kinds;
pub use ecad_electrical::link_lines;
pub use ecad_electrical::page_types;
pub use ecad_electrical::proposals;
pub use ecad_electrical::rating;
pub use ecad_electrical::reps;
pub use ecad_electrical::revisions;
pub use ecad_electrical::slash;
pub use ecad_electrical::strips;
pub use ecad_electrical::styles;
pub use ecad_electrical::symbols;
pub use ecad_electrical::terminal_types;
pub use ecad_electrical::words;
/// `ecad_electrical::checks` and its engine half, [`draw::checks`].
pub mod checks {
    pub use crate::draw::checks::{all, all_with, handles_of};
    pub use ecad_electrical::checks::*;
}
/// `ecad_electrical::continuations` and its engine half, [`draw::continuations`].
pub mod continuations {
    pub use crate::draw::continuations::*;
    pub use ecad_electrical::continuations::*;
}
/// `ecad_electrical::device_list` and its engine half, [`draw::device_list`].
pub mod device_list {
    pub use crate::draw::device_list::{DeviceList, symbol_target};
    pub use ecad_electrical::device_list::*;
}
/// `ecad_electrical::devices` and its engine half, [`draw::devices`].
pub mod devices {
    pub use crate::draw::devices::*;
    pub use ecad_electrical::devices::*;
}
/// `ecad_electrical::front` and its engine half, [`draw::front`].
pub mod front {
    pub use crate::draw::front::cover;
    pub use ecad_electrical::front::*;
}
/// `ecad_electrical::generated` and its engine half, [`draw::generated`].
pub mod generated {
    pub use crate::draw::generated::{Draw, input_of, missing_mark, page_parts, parts_of, parts_of_pending};
    pub use ecad_electrical::generated::*;
}
/// `ecad_electrical::locations` and its engine half, [`draw::locations`].
pub mod locations {
    pub use crate::draw::locations::*;
    pub use ecad_electrical::locations::*;
}
/// `ecad_electrical::outputs`.
pub mod outputs {
    pub use ecad_electrical::outputs::*;
}
/// `ecad_electrical::plc` and its engine half, [`draw::plc`].
pub mod plc {
    pub use crate::draw::plc::Plc;
    pub use ecad_electrical::plc::*;
    /// `ecad_electrical::plc::look` and its engine half, [`crate::draw::plc::look`].
    pub mod look {
        pub use crate::draw::plc::look::parts_on_page;
        pub use ecad_electrical::plc::look::*;
    }
}
/// `ecad_electrical::references` and its engine half, [`draw::references`].
pub mod references {
    pub use crate::draw::references::reference_at;
    pub use ecad_electrical::references::*;
}
/// `ecad_electrical::text_style` and its engine half, [`draw::text_style`].
pub mod text_style {
    pub use crate::draw::text_style::*;
    pub use ecad_electrical::text_style::*;
}
/// `ecad_electrical::title_block` and its engine half, [`draw::title_block`].
pub mod title_block {
    pub use crate::draw::title_block::{dxf, layout};
    pub use ecad_electrical::title_block::*;
}
/// `ecad_electrical::wires` and its engine half, [`draw::wires`].
pub mod wires {
    pub use crate::draw::wires::*;
    pub use ecad_electrical::wires::*;
}
/// `ecad_electrical::xref` and its engine half, [`draw::xref`].
pub mod xref {
    pub use crate::draw::xref::*;
    pub use ecad_electrical::xref::*;
}

// The commands' paths before step 11 of lane 067, used outside the crate.

// Brief 035: a per-user settings file and a window's port; nothing of it on the web build. Its
// path before cut 12.
pub use access::{BUSY, NOT_ATTACHED, PagesRef, ProjectHandle};
#[cfg(not(target_arch = "wasm32"))]
pub use file::{FileStatus, HOME_UNKNOWN, NO_FILE_YET, SAVE_AS_NEEDS_PATH, expand_home, home, normalize, normalize_with, same_path};
pub use project::{
    Change, Faults, HandleMap, Mapped, PageRow, PageView, Persistence, Project, SAVE_UNAVAILABLE, TreeRow, View, project_page, project_page_framed,
    project_page_with, session_seed, tree_rows, writer,
};

/// The incorporated CADCraft commit (see `ecad/UPSTREAM.md`).
pub const UPSTREAM_COMMIT: &str = "227e3c385b20129cf525775e3d9cbe78b722b3bb";

/// Install the ECAD commands into the engine's registry. Call once, before the engine's command
/// list is first used. Returns `false` if it was too late or already done.
pub fn install() -> bool {
    // Brief 040: the sheet font for every page drawn or printed, in the window or headless.
    text_style::register_sheet_font();
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
