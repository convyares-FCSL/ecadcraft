//! Small geometry value types owned by the model (no CADCraft types), and geometry
//! over primitives: the nearest point on one, their extents, and moving or scaling one.

use crate::Primitive;
use crate::geometry::units::Radians;
use serde::{Deserialize, Serialize};

/// A point on a page, in drawing units.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

impl Pt {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Distance to `o`.
    pub fn dist(self, o: Pt) -> f64 {
        (self.x - o.x).hypot(self.y - o.y)
    }

    /// Whether `o` is within `tol` of this point.
    pub fn near(self, o: Pt, tol: f64) -> bool {
        self.dist(o) <= tol
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// Where a symbol instance sits on its page.
///
/// # Semantics
/// `page = origin + R(rotation) · M(mirror) · scale · local`, where `M` negates x when mirrored.
/// `rotation` is in radians, counter-clockwise; `scale` is uniform and positive.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub origin: Pt,
    pub rotation: Radians,
    pub mirror: bool,
    pub scale: f64,
}

impl Default for Placement {
    fn default() -> Self {
        Self { origin: Pt::default(), rotation: Radians(0.0), mirror: false, scale: 1.0 }
    }
}

impl Placement {
    /// Map a point in the symbol's local frame onto the page.
    pub fn apply(&self, local: Pt) -> Pt {
        let x = if self.mirror { -local.x } else { local.x } * self.scale;
        let y = local.y * self.scale;
        let (s, c) = self.rotation.sin_cos();
        Pt::new(self.origin.x + c * x - s * y, self.origin.y + s * x + c * y)
    }

    /// Whether every value is usable (finite, positive scale).
    pub fn is_valid(&self) -> bool {
        self.origin.is_finite() && self.rotation.is_finite() && self.scale.is_finite() && self.scale > 0.0
    }
}

/// The point on the segment `a`–`b` nearest to `p` (`a` when the segment has no length).
pub fn nearest_on_segment(p: Pt, a: Pt, b: Pt) -> Pt {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    if !(len2.is_finite() && len2 > 0.0) {
        return a;
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0);
    Pt::new(a.x + t * dx, a.y + t * dy)
}

/// The point on `prim` nearest to `p` (its outline), or `None` for kinds without one.
pub fn nearest_on(prim: &Primitive, p: Pt) -> Option<Pt> {
    let seg = |a: Pt, b: Pt| nearest_on_segment(p, a, b);
    let best = |cands: Vec<Pt>| cands.into_iter().filter(|q| q.is_finite()).min_by(|a, b| a.dist(p).total_cmp(&b.dist(p)));
    match prim {
        Primitive::Line { a, b } => best(vec![seg(*a, *b)]),
        Primitive::Polyline { vertices, closed } => {
            let mut cands: Vec<Pt> = vertices.windows(2).filter_map(|w| Some(seg(*w.first()?, *w.get(1)?))).collect();
            if *closed && let (Some(a), Some(b)) = (vertices.last(), vertices.first()) {
                cands.push(seg(*a, *b));
            }
            if vertices.len() == 1 {
                cands.extend(vertices.first().copied());
            }
            best(cands)
        }
        Primitive::Circle { center, radius } => on_circle(*center, *radius, p),
        Primitive::Arc { center, radius, start, end } => {
            let on = on_circle(*center, *radius, p)?;
            let angle = (on.y - center.y).atan2(on.x - center.x);
            let tau = std::f64::consts::TAU;
            let sweep = (end - start).rem_euclid(tau);
            if (angle - start).rem_euclid(tau) <= sweep {
                return Some(on);
            }
            let at = |a: f64| Pt::new(center.x + radius * a.cos(), center.y + radius * a.sin());
            best(vec![at(*start), at(*end)])
        }
        Primitive::Text { .. } | Primitive::Point { .. } => None,
    }
}

fn on_circle(center: Pt, radius: f64, p: Pt) -> Option<Pt> {
    let d = p.dist(center);
    if !(d.is_finite() && d > 0.0 && radius.is_finite()) {
        return None;
    }
    Some(Pt::new(center.x + (p.x - center.x) * radius / d, center.y + (p.y - center.y) * radius / d))
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

/// `p` scaled by `s` about `from` and moved to `to`, in drawing units `k`.
pub fn place(p: &Primitive, from: Pt, s: f64, to: Pt, k: f64) -> Primitive {
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

/// A primitive moved by a placement (the shapes the coil and its kind use).
pub fn placed(prim: &Primitive, pl: &Placement) -> Option<Primitive> {
    Some(match prim {
        Primitive::Line { a, b } => Primitive::Line { a: pl.apply(*a), b: pl.apply(*b) },
        Primitive::Polyline { vertices, closed } => {
            Primitive::Polyline { vertices: vertices.iter().map(|v| pl.apply(*v)).collect(), closed: *closed }
        }
        Primitive::Circle { center, radius } => Primitive::Circle { center: pl.apply(*center), radius: *radius },
        Primitive::Point { at } => Primitive::Point { at: pl.apply(*at) },
        _ => return None,
    })
}
