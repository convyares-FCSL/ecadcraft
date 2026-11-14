//! Settings ▸ Appearance (UI rev 3; was Display): the interface theme and the sheet colour, which
//! the bottom bar's sheet colour menu sets today (Paper, Follow, Dark); the light interface comes
//! with board 1.17. Under it, the developer switch ([`super::developer`]).

use ecad_ui::tokens::space;
use eframe::egui;

use super::{card, later_row, row};
use crate::Trial;

/// What the section says about the sheet colour.
pub const LATER: &str = "The sheet colour is chosen in the bottom bar (Paper or Dark). The light interface comes later.";

/// Draw the section.
pub fn show(t: &mut Trial, ui: &mut egui::Ui) {
    ui.set_max_width(760.0);
    let sheet = match t.shell.state.sheet {
        crate::canvas::Sheet::Paper => "Paper",
        crate::canvas::Sheet::Dark => "Dark",
    };
    card(
        ui,
        "Appearance",
        |_| {},
        |ui| {
            row(ui, "Interface", |ui| ui.label("Dark"));
            row(ui, "Sheet colour", |ui| ui.label(sheet));
            later_row(ui, "Light interface", "Comes with the light theme");
            ecad_ui::prim::read_value(ui, LATER);
        },
    );
    ui.add_space(space::L);
    card(ui, "Developer", |_| {}, |ui| super::developer::show(t, ui));
}
