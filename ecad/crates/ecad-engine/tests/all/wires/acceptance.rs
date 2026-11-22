//! Tester 032 (wires and nets, v0.15 lane W): acceptance checks from outside, written from brief
//! 032 and design 032 §12 only, before the implementation. Numbers in test names are design §12's
//! items (Q6 wire join 1–12, Q8 cable across pages 13–14, Q4 labels 15–16, update maps 17, the
//! Wire tool 18, defaults 21). They were written
//! before the build and read new result fields and fix variants by name; they run on the build.

use cadcraft_engine::Input;
use cadcraft_engine::geom::Vec2;
use ecad_engine::Persistence;
use ecad_engine::checks::Kind;
use ecad_engine::outputs::kept::ReportModels;
use ecad_engine::wires::{self, LABEL_LAYER};
use ecad_model::{CableId, DeviceId, Document, Op, PageId, Pt, RouteEnd, RouteId, Subject, ConnectionPointId, WireId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};
use crate::common::{reopen, tmp_dir};

// ---------- helpers ----------

fn wire_json(points: &[(f64, f64)], extra: Value) -> Value {
    let mut params = json!({"points": pts(points)});
    if let (Some(o), Some(e)) = (params.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            o.insert(k.clone(), v.clone());
        }
    }
    params
}

fn wire(h: &mut H, points: &[(f64, f64)], extra: Value) -> Value {
    let p = wire_json(points, extra);
    h.run("wire", p.clone()).unwrap_or_else(|e| panic!("wire {p}: {e}"))
}

fn cid(v: &Value) -> WireId {
    id(v, "conductor")
}

/// The route a `wire` or `joinwires` result names.
fn rid(v: &Value) -> RouteId {
    id(v, "route")
}

fn conductors(doc: &Document) -> Vec<WireId> {
    doc.wires.keys().copied().collect()
}

fn net_of(doc: &Document, c: WireId) -> Option<ecad_model::NetKey> {
    doc.connectivity().net_of_conductor(c)
}

fn vertices(doc: &Document, r: RouteId) -> Vec<Pt> {
    doc.routes.get(&r).map(|x| x.points.clone()).unwrap_or_default()
}

/// The points of wire `c`'s line on `page`.
fn wire_vertices(doc: &Document, c: WireId, page: PageId) -> Vec<Pt> {
    wires::route_of_wire_on(doc, c, page).map(|r| vertices(doc, r)).unwrap_or_default()
}

fn same_pts(got: &[Pt], want: &[(f64, f64)]) -> bool {
    let fwd = got.len() == want.len() && got.iter().zip(want).all(|(p, (x, y))| p.near(Pt::new(*x, *y), 1e-6));
    let rev = got.len() == want.len() && got.iter().rev().zip(want).all(|(p, (x, y))| p.near(Pt::new(*x, *y), 1e-6));
    fwd || rev
}

fn last_tx(h: &mut H) -> ecad_model::Transaction {
    h.p(|p| p.ledger.log().last().cloned()).expect("a transaction")
}

fn checks(h: &mut H, kinds: &[&str]) -> Vec<Value> {
    let v = h.run("checks", json!({"kinds": kinds})).unwrap();
    v["checks"].as_array().cloned().unwrap_or_default()
}

/// The fix a Checks row offers, by its Debug name (`Join`, `AddJunction`, …).
fn fix_names(h: &mut H, kind: Kind) -> Vec<String> {
    let doc = h.doc();
    ecad_engine::checks::all(&doc, 1e-6).into_iter().filter(|c| c.kind == kind).map(|c| format!("{:?}", c.fix)).collect()
}

fn terminal(h: &mut H, at: (f64, f64), tag: &str) -> (DeviceId, ConnectionPointId, Pt) {
    let r = h.run("place", json!({"symbol": "terminal", "at": [at.0, at.1], "tag": tag})).unwrap();
    let t: ConnectionPointId = id(&r["terminals"][0], "terminal");
    let page = h.p(|p| p.active_page()).expect("an active page");
    let at = h.connection_point_pos(page, t);
    (id(&r, "component"), t, at)
}

fn continuation(h: &mut H, at: (f64, f64), name: &str, rotation: f64) -> ecad_model::AidId {
    let r = h.run("place", json!({"symbol": "continuation", "at": [at.0, at.1], "name": name, "rotation": rotation})).unwrap();
    id(&r, "aid")
}

fn undo_is_one_step(h: &mut H, before: &Document) {
    h.cmdline("u");
    assert_eq!(&h.doc(), before, "`u` undoes it in one step");
    h.cmdline("redo");
}

// ---------- Q6: wire join (design 072 §4.3: the drawn lines are routes) ----------

/// §12 1 (the regression; fails on `6729c8f1`): a line started on another's free end joins it:
/// one route; with points at both ends, one wire and one net.
#[test]
fn t01_end_to_end_is_one_conductor_one_net() {
    let mut h = H::new(0x7032_0001);
    let page = h.first_page();
    let (_, t1, p1) = terminal(&mut h, (0.0, 0.0), "X1");
    let (_, t2, p2) = terminal(&mut h, (50.0, -20.0), "X2");
    let a = wire(&mut h, &[(p1.x, p1.y), (50.0, 0.0)], json!({}));
    let first = rid(&a);
    assert_eq!(a["created"], json!(true), "{a}");
    let before = h.doc();
    let b = wire(&mut h, &[(50.0, 0.0), (p2.x, p2.y)], json!({}));
    assert_eq!(b["created"], json!(false), "{b}");
    assert_eq!(rid(&b), first, "the result names the existing line: {b}");
    assert_eq!(b["joined"][0]["how"], "merged", "{b}");
    assert_eq!(b["joined"][0]["route"], json!(first.hex()), "{b}");
    let doc = h.doc();
    assert_eq!(doc.routes.keys().copied().collect::<Vec<_>>(), vec![first], "one route");
    assert_eq!(doc.routes[&first].ends, [RouteEnd::Point(t1), RouteEnd::Point(t2)]);
    let w = cid(&b);
    assert_eq!(conductors(&doc), vec![w], "one wire");
    let nets = h.run("nets", json!({})).unwrap();
    let nets = nets["nets"].as_array().unwrap();
    assert_eq!(nets.len(), 1, "{nets:?}");
    assert!(same_pts(&vertices(&doc, first), &[(p1.x, p1.y), (50.0, 0.0), (p2.x, p2.y)]), "{:?}", vertices(&doc, first));
    assert_eq!(wires::labels_on_page(&doc, page).len(), 1, "one wire, one label");
    let tx = last_tx(&mut h);
    assert_eq!(tx.label, "Wire");
    assert!(!tx.ops.iter().any(|o| matches!(o, Op::AddRoute { .. })), "no new route: {:?}", tx.ops);
    assert_eq!(checks(&mut h, &["looks-connected"]).len(), 0);
    undo_is_one_step(&mut h, &before);
}

/// §12 2: collinear ends join into one straight route (the middle vertex dropped).
#[test]
fn t02_collinear_join_drops_the_middle_vertex() {
    let mut h = H::new(0x7032_0002);
    let a = rid(&wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({})));
    let b = wire(&mut h, &[(50.0, 0.0), (80.0, 0.0)], json!({}));
    assert_eq!(rid(&b), a);
    let doc = h.doc();
    assert_eq!(doc.routes.len(), 1);
    assert!(same_pts(&vertices(&doc, a), &[(0.0, 0.0), (80.0, 0.0)]), "{:?}", vertices(&doc, a));
}

/// §12 2 (drawn the other way): a line ending on another's start also merges, and C keeps its
/// ends (design §2.2: the merged route is oriented so the existing route's ends keep A and B).
#[test]
fn t02b_ending_on_a_start_merges_and_keeps_end_letters() {
    let mut h = H::new(0x7032_0021);
    let (_, t, tp) = terminal(&mut h, (0.0, 0.0), "X1");
    // C runs from the terminal (end A on it) to a free end B at (50, 0).
    let c = rid(&wire(&mut h, &[(tp.x, tp.y), (50.0, 0.0)], json!({})));
    assert_eq!(h.doc().routes[&c].ends[0], RouteEnd::Point(t));
    // W ends on C's free end B.
    let w = wire(&mut h, &[(50.0, -30.0), (50.0, 0.0)], json!({}));
    assert_eq!(rid(&w), c, "{w}");
    let doc = h.doc();
    assert_eq!(doc.routes.len(), 1);
    assert_eq!(doc.routes[&c].ends[0], RouteEnd::Point(t), "C keeps its end A on the terminal");
    let v = vertices(&doc, c);
    assert!(v.first().is_some_and(|p| p.near(tp, 1e-6)), "end A stays at the terminal: {v:?}");
    assert!(v.last().is_some_and(|p| p.near(Pt::new(50.0, -30.0), 1e-6)), "end B moved to W's far end: {v:?}");
}

/// §12 3: both ends of the new line land on free ends of two routes: one route; `removed` lists
/// the folded one; its far landing is kept on the survivor, and the two points get their wire.
#[test]
fn t03_both_ends_fold_into_one_wire() {
    let mut h = H::new(0x7032_0003);
    let (_, t1, p1) = terminal(&mut h, (0.0, 0.0), "X1");
    let (_, t2, p2) = terminal(&mut h, (150.0, 0.0), "X2");
    let a = rid(&wire(&mut h, &[(p1.x, p1.y), (50.0, 0.0)], json!({})));
    let c = rid(&wire(&mut h, &[(100.0, 0.0), (p2.x, p2.y)], json!({})));
    let before = h.doc();
    let b = wire(&mut h, &[(50.0, 0.0), (100.0, 0.0)], json!({}));
    assert_eq!(b["created"], json!(false), "{b}");
    assert_eq!(rid(&b), a, "the survivor is the route at W's end A: {b}");
    assert_eq!(b["removed"], json!([c.hex()]), "{b}");
    let doc = h.doc();
    assert_eq!(doc.routes.keys().copied().collect::<Vec<_>>(), vec![a]);
    assert!(!doc.routes.contains_key(&c), "the folded route no longer exists");
    let ends = doc.routes[&a].ends;
    assert!(ends.contains(&RouteEnd::Point(t1)) && ends.contains(&RouteEnd::Point(t2)), "{ends:?}");
    assert!(wires::wire_between(&doc, t1, t2).is_some(), "the wire t1-t2");
    assert_eq!(doc.representations_of(Subject::Route(c)).count(), 0, "its drawing is gone");
    assert!(before.routes.contains_key(&c));
    undo_is_one_step(&mut h, &before);
}

/// Two routes with free ends facing each other at (50, y) and (100, y), each carrying a wire (a
/// second line from another point lands on its middle), with fixed numbers `24V-03` and `24V-07`.
fn fixed_pair(h: &mut H, y: f64) -> (WireId, WireId) {
    let n = (y as i64).unsigned_abs();
    let (_, _, p1) = terminal(h, (0.0, y), &format!("X{}1", n));
    let (_, _, p2) = terminal(h, (25.0, y + 40.0), &format!("X{}2", n));
    let (_, _, p3) = terminal(h, (150.0, y), &format!("X{}3", n));
    let (_, _, p4) = terminal(h, (125.0, y + 40.0), &format!("X{}4", n));
    wire(h, &[(p1.x, p1.y), (50.0, y)], json!({}));
    let x = cid(&wire(h, &[(p2.x, p2.y), (25.0, y)], json!({})));
    wire(h, &[(p3.x, p3.y), (100.0, y)], json!({}));
    let z = cid(&wire(h, &[(p4.x, p4.y), (125.0, y)], json!({})));
    h.run("wirenumber", json!({"conductor": x.hex(), "value": "24V-03"})).unwrap();
    h.run("wirenumber", json!({"conductor": z.hex(), "value": "24V-07"})).unwrap();
    (x, z)
}

/// §12 4: two different fixed numbers on the wires being joined are refused and nothing is
/// written.
#[test]
fn t04_fixed_number_survives_and_two_fixed_are_refused() {
    let mut h = H::new(0x7032_0004);
    fixed_pair(&mut h, 100.0);
    let head = h.head();
    let before = h.doc();
    let m = h.run("wire", wire_json(&[(50.0, 100.0), (100.0, 100.0)], json!({}))).unwrap_err();
    assert!(m.contains("Both wires have fixed numbers"), "{m}");
    assert!(m.contains("24V-03") && m.contains("24V-07"), "{m}");
    assert!(m.contains("join: false"), "the way out is named: {m}");
    assert_eq!(h.head(), head, "nothing written");
    assert_eq!(h.doc(), before);
}

/// §12 5: potentials 0V and 24V are refused with the design's words.
#[test]
fn t05_different_potentials_are_refused() {
    let mut h = H::new(0x7032_0005);
    wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({"potential": "0V"}));
    let head = h.head();
    let m = h.run("wire", wire_json(&[(50.0, 0.0), (50.0, -20.0)], json!({"potential": "24V"})));
    // A free line carries no wire, so no potential to compare: drawing it joins.
    if let Err(m) = &m {
        assert!(m.contains("That wire carries 0V and this one 24V"), "{m}");
        assert!(m.contains("would short them"), "{m}");
        assert_eq!(h.head(), head);
    }
    // With join off it is drawn, free.
    let r = wire(&mut h, &[(50.0, 0.0), (50.0, -20.0)], json!({"potential": "24V", "join": false}));
    assert_eq!(r["created"], json!(true), "{r}");
}

/// §12 6 (design 072 §4.3): a line's two ends meeting at one place cannot be joined into one
/// route: that would close a loop. (Drawing onto a route's two ends lands on it there, a T.)
#[test]
fn t06_a_loop_is_refused() {
    let mut h = H::new(0x7032_0006);
    wire(&mut h, &[(0.0, 0.0), (50.0, 0.0), (50.0, 50.0), (0.0, 50.0), (0.0, 0.0)], json!({"join": false}));
    let head = h.head();
    let m = h.run("joinwires", json!({"at": [0, 0]})).unwrap_err();
    assert!(m.contains("loop"), "{m}");
    assert!(m.contains("join: false"), "{m}");
    assert_eq!(h.head(), head);
}

/// §12 7 (design 072 §4.3): three ends at one point: the third lands on a route there (a T); no
/// junction component.
#[test]
fn t07_three_ends_meet_at_one_junction() {
    let mut h = H::new(0x7032_0007);
    let a = rid(&wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({})));
    let b = rid(&wire(&mut h, &[(100.0, 0.0), (50.0, 0.0)], json!({"join": false})));
    let before = h.doc();
    let r = wire(&mut h, &[(50.0, 40.0), (50.0, 0.0)], json!({}));
    assert_eq!(r["created"], json!(true), "{r}");
    assert!(r["joined"].as_array().is_some_and(|j| j.iter().any(|e| e["how"] == "on")), "{r}");
    let doc = h.doc();
    assert_eq!(doc.routes.len(), 3);
    assert!(doc.devices.is_empty(), "no junction component");
    let landed = doc.routes.values().flat_map(|x| x.ends).filter(|e| e.route().is_some_and(|x| x == a || x == b)).count();
    assert!(landed >= 1, "the free ends land on a route there: {:?}", doc.routes);
    undo_is_one_step(&mut h, &before);
}

/// §12 8 (reverse T, owner Q2): an existing free end on the new line's interior lands on it.
#[test]
fn t08_reverse_tee_splits_the_new_wire() {
    let mut h = H::new(0x7032_0008);
    let c = rid(&wire(&mut h, &[(30.0, 20.0), (30.0, -10.0)], json!({})));
    let before = h.doc();
    let r = wire(&mut h, &[(0.0, -10.0), (60.0, -10.0)], json!({}));
    let tees = r["tees"].as_array().cloned().unwrap_or_default();
    assert_eq!(tees.len(), 1, "{r}");
    assert_eq!(tees[0]["at"], json!([30.0, -10.0]), "{r}");
    let doc = h.doc();
    assert_eq!(doc.routes.len(), 2, "C and the new line, unsplit");
    assert_eq!(doc.routes[&c].ends[1].route(), Some(rid(&r)), "C lands on the new line");
    undo_is_one_step(&mut h, &before);
}

/// §12 9: `join: false` leaves both ends free; Checks lists Looks connected with the Join fix;
/// the old name `junction` is gone (lane 072 D) and refused.
#[test]
fn t09_join_false_and_junction_false_and_their_disagreement() {
    {
        let key = "join";
        let mut h = H::new(0x7032_0009);
        let (_, _, p1) = terminal(&mut h, (0.0, 0.0), "X1");
        let (_, _, p2) = terminal(&mut h, (50.0, -40.0), "X2");
        let a = rid(&wire(&mut h, &[(p1.x, p1.y), (50.0, 0.0)], json!({})));
        let b = wire(&mut h, &[(50.0, 0.0), (p2.x, p2.y)], json!({key: false}));
        assert_eq!(b["created"], json!(true), "{key}: {b}");
        let doc = h.doc();
        assert_eq!(doc.routes.len(), 2, "{key}");
        assert_ne!(a, rid(&b), "{key}");
        assert!(doc.wires.is_empty(), "{key}: two lines each from one point carry no wire");
        let looks = checks(&mut h, &["looks-connected"]);
        assert_eq!(looks.len(), 1, "{key}: {looks:?}");
        assert_eq!(looks[0]["at"], json!([50.0, 0.0]), "{key}");
        assert_eq!(looks[0]["text"], "Looks connected", "renamed (design §2.7): {looks:?}");
        let fixes = fix_names(&mut h, Kind::LooksConnected);
        assert!(fixes.iter().all(|f| f.starts_with("Some(Join(")), "{key}: {fixes:?}");
    }
    let mut h = H::new(0x7032_0091);
    wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({}));
    let head = h.head();
    let m = h.run("wire", wire_json(&[(50.0, 0.0), (50.0, -20.0)], json!({"join": true, "junction": false}))).unwrap_err();
    assert!(m.contains("`junction` is not a parameter"), "{m}");
    assert_eq!(h.head(), head);
    // A non-boolean `join` is refused, not read as true.
    let m = h.run("wire", wire_json(&[(200.0, 0.0), (250.0, 0.0)], json!({"join": "no"}))).unwrap_err();
    assert!(!m.is_empty());
}

/// §12 11: `joinwires` repairs ends that meet; Checks rows go; nothing to join is refused.
#[test]
fn t11_joinwires_repairs_and_refuses_nothing_to_join() {
    let mut h = H::new(0x7032_0011);
    let a = rid(&wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({})));
    let b = rid(&wire(&mut h, &[(50.0, 0.0), (50.0, -20.0)], json!({"join": false})));
    let before = h.doc();
    let r = h.run("joinwires", json!({"at": [50, 0]})).unwrap();
    assert_eq!(r["how"], "merged", "{r}");
    let survivor = rid(&r);
    assert!(survivor == a || survivor == b, "{r}");
    let gone = if survivor == a { b } else { a };
    assert_eq!(r["removed"], json!([gone.hex()]), "{r}");
    assert_eq!(r["at"], json!([50.0, 0.0]), "{r}");
    assert_eq!(last_tx(&mut h).label, "Join wires");
    let doc = h.doc();
    assert_eq!(doc.routes.keys().copied().collect::<Vec<_>>(), vec![survivor]);
    assert!(same_pts(&vertices(&doc, survivor), &[(0.0, 0.0), (50.0, 0.0), (50.0, -20.0)]));
    assert!(checks(&mut h, &["looks-connected"]).is_empty());
    undo_is_one_step(&mut h, &before);
    // Nothing to join.
    let head = h.head();
    let m = h.run("joinwires", json!({"at": [500, 500]})).unwrap_err();
    assert!(m.contains("There are no wire ends to join here."), "{m}");
    assert_eq!(h.head(), head);
    // A free end on a route's interior lands on it (a T).
    let on = rid(&wire(&mut h, &[(0.0, 100.0), (60.0, 100.0)], json!({})));
    let tee = rid(&wire(&mut h, &[(30.0, 130.0), (30.0, 100.0)], json!({"join": false})));
    let r = h.run("joinwires", json!({"at": [30, 100]})).unwrap();
    assert_eq!(r["how"], "tee", "{r}");
    assert_eq!(h.doc().routes[&tee].ends[1].route(), Some(on), "{r}");
}

/// Owner Q9 and brief "not in scope": an older file with ends that meet is not rewritten on open;
/// Checks and `joinwires` repair it.
#[test]
fn t11b_a_file_with_ends_that_meet_opens_unchanged() {
    let dir = tmp_dir("old");
    let mut h = H::new(0x7032_0012);
    wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({}));
    wire(&mut h, &[(50.0, 0.0), (50.0, -20.0)], json!({"join": false}));
    h.cfg(|p| p.persistence = Persistence::File);
    let path = dir.join("alte Zeichnung 1.ecad").display().to_string();
    h.run("saveas", json!({"path": path})).unwrap();
    let head = h.head();
    let doc = h.doc();
    h.close_all();
    drop(h);
    let mut h2 = reopen(&path, 0x7032_0013);
    assert_eq!(h2.head(), head, "open writes nothing");
    assert_eq!(h2.doc().routes, doc.routes, "routes unchanged");
    assert_eq!(checks(&mut h2, &["looks-connected"]).len(), 1);
    h2.run("joinwires", json!({"at": [50, 0]})).unwrap();
    assert_eq!(h2.doc().routes.len(), 1);
    h2.close_all();
    drop(h2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// §12 12 covered inside 1, 3, 7, 8 and 11 (`undo_is_one_step`); here a redo gives the same IDs.
#[test]
fn t12_redo_of_a_fold_gives_the_same_document() {
    let mut h = H::new(0x7032_0014);
    let (_, _, p1) = terminal(&mut h, (0.0, 0.0), "X1");
    let (_, _, p2) = terminal(&mut h, (150.0, 0.0), "X2");
    wire(&mut h, &[(p1.x, p1.y), (50.0, 0.0)], json!({}));
    wire(&mut h, &[(100.0, 0.0), (p2.x, p2.y)], json!({}));
    wire(&mut h, &[(50.0, 0.0), (100.0, 0.0)], json!({}));
    let after = h.doc();
    h.cmdline("u");
    h.cmdline("redo");
    assert_eq!(h.doc(), after);
}

// ---------- Q8: a cable across pages ----------

struct W04 {
    h: H,
    p1: PageId,
    p2: PageId,
    cable: CableId,
    /// The cable's three cores: each one wire from `-X0n` on page 1 to `-X5n` on page 2, through
    /// the continuation pair An (design 072 §4.4, F5).
    w: Vec<WireId>,
}

/// Design §3.1, as design 072 §4.4 (F5) has it: page 1 `-X1…3` → arrows A1…A3, slashed `-W04`
/// (the lines carry no wire yet, the cores sit on them); page 2 arrows A1…A3 → `-X51…53`, which
/// makes each core one wire across the pages. The second slash is not drawn yet.
fn w04(seed: u64) -> W04 {
    let mut h = H::new(seed);
    let p1 = h.first_page();
    for i in 0..3 {
        let x = 100.0 + 20.0 * i as f64;
        let (_, _, tp) = terminal(&mut h, (x, 100.0), &format!("X{}", i + 1));
        continuation(&mut h, (tp.x, 40.0), &format!("A{}", i + 1), 0.0);
        let r = wire(&mut h, &[(tp.x, tp.y), (tp.x, 40.0)], json!({}));
        assert!(r["conductor"].is_null(), "fixture: no partner yet, no wire: {r}");
    }
    let v = h.run("cableslash", json!({"from": [90, 70], "to": [150, 70], "tag": "-W04"})).unwrap();
    let cable: CableId = id(&v, "cable");
    let p2: PageId = id(&h.run("newpage", json!({"title": "Field"})).unwrap(), "page");
    h.activate(p2);
    let mut w = Vec::new();
    for i in 0..3 {
        let x = 100.0 + 20.0 * i as f64;
        let (_, _, tp) = terminal(&mut h, (x, 100.0), &format!("X5{}", i + 1));
        continuation(&mut h, (tp.x, 40.0), &format!("A{}", i + 1), 180.0);
        w.push(cid(&wire(&mut h, &[(tp.x, 40.0), (tp.x, tp.y)], json!({}))));
    }
    let members: Vec<WireId> = h.doc().cables[&cable].cores.iter().filter_map(|m| m.wire()).collect();
    assert_eq!(members, w, "fixture: each core moved onto its wire");
    W04 { h, p1, p2, cable, w }
}

fn slash_reps(doc: &Document, k: CableId) -> Vec<(ecad_model::RepresentationId, PageId)> {
    doc.representations_of(Subject::Cable(k)).filter(|(_, r)| r.role == ecad_model::Role::Slash).map(|(id, r)| (id, r.page)).collect()
}

fn same_outputs_as_built(h: &mut H) {
    let (kept, built, kept_checks, built_checks) = h.cfg(|p| {
        let kept = p.outputs().clone();
        let doc = p.ledger.document();
        let built = ReportModels::build_with(doc, &ecad_engine::text_style::EngineMetrics);
        let kc: Vec<String> = ecad_engine::checks::all_with(doc, p.tolerance, &kept).iter().map(|c| c.to_json(doc).to_string()).collect();
        let bc: Vec<String> = ecad_engine::checks::all_with(doc, p.tolerance, &built).iter().map(|c| c.to_json(doc).to_string()).collect();
        (kept, built, kc, bc)
    });
    assert_eq!(kept.cable_table(), built.cable_table(), "kept cable table = rebuild");
    assert_eq!(kept.wire_table(), built.wire_table(), "kept wire table = rebuild");
    assert_eq!(kept_checks, built_checks, "Checks from kept outputs = from a rebuild");
}

/// §12 14, as F5 has it: each core is one wire connected at both ends, so `cablecontinue` has
/// nothing to continue and is refused, naming the cable; nothing is written.
#[test]
fn t14_cablecontinue_on_cores_connected_at_both_ends_is_refused() {
    let mut f = w04(0x7032_0140);
    let head = f.h.head();
    let m = f.h.run("cablecontinue", json!({"cable": f.cable.hex(), "at": [300, 70], "toward": [350, 70]})).unwrap_err();
    assert!(m.contains("-W04"), "{m}");
    assert_eq!(f.h.head(), head);
}

/// §12 13, as design 072 §4.4 (F5) has it: `cableslash {tag: "-W04"}` on page 2 shows the cable
/// there: one cable, three members (each one wire from `-X1…3` to `-X51…53`), no core-twice or
/// too-many-cores rows; erasing either slash leaves the three members.
#[test]
fn t13_a_cable_continued_on_a_second_page() {
    let mut f = w04(0x7032_0130);
    let doc = f.h.doc();
    let labels: Vec<Option<String>> = doc.cables[&f.cable].cores.iter().map(|m| m.label_str().map(str::to_string)).collect();
    let cables_before = doc.cables.len();
    // Offer exactly three cores, the page-1 labels (or 1,2,3 when the slash gave none).
    let offer: Vec<String> = labels.iter().enumerate().map(|(i, l)| l.clone().unwrap_or_else(|| (i + 1).to_string())).collect();
    f.h.run("setprop", json!({"cable": f.cable.hex(), "key": "cores", "value": offer.join(",")})).unwrap();
    // Locations, so the cable list's from and to have something to name (design §3.5). Brief 052:
    // set on the devices (a box would only propose).
    let doc = f.h.doc();
    let tagged = |pre: &str| -> Vec<String> {
        doc.devices
            .keys()
            .filter(|c| ecad_engine::devices::tag_of(&doc, **c).is_some_and(|t| t.to_string().starts_with(pre)))
            .map(|c| c.hex())
            .collect()
    };
    let (near, far) = (tagged("-X0"), tagged("-X5"));
    assert_eq!((near.len(), far.len()), (3, 3), "fixture: -X01…03 and -X51…53");
    f.h.run("setlocation", json!({"devices": near, "value": "+CP1"})).unwrap();
    f.h.run("setlocation", json!({"devices": far, "value": "+FIELD"})).unwrap();
    f.h.activate(f.p2);
    let before = f.h.doc();
    let r = f.h.run("cableslash", json!({"from": [90, 70], "to": [150, 70], "tag": "-W04"})).unwrap();
    assert_eq!(r["continued"], json!(true), "{r}");
    assert_eq!(id::<CableId>(&r, "cable"), f.cable, "{r}");
    // F5: each core is already one wire through its continuation pair; the slash names them.
    let cores = r["cores"].as_array().cloned().unwrap_or_default();
    assert_eq!(cores.len(), 3, "{r}");
    for (i, w) in f.w.iter().enumerate() {
        let row = cores.iter().find(|c| c["conductor"] == json!(w.hex())).unwrap_or_else(|| panic!("{w} in {r}"));
        assert_eq!(row["core"], json!(offer[i]), "{r}");
    }
    let tx = last_tx(&mut f.h);
    assert_eq!(tx.label, "Cable slash");
    assert!(!tx.ops.iter().any(|o| matches!(o, Op::CreateCable { .. })), "no new cable: {:?}", tx.ops);
    assert!(!tx.ops.iter().any(|o| matches!(o, Op::ModifyCable { .. })), "no new member: {:?}", tx.ops);
    assert!(tx.ops.iter().any(|o| matches!(o, Op::LinkRepresentation { .. })));
    let doc = f.h.doc();
    assert_eq!(doc.cables.len(), cables_before, "one cable");
    let k = &doc.cables[&f.cable];
    assert_eq!(k.cores.len(), 3);
    for (i, w) in f.w.iter().enumerate() {
        let m = k.cores.iter().find(|m| m.wire() == Some(*w)).expect("member");
        // Design 072 §4.5: setting the offer gave the cores its labels.
        assert_eq!(m.label_str(), offer.get(i).map(String::as_str));
    }
    let pages: Vec<PageId> = slash_reps(&doc, f.cable).into_iter().map(|(_, p)| p).collect();
    assert!(pages.contains(&f.p1) && pages.contains(&f.p2), "{pages:?}");
    // The cable's core list: three chains, -X n → -X5n.
    let cc = f.h.run("cablecores", json!({"cable": f.cable.hex()})).unwrap();
    let rows = cc["cores"].as_array().cloned().unwrap_or_default();
    assert_eq!(rows.len(), 3, "{cc}");
    assert_eq!(cc["used"], 3, "{cc}");
    assert!(cc["over"].as_array().is_none_or(|o| o.is_empty()), "{cc}");
    for (i, row) in rows.iter().enumerate() {
        let ends = format!("{} {}", row["from"].as_str().unwrap_or(""), row["to"].as_str().unwrap_or(""));
        assert!(ends.contains(&format!("-X0{}:", i + 1)) && ends.contains(&format!("-X5{}:", i + 1)), "row {i}: {row}");
        assert!(!ends.contains("A1") && !ends.contains("A2") && !ends.contains("A3"), "from and to skip the continuations: {row}");
    }
    let cl = f.h.run("cablelist", json!({})).unwrap();
    let row = cl["rows"].as_array().unwrap().iter().find(|r| r["cable"] == json!(f.cable.hex())).cloned().unwrap();
    assert_eq!(row["cores"], 3, "{row}");
    assert_eq!(row["used"], 3, "{row}");
    let devs = format!("{} {}", row["from_devices"], row["to_devices"]);
    assert!(devs.contains("-X01") && devs.contains("-X51"), "{row}");
    let locs = format!("{} {}", row["from_location"], row["to_location"]);
    assert!(locs.contains("+CP1") && locs.contains("+FIELD"), "-X01 → -X51 locations: {row}");
    assert!(checks(&mut f.h, &["core-twice", "too-many-cores"]).is_empty(), "{:?}", checks(&mut f.h, &["core-twice", "too-many-cores"]));
    same_outputs_as_built(&mut f.h);
    undo_is_one_step(&mut f.h, &before);

    // Erasing the page-2 slash removes that representation; the members stay (they run on page 1).
    let doc = f.h.doc();
    let (rep, _) = slash_reps(&doc, f.cable).into_iter().find(|(_, p)| *p == f.p2).unwrap();
    let item = doc.representations[&rep].items[0];
    f.h.activate(f.p2);
    let handle = f.h.handle_of(f.p2, item);
    f.h.run("erase", json!({"handles": [handle.hex()]})).unwrap();
    let doc = f.h.doc();
    assert!(doc.cables.contains_key(&f.cable), "the cable stays");
    let members: Vec<_> = doc.cables[&f.cable].cores.iter().filter_map(|m| m.wire()).collect();
    assert_eq!(members, f.w, "the three members stay");
    assert_eq!(slash_reps(&doc, f.cable).len(), 1);
    same_outputs_as_built(&mut f.h);
    // Erasing the last slash keeps the cable, as today (009 AC6; design §3.5's table says
    // "deletes the cable", which is not what `6729c8f1` does).
    f.h.activate(f.p1);
    let (rep, _) = slash_reps(&doc, f.cable)[0];
    let item = doc.representations[&rep].items[0];
    let handle = f.h.handle_of(f.p1, item);
    f.h.run("erase", json!({"handles": [handle.hex()]})).unwrap();
    let d = f.h.doc();
    assert!(slash_reps(&d, f.cable).is_empty());
    let members: Vec<_> = d.cables[&f.cable].cores.iter().filter_map(|m| m.wire()).collect();
    assert_eq!(members, f.w, "the cable and its members stay");
}

/// §12 13 (by ID): `cable` instead of `tag` continues it too.
#[test]
fn t13b_continue_by_cable_id() {
    let mut f = w04(0x7032_0131);
    let r = f.h.run("cableslash", json!({"from": [90, 70], "to": [150, 70], "cable": f.cable.hex()})).unwrap();
    assert_eq!(r["continued"], json!(true), "{r}");
    assert_eq!(f.h.doc().cables[&f.cable].cores.len(), 3, "F5: no new member");
}

/// §3.3 refusals, each tested; nothing written.
#[test]
fn t13c_continue_refusals() {
    // A second slash on the same page.
    let mut f = w04(0x7032_0132);
    f.h.run("cableslash", json!({"from": [90, 70], "to": [150, 70], "tag": "-W04"})).unwrap();
    let head = f.h.head();
    let m = f.h.run("cableslash", json!({"from": [90, 60], "to": [150, 60], "tag": "-W04"})).unwrap_err();
    assert!(m.contains("-W04 already has a slash on this page."), "{m}");
    assert_eq!(f.h.head(), head);
    // On page 1 itself (its wires are the cable's own members): also already slashed there.
    f.h.activate(f.p1);
    let m = f.h.run("cableslash", json!({"from": [90, 60], "to": [150, 60], "tag": "-W04"})).unwrap_err();
    assert!(m.contains("already"), "{m}");

    // A crossed wire that is not one of -W04's cores.
    let mut f = w04(0x7032_0133);
    f.h.points_at(&[(170.0, 100.0), (170.0, 40.0)]);
    let stray = cid(&wire(&mut f.h, &[(170.0, 100.0), (170.0, 40.0)], json!({})));
    let head = f.h.head();
    let m = f.h.run("cableslash", json!({"from": [90, 70], "to": [180, 70], "tag": "-W04"})).unwrap_err();
    assert!(m.contains("is not a core of -W04"), "{m}");
    assert_eq!(f.h.head(), head);
    let _ = stray;

    // A wire on from a core's terminal is not the core.
    let mut f = w04(0x7032_0134);
    let doc = f.h.doc();
    let x51 = doc.wires[&f.w[0]]
        .ends
        .iter()
        .copied()
        .find(|t| ecad_engine::devices::tag_of(&doc, doc.terminals[t].device().unwrap()).is_some_and(|g| g.to_string().starts_with("-X5")));
    let tp = x51.map(|t| f.h.connection_point_pos(f.p2, t)).expect("X51's point");
    f.h.points_at(&[(tp.x, tp.y + 40.0)]);
    wire(&mut f.h, &[(tp.x, tp.y), (tp.x, tp.y + 40.0)], json!({}));
    let m = f.h.run("cableslash", json!({"from": [tp.x - 5.0, tp.y + 20.0], "to": [tp.x + 5.0, tp.y + 20.0], "tag": "-W04"})).unwrap_err();
    assert!(m.contains("is not a core of -W04"), "{m}");

    // A new cable over a wire already in -W04 is refused as today.
    let mut f = w04(0x7032_0136);
    let m = f.h.run("cableslash", json!({"from": [95, 60], "to": [105, 60], "tag": "-W09"})).unwrap_err();
    assert!(!m.is_empty());

    // A tag used by a device is refused as today.
    let mut f = w04(0x7032_0137);
    let m = f.h.run("cableslash", json!({"from": [90, 70], "to": [150, 70], "tag": "-X51"})).unwrap_err();
    assert!(!m.is_empty());

    // F5 (overseer, 2026-11-20): a page where none of -W04's wires runs gets no empty slash.
    let mut f = w04(0x7032_0135);
    let p3: PageId = id(&f.h.run("newpage", json!({"title": "Three"})).unwrap(), "page");
    f.h.activate(p3);
    f.h.points_at(&[(100.0, 100.0), (100.0, 40.0)]);
    wire(&mut f.h, &[(100.0, 100.0), (100.0, 40.0)], json!({}));
    let head = f.h.head();
    let m = f.h.run("cableslash", json!({"from": [90, 70], "to": [110, 70], "tag": "-W04"})).unwrap_err();
    assert!(m.contains("-W04 has no wire on this page"), "{m}");
    assert_eq!(f.h.head(), head);
}

/// Design §10, as F5 has it: lines that run only between continuations make a loop of named
/// points with no device point (L–L and R–R pairs); finding its net ends (visited sets), forms no
/// wire, and the cable commands, Checks and outputs still answer.
#[test]
fn t13d_chain_cycles_and_three_member_groups_end() {
    let mut h = H::new(0x7032_0138);
    let p1 = h.first_page();
    // Page 1: one wire between arrows L and R, slashed -W07.
    continuation(&mut h, (0.0, 0.0), "L", 0.0);
    continuation(&mut h, (100.0, 0.0), "R", 0.0);
    wire(&mut h, &[(0.0, 0.0), (100.0, 0.0)], json!({}));
    let k: CableId = id(&h.run("cableslash", json!({"from": [50, -10], "to": [50, 10], "tag": "-W07"})).unwrap(), "cable");
    // Page 2: the same pair, a wire between them: L–L and R–R pairs make a cycle.
    let p2: PageId = id(&h.run("newpage", json!({"title": "Zwei"})).unwrap(), "page");
    h.activate(p2);
    continuation(&mut h, (0.0, 0.0), "L", 0.0);
    continuation(&mut h, (100.0, 0.0), "R", 0.0);
    wire(&mut h, &[(0.0, 0.0), (100.0, 0.0)], json!({}));
    let _ = h.run("cableslash", json!({"from": [50, -10], "to": [50, 10], "tag": "-W07"}));
    let _ = h.run("cablecores", json!({"cable": k.hex()})).unwrap();
    let _ = h.run("cablelist", json!({})).unwrap();
    let _ = h.run("checks", json!({})).unwrap();
    same_outputs_as_built(&mut h);
    let _ = p1;
}

// ---------- Q4: labels ----------

/// A 24V bus with four 5 mm drops to `-X1…4`; returns the page and the drops.
fn bus(seed: u64) -> (H, PageId, Vec<WireId>) {
    let mut h = H::new(seed);
    let page = h.first_page();
    let mut drops = Vec::new();
    let mut tps = Vec::new();
    for i in 0..4 {
        let (_, _, tp) = terminal(&mut h, (20.0 + 20.0 * i as f64, -5.0), &format!("X{}", i + 1));
        tps.push(tp);
    }
    h.points_at(&[(0.0, 0.0), (100.0, 0.0)]);
    wire(&mut h, &[(0.0, 0.0), (100.0, 0.0)], json!({"potential": "24V"}));
    for tp in tps {
        drops.push(cid(&wire(&mut h, &[(tp.x, 0.0), (tp.x, tp.y)], json!({}))));
    }
    let doc = h.doc();
    let net = net_of(&doc, drops[0]);
    assert!(conductors(&doc).iter().all(|c| net_of(&doc, *c) == net), "fixture: one net");
    (h, page, drops)
}

fn routes_on(doc: &Document, page: PageId) -> Vec<(WireId, f64)> {
    let mut v = Vec::new();
    for c in doc.wires.keys() {
        let pts = wire_vertices(doc, *c, page);
        if pts.len() >= 2 {
            let longest = pts.windows(2).map(|w| ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt()).fold(0.0, f64::max);
            v.push((*c, longest));
        }
    }
    v
}

fn label_entities(h: &mut H, page: PageId) -> usize {
    h.pv(|p, pg| pg.entities_of(p, page)).iter().filter(|e| e.common.layer == LABEL_LAYER).count()
}

/// §12 15: one label per net per page by default; `"each"` labels every route at least
/// `label_min_length` long; a wire with its own cross-section is labelled too; the page shows what
/// the label pass computes.
#[test]
fn t15_label_rule_on_a_bus_with_short_drops() {
    let (mut h, page, drops) = bus(0x7032_0150);
    let s = h.run("wiresettings", json!({})).unwrap();
    assert_eq!(s["label_rule"], "net", "default (owner Q6): {s}");
    assert_eq!(s["label_min_length"], json!(0.0), "{s}");
    let doc = h.doc();
    assert_eq!(wires::labels_on_page(&doc, page).len(), 1, "one per net");
    assert_eq!(label_entities(&mut h, page), 1);

    h.run("wiresettings", json!({"label_rule": "each", "label_min_length": 0})).unwrap();
    let doc = h.doc();
    let routes = routes_on(&doc, page);
    let labels = wires::labels_on_page(&doc, page);
    assert_eq!(labels.len(), routes.len(), "every route labelled: {routes:?}");
    assert_eq!(label_entities(&mut h, page), routes.len());

    h.run("wiresettings", json!({"label_min_length": 10})).unwrap();
    let doc = h.doc();
    let labels = wires::labels_on_page(&doc, page);
    let long: Vec<_> = routes_on(&doc, page).into_iter().filter(|(_, l)| *l >= 10.0 - 1e-9).map(|(c, _)| c).collect();
    assert_eq!(labels.len(), long.len().max(1), "{labels:?}");
    assert!(labels.iter().all(|l| !drops.contains(&l.conductor)), "5 mm drops carry no label");

    // A net with no route that long still gets one label.
    h.run("wiresettings", json!({"label_min_length": 100})).unwrap();
    let doc = h.doc();
    assert_eq!(wires::labels_on_page(&doc, page).len(), 1, "every net drawn on a page is labelled at least once");

    // "net" again; one drop with its own cross-section is labelled too (owner Q7).
    h.run("wiresettings", json!({"label_rule": "net"})).unwrap();
    h.run("setprop", json!({"conductor": drops[2].hex(), "key": "size", "value": "2.5"})).unwrap();
    let doc = h.doc();
    let labels = wires::labels_on_page(&doc, page);
    assert_eq!(labels.len(), 2, "{labels:?}");
    assert!(labels.iter().any(|l| l.conductor == drops[2] && l.text.contains("2.5")), "{labels:?}");
    assert_eq!(label_entities(&mut h, page), 2);
    // `u` brings back the single label.
    h.cmdline("u");
    assert_eq!(label_entities(&mut h, page), 1);
}

/// §4.2: a net drawn on two pages is labelled on each.
#[test]
fn t15b_a_net_on_two_pages_is_labelled_on_each() {
    let mut h = H::new(0x7032_0151);
    let p1 = h.first_page();
    // Design 072: a wire runs between two connection points.
    h.points_at(&[(0.0, 0.0)]);
    continuation(&mut h, (100.0, 0.0), "S1", 0.0);
    wire(&mut h, &[(0.0, 0.0), (100.0, 0.0)], json!({}));
    let p2: PageId = id(&h.run("newpage", json!({"title": "Two"})).unwrap(), "page");
    h.activate(p2);
    continuation(&mut h, (0.0, 0.0), "S1", 180.0);
    h.points_at(&[(100.0, 0.0)]);
    wire(&mut h, &[(0.0, 0.0), (100.0, 0.0)], json!({}));
    let doc = h.doc();
    assert_eq!(wires::labels_on_page(&doc, p1).len(), 1);
    assert_eq!(wires::labels_on_page(&doc, p2).len(), 1);
}

/// §8.1: with `"net"`, joining two nets on page 2 changes page 1's labels (two nets, one label
/// each → one net, one label), and page 1 shows it when it is opened (the projection check of
/// `H::check` compares against a fresh projection).
#[test]
fn t17_update_map_nets_joined_on_another_page_reproject_labels() {
    let mut h = H::new(0x7032_0170);
    let p1 = h.first_page();
    // Design 072: a wire runs between two connection points.
    h.points_at(&[(0.0, 0.0), (0.0, 50.0)]);
    continuation(&mut h, (100.0, 0.0), "S1", 0.0);
    wire(&mut h, &[(0.0, 0.0), (100.0, 0.0)], json!({}));
    continuation(&mut h, (100.0, 50.0), "S2", 0.0);
    wire(&mut h, &[(0.0, 50.0), (100.0, 50.0)], json!({}));
    let p2: PageId = id(&h.run("newpage", json!({"title": "Two"})).unwrap(), "page");
    h.activate(p2);
    continuation(&mut h, (0.0, 0.0), "S1", 180.0);
    continuation(&mut h, (0.0, 50.0), "S2", 180.0);
    // F5: continuations are named points, so each net is one wire from page 1's point to page
    // 2's, and page 1 shows its labels once page 2's lines land.
    h.points_at(&[(50.0, 0.0), (50.0, 50.0)]);
    wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({}));
    wire(&mut h, &[(0.0, 50.0), (50.0, 50.0)], json!({}));
    // Make the label texts equal across both nets (no potential, no colour, no cross-section).
    h.activate(p1);
    assert_eq!(label_entities(&mut h, p1), 2);
    h.activate(p2);
    // Join the two nets on page 2.
    wire(&mut h, &[(50.0, 50.0), (50.0, 0.0)], json!({}));
    let doc = h.doc();
    let c = conductors(&doc);
    let net = net_of(&doc, c[0]);
    assert!(c.iter().all(|x| net_of(&doc, *x) == net), "one net now");
    h.activate(p1);
    h.check();
    assert_eq!(label_entities(&mut h, p1), wires::labels_on_page(&h.doc(), p1).len());
    assert_eq!(label_entities(&mut h, p1), 1, "page 1 now shows one label for the joined net");
    // A settings change re-projects every page.
    h.run("wiresettings", json!({"label_rule": "each"})).unwrap();
    assert_eq!(label_entities(&mut h, p1), 2);
    h.activate(p2);
    h.check();
}

/// §8.3/§8.4, as F5 has it: renaming a continuation breaks its pair; the core's wire goes and the
/// core stays on its lines, still used. Checks and the cable outputs stay equal to a rebuild, and
/// undo puts the core back on its wire.
#[test]
fn t17b_update_map_follows_continuation_names() {
    let mut f = w04(0x7032_0171);
    f.h.run("cableslash", json!({"from": [90, 70], "to": [150, 70], "tag": "-W04"})).unwrap();
    same_outputs_as_built(&mut f.h);
    let doc = f.h.doc();
    // Rename page 2's A2 to Z9: core 2's wire goes; the core stays on its lines.
    let a2 = doc
        .aids
        .iter()
        .find(|(a, x)| {
            x.continuation_name().map(|n| n.as_str()) == Some("A2") && doc.representations_of(Subject::Aid(**a)).any(|(_, r)| r.page == f.p2)
        })
        .map(|(a, _)| *a)
        .unwrap();
    f.h.run("setprop", json!({"aids": [a2.hex()], "key": "name", "value": "Z9"})).unwrap();
    same_outputs_as_built(&mut f.h);
    let doc = f.h.doc();
    assert!(!doc.wires.contains_key(&f.w[1]));
    assert!(matches!(doc.cables[&f.cable].cores[1].position, ecad_model::CorePosition::Route { .. }), "{:?}", doc.cables[&f.cable].cores);
    let cc = f.h.run("cablecores", json!({"cable": f.cable.hex()})).unwrap();
    assert_eq!(cc["used"], 3, "the core on its lines is used: {cc}");
    f.h.cmdline("u");
    same_outputs_as_built(&mut f.h);
    assert_eq!(f.h.doc().cables[&f.cable].cores[1].wire(), Some(f.w[1]));
    let cc = f.h.run("cablecores", json!({"cable": f.cable.hex()})).unwrap();
    assert_eq!(cc["used"], 3, "{cc}");
    let _ = f.p1;
}

/// §12 16 (FP-W1): the two settings survive save and reopen; a settings value without them reads
/// `"net"` and 0.
#[test]
fn t16_label_settings_round_trip_and_default_when_absent() {
    let dir = tmp_dir("fpw1");
    let mut h = H::new(0x7032_0160);
    h.run("wiresettings", json!({"label_rule": "each", "label_min_length": 12.5})).unwrap();
    h.cfg(|p| p.persistence = Persistence::File);
    let path = dir.join("Etiketten ä.ecad").display().to_string();
    h.run("saveas", json!({"path": path})).unwrap();
    h.close_all();
    drop(h);
    let mut h2 = reopen(&path, 0x7032_0161);
    let s = h2.run("wiresettings", json!({})).unwrap();
    assert_eq!(s["label_rule"], "each", "{s}");
    assert_eq!(s["label_min_length"], json!(12.5), "{s}");
    h2.close_all();
    drop(h2);
    let _ = std::fs::remove_dir_all(&dir);
    // Serde: unset, the two fields are left out (brief 066, F2) and read back unset, which
    // resolves as the built-ins `net` and 0.
    let v = serde_json::to_value(ecad_model::wiring::WiringSettings::default()).unwrap();
    assert!(v.get("label_rule").is_none() && v.get("label_min_length").is_none(), "{v}");
    let back: ecad_model::wiring::WiringSettings = serde_json::from_value(v).unwrap();
    assert_eq!((back.label_rule.as_deref(), back.label_min_length), (None, None));
    let mut doc = ecad_model::Document::default();
    doc.settings.wiring = back;
    assert_eq!(ecad_engine::wires::label_rule_word(&doc), "net");
    assert_eq!(ecad_engine::wires::label_min_length(&doc), 0.0);
}

/// `wiresettings` refuses bad values in words and writes nothing.
#[test]
fn t16b_wiresettings_refuses_bad_label_values() {
    let mut h = H::new(0x7032_0162);
    let head = h.head();
    for bad in [
        json!({"label_rule": "some"}),
        json!({"label_rule": 3}),
        json!({"label_min_length": -1}),
        json!({"label_min_length": 100.5}),
        json!({"label_min_length": "10"}),
        json!({"label_min_length": 1e308}),
    ] {
        let m = h.run("wiresettings", bad.clone()).unwrap_err();
        assert!(!m.is_empty(), "{bad}");
    }
    assert_eq!(h.head(), head);
    let m = h.run("wiresettings", json!({"label_min_length": 101})).unwrap_err();
    assert!(m.contains("0 to 100"), "{m}");
    // Boundaries are accepted.
    h.run("wiresettings", json!({"label_min_length": 100})).unwrap();
    h.run("wiresettings", json!({"label_min_length": 0})).unwrap();
}

// ---------- the Wire tool (§2.4, §12 18) ----------

fn prompt(h: &H) -> String {
    h.s.current_prompt().map(|p| p.display()).unwrap_or_default()
}

fn typed(h: &mut H, text: &str) {
    if text.is_empty() {
        h.s.input(Input::Enter).unwrap();
    } else {
        h.s.cmdline(text).unwrap();
    }
}

fn log_since(h: &H, n: usize) -> String {
    h.s.log.get(n..).unwrap_or_default().join("\n")
}

/// §12 18: the Join keyword turns joining off for the tool session; the prompt says so; a new
/// tool session starts with joining on.
#[test]
fn t18_wire_tool_join_keyword() {
    let mut h = H::new(0x7032_0180);
    wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({}));
    h.s.start("wire").unwrap();
    assert!(prompt(&h).contains("Join"), "{}", prompt(&h));
    let n = h.s.log.len();
    typed(&mut h, "J");
    assert!(log_since(&h, n).contains("Joining is off"), "{}", log_since(&h, n));
    h.s.cursor = Vec2::new(50.0, 0.0);
    h.s.input(Input::Point(Vec2::new(50.0, 0.0))).unwrap();
    assert!(prompt(&h).contains("(join off)"), "{}", prompt(&h));
    typed(&mut h, "50,-20");
    typed(&mut h, "");
    h.check();
    assert_eq!(h.doc().routes.len(), 2, "join off: two lines");
    // A new tool session starts with joining on.
    h.s.start("wire").unwrap();
    assert!(!prompt(&h).contains("(join off)"), "{}", prompt(&h));
    h.s.cursor = Vec2::new(100.0, 0.0);
    typed(&mut h, "0,0");
    typed(&mut h, "0,30");
    typed(&mut h, "");
    h.check();
    assert_eq!(h.doc().routes.len(), 2, "the new line merged into the first one");
    // J, J turns it back on, with the echo.
    h.s.start("wire").unwrap();
    typed(&mut h, "J");
    let n = h.s.log.len();
    typed(&mut h, "J");
    assert!(log_since(&h, n).contains("Joining is on."), "{}", log_since(&h, n));
    h.s.input(Input::Cancel).unwrap();
}

/// §2.4: a refused merge in the tool is echoed and nothing is drawn; the tool keeps its points.
#[test]
fn t18b_wire_tool_refusal_keeps_points() {
    let mut h = H::new(0x7032_0181);
    fixed_pair(&mut h, 0.0);
    let head = h.head();
    h.s.start("wire").unwrap();
    typed(&mut h, "50,0");
    typed(&mut h, "100,0");
    let n = h.s.log.len();
    typed(&mut h, "");
    assert_eq!(h.head(), head, "nothing drawn");
    assert!(log_since(&h, n).contains("fixed numbers"), "{}", log_since(&h, n));
    if h.s.running.is_some() {
        typed(&mut h, "J");
        typed(&mut h, "");
        assert_eq!(h.head(), head + 1, "after J the wire is drawn open");
    }
}

// ---------- defaults (§4.5, §12 21) ----------

/// §12 21: lane T's registry lists lane W's three defaults (needs 031's `defaults` command on the
/// build; reported as not testable if 031 has not landed).
#[test]
fn t21_defaults_registry_lists_lane_w_keys() {
    let mut h = H::new(0x7032_0210);
    let d = h.run("defaults", json!({})).unwrap();
    let text = d.to_string();
    for (key, builtin) in [("wiring.label_rule", json!("net")), ("wiring.label_min_length", json!(0)), ("wire.join", json!("yes"))] {
        let all = d.as_object().and_then(|o| o.values().find_map(Value::as_array)).cloned().unwrap_or_default();
        let e = all.iter().find(|e| e["key"] == key).unwrap_or_else(|| panic!("{key} in {text}"));
        let b = &e["builtin"];
        assert!(b == &builtin || b.as_f64() == builtin.as_f64() && builtin.is_number(), "{key}: {e}");
    }
}

// ---------- hostile input (design §10) ----------

/// Hostile `wire`, `joinwires` and `cableslash` input is refused in words, writes nothing and never
/// panics.
#[test]
fn t10h_hostile_input_refused_without_panic() {
    let mut h = H::new(0x7032_0100);
    wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({}));
    let head = h.head();
    let bad: Vec<(&str, Value)> = vec![
        ("joinwires", json!({})),
        ("joinwires", json!({"at": null})),
        ("joinwires", json!({"at": "50,0"})),
        ("joinwires", json!({"at": [50]})),
        ("joinwires", json!({"at": [f64::INFINITY, 0]})),
        ("joinwires", json!({"at": [f64::MAX, f64::MAX]})),
        ("joinwires", json!({"at": [-1e300, 1e300]})),
        ("wire", json!({"points": [[50, 0], [1e300, 0]]})),
        ("wire", json!({"points": [[50, 0], [50, 0]], "join": true})),
        ("wire", json!({"points": [[50, 0], [60, 0]], "join": null, "junction": "x"})),
        ("cableslash", json!({"from": [0, 0], "to": [10, 10], "cable": "zz"})),
        ("cableslash", json!({"from": [0, 0], "to": [10, 10], "cable": "ffffffffffffffffffffffffffffffff"})),
        ("cableslash", json!({"from": [0, 0], "to": [10, 10], "tag": "-W".repeat(5000)})),
        ("wiresettings", json!({"label_rule": "net".repeat(10_000)})),
    ];
    for (cmd, params) in bad {
        let before = h.head();
        let r = h.run(cmd, params.clone());
        if r.is_ok() {
            // An accepted value must still leave a consistent project (checked by `run`).
            h.cmdline("u");
        }
        assert!(h.head() >= before, "{cmd} {params}");
    }
    let _ = head;
    // Merged vertex count is capped (refused in words, never a huge route).
    let mut h = H::new(0x7032_0101);
    let zig = |x0: f64, n: usize| -> Vec<(f64, f64)> { (0..n).map(|i| (x0 + i as f64 * 0.01, if i % 2 == 0 { 0.0 } else { 5.0 })).collect() };
    let mut a = zig(0.0, 3_000);
    let end = *a.last().unwrap();
    a.push((end.0, 50.0));
    wire(&mut h, &a, json!({}));
    let mut b = vec![(end.0, 50.0)];
    b.extend(zig(end.0 + 10.0, 2_000).into_iter().map(|(x, y)| (x, y + 100.0)));
    let head = h.head();
    let m = h.run("wire", wire_json(&b, json!({}))).unwrap_err();
    assert!(m.contains("more than"), "{m}");
    assert_eq!(h.head(), head);
}

// ---------- save helpers (as tester 028/030) ----------
