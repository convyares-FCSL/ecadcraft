//! Settings ▸ Text styles: brief 031's Text section (the style table and the project font),
//! which was part of the Project settings popout (brief 042). It edits the same copy as Settings ▸
//! Project ▸ Settings, so OK writes one `textstyle` line as before.

use eframe::egui;

use crate::Trial;
use crate::sheets_ui::{Part, Popout};

/// Draw the section.
pub fn show(t: &mut Trial, ui: &mut egui::Ui) {
    ecad_ui::prim::read_value(ui, "Each kind of text: its height, weight and font, and whether it is shown. Also listed in Standards.");
    ui.add_space(ecad_ui::tokens::space::S);
    super::project::body(t, ui, Popout::Settings, Part::Text);
}
