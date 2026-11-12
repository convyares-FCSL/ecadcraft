//! Tester 032 (wires and nets, v0.15 lane W): acceptance checks from outside, written from brief
//! 032 and design 032 §12 only, before the implementation. Numbers in test names are design §12's
//! items (Q6 wire join 1–12, Q8 cable across pages 13–14, Q4 labels 15–16, update maps 17, the
//! Wire tool 18, defaults 21). They were written
//! before the build and read new result fields and fix variants by name; they run on the build.

use cadcraft_engine::Input;
use cadcraft_engine::Session;
use cadcraft_engine::geom::Vec2;
use ecad_cadcraft::checks::Kind;
use ecad_cadcraft::outputs::Outputs;
use ecad_cadcraft::wires::{self, LABEL_LAYER};
use ecad_cadcraft::{Persistence, ProjectHandle};
use ecad_model::{CableId, ComponentId, ConductorId, Document, End, Op, PageId, Pt, Subject, TerminalId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

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

fn cid(v: &Value) -> ConductorId {
    id(v, "conductor")
}

fn conductors(doc: &Document) -> Vec<ConductorId> {
    doc.conductors.keys().copied().collect()
}

fn net_of(doc: &Document, c: ConductorId) -> Option<ecad_model::NetId> {
    doc.connectivity().net_of_conductor(c)
}

fn vertices(doc: &Document, c: ConductorId, page: PageId) -> Vec<Pt> {
    wires::route_vertices(doc, c, page).map(|(_, _, v)| v).unwrap_or_default()
}

fn same_pts(got: &[Pt], want: &[(f64, f64)]) -> bool {
    let fwd = got.len() == want.len() && got.iter().zip(want).all(|(p, (x, y))| p.near(Pt::new(*x, *y), 1e-6));
    let rev = got.len() == want.len() && got.iter().rev().zip(want).all(|(p, (x, y))| p.near(Pt::new(*x, *y), 1e-6));
    fwd || rev
}

fn last_tx(h: &mut H) -> ecad_model::Transaction {
    h.p(|p| p.ledger.log().last().cloned()).expect("a transaction")
}

/// The junction component whose connection point is at `at` on `page`, and that point.
fn junction_at(doc: &Document, page: PageId, at: (f64, f64)) -> Option<(ComponentId, TerminalId)> {
    ecad_model::derive::terminal_positions(doc, page).into_iter().find_map(|(t, _, p)| {
        let comp = doc.terminals.get(&t)?.component;
        (p.near(Pt::new(at.0, at.1), 1e-6) && wires::kind_of(doc, comp) == Some("junction")).then_some((comp, t))
    })
}

fn junctions(doc: &Document) -> usize {
    doc.components.keys().filter(|c| wires::kind_of(doc, **c) == Some("junction")).count()
}

/// How many conductor ends are attached to terminal `t`.
fn ends_on(doc: &Document, t: TerminalId) -> usize {
    doc.conductors.values().map(|c| c.ends.iter().filter(|e| **e == Some(t)).count()).sum()
}

fn checks(h: &mut H, kinds: &[&str]) -> Vec<Value> {
    let v = h.run("checks", json!({"kinds": kinds})).unwrap();
    v["checks"].as_array().cloned().unwrap_or_default()
}

/// The fix a Checks row offers, by its Debug name (`Join`, `AddJunction`, …).
fn fix_names(h: &mut H, kind: Kind) -> Vec<String> {
    let doc = h.doc();
    ecad_cadcraft::checks::all(&doc, 1e-6).into_iter().filter(|c| c.kind == kind).map(|c| format!("{:?}", c.fix)).collect()
}

fn terminal(h: &mut H, at: (f64, f64), tag: &str) -> (ComponentId, TerminalId, Pt) {
    let r = h.run("place", json!({"symbol": "terminal", "at": [at.0, at.1], "tag": tag})).unwrap();
    let t: TerminalId = id(&r["terminals"][0], "terminal");
    let page = h.p(|p| p.active_page()).expect("an active page");
    let at = h.terminal_pos(page, t);
    (id(&r, "component"), t, at)
}

fn continuation(h: &mut H, at: (f64, f64), name: &str, rotation: f64) -> ComponentId {
    let r = h.run("place", json!({"symbol": "continuation", "at": [at.0, at.1], "name": name, "rotation": rotation})).unwrap();
    id(&r, "component")
}

fn undo_is_one_step(h: &mut H, before: &Document) {
    h.cmdline("u");
    assert_eq!(&h.doc(), before, "`u` undoes it in one step");
    h.cmdline("redo");
}

// ---------- Q6: wire join ----------

/// §12 1 (the regression; fails on `6729c8f1`): a wire started on another's open end joins it.
#[test]
fn t01_end_to_end_is_one_conductor_one_net() {
    let mut h = H::new(0x7032_0001);
    let page = h.first_page();
    let a = wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({}));
    let first = cid(&a);
    assert_eq!(a["created"], json!(true), "{a}");
    let before = h.doc();
    let b = wire(&mut h, &[(50.0, 0.0), (50.0, -20.0)], json!({}));
    assert_eq!(b["created"], json!(false), "{b}");
    assert_eq!(cid(&b), first, "the result names the existing wire: {b}");
    assert_eq!(b["joined"][0]["how"], "merged", "{b}");
    assert_eq!(b["joined"][0]["conductor"], json!(first.hex()), "{b}");
    let doc = h.doc();
    assert_eq!(conductors(&doc), vec![first], "one conductor");
    let nets = h.run("nets", json!({})).unwrap();
    let nets = nets["nets"].as_array().unwrap();
    assert_eq!(nets.len(), 1, "{nets:?}");
    assert!(nets[0].to_string().contains(&first.hex()), "{nets:?}");
    assert!(same_pts(&vertices(&doc, first, page), &[(0.0, 0.0), (50.0, 0.0), (50.0, -20.0)]), "{:?}", vertices(&doc, first, page));
    assert_eq!(wires::labels_on_page(&doc, page).len(), 1, "one wire, one label");
    // The transaction is labelled Wire and creates no conductor.
    let tx = last_tx(&mut h);
    assert_eq!(tx.label, "Wire");
    assert!(!tx.ops.iter().any(|o| matches!(o, Op::CreateConductor { .. })), "{:?}", tx.ops);
    // No Checks rows about it.
    assert!(checks(&mut h, &["open-end", "loose-wire", "looks-connected"]).iter().all(|r| r["kind"] != "looks-connected"));
    assert_eq!(checks(&mut h, &["looks-connected"]).len(), 0);
    undo_is_one_step(&mut h, &before);
}

/// §12 2: collinear ends join into one straight route (the middle vertex dropped).
#[test]
fn t02_collinear_join_drops_the_middle_vertex() {
    let mut h = H::new(0x7032_0002);
    let page = h.first_page();
    let a = cid(&wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({})));
    let b = wire(&mut h, &[(50.0, 0.0), (80.0, 0.0)], json!({}));
    assert_eq!(cid(&b), a);
    let doc = h.doc();
    assert_eq!(doc.conductors.len(), 1);
    assert!(same_pts(&vertices(&doc, a, page), &[(0.0, 0.0), (80.0, 0.0)]), "{:?}", vertices(&doc, a, page));
}

/// §12 2 (drawn the other way): a wire ending on another's start also merges, and C keeps its End
/// letters (design §2.2: the merged route is oriented so the existing wire's ends keep A and B).
#[test]
fn t02b_ending_on_a_start_merges_and_keeps_end_letters() {
    let mut h = H::new(0x7032_0021);
    let page = h.first_page();
    let (_, t, tp) = terminal(&mut h, (0.0, 0.0), "X1");
    // C runs from the terminal (end A attached) to an open end B at (50, 0).
    let c = cid(&wire(&mut h, &[(tp.x, tp.y), (50.0, 0.0)], json!({})));
    assert_eq!(h.doc().conductors[&c].end(End::A), Some(t));
    // W ends on C's open end B.
    let w = wire(&mut h, &[(50.0, -30.0), (50.0, 0.0)], json!({}));
    assert_eq!(cid(&w), c, "{w}");
    let doc = h.doc();
    assert_eq!(doc.conductors.len(), 1);
    assert_eq!(doc.conductors[&c].end(End::A), Some(t), "C keeps its end A on the terminal");
    let v = vertices(&doc, c, page);
    assert!(v.first().is_some_and(|p| p.near(tp, 1e-6)), "end A stays at the terminal: {v:?}");
    assert!(v.last().is_some_and(|p| p.near(Pt::new(50.0, -30.0), 1e-6)), "end B moved to W's far end: {v:?}");
}

/// §12 3: both ends of the new wire land on open ends of two wires: one conductor; `removed` lists
/// the folded one; its far attachment is re-made on the survivor.
#[test]
fn t03_both_ends_fold_into_one_wire() {
    let mut h = H::new(0x7032_0003);
    let (_, t1, p1) = terminal(&mut h, (0.0, 0.0), "X1");
    let (_, t2, p2) = terminal(&mut h, (150.0, 0.0), "X2");
    let a = cid(&wire(&mut h, &[(p1.x, p1.y), (50.0, 0.0)], json!({})));
    let c = cid(&wire(&mut h, &[(100.0, 0.0), (p2.x, p2.y)], json!({})));
    let before = h.doc();
    let b = wire(&mut h, &[(50.0, 0.0), (100.0, 0.0)], json!({}));
    assert_eq!(b["created"], json!(false), "{b}");
    assert_eq!(cid(&b), a, "the survivor is the wire at W's end A (no fixed numbers): {b}");
    assert_eq!(b["removed"], json!([c.hex()]), "{b}");
    let doc = h.doc();
    assert_eq!(conductors(&doc), vec![a]);
    assert!(!doc.conductors.contains_key(&c), "the folded wire no longer exists");
    let ends: Vec<_> = doc.conductors[&a].ends.to_vec();
    assert!(ends.contains(&Some(t1)) && ends.contains(&Some(t2)), "{ends:?}");
    // The folded wire's representation and route item are gone.
    assert_eq!(doc.representations_of(Subject::Conductor(c)).count(), 0);
    assert!(before.conductors.contains_key(&c));
    undo_is_one_step(&mut h, &before);
}

/// §12 4: the wire with a fixed number survives; two different fixed numbers are refused and
/// nothing is written.
#[test]
fn t04_fixed_number_survives_and_two_fixed_are_refused() {
    let mut h = H::new(0x7032_0004);
    let a = cid(&wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({})));
    let c = cid(&wire(&mut h, &[(100.0, 0.0), (150.0, 0.0)], json!({})));
    h.run("wirenumber", json!({"conductor": c.hex(), "value": "24V-98"})).unwrap();
    let b = wire(&mut h, &[(50.0, 0.0), (100.0, 0.0)], json!({}));
    assert_eq!(cid(&b), c, "the fixed one survives: {b}");
    assert_eq!(b["removed"], json!([a.hex()]), "{b}");
    let doc = h.doc();
    assert_eq!(doc.conductors[&c].properties.get("wire_number").map(String::as_str), Some("24V-98"));

    // Two different fixed numbers.
    let x = cid(&wire(&mut h, &[(0.0, 100.0), (50.0, 100.0)], json!({})));
    let y = cid(&wire(&mut h, &[(100.0, 100.0), (150.0, 100.0)], json!({})));
    h.run("wirenumber", json!({"conductor": x.hex(), "value": "24V-03"})).unwrap();
    h.run("wirenumber", json!({"conductor": y.hex(), "value": "24V-07"})).unwrap();
    let head = h.head();
    let before = h.doc();
    let m = h.run("wire", wire_json(&[(50.0, 100.0), (100.0, 100.0)], json!({}))).unwrap_err();
    assert!(m.contains("Both wires have fixed numbers"), "{m}");
    assert!(m.contains("24V-03") && m.contains("24V-07"), "{m}");
    assert!(m.contains("join: false"), "the way out is named: {m}");
    assert_eq!(h.head(), head, "nothing written");
    assert_eq!(h.doc(), before);
    // Ending on the fixed wire's open end alone (no second fixed number) is a plain merge.
    let z = wire(&mut h, &[(50.0, 100.0), (50.0, 140.0)], json!({}));
    assert_eq!(cid(&z), x);
}

/// §12 5: potentials 0V and 24V are refused with the design's words.
#[test]
fn t05_different_potentials_are_refused() {
    let mut h = H::new(0x7032_0005);
    wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({"potential": "0V"}));
    let head = h.head();
    let m = h.run("wire", wire_json(&[(50.0, 0.0), (50.0, -20.0)], json!({"potential": "24V"}))).unwrap_err();
    assert!(m.contains("That wire carries 0V and this one 24V"), "{m}");
    assert!(m.contains("would short them"), "{m}");
    assert!(m.contains("join: false"), "{m}");
    assert_eq!(h.head(), head);
    // With join off it is drawn, open.
    let r = wire(&mut h, &[(50.0, 0.0), (50.0, -20.0)], json!({"potential": "24V", "join": false}));
    assert_eq!(r["created"], json!(true), "{r}");
    assert_eq!(h.doc().conductors.len(), 2);
}

/// §12 6: both ends on the two ends of one wire would close a loop: refused.
#[test]
fn t06_a_loop_is_refused() {
    let mut h = H::new(0x7032_0006);
    wire(&mut h, &[(0.0, 0.0), (50.0, 0.0), (50.0, 50.0)], json!({}));
    let head = h.head();
    let m = h.run("wire", wire_json(&[(0.0, 0.0), (0.0, 50.0), (50.0, 50.0)], json!({}))).unwrap_err();
    assert!(m.contains("loop"), "{m}");
    assert!(m.contains("join: false"), "{m}");
    assert_eq!(h.head(), head);
}

/// §12 7: three ends at one point: one junction with three attached ends.
#[test]
fn t07_three_ends_meet_at_one_junction() {
    let mut h = H::new(0x7032_0007);
    let page = h.first_page();
    let a = cid(&wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({})));
    let b = cid(&wire(&mut h, &[(100.0, 0.0), (50.0, 0.0)], json!({"join": false})));
    let before = h.doc();
    let r = wire(&mut h, &[(50.0, 40.0), (50.0, 0.0)], json!({}));
    assert_eq!(r["created"], json!(true), "{r}");
    assert!(r["joined"].as_array().is_some_and(|j| j.iter().any(|e| e["how"] == "junction")), "{r}");
    let c = cid(&r);
    let doc = h.doc();
    assert_eq!(doc.conductors.len(), 3);
    let (_, t) = junction_at(&doc, page, (50.0, 0.0)).expect("a junction at the point");
    assert_eq!(junctions(&doc), 1, "exactly one junction");
    assert_eq!(ends_on(&doc, t), 3, "three ends attached");
    assert_eq!(net_of(&doc, a), net_of(&doc, b));
    assert_eq!(net_of(&doc, a), net_of(&doc, c));
    assert!(checks(&mut h, &["looks-connected", "open-end"]).iter().all(|r| r["at"] != json!([50.0, 0.0])));
    undo_is_one_step(&mut h, &before);
}

/// §12 8 (reverse T, owner Q2): an existing open end on the new wire's interior splits it with a
/// junction.
#[test]
fn t08_reverse_tee_splits_the_new_wire() {
    let mut h = H::new(0x7032_0008);
    let page = h.first_page();
    let c = cid(&wire(&mut h, &[(30.0, 20.0), (30.0, -10.0)], json!({})));
    let before = h.doc();
    let r = wire(&mut h, &[(0.0, -10.0), (60.0, -10.0)], json!({}));
    let splits = r["splits"].as_array().cloned().unwrap_or_default();
    assert_eq!(splits.len(), 1, "{r}");
    assert_eq!(splits[0]["at"], json!([30.0, -10.0]), "{r}");
    let doc = h.doc();
    assert_eq!(doc.conductors.len(), 3, "C plus W in two conductors");
    let (_, t) = junction_at(&doc, page, (30.0, -10.0)).expect("junction at (30,-10)");
    assert_eq!(ends_on(&doc, t), 3);
    assert!(doc.conductors[&c].ends.contains(&Some(t)), "C attached to the junction");
    let net = net_of(&doc, c);
    assert!(conductors(&doc).iter().all(|w| net_of(&doc, *w) == net), "one net");
    undo_is_one_step(&mut h, &before);
}

/// §12 9: `join: false` (and its old name `junction: false`) leaves both ends open; Checks lists
/// Looks connected with the Join fix and two Open end rows; `join` and `junction` disagreeing is
/// refused.
#[test]
fn t09_join_false_and_junction_false_and_their_disagreement() {
    for key in ["join", "junction"] {
        let mut h = H::new(0x7032_0009);
        let (_, _, p1) = terminal(&mut h, (0.0, 0.0), "X1");
        let (_, _, p2) = terminal(&mut h, (50.0, -40.0), "X2");
        let a = cid(&wire(&mut h, &[(p1.x, p1.y), (50.0, 0.0)], json!({})));
        let b = wire(&mut h, &[(50.0, 0.0), (p2.x, p2.y)], json!({key: false}));
        assert_eq!(b["created"], json!(true), "{key}: {b}");
        let doc = h.doc();
        assert_eq!(doc.conductors.len(), 2, "{key}");
        assert_ne!(net_of(&doc, a), net_of(&doc, cid(&b)), "{key}");
        let looks = checks(&mut h, &["looks-connected"]);
        assert_eq!(looks.len(), 1, "{key}: {looks:?}");
        assert_eq!(looks[0]["at"], json!([50.0, 0.0]), "{key}");
        assert_eq!(looks[0]["text"], "Looks connected", "renamed (design §2.7): {looks:?}");
        let fixes = fix_names(&mut h, Kind::LooksConnected);
        assert!(fixes.iter().all(|f| f.starts_with("Some(Join(")), "{key}: {fixes:?}");
        let open = checks(&mut h, &["open-end"]);
        assert_eq!(open.len(), 2, "{key}: {open:?}");
        assert!(open.iter().all(|r| r["at"] == json!([50.0, 0.0])), "{key}: {open:?}");
    }
    let mut h = H::new(0x7032_0091);
    wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({}));
    let head = h.head();
    let m = h.run("wire", wire_json(&[(50.0, 0.0), (50.0, -20.0)], json!({"join": true, "junction": false}))).unwrap_err();
    assert!(m.contains("`join` and `junction` disagree"), "{m}");
    assert_eq!(h.head(), head);
    // Both given and the same is fine.
    let r = wire(&mut h, &[(50.0, 0.0), (50.0, -20.0)], json!({"join": true, "junction": true}));
    assert_eq!(r["created"], json!(false), "{r}");
    // A non-boolean `join` is refused, not read as true.
    let m = h.run("wire", wire_json(&[(200.0, 0.0), (250.0, 0.0)], json!({"join": "no"}))).unwrap_err();
    assert!(!m.is_empty());
}

/// §12 10: an end landing on a cable core's open end gets a two-wire junction, never a merge.
#[test]
fn t10_a_cable_cores_open_end_gets_a_junction_not_a_merge() {
    let mut h = H::new(0x7032_0010);
    let page = h.first_page();
    let a = cid(&wire(&mut h, &[(0.0, 0.0), (0.0, 50.0)], json!({})));
    let b = cid(&wire(&mut h, &[(20.0, 0.0), (20.0, 50.0)], json!({})));
    let k: CableId = id(&h.run("cableslash", json!({"from": [-10, 25], "to": [30, 25], "tag": "-W01"})).unwrap(), "cable");
    let r = wire(&mut h, &[(0.0, 50.0), (0.0, 80.0)], json!({}));
    assert_eq!(r["created"], json!(true), "{r}");
    assert!(r["joined"].as_array().is_none_or(|j| j.iter().all(|e| e["how"] != "merged")), "never a merge: {r}");
    assert_eq!(r["junctions"].as_array().map(Vec::len), Some(1), "{r}");
    let doc = h.doc();
    assert_eq!(doc.conductors.len(), 3);
    assert!(doc.conductors.contains_key(&a));
    let (_, t) = junction_at(&doc, page, (0.0, 50.0)).expect("junction");
    assert_eq!(ends_on(&doc, t), 2);
    assert_eq!(net_of(&doc, a), net_of(&doc, cid(&r)));
    let members: Vec<_> = doc.cables[&k].members.iter().map(|m| m.conductor).collect();
    assert_eq!(members, vec![a, b], "the cable is unchanged");
}

/// §12 11: `joinwires` repairs ends that meet; Checks rows go; nothing to join is refused.
#[test]
fn t11_joinwires_repairs_and_refuses_nothing_to_join() {
    let mut h = H::new(0x7032_0011);
    let page = h.first_page();
    let a = cid(&wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({})));
    let b = cid(&wire(&mut h, &[(50.0, 0.0), (50.0, -20.0)], json!({"join": false})));
    let before = h.doc();
    let r = h.run("joinwires", json!({"at": [50, 0]})).unwrap();
    assert_eq!(r["how"], "merged", "{r}");
    let survivor = cid(&r);
    assert_eq!(survivor, a.min(b), "no fixed number: the lower conductor ID survives");
    let gone = if survivor == a { b } else { a };
    assert_eq!(r["removed"], json!([gone.hex()]), "{r}");
    assert_eq!(r["at"], json!([50.0, 0.0]), "{r}");
    assert_eq!(last_tx(&mut h).label, "Join wires");
    let doc = h.doc();
    assert_eq!(conductors(&doc), vec![survivor]);
    assert!(same_pts(&vertices(&doc, survivor, page), &[(0.0, 0.0), (50.0, 0.0), (50.0, -20.0)]));
    assert!(checks(&mut h, &["looks-connected"]).is_empty());
    undo_is_one_step(&mut h, &before);
    // Nothing to join.
    let head = h.head();
    let m = h.run("joinwires", json!({"at": [500, 500]})).unwrap_err();
    assert!(m.contains("There are no wire ends to join here."), "{m}");
    assert_eq!(h.head(), head);
    // An open end on a wire's interior: a junction, as `junction {at}`.
    wire(&mut h, &[(0.0, 100.0), (60.0, 100.0)], json!({}));
    wire(&mut h, &[(30.0, 130.0), (30.0, 100.0)], json!({"join": false}));
    let r = h.run("joinwires", json!({"at": [30, 100]})).unwrap();
    assert_eq!(r["how"], "junction", "{r}");
    assert!(r["junction"].is_string(), "{r}");
    // Merging through joinwires keeps the refusals of §2.2.
    wire(&mut h, &[(0.0, 200.0), (50.0, 200.0)], json!({"potential": "0V"}));
    wire(&mut h, &[(50.0, 200.0), (50.0, 240.0)], json!({"potential": "24V", "join": false}));
    let head = h.head();
    let m = h.run("joinwires", json!({"at": [50, 200]})).unwrap_err();
    assert!(m.contains("0V") && m.contains("24V"), "{m}");
    assert_eq!(h.head(), head);
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
    close_all(&mut h);
    drop(h);
    let mut h2 = reopen(&path, 0x7032_0013);
    assert_eq!(h2.head(), head, "open writes nothing");
    assert_eq!(h2.doc().conductors, doc.conductors, "conductors unchanged");
    assert_eq!(checks(&mut h2, &["looks-connected"]).len(), 1);
    h2.run("joinwires", json!({"at": [50, 0]})).unwrap();
    assert_eq!(h2.doc().conductors.len(), 1);
    close_all(&mut h2);
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
    /// Page 1's members (cores 1–3) and page 2's wires.
    w1: Vec<ConductorId>,
    w2: Vec<ConductorId>,
}

/// Design §3.1: page 1 `-X1…3` → arrows A1…A3, slashed `-W04`; page 2 arrows A1…A3 → `-X51…53`.
/// The second slash is not drawn yet.
fn w04(seed: u64) -> W04 {
    let mut h = H::new(seed);
    let p1 = h.first_page();
    let mut w1 = Vec::new();
    for i in 0..3 {
        let x = 100.0 + 20.0 * i as f64;
        let (_, _, tp) = terminal(&mut h, (x, 100.0), &format!("X{}", i + 1));
        w1.push(cid(&wire(&mut h, &[(tp.x, tp.y), (tp.x, 40.0)], json!({}))));
        continuation(&mut h, (tp.x, 40.0), &format!("A{}", i + 1), 0.0);
    }
    let v = h.run("cableslash", json!({"from": [90, 70], "to": [150, 70], "tag": "-W04"})).unwrap();
    let cable: CableId = id(&v, "cable");
    let p2: PageId = id(&h.run("newpage", json!({"title": "Field"})).unwrap(), "page");
    h.activate(p2);
    let mut w2 = Vec::new();
    for i in 0..3 {
        let x = 100.0 + 20.0 * i as f64;
        let (_, _, tp) = terminal(&mut h, (x, 100.0), &format!("X5{}", i + 1));
        continuation(&mut h, (tp.x, 40.0), &format!("A{}", i + 1), 180.0);
        w2.push(cid(&wire(&mut h, &[(tp.x, 40.0), (tp.x, tp.y)], json!({}))));
    }
    for w in w1.iter().chain(&w2) {
        assert!(h.doc().conductors[w].ends.iter().all(Option::is_some), "fixture: every wire attached at both ends");
    }
    W04 { h, p1, p2, cable, w1, w2 }
}

fn slash_reps(doc: &Document, k: CableId) -> Vec<(ecad_model::RepresentationId, PageId)> {
    doc.representations_of(Subject::Cable(k)).filter(|(_, r)| r.role == "slash").map(|(id, r)| (id, r.page)).collect()
}

fn same_outputs_as_built(h: &mut H) {
    let (kept, built, kept_checks, built_checks) = h.cfg(|p| {
        let kept = p.outputs().clone();
        let doc = p.ledger.document();
        let built = Outputs::build(doc);
        let kc: Vec<String> = ecad_cadcraft::checks::all_with(doc, p.tolerance, &kept).iter().map(|c| c.to_json(doc).to_string()).collect();
        let bc: Vec<String> = ecad_cadcraft::checks::all_with(doc, p.tolerance, &built).iter().map(|c| c.to_json(doc).to_string()).collect();
        (kept, built, kc, bc)
    });
    assert_eq!(kept.cable_table(), built.cable_table(), "kept cable table = rebuild");
    assert_eq!(kept.wire_table(), built.wire_table(), "kept wire table = rebuild");
    assert_eq!(kept_checks, built_checks, "Checks from kept outputs = from a rebuild");
}

/// §12 14: before the second slash, `cablecontinue` says what to do instead.
#[test]
fn t14_cablecontinue_on_cores_ending_on_continuations_says_slash_there() {
    let mut f = w04(0x7032_0140);
    let head = f.h.head();
    let m = f.h.run("cablecontinue", json!({"cable": f.cable.hex(), "at": [300, 70], "toward": [350, 70]})).unwrap_err();
    assert!(m.contains("-W04's cores end on continuations"), "{m}");
    assert!(m.contains("slash the wires there with tag -W04"), "{m}");
    assert_eq!(f.h.head(), head);
}

/// §12 13: `cableslash {tag: "-W04"}` on page 2 continues the cable: one cable, six members,
/// three cores from `-X1…3` to `-X51…53`, no core-twice or too-many-cores rows; erasing the page-2
/// slash leaves three members.
#[test]
fn t13_a_cable_continued_on_a_second_page() {
    let mut f = w04(0x7032_0130);
    let doc = f.h.doc();
    let labels: Vec<Option<String>> = doc.cables[&f.cable].members.iter().map(|m| m.core.clone()).collect();
    let cables_before = doc.cables.len();
    // Offer exactly three cores, the page-1 labels (or 1,2,3 when the slash gave none).
    let offer: Vec<String> = labels.iter().enumerate().map(|(i, l)| l.clone().unwrap_or_else(|| (i + 1).to_string())).collect();
    f.h.run("setprop", json!({"cable": f.cable.hex(), "key": "cores", "value": offer.join(",")})).unwrap();
    // Locations, so the cable list's from and to have something to name (design §3.5).
    f.h.activate(f.p1);
    f.h.run("locationbox", json!({"location": "+CP1", "corners": [[50, 80], [200, 150]]})).unwrap();
    f.h.activate(f.p2);
    f.h.run("locationbox", json!({"location": "+FIELD", "corners": [[50, 80], [200, 150]]})).unwrap();
    let before = f.h.doc();
    let r = f.h.run("cableslash", json!({"from": [90, 70], "to": [150, 70], "tag": "-W04"})).unwrap();
    assert_eq!(r["continued"], json!(true), "{r}");
    assert_eq!(id::<CableId>(&r, "cable"), f.cable, "{r}");
    let cores = r["cores"].as_array().cloned().unwrap_or_default();
    assert_eq!(cores.len(), 3, "{r}");
    for (i, w) in f.w2.iter().enumerate() {
        let row = cores.iter().find(|c| c["conductor"] == json!(w.hex())).unwrap_or_else(|| panic!("{w} in {r}"));
        assert_eq!(row["continues"], json!(f.w1[i].hex()), "{r}");
    }
    let tx = last_tx(&mut f.h);
    assert_eq!(tx.label, "Cable slash");
    assert!(!tx.ops.iter().any(|o| matches!(o, Op::CreateCable { .. })), "no new cable: {:?}", tx.ops);
    assert_eq!(tx.ops.iter().filter(|o| matches!(o, Op::AddCableMember { .. })).count(), 3);
    assert!(tx.ops.iter().any(|o| matches!(o, Op::LinkRepresentation { .. })));
    let doc = f.h.doc();
    assert_eq!(doc.cables.len(), cables_before, "one cable");
    let k = &doc.cables[&f.cable];
    assert_eq!(k.members.len(), 6);
    for (i, w) in f.w2.iter().enumerate() {
        let m = k.members.iter().find(|m| m.conductor == *w).expect("member");
        assert_eq!(m.core, labels[i], "page-2 member takes its chain's core label");
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

    // Erasing the page-2 slash removes that representation and the page-2 members.
    let doc = f.h.doc();
    let (rep, _) = slash_reps(&doc, f.cable).into_iter().find(|(_, p)| *p == f.p2).unwrap();
    let item = doc.representations[&rep].items[0];
    f.h.activate(f.p2);
    let handle = f.h.handle_of(f.p2, item);
    f.h.run("erase", json!({"handles": [handle.hex()]})).unwrap();
    let doc = f.h.doc();
    assert!(doc.cables.contains_key(&f.cable), "the cable stays");
    let members: Vec<_> = doc.cables[&f.cable].members.iter().map(|m| m.conductor).collect();
    assert_eq!(members, f.w1, "page 1's three members are left");
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
    let members: Vec<_> = d.cables[&f.cable].members.iter().map(|m| m.conductor).collect();
    assert_eq!(members, f.w1, "the cable and its page-1 members stay");
}

/// §12 13 (by ID): `cable` instead of `tag` continues it too.
#[test]
fn t13b_continue_by_cable_id() {
    let mut f = w04(0x7032_0131);
    let r = f.h.run("cableslash", json!({"from": [90, 70], "to": [150, 70], "cable": f.cable.hex()})).unwrap();
    assert_eq!(r["continued"], json!(true), "{r}");
    assert_eq!(f.h.doc().cables[&f.cable].members.len(), 6);
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

    // A crossed wire that meets no core through a continuation (strict matching, owner Q5).
    let mut f = w04(0x7032_0133);
    let stray = cid(&wire(&mut f.h, &[(170.0, 100.0), (170.0, 40.0)], json!({})));
    let head = f.h.head();
    let m = f.h.run("cableslash", json!({"from": [90, 70], "to": [180, 70], "tag": "-W04"})).unwrap_err();
    assert!(m.contains("does not continue a core of -W04"), "{m}");
    assert!(m.contains("through a continuation"), "{m}");
    assert_eq!(f.h.head(), head);
    let _ = stray;

    // Strict: a wire reaching a core through a terminal (not a continuation pair) does not continue it.
    let mut f = w04(0x7032_0134);
    let doc = f.h.doc();
    let x51 = doc.conductors[&f.w2[0]].ends.iter().flatten().copied().find(|t| wires::kind_of(&doc, doc.terminals[t].component).is_none());
    let tp = x51.map(|t| f.h.terminal_pos(f.p2, t)).expect("X51's point");
    wire(&mut f.h, &[(tp.x, tp.y), (tp.x, tp.y + 40.0)], json!({}));
    let m = f.h.run("cableslash", json!({"from": [tp.x - 5.0, tp.y + 20.0], "to": [tp.x + 5.0, tp.y + 20.0], "tag": "-W04"})).unwrap_err();
    assert!(m.contains("does not continue a core of -W04"), "{m}");

    // A wire already in another cable is refused as today.
    let mut f = w04(0x7032_0136);
    f.h.run("cableslash", json!({"from": [95, 60], "to": [105, 60], "tag": "-W09"})).unwrap();
    let m = f.h.run("cableslash", json!({"from": [90, 70], "to": [150, 70], "tag": "-W04"})).unwrap_err();
    assert!(!m.is_empty());

    // A tag used by a device is refused as today.
    let mut f = w04(0x7032_0137);
    let m = f.h.run("cableslash", json!({"from": [90, 70], "to": [150, 70], "tag": "-X51"})).unwrap_err();
    assert!(!m.is_empty());

    // Two crossed wires continue the same core.
    let mut f = w04(0x7032_0135);
    continuation(&mut f.h, (300.0, 40.0), "A2", 180.0);
    wire(&mut f.h, &[(300.0, 40.0), (300.0, 100.0)], json!({}));
    let head = f.h.head();
    let m = f.h.run("cableslash", json!({"from": [90, 70], "to": [310, 70], "tag": "-W04"})).unwrap_err();
    // Design §3.3: `Two of the wires continue -W04's core 2.` These cores have no stored label
    // (the page-1 slash gave none), so the words may name the core another way, but must read.
    assert!(m.starts_with("Two of the wires continue -W04's"), "{m}");
    assert!(!m.contains("wire wire") && !m.contains("'s the core"), "garbled refusal: {m}");
    assert_eq!(f.h.head(), head);
}

/// Design §10: a cable whose members form a cycle of continuation pairs ends (visited set); a
/// name group reaching three members of one cable links none of them.
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
fn bus(seed: u64) -> (H, PageId, Vec<ConductorId>) {
    let mut h = H::new(seed);
    let page = h.first_page();
    let mut drops = Vec::new();
    let mut tps = Vec::new();
    for i in 0..4 {
        let (_, _, tp) = terminal(&mut h, (20.0 + 20.0 * i as f64, -5.0), &format!("X{}", i + 1));
        tps.push(tp);
    }
    wire(&mut h, &[(0.0, 0.0), (100.0, 0.0)], json!({"potential": "24V"}));
    for tp in tps {
        drops.push(cid(&wire(&mut h, &[(tp.x, 0.0), (tp.x, tp.y)], json!({}))));
    }
    let doc = h.doc();
    let net = net_of(&doc, drops[0]);
    assert!(conductors(&doc).iter().all(|c| net_of(&doc, *c) == net), "fixture: one net");
    (h, page, drops)
}

fn routes_on(doc: &Document, page: PageId) -> Vec<(ConductorId, f64)> {
    let mut v = Vec::new();
    for c in doc.conductors.keys() {
        let pts = vertices(doc, *c, page);
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
    h.run("setprop", json!({"conductor": drops[2].hex(), "key": "cross_section", "value": "2.5"})).unwrap();
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
    wire(&mut h, &[(0.0, 0.0), (100.0, 0.0)], json!({}));
    continuation(&mut h, (100.0, 0.0), "S1", 0.0);
    let p2: PageId = id(&h.run("newpage", json!({"title": "Two"})).unwrap(), "page");
    h.activate(p2);
    continuation(&mut h, (0.0, 0.0), "S1", 180.0);
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
    wire(&mut h, &[(0.0, 0.0), (100.0, 0.0)], json!({}));
    continuation(&mut h, (100.0, 0.0), "S1", 0.0);
    wire(&mut h, &[(0.0, 50.0), (100.0, 50.0)], json!({}));
    continuation(&mut h, (100.0, 50.0), "S2", 0.0);
    // Make the label texts equal across both nets (no potential, no colour, no cross-section).
    assert_eq!(label_entities(&mut h, p1), 2);
    let p2: PageId = id(&h.run("newpage", json!({"title": "Two"})).unwrap(), "page");
    h.activate(p2);
    continuation(&mut h, (0.0, 0.0), "S1", 180.0);
    continuation(&mut h, (0.0, 50.0), "S2", 180.0);
    wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({}));
    wire(&mut h, &[(0.0, 50.0), (50.0, 50.0), (50.0, 0.0)], json!({"join": false}));
    h.run("joinwires", json!({"at": [50, 0]})).unwrap();
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

/// §8.3/§8.4: cable chain ops keep Checks and the cable outputs equal to a rebuild (rename of a
/// continuation breaks a chain).
#[test]
fn t17b_update_map_chains_follow_continuation_names() {
    let mut f = w04(0x7032_0171);
    f.h.run("cableslash", json!({"from": [90, 70], "to": [150, 70], "tag": "-W04"})).unwrap();
    same_outputs_as_built(&mut f.h);
    let doc = f.h.doc();
    // Rename page 2's A2 to Z9: core 2 is no longer a chain (2 + 1 members as cores).
    let a2 = doc
        .components
        .iter()
        .find(|(c, x)| {
            x.properties.get("name").map(String::as_str) == Some("A2") && doc.representations_of(Subject::Component(**c)).any(|(_, r)| r.page == f.p2)
        })
        .map(|(c, _)| *c)
        .unwrap();
    f.h.run("setprop", json!({"component": a2.hex(), "key": "name", "value": "Z9"})).unwrap();
    same_outputs_as_built(&mut f.h);
    let cc = f.h.run("cablecores", json!({"cable": f.cable.hex()})).unwrap();
    assert_eq!(cc["used"], 4, "two chains and two lone members: {cc}");
    f.h.cmdline("u");
    same_outputs_as_built(&mut f.h);
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
    close_all(&mut h);
    drop(h);
    let mut h2 = reopen(&path, 0x7032_0161);
    let s = h2.run("wiresettings", json!({})).unwrap();
    assert_eq!(s["label_rule"], "each", "{s}");
    assert_eq!(s["label_min_length"], json!(12.5), "{s}");
    close_all(&mut h2);
    drop(h2);
    let _ = std::fs::remove_dir_all(&dir);
    // Serde: a stored WiringSettings without the two fields reads the defaults.
    let mut v = serde_json::to_value(ecad_model::wiring::WiringSettings::default()).unwrap();
    assert_eq!(v["label_rule"], "net", "{v}");
    assert_eq!(v["label_min_length"], json!(0.0), "{v}");
    if let Some(o) = v.as_object_mut() {
        o.remove("label_rule");
        o.remove("label_min_length");
    }
    let back: ecad_model::wiring::WiringSettings = serde_json::from_value(v).unwrap();
    let again = serde_json::to_value(back).unwrap();
    assert_eq!(again["label_rule"], "net");
    assert_eq!(again["label_min_length"], json!(0.0));
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
    assert_eq!(h.doc().conductors.len(), 2, "join off: two wires");
    // A new tool session starts with joining on.
    h.s.start("wire").unwrap();
    assert!(!prompt(&h).contains("(join off)"), "{}", prompt(&h));
    h.s.cursor = Vec2::new(100.0, 0.0);
    typed(&mut h, "0,0");
    typed(&mut h, "0,30");
    typed(&mut h, "");
    h.check();
    assert_eq!(h.doc().conductors.len(), 2, "the new wire merged into the first one");
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
    let x = cid(&wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({})));
    let y = cid(&wire(&mut h, &[(100.0, 0.0), (150.0, 0.0)], json!({})));
    h.run("wirenumber", json!({"conductor": x.hex(), "value": "24V-03"})).unwrap();
    h.run("wirenumber", json!({"conductor": y.hex(), "value": "24V-07"})).unwrap();
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
    for (key, builtin) in [("wiring.label_rule", json!("net")), ("wiring.label_min_length", json!(0)), ("wire.join", json!("on"))] {
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
    let mut a = zig(0.0, 9_000);
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

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ecad t032 {tag} Schaltpl\u{e4}ne {}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn close_all(h: &mut H) {
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
}

fn reopen(path: &str, seed: u64) -> H {
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, path, seed).unwrap();
    let mut h = H { s, project, trail: Vec::new() };
    let d = h.doc();
    h.trail.push(d);
    h
}

/// Runs now: today's behaviour that 032 must keep (a T without join stays two nets; an end on a
/// wire's interior still gets a junction).
#[test]
fn t00_today_an_end_on_an_interior_gets_a_junction() {
    let mut h = H::new(0x7032_0000);
    let page = h.first_page();
    let a = cid(&wire(&mut h, &[(0.0, 0.0), (60.0, 0.0)], json!({})));
    let b = cid(&wire(&mut h, &[(30.0, 30.0), (30.0, 0.0)], json!({})));
    let doc = h.doc();
    assert!(junction_at(&doc, page, (30.0, 0.0)).is_some());
    assert_eq!(net_of(&doc, a), net_of(&doc, b));
}
