//! The component set for values in panels and dialogs (UI rev 4, boards 2.03, 2.05, 2.06, 3.05;
//! style guide `docs/design/components.md`). One component per kind of control, each drawn from
//! tokens in every state: editable, hovered, focused, read-only, disabled, empty.
//!
//! **The rule:** anything the person can change looks editable at a glance (a filled, rounded
//! box that answers the pointer), and a value read from the drawing never looks like an input
//! (plain grey text, no box).
//!
//! - [`TextEntry`]: free text, or a number with its unit (`mm²`, `m`, `A`, `V`, `°C`).
//! - [`Select`]: one of a list, a box with a chevron that opens the list.
//! - [`crate::prim::Segmented`]: one of two to six short choices, all visible.
//! - [`Toggle`]: on or off.
//! - [`ValueBox`]: a value set on this object but edited elsewhere (in the table, on the page):
//!   the box with its value, unit, colour swatch and the hint that says where it comes from.
//! - [`read_text`]: a value read from the drawing, plain. [`crate::prim::read_value`] is the same
//!   with a go-to.
//! - [`link`]: a go-to that names its target (`→ 2/C1`).
//! - [`crate::panel::status_pill`], [`crate::panel::section_header`], and [`crate::prim::Button`]
//!   (primary, secondary, quiet, danger) complete the set.
//!
//! Every box is [`size::CONTROL_SMALL`] high, centred in a [`size::CONTROL`] row, so rows of
//! mixed components line up.

use {crate::tokens::Color, crate::tokens::Icon, crate::tokens::line, crate::tokens::radius, crate::tokens::size, crate::tokens::space, crate::tokens::ui_type};
use egui::{CursorIcon, Rect, Response, RichText, Role, Sense, Stroke, StrokeKind, TextEdit, Ui, WidgetInfo, pos2, vec2};

use crate::prim::{Look, accessible};
use crate::style::corner;
use {crate::color, crate::fonts, crate::icons};

/// What a value is shown with besides its text.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Decor<'a> {
    /// Where the value comes from, right-aligned in the label grey (`auto`, `from category`).
    pub hint: &'a str,
    /// Its unit, after the value (`mm²`).
    pub unit: &'a str,
    /// The object's key identity value (a wire's number): bold.
    pub bold: bool,
    /// A colour swatch before the value: one colour, or two for a two-colour code (GNYE).
    pub swatch: Option<(Color, Option<Color>)>,
}

/// The state a box is drawn in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct BoxState {
    hovered: bool,
    focused: bool,
    disabled: bool,
    invalid: bool,
}

/// The box: `raised`, rounded; a `line_strong` hairline under the pointer, the focus ring when
/// focused, an `error` line when invalid; `panel` when disabled. Returns the box.
fn paint_box(ui: &Ui, rect: Rect, st: BoxState) -> Rect {
    paint_box_with(ui, rect, st, true)
}

/// [`paint_box`], its ground left out when `fill` is false (an editor's ground goes behind it).
fn paint_box_with(ui: &Ui, rect: Rect, st: BoxState, fill: bool) -> Rect {
    let s = crate::SHELL;
    let r = Rect::from_center_size(rect.center(), vec2(rect.width(), size::CONTROL_SMALL.min(rect.height())));
    let p = ui.painter();
    let ground = if !fill {
        egui::Color32::TRANSPARENT
    } else if st.disabled {
        color(s.panel)
    } else {
        color(s.raised)
    };
    let stroke = if st.disabled {
        Stroke::new(line::HAIRLINE_PX, color(s.line))
    } else if st.hovered && !st.focused {
        Stroke::new(line::HAIRLINE_PX, color(s.line_strong))
    } else {
        Stroke::NONE
    };
    p.rect(r, corner(radius::FIELD), ground, stroke, StrokeKind::Inside);
    if st.focused {
        p.rect_stroke(r, corner(radius::FIELD), Stroke::new(line::FOCUS_RING_PX, color(s.accent_text)), StrokeKind::Inside);
    }
    if st.invalid {
        let y = r.bottom() - line::FOCUS_RING_PX / 2.0;
        p.line_segment([pos2(r.left() + radius::FIELD, y), pos2(r.right() - radius::FIELD, y)], Stroke::new(line::FOCUS_RING_PX, color(s.error)));
    }
    r
}

/// Paint `hint` at the right of `room` and the chevron when asked; returns the room left.
fn paint_trailing(ui: &Ui, room: Rect, hint: &str, chevron: bool, disabled: bool) -> Rect {
    let s = crate::SHELL;
    let mut right = room.right();
    if chevron {
        let icon = Rect::from_center_size(pos2(right - size::ICON / 2.0, room.center().y), vec2(size::ICON, size::ICON));
        icons::paint(ui.painter(), icon, Icon::ChevronDown, color(if disabled { s.text_3 } else { s.text_2 }));
        right -= size::ICON + space::XS;
    }
    if !hint.is_empty() {
        let g = crate::table::cells::fitted(ui, hint, ui_type::SMALL, color(s.text_3), (room.width() / 2.0).max(0.0));
        right -= g.size().x;
        ui.painter().galley(pos2(right, room.center().y - g.size().y / 2.0), g, color(s.text_3));
        right -= space::M;
    }
    Rect::from_min_max(room.min, pos2(right.max(room.left()), room.bottom()))
}

/// Paint a swatch at the left of `room`; returns the room left.
fn paint_swatch(ui: &Ui, room: Rect, swatch: Option<(Color, Option<Color>)>) -> Rect {
    let Some((a, b)) = swatch else { return room };
    let s = crate::SHELL;
    let w = size::ICON * 0.75;
    let r =
        Rect::from_min_size(pos2(room.left(), room.center().y - space::XS / 2.0 - line::HAIRLINE_PX), vec2(w, space::XS + 2.0 * line::HAIRLINE_PX));
    let p = ui.painter();
    p.rect_filled(r, corner(0.0), color(a));
    if let Some(b) = b {
        // The second colour as the stripe's middle third, as a two-colour core is marked.
        let third = r.width() / 3.0;
        p.rect_filled(Rect::from_min_max(pos2(r.left() + third, r.top()), pos2(r.right() - third, r.bottom())), corner(0.0), color(b));
    }
    p.rect_stroke(r, corner(0.0), Stroke::new(line::HAIRLINE_PX, color(s.line_strong)), StrokeKind::Outside);
    Rect::from_min_max(pos2(r.right() + space::S, room.top()), room.max)
}

/// Paint `text` (or `—` in the dimmest grey when empty) and its unit, left in `room`.
fn paint_text(ui: &Ui, room: Rect, text: &str, d: &Decor<'_>, ink: Color) {
    let s = crate::SHELL;
    let (shown, ink) = if text.trim().is_empty() { (EMPTY.to_string(), s.text_3) } else { (with_unit(text, d.unit), ink) };
    let style = if d.bold { ui_type::BODY_EMPHASIS } else { ui_type::BODY };
    let g = crate::table::cells::fitted(ui, &shown, style, color(ink), room.width());
    ui.painter().galley(pos2(room.left(), room.center().y - g.size().y / 2.0), g, color(ink));
}

/// What an empty value shows.
pub const EMPTY: &str = "\u{2014}";

/// `text` followed by `unit` unless it already ends with it (`2.5` + `mm²` → `2.5 mm²`).
pub fn with_unit(text: &str, unit: &str) -> String {
    let t = text.trim();
    if unit.is_empty() || t.is_empty() || t == EMPTY || t.ends_with(unit) { t.to_string() } else { format!("{t} {unit}") }
}

/// The room inside a box for its content.
fn inner(r: Rect) -> Rect {
    r.shrink2(vec2(space::M, 0.0))
}

/// Allocate one row-high strip `width` wide (all that is left when `None`).
fn strip(ui: &mut Ui, width: Option<f32>, sense: Sense) -> (Rect, Response) {
    let w = width.unwrap_or_else(|| ui.available_width()).max(0.0);
    ui.allocate_exact_size(vec2(w, size::CONTROL), sense)
}

/// A value read from the drawing: plain `text_2`, no box, its unit after it and its hint at the
/// right. Read-only values never look like inputs.
pub fn read_text(ui: &mut Ui, text: &str, d: Decor<'_>, name: &str) -> Response {
    let s = crate::SHELL;
    let (rect, r) = strip(ui, None, Sense::hover());
    // Named only: a label node announces its name, and a value set on it would replace that.
    accessible(&r, || WidgetInfo::labeled(Role::Label, true, name), None);
    if ui.is_rect_visible(rect) {
        let room = paint_trailing(ui, rect, d.hint, false, false);
        let room = paint_swatch(ui, room, d.swatch);
        paint_text(ui, room, text, &d, s.text_2);
    }
    r
}

/// A value set on this object but edited elsewhere (the table, the page, a dialog): the box with
/// its value, unit, swatch and hint, and a chevron when it is one of a list. It answers the pointer
/// (hover hairline, pointing hand) and reports `clicked`, so it can open where it is edited.
#[derive(Debug, Clone, Copy)]
pub struct ValueBox<'a> {
    text: &'a str,
    name: &'a str,
    decor: Decor<'a>,
    chevron: bool,
    width: Option<f32>,
    look: Look,
}

impl<'a> ValueBox<'a> {
    pub fn new(text: &'a str, name: &'a str) -> Self {
        Self { text, name, decor: Decor::default(), chevron: false, width: None, look: Look::Auto }
    }
    pub fn decor(mut self, decor: Decor<'a>) -> Self {
        self.decor = decor;
        self
    }
    pub fn hint(mut self, hint: &'a str) -> Self {
        self.decor.hint = hint;
        self
    }
    /// One of a list: a chevron at the right.
    pub fn chevron(mut self, chevron: bool) -> Self {
        self.chevron = chevron;
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
        let s = crate::SHELL;
        let disabled = !ui.is_enabled();
        let (rect, r) = strip(ui, self.width, if disabled { Sense::hover() } else { Sense::click() });
        let value = with_unit(self.text, self.decor.unit);
        let name = self.name;
        accessible(&r, || WidgetInfo { current_text_value: Some(value.clone()), ..WidgetInfo::labeled(Role::Button, !disabled, name) }, None);
        let r = if disabled { r } else { r.on_hover_cursor(CursorIcon::PointingHand) };
        if ui.is_rect_visible(rect) {
            let st = BoxState { hovered: !disabled && self.look.hovered(&r), focused: !disabled && self.look.focused(&r), disabled, invalid: false };
            let b = paint_box(ui, rect, st);
            let room = paint_trailing(ui, inner(b), self.decor.hint, self.chevron, disabled);
            let room = paint_swatch(ui, room, self.decor.swatch);
            paint_text(ui, room, self.text, &self.decor, if disabled { s.text_3 } else { s.text });
        }
        r
    }
}

/// A text entry, or a number with its unit. Read-only shows the value as [`read_text`] (no box);
/// disabled keeps the box on the `panel` ground; empty shows the placeholder, or `—`.
#[derive(Debug)]
pub struct TextEntry<'a> {
    text: &'a mut String,
    name: &'a str,
    placeholder: &'a str,
    decor: Decor<'a>,
    number: bool,
    read_only: bool,
    invalid: Option<&'a str>,
    id: Option<egui::Id>,
    width: Option<f32>,
    look: Look,
}

/// Why a number entry is invalid.
pub const NOT_A_NUMBER: &str = "Not a number";

impl<'a> TextEntry<'a> {
    pub fn new(text: &'a mut String, name: &'a str) -> Self {
        Self {
            text,
            name,
            placeholder: "",
            decor: Decor::default(),
            number: false,
            read_only: false,
            invalid: None,
            id: None,
            width: None,
            look: Look::Auto,
        }
    }
    /// A number with `unit` after it; text that is not a number marks it invalid.
    pub fn number(mut self, unit: &'a str) -> Self {
        self.number = true;
        self.decor.unit = unit;
        self
    }
    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }
    pub fn hint(mut self, hint: &'a str) -> Self {
        self.decor.hint = hint;
        self
    }
    pub fn bold(mut self, bold: bool) -> Self {
        self.decor.bold = bold;
        self
    }
    pub fn swatch(mut self, swatch: Option<(Color, Option<Color>)>) -> Self {
        self.decor.swatch = swatch;
        self
    }
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }
    /// Mark it invalid with `message` (its tooltip and accessible description); an empty message
    /// marks it without words, for a caller that shows the message beside it.
    pub fn invalid(mut self, message: Option<&'a str>) -> Self {
        self.invalid = message;
        self
    }
    /// The editor's id, when its edit state must survive a change of layout.
    pub fn id(mut self, id: egui::Id) -> Self {
        self.id = Some(id);
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

    /// Draw it. The response is the editor's (`changed`, `has_focus`, `lost_focus`).
    pub fn show(self, ui: &mut Ui) -> Response {
        let s = crate::SHELL;
        if self.read_only {
            return read_text(ui, self.text, self.decor, self.name);
        }
        let problem = self.invalid.or_else(|| {
            let t = self.text.trim().trim_end_matches(self.decor.unit).trim();
            (self.number && !t.is_empty() && t.replace(',', ".").parse::<f64>().is_err()).then_some(NOT_A_NUMBER)
        });
        let disabled = !ui.is_enabled();
        let (rect, _) = strip(ui, self.width, Sense::hover());
        let b = Rect::from_center_size(rect.center(), vec2(rect.width(), size::CONTROL_SMALL));
        // The box goes behind the editor, which is laid out first.
        let behind = ui.painter().add(egui::Shape::Noop);
        let unit_w = if self.decor.unit.is_empty() {
            0.0
        } else {
            ui.painter().layout_no_wrap(self.decor.unit.to_string(), fonts::font(ui.ctx(), ui_type::BODY), color(s.text_2)).size().x + space::XS
        };
        let room = paint_trailing(ui, inner(b), self.decor.hint, false, disabled);
        let room = paint_swatch(ui, room, self.decor.swatch);
        let body = fonts::font(ui.ctx(), if self.decor.bold { ui_type::BODY_EMPHASIS } else { ui_type::BODY });
        let body_font = body.clone();
        let line_h = ui.fonts_mut(|f| f.row_height(&body)) + space::XXS;
        let edit_rect = Rect::from_min_max(pos2(room.left(), b.center().y - line_h / 2.0), pos2(room.right() - unit_w, b.center().y + line_h / 2.0));
        let placeholder = if self.placeholder.is_empty() { EMPTY } else { self.placeholder };
        let mut edit = TextEdit::singleline(self.text)
            .frame(egui::Frame::NONE)
            .font(body)
            .text_color(color(if disabled { s.text_3 } else { s.text }))
            .hint_text(RichText::new(placeholder).color(color(s.text_3)))
            .desired_width(edit_rect.width());
        if let Some(id) = self.id {
            edit = edit.id(id);
        }
        let r = ui.put(edit_rect, edit);
        let value = self.text.clone();
        let name = self.name;
        // An empty message marks it invalid without words (they are shown beside it).
        let message = problem.filter(|m| !m.is_empty());
        accessible(&r, || WidgetInfo { current_text_value: Some(value.clone()), ..WidgetInfo::labeled(Role::TextInput, !disabled, name) }, message);
        let r = match message {
            Some(m) => r.on_hover_text(m),
            None => r,
        };
        if ui.is_rect_visible(rect) {
            let st = BoxState {
                hovered: !disabled && self.look.hovered(&r),
                focused: !disabled && self.look.focused(&r),
                disabled,
                invalid: problem.is_some(),
            };
            let fill = if st.disabled { s.panel } else { s.raised };
            ui.painter().set(behind, egui::epaint::RectShape::filled(b, corner(radius::FIELD), color(fill)));
            paint_box_with(ui, rect, st, false);
            if unit_w > 0.0 && !self.text.trim().is_empty() {
                // The unit follows the number, as it is read: "2.5 mm²".
                let typed = ui.painter().layout_no_wrap(self.text.clone(), body_font.clone(), color(s.text)).size().x;
                let g = ui.painter().layout_no_wrap(self.decor.unit.to_string(), fonts::font(ui.ctx(), ui_type::BODY), color(s.text_2));
                let x = (edit_rect.left() + typed + space::XS).min(room.right() - g.size().x);
                ui.painter().galley(pos2(x, b.center().y - g.size().y / 2.0), g, color(s.text_2));
            }
        }
        r
    }
}

/// Several lines of text typed here (brief 057: a rating table's CSV rows): the box of a
/// [`TextEntry`], taller, in the code font, scrolling past `rows` lines. The response is the
/// editor's.
#[derive(Debug)]
pub struct TextArea<'a> {
    text: &'a mut String,
    name: &'a str,
    rows: usize,
    look: Look,
}

impl<'a> TextArea<'a> {
    pub fn new(text: &'a mut String, name: &'a str) -> Self {
        Self { text, name, rows: 8, look: Look::Auto }
    }
    /// How many lines show before it scrolls.
    pub fn rows(mut self, rows: usize) -> Self {
        self.rows = rows.clamp(2, 40);
        self
    }
    pub fn look(mut self, look: Look) -> Self {
        self.look = look;
        self
    }
    pub fn show(self, ui: &mut Ui) -> Response {
        let s = crate::SHELL;
        let disabled = !ui.is_enabled();
        let font = egui::FontId::monospace(ui_type::SMALL.size);
        let line_h = ui.fonts_mut(|f| f.row_height(&font));
        let h = line_h * self.rows as f32 + 2.0 * space::S;
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
        let behind = ui.painter().add(egui::Shape::Noop);
        let name = self.name;
        let inner_r = rect.shrink2(vec2(space::M, space::S));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner_r));
        let r = egui::ScrollArea::vertical()
            .id_salt(("ecad.textarea", name))
            .max_height(inner_r.height())
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                ui.add(
                    TextEdit::multiline(self.text)
                        .frame(egui::Frame::NONE)
                        .font(font)
                        .text_color(color(if disabled { s.text_3 } else { s.text }))
                        .desired_width(f32::INFINITY)
                        .desired_rows(self.rows),
                )
            })
            .inner;
        let value_len = self.text.len();
        accessible(
            &r,
            || WidgetInfo {
                current_text_value: Some(format!("{value_len} bytes")),
                ..WidgetInfo::labeled(Role::MultilineTextInput, !disabled, name)
            },
            None,
        );
        if ui.is_rect_visible(rect) {
            let hovered = !disabled && self.look.hovered(&r);
            let focused = !disabled && self.look.focused(&r);
            let fill = if disabled { s.panel } else { s.raised };
            ui.painter().set(behind, egui::epaint::RectShape::filled(rect, corner(radius::FIELD), color(fill)));
            if focused {
                ui.painter().rect_stroke(rect, corner(radius::FIELD), Stroke::new(line::FOCUS_RING_PX, color(s.accent_text)), StrokeKind::Inside);
            } else if hovered || disabled {
                ui.painter().rect_stroke(
                    rect,
                    corner(radius::FIELD),
                    Stroke::new(line::HAIRLINE_PX, color(if disabled { s.line } else { s.line_strong })),
                    StrokeKind::Inside,
                );
            }
        }
        r
    }
}

/// One of a list: a box showing the choice with a chevron; a click opens the list. Read-only shows
/// the choice as [`read_text`]. The response reports `changed` when the choice changes.
#[derive(Debug)]
pub struct Select<'a> {
    selected: &'a mut usize,
    options: &'a [&'a str],
    name: &'a str,
    decor: Decor<'a>,
    placeholder: &'a str,
    read_only: bool,
    width: Option<f32>,
    look: Look,
}

impl<'a> Select<'a> {
    /// A selector over `options`; a `selected` out of range shows the placeholder.
    pub fn new(selected: &'a mut usize, options: &'a [&'a str], name: &'a str) -> Self {
        Self { selected, options, name, decor: Decor::default(), placeholder: "", read_only: false, width: None, look: Look::Auto }
    }
    pub fn hint(mut self, hint: &'a str) -> Self {
        self.decor.hint = hint;
        self
    }
    pub fn swatch(mut self, swatch: Option<(Color, Option<Color>)>) -> Self {
        self.decor.swatch = swatch;
        self
    }
    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
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
        let current = self.options.get(*self.selected).copied().unwrap_or(self.placeholder);
        if self.read_only {
            return read_text(ui, current, self.decor, self.name);
        }
        let mut r = ValueBox::new(current, self.name).decor(self.decor).chevron(true).look(self.look);
        if let Some(w) = self.width {
            r = r.width(w);
        }
        let mut response = r.show(ui);
        let before = *self.selected;
        let options = self.options;
        let selected = &mut *self.selected;
        egui::Popup::menu(&response).show(|ui| {
            ui.set_min_width(size::LABEL_COLUMN * 1.5);
            for (i, o) in options.iter().enumerate() {
                if ui.selectable_label(i == *selected, RichText::new(*o).font(fonts::font(ui.ctx(), ui_type::BODY))).clicked() {
                    *selected = i;
                }
            }
        });
        if *self.selected != before {
            response.mark_changed();
        }
        response
    }
}

/// On or off: a pill track, `accent` when on, with a round knob. Read-only and disabled draw it
/// dimmed and take no clicks. The response reports `changed`.
#[derive(Debug)]
pub struct Toggle<'a> {
    on: &'a mut bool,
    name: &'a str,
    read_only: bool,
    look: Look,
}

impl<'a> Toggle<'a> {
    pub fn new(on: &'a mut bool, name: &'a str) -> Self {
        Self { on, name, read_only: false, look: Look::Auto }
    }
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }
    pub fn look(mut self, look: Look) -> Self {
        self.look = look;
        self
    }
    pub fn show(self, ui: &mut Ui) -> Response {
        let s = crate::SHELL;
        let enabled = ui.is_enabled() && !self.read_only;
        let track = vec2(space::XXL + space::S, space::XL);
        let (rect, at) = ui.allocate_exact_size(vec2(track.x, size::CONTROL), Sense::hover());
        let t = Rect::from_center_size(rect.center(), track);
        let mut r = ui.interact(t, at.id.with("toggle"), if enabled { Sense::click() } else { Sense::hover() });
        if enabled && r.clicked() {
            *self.on = !*self.on;
            r.mark_changed();
        }
        let on = *self.on;
        let name = self.name;
        accessible(&r, || WidgetInfo::selected(Role::CheckBox, enabled, on, name), None);
        if ui.is_rect_visible(t) {
            let p = ui.painter();
            let hovered = enabled && self.look.hovered(&r);
            let ground = match (on, enabled) {
                (true, true) => s.accent,
                (true, false) => s.accent.at_percent(crate::tokens::shell::SOFT_PERCENT * 3),
                (false, _) if hovered => s.line_strong,
                (false, _) => s.active,
            };
            p.rect_filled(t, corner(radius::pill(t.height())), color(ground));
            let k = t.height() / 2.0 - space::XXS;
            let cx = if on { t.right() - t.height() / 2.0 } else { t.left() + t.height() / 2.0 };
            let knob = if enabled { s.on_accent } else { s.text_3 };
            p.circle_filled(pos2(cx, t.center().y), k, color(knob));
            if enabled && self.look.focused(&r) {
                p.rect_stroke(t, corner(radius::pill(t.height())), Stroke::new(line::FOCUS_RING_PX, color(s.accent_text)), StrokeKind::Outside);
            }
        }
        r
    }
}

/// A go-to that names its target (`→ 2/C1`): `accent_text`, underlined under the pointer; its
/// accessible role is a link labelled with the text.
pub fn link(ui: &mut Ui, text: &str) -> Response {
    let s = crate::SHELL;
    let g = ui.painter().layout_no_wrap(text.to_string(), fonts::font(ui.ctx(), ui_type::BODY), color(s.accent_text));
    let (rect, r) = ui.allocate_exact_size(vec2(g.size().x, size::CONTROL), Sense::click());
    accessible(&r, || WidgetInfo::labeled(Role::Link, true, text), None);
    let r = r.on_hover_cursor(CursorIcon::PointingHand);
    if ui.is_rect_visible(rect) {
        let at = rect.left_center() - vec2(0.0, g.size().y / 2.0);
        let size = g.size();
        ui.painter().galley(at, g, color(s.accent_text));
        if r.hovered() {
            let y = at.y + size.y;
            ui.painter().line_segment([pos2(at.x, y), pos2(at.x + size.x, y)], Stroke::new(line::HAIRLINE_PX, color(s.accent_text)));
        }
    }
    r
}

/// A row: the grey label in the label column, then `add` in the rest, one row high.
pub fn row<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.set_min_height(size::CONTROL);
        ui.spacing_mut().item_spacing.x = 0.0;
        crate::panel::label(ui, label);
        ui.spacing_mut().item_spacing.x = space::S;
        add(ui)
    })
    .inner
}

/// The text a [`Decor`]'d value is announced with (value and unit).
pub fn announced(text: &str, d: &Decor<'_>) -> String {
    with_unit(text, d.unit)
}
