//! Cells for laying out a page's wire labels: the page's line segments and the labels placed so
//! far, each kept under the cells its box covers, so trying a place for a label looks only at
//! what lies near it ([`crate::wires::labels`]). Built for one layout and dropped after it.

use std::collections::HashMap;

use ecad_model::Pt;

/// The side of a cell, in millimetres: a few label heights.
const CELL_MM: f64 = 10.0;
/// Most cells one box is filed under. A box past it (a hostile huge line) goes on the list every
/// query returns, so it costs one look per query instead of a page of cells.
const MAX_CELLS: i64 = 1_024;

/// An axis-aligned box `(lo, hi)`.
pub(crate) type Bbox = (Pt, Pt);

/// Items of type `T`, each filed under the cells its box covers.
#[derive(Debug, Clone)]
pub(crate) struct Grid<T> {
    items: Vec<T>,
    cells: HashMap<(i64, i64), Vec<usize>>,
    /// Items whose box covers more than [`MAX_CELLS`] cells or is not finite: in every answer.
    everywhere: Vec<usize>,
}

impl<T> Default for Grid<T> {
    fn default() -> Self {
        Grid { items: Vec::new(), cells: HashMap::new(), everywhere: Vec::new() }
    }
}

/// The cell range `(x0, y0, x1, y1)` box `b` covers, or `None` when it is not finite or covers
/// more than [`MAX_CELLS`] cells.
fn range(b: &Bbox) -> Option<(i64, i64, i64, i64)> {
    let (lo, hi) = b;
    if !(lo.is_finite() && hi.is_finite()) {
        return None;
    }
    // `as` saturates for numbers past the i64 range, so a far box still gets a (huge) range.
    let cell = |v: f64| (v / CELL_MM).floor() as i64;
    let (x0, y0, x1, y1) = (cell(lo.x.min(hi.x)), cell(lo.y.min(hi.y)), cell(lo.x.max(hi.x)), cell(lo.y.max(hi.y)));
    let count = x1.checked_sub(x0)?.checked_add(1)?.checked_mul(y1.checked_sub(y0)?.checked_add(1)?)?;
    (count <= MAX_CELLS).then_some((x0, y0, x1, y1))
}

impl<T> Grid<T> {
    /// File `item` under the cells box `b` covers.
    pub(crate) fn insert(&mut self, b: Bbox, item: T) {
        let i = self.items.len();
        self.items.push(item);
        match range(&b) {
            Some((x0, y0, x1, y1)) => {
                for x in x0..=x1 {
                    for y in y0..=y1 {
                        self.cells.entry((x, y)).or_default().push(i);
                    }
                }
            }
            None => self.everywhere.push(i),
        }
    }

    /// The items filed under any cell box `b` covers, each once: every item whose box meets `b`,
    /// and possibly a few more from the same cells. A box `b` too large to look up cell by cell
    /// gets every item.
    pub(crate) fn near(&self, b: &Bbox) -> Vec<&T> {
        let mut ix: Vec<usize> = match range(b) {
            Some((x0, y0, x1, y1)) => {
                let mut v = self.everywhere.clone();
                for x in x0..=x1 {
                    for y in y0..=y1 {
                        v.extend(self.cells.get(&(x, y)).into_iter().flatten().copied());
                    }
                }
                v
            }
            None => (0..self.items.len()).collect(),
        };
        ix.sort_unstable();
        ix.dedup();
        ix.into_iter().filter_map(|i| self.items.get(i)).collect()
    }
}

/// Box `b` grown by `by` on every side.
pub(crate) fn grown(b: &Bbox, by: f64) -> Bbox {
    (Pt::new(b.0.x - by, b.0.y - by), Pt::new(b.1.x + by, b.1.y + by))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> Bbox {
        (Pt::new(x0, y0), Pt::new(x1, y1))
    }

    /// A query returns the items near the box and none of the far ones, each once.
    #[test]
    fn a_query_returns_only_what_is_near_the_box() {
        let mut g = Grid::default();
        // A row of short lines 30 mm apart, and one line crossing three cells.
        for i in 0..20 {
            let x = f64::from(i) * 30.0;
            g.insert(bx(x, 0.0, x + 5.0, 0.0), i);
        }
        g.insert(bx(0.0, 50.0, 25.0, 50.0), 100);
        let mut near: Vec<i32> = g.near(&bx(58.0, -2.0, 66.0, 2.0)).into_iter().copied().collect();
        near.sort_unstable();
        assert_eq!(near, [2], "only the line at x 60");
        assert_eq!(g.near(&bx(12.0, 48.0, 13.0, 52.0)).into_iter().copied().collect::<Vec<_>>(), [100], "the long line once");
        assert!(g.near(&bx(1000.0, 1000.0, 1001.0, 1001.0)).is_empty());
    }

    /// Every item whose box meets the query box is returned, whatever cells they share.
    #[test]
    fn every_item_meeting_the_box_is_returned() {
        let mut g = Grid::default();
        let boxes: Vec<Bbox> =
            (0..200).map(f64::from).map(|i| bx(i * 3.7 % 97.0, i * 5.3 % 89.0, i * 3.7 % 97.0 + 4.0, i * 5.3 % 89.0 + 2.0)).collect();
        for (i, b) in boxes.iter().enumerate() {
            g.insert(*b, i);
        }
        let q = bx(40.0, 40.0, 52.0, 47.0);
        let meets = |b: &Bbox| b.0.x <= q.1.x && q.0.x <= b.1.x && b.0.y <= q.1.y && q.0.y <= b.1.y;
        let near: Vec<usize> = g.near(&q).into_iter().copied().collect();
        for (i, b) in boxes.iter().enumerate() {
            if meets(b) {
                assert!(near.contains(&i), "box {i} meets the query");
            }
        }
        assert!(near.len() < boxes.len() / 2, "a query looks at a part of the page: {}", near.len());
    }

    /// A huge or broken box never fills a page of cells: it is in every answer instead.
    #[test]
    fn a_huge_or_broken_box_is_in_every_answer() {
        let mut g = Grid::default();
        g.insert(bx(-1e300, -1e300, 1e300, 1e300), "huge");
        g.insert(bx(f64::NAN, 0.0, 1.0, 1.0), "broken");
        g.insert(bx(0.0, 0.0, 1.0, 1.0), "small");
        assert!(g.cells.values().map(Vec::len).sum::<usize>() < 10);
        let mut near: Vec<&str> = g.near(&bx(500.0, 500.0, 501.0, 501.0)).into_iter().copied().collect();
        near.sort_unstable();
        assert_eq!(near, ["broken", "huge"]);
        assert_eq!(g.near(&bx(-1e300, -1e300, 1e300, 1e300)).len(), 3, "a huge query gets everything");
    }
}
