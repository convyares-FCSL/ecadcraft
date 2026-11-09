//! The Project panel's **Developer** section (owner, 2026-11-08): folded by default; unfolded, a
//! resizable lower part of the panel that hosts the developer tools itself:
//!
//! - the ECAD command field (`name {json}` forms) with **Run** and the reply under it (the
//!   template buttons of Tool Sets fill this field);
//! - the read-only debugger ([`crate::debugger`]) drawn inline;
//! - **Inspect Project**: a readable, copyable summary of the project.
//!
//! Everything here reads, except the command field, which runs commands through the ordinary
//! command path. Excluded: a separate debugger panel or command window.

use eframe::egui::{self, RichText};

use crate::debugger::{self, Debugger};

/// Accessible label of the section's header (fold / unfold).
pub const HEADER: &str = "Developer";
/// Accessible label of the command field.
pub const FIELD: &str = "ECAD command";
/// Button labels.
pub const DEBUGGER: &str = "Debugger";
pub const INSPECT: &str = "Inspect Project";
pub const SNAPSHOT: &str = "Export Inspector Snapshot";

/// What the section asks the trial for.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Run this line through the command path.
    Run(String),
    /// Show or hide the inline debugger.
    ToggleDebugger,
    /// Refresh the project summary.
    Inspect,
    /// Export an inspector snapshot (`--snapshots`).
    Snapshot,
    /// A debugger action that needs the project.
    Debugger(debugger::Action),
}

/// What the section shows under its tools.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Body {
    #[default]
    None,
    Debugger,
    Inspect,
}

/// The section's state (the command field's text lives in the trial, as the template buttons
/// fill it).
#[derive(Debug, Default)]
pub struct Developer {
    pub open: bool,
    pub body: Body,
    /// The last reply to a command run here (the lines it echoed).
    pub reply: Vec<String>,
    /// The project summary, from Inspect Project.
    pub inspect: Option<String>,
}

/// Everything the section draws, borrowed from the trial.
pub struct Parts<'a> {
    pub dev: &'a mut Developer,
    pub command: &'a mut String,
    pub hint: Option<&'static str>,
    pub focus: &'a mut bool,
    pub debugger: &'a mut Debugger,
    pub running: bool,
    pub snapshots: bool,
    pub project: bool,
}

/// Draw the section at the bottom of the panel's `ui`. Returns what was asked for.
pub fn show(ui: &mut egui::Ui, p: Parts<'_>) -> Vec<Action> {
    let mut out = Vec::new();
    let panel_h = ui.available_height();
    let open = p.dev.open;
    let mut bottom = egui::Panel::bottom("ecad.project.developer").resizable(open);
    bottom = if open { bottom.default_size(panel_h * 0.6).min_size(120.0).max_size(panel_h * 0.9) } else { bottom.exact_size(24.0) };
    bottom.show(ui, |ui| {
        let mut toggled = false;
        ui.horizontal_wrapped(|ui| {
            let arrow = if p.dev.open { "v" } else { ">" };
            let header = ui.add(egui::Button::new(RichText::new(format!("{arrow} {HEADER}")).small()).frame(false));
            header.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, p.dev.open, HEADER));
            toggled = header.clicked();
            if !p.dev.open {
                return;
            }
            // The tools, on the header line.
            if p.project {
                if ui.selectable_label(p.dev.body == Body::Debugger, RichText::new(DEBUGGER).small()).on_hover_text(debugger::HOVER).clicked() {
                    out.push(Action::ToggleDebugger);
                }
                if ui
                    .selectable_label(p.dev.body == Body::Inspect, RichText::new(INSPECT).small())
                    .on_hover_text("A readable summary of the project: pages, devices, cables, counts, file")
                    .clicked()
                {
                    out.push(Action::Inspect);
                }
                if p.snapshots && ui.small_button(SNAPSHOT).on_hover_text(crate::SNAPSHOT_NOTE).clicked() {
                    out.push(Action::Snapshot);
                }
            }
        });
        if toggled {
            p.dev.open = !p.dev.open;
        }
        if !p.dev.open {
            return;
        }
        // The command field and Run, the template's note and the last reply.
        let mut submit = None;
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(p.command)
                    .id(egui::Id::unique("ecad.command.field"))
                    .hint_text("wire {\"points\":[[0,0],[40,0]]}")
                    .font(egui::TextStyle::Monospace)
                    .desired_width(ui.available_width() - 40.0),
            );
            field.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, FIELD));
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if enter {
                ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            }
            if (ui.small_button("Run").clicked() || enter) && (p.running || !p.command.trim().is_empty()) {
                submit = Some(std::mem::take(p.command));
                field.request_focus();
            } else if std::mem::take(p.focus) {
                field.request_focus();
            }
        });
        if let Some(hint) = p.hint {
            ui.add(egui::Label::new(RichText::new(hint).small()).wrap());
        }
        if !p.dev.reply.is_empty() {
            let text = p.dev.reply.join("\n");
            egui::ScrollArea::vertical().id_salt("ecad.dev.reply").max_height(80.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut text.as_str()).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY).desired_rows(1));
            });
        }
        if let Some(line) = submit {
            out.push(Action::Run(line));
        }
        ui.separator();
        match p.dev.body {
            Body::Debugger if p.debugger.open => {
                if let Some(a) = p.debugger.show(ui) {
                    out.push(Action::Debugger(a));
                }
            }
            Body::Inspect => {
                if let Some(text) = &p.dev.inspect {
                    egui::ScrollArea::both().id_salt("ecad.dev.inspect").auto_shrink([false, false]).show(ui, |ui| {
                        ui.add(egui::TextEdit::multiline(&mut text.as_str()).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY));
                    });
                }
            }
            _ => {}
        }
    });
    out
}
