//! Settings ▸ Developer: the developer switch, which shows CADCraft's own interface inside the
//! main view (`--cad-ui`, `ui.cadchrome`; brief 038).

use eframe::egui;

use crate::Trial;
use crate::shell::DEVELOPER_SWITCH;

/// Draw the section.
pub fn show(t: &mut Trial, ui: &mut egui::Ui) {
    let mut on = t.shell.state.cad_chrome;
    ui.checkbox(&mut on, DEVELOPER_SWITCH).on_hover_text(
        "CADCraft's menus, toolbar, Tool Sets, layers, layouts and command line, inside the main view. Also --cad-ui or ui.cadchrome.",
    );
    if on != t.shell.state.cad_chrome {
        t.set_cad_chrome(on);
    }
    ecad_ui::prim::read_value(ui, "The ECAD command field, the debugger and Inspect Project are in the Commands panel (Ctrl+/).");
}
