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

use ecad_model::{
    CableId, ConductorId, Document, End, ItemId, ItemKind, PageId, Primitive, Pt, RepresentationId, Subject,
};

// ---------------------------------------------------------------- tokens (design §12)

/// `ecad.label-height`: core labels, wire numbers, label box lines, reference text.
pub const LABEL_HEIGHT: f64 = 2.5;
/// `ecad.label-gap`: between the bend and the first label, between labels, after the last.
pub const LABEL_GAP: f64 = 1.0;
/// `ecad.tail-min`: the shortest tail in fan view.
pub const TAIL_MIN: f64 = 20.0;
/// `ecad.bend-radius`, capped at half the core pitch.
pub const BEND_RADIUS: f64 = 2.5;
/// `ecad.spine-lead`: the spine beyond the first core, before the trunk joins it.
pub const SPINE_LEAD: f64 = 5.0;
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
/// The representation role of a cable's reference end (brief 010's model).
pub const REFERENCE_ROLE: &str = "reference";
/// The text a reference marker shows until brief 010 makes it a live reference.
pub const REFERENCE_PLACEHOLDER: &str = "->";

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

/// The text a reference end shows (design §7.4). Brief 010 makes it the live reference; here it is
/// a placeholder for every reference end.
pub fn reference_text(_doc: &Document, _reference: RepresentationId) -> String {
    REFERENCE_PLACEHOLDER.to_string()
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
}

impl Part {
    fn new(role: Role, shape: Shape, weight: Weight) -> Part {
        Part { role, shape, weight, core: None, reference: None }
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
        .filter(|p| {
            p.len() >= 2 && p.windows(2).any(|w| w.first().zip(w.get(1)).is_some_and(|(a, b)| !a.near(*b, tol)))
        })
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
}

impl Marker {
    /// The flag's rectangle (design §7.2): `(lo, hi)`.
    fn rect(&self) -> (Pt, Pt) {
        let h = LABEL_HEIGHT;
        (
            Pt::new(self.at.x - 0.5 * h, self.at.y - 0.5 * h),
            Pt::new(self.at.x + self.width + 0.5 * h, self.at.y + 1.5 * h),
        )
    }
    fn centre(&self) -> Pt {
        let (lo, hi) = self.rect();
        mul(add(lo, hi), 0.5)
    }
    /// The flag (flat side towards `toward`) and its attach point M.
    fn flag(&self, toward: Option<Pt>) -> (Vec<Pt>, Pt) {
        let (lo, hi) = self.rect();
        let ym = 0.5 * (lo.y + hi.y);
        let flat_left = toward.is_none_or(|q| self.centre().x > q.x);
        if flat_left {
            let tip = Pt::new(hi.x + LABEL_HEIGHT, ym);
            (vec![lo, Pt::new(hi.x, lo.y), tip, hi, Pt::new(lo.x, hi.y)], Pt::new(lo.x, ym))
        } else {
            let tip = Pt::new(lo.x - LABEL_HEIGHT, ym);
            (vec![lo, Pt::new(hi.x, lo.y), hi, Pt::new(lo.x, hi.y), tip], Pt::new(hi.x, ym))
        }
    }
}

/// The reference ends of `cable` on `page`, in ID order: each with the point of its first Point
/// (or Text) item. Ends whose point is not finite are left out.
fn markers_of(doc: &Document, page: PageId, cable: CableId, measure: &dyn Fn(&str) -> f64) -> Vec<Marker> {
    doc.representations
        .iter()
        .filter(|(_, r)| r.page == page && r.subject == Subject::Cable(cable) && r.role == REFERENCE_ROLE)
        .filter_map(|(id, r)| {
            let at = r.items.iter().find_map(|i| match doc.items.get(i).map(|x| &x.kind) {
                Some(ItemKind::Primitive(Primitive::Point { at } | Primitive::Text { at, .. })) => Some(*at),
                _ => None,
            })?;
            let text = shorten(&reference_text(doc, *id), 64);
            let width = measure(&text);
            (at.is_finite() && width.is_finite()).then_some(Marker { id: *id, at, text, width })
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

/// A fanned end: its junction J, the trunk's outward direction σv, and its parts.
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

/// One fanned end (design §4.2). `ends`: each member's end on this side; `q`: the other side's
/// centroid or attach point.
#[allow(clippy::too_many_arguments)]
fn fan_end(
    doc: &Document,
    members: &[Member],
    reference: usize,
    side: usize,
    ends: &[Pt],
    q: Pt,
    tol: f64,
    measure: &dyn Fn(&str) -> f64,
) -> Option<FannedEnd> {
    let g = LABEL_GAP;
    let h = LABEL_HEIGHT;
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
    let pitch = sorted
        .windows(2)
        .filter_map(|w| w.first().zip(w.get(1)).map(|(x, y)| y - x))
        .filter(|d| *d > tol)
        .fold(f64::INFINITY, f64::min);
    let rho = BEND_RADIUS.min(pitch / 2.0);
    // Labels and tail length.
    let labels: Vec<Labels> = members
        .iter()
        .map(|m| {
            let core = m.core.as_deref().filter(|c| !c.trim().is_empty()).map(|c| shorten(c, MAX_CORE_CHARS));
            let wire = doc
                .conductors
                .get(&m.conductor)
                .and_then(|c| c.properties.get("tag"))
                .filter(|t| !t.is_empty())
                .map(|t| shorten(t, MAX_SHOWN_CHARS));
            let core_w = core.as_deref().map_or(0.0, measure);
            let wire_w = wire.as_deref().map_or(0.0, measure);
            Labels { core, wire, core_w, wire_w }
        })
        .collect();
    let wc = labels.iter().map(|l| l.core_w).fold(0.0, f64::max);
    let ww = labels.iter().map(|l| l.wire_w).fold(0.0, f64::max);
    let tail = TAIL_MIN.max(g + wc + g + ww + g);
    let amax = a.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let c = amax + tail + rho;
    let (tmin, tmax) = (sorted.first().copied()?, sorted.last().copied()?);
    // Which spine end the trunk joins.
    let tq = dot(sub(q, o), v);
    let (dhi, dlo) = ((tq - tmax).abs(), (tq - tmin).abs());
    let hi_end = if (dhi - dlo).abs() <= tol {
        let (phi, plo) = (at(o, u, v, c, tmax), at(o, u, v, c, tmin));
        if (phi.y - plo.y).abs() > tol { phi.y > plo.y } else { phi.x <= plo.x }
    } else {
        dhi < dlo
    };
    let sigma = if hi_end { 1.0 } else { -1.0 };
    let (tnear, tfar) = if hi_end { (tmax, tmin) } else { (tmin, tmax) };
    let junction = at(o, u, v, c, tnear + sigma * (rho + SPINE_LEAD));
    let spine_end = at(o, u, v, c, tfar + sigma * rho);
    if !(junction.is_finite() && spine_end.is_finite() && c.is_finite()) {
        return None;
    }
    let mut parts =
        vec![Part::new(Role::Spine, Shape::Path { points: vec![junction, spine_end], closed: false }, Weight::Cable)];
    // Cores in order of distance from the junction end; equal t in member order.
    let mut order: Vec<usize> = (0..members.len()).collect();
    order.sort_by(|x, y| {
        let (tx, ty) = (t.get(*x).copied().unwrap_or(0.0), t.get(*y).copied().unwrap_or(0.0));
        (sigma * ty).total_cmp(&(sigma * tx)).then(x.cmp(y))
    });
    let r = mul(u, -1.0);
    for i in order {
        let (Some(m), Some(e), Some(ti), Some(l)) = (members.get(i), ends.get(i), t.get(i), labels.get(i)) else {
            continue;
        };
        let start = at(o, u, v, c - rho, *ti);
        if rho > tol {
            let centre = at(o, u, v, c - rho, *ti + sigma * rho);
            let (a1, a2) = (angle(u), angle(mul(v, -sigma)));
            let (s0, s1) = if cross(u, mul(v, -sigma)) > 0.0 { (a1, a2) } else { (a2, a1) };
            parts.push(
                Part::new(Role::Bend, Shape::Arc { center: centre, radius: rho, start: s0, end: s1 }, Weight::Wire)
                    .of_core(m.conductor),
            );
        }
        if !start.near(*e, tol) {
            parts.push(
                Part::new(Role::Tail, Shape::Path { points: vec![start, *e], closed: false }, Weight::Wire)
                    .of_core(m.conductor),
            );
        }
        // Labels at the tail start, reading left to right or bottom to top (design §4.5).
        let second = if l.core.is_some() { g + l.core_w + g } else { g };
        let lift = 0.3 * h;
        let (core_at, wire_at, rotation) = if r.x > 0.5 {
            (add(start, Pt::new(g, lift)), add(start, Pt::new(second, lift)), 0.0)
        } else if r.x < -0.5 {
            (add(start, Pt::new(-g - l.core_w, lift)), add(start, Pt::new(-second - l.wire_w, lift)), 0.0)
        } else if r.y > 0.5 {
            (add(start, Pt::new(-lift, g)), add(start, Pt::new(-lift, second)), std::f64::consts::FRAC_PI_2)
        } else {
            (
                add(start, Pt::new(-lift, -g - l.core_w)),
                add(start, Pt::new(-lift, -second - l.wire_w)),
                std::f64::consts::FRAC_PI_2,
            )
        };
        if let Some(text) = &l.core {
            parts.push(
                Part::new(Role::CoreLabel, Shape::Text { at: core_at, rotation, value: text.clone() }, Weight::Text)
                    .of_core(m.conductor),
            );
        }
        if let Some(text) = &l.wire {
            parts.push(
                Part::new(Role::WireNumber, Shape::Text { at: wire_at, rotation, value: text.clone() }, Weight::Text)
                    .of_core(m.conductor),
            );
        }
    }
    if parts.iter().any(|p| !shape_finite(&p.shape)) {
        return None;
    }
    Some(FannedEnd { junction, outward: mul(v, sigma), parts })
}

fn shape_finite(s: &Shape) -> bool {
    match s {
        Shape::Path { points, .. } => points.iter().all(|p| p.is_finite()),
        Shape::Arc { center, radius, start, end } => {
            center.is_finite() && radius.is_finite() && start.is_finite() && end.is_finite()
        }
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
        if dot(sub(corner, j0), d0) > tol && dot(sub(corner, j1), d1) > tol {
            vec![j0, corner, j1]
        } else {
            vec![j0, j1]
        }
    }
}

/// The trunk from a fanned end to a reference end's attach point `m`.
fn trunk_to_marker(a: &FannedEnd, m: Pt, tol: f64) -> Vec<Pt> {
    let w = dot(sub(m, a.junction), a.outward);
    if w > tol { vec![a.junction, add(a.junction, mul(a.outward, w)), m] } else { vec![a.junction, m] }
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
    parts.push(Part::new(
        Role::LabelBox,
        Shape::Path { points: vec![lo, Pt::new(hi.x, lo.y), hi, Pt::new(lo.x, hi.y)], closed: true },
        Weight::Wire,
    ));
    for (i, (line, w)) in lines.into_iter().zip(widths).enumerate() {
        let baseline = hi.y - BOX_PAD - h - i as f64 * BOX_LINE_PITCH;
        parts.push(Part::new(
            Role::BoxText,
            Shape::Text { at: Pt::new(centre.x - w / 2.0, baseline), rotation: 0.0, value: line },
            Weight::Text,
        ));
    }
    parts
}

/// Core labels just past the slash crossings (design §6).
fn slash_labels(doc: &Document, page: PageId, cable: CableId, tol: f64) -> Vec<Part> {
    let Some(c) = doc.cables.get(&cable) else { return Vec::new() };
    let core_of: BTreeMap<ConductorId, String> = c
        .members
        .iter()
        .filter_map(|m| {
            m.core.as_deref().filter(|x| !x.trim().is_empty()).map(|x| (m.conductor, shorten(x, MAX_CORE_CHARS)))
        })
        .collect();
    if core_of.is_empty() {
        return Vec::new();
    }
    let h = LABEL_HEIGHT;
    let mut out = Vec::new();
    for (w, p, dir) in crate::slash::member_crossings_of(doc, page, cable, tol) {
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
        out.push(
            Part::new(Role::CoreAtSlash, Shape::Text { at, rotation: angle(d), value: text.clone() }, Weight::Text)
                .of_core(w),
        );
    }
    out
}

/// `cable` on `page`: its fan when `fan` is asked for and can be drawn, else its traditional
/// computed parts (core labels at the slashes), with its reference ends in both views.
pub fn cable_on_page(
    doc: &Document,
    page: PageId,
    cable: CableId,
    fan: bool,
    tol: f64,
    measure: &dyn Fn(&str) -> f64,
) -> CableDrawing {
    let tol = if tol.is_finite() && tol >= 0.0 { tol } else { 0.0 };
    let Some(c) = doc.cables.get(&cable) else { return CableDrawing::default() };
    let mut seen = BTreeSet::new();
    let mut members: Vec<Member> = Vec::new();
    for m in &c.members {
        if !seen.insert(m.conductor) {
            continue;
        }
        if let Some(path) = ecad_model::derive::route_on_page(doc, m.conductor, page).and_then(|(_, p)| path_of(&p)) {
            members.push(Member { conductor: m.conductor, core: m.core.clone(), path });
        }
    }
    let markers = markers_of(doc, page, cable, measure);
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
    // Each marker's flag and attach point: the flat side faces its side's centroid.
    let all_ends: Vec<Pt> = side_ends.iter().flatten().copied().collect();
    let flags: Vec<(Vec<Pt>, Pt)> = markers
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let toward = match stopped.iter().position(|s| *s == Some(i)) {
                Some(s) => cents.get(s).copied().flatten(),
                None => centroid(&all_ends),
            };
            m.flag(toward)
        })
        .collect();
    let mut out = CableDrawing { markers: ends_of, ..CableDrawing::default() };
    let mut fanned: Option<(Vec<Part>, Vec<ItemId>, [Option<Pt>; 2])> = None;
    if fan {
        out.state = if members.len() < 2 { FanState::TooFew } else { FanState::Failed };
        if let Some((reference, _)) = &sides
            && members.len() >= 2
        {
            fanned =
                fan_parts(doc, page, cable, &members, *reference, &side_ends, &cents, &stopped, &flags, tol, measure);
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
        None => out.parts = slash_labels(doc, page, cable, tol),
    }
    for (m, (flag, _)) in markers.iter().zip(&flags) {
        out.parts.push(
            Part::new(Role::Flag, Shape::Path { points: flag.clone(), closed: true }, Weight::Wire).of_reference(m.id),
        );
        out.parts.push(
            Part::new(
                Role::ReferenceText,
                Shape::Text { at: m.at, rotation: 0.0, value: m.text.clone() },
                Weight::Text,
            )
            .of_reference(m.id),
        );
    }
    out
}

/// The fan's parts and hidden items, or `None` when it cannot be drawn.
#[allow(clippy::too_many_arguments)]
fn fan_parts(
    doc: &Document,
    page: PageId,
    cable: CableId,
    members: &[Member],
    reference: usize,
    side_ends: &[Vec<Pt>; 2],
    cents: &[Option<Pt>; 2],
    stopped: &[Option<usize>; 2],
    flags: &[(Vec<Pt>, Pt)],
    tol: f64,
    measure: &dyn Fn(&str) -> f64,
) -> Option<(Vec<Part>, Vec<ItemId>, [Option<Pt>; 2])> {
    let attach = |s: usize| stopped.get(s).copied().flatten().and_then(|i| flags.get(i)).map(|(_, m)| *m);
    let m = [attach(0), attach(1)];
    let mut fanned: [Option<FannedEnd>; 2] = [None, None];
    for s in 0..2 {
        if m.get(s).copied().flatten().is_some() {
            continue;
        }
        let other = 1 - s;
        let q = m.get(other).copied().flatten().or(cents.get(other).copied().flatten())?;
        let ends = side_ends.get(s)?;
        let f = fan_end(doc, members, reference, s, ends, q, tol, measure)?;
        if let Some(slot) = fanned.get_mut(s) {
            *slot = Some(f);
        }
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
    let used: BTreeSet<ConductorId> = members.iter().map(|m| m.conductor).collect();
    let mut hidden = Vec::new();
    for r in doc.representations.values().filter(|r| r.page == page) {
        let hide = match r.subject {
            Subject::Conductor(w) => used.contains(&w),
            Subject::Cable(x) => x == cable && r.role != REFERENCE_ROLE,
            Subject::Component(_) => false,
        };
        if hide {
            hidden.extend(r.items.iter().copied());
        }
    }
    Some((parts, hidden, m))
}

/// Every cable's computed drawing on `page` (cables in `fan` drawn as fans where they can be),
/// leaving out cables with nothing computed there.
pub fn cables_on_page(
    doc: &Document,
    page: PageId,
    fan: &BTreeSet<CableId>,
    tol: f64,
    measure: &dyn Fn(&str) -> f64,
) -> BTreeMap<CableId, CableDrawing> {
    doc.cables
        .keys()
        .map(|k| (*k, cable_on_page(doc, page, *k, fan.contains(k), tol, measure)))
        .filter(|(_, d)| !d.parts.is_empty() || !d.hidden.is_empty())
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
        let pieces =
            clip_outside(&[Pt::new(0.0, 0.0), Pt::new(10.0, 0.0)], Pt::new(4.0, -1.0), Pt::new(6.0, 1.0), 1e-9);
        assert_eq!(
            pieces,
            vec![vec![Pt::new(0.0, 0.0), Pt::new(4.0, 0.0)], vec![Pt::new(6.0, 0.0), Pt::new(10.0, 0.0)]]
        );
    }

    #[test]
    fn leaving_snaps_to_the_nearest_axis() {
        let p = [Pt::new(0.0, 0.0), Pt::new(3.0, 3.0)];
        assert_eq!(leaving(p.iter().copied(), 1e-9), Some(Pt::new(1.0, 0.0)));
        let p = [Pt::new(0.0, 0.0), Pt::new(1.0, -3.0)];
        assert_eq!(leaving(p.iter().copied(), 1e-9), Some(Pt::new(0.0, -1.0)));
    }
}
