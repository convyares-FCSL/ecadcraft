//! A frame and its title block as parts (design 055 §3, §1.2): the border with zones and centre
//! marks, then the block's items in drawing order.
//!
//! Coordinates here are paper millimetres from the paper's bottom-left corner; [`Pen`] scales them
//! to drawing units. Drawing never fails: items breaking a bound are skipped
//! ([`TitleBlockLayout::usable`]), values longer than their box are narrowed, then cut with `…`
//! (019's `fit`), and the cut fields are reported ([`Drawn::cut`]).

use cadcraft_engine::doc::{EntityKind, HAlign, Text, VAlign};
use cadcraft_engine::geom::Vec3;
use ecad_model::sheet::{Grid, Margins};
use ecad_model::title_block::{Align, BlockItem, Ink, LineWeight, Newest, Rect, TextLook as ItemLook, TitleBlockLayout};
use ecad_model::{LengthUnit, Primitive, Pt};

use crate::convert::kind_for_primitive;
use crate::frame::{LABEL_ACI, LAYER, Part, TextLook};
use crate::frames::FrameInput;

/// `ecad.title-red`: ACI 1, red (a field's Red colour; house style: the status).
pub const RED_ACI: u8 = 1;
/// How far a zone tick reaches into the margin, mm.
pub const TICK_MM: f64 = 3.0;
pub use crate::title_block::INSET_MM;
/// Gap under a caption before the value area, mm.
pub const CAPTION_GAP_MM: f64 = 1.5;

/// Draws in paper millimetres (origin at the paper's bottom-left corner).
pub struct Pen {
    /// Millimetres to drawing units.
    pub k: f64,
    /// Added to every x, mm (a generated page's later sheets).
    pub dx: f64,
    pub out: Vec<Part>,
}

impl Pen {
    pub fn new(k: f64) -> Pen {
        Pen { k, dx: 0.0, out: Vec::new() }
    }

    fn p(&self, x: f64, y: f64) -> Pt {
        Pt::new((x + self.dx) * self.k, y * self.k)
    }

    pub fn line(&mut self, a: (f64, f64), b: (f64, f64), weight: u16, aci: Option<u8>) {
        let kind = kind_for_primitive(&Primitive::Line { a: self.p(a.0, a.1), b: self.p(b.0, b.1) });
        self.out.push(Part { kind, weight: Some(weight), aci, layer: LAYER });
    }

    pub fn rect(&mut self, a: (f64, f64), b: (f64, f64), weight: u16, aci: Option<u8>) {
        let v = vec![self.p(a.0, a.1), self.p(b.0, a.1), self.p(b.0, b.1), self.p(a.0, b.1)];
        let kind = kind_for_primitive(&Primitive::Polyline { vertices: v, closed: true });
        self.out.push(Part { kind, weight: Some(weight), aci, layer: LAYER });
    }

    /// Text at `at`, aligned, fitted into `room` mm when given. Returns whether it was cut.
    pub fn text(&mut self, at: (f64, f64), look: TextLook, value: &str, align: (HAlign, VAlign), aci: Option<u8>, room: Option<f64>) -> bool {
        if value.is_empty() || !(look.mm.is_finite() && look.mm > 0.0) {
            return false;
        }
        let (shown, wf) = crate::frames::fit(value, look.mm, room);
        let cut = shown.ends_with('…') && !value.ends_with('…');
        let at = self.p(at.0, at.1);
        let pt = Vec3::new(at.x, at.y, 0.0);
        let left_base = matches!(align, (HAlign::Left, VAlign::Baseline));
        let kind = EntityKind::Text(Text {
            insert: pt,
            align_pt: if left_base { None } else { Some(pt) },
            height: look.mm * self.k,
            value: shown,
            rotation: 0.0,
            width_factor: wf,
            oblique: 0.0,
            style: look.style.into(),
            halign: align.0,
            valign: align.1,
        });
        self.out.push(Part { kind, weight: look.weight, aci, layer: LAYER });
        cut
    }

    /// A primitive already in drawing units.
    fn prim(&mut self, p: &Primitive, weight: u16) {
        let weight = match p {
            Primitive::Text { .. } => None,
            _ => Some(weight),
        };
        self.out.push(Part { kind: kind_for_primitive(p), weight, aci: None, layer: LAYER });
    }
}

/// What drawing a block found: the items skipped (bounds, unknown kinds) and the fields whose
/// values were cut, by item index.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Drawn {
    pub skipped: usize,
    pub cut: Vec<usize>,
}

/// The border of the frame `input` describes (design 055 §3): the paper edge, the frame, zones
/// labelled on all four sides, centre marks.
pub fn border(pen: &mut Pen, input: &FrameInput) {
    let s = input.sheet;
    let m = input.look.margins;
    let grid = Grid::with_layout(s, LengthUnit::Millimetre, m, ecad_model::sheet::Corner::TopLeft);
    let (w, h) = (f64::from(s.width), f64::from(s.height));
    let (l, r, b, t) = (m.left, w - m.right, m.bottom, h - m.top);
    let (border, inner) = (input.look.border, input.look.inner);
    let ml = (HAlign::Middle, VAlign::Middle);
    pen.rect((0.0, 0.0), (w, h), inner, Some(LABEL_ACI));
    pen.rect((l, b), (r, t), border, None);
    let (cw, rh) = (grid.column_width(), grid.row_height());
    // A page type with zones off: no ticks and no labels, whatever the frame says.
    let (columns, rows) = if input.look.zones { (s.columns, s.rows) } else { (0, 0) };
    for ci in 0..columns {
        let x0 = l + f64::from(ci) * cw;
        if ci > 0 {
            pen.line((x0, t), (x0, t + TICK_MM.min(m.top)), inner, None);
            pen.line((x0, b), (x0, b - TICK_MM.min(m.bottom)), inner, None);
        }
        if input.look.grid_labels {
            let label = grid.column_label(grid.column_number(ci));
            pen.text((x0 + cw / 2.0, t + m.top / 2.0), input.text.grid, &label, ml, None, None);
            pen.text((x0 + cw / 2.0, b - m.bottom / 2.0), input.text.grid, &label, ml, None, None);
        }
    }
    for ri in 0..rows {
        let y1 = t - f64::from(ri) * rh;
        if ri > 0 {
            pen.line((l, y1), (l - TICK_MM.min(m.left), y1), inner, None);
            pen.line((r, y1), (r + TICK_MM.min(m.right), y1), inner, None);
        }
        if input.look.grid_labels {
            let label = grid.row_label(grid.row_number(ri));
            pen.text((l - m.left / 2.0, y1 - rh / 2.0), input.text.grid, &label, ml, None, None);
            pen.text((r + m.right / 2.0, y1 - rh / 2.0), input.text.grid, &label, ml, None, None);
        }
    }
    if input.look.centre_marks {
        // From the frame to just short of the paper edge, at the middle of each side.
        let (cx, cy) = ((l + r) / 2.0, (b + t) / 2.0);
        let reach = |margin: f64| (margin - 1.0).max(0.0);
        pen.line((cx, t), (cx, t + reach(m.top)), border, None);
        pen.line((cx, b), (cx, b - reach(m.bottom)), border, None);
        pen.line((l, cy), (l - reach(m.left), cy), border, None);
        pen.line((r, cy), (r + reach(m.right), cy), border, None);
    }
}

/// Where a block's bottom-left corner sits on the paper (mm): the frame's bottom-left for a
/// built-in stretched across the frame, else against the frame's bottom-right corner.
pub fn block_origin(input: &FrameInput, layout: &TitleBlockLayout) -> (f64, f64) {
    let m: Margins = input.look.margins;
    let right = f64::from(input.sheet.width) - m.right;
    let w = if layout.size.0.is_finite() { layout.size.0 } else { 0.0 };
    (right - w, m.bottom)
}

/// The look a block item's text is drawn with: the frame's main style for bold, its value style
/// otherwise, at the item's size.
pub fn text_look(input: &FrameInput, look: &ItemLook) -> TextLook {
    let base = if look.bold { input.text.main } else { input.text.value };
    TextLook { mm: look.size, style: base.style, weight: if look.bold { base.weight.or(input.text.bold_weight) } else { None } }
}

fn aci_of(ink: Ink) -> Option<u8> {
    match ink {
        Ink::Ink => None,
        Ink::Red => Some(RED_ACI),
    }
}

/// Draw `layout`'s items with the block's bottom-left corner at `origin` (paper mm).
pub fn block(pen: &mut Pen, input: &FrameInput, layout: &TitleBlockLayout, origin: (f64, f64)) -> Drawn {
    let (usable, skipped) = layout.usable();
    let mut drawn = Drawn { skipped: skipped.items, cut: Vec::new() };
    let (ox, oy) = origin;
    let at = |x: f64, y: f64| (ox + x, oy + y);
    let tl = (HAlign::Left, VAlign::Top);
    for i in usable {
        let Some(item) = layout.items.get(i) else { continue };
        match item {
            BlockItem::Line { a, b, weight } => {
                let w = match weight {
                    LineWeight::Thin => input.look.inner,
                    LineWeight::Thick => input.look.border,
                };
                pen.line(at(a.0, a.1), at(b.0, b.1), w, None);
            }
            BlockItem::Text { at: r, text, look } => {
                let cut = value_text(pen, input, *r, origin, text, look, 0.0);
                if cut {
                    drawn.cut.push(i);
                }
            }
            BlockItem::Field { at: r, bind, label, look, if_empty } => {
                let mut top_gap = 0.0;
                if let Some(l) = label.as_deref().filter(|l| !l.is_empty()) {
                    pen.text(at(r.x + INSET_MM, r.top() - INSET_MM), input.text.caption, l, tl, Some(LABEL_ACI), Some(r.w - 2.0 * INSET_MM));
                    top_gap = input.text.caption.mm + CAPTION_GAP_MM;
                }
                let v = crate::title_block::bind::shown(crate::title_block::bind::value(input, bind), if_empty);
                if value_text(pen, input, *r, origin, &v, look, top_gap) {
                    drawn.cut.push(i);
                }
            }
            BlockItem::RevTable { at: r, rows, header, newest } => rev_table(pen, input, *r, origin, *rows, *header, *newest),
            BlockItem::Logo { at: r, prims, .. } => logo(pen, input, *r, origin, prims),
            BlockItem::Unknown => {}
        }
    }
    drawn
}

/// A value or static text in its box below `top_gap` mm, aligned; returns whether it was cut.
fn value_text(pen: &mut Pen, input: &FrameInput, r: Rect, origin: (f64, f64), value: &str, look: &ItemLook, top_gap: f64) -> bool {
    let tl = text_look(input, look);
    let room = r.w - 2.0 * INSET_MM;
    let y = origin.1 + r.y + (r.h - top_gap).max(0.0) / 2.0;
    let (x, h) = match look.align {
        Align::Left => (r.x + INSET_MM, HAlign::Left),
        Align::Centre => (r.x + r.w / 2.0, HAlign::Center),
        Align::Right => (r.right() - INSET_MM, HAlign::Right),
    };
    pen.text((origin.0 + x, y), tl, value, (h, VAlign::Middle), aci_of(look.colour), Some(room))
}

/// A revision table (design 055 §3): columns from the project's revision columns, a header row,
/// `rows` rows of the newest issued revisions.
fn rev_table(pen: &mut Pen, input: &FrameInput, r: Rect, origin: (f64, f64), rows: u8, header: bool, newest: Newest) {
    let (ox, oy) = (origin.0 + r.x, origin.1 + r.y);
    let thin = input.look.inner;
    let n = usize::from(rows) + usize::from(header);
    if n == 0 {
        return;
    }
    let rh = r.h / n as f64;
    let (widths, _) = crate::revisions::widths(&input.rev_columns, r.w);
    let mut xs = vec![0.0];
    for w in &widths {
        let last = xs.last().copied().unwrap_or(0.0);
        xs.push(last + w);
    }
    pen.rect((ox, oy), (ox + r.w, oy + r.h), thin, None);
    for x in xs.iter().skip(1).take(widths.len().saturating_sub(1)) {
        pen.line((ox + x, oy), (ox + x, oy + r.h), thin, None);
    }
    for i in 1..n {
        let y = oy + r.h - rh * i as f64;
        pen.line((ox, y), (ox + r.w, y), thin, None);
    }
    let tl = (HAlign::Left, VAlign::Top);
    if header {
        for (i, c) in input.rev_columns.iter().enumerate() {
            let x = xs.get(i).copied().unwrap_or_default();
            let w = widths.get(i).copied().unwrap_or_default();
            pen.text((ox + x + INSET_MM, oy + r.h - INSET_MM), input.text.caption, &c.header, tl, Some(LABEL_ACI), Some(w - 2.0 * INSET_MM));
        }
    }
    let mut look = input.text.value;
    look.mm = look.mm.min(rh * 0.7);
    let shown = input.revisions.len().min(usize::from(rows));
    for (k, rev) in input.revisions.iter().take(shown).enumerate() {
        // Row index from the top among the data rows.
        let row = match newest {
            Newest::Top => k,
            Newest::Bottom => usize::from(rows) - 1 - k,
        };
        let y = oy + r.h - rh * (row + usize::from(header)) as f64 - rh / 2.0;
        for (i, value) in rev.iter().enumerate() {
            let (x0, x1) = (xs.get(i).copied().unwrap_or_default(), xs.get(i + 1).copied().unwrap_or_default());
            pen.text((ox + x0 + INSET_MM, y), look, value, (HAlign::Left, VAlign::Middle), None, Some(x1 - x0 - 2.0 * INSET_MM));
        }
    }
}

/// The extents of `prims` (min, max), when they have any finite points.
pub fn extents(prims: &[Primitive]) -> Option<(Pt, Pt)> {
    let mut lo = Pt::new(f64::INFINITY, f64::INFINITY);
    let mut hi = Pt::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut add = |p: Pt| {
        if p.x.is_finite() && p.y.is_finite() {
            lo = Pt::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Pt::new(hi.x.max(p.x), hi.y.max(p.y));
        }
    };
    for p in prims {
        match p {
            Primitive::Line { a, b } => {
                add(*a);
                add(*b);
            }
            Primitive::Polyline { vertices, .. } => vertices.iter().copied().for_each(&mut add),
            Primitive::Arc { center, radius, .. } | Primitive::Circle { center, radius } => {
                add(Pt::new(center.x - radius, center.y - radius));
                add(Pt::new(center.x + radius, center.y + radius));
            }
            Primitive::Text { at, height, value, .. } => {
                add(*at);
                add(Pt::new(at.x + height * 0.8 * value.chars().count() as f64, at.y + height));
            }
            Primitive::Point { at } => add(*at),
        }
    }
    (lo.x <= hi.x && lo.y <= hi.y).then_some((lo, hi))
}

/// `prims` scaled by `s` about `from` and moved to `to`, in drawing units `k`.
fn place(p: &Primitive, from: Pt, s: f64, to: Pt, k: f64) -> Primitive {
    let t = |q: Pt| Pt::new((to.x + (q.x - from.x) * s) * k, (to.y + (q.y - from.y) * s) * k);
    match p {
        Primitive::Line { a, b } => Primitive::Line { a: t(*a), b: t(*b) },
        Primitive::Polyline { vertices, closed } => Primitive::Polyline { vertices: vertices.iter().map(|v| t(*v)).collect(), closed: *closed },
        Primitive::Arc { center, radius, start, end } => Primitive::Arc { center: t(*center), radius: radius * s * k, start: *start, end: *end },
        Primitive::Circle { center, radius } => Primitive::Circle { center: t(*center), radius: radius * s * k },
        Primitive::Text { at, height, rotation, value, halign, valign } => {
            Primitive::Text { at: t(*at), height: height * s * k, rotation: *rotation, value: value.clone(), halign: *halign, valign: *valign }
        }
        Primitive::Point { at } => Primitive::Point { at: t(*at) },
    }
}

/// A logo fitted into its box keeping its proportions, centred (design 055 §6).
fn logo(pen: &mut Pen, input: &FrameInput, r: Rect, origin: (f64, f64), prims: &[Primitive]) {
    let Some((lo, hi)) = extents(prims) else { return };
    let (w, h) = (hi.x - lo.x, hi.y - lo.y);
    let s = match (w > 1e-9, h > 1e-9) {
        (true, true) => (r.w / w).min(r.h / h),
        (true, false) => r.w / w,
        (false, true) => r.h / h,
        (false, false) => 1.0,
    };
    if !s.is_finite() {
        return;
    }
    let to = Pt::new(origin.0 + pen.dx + r.x + (r.w - w * s) / 2.0, origin.1 + r.y + (r.h - h * s) / 2.0);
    for p in prims {
        let q = place(p, lo, s, to, pen.k);
        pen.prim(&q, input.look.inner);
    }
}
