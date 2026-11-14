//! The building blocks of a right-hand panel: the Draw inspector and the Project panel (UI rev 4,
//! boards 2.03, 2.05 and 2.06). One set, so both panels line up alike:
//!
//! - [`section_header`]: a fold chevron, the title, and a right-aligned summary ("18.8 A ≥ 4 A").
//! - [`label`]: the grey label in the label column ([`size::LABEL_COLUMN`]).
//! - [`field_box`]: the input box a value set here sits in, with its hint at the right ("auto",
//!   "from category").
//! - [`value_row`]: a whole row: label, then the value boxed when it is set here, grey and unboxed
//!   when it is read from the drawing, dimmer still when it is not built yet.
//! - [`status_pill`]: a one-line status ("✓ No checks on L2-03"): a mark in its tone, neutral words.
//! - [`rule`]: the hairline between sections, edge to edge of the panel.
//!
//! Every row is [`size::CONTROL`] high and its box [`size::CONTROL_SMALL`], centred, so rows of
//! either panel share one pitch.

use ecad_tokens::{Icon, line, radius, size, space, ui_type};
use egui::{Rangef, Rect, Response, Sense, Stroke, Ui, pos2, vec2};

use crate::prim::Tone;
use crate::style::corner;
use crate::{color, fonts};

/// A section's header, as wide as the panel: `˅ Title ........ summary`. Returns its response,
/// sensing clicks; the caller keeps whether it is open and gives it its accessible name.
pub fn section_header(ui: &mut Ui, open: bool, title: &str, summary: &str) -> Response {
    let s = crate::SHELL;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), size::CONTROL), Sense::click());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let p = ui.painter();
    let chevron = Rect::from_min_size(pos2(rect.left(), rect.center().y - size::ICON / 2.0), vec2(size::ICON, size::ICON));
    let icon = if open { Icon::ChevronDown } else { Icon::ChevronRight };
    crate::icons::paint(p, chevron, icon, color(if resp.hovered() { s.text } else { s.text_3 }));
    let title_g = p.layout_no_wrap(title.to_string(), fonts::font(ui.ctx(), ui_type::BODY_EMPHASIS), color(s.text));
    let title_x = chevron.right() + space::XS;
    let title_w = title_g.size().x;
    p.galley(pos2(title_x, rect.center().y - title_g.size().y / 2.0), title_g, color(s.text));
    if !summary.is_empty() {
        let room = (rect.right() - title_x - title_w - space::L).max(0.0);
        let g = crate::table::cells::fitted(ui, summary, ui_type::SMALL, color(s.text_3), room);
        ui.painter().galley(pos2(rect.right() - g.size().x, rect.center().y - g.size().y / 2.0), g, color(s.text_3));
    }
    resp
}

/// The grey label in the label column, as high as a row.
pub fn label(ui: &mut Ui, text: &str) -> Response {
    let s = crate::SHELL;
    let (rect, resp) = ui.allocate_exact_size(vec2(size::LABEL_COLUMN, size::CONTROL), Sense::hover());
    if ui.is_rect_visible(rect) {
        let room = (rect.width() - space::S).max(0.0);
        let g = crate::table::cells::fitted(ui, text, ui_type::SMALL, color(s.text_3), room);
        ui.painter().galley(pos2(rect.left(), rect.center().y - g.size().y / 2.0), g, color(s.text_3));
    }
    resp
}

/// The box a value set here sits in, `rect` being the row's value column: raised and rounded,
/// [`size::CONTROL_SMALL`] high, centred, with `hint` at its right in the label grey. Returns the
/// box and the room left for the value inside it (padding and the hint taken off).
pub fn field_box(ui: &Ui, rect: Rect, hint: &str) -> (Rect, Rect) {
    let s = crate::SHELL;
    let r = Rect::from_center_size(rect.center(), vec2(rect.width(), size::CONTROL_SMALL.min(rect.height())));
    let p = ui.painter();
    p.rect_filled(r, corner(radius::FIELD), color(s.raised));
    let mut right = r.right() - space::M;
    if !hint.is_empty() {
        let room = (r.width() / 2.0).max(0.0);
        let g = crate::table::cells::fitted(ui, hint, ui_type::SMALL, color(s.text_3), room);
        right -= g.size().x;
        p.galley(pos2(right, r.center().y - g.size().y / 2.0), g, color(s.text_3));
        right -= space::M;
    }
    (r, Rect::from_min_max(pos2(r.left() + space::M, r.top()), pos2(right.max(r.left() + space::M), r.bottom())))
}

/// How a value row's value is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ValueKind {
    /// Set here: in an input box, primary text.
    Set,
    /// Read from the drawing: grey, no box (change it on its page).
    #[default]
    Read,
    /// Will be set here, not built yet: an input box with its value in the dimmest grey.
    Later,
}

/// A one-line text input in a field box filling `rect` (a row's value column), `hint` at its
/// right. `edit` is the caller's (its id, placeholder); this draws it frameless in the body type,
/// with the focus ring when focused and an `error` line when `invalid`. Its accessible name is
/// `name`, disabled when the `Ui` is.
pub fn input(ui: &mut Ui, rect: Rect, edit: egui::TextEdit<'_>, hint: &str, name: &str, invalid: bool) -> Response {
    let s = crate::SHELL;
    let (bx, room) = field_box(ui, rect, hint);
    let body = fonts::font(ui.ctx(), ui_type::BODY);
    let line_h = ui.fonts_mut(|f| f.row_height(&body)) + space::XXS;
    let top = bx.center().y - line_h / 2.0;
    let edit_rect = Rect::from_min_max(pos2(room.left(), top), pos2(room.right(), top + line_h));
    let r = ui.put(edit_rect, edit.frame(egui::Frame::NONE).font(body).text_color(color(s.text)));
    let enabled = ui.is_enabled();
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, enabled, name));
    if invalid {
        ui.painter().rect_stroke(bx, corner(radius::FIELD), Stroke::new(line::HAIRLINE_PX, color(s.error)), egui::StrokeKind::Outside);
    } else if r.has_focus() {
        ui.painter().rect_stroke(bx, corner(radius::FIELD), Stroke::new(line::FOCUS_RING_PX, color(s.accent_text)), egui::StrokeKind::Inside);
    }
    r
}

/// A label and a text input ([`input`]) as one row, as wide as the panel.
pub fn input_row(ui: &mut Ui, label_text: &str, edit: egui::TextEdit<'_>, hint: &str, name: &str, invalid: bool) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), size::CONTROL), Sense::hover());
    let s = crate::SHELL;
    let lg = crate::table::cells::fitted(ui, label_text, ui_type::SMALL, color(s.text_3), size::LABEL_COLUMN - space::S);
    ui.painter().galley(pos2(rect.left(), rect.center().y - lg.size().y / 2.0), lg, color(s.text_3));
    let column = Rect::from_min_max(pos2(rect.left() + size::LABEL_COLUMN, rect.top()), rect.max);
    input(ui, column, edit, hint, name, invalid)
}

/// One label-and-value row, as wide as the panel. Its accessible name is `"<label>: <value>"`.
pub fn value_row(ui: &mut Ui, label_text: &str, value: &str, hint: &str, kind: ValueKind) -> Response {
    let s = crate::SHELL;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), size::CONTROL), Sense::hover());
    let name = format!("{label_text}: {value}");
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &name));
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let p = ui.painter();
    let lg = crate::table::cells::fitted(ui, label_text, ui_type::SMALL, color(s.text_3), size::LABEL_COLUMN - space::S);
    p.galley(pos2(rect.left(), rect.center().y - lg.size().y / 2.0), lg, color(s.text_3));
    let column = Rect::from_min_max(pos2(rect.left() + size::LABEL_COLUMN, rect.top()), rect.max);
    let (room, ink) = match kind {
        ValueKind::Set => (field_box(ui, column, hint).1, s.text),
        ValueKind::Later => (field_box(ui, column, hint).1, s.text_3),
        ValueKind::Read => {
            let ink = s.text_2;
            let mut right = column.right();
            if !hint.is_empty() {
                let g = crate::table::cells::fitted(ui, hint, ui_type::SMALL, color(s.text_3), column.width() / 2.0);
                right -= g.size().x;
                ui.painter().galley(pos2(right, column.center().y - g.size().y / 2.0), g, color(s.text_3));
                right -= space::M;
            }
            (Rect::from_min_max(column.min, pos2(right.max(column.left()), column.bottom())), ink)
        }
    };
    let g = crate::table::cells::fitted(ui, value, ui_type::BODY, color(ink), room.width());
    ui.painter().galley(pos2(room.left(), room.center().y - g.size().y / 2.0), g, color(ink));
    resp
}

/// A one-line status pill: a tick in `ok` for [`Tone::Ok`], else a dot in the tone, then the
/// words in secondary text on the raised ground. `name` is its accessible name, the tone its
/// description.
pub fn status_pill(ui: &mut Ui, tone: Tone, words: &str, name: &str) -> Response {
    let s = crate::SHELL;
    let g = ui.painter().layout_no_wrap(words.to_string(), fonts::font(ui.ctx(), ui_type::SMALL), color(s.text_2));
    let h = size::CONTROL_SMALL;
    let mark = size::ICON * 0.75;
    let w = space::M + mark + space::XS + g.size().x + space::M;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    crate::prim::accessible(&resp, || egui::WidgetInfo::labeled(egui::Role::Label, true, name), Some(tone.name()));
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let p = ui.painter();
    p.rect_filled(rect, corner(radius::pill(h)), color(s.raised));
    let c = pos2(rect.left() + space::M + mark / 2.0, rect.center().y);
    let ink = color(tone.text());
    if tone == Tone::Ok {
        let k = mark / 2.0;
        let stroke = Stroke::new(line::ICON_STROKE_PX, ink);
        let knee = c + vec2(-k * 0.2, k * 0.5);
        p.line_segment([c + vec2(-k * 0.8, 0.0), knee], stroke);
        p.line_segment([knee, c + vec2(k * 0.9, -k * 0.6)], stroke);
    } else {
        p.circle_filled(c, mark / 4.0, ink);
    }
    p.galley(pos2(rect.left() + space::M + mark + space::XS, rect.center().y - g.size().y / 2.0), g, color(s.text_2));
    resp
}

/// A hairline between sections across `full` (the panel's whole width), with [`space::S`] above
/// and below.
pub fn rule(ui: &mut Ui, full: Rangef) {
    ui.add_space(space::S);
    hairline(ui, full);
    ui.add_space(space::S);
}

/// Just the hairline across `full`, taking its own height.
pub fn hairline(ui: &mut Ui, full: Rangef) {
    let s = crate::SHELL;
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), line::HAIRLINE_PX), Sense::hover());
    // Past the panel's padding, which the content's clip rect leaves out (a painter's own
    // `with_clip_rect` can only narrow it): the same layer, clipped to the panel's width and the
    // content's height.
    let clip = Rect::from_x_y_ranges(full, ui.clip_rect().y_range());
    let wide = egui::Painter::new(ui.ctx().clone(), ui.layer_id(), clip);
    wide.hline(full, r.center().y, Stroke::new(line::HAIRLINE_PX, color(s.line)));
}

/// Space before a section's body, after its header.
pub const BODY_GAP: f32 = space::XXS;
