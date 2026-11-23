//! Routes and the wires a route carries.

use super::*;

/// A route's drawing: its representation, its item's ID and the item.
pub fn route_item(doc: &Document, r: RouteId) -> Option<(RepresentationId, ItemId, &Item)> {
    doc.representations_of(Subject::Route(r)).find_map(|(rid, rep)| {
        let id = *rep.items.first()?;
        Some((rid, id, doc.items().get(&id)?))
    })
}

/// The T's on `page`: each route end that lands on another route
/// (`RouteEnd::On`), as (route, end index, point). Derived from the routes, never stored; a page
/// is drawn again when a route op touches it, so the dots follow AddRoute, ModifyRoute and
/// RemoveRoute of that page's routes and nothing else.
pub fn tees_on_page(doc: &Document, page: PageId) -> Vec<(RouteId, u8, Pt)> {
    let mut out = Vec::new();
    for r in doc.routes_on(page) {
        let Some(route) = doc.routes().get(&r) else { continue };
        for (i, end) in (0u8..).zip(route.ends) {
            if let RouteEnd::On { at, .. } = end
                && at.is_finite()
            {
                out.push((r, i, at));
            }
        }
    }
    out
}

/// Route `r`'s item, the item and the route's points.
pub fn route_vertices(doc: &Document, r: RouteId) -> Option<(ItemId, Item, Vec<Pt>)> {
    let route = doc.routes().get(&r)?;
    let (_, id, item) = route_item(doc, r)?;
    Some((id, item.clone(), route.points.clone()))
}

/// The polyline item drawing a route through `points` on `page`.
pub fn route_drawing(page: PageId, style: Style, points: Vec<Pt>) -> Item {
    Item { page, style, kind: ItemKind::Primitive(Primitive::Polyline { vertices: points, closed: false }) }
}

/// The ops that draw a new route on `page` through `points` with `ends`: the
/// route, its item and the representation linking them. Returns the route's and the item's IDs.
pub fn add_route(ids: &mut IdAllocator, page: PageId, style: Style, points: Vec<Pt>, ends: [RouteEnd; 2]) -> (RouteId, ItemId, Vec<Op>) {
    let (r, i, rep): (RouteId, ItemId, RepresentationId) = (ids.fresh(), ids.fresh(), ids.fresh());
    let ops = vec![
        Op::AddRoute { id: r, route: Route::new(page, points.clone(), ends) },
        Op::AddItem { id: i, item: route_drawing(page, style, points) },
        Op::LinkRepresentation {
            id: rep,
            rep: Representation {
                subject: Subject::Route(r),
                page,
                form: Form::Drawn,
                role: ecad_model::Role::Route,
                items: vec![i],
                connection_points: BTreeMap::new(),
            },
        },
    ];
    (r, i, ops)
}

/// The ops that remove route `r` with its drawing. Routes landing on it must be changed first
/// ([`RouteEdits::free_tees`] or [`RouteEdits::retarget_tees`]).
pub fn remove_route_ops(doc: &Document, r: RouteId) -> Vec<Op> {
    let mut ops = Vec::new();
    let mut items = Vec::new();
    for (rid, rep) in doc.representations_of(Subject::Route(r)) {
        ops.push(Op::UnlinkRepresentation { id: rid, rep: rep.clone() });
        items.extend(rep.items.iter().copied());
    }
    for i in items {
        if let Some(item) = doc.items().get(&i) {
            ops.push(Op::RemoveItem { id: i, item: item.clone() });
        }
    }
    if let Some(route) = doc.routes().get(&r) {
        ops.push(Op::RemoveRoute { id: r, route: route.clone() });
    }
    ops
}

/// Changes to existing routes gathered while planning, so that a route changed more than once is
/// one `ModifyRoute`, with one `ModifyItem` when its points move (its drawing follows it).
#[derive(Debug, Clone)]
pub(crate) struct RouteEdits<'a> {
    doc: &'a Document,
    after: BTreeMap<RouteId, Route>,
}

impl<'a> RouteEdits<'a> {
    pub fn new(doc: &'a Document) -> Self {
        RouteEdits { doc, after: BTreeMap::new() }
    }

    /// Route `r` as changed so far, to change further; `None` when there is no such route.
    pub fn get(&mut self, r: RouteId) -> Option<&mut Route> {
        if !self.after.contains_key(&r) {
            let route = self.doc.routes().get(&r)?.clone();
            self.after.insert(r, route);
        }
        self.after.get_mut(&r)
    }

    /// End `e` of route `r` lands on `to`.
    pub fn set_end(&mut self, r: RouteId, e: End, to: RouteEnd) {
        if let Some(x) = self.get(r).and_then(|route| route.ends.get_mut(end_index(e))) {
            *x = to;
        }
    }

    /// Every route end on connection point `from` (found through the index) lands on `to`
    /// instead: another point, or `Free` when the point goes.
    pub fn repoint(&mut self, from: ConnectionPointId, to: RouteEnd) {
        let routes: Vec<RouteId> = self.doc.routes_at(from).collect();
        for r in routes {
            if let Some(route) = self.get(r) {
                for e in route.ends.iter_mut().filter(|e| **e == RouteEnd::Point(from)) {
                    *e = to;
                }
            }
        }
    }

    /// The routes landing on route `from` land on route `to` at the same places.
    pub fn retarget_tees(&mut self, from: RouteId, to: RouteId) {
        let tees: Vec<RouteId> = self.doc.tees_of(from).collect();
        for t in tees {
            if let Some(route) = self.get(t) {
                for e in route.ends.iter_mut() {
                    if let RouteEnd::On { route: x, at } = *e
                        && x == from
                    {
                        *e = RouteEnd::On { route: to, at };
                    }
                }
            }
        }
    }

    /// The routes landing on route `r` end free where they are (before `r` goes).
    pub fn free_tees(&mut self, r: RouteId) {
        let tees: Vec<RouteId> = self.doc.tees_of(r).collect();
        for t in tees {
            if let Some(route) = self.get(t) {
                for e in route.ends.iter_mut().filter(|e| e.route() == Some(r)) {
                    *e = RouteEnd::Free;
                }
            }
        }
    }

    /// Route `r` is removed by other ops: no change of it is planned.
    pub fn forget(&mut self, r: RouteId) {
        self.after.remove(&r);
    }

    /// The ops: a `ModifyRoute` per changed route, with a `ModifyItem` of its drawing when its
    /// points changed.
    pub fn ops(self) -> Vec<Op> {
        let mut ops = Vec::new();
        for (r, after) in self.after {
            let Some(before) = self.doc.routes().get(&r) else { continue };
            if *before == after {
                continue;
            }
            if before.points != after.points
                && let Some((_, i, item)) = route_item(self.doc, r)
            {
                let moved = Item { kind: ItemKind::Primitive(Primitive::Polyline { vertices: after.points.clone(), closed: false }), ..item.clone() };
                ops.push(Op::ModifyItem { id: i, before: item.clone(), after: moved });
            }
            ops.push(Op::ModifyRoute { id: r, before: before.clone(), after });
        }
        ops
    }
}

/// The ops that let go of connection points `points` (being removed or replaced): every route
/// end on one of them ends free where it is, so the commit's rewrite takes their wires away.
pub(crate) fn free_points_ops(doc: &Document, points: impl IntoIterator<Item = ConnectionPointId>) -> Vec<Op> {
    let mut edits = RouteEdits::new(doc);
    for t in points {
        edits.repoint(t, RouteEnd::Free);
    }
    edits.ops()
}

/// The ops moving every route end from one connection point to another (`map`: from → to), in
/// one `ModifyRoute` per route.
pub(crate) fn repoint_ops(doc: &Document, map: &BTreeMap<ConnectionPointId, ConnectionPointId>) -> Vec<Op> {
    let mut edits = RouteEdits::new(doc);
    for (from, to) in map {
        edits.repoint(*from, RouteEnd::Point(*to));
    }
    edits.ops()
}

/// The free route ends on `page` drawn within `tol` of `at`.
pub(crate) fn loose_ends_at(doc: &Document, page: PageId, at: Pt, tol: f64) -> Vec<(RouteId, End)> {
    let mut out = Vec::new();
    for r in doc.routes_on(page) {
        let Some(route) = doc.routes().get(&r) else { continue };
        for (end, e) in End::BOTH.into_iter().zip(route.ends) {
            if e == RouteEnd::Free && end_point(doc, r, end).is_some_and(|p| p.near(at, tol)) {
                out.push((r, end));
            }
        }
    }
    out
}

// ---------------------------------------------------------------- the wires a route carries

/// The wire between connection points `a` and `b`, if there is one (through the index).
pub fn wire_between(doc: &Document, a: ConnectionPointId, b: ConnectionPointId) -> Option<WireId> {
    doc.ends_at(a).into_iter().map(|(w, _)| w).find(|w| doc.wires().get(w).is_some_and(|x| x.ends.contains(&a) && x.ends.contains(&b)))
}

/// A connection point route `r` reaches over routes, the nearest first: its own ends, then the
/// routes it lands on and that land on it; never a named point. The cost follows the routes
/// joined to it.
fn point_of_route(doc: &Document, r: RouteId) -> Option<ConnectionPointId> {
    let mut seen = BTreeSet::new();
    let mut todo = std::collections::VecDeque::from([r]);
    while let Some(x) = todo.pop_front() {
        if !seen.insert(x) {
            continue;
        }
        let Some(route) = doc.routes().get(&x) else { continue };
        // A named point (a cable reference's, F4) is an aid: no wire ends there.
        if let Some(t) = route.points_landed().find(|t| doc.named_points(*t).is_none()) {
            return Some(t);
        }
        todo.extend(route.ends.iter().filter_map(|e| e.route()));
        todo.extend(doc.tees_of(x));
    }
    None
}

/// The wire route `r` stands for when a person clicks it: the wire between its two connection
/// points when it runs from one to the other, else the first (in net order) of the wires at the
/// nearest connection point it reaches. `None` for a route on no net with wires.
pub fn wire_of_route(doc: &Document, r: RouteId) -> Option<WireId> {
    let route = doc.routes().get(&r)?;
    if let [RouteEnd::Point(a), RouteEnd::Point(b)] = route.ends
        && let Some(w) = wire_between(doc, a, b)
    {
        return Some(w);
    }
    let t = point_of_route(doc, r)?;
    doc.ends_at(t).into_iter().map(|(w, _)| w).filter_map(|w| doc.wires().get(&w).map(|x| (x.net_order, w))).min().map(|(_, w)| w)
}

/// The wires route `r` carries: the wire between its two connection points when it runs from one
/// to the other, else every wire of its net, in net order.
pub fn wires_of_route(doc: &Document, r: RouteId) -> Vec<WireId> {
    let Some(route) = doc.routes().get(&r) else { return Vec::new() };
    if let [RouteEnd::Point(a), RouteEnd::Point(b)] = route.ends
        && let Some(w) = wire_between(doc, a, b)
    {
        return vec![w];
    }
    let Some(net) = point_of_route(doc, r).and_then(|t| doc.ends_at(t).first().and_then(|(w, _)| doc.wires().get(w)).map(|w| w.net)) else {
        return Vec::new();
    };
    let mut v: Vec<(u16, WireId)> = doc.wires_of_net(net).filter_map(|w| doc.wires().get(&w).map(|x| (x.net_order, w))).collect();
    v.sort_unstable();
    v.into_iter().map(|(_, w)| w).collect()
}

/// The routes drawing wire `w`: those with an end on either of its connection points, in ID
/// order (through the index).
pub fn routes_of_wire(doc: &Document, w: WireId) -> Vec<RouteId> {
    let Some(wire) = doc.wires().get(&w) else { return Vec::new() };
    let mut v: Vec<RouteId> = wire.ends.iter().flat_map(|t| doc.routes_at(*t)).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// The route drawing wire `w` on `page`: the one running straight between its two points when
/// there is one, else the first of [`routes_of_wire`] there.
pub fn route_of_wire_on(doc: &Document, w: WireId, page: PageId) -> Option<RouteId> {
    let ends = doc.wires().get(&w)?.ends;
    let here: Vec<RouteId> = routes_of_wire(doc, w).into_iter().filter(|r| doc.routes().get(r).is_some_and(|x| x.page == page)).collect();
    let direct = here.iter().copied().find(|r| doc.routes().get(r).is_some_and(|x| ends.iter().all(|t| x.ends.contains(&RouteEnd::Point(*t)))));
    direct.or_else(|| here.first().copied())
}

/// Where connection point `t` is drawn as a route end: (page, point) of every route end landing
/// on it, in route ID order (through the index).
pub(crate) fn drawn_ends_at(doc: &Document, t: ConnectionPointId) -> Vec<(PageId, Pt)> {
    let mut out = Vec::new();
    for r in doc.routes_at(t) {
        let Some(route) = doc.routes().get(&r) else { continue };
        for (p, e) in [route.points.first(), route.points.last()].into_iter().zip(route.ends) {
            if let Some(p) = p
                && e == RouteEnd::Point(t)
            {
                out.push((route.page, *p));
            }
        }
    }
    out
}

/// The pages wire `w` is drawn on (see [`routes_of_wire`]).
pub fn wire_pages(doc: &Document, w: WireId) -> BTreeSet<PageId> {
    routes_of_wire(doc, w).into_iter().filter_map(|r| doc.routes().get(&r).map(|x| x.page)).collect()
}
