//! A net's wire order: which pairs of its connection points the wires join, in order (design 072
//! §4.4, F32, lane D).
//!
//! # Semantics
//! - A net with points P1…Pn has n−1 wires, each between two connection points, in a stored order
//!   (`net_order` 1, 2, …). [`Order`] holds that list for one net and the points on it, in the
//!   order they joined.
//! - **Proposal: drawing order.** Each new point becomes the next wire, from the point its route
//!   comes from ([`Join::From`]); a route landing on the middle of a route joins from the **nearer
//!   end** of the wire that route carries ([`Join::OnWire`]); a point that joins with no route to
//!   follow ([`Join::Next`]) continues the chain from the last point added. A point takes at most
//!   as many wires as it accepts (2 normally, more on PE bars and earth studs), so the order chains
//!   by default and fans out (a star) only at points that accept more than two.
//! - **When the point is full** (overseer, 2026-11-17: predictable, never a point the person did
//!   not draw at). A T: the nearer end of the hit wire; if it is full, the far end of that wire; if
//!   both are full, the nearer end takes it anyway. A route from a point: that point, full or not.
//!   A point taking more wires than it accepts is an error the over-count check reports, so the
//!   person fixes it where they can see it ([`Order::over`]).
//! - **A point that leaves** is cut out: its neighbours in the order are joined to each other in
//!   order (a chain A–X–B becomes A–B), in the place of its first wire.
//! - Every operation costs time in proportion to the one net's wires, never the project's.
//!
//! - **The rewrite** ([`rewrite`]): after an edit, the nets whose set of points changed get their
//!   wires rewritten in the same transaction. A wire net is the connection points joined by routes
//!   (strip jumpers and a device's own bridges join nets but never carry a wire). Nets whose points
//!   did not change keep their wires untouched, even when their routes are redrawn.
//!
//! This module excludes where routes are drawn (the planners) and the over-count check's message.

use std::collections::{BTreeMap, BTreeSet};

use crate::doc::{Cable, Core, CorePosition, Document, Wire};
use crate::geom::Pt;
use crate::id::{ConductorId, IdAllocator, NetId, RouteId, TerminalId, WireId};
use crate::op::Op;
use crate::route::RouteEnd;

/// How a new point joins its net.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    /// Its route starts at this point of the net.
    From(TerminalId),
    /// Its route lands on the middle of a route carrying the wire `wire`; `near` is the wire's end
    /// nearer to where it lands.
    OnWire { wire: [TerminalId; 2], near: TerminalId },
    /// No route to follow: the chain continues from the last point added.
    Next,
}

/// One net's points, in the order they joined, and its wires, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Order {
    points: Vec<TerminalId>,
    wires: Vec<[TerminalId; 2]>,
}

impl Order {
    /// An order from stored wires (in `net_order`): the points in the order they first appear.
    pub fn from_wires(wires: Vec<[TerminalId; 2]>) -> Self {
        let mut points: Vec<TerminalId> = Vec::new();
        for w in &wires {
            for t in w {
                if !points.contains(t) {
                    points.push(*t);
                }
            }
        }
        Order { points, wires }
    }

    /// The wires, in order: index 0 is `net_order` 1.
    pub fn wires(&self) -> &[[TerminalId; 2]] {
        &self.wires
    }

    /// The points on the net, in the order they joined.
    pub fn points(&self) -> &[TerminalId] {
        &self.points
    }

    /// How many wires end at `t`.
    pub fn count(&self, t: TerminalId) -> usize {
        self.wires.iter().map(|w| w.iter().filter(|x| **x == t).count()).sum()
    }

    /// Whether `t` is on the net.
    pub fn has(&self, t: TerminalId) -> bool {
        self.points.contains(&t)
    }

    /// Add point `p` by the drawing-order rule (see the module docs). A point already on the net
    /// changes nothing.
    pub fn add(&mut self, p: TerminalId, join: Join, accepts: &impl Fn(TerminalId) -> usize) {
        if self.has(p) {
            return;
        }
        let Some(&last) = self.points.last() else {
            self.points.push(p);
            return;
        };
        let room = |t: TerminalId| self.has(t) && self.count(t) < accepts(t).max(1);
        let on_net = |t: TerminalId| self.has(t).then_some(t);
        let start = match join {
            Join::From(q) => on_net(q),
            Join::OnWire { wire, near } => {
                let far = if wire[0] == near { wire[1] } else { wire[0] };
                if room(near) || !room(far) { on_net(near).or_else(|| on_net(far)) } else { Some(far) }
            }
            Join::Next => Some(last),
        }
        .unwrap_or(last);
        self.points.push(p);
        self.wires.push([start, p]);
    }

    /// Cut point `x` out: its neighbours are joined to each other, in order, in the place of its
    /// first wire. A point not on the net changes nothing.
    pub fn cut(&mut self, x: TerminalId) {
        if !self.has(x) {
            return;
        }
        self.points.retain(|t| *t != x);
        let first = self.wires.iter().position(|w| w.contains(&x));
        let mut neighbours: Vec<TerminalId> = Vec::new();
        for w in self.wires.iter().filter(|w| w.contains(&x)) {
            for t in w {
                if *t != x && !neighbours.contains(t) {
                    neighbours.push(*t);
                }
            }
        }
        self.wires.retain(|w| !w.contains(&x));
        let Some(at) = first else { return };
        let joined: Vec<[TerminalId; 2]> = neighbours.windows(2).filter_map(|p| Some([*p.first()?, *p.get(1)?])).collect();
        let at = at.min(self.wires.len());
        self.wires.splice(at..at, joined);
    }

    /// Insert point `p` into wire `a`–`b`: it becomes `a`–`p` and `p`–`b`, in its place (the
    /// reverse of [`Order::cut`]: a terminal placed on a wire's line splits that wire). Returns
    /// whether there was such a wire; a point already on the net changes nothing.
    pub fn insert(&mut self, p: TerminalId, a: TerminalId, b: TerminalId) -> bool {
        if self.has(p) {
            return false;
        }
        let Some(at) = self.wires.iter().position(|w| *w == [a, b] || *w == [b, a]) else { return false };
        let [x, y] = self.wires.get(at).copied().unwrap_or([a, b]);
        self.wires.splice(at..=at, [[x, p], [p, y]]);
        self.points.push(p);
        true
    }

    /// The points that take more wires than they accept: `(point, wires, accepted)`.
    pub fn over(&self, accepts: &impl Fn(TerminalId) -> usize) -> Vec<(TerminalId, usize, usize)> {
        self.points.iter().filter(|t| self.count(**t) > accepts(**t)).map(|t| (*t, self.count(*t), accepts(*t))).collect()
    }
}

/// How many conductors connection point `t` accepts (design 072 §4.4, F38): its stored
/// [`crate::Accepts`]; two for a point that does not exist.
pub fn accepts(doc: &Document, t: TerminalId) -> usize {
    doc.terminals.get(&t).map_or(2, |x| x.accepts.get())
}

/// The rewrite pass (design 072 §4.4): the ops that bring the wires of every net whose points
/// `ops` changed in line with the routes, on `doc` as `ops` left it. Run in the same transaction
/// (the ledger runs it on every edit). Empty when no net's points changed.
///
/// # Semantics
/// - Seeds: the points and routes of every route `ops` add, remove or change (before and after),
///   the routes they land on, and every connection point `ops` remove.
/// - Each wire net touching a seed is found again over the routes; its id goes to the old net
///   with the most of its points there (ties: the lower id), else a new one; an old net keeps its
///   id in one net only, the one holding most of its points.
/// - The kept net's order stays, points that left are cut out ([`Order::cut`]), other old nets
///   joining it follow in their order, and new points join by the drawing-order rule.
/// - A pair already joined by a wire keeps that wire (its values, number and conductor id); a new
///   pair takes the values of a wire it replaces, else of a wire of its net. Wires of pairs no
///   longer wanted are deleted.
///
/// The cost grows with the nets touched, never with the project.
pub fn rewrite(doc: &Document, ops: &[Op]) -> Vec<Op> {
    let mut seeds = Seeds::default();
    for op in ops {
        match op {
            Op::AddRoute { id, route } | Op::RemoveRoute { id, route } => seeds.route(doc, *id, route),
            Op::ModifyRoute { id, before, after } => {
                seeds.route(doc, *id, before);
                seeds.route(doc, *id, after);
            }
            Op::RemoveTerminal { id, .. } => {
                seeds.points.insert(*id);
            }
            // A point added to a continuation joins its name group (F5).
            Op::AddTerminal { id, .. } if doc.named_points(*id).is_some() => {
                seeds.points.insert(*id);
            }
            // A continuation joining or leaving a name group (F5): the lines at its partners, old
            // and new, may now run on through it or no longer.
            Op::CreateAid { id, aid } | Op::DeleteAid { id, aid } => seeds.continuation(doc, *id, aid),
            Op::ModifyAid { id, before, after } => {
                seeds.continuation(doc, *id, before);
                seeds.continuation(doc, *id, after);
            }
            _ => {}
        }
    }
    if seeds.points.is_empty() && seeds.routes.is_empty() {
        return Vec::new();
    }
    Rewrite::new(doc, ops).run(&seeds)
}

/// A loose route (one with a free end) that has landed at one of `ends`, with the point it landed
/// at: the line that carried a wire whose other end came away.
fn loose_route_at(doc: &Document, ends: [TerminalId; 2]) -> Option<(RouteId, TerminalId)> {
    // A line ending at a named point (F4) is loose too once the wire through it went.
    let open = |e: &RouteEnd| *e == RouteEnd::Free || e.point().is_some_and(|t| doc.named_points(t).is_some());
    ends.into_iter().find_map(|e| doc.routes_at(e).find(|r| doc.routes.get(r).is_some_and(|x| x.ends.iter().any(open))).map(|r| (r, e)))
}

/// The wire route `r` carries, once the rewrite has run: the wire between its two connection
/// points when it runs from one to the other, else the first wire in net order at a point it
/// lands on. `None` for a route with a free end (a loose line carries no wire) or no point.
pub fn wire_of_route(doc: &Document, r: RouteId) -> Option<WireId> {
    let route = doc.routes.get(&r)?;
    if route.ends.contains(&RouteEnd::Free) {
        return None;
    }
    if let [RouteEnd::Point(a), RouteEnd::Point(b)] = route.ends
        && let Some(w) = doc.ends_at(a).into_iter().map(|(w, _)| w).find(|w| doc.wires.get(w).is_some_and(|x| x.ends.contains(&b)))
    {
        return Some(w);
    }
    route
        .ends
        .iter()
        .filter_map(|e| e.point())
        .flat_map(|t| doc.ends_at(t))
        .filter_map(|(w, _)| doc.wires.get(&w).map(|x| (x.net_order, w)))
        .min()
        .map(|(_, w)| w)
}

/// Where a core on route `r` belongs (owner ruling, 2026-11-18): on the wire the route carries
/// once both ends have landed, else on the route with the one end that has landed.
/// [`CorePosition::Unplaced`] when the route is gone.
pub fn core_position(doc: &Document, r: RouteId) -> CorePosition {
    if !doc.routes.contains_key(&r) {
        return CorePosition::Unplaced;
    }
    if let Some(w) = wire_of_route(doc, r) {
        return CorePosition::Wire(w);
    }
    line_position(doc, r)
}

/// Where a core on route `r` sits when the route carries no wire: on the route, with the one end
/// that has landed on a device point ([`CorePosition::Unplaced`] when the route is gone).
fn line_position(doc: &Document, r: RouteId) -> CorePosition {
    let Some(route) = doc.routes.get(&r) else { return CorePosition::Unplaced };
    // A named point (a cable reference's, a continuation's) is an aid, not a landing (F4).
    let open = |e: RouteEnd| e == RouteEnd::Free || e.point().is_some_and(|t| doc.named_points(t).is_some());
    let landed = match route.ends {
        [RouteEnd::Point(t), o] | [o, RouteEnd::Point(t)] if open(o) && doc.named_points(t).is_none() => Some(t),
        _ => None,
    };
    CorePosition::Route { route: r, landed }
}

/// The cores pass, run in the same transaction after [`rewrite`] (owner ruling, 2026-11-18): every
/// core on a route that `ops` added, changed or removed moves to where it now belongs: onto the
/// wire the route carries once both its ends have landed, or kept on the route with its landed end
/// brought up to date. A core whose route is gone stays unplaced or goes ([`Core::off_line`]). The
/// command makes the move; reading a file never does.
pub fn cores(doc: &Document, ops: &[Op]) -> Vec<Op> {
    let mut routes: BTreeSet<RouteId> = BTreeSet::new();
    for op in ops {
        if let Op::AddRoute { id, .. } | Op::RemoveRoute { id, .. } | Op::ModifyRoute { id, .. } = op {
            routes.insert(*id);
        }
    }
    // A line joined through a named point (F4) may now carry a wire: the lines at its partners too.
    let partners: Vec<RouteId> = routes
        .iter()
        .filter_map(|r| doc.routes.get(r))
        .flat_map(|r| r.ends)
        .filter_map(|e| doc.named_points(e.point()?))
        .flatten()
        .flat_map(|t| doc.routes_at(t).collect::<Vec<_>>())
        .collect();
    routes.extend(partners);
    // Each cable once, with every core on any of the routes: indices then stay right.
    let after: BTreeMap<RouteId, CorePosition> = routes.iter().map(|r| (*r, core_position(doc, *r))).collect();
    let cables: BTreeSet<crate::id::CableId> = routes.iter().flat_map(|r| doc.cables_on_route(*r)).collect();
    let mut out = Vec::new();
    for cable in cables {
        let Some(c) = doc.cables.get(&cable) else { continue };
        let mut members = c.cores.clone();
        let moving: Vec<(usize, Core, CorePosition)> = members
            .iter()
            .enumerate()
            .filter_map(|(i, m)| {
                let to = *after.get(&m.route()?)?;
                (m.position != to).then(|| (i, m.clone(), to))
            })
            .collect();
        // Take them out from the back, then put each back in its place where it now belongs.
        for (index, _, _) in moving.iter().rev() {
            members.remove(*index);
        }
        for (index, m, to) in moving {
            let member = if to == CorePosition::Unplaced {
                match m.off_line(&members) {
                    Some(k) => k,
                    None => continue,
                }
            } else if members.iter().any(|x| x.position == to) {
                continue;
            } else {
                Core { position: to, ..m }
            };
            let at = index.min(members.len());
            members.insert(at, member);
        }
        out.extend(Op::modify_cable(cable, c, Cable { cores: members, ..c.clone() }));
    }
    out
}

#[derive(Default)]
struct Seeds {
    points: BTreeSet<TerminalId>,
    routes: BTreeSet<RouteId>,
}

impl Seeds {
    /// Continuation `id` as `aid` (before or after an op): its own points and its partners'.
    fn continuation(&mut self, doc: &Document, id: crate::id::AidId, aid: &crate::aid::Aid) {
        if !matches!(aid, crate::aid::Aid::Continuation { .. }) {
            return;
        }
        self.points.extend(doc.continuation_points(id, aid));
    }

    fn route(&mut self, doc: &Document, id: RouteId, r: &crate::route::Route) {
        if doc.routes.contains_key(&id) {
            self.routes.insert(id);
        }
        for e in r.ends {
            match e {
                RouteEnd::Point(t) => {
                    self.points.insert(t);
                }
                RouteEnd::On { route, .. } if doc.routes.contains_key(&route) => {
                    self.routes.insert(route);
                }
                RouteEnd::On { .. } | RouteEnd::Free => {}
            }
        }
    }
}

/// A wire net found over the routes.
struct Component {
    points: BTreeSet<TerminalId>,
    routes: BTreeSet<RouteId>,
    /// The named points it runs through (F4).
    named: BTreeSet<TerminalId>,
}

impl Component {
    fn inside(&self, w: &Wire) -> bool {
        self.points.contains(&w.ends[0]) && self.points.contains(&w.ends[1])
    }
}

#[derive(Debug, Clone, Copy)]
enum Start {
    Point(TerminalId),
    Route(RouteId),
}

/// Whether segments `s` and `t` meet (touching counts). Non-finite points meet nothing.
fn segments_meet(s: (Pt, Pt), t: (Pt, Pt)) -> bool {
    let all = [s.0, s.1, t.0, t.1];
    if !all.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
        return false;
    }
    let cross = |o: Pt, a: Pt, b: Pt| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let eps = 1e-9;
    let sign = |v: f64| {
        if v > eps {
            1
        } else if v < -eps {
            -1
        } else {
            0
        }
    };
    let within =
        |p: Pt, a: Pt, b: Pt| p.x >= a.x.min(b.x) - eps && p.x <= a.x.max(b.x) + eps && p.y >= a.y.min(b.y) - eps && p.y <= a.y.max(b.y) + eps;
    let (d1, d2) = (sign(cross(t.0, t.1, s.0)), sign(cross(t.0, t.1, s.1)));
    let (d3, d4) = (sign(cross(s.0, s.1, t.0)), sign(cross(s.0, s.1, t.1)));
    if d1 * d2 < 0 && d3 * d4 < 0 {
        return true;
    }
    (d1 == 0 && within(s.0, t.0, t.1))
        || (d2 == 0 && within(s.1, t.0, t.1))
        || (d3 == 0 && within(t.0, s.0, s.1))
        || (d4 == 0 && within(t.1, s.0, s.1))
}

/// The two ends of a wire in ID order: the pair it joins, whichever way it runs.
fn pair(ends: [TerminalId; 2]) -> [TerminalId; 2] {
    if ends[0] <= ends[1] { ends } else { [ends[1], ends[0]] }
}

struct Rewrite<'a> {
    doc: &'a Document,
    ids: IdAllocator,
    /// The routes `ops` add or change, in op order: where a new net starts (drawing order).
    drawn: Vec<RouteId>,
    /// The routes `ops` change, with their ends before.
    lifted: Vec<(RouteId, [RouteEnd; 2])>,
}

impl<'a> Rewrite<'a> {
    fn new(doc: &'a Document, ops: &[Op]) -> Self {
        // Fresh IDs from the transaction's own content: replay reads them back from the log.
        let mut seed: u64 = 0x0072_D5EE_D0F0_0072;
        for (kind, id) in crate::diff::objects(ops).keys() {
            seed = seed.rotate_left(13) ^ (*id as u64) ^ ((*id >> 64) as u64) ^ (kind.len() as u64);
        }
        let drawn = ops
            .iter()
            .filter_map(|op| match op {
                Op::AddRoute { id, .. } | Op::ModifyRoute { id, .. } => Some(*id),
                _ => None,
            })
            .collect();
        let lifted = ops
            .iter()
            .filter_map(|op| match op {
                Op::ModifyRoute { id, before, .. } => Some((*id, before.ends)),
                _ => None,
            })
            .collect();
        Rewrite { doc, ids: IdAllocator::seeded(seed ^ ops.len() as u64), drawn, lifted }
    }

    fn fresh_wire(&mut self) -> WireId {
        loop {
            let id: WireId = self.ids.fresh();
            if !self.doc.wires.contains_key(&id) {
                return id;
            }
        }
    }

    fn fresh_net(&mut self) -> NetId {
        loop {
            let id: NetId = self.ids.fresh();
            if self.doc.wires_of_net(id).next().is_none() {
                return id;
            }
        }
    }

    /// The routes and points joined to `start` by routes.
    fn component(&self, start: Start) -> Component {
        let doc = self.doc;
        let mut c = Component { points: BTreeSet::new(), routes: BTreeSet::new(), named: BTreeSet::new() };
        let mut todo = vec![start];
        while let Some(n) = todo.pop() {
            match n {
                Start::Point(t) => {
                    // A named point is an aid (design 072 §4.4, F4): the net runs on through its
                    // partners, on any page, and no wire ends there.
                    if let Some(group) = doc.named_points(t) {
                        for g in group {
                            if c.named.insert(g) {
                                todo.extend(doc.routes_at(g).map(Start::Route));
                            }
                        }
                    } else if doc.terminals.contains_key(&t) && c.points.insert(t) {
                        todo.extend(doc.routes_at(t).map(Start::Route));
                    }
                }
                Start::Route(r) => {
                    let Some(route) = doc.routes.get(&r) else { continue };
                    if !c.routes.insert(r) {
                        continue;
                    }
                    for e in route.ends {
                        match e {
                            RouteEnd::Point(t) => todo.push(Start::Point(t)),
                            RouteEnd::On { route, .. } => todo.push(Start::Route(route)),
                            RouteEnd::Free => {}
                        }
                    }
                    todo.extend(doc.tees_of(r).map(Start::Route));
                }
            }
        }
        c
    }

    fn run(mut self, seeds: &Seeds) -> Vec<Op> {
        let doc = self.doc;
        // 1. The wire nets touching the seeds, and every old net with a wire at one of their points
        //    (its other points are seeds too: a net may have split).
        let mut comps: Vec<Component> = Vec::new();
        let mut placed: BTreeSet<TerminalId> = BTreeSet::new();
        let mut seen_routes: BTreeSet<RouteId> = BTreeSet::new();
        let mut old_nets: BTreeSet<NetId> = BTreeSet::new();
        let mut todo: Vec<Start> = seeds.points.iter().map(|t| Start::Point(*t)).chain(seeds.routes.iter().map(|r| Start::Route(*r))).collect();
        let note_net = |n: NetId, todo: &mut Vec<Start>, old_nets: &mut BTreeSet<NetId>| {
            if old_nets.insert(n) {
                for x in doc.wires_of_net(n) {
                    if let Some(x) = doc.wires.get(&x) {
                        todo.extend(x.ends.iter().map(|e| Start::Point(*e)));
                    }
                }
            }
        };
        while let Some(s) = todo.pop() {
            match s {
                Start::Point(t) if placed.contains(&t) => continue,
                Start::Route(r) if seen_routes.contains(&r) => continue,
                Start::Point(t) if !doc.has_point(t) => {
                    // A point that is gone: its wires' nets are rewritten without it.
                    placed.insert(t);
                    for (w, _) in doc.ends_at(t) {
                        if let Some(w) = doc.wires.get(&w) {
                            note_net(w.net, &mut todo, &mut old_nets);
                        }
                    }
                    continue;
                }
                Start::Point(_) | Start::Route(_) => {}
            }
            let c = self.component(s);
            // A wire ending at a named point (one made before its point became one) goes too.
            for t in c.points.iter().chain(&c.named) {
                for (w, _) in doc.ends_at(*t) {
                    if let Some(w) = doc.wires.get(&w) {
                        note_net(w.net, &mut todo, &mut old_nets);
                    }
                }
            }
            placed.extend(c.points.iter().copied());
            placed.extend(c.named.iter().copied());
            seen_routes.extend(c.routes.iter().copied());
            comps.push(c);
        }
        comps.sort_by_key(|c| c.points.first().copied());
        // 2. Each old net's wires, in order.
        let mut old: BTreeMap<NetId, Vec<(WireId, &Wire)>> = BTreeMap::new();
        for n in &old_nets {
            let mut ws: Vec<(WireId, &Wire)> = doc.wires_of_net(*n).filter_map(|w| doc.wires.get(&w).map(|x| (w, x))).collect();
            ws.sort_by_key(|(w, x)| (x.net_order, *w));
            old.insert(*n, ws);
        }
        // 3. Which net each old net's id goes to: the one holding most of its points (so most of
        //    its wires once rewritten; ties: the first).
        let mut claims: BTreeMap<usize, Vec<(usize, NetId)>> = BTreeMap::new();
        for (n, ws) in &old {
            let points: BTreeSet<TerminalId> = ws.iter().flat_map(|(_, w)| w.ends).collect();
            let best = comps
                .iter()
                .enumerate()
                .map(|(i, c)| (points.iter().filter(|p| c.points.contains(p)).count(), i))
                .filter(|(k, _)| *k > 1)
                .max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
            if let Some((k, i)) = best {
                claims.entry(i).or_default().push((k, *n));
            }
        }
        let mut deletes: Vec<Op> = Vec::new();
        let mut modifies: Vec<Op> = Vec::new();
        let mut creates: Vec<Op> = Vec::new();
        let mut kept: BTreeSet<WireId> = BTreeSet::new();
        // A deleted wire → the wire made in its place: its cable membership moves there.
        let mut replacements: BTreeMap<WireId, WireId> = BTreeMap::new();
        // A deleted wire → every wire made sharing an end with it (the pieces of a split wire).
        let mut pieces: BTreeMap<WireId, Vec<(WireId, [TerminalId; 2])>> = BTreeMap::new();
        // The old net ids a net keeps (step 3's keepers).
        let taken: BTreeSet<NetId> =
            claims.values().filter_map(|ks| ks.iter().max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1))).map(|(_, n)| *n)).collect();
        for (i, c) in comps.iter().enumerate() {
            let mut keepers = claims.remove(&i).unwrap_or_default();
            keepers.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            let keeper = keepers.first().map(|(_, n)| *n);
            let nets_here: Vec<NetId> = old.iter().filter(|(_, ws)| ws.iter().any(|(_, w)| c.inside(w))).map(|(n, _)| *n).collect();
            // Unchanged: one old net, on exactly these points, every wire inside.
            if let [only] = nets_here[..]
                && let Some(ws) = old.get(&only)
                && ws.iter().all(|(_, w)| c.inside(w))
                && ws.iter().flat_map(|(_, w)| w.ends).collect::<BTreeSet<_>>() == c.points
            {
                kept.extend(ws.iter().map(|(id, _)| *id));
                continue;
            }
            if c.points.len() < 2 {
                continue;
            }
            let net = match keeper {
                Some(n) => n,
                None => self.fresh_net(),
            };
            let trimmed = |n: NetId| -> Order {
                let mut o = Order::from_wires(old.get(&n).map(|ws| ws.iter().map(|(_, w)| w.ends).collect()).unwrap_or_default());
                for p in o.points().to_vec() {
                    if !c.points.contains(&p) {
                        o.cut(p);
                    }
                }
                o
            };
            let fixed = keeper.and_then(|n| old.get(&n)).is_some_and(|ws| ws.iter().any(|(_, w)| w.order_fixed));
            let mut o = keeper.map(trimmed).unwrap_or_default();
            let others: Vec<Order> = nets_here.iter().copied().filter(|n| Some(*n) != keeper).map(trimmed).collect();
            let acc = |t: TerminalId| accepts(doc, t);
            let mut pending: Vec<TerminalId> = c.points.iter().copied().filter(|p| !o.has(*p)).collect();
            // A new net starts where its first drawn route starts (drawing order), not at the lower ID.
            if o.points().is_empty()
                && let Some(first) = self
                    .drawn
                    .iter()
                    .filter(|r| c.routes.contains(r))
                    .find_map(|r| doc.routes.get(r)?.ends[0].point())
                    .filter(|q| pending.contains(q))
            {
                pending.retain(|x| *x != first);
                pending.insert(0, first);
            }
            // A point drawn into the line of a wire splits that wire (the reverse of a cut).
            pending.retain(|p| {
                let near = self.neighbours(c, *p);
                let split = o.wires().iter().copied().find(|[a, b]| near.contains(a) && near.contains(b) && !self.neighbours(c, *a).contains(b));
                !split.is_some_and(|[a, b]| o.insert(*p, a, b))
            });
            while !pending.is_empty() {
                let found = pending.iter().enumerate().find_map(|(k, p)| self.join_for(c, &o, *p).map(|j| (k, j)));
                let (at, join) = found.unwrap_or((0, Join::Next));
                let p = pending.remove(at);
                o.add(p, join, &acc);
                // An old net reached here follows in its own order.
                if let Some(other) = others.iter().find(|x| x.has(p)) {
                    let mut grew = true;
                    while grew {
                        grew = false;
                        for w in other.wires() {
                            let next = match (o.has(w[0]), o.has(w[1])) {
                                (true, false) => Some((w[1], w[0])),
                                (false, true) => Some((w[0], w[1])),
                                (true, true) | (false, false) => None,
                            };
                            if let Some((n, from)) = next {
                                o.add(n, Join::From(from), &acc);
                                pending.retain(|x| *x != n);
                                grew = true;
                            }
                        }
                    }
                }
            }
            // 4. A wire for each of the order's pairs.
            let pool: Vec<(WireId, &Wire)> = old.values().flatten().filter(|(_, w)| c.inside(w)).map(|(id, w)| (*id, *w)).collect();
            let mut by_pair: BTreeMap<[TerminalId; 2], (WireId, &Wire)> = BTreeMap::new();
            for (id, w) in &pool {
                by_pair.entry(pair(w.ends)).or_insert((*id, *w));
            }
            let wanted: BTreeSet<[TerminalId; 2]> = o.wires().iter().map(|w| pair(*w)).collect();
            // Dropped: the pool's wires no longer wanted, and old wires with one end here and the
            // other in another net now (a wire split across a device's mated points, a fuse): no
            // net keeps those, and the new wires here replace them.
            let mut dropped: Vec<(WireId, &Wire)> = pool.iter().filter(|(_, w)| !wanted.contains(&pair(w.ends))).copied().collect();
            dropped
                .extend(old.values().flatten().filter(|(_, w)| !c.inside(w) && w.ends.iter().any(|e| c.points.contains(e))).map(|(id, w)| (*id, *w)));
            for (k, ends) in o.wires().iter().enumerate() {
                let net_order = u16::try_from(k + 1).unwrap_or(u16::MAX);
                if let Some((id, w)) = by_pair.get(&pair(*ends)) {
                    if !kept.insert(*id) {
                        continue;
                    }
                    let after = Wire { net, net_order, order_fixed: fixed, ..(*w).clone() };
                    if after != **w {
                        modifies.push(Op::ModifyWire { id: *id, before: (*w).clone(), after });
                    }
                    continue;
                }
                // The wire it replaces: a dropped one sharing an end, else any dropped one.
                let replaced = dropped.iter().find(|(_, w)| w.ends.iter().any(|e| ends.contains(e))).or(dropped.first()).copied();
                let template = replaced.map(|(_, w)| w).or(pool.first().map(|(_, w)| *w));
                let conductor: ConductorId = self.ids.fresh();
                let mut wire = Wire::new(net, conductor, ends[0], ends[1]);
                wire.net_order = net_order;
                wire.order_fixed = fixed;
                if let Some(t) = template {
                    wire = t.values_onto(&wire);
                    // A wire split across two nets (a fuse, mated pins): its number stays with the
                    // half in the net that keeps the old net's id, else with the half at its end A;
                    // the other half gets its own, so no two nets share a number.
                    let keeps = net == t.net || (!taken.contains(&t.net) && c.points.contains(&t.ends[0]));
                    if !c.inside(t) && !keeps {
                        wire.number = None;
                    }
                }
                let id = self.fresh_wire();
                creates.push(Op::CreateWire { id, wire });
                if let Some((old_id, _)) = replaced {
                    replacements.entry(old_id).or_insert(id);
                }
                for (old_id, w) in &dropped {
                    if w.ends.iter().any(|e| ends.contains(e)) {
                        pieces.entry(*old_id).or_default().push((id, *ends));
                    }
                }
            }
        }
        // 5. Every other wire of the nets touched goes; a cable member's place in its cable goes to
        //    the wire made in its place, else it stays unplaced or goes (`Core::off_line`).
        let mut moved_in: BTreeSet<(crate::id::CableId, WireId)> = BTreeSet::new();
        // Per cable: (index, member, where it goes), applied together so indices stay right.
        let mut per_cable: BTreeMap<crate::id::CableId, Vec<(usize, Core, Option<CorePosition>)>> = BTreeMap::new();
        for ws in old.values() {
            for (id, w) in ws {
                if kept.contains(id) {
                    continue;
                }
                for cable in doc.cables_of(*id) {
                    let Some(c) = doc.cables.get(&cable) else { continue };
                    let Some(index) = c.cores.iter().position(|m| m.wire() == Some(*id)) else { continue };
                    let Some(member) = c.cores.get(index).cloned() else { continue };
                    // A split wire's core goes to the piece the cable's slash crosses, else to the
                    // wire made in its place.
                    let crossed = pieces.get(id).and_then(|ps| {
                        ps.iter().find(|(new, ends)| !moved_in.contains(&(cable, *new)) && self.slash_crosses(cable, *ends)).map(|(new, _)| *new)
                    });
                    let position = match crossed.as_ref().or(replacements.get(id)) {
                        Some(new) if moved_in.insert((cable, *new)) => Some(CorePosition::Wire(*new)),
                        Some(_) => None,
                        // No wire in its place: the core stays on the line that carried it, now loose.
                        None => self.loose_line_of(*id, w.ends),
                    };
                    per_cable.entry(cable).or_default().push((index, member, position));
                }
                deletes.push(Op::DeleteWire { id: *id, wire: (*w).clone() });
            }
        }
        let mut members: Vec<Op> = Vec::new();
        for (cable, mut changes) in per_cable {
            let Some(c) = doc.cables.get(&cable) else { continue };
            let mut list = c.cores.clone();
            changes.sort_by_key(|(i, _, _)| *i);
            for (index, _, _) in changes.iter().rev() {
                list.remove(*index);
            }
            for (index, m, to) in changes {
                let member = match to {
                    Some(to) if list.iter().any(|x| x.position == to) => continue,
                    Some(to) => Core { position: to, ..m },
                    None => match m.off_line(&list) {
                        Some(k) => k,
                        None => continue,
                    },
                };
                let at = index.min(list.len());
                list.insert(at, member);
            }
            members.extend(Op::modify_cable(cable, c, Cable { cores: list, ..c.clone() }));
        }
        let mut out = modifies;
        out.extend(creates);
        out.extend(members);
        out.extend(deletes);
        out
    }

    /// Whether a slash of `cable` (a line of one of its drawings) crosses the route running
    /// straight between points `ends` (one route from one to the other, on the slash's page).
    fn slash_crosses(&self, cable: crate::id::CableId, ends: [TerminalId; 2]) -> bool {
        let doc = self.doc;
        let [a, b] = ends;
        let Some(route) = doc
            .routes_at(a)
            .filter_map(|r| doc.routes.get(&r))
            .find(|r| r.ends.contains(&RouteEnd::Point(a)) && r.ends.contains(&RouteEnd::Point(b)))
        else {
            return false;
        };
        let path: Vec<(Pt, Pt)> = route.points.windows(2).map(|w| (w[0], w[1])).collect();
        doc.representations_of(crate::doc::Subject::Cable(cable)).filter(|(_, r)| r.page == route.page).flat_map(|(_, r)| r.items.iter()).any(|i| {
            let segs: Vec<(Pt, Pt)> = match doc.items.get(i).map(|x| &x.kind) {
                Some(crate::doc::ItemKind::Primitive(crate::doc::Primitive::Line { a, b })) => vec![(*a, *b)],
                Some(crate::doc::ItemKind::Primitive(crate::doc::Primitive::Polyline { vertices, .. })) => {
                    vertices.windows(2).map(|w| (w[0], w[1])).collect()
                }
                _ => Vec::new(),
            };
            segs.iter().any(|s| path.iter().any(|p| segments_meet(*s, *p)))
        })
    }

    /// The loose line that carried a wire between `ends` (its route now has a free end): one still
    /// landed at an end, else one that `ops` moved off them. Where a core on it now sits.
    fn loose_line_of(&self, gone: WireId, ends: [TerminalId; 2]) -> Option<CorePosition> {
        let doc = self.doc;
        let r = loose_route_at(doc, ends).map(|(r, _)| r).or_else(|| {
            self.lifted
                .iter()
                .find(|(r, before)| before.iter().any(|e| e.point().is_some_and(|t| ends.contains(&t))) && doc.routes.contains_key(r))
                .map(|(r, _)| *r)
        })?;
        match core_position(doc, r) {
            p @ CorePosition::Route { .. } => Some(p),
            // The wire this pass deletes still shows here (F5: a renamed continuation broke it).
            CorePosition::Wire(w) if w == gone => match line_position(doc, r) {
                p @ CorePosition::Route { .. } => Some(p),
                CorePosition::Wire(_) | CorePosition::Unplaced => None,
            },
            CorePosition::Wire(_) | CorePosition::Unplaced => None,
        }
    }

    /// The points joined to `t` by routes of `c` without passing another point.
    fn neighbours(&self, c: &Component, t: TerminalId) -> BTreeSet<TerminalId> {
        let doc = self.doc;
        let mut out = BTreeSet::new();
        let mut seen: BTreeSet<RouteId> = BTreeSet::new();
        let mut todo: Vec<RouteId> = doc.routes_at(t).filter(|r| c.routes.contains(r)).collect();
        while let Some(r) = todo.pop() {
            if !seen.insert(r) {
                continue;
            }
            let Some(route) = doc.routes.get(&r) else { continue };
            for e in route.ends {
                match e {
                    // Through a named point to the lines at its partners (F4).
                    RouteEnd::Point(q) if c.named.contains(&q) => {
                        for g in doc.named_points(q).unwrap_or_default() {
                            todo.extend(doc.routes_at(g).filter(|x| c.routes.contains(x)));
                        }
                    }
                    RouteEnd::Point(q) if q != t => {
                        out.insert(q);
                    }
                    RouteEnd::On { route, .. } if c.routes.contains(&route) => todo.push(route),
                    RouteEnd::Point(_) | RouteEnd::On { .. } | RouteEnd::Free => {}
                }
            }
            todo.extend(doc.tees_of(r).filter(|x| c.routes.contains(x)));
        }
        out
    }

    /// How new point `p` joins order `o` through the routes of `c`, when a route leads to a point
    /// already in the order.
    fn join_for(&self, c: &Component, o: &Order, p: TerminalId) -> Option<Join> {
        let doc = self.doc;
        for r in doc.routes_at(p).filter(|r| c.routes.contains(r)) {
            let Some(route) = doc.routes.get(&r) else { continue };
            let Some(here) = route.ends.iter().position(|e| *e == RouteEnd::Point(p)) else { continue };
            match route.ends.get(1 - here).copied() {
                Some(RouteEnd::Point(q)) if o.has(q) => return Some(Join::From(q)),
                // Through a named point: from a point beyond it already in the order (F4), the
                // first with room for another wire, else the first (F5: a continuation's partner
                // may sit on a T whose point is already full).
                Some(RouteEnd::Point(q)) if c.named.contains(&q) => {
                    let beyond = self.neighbours(c, p);
                    let mut found = o.points().iter().copied().filter(|x| beyond.contains(x));
                    let first = found.clone().next();
                    if let Some(x) = found.find(|x| o.count(*x) < accepts(doc, *x).max(1)).or(first) {
                        return Some(Join::From(x));
                    }
                }
                Some(RouteEnd::On { route: h, at }) => {
                    if let Some(j) = self.on_route(o, h, at) {
                        return Some(j);
                    }
                }
                Some(RouteEnd::Point(_) | RouteEnd::Free) | None => {}
            }
            // A route landing on this one from a point in the order.
            for s in doc.tees_of(r) {
                if let Some(q) = doc.routes.get(&s).and_then(|sr| sr.points_landed().find(|q| o.has(*q))) {
                    return Some(Join::From(q));
                }
            }
        }
        None
    }

    /// The join of a route landing at `at` on route `h`: from the nearer end of the wire `h`
    /// carries, else from its one point in the order.
    fn on_route(&self, o: &Order, h: RouteId, at: crate::geom::Pt) -> Option<Join> {
        let hr = self.doc.routes.get(&h)?;
        let a = hr.ends[0].point().filter(|t| o.has(*t));
        let b = hr.ends[1].point().filter(|t| o.has(*t));
        match (a, b) {
            (Some(a), Some(b)) => {
                let near = match hr.along(at) {
                    Some((s, len)) if s * 2.0 > len => b,
                    _ => a,
                };
                Some(Join::OnWire { wire: [a, b], near })
            }
            (Some(x), None) | (None, Some(x)) => Some(Join::From(x)),
            (None, None) => None,
        }
    }
}

/// The ops clearing every `protected_by` that names a device `ops` deleted (design 072 §4.5,
/// overseer ruling 2026-11-18), on `doc` as `ops` left it: the same transaction, so one undo
/// brings them back. Nothing unless `ops` delete a device.
pub fn unprotect(doc: &Document, ops: &[Op]) -> Vec<Op> {
    let gone: BTreeSet<crate::id::DeviceId> = ops
        .iter()
        .filter_map(|o| if let Op::DeleteDevice { id, .. } = o { Some(*id) } else { None })
        .filter(|d| !doc.devices.contains_key(d))
        .collect();
    if gone.is_empty() {
        return Vec::new();
    }
    let hit = |p: Option<crate::wire::ProtectedBy>| p.is_some_and(|p| gone.contains(&p.device));
    let mut out = Vec::new();
    for (id, w) in &doc.wires {
        if hit(w.protected_by) {
            out.push(Op::ModifyWire { id: *id, before: w.clone(), after: Wire { protected_by: None, ..w.clone() } });
        }
    }
    for (id, c) in &doc.cables {
        if hit(c.protected_by) {
            out.extend(Op::modify_cable(*id, c, Cable { protected_by: None, ..c.clone() }));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(n: u128) -> TerminalId {
        TerminalId(n)
    }

    fn two(_: TerminalId) -> usize {
        2
    }

    fn order(o: &Order) -> Vec<(u128, u128)> {
        o.wires().iter().map(|w| (w[0].0, w[1].0)).collect()
    }

    #[test]
    fn drawing_order_chains_and_a_tee_joins_from_the_nearer_end() {
        // A–B drawn, then C onto the middle of A–B, nearer B: 1 = A–B, 2 = B–C.
        let mut o = Order::default();
        o.add(t(1), Join::Next, &two);
        o.add(t(2), Join::From(t(1)), &two);
        o.add(t(3), Join::OnWire { wire: [t(1), t(2)], near: t(2) }, &two);
        assert_eq!(order(&o), [(1, 2), (2, 3)]);
        assert!(o.over(&two).is_empty());
    }

    #[test]
    fn a_tee_takes_the_nearer_end_then_the_far_end_then_the_nearer_end_over() {
        let mut o = Order::from_wires(vec![[t(1), t(2)], [t(2), t(3)]]);
        // 1. The nearer end has room: it takes the wire.
        let mut room = o.clone();
        room.add(t(4), Join::OnWire { wire: [t(2), t(3)], near: t(3) }, &two);
        assert_eq!(order(&room), [(1, 2), (2, 3), (3, 4)]);
        // 2. The nearer end (2) is full: the far end of the same wire (1) takes it.
        o.add(t(4), Join::OnWire { wire: [t(1), t(2)], near: t(2) }, &two);
        assert_eq!(order(&o), [(1, 2), (2, 3), (1, 4)]);
        assert!(o.over(&two).is_empty());
        // 3. Both ends (1 and 2) are full: the nearer end takes it anyway and is reported over.
        o.add(t(5), Join::OnWire { wire: [t(1), t(2)], near: t(2) }, &two);
        assert_eq!(order(&o), [(1, 2), (2, 3), (1, 4), (2, 5)]);
        assert_eq!(o.over(&two), [(t(2), 3, 2)]);
    }

    #[test]
    fn a_route_from_a_full_point_starts_there_and_is_reported_over() {
        let mut o = Order::from_wires(vec![[t(1), t(2)], [t(2), t(3)]]);
        o.add(t(4), Join::From(t(2)), &two);
        assert_eq!(order(&o), [(1, 2), (2, 3), (2, 4)], "never a point the person did not draw at");
        assert_eq!(o.over(&two), [(t(2), 3, 2)]);
    }

    #[test]
    fn a_point_accepting_more_fans_out_as_a_star() {
        let pe = |x: TerminalId| if x == t(1) { 6 } else { 2 };
        let mut o = Order::default();
        o.add(t(1), Join::Next, &pe);
        for n in 2..=5 {
            o.add(t(n), Join::From(t(1)), &pe);
        }
        assert_eq!(order(&o), [(1, 2), (1, 3), (1, 4), (1, 5)]);
        assert_eq!(o.count(t(1)), 4);
    }

    #[test]
    fn a_point_that_leaves_is_cut_out_and_its_neighbours_joined() {
        let mut o = Order::from_wires(vec![[t(1), t(2)], [t(2), t(3)], [t(3), t(4)]]);
        o.cut(t(2));
        assert_eq!(order(&o), [(1, 3), (3, 4)]);
        o.cut(t(4));
        assert_eq!(order(&o), [(1, 3)]);
        o.cut(t(9));
        assert_eq!(order(&o), [(1, 3)]);
        assert_eq!(o.points(), [t(1), t(3)]);
    }

    #[test]
    fn with_no_room_anywhere_the_preferred_point_takes_it_and_is_over() {
        let one = |_: TerminalId| 1;
        let mut o = Order::default();
        o.add(t(1), Join::Next, &one);
        o.add(t(2), Join::From(t(1)), &one);
        o.add(t(3), Join::From(t(1)), &one);
        assert_eq!(order(&o), [(1, 2), (1, 3)]);
        assert_eq!(o.over(&one), [(t(1), 2, 1)]);
    }
}
