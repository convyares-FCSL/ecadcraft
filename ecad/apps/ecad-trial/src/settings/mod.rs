//! Settings, a place you visit (brief 042, U4c; UI rev 3 boards 5.01 and 5.02): opened from the
//! project name menu, which shows as active while Settings is open, between the same top and
//! bottom bars as every other screen. A list on the left, the section's title and body on the
//! right. Esc or Back to Draw returns to where you were: the workspace, page and selection are
//! never touched while it is open; choosing a workspace in the top bar closes it too.
//!
//! The list (rev 3): Back to Draw (Esc); for this project Project info, Standards, Pages and
//! sheets, Libraries, Agent access, Versions and backups; for this computer Appearance and
//! Shortcuts. Sections: [`project`] (Project info, and Pages and sheets with 031's text styles),
//! [`standards`] (every default with its origin and chain, roadmap 1.13(b)), [`agent`] (035's
//! Agent access), [`display`] (Appearance, with the developer switch from [`developer`]) and
//! [`later`] (Libraries, Versions and backups, Shortcuts: what exists today, and what comes).

pub mod agent;
pub mod developer;
pub mod display;
pub mod later;
pub mod project;
pub mod standards;
pub mod text;

use ecad_ui::tokens::{Icon, space, ui_type};
use eframe::egui::{self, Key, Modifiers, Rect, pos2, vec2};

use crate::Trial;
use crate::shell::{Workspace, bar};

/// Accessible labels.
pub const TITLE: &str = "SETTINGS";
/// Back to Draw (Esc): closes Settings.
pub const CLOSE: &str = "Close Settings";
/// The standard, as shown everywhere until named standards (brief 042 Q1).
pub const STANDARD: &str = "IEC (built in)";
/// The list's width at 1920 × 1080 (board 5.01).
pub const LIST_W: f32 = 232.0;
/// The title row's height.
pub const HEAD_H: f32 = 64.0;

/// A section of Settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Section {
    #[default]
    Standards,
    /// Project info (board 5.01).
    Project,
    /// Pages and sheets: the sheet, references, tags, revisions, numbering and wires.
    Pages,
    /// Pages and sheets on its Text styles tab (031's text section).
    TextStyles,
    Libraries,
    AgentAccess,
    Versions,
    /// Appearance.
    Display,
    /// Appearance, at its developer switch.
    Developer,
    Shortcuts,
}

impl Section {
    /// The list, in the board's order: for this project, then for this computer.
    pub const ALL: [Section; 8] = [
        Section::Project,
        Section::Standards,
        Section::Pages,
        Section::Libraries,
        Section::AgentAccess,
        Section::Versions,
        Section::Display,
        Section::Shortcuts,
    ];
    /// How many of [`Section::ALL`] are for this project.
    pub const PROJECT_COUNT: usize = 6;

    /// The list item this section is shown under.
    pub fn item(self) -> Section {
        match self {
            Section::TextStyles => Section::Pages,
            Section::Developer => Section::Display,
            s => s,
        }
    }

    /// The words in the list and the title.
    pub fn label(self) -> &'static str {
        match self {
            Section::Standards => "Standards",
            Section::Project => "Project info",
            Section::Pages | Section::TextStyles => "Pages and sheets",
            Section::Libraries => "Libraries",
            Section::AgentAccess => "Agent access",
            Section::Versions => "Versions and backups",
            Section::Display | Section::Developer => "Appearance",
            Section::Shortcuts => "Shortcuts",
        }
    }

    /// The line beside the title: what the section is for.
    pub fn about(self) -> &'static str {
        match self {
            Section::Standards => {
                "Every default has one entry here. A value is taken from the nearest level that sets it: built-in, standard, project, page, object."
            }
            Section::Project => {
                "What the project is and who it is for. These values fill every title block, report header and PDF property, so you set them once here."
            }
            Section::Pages | Section::TextStyles => "The sheet and its grid, references, tags, revisions, numbering, wires and text.",
            Section::Libraries => "Where this project's symbols come from.",
            Section::AgentAccess => "Whether an agent may read this project, or read and change it.",
            Section::Versions => "Revisions, named versions and backups of this project.",
            Section::Display | Section::Developer => "How ECADCraft looks on this computer.",
            Section::Shortcuts => "The keys on this computer.",
        }
    }

    /// The list item's accessible name.
    pub fn name(self) -> &'static str {
        match self {
            Section::Standards => "settings.standards",
            Section::Project => "settings.project",
            Section::Pages => "settings.pages",
            Section::TextStyles => "settings.text_styles",
            Section::Libraries => "settings.libraries",
            Section::AgentAccess => "settings.agent_access",
            Section::Versions => "settings.versions",
            Section::Display => "settings.display",
            Section::Developer => "settings.developer",
            Section::Shortcuts => "settings.shortcuts",
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
    /// The workspace Settings was opened over: choosing another in the top bar closes it.
    pub over: Option<Workspace>,
}

impl Trial {
    /// Open Settings at `section`.
    pub fn open_settings(&mut self, section: Section) {
        self.settings.section = section;
        self.settings.over = Some(self.shell.state.workspace);
        self.shell.settings_open = true;
    }

    /// Open Settings ▸ Standards with the default `key` shown (an origin badge, the status bar).
    pub fn open_standards(&mut self, key: Option<&str>) {
        self.open_settings(Section::Standards);
        if let Some(k) = key {
            self.settings.standards.focus_key(k);
        }
    }

    /// Open Settings ▸ Standards at a style catalogue (brief 050a: a Checks row's Go to).
    pub fn open_standards_at(&mut self, catalogue: &str) {
        self.open_settings(Section::Standards);
        let v = &mut self.settings.standards;
        if let Some(i) = standards::CATALOGUES.iter().position(|(w, _, _)| *w == catalogue) {
            v.catalogue = i + 1;
            v.style_focus = None;
        }
    }

    /// Close Settings: back to where you were. An unsent Project or Text styles copy is dropped,
    /// as the popout's Cancel did.
    pub fn close_settings(&mut self) {
        self.shell.settings_open = false;
        self.settings.over = None;
        self.project_props = None;
    }

    /// Back's words: `Back to Draw, page 2`, or the workspace Settings was opened over.
    pub fn settings_back_words(&self) -> String {
        let ws = self.settings.over.unwrap_or(Workspace::Draw);
        let page = self.panel.active.and_then(|a| self.panel.rows().iter().find(|r| r.id == a)).map(|r| r.number.clone());
        match (ws, page) {
            (Workspace::Draw, Some(n)) => format!("Back to Draw, page {n}"),
            (w, _) => format!("Back to {}", w.label()),
        }
    }
}

/// Draw Settings when it is open: between the top and bottom bars, in the foreground, so the page
/// under it answers no hover ([`Trial::over_shown_page`]). Called before the shell's keys: Esc
/// closes it, and every other key is taken once Settings has drawn, so no key reaches the hidden
/// page.
pub fn show(t: &mut Trial, ui: &mut egui::Ui) {
    if !t.shell.settings_open {
        return;
    }
    // A workspace chosen in the top bar (last frame) leaves Settings.
    if t.settings.over.is_some_and(|w| w != t.shell.state.workspace) {
        t.close_settings();
        return;
    }
    let ctx = ui.ctx().clone();
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        t.close_settings();
        return;
    }
    let window = ctx.content_rect();
    let rect = match t.shell.slots {
        Some(s) => Rect::from_min_max(pos2(window.left(), s.top_bar.bottom()), pos2(window.right(), s.status.top())),
        None => window,
    };
    let s = ecad_ui::SHELL;
    let mut close = false;
    let back = t.settings_back_words();
    egui::Area::new(egui::Id::unique("ecad.settings")).fade_in(false).order(egui::Order::Foreground).fixed_pos(rect.min).show(&ctx, |ui| {
        ui.set_min_size(rect.size());
        ui.set_max_size(rect.size());
        ui.painter().rect_filled(rect, 0.0, ecad_ui::color(s.bg));
        // Swallow clicks and hovers on the ground: nothing under the overlay answers.
        let _ = ui.interact(rect, egui::Id::unique("ecad.settings.ground"), egui::Sense::click_and_drag());
        let list = Rect::from_min_size(rect.min, vec2(LIST_W, rect.height()));
        ui.painter().rect_filled(list, 0.0, ecad_ui::color(s.panel));
        ui.painter().vline(list.right() - 0.5, list.y_range(), egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
        let (chosen, back_clicked) = list_ui(t, ui, list, &back);
        close |= back_clicked;
        if let Some(sec) = chosen {
            if sec.item() != t.settings.section.item() {
                t.project_props = None;
            }
            t.settings.section = sec;
        }
        // The title and what the section is for, over a hairline.
        let head = Rect::from_min_max(pos2(list.right(), rect.top()), pos2(rect.right(), rect.top() + HEAD_H));
        let sec = t.settings.section;
        let x = head.left() + space::XL + space::S;
        let y = head.center().y;
        let g = ui.painter().layout_no_wrap(
            sec.label().to_string(),
            ecad_ui::style::ui_font(ecad_ui::tokens::ui_type::OBJECT.size),
            ecad_ui::color(s.text),
        );
        let w = g.size().x;
        let h = g.size().y;
        ui.painter().galley(pos2(x, y - h / 2.0), g, ecad_ui::color(s.text));
        ui.interact(Rect::from_min_size(pos2(x, y - h / 2.0), vec2(w, h)), egui::Id::unique("ecad.settings.title"), egui::Sense::hover())
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, TITLE));
        let about = Rect::from_min_max(pos2(x + w + space::L, head.top()), pos2(head.right() - space::XL, head.bottom()));
        ui.painter().with_clip_rect(about).text(
            pos2(about.left(), y + 2.0),
            egui::Align2::LEFT_CENTER,
            sec.about(),
            bar::font(ui, ui_type::BODY),
            ecad_ui::color(s.text_2),
        );
        ui.painter().hline(head.x_range(), head.bottom() - 0.5, egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
        // Standards lays out its own columns edge to edge; the others sit in a padded body.
        let body = if sec == Section::Standards {
            Rect::from_min_max(pos2(head.left(), head.bottom()), rect.max)
        } else {
            let pad = space::XL + space::S;
            Rect::from_min_max(pos2(head.left() + pad, head.bottom() + space::XL), pos2(rect.right() - pad, rect.bottom() - space::L))
        };
        in_rect(ui, body, "ecad.settings.body", |ui| match sec {
            Section::Standards => standards::show(t, ui),
            Section::Project => project::show(t, ui),
            Section::Pages | Section::TextStyles => project::pages(t, ui),
            Section::Libraries => later::libraries(t, ui),
            Section::AgentAccess => agent::show(t, ui),
            Section::Versions => later::versions(t, ui),
            Section::Display | Section::Developer => display::show(t, ui),
            Section::Shortcuts => later::shortcuts(ui),
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

/// The list: Back to Draw, then the sections in their two groups. Returns the section chosen and
/// whether Back was clicked.
fn list_ui(t: &Trial, ui: &mut egui::Ui, list: Rect, back: &str) -> (Option<Section>, bool) {
    let s = ecad_ui::SHELL;
    let row_h = 28.0;
    let line = egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line));
    // Back to Draw (Esc).
    let back_r = Rect::from_min_size(list.min + vec2(0.0, space::S), vec2(list.width(), 32.0));
    let br = ui.interact(back_r, egui::Id::unique("ecad.settings.back"), egui::Sense::click());
    br.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, CLOSE));
    if br.hovered() {
        ui.painter().rect_filled(back_r.shrink2(vec2(space::XS, 0.0)), ecad_ui::style::corner(4.0), ecad_ui::color(s.hover));
    }
    let y = back_r.center().y;
    ecad_ui::icons::paint(
        ui.painter(),
        Rect::from_center_size(pos2(list.left() + space::L + 6.0, y), vec2(14.0, 14.0)),
        Icon::ChevronLeft,
        ecad_ui::color(s.text),
    );
    bar::text_at(ui, list.left() + space::L + 20.0, y, back, ui_type::BODY, ecad_ui::color(s.text));
    let cap = Rect::from_min_size(pos2(list.right() - space::L - 28.0, y - 9.0), vec2(28.0, 18.0));
    ui.painter().rect_stroke(cap, ecad_ui::style::corner(3.0), egui::Stroke::new(1.0, ecad_ui::color(s.line_strong)), egui::StrokeKind::Inside);
    ui.painter().text(cap.center(), egui::Align2::CENTER_CENTER, "Esc", bar::font(ui, ui_type::CAPTION), ecad_ui::color(s.text_2));
    let mut yy = back_r.bottom() + space::S;
    ui.painter().hline(list.x_range(), yy - 0.5, line);
    yy += space::M;
    let mut chosen = None;
    let current = t.settings.section.item();
    for (i, sec) in Section::ALL.iter().enumerate() {
        if i == 0 || i == Section::PROJECT_COUNT {
            if i > 0 {
                yy += space::L;
            }
            let group = if i == 0 { "Settings · this project" } else { "Settings · this computer" };
            bar::text_at(ui, list.left() + space::L, yy + 8.0, group, ui_type::CAPTION, ecad_ui::color(s.text_3));
            yy += 24.0;
        }
        let r = Rect::from_min_size(pos2(list.left(), yy), vec2(list.width() - 1.0, row_h));
        let on = current == *sec;
        let resp = ui.interact(r, egui::Id::unique(("ecad.settings.item", sec.name())), egui::Sense::click());
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, on, sec.name()));
        if on {
            ui.painter().rect_filled(r, 0.0, ecad_ui::color(s.accent_soft));
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 0.0, ecad_ui::color(s.hover));
        }
        bar::text_at(ui, list.left() + space::L, r.center().y, sec.label(), ui_type::BODY, ecad_ui::color(if on { s.text } else { s.text_2 }));
        if *sec == Section::Standards {
            let n = ecad_cadcraft::defaults::all().count();
            ui.painter().text(
                pos2(r.right() - space::L, r.center().y),
                egui::Align2::RIGHT_CENTER,
                n.to_string(),
                bar::font(ui, ui_type::SMALL),
                ecad_ui::color(s.text_3),
            );
        }
        if resp.clicked() {
            chosen = Some(*sec);
        }
        yy += row_h;
    }
    (chosen, br.clicked())
}

/// A child `Ui` filling `rect`.
pub(crate) fn in_rect<R>(ui: &mut egui::Ui, rect: Rect, id: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope_builder(egui::UiBuilder::new().id_salt(id).max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        ui.set_clip_rect(rect);
        add(ui)
    })
    .inner
}

/// A rev 3 card (board 5.01): a panel-coloured box with its title over a hairline and the contents
/// under it. `right` draws at the title row's right end.
pub(crate) fn card<R>(ui: &mut egui::Ui, title: &str, right: impl FnOnce(&mut egui::Ui), add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let s = ecad_ui::SHELL;
    egui::Frame::new()
        .fill(ecad_ui::color(s.panel))
        .stroke(egui::Stroke::new(1.0, ecad_ui::color(s.line)))
        .corner_radius(8.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let top = ui.cursor().min.y;
            ui.allocate_ui_with_layout(vec2(ui.available_width(), 40.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add_space(space::L);
                ui.label(egui::RichText::new(title).strong().color(ecad_ui::color(s.text)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(space::L);
                    right(ui);
                });
            });
            ui.painter().hline(ui.max_rect().x_range(), top + 40.0, egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
            ui.add_space(space::M);
            egui::Frame::new()
                .inner_margin(egui::Margin { left: space::L as i8, right: space::L as i8, top: 0, bottom: space::L as i8 })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    add(ui)
                })
                .inner
        })
        .inner
}

/// One labelled row in a card: the label at the left in grey, then the contents.
pub(crate) fn row<R>(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let s = ecad_ui::SHELL;
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(118.0, 28.0), egui::Sense::hover());
        ui.painter().text(pos2(r.left(), r.center().y), egui::Align2::LEFT_CENTER, label, bar::font(ui, ui_type::SMALL), ecad_ui::color(s.text_2));
        add(ui)
    })
    .inner
}

/// A row whose value does not exist yet: a dashed box with its words in grey.
pub(crate) fn later_row(ui: &mut egui::Ui, label: &str, words: &str) {
    row(ui, label, |ui| later_box(ui, words, 28.0));
}

/// A dashed box with grey words: something that comes later.
pub(crate) fn later_box(ui: &mut egui::Ui, words: &str, h: f32) {
    let s = ecad_ui::SHELL;
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), egui::Sense::hover());
    dashed_rect(ui.painter(), r.shrink(0.5), ecad_ui::color(s.line_strong));
    ui.painter().text(
        pos2(r.left() + space::M, r.center().y),
        egui::Align2::LEFT_CENTER,
        words,
        bar::font(ui, ui_type::BODY),
        ecad_ui::color(s.text_3),
    );
    resp.on_hover_text("Not available yet");
}

/// A dashed rectangle outline.
pub(crate) fn dashed_rect(p: &egui::Painter, r: Rect, c: egui::Color32) {
    let st = egui::Stroke::new(1.0, c);
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    p.extend(egui::Shape::dashed_line(&pts, st, 4.0, 3.0));
}
