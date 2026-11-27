//! The page and folder order diff costs O(n log n) time and O(n) memory: a damaged or hostile file with a huge
//! page order (verification compares a stored snapshot with replay through this diff) never allocates n × m.

use ecad_model::document::diff::ops_between;
use ecad_model::{Document, Op, Page, PageId, PageTitle};

/// A document whose pages are `order`, in that order.
fn with_pages(order: &[u128]) -> Document {
    let mut doc = Document::default();
    let page = Page::new(PageTitle::new("P").unwrap());
    for (index, id) in order.iter().enumerate() {
        Op::AddPage { id: PageId(*id), page: page.clone(), index }.apply(&mut doc).unwrap();
    }
    doc
}

/// Diff `from` into `to`, check the ops get there, and return how many pages moved.
fn moves(from: &[u128], to: &[u128]) -> usize {
    let (a, b) = (with_pages(from), with_pages(to));
    let ops = ops_between(&a, &b, &|_| true);
    let mut done = a.clone();
    for op in &ops {
        op.apply(&mut done).unwrap();
    }
    assert_eq!(done.page_order(), b.page_order());
    ops.iter().filter(|o| matches!(o, Op::RemovePage { .. })).count()
}

#[test]
fn a_huge_page_order_diffs_without_an_n_by_m_table() {
    // 50,000 pages: an n × m table of `u32` would need 10 GB.
    let from: Vec<u128> = (1..=50_000).collect();
    let mut to = from.clone();
    let last = to.len() - 1;
    to.swap(0, last);
    // The first and last pages swap places: two pages move, every other page keeps its place.
    assert_eq!(moves(&from, &to), 2);
}

#[test]
fn the_kept_run_is_the_longest_one() {
    // 1 2 3 4 5 6 -> 4 1 2 5 3 6: the longest kept run is 1 2 5 6 (or 1 2 3 6), so two pages move.
    assert_eq!(moves(&[1, 2, 3, 4, 5, 6], &[4, 1, 2, 5, 3, 6]), 2);
    assert_eq!(moves(&[1, 2, 3], &[3, 2, 1]), 2);
    assert_eq!(moves(&[1, 2, 3], &[1, 2, 3]), 0);
}
