//! Settings ▸ Appearance: where the interface theme and the sheet colour
//! are set, read from the shell's state so the page never contradicts the controls. Both are
//! changed where they live: the theme with the top bar's switch, the sheet colour with the bottom
//! bar's sheet colour menu. Under it, the developer switch ([`crate::settings::developer`]).

use ecad_widgets::tokens::space;
use eframe::egui;

use crate::settings::{card, row};
use crate::shell::app::App;

/// Where the two are changed.
pub(crate) const WHERE: &str = "The theme is switched in the top bar; the sheet colour in the bottom bar's sheet colour menu (it changes the screen only: PDFs and prints use paper colours).";

/// Draw the section.
pub(crate) fn show(t: &mut App, ui: &mut egui::Ui) {
    ui.set_max_width(760.0);
    // Every sheet colour the shell knows, by its own name (a new one must be named here).
    let sheet = match t.shell.state.sheet {
        crate::canvas::Sheet::Paper => "Paper",
        crate::canvas::Sheet::Follow => "Follow the theme",
        crate::canvas::Sheet::Dark => "Dark",
    };
    let theme = match t.shell.state.theme {
        crate::shell::state::Theme::Dark => "Dark",
        crate::shell::state::Theme::Light => "Light",
    };
    let grey = |s: &str| egui::RichText::new(s).color(ecad_widgets::color(ecad_widgets::SHELL.text_2));
    card(
        ui,
        "Appearance",
        |_| {},
        |ui| {
            row(ui, "Theme", |ui| ui.label(grey(theme)));
            row(ui, "Sheet colour", |ui| ui.label(grey(sheet)));
            ui.label(egui::RichText::new(WHERE).small().color(ecad_widgets::color(ecad_widgets::SHELL.text_3)));
        },
    );
    ui.add_space(space::L);
    card(ui, "Developer", |_| {}, |ui| crate::settings::developer::show(t, ui));
}
