//! A splitter: the handle between two panes that a person drags to share the space (brief 057,
//! owner: Project ▸ Cables' Details pane). One look for every split in the app: a double hairline
//! with a small gap and a grip in the middle, a resize cursor on hover, and the hover colour while
//! hovered or dragged.

use egui::{CursorIcon, Response, Sense, Stroke, Ui, pos2, vec2};

use crate::tokens::{line, space};

/// The handle's height (a horizontal splitter) or width (a vertical one).
pub const THICKNESS: f32 = space::M;
/// The gap between the two hairlines.
const GAP: f32 = 3.0;
/// The grip's length in the middle.
const GRIP: f32 = 28.0;

/// A horizontal splitter across the available width, between a pane above and one below. Drag it
/// up or down: its [`Response::drag_delta`] is the move. `name` is its accessible name.
pub fn horizontal(ui: &mut Ui, name: &str) -> Response {
    let s = crate::SHELL;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), THICKNESS), Sense::drag());
    let resp = resp.on_hover_cursor(CursorIcon::ResizeVertical);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    let hot = resp.hovered() || resp.dragged();
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        if hot {
            p.rect_filled(rect, 0.0, crate::color(s.hover));
        }
        let ink = crate::color(if hot { s.accent } else { s.line_strong });
        let cy = rect.center().y;
        for y in [cy - GAP / 2.0, cy + GAP / 2.0] {
            p.hline(rect.x_range(), y, Stroke::new(line::HAIRLINE_PX, ink));
        }
        // The grip: a short, thicker pair in the middle, so it reads as something to hold.
        let cx = rect.center().x;
        for y in [cy - GAP / 2.0, cy + GAP / 2.0] {
            p.line_segment([pos2(cx - GRIP / 2.0, y), pos2(cx + GRIP / 2.0, y)], Stroke::new(2.0, ink));
        }
    }
    resp
}
