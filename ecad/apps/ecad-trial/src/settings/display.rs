//! Settings ▸ Appearance (UI rev 3; was Display): where the interface theme and the sheet colour
//! are set, read from the shell's state so the page never contradicts the controls. Both are
//! changed where they live: the theme with the top bar's switch, the sheet colour with the bottom
//! bar's sheet colour menu. Under it, the developer switch ([`super::developer`]).

use ecad_ui::tokens::space;
use eframe::egui;

use super::{card, row};
use crate::Trial;

/// Where the two are changed.
pub const WHERE: &str = "The theme is switched in the top bar; the sheet colour in the bottom bar's sheet colour menu (it changes the screen only: PDFs and prints use paper colours).";

/// Draw the section.
pub fn show(t: &mut Trial, ui: &mut egui::Ui) {
    ui.set_max_width(760.0);
    // Every sheet colour the shell knows, by its own name (a new one must be named here).
    let sheet = match t.shell.state.sheet {
        crate::canvas::Sheet::Paper => "Paper",
        crate::canvas::Sheet::Dark => "Dark",
    };
    let grey = |s: &str| egui::RichText::new(s).color(ecad_ui::color(ecad_ui::SHELL.text_2));
    card(
        ui,
        "Appearance",
        |_| {},
        |ui| {
            row(ui, "Theme", |ui| ui.label(grey("As the top bar's switch shows")));
            row(ui, "Sheet colour", |ui| ui.label(grey(sheet)));
            ui.label(egui::RichText::new(WHERE).small().color(ecad_ui::color(ecad_ui::SHELL.text_3)));
        },
    );
    ui.add_space(space::L);
    card(ui, "Developer", |_| {}, |ui| super::developer::show(t, ui));
}
