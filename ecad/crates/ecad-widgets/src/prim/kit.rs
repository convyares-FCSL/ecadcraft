//! The shared pieces: the widgets every panel would otherwise rebuild by hand.
//!
//! - [`SectionHeader`]: a folding section's header (chevron, title, summary), keeping its state.
//! - [`property_row`] and [`PropertyGrid`]: a label in the label column and its value.
//! - [`ListRow`]: a selectable list row.
//! - [`caption`], [`heading`], [`body`] and [`note`]: text in the type ramp's styles and tones.
//! - [`Checkbox`] and [`Radio`]: a choice ticked in a box, one of a few in circles.
//! - [`SearchField`]: the search box with its lens.

use egui::{Rect, Response, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, pos2, vec2};
use {crate::tokens::line, crate::tokens::radius, crate::tokens::size, crate::tokens::space, crate::tokens::ui_type};

use crate::prim::{Field, FieldKind, Look, RowKind, RowSpec, Tone, accessible, galley};
use crate::style::corner;
use {crate::color, crate::fonts};

/// A folding section's header, as wide as the panel: `˅ Title ........ summary`. A click folds
/// or unfolds it (`open` is flipped); its accessible name is the title unless [`Self::name`]
/// gives one.
///
/// # Example
/// ```
/// # egui::__run_test_ui(|ui| {
/// let mut open = true;
/// ecad_widgets::prim::SectionHeader::new(&mut open, "Rating").summary("18.8 A ≥ 4 A").show(ui);
/// if open { /* the section's rows */ }
/// # });
/// ```
#[derive(Debug)]
pub struct SectionHeader<'a> {
    open: &'a mut bool,
    title: &'a str,
    summary: &'a str,
    name: Option<&'a str>,
    action: Option<(&'a str, bool)>,
}

/// What [`SectionHeader::show_with_action`] returns.
#[derive(Debug)]
pub struct SectionResponse {
    /// The header's own response (clicked: folded or unfolded).
    pub header: Response,
    /// The action at its right was clicked.
    pub action: bool,
}

impl<'a> SectionHeader<'a> {
    pub fn new(open: &'a mut bool, title: &'a str) -> Self {
        Self { open, title, summary: "", name: None, action: None }
    }
    /// A small quiet button at the right (`New branch`), in place of the summary; `enabled`
    /// false greys it. Read its click from [`Self::show_with_action`].
    pub fn action(mut self, label: &'a str, enabled: bool) -> Self {
        self.action = Some((label, enabled));
        self
    }
    /// The right-aligned summary (`18.8 A ≥ 4 A`, a count).
    pub fn summary(mut self, summary: &'a str) -> Self {
        self.summary = summary;
        self
    }
    /// Its accessible name, when tests or the control channel look it up by another name.
    pub fn name(mut self, name: &'a str) -> Self {
        self.name = Some(name);
        self
    }
    /// Draw it; the response is clicked when it was folded or unfolded.
    pub fn show(self, ui: &mut Ui) -> Response {
        self.show_with_action(ui).header
    }

    /// Draw it with its action, if it has one.
    pub fn show_with_action(self, ui: &mut Ui) -> SectionResponse {
        let summary = if self.action.is_some() { "" } else { self.summary };
        let r = crate::panel::section_header(ui, *self.open, self.title, summary);
        let mut action = false;
        if let Some((label, enabled)) = self.action {
            // Over the header's right end, registered after it so it takes its own clicks.
            let mut b = crate::prim::Button::new(label).kind(crate::prim::Kind::Quiet).small();
            if !enabled {
                b = b.disabled("Not available here");
            }
            let w = galley(ui, label, ui_type::BODY_EMPHASIS).size().x + 2.0 * space::L;
            let at = Rect::from_min_max(pos2(r.rect.right() - w, r.rect.top()), r.rect.max);
            action = ui.put(at, b).clicked();
        }
        if r.clicked() && !action {
            *self.open = !*self.open;
        }
        let (name, open) = (self.name.unwrap_or(self.title), *self.open);
        accessible(&r, || WidgetInfo::selected(Role::Button, true, open, name), None);
        SectionResponse { header: r, action }
    }
}

/// One label-and-value row: the grey label in the label column ([`size::LABEL_COLUMN`]), then
/// `add` in the rest, one control high. The same as [`crate::control::row`].
pub fn property_row<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    crate::control::row(ui, label, add)
}

/// Rows of [`property_row`]s under each other with no gap, so a form's labels and values line up.
///
/// # Example
/// ```
/// # egui::__run_test_ui(|ui| {
/// let mut name = String::from("main");
/// ecad_widgets::prim::PropertyGrid::new().show(ui, |g| {
///     g.row("Name", |ui| ecad_widgets::control::TextEntry::new(&mut name, "Name").show(ui));
///     g.row("Made", |ui| ecad_widgets::control::read_text(ui, "today", Default::default(), "Made"));
/// });
/// # });
/// ```
#[derive(Debug, Default, Clone, Copy)]
pub struct PropertyGrid;

/// The rows of a [`PropertyGrid`] being drawn.
pub struct GridRows<'u> {
    ui: &'u mut Ui,
}

impl std::fmt::Debug for GridRows<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GridRows")
    }
}

impl GridRows<'_> {
    /// One row: `label`, then `add`.
    pub fn row<R>(&mut self, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
        property_row(self.ui, label, add)
    }
    /// The grid's `Ui`, for a line that is not a row (a note under a value).
    pub fn ui(&mut self) -> &mut Ui {
        self.ui
    }
}

impl PropertyGrid {
    pub fn new() -> Self {
        Self
    }
    pub fn show<R>(self, ui: &mut Ui, add: impl FnOnce(&mut GridRows<'_>) -> R) -> R {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            add(&mut GridRows { ui })
        })
        .inner
    }
}

/// A selectable list row ([`RowKind::List`], [`size::ROW_LIST`] high): its text, and the
/// `accent_soft` ground when selected. Clicked selects it.
#[derive(Debug, Clone, Copy)]
pub struct ListRow<'a> {
    text: &'a str,
    name: Option<&'a str>,
    selected: bool,
    dim: bool,
    look: Look,
}

impl<'a> ListRow<'a> {
    pub fn new(text: &'a str) -> Self {
        Self { text, name: None, selected: false, dim: false, look: Look::Auto }
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    /// Secondary text (an archived branch, a closed item).
    pub fn dim(mut self, dim: bool) -> Self {
        self.dim = dim;
        self
    }
    /// Its accessible name, if not the text.
    pub fn name(mut self, name: &'a str) -> Self {
        self.name = Some(name);
        self
    }
    pub fn look(mut self, look: Look) -> Self {
        self.look = look;
        self
    }
    /// Draw it, `trailing` laid at its right (a count, a page); the response is the row's.
    pub fn show_with(self, ui: &mut Ui, trailing: impl FnOnce(&mut Ui)) -> Response {
        let s = crate::SHELL;
        let spec = RowSpec { kind: RowKind::List, selected: self.selected, tint: 0.0, look: self.look };
        let ink = if self.dim { s.text_2 } else { s.text };
        let text = self.text;
        crate::prim::row(ui, spec, self.name.unwrap_or(text), |ui| {
            let g = galley(ui, text, ui_type::BODY);
            let (r, _) = ui.allocate_exact_size(g.size(), Sense::hover());
            ui.painter().galley(r.min, g, color(ink));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), trailing);
        })
        .response
    }
    pub fn show(self, ui: &mut Ui) -> Response {
        self.show_with(ui, |_| {})
    }
}

/// A line of text in a type style and colour, wrapped to the width available.
fn text(ui: &mut Ui, text: &str, style: ui_type::TypeStyle, ink: ecad_model::palette::Color) -> Response {
    ui.add(egui::Label::new(egui::RichText::new(text).font(fonts::font(ui.ctx(), style)).color(color(ink))).wrap())
}

/// A section label or a small note, in the caption style and the label grey.
pub fn caption(ui: &mut Ui, words: &str) -> Response {
    text(ui, words, ui_type::CAPTION, crate::SHELL.text_3)
}

/// A dialog's or a page's heading, in the title style.
pub fn heading(ui: &mut Ui, words: &str) -> Response {
    text(ui, words, ui_type::TITLE, crate::SHELL.text)
}

/// Body text: `secondary` in `text_2`, else `text`.
pub fn body(ui: &mut Ui, words: &str, secondary: bool) -> Response {
    let s = crate::SHELL;
    text(ui, words, ui_type::BODY, if secondary { s.text_2 } else { s.text })
}

/// A short line in a status tone, in the small style: an error under a field (`Tone::Error`), a
/// warning (`Tone::Warning`), a note (`Tone::Neutral`). Its accessible description is the tone.
pub fn note(ui: &mut Ui, words: &str, tone: Tone) -> Response {
    let r = text(ui, words, ui_type::SMALL, tone.text());
    accessible(&r, || WidgetInfo::labeled(Role::Label, true, words), Some(tone.name()));
    r
}

/// The box or circle of a [`Checkbox`] or [`Radio`]: [`size::ICON`] square.
const MARK: f32 = size::ICON;

/// Lay out a mark and its label in one control-high row; returns the row, the mark and the
/// response.
fn marked(ui: &mut Ui, label: &str, enabled: bool) -> (Rect, Rect, Response, std::sync::Arc<egui::Galley>) {
    let g = galley(ui, label, ui_type::BODY);
    let w = MARK + if label.is_empty() { 0.0 } else { space::S + g.size().x };
    let (rect, r) = ui.allocate_exact_size(vec2(w, size::CONTROL), if enabled { Sense::click() } else { Sense::hover() });
    let mark = Rect::from_center_size(pos2(rect.left() + MARK / 2.0, rect.center().y), vec2(MARK, MARK));
    (rect, mark, r, g)
}

/// A yes/no ticked in a box, with its label: for a choice confirmed with the dialog's button
/// (an applied-at-once setting is a [`crate::control::Toggle`]). The response reports `changed`.
#[derive(Debug)]
pub struct Checkbox<'a> {
    on: &'a mut bool,
    label: &'a str,
    look: Look,
}

impl<'a> Checkbox<'a> {
    pub fn new(on: &'a mut bool, label: &'a str) -> Self {
        Self { on, label, look: Look::Auto }
    }
    pub fn look(mut self, look: Look) -> Self {
        self.look = look;
        self
    }
    pub fn show(self, ui: &mut Ui) -> Response {
        let s = crate::SHELL;
        let enabled = ui.is_enabled();
        let (rect, mark, mut r, g) = marked(ui, self.label, enabled);
        if enabled && r.clicked() {
            *self.on = !*self.on;
            r.mark_changed();
        }
        let (on, label) = (*self.on, self.label);
        accessible(&r, || WidgetInfo::selected(Role::CheckBox, enabled, on, label), None);
        if ui.is_rect_visible(rect) {
            let p = ui.painter();
            let hovered = enabled && self.look.hovered(&r);
            let (fill, stroke) = match (on, enabled) {
                (true, true) => (s.accent, Stroke::NONE),
                (true, false) => (s.active, Stroke::NONE),
                (false, _) => (s.raised, Stroke::new(line::HAIRLINE_PX, color(if hovered { s.text_2 } else { s.line_strong }))),
            };
            p.rect(mark, corner(radius::BADGE), color(fill), stroke, StrokeKind::Inside);
            if on {
                let ink = Stroke::new(line::ICON_STROKE_PX, color(if enabled { s.on_accent } else { s.text_3 }));
                let c = mark.center();
                let k = MARK / 4.0;
                let knee = c + vec2(-k * 0.3, k * 0.7);
                p.line_segment([c + vec2(-k * 1.1, 0.0), knee], ink);
                p.line_segment([knee, c + vec2(k * 1.2, -k * 0.9)], ink);
            }
            if enabled && self.look.focused(&r) {
                p.rect_stroke(mark, corner(radius::BADGE), Stroke::new(line::FOCUS_RING_PX, color(s.accent_text)), StrokeKind::Outside);
            }
            let ink = if enabled { s.text } else { s.text_3 };
            p.galley(pos2(mark.right() + space::S, rect.center().y - g.size().y / 2.0), g, color(ink));
        }
        r
    }
}

/// One of a few choices in circles, with its label; the caller keeps which is chosen. A click
/// returns `clicked`. For two to six short choices side by side, prefer
/// [`crate::prim::Segmented`]; a radio list is for choices that each need a sentence.
#[derive(Debug, Clone, Copy)]
pub struct Radio<'a> {
    on: bool,
    label: &'a str,
    look: Look,
}

impl<'a> Radio<'a> {
    pub fn new(on: bool, label: &'a str) -> Self {
        Self { on, label, look: Look::Auto }
    }
    pub fn look(mut self, look: Look) -> Self {
        self.look = look;
        self
    }
    pub fn show(self, ui: &mut Ui) -> Response {
        let s = crate::SHELL;
        let enabled = ui.is_enabled();
        let (rect, mark, r, g) = marked(ui, self.label, enabled);
        let (on, label) = (self.on, self.label);
        accessible(&r, || WidgetInfo::selected(Role::RadioButton, enabled, on, label), None);
        if ui.is_rect_visible(rect) {
            let p = ui.painter();
            let hovered = enabled && self.look.hovered(&r);
            let c = mark.center();
            let rr = MARK / 2.0;
            let ring = if on && enabled {
                s.accent
            } else if hovered {
                s.text_2
            } else {
                s.line_strong
            };
            p.circle(c, rr - line::HAIRLINE_PX, color(s.raised), Stroke::new(line::HAIRLINE_PX, color(ring)));
            if on {
                p.circle_filled(c, rr / 2.0, color(if enabled { s.accent } else { s.text_3 }));
            }
            if enabled && self.look.focused(&r) {
                p.circle_stroke(c, rr + line::FOCUS_RING_PX, Stroke::new(line::FOCUS_RING_PX, color(s.accent_text)));
            }
            let ink = if enabled { s.text } else { s.text_3 };
            p.galley(pos2(mark.right() + space::S, rect.center().y - g.size().y / 2.0), g, color(ink));
        }
        r
    }
}

/// The search box: a lens, the typed text, the placeholder while empty ([`size::SEARCH_FIELD`]
/// high). The response is the editor's.
#[derive(Debug)]
pub struct SearchField<'a> {
    text: &'a mut String,
    name: &'a str,
    placeholder: &'a str,
    width: Option<f32>,
    look: Look,
}

impl<'a> SearchField<'a> {
    pub fn new(text: &'a mut String, name: &'a str) -> Self {
        Self { text, name, placeholder: "Search", width: None, look: Look::Auto }
    }
    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }
    pub fn look(mut self, look: Look) -> Self {
        self.look = look;
        self
    }
    pub fn show(self, ui: &mut Ui) -> Response {
        let mut f = Field::new(self.text, FieldKind::Search, self.name).placeholder(self.placeholder).look(self.look);
        if let Some(w) = self.width {
            f = f.width(w);
        }
        f.show(ui)
    }
}
