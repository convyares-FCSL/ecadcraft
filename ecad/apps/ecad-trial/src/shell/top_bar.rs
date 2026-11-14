//! The top bar (UI rev 3: 40 px, the same on every screen): the project and the app, nothing
//! else. No branding and no revision pill.
//!
//! - Far left: the project name pill and its menu (New, Open, Save, Save As, Close, the project
//!   popouts, Settings and standards).
//! - Beside it: the workspace switch, Draw / Project / Library / Publish, in the same pill.
//! - Centred on the window: search (Ctrl K).
//! - Right: the agent icon, share (coming), the light/dark switch, help and the account avatar.
//!
//! The Checks counter and the revision moved to the bottom bar ([`super::status_bar`]); their
//! accessible labels ([`CHECKS_PILL`], [`REVISION_PILL`]) moved with them.

use ecad_ui::tokens::{Icon, size, space, ui_type};
use eframe::egui::{self, Rect, pos2, vec2};

use super::{Workspace, bar};
use crate::Trial;
use crate::project_panel::Action;

/// Accessible labels.
pub const PROJECT_MENU: &str = "Project menu";
pub const SEARCH: &str = "Search (Ctrl+K)";
/// The search box's grey text.
pub const SEARCH_HINT: &str = "Find a page, tag, symbol or command";
/// The bottom bar's checks counter (opens the inspector's Checks tab).
pub const CHECKS_PILL: &str = "Checks pill";
/// The bottom bar's revision (opens History).
pub const REVISION_PILL: &str = "Revision pill";
/// The agent icon.
pub const AGENT_PILL: &str = "Agent pill";
pub const THEME_DARK: &str = "Dark interface";
pub const THEME_LIGHT: &str = "Light interface";
pub const HELP: &str = "Help";
/// Share (coming; it does nothing yet).
pub const SHARE: &str = "Share";
pub const ACCOUNT: &str = "Account";
/// The project menu's entries (the header's labels).
pub const NEW: &str = "New project";
pub const OPEN: &str = "Open project…";
pub const SAVE: &str = "Save";
pub const SAVE_AS: &str = "Save As…";
pub const CLOSE: &str = "Close project";
/// The project menu's last entry.
pub const STANDARDS: &str = "Standards";

/// Heights inside the bar: the pills and the search box.
const PILL_H: f32 = 28.0;
/// The search box's width (the board's).
const SEARCH_W: f32 = 460.0;
/// A workspace segment's padding either side of its label.
const SEG_PAD: f32 = 12.0;

/// Draw the top bar in `rect`.
pub fn show(t: &mut Trial, ui: &mut egui::Ui, rect: Rect) {
    let s = ecad_ui::SHELL;
    super::fill(ui, rect, s.panel);
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
    let header = t.header();
    let mut actions = Vec::new();
    let mut ws = t.shell.state.workspace;
    let mut open_settings = false;
    let mut open_standards = false;

    let y = rect.center().y;
    let pill_rect = |x: f32, w: f32| Rect::from_min_size(pos2(x, y - PILL_H / 2.0), vec2(w, PILL_H));

    // The project pill and its menu.
    let name = header.name.clone().unwrap_or_else(|| "No project".into());
    let name_w = bar::text_width(ui, &name, ui_type::BODY_EMPHASIS);
    let project = pill_rect(rect.left() + space::M, name_w + 2.0 * space::L + size::ICON);
    bar::pill(ui, project);
    let menu = bar::hit(ui, project.shrink(1.0), PROJECT_MENU, t.shell.settings_open, false, true);
    bar::text_at(ui, project.left() + space::L, y, &name, ui_type::BODY_EMPHASIS, ecad_ui::color(s.text));
    let chevron = Rect::from_center_size(pos2(project.right() - space::M - size::ICON / 2.0 + 2.0, y), vec2(12.0, 12.0));
    ecad_ui::icons::paint(ui.painter(), chevron, Icon::ChevronDown, ecad_ui::color(s.text_2));
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
        if ui.button(STANDARDS).clicked() {
            open_standards = true;
            ui.close();
        }
    });

    // The workspace switch, in the same pill beside it.
    let widths: Vec<f32> = Workspace::ALL.iter().map(|w| bar::text_width(ui, w.label(), ui_type::BODY) + 2.0 * SEG_PAD).collect();
    let switch = pill_rect(project.right() + space::M, widths.iter().sum::<f32>() + 2.0 * space::XS);
    bar::pill(ui, switch);
    let mut x = switch.left() + space::XS;
    for (w, width) in Workspace::ALL.iter().zip(&widths) {
        let r = Rect::from_min_size(pos2(x, switch.top() + space::XS / 2.0 + 1.0), vec2(*width, PILL_H - space::XS - 2.0));
        let on = t.shell.state.workspace == *w;
        let resp = bar::text_button(ui, r, w.label(), w.label(), on, true);
        // A tab for screen readers and tests (the switch used to be a segmented control).
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, on, w.label()));
        if resp.clicked() {
            ws = *w;
        }
        x += width;
    }

    // Search, centred on the window.
    let search = Rect::from_center_size(pos2(rect.center().x, y), vec2(SEARCH_W, size::SEARCH_FIELD));
    let search_was = t.search.query.clone();
    super::in_rect(ui, search, "ecad.shell.topbar.search", |ui| {
        ecad_ui::prim::Field::new(&mut t.search.query, ecad_ui::prim::FieldKind::Search, SEARCH).placeholder(SEARCH_HINT).width(SEARCH_W).show(ui);
    });
    keycap(ui, search, "Ctrl K");

    // Right: avatar, help, the light/dark switch, the agent icon (laid out from the right edge).
    let mut x = rect.right() - space::M;
    let avatar = Rect::from_center_size(pos2(x - 12.0, y), vec2(24.0, 24.0));
    x = avatar.left() - space::M;
    let a = ui.interact(avatar, egui::Id::unique("ecad.shell.account"), egui::Sense::hover());
    a.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, ACCOUNT));
    a.on_hover_text("Accounts come later");
    ui.painter().circle_filled(avatar.center(), 12.0, ecad_ui::color(s.accent_soft));
    ui.painter().circle_stroke(avatar.center(), 12.0, egui::Stroke::new(1.0, ecad_ui::color(s.accent)));
    ui.painter().text(avatar.center(), egui::Align2::CENTER_CENTER, initials(), bar::font(ui, ui_type::CAPTION), ecad_ui::color(s.text));
    let help = Rect::from_center_size(pos2(x - 14.0, y), vec2(28.0, 28.0));
    x = help.left() - space::S;
    bar::icon_button(ui, help, Icon::Help, HELP, size::ICON, true).on_hover_text("Help comes later");
    bar::vline(ui, x, y - 10.0, y + 10.0);
    x -= space::S;
    // One light/dark switch: the shell has one appearance until the light theme (board 1.17).
    let theme = Rect::from_min_max(pos2(x - 56.0, y - 12.0), pos2(x, y + 12.0));
    x = theme.left() - space::S;
    ui.painter().rect_filled(theme, ecad_ui::style::corner(12.0), ecad_ui::color(s.raised));
    let dark = Rect::from_min_size(theme.min + vec2(2.0, 2.0), vec2(26.0, 20.0));
    let light = Rect::from_min_size(pos2(dark.right(), dark.top()), vec2(26.0, 20.0));
    ui.painter().rect_filled(dark, ecad_ui::style::corner(10.0), ecad_ui::color(s.active));
    let d = ui.interact(dark, egui::Id::unique(THEME_DARK), egui::Sense::click());
    d.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, true, THEME_DARK));
    ecad_ui::icons::paint(ui.painter(), Rect::from_center_size(dark.center(), vec2(13.0, 13.0)), Icon::Moon, ecad_ui::color(s.text));
    let l = ui.interact(light, egui::Id::unique(THEME_LIGHT), egui::Sense::hover());
    l.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, false, false, THEME_LIGHT));
    l.on_hover_text("The light interface is not available yet.");
    ecad_ui::icons::paint(ui.painter(), Rect::from_center_size(light.center(), vec2(13.0, 13.0)), Icon::Sun, ecad_ui::color(s.text_3));
    let share = Rect::from_center_size(pos2(x - 14.0, y), vec2(28.0, 28.0));
    x = share.left() - space::XS;
    bar::icon_button(ui, share, Icon::Share, SHARE, size::ICON, true).on_hover_text("Share (coming)");
    let agent_rect = Rect::from_center_size(pos2(x - 14.0, y), vec2(28.0, 28.0));
    let (tone, state, hover) = agent_state(ui, t);
    let r = bar::icon_button(ui, agent_rect, Icon::Agent, &format!("{AGENT_PILL}: {state}"), size::ICON, true).on_hover_text(hover);
    if let Some(c) = tone {
        ecad_ui::icons::paint(ui.painter(), Rect::from_center_size(agent_rect.center(), vec2(size::ICON, size::ICON)), Icon::Agent, c);
    }
    let agent = r.clicked();

    if t.search.query != search_was {
        t.search.open = true;
    }
    t.set_workspace(ws);
    if open_settings {
        t.shell.settings_open = true;
    }
    if open_standards {
        t.open_standards(None);
    }
    if agent {
        // With an agent shown the icon hides or shows its banner (brief 043); without one it opens
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

/// The key cap drawn inside the right end of the search box.
fn keycap(ui: &egui::Ui, search: Rect, text: &str) {
    let s = ecad_ui::SHELL;
    let w = bar::text_width(ui, text, ui_type::CAPTION) + 2.0 * space::S;
    let r = Rect::from_min_size(pos2(search.right() - space::S - w, search.center().y - 9.0), vec2(w, 18.0));
    ui.painter().rect(
        r,
        ecad_ui::style::corner(ecad_ui::tokens::radius::KEYCAP),
        egui::Color32::TRANSPARENT,
        egui::Stroke::new(1.0, ecad_ui::color(s.line_strong)),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, text, bar::font(ui, ui_type::CAPTION), ecad_ui::color(s.text_2));
}

/// The agent icon's colour while an agent is shown (its state's tone), its state in words (its
/// accessible label: `Agent tester`, `Agent off`, `Agent ready`), and its hover text.
fn agent_state(ui: &egui::Ui, t: &Trial) -> (Option<egui::Color32>, String, String) {
    let off = t.agent.pref.mode == ecad_cadcraft::agent_access::Mode::Off && !t.agent.this_run;
    if let Some(b) = &t.agent.banner {
        let state = b.shown_state(std::time::Instant::now());
        let hidden = crate::agent::banner::hidden(ui.ctx(), &b.session);
        let hover = if hidden { "Show the agent banner" } else { "Hide the agent banner" };
        let tone = crate::agent::banner::state_tone(state);
        (Some(ecad_ui::color(tone.text())), format!("Agent {}", b.agent), hover.to_string())
    } else if off {
        (None, "Agent off".into(), t.agent.status_text())
    } else {
        (None, "Agent ready".into(), t.agent.status_text())
    }
}

/// The account avatar's letters: there are no accounts yet, so two letters of the person's login
/// name (read from the environment, shown only on this screen): its capitals when it has two
/// (`JaneDoe` → `JD`), else its first two letters, else a neutral mark.
fn initials() -> String {
    let name = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default();
    let caps: String = name.chars().filter(|c| c.is_uppercase()).take(2).collect();
    let letters =
        if caps.chars().count() == 2 { caps } else { name.chars().filter(|c| c.is_alphabetic()).take(2).collect::<String>().to_uppercase() };
    if letters.is_empty() { "·".into() } else { letters }
}

impl Trial {
    /// The Checks count and how many are errors, from the kept Checks (the inspector's copy).
    pub fn checks_count(&self) -> (usize, usize) {
        (self.panel.checks.rows().len(), self.panel.checks.counts().0)
    }
}
