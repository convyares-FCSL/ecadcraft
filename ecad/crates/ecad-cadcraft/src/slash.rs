//! What a cable slash crosses (design 004 §4), and the words used about cables (§5.3, §5.4).
//!
//! One function, [`crossings`], decides which conductors a straight slash takes. The JSON form of
//! `cableslash`, its interactive preview and the crossing dots drawn over the page all use it, so
//! what is previewed is what is committed.
//!
//! # Semantics
//! Only conductor routes on the given page are tested: every segment of every Line or Polyline
//! item of every conductor representation there (a closed polyline includes its closing segment).
//! All tests use the project's tolerance.
//! - C1 to C3: the slash crosses the inside of a route segment, passes through a bend, or ends on
//!   the inside of a route: the conductor **counts**.
//! - C4: the slash meets a route only at the route's own end point: **touched**, not counted.
//! - C5: the slash runs along a route segment (collinear overlap): **touched**, not counted.
//! - C6: a conductor crossed more than once counts once, at its first crossing.
//! - C7: a Line or Polyline item that is not a conductor route and meets the slash is
//!   **ignored** and reported. Arcs, circles, text, points and symbols are not tested (C8).
//!
//! A conductor with any C1 to C3 crossing counts, whatever else it touches. Counted conductors
//! are ordered by the distance from the slash's first point to their first crossing; equal
//! distances (within tolerance) by conductor ID.
//!
//! This module stores nothing and has no notion of cores or numbering: the order is the member
//! order and nothing else.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::{CableId, ConductorId, Document, ItemId, ItemKind, PageId, Primitive, Pt, Subject};

/// Most wires one cable may hold (input-derived sizes are capped, never trusted).
pub const MAX_CABLE_MEMBERS: usize = 10_000;

/// A conductor the slash takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crossing {
    pub conductor: ConductorId,
    /// Its first crossing point on the slash.
    pub at: Pt,
    /// The distance from the slash's first point to `at`.
    pub along: f64,
}

/// What a slash crosses on one page.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Crossings {
    /// Counted conductors (C1 to C3), in crossing order.
    pub members: Vec<Crossing>,
    /// Lines and polylines that are not conductor routes (C7), in crossing order, with where.
    pub ignored: Vec<(ItemId, Pt)>,
    /// Conductors met only at a route end or along a segment (C4, C5), in ID order.
    pub touched: Vec<ConductorId>,
}

/// How a slash meets one segment.
enum Hit {
    /// They meet at one point, `along` the slash from its first point.
    Point { at: Pt, along: f64 },
    /// They overlap along a stretch (collinear).
    Along { along: f64 },
}

fn cross(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    ax * by - ay * bx
}

/// The distance from `p` to the infinite line through `a` with direction `(dx, dy)` of length
/// `len` (> 0).
fn line_dist(p: Pt, a: Pt, dx: f64, dy: f64, len: f64) -> f64 {
    (cross(dx, dy, p.x - a.x, p.y - a.y) / len).abs()
}

/// How slash `s0 → s1` meets segment `a → b`, within `tol`.
fn hit(s0: Pt, s1: Pt, a: Pt, b: Pt, tol: f64) -> Option<Hit> {
    let (dx, dy) = (s1.x - s0.x, s1.y - s0.y);
    let len_s = dx.hypot(dy);
    if !(len_s.is_finite() && len_s > 0.0) {
        return None;
    }
    let along_of = |p: Pt| ((p.x - s0.x) * dx + (p.y - s0.y) * dy) / len_s;
    let (ex, ey) = (b.x - a.x, b.y - a.y);
    let len_e = ex.hypot(ey);
    if !len_e.is_finite() {
        return None;
    }
    // A segment of no length is a point.
    if len_e <= tol {
        let t = along_of(a);
        return (line_dist(a, s0, dx, dy, len_s) <= tol && t >= -tol && t <= len_s + tol)
            .then(|| Hit::Point { at: a, along: t.clamp(0.0, len_s) });
    }
    // Collinear: both ends of the segment on the slash's line.
    if line_dist(a, s0, dx, dy, len_s) <= tol && line_dist(b, s0, dx, dy, len_s) <= tol {
        let (ta, tb) = (along_of(a), along_of(b));
        let (lo, hi) = (ta.min(tb), ta.max(tb));
        return (hi >= -tol && lo <= len_s + tol).then(|| Hit::Along { along: lo.clamp(0.0, len_s) });
    }
    let denom = cross(dx, dy, ex, ey);
    if !(denom.is_finite() && denom != 0.0) {
        return None;
    }
    let (wx, wy) = (a.x - s0.x, a.y - s0.y);
    let t = cross(wx, wy, ex, ey) / denom;
    let u = cross(wx, wy, dx, dy) / denom;
    let (ts, ue) = (t * len_s, u * len_e);
    if !(ts.is_finite() && ue.is_finite()) || ts < -tol || ts > len_s + tol || ue < -tol || ue > len_e + tol {
        return None;
    }
    let along = ts.clamp(0.0, len_s);
    let at = Pt::new(s0.x + dx * along / len_s, s0.y + dy * along / len_s);
    at.is_finite().then_some(Hit::Point { at, along })
}

/// The segments of a path primitive (Line or Polyline; a closed polyline includes its closing
/// segment), with the path's own end points (none for a closed polyline).
fn segments(prim: &Primitive) -> Option<(Vec<(Pt, Pt)>, Option<(Pt, Pt)>)> {
    match prim {
        Primitive::Line { a, b } => Some((vec![(*a, *b)], Some((*a, *b)))),
        Primitive::Polyline { vertices, closed } => {
            let mut segs: Vec<(Pt, Pt)> = vertices.windows(2).filter_map(|w| Some((*w.first()?, *w.get(1)?))).collect();
            if *closed
                && vertices.len() > 2
                && let (Some(a), Some(b)) = (vertices.last(), vertices.first())
            {
                segs.push((*a, *b));
            }
            if vertices.len() == 1
                && let Some(a) = vertices.first()
            {
                segs.push((*a, *a));
            }
            let ends = if *closed { None } else { prim.path_ends() };
            Some((segs, ends))
        }
        _ => None,
    }
}

fn usable(tol: f64) -> f64 {
    if tol.is_finite() && tol >= 0.0 { tol } else { 0.0 }
}

/// The conductor routes shown on `page`: route item → conductor.
fn routes(doc: &Document, page: PageId) -> BTreeMap<ItemId, ConductorId> {
    let mut out = BTreeMap::new();
    for r in doc.representations.values().filter(|r| r.page == page) {
        if let Subject::Conductor(c) = r.subject {
            out.extend(r.items.iter().map(|i| (*i, c)));
        }
    }
    out
}

/// How one slash meets one conductor's routes.
#[derive(Default)]
struct Meets {
    /// The first counted crossing (C1 to C3).
    first: Option<(f64, Pt)>,
    /// Met only at an end or along a segment (C4, C5).
    touched: bool,
}

fn meet_route(m: &mut Meets, prim: &Primitive, from: Pt, to: Pt, tol: f64) {
    let Some((segs, ends)) = segments(prim) else { return };
    for (a, b) in segs {
        match hit(from, to, a, b, tol) {
            Some(Hit::Point { at, along }) => {
                let at_end = ends.is_some_and(|(e0, e1)| at.near(e0, tol) || at.near(e1, tol));
                if at_end {
                    m.touched = true;
                } else if m.first.is_none_or(|(d, _)| along < d) {
                    m.first = Some((along, at));
                }
            }
            Some(Hit::Along { .. }) => m.touched = true,
            None => {}
        }
    }
}

/// Crossing order: by distance along the slash (a total order: `f64::total_cmp`, then wire ID),
/// then each run of crossings whose neighbours are within `tol` of each other is ordered by wire
/// ID. Both passes are total orders, so sorting never panics and the result is deterministic.
fn order(members: &mut [Crossing], tol: f64) {
    members.sort_by(|x, y| x.along.total_cmp(&y.along).then(x.conductor.cmp(&y.conductor)));
    let mut start = 0;
    while start < members.len() {
        let mut end = start + 1;
        while let (Some(prev), Some(next)) = (members.get(end - 1), members.get(end)) {
            if (next.along - prev.along).abs() > tol {
                break;
            }
            end += 1;
        }
        if let Some(run) = members.get_mut(start..end) {
            run.sort_by_key(|m| m.conductor);
        }
        start = end;
    }
}

/// What the slash `from → to` crosses on `page` (module rules). A slash of no length, or with
/// non-finite ends, crosses nothing.
pub fn crossings(doc: &Document, page: PageId, from: Pt, to: Pt, tol: f64) -> Crossings {
    let tol = usable(tol);
    let mut out = Crossings::default();
    if !(from.is_finite() && to.is_finite()) || from.near(to, tol) {
        return out;
    }
    let routes = routes(doc, page);
    let mut meets: BTreeMap<ConductorId, Meets> = BTreeMap::new();
    let mut ignored: Vec<(f64, ItemId, Pt)> = Vec::new();
    for (id, item) in doc.items.iter().filter(|(_, i)| i.page == page) {
        let ItemKind::Primitive(prim) = &item.kind else { continue };
        match routes.get(id) {
            Some(c) => meet_route(meets.entry(*c).or_default(), prim, from, to, tol),
            None => {
                let Some((segs, _)) = segments(prim) else { continue };
                let first = segs
                    .into_iter()
                    .filter_map(|(a, b)| match hit(from, to, a, b, tol)? {
                        Hit::Point { at, along } => Some((along, at)),
                        Hit::Along { along } => {
                            let len = from.dist(to);
                            let at =
                                Pt::new(from.x + (to.x - from.x) * along / len, from.y + (to.y - from.y) * along / len);
                            Some((along, at))
                        }
                    })
                    .min_by(|x, y| x.0.total_cmp(&y.0));
                if let Some((along, at)) = first {
                    ignored.push((along, *id, at));
                }
            }
        }
    }
    for (c, m) in meets {
        match m.first {
            Some((along, at)) => out.members.push(Crossing { conductor: c, at, along }),
            None if m.touched => out.touched.push(c),
            None => {}
        }
    }
    order(&mut out.members, tol);
    ignored.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
    out.ignored = ignored.into_iter().map(|(_, i, at)| (i, at)).collect();
    out
}

/// The crossed conductors that are already in a cable, each with the first such cable.
pub fn blocked(doc: &Document, members: &[Crossing]) -> Vec<(Crossing, CableId)> {
    let wanted: BTreeSet<ConductorId> = members.iter().map(|m| m.conductor).collect();
    let mut cable_of: BTreeMap<ConductorId, CableId> = BTreeMap::new();
    for (k, cable) in &doc.cables {
        for m in &cable.members {
            if wanted.contains(&m.conductor) {
                cable_of.entry(m.conductor).or_insert(*k);
            }
        }
    }
    members.iter().filter_map(|m| cable_of.get(&m.conductor).map(|k| (*m, *k))).collect()
}

/// The slash segments of cable `cable` on `page`: the Line items of its representations there.
pub fn slashes_of(doc: &Document, page: PageId, cable: CableId) -> Vec<(ItemId, Pt, Pt)> {
    doc.representations_of(Subject::Cable(cable))
        .filter(|(_, r)| r.page == page)
        .flat_map(|(_, r)| r.items.iter())
        .filter_map(|i| match doc.items.get(i).map(|x| &x.kind) {
            Some(ItemKind::Primitive(Primitive::Line { a, b })) => Some((*i, *a, *b)),
            _ => None,
        })
        .collect()
}

/// Where each slash of every cable on `page` crosses one of that cable's own members (design
/// §7.2): one point per member and slash, by the rules above, from the current geometry. A member
/// moved away from its slash gets none.
pub fn member_crossing_points(doc: &Document, page: PageId, tol: f64) -> Vec<Pt> {
    member_crossing_points_where(doc, page, tol, |_| true)
}

/// [`member_crossing_points`] for the slashes `shown` accepts only (by slash item): a slash that
/// is not drawn, because its cable is shown as a fan (brief 005), has no dots.
pub fn member_crossing_points_where(doc: &Document, page: PageId, tol: f64, shown: impl Fn(ItemId) -> bool) -> Vec<Pt> {
    let tol = usable(tol);
    let routes = routes(doc, page);
    let mut by_conductor: BTreeMap<ConductorId, Vec<&Primitive>> = BTreeMap::new();
    for (i, c) in &routes {
        if let Some(ItemKind::Primitive(prim)) = doc.items.get(i).map(|x| &x.kind) {
            by_conductor.entry(*c).or_default().push(prim);
        }
    }
    let mut out = Vec::new();
    for (k, cable) in &doc.cables {
        for (item, from, to) in slashes_of(doc, page, *k) {
            if !shown(item) || !(from.is_finite() && to.is_finite()) || from.near(to, tol) {
                continue;
            }
            for m in &cable.members {
                let mut meets = Meets::default();
                for prim in by_conductor.get(&m.conductor).into_iter().flatten() {
                    meet_route(&mut meets, prim, from, to, tol);
                }
                out.extend(meets.first.map(|(_, at)| at));
            }
        }
    }
    out
}

/// Where each slash of `cable` on `page` crosses one of its own members (the
/// [`member_crossing_points`] rules), with the member and the direction of its route segment there
/// (design 009 §6): one entry per member and slash, in slash order then member order.
pub fn member_crossings_of(doc: &Document, page: PageId, cable: CableId, tol: f64) -> Vec<(ConductorId, Pt, Pt)> {
    let Some(c) = doc.cables.get(&cable) else { return Vec::new() };
    let routes = routes(doc, page);
    let mut by_conductor: BTreeMap<ConductorId, Vec<&Primitive>> = BTreeMap::new();
    for (i, w) in &routes {
        if let Some(ItemKind::Primitive(prim)) = doc.items.get(i).map(|x| &x.kind) {
            by_conductor.entry(*w).or_default().push(prim);
        }
    }
    let slashes: Vec<(Pt, Pt)> = slashes_of(doc, page, cable).into_iter().map(|(_, a, b)| (a, b)).collect();
    member_crossings_with(c, &slashes, |w| by_conductor.get(&w).map(Vec::as_slice).unwrap_or_default(), tol)
}

/// [`member_crossings_of`] for one cable with its `slashes` on the page and each member's route
/// primitives there (`prims`), already gathered by the caller (one index per page, brief 009
/// review S4).
pub fn member_crossings_with<'p>(
    c: &ecad_model::Cable,
    slashes: &[(Pt, Pt)],
    prims: impl Fn(ConductorId) -> &'p [&'p Primitive],
    tol: f64,
) -> Vec<(ConductorId, Pt, Pt)> {
    let tol = usable(tol);
    let mut out = Vec::new();
    for (from, to) in slashes.iter().copied() {
        if !(from.is_finite() && to.is_finite()) || from.near(to, tol) {
            continue;
        }
        for m in &c.members {
            let prims = prims(m.conductor);
            let mut meets = Meets::default();
            for prim in prims {
                meet_route(&mut meets, prim, from, to, tol);
            }
            let Some((_, at)) = meets.first else { continue };
            // The segment the crossing lies on: the nearest one of the member's routes.
            let nearest = prims
                .iter()
                .filter_map(|p| segments(p))
                .flat_map(|(segs, _)| segs)
                .filter(|(a, b)| !a.near(*b, tol))
                .map(|(a, b)| (distance_to_segment(at, a, b), a, b))
                .min_by(|x, y| x.0.total_cmp(&y.0));
            if let Some((_, a, b)) = nearest {
                out.push((m.conductor, at, Pt::new(b.x - a.x, b.y - a.y)));
            }
        }
    }
    out
}

fn distance_to_segment(p: Pt, a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    p.dist(Pt::new(a.x + dx * t, a.y + dy * t))
}

// ---------------------------------------------------------------- words (§5.3, §5.4)

fn short(hex: &str) -> &str {
    hex.get(..8).unwrap_or(hex)
}

/// A wire's name: its `tag` property, else `wire <first 8 hex>…`.
pub fn wire_name(doc: &Document, c: ConductorId) -> String {
    match doc.conductors.get(&c).and_then(|x| x.properties.get("tag")) {
        Some(t) => t.clone(),
        None => format!("wire {}…", short(&c.hex())),
    }
}

/// A cable's name: its `tag` property, else `cable <first 8 hex>…`.
pub fn cable_name(doc: &Document, k: CableId) -> String {
    match doc.cables.get(&k).and_then(|x| x.properties.get("tag")) {
        Some(t) => t.clone(),
        None => format!("cable {}…", short(&k.hex())),
    }
}

/// Most names one message lists before `and N more`.
pub const MAX_NAMES: usize = 5;

/// `a, b, c` for at most [`MAX_NAMES`] names, then ` and N more`.
pub fn name_list(names: &[String]) -> String {
    let shown: Vec<&str> = names.iter().take(MAX_NAMES).map(String::as_str).collect();
    let mut out = shown.join(", ");
    if names.len() > MAX_NAMES {
        out.push_str(&format!(" and {} more", names.len() - MAX_NAMES));
    }
    out
}

/// `1 wire` / `3 wires`.
pub fn wires(n: usize) -> String {
    if n == 1 { "1 wire".into() } else { format!("{n} wires") }
}

/// Refusal: the slash crosses no wires (with the count of other objects it crosses).
pub fn no_wires(ignored: usize) -> String {
    let mut m = "The slash crosses no wires; nothing was created.".to_string();
    match ignored {
        0 => {}
        1 => m.push_str(" It crosses 1 object that is not a wire."),
        n => m.push_str(&format!(" It crosses {n} objects that are not wires.")),
    }
    m
}

/// Refusal: more wires than a cable may hold.
pub fn too_many() -> String {
    format!("The slash crosses more than {MAX_CABLE_MEMBERS} wires; nothing was created.")
}

/// A wire after the word "Wire": its tag, else `8a3f19c0…` (so it never reads "Wire wire …").
fn bare_wire_name(doc: &Document, c: ConductorId) -> String {
    match doc.conductors.get(&c).and_then(|x| x.properties.get("tag")) {
        Some(t) => t.clone(),
        None => format!("{}…", short(&c.hex())),
    }
}

/// Refusal: some wires are already in a cable. `tail` ends the message (`Nothing was created.`).
pub fn already_in_cable(doc: &Document, blocked: &[(ConductorId, CableId)], tail: &str) -> String {
    match blocked {
        [(w, k)] => format!(
            "Wire {} is already in {}; a wire can be in only one cable. {tail}",
            bare_wire_name(doc, *w),
            cable_label(doc, *k)
        ),
        _ => {
            let names: Vec<String> = blocked.iter().map(|(w, _)| bare_wire_name(doc, *w)).collect();
            format!("Wires {} are already in a cable; a wire can be in only one cable. {tail}", name_list(&names))
        }
    }
}

/// `cable C1` for a tagged cable, else `cable 8a3f19c0…`.
fn cable_label(doc: &Document, k: CableId) -> String {
    match doc.cables.get(&k).and_then(|x| x.properties.get("tag")) {
        Some(t) => format!("cable {t}"),
        None => cable_name(doc, k),
    }
}

/// The echo after a slash (§5.3): the first line, then one line each for ignored objects and
/// touched wires when there are any.
pub fn created_lines(
    doc: &Document,
    cable: CableId,
    members: &[ConductorId],
    ignored: usize,
    touched: usize,
) -> Vec<String> {
    let names: Vec<String> = members.iter().map(|c| wire_name(doc, *c)).collect();
    let mut out = vec![format!("Cable {} created with {}: {}.", cable.hex(), wires(members.len()), name_list(&names))];
    match ignored {
        0 => {}
        1 => out.push("1 object that is not a wire was ignored.".into()),
        n => out.push(format!("{n} objects that are not wires were ignored.")),
    }
    match touched {
        0 => {}
        1 => out.push("1 wire touched at its end or along its length was not included.".into()),
        n => out.push(format!("{n} wires touched at their ends or along their length were not included.")),
    }
    out
}
