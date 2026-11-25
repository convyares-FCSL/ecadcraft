//! The grid of cells the wire checks look things up in: which routes touch a cell, and how near a point is to a segment.

use super::*;
pub(super) type Cell = (PageId, i64, i64);

pub(super) fn cell_of(page: PageId, p: Pt) -> Option<Cell> {
    if !p.is_finite() {
        return None;
    }
    Some((page, (p.x / GRID_MM).floor() as i64, (p.y / GRID_MM).floor() as i64))
}

/// The cells a segment's box (grown by [`ON_WIRE`]) covers; capped, so a hostile huge segment
/// cannot cost more than a page.
pub(super) fn cells_of_segment(page: PageId, a: Pt, b: Pt) -> Vec<Cell> {
    let (Some(lo), Some(hi)) = (
        cell_of(page, Pt::new(a.x.min(b.x) - ON_WIRE, a.y.min(b.y) - ON_WIRE)),
        cell_of(page, Pt::new(a.x.max(b.x) + ON_WIRE, a.y.max(b.y) + ON_WIRE)),
    ) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for x in lo.1..=hi.1 {
        for y in lo.2..=hi.2 {
            out.push((page, x, y));
            if out.len() > 10_000 {
                return out;
            }
        }
    }
    out
}

/// The per-page grid of route segments and free route ends.
#[derive(Debug, Clone, Default)]
pub(crate) struct Grid {
    segments: HashMap<Cell, BTreeSet<(RouteId, usize)>>,
    ends: HashMap<Cell, BTreeSet<(RouteId, End)>>,
    /// What each route put in the grid, to take it out again.
    placed: HashMap<RouteId, Placed>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct Placed {
    /// (page, segment index, a, b)
    segments: Vec<(PageId, usize, Pt, Pt)>,
    /// (page, end, point) of free ends
    ends: Vec<(PageId, End, Pt)>,
    cells: BTreeSet<Cell>,
}

impl Grid {
    pub(super) fn remove(&mut self, c: RouteId) {
        let Some(p) = self.placed.remove(&c) else { return };
        for (page, i, a, b) in &p.segments {
            for cell in cells_of_segment(*page, *a, *b) {
                if let Some(s) = self.segments.get_mut(&cell) {
                    s.remove(&(c, *i));
                    if s.is_empty() {
                        self.segments.remove(&cell);
                    }
                }
            }
        }
        for (page, end, at) in &p.ends {
            if let Some(cell) = cell_of(*page, *at)
                && let Some(s) = self.ends.get_mut(&cell)
            {
                s.remove(&(c, *end));
                if s.is_empty() {
                    self.ends.remove(&cell);
                }
            }
        }
    }

    pub(super) fn insert(&mut self, doc: &Document, c: RouteId) {
        self.remove(c);
        let Some(route) = doc.routes().get(&c) else { return };
        let (page, v) = (route.page, &route.points);
        let mut placed = Placed::default();
        for (i, w) in v.windows(2).enumerate() {
            let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
            for cell in cells_of_segment(page, *a, *b) {
                self.segments.entry(cell).or_default().insert((c, i));
                placed.cells.insert(cell);
            }
            placed.segments.push((page, i, *a, *b));
        }
        for ((end, at), e) in [(End::A, v.first().copied()), (End::B, v.last().copied())].into_iter().zip(route.ends) {
            let Some(at) = at else { continue };
            if e == RouteEnd::Free
                && let Some(cell) = cell_of(page, at)
            {
                self.ends.entry(cell).or_default().insert((c, end));
                placed.cells.insert(cell);
                placed.ends.push((page, end, at));
            }
        }
        self.placed.insert(c, placed);
    }

    /// The cells `c` occupies now.
    pub(super) fn cells(&self, c: RouteId) -> BTreeSet<Cell> {
        self.placed.get(&c).map(|p| p.cells.clone()).unwrap_or_default()
    }

    /// Routes with a free end in any of `cells` (and the 8 cells round each). A cell at the edge of
    /// the grid's range (a point at about 1e308 mm) has no neighbours past that edge.
    pub(super) fn loose_near(&self, cells: &BTreeSet<Cell>) -> BTreeSet<RouteId> {
        let mut out = BTreeSet::new();
        for (page, x, y) in cells {
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let (Some(nx), Some(ny)) = (x.checked_add(dx), y.checked_add(dy)) else { continue };
                    if let Some(s) = self.ends.get(&(*page, nx, ny)) {
                        out.extend(s.iter().map(|(c, _)| *c));
                    }
                }
            }
        }
        out
    }

    /// Whether `at` on `page` lies on the interior of another route, or on another free end: the
    /// route it lies on.
    pub(super) fn under(&self, page: PageId, at: Pt, me: RouteId) -> Option<(RouteId, bool)> {
        let cell = cell_of(page, at)?;
        for (c, i) in self.segments.get(&cell).into_iter().flatten() {
            if *c == me {
                continue;
            }
            let Some(p) = self.placed.get(c) else { continue };
            let Some((_, _, a, b)) = p.segments.iter().find(|(pg, j, _, _)| *pg == page && j == i) else { continue };
            if near_segment(at, *a, *b) && !at.near(*a, ON_WIRE) && !at.near(*b, ON_WIRE) {
                return Some((*c, false));
            }
        }
        for (c, end) in self.ends.get(&cell).into_iter().flatten() {
            if *c == me {
                continue;
            }
            let Some(p) = self.placed.get(c) else { continue };
            if p.ends.iter().any(|(pg, e, x)| *pg == page && e == end && x.near(at, ON_WIRE)) {
                return Some((*c, true));
            }
        }
        None
    }
}

pub(super) fn near_segment(p: Pt, a: Pt, b: Pt) -> bool {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let q = Pt::new(a.x + t * dx, a.y + t * dy);
    q.is_finite() && q.near(p, ON_WIRE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cell_at_the_edge_of_the_range_has_no_neighbours_past_it() {
        let page = PageId(1);
        let far = cell_of(page, Pt::new(1e308, -1e308));
        assert_eq!(far, Some((page, i64::MAX, i64::MIN)));
        let cells: BTreeSet<Cell> = far.into_iter().collect();
        assert!(Grid::default().loose_near(&cells).is_empty());
    }
}
