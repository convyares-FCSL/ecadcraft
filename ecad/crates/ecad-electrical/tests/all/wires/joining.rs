//! Wire planning, joining and the label rule (formerly the inline tests of `wires`).

use ecad_electrical::wires::{joining::*, routes::*};
use ecad_model::{Color, Document, End, IdAllocator, Item, ItemKind, Layer, LayerId, Op, Page, PageId, Primitive, Pt, RouteEnd, RouteId, Style};

/// A document with one page and one layer, and the style to draw on it.
fn page_doc() -> (Document, PageId, Style) {
    let mut doc = Document::default();
    let (page, layer) = (PageId::from(1u128), LayerId::from(2u128));
    for op in [Op::AddLayer { id: layer, layer: Layer { name: "0".into() } }, Op::AddPage { id: page, page: Page::titled("1").unwrap(), index: 0 }] {
        op.apply(&mut doc).expect("setup");
    }
    doc.rebuild_index();
    (doc, page, Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 })
}

fn draw(doc: &mut Document, ids: &mut IdAllocator, page: PageId, style: &Style, pts: &[(f64, f64)]) -> WirePlan {
    let input = WireInput {
        page,
        style: style.clone(),
        points: pts.iter().map(|(x, y)| Pt::new(*x, *y)).collect(),
        potential: None,
        join: true,
        tol: 0.1,
        quiet: &[],
        max_points: 100,
    };
    let plan = plan_wire(doc, ids, input).expect("plan");
    for op in &plan.ops {
        op.apply(doc).expect("apply");
    }
    doc.rebuild_index();
    plan
}

/// The item drawing route `r` has the route's points.
fn drawn_as_routed(doc: &Document, r: RouteId) -> bool {
    matches!(route_vertices(doc, r), Some((_, Item { kind: ItemKind::Primitive(Primitive::Polyline { vertices, .. }), .. }, v)) if vertices == v)
}

#[test]
fn a_line_is_a_route_drawn_by_one_polyline() {
    let (mut doc, page, style) = page_doc();
    let mut ids = IdAllocator::seeded(7);
    let plan = draw(&mut doc, &mut ids, page, &style, &[(0.0, 0.0), (10.0, 0.0)]);
    assert!(plan.created);
    let route = doc.routes().get(&plan.route).expect("route");
    assert_eq!(route.ends, [RouteEnd::Free, RouteEnd::Free]);
    assert!(drawn_as_routed(&doc, plan.route));
    assert!(doc.wires().is_empty(), "no connection points: no wire");
}

#[test]
fn a_line_from_a_free_end_extends_that_route() {
    let (mut doc, page, style) = page_doc();
    let mut ids = IdAllocator::seeded(7);
    let first = draw(&mut doc, &mut ids, page, &style, &[(0.0, 0.0), (10.0, 0.0)]);
    let second = draw(&mut doc, &mut ids, page, &style, &[(10.0, 0.0), (20.0, 0.0)]);
    assert!(!second.created);
    assert_eq!(second.route, first.route);
    assert_eq!(doc.routes().len(), 1);
    assert_eq!(doc.routes().get(&first.route).map(|r| r.points.clone()), Some(vec![Pt::new(0.0, 0.0), Pt::new(20.0, 0.0)]), "one straight line");
    assert!(drawn_as_routed(&doc, first.route));
}

#[test]
fn a_line_between_two_free_ends_makes_one_route_of_three() {
    let (mut doc, page, style) = page_doc();
    let mut ids = IdAllocator::seeded(7);
    let a = draw(&mut doc, &mut ids, page, &style, &[(0.0, 0.0), (10.0, 0.0)]);
    let b = draw(&mut doc, &mut ids, page, &style, &[(20.0, 10.0), (20.0, 20.0)]);
    let c = draw(&mut doc, &mut ids, page, &style, &[(10.0, 0.0), (20.0, 0.0), (20.0, 10.0)]);
    assert_eq!(c.route, a.route);
    assert_eq!(c.removed, vec![b.route]);
    assert_eq!(doc.routes().len(), 1);
    assert_eq!(doc.routes().get(&a.route).map(|r| r.points.len()), Some(3));
    assert!(drawn_as_routed(&doc, a.route));
    assert!(route_item(&doc, b.route).is_none(), "the merged route's drawing goes with it");
}

#[test]
fn a_line_ending_on_a_route_is_a_tee_and_a_free_end_on_a_new_line_lands_on_it() {
    let (mut doc, page, style) = page_doc();
    let mut ids = IdAllocator::seeded(7);
    let main = draw(&mut doc, &mut ids, page, &style, &[(0.0, 0.0), (20.0, 0.0)]);
    let tee = draw(&mut doc, &mut ids, page, &style, &[(5.0, 10.0), (5.0, 0.0)]);
    assert_eq!(doc.routes().get(&tee.route).map(|r| r.ends[1]), Some(RouteEnd::On { route: main.route, at: Pt::new(5.0, 0.0) }));
    assert!(doc.devices().is_empty(), "no junction component");
    // Reverse T: a free end lying on a new line's middle lands on it.
    let stub = draw(&mut doc, &mut ids, page, &style, &[(40.0, 10.0), (40.0, 0.0)]);
    let across = draw(&mut doc, &mut ids, page, &style, &[(30.0, 0.0), (50.0, 0.0)]);
    assert_eq!(across.tees, vec![(Pt::new(40.0, 0.0), stub.route)]);
    assert_eq!(doc.routes().get(&stub.route).map(|r| r.ends[1]), Some(RouteEnd::On { route: across.route, at: Pt::new(40.0, 0.0) }));
}

#[test]
fn joining_two_free_ends_merges_and_tees_follow_the_survivor() {
    let (mut doc, page, style) = page_doc();
    let mut ids = IdAllocator::seeded(7);
    let a = draw(&mut doc, &mut ids, page, &style, &[(0.0, 0.0), (10.0, 0.0)]);
    let input = WireInput {
        page,
        style: style.clone(),
        points: vec![Pt::new(10.0, 0.0), Pt::new(20.0, 0.0)],
        potential: None,
        join: false,
        tol: 0.1,
        quiet: &[],
        max_points: 100,
    };
    let b = plan_wire(&doc, &mut ids, input).expect("plan");
    for op in &b.ops {
        op.apply(&mut doc).expect("apply");
    }
    doc.rebuild_index();
    let tee = draw(&mut doc, &mut ids, page, &style, &[(15.0, 10.0), (15.0, 0.0)]);
    let ends = [(a.route, End::B), (b.route, End::A)];
    let j = plan_end_join(&doc, ends[0], ends[1], 100).expect("join").expect("merge");
    for op in &j.ops {
        op.apply(&mut doc).expect("apply");
    }
    doc.rebuild_index();
    assert_eq!(doc.routes().len(), 2);
    let survivor = j.survivor;
    assert_eq!(doc.routes().get(&tee.route).and_then(|r| r.ends[1].route()), Some(survivor));
    assert!(drawn_as_routed(&doc, survivor));
}
