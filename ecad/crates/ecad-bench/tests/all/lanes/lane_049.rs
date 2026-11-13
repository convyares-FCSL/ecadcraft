//! Brief 049 bench rows (design 049 §9), timed on a project of its own: pages with continuation
//! arrows of 20 per name (a fifth captioned), and one name with many arrows. The shared generator
//! is not changed. Debug runs (the fast tests) use smaller counts and check only that the work is
//! scoped; `cargo test --release -p ecad-bench --test all lane_049 -- --nocapture` prints the rows.
//!
//! The per-commit rows time the continuation work a commit does (`continuations::names_named`,
//! `shown` before and after, and refreshing the name group), not a whole commit: this crate's
//! tests run in verify mode, which rebuilds the whole index after every commit.

use std::collections::BTreeSet;
use std::time::Instant;

use ecad_cadcraft::continuations;
use ecad_cadcraft::xref::{Places, RefIndex};
use ecad_model::{Color, Document, IdAllocator, ItemKind, Op, PageId, Placement, Pt, Style};

/// Pages, names of 20 arrows, and the size of the big name.
fn counts() -> (usize, usize, usize) {
    if cfg!(debug_assertions) { (10, 5, 200) } else { (100, 50, 2000) }
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v.get(v.len() / 2).copied().unwrap_or(0.0)
}

fn time(runs: usize, mut f: impl FnMut()) -> f64 {
    median(
        (0..runs)
            .map(|_| {
                let t = Instant::now();
                f();
                ms(t)
            })
            .collect(),
    )
}

/// A document with `pages` pages and no arrows.
fn pages(n: usize) -> Document {
    let mut doc = Document::default();
    let layer = ecad_model::LayerId(1);
    Op::AddLayer { id: layer, layer: ecad_model::Layer { name: "0".into() } }.apply(&mut doc).unwrap();
    for i in 0..n {
        let id = PageId(0x1000 + i as u128);
        Op::AddPage { id, page: ecad_model::Page::new(format!("{}", 100 + i)), index: i }.apply(&mut doc).unwrap();
    }
    doc
}

/// Add an arrow named `name` at `at` on `page` (with a caption when given).
fn arrow(doc: &mut Document, ids: &mut IdAllocator, page: PageId, at: Pt, name: &str, caption: Option<&str>) {
    let mut props = ecad_model::Properties::new();
    props.insert("kind".into(), "continuation".into());
    props.insert("name".into(), name.into());
    if let Some(c) = caption {
        props.insert("caption".into(), c.into());
    }
    let style = Style { layer: ecad_model::LayerId(1), color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
    let starter = ecad_cadcraft::wires::continuation_starter().unwrap();
    let pl = Placement { origin: at, rotation: 0.0, mirror: false, scale: 1.0 };
    let t = ecad_cadcraft::wires::plan_tagless(doc, ids, page, style, starter, pl, props).unwrap();
    for op in &t.ops {
        op.apply(doc).unwrap();
    }
}

/// The ops moving the symbol item of the first arrow named `name` by `dx`.
fn move_first(doc: &Document, name: &str, dx: f64) -> Vec<Op> {
    let c = doc.continuations_named(name)[0];
    let (_, r) = doc.representations_of(ecad_model::Subject::Component(c)).next().unwrap();
    let id = r.items[0];
    let before = doc.items[&id].clone();
    let mut after = before.clone();
    if let ItemKind::Symbol(s) = &mut after.kind {
        s.placement.origin = Pt::new(s.placement.origin.x + dx, s.placement.origin.y);
    }
    vec![Op::ModifyItem { id, before, after }]
}

/// One commit's continuation work for `ops` on `doc`: the names, what they show before and after,
/// and the groups refreshed. Returns the pages whose parts changed.
fn commit_work(doc: &Document, idx: &mut RefIndex, ops: &[Op]) -> (Document, BTreeSet<PageId>) {
    let mut names = continuations::names_named(doc, ops);
    let mut groups = RefIndex::groups_named(doc, ops);
    let before = continuations::shown(doc, &mut Places::new(doc), &names);
    let mut after_doc = doc.clone();
    for op in ops {
        op.apply(&mut after_doc).unwrap();
    }
    names.extend(continuations::names_named(&after_doc, ops));
    groups.extend(RefIndex::groups_named(&after_doc, ops));
    idx.refresh(&after_doc, &groups);
    let after = continuations::shown(&after_doc, &mut Places::new(&after_doc), &names);
    let pages = before.keys().chain(after.keys()).filter(|p| before.get(*p) != after.get(*p)).copied().collect();
    (after_doc, pages)
}

#[test]
fn bench_049_continuation_rows() {
    let (n_pages, n_names, big) = counts();
    let mut doc = pages(n_pages);
    let order = doc.page_order.clone();
    let runs = if cfg!(debug_assertions) { 3 } else { 9 };
    let build_plain = time(runs, || drop(RefIndex::build(&doc)));
    let mut ids = IdAllocator::seeded(0x049);
    // 20 arrows per name, spread over the pages; a fifth captioned, in runs of adjacent arrows.
    for k in 0..n_names {
        for j in 0..20 {
            let page = order[(k * 7 + j * 3) % order.len()];
            let at = Pt::new(20.0 + (j % 10) as f64 * 5.0 + (k % 5) as f64 * 60.0, 50.0 + (k / 5 % 4) as f64 * 40.0);
            let caption = (j % 5 == 0).then_some("TO CONTROL SUPPLY");
            arrow(&mut doc, &mut ids, page, at, &format!("N{k}"), caption);
        }
    }
    // The page projected: 20 arrows of 5 names, 4 caption runs.
    let shown_page = order[order.len() / 2];
    for k in 0..5 {
        for j in 0..4 {
            let caption = (j == 0 && k < 4).then_some("TO CONTROL SUPPLY");
            arrow(&mut doc, &mut ids, shown_page, Pt::new(30.0 + k as f64 * 60.0 + j as f64 * 5.0, 150.0), &format!("N{k}"), caption);
        }
    }
    let build_with = time(runs, || drop(RefIndex::build(&doc)));
    let mut idx = RefIndex::build(&doc);
    let project_page = time(runs, || drop(continuations::parts_on_page(&doc, &mut Places::new(&doc), shown_page)));
    let page_parts = continuations::parts_on_page(&doc, &mut Places::new(&doc), shown_page);
    let all_parts = time(runs, || drop(ecad_cadcraft::xref::parts_on_page(&doc, shown_page)));

    // Move one arrow of a 20-arrow name.
    let ops = move_first(&doc, "N3", 5.0);
    let mut redrawn = BTreeSet::new();
    let move20 = time(runs, || {
        let (_, p) = commit_work(&doc, &mut idx.clone(), &ops);
        redrawn = p;
    });
    let partners: BTreeSet<PageId> =
        doc.continuations_named("N3").iter().flat_map(|c| doc.representations_of(ecad_model::Subject::Component(*c))).map(|(_, r)| r.page).collect();
    assert!(redrawn.is_subset(&partners), "only pages with an N3 arrow: {redrawn:?}");

    // Rename one arrow from N3 to N4.
    let c = doc.continuations_named("N3")[1];
    let rename = vec![Op::SetProperty {
        object: ecad_model::ObjectRef::Component(c),
        key: "name".into(),
        before: Some("N3".into()),
        after: Some("N4".into()),
    }];
    let rename20 = time(runs, || drop(commit_work(&doc, &mut idx.clone(), &rename)));

    // A name with many arrows; move one.
    for i in 0..big {
        let page = order[i % order.len()];
        arrow(&mut doc, &mut ids, page, Pt::new(20.0 + (i / order.len() % 40) as f64 * 8.0, 230.0), "BIG", None);
    }
    idx = RefIndex::build(&doc);
    let ops = move_first(&doc, "BIG", 5.0);
    let move_big = time(runs, || drop(commit_work(&doc, &mut idx.clone(), &ops)));
    let (after, _) = commit_work(&doc, &mut idx, &ops);
    assert_eq!(idx, RefIndex::build(&after), "the kept index equals a rebuild");

    eprintln!(
        "bench 049 ({n_pages} pages, {} arrows, big name {big}; median of {runs}, ms): index build without arrows {build_plain:.3}, with {build_with:.3}; \
         page with 20 arrows / 5 names / 4 runs: continuation parts {project_page:.3} ({} parts), all reference parts {all_parts:.3}; \
         move one of 20 {move20:.3} (pages {}); rename one of 20 {rename20:.3}; move one of {big} {move_big:.3}",
        n_names * 20 + 20 + big,
        page_parts.len(),
        redrawn.len(),
    );
    assert_eq!(page_parts.iter().filter(|p| p.index == 1).count(), 4, "four caption runs on the page");
}
