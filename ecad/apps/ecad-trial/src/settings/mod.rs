//! Settings, a place you visit (brief 042, U4c; spec §2 "Settings is a place you visit"): a
//! full-window overlay over the main view and the panels, with a list of sections on the left and
//! the section's body. Esc or Close returns to where you were: the workspace, page and selection
//! are never touched while it is open.
//!
//! Sections: [`standards`] (every default with its origin and chain, roadmap 1.13(b)),
//! [`project`] (the Project info and Project settings popouts, contents unchanged), [`text`]
//! (031's text styles), [`display`], [`agent`] (035's Agent access, moved out of the Developer
//! section, design 035 §3.2 and U0 O7) and [`developer`] (the developer switch).
//!
//! It is opened from the project menu (Settings, Project info, Project settings), the status
//! bar's standard and any origin badge in the inspector (at that key). Excluded: named standards
//! and the IEC / US switch (roadmap 1.13(a)): the standard is shown as IEC, built in, read only.

pub mod agent;
pub mod developer;
pub mod display;
pub mod project;
pub mod standards;
pub mod text;

use ecad_ui::tokens::{Icon, space};
use eframe::egui::{self, Key, Modifiers, Rect, pos2, vec2};

use crate::Trial;

/// Accessible labels.
pub const TITLE: &str = "SETTINGS";
pub const CLOSE: &str = "Close Settings";
/// The standard, as shown everywhere until named standards (brief 042 Q1).
pub const STANDARD: &str = "IEC (built in)";

/// A section of Settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Section {
    #[default]
    Standards,
    Project,
    TextStyles,
    Display,
    AgentAccess,
    Developer,
}

impl Section {
    pub const ALL: [Section; 6] =
        [Section::Standards, Section::Project, Section::TextStyles, Section::Display, Section::AgentAccess, Section::Developer];

    /// The words in the list.
    pub fn label(self) -> &'static str {
        match self {
            Section::Standards => "Standards",
            Section::Project => "Project",
            Section::TextStyles => "Text styles",
            Section::Display => "Display",
            Section::AgentAccess => "Agent access",
            Section::Developer => "Developer",
        }
    }

    /// The list item's accessible name.
    pub fn name(self) -> &'static str {
        match self {
            Section::Standards => "settings.standards",
            Section::Project => "settings.project",
            Section::TextStyles => "settings.text_styles",
            Section::Display => "settings.display",
            Section::AgentAccess => "settings.agent_access",
            Section::Developer => "settings.developer",
        }
    }
}

/// Settings' state, kept while it is closed (the section, the Standards screen's views) and the
/// inspector's rows ([`crate::shell::right`]).
#[derive(Debug, Default)]
pub struct Settings {
    pub section: Section,
    pub standards: standards::StandardsView,
    pub inspector: crate::shell::right::Inspector,
}

impl Trial {
    /// Open Settings at `section`.
    pub fn open_settings(&mut self, section: Section) {
        self.settings.section = section;
        self.shell.settings_open = true;
    }

    /// Open Settings ▸ Standards with the default `key` shown (an origin badge, the status bar).
    pub fn open_standards(&mut self, key: Option<&str>) {
        self.open_settings(Section::Standards);
        if let Some(k) = key {
            self.settings.standards.focus_key(k);
        }
    }

    /// Close Settings: back to where you were. An unsent Project or Text styles copy is dropped,
    /// as the popout's Cancel did.
    pub fn close_settings(&mut self) {
        self.shell.settings_open = false;
        self.project_props = None;
    }
}

/// Draw Settings when it is open: over the whole window, in the foreground, so the page under it
/// answers no hover ([`Trial::over_shown_page`]). Called before the shell's keys: Esc closes it,
/// and every other key is taken once Settings has drawn, so no key reaches the hidden page.
pub fn show(t: &mut Trial, ui: &mut egui::Ui) {
    if !t.shell.settings_open {
        return;
    }
    let ctx = ui.ctx().clone();
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        t.close_settings();
        return;
    }
    let rect = ctx.content_rect();
    let s = ecad_ui::SHELL;
    let mut close = false;
    egui::Area::new(egui::Id::unique("ecad.settings")).order(egui::Order::Foreground).fixed_pos(rect.min).show(&ctx, |ui| {
        ui.set_min_size(rect.size());
        ui.set_max_size(rect.size());
        ui.painter().rect_filled(rect, 0.0, ecad_ui::color(s.bg));
        // Swallow clicks and hovers on the ground: nothing under the overlay answers.
        let _ = ui.interact(rect, egui::Id::unique("ecad.settings.ground"), egui::Sense::click_and_drag());
        let list = Rect::from_min_size(rect.min, vec2(220.0, rect.height()));
        ui.painter().rect_filled(list, 0.0, ecad_ui::color(s.panel));
        ui.painter().vline(list.right() + 0.5, list.y_range(), egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
        let chosen = in_rect(ui, list.shrink(space::L), "ecad.settings.list", |ui| {
            ecad_ui::prim::read_value(ui, TITLE).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, TITLE));
            ui.add_space(space::M);
            let mut chosen = None;
            for sec in Section::ALL {
                let on = t.settings.section == sec;
                let r = ui.add(egui::Button::selectable(on, sec.label()).min_size(vec2(ui.available_width(), 28.0)));
                r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, on, sec.name()));
                if r.clicked() {
                    chosen = Some(sec);
                }
            }
            chosen
        });
        if let Some(sec) = chosen {
            t.settings.section = sec;
        }
        let head = Rect::from_min_max(pos2(list.right() + space::L, rect.top()), pos2(rect.right() - space::L, rect.top() + 44.0));
        close = in_rect(ui, head, "ecad.settings.head", |ui| {
            ui.horizontal_centered(|ui| {
                ui.heading(t.settings.section.label());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(ecad_ui::prim::IconButton::new(Icon::Close, CLOSE)).clicked()
                })
                .inner
            })
            .inner
        });
        let body = Rect::from_min_max(pos2(head.left(), head.bottom()), pos2(rect.right() - space::L, rect.bottom() - space::L));
        in_rect(ui, body, "ecad.settings.body", |ui| match t.settings.section {
            Section::Standards => standards::show(t, ui),
            Section::Project => project::show(t, ui),
            Section::TextStyles => text::show(t, ui),
            Section::Display => display::show(t, ui),
            Section::AgentAccess => agent::show(t, ui),
            Section::Developer => developer::show(t, ui),
        });
    });
    // Settings' own fields and buttons have read this frame's keys. Take them now, so that the
    // shell's keys (Del, the tool letters, Ctrl+1–4…) and CADCraft, which run after this, never
    // act on the page hidden under Settings (review of PR #78).
    ctx.input_mut(|i| {
        i.events
            .retain(|e| !matches!(e, egui::Event::Key { .. } | egui::Event::Text(_) | egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_)))
    });
    if close {
        t.close_settings();
    }
}

/// A child `Ui` filling `rect`.
pub(crate) fn in_rect<R>(ui: &mut egui::Ui, rect: Rect, id: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope_builder(egui::UiBuilder::new().id_salt(id).max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        ui.set_clip_rect(rect);
        add(ui)
    })
    .inner
}
