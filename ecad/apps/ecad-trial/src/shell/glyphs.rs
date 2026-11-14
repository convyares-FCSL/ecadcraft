//! Small glyphs the left panels draw (UI rev 3, boards 1.02–1.04 and 1.08): the header's pin and
//! its list and thumbnail switch, a row's checkbox and drag grip, a page's thumbnail and a
//! change's lane dot. Drawn in code with the painter, in the shell's colours; they are panel
//! furniture rather than icons of the app, so they stay out of [`ecad_ui::tokens::Icon`].

use eframe::egui::{Color32, Painter, Pos2, Rect, Stroke, StrokeKind, pos2, vec2};

fn stroke(c: Color32) -> Stroke {
    Stroke::new(1.3, c)
}

/// A push pin: a head, a collar and a needle, upright in `r`.
pub fn pin(p: &Painter, r: Rect, c: Color32) {
    let (cx, w, h) = (r.center().x, r.width(), r.height());
    let top = r.top() + h * 0.12;
    let head = Rect::from_min_max(pos2(cx - w * 0.2, top), pos2(cx + w * 0.2, top + h * 0.38));
    p.rect_stroke(head, 1.0, stroke(c), StrokeKind::Inside);
    let collar = top + h * 0.38;
    p.line_segment([pos2(cx - w * 0.34, collar), pos2(cx + w * 0.34, collar)], stroke(c));
    p.line_segment([pos2(cx, collar), pos2(cx, r.bottom() - h * 0.06)], stroke(c));
}

/// List view: three lines.
pub fn list(p: &Painter, r: Rect, c: Color32) {
    let r = r.shrink(r.width() * 0.15);
    for i in 0..3 {
        let y = r.top() + r.height() * (0.18 + 0.32 * i as f32);
        p.line_segment([pos2(r.left(), y), pos2(r.right(), y)], stroke(c));
    }
}

/// Thumbnail view: a small page beside two lines, twice.
pub fn thumbs(p: &Painter, r: Rect, c: Color32) {
    let r = r.shrink(r.width() * 0.12);
    let half = r.height() / 2.0;
    for i in 0..2 {
        let top = r.top() + half * i as f32;
        let page = Rect::from_min_size(pos2(r.left(), top + 1.0), vec2(r.width() * 0.4, half - 2.0));
        p.rect_stroke(page, 0.5, stroke(c), StrokeKind::Inside);
        let y = page.center().y;
        p.line_segment([pos2(page.right() + 2.0, y), pos2(r.right(), y)], stroke(c));
    }
}

/// A checkbox in `r`: ticked and filled in `accent` when `on`, else an outline with a faint tick.
pub fn checkbox(p: &Painter, r: Rect, on: bool, accent: Color32, line: Color32, faint: Color32) {
    if on {
        p.rect_filled(r, 2.0, accent);
        tick(p, r.shrink(2.5), Color32::WHITE);
    } else {
        p.rect_stroke(r, 2.0, Stroke::new(1.0, line), StrokeKind::Inside);
        tick(p, r.shrink(3.0), faint);
    }
}

/// A tick.
pub fn tick(p: &Painter, r: Rect, c: Color32) {
    let a = pos2(r.left(), r.center().y);
    let b = pos2(r.left() + r.width() * 0.38, r.bottom() - r.height() * 0.1);
    let d = pos2(r.right(), r.top() + r.height() * 0.12);
    p.line_segment([a, b], Stroke::new(1.5, c));
    p.line_segment([b, d], Stroke::new(1.5, c));
}

/// A drag grip: two columns of three dots.
pub fn grip(p: &Painter, r: Rect, c: Color32) {
    for col in 0..2 {
        for row in 0..3 {
            let at = pos2(r.center().x - 2.0 + 4.0 * col as f32, r.center().y - 4.0 + 4.0 * row as f32);
            p.circle_filled(at, 0.9, c);
        }
    }
}

/// A page thumbnail: the sheet's ground with a frame and a schematic sketch (three rails and a
/// box, as the boards draw it); outlined in `outline` (the open page's blue) when given.
pub fn thumbnail(p: &Painter, r: Rect, paper: Color32, ink: Color32, outline: Option<Color32>) {
    p.rect_filled(r, 1.0, paper);
    let inner = r.shrink(r.width() * 0.1);
    let x = |f: f32| inner.left() + inner.width() * f;
    let y = |f: f32| inner.top() + inner.height() * f;
    let thin = Stroke::new(1.0, ink);
    for f in [0.14, 0.28, 0.42] {
        p.line_segment([pos2(x(f), y(0.05)), pos2(x(f), y(0.95))], thin);
    }
    let body = Rect::from_min_max(pos2(x(0.6), y(0.2)), pos2(x(0.86), y(0.8)));
    p.rect_stroke(body, 0.0, thin, StrokeKind::Inside);
    p.line_segment([pos2(body.center().x, y(0.05)), pos2(body.center().x, body.top())], thin);
    p.line_segment([pos2(body.center().x, body.bottom()), pos2(body.center().x, y(0.95))], thin);
    match outline {
        Some(c) => {
            p.rect_stroke(r.expand(1.0), 2.0, Stroke::new(1.5, c), StrokeKind::Inside);
        }
        None => {
            p.rect_stroke(r, 1.0, Stroke::new(1.0, ink.gamma_multiply(0.6)), StrokeKind::Inside);
        }
    }
}

/// A larger page preview (the page well's hover): frame, title block and the schematic sketch.
pub fn preview(p: &Painter, r: Rect, paper: Color32, ink: Color32) {
    p.rect_filled(r, 2.0, paper);
    let frame = r.shrink(6.0);
    p.rect_stroke(frame, 0.0, Stroke::new(1.0, ink), StrokeKind::Inside);
    let thin = Stroke::new(1.0, ink);
    let x = |f: f32| frame.left() + frame.width() * f;
    let y = |f: f32| frame.top() + frame.height() * f;
    for f in [0.1, 0.17, 0.24, 0.31, 0.38] {
        p.line_segment([pos2(x(f), y(0.12)), pos2(x(f), y(0.7))], thin);
    }
    let dash = Stroke::new(1.0, ink.gamma_multiply(0.7));
    p.line_segment([pos2(x(0.05), y(0.3)), pos2(x(0.45), y(0.3))], dash);
    for i in 0..3 {
        let b = Rect::from_min_size(pos2(x(0.55), y(0.18 + 0.16 * i as f32)), vec2(frame.width() * 0.35, frame.height() * 0.1));
        p.rect_stroke(b, 0.0, thin, StrokeKind::Inside);
    }
    let block = Rect::from_min_max(pos2(x(0.55), y(0.8)), pos2(x(1.0), y(1.0)));
    p.rect_stroke(block, 0.0, thin, StrokeKind::Inside);
}

/// A lane dot of the Changes graph: filled in the lane's colour, ringed in the panel's ground.
pub fn lane_dot(p: &Painter, at: Pos2, c: Color32, ground: Color32) {
    p.circle_filled(at, 4.0, ground);
    p.circle_stroke(at, 3.0, Stroke::new(1.5, c));
}
