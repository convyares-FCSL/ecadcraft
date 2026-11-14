//! Settings sections that show what exists today and say what comes later (UI rev 3: a UI-only
//! round, so nothing new is set here): **Libraries** (the symbol sources and their counts),
//! **Versions and backups** (Revisions opens the existing dialog) and **Shortcuts** (the keys, read
//! only).

use ecad_ui::tokens::space;
use eframe::egui;

use super::{card, later_box, row};
use crate::Trial;

/// Accessible names.
pub const REVISIONS: &str = "settings.versions.revisions";
pub const LIBRARIES: &str = "settings.libraries.sources";

/// The keys, read only (the shell's keys and the action table's; [`crate::shell::keys`]).
pub const KEYS: &[(&str, &str)] = &[
    ("V", "Select"),
    ("D", "Draw"),
    ("T", "Text"),
    ("W", "Wire"),
    ("C", "Cable"),
    ("]", "Collapse or expand the inspector"),
    ("Ctrl 1–4", "Draw, Project, Library, Publish"),
    ("Ctrl K", "Search"),
    ("Ctrl /", "Commands panel"),
    ("PgUp, PgDn", "Previous and next page"),
    ("F", "Fit the sheet"),
    ("Alt ←, Alt →", "Back and forward"),
    ("Del", "Delete"),
    ("F2", "Rename"),
    ("R", "Rotate"),
    ("X", "Mirror"),
    ("Ctrl C, Ctrl V", "Copy and paste"),
    ("Ctrl D", "Duplicate"),
    ("Ctrl Z, Ctrl Y", "Undo and redo"),
    ("Esc", "Cancel, or leave Settings"),
];

/// Settings ▸ Libraries: the sources the Library reads, with their counts.
pub fn libraries(t: &mut Trial, ui: &mut egui::Ui) {
    let builtin = ecad_cadcraft::symbols::STARTERS.len();
    let project = t
        .project
        .clone()
        .filter(|p| p.is_attached(&t.cad.session))
        .and_then(|p| p.read(&mut t.cad.session, |p, _| ecad_cadcraft::symbols::project_symbols(p.ledger.document()).len()).ok());
    ui.set_max_width(760.0);
    card(
        ui,
        "Sources",
        |_| {},
        |ui| {
            row(ui, "IEC built-in", |ui| {
                ui.label(format!("{builtin} symbols · read-only")).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, LIBRARIES));
            });
            row(ui, "This project", |ui| match project {
                Some(n) => ui.label(format!("{n} symbols")),
                None => ui.label("No project is open."),
            });
            row(ui, "Libraries", |ui| later_box(ui, "Open an .ecadlib: comes with shared libraries", 28.0));
        },
    );
}

/// Settings ▸ Versions and backups: Revisions (the existing dialog); versions and backups later.
pub fn versions(t: &mut Trial, ui: &mut egui::Ui) {
    ui.set_max_width(760.0);
    let open = t.project_open();
    let mut revisions = false;
    card(
        ui,
        "Revisions",
        |_| {},
        |ui| {
            ui.label(
                egui::RichText::new("Released revisions and the pending one. Release lives in History.").color(ecad_ui::color(ecad_ui::SHELL.text_2)),
            );
            ui.add_space(space::S);
            let b = ui.add_enabled(open, ecad_ui::prim::Button::new("Revisions…").small());
            b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, open, REVISIONS));
            revisions = b.clicked();
        },
    );
    ui.add_space(space::L);
    card(
        ui,
        "Named versions and backups",
        |_| {},
        |ui| {
            later_box(ui, "Named versions and automatic backups come later", 32.0);
        },
    );
    if revisions {
        t.open_revisions();
    }
}

/// Settings ▸ Shortcuts: the keys, read only.
pub fn shortcuts(ui: &mut egui::Ui) {
    ui.set_max_width(760.0);
    card(
        ui,
        "Keys",
        |ui| {
            ui.label(egui::RichText::new("Changing a key comes later").small().color(ecad_ui::color(ecad_ui::SHELL.text_3)));
        },
        |ui| {
            for (k, what) in KEYS {
                row(ui, what, |ui| {
                    ecad_ui::prim::keycap(ui, k);
                });
            }
        },
    );
}
