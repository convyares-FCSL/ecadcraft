//! Tester probes for lane 072 D step F2c (design 072 §4.2–§4.4): wires point to point on routes,
//! drawing order, the T-join rule, junctions not stored, and CADCraft edits (erase, undo, save)
//! becoming the right transaction. Driven through the commands' JSON forms.

use std::collections::BTreeSet;

use ecad_model::{ConnectionPointId, NetId};
use serde_json::{Value, json};

use crate::common::{H, id, tmp_dir};

const SEED: u64 = 0x72_d7;

/// Every wire as (net, net_order, {ends}), sorted by net then order.
fn wires(h: &mut H) -> Vec<(NetId, u16, BTreeSet<ConnectionPointId>)> {
    let doc = h.doc();
    let mut v: Vec<_> = doc.wires().values().map(|w| (w.net, w.net_order, w.ends.iter().copied().collect())).collect();
    v.sort_by_key(|(n, o, _)| (*n, *o));
    v
}

fn pair(a: ConnectionPointId, b: ConnectionPointId) -> BTreeSet<ConnectionPointId> {
    [a, b].into_iter().collect()
}

/// The ends of the wire with `net_order` `n` (only one net expected).
fn nth(h: &mut H, n: u16) -> BTreeSet<ConnectionPointId> {
    wires(h).into_iter().find(|(_, o, _)| *o == n).map(|(_, _, e)| e).unwrap_or_else(|| panic!("no wire {n}"))
}

fn wire(h: &mut H, pts: &[(f64, f64)]) -> Value {
    h.run("wire", json!({"points": crate::common::pts(pts)})).unwrap_or_else(|e| panic!("wire {pts:?}: {e}"))
}

/// P1. Drawing order: B–C, then A→B, then C→D give 1 = B–C, 2 = B–A, 3 = C–D on one net; every
/// wire has its own conductor id and runs between two connection points.
#[test]
fn p1_wires_follow_drawing_order_point_to_point() {
    let mut h = H::new(SEED);
    let p = h.points_at(&[(0.0, 0.0), (50.0, 0.0), (100.0, 0.0), (150.0, 0.0)]);
    let (a, b, c, d) = (p[0], p[1], p[2], p[3]);
    wire(&mut h, &[(50.0, 0.0), (100.0, 0.0)]);
    wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)]);
    wire(&mut h, &[(100.0, 0.0), (150.0, 0.0)]);
    let w = wires(&mut h);
    assert_eq!(w.len(), 3, "{w:?}");
    assert!(w.iter().all(|(n, _, _)| *n == w[0].0), "one net: {w:?}");
    assert_eq!(w.iter().map(|(_, o, _)| *o).collect::<Vec<_>>(), vec![1, 2, 3]);
    assert_eq!(nth(&mut h, 1), pair(b, c));
    assert_eq!(nth(&mut h, 2), pair(a, b));
    assert_eq!(nth(&mut h, 3), pair(c, d));
    let doc = h.doc();
    let conductors: BTreeSet<_> = doc.wires().values().map(|w| w.conductor).collect();
    assert_eq!(conductors.len(), 3, "distinct conductor ids");
    assert!(doc.wires().values().all(|w| w.ends[0] != w.ends[1]));
    h.check();
}

/// P2. T-join (overseer ruling 2026-11-17): onto A–B near A → A–C; near A again with A full → the
/// far end, B–D; with both full → the nearer end anyway, A–E. No junction device is made.
#[test]
fn p2_tee_joins_nearer_end_then_far_end_then_nearer_anyway() {
    let mut h = H::new(SEED + 1);
    let p = h.points_at(&[(0.0, 0.0), (100.0, 0.0), (20.0, 40.0), (30.0, -40.0), (40.0, 40.0)]);
    let (a, b, c, d, e) = (p[0], p[1], p[2], p[3], p[4]);
    let devices = h.doc().devices().len();
    wire(&mut h, &[(0.0, 0.0), (100.0, 0.0)]);
    let r = wire(&mut h, &[(20.0, 40.0), (20.0, 0.0)]);
    assert_eq!(r["joined"][0]["how"], json!("on"), "a T: {r}");
    assert_eq!(nth(&mut h, 2), pair(a, c), "nearer end A");
    wire(&mut h, &[(30.0, -40.0), (30.0, 0.0)]);
    assert_eq!(nth(&mut h, 3), pair(b, d), "A full → far end B");
    wire(&mut h, &[(40.0, 40.0), (40.0, 0.0)]);
    assert_eq!(nth(&mut h, 4), pair(a, e), "both full → nearer end A anyway");
    let w = wires(&mut h);
    assert!(w.iter().all(|(n, _, _)| *n == w[0].0), "one net: {w:?}");
    let doc = h.doc();
    assert_eq!(doc.devices().len(), devices, "no junction device stored");
    h.check();
}

/// P3. A point that leaves is cut out: routes A–X, X–B and B–A (a loop is allowed); erasing both of
/// X's lines in CADCraft leaves one wire A–B, net_order 1, same net id. One undo puts both back.
#[test]
fn p3_erasing_a_points_lines_cuts_it_out_and_undo_restores() {
    let mut h = H::new(SEED + 2);
    let page = h.first_page();
    let p = h.points_at(&[(0.0, 0.0), (50.0, 0.0), (100.0, 0.0)]);
    let (a, x, b) = (p[0], p[1], p[2]);
    let r1 = id(&wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)]), "route");
    let r2 = id(&wire(&mut h, &[(50.0, 0.0), (100.0, 0.0)]), "route");
    wire(&mut h, &[(100.0, 0.0), (100.0, 30.0), (0.0, 30.0), (0.0, 0.0)]);
    let before = wires(&mut h);
    assert_eq!(before.len(), 2, "A–X, X–B; the loop adds no point: {before:?}");
    let net = before[0].0;
    let doc_before = h.doc();
    let hs: Vec<String> = [r1, r2].into_iter().map(|r| h.line_handle(page, r).hex()).collect();
    let head = h.head();
    h.run("erase", json!({"handles": hs})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    let after = wires(&mut h);
    assert_eq!(after, vec![(net, 1, pair(a, b))], "X cut out, A–B joined, net kept");
    let _ = x;
    h.check();
    h.cmdline("u");
    assert!(h.doc() == doc_before, "undo restores");
    h.check();
}

/// P4. Two nets joined by a line keep the id of the one with more wires; erasing the line splits
/// them again and the bigger keeps it.
#[test]
fn p4_net_ids_follow_join_and_split() {
    let mut h = H::new(SEED + 3);
    let page = h.first_page();
    let p = h.points_at(&[(0.0, 0.0), (50.0, 0.0), (0.0, 50.0), (50.0, 50.0), (100.0, 50.0)]);
    wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)]);
    wire(&mut h, &[(0.0, 50.0), (50.0, 50.0)]);
    wire(&mut h, &[(50.0, 50.0), (100.0, 50.0)]);
    let doc = h.doc();
    let small = doc.wires().values().find(|w| w.ends.contains(&p[0])).unwrap().net;
    let big = doc.wires().values().find(|w| w.ends.contains(&p[4])).unwrap().net;
    assert_ne!(small, big);
    let j = id(&wire(&mut h, &[(50.0, 0.0), (50.0, 50.0)]), "route");
    let w = wires(&mut h);
    assert_eq!(w.len(), 4, "{w:?}");
    assert!(w.iter().all(|(n, _, _)| *n == big), "bigger net's id kept: {w:?}");
    let hd = h.line_handle(page, j).hex();
    h.run("erase", json!({"handles": [hd]})).unwrap();
    let doc = h.doc();
    let nb = doc.wires().values().find(|w| w.ends.contains(&p[4])).unwrap().net;
    let ns = doc.wires().values().find(|w| w.ends.contains(&p[0])).unwrap().net;
    assert_eq!(nb, big, "bigger part keeps the id");
    assert_ne!(ns, big, "smaller part gets another id");
    h.check();
}

/// P5. A T onto a line of another potential is refused, nothing written.
#[test]
fn p5_a_tee_onto_another_potential_is_refused() {
    let mut h = H::new(SEED + 4);
    h.points_at(&[(0.0, 0.0), (100.0, 0.0), (40.0, 40.0)]);
    h.run("wire", json!({"points": [[0, 0], [100, 0]], "potential": "0V"})).unwrap();
    let head = h.head();
    let e = h.run("wire", json!({"points": [[40, 40], [40, 0]], "potential": "24V"})).unwrap_err();
    assert!(e.contains("0V") && e.contains("24V"), "{e}");
    assert_eq!(h.head(), head, "nothing written");
}

/// P6. Save, reopen: the wires (net, conductor, order, ends) and routes come back the same.
#[test]
fn p6_save_and_reopen_keep_wires_and_routes() {
    ecad_engine::install();
    ecad_engine::cables::views::isolate_for_tests();
    let mut s = cadcraft_engine::Session::empty();
    let project = ecad_engine::Project::create_with(&mut s, "P6", SEED + 5, ecad_engine::Persistence::File).unwrap();
    let mut h = H { s, project, trail: Vec::new() };
    let d0 = h.doc();
    h.trail.push(d0);
    h.points_at(&[(0.0, 0.0), (100.0, 0.0), (20.0, 40.0)]);
    wire(&mut h, &[(0.0, 0.0), (100.0, 0.0)]);
    wire(&mut h, &[(20.0, 40.0), (20.0, 0.0)]);
    let dir = tmp_dir("072d tester p6");
    let path = dir.join("p6.ecad").to_string_lossy().to_string();
    h.run("saveas", json!({"path": path})).unwrap();
    let (w0, r0) = {
        let d = h.doc();
        (d.wires().clone(), d.routes().clone())
    };
    h.close_all();
    let mut h2 = crate::common::reopen(&path, SEED + 6);
    let d = h2.doc();
    assert!(*d.wires() == w0, "wires round trip");
    assert!(*d.routes() == r0, "routes round trip");
    h2.check();
    let _ = std::fs::remove_dir_all(dir);
}
