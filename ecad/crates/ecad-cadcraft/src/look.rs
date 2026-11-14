//! How a cable looks on a page (brief 009, design 009): the restyled fan (label box, thick trunk
//! and spines, curved bends, thin tails with core labels and wire numbers), the marker drawn at a
//! cable's reference end, and core labels at the slash crossings in traditional view.
//!
//! # Semantics
//! - Everything here is computed from the document every time a page is projected and is never
//!   stored (design 005 rule 3). It reads the members' `core`, the cable's `tag`, `designation`
//!   and `type`, the conductors' `tag`, the routes, the slashes and the cable's `reference`
//!   representations (brief 010's model: one Point item each).
//! - Sizes are drawing units ([`LABEL_HEIGHT`] and the others); text widths are measured with the
//!   drafting font CADCraft draws text with ([`font_width`]), passed in as a function so the
//!   geometry can be tested on its own.
//! - Hostile values never panic: any value that is not finite makes the fan fall back to the
//!   traditional drawing on that page, and text is shortened before it is measured.
//!
//! This module draws nothing itself and records nothing: the projection turns [`Part`]s into
//! entities on [`crate::views::LAYER`].

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::{CableId, ConductorId, Document, End, ItemId, ItemKind, PageId, Primitive, Pt, RepresentationId, Subject};

// ---------------------------------------------------------------- tokens (design §12)

/// The height the cable look is laid out for (label spacing, box pitch and padding, and the width
/// [`font_width`] measures). The texts themselves are drawn at the project's cable text and
/// reference heights (brief 031, `text_style`), never larger than this by default, so they fit.
pub const LABEL_HEIGHT: f64 = 2.5;
/// `ecad.label-gap`: between the bend and the first label, between labels, after the last.
pub const LABEL_GAP: f64 = 1.0;
/// Between a core label and the wire number after it: one text height, so the two never read as
/// one (overseer decision on the brief 009 review, `W4 4` not `W44`).
pub const LABEL_SPACING: f64 = LABEL_HEIGHT;
/// `ecad.tail-min`: the shortest tail in fan view.
pub const TAIL_MIN: f64 = 20.0;
/// `ecad.bend-radius`, capped at half the core pitch.
pub const BEND_RADIUS: f64 = 2.5;
/// `ecad.box-line-pitch`: line spacing in the label box.
pub const BOX_LINE_PITCH: f64 = 1.5 * LABEL_HEIGHT;
/// `ecad.box-pad`: padding in the label box and the reference flag.
pub const BOX_PAD: f64 = 0.5 * LABEL_HEIGHT;
/// `ecad.cable-weight`: trunk and spine (cable section), hundredths of a millimetre.
pub const CABLE_WEIGHT: u16 = 70;
/// `ecad.wire-weight`: bends, tails, label box frame, reference flag (wire section).
pub const WIRE_WEIGHT: u16 = 25;

/// Longest core label accepted (Unicode scalar values, after trimming).
pub const MAX_CORE_CHARS: usize = 32;
/// Wire numbers and label box lines longer than this are drawn shortened.
pub const MAX_SHOWN_CHARS: usize = 24;
/// The representation role of a cable's reference end: brief 010's, one definition for both briefs.
pub use ecad_model::derive::{MAX_REFERENCES_PER_CABLE, REFERENCE_ROLE};
/// The representation role of a stored fan join (owner request, 2026-11-07): one Point item per
/// fanned cable end per page, where the trunk joins the spine. Without one the trunk joins the
/// middle of the spine.
pub const ANCHOR_ROLE: &str = "fan-anchor";
/// The text a reference end shows when brief 010 cannot derive one (a hostile file): an orphan.
pub const REFERENCE_PLACEHOLDER: &str = "/?";
/// `ecad.orphan`: a reference end with no other end shows its text in red.
pub const ORPHAN_ACI: u8 = crate::references::ORPHAN_ACI;

/// The width of `text` at [`LABEL_HEIGHT`] in the drafting font CADCraft draws text with (0 for
/// anything that does not measure).
pub fn font_width(text: &str) -> f64 {
    let w = cadcraft_engine::fonts::line_width(text, LABEL_HEIGHT, 1.0);
    if w.is_finite() && w >= 0.0 { w } else { 0.0 }
}

/// `text` as drawn: control characters become `?`, and text longer than `max` characters is cut
/// to `max - 1` characters and `…` (design §4.5). The stored value is never changed.
pub fn shorten(text: &str, max: usize) -> String {
    let clean: Vec<char> = text.chars().map(|c| if c.is_control() { '?' } else { c }).collect();
    if clean.len() <= max {
        return clean.into_iter().collect();
    }
    let mut out: String = clean.into_iter().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// The text a reference end shows (design 009 §7.4): brief 010's live reference (`/2.3`), or
/// `/?` for an orphan. The one function both the marker and the hover take it from.
pub fn reference_text(doc: &Document, reference: RepresentationId) -> String {
    reference_marker(doc, reference).0
}

/// [`reference_text`] with whether the end is an orphan (drawn red).
pub fn reference_marker(doc: &Document, reference: RepresentationId) -> (String, bool) {
    let (text, orphan, _) = reference_marker_side(doc, reference);
    (text, orphan)
}

/// [`reference_marker`] with the side its flag sits on (brief 010's direction away from the wires).
fn reference_marker_side(doc: &Document, reference: RepresentationId) -> (String, bool, FlagSide) {
    match crate::references::marker_text_dir(doc, reference) {
        Some((text, orphan, dir)) => (text, orphan, FlagSide::away(dir)),
        None => (REFERENCE_PLACEHOLDER.to_string(), true, FlagSide::Right),
    }
}

/// The side of its reference point a reference end's flag sits on, pointing away from the cable's
/// wires (design 009 §7.2, with design 010 §6.2's direction u: the opposite of the mean direction in
/// which the member routes on the page run away from the point). The flat end faces the wires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagSide {
    /// Wires come from the left: the flag reaches right of the point (fixture R's page 1).
    Right,
    /// Wires come from the right: the flag reaches left of the point (fixture R's page 2).
    Left,
    /// Wires come from below: the flag sits above the point, pointing up.
    Up,
    /// Wires come from above: the flag sits below the point, pointing down.
    Down,
}

impl FlagSide {
    /// The side for direction `u` (away from the wires), by its larger component; a tie, a zero or
    /// a non-finite direction is horizontal, and `u.x = 0` horizontal is `Right` (design 010's
    /// left-aligned text when `u.x ≥ 0`).
    pub fn away(u: Pt) -> FlagSide {
        if !u.is_finite() || u.x.abs() >= u.y.abs() {
            if u.x < 0.0 { FlagSide::Left } else { FlagSide::Right }
        } else if u.y > 0.0 {
            FlagSide::Up
        } else {
            FlagSide::Down
        }
    }
}

/// A reference end's flag laid out at `at` around a text `w` wide, on `side`: the rectangle
/// `(lo, hi)`, where the text starts (bottom left, height [`LABEL_HEIGHT`]), the closed outline
/// (rectangle plus a point of depth h on `side`) and the attach point M (middle of the flat side).
/// The reference point stays inside the rectangle, h/2 from the flat side.
pub struct FlagShape {
    pub lo: Pt,
    pub hi: Pt,
    pub text_at: Pt,
    pub outline: Vec<Pt>,
    pub attach: Pt,
}

/// Lay out a flag (see [`FlagShape`]). Midpoints are `lo + (hi - lo) / 2`, finite for any finite
/// rectangle.
pub fn flag_shape(at: Pt, w: f64, side: FlagSide) -> FlagShape {
    let h = LABEL_HEIGHT;
    let (lo, hi, text_at) = match side {
        FlagSide::Right => (Pt::new(at.x - 0.5 * h, at.y - 0.5 * h), Pt::new(at.x + w + 0.5 * h, at.y + 1.5 * h), at),
        FlagSide::Left => (Pt::new(at.x - w - 0.5 * h, at.y - 0.5 * h), Pt::new(at.x + 0.5 * h, at.y + 1.5 * h), Pt::new(at.x - w, at.y)),
        FlagSide::Up => {
            (Pt::new(at.x - 0.5 * w - 0.5 * h, at.y - 0.5 * h), Pt::new(at.x + 0.5 * w + 0.5 * h, at.y + 1.5 * h), Pt::new(at.x - 0.5 * w, at.y))
        }
        FlagSide::Down => {
            (Pt::new(at.x - 0.5 * w - 0.5 * h, at.y - 1.5 * h), Pt::new(at.x + 0.5 * w + 0.5 * h, at.y + 0.5 * h), Pt::new(at.x - 0.5 * w, at.y - h))
        }
    };
    let xm = lo.x + 0.5 * (hi.x - lo.x);
    let ym = lo.y + 0.5 * (hi.y - lo.y);
    let (outline, attach) = match side {
        FlagSide::Right => (vec![lo, Pt::new(hi.x, lo.y), Pt::new(hi.x + h, ym), hi, Pt::new(lo.x, hi.y)], Pt::new(lo.x, ym)),
        FlagSide::Left => (vec![lo, Pt::new(hi.x, lo.y), hi, Pt::new(lo.x, hi.y), Pt::new(lo.x - h, ym)], Pt::new(hi.x, ym)),
        FlagSide::Up => (vec![lo, Pt::new(hi.x, lo.y), hi, Pt::new(xm, hi.y + h), Pt::new(lo.x, hi.y)], Pt::new(xm, lo.y)),
        FlagSide::Down => (vec![lo, Pt::new(xm, lo.y - h), Pt::new(hi.x, lo.y), hi, Pt::new(lo.x, hi.y)], Pt::new(xm, hi.y)),
    };
    FlagShape { lo, hi, text_at, outline, attach }
}

// ---------------------------------------------------------------- what is drawn

/// One computed shape.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// Two or more points; `closed` for the label box and the reference flag.
    Path { points: Vec<Pt>, closed: bool },
    /// Radians, counter-clockwise from `start` to `end`.
    Arc { center: Pt, radius: f64, start: f64, end: f64 },
    /// Left-baseline text at [`LABEL_HEIGHT`].
    Text { at: Pt, rotation: f64, value: String },
}

/// Line weight of a part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    /// [`CABLE_WEIGHT`]: trunk and spine.
    Cable,
    /// [`WIRE_WEIGHT`]: bends, tails, box frame, flag.
    Wire,
    /// Text (the layer's weight).
    Text,
}

/// What a part is (for hover, picks and tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Trunk,
    Spine,
    Bend,
    Tail,
    /// A core label at the start of a tail (fan view).
    CoreLabel,
    /// A wire number at the start of a tail (fan view).
    WireNumber,
    LabelBox,
    BoxText,
    /// The flag around a reference end's text.
    Flag,
    ReferenceText,
    /// A core label just past a slash crossing (traditional view).
    CoreAtSlash,
}

/// One computed part of a cable's drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub role: Role,
    pub shape: Shape,
    pub weight: Weight,
    /// The member the part belongs to (bends, tails, their labels, core labels).
    pub core: Option<ConductorId>,
    /// The reference end the part belongs to (flag, reference text).
    pub reference: Option<RepresentationId>,
    /// A colour of its own (ACI), else the layer's: an orphan reference's text is red.
    pub color: Option<u8>,
}

impl Part {
    fn new(role: Role, shape: Shape, weight: Weight) -> Part {
        Part { role, shape, weight, core: None, reference: None, color: None }
    }
    fn of_core(mut self, c: ConductorId) -> Part {
        self.core = Some(c);
        self
    }
    fn of_reference(mut self, r: RepresentationId) -> Part {
        self.reference = Some(r);
        self
    }
}

/// How a cable in fan view came out on a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FanState {
    /// Traditional view (or no fan asked for).
    #[default]
    NotAsked,
    /// Drawn as a fan.
    Drawn,
    /// Fewer than two of its wires on this page: drawn traditional here (design §4.1).
    TooFew,
    /// A route of no length, or a value that is not finite: drawn traditional here.
    Failed,
}

/// Which end of the cable on a page a reference end stops (design §7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerEnd {
    /// The side of the reference member's first point (r₀), shown as "A side".
    A,
    /// The side of its last point (r₁), "B side".
    B,
    /// A third or later reference on the page: it stops no end.
    Extra,
}

/// A cable's computed drawing on one page.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CableDrawing {
    pub state: FanState,
    /// Items not drawn: the member routes and slashes a fan replaces.
    pub hidden: Vec<ItemId>,
    pub parts: Vec<Part>,
    /// Loose wire ends at an end that stops at a reference: no loose-end square there.
    pub quiet: Vec<Pt>,
    /// Members attached to a terminal at an end that stops at a reference.
    pub at_reference: Vec<ConductorId>,
    /// Each reference end on the page and the end it stops.
    pub markers: BTreeMap<RepresentationId, MarkerEnd>,
}

impl CableDrawing {
    /// Whether it is drawn as a fan.
    pub fn is_fan(&self) -> bool {
        self.state == FanState::Drawn
    }
    /// Every path drawn (trunk pieces, spines, tails, box frame, flags).
    pub fn paths(&self) -> impl Iterator<Item = &Vec<Pt>> {
        self.parts.iter().filter_map(|p| match &p.shape {
            Shape::Path { points, .. } => Some(points),
            _ => None,
        })
    }
}

// ---------------------------------------------------------------- vectors

fn add(a: Pt, b: Pt) -> Pt {
    Pt::new(a.x + b.x, a.y + b.y)
}
fn sub(a: Pt, b: Pt) -> Pt {
    Pt::new(a.x - b.x, a.y - b.y)
}
fn mul(a: Pt, s: f64) -> Pt {
    Pt::new(a.x * s, a.y * s)
}
fn dot(a: Pt, b: Pt) -> f64 {
    a.x * b.x + a.y * b.y
}
fn cross(a: Pt, b: Pt) -> f64 {
    a.x * b.y - a.y * b.x
}
fn rot90(a: Pt) -> Pt {
    Pt::new(-a.y, a.x)
}
fn angle(a: Pt) -> f64 {
    a.y.atan2(a.x)
}
fn centroid(points: &[Pt]) -> Option<Pt> {
    if points.is_empty() {
        return None;
    }
    let n = points.len() as f64;
    let s = points.iter().fold(Pt::new(0.0, 0.0), |acc, p| add(acc, *p));
    let c = mul(s, 1.0 / n);
    c.is_finite().then_some(c)
}
/// `o + a·u + t·v`.
fn at(o: Pt, u: Pt, v: Pt, a: f64, t: f64) -> Pt {
    add(o, add(mul(u, a), mul(v, t)))
}

/// The open path of a route.
fn path_of(prim: &Primitive) -> Option<Vec<Pt>> {
    match prim {
        Primitive::Line { a, b } => Some(vec![*a, *b]),
        Primitive::Polyline { vertices, closed: false } if vertices.len() >= 2 => Some(vertices.clone()),
        _ => None,
    }
}

/// The direction in which `path` leaves its first point, snapped to the nearest axis (a tie goes
/// to the x axis); `None` when the path has no length.
fn leaving(path: impl Iterator<Item = Pt>, tol: f64) -> Option<Pt> {
    let mut it = path;
    let o = it.next()?;
    for p in it {
        let d = sub(p, o);
        if !d.is_finite() {
            return None;
        }
        if d.x.hypot(d.y) > tol {
            return Some(if d.x.abs() >= d.y.abs() { Pt::new(d.x.signum(), 0.0) } else { Pt::new(0.0, d.y.signum()) });
        }
    }
    None
}

/// Remove repeated and collinear points of an orthogonal or straight path.
fn tidy(points: Vec<Pt>, tol: f64) -> Vec<Pt> {
    let mut out: Vec<Pt> = Vec::with_capacity(points.len());
    for p in points {
        if out.last().is_some_and(|q| q.near(p, tol)) {
            continue;
        }
        if out.len() >= 2
            && let (Some(a), Some(b)) = (out.get(out.len() - 2).copied(), out.last().copied())
        {
            let (d0, d1) = (sub(b, a), sub(p, b));
            if cross(d0, d1).abs() <= tol * d0.x.hypot(d0.y).max(1.0) && dot(d0, d1) >= 0.0 {
                out.pop();
            }
        }
        out.push(p);
    }
    out
}

/// The parts of `path` outside the rectangle `lo`..`hi` (each two or more points).
fn clip_outside(path: &[Pt], lo: Pt, hi: Pt, tol: f64) -> Vec<Vec<Pt>> {
    let mut pieces: Vec<Vec<Pt>> = Vec::new();
    let mut current: Vec<Pt> = Vec::new();
    for w in path.windows(2) {
        let (Some(a), Some(b)) = (w.first().copied(), w.get(1).copied()) else { continue };
        if current.is_empty() {
            current.push(a);
        }
        match inside_interval(a, b, lo, hi) {
            Some((t0, t1)) if t1 > t0 => {
                let (p0, p1) = (add(a, mul(sub(b, a), t0)), add(a, mul(sub(b, a), t1)));
                current.push(p0);
                pieces.push(std::mem::take(&mut current));
                current.push(p1);
                current.push(b);
            }
            _ => current.push(b),
        }
    }
    pieces.push(current);
    pieces
        .into_iter()
        .map(|p| tidy(p, tol))
        .filter(|p| p.len() >= 2 && p.windows(2).any(|w| w.first().zip(w.get(1)).is_some_and(|(a, b)| !a.near(*b, tol))))
        .collect()
}

/// Liang–Barsky: the parameter interval of `a`→`b` inside the rectangle.
fn inside_interval(a: Pt, b: Pt, lo: Pt, hi: Pt) -> Option<(f64, f64)> {
    let d = sub(b, a);
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    for (p, q) in [(-d.x, a.x - lo.x), (d.x, hi.x - a.x), (-d.y, a.y - lo.y), (d.y, hi.y - a.y)] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
        }
    }
    (t0 <= t1).then_some((t0, t1))
}

// ---------------------------------------------------------------- the cable on a page

/// One member with a route on the page.
struct Member {
    conductor: ConductorId,
    core: Option<String>,
    path: Vec<Pt>,
}

/// A reference end on the page.
struct Marker {
    id: RepresentationId,
    at: Pt,
    text: String,
    width: f64,
    orphan: bool,
    side: FlagSide,
}

/// The box a reference end's marker covers on the page (its flag, point included), for a
/// reference end at `at` showing `text` with its flag on `side`: what a pick of the marker hits
/// (brief 010's pick-first rule). It follows the flag to whichever side it is drawn.
pub fn marker_box(at: Pt, text: &str, side: FlagSide) -> (Pt, Pt) {
    let f = flag_shape(at, font_width(&shorten(text, 64)), side);
    let lo = f.outline.iter().fold(f.lo, |a, p| Pt::new(a.x.min(p.x), a.y.min(p.y)));
    let hi = f.outline.iter().fold(f.hi, |a, p| Pt::new(a.x.max(p.x), a.y.max(p.y)));
    (lo, hi)
}

impl Marker {
    fn shape(&self) -> FlagShape {
        flag_shape(self.at, self.width, self.side)
    }
    /// The flag rectangle's centre, as `lo + (hi - lo) / 2`, which stays finite for a point near
    /// ±f64::MAX.
    fn centre(&self) -> Pt {
        let f = self.shape();
        add(f.lo, mul(sub(f.hi, f.lo), 0.5))
    }
    /// The flag (pointing away from the wires, design 009 §7.2) and its attach point M.
    fn flag(&self) -> (Vec<Pt>, Pt) {
        let f = self.shape();
        (f.outline, f.attach)
    }
}

/// The reference ends of `cable` on `page`, in ID order: each with the point of its first Point
/// (or Text) item. Ends whose point is not finite are left out.
fn markers_of(doc: &Document, idx: &PageIndex<'_>, cable: CableId, measure: &dyn Fn(&str) -> f64) -> Vec<Marker> {
    idx.cable_reps(cable)
        .iter()
        .filter(|(_, r)| r.role == REFERENCE_ROLE)
        .filter_map(|(id, r)| {
            let at = r.items.iter().find_map(|i| match doc.items.get(i).map(|x| &x.kind) {
                Some(ItemKind::Primitive(Primitive::Point { at } | Primitive::Text { at, .. })) => Some(*at),
                _ => None,
            })?;
            let (text, orphan, side) = reference_marker_side(doc, *id);
            let text = shorten(&text, 64);
            let width = measure(&text);
            (at.is_finite() && width.is_finite()).then_some(Marker { id: *id, at, text, width, orphan, side })
        })
        .collect()
}

/// The cable's two sides on the page (design 005 §5 steps 1 and 2): the reference member's index
/// and, per member in order, its (r₀-side, r₁-side) ends.
fn sides(members: &[Member]) -> Option<(usize, Vec<(Pt, Pt)>)> {
    let reference = members.len().checked_sub(1)? / 2;
    let r = &members.get(reference)?.path;
    let (r0, r1) = (*r.first()?, *r.last()?);
    if !(r0.is_finite() && r1.is_finite()) {
        return None;
    }
    let mut out = Vec::with_capacity(members.len());
    for (i, m) in members.iter().enumerate() {
        let (p, q) = (*m.path.first()?, *m.path.last()?);
        if !(p.is_finite() && q.is_finite()) {
            return None;
        }
        if i == reference {
            out.push((r0, r1));
        } else if p.dist(r0) + q.dist(r1) <= q.dist(r0) + p.dist(r1) {
            out.push((p, q));
        } else {
            out.push((q, p));
        }
    }
    Some((reference, out))
}

/// One fanned end's spine (design §4.2, with the owner's movable join): origin O (the reference
/// member's end), `u` towards the spine (snapped to an axis), `v` along it, the spine on the line
/// a = `c`, and the cores' range `tmin..=tmax` along `v`. The trunk joins the spine somewhere in
/// that range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spine {
    pub o: Pt,
    pub u: Pt,
    pub v: Pt,
    pub c: f64,
    pub tmin: f64,
    pub tmax: f64,
}

impl Spine {
    /// The point of the spine's line at `t`.
    pub fn point(&self, t: f64) -> Pt {
        at(self.o, self.u, self.v, self.c, t)
    }
    /// Where the trunk joins for a requested point `p`: `p` projected onto the spine and kept
    /// within the cores' range; `None` for a point that is not finite.
    pub fn join_for(&self, p: Pt) -> Option<Pt> {
        let t = dot(sub(p, self.o), self.v);
        t.is_finite().then(|| self.point(t.clamp(self.tmin, self.tmax)))
    }
    /// The default join: the middle of the cores' range.
    pub fn middle(&self) -> Pt {
        self.point(self.tmin + 0.5 * (self.tmax - self.tmin))
    }
    /// Distance from `p` to the spine segment.
    fn distance(&self, p: Pt) -> f64 {
        let (a, t) = (dot(sub(p, self.o), self.u), dot(sub(p, self.o), self.v));
        let dt = if t < self.tmin {
            self.tmin - t
        } else if t > self.tmax {
            t - self.tmax
        } else {
            0.0
        };
        (a - self.c).hypot(dt)
    }
}

/// A fanned end: its junction J, the trunk's outward direction (`u`: away from the tails), its
/// spine and its parts.
struct FannedEnd {
    junction: Pt,
    outward: Pt,
    parts: Vec<Part>,
}

/// Labels of one core at its tail start.
struct Labels {
    core: Option<String>,
    wire: Option<String>,
    core_w: f64,
    wire_w: f64,
}

/// Everything one fanned end needs before its join is known.
struct Frame {
    spine: Spine,
    t: Vec<f64>,
    rho: f64,
    labels: Vec<Labels>,
}

/// The frame of one fanned end (design §4.2 steps 1 to 5). `ends`: each member's end on this side.
#[allow(clippy::too_many_arguments)]
fn frame(doc: &Document, members: &[Member], reference: usize, side: usize, ends: &[Pt], tol: f64, measure: &dyn Fn(&str) -> f64) -> Option<Frame> {
    let g = LABEL_GAP;
    let o = *ends.get(reference)?;
    let rpath = &members.get(reference)?.path;
    let u = if side == 0 { leaving(rpath.iter().copied(), tol)? } else { leaving(rpath.iter().rev().copied(), tol)? };
    let v = rot90(u);
    let a: Vec<f64> = ends.iter().map(|e| dot(sub(*e, o), u)).collect();
    let t: Vec<f64> = ends.iter().map(|e| dot(sub(*e, o), v)).collect();
    if a.iter().chain(t.iter()).any(|x| !x.is_finite()) {
        return None;
    }
    // Pitch and bend radius.
    let mut sorted = t.clone();
    sorted.sort_by(f64::total_cmp);
    let pitch = sorted.windows(2).filter_map(|w| w.first().zip(w.get(1)).map(|(x, y)| y - x)).filter(|d| *d > tol).fold(f64::INFINITY, f64::min);
    let rho = BEND_RADIUS.min(pitch / 2.0);
    // Labels and tail length.
    let labels: Vec<Labels> = members
        .iter()
        .map(|m| {
            let core = m.core.as_deref().filter(|c| !c.trim().is_empty()).map(|c| shorten(c, MAX_CORE_CHARS));
            let wire =
                doc.conductors.get(&m.conductor).and_then(|c| c.properties.get("tag")).filter(|t| !t.is_empty()).map(|t| shorten(t, MAX_SHOWN_CHARS));
            let core_w = core.as_deref().map_or(0.0, measure);
            let wire_w = wire.as_deref().map_or(0.0, measure);
            Labels { core, wire, core_w, wire_w }
        })
        .collect();
    let wc = labels.iter().map(|l| l.core_w).fold(0.0, f64::max);
    let ww = labels.iter().map(|l| l.wire_w).fold(0.0, f64::max);
    let tail = TAIL_MIN.max(g + wc + LABEL_SPACING + ww + g);
    let amax = a.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let c = amax + tail + rho;
    let (tmin, tmax) = (sorted.first().copied()?, sorted.last().copied()?);
    let spine = Spine { o, u, v, c, tmin, tmax };
    (c.is_finite() && spine.middle().is_finite()).then_some(Frame { spine, t, rho, labels })
}

/// One fanned end joined at `join` (a point on its spine; design §4.2 steps 6 to 9, with the
/// owner's movable join). Each core bends from the spine towards its tail, away from the join;
/// a core level with the join leaves the spine straight.
fn fan_end(f: &Frame, members: &[Member], ends: &[Pt], join: Pt, tol: f64) -> Option<FannedEnd> {
    let g = LABEL_GAP;
    let h = LABEL_HEIGHT;
    let Spine { o, u, v, c, .. } = f.spine;
    let tj = dot(sub(join, o), v);
    if !tj.is_finite() {
        return None;
    }
    let mut parts = Vec::new();
    let (mut lo, mut hi) = (tj, tj);
    let r = mul(u, -1.0);
    for (i, m) in members.iter().enumerate() {
        let (Some(e), Some(ti), Some(l)) = (ends.get(i), f.t.get(i), f.labels.get(i)) else { continue };
        // Towards the join along the spine (+1 above the core, -1 below), and the bend radius.
        let s = if tj >= *ti { 1.0 } else { -1.0 };
        let rad = f.rho.min((tj - ti).abs());
        let bend = rad > tol;
        let start = if bend { at(o, u, v, c - rad, *ti) } else { at(o, u, v, c, *ti) };
        let on_spine = if bend { *ti + s * rad } else { *ti };
        lo = lo.min(on_spine);
        hi = hi.max(on_spine);
        if bend {
            let centre = at(o, u, v, c - rad, *ti + s * rad);
            let (a1, a2) = (angle(u), angle(mul(v, -s)));
            let (s0, s1) = if cross(u, mul(v, -s)) > 0.0 { (a1, a2) } else { (a2, a1) };
            parts.push(Part::new(Role::Bend, Shape::Arc { center: centre, radius: rad, start: s0, end: s1 }, Weight::Wire).of_core(m.conductor));
        }
        if !start.near(*e, tol) {
            parts.push(Part::new(Role::Tail, Shape::Path { points: vec![start, *e], closed: false }, Weight::Wire).of_core(m.conductor));
        }
        // Labels at the tail start, reading left to right or bottom to top (design §4.5).
        let second = if l.core.is_some() { g + l.core_w + LABEL_SPACING } else { g };
        let lift = 0.3 * h;
        let (core_at, wire_at, rotation) = if r.x > 0.5 {
            (add(start, Pt::new(g, lift)), add(start, Pt::new(second, lift)), 0.0)
        } else if r.x < -0.5 {
            (add(start, Pt::new(-g - l.core_w, lift)), add(start, Pt::new(-second - l.wire_w, lift)), 0.0)
        } else if r.y > 0.5 {
            (add(start, Pt::new(-lift, g)), add(start, Pt::new(-lift, second)), std::f64::consts::FRAC_PI_2)
        } else {
            (add(start, Pt::new(-lift, -g - l.core_w)), add(start, Pt::new(-lift, -second - l.wire_w)), std::f64::consts::FRAC_PI_2)
        };
        if let Some(text) = &l.core {
            parts.push(Part::new(Role::CoreLabel, Shape::Text { at: core_at, rotation, value: text.clone() }, Weight::Text).of_core(m.conductor));
        }
        if let Some(text) = &l.wire {
            parts.push(Part::new(Role::WireNumber, Shape::Text { at: wire_at, rotation, value: text.clone() }, Weight::Text).of_core(m.conductor));
        }
    }
    let junction = at(o, u, v, c, tj);
    if hi - lo > tol {
        parts.insert(0, Part::new(Role::Spine, Shape::Path { points: vec![at(o, u, v, c, lo), at(o, u, v, c, hi)], closed: false }, Weight::Cable));
    }
    if !junction.is_finite() || parts.iter().any(|p| !shape_finite(&p.shape)) {
        return None;
    }
    Some(FannedEnd { junction, outward: u, parts })
}

fn shape_finite(s: &Shape) -> bool {
    match s {
        Shape::Path { points, .. } => points.iter().all(|p| p.is_finite()),
        Shape::Arc { center, radius, start, end } => center.is_finite() && radius.is_finite() && start.is_finite() && end.is_finite(),
        Shape::Text { at, rotation, .. } => at.is_finite() && rotation.is_finite(),
    }
}

/// The trunk between two fanned ends (design §4.3).
fn trunk_between(a: &FannedEnd, b: &FannedEnd, tol: f64) -> Vec<Pt> {
    let (j0, d0, j1, d1) = (a.junction, a.outward, b.junction, b.outward);
    let same = |p: Pt, q: Pt| p.near(q, 1e-9);
    if same(d0, d1) {
        let (w0, w1) = (dot(j0, d0), dot(j1, d0));
        let w = w0.max(w1);
        vec![j0, add(j0, mul(d0, w - w0)), add(j1, mul(d0, w - w1)), j1]
    } else if same(d0, mul(d1, -1.0)) {
        if dot(sub(j1, j0), d0) > tol {
            let mid = 0.5 * (dot(j0, d0) + dot(j1, d0));
            vec![j0, add(j0, mul(d0, mid - dot(j0, d0))), add(j1, mul(d0, mid - dot(j1, d0))), j1]
        } else {
            vec![j0, j1]
        }
    } else {
        let corner = if d0.x.abs() > 0.5 { Pt::new(j1.x, j0.y) } else { Pt::new(j0.x, j1.y) };
        if dot(sub(corner, j0), d0) > tol && dot(sub(corner, j1), d1) > tol { vec![j0, corner, j1] } else { vec![j0, j1] }
    }
}

/// The trunk from a fanned end to a reference end's attach point `m`: out from the junction
/// half-way, across to the attach point's row (or column), then on to it.
fn trunk_to_marker(a: &FannedEnd, m: Pt, tol: f64) -> Vec<Pt> {
    let w = dot(sub(m, a.junction), a.outward);
    if w > tol {
        let p1 = add(a.junction, mul(a.outward, 0.5 * w));
        let p2 = add(p1, mul(sub(m, add(a.junction, mul(a.outward, w))), 1.0));
        vec![a.junction, p1, p2, m]
    } else {
        vec![a.junction, m]
    }
}

/// The label box on the trunk (design §4.4): its parts and the trunk clipped by it.
fn label_box(doc: &Document, cable: CableId, trunk: &[Pt], tol: f64, measure: &dyn Fn(&str) -> f64) -> Vec<Part> {
    let props = doc.cables.get(&cable).map(|c| &c.properties);
    let get = |k: &str| props.and_then(|p| p.get(k)).filter(|v| !v.is_empty()).map(|v| shorten(v, MAX_SHOWN_CHARS));
    let mut lines = vec![get("tag").unwrap_or_else(|| "?".to_string())];
    lines.extend(get("designation"));
    lines.extend(get("type"));
    let widths: Vec<f64> = lines.iter().map(|l| measure(l)).collect();
    let h = LABEL_HEIGHT;
    let width = widths.iter().copied().fold(0.0, f64::max) + 2.0 * BOX_PAD;
    let height = (lines.len().saturating_sub(1)) as f64 * BOX_LINE_PITCH + h + 2.0 * BOX_PAD;
    // Centred on the midpoint of the trunk's longest segment (the first on a tie).
    let mut best: Option<(f64, Pt)> = None;
    for w in trunk.windows(2) {
        let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
        let len = a.dist(*b);
        if best.is_none_or(|(l, _)| len > l) {
            best = Some((len, mul(add(*a, *b), 0.5)));
        }
    }
    let Some(centre) = best.map(|(_, c)| c).or_else(|| trunk.first().copied()) else { return Vec::new() };
    let lo = Pt::new(centre.x - width / 2.0, centre.y - height / 2.0);
    let hi = Pt::new(centre.x + width / 2.0, centre.y + height / 2.0);
    let mut parts: Vec<Part> = clip_outside(trunk, lo, hi, tol)
        .into_iter()
        .map(|p| Part::new(Role::Trunk, Shape::Path { points: p, closed: false }, Weight::Cable))
        .collect();
    parts.push(Part::new(Role::LabelBox, Shape::Path { points: vec![lo, Pt::new(hi.x, lo.y), hi, Pt::new(lo.x, hi.y)], closed: true }, Weight::Wire));
    for (i, (line, w)) in lines.into_iter().zip(widths).enumerate() {
        let baseline = hi.y - BOX_PAD - h - i as f64 * BOX_LINE_PITCH;
        parts.push(Part::new(Role::BoxText, Shape::Text { at: Pt::new(centre.x - w / 2.0, baseline), rotation: 0.0, value: line }, Weight::Text));
    }
    parts
}

/// Core labels just past the slash crossings (design §6).
fn slash_labels(doc: &Document, idx: &PageIndex<'_>, cable: CableId, tol: f64) -> Vec<Part> {
    let Some(c) = doc.cables.get(&cable) else { return Vec::new() };
    let core_of: BTreeMap<ConductorId, String> = c
        .members
        .iter()
        .filter_map(|m| m.core.as_deref().filter(|x| !x.trim().is_empty()).map(|x| (m.conductor, shorten(x, MAX_CORE_CHARS))))
        .collect();
    if core_of.is_empty() {
        return Vec::new();
    }
    let h = LABEL_HEIGHT;
    let mut out = Vec::new();
    let slashes: Vec<(Pt, Pt)> = idx
        .cable_reps(cable)
        .iter()
        .flat_map(|(_, r)| r.items.iter())
        .filter_map(|i| match doc.items.get(i).map(|x| &x.kind) {
            Some(ItemKind::Primitive(Primitive::Line { a, b })) => Some((*a, *b)),
            _ => None,
        })
        .collect();
    let crossings = crate::slash::member_crossings_with(c, &slashes, |w| idx.route_prims(w), tol);
    for (w, p, dir) in crossings {
        let Some(text) = core_of.get(&w) else { continue };
        let len = dir.x.hypot(dir.y);
        if !(len.is_finite() && len > 0.0 && p.is_finite()) {
            continue;
        }
        let mut d = mul(dir, 1.0 / len);
        if d.x < -1e-12 || (d.x.abs() <= 1e-12 && d.y < 0.0) {
            d = mul(d, -1.0);
        }
        let n = rot90(d);
        let at = add(p, add(mul(d, 0.5 * h), mul(n, 0.3 * h)));
        out.push(Part::new(Role::CoreAtSlash, Shape::Text { at, rotation: angle(d), value: text.clone() }, Weight::Text).of_core(w));
    }
    out
}

/// `cable` on `page`: its fan when `fan` is asked for and can be drawn, else its traditional
/// computed parts (core labels at the slashes), with its reference ends in both views.
pub fn cable_on_page(doc: &Document, page: PageId, cable: CableId, fan: bool, tol: f64, measure: &dyn Fn(&str) -> f64) -> CableDrawing {
    cable_in(doc, &PageIndex::new(doc, page), cable, fan, tol, measure)
}

/// [`cable_on_page`] with the page's [`PageIndex`] built once by the caller. Any part that is not
/// finite (a point near ±f64::MAX) is left out, in both views.
fn cable_in(doc: &Document, idx: &PageIndex<'_>, cable: CableId, fan: bool, tol: f64, measure: &dyn Fn(&str) -> f64) -> CableDrawing {
    let mut out = cable_unfiltered(doc, idx, cable, fan, tol, measure);
    out.parts.retain(|p| shape_finite(&p.shape));
    out.quiet.retain(|p| p.is_finite());
    out
}

fn cable_unfiltered(doc: &Document, idx: &PageIndex<'_>, cable: CableId, fan: bool, tol: f64, measure: &dyn Fn(&str) -> f64) -> CableDrawing {
    let tol = if tol.is_finite() && tol >= 0.0 { tol } else { 0.0 };
    let Some(c) = doc.cables.get(&cable) else { return CableDrawing::default() };
    let mut seen = BTreeSet::new();
    let mut members: Vec<Member> = Vec::new();
    for m in &c.members {
        if !seen.insert(m.conductor) {
            continue;
        }
        if let Some(path) = idx.route(m.conductor).and_then(path_of) {
            members.push(Member { conductor: m.conductor, core: m.core.clone(), path });
        }
    }
    let markers = markers_of(doc, idx, cable, measure);
    let sides = sides(&members);
    // Ends of each side and their centroids.
    let side_ends: [Vec<Pt>; 2] = match &sides {
        Some((_, s)) => [s.iter().map(|x| x.0).collect(), s.iter().map(|x| x.1).collect()],
        None => [Vec::new(), Vec::new()],
    };
    let cents = [centroid(&side_ends[0]), centroid(&side_ends[1])];
    // Which end each reference stops (design §7.3).
    let mut ends_of: BTreeMap<RepresentationId, MarkerEnd> = markers.iter().map(|m| (m.id, MarkerEnd::Extra)).collect();
    let mut stopped: [Option<usize>; 2] = [None, None];
    if let (Some(c0), Some(c1)) = (cents[0], cents[1]) {
        let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
        for (i, m) in markers.iter().enumerate() {
            pairs.push((m.centre().dist(c0), i, 0));
            pairs.push((m.centre().dist(c1), i, 1));
        }
        pairs.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
        if let Some((_, i, s)) = pairs.first().copied() {
            stopped[s] = Some(i);
            let other = 1 - s;
            let second = pairs.iter().find(|(_, j, t)| *j != i && *t == other).map(|(_, j, _)| *j);
            stopped[other] = second;
        }
        for (s, slot) in stopped.iter().enumerate() {
            if let Some(m) = slot.and_then(|i| markers.get(i)) {
                ends_of.insert(m.id, if s == 0 { MarkerEnd::A } else { MarkerEnd::B });
            }
        }
    }
    // Each marker's flag and attach point: pointing away from the wires that leave its point, the
    // flat side facing them (brief 010's direction, the same rule on every side).
    let flags: Vec<(Vec<Pt>, Pt)> = markers.iter().map(Marker::flag).collect();
    let mut out = CableDrawing { markers: ends_of, ..CableDrawing::default() };
    let mut fanned: Option<(Vec<Part>, Vec<ItemId>, [Option<Pt>; 2])> = None;
    if fan {
        out.state = if members.len() < 2 { FanState::TooFew } else { FanState::Failed };
        if let Some((reference, _)) = &sides
            && members.len() >= 2
        {
            fanned = fan_parts(doc, idx, cable, &members, *reference, &side_ends, &stopped, &flags, tol, measure);
        }
    }
    match fanned {
        Some((parts, hidden, _)) => {
            out.state = FanState::Drawn;
            out.parts = parts;
            out.hidden = hidden;
            // The members' ends at an end that stops at a reference.
            for (s, slot) in stopped.iter().enumerate() {
                if slot.is_none() {
                    continue;
                }
                for (m, e) in members.iter().zip(side_ends.get(s).into_iter().flatten()) {
                    let attached = doc.conductors.get(&m.conductor).is_some_and(|x| {
                        let (a, b) = (m.path.first().copied(), m.path.last().copied());
                        let which = if a.is_some_and(|a| a.near(*e, tol)) {
                            End::A
                        } else if b.is_some_and(|b| b.near(*e, tol)) {
                            End::B
                        } else {
                            return false;
                        };
                        x.end(which).is_some()
                    });
                    if attached {
                        out.at_reference.push(m.conductor);
                    } else {
                        out.quiet.push(*e);
                    }
                }
            }
        }
        None => out.parts = slash_labels(doc, idx, cable, tol),
    }
    // Stored fan joins are not drawn: the trunk shows where the cable joins (owner request).
    out.hidden.extend(idx.cable_reps(cable).iter().filter(|(_, r)| r.role == ANCHOR_ROLE).flat_map(|(_, r)| r.items.iter().copied()));
    for (m, (flag, _)) in markers.iter().zip(&flags) {
        out.parts.push(Part::new(Role::Flag, Shape::Path { points: flag.clone(), closed: true }, Weight::Wire).of_reference(m.id));
        let mut text = Part::new(Role::ReferenceText, Shape::Text { at: m.shape().text_at, rotation: 0.0, value: m.text.clone() }, Weight::Text)
            .of_reference(m.id);
        text.color = m.orphan.then_some(ORPHAN_ACI);
        out.parts.push(text);
    }
    // Brief 010 §6.4, folded in: at each reference end (at most two per cable), the end of each
    // member route here that leaves for another page (the end nearer the reference point) shows
    // no loose-end square, in either view.
    for m in markers.iter().take(MAX_REFERENCES_PER_CABLE) {
        for mem in &members {
            if crate::references::route_pages(doc, mem.conductor).len() < 2 {
                continue;
            }
            let (Some(a), Some(b)) = (mem.path.first().copied(), mem.path.last().copied()) else { continue };
            let near = if a.dist(m.at) <= b.dist(m.at) { a } else { b };
            if near.is_finite() && !out.quiet.iter().any(|q| q.near(near, tol)) {
                out.quiet.push(near);
            }
        }
    }
    out
}

/// The fan's parts and hidden items, or `None` when it cannot be drawn.
#[allow(clippy::too_many_arguments)]
fn fan_parts(
    doc: &Document, idx: &PageIndex<'_>, cable: CableId, members: &[Member], reference: usize, side_ends: &[Vec<Pt>; 2],
    stopped: &[Option<usize>; 2], flags: &[(Vec<Pt>, Pt)], tol: f64, measure: &dyn Fn(&str) -> f64,
) -> Option<(Vec<Part>, Vec<ItemId>, [Option<Pt>; 2])> {
    let attach = |s: usize| stopped.get(s).copied().flatten().and_then(|i| flags.get(i)).map(|(_, m)| *m);
    let m = [attach(0), attach(1)];
    // Each fanned end's frame, then its stored join (if any), then its parts.
    let mut frames: [Option<Frame>; 2] = [None, None];
    for (s, slot) in frames.iter_mut().enumerate() {
        if m.get(s).copied().flatten().is_none() {
            *slot = Some(frame(doc, members, reference, s, side_ends.get(s)?, tol, measure)?);
        }
    }
    let spines = [frames[0].as_ref().map(|f| f.spine), frames[1].as_ref().map(|f| f.spine)];
    let joins = anchors_by_side(doc, idx, cable, &spines);
    let mut fanned: [Option<FannedEnd>; 2] = [None, None];
    for (s, slot) in fanned.iter_mut().enumerate() {
        let Some(f) = frames.get(s).and_then(Option::as_ref) else { continue };
        let join = joins.get(s).copied().flatten().and_then(|(_, _, p)| f.spine.join_for(p)).unwrap_or(f.spine.middle());
        *slot = Some(fan_end(f, members, side_ends.get(s)?, join, tol)?);
    }
    let trunk = match (&fanned[0], &fanned[1], m[0], m[1]) {
        (Some(a), Some(b), _, _) => trunk_between(a, b, tol),
        (Some(a), None, _, Some(mb)) => trunk_to_marker(a, mb, tol),
        (None, Some(b), Some(ma), _) => trunk_to_marker(b, ma, tol),
        (None, None, Some(ma), Some(mb)) => vec![ma, mb],
        _ => return None,
    };
    let trunk = tidy(trunk, tol);
    if trunk.iter().any(|p| !p.is_finite()) {
        return None;
    }
    let mut parts = label_box(doc, cable, &trunk, tol, measure);
    for f in fanned.iter_mut().flatten() {
        parts.append(&mut f.parts);
    }
    if parts.iter().any(|p| !shape_finite(&p.shape)) {
        return None;
    }
    // What the fan replaces: the members' routes here and the cable's slashes (not its reference
    // ends, which stay drawn).
    let mut hidden: Vec<ItemId> = Vec::new();
    for m in members {
        hidden.extend(idx.conductor_reps(m.conductor).iter().flat_map(|r| r.items.iter().copied()));
    }
    hidden.extend(
        idx.cable_reps(cable).iter().filter(|(_, r)| r.role != REFERENCE_ROLE && r.role != ANCHOR_ROLE).flat_map(|(_, r)| r.items.iter().copied()),
    );
    Some((parts, hidden, m))
}

/// The stored fan join of each fanned end (design: owner request): each `fan-anchor`
/// representation of `cable` on the page, with the point of its first Point item, goes to the
/// fanned end whose spine is nearest (one per end; the nearest pair first, then the other end).
/// Each entry: representation, item, point.
fn anchors_by_side(doc: &Document, idx: &PageIndex<'_>, cable: CableId, spines: &[Option<Spine>; 2]) -> [Option<(RepresentationId, ItemId, Pt)>; 2] {
    let found: Vec<(RepresentationId, ItemId, Pt)> = idx
        .cable_reps(cable)
        .iter()
        .filter(|(_, r)| r.role == ANCHOR_ROLE)
        .filter_map(|(id, r)| {
            r.items.iter().find_map(|i| match doc.items.get(i).map(|x| &x.kind) {
                Some(ItemKind::Primitive(Primitive::Point { at })) if at.is_finite() => Some((*id, *i, *at)),
                _ => None,
            })
        })
        .collect();
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for (i, (_, _, p)) in found.iter().enumerate() {
        for (s, spine) in spines.iter().enumerate() {
            if let Some(d) = spine.map(|sp| sp.distance(*p)).filter(|d| d.is_finite()) {
                pairs.push((d, i, s));
            }
        }
    }
    pairs.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
    let mut out: [Option<(RepresentationId, ItemId, Pt)>; 2] = [None, None];
    let mut used = BTreeSet::new();
    for (_, i, s) in pairs {
        if used.contains(&i) || out.get(s).is_some_and(Option::is_some) {
            continue;
        }
        if let (Some(slot), Some(a)) = (out.get_mut(s), found.get(i)) {
            *slot = Some(*a);
            used.insert(i);
        }
    }
    out
}

/// A fanned cable end on a page, for moving where the trunk joins it (`cablefan`).
#[derive(Debug, Clone, PartialEq)]
pub struct FanEnd {
    /// `A` (the reference member's first point) or `B`.
    pub end: MarkerEnd,
    pub spine: Spine,
    /// The stored join, when there is one: representation, item and stored point.
    pub anchor: Option<(RepresentationId, ItemId, Pt)>,
}

/// The fanned ends of `cable` on `page` as its fan would draw them (whatever its view): none when
/// the fan cannot be drawn there; an end that stops at a reference end has no spine.
pub fn fan_ends(doc: &Document, page: PageId, cable: CableId, tol: f64, measure: &dyn Fn(&str) -> f64) -> Vec<FanEnd> {
    let tol = if tol.is_finite() && tol >= 0.0 { tol } else { 0.0 };
    let idx = PageIndex::new(doc, page);
    let drawing = cable_in(doc, &idx, cable, true, tol, measure);
    if drawing.state != FanState::Drawn {
        return Vec::new();
    }
    let Some(c) = doc.cables.get(&cable) else { return Vec::new() };
    let mut seen = BTreeSet::new();
    let members: Vec<Member> = c
        .members
        .iter()
        .filter(|m| seen.insert(m.conductor))
        .filter_map(|m| idx.route(m.conductor).and_then(path_of).map(|path| Member { conductor: m.conductor, core: m.core.clone(), path }))
        .collect();
    let Some((reference, s)) = sides(&members) else { return Vec::new() };
    let ends: [Vec<Pt>; 2] = [s.iter().map(|x| x.0).collect(), s.iter().map(|x| x.1).collect()];
    let stopped: BTreeSet<usize> = drawing
        .markers
        .values()
        .filter_map(|e| match e {
            MarkerEnd::A => Some(0),
            MarkerEnd::B => Some(1),
            MarkerEnd::Extra => None,
        })
        .collect();
    let mut spines: [Option<Spine>; 2] = [None, None];
    for (side, slot) in spines.iter_mut().enumerate() {
        if !stopped.contains(&side)
            && let Some(f) = ends.get(side).and_then(|e| frame(doc, &members, reference, side, e, tol, measure))
        {
            *slot = Some(f.spine);
        }
    }
    let anchors = anchors_by_side(doc, &idx, cable, &spines);
    spines
        .iter()
        .enumerate()
        .filter_map(|(side, sp)| {
            sp.map(|spine| FanEnd { end: if side == 0 { MarkerEnd::A } else { MarkerEnd::B }, spine, anchor: anchors.get(side).copied().flatten() })
        })
        .collect()
}

/// Every cable's computed drawing on `page` (cables in `fan` drawn as fans where they can be),
/// leaving out cables with nothing computed there.
pub fn cables_on_page(
    doc: &Document, page: PageId, fan: &BTreeSet<CableId>, tol: f64, measure: &dyn Fn(&str) -> f64,
) -> BTreeMap<CableId, CableDrawing> {
    let idx = PageIndex::new(doc, page);
    // Only cables shown on the page: with a representation there, or a member routed there.
    let mut shown: BTreeSet<CableId> = idx.cables.keys().copied().collect();
    for w in idx.routes.keys() {
        shown.extend(doc.cables_of(*w));
    }
    shown
        .into_iter()
        .map(|k| (k, cable_in(doc, &idx, k, fan.contains(&k), tol, measure)))
        .filter(|(_, d)| !d.parts.is_empty() || !d.hidden.is_empty())
        .collect()
}

/// What one page shows, indexed once per projection (brief 009 review S4): each conductor's
/// route (its first route item, as `derive::route_on_page` picks it), its route primitives and
/// representations, and each cable's representations. Every cable is then drawn without scanning
/// the whole document again.
pub struct PageIndex<'a> {
    routes: BTreeMap<ConductorId, &'a Primitive>,
    prims: BTreeMap<ConductorId, Vec<&'a Primitive>>,
    conductors: BTreeMap<ConductorId, Vec<&'a ecad_model::Representation>>,
    cables: BTreeMap<CableId, Vec<(RepresentationId, &'a ecad_model::Representation)>>,
}

impl<'a> PageIndex<'a> {
    /// One pass over the representations on `page` (through the page index, brief 014).
    pub fn new(doc: &'a Document, page: PageId) -> PageIndex<'a> {
        let mut idx = PageIndex { routes: BTreeMap::new(), prims: BTreeMap::new(), conductors: BTreeMap::new(), cables: BTreeMap::new() };
        for (id, r) in doc.representations_on(page) {
            match r.subject {
                Subject::Conductor(w) => {
                    idx.conductors.entry(w).or_default().push(r);
                    let prim = |i: &ItemId| match doc.items.get(i).map(|x| &x.kind) {
                        Some(ItemKind::Primitive(p)) => Some(p),
                        _ => None,
                    };
                    idx.prims.entry(w).or_default().extend(r.items.iter().filter_map(prim));
                    if !idx.routes.contains_key(&w)
                        && let Some(p) = r.items.first().and_then(prim).filter(|p| p.path_ends().is_some())
                    {
                        idx.routes.insert(w, p);
                    }
                }
                Subject::Cable(k) => idx.cables.entry(k).or_default().push((id, r)),
                Subject::Component(_) => {}
            }
        }
        idx
    }
    fn route(&self, w: ConductorId) -> Option<&'a Primitive> {
        self.routes.get(&w).copied()
    }
    fn route_prims(&self, w: ConductorId) -> &[&'a Primitive] {
        self.prims.get(&w).map(Vec::as_slice).unwrap_or_default()
    }
    fn conductor_reps(&self, w: ConductorId) -> &[&'a ecad_model::Representation] {
        self.conductors.get(&w).map(Vec::as_slice).unwrap_or_default()
    }
    fn cable_reps(&self, k: CableId) -> &[(RepresentationId, &'a ecad_model::Representation)] {
        self.cables.get(&k).map(Vec::as_slice).unwrap_or_default()
    }
}

// ---------------------------------------------------------------- the canvas (brief 040)

/// The dark sheet is in use (brief 040, step 1): a viewing preference of the person, set by the
/// app, never stored in the project. Process-wide, as there is one window.
static DARK_SHEET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Use the dark sheet (`true`) or the paper sheet (`false`, the default) for ECAD's overlay marks.
pub fn set_dark_sheet(dark: bool) {
    DARK_SHEET.store(dark, std::sync::atomic::Ordering::Relaxed);
}

/// The sheet in use: [`ecad_tokens::sheet::PAPER`] unless [`set_dark_sheet`] chose the dark one.
pub fn sheet() -> &'static ecad_tokens::Sheet {
    if DARK_SHEET.load(std::sync::atomic::Ordering::Relaxed) { &ecad_tokens::sheet::DARK } else { &ecad_tokens::sheet::PAPER }
}

thread_local! {
    /// A sheet the projection uses instead of the one in use ([`with_paper`]: printing).
    static PROJECTION_SHEET: std::cell::Cell<Option<&'static ecad_tokens::Sheet>> = const { std::cell::Cell::new(None) };
}

/// The sheet a page is projected for: [`sheet`], or paper while [`with_paper`] runs.
pub fn projection_sheet() -> &'static ecad_tokens::Sheet {
    PROJECTION_SHEET.with(std::cell::Cell::get).unwrap_or_else(sheet)
}

/// Keeps the sheet [`pin_sheet`] read for this thread until it is dropped.
#[derive(Debug)]
pub struct SheetPin(Option<&'static ecad_tokens::Sheet>);

impl Drop for SheetPin {
    fn drop(&mut self) {
        PROJECTION_SHEET.with(|c| c.set(self.0));
    }
}

/// Read the sheet once and keep it for this thread until the pin drops: a page's projection and
/// its check then agree on every colour. Without it, a sheet switch on another thread
/// (`canvassheet`, which is process-wide) between the projection and the check made a rebuilt
/// page fail its check and become unavailable (CI on PR 88, `history_ui`).
pub fn pin_sheet() -> SheetPin {
    let s = projection_sheet();
    SheetPin(PROJECTION_SHEET.with(|c| c.replace(Some(s))))
}

/// Run `f` with pages projected for the paper sheet (a print is on white paper, whatever sheet
/// the screen shows).
pub fn with_paper<R>(f: impl FnOnce() -> R) -> R {
    let before = PROJECTION_SHEET.with(|c| c.replace(Some(&ecad_tokens::sheet::PAPER)));
    let r = f();
    PROJECTION_SHEET.with(|c| c.set(before));
    r
}

/// The selection with every wire dash replaced by its wire's route (picking a dash picks the
/// wire), or `None` when it holds no dash.
pub fn pick_routes(p: &crate::Project, page: PageId, selection: &[cadcraft_engine::doc::Handle]) -> Option<Vec<cadcraft_engine::doc::Handle>> {
    let map = &p.pages.get(&page)?.map;
    if !selection.iter().any(|h| map.route_of_dash(*h).is_some()) {
        return None;
    }
    let mut out: Vec<cadcraft_engine::doc::Handle> = Vec::with_capacity(selection.len());
    for h in selection {
        let h = map.route_of_dash(*h).unwrap_or(*h);
        if !out.contains(&h) {
            out.push(h);
        }
    }
    Some(out)
}

/// `canvassheet {dark: bool}` (brief 040): the sheet the app shows, for the colours ECAD draws
/// in; the open page is redrawn when it changes. Display only, nothing recorded.
pub fn specs() -> Vec<cadcraft_engine::CommandSpec> {
    vec![
        cadcraft_engine::CommandSpec::new("canvassheet", "Canvas Sheet", run_canvassheet)
            .params("{dark: bool} (display only: the sheet the page is drawn on)")
            .noundo(),
    ]
}

fn run_canvassheet(s: &mut cadcraft_engine::Session, params: &serde_json::Value) -> cadcraft_engine::Result<serde_json::Value> {
    let dark = params.get("dark").and_then(serde_json::Value::as_bool).ok_or_else(|| crate::commands::err("`dark` is true or false."))?;
    let changed = DARK_SHEET.swap(dark, std::sync::atomic::Ordering::Relaxed) != dark;
    if changed {
        // The open page only: there is one page window, and a page is projected afresh when shown.
        let _ = crate::commands::with_any_page(s, |p, pg| {
            p.redraw_window(pg, Some("canvassheet"));
            Ok(())
        });
    }
    Ok(serde_json::json!({"dark": dark, "changed": changed}))
}

/// A token colour as CADCraft's RGB.
pub fn rgb(c: ecad_tokens::Color) -> cadcraft_engine::doc::color::Rgb {
    cadcraft_engine::doc::color::Rgb(c.r, c.g, c.b)
}

/// `ecad.wire_colour.*` (design `040.md` §2): the colour each IEC 60757 code draws in, on the
/// paper and the dark sheet. Black draws in ink, so it shows on both sheets; white draws as a warm
/// grey on paper. Codes as `ecad_model::wiring::COLOUR_CODES` (the single colours).
pub const CODE_COLOURS: [(&str, &str, ecad_tokens::Color, ecad_tokens::Color); 14] = {
    use ecad_tokens::Color as C;
    [
        ("BK", "BLK", ecad_tokens::sheet::PAPER.ink, ecad_tokens::sheet::DARK.ink),
        ("BN", "BRN", C::hex(0x8A4B1F), C::hex(0xC9895A)),
        ("RD", "RED", C::hex(0xC62828), C::hex(0xFF7A70)),
        ("OG", "ORG", C::hex(0xD96800), C::hex(0xFFA34D)),
        ("YE", "YEL", C::hex(0xE3B800), C::hex(0xF2D34B)),
        ("GN", "GRN", C::hex(0x2F8F3A), C::hex(0x6FD67A)),
        ("BU", "BLU", C::hex(0x23408F), C::hex(0x8FB0FF)),
        ("VT", "VIO", C::hex(0x7B3FA0), C::hex(0xC392F0)),
        ("GY", "GRY", C::hex(0x7C8591), C::hex(0xA3ABB6)),
        ("WH", "WHT", C::hex(0xB9B4A6), C::hex(0xF2F3F5)),
        ("PK", "PNK", C::hex(0xD9528F), C::hex(0xFF9CC8)),
        ("GD", "GLD", C::hex(0xA8851C), C::hex(0xE3C463)),
        ("TQ", "TQS", C::hex(0x14948F), C::hex(0x54D3CC)),
        ("SR", "SLR", C::hex(0x98A0A8), C::hex(0xC9CED4)),
    ]
};

/// Longest colour code looked at (characters after trimming).
pub const MAX_CODE_CHARS: usize = 16;

/// Index into [`CODE_COLOURS`] of one colour code, either form, any case.
fn one_code(s: &str) -> Option<usize> {
    CODE_COLOURS.iter().position(|(iec, three, _, _)| s.eq_ignore_ascii_case(iec) || s.eq_ignore_ascii_case(three))
}

/// The colours of a colour code: one colour (`BU`, `BLU`), or two, the first solid and the second
/// laid over it as dashes (`GNYE`, `GN/YE`, `GRN/YEL`, `RDBU`, `YEBN`). `None` for a code the app
/// does not know (it draws in ink and raises an information check) and for an empty one.
///
/// # Example
/// ```
/// use ecad_cadcraft::look::colour_code;
/// assert_eq!(colour_code("GNYE"), colour_code("grn/yel"));
/// assert!(colour_code("GNYE").is_some_and(|(_, dashes)| dashes.is_some()));
/// assert_eq!(colour_code("PURPLISH"), None);
/// ```
pub fn colour_code(code: &str) -> Option<(usize, Option<usize>)> {
    let c = code.trim();
    if c.is_empty() || c.chars().count() > MAX_CODE_CHARS || !c.is_ascii() {
        return None;
    }
    if let Some(i) = one_code(c) {
        return Some((i, None));
    }
    let parts: Vec<&str> = c.split(['/', '-', ' ', '+']).filter(|p| !p.is_empty()).collect();
    match parts.as_slice() {
        [a, b] => Some((one_code(a)?, Some(one_code(b)?))),
        // Run together: two two-letter codes (`GNYE`) or two three-letter ones (`GRNYEL`).
        [ab] => [2usize, 3].iter().find_map(|n| {
            let (a, b) = (ab.get(..*n)?, ab.get(*n..)?);
            if b.len() != *n {
                return None;
            }
            Some((one_code(a)?, Some(one_code(b)?)))
        }),
        _ => None,
    }
}

/// How a wire of colour `code` is drawn on `sheet`: its solid colour, and the dashes' colour for a
/// two-colour code; `None` for an unknown code (drawn in ink).
pub fn wire_colours(code: &str, sheet: &ecad_tokens::Sheet) -> Option<ecad_tokens::wire::WireColors> {
    let dark = sheet.sheet == ecad_tokens::sheet::DARK.sheet;
    let colour = |i: usize| CODE_COLOURS.get(i).map(|(_, _, paper, on_dark)| if dark { *on_dark } else { *paper });
    let (a, b) = colour_code(code)?;
    Some(ecad_tokens::wire::WireColors { solid: colour(a)?, dashes: b.and_then(colour) })
}

/// The colour code a conductor's route is drawn in (brief 050b, design 050 §5.5): its wire
/// category's line colour (`wire`: the insulation colour, its own or the category's; `ink`: none;
/// or a code). `None` when it is drawn in ink.
pub fn conductor_colour(doc: &Document, c: ConductorId) -> Option<String> {
    crate::styles::wire::wire_look(doc, c).route_code()
}

/// [`conductor_colour`] with the project's categories read once (`looks`).
pub fn conductor_colour_in(doc: &Document, looks: &crate::styles::wire::WireLooks, c: ConductorId) -> Option<String> {
    looks.look(doc, c).route_code()
}

/// A wire route drawn in its colours on a page (brief 040, step 3).
#[derive(Debug, Clone, PartialEq)]
pub struct ColouredRoute {
    pub conductor: ConductorId,
    /// The route representation.
    pub rep: ecad_model::RepresentationId,
    /// The handle the route is drawn with on the page (to leave a selected wire to CADCraft).
    pub handle: Option<cadcraft_engine::doc::Handle>,
    pub points: Vec<Pt>,
    pub colours: ecad_tokens::wire::WireColors,
}

/// Most routes [`coloured_routes`] returns for one page.
pub const MAX_COLOURED_ROUTES: usize = 20_000;

/// The wire routes of `page` whose colour code is known, with their colours on `sheet`. Routes
/// not drawn (a cable in fan view hides its members) and plain black wires (ink already) are left
/// out. `map`: the page's handles when it is projected (routes it does not draw are left out).
/// Display and print only; nothing is stored.
pub fn coloured_routes(doc: &Document, page: PageId, map: Option<&crate::project::HandleMap>, sheet: &ecad_tokens::Sheet) -> Vec<ColouredRoute> {
    let mut out = Vec::new();
    let looks = crate::styles::wire::WireLooks::new(doc);
    for (rep, r) in doc.representations_on(page) {
        let Subject::Conductor(c) = r.subject else { continue };
        if r.form != ecad_model::Form::Drawn {
            continue;
        }
        let Some(item) = r.items.first() else { continue };
        let handle = map.and_then(|m| m.item(*item));
        if map.is_some() && handle.is_none() {
            continue;
        }
        let Some(ItemKind::Primitive(prim)) = doc.items.get(item).map(|i| &i.kind) else { continue };
        let Some(points) = crate::wires::path_vertices(prim) else { continue };
        if points.len() < 2 || !points.iter().all(|q| q.x.is_finite() && q.y.is_finite()) {
            continue;
        }
        let Some(colours) = conductor_colour_in(doc, &looks, c).and_then(|code| wire_colours(&code, sheet)) else { continue };
        if colours.solid == sheet.ink && colours.dashes.is_none() {
            continue;
        }
        out.push(ColouredRoute { conductor: c, rep, handle, points, colours });
        if out.len() >= MAX_COLOURED_ROUTES {
            break;
        }
    }
    out
}

/// Most dashes [`dash_segments`] makes for one route.
pub const MAX_DASHES: usize = 4_000;

/// The dashes of a two-colour wire along `points`: `on` long, `off` apart, starting with a dash
/// at the first point (spec: 2 mm on, 2 mm off, `ecad_tokens::wire`). At most [`MAX_DASHES`];
/// none when a length is not positive and finite.
pub fn dash_segments(points: &[Pt], on: f64, off: f64) -> Vec<(Pt, Pt)> {
    let mut out = Vec::new();
    if !(on.is_finite() && off.is_finite() && on > 0.0 && off > 0.0) {
        return out;
    }
    let period = on + off;
    // How far along the dash pattern the current segment starts.
    let mut phase = 0.0_f64;
    for w in points.windows(2) {
        let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
        let len = a.dist(*b);
        if !len.is_finite() || len <= 0.0 {
            continue;
        }
        let at = |t: f64| Pt::new(a.x + (b.x - a.x) * t / len, a.y + (b.y - a.y) * t / len);
        // Distance along this segment of the next dash start.
        let mut t = if phase < on { -phase } else { period - phase };
        while t < len {
            let (s, e) = (t.max(0.0), (t + on).min(len));
            if e > s {
                out.push((at(s), at(e)));
                if out.len() >= MAX_DASHES {
                    return out;
                }
            }
            t += period;
        }
        phase = (phase + len) % period;
    }
    out
}

/// Most two-colour dashes one page draws (all its wires together). A hostile file could ask for
/// tens of millions; beyond this the remaining two-colour wires are drawn in their first colour
/// only, and Checks lists the page ([`dash_budget`], `checks::Kind::DashesLeftOut`).
pub const MAX_PAGE_DASHES: usize = 20_000;

/// An upper bound of the dashes [`dash_segments`] makes along `points` (`on` + `off` apart): per
/// segment its whole periods and two more. Saturates; a length that is not finite counts as
/// `usize::MAX`.
pub fn dash_count(points: &[Pt], on: f64, off: f64) -> usize {
    let period = on + off;
    if !(period.is_finite() && period > 0.0) {
        return 0;
    }
    let mut n: usize = 0;
    for w in points.windows(2) {
        let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
        let k = a.dist(*b) / period;
        if !k.is_finite() || k >= (MAX_PAGE_DASHES as f64) * 2.0 {
            return usize::MAX;
        }
        n = n.saturating_add(k as usize).saturating_add(2);
    }
    n
}

/// Which two-colour wires on `page` get their dashes, within [`MAX_PAGE_DASHES`] (and
/// [`MAX_DASHES`] per wire), in the page's route order; the rest are drawn in their first colour
/// only. The same answer for the projection and for Checks: it reads the document only.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DashBudget {
    pub dashed: std::collections::BTreeSet<ecad_model::RepresentationId>,
    /// The conductors whose dashes are left out, in route order.
    pub left_out: Vec<ConductorId>,
}

/// The [`DashBudget`] of `page`.
pub fn dash_budget(doc: &Document, page: PageId) -> DashBudget {
    let (on, off) = (f64::from(ecad_tokens::wire::DASH_ON_MM), f64::from(ecad_tokens::wire::DASH_OFF_MM));
    let mut out = DashBudget::default();
    let mut total: usize = 0;
    let looks = crate::styles::wire::WireLooks::new(doc);
    for (rep, r) in doc.representations_on(page) {
        let Subject::Conductor(c) = r.subject else { continue };
        if r.form != ecad_model::Form::Drawn {
            continue;
        }
        // Only two-colour codes have dashes.
        let Some(code) = conductor_colour_in(doc, &looks, c) else { continue };
        if !colour_code(&code).is_some_and(|(_, b)| b.is_some()) {
            continue;
        }
        let Some(ItemKind::Primitive(prim)) = r.items.first().and_then(|i| doc.items.get(i)).map(|i| &i.kind) else { continue };
        let Some(points) = crate::wires::path_vertices(prim) else { continue };
        let n = dash_count(&points, on, off);
        if n > MAX_DASHES || total.saturating_add(n) > MAX_PAGE_DASHES {
            if !out.left_out.contains(&c) {
                out.left_out.push(c);
            }
            continue;
        }
        total += n;
        out.dashed.insert(rep);
    }
    out
}

/// A marker ECAD paints over the page in a shape CADCraft's marks lack (brief 040, step 2):
/// display only, never an entity, so it never reaches a plot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Painted {
    /// An error (an orphan cable reference): a filled disc with "!".
    Error(Pt),
    /// A warning (an open end that looks connected): a filled triangle with "!".
    Warning(Pt),
}

/// The painted markers of page drawing `uid`.
pub fn painted_marks(p: &crate::Project, uid: u64) -> Vec<Painted> {
    let Some(page) = p.page_of(uid) else { return Vec::new() };
    let Some(view) = p.pages.get(&page).filter(|v| !v.unavailable) else { return Vec::new() };
    let doc = p.ledger.document();
    let mut out: Vec<Painted> =
        crate::references::markers_on_page(doc, page).into_iter().filter(|m| m.orphan).map(|m| Painted::Error(m.at)).collect();
    out.extend(view.map.looks_connected().iter().map(|at| Painted::Warning(*at)));
    out
}

/// The handles of the provisional device tags drawn on `page` (`-K?1`): painted in the
/// provisional colour with a dotted underline (spec §5, "dotted means not final yet").
pub fn provisional_tag_handles(p: &crate::Project, page: PageId) -> Vec<cadcraft_engine::doc::Handle> {
    let doc = p.ledger.document();
    let Some(map) = p.pages.get(&page).map(|v| &v.map) else { return Vec::new() };
    map.tag_text_handles()
        .filter(|h| {
            let Some((rep, crate::devices::TextOf::Tag)) = map.tag_text_of(*h) else { return false };
            let Some(Subject::Component(c)) = doc.representations.get(&rep).map(|r| r.subject) else { return false };
            crate::devices::tag_of(doc, c).is_some_and(ecad_model::tag::is_provisional)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortening_counts_characters_and_hides_controls() {
        assert_eq!(shorten("W1", 24), "W1");
        assert_eq!(shorten(&"x".repeat(25), 24), format!("{}…", "x".repeat(23)));
        assert_eq!(shorten("a\nb", 24), "a?b");
    }

    #[test]
    fn clipping_removes_the_middle_of_a_segment() {
        let pieces = clip_outside(&[Pt::new(0.0, 0.0), Pt::new(10.0, 0.0)], Pt::new(4.0, -1.0), Pt::new(6.0, 1.0), 1e-9);
        assert_eq!(pieces, vec![vec![Pt::new(0.0, 0.0), Pt::new(4.0, 0.0)], vec![Pt::new(6.0, 0.0), Pt::new(10.0, 0.0)]]);
    }

    #[test]
    fn leaving_snaps_to_the_nearest_axis() {
        let p = [Pt::new(0.0, 0.0), Pt::new(3.0, 3.0)];
        assert_eq!(leaving(p.iter().copied(), 1e-9), Some(Pt::new(1.0, 0.0)));
        let p = [Pt::new(0.0, 0.0), Pt::new(1.0, -3.0)];
        assert_eq!(leaving(p.iter().copied(), 1e-9), Some(Pt::new(0.0, -1.0)));
    }
}
