//! ecad-app: the ECAD user interface on a CADCraft window.
//!
//! Screens, panels, lists, editors and UI state. It turns what a person does into commands and shows their
//! results; everything about a project goes through `ecad-engine`. No rules, no disk, no OS access and no
//! parsing of domain values: the program that opens the window and supplies the OS is `ecad-desktop`. One
//! folder per screen area:
//!
//! - [`shell`]: the frame and the [`shell::app::App`] struct: top bar, panels, status bar, tool strip, keys,
//!   layout, file dialogs, the close flow, and what the program supplies ([`shell::host`]).
//! - [`commands`]: the action table, what the app runs, and the Tool Sets tab.
//! - [`canvas`]: the page canvas: the sheet, its surround and grid, and the marks over the drawing.
//! - [`inspector`]: the right panel.
//! - [`pages`]: the Project panel's page list and page previews.
//! - [`workspace`]: the workspaces and their tabs.
//! - [`devices`], [`cables`], [`wires`], [`terminal_strips`], [`plc`], [`references`]: the lists and editors
//!   of each electrical area.
//! - [`checks`], [`reports`], [`publish`]: the Checks list, the reports, and printing and export.
//! - [`title_blocks`], [`library`]: the title-block editor and the symbol library.
//! - [`history`]: the History tab, merge and the Release dialog.
//! - [`settings`]: project and app settings.
//! - [`agent`]: live agent access (off by default) and its banner.
//! - [`developer`]: the Developer section and the read-only debugger.
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
pub mod references;
pub mod reports;
pub mod settings;
pub mod shell;
pub mod terminal_strips;
pub mod title_blocks;
pub mod wires;
pub mod workspace;
