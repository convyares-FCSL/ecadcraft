//! Settings ▸ Display. The canvas lane (brief 040, U4a) adds the sheet's look here (paper or dark
//! sheet) once it is in `dev`; until then the section says so (brief 042 §0).

use eframe::egui;

use crate::Trial;

/// What the section says until the canvas lane lands.
pub const LATER: &str = "The page's look (paper or dark sheet) is chosen here once the new canvas is in.";

/// Draw the section.
pub fn show(_t: &mut Trial, ui: &mut egui::Ui) {
    ecad_ui::prim::read_value(ui, LATER);
}
