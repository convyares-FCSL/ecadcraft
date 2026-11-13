//! Field: the one input look (spec §5, "What you set and what is read"). A value set here looks
//! like this; a value read from the drawing is [`super::read_value`] instead.

use ecad_tokens::{GRAPHITE, Icon, line, radius, size, space, ui_type};
use egui::{Align2, Frame, Rect, Response, RichText, Role, Sense, Shape, Stroke, StrokeKind, TextEdit, Ui, WidgetInfo, pos2, vec2};

use super::{Look, accessible};
use crate::style::corner;
use crate::{color, fonts, icons};

/// What a field holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FieldKind {
    /// Free text, 28 px.
    #[default]
    Text,
    /// A search box with a lens, 32 px.
    Search,
    /// A number, right-aligned; text that is not a number marks the field invalid.
    Number,
}

/// A text, search or number field.
///
/// # States
/// Normal (`raised` ground), focused (2 px `accent_text` ring), invalid (an `error` line under it,
/// the message on hover and as the accessible description), read-only (`panel` ground, `text_2`).
#[derive(Debug)]
pub struct Field<'a> {
    text: &'a mut String,
    kind: FieldKind,
    label: &'a str,
    placeholder: &'a str,
    invalid: Option<&'a str>,
    read_only: bool,
    width: Option<f32>,
    look: Look,
}

/// Why a number field is invalid.
pub const NOT_A_NUMBER: &str = "Not a number";

impl<'a> Field<'a> {
    /// A field editing `text`; `label` is its accessible name.
    pub fn new(text: &'a mut String, kind: FieldKind, label: &'a str) -> Self {
        Self { text, kind, label, placeholder: "", invalid: None, read_only: false, width: None, look: Look::Auto }
    }
    /// Grey text shown while it is empty.
    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }
    /// Mark it invalid with a message.
    pub fn invalid(mut self, message: &'a str) -> Self {
        self.invalid = Some(message);
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

    /// The message that makes the field invalid, if any: the one given, else for a number field
    /// text that does not parse.
    fn problem(&self) -> Option<&'a str> {
        self.invalid.or_else(|| {
            let t = self.text.trim();
            (self.kind == FieldKind::Number && !t.is_empty() && t.parse::<f64>().is_err()).then_some(NOT_A_NUMBER)
        })
    }

    /// Draw it. The response is the text editor's (`changed`, `has_focus`, `lost_focus`).
    pub fn show(self, ui: &mut Ui) -> Response {
        let s = GRAPHITE;
        let problem = self.problem();
        let h = if self.kind == FieldKind::Search { size::SEARCH_FIELD } else { size::CONTROL };
        let w = self.width.unwrap_or(ui.spacing().text_edit_width);
        let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
        // The frame goes behind the editor, which is laid out first.
        let frame = ui.painter().add(Shape::Noop);
        let lens = self.kind == FieldKind::Search;
        let left = rect.left() + space::M + if lens { size::ICON + space::S } else { 0.0 };
        let inner = Rect::from_min_max(pos2(left, rect.top()), pos2(rect.right() - space::M, rect.bottom()));
        let ink = if self.read_only { s.text_2 } else { s.text };
        let font = fonts::font(ui.ctx(), ui_type::BODY);
        let editor = TextEdit::singleline(self.text)
            .frame(Frame::NONE)
            .font(font)
            .text_color(color(ink))
            .hint_text(RichText::new(self.placeholder).color(color(s.text_3)))
            .interactive(!self.read_only)
            .align(if self.kind == FieldKind::Number { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER })
            .desired_width(inner.width());
        let response = ui.place(inner, editor);
        let label = self.label;
        let value = self.text.clone();
        let enabled = ui.is_enabled() && !self.read_only;
        accessible(
            &response,
            || WidgetInfo { current_text_value: Some(value.clone()), ..WidgetInfo::labeled(Role::TextInput, enabled, label) },
            problem,
        );
        let response = match problem {
            Some(message) => response.on_hover_text(message),
            None => response,
        };
        if ui.is_rect_visible(rect) {
            let focused = self.look.focused(&response);
            let hovered = self.look.hovered(&response);
            let fill = if self.read_only { s.panel } else { s.raised };
            let stroke = if focused {
                Stroke::new(line::FOCUS_RING_PX, color(s.accent_text))
            } else if hovered && !self.read_only {
                Stroke::new(line::HAIRLINE_PX, color(s.line_strong))
            } else {
                Stroke::new(line::HAIRLINE_PX, color(s.line))
            };
            let p = ui.painter();
            p.set(frame, egui::epaint::RectShape::new(rect, corner(radius::FIELD), color(fill), stroke, StrokeKind::Inside));
            if problem.is_some() {
                let y = rect.bottom() - line::FOCUS_RING_PX / 2.0;
                p.line_segment(
                    [pos2(rect.left() + radius::FIELD, y), pos2(rect.right() - radius::FIELD, y)],
                    Stroke::new(line::FOCUS_RING_PX, color(s.error)),
                );
            }
            if lens {
                let icon = Rect::from_center_size(pos2(rect.left() + space::M + size::ICON / 2.0, rect.center().y), vec2(size::ICON, size::ICON));
                icons::paint(p, icon, Icon::Search, color(s.text_3));
            }
        }
        response
    }
}

/// A field with no placeholder.
pub fn field(ui: &mut Ui, text: &mut String, kind: FieldKind, label: &str) -> Response {
    Field::new(text, kind, label).show(ui)
}
