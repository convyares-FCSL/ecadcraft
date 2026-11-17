//! Brief 041 (U4b): what is under the cursor, and the running tool's intent. The action table's
//! own tests moved to ecad-app with the table (design 065 §3 cut 13).

use crate::common::{H, id};

use cadcraft_engine::geom::Vec2;
use ecad_cadcraft::interact::{intent, kind};
use ecad_cadcraft::kinds::ObjectKind;
use ecad_model::{ComponentId, End, PageId, WireId};
use serde_json::json;

// ---------------------------------------------------------------- what is under the cursor

fn new_h() -> H {
    let mut h = H::new(41);
    h.s.viewport_px = (1280.0, 800.0);
    h
}

fn uid(h: &H) -> u64 {
    h.s.state().unwrap().uid
}

fn classify(h: &mut H, at: (f64, f64), picked: Option<cadcraft_engine::doc::Handle>, selection: &[cadcraft_engine::doc::Handle]) -> kind::Target {
    let u = uid(h);
    let sel = selection.to_vec();
    h.p(|p| kind::classify(p, u, Vec2::new(at.0, at.1), 0.5, picked, &sel, false))
}

/// `-K01`'s coil at (0, 0) and a wire from its A1 to the right.
fn coil_and_wire() -> (H, PageId, ComponentId, WireId) {
    let mut h = new_h();
    let page = h.first_page();
    let k1: ComponentId = id(&h.run("place", json!({"symbol": "coil", "at": [0, 0]})).unwrap(), "component");
    let doc = h.doc();
    let a1 = doc.terminals_of(k1).find(|(_, t)| t.label.as_deref() == Some("A1")).map(|(t, _)| t).unwrap();
    let at = h.terminal_pos(page, a1);
    let w: WireId = id(&h.run("wire", json!({"points": [[at.x, at.y], [at.x + 60.0, at.y]]})).unwrap(), "conductor");
    (h, page, k1, w)
}

#[test]
fn the_coil_is_a_device_named_by_its_tag() {
    let (mut h, page, k1, _) = coil_and_wire();
    let item = h.symbol_item(k1);
    let handle = h.handle_of(page, item);
    let t = classify(&mut h, (0.0, 0.0), Some(handle), &[]);
    assert_eq!(t.kind, ObjectKind::Device);
    assert_eq!(t.component, Some(k1));
    assert_eq!(t.name, "-K01");
    assert!(t.handles.contains(&handle));
    assert_eq!(t.others, 0);
}

#[test]
fn a_wire_its_end_and_empty_paper() {
    let (mut h, page, k1, w) = coil_and_wire();
    let (a, b) = h.route_ends(page, w);
    let route = h.route_handle(page, w);
    let mid = ((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let t = classify(&mut h, mid, Some(route), &[]);
    assert_eq!((t.kind, t.conductor), (ObjectKind::Wire, Some(w)));
    let t = classify(&mut h, (b.x, b.y), Some(route), &[]);
    assert_eq!((t.kind, t.conductor, t.end), (ObjectKind::WireEnd, Some(w), Some(End::B)));
    let t = classify(&mut h, (500.0, 500.0), None, &[]);
    assert_eq!(t.kind, ObjectKind::EmptyPage);
    assert!(t.name.starts_with("Page "), "{}", t.name);
    // A selection of two that holds the picked object is mixed.
    let item = h.symbol_item(k1);
    let coil = h.handle_of(page, item);
    let t = classify(&mut h, mid, Some(route), &[route, coil]);
    assert_eq!((t.kind, t.handles.len()), (ObjectKind::Mixed, 2));
}

#[test]
fn wire_intent_names_the_connection_point_under_the_cursor() {
    let (mut h, page, k1, w) = coil_and_wire();
    let doc = h.doc();
    let a2 = doc.terminals_of(k1).find(|(_, t)| t.label.as_deref() == Some("A2")).map(|(t, _)| t).unwrap();
    let a1 = doc.terminals_of(k1).find(|(_, t)| t.label.as_deref() == Some("A1")).map(|(t, _)| t).unwrap();
    let (p1, p2) = (h.terminal_pos(page, a1), h.terminal_pos(page, a2));
    let u = uid(&h);
    let s = h.p(|p| intent::sentence(p, u, "wire", Vec2::new(p2.x, p2.y), 0.5, None));
    assert_eq!(s.as_deref(), Some("joins A2 of -K01"));
    let wire = kind::wire_words(&doc, w);
    assert!(wire.starts_with("wire "), "{wire}");
    let s = h.p(|p| intent::sentence(p, u, "wire", Vec2::new(p1.x, p1.y), 0.5, None));
    assert_eq!(s, Some(format!("joins A1 of -K01 to {wire}")));
    let s = h.p(|p| intent::sentence(p, u, "line", Vec2::new(p1.x, p1.y), 0.5, None));
    assert_eq!(s, None, "other tools show their prompt");
}
