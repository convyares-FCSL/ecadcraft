//! The building blocks of a right-hand panel: the Draw inspector and the Project panel (UI rev 4,
//! boards 2.03, 2.05 and 2.06). One set, so both panels line up alike:
//!
//! - [`section_header`]: a fold chevron, the title, and a right-aligned summary ("18.8 A ≥ 4 A").
//! - [`label`]: the grey label in the label column ([`size::LABEL_COLUMN`]).
//! - [`field_box`]: the input box a value set here sits in, with its hint at the right ("auto",
//!   "from category").
//! - [`value_row`]: a whole row: label, then the value boxed when it is set on the object, plain
//!   grey when it is read from the drawing, a dim box when it is not built yet.
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

/// One label-and-value row, as wide as the panel ([`crate::control`]): a [`ValueKind::Set`] value
/// is a [`crate::control::ValueBox`] (with a chevron when `chevron`), a [`ValueKind::Read`] one is
/// [`crate::control::read_text`], a [`ValueKind::Later`] one a disabled box. Its accessible name
/// is `"<label>: <value>"`.
pub fn value_row(ui: &mut Ui, label_text: &str, value: &str, decor: crate::control::Decor<'_>, kind: ValueKind, chevron: bool) -> Response {
    let name = format!("{label_text}: {value}");
    crate::control::row(ui, label_text, |ui| match kind {
        ValueKind::Set => crate::control::ValueBox::new(value, &name).decor(decor).chevron(chevron).show(ui),
        ValueKind::Read => crate::control::read_text(ui, value, decor, &name),
        ValueKind::Later => ui.add_enabled_ui(false, |ui| crate::control::ValueBox::new(value, &name).decor(decor).chevron(chevron).show(ui)).inner,
    })
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
