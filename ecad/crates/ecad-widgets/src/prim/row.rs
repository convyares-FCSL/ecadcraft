//! Row, and the set-versus-read pair: [`read_value`] for a value read from the drawing,
//! [`provisional`] for a provisional value.

use ecad_tokens::{line, size, space, ui_type};
use egui::{CursorIcon, InnerResponse, Layout, Rect, Response, Role, Sense, Stroke, Ui, UiBuilder, WidgetInfo, pos2, vec2};

use super::{Look, accessible, galley};
use crate::color;

/// The kind of a row, which sets its height.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowKind {
    /// A list row, 28 px.
    #[default]
    List,
    /// A table row, 30 px.
    Table,
    /// A group row (a heading over rows), 32 px on the `raised` ground.
    Group,
}

impl RowKind {
    pub fn height(self) -> f32 {
        match self {
            RowKind::List => size::ROW_LIST,
            RowKind::Table => size::ROW_TABLE,
            RowKind::Group => size::ROW_GROUP,
        }
    }
}

/// How a row is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RowSpec {
    pub kind: RowKind,
    /// Selected: the `accent_soft` ground.
    pub selected: bool,
    /// The change tint (design §8.3), 0 (none) to 1 (just changed); it fades as the caller lowers it.
    pub tint: f32,
    /// A forced look (the showcase's hovered row).
    pub look: Look,
}

/// A full-width row of `spec.kind`'s height, its contents laid out left to right and centred
/// vertically. The response is the row's own (clicked selects it); widgets inside take their
/// own clicks. `label` is its accessible name.
pub fn row<R>(ui: &mut Ui, spec: RowSpec, label: &str, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    let s = crate::SHELL;
    let rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), spec.kind.height()));
    // The row is registered before its contents, so they sit above it and take their own clicks.
    let response = ui.interact(rect, ui.next_auto_id().with("ecad.row"), Sense::click());
    let selected = spec.selected;
    accessible(&response, || WidgetInfo::selected(Role::Row, true, selected, label), None);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let ground = if spec.selected {
            Some(s.accent_soft)
        } else if spec.look.hovered(&response) {
            Some(s.hover)
        } else if spec.kind == RowKind::Group {
            Some(s.raised)
        } else {
            None
        };
        if let Some(g) = ground {
            p.rect_filled(rect, 0.0, color(g));
        }
        let tint = spec.tint.clamp(0.0, 1.0);
        if tint > 0.0 {
            let a = (f32::from(s.accent_soft.a) * tint).round().clamp(0.0, 255.0) as u8;
            p.rect_filled(rect, 0.0, color(ecad_tokens::Color { a, ..s.accent_soft }));
        }
        p.line_segment(
            [pos2(rect.left(), rect.bottom() - 0.5), pos2(rect.right(), rect.bottom() - 0.5)],
            Stroke::new(line::HAIRLINE_PX, color(s.line)),
        );
    }
    let inner_rect = rect.shrink2(vec2(space::M, 0.0));
    let mut child = ui.new_child(UiBuilder::new().max_rect(inner_rect).layout(Layout::left_to_right(egui::Align::Center)));
    let inner = add_contents(&mut child);
    ui.advance_cursor_after_rect(rect);
    InnerResponse::new(inner, response)
}

/// A value read from the drawing (spec §5): plain `text_2` with no box, and a go-to: hovering
/// underlines it, a click returns `clicked`. Its accessible role is a link labelled with the text.
pub fn read_value(ui: &mut Ui, text: &str) -> Response {
    let s = crate::SHELL;
    let g = galley(ui, text, ui_type::BODY);
    let (rect, response) = ui.allocate_exact_size(g.size(), Sense::click());
    accessible(&response, || WidgetInfo::labeled(Role::Link, true, text), None);
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let ink = if hovered { s.text } else { s.text_2 };
        let p = ui.painter();
        p.galley(rect.min, g, color(ink));
        if hovered {
            p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(line::HAIRLINE_PX, color(ink)));
        }
    }
    response
}

/// A provisional value: the provisional colour with a dotted underline, everywhere (page, list,
/// inspector). Its accessible description is "provisional".
pub fn provisional(ui: &mut Ui, text: &str) -> Response {
    let ink = color(crate::SHELL.provisional);
    let g = galley(ui, text, ui_type::BODY);
    let (rect, response) = ui.allocate_exact_size(g.size() + vec2(0.0, space::XXS), Sense::hover());
    accessible(&response, || WidgetInfo::labeled(Role::Label, true, text), Some("provisional"));
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        p.galley(rect.min, g, ink);
        // Dots: 1 px on, 2 px off, on the line under the text.
        let y = rect.bottom() - line::HAIRLINE_PX / 2.0;
        let mut x = rect.left();
        while x < rect.right() {
            let end = (x + line::HAIRLINE_PX).min(rect.right());
            p.line_segment([pos2(x, y), pos2(end, y)], Stroke::new(line::HAIRLINE_PX, ink));
            x += space::XXS + line::HAIRLINE_PX;
        }
    }
    response
}
