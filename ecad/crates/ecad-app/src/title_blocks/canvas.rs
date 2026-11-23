//! The editor's main view: the block on paper at print size,
//! zoomed to fit, with 20 mm of the frame around it; fields as dashed boxes; the tools.
//!
//! Coordinates: the block's own millimetres, from its bottom-left corner, y up ([`View`]).

use ecad_electrical::title_blocks::bind;
use ecad_model::objects::title_block::{Align, BlockItem, Ink, LineWeight, Newest, Rect as BRect, TextDraw, TitleBlockLayout};
use ecad_model::{Primitive, Pt};
use ecad_widgets::tokens::{space, ui_type};
use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Shape, Stroke, pos2, vec2};

use crate::library::browser;
use crate::shell::bar;
use crate::title_blocks::draft::{self, Draft};
use crate::title_blocks::list::{Drops, PALETTE};
use crate::title_blocks::{Act, DxfAsk, Editor, Sample, Tool};

/// Element names.
pub(crate) const LOGO_PATH: &str = "library.tb.logo.path";
pub(crate) const LOGO_GO: &str = "library.tb.logo.go";
/// The footer's height.
pub(crate) const FOOTER_H: f32 = 32.0;
/// The frame shown around the block (mm).
pub(crate) const AROUND_MM: f64 = 20.0;
/// A handle's half size (px).
const HANDLE: f32 = 4.0;

fn c32(c: ecad_widgets::tokens::Color) -> Color32 {
    ecad_widgets::color(c)
}

/// Block millimetres to the screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// The screen point of the block's (0, 0).
    pub o: Pos2,
    /// Pixels per millimetre.
    pub k: f32,
}

impl View {
    pub fn at(&self, x: f64, y: f64) -> Pos2 {
        pos2(self.o.x + x as f32 * self.k, self.o.y - y as f32 * self.k)
    }
    pub fn mm(&self, p: Pos2) -> (f64, f64) {
        (f64::from((p.x - self.o.x) / self.k), f64::from((self.o.y - p.y) / self.k))
    }
    pub fn rect(&self, r: &BRect) -> Rect {
        Rect::from_two_pos(self.at(r.x, r.y), self.at(r.right(), r.top()))
    }
}

/// A drag in progress.
#[derive(Debug, Clone, PartialEq)]
pub enum Drag {
    /// Moving the selection; `last`: the snapped point already applied.
    Move {
        last: (f64, f64),
    },
    /// Resizing item `item` by handle `h` (0–7, clockwise from the top-left).
    Resize {
        item: usize,
        h: u8,
        start: BRect,
    },
    /// A line's end `end` (0 or 1).
    LineEnd {
        item: usize,
        end: u8,
    },
    Window {
        from: Pos2,
    },
    /// The Line tool; `rect` with Shift.
    Line {
        from: (f64, f64),
    },
    /// The Field and Logo tools' box.
    Box {
        from: (f64, f64),
    },
}

/// The canvas's state between frames.
#[derive(Debug, Clone, Default)]
pub struct CanvasState {
    /// A palette row being dragged.
    pub palette: Option<usize>,
    /// A palette row to drop at the next free place.
    pub drop_free: Option<usize>,
    pub drag: Option<Drag>,
    pub frame_button: Option<Rect>,
    /// The Field tool's box waiting for a binding, and where its menu opens.
    pub field_menu: Option<(BRect, Pos2)>,
    pub view: Option<View>,
    /// A text item whose text the inspector should focus.
    pub edit_text: Option<usize>,
    /// The point a snap dot shows at.
    pub snapped: Option<(f64, f64)>,
}

/// The next free place in the block's top-left for a box of `size` (its top-left corner).
pub(crate) fn free_place(l: &TitleBlockLayout, size: (f64, f64)) -> (f64, f64) {
    let (w, h) = l.size;
    let taken: Vec<BRect> = l.items.iter().filter_map(|i| i.bounds()).collect();
    let mut y = h - 1.0;
    while y - size.1 >= 0.0 {
        let mut x = 1.0;
        while x + size.0 <= w {
            let r = BRect::new(x, y - size.1, size.0, size.1);
            if !taken.iter().any(|t| t.x < r.right() && r.x < t.right() && t.y < r.top() && r.y < t.top()) {
                return (x, y);
            }
            x += 2.0;
        }
        y -= 2.0;
    }
    (1.0, h - 1.0)
}

/// `p` snapped: to item edges and line ends within 1.5 mm, else to 1 mm.
/// Also whether it snapped to an item.
pub(crate) fn snap_point(p: (f64, f64), l: &TitleBlockLayout, on: bool) -> ((f64, f64), bool) {
    if !on {
        return ((draft::snap(p.0, false), draft::snap(p.1, false)), false);
    }
    let mut best: Option<((f64, f64), f64)> = None;
    let mut consider = |q: (f64, f64)| {
        let d = (q.0 - p.0).hypot(q.1 - p.1);
        if d <= draft::SNAP_NEAR_MM && best.is_none_or(|b| d < b.1) {
            best = Some((q, d));
        }
    };
    let (w, h) = l.size;
    for q in [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)] {
        consider(q);
    }
    for i in l.items.iter().take(ecad_model::objects::title_block::MAX_ITEMS) {
        match i {
            BlockItem::Line { a, b, .. } => {
                consider(*a);
                consider(*b);
            }
            _ => {
                if let Some(r) = i.bounds() {
                    for q in [(r.x, r.y), (r.right(), r.y), (r.x, r.top()), (r.right(), r.top())] {
                        consider(q);
                    }
                }
            }
        }
    }
    // Edges of the block: x or y alone.
    let mut x = draft::snap(p.0, true);
    let mut y = draft::snap(p.1, true);
    if let Some((q, _)) = best {
        return (q, true);
    }
    for edge in [0.0, w] {
        if (p.0 - edge).abs() <= draft::SNAP_NEAR_MM {
            x = edge;
        }
    }
    for edge in [0.0, h] {
        if (p.1 - edge).abs() <= draft::SNAP_NEAR_MM {
            y = edge;
        }
    }
    ((x, y), false)
}

/// The top item at `p` (mm), within `tol` mm.
pub fn pick(l: &TitleBlockLayout, p: (f64, f64), tol: f64) -> Option<usize> {
    l.items.iter().enumerate().rev().find_map(|(i, item)| match item {
        BlockItem::Line { a, b, .. } => (segment_distance(p, *a, *b) <= tol).then_some(i),
        _ => item.bounds().filter(|r| p.0 >= r.x - tol && p.0 <= r.right() + tol && p.1 >= r.y - tol && p.1 <= r.top() + tol).map(|_| i),
    })
}

fn segment_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    (p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy)
}

/// The eight handles of a box on screen, clockwise from the top-left.
fn handles(r: Rect) -> [Pos2; 8] {
    [r.left_top(), r.center_top(), r.right_top(), r.right_center(), r.right_bottom(), r.center_bottom(), r.left_bottom(), r.left_center()]
}

/// `start` resized by handle `h` to the point `p` (mm), at least 0.5 mm a side.
fn resized(start: BRect, h: u8, p: (f64, f64)) -> BRect {
    let (mut l, mut b, mut r, mut t) = (start.x, start.y, start.right(), start.top());
    match h {
        0 => (l, t) = (p.0, p.1),
        1 => t = p.1,
        2 => (r, t) = (p.0, p.1),
        3 => r = p.0,
        4 => (r, b) = (p.0, p.1),
        5 => b = p.1,
        6 => (l, b) = (p.0, p.1),
        _ => l = p.0,
    }
    let (x0, x1) = (l.min(r), l.max(r));
    let (y0, y1) = (b.min(t), b.max(t));
    BRect::new(x0, y0, (x1 - x0).max(0.5), (y1 - y0).max(0.5))
}

fn set_rect(item: &mut BlockItem, r: BRect) {
    match item {
        BlockItem::Text { at, .. } | BlockItem::Field { at, .. } | BlockItem::RevTable { at, .. } | BlockItem::Logo { at, .. } => *at = r,
        BlockItem::Line { .. } => {}
    }
}

/// The text shown for a field: the sample value, or its binding's name in grey.
pub fn field_text(
    sample: Option<&Sample>, on: bool, b: &ecad_model::objects::title_block::Binding, if_empty: &ecad_model::objects::title_block::IfEmpty,
) -> (String, bool) {
    match sample.filter(|_| on) {
        Some(s) => (bind::shown(s.value(b), if_empty), false),
        None => (bind::name(b), true),
    }
}

/// Paint the text `value` in box `r` (screen) with `draw`; returns whether it was cut.
fn paint_text(p: &egui::Painter, v: View, r: &BRect, top_gap: f64, value: &str, draw: &TextDraw, ink: Color32) -> bool {
    if value.is_empty() {
        return false;
    }
    let room = r.w - 2.0;
    let (shown, wf) = ecad_engine::pages::frame::fit(value, draw.size, Some(room));
    let cut = shown.ends_with('…') && !value.ends_with('…');
    let px = (draw.size as f32 * v.k / 0.72).max(4.0);
    let y = r.y + (r.h - top_gap).max(0.0) / 2.0;
    let (x, align) = match draw.align {
        Align::Left => (r.x + 1.0, Align2::LEFT_CENTER),
        Align::Centre => (r.x + r.w / 2.0, Align2::CENTER_CENTER),
        Align::Right => (r.right() - 1.0, Align2::RIGHT_CENTER),
    };
    // Bold in the UI's emphasis face, at the drawing's size (fonts come through ecad-widgets).
    let font = if draw.bold {
        let mut f = ecad_widgets::fonts::font(p.ctx(), ui_type::BODY_EMPHASIS);
        f.size = px * wf as f32;
        f
    } else {
        ecad_widgets::style::ui_font(px * wf as f32)
    };
    p.text(v.at(x, y), align, shown, font, ink);
    cut
}

fn dashed_rect(p: &egui::Painter, r: Rect, stroke: Stroke) {
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    p.extend(Shape::dashed_line(&pts, stroke, 5.0, 3.0));
}

/// Paint the block's items (and their editor marks) in `v`.
#[allow(clippy::too_many_arguments)]
fn paint_items(ui: &egui::Ui, v: View, d: &Draft, sample: Option<&Sample>, sample_on: bool, sheet: &ecad_drawing_style::Sheet, marks: bool) {
    let p = ui.painter();
    let s = ecad_widgets::SHELL;
    let ink = c32(sheet.ink);
    let ink2 = c32(sheet.ink_2);
    let red = c32(sheet.error);
    let accent = c32(s.accent);
    let thin = Stroke::new(1.0, ink);
    let thick = Stroke::new(2.0, ink);
    let outside = d.outside();
    for (i, item) in d.now.layout.items.iter().enumerate() {
        let colour = |draw: &TextDraw| if draw.colour == Ink::Red { red } else { ink };
        match item {
            BlockItem::Line { a, b, weight } => {
                p.line_segment([v.at(a.0, a.1), v.at(b.0, b.1)], if *weight == LineWeight::Thick { thick } else { thin });
            }
            BlockItem::Text { at, text, draw } => {
                if paint_text(p, v, at, 0.0, text, draw, colour(draw)) && marks {
                    cut_mark(p, v.rect(at), sheet);
                }
            }
            BlockItem::Field { at, bind: b, label, draw, if_empty } => {
                let r = v.rect(at);
                let mut gap = 0.0;
                if let Some(l) = label.as_deref().filter(|l| !l.is_empty()) {
                    let px = (1.8 * v.k / 0.72).max(4.0);
                    p.text(v.at(at.x + 1.0, at.top() - 1.0), Align2::LEFT_TOP, l, ecad_widgets::style::ui_font(px), ink2);
                    gap = 1.8 + 1.5;
                }
                let (value, name) = field_text(sample, sample_on, b, if_empty);
                let c = if name { ink2 } else { colour(draw) };
                if paint_text(p, v, at, gap, &value, draw, c) && marks && !name {
                    cut_mark(p, r, sheet);
                }
                if marks {
                    dashed_rect(p, r.shrink(1.0), Stroke::new(1.0, accent));
                }
            }
            BlockItem::RevTable { at, rows, header, newest } => {
                paint_table(p, v, at, *rows, *header, *newest, sample.filter(|_| sample_on), ink, ink2)
            }
            BlockItem::Logo { at, prims, .. } => {
                let r = v.rect(at);
                if prims.is_empty() {
                    if marks {
                        dashed_rect(p, r.shrink(1.0), Stroke::new(1.0, accent));
                        p.text(r.center(), Align2::CENTER_CENTER, "Logo", ecad_widgets::style::ui_font(13.0), accent);
                    }
                } else {
                    paint_logo(p, v, at, prims, ink);
                }
            }
        }
        if marks
            && outside.contains(&i)
            && let Some(b) = item.bounds()
        {
            p.rect_stroke(v.rect(&b).expand(2.0), 2.0, Stroke::new(1.5, c32(sheet.warning)), egui::StrokeKind::Outside);
        }
    }
}

fn cut_mark(p: &egui::Painter, r: Rect, sheet: &ecad_drawing_style::Sheet) {
    let c = r.right_top();
    p.add(Shape::convex_polygon(vec![c, c + vec2(-8.0, 0.0), c + vec2(0.0, 8.0)], c32(sheet.warning), Stroke::NONE));
}

#[allow(clippy::too_many_arguments)]
fn paint_table(p: &egui::Painter, v: View, at: &BRect, rows: u8, header: bool, newest: Newest, sample: Option<&Sample>, ink: Color32, ink2: Color32) {
    let thin = Stroke::new(1.0, ink);
    let r = v.rect(at);
    p.rect_stroke(r, 0.0, thin, egui::StrokeKind::Middle);
    let n = usize::from(rows) + usize::from(header);
    if n == 0 {
        return;
    }
    let rh = at.h / n as f64;
    for i in 1..n {
        let y = at.top() - rh * i as f64;
        p.line_segment([v.at(at.x, y), v.at(at.right(), y)], thin);
    }
    let cols = sample.map(|s| s.input.rev_columns.clone()).unwrap_or_default();
    let (widths, _) = ecad_electrical::releases::revisions::widths(&cols, at.w);
    let mut x = at.x;
    let px = (1.8 * v.k / 0.72).max(4.0);
    let vpx = ((rh * 0.55).min(2.5) as f32 * v.k / 0.72).max(4.0);
    for (i, (c, w)) in cols.iter().zip(&widths).enumerate() {
        if i > 0 {
            p.line_segment([v.at(x, at.y), v.at(x, at.top())], thin);
        }
        if header {
            p.text(v.at(x + 1.0, at.top() - 1.0), Align2::LEFT_TOP, &c.header, ecad_widgets::style::ui_font(px), ink2);
        }
        if let Some(s) = sample {
            for (k, rev) in s.input.revisions.iter().take(usize::from(rows)).enumerate() {
                let row = match newest {
                    Newest::Top => k,
                    Newest::Bottom => usize::from(rows) - 1 - k,
                };
                let y = at.top() - rh * (row + usize::from(header)) as f64 - rh / 2.0;
                if let Some(val) = rev.get(i) {
                    let (shown, _) = ecad_engine::pages::frame::fit(val, 2.5, Some(w - 2.0));
                    p.text(v.at(x + 1.0, y), Align2::LEFT_CENTER, shown, ecad_widgets::style::ui_font(vpx), ink);
                }
            }
        }
        x += w;
    }
}

fn paint_logo(p: &egui::Painter, v: View, at: &BRect, prims: &[Primitive], ink: Color32) {
    let Some((lo, hi)) = ecad_model::geometry::geom::extents(prims) else { return };
    let (w, h) = (hi.x - lo.x, hi.y - lo.y);
    let s = match (w > 1e-9, h > 1e-9) {
        (true, true) => (at.w / w).min(at.h / h),
        (true, false) => at.w / w,
        (false, true) => at.h / h,
        _ => 1.0,
    };
    let ox = at.x + (at.w - w * s) / 2.0;
    let oy = at.y + (at.h - h * s) / 2.0;
    let q = |pt: &Pt| v.at(ox + (pt.x - lo.x) * s, oy + (pt.y - lo.y) * s);
    let st = Stroke::new(1.0, ink);
    for prim in prims.iter().take(ecad_model::objects::title_block::MAX_LOGO_PRIMS) {
        match prim {
            Primitive::Line { a, b } => {
                p.line_segment([q(a), q(b)], st);
            }
            Primitive::Polyline { vertices, closed } => {
                let pts: Vec<Pos2> = vertices.iter().map(q).collect();
                p.add(if *closed { Shape::closed_line(pts, st) } else { Shape::line(pts, st) });
            }
            Primitive::Circle { center, radius } => {
                p.circle_stroke(q(center), (*radius * s) as f32 * v.k, st);
            }
            Primitive::Arc { center, radius, start, end } => {
                let mut sweep = end - start;
                if sweep <= 0.0 {
                    sweep += std::f64::consts::TAU;
                }
                let steps = 24;
                let pts: Vec<Pos2> = (0..=steps)
                    .map(|i| {
                        let a = start + sweep * f64::from(i) / f64::from(steps);
                        q(&Pt::new(center.x + radius * a.cos(), center.y + radius * a.sin()))
                    })
                    .collect();
                p.add(Shape::line(pts, st));
            }
            Primitive::Text { at: t, height, value, .. } => {
                p.text(q(t), Align2::LEFT_BOTTOM, value, ecad_widgets::style::ui_font(((height * s) as f32 * v.k / 0.72).max(3.0)), ink);
            }
            Primitive::Point { .. } => {}
        }
    }
}

/// The name tag above a selected item: `Drawing number · project`.
fn tag_of(item: &BlockItem) -> String {
    match item {
        BlockItem::Field { bind: b, .. } => format!("{} · {}", bind::name(b), bind::chip(b)),
        BlockItem::RevTable { .. } => "Revision table · revision".to_string(),
        BlockItem::Logo { .. } => "Logo · image".to_string(),
        BlockItem::Text { .. } => "Text".to_string(),
        BlockItem::Line { .. } => "Line".to_string(),
    }
}

/// Draw the canvas and handle the tools; returns what the window must do.
pub fn show(ui: &mut egui::Ui, main: Rect, ed: &mut Editor, sheet: &ecad_drawing_style::Sheet, writable: bool) -> Vec<Act> {
    let s = ecad_widgets::SHELL;
    let mut acts = Vec::new();
    let body = Rect::from_min_max(pos2(main.left() + crate::title_blocks::list::LIST_W, main.top() + browser::TABS_H), main.max);
    let footer = Rect::from_min_max(pos2(body.left(), body.bottom() - FOOTER_H), body.max);
    let area = Rect::from_min_max(body.min, pos2(body.right(), footer.top()));
    ui.painter().rect_filled(footer, 0.0, c32(s.panel));
    ui.painter().hline(footer.x_range(), footer.top() + 0.5, Stroke::new(1.0, c32(s.line)));
    bar::text_at(ui, footer.left() + space::M, footer.center().y, crate::title_blocks::FOOTER, ui_type::SMALL, c32(s.text_3));
    let used = ed.open_row().map_or(0, |r| r.pages);
    let used = if used == 1 { "Used by 1 page".to_string() } else { format!("Used by {used} pages") };
    let f = bar::font(ui, ui_type::SMALL);
    let resp = ui.interact(
        Rect::from_min_size(footer.right_top() - vec2(160.0, 0.0), vec2(160.0, FOOTER_H)),
        egui::Id::unique(crate::title_blocks::USED_BY),
        Sense::hover(),
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, crate::title_blocks::USED_BY));
    ui.painter().text(pos2(footer.right() - space::M, footer.center().y), Align2::RIGHT_CENTER, used, f, c32(s.text_3));

    let sample_data = ed.sample_data.clone();
    let Some(d) = ed.draft.as_mut() else {
        ui.painter().text(area.center(), Align2::CENTER_CENTER, "No title block open.", bar::font(ui, ui_type::BODY), c32(s.text_3));
        return acts;
    };
    let (w, h) = d.now.layout.size;
    let (w, h) = (if w.is_finite() { w.max(1.0) } else { 180.0 }, if h.is_finite() { h.max(1.0) } else { 40.0 });
    let span = (w + AROUND_MM + 10.0, h + AROUND_MM + 10.0);
    let fit = area.shrink(24.0);
    let k = (fit.width() / span.0 as f32).min((fit.height() - 30.0).max(10.0) / span.1 as f32).max(0.1);
    let paper_px = vec2(span.0 as f32 * k, span.1 as f32 * k);
    let paper = Rect::from_center_size(area.center() - vec2(0.0, 10.0), paper_px);
    let v = View { o: pos2(paper.left() + (AROUND_MM as f32) * k, paper.bottom() - 10.0 * k), k };
    ed.canvas.view = Some(v);
    let p = ui.painter_at(area);
    p.rect_filled(paper, 0.0, c32(sheet.sheet));
    let ink2 = Stroke::new(1.5, c32(sheet.ink_2));
    // The frame around the block: its bottom and right edges, and 20 mm beyond.
    p.line_segment([v.at(-AROUND_MM, 0.0), v.at(w, 0.0)], ink2);
    p.line_segment([v.at(w, 0.0), v.at(w, h + AROUND_MM)], ink2);
    for x in [w * 0.15, w * 0.55] {
        p.line_segment([v.at(x, -4.0), v.at(x + 6.0, -4.0)], Stroke::new(1.0, c32(sheet.ink_2)));
    }
    p.text(
        pos2(paper.center().x, paper.bottom() + 14.0),
        Align2::CENTER_CENTER,
        crate::title_blocks::CAPTION,
        bar::font(ui, ui_type::SMALL),
        c32(s.text_3),
    );
    paint_items(ui, v, d, sample_data.as_ref(), ed.sample, sheet, true);
    if d.now.layout.items.is_empty() {
        p.text(v.at(w / 2.0, h / 2.0), Align2::CENTER_CENTER, crate::title_blocks::EMPTY_HINT, bar::font(ui, ui_type::BODY), c32(s.text_3));
    }
    // Selection: solid outline and handles; one item's name tag above it.
    let accent = c32(s.accent);
    for i in d.selected.iter().copied() {
        let Some(item) = d.now.layout.items.get(i) else { continue };
        match item {
            BlockItem::Line { a, b, .. } => {
                for e in [a, b] {
                    p.rect_filled(Rect::from_center_size(v.at(e.0, e.1), vec2(2.0 * HANDLE, 2.0 * HANDLE)), 1.0, accent);
                }
            }
            _ => {
                if let Some(b) = item.bounds() {
                    let r = v.rect(&b);
                    p.rect_stroke(r, 0.0, Stroke::new(2.0, accent), egui::StrokeKind::Middle);
                    if d.selected.len() == 1 {
                        for hpt in handles(r) {
                            let hr = Rect::from_center_size(hpt, vec2(2.0 * HANDLE, 2.0 * HANDLE));
                            p.rect(hr, 1.0, Color32::WHITE, Stroke::new(1.5, accent), egui::StrokeKind::Middle);
                        }
                    }
                }
            }
        }
        if d.selected.len() == 1
            && let Some(b) = item.bounds()
        {
            let text = tag_of(item);
            let font = bar::font(ui, ui_type::BODY_EMPHASIS);
            let tw = bar::text_width(ui, &text, ui_type::BODY_EMPHASIS) + 2.0 * space::M;
            let r = v.rect(&b);
            let tag = Rect::from_min_size(pos2(r.left(), r.top() - 32.0), vec2(tw, 26.0));
            p.rect_filled(tag, 4.0, accent);
            p.text(tag.center(), Align2::CENTER_CENTER, text, font, Color32::WHITE);
        }
    }
    if let Some(sp) = ed.canvas.snapped {
        p.circle_filled(v.at(sp.0, sp.1), 3.5, c32(sheet.connection_point));
    }

    // Banners.
    if ed.meanwhile {
        let name = d.now.name.clone();
        let banner = Rect::from_min_size(area.min + vec2(space::M, space::M), vec2(area.width() - 2.0 * space::M, 36.0));
        p.rect_filled(banner, 4.0, c32(s.raised));
        ui.scope_builder(egui::UiBuilder::new().max_rect(banner.shrink2(vec2(space::M, 4.0))), |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(egui::RichText::new(format!("{name} was changed meanwhile.")).color(c32(s.warning)));
                let r = ui.small_button("Reload");
                r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, crate::title_blocks::RELOAD));
                if r.clicked() {
                    acts.push(Act::Discard);
                }
                let k = ui.small_button("Keep mine");
                k.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, crate::title_blocks::KEEP));
                if k.clicked() {
                    ed.meanwhile = false;
                }
            });
        });
    }

    // The Logo tool's prompt.
    let mut logo_go = None;
    if let Some(ask) = ed.dxf.as_mut().filter(|a| a.logo_at.is_some()) {
        let at = ask.logo_at.map(|r| v.rect(&r)).unwrap_or(area);
        let pop = Rect::from_min_size(pos2(at.left(), at.bottom() + 8.0), vec2(360.0, 92.0));
        ui.painter().rect(pop, 6.0, c32(s.raised), Stroke::new(1.0, c32(s.line)), egui::StrokeKind::Inside);
        ui.scope_builder(egui::UiBuilder::new().max_rect(pop.shrink(space::S)), |ui| {
            ui.label(egui::RichText::new("A DXF drawing for the logo").small().color(c32(s.text_2)));
            ecad_widgets::control::TextEntry::new(&mut ask.path, LOGO_PATH).placeholder("C:\\logos\\company.dxf").show(ui);
            if let Some(e) = &ask.error {
                ui.label(egui::RichText::new(e).small().color(c32(s.error)));
            }
            ui.horizontal(|ui| {
                let b = ui.small_button("Insert");
                b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, LOGO_GO));
                if b.clicked() {
                    logo_go = Some((ask.logo_at, ask.path.trim().to_string()));
                }
                if ui.small_button("Cancel").clicked() {
                    ask.path.clear();
                    ask.logo_at = None;
                }
            });
        });
    }
    if let Some((at, path)) = logo_go {
        acts.push(Act::Logo { at, path });
    }
    if ed.dxf.as_ref().is_some_and(|a| a.logo_at.is_none() && a.path.is_empty() && a.sheet.is_empty()) {
        ed.dxf = None;
    }

    // The Field tool's menu.
    if let Some((r, at)) = ed.canvas.field_menu {
        let mut pick: Option<Option<&str>> = None;
        egui::Area::new(egui::Id::unique("library.tb.field.menu")).fixed_pos(at).show(ui.ctx(), |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                for row in PALETTE.iter() {
                    if let Drops::Fields(f) = row.drops {
                        for (b, label) in f {
                            if ui.button(format!("{} ({label})", row.label)).clicked() {
                                pick = Some(Some(*b));
                            }
                        }
                    }
                }
                if ui.button("Cancel").clicked() {
                    pick = Some(None);
                }
            });
        });
        if let Some(choice) = pick {
            ed.canvas.field_menu = None;
            if let (Some(b), Some(d)) = (choice, ed.draft.as_mut())
                && let Ok(bind) = ecad_model::objects::title_block::Binding::parse(b)
            {
                let label = PALETTE
                    .iter()
                    .find_map(|row| match row.drops {
                        Drops::Fields(f) => f.iter().find(|x| x.0 == b).map(|x| x.1.to_string()),
                        _ => None,
                    })
                    .unwrap_or_else(|| bind::name(&bind));
                d.add(BlockItem::Field { at: r, bind, label: Some(label), draw: TextDraw::default(), if_empty: Default::default() });
                acts.push(Act::Resample);
            }
        }
    }

    // Input.
    let Some(d) = ed.draft.as_mut() else { return acts };
    let read_only = d.builtin;
    let resp = ui.interact(area, egui::Id::unique(crate::title_blocks::CANVAS).with("input"), Sense::click_and_drag());
    // The canvas is named for the UI tests.
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Canvas, true, crate::title_blocks::CANVAS));
    let pointer = ui.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos()));
    let mm = pointer.map(|p| v.mm(p));
    let shift = ui.input(|i| i.modifiers.shift);
    let block = BRect::new(0.0, 0.0, w, h);
    let inside = |q: (f64, f64)| q.0 >= 0.0 && q.1 >= 0.0 && q.0 <= w && q.1 <= h;
    let _ = writable;

    // A palette row dragged from the left.
    if let Some(i) = ed.canvas.palette {
        if let (Some(pt), Some(row)) = (pointer, PALETTE.get(i)) {
            let (sw, sh) = row.size();
            let ghost = Rect::from_min_size(pt, vec2(sw as f32 * k, sh as f32 * k));
            let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("tb.ghost")));
            dashed_rect(&p, ghost, Stroke::new(1.5, accent));
            p.text(ghost.left_top() + vec2(4.0, 4.0), Align2::LEFT_TOP, row.label, ecad_widgets::style::ui_font(12.0), accent);
        }
        if ui.input(|i| i.pointer.any_released()) {
            ed.canvas.palette = None;
            if let (Some(q), Some(row)) = (mm, PALETTE.get(i)) {
                if inside(q) && area.contains(pointer.unwrap_or_default()) {
                    let (top_left, _) = snap_point(q, &d.now.layout, ed.snap);
                    drop_row(ed, row, top_left, &mut acts);
                } else {
                    ed.notice = Some(crate::title_blocks::DROP_ON_BLOCK.into());
                }
            }
        }
        return acts;
    }
    if let Some(i) = ed.canvas.drop_free.take()
        && let Some(row) = PALETTE.get(i)
        && let Some(at) = ed.draft.as_ref().map(|d| free_place(&d.now.layout, row.size()))
    {
        drop_row(ed, row, at, &mut acts);
        return acts;
    }
    let Some(d) = ed.draft.as_mut() else { return acts };

    // Keys, while the pointer is over the canvas and nothing takes typing.
    let typing = ui.ctx().egui_wants_keyboard_input();
    if resp.hovered() && !typing {
        ui.input(|i| {
            for t in Tool::ALL {
                if i.key_pressed(t.key()) && !i.modifiers.command && (!read_only || t == Tool::Select) {
                    ed.tool = t;
                }
            }
        });
        let (undo, redo, del, dup, esc, arrows) = ui.input(|i| {
            let c = i.modifiers.command;
            let step = if i.modifiers.shift { 5.0 } else { 1.0 };
            let mut a = (0.0, 0.0);
            if i.key_pressed(egui::Key::ArrowLeft) {
                a.0 -= step;
            }
            if i.key_pressed(egui::Key::ArrowRight) {
                a.0 += step;
            }
            if i.key_pressed(egui::Key::ArrowUp) {
                a.1 += step;
            }
            if i.key_pressed(egui::Key::ArrowDown) {
                a.1 -= step;
            }
            (
                c && i.key_pressed(egui::Key::Z) && !i.modifiers.shift,
                c && (i.key_pressed(egui::Key::Y) || (i.modifiers.shift && i.key_pressed(egui::Key::Z))),
                i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
                c && i.key_pressed(egui::Key::D),
                i.key_pressed(egui::Key::Escape),
                a,
            )
        });
        if !read_only {
            if undo && d.undo() {
                acts.push(Act::Resample);
            }
            if redo && d.redo() {
                acts.push(Act::Resample);
            }
            if del {
                d.delete_selected();
            }
            if dup {
                d.duplicate_selected();
            }
            if arrows != (0.0, 0.0) && !d.selected.is_empty() {
                d.step();
                d.nudge(arrows.0, arrows.1);
            }
        }
        if esc {
            d.selected.clear();
            ed.canvas.drag = None;
            if ed.tool != Tool::Select {
                ed.tool = Tool::Select;
            }
        }
    }

    let Some(q) = mm else { return acts };
    let (sq, item_snap) = snap_point(q, &d.now.layout, ed.snap);
    ed.canvas.snapped = (ed.snap && item_snap && resp.hovered() && ed.tool != Tool::Select).then_some(sq);
    let tool = if read_only { Tool::Select } else { ed.tool };
    let tol = f64::from(4.0 / k);

    if resp.drag_started() {
        ed.canvas.drag = match tool {
            Tool::Select => {
                let single = (d.selected.len() == 1).then(|| d.selected.iter().next().copied()).flatten();
                let on_handle = single.and_then(|i| {
                    let item = d.now.layout.items.get(i)?;
                    let start = pointer?;
                    match item {
                        BlockItem::Line { a, b, .. } => [a, b]
                            .iter()
                            .position(|e| v.at(e.0, e.1).distance(start) <= HANDLE + 2.0)
                            .map(|e| Drag::LineEnd { item: i, end: e as u8 }),
                        _ => {
                            let b = item.bounds()?;
                            handles(v.rect(&b)).iter().position(|hp| hp.distance(start) <= HANDLE + 2.0).map(|h| Drag::Resize {
                                item: i,
                                h: h as u8,
                                start: b,
                            })
                        }
                    }
                });
                match on_handle.filter(|_| !read_only) {
                    Some(dr) => {
                        d.step();
                        Some(dr)
                    }
                    None => match pick(&d.now.layout, q, tol) {
                        Some(i) if !read_only => {
                            if !d.selected.contains(&i) {
                                if !shift {
                                    d.selected.clear();
                                }
                                d.selected.insert(i);
                            }
                            d.step();
                            Some(Drag::Move { last: sq })
                        }
                        _ => pointer.map(|from| Drag::Window { from }),
                    },
                }
            }
            Tool::Line => Some(Drag::Line { from: sq }),
            Tool::Field | Tool::Logo => Some(Drag::Box { from: sq }),
            Tool::Text => None,
        };
    }
    if let Some(drag) = ed.canvas.drag.clone() {
        let released = ui.input(|i| i.pointer.any_released());
        match drag {
            Drag::Move { last } => {
                let (dx, dy) = (sq.0 - last.0, sq.1 - last.1);
                if dx != 0.0 || dy != 0.0 {
                    d.nudge(dx, dy);
                    ed.canvas.drag = Some(Drag::Move { last: sq });
                }
            }
            Drag::Resize { item, h: hd, start } => {
                if let Some(it) = d.now.layout.items.get_mut(item) {
                    set_rect(it, resized(start, hd, sq));
                }
            }
            Drag::LineEnd { item, end } => {
                if let Some(BlockItem::Line { a, b, .. }) = d.now.layout.items.get_mut(item) {
                    if end == 0 {
                        *a = sq;
                    } else {
                        *b = sq;
                    }
                }
            }
            Drag::Window { from } => {
                if let Some(to) = pointer {
                    let r = Rect::from_two_pos(from, to);
                    ui.painter().rect(r, 0.0, c32(s.accent_soft), Stroke::new(1.0, accent), egui::StrokeKind::Inside);
                    if released {
                        if !shift {
                            d.selected.clear();
                        }
                        for (i, it) in d.now.layout.items.iter().enumerate() {
                            if it.bounds().is_some_and(|b| r.contains_rect(v.rect(&b))) {
                                d.selected.insert(i);
                            }
                        }
                    }
                }
            }
            Drag::Line { from } => {
                let to = if shift { sq } else { ortho(from, sq) };
                if shift {
                    let r = Rect::from_two_pos(v.at(from.0, from.1), v.at(to.0, to.1));
                    ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0, accent), egui::StrokeKind::Middle);
                } else {
                    ui.painter().line_segment([v.at(from.0, from.1), v.at(to.0, to.1)], Stroke::new(1.5, accent));
                }
                if released && (to.0 - from.0).hypot(to.1 - from.1) >= 0.5 {
                    if shift {
                        d.step();
                        let (x0, y0, x1, y1) = (from.0.min(to.0), from.1.min(to.1), from.0.max(to.0), from.1.max(to.1));
                        for (a, b) in [((x0, y0), (x1, y0)), ((x1, y0), (x1, y1)), ((x1, y1), (x0, y1)), ((x0, y1), (x0, y0))] {
                            d.now.layout.items.push(BlockItem::Line { a, b, weight: LineWeight::Thin });
                        }
                        let n = d.now.layout.items.len();
                        d.selected = (n - 4..n).collect();
                    } else {
                        d.add(BlockItem::Line { a: from, b: to, weight: LineWeight::Thin });
                    }
                }
            }
            Drag::Box { from } => {
                let r = BRect::new(from.0.min(sq.0), from.1.min(sq.1), (sq.0 - from.0).abs().max(0.5), (sq.1 - from.1).abs().max(0.5));
                dashed_rect(ui.painter(), v.rect(&r), Stroke::new(1.5, accent));
                if released && r.w >= 2.0 && r.h >= 2.0 {
                    if !inside((r.x, r.y)) || !inside((r.right(), r.top())) {
                        ed.notice = Some(format!("Keep it inside the block ({} × {} mm).", fmt(block.w), fmt(block.h)));
                    } else if tool == Tool::Field {
                        ed.canvas.field_menu = pointer.map(|p| (r, p));
                    } else {
                        ed.dxf = Some(DxfAsk { logo_at: Some(r), ..DxfAsk::default() });
                    }
                }
            }
        }
        if released {
            ed.canvas.drag = None;
            // A drag that changed nothing leaves no undo step.
            if d.undo.last() == Some(&d.now) {
                d.undo.pop();
            }
        }
        return acts;
    }
    if resp.clicked() {
        match tool {
            Tool::Select => match pick(&d.now.layout, q, tol) {
                Some(i) if shift => {
                    if !d.selected.remove(&i) {
                        d.selected.insert(i);
                    }
                }
                Some(i) => d.selected = [i].into(),
                None if !shift => d.selected.clear(),
                None => {}
            },
            Tool::Text if inside(sq) => {
                let i =
                    d.add(BlockItem::Text { at: BRect::new(sq.0, (sq.1 - 6.0).max(0.0), 30.0, 6.0), text: "Text".into(), draw: TextDraw::default() });
                ed.canvas.edit_text = Some(i);
                ed.tool = Tool::Select;
            }
            _ => {}
        }
    }
    acts
}

/// Lines snap to horizontal or vertical (the block's lines are).
fn ortho(from: (f64, f64), to: (f64, f64)) -> (f64, f64) {
    if (to.0 - from.0).abs() >= (to.1 - from.1).abs() { (to.0, from.1) } else { (from.0, to.1) }
}

fn fmt(v: f64) -> String {
    if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v:.1}") }
}

/// Drop palette row `row` with its top-left at `at` (mm).
fn drop_row(ed_state: &mut Editor, row: &crate::title_blocks::list::PaletteRow, at: (f64, f64), acts: &mut Vec<Act>) {
    match row.drops {
        Drops::ProjectField => ed_state.project_key = Some(String::new()),
        Drops::Logo => ed_state.tool = Tool::Logo,
        _ => {
            let Some(d) = ed_state.draft.as_mut() else { return };
            let items = row.items(at);
            if items.is_empty() {
                return;
            }
            d.step();
            let start = d.now.layout.items.len();
            d.now.layout.items.extend(items);
            d.selected = (start..d.now.layout.items.len()).collect();
            acts.push(Act::Resample);
        }
    }
}
