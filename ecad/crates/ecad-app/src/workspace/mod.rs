//! The column sets on `ecad-widgets`'s one table: each
//! manager is a [`ecad_widgets::table::TableSource`] over a kept model plus the table's state, never a
//! panel of its own.
//!
//! - [`crate::checks::list`]: Checks (read-mostly): severity, message, object, page (a go-to) and fixes.
//! - [`crate::devices::list`]: Devices: tag and description set here, function, location, symbol, pages, part
//!   and state read.
//! - [`crate::terminal_strips::plan`]: the terminal manager, mirrored (inside | strip | outside).
//! - [`crate::plc::list`]: Lists ▸ PLC: rack chips, tiles, the channel table.
//! - [`crate::wires::list`], [`crate::cables::list`]: the Project ▸ Wires and Cables tabs over the report models
//!   ([`crate::reports::report`]); [`crate::reports::list`]: Project ▸ Reports' list round today's Reports window.
//! - [`frame`]: the Project workspace: the tab strip, the head's
//!   actions, each tab's body and the foot; [`detail`]: its right panel, the selected item.
//!
//! The Project workspace ([`frame`]) places each view; the Project panel's Checks and Devices
//! tabs ([`crate::checks::panel`], [`crate::devices::panel`]) and Reports ▸ Terminals
//! ([`crate::reports::panel`]) call the same views.
//!
//! Each view keeps an update map: a full rebuild only on first show, reload or a feed
//! that rolled over; otherwise only the keys a commit touched are patched.

pub mod detail;
pub mod frame;

use crate::pages::project_panel::Action;

/// The list views (Lists ▸ …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Checks,
    Devices,
    TerminalBlocks,
    /// Lists ▸ PLC (the I/O list).
    Plc,
}

/// What a list view asks the app for.
#[derive(Debug, Clone, PartialEq)]
pub enum ListAction {
    /// A Project panel action (go to a check or symbol, a fix, a device property).
    Panel(Action),
    /// Run this command line (a terminal plan edit); its refusal goes back to the view.
    Run(String),
    /// Show this page, centred on this point.
    GoTo(ecad_model::PageId, ecad_model::Pt),
    /// Say this in the status line (the command line's echo).
    Say(String),
    /// Lists ▸ PLC's own steps, answered by the view ([`crate::plc::list::IoView::handle`]).
    Tile(usize),
    Chip(usize),
    Scope(bool),
    Reveal(crate::plc::list::IoKey),
    /// Show this page at the card's first drawing on it.
    GoToCard(ecad_model::PageId, Option<ecad_model::DeviceId>),
    /// The Check list, filtered to this device.
    ChecksOf(ecad_model::DeviceId),
    /// Place Symbol for a card body, for this free slot.
    PlaceCard(u32),
    /// Add a rack, Import an I/O
    /// list, Export tags, the path row's OK, and unfold the free channels.
    AddRack,
    ImportTags,
    ExportTags,
    PathOk,
    Unfold,
}

/// The views' state, for the shell's slots.
#[derive(Debug)]
pub struct Lists<'a> {
    pub checks: &'a mut crate::checks::panel::ChecksPanel,
    pub devices: &'a mut crate::devices::panel::DevicesPanel,
    pub connection_points: &'a mut crate::terminal_strips::plan::TerminalStripPlanView,
    pub io: &'a mut crate::plc::list::IoView,
}

/// Draw list view `tab` (one call per view). `writable`: the
/// project can be written (fixes and edits are offered).
pub fn show(ui: &mut eframe::egui::Ui, tab: Tab, lists: Lists<'_>, writable: bool) -> Vec<ListAction> {
    match tab {
        Tab::Checks => lists.checks.show(ui, writable).into_iter().map(ListAction::Panel).collect(),
        Tab::Devices => lists.devices.show(ui, writable).into_iter().map(ListAction::Panel).collect(),
        Tab::TerminalBlocks => lists.connection_points.show(ui, writable),
        Tab::Plc => lists.io.show(ui).into_iter().filter_map(|a| lists.io.handle(a)).collect(),
    }
}

/// Copy `csv` to the clipboard (Checks and Devices until the shell gives Lists a save path).
pub(crate) fn copy_csv(ui: &eframe::egui::Ui, csv: String) {
    ui.ctx().copy_text(csv);
}
