//! The Project panel's **Developer** section (owner, 2026-11-08): folded by default; unfolded, a
//! resizable lower part of the panel that hosts the developer tools itself:
//!
//! - the ECAD command field (`name {json}` forms) with **Run** and the reply under it (the
//!   template buttons of Tool Sets fill this field);
//! - the read-only debugger ([`crate::debugger`]) drawn inline;
//! - **Inspect Project**: a readable, copyable summary of the project;
//! - **Agent access** (brief 035, design §3.2): Off, Read only or Read and change, the port, and
//!   what listens (until the UI shell gives settings a home).
//! - `ui.primitives`: the hidden page showing every primitive of `ecad-ui` in every state (brief
//!   037), for screenshots; it needs no project.
//!
//! Everything here reads, except the command field, which runs commands through the ordinary
//! command path. Excluded: a separate debugger panel or command window.

use ecad_cadcraft::agent_access::Mode;
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
/// Element names of the Agent access row (brief 035, design §13).
pub const AGENT_MODE: &str = "agent.access.mode";
pub const AGENT_PORT: &str = "agent.access.port";
pub const AGENT_STATUS: &str = "agent.access.status";
/// The port field's tooltip and warning.
pub const AGENT_PORT_RANGE: &str = "1024–65535";
/// Said when the port changes while the listener runs on another.
pub const AGENT_RESTART: &str = "The new port is used the next time ECAD starts.";
/// The mode's tooltip under `--control`.
pub const AGENT_THIS_RUN: &str = "Set by --control for this run.";
/// The primitives showcase's button (brief 037), named as its page.
pub const PRIMITIVES: &str = ecad_ui::showcase::PAGE;

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
    /// Agent access: the mode chosen.
    AgentMode(Mode),
    /// Agent access: the port field was applied (Enter or focus loss).
    AgentPort,
    /// A control of the agent banner's line.
    Banner(crate::agent::BannerAction),
}

/// The Agent access row's state, borrowed from the trial.
pub struct AgentRow<'a> {
    pub mode: Mode,
    pub port: &'a mut String,
    pub status: String,
    /// The status dot's warning colour (port in use, stopped).
    pub warning: bool,
    /// Set by `--control`: the mode is shown, not changed.
    pub locked: bool,
    pub port_warning: bool,
    pub restart_note: bool,
    /// The agent banner, drawn as one line under the row while an agent is shown.
    pub banner: Option<crate::agent::AgentBanner>,
    /// A command is running (Undo session is disabled).
    pub busy: bool,
}

/// What the section shows under its tools.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Body {
    #[default]
    None,
    Debugger,
    Inspect,
    /// The primitives showcase (brief 037).
    Primitives,
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
    /// What the primitives showcase's live widgets edit.
    pub showcase: ecad_ui::showcase::Showcase,
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
    pub agent: AgentRow<'a>,
}

/// The Agent access row: mode, port and status (design 035 §3.2).
fn agent_row(ui: &mut egui::Ui, a: AgentRow<'_>, out: &mut Vec<Action>) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Agent access").small());
        let mut mode = a.mode;
        let combo = ui.add_enabled_ui(!a.locked, |ui| {
            egui::ComboBox::from_id_salt("ecad.agent.access.mode").selected_text(mode.label()).show_ui(ui, |ui| {
                for m in [Mode::Off, Mode::Read, Mode::Write] {
                    ui.selectable_value(&mut mode, m, m.label());
                }
            })
        });
        let r = combo.inner.response;
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, !a.locked, AGENT_MODE));
        if a.locked {
            r.on_disabled_hover_text(AGENT_THIS_RUN);
        }
        if mode != a.mode {
            out.push(Action::AgentMode(mode));
        }
        ui.label(RichText::new("Port").small());
        let port = ui.add_enabled(!a.locked, egui::TextEdit::singleline(a.port).desired_width(52.0).char_limit(5));
        port.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, !a.locked, AGENT_PORT));
        if port.changed() {
            a.port.retain(|c| c.is_ascii_digit());
        }
        if port.lost_focus() {
            out.push(Action::AgentPort);
        }
        let status =
            ui.label(RichText::new(&a.status).small().color(if a.warning { ui.visuals().warn_fg_color } else { ui.visuals().weak_text_color() }));
        status.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, AGENT_STATUS));
    });
    if a.port_warning {
        ui.label(RichText::new(AGENT_PORT_RANGE).small().color(ui.visuals().warn_fg_color));
    }
    if a.restart_note {
        ui.label(RichText::new(AGENT_RESTART).small());
    }
    if let Some(b) = &a.banner
        && let Some(act) = crate::agent::banner::show_line(ui, b, a.busy)
    {
        out.push(Action::Banner(act));
    }
}

/// Draw the section in `ui`, filling it (brief 038: the Text panel's Commands tab; it was the
/// lower part of the Project panel). Returns what was asked for.
pub fn show(ui: &mut egui::Ui, p: Parts<'_>) -> Vec<Action> {
    let mut out = Vec::new();
    {
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
            let showing = p.dev.body == Body::Primitives;
            if ui.selectable_label(showing, RichText::new(PRIMITIVES).small()).on_hover_text("Every interface primitive in every state").clicked() {
                p.dev.body = if showing { Body::None } else { Body::Primitives };
            }
        });
        if toggled {
            p.dev.open = !p.dev.open;
        }
        if !p.dev.open {
            return out;
        }
        agent_row(ui, p.agent, &mut out);
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
            Body::Primitives => ecad_ui::showcase::show(ui, &mut p.dev.showcase),
            _ => {}
        }
    }
    out
}
