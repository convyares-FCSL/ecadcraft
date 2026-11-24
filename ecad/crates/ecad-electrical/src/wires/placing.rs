//! Placing a wire on a route: tagless runs, hits and splits.

use super::*;

/// A planned continuation (a drawing aid).
#[derive(Debug, Clone)]
pub struct Tagless {
    pub ops: Vec<Op>,
    pub aid: AidId,
    pub connection_point: ConnectionPointId,
    pub item: ItemId,
    pub representation: RepresentationId,
    pub definition: DefinitionId,
    /// Free route ends found at the point, now on it.
    pub attached: Vec<(RouteId, End)>,
}

/// Plan aid `aid` drawn with starter `s` at `placement` on `page` (a continuation: its symbol,
/// its one connection point); free route ends lying on its connection point land on it.
///
/// # Errors
/// The starter definition cannot be built (never expected).
pub fn plan_tagless(
    doc: &Document, ids: &mut IdAllocator, page: PageId, style: Style, s: &Starter, placement: Placement, aid: Aid,
) -> Result<Tagless, CmdError> {
    let mut ops = Vec::new();
    let definition = match symbols::catalogue::embedded(doc, s) {
        Some(d) => d,
        None => {
            let d: DefinitionId = ids.fresh();
            ops.push(Op::EmbedDefinition { id: d, definition: s.definition()?.clone() });
            d
        }
    };
    let (id, connection_point, item, representation): (AidId, ConnectionPointId, ItemId, RepresentationId) =
        (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
    ops.push(Op::CreateAid { id, aid });
    ops.push(Op::AddConnectionPoint {
        id: connection_point,
        connection_point: ConnectionPoint { owner: id.into(), label: None, key: Some(ecad_model::PointKey::def("1")), accepts: Default::default() },
    });
    ops.push(Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Symbol(SymbolInstance { definition, placement }) } });
    ops.push(Op::LinkRepresentation {
        id: representation,
        rep: Representation {
            subject: Subject::Aid(id),
            page,
            form: Form::Symbol,
            role: ecad_model::Role::Part(s.part_role()),
            items: vec![item],
            connection_points: BTreeMap::new(),
        },
    });
    let attached = loose_ends_at(doc, page, placement.origin, ON_WIRE);
    let mut edits = RouteEdits::new(doc);
    for (r, end) in &attached {
        edits.set_end(*r, *end, RouteEnd::Point(connection_point));
    }
    ops.extend(edits.ops());
    Ok(Tagless { ops, aid: id, connection_point, item, representation, definition, attached })
}

/// The nearest point to `p` on segment `a`-`b`, and its distance.
pub(super) fn on_segment(p: Pt, a: Pt, b: Pt) -> (Pt, f64) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let q = Pt::new(a.x + t * dx, a.y + t * dy);
    (q, q.dist(p))
}

/// A route found under a point: the route, the segment index and the point on it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OnRoute {
    pub route: RouteId,
    pub segment: usize,
    pub at: Pt,
}

/// Where a terminal placed at `at` lands on a route, or `None` when `at` is on no route (or on a
/// route's free end): it is then placed on its own.
pub fn landing_point(doc: &Document, page: PageId, at: Pt) -> Option<OnRoute> {
    let on = route_at(doc, page, at, ON_WIRE, None)?;
    let route = doc.routes().get(&on.route)?;
    let [first, last] = route.end_points()?;
    for (p, e) in [first, last].into_iter().zip(route.ends) {
        if p.near(on.at, ON_WIRE) && e == RouteEnd::Free {
            return None;
        }
    }
    Some(on)
}

/// The route on `page` passing within `tol` of `p` (the nearest; never `except`), with the point
/// snapped to a route vertex within `tol`. Reads the page's routes through the index.
pub fn route_at(doc: &Document, page: PageId, p: Pt, tol: f64, except: Option<RouteId>) -> Option<OnRoute> {
    let mut best: Option<(f64, OnRoute)> = None;
    for r in doc.routes_on(page) {
        if Some(r) == except {
            continue;
        }
        let Some(v) = doc.routes().get(&r).map(|x| &x.points) else { continue };
        let (first, last) = (v.first().copied(), v.last().copied());
        for (i, w) in v.windows(2).enumerate() {
            let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
            let (q, d) = on_segment(p, *a, *b);
            // Hostile geometry (an overflowing segment) gives NaN: never a route there.
            if !(d <= tol && q.is_finite()) {
                continue;
            }
            // A route's interior wins over another route's end at the same point.
            let at_end = first.is_some_and(|f| f.near(q, tol)) || last.is_some_and(|l| l.near(q, tol));
            let d = if at_end { d + tol } else { d };
            if best.is_none_or(|(bd, _)| d < bd) {
                let at = v.iter().copied().find(|x| x.near(q, tol)).unwrap_or(q);
                best = Some((d, OnRoute { route: r, segment: i, at }));
            }
        }
    }
    best.map(|(_, w)| w)
}

/// The route `r` met at `at`: its segment there, when `at` lies within [`ON_WIRE`] of it.
pub fn on_route_of(doc: &Document, r: RouteId, at: Pt) -> Option<OnRoute> {
    let v = &doc.routes().get(&r)?.points;
    let mut best: Option<(f64, usize)> = None;
    for (i, w) in v.windows(2).enumerate() {
        let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
        let (_, d) = on_segment(at, *a, *b);
        if d.is_finite() && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, i));
        }
    }
    let (d, segment) = best?;
    (d <= ON_WIRE).then_some(OnRoute { route: r, segment, at })
}

/// A route split in two at a point.
#[derive(Debug, Clone)]
pub struct Split {
    pub ops: Vec<Op>,
    /// The route, which keeps its first end and its points up to the split.
    pub route: RouteId,
    /// The new second part, from the split to the route's old last end.
    pub new: RouteId,
    /// The second part's item.
    pub new_item: ItemId,
}

/// Plan splitting route `on.route` at `on.at`: the route keeps
/// its first end and ends on `first` (an end and the point it is drawn to); a new route starts on
/// `second` and keeps the old last end. Routes landing on the second part land on the new route.
///
/// # Errors
/// No such route, or the point at one of its ends.
pub(crate) fn plan_split(
    doc: &Document, ids: &mut IdAllocator, on: &OnRoute, first: (RouteEnd, Pt), second: (RouteEnd, Pt),
) -> Result<Split, CmdError> {
    let r = on.route;
    let route = doc.routes().get(&r).ok_or(CmdError::NotFound(Noun::Wire))?;
    let v = &route.points;
    let [a, b] = route.end_points().ok_or(CmdError::refused("empty route"))?;
    if on.at.near(a, ON_WIRE) || on.at.near(b, ON_WIRE) {
        return Err(CmdError::refused("That is the end of the wire. Click along the wire, away from its ends, or on empty space."));
    }
    let seg = on.segment;
    let mut keep: Vec<Pt> = v.get(..=seg).unwrap_or_default().to_vec();
    let mut rest: Vec<Pt> = v.get(seg + 1..).unwrap_or_default().to_vec();
    if keep.last().is_some_and(|p| p.near(on.at, 1e-9)) {
        keep.pop();
    }
    if rest.first().is_some_and(|p| p.near(on.at, 1e-9)) {
        rest.remove(0);
    }
    keep.push(first.1);
    rest.insert(0, second.1);
    let style = route_item(doc, r).map(|(_, _, i)| i.style.clone()).ok_or(CmdError::refused("The wire has no drawing on this page."))?;
    let (new, new_item, mut ops) = add_route(ids, route.page, style, rest, [second.0, route.ends[1]]);
    let split_at = route.along(on.at).map_or(0.0, |(s, _)| s);
    let mut edits = RouteEdits::new(doc);
    // The routes landing on the second part land on the new route.
    for t in doc.tees_of(r).collect::<Vec<_>>() {
        if let Some(x) = edits.get(t) {
            for e in x.ends.iter_mut() {
                if let RouteEnd::On { route: h, at } = *e
                    && h == r
                    && route.along(at).is_some_and(|(s, _)| s > split_at)
                {
                    *e = RouteEnd::On { route: new, at };
                }
            }
        }
    }
    if let Some(x) = edits.get(r) {
        x.points = keep;
        x.ends[1] = first.0;
    }
    ops.extend(edits.ops());
    Ok(Split { ops, route: r, new, new_item })
}
