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

use crate::doc::{Document, Wire};
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

    /// The points that take more wires than they accept: `(point, wires, accepted)`.
    pub fn over(&self, accepts: &impl Fn(TerminalId) -> usize) -> Vec<(TerminalId, usize, usize)> {
        self.points.iter().filter(|t| self.count(**t) > accepts(**t)).map(|t| (*t, self.count(*t), accepts(*t))).collect()
    }
}

/// How many conductors connection point `t` accepts (design 072 §4.4, F38). Until points store
/// it (lane B's `Terminal.accepts`), every point accepts two.
pub fn accepts(_doc: &Document, _t: TerminalId) -> usize {
    2
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
            _ => {}
        }
    }
    if seeds.points.is_empty() && seeds.routes.is_empty() {
        return Vec::new();
    }
    Rewrite::new(doc, ops).run(&seeds)
}

#[derive(Default)]
struct Seeds {
    points: BTreeSet<TerminalId>,
    routes: BTreeSet<RouteId>,
}

impl Seeds {
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

/// The two ends of a wire in ID order: the pair it joins, whichever way it runs.
fn pair(ends: [TerminalId; 2]) -> [TerminalId; 2] {
    if ends[0] <= ends[1] { ends } else { [ends[1], ends[0]] }
}

struct Rewrite<'a> {
    doc: &'a Document,
    ids: IdAllocator,
}

impl<'a> Rewrite<'a> {
    fn new(doc: &'a Document, ops: &[Op]) -> Self {
        // Fresh IDs from the transaction's own content: replay reads them back from the log.
        let mut seed: u64 = 0x0072_D5EE_D0F0_0072;
        for (kind, id) in crate::diff::objects(ops).keys() {
            seed = seed.rotate_left(13) ^ (*id as u64) ^ ((*id >> 64) as u64) ^ (kind.len() as u64);
        }
        Rewrite { doc, ids: IdAllocator::seeded(seed ^ ops.len() as u64) }
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
        let mut c = Component { points: BTreeSet::new(), routes: BTreeSet::new() };
        let mut todo = vec![start];
        while let Some(n) = todo.pop() {
            match n {
                Start::Point(t) => {
                    if doc.terminals.contains_key(&t) && c.points.insert(t) {
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
                Start::Point(t) if !doc.terminals.contains_key(&t) => {
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
            for t in &c.points {
                for (w, _) in doc.ends_at(*t) {
                    if let Some(w) = doc.wires.get(&w) {
                        note_net(w.net, &mut todo, &mut old_nets);
                    }
                }
            }
            placed.extend(c.points.iter().copied());
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
            let dropped: Vec<(WireId, &Wire)> = pool.iter().filter(|(_, w)| !wanted.contains(&pair(w.ends))).copied().collect();
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
                }
                let id = self.fresh_wire();
                creates.push(Op::CreateWire { id, wire });
                if let Some((old_id, _)) = replaced {
                    replacements.entry(old_id).or_insert(id);
                }
            }
        }
        // 5. Every other wire of the nets touched goes; a cable member's place in its cable goes to
        //    the wire made in its place, else the member is removed.
        let mut members: Vec<Op> = Vec::new();
        let mut moved_in: BTreeSet<(crate::id::CableId, WireId)> = BTreeSet::new();
        for ws in old.values() {
            for (id, w) in ws {
                if kept.contains(id) {
                    continue;
                }
                for cable in doc.cables_of(*id) {
                    let Some(c) = doc.cables.get(&cable) else { continue };
                    let Some(index) = c.members.iter().position(|m| m.conductor == *id) else { continue };
                    let Some(member) = c.members.get(index).cloned() else { continue };
                    members.push(Op::RemoveCableMember { cable, index, member: member.clone() });
                    if let Some(new) = replacements.get(id)
                        && moved_in.insert((cable, *new))
                    {
                        members.push(Op::AddCableMember { cable, index, member: crate::doc::CableMember { conductor: *new, ..member } });
                    }
                }
                deletes.push(Op::DeleteWire { id: *id, wire: (*w).clone() });
            }
        }
        let mut out = modifies;
        out.extend(creates);
        out.extend(members);
        out.extend(deletes);
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
