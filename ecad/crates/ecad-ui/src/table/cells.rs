//! Drawing one cell from its [`Cell`]: set values look like fields, read values are grey text
//! (a go-to when they have a target), provisional values use the provisional look (spec §5).

use ecad_tokens::{GRAPHITE, line, radius, space, ui_type};
use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{Color32, Galley, Rect, Response, Stroke, StrokeKind, Ui, pos2, vec2};
use std::sync::Arc;

use super::source::{Align, Cell, LaneCell};
use crate::style::corner;
use crate::{color, fonts, prim};

/// Distance between two link lanes.
pub const LANE_STEP: f32 = 8.0;

/// `text` on one line, cut with an ellipsis to `width`.
pub fn fitted(ui: &Ui, text: &str, style: ecad_tokens::ui_type::TypeStyle, ink: Color32, width: f32) -> Arc<Galley> {
    let font_id = fonts::font(ui.ctx(), style);
    let mut job = LayoutJob::single_section(text.to_string(), TextFormat { font_id, color: ink, ..Default::default() });
    job.wrap = TextWrapping { max_width: width.max(0.0), max_rows: 1, break_anywhere: true, overflow_character: Some('\u{2026}') };
    ui.painter().layout_job(job)
}

/// Where a galley of `size` sits in `rect` (padded) for `align`, centred vertically.
fn place(rect: Rect, size: egui::Vec2, align: Align) -> egui::Pos2 {
    let y = rect.center().y - size.y / 2.0;
    match align {
        Align::Left => pos2(rect.left() + space::M, y),
        Align::Right => pos2(rect.right() - space::M - size.x, y),
        Align::Center => pos2(rect.center().x - size.x / 2.0, y),
    }
}

/// Paint plain text in a cell.
pub fn text(ui: &Ui, rect: Rect, t: &str, ink: Color32, align: Align) {
    let g = fitted(ui, t, ui_type::BODY, ink, rect.width() - 2.0 * space::M);
    ui.painter().galley(place(rect, g.size(), align), g, ink);
}

/// The field look of a set value that is not being edited (the field primitive's frame, without
/// an editor, so a table of thousands of rows registers no text editors).
pub fn input_look(ui: &Ui, rect: Rect, t: &str, align: Align, focused: bool) {
    let s = GRAPHITE;
    let frame = rect.shrink2(vec2(space::XS, space::XXS));
    let stroke = if focused { Stroke::new(line::FOCUS_RING_PX, color(s.accent_text)) } else { Stroke::new(line::HAIRLINE_PX, color(s.line)) };
    ui.painter().rect(frame, corner(radius::FIELD), color(s.raised), stroke, StrokeKind::Inside);
    let g = fitted(ui, t, ui_type::BODY, color(s.text), frame.width() - 2.0 * space::M);
    let at = place(frame.shrink2(vec2(space::XS, 0.0)), g.size(), align);
    ui.painter().galley(at, g, color(s.text));
}

/// Link lanes: a vertical line per lane through the rows it spans, a dot on the rows it joins.
pub fn lanes(ui: &Ui, rect: Rect, l: &LaneCell) {
    let ink = color(GRAPHITE.connect);
    let p = ui.painter();
    let stroke = Stroke::new(line::FOCUS_RING_PX, ink);
    for (i, lane) in l.lanes.iter().enumerate() {
        let Some(lane) = lane else { continue };
        let x = rect.left() + space::M + LANE_STEP * i as f32;
        let c = rect.center().y;
        if lane.up {
            p.line_segment([pos2(x, rect.top()), pos2(x, c)], stroke);
        }
        if lane.down {
            p.line_segment([pos2(x, c), pos2(x, rect.bottom())], stroke);
        }
        if lane.on {
            p.circle_filled(pos2(x, c), space::XS * 0.75, ink);
        }
    }
    if !l.text.is_empty() {
        let left = rect.left() + space::M + LANE_STEP * l.lanes.len() as f32;
        let r = Rect::from_min_max(pos2(left, rect.top()), rect.max);
        text(ui, r, &l.text, color(GRAPHITE.text_2), Align::Left);
    }
}

/// Draw `cell` in `rect` of `ui` (the cell's own ui). Returns the go-to link's response for a
/// read cell with a target.
pub fn draw(ui: &mut Ui, rect: Rect, cell: &Cell, align: Align, focused: bool) -> Option<Response> {
    let s = GRAPHITE;
    match cell {
        Cell::Text(t) => text(ui, rect, t, color(s.text), align),
        Cell::Number(..) => text(ui, rect, &cell.text(), color(s.text), Align::Right),
        Cell::Input(t) => input_look(ui, rect, t, align, focused),
        Cell::Read { text: t, go: false } => text(ui, rect, t, color(s.text_2), align),
        Cell::Read { text: t, go: true } => {
            if t.is_empty() {
                return None;
            }
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(space::M, 0.0))).layout(layout(align)));
            child.set_clip_rect(rect.intersect(ui.clip_rect()));
            return Some(prim::read_value(&mut child, t));
        }
        Cell::Provisional(t) => {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(space::M, 0.0))).layout(layout(align)));
            child.set_clip_rect(rect.intersect(ui.clip_rect()));
            prim::provisional(&mut child, t);
        }
        Cell::Badge(t, tone) => {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(space::M, 0.0))).layout(layout(align)));
            prim::badge(&mut child, t, *tone);
        }
        Cell::Swatch(colours) => {
            let side = space::L;
            let mut x = rect.left() + space::M;
            for c in colours {
                let r = Rect::from_min_size(pos2(x, rect.center().y - side / 2.0), vec2(side, side));
                ui.painter().rect(r, corner(radius::CHIP / 2.0), color(*c), Stroke::new(line::HAIRLINE_PX, color(s.line_strong)), StrokeKind::Inside);
                x += side + space::XXS;
            }
        }
        Cell::Lanes(l) => lanes(ui, rect, l),
        Cell::Empty => {}
    }
    None
}

fn layout(align: Align) -> egui::Layout {
    match align {
        Align::Left => egui::Layout::left_to_right(egui::Align::Center),
        Align::Right => egui::Layout::right_to_left(egui::Align::Center),
        Align::Center => egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
    }
}
