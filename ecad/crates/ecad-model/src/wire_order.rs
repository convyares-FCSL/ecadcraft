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
//!   as many wires as it accepts (2 normally, more on PE bars and earth studs): when the preferred
//!   point is full, the wire starts from the most recently added point that still has room, so the
//!   order chains by default and fans out (a star) only at points that accept more than two. When
//!   no point has room, the preferred point takes it anyway, and the over-count check reports it.
//! - **A point that leaves** is cut out: its neighbours in the order are joined to each other in
//!   order (a chain A–X–B becomes A–B), in the place of its first wire.
//! - Every operation costs time in proportion to the one net's wires, never the project's.
//!
//! This module excludes where the points are drawn and which nets changed (the routes and the
//! rewrite pass) and the over-count check; it is pure data.

use crate::id::TerminalId;

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
        let room = |o: &Order, t: TerminalId| o.has(t) && o.count(t) < accepts(t).max(1);
        let preferred: Vec<TerminalId> = match join {
            Join::From(q) => vec![q],
            Join::OnWire { wire, near } => {
                let far = if wire[0] == near { wire[1] } else { wire[0] };
                vec![near, far]
            }
            Join::Next => vec![last],
        };
        let start = preferred
            .iter()
            .copied()
            .find(|t| room(self, *t))
            .or_else(|| self.points.iter().rev().copied().find(|t| room(self, *t)))
            .or_else(|| preferred.first().copied().filter(|t| self.has(*t)))
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
        // D onto A–B nearer B: B is full, so the far end A takes it.
        o.add(t(4), Join::OnWire { wire: [t(1), t(2)], near: t(2) }, &two);
        assert_eq!(order(&o), [(1, 2), (2, 3), (1, 4)]);
        // E from the full B: the most recent point with room (D) takes it.
        o.add(t(5), Join::From(t(2)), &two);
        assert_eq!(order(&o), [(1, 2), (2, 3), (1, 4), (4, 5)]);
        assert!(o.over(&two).is_empty());
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
