//! ecad-app: the ECAD application. The program that opens its window, parses the command line and
//! times start-up is `ecad-desktop` (design 065 §2.2).
//!
//! It is CADCraft's own window ([`CadApp`]) with the ECAD commands installed and one ECAD project
//! open at a time. Everything ECAD-specific runs through `ecad-engine`; this crate only adds
//! what a person needs to try it:
//!
//! - the **Project** panel on the far left ([`project_panel`], brief 016): the project's pages,
//!   one click to make a page the one active page, new / rename / reorder through the page
//!   commands, and in its header the project's status (name, file, unsaved changes, read-only
//!   and unconfirmed-save warnings), New, Open, Save, Save As and Close, and the experiment note
//!   (they were in a top banner until owner, 2026-11-08);
//! - a read-only **preview** of another page ([`preview`], brief 016), floating over the drawing;
//! - **project files** (`.ecad`, persistence stage 2a): Save writes to the project's file, Save As
//!   (and the first Save) asks for a file name in a small dialog of its own (the app has no
//!   native file dialogs), Open opens a project file; a project another session is editing, or a
//!   damaged one, opens read-only and can be saved as another file;
//! - one page window: closing its tab leaves the project open with no page shown (a click in the
//!   panel shows a page again); closing the project (the panel's Close, or quitting) with unsaved
//!   changes asks **Save**, **Discard** or **Cancel** (it replaces CADCraft's prompt for ECAD
//!   pages, through `CadApp`'s public close API; no upstream change);
//! - optional export of inspector snapshots (`--snapshots <dir>`), which are diagnostic files, not
//!   saved projects;
//! - an **ECAD** tab in CADCraft's Tool Sets panel ([`tools`]): buttons start the click-based
//!   forms of the ECAD commands (brief 008), run a command, or open the ECAD command field with a
//!   template; and an **ECAD Dev** tab beside it: the debugger, snapshot export and the ECAD
//!   command window;
//! - terminal and wire-end marks over ECAD pages (drawn by CADCraft from the project's overlay),
//!   and a hover tooltip naming the ECAD object under the pointer and its connections
//!   ([`App::hover_lines`]), read only.
//! - the read-only **debugger** panel (persistence stage 2b, [`debugger`]), opened from ECAD Dev
//!   and never shown by default: selected drawing objects → ECAD IDs → model objects →
//!   transactions with before/after values → saved record offsets and bytes, with pending
//!   changes kept apart from durably saved history.
//!
//! - **live agent access** ([`agent`], brief 035): off by default; when the person turns it on
//!   (ECAD Dev ▸ Agent access, or `--control PORT` for one run), an agent connected through
//!   `ecad-mcp --connect` changes the open project between frames, recorded as the agent's, while
//!   the person watches; a minimal banner names it and offers Undo session, Pause and Disconnect.
//!
//! Excluded on purpose: native file dialogs, CADCraft file formats (plain drawings cannot be
//! written), autosave, CADCraft's raw control channel, saved window or profile state and ECAD menus
//! (CADCraft shows no top-level menu supplied by an embedder). The JSON forms of ECAD commands are
//! typed in the ECAD command field, for example `wire {"points":[[0,0],[40,0]]}` (CADCraft's
//! own command line takes a space as Enter, so it cannot take a JSON argument with spaces); a bare
//! command name starts its interactive form in either.
//!
//! Project files belong in a local folder: by default `C:\dev\ecad-projects` on Windows (never the
//! OneDrive-redirected Documents folder) and `~/ecad-projects` on Linux, including the Linux home
//! under WSL. Network, synced and removable folders, and Windows drives under `/mnt`, are
//! unverified.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod agent;
pub mod cables;
pub mod canvas;
pub mod checks;
pub mod commands;
pub mod developer;
pub mod devices;
pub mod history;
pub mod inspector;
pub mod library;
pub mod pages;
pub mod plc;
pub mod publish;
pub mod reports;
pub mod settings;
pub mod shell;
pub mod terminal_strips;
pub mod title_blocks;
pub mod wires;
pub mod workspace;
