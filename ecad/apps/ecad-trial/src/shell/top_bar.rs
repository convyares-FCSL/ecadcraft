//! The top bar (spec §2: 44 px): the project menu, the four workspaces, search, and the Checks,
//! revision and agent pills. Nothing else.
//!
//! The project menu holds what the Project panel's header held: New, Open, Save, Save As, Close,
//! and the project popouts (Project info, Project settings, Revisions), and opens Settings.

use ecad_ui::prim::{self, Tone};
use ecad_ui::tokens::space;
use eframe::egui::{self, Rect};

use super::Workspace;
use crate::Trial;
use crate::project_panel::Action;

/// Accessible labels.
pub const PROJECT_MENU: &str = "Project menu";
pub const SEARCH: &str = "Search (Ctrl+K)";
pub const CHECKS_PILL: &str = "Checks pill";
pub const REVISION_PILL: &str = "Revision pill";
pub const AGENT_PILL: &str = "Agent pill";
/// The project menu's entries (the header's labels).
pub const NEW: &str = "New project";
pub const OPEN: &str = "Open project…";
pub const SAVE: &str = "Save";
pub const SAVE_AS: &str = "Save As…";
pub const CLOSE: &str = "Close project";

/// Draw the top bar in `rect`.
pub fn show(t: &mut Trial, ui: &mut egui::Ui, rect: Rect) {
    let s = ecad_ui::SHELL;
    super::fill(ui, rect, s.panel);
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
    let header = t.header();
    let checks = t.checks_count();
    let mut actions = Vec::new();
    let mut ws = t.shell.state.workspace.index();
    let mut open_settings = false;
    let mut toggle_drawer = false;
    let mut release = false;
    let mut agent = false;
    let search_was = t.search.query.clone();
    super::in_rect(ui, rect.shrink2(egui::vec2(space::L, space::S)), "ecad.shell.topbar", |ui| {
        ui.horizontal_centered(|ui| {
            let name = header.name.clone().unwrap_or_else(|| "No project".into());
            let menu = ui.add(prim::Button::new(&name).kind(prim::Kind::Quiet));
            menu.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, PROJECT_MENU));
            egui::Popup::menu(&menu).show(|ui| {
                let open = header.name.is_some();
                let mut item = |ui: &mut egui::Ui, label: &str, enabled: bool, a: Action| {
                    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                        actions.push(a);
                        ui.close();
                    }
                };
                item(ui, NEW, true, Action::NewProject);
                item(ui, OPEN, true, Action::OpenProject);
                item(ui, SAVE, open, Action::Save);
                item(ui, SAVE_AS, open, Action::SaveAs);
                item(ui, CLOSE, open, Action::CloseProject);
                ui.separator();
                item(ui, crate::project_panel::PROJECT_INFO, open, Action::ProjectInfo);
                item(ui, crate::project_panel::PROJECT_SETTINGS, open, Action::ProjectSettings);
                item(ui, crate::project_panel::REVISIONS, open, Action::Revisions);
                ui.separator();
                if ui.button(super::SETTINGS).clicked() {
                    open_settings = true;
                    ui.close();
                }
            });
            ui.add_space(space::L);
            let labels: Vec<&str> = Workspace::ALL.iter().map(|w| w.label()).collect();
            prim::segmented(ui, &mut ws, &labels);
            ui.add_space(space::L);
            prim::Field::new(&mut t.search.query, prim::FieldKind::Text, SEARCH).placeholder(SEARCH).width(280.0).show(ui);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(r) = agent_pill(ui, t) {
                    agent = r;
                }
                if let Some((rev, counts)) = &header.release {
                    let tone = match counts {
                        Some((e, _)) if *e > 0 => Tone::Error,
                        Some((_, w)) if *w > 0 => Tone::Warning,
                        _ => Tone::Provisional,
                    };
                    release = clickable_pill(ui, &format!("Rev {rev}"), tone, REVISION_PILL, "Publish ▸ Release: check, issue and write its folder");
                }
                let (n, errors) = checks;
                let tone = if errors > 0 {
                    Tone::Error
                } else if n > 0 {
                    Tone::Warning
                } else {
                    Tone::Ok
                };
                toggle_drawer = clickable_pill(ui, &format!("Checks {n}"), tone, CHECKS_PILL, "Open or close the Checks drawer");
            });
        });
    });
    if t.search.query != search_was {
        t.search.open = true;
    }
    if let Some(w) = Workspace::from_index(ws) {
        t.set_workspace(w);
    }
    if open_settings {
        t.shell.settings_open = true;
    }
    if toggle_drawer {
        t.shell.state.drawer_open = !t.shell.state.drawer_open;
    }
    if release {
        t.shell.publish = super::PublishTab::Release;
        t.set_workspace(Workspace::Publish);
        t.open_release(None);
    }
    if agent {
        // With an agent shown the pill hides or shows its banner (brief 043); without one it opens
        // the Text panel, where Agent access is set until Settings has it (042).
        match t.agent.banner.as_ref().map(|b| b.session.clone()) {
            Some(session) => crate::agent::banner::toggle_hidden(ui.ctx(), &session),
            None => t.toggle_text_panel(),
        }
    }
    for a in actions {
        t.panel_action(a);
    }
}

/// A pill that clicks: its text drawn as [`prim::pill`], its accessible label `label`.
fn clickable_pill(ui: &mut egui::Ui, text: &str, tone: Tone, label: &str, hover: &str) -> bool {
    let r = prim::pill(ui, text, tone);
    let c = ui.interact(r.rect, egui::Id::unique(("ecad.shell.pill", label)), egui::Sense::click());
    c.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{label}: {text}")));
    c.on_hover_text(hover).clicked()
}

/// The agent pill: the agent's name in its state's tone while one is shown (brief 043: it mirrors
/// the banner; a click hides or shows the banner), else the bridge's state (off when agent access
/// is off).
fn agent_pill(ui: &mut egui::Ui, t: &Trial) -> Option<bool> {
    let off = t.agent.pref.mode == ecad_cadcraft::agent_access::Mode::Off && !t.agent.this_run;
    let (text, tone, hover) = if let Some(b) = &t.agent.banner {
        let state = b.shown_state(std::time::Instant::now());
        let hidden = crate::agent::banner::hidden(ui.ctx(), &b.session);
        let hover = if hidden { "Show the agent banner" } else { "Hide the agent banner" };
        (format!("Agent {}", b.agent), crate::agent::banner::state_tone(state), hover.to_string())
    } else if off {
        ("Agent off".to_string(), Tone::Neutral, t.agent.status_text())
    } else {
        ("Agent ready".to_string(), Tone::Neutral, t.agent.status_text())
    };
    Some(clickable_pill(ui, &text, tone, AGENT_PILL, &hover))
}

impl Trial {
    /// The Checks count and how many are errors, from the kept Checks (the drawer's copy).
    pub fn checks_count(&self) -> (usize, usize) {
        (self.panel.checks.rows().len(), self.panel.checks.counts().0)
    }
}
