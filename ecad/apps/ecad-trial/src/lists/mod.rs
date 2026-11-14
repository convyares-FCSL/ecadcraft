//! The column sets on `ecad-ui`'s one table (brief 039, design `ui-architecture.md` §6.3): each
//! manager is a [`ecad_ui::table::TableSource`] over a kept model plus the table's state, never a
//! panel of its own.
//!
//! - [`checks`]: Checks (read-mostly): severity, message, object, page (a go-to) and fixes.
//! - [`devices`]: Devices: tag and description set here, function, location, symbol, pages, part
//!   and state read.
//! - [`terminals`]: the terminal manager, mirrored (inside | strip | outside) on 036's row model.
//! - [`plc`]: Lists ▸ PLC (brief 051; 045c's I/O list): rack chips, tiles, the channel table.
//!
//! The shell (U2) places each view through [`show`], one call per tab. Until it lands, today's
//! hosts call the same views: the Project panel's Checks and Devices tabs
//! ([`crate::checks_panel`], [`crate::devices_panel`]) and Reports ▸ Terminals
//! ([`crate::outputs_ui`]).
//!
//! Each view keeps an update map (design §8): a full rebuild only on first show, reload or a feed
//! that rolled over; otherwise only the keys a commit touched are patched.

pub mod checks;
pub mod devices;
pub mod plc;
pub mod standards;
pub mod style_catalogues;
pub mod terminals;

use crate::project_panel::Action;

/// The list views (Lists ▸ …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Checks,
    Devices,
    Terminals,
    /// Brief 051: Lists ▸ PLC (045c's Lists ▸ I/O).
    Plc,
}

/// What a list view asks the trial for.
#[derive(Debug, Clone, PartialEq)]
pub enum ListAction {
    /// A Project panel action (go to a check or symbol, a fix, a device property).
    Panel(Action),
    /// Run this command line (a terminal plan edit); its refusal goes back to the view.
    Run(String),
    /// Show this page, centred on this point.
    GoTo(ecad_model::PageId, ecad_model::Pt),
    /// Brief 051: say this in the status line (the command line's echo).
    Say(String),
    /// Brief 051: Lists ▸ PLC's own steps, answered by the view ([`plc::IoView::handle`]).
    Tile(usize),
    Chip(usize),
    Scope(bool),
    Reveal(plc::IoKey),
    /// Brief 051: show this page at the card's first drawing on it.
    GoToCard(ecad_model::PageId, Option<ecad_model::ComponentId>),
    /// Brief 051: the Check list, filtered to this device.
    ChecksOf(ecad_model::ComponentId),
    /// Brief 051: Place Symbol for a card body, for this free slot.
    PlaceCard(u32),
}

/// The views' state, for the shell's slots.
#[derive(Debug)]
pub struct Lists<'a> {
    pub checks: &'a mut crate::checks_panel::ChecksPanel,
    pub devices: &'a mut crate::devices_panel::DevicesPanel,
    pub terminals: &'a mut terminals::TerminalsView,
    pub io: &'a mut plc::IoView,
}

/// Draw list view `tab` (one call per view, for U2's slots; design §12 O4). `writable`: the
/// project can be written (fixes and edits are offered).
pub fn show(ui: &mut eframe::egui::Ui, tab: Tab, lists: Lists<'_>, writable: bool) -> Vec<ListAction> {
    match tab {
        Tab::Checks => lists.checks.show(ui, writable).into_iter().map(ListAction::Panel).collect(),
        Tab::Devices => lists.devices.show(ui, writable).into_iter().map(ListAction::Panel).collect(),
        Tab::Terminals => lists.terminals.show(ui, writable),
        Tab::Plc => lists.io.show(ui).into_iter().filter_map(|a| lists.io.handle(a)).collect(),
    }
}

/// Copy `csv` to the clipboard (Checks and Devices until the shell gives Lists a save path).
pub fn copy_csv(ui: &eframe::egui::Ui, csv: String) {
    ui.ctx().copy_text(csv);
}
