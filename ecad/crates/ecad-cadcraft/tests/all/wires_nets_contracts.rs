//! Brief 032 acceptance (design 032 §12): wires that meet end to end join, the reverse T, the way
//! out (`join: false`), `joinwires`, a cable continued across pages, and the wire label rule.
//! Driven through the commands' JSON forms.

use ecad_cadcraft::checks::{Fix, Kind};
use ecad_model::{ConductorId, Document, PageId, Pt};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

fn wire_r(h: &mut H, points: &[(f64, f64)], extra: Value) -> Result<Value, String> {
    let mut params = json!({"points": pts(points)});
    if let (Some(o), Some(e)) = (params.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            o.insert(k.clone(), v.clone());
        }
    }
    h.run("wire", params)
}

fn wire(h: &mut H, points: &[(f64, f64)], extra: Value) -> Value {
    wire_r(h, points, extra).unwrap()
}

fn net_of(doc: &Document, c: ConductorId) -> Option<ecad_model::NetId> {
    doc.connectivity().net_of_conductor(c)
}

fn route(h: &mut H, page: PageId, c: ConductorId) -> Vec<Pt> {
    h.p(|p| ecad_cadcraft::wires::route_vertices(p.ledger.document(), c, page).map(|(_, _, v)| v).unwrap_or_default())
}

fn same(a: &[Pt], b: &[(f64, f64)]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(p, (x, y))| p.near(Pt::new(*x, *y), 1e-9))
}

fn terminal(h: &mut H, at: (f64, f64), tag: &str) -> ecad_model::TerminalId {
    let r = h.run("place", json!({"symbol": "terminal", "at": [at.0, at.1], "tag": tag})).unwrap();
    id(&r["terminals"][0], "terminal")
}

/// §12 item 1, the regression: a wire drawn from another wire's open end joins it. On `6729c8f1`
/// the second wire was a conductor and a net of its own.
#[test]
fn a_wire_from_an_open_end_joins_it() {
    let mut h = H::new(0x32_01);
    let page = h.first_page();
    let first: ConductorId = id(&wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({})), "conductor");
    let r = wire(&mut h, &[(50.0, 0.0), (50.0, -20.0)], json!({}));
    let second: ConductorId = id(&r, "conductor");
    let doc = h.doc();
    assert_eq!(doc.conductors.len(), 1, "one conductor: {r}");
    assert_eq!(second, first, "the drawn route belongs to the existing wire: {r}");
    assert_eq!(r["created"], json!(false), "{r}");
    assert_eq!(r["joined"][0]["how"], json!("merged"), "{r}");
    assert_eq!(doc.connectivity().nets().count(), 1);
    assert!(net_of(&doc, first).is_some());
    let v = route(&mut h, page, first);
    assert!(same(&v, &[(0.0, 0.0), (50.0, 0.0), (50.0, -20.0)]), "{v:?}");
}

fn ends_of(doc: &Document, c: ConductorId) -> [Option<ecad_model::TerminalId>; 2] {
    doc.conductors.get(&c).map(|x| x.ends).unwrap_or_default()
}

fn junction_count(doc: &Document) -> usize {
    doc.components.keys().filter(|c| ecad_cadcraft::wires::kind_of(doc, **c) == Some("junction")).count()
}

/// §12 item 2: end to end in one line drops the middle vertex; drawn onto end A, the wire keeps
/// its end letters.
#[test]
fn a_straight_join_drops_the_middle_vertex() {
    let mut h = H::new(0x32_02);
    let page = h.first_page();
    let c: ConductorId = id(&wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({})), "conductor");
    wire(&mut h, &[(50.0, 0.0), (80.0, 0.0)], json!({}));
    assert_eq!(h.doc().conductors.len(), 1);
    let v = route(&mut h, page, c);
    assert!(same(&v, &[(0.0, 0.0), (80.0, 0.0)]), "{v:?}");
    wire(&mut h, &[(-20.0, 0.0), (0.0, 0.0)], json!({}));
    let v = route(&mut h, page, c);
    assert!(same(&v, &[(-20.0, 0.0), (80.0, 0.0)]), "{v:?}");
    assert_eq!(h.doc().conductors.len(), 1);
}

/// §12 items 3 and 12: a wire between two open ends folds them into one; the far attachment is on
/// the survivor; `u` undoes it in one step.
#[test]
fn both_ends_fold_two_wires_into_one() {
    let mut h = H::new(0x32_03);
    let page = h.first_page();
    let x = terminal(&mut h, (100.0, 0.0), "X1");
    let a: ConductorId = id(&wire(&mut h, &[(0.0, 0.0), (40.0, 0.0)], json!({})), "conductor");
    let c: ConductorId = id(&wire(&mut h, &[(60.0, 10.0), (100.0, 0.0)], json!({})), "conductor");
    assert_eq!(ends_of(&h.doc(), c)[1], Some(x));
    let before = h.doc();
    let head = h.head();
    let r = wire(&mut h, &[(40.0, 0.0), (60.0, 10.0)], json!({}));
    assert_eq!(h.head(), head + 1);
    let doc = h.doc();
    assert_eq!(doc.conductors.len(), 1, "{r}");
    assert_eq!(r["conductor"], json!(a.hex()));
    assert_eq!(r["removed"], json!([c.hex()]));
    assert!(!doc.conductors.contains_key(&c));
    assert_eq!(ends_of(&doc, a), [None, Some(x)]);
    let v = route(&mut h, page, a);
    assert!(same(&v, &[(0.0, 0.0), (40.0, 0.0), (60.0, 10.0), (100.0, 0.0)]), "{v:?}");
    h.cmdline("u");
    assert_eq!(h.doc().conductors, before.conductors);
}

/// §12 item 4: the wire with a fixed number survives; two different fixed numbers are refused.
#[test]
fn a_fixed_number_survives_and_two_are_refused() {
    let mut h = H::new(0x32_04);
    let a: ConductorId = id(&wire(&mut h, &[(0.0, 0.0), (40.0, 0.0)], json!({})), "conductor");
    let c: ConductorId = id(&wire(&mut h, &[(60.0, 0.0), (100.0, 0.0)], json!({})), "conductor");
    h.run("wirenumber", json!({"conductor": c.hex(), "value": "W-7"})).unwrap();
    let r = wire(&mut h, &[(40.0, 0.0), (60.0, 0.0)], json!({}));
    assert_eq!(r["conductor"], json!(c.hex()), "{r}");
    assert_eq!(r["removed"], json!([a.hex()]));
    assert_eq!(r["number"], json!("W-7"));
    let d: ConductorId = id(&wire(&mut h, &[(200.0, 0.0), (240.0, 0.0)], json!({})), "conductor");
    h.run("wirenumber", json!({"conductor": d.hex(), "value": "W-9"})).unwrap();
    let head = h.head();
    let m = wire_r(&mut h, &[(100.0, 0.0), (200.0, 0.0)], json!({})).unwrap_err();
    assert!(m.contains("Both wires have fixed numbers of their own (W-7 and W-9)"), "{m}");
    assert!(m.contains("join: false"), "{m}");
    assert_eq!(h.head(), head, "nothing written");
}

/// §12 item 5: different potentials are refused with the reason and the way out.
#[test]
fn different_potentials_are_refused() {
    let mut h = H::new(0x32_05);
    wire(&mut h, &[(0.0, 0.0), (40.0, 0.0)], json!({"potential": "0V"}));
    let head = h.head();
    let m = wire_r(&mut h, &[(40.0, 0.0), (40.0, -20.0)], json!({"potential": "24V"})).unwrap_err();
    assert!(m.contains("That wire carries 0V and this one 24V; joining them would short them."), "{m}");
    assert!(m.contains("Draw it with join off (join: false), or end it on a connection point."), "{m}");
    assert_eq!(h.head(), head);
    let r = wire(&mut h, &[(40.0, 0.0), (40.0, -20.0)], json!({"potential": "0V"}));
    assert_eq!(r["created"], json!(false));
}

/// §12 item 6: both ends on the two ends of one wire close a loop: refused.
#[test]
fn a_loop_is_refused() {
    let mut h = H::new(0x32_06);
    wire(&mut h, &[(0.0, 0.0), (40.0, 0.0), (40.0, 20.0)], json!({}));
    let head = h.head();
    let m = wire_r(&mut h, &[(40.0, 20.0), (0.0, 20.0), (0.0, 0.0)], json!({})).unwrap_err();
    assert!(m.contains("close it into a loop"), "{m}");
    assert_eq!(h.head(), head);
}

/// §12 item 7: three ends at one point meet at one junction.
#[test]
fn three_ends_meet_at_one_junction() {
    let mut h = H::new(0x32_07);
    let a: ConductorId = id(&wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({})), "conductor");
    let b: ConductorId = id(&wire(&mut h, &[(50.0, 0.0), (50.0, 30.0)], json!({"join": false})), "conductor");
    let r = wire(&mut h, &[(50.0, 0.0), (90.0, 0.0)], json!({}));
    let doc = h.doc();
    assert_eq!(junction_count(&doc), 1, "{r}");
    assert_eq!(doc.conductors.len(), 3);
    assert_eq!(r["joined"][0]["how"], json!("junction"));
    let w: ConductorId = id(&r, "conductor");
    let j = ends_of(&doc, w)[0];
    assert!(j.is_some());
    assert_eq!(ends_of(&doc, a)[1], j);
    assert_eq!(ends_of(&doc, b)[0], j);
    assert_eq!(net_of(&doc, a), net_of(&doc, w));
}

/// §12 items 8 and 12: the reverse T: an open end on the new wire's interior splits it with a
/// junction.
#[test]
fn an_open_end_on_the_new_wire_joins_it() {
    let mut h = H::new(0x32_08);
    let c: ConductorId = id(&wire(&mut h, &[(30.0, 20.0), (30.0, -10.0)], json!({})), "conductor");
    let head = h.head();
    let r = wire(&mut h, &[(0.0, -10.0), (60.0, -10.0)], json!({}));
    let doc = h.doc();
    assert_eq!(junction_count(&doc), 1, "{r}");
    assert_eq!(doc.conductors.len(), 3, "W is two pieces: {r}");
    let split = &r["splits"][0];
    assert_eq!(split["at"], json!([30.0, -10.0]));
    let w: ConductorId = id(&r, "conductor");
    let w2: ConductorId = id(split, "conductor");
    let j = ends_of(&doc, c)[1];
    assert!(j.is_some());
    assert_eq!(ends_of(&doc, w)[1], j);
    assert_eq!(ends_of(&doc, w2)[0], j);
    assert_eq!(net_of(&doc, c), net_of(&doc, w));
    assert_eq!(net_of(&doc, c), net_of(&doc, w2));
    assert_eq!(h.head(), head + 1);
    h.cmdline("u");
    assert_eq!(h.doc().conductors.len(), 1);
}

/// §12 item 9: `join: false` (or `junction: false`) leaves both ends open; Checks lists Looks
/// connected with Join and two Open end rows; `join` and `junction` that disagree are refused.
#[test]
fn join_false_leaves_the_ends_open() {
    for key in ["join", "junction"] {
        let mut h = H::new(0x32_09);
        terminal(&mut h, (0.0, 0.0), "X1");
        terminal(&mut h, (50.0, -20.0), "X1");
        wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({}));
        wire(&mut h, &[(50.0, 0.0), (50.0, -20.0)], json!({key: false}));
        let doc = h.doc();
        assert_eq!(doc.conductors.len(), 2, "{key}");
        let all = ecad_cadcraft::checks::all(&doc, 1e-6);
        let looks: Vec<_> = all.iter().filter(|c| c.kind == Kind::LooksConnected).collect();
        assert_eq!(looks.len(), 1, "{all:?}");
        assert_eq!(looks[0].kind.name(), "looks-connected");
        assert_eq!(looks[0].kind.words(), "Looks connected");
        assert!(matches!(looks[0].fix, Some(Fix::Join(_))), "{looks:?}");
        assert_eq!(all.iter().filter(|c| c.kind == Kind::OpenEnd).count(), 2, "{all:?}");
    }
    let mut h = H::new(0x32_19);
    let m = wire_r(&mut h, &[(0.0, 0.0), (5.0, 0.0)], json!({"join": true, "junction": false})).unwrap_err();
    assert!(m.contains("`join` and `junction` disagree; give only `join`"), "{m}");
}

/// §12 item 10: an end landing on a cable core's open end gets a two-wire junction, never a merge.
#[test]
fn a_cable_cores_open_end_takes_a_junction() {
    let mut h = H::new(0x32_10);
    let core: ConductorId = id(&wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({})), "conductor");
    h.run("cableslash", json!({"from": [25, 10], "to": [25, -10]})).unwrap();
    assert_eq!(h.doc().cables_of(core).len(), 1);
    let r = wire(&mut h, &[(50.0, 0.0), (50.0, -20.0)], json!({}));
    let doc = h.doc();
    assert_eq!(r["created"], json!(true), "{r}");
    assert_eq!(doc.conductors.len(), 2);
    assert_eq!(junction_count(&doc), 1);
    let w: ConductorId = id(&r, "conductor");
    assert_eq!(net_of(&doc, core), net_of(&doc, w));
}

/// §12 items 11 and 12: `joinwires` merges ends that meet; nothing to join is refused.
#[test]
fn joinwires_repairs_ends_that_meet() {
    let mut h = H::new(0x32_11);
    let page = h.first_page();
    let a: ConductorId = id(&wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({})), "conductor");
    wire(&mut h, &[(50.0, 0.0), (50.0, -20.0)], json!({"join": false}));
    let head = h.head();
    let r = h.run("joinwires", json!({"at": [50, 0]})).unwrap();
    assert_eq!(h.head(), head + 1);
    assert_eq!(r["how"], json!("merged"), "{r}");
    let doc = h.doc();
    assert_eq!(doc.conductors.len(), 1);
    let survivor: ConductorId = id(&r, "conductor");
    assert_eq!(survivor, a, "the lower ID survives");
    let v = route(&mut h, page, a);
    assert!(same(&v, &[(0.0, 0.0), (50.0, 0.0), (50.0, -20.0)]), "{v:?}");
    let all = ecad_cadcraft::checks::all(&doc, 1e-6);
    assert!(all.iter().all(|c| c.kind != Kind::LooksConnected), "{all:?}");
    let m = h.run("joinwires", json!({"at": [500, 500]})).unwrap_err();
    assert!(m.contains("There are no wire ends to join here."), "{m}");
    h.cmdline("u");
    assert_eq!(h.doc().conductors.len(), 2);
}

/// §12 item 11: `joinwires` on an open end lying on a wire's interior places a junction.
#[test]
fn joinwires_on_a_tee_places_a_junction() {
    let mut h = H::new(0x32_12);
    wire(&mut h, &[(0.0, 0.0), (60.0, 0.0)], json!({}));
    wire(&mut h, &[(30.0, 30.0), (30.0, 0.0)], json!({"join": false}));
    let r = h.run("joinwires", json!({"at": [30, 0]})).unwrap();
    assert_eq!(r["how"], json!("junction"), "{r}");
    assert_eq!(junction_count(&h.doc()), 1);
}

/// §12 item 19: after `join: false` the page shows a yellow ring at the point; after
/// `joinwires`, none.
#[test]
fn a_yellow_ring_marks_ends_that_look_connected() {
    use cadcraft_engine::ext::MarkShape;
    let mut h = H::new(0x32_13);
    let page = h.first_page();
    wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({}));
    wire(&mut h, &[(50.0, 0.0), (50.0, -20.0)], json!({"join": false}));
    let uid = h.p(|p| p.pages[&page].uid).unwrap();
    let yellow = |h: &mut H| {
        h.p(|p| ecad_cadcraft::interact::overlay_marks(p, uid))
            .into_iter()
            .filter(|m| m.shape == MarkShape::Ring && (m.stroke - 1.5).abs() < 1e-6 && (m.size - 10.0).abs() < 1e-6)
            .map(|m| (m.at.x, m.at.y))
            .collect::<Vec<_>>()
    };
    assert_eq!(yellow(&mut h), vec![(50.0, 0.0)]);
    h.run("joinwires", json!({"at": [50, 0]})).unwrap();
    assert!(yellow(&mut h).is_empty());
}

/// §12 item 18: the Wire tool joins by default; `J` turns it off, the prompt says so, and the
/// wire is drawn with its ends open.
#[test]
fn the_wire_tool_has_a_join_keyword() {
    let mut h = H::new(0x32_14);
    wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({}));
    h.s.start("wire").unwrap();
    h.s.cmdline("50,0").unwrap();
    h.s.cmdline("J").unwrap();
    let prompt = h.s.current_prompt().map(|p| p.display()).unwrap_or_default();
    assert!(prompt.contains("(join off)"), "{prompt}");
    h.s.cmdline("50,-20").unwrap();
    h.cmdline("");
    assert_eq!(h.doc().conductors.len(), 2, "join off: two wires");
    h.s.start("wire").unwrap();
    h.s.cmdline("0,0").unwrap();
    h.s.cmdline("-30,0").unwrap();
    h.cmdline("");
    assert_eq!(h.doc().conductors.len(), 2, "joining is on again when the tool starts");
}

