//! Graphical terminal-strip pages: the `graphic` style of a `connection_points`
//! generated page. Each strip is drawn as it is wired on the rail: one column per plan row of
//! [`ecad_electrical::terminal_strips::strip_model::StripData::plan`], inside conductors up to their targets, outside conductors
//! down, cable bars and boxes under them, jumper bars, bridges, levels and accessories in the band.
//!
//! It reads only the kept report models ([`ReportModels`]: the strips, the cable and device rows). It
//! stores nothing in the document and is never a second model: the table style and this one draw
//! the same rows. It does not draw panels, click-through or colour per text role.
//!
//! The kept layout data, the style and the sheets are [`ecad_electrical::terminal_strips::pages`]'s; this module
//! draws.

use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::FRAC_PI_2;

use cadcraft_engine::doc::{EntityKind, HAlign, Text, VAlign};
use cadcraft_engine::geom::Vec3;
use ecad_model::document::sheet::unit_scale;
use ecad_model::{CableId, DeviceId, Document, PageId, Primitive, Pt};

use crate::convert::kind_for_primitive;
use crate::pages::frame::{FrameInput, Part, THIN_WEIGHT, TextDraw};
use ecad_electrical::reports::kept::ReportModels;
use ecad_electrical::terminal_strips::accessories::Acc;
use ecad_electrical::terminal_strips::strip_model::{SideCell, StripData};
use {crate::styles::draw::DrawStyles, ecad_model::look::TextKind};
use {ecad_electrical::pages::generated::LAYER, ecad_electrical::pages::generated::MISSING};

/// Guard: at most this many cable bar lanes; further cables share the last.
pub(crate) const MAX_CABLE_LANES: usize = 8;
/// Guard: conductors drawn as leads on one side of a column; more are counted `+n`.
pub(crate) const MAX_LEADS_PER_SIDE: usize = 2;
/// Guard: far devices named under a cable box; more are counted `+n`.
pub(crate) const MAX_FAR_DEVICES: usize = 3;
/// Lane spacing (`ecad.strip.lane-gap`): jumper bars, cable bars.
const JUMPER_BAR_LANE_MM: f64 = 2.0;
const CABLE_LANE_MM: f64 = 2.5;
/// `ecad.jumper bar-stub`: a jumper bar's continuation past the sheet's first or last column.
const JUMPER_BAR_STUB_MM: f64 = 4.0;
/// Two leads on one side sit this far either side of the column centre.
const LEAD_OFFSET_MM: f64 = 1.5;
/// Cable box: width and height.
const BOX_W_MM: f64 = 8.0;
const BOX_H_MM: f64 = 24.0;
/// Room for an outside target's text below its lead.
const OUT_TARGET_MM: f64 = 26.0;
/// The most room an inside target's text gets above its lead.
const IN_TARGET_MM: f64 = 45.0;
/// `ecad.dot-size`: a connection dot's radius.
const DOT_MM: f64 = 0.5;
/// The area height (mm) at which the vertical layout is drawn at full size.
const FULL_AREA_MM: f64 = 260.0;
/// Below this area height the band and leads scale down.
const SCALE_BELOW_MM: f64 = 160.0;

/// Draws in paper millimetres, sheet origin at `x0`.
struct Pen {
    k: f64,
    x0: f64,
    out: Vec<Part>,
}

impl Pen {
    fn p(&self, x: f64, y: f64) -> Pt {
        Pt::new((x + self.x0) * self.k, y * self.k)
    }
    fn line_w(&mut self, a: (f64, f64), b: (f64, f64), weight: u16) {
        let kind = kind_for_primitive(&Primitive::Line { a: self.p(a.0, a.1), b: self.p(b.0, b.1) });
        self.out.push(Part { kind, weight: Some(weight), aci: None, layer: LAYER });
    }
    fn line(&mut self, a: (f64, f64), b: (f64, f64)) {
        self.line_w(a, b, THIN_WEIGHT);
    }
    /// A dashed line (`ecad.jumper_bar`): 1.5 mm dashes, 1 mm gaps.
    fn dashed(&mut self, a: (f64, f64), b: (f64, f64)) {
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        if !(len.is_finite() && len > 0.0) {
            return;
        }
        let (ux, uy) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
        let mut s = 0.0;
        // Bounded by the line's length (at most a sheet's width).
        while s < len && s < 2000.0 {
            let e = (s + 1.5).min(len);
            self.line((a.0 + ux * s, a.1 + uy * s), (a.0 + ux * e, a.1 + uy * e));
            s += 2.5;
        }
    }
    fn rect(&mut self, a: (f64, f64), b: (f64, f64)) {
        let v = vec![self.p(a.0, a.1), self.p(b.0, a.1), self.p(b.0, b.1), self.p(a.0, b.1)];
        let kind = kind_for_primitive(&Primitive::Polyline { vertices: v, closed: true });
        self.out.push(Part { kind, weight: Some(THIN_WEIGHT), aci: None, layer: LAYER });
    }
    fn dashed_rect(&mut self, a: (f64, f64), b: (f64, f64)) {
        self.dashed((a.0, a.1), (b.0, a.1));
        self.dashed((b.0, a.1), (b.0, b.1));
        self.dashed((b.0, b.1), (a.0, b.1));
        self.dashed((a.0, b.1), (a.0, a.1));
    }
    /// A solid bar (`ecad.strip.accessory`): its outline filled with lines 0.2 mm apart.
    fn bar(&mut self, a: (f64, f64), b: (f64, f64)) {
        let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
        self.rect(a, b);
        let mut x = x0 + 0.2;
        while x < x1 - 0.1 && x < x0 + 10.0 {
            self.line((x, a.1), (x, b.1));
            x += 0.2;
        }
    }
    /// A 45° hatch at 1 mm across the rectangle `a`–`b` (`ecad.strip.pe-hatch`).
    fn hatch(&mut self, a: (f64, f64), b: (f64, f64)) {
        let (x0, x1, y0, y1) = (a.0.min(b.0), a.0.max(b.0), a.1.min(b.1), a.1.max(b.1));
        let (w, h) = (x1 - x0, y1 - y0);
        // Lines y − y0 = (x − x0) − c for c from −h to w.
        let mut c = -h + 1.0;
        while c < w && c < 100.0 {
            let (sx, sy) = if c >= 0.0 { (x0 + c, y0) } else { (x0, y0 - c) };
            let run = (x1 - sx).min(y1 - sy);
            if run > 0.0 {
                self.line((sx, sy), (sx + run, sy + run));
            }
            c += 1.0;
        }
    }
    fn dot(&mut self, at: (f64, f64), radius: f64) {
        let center = self.p(at.0, at.1);
        for k in [1.0, 0.66, 0.33] {
            let kind = kind_for_primitive(&Primitive::Circle { center, radius: radius * k * self.k });
            self.out.push(Part { kind, weight: Some(THIN_WEIGHT), aci: None, layer: LAYER });
        }
    }
    /// Text at `at`, rotated 90° (reading upwards) when `up`, fitted to `room` along its line.
    fn text(&mut self, at: (f64, f64), draw: TextDraw, value: &str, h: HAlign, up: bool, room: Option<f64>) {
        if value.is_empty() {
            return;
        }
        let (shown, wf) = crate::pages::frame::fit(value, draw.mm, room);
        let at = self.p(at.0, at.1);
        let pt = Vec3::new(at.x, at.y, 0.0);
        let kind = EntityKind::Text(Text {
            insert: pt,
            align_pt: Some(pt),
            height: draw.mm * self.k,
            value: shown,
            rotation: if up { FRAC_PI_2 } else { 0.0 },
            width_factor: wf,
            oblique: 0.0,
            style: draw.style.into(),
            halign: h,
            valign: VAlign::Middle,
        });
        self.out.push(Part { kind, weight: draw.weight, aci: None, layer: LAYER });
    }
    /// A value, or the `?` box about `at` when it is missing.
    fn value(&mut self, doc: &Document, at: (f64, f64), draw: TextDraw, value: Option<&str>, h: HAlign, room: Option<f64>) {
        match value.map(str::trim).filter(|v| !v.is_empty()) {
            Some(v) => self.text(at, draw, v, h, true, room),
            None => {
                let s = (draw.mm + 1.4).max(2.4);
                let y = match h {
                    HAlign::Right => at.1 - s,
                    HAlign::Center => at.1 - s / 2.0,
                    _ => at.1,
                };
                let (lo, hi) = (self.p(at.0 - s / 2.0, y), self.p(at.0 + s / 2.0, y + s));
                self.out.extend(crate::pages::generated::missing_mark(doc, lo, hi, self.k, draw));
            }
        }
    }
}

/// The text looks a strip page draws with.
struct Draws {
    tag: TextDraw,
    table: TextDraw,
    /// A terminal's number or label (its own kind, `PointLabel`).
    point: TextDraw,
    /// The strip's location in the tag box (`LocationLabel`).
    location: TextDraw,
    reference: TextDraw,
    wire: TextDraw,
    cable: TextDraw,
    description: TextDraw,
}

/// The content of sheet `i` of graphic `connection_points` page `page` (paper millimetres, sheet origin at
/// `x0`): its strip's columns that fall on this sheet. O(rows of that strip + what it draws).
pub fn content(doc: &Document, page: PageId, input: &FrameInput, i: usize, x0: f64, o: &ReportModels) -> Vec<Part> {
    let (l, b, r, t) = ecad_electrical::pages::generated::area(input);
    let mut pen = Pen { k: unit_scale(input.unit), x0, out: Vec::new() };
    let draws = DrawStyles::new(doc);
    let draw = |kind| TextDraw::of(&draws, kind, input.unit);
    let g = o.strip_pages.geometry;
    let heading = draw(TextKind::TableHeading);
    let lk = Draws {
        tag: TextDraw { mm: heading.mm * g.tag_scale, ..heading },
        table: draw(TextKind::Table),
        point: draw(TextKind::PointLabel),
        location: draw(TextKind::LocationLabel),
        reference: draw(TextKind::Reference),
        wire: draw(TextKind::WireLabel),
        cable: draw(TextKind::CableText),
        description: draw(TextKind::Description),
    };
    // A short area scales the band and the leads (texts keep their heights).
    let f = if t - b < SCALE_BELOW_MM { ((t - b) / FULL_AREA_MM).clamp(0.5, 1.0) } else { 1.0 };
    let yc = t - g.band_y * f;
    let lead = g.lead * f;
    match sheet_at(doc, page, o, i) {
        Err(()) => {
            pen.rect((l, yc - g.body / 2.0), (l + g.tagbox, yc + g.body / 2.0));
            let (lo, hi) = (pen.p(l + 1.0, yc - 3.0), pen.p(l + g.tagbox - 1.0, yc + 3.0));
            pen.out.extend(crate::pages::generated::missing_mark(doc, lo, hi, pen.k, lk.table));
        }
        Ok(None) => pen.text((l, yc), lk.table, NO_STRIPS, HAlign::Left, false, Some(r - l)),
        Ok(Some((s, j, _))) => {
            let avail = page_avail(doc, page, o);
            let frame = Frame { l, b, t, yc, lead, x_first: l + g.tagbox + TAG_GAP_MM };
            draw_strip(doc, &mut pen, s, j, avail, &g, &frame, &lk, o);
        }
    }
    pen.out
}

/// Where a strip sheet is drawn.
struct Frame {
    l: f64,
    b: f64,
    t: f64,
    /// The band's centre line.
    yc: f64,
    lead: f64,
    /// The left edge of the first column.
    x_first: f64,
}

/// A drawn column on this sheet.
struct Placed<'a> {
    col: Col<'a>,
    x: f64,
    w: f64,
}

impl Placed<'_> {
    fn cx(&self) -> f64 {
        self.x + self.w / 2.0
    }
}

/// The x of lead `k` of `n` in a column centred at `cx`.
fn lead_x(cx: f64, k: usize, n: usize) -> f64 {
    if n < 2 {
        cx
    } else if k == 0 {
        cx - LEAD_OFFSET_MM
    } else {
        cx + LEAD_OFFSET_MM
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_strip(doc: &Document, pen: &mut Pen, s: &StripData, j: usize, avail: f64, g: &Geometry, fr: &Frame, lk: &Draws, o: &ReportModels) {
    let (cols, blocks) = columns(s, g);
    let widths: Vec<f64> = blocks.iter().map(|(_, _, w)| *w).collect();
    let sheets = paginate(&widths, avail);
    let (b0, b1) = sheets.get(j).copied().unwrap_or((0, 0));
    let c0 = blocks.get(b0).map_or(cols.len(), |b| b.0);
    let c1 = if b1 > b0 { blocks.get(b1 - 1).map_or(c0, |b| b.1) } else { c0 };
    let mut placed: Vec<Placed<'_>> = Vec::with_capacity(c1.saturating_sub(c0));
    let mut x = fr.x_first;
    for (col, w) in cols.get(c0..c1).unwrap_or_default() {
        placed.push(Placed { col: *col, x, w: *w });
        x += w;
    }
    let style = doc.settings().reference_style();
    let (half, yc) = (g.body / 2.0, fr.yc);
    // The strip tag box.
    tag_box(pen, s, g, fr, lk);
    // Bodies, labels, leads.
    let mut out_leads: Vec<(f64, &SideCell)> = Vec::new();
    for p in &placed {
        match p.col {
            Col::Gap(acc) => gap(pen, acc, p, g, yc, s, lk),
            Col::Term(_, r) => {
                let up = ecad_electrical::terminal_strips::raise(r);
                let (bot, top) = (yc - half + up, yc + half + up);
                let cx = p.cx();
                if r.spare() {
                    pen.dashed_rect((p.x, bot), (p.x + p.w, top));
                } else {
                    pen.rect((p.x, bot), (p.x + p.w, top));
                }
                if r.pe {
                    pen.hatch((p.x, bot), (p.x + p.w, top));
                }
                // Label (with a fuse's rating) and the terminal's own place, rotated.
                let label = match (&r.rating, r.breaks()) {
                    (Some(rt), true) => format!("{} {rt}", r.shown),
                    _ => r.shown.clone(),
                };
                let mid = if r.breaks() { yc + up + g.body * 0.2 } else { yc + up };
                pen.text((cx - 1.2, mid), lk.point, &label, HAlign::Center, true, Some(g.body * if r.breaks() { 0.55 } else { 0.9 }));
                if let Some(place) = &r.place {
                    pen.text((cx + 2.0, mid), lk.reference, &place.text(style), HAlign::Center, true, Some(g.body * 0.55));
                }
                if r.breaks() {
                    // A fuse a 3 × 6 mm box on a line, a disconnect a slanted stroke.
                    let (y0, y1) = (bot + 1.0, yc + up - 1.0);
                    if r.ty.code == "fuse" {
                        let m = (y0 + y1) / 2.0;
                        pen.line((cx, y0), (cx, y1));
                        pen.rect((cx - 1.5, m - 3.0), (cx + 1.5, m + 3.0));
                    } else {
                        pen.line((cx, y0), (cx, y0 + 2.0));
                        pen.line((cx - 1.5, y0 + 2.0), (cx + 1.5, y1 - 2.0));
                        pen.line((cx, y1 - 2.0), (cx, y1));
                    }
                }
                if s.acc.markers.contains(&r.label) && ecad_electrical::terminal_strips::first_of_label(s, r) {
                    pen.rect((cx - 1.5, top), (cx + 1.5, top + 1.5));
                }
                if r.free {
                    pen.text((cx, top + 2.0), lk.table, "\u{2014}", HAlign::Center, false, None);
                    pen.text((cx, bot - 2.0), lk.table, "\u{2014}", HAlign::Center, false, None);
                }
                if r.connection_point.is_none() {
                    continue;
                }
                side(doc, pen, &r.inside, cx, top, fr.yc + half + fr.lead, true, fr, lk);
                side(doc, pen, &r.outside, cx, bot, fr.yc - half - fr.lead, false, fr, lk);
                let n = r.outside.len().min(MAX_LEADS_PER_SIDE);
                for (k, c) in r.outside.iter().take(MAX_LEADS_PER_SIDE).enumerate() {
                    if c.cable.is_some() {
                        out_leads.push((lead_x(cx, k, n), c));
                    }
                }
            }
        }
    }
    level_boxes(pen, &placed, g, yc);
    jumper_bars(pen, &placed, s, yc);
    bridges(pen, &placed, g, yc, lk);
    cables(doc, pen, &cols, c0, &out_leads, g, fr, lk, o);
}

/// The tag box left of the first column.
fn tag_box(pen: &mut Pen, s: &StripData, g: &Geometry, fr: &Frame, lk: &Draws) {
    let (x0, x1) = (fr.l, fr.l + g.tagbox);
    let (y0, y1) = (fr.yc - g.body / 2.0 - 4.0, fr.yc + g.body / 2.0 + 4.0);
    pen.rect((x0, y0), (x1, y1));
    let room = Some(y1 - y0 - 2.0);
    pen.text((x0 + g.tagbox * 0.3, fr.yc), lk.tag, &s.tag, HAlign::Center, true, room);
    let loc = s.location.clone().unwrap_or_else(|| MISSING.into());
    pen.text((x0 + g.tagbox * 0.62, fr.yc), lk.location, &loc, HAlign::Center, true, room);
    pen.text((x0 + g.tagbox * 0.85, fr.yc), lk.description, &s.counts_text(), HAlign::Center, true, room);
    // The lanes the band uses: jumper bars in its upper half, wire bridges in its lower half.
    if !s.acc.jumper_bars.is_empty() {
        pen.text((x0, y1 + 2.0), lk.description, "jumper bars \u{25b8}", HAlign::Left, false, Some(g.tagbox));
    }
    if s.rows.iter().any(|r| r.bridge_group.is_some()) {
        pen.text((x0, y0 - 2.0), lk.description, "bridges \u{25b8}", HAlign::Left, false, Some(g.tagbox));
    }
}

/// A gap accessory's column.
fn gap(pen: &mut Pen, acc: Acc, p: &Placed<'_>, g: &Geometry, yc: f64, s: &StripData, lk: &Draws) {
    let half = g.body / 2.0;
    match acc {
        Acc::EndStop => {
            let (y0, y1) = (yc - half - 2.0, yc + half + 2.0);
            pen.bar((p.x + 0.25, y0), (p.x + 1.25, y1));
            pen.bar((p.x + 1.75, y0), (p.x + 2.75, y1));
        }
        Acc::EndPlate => pen.bar((p.x + 0.25, yc - half), (p.x + p.w - 0.25, yc + half)),
        Acc::Partition => pen.bar((p.x + 0.4, yc - half - 3.0), (p.x + p.w - 0.4, yc + half + 3.0)),
        Acc::StripLabel | Acc::Marker => {
            pen.rect((p.x + 0.5, yc - half * 0.8), (p.x + p.w - 0.5, yc + half * 0.8));
            pen.text((p.cx(), yc), lk.description, &s.tag, HAlign::Center, true, Some(g.body * 0.75));
        }
    }
}

/// One side's leads of a column: lead, dot, wire number, core label (outside), target.
#[allow(clippy::too_many_arguments)]
fn side(doc: &Document, pen: &mut Pen, cells: &[SideCell], cx: f64, from: f64, to: f64, inside: bool, fr: &Frame, lk: &Draws) {
    let n = cells.len().min(MAX_LEADS_PER_SIDE);
    let lead = (to - from).abs();
    for (k, c) in cells.iter().take(MAX_LEADS_PER_SIDE).enumerate() {
        let x = lead_x(cx, k, n);
        pen.line((x, from), (x, to));
        pen.dot((x, from), DOT_MM);
        if inside {
            pen.value(doc, (x - 1.0, from + 1.5), lk.wire, c.wire.as_deref(), HAlign::Left, Some(lead * 0.6));
            let room = (fr.t - to - 2.0).clamp(5.0, IN_TARGET_MM);
            pen.value(doc, (x, to + 1.0), lk.reference, c.target.as_deref(), HAlign::Left, Some(room));
        } else {
            pen.value(doc, (x - 1.0, from - 1.5), lk.wire, c.wire.as_deref(), HAlign::Right, Some(lead * 0.45));
            if c.cable.is_some() {
                pen.value(doc, (x + 1.0, from - lead * 0.55), lk.cable, c.core.as_deref(), HAlign::Right, Some(lead * 0.4));
            }
            pen.value(doc, (x, to - 1.0), lk.reference, c.target.as_deref(), HAlign::Right, Some(OUT_TARGET_MM));
        }
    }
    if cells.len() > MAX_LEADS_PER_SIDE {
        let more = format!("+{}", cells.len() - MAX_LEADS_PER_SIDE);
        let y = if inside { (to + 1.0 + IN_TARGET_MM).min(fr.t - 2.0) } else { to - 2.0 - OUT_TARGET_MM };
        pen.text((cx, y), lk.description, &more, HAlign::Center, false, None);
    }
}

/// One outline around the levels of a multi-level position (`ecad.level-box`), and the
/// internal bridges inside it.
fn level_boxes(pen: &mut Pen, placed: &[Placed<'_>], g: &Geometry, yc: f64) {
    let half = g.body / 2.0;
    let mut i = 0;
    while i < placed.len() {
        let Some(Col::Term(_, r)) = placed.get(i).map(|p| p.col) else {
            i += 1;
            continue;
        };
        let mut e = i + 1;
        while let Some(Col::Term(_, n)) = placed.get(e).map(|p| p.col) {
            if n.label != r.label {
                break;
            }
            e += 1;
        }
        if r.ty.multi()
            && let (Some(a), Some(z)) = (placed.get(i), placed.get(e - 1))
        {
            let top = placed
                .get(i..e)
                .unwrap_or_default()
                .iter()
                .filter_map(|p| if let Col::Term(_, x) = p.col { Some(ecad_electrical::terminal_strips::raise(x)) } else { None })
                .fold(0.0, f64::max);
            pen.rect((a.x - 0.5, yc - half - 0.8), (z.x + z.w + 0.5, yc + half + top + 0.8));
            // Internal bridges: levels of one group joined near the bottom.
            let mut groups: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
            for p in placed.get(i..e).unwrap_or_default() {
                if let Col::Term(_, x) = p.col
                    && let Some(gr) = x.internal_bridge
                {
                    groups.entry(gr).or_default().push(p.cx());
                }
            }
            for (k, xs) in groups.values().enumerate() {
                if let (Some(a), Some(z)) = (xs.first(), xs.last())
                    && xs.len() > 1
                {
                    let y = yc - half + 1.5 + k as f64 * 1.0;
                    pen.line_w((*a, y), (*z, y), ecad_electrical::cables::layout::CABLE_WEIGHT);
                }
            }
        }
        i = e;
    }
}

/// Jumper bars in lanes in the band's upper half, with continuation stubs at the sheet's edges.
fn jumper_bars(pen: &mut Pen, placed: &[Placed<'_>], s: &StripData, yc: f64) {
    let positions: Vec<usize> = placed.iter().filter_map(|p| if let Col::Term(i, _) = p.col { Some(s.position_index(i)) } else { None }).collect();
    let (Some(lo), Some(hi)) = (positions.iter().min().copied(), positions.iter().max().copied()) else { return };
    for l in s.acc.jumper_bars.iter().filter(|l| l.span > 0 && l.last >= lo && l.first <= hi) {
        let y = yc + 3.0 + l.lane.min(4) as f64 * JUMPER_BAR_LANE_MM;
        let pts: Vec<(f64, bool)> = placed
            .iter()
            .filter_map(|p| match p.col {
                Col::Term(i, r) => {
                    let pi = s.position_index(i);
                    (l.first <= pi && pi <= l.last && l.runs_on(r.level)).then(|| (p.cx(), l.labels.contains(&r.label) && l.names_level(r.level)))
                }
                Col::Gap(_) => None,
            })
            .collect();
        let (Some(first), Some(last)) = (pts.first(), pts.last()) else { continue };
        for w in pts.windows(2) {
            if let [a, b] = w {
                if a.1 && b.1 {
                    pen.line((a.0, y), (b.0, y));
                } else {
                    pen.dashed((a.0, y), (b.0, y));
                }
            }
        }
        for (x, on) in &pts {
            if *on {
                pen.dot((*x, y), DOT_MM);
            }
        }
        if l.first < lo {
            pen.line((first.0 - JUMPER_BAR_STUB_MM, y), (first.0, y));
        }
        if l.last > hi {
            pen.line((last.0, y), (last.0 + JUMPER_BAR_STUB_MM, y));
        }
    }
}

/// Wire bridges (the bridge groups) in the band's lower half, with the group number.
fn bridges(pen: &mut Pen, placed: &[Placed<'_>], g: &Geometry, yc: f64, lk: &Draws) {
    let mut groups: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
    for p in placed {
        if let Col::Term(_, r) = p.col
            && let Some(gr) = r.bridge_group
        {
            groups.entry(gr).or_default().push(p.cx());
        }
    }
    let lanes = ((g.body / 2.0 - 4.0) / JUMPER_BAR_LANE_MM).floor().max(1.0) as usize;
    for (gr, xs) in &groups {
        let y = yc - 3.0 - (gr.saturating_sub(1) % lanes) as f64 * JUMPER_BAR_LANE_MM;
        if let (Some(a), Some(z)) = (xs.first(), xs.last()) {
            pen.line((*a, y), (*z, y));
            pen.text((*a - 0.8, y), lk.description, &gr.to_string(), HAlign::Right, false, None);
        }
        for x in xs {
            pen.dot((*x, y), DOT_MM);
        }
    }
}

/// Cable bars in lanes below the outside targets, one box per bar with its tag, type and
/// maker, and an arrow to the far devices.
#[allow(clippy::too_many_arguments)]
fn cables(
    doc: &Document, pen: &mut Pen, cols: &[(Col<'_>, f64)], c0: usize, leads: &[(f64, &SideCell)], g: &Geometry, fr: &Frame, lk: &Draws,
    o: &ReportModels,
) {
    // Per cable on this sheet: first and last lead x, its far devices.
    let mut bars: BTreeMap<CableId, (f64, f64, Option<String>, BTreeSet<DeviceId>)> = BTreeMap::new();
    for (x, c) in leads {
        let Some((k, tag)) = &c.cable else { continue };
        let e = bars.entry(*k).or_insert((*x, *x, tag.clone(), BTreeSet::new()));
        e.0 = e.0.min(*x);
        e.1 = e.1.max(*x);
        e.3.extend(c.far);
    }
    if bars.is_empty() {
        return;
    }
    // Lanes: greedy by first lead, then tag (stable).
    let mut order: Vec<(CableId, f64, f64, Option<String>, BTreeSet<DeviceId>)> = bars.into_iter().map(|(k, (a, b, t, f))| (k, a, b, t, f)).collect();
    order.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.3.cmp(&b.3)).then(a.0.cmp(&b.0)));
    let mut lane_end: Vec<f64> = Vec::new();
    let mut lanes: Vec<usize> = Vec::with_capacity(order.len());
    for (_, a, b, _, _) in &order {
        let lane = match lane_end.iter().position(|e| *e < a - 0.1) {
            Some(i) => i,
            None if lane_end.len() < MAX_CABLE_LANES => {
                lane_end.push(f64::NEG_INFINITY);
                lane_end.len() - 1
            }
            None => MAX_CABLE_LANES - 1,
        };
        if let Some(e) = lane_end.get_mut(lane) {
            *e = e.max(*b);
        }
        lanes.push(lane);
    }
    let bar0 = fr.yc - g.body / 2.0 - fr.lead - 1.0 - OUT_TARGET_MM - g.bar;
    let used = lane_end.len().max(1);
    // Kept above the area's bottom with the largest registry values.
    let box_top = (bar0 - (used - 1) as f64 * CABLE_LANE_MM - 3.0).max(fr.b + 8.0);
    let box_bot = (box_top - BOX_H_MM).max(fr.b + 4.0);
    // A cable already on an earlier sheet of this strip says `(cont.)`.
    let earlier: BTreeSet<CableId> = cols
        .get(..c0)
        .unwrap_or_default()
        .iter()
        .filter_map(|(c, _)| if let Col::Term(_, r) = c { Some(r) } else { None })
        .flat_map(|r| r.outside.iter().filter_map(|c| c.cable.as_ref().map(|(k, _)| *k)))
        .collect();
    for ((k, a, b, tag, far), lane) in order.iter().zip(&lanes) {
        let y = bar0 - *lane as f64 * CABLE_LANE_MM;
        pen.line_w((*a - 1.0, y), (*b + 1.0, y), ecad_electrical::cables::layout::CABLE_WEIGHT.saturating_mul(2));
        // The box under the bar's first lead, joined to the bar.
        let (bx0, bx1) = (a - BOX_W_MM / 2.0, a + BOX_W_MM / 2.0);
        pen.line((*a, y), (*a, box_top));
        pen.rect((bx0, box_bot), (bx1, box_top));
        let room = Some(box_top - box_bot - 2.0);
        let mid = (box_top + box_bot) / 2.0;
        let row = o.cable_row(*k);
        let tag_text = match tag.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            Some(t) if earlier.contains(k) => Some(format!("{t} (cont.)")),
            Some(t) => Some(t.to_string()),
            None => None,
        };
        pen.value(doc, (a - 2.5, mid), lk.cable, tag_text.as_deref(), HAlign::Center, room);
        if let Some(kind) = row.and_then(|r| r.kind.as_deref()) {
            pen.text((*a, mid), lk.cable, kind, HAlign::Center, true, room);
        }
        if let Some(m) = row.and_then(|r| r.maker.as_deref()) {
            pen.text((a + 2.5, mid), lk.cable, m, HAlign::Center, true, room);
        }
        // The arrow to the far devices, and their tags and descriptions.
        let ay = box_bot - 3.0;
        if ay - 2.0 > fr.b {
            pen.line((*a, box_bot), (*a, ay));
            pen.line((a - 0.8, ay + 1.2), (*a, ay));
            pen.line((a + 0.8, ay + 1.2), (*a, ay));
            let mut names: Vec<(String, Option<String>)> =
                far.iter().filter_map(|f| o.device(*f)).map(|d| (d.tag.clone(), d.description.clone().filter(|x| !x.trim().is_empty()))).collect();
            names.sort_by_key(|x| ecad_electrical::locations::natural_key(&x.0));
            let room = Some((ay - 1.0 - fr.b).max(2.0));
            let shown = names.len().min(MAX_FAR_DEVICES);
            for (n, (t, d)) in names.iter().take(MAX_FAR_DEVICES).enumerate() {
                let x = a + (n as f64 - (shown as f64 - 1.0) / 2.0) * 2.4;
                let text = match d {
                    Some(d) => format!("{t} {d}"),
                    None => t.clone(),
                };
                pen.text((x, ay - 1.0), lk.cable, &text, HAlign::Right, true, room);
            }
            if names.len() > MAX_FAR_DEVICES {
                pen.text((a + 4.5, ay - 1.0), lk.description, &format!("+{}", names.len() - MAX_FAR_DEVICES), HAlign::Left, false, None);
            }
        }
    }
}
pub use ecad_electrical::terminal_strips::pages::*;
