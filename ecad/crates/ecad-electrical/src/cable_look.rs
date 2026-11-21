//! The cable view's geometry over Core types (brief 009, design 005 and 009): vector helpers, a
//! route's open path, the axis it leaves along, tidying and clipping paths, and a cable's two sides
//! on a page. The engine's `look` module lays the fan out with them (brief 074 moved them here).
//!
//! # Semantics
//! - Pure functions over [`Pt`] and model values: nothing is drawn, read from a session or stored.
//! - Hostile values never panic: a point that is not finite gives `None` or an empty answer.
//!
//! This module excludes text measuring, drawing and reading the document (the engine's `look`).

use ecad_model::{CorePosition, Primitive, Pt, RouteId};

// ---------------------------------------------------------------- vectors

pub fn add(a: Pt, b: Pt) -> Pt {
    Pt::new(a.x + b.x, a.y + b.y)
}
pub fn sub(a: Pt, b: Pt) -> Pt {
    Pt::new(a.x - b.x, a.y - b.y)
}
pub fn mul(a: Pt, s: f64) -> Pt {
    Pt::new(a.x * s, a.y * s)
}
pub fn dot(a: Pt, b: Pt) -> f64 {
    a.x * b.x + a.y * b.y
}
pub fn cross(a: Pt, b: Pt) -> f64 {
    a.x * b.y - a.y * b.x
}
pub fn rot90(a: Pt) -> Pt {
    Pt::new(-a.y, a.x)
}
pub fn angle(a: Pt) -> f64 {
    a.y.atan2(a.x)
}
pub fn centroid(points: &[Pt]) -> Option<Pt> {
    if points.is_empty() {
        return None;
    }
    let n = points.len() as f64;
    let s = points.iter().fold(Pt::new(0.0, 0.0), |acc, p| add(acc, *p));
    let c = mul(s, 1.0 / n);
    c.is_finite().then_some(c)
}
/// `o + a·u + t·v`.
pub fn at(o: Pt, u: Pt, v: Pt, a: f64, t: f64) -> Pt {
    add(o, add(mul(u, a), mul(v, t)))
}

/// The open path of a route.
pub fn path_of(prim: &Primitive) -> Option<Vec<Pt>> {
    match prim {
        Primitive::Line { a, b } => Some(vec![*a, *b]),
        Primitive::Polyline { vertices, closed: false } if vertices.len() >= 2 => Some(vertices.clone()),
        _ => None,
    }
}

/// The direction in which `path` leaves its first point, snapped to the nearest axis (a tie goes
/// to the x axis); `None` when the path has no length.
pub fn leaving(path: impl Iterator<Item = Pt>, tol: f64) -> Option<Pt> {
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
pub fn tidy(points: Vec<Pt>, tol: f64) -> Vec<Pt> {
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
pub fn clip_outside(path: &[Pt], lo: Pt, hi: Pt, tol: f64) -> Vec<Vec<Pt>> {
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
pub fn inside_interval(a: Pt, b: Pt, lo: Pt, hi: Pt) -> Option<(f64, f64)> {
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

/// One member with a route on the page.
pub struct Member {
    /// Where the core sits: its wire, or its line while that has no wire yet.
    pub position: CorePosition,
    /// The line drawing it on the page.
    pub route: Option<RouteId>,
    pub core: Option<String>,
    pub path: Vec<Pt>,
}

/// The cable's two sides on the page (design 005 §5 steps 1 and 2): the reference member's index
/// and, per member in order, its (r₀-side, r₁-side) ends.
pub fn sides(members: &[Member]) -> Option<(usize, Vec<(Pt, Pt)>)> {
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

/// A core position as an index key: a loose line by its route alone (where its free end landed
/// does not change which line it is).
pub fn key(p: CorePosition) -> CorePosition {
    match p {
        CorePosition::Route { route, .. } => CorePosition::Route { route, landed: None },
        CorePosition::Wire(_) | CorePosition::Unplaced => p,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
