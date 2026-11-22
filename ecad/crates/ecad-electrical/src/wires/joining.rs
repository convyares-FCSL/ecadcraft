//! Joining wires (brief 032 §2, design 072 §4.3).

use super::*;

/// The way out every join refusal names (design 032 §2.2).
pub(crate) const JOIN_WAY_OUT: &str = "Draw it with join off (join: false), or end it on a connection point.";

/// What one end of a new route meets (design 032 §2.1, design 072 §4.3).
#[derive(Debug, Clone, PartialEq)]
enum Meets {
    /// Nothing there: a deliberate open end.
    Free,
    /// A connection point there.
    Point(ConnectionPointId),
    /// The one free end of another route: the new line extends that route.
    Merge { route: RouteId, end: End },
    /// The middle of route `route` (a T), or its free end where the free ends of several routes
    /// meet; the free ends of `others` there land on `route` too.
    On { route: RouteId, others: Vec<(RouteId, End)> },
}

impl Meets {
    /// The other routes this end changes or lands on.
    fn routes(&self) -> Vec<RouteId> {
        match self {
            Meets::Free | Meets::Point(_) => Vec::new(),
            Meets::Merge { route, .. } => vec![*route],
            Meets::On { route, others } => std::iter::once(*route).chain(others.iter().map(|(r, _)| *r)).collect(),
        }
    }

    /// A merge that cannot be made lands on the route instead.
    fn not_merged(self) -> Meets {
        match self {
            Meets::Merge { route, .. } => Meets::On { route, others: Vec::new() },
            m => m,
        }
    }
}

/// A planned `wire` (design 032 §2.3, design 072 §4.3): its ops and what the result reports.
#[derive(Debug, Clone)]
pub struct WirePlan {
    pub ops: Vec<Op>,
    /// The route the drawn line now belongs to: a new one, or the one it extends.
    pub route: RouteId,
    /// That route's item.
    pub item: ItemId,
    /// False when the line was merged into an existing route.
    pub created: bool,
    /// The ends that met other routes: (end, the route met, `merged` or `on`).
    pub joined: Vec<(End, RouteId, &'static str)>,
    /// Routes merged into the survivor and removed.
    pub removed: Vec<RouteId>,
    /// Ends on a connection point.
    pub attached: Vec<(End, ConnectionPointId)>,
    /// Reverse T: (point, the route whose free end now lands on the drawn line there).
    pub tees: Vec<(Pt, RouteId)>,
    /// The wire made between the two connection points to carry the tool's potential.
    pub wire: Option<WireId>,
}

/// Whether `a`-`b`-`c` runs straight on through `b` (one line, same direction).
fn straight(a: Pt, b: Pt, c: Pt) -> bool {
    let (u, v) = (Pt::new(b.x - a.x, b.y - a.y), Pt::new(c.x - b.x, c.y - b.y));
    let (lu, lv) = (u.x.hypot(u.y), v.x.hypot(v.y));
    let cross = u.x * v.y - u.y * v.x;
    let dot = u.x * v.x + u.y * v.y;
    lu > 0.0 && lv > 0.0 && cross.abs() <= 1e-9 * lu * lv && dot > 0.0
}

/// `a` (ending at a point P) followed by `b` (starting at P): P once, dropped when the two
/// segments meeting there are one straight line (design 032 §2.2).
fn joined(mut a: Vec<Pt>, b: &[Pt]) -> Vec<Pt> {
    let Some((p, rest)) = b.split_first() else { return a };
    if a.is_empty() {
        a.push(*p);
    }
    let at = a.len().saturating_sub(1);
    a.extend_from_slice(rest);
    if let (Some(prev), Some(mid), Some(next)) = (at.checked_sub(1).and_then(|i| a.get(i)), a.get(at), a.get(at + 1))
        && straight(*prev, *mid, *next)
    {
        a.remove(at);
    }
    a
}

fn reversed(v: &[Pt]) -> Vec<Pt> {
    v.iter().rev().copied().collect()
}

/// The points of a route (`cv`, free end `open`) with `add` (starting at that end) joined on, so
/// the route keeps its direction.
fn merged_route(cv: &[Pt], open: End, add: &[Pt]) -> Vec<Pt> {
    match open {
        End::B => joined(cv.to_vec(), add),
        End::A => joined(reversed(add), cv),
    }
}

/// Whether a route end at `at` is quiet (a cable reference end, brief 009): never joined.
fn is_quiet(quiet: &[Pt], at: Pt) -> bool {
    quiet.iter().any(|q| q.near(at, ON_WIRE))
}

/// The route on `page` whose interior (not an end) passes within [`ON_WIRE`] of `at`, the nearest.
pub(crate) fn interior_route_at(doc: &Document, page: PageId, at: Pt) -> Option<OnRoute> {
    route_at(doc, page, at, ON_WIRE, None)
        .filter(|on| doc.routes.get(&on.route).and_then(Route::end_points).is_some_and(|[f, l]| !f.near(on.at, ON_WIRE) && !l.near(on.at, ON_WIRE)))
}

/// The net of the wires route `r` carries.
fn route_net_key(doc: &Document, r: RouteId) -> Option<ecad_model::NetKey> {
    wire_of_route(doc, r).and_then(|w| doc.connectivity().net_of_conductor(w))
}

/// The net of the wires route `r` carries.
fn route_net(doc: &Document, r: RouteId) -> Option<&ecad_model::NetData> {
    route_net_key(doc, r).and_then(|n| doc.connectivity().net(n))
}

/// Refuse joining route `r` with a new line of potential `potential` (design 032 §2.2).
fn merge_potential(doc: &Document, r: RouteId, potential: Option<&str>) -> Result<(), CmdError> {
    let Some(net) = route_net(doc, r) else { return Ok(()) };
    let theirs = ecad_model::connectivity::wiring::net_potential(doc, net).name;
    if let (Some(x), Some(y)) = (theirs.as_deref(), potential)
        && !x.eq_ignore_ascii_case(y)
    {
        return Err(CmdError::refused(format!("That wire carries {x} and this one {y}; joining them would short them. {JOIN_WAY_OUT}")));
    }
    Ok(())
}

/// Refuse joining routes `a` and `b` into one (design 032 §2.2): their nets carry different
/// potentials, or both have fixed numbers that differ.
pub(crate) fn join_sense(doc: &Document, a: RouteId, b: RouteId) -> Result<(), CmdError> {
    let (Some(ka), Some(kb)) = (route_net_key(doc, a), route_net_key(doc, b)) else { return Ok(()) };
    let conn = doc.connectivity();
    let (Some(x), Some(y)) = (conn.net(ka), conn.net(kb)) else { return Ok(()) };
    if ka == kb {
        return Ok(());
    }
    let (px, py) = (ecad_model::connectivity::wiring::net_potential(doc, x).name, ecad_model::connectivity::wiring::net_potential(doc, y).name);
    if let (Some(px), Some(py)) = (&px, &py)
        && !px.eq_ignore_ascii_case(py)
    {
        return Err(CmdError::refused(format!("That wire carries {px} and this one {py}; joining them would short them. {JOIN_WAY_OUT}")));
    }
    if ecad_model::connectivity::wiring::net_fixed(x) && ecad_model::connectivity::wiring::net_fixed(y) {
        let (fx, fy) = (
            ecad_model::connectivity::wiring::net_number(doc, x).unwrap_or_default(),
            ecad_model::connectivity::wiring::net_number(doc, y).unwrap_or_default(),
        );
        if fx != fy {
            return Err(CmdError::refused(format!("Both wires have fixed numbers of their own ({fx} and {fy}). {JOIN_WAY_OUT}")));
        }
    }
    Ok(())
}

/// The point at end `e` of route `r`.
pub(crate) fn end_point(doc: &Document, r: RouteId, e: End) -> Option<Pt> {
    let v = &doc.routes.get(&r)?.points;
    match e {
        End::A => v.first().copied(),
        End::B => v.last().copied(),
    }
}

/// The index of end `e` in a route's `ends`.
pub(crate) fn end_index(e: End) -> usize {
    match e {
        End::A => 0,
        End::B => 1,
    }
}

/// The free route ends at `at` on `page` that a new line may join (not quiet ones).
pub(crate) fn joinable_ends_at(doc: &Document, page: PageId, at: Pt, quiet: &[Pt]) -> Vec<(RouteId, End)> {
    loose_ends_at(doc, page, at, ON_WIRE).into_iter().filter(|(r, e)| end_point(doc, *r, *e).is_some_and(|p| !is_quiet(quiet, p))).collect()
}

/// What the end of a new line at `at` meets (design 032 §2.1, step 2), and the point it snaps to.
fn meet_at(doc: &Document, page: PageId, at: Pt, quiet: &[Pt]) -> (Meets, Pt) {
    let loose = joinable_ends_at(doc, page, at, quiet);
    if let Some(on) = interior_route_at(doc, page, at) {
        let others = loose.into_iter().filter(|(r, _)| *r != on.route).collect();
        return (Meets::On { route: on.route, others }, on.at);
    }
    match loose.as_slice() {
        [] => (Meets::Free, at),
        [(r, e)] => (Meets::Merge { route: *r, end: *e }, end_point(doc, *r, *e).unwrap_or(at)),
        [(r, e), rest @ ..] => {
            let others = rest.iter().filter(|(x, _)| x != r).copied().collect();
            (Meets::On { route: *r, others }, end_point(doc, *r, *e).unwrap_or(at))
        }
    }
}

/// The free ends of other routes on `page` that lie on the interior of path `v` (design 032 §2.1
/// step 3), each as (segment index, distance along it, point, route, end), in order along the
/// path.
fn ends_on_path(doc: &Document, page: PageId, v: &[Pt], quiet: &[Pt]) -> Vec<(usize, f64, Pt, RouteId, End)> {
    let (Some(first), Some(last)) = (v.first().copied(), v.last().copied()) else { return Vec::new() };
    // A grid of the new line's segments (overseer ruling: no segment-by-segment test of every end
    // on the page): an end is tested only against the segments in its cell.
    let mut grid: std::collections::HashMap<(i64, i64), Vec<usize>> = std::collections::HashMap::new();
    let mut long: Vec<usize> = Vec::new();
    for (i, w) in v.windows(2).enumerate() {
        let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
        let (Some(lo), Some(hi)) = (
            looks_cell(Pt::new(a.x.min(b.x) - ON_WIRE, a.y.min(b.y) - ON_WIRE)),
            looks_cell(Pt::new(a.x.max(b.x) + ON_WIRE, a.y.max(b.y) + ON_WIRE)),
        ) else {
            continue;
        };
        let n = (hi.0.saturating_sub(lo.0).saturating_add(1)).saturating_mul(hi.1.saturating_sub(lo.1).saturating_add(1));
        if usize::try_from(n).map_or(true, |n| n > LOOKS_MAX_CELLS) {
            long.push(i);
            continue;
        }
        for x in lo.0..=hi.0 {
            for y in lo.1..=hi.1 {
                grid.entry((x, y)).or_default().push(i);
            }
        }
    }
    let mut out: Vec<(usize, f64, Pt, RouteId, End)> = Vec::new();
    for r in doc.routes_on(page) {
        let Some(route) = doc.routes.get(&r) else { continue };
        for (end, e) in End::BOTH.into_iter().zip(route.ends) {
            let Some(q) = end_point(doc, r, end) else { continue };
            if e != RouteEnd::Free || !q.is_finite() || q.near(first, ON_WIRE) || q.near(last, ON_WIRE) || is_quiet(quiet, q) {
                continue;
            }
            let Some(cell) = looks_cell(q) else { continue };
            let mut segs: Vec<usize> = grid.get(&cell).into_iter().flatten().chain(long.iter()).copied().collect();
            segs.sort_unstable();
            segs.dedup();
            let hit = segs.into_iter().find_map(|i| {
                let (s, e) = (*v.get(i)?, *v.get(i + 1)?);
                let (on, d) = on_segment(q, s, e);
                (d <= ON_WIRE && on.is_finite()).then(|| (i, s.dist(on)))
            });
            if let Some((i, along)) = hit {
                out.push((i, along, q, r, end));
            }
        }
    }
    out.sort_by(|x, y| x.0.cmp(&y.0).then(x.1.total_cmp(&y.1)).then(x.3.cmp(&y.3)));
    out
}

/// The inputs of [`plan_wire`] besides the document and IDs.
#[derive(Debug, Clone)]
pub struct WireInput<'a> {
    pub page: PageId,
    pub style: Style,
    pub points: Vec<Pt>,
    pub potential: Option<&'a str>,
    /// Join ends that meet other routes (design 032 §2.1 steps 2 and 3).
    pub join: bool,
    /// The project tolerance (an end on a connection point).
    pub tol: f64,
    /// The page's quiet ends (cable reference ends): never joined.
    pub quiet: &'a [Pt],
    /// Most points a route may have, and most route ends one line may cross.
    pub max_points: usize,
}

/// Plan drawing a line (design 032 §2, design 072 §4.3): a route whose ends land on connection
/// points, and with `join` on other routes: an end on another route's free end extends that route
/// (two such ends make the two routes one), an end on a route's middle lands there (a T), and free
/// ends of other routes lying on the line land on it (the reverse T). The wires come from the
/// commit's rewrite pass; a line from one connection point straight to another also makes that
/// wire, to carry the tool's potential.
///
/// # Errors
/// Design 032 §2.2's refusals (potentials, fixed numbers, too many points or ends crossed);
/// nothing is planned then.
pub fn plan_wire(doc: &Document, ids: &mut IdAllocator, input: WireInput<'_>) -> Result<WirePlan, CmdError> {
    let WireInput { page, style, points: mut pts, potential, join, tol, quiet, max_points } = input;
    let (Some(first), Some(last)) = (pts.first().copied(), pts.last().copied()) else { return Err(CmdError::refused("a wire needs two points")) };
    let mut shown = ShownOnce::new(doc, page);
    // 1. The ends.
    let mut meets: Vec<Meets> = Vec::with_capacity(2);
    for (i, at) in [first, last].into_iter().enumerate() {
        let (m, p) = if let Some(t) = shown.connection_point_at(at, tol) {
            (Meets::Point(t), at)
        } else if join {
            meet_at(doc, page, at, quiet)
        } else {
            (Meets::Free, at)
        };
        let slot = if i == 0 { pts.first_mut() } else { pts.last_mut() };
        if let Some(s) = slot {
            *s = p;
        }
        meets.push(m);
    }
    let mut mb = meets.pop().unwrap_or(Meets::Free);
    let mut ma = meets.pop().unwrap_or(Meets::Free);
    // 2. The reverse T: free ends lying on the line's interior land on it.
    let tees: Vec<(Pt, RouteId, End)> = if join {
        ends_on_path(doc, page, &pts, quiet)
            .into_iter()
            .filter(|(_, _, q, _, _)| shown.connection_point_at(*q, tol).is_none())
            .map(|(_, _, q, r, e)| (q, r, e))
            .collect()
    } else {
        Vec::new()
    };
    if tees.len() > max_points {
        return Err(CmdError::refused(format!("This wire crosses more than {max_points} wire ends; draw it in parts.")));
    }
    // A route merged into the line must not also land on it, or be landed on by its other end:
    // that end lands on the route instead.
    let tee_routes: BTreeSet<RouteId> = tees.iter().map(|(_, r, _)| *r).collect();
    let clash = |m: &Meets, other: &Meets| match m {
        Meets::Merge { route, .. } => tee_routes.contains(route) || other.routes().contains(route),
        _ => false,
    };
    let (clash_a, clash_b) = (clash(&ma, &mb), clash(&mb, &ma));
    if clash_a {
        ma = ma.not_merged();
    }
    if clash_b {
        mb = mb.not_merged();
    }
    if let (Meets::Merge { route: a, .. }, Meets::Merge { route: b, .. }) = (&ma, &mb) {
        let lands = |x: RouteId, y: RouteId| doc.routes.get(&x).is_some_and(|r| r.ends.iter().any(|e| e.route() == Some(y)));
        if lands(*a, *b) || lands(*b, *a) {
            mb = mb.not_merged();
        }
    }
    // Joining at an open end or landing on a line's middle (a T) joins the nets alike.
    for m in [&ma, &mb] {
        if let Meets::Merge { route, .. } | Meets::On { route, .. } = m {
            merge_potential(doc, *route, potential)?;
        }
    }
    if let (Meets::Merge { route: a, .. }, Meets::Merge { route: b, .. }) = (&ma, &mb) {
        join_sense(doc, *a, *b)?;
    }
    let (pa, pb) = (pts.first().copied().unwrap_or(first), pts.last().copied().unwrap_or(last));
    let end_of = |m: &Meets, at: Pt| match m {
        Meets::Point(t) => RouteEnd::Point(*t),
        Meets::On { route, .. } => RouteEnd::On { route: *route, at },
        Meets::Free | Meets::Merge { .. } => RouteEnd::Free,
    };
    let (ea, eb) = (end_of(&ma, pa), end_of(&mb, pb));
    let too_long = || CmdError::refused(format!("The joined wire would have more than {max_points} points."));
    let mut ops: Vec<Op> = Vec::new();
    let mut tail: Vec<Op> = Vec::new();
    let mut edits = RouteEdits::new(doc);
    let mut removed = Vec::new();
    let mut joined_ends = Vec::new();
    // 3. The route that holds the line.
    let (holder, item, created) = match (&ma, &mb) {
        (Meets::Merge { route: r1, end: e1 }, Meets::Merge { route: r2, end: e2 }) => {
            // Both ends meet free ends of two routes: one route (§2.2), the one at end A.
            let o = doc.routes.get(r2).ok_or(CmdError::NotFound(Noun::Wire))?;
            let o_from_end = if *e2 == End::A { o.points.clone() } else { reversed(&o.points) };
            let far = o.ends.get(end_index(e2.other())).copied().unwrap_or(RouteEnd::Free);
            let s = edits.get(*r1).ok_or(CmdError::NotFound(Noun::Wire))?;
            let points = merged_route(&s.points, *e1, &joined(pts.clone(), &o_from_end));
            if points.len() > max_points {
                return Err(too_long());
            }
            s.points = points;
            if let Some(x) = s.ends.get_mut(end_index(*e1)) {
                *x = far;
            }
            edits.retarget_tees(*r2, *r1);
            tail = remove_route_ops(doc, *r2);
            edits.forget(*r2);
            removed.push(*r2);
            joined_ends.push((End::A, *r1, "merged"));
            joined_ends.push((End::B, *r2, "merged"));
            let item = route_item(doc, *r1).map(|(_, i, _)| i).ok_or(CmdError::refused("The wire has no drawing on this page."))?;
            (*r1, item, false)
        }
        (Meets::Merge { route, end }, _) | (_, Meets::Merge { route, end }) => {
            let at_a = matches!(ma, Meets::Merge { .. });
            let (add, far) = if at_a { (pts.clone(), eb) } else { (reversed(&pts), ea) };
            let s = edits.get(*route).ok_or(CmdError::NotFound(Noun::Wire))?;
            let points = merged_route(&s.points, *end, &add);
            if points.len() > max_points {
                return Err(too_long());
            }
            s.points = points;
            if let Some(x) = s.ends.get_mut(end_index(*end)) {
                *x = far;
            }
            joined_ends.push((if at_a { End::A } else { End::B }, *route, "merged"));
            let item = route_item(doc, *route).map(|(_, i, _)| i).ok_or(CmdError::refused("The wire has no drawing on this page."))?;
            (*route, item, false)
        }
        _ => {
            if pts.len() > max_points {
                return Err(CmdError::refused(format!("A wire has at most {max_points} points.")));
            }
            let (r, i, add) = add_route(ids, page, style, pts.clone(), [ea, eb]);
            ops.extend(add);
            (r, i, true)
        }
    };
    // 4. What lands where.
    let mut attached = Vec::new();
    for (end, m, at) in [(End::A, &ma, pa), (End::B, &mb, pb)] {
        match m {
            Meets::Point(t) => attached.push((end, *t)),
            Meets::On { route, others } => {
                joined_ends.push((end, *route, "on"));
                for (r, e) in others {
                    edits.set_end(*r, *e, RouteEnd::On { route: *route, at });
                }
            }
            Meets::Free | Meets::Merge { .. } => {}
        }
    }
    let mut landed = Vec::new();
    for (q, r, e) in &tees {
        edits.set_end(*r, *e, RouteEnd::On { route: holder, at: *q });
        landed.push((*q, *r));
    }
    // 5. The tool's potential, on the wire between two connection points.
    let mut wire = None;
    if created
        && let (Some(v), Meets::Point(a), Meets::Point(b)) = (potential, &ma, &mb)
        && a != b
        && wire_between(doc, *a, *b).is_none()
    {
        let (id, net, conductor): (WireId, NetId, ConductorId) = (ids.fresh(), ids.fresh(), ids.fresh());
        let mut w = Wire::new(net, conductor, *a, *b);
        w.potential = ecad_model::naming::text::Text::new(v).ok();
        ops.push(Op::CreateWire { id, wire: w });
        wire = Some(id);
    }
    ops.extend(edits.ops());
    ops.extend(tail);
    Ok(WirePlan { ops, route: holder, item, created, joined: joined_ends, removed, attached, tees: landed, wire })
}

/// A planned merge of two free route ends that meet (`joinwires`, design 032 §2.5).
#[derive(Debug, Clone)]
pub struct EndJoin {
    pub ops: Vec<Op>,
    pub survivor: RouteId,
    pub removed: RouteId,
}

/// Plan merging the free ends `a` and `b` of two routes that meet into one route (design 032
/// §2.5): the lower route ID survives and takes the other's points and far end; routes landing on
/// the other land on it; the other is removed. `Ok(None)` when one of them lands on the other (the
/// caller lands one end on the other route instead).
///
/// # Errors
/// One route's two ends; a route too long; §2.2's refusals.
pub fn plan_end_join(doc: &Document, a: (RouteId, End), b: (RouteId, End), max_points: usize) -> Result<Option<EndJoin>, CmdError> {
    if a.0 == b.0 {
        return Err(CmdError::refused(format!("Both ends belong to one wire: joining them would close it into a loop. {JOIN_WAY_OUT}")));
    }
    let lands = |x: RouteId, y: RouteId| doc.routes.get(&x).is_some_and(|r| r.ends.iter().any(|e| e.route() == Some(y)));
    if lands(a.0, b.0) || lands(b.0, a.0) {
        return Ok(None);
    }
    join_sense(doc, a.0, b.0)?;
    let ((s, es), (o, eo)) = if a.0 < b.0 { (a, b) } else { (b, a) };
    let ro = doc.routes.get(&o).ok_or(CmdError::NotFound(Noun::Wire))?;
    // The other route's free end lies on the survivor's (within ON_WIRE): it starts there.
    let mut add = if eo == End::A { ro.points.clone() } else { reversed(&ro.points) };
    if let (Some(first), Some(at)) = (add.first_mut(), end_point(doc, s, es)) {
        *first = at;
    }
    let far = ro.ends.get(end_index(eo.other())).copied().unwrap_or(RouteEnd::Free);
    let mut edits = RouteEdits::new(doc);
    let rs = edits.get(s).ok_or(CmdError::NotFound(Noun::Wire))?;
    let points = merged_route(&rs.points, es, &add);
    if points.len() > max_points {
        return Err(CmdError::refused(format!("The joined wire would have more than {max_points} points.")));
    }
    rs.points = points;
    if let Some(x) = rs.ends.get_mut(end_index(es)) {
        *x = far;
    }
    edits.retarget_tees(o, s);
    edits.forget(o);
    let mut ops = edits.ops();
    ops.extend(remove_route_ops(doc, o));
    Ok(Some(EndJoin { ops, survivor: s, removed: o }))
}

/// The nearest free route end on `page` within `tol` of `p` that a new line may join (not a quiet
/// one): the Wire tool snaps a click there (design 032 §2.4).
pub fn open_end_near(doc: &Document, page: PageId, p: Pt, tol: f64, quiet: &[Pt]) -> Option<Pt> {
    let mut best: Option<(f64, Pt)> = None;
    for r in doc.routes_on(page) {
        let Some(route) = doc.routes.get(&r) else { continue };
        for (end, e) in End::BOTH.into_iter().zip(route.ends) {
            let Some(q) = end_point(doc, r, end) else { continue };
            let d = q.dist(p);
            if e == RouteEnd::Free && d <= tol && !is_quiet(quiet, q) && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, q));
            }
        }
    }
    best.map(|(_, q)| q)
}

/// What the Wire tool shows at an end point `at` (design 032 §2.4): the point, the word (`join`,
/// `tee`, or the first words of a refusal such as `0V ≠ 24V`) and whether it is refused; `None`
/// when the end would stay open.
pub fn join_preview(doc: &Document, page: PageId, at: Pt, quiet: &[Pt], potential: Option<&str>) -> Option<(Pt, String, bool)> {
    let (m, p) = meet_at(doc, page, at, quiet);
    match m {
        Meets::Free | Meets::Point(_) => None,
        Meets::Merge { route, .. } => {
            let theirs = route_net(doc, route).and_then(|n| ecad_model::connectivity::wiring::net_potential(doc, n).name);
            match (theirs, potential) {
                (Some(x), Some(y)) if !x.eq_ignore_ascii_case(y) => Some((p, format!("{x} \u{2260} {y}"), true)),
                _ => Some((p, "join".into(), false)),
            }
        }
        Meets::On { .. } => Some((p, "tee".into(), false)),
    }
}

/// The cell size of [`looks_connected_on_page`]'s grid, millimetres.
const LOOKS_CELL_MM: f64 = 10.0;
/// Most cells one segment enters in that grid (a hostile huge segment costs no more than a page);
/// a longer segment is checked against every end instead.
const LOOKS_MAX_CELLS: usize = 4_096;

fn looks_cell(p: Pt) -> Option<(i64, i64)> {
    p.is_finite().then(|| ((p.x / LOOKS_CELL_MM).floor() as i64, (p.y / LOOKS_CELL_MM).floor() as i64))
}

/// The free route ends on `page` that look connected (design 032 §2.6): lying on another route's
/// end or interior, not quiet (a cable reference end). One point per place, in route order.
///
/// # Semantics
/// Computed during the page's projection from that page's routes only (the page index), with a
/// grid of their segments, so the cost follows the page, never the project (update map §8.2).
pub fn looks_connected_on_page(doc: &Document, page: PageId, quiet: &[Pt]) -> Vec<Pt> {
    let routes: Vec<&Route> = doc.routes_on(page).filter_map(|r| doc.routes.get(&r)).collect();
    let mut grid: std::collections::HashMap<(i64, i64), Vec<(usize, usize)>> = std::collections::HashMap::new();
    let mut long: Vec<(usize, usize)> = Vec::new();
    for (r, route) in routes.iter().enumerate() {
        for (s, w) in route.points.windows(2).enumerate() {
            let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
            let (Some(lo), Some(hi)) = (
                looks_cell(Pt::new(a.x.min(b.x) - ON_WIRE, a.y.min(b.y) - ON_WIRE)),
                looks_cell(Pt::new(a.x.max(b.x) + ON_WIRE, a.y.max(b.y) + ON_WIRE)),
            ) else {
                continue;
            };
            let n = (hi.0.saturating_sub(lo.0).saturating_add(1)).saturating_mul(hi.1.saturating_sub(lo.1).saturating_add(1));
            if usize::try_from(n).map_or(true, |n| n > LOOKS_MAX_CELLS) {
                long.push((r, s));
                continue;
            }
            for x in lo.0..=hi.0 {
                for y in lo.1..=hi.1 {
                    grid.entry((x, y)).or_default().push((r, s));
                }
            }
        }
    }
    let mut out: Vec<Pt> = Vec::new();
    for (r, route) in routes.iter().enumerate() {
        for (at, e) in [route.points.first(), route.points.last()].into_iter().zip(route.ends) {
            let Some(at) = at.copied() else { continue };
            if e != RouteEnd::Free || is_quiet(quiet, at) || out.iter().any(|q| q.near(at, ON_WIRE)) {
                continue;
            }
            let Some(cell) = looks_cell(at) else { continue };
            let near = grid.get(&cell).into_iter().flatten().chain(long.iter()).any(|(o, s)| {
                if *o == r {
                    return false;
                }
                let Some(ov) = routes.get(*o).map(|x| &x.points) else { return false };
                let (Some(a), Some(b)) = (ov.get(*s), ov.get(s + 1)) else { return false };
                let (q, d) = on_segment(at, *a, *b);
                d <= ON_WIRE && q.is_finite()
            });
            if near {
                out.push(at);
            }
        }
    }
    out
}
