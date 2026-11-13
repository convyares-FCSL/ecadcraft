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
    let mut h = H::new(0x32_13);
    let page = h.first_page();
    wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({}));
    wire(&mut h, &[(50.0, 0.0), (50.0, -20.0)], json!({"join": false}));
    let uid = h.p(|p| p.pages[&page].uid).unwrap();
    let yellow = |h: &mut H| {
        // Brief 040: painted as the warning triangle (was a yellow ring mark).
        h.p(|p| ecad_cadcraft::look::painted_marks(p, uid))
            .into_iter()
            .filter_map(|m| match m {
                ecad_cadcraft::look::Painted::Warning(at) => Some((at.x, at.y)),
                ecad_cadcraft::look::Painted::Error(_) => None,
            })
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

// ---------------------------------------------------------------- Q8: a cable across pages

/// Design 032 §3.1: page 100 wires from `-X01:1…3` to continuations A1…A3 slashed as `-W04`;
/// page 115 continuations A1…A3 and wires on to `-X05:1…3`.
struct Pack {
    h: H,
    p100: PageId,
    p115: PageId,
    cable: ecad_model::CableId,
    first: Vec<ConductorId>,
    second: Vec<ConductorId>,
}

fn pack() -> Pack {
    let mut h = H::new(0x32_20);
    let p100 = h.first_page();
    let mut first: Vec<ConductorId> = Vec::new();
    for (i, y) in [0.0, 10.0, 20.0].into_iter().enumerate() {
        terminal(&mut h, (0.0, y), "X01");
        h.run("place", json!({"symbol": "continuation", "at": [100, y], "name": format!("A{}", i + 1)})).unwrap();
        first.push(id(&wire(&mut h, &[(0.0, y), (100.0, y)], json!({})), "conductor"));
    }
    let r = h.run("cableslash", json!({"from": [50, -5], "to": [50, 25], "tag": "-W04"})).unwrap();
    let cable: ecad_model::CableId = id(&r, "cable");
    for (i, w) in first.iter().enumerate() {
        h.run("cablecore", json!({"conductor": w.hex(), "core": format!("{}", i + 1)})).unwrap();
    }
    let p115: PageId = id(&h.run("newpage", json!({"title": "115"})).unwrap(), "page");
    h.activate(p115);
    let mut second = Vec::new();
    for (i, y) in [0.0, 10.0, 20.0].into_iter().enumerate() {
        h.run("place", json!({"symbol": "continuation", "at": [0, y], "name": format!("A{}", i + 1)})).unwrap();
        terminal(&mut h, (100.0, y), "X05");
        second.push(id(&wire(&mut h, &[(0.0, y), (100.0, y)], json!({})), "conductor"));
    }
    Pack { h, p100, p115, cable, first, second }
}

/// §12 item 13: slashing on page 115 with the cable's tag continues it: one cable, six members,
/// three cores from `-X01:n` to `-X05:n`, no core rows in Checks; erasing that slash leaves the
/// cable with its page-100 members.
#[test]
fn a_cable_continues_through_continuations_by_its_tag() {
    let mut f = pack();
    let before = f.h.doc().cables.len();
    let head = f.h.head();
    let r = f.h.run("cableslash", json!({"from": [50, -5], "to": [50, 25], "tag": "-W04"})).unwrap();
    assert_eq!(f.h.head(), head + 1);
    assert_eq!(r["continued"], json!(true), "{r}");
    assert_eq!(r["cable"], json!(f.cable.hex()));
    let doc = f.h.doc();
    assert_eq!(doc.cables.len(), before, "no new cable");
    let members: Vec<ConductorId> = doc.cables[&f.cable].members.iter().map(|m| m.conductor).collect();
    assert_eq!(members.len(), 6);
    let cores = r["cores"].as_array().unwrap();
    assert_eq!(cores.len(), 3);
    for (c, (w2, w1)) in cores.iter().zip(f.second.iter().zip(&f.first)) {
        assert_eq!(c["conductor"], json!(w2.hex()));
        assert_eq!(c["continues"], json!(w1.hex()));
    }
    let chains = ecad_cadcraft::references::core_chains(&doc, f.cable);
    assert_eq!(chains, f.first.iter().zip(&f.second).map(|(a, b)| vec![*a, *b]).collect::<Vec<_>>());
    let cc = f.h.run("cablecores", json!({"cable": f.cable.hex()})).unwrap();
    let rows = cc["cores"].as_array().unwrap();
    assert_eq!(rows.len(), 3, "{cc}");
    for (i, row) in rows.iter().enumerate() {
        let from = row["from"].as_str().unwrap_or_default();
        let to = row["to"].as_str().unwrap_or_default();
        assert!(from.starts_with("-X01") && from.ends_with(&format!(":{}", i + 1)), "{row}");
        assert!(to.starts_with("-X05") && to.ends_with(&format!(":{}", i + 1)), "{row}");
    }
    let list = f.h.run("cablelist", json!({})).unwrap();
    let text = list.to_string();
    assert!(text.contains("\"cores\":3") || text.contains("\"cores\": 3"), "{list}");
    let checks = f.h.run("checks", json!({"kinds": ["core-twice", "too-many-cores"]})).unwrap();
    assert_eq!(checks["total"], json!(0), "{checks}");
    // Erasing the page-115 slash: the cable keeps its page-100 members.
    let rep: ecad_model::RepresentationId = id(&r, "representation");
    let item: ecad_model::ItemId = id(&r, "item");
    let handle = f.h.handle_of(f.p115, item);
    f.h.run("erase", json!({"handles": [handle.hex()]})).unwrap();
    let doc = f.h.doc();
    assert!(!doc.representations.contains_key(&rep));
    let left: Vec<ConductorId> = doc.cables[&f.cable].members.iter().map(|m| m.conductor).collect();
    assert_eq!(left, f.first);
    let _ = f.p100;
}

/// §12 item 13: the refusals of §3.3.
#[test]
fn continuing_a_cable_is_refused_with_the_reason() {
    let mut f = pack();
    // A wire on page 115 that meets no core through a continuation.
    wire(&mut f.h, &[(0.0, 40.0), (100.0, 40.0)], json!({}));
    let head = f.h.head();
    let m = f.h.run("cableslash", json!({"from": [50, -5], "to": [50, 45], "tag": "-W04"})).unwrap_err();
    assert!(m.contains("does not continue a core of -W04: none of its ends meets -W04 through a continuation."), "{m}");
    assert_eq!(f.h.head(), head);
    // Already slashed on page 100.
    f.h.activate(f.p100);
    let m = f.h.run("cableslash", json!({"from": [60, -5], "to": [60, 25], "cable": f.cable.hex()})).unwrap_err();
    assert!(m.contains("-W04 already has a slash on this page."), "{m}");
    // Two wires continuing one core.
    f.h.activate(f.p115);
    f.h.run("place", json!({"symbol": "continuation", "at": [0, 60], "name": "A1"})).unwrap();
    wire(&mut f.h, &[(0.0, 60.0), (100.0, 60.0)], json!({}));
    let m = f.h.run("cableslash", json!({"from": [50, -5], "to": [50, 65], "tag": "-W04"})).unwrap_err();
    assert!(m.contains("does not continue") || m.contains("Two of the wires continue -W04's core 1."), "{m}");
}

/// §12 item 14: `cablecontinue` on a cable whose cores end on continuations says what to do.
#[test]
fn cablecontinue_says_to_slash_where_the_cores_continue() {
    let mut f = pack();
    let head = f.h.head();
    let m = f.h.run("cablecontinue", json!({"cable": f.cable.hex(), "at": [20, 80], "toward": [40, 80]})).unwrap_err();
    assert!(m.contains("-W04's cores end on continuations. To show -W04 where they continue, slash the wires there with tag -W04."), "{m}");
    assert_eq!(f.h.head(), head);
}

// ---------------------------------------------------------------- Q4: the wire label rule

/// §12 item 15: a 24V bus of 100 mm with four 5 mm drops to `-X01:1…4` (the drops' junctions
/// split the bus into five wires).
fn bus() -> (H, PageId) {
    let mut h = H::new(0x32_40);
    let page = h.first_page();
    wire(&mut h, &[(0.0, 0.0), (100.0, 0.0)], json!({"potential": "24V"}));
    for x in [20.0, 40.0, 60.0, 80.0] {
        terminal(&mut h, (x, -5.0), "X01");
        wire(&mut h, &[(x, 0.0), (x, -5.0)], json!({}));
    }
    (h, page)
}

fn labels(h: &mut H, page: PageId) -> Vec<ecad_cadcraft::wires::WireLabel> {
    h.p(|p| ecad_cadcraft::wires::labels_on_page(p.ledger.document(), page))
}

#[test]
fn the_label_rule_labels_a_net_once_or_every_wire() {
    let (mut h, page) = bus();
    let doc = h.doc();
    assert_eq!(doc.connectivity().nets().count(), 1, "one net");
    assert_eq!(labels(&mut h, page).len(), 1, "net: one label");
    h.run("wiresettings", json!({"label_rule": "each", "label_min_length": 0})).unwrap();
    // The drops' junctions split the bus: five bus pieces and four drops.
    assert_eq!(h.doc().conductors.len(), 9);
    assert_eq!(labels(&mut h, page).len(), 9, "each, 0: every wire");
    h.run("wiresettings", json!({"label_min_length": 10})).unwrap();
    let l = labels(&mut h, page);
    assert_eq!(l.len(), 5, "each, 10: the bus pieces, no 5 mm drop");
    // A drop of the net with its own cross-section is labelled too (owner Q7).
    h.run("wiresettings", json!({"label_rule": "net"})).unwrap();
    let drop = h.doc().conductors.keys().copied().find(|c| {
        let d = h.doc();
        ecad_cadcraft::wires::route_vertices(&d, *c, page).is_some_and(|(_, _, v)| v.len() == 2 && v[0].x == 20.0 && v[0].y == 0.0)
    });
    let drop = drop.expect("the first drop");
    h.run("setprop", json!({"conductor": drop.hex(), "key": "cross_section", "value": "2.5"})).unwrap();
    let l = labels(&mut h, page);
    assert_eq!(l.len(), 2, "{l:?}");
    for (i, a) in l.iter().enumerate() {
        for b in l.iter().skip(i + 1) {
            let (x, y) = (ecad_cadcraft::wires::label_box(a), ecad_cadcraft::wires::label_box(b));
            assert!(x.1.x < y.0.x || y.1.x < x.0.x || x.1.y < y.0.y || y.1.y < x.0.y, "labels touch: {a:?} {b:?}");
        }
    }
    let m = h.run("wiresettings", json!({"label_min_length": 101})).unwrap_err();
    assert!(m.contains("Shortest labelled wire is 0 to 100 mm."), "{m}");
    let m = h.run("wiresettings", json!({"label_rule": "some"})).unwrap_err();
    assert!(m.contains("`label_rule` must be"), "{m}");
}

/// §12 item 21: lane W's three defaults are registered, and the project's value of `wire.join` is
/// what `wire` uses (the label rule's project value is what the label pass uses: the bus test).
#[test]
fn the_join_default_comes_from_the_registry() {
    let keys: Vec<&str> = ecad_cadcraft::defaults::all().map(|e| e.key).collect();
    for k in ["wire.join", "wiring.label_rule", "wiring.label_min_length"] {
        assert!(keys.contains(&k), "{k} not in {keys:?}");
    }
    // The label rule's built-ins are ecad-model's constants.
    let rule = &ecad_cadcraft::defaults::wires::WIRING_LABEL_RULE;
    assert_eq!(rule.builtin, ecad_cadcraft::defaults::Value::Choice(ecad_model::wiring::DEFAULT_LABEL_RULE));
    let mut h = H::new(0x32_41);
    wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({}));
    h.cfg(|p| {
        let before = p.ledger.document().properties.get("wire.join").cloned();
        p.ledger
            .commit(
                ecad_model::Author::User,
                "test default",
                vec![ecad_model::Op::SetProperty {
                    object: ecad_model::ObjectRef::Project,
                    key: "wire.join".into(),
                    before,
                    after: Some("off".into()),
                }],
            )
            .unwrap();
    });
    wire(&mut h, &[(50.0, 0.0), (50.0, -20.0)], json!({}));
    assert_eq!(h.doc().conductors.len(), 2, "wire.join off: the ends stay open");
}

/// Overseer ruling on §2.2: a fold carries the folded wire's own properties onto the survivor
/// (never lost), refuses a value both store differently, and leaves one number on the joined net
/// in the same transaction.
#[test]
fn a_fold_keeps_the_folded_wires_properties() {
    let mut h = H::new(0x32_15);
    let a: ConductorId = id(&wire(&mut h, &[(0.0, 0.0), (40.0, 0.0)], json!({})), "conductor");
    let c: ConductorId = id(&wire(&mut h, &[(60.0, 0.0), (100.0, 0.0)], json!({})), "conductor");
    h.run("setprop", json!({"conductor": c.hex(), "key": "cross_section", "value": "2.5"})).unwrap();
    let head = h.head();
    let r = wire(&mut h, &[(40.0, 0.0), (60.0, 0.0)], json!({}));
    assert_eq!(h.head(), head + 1, "one transaction, numbering included");
    assert_eq!(r["removed"], json!([c.hex()]));
    let doc = h.doc();
    assert_eq!(doc.conductors[&a].properties.get("cross_section").map(String::as_str), Some("2.5"));
    let numbers: std::collections::BTreeSet<_> = doc.conductors.values().filter_map(|x| x.properties.get("wire_number")).collect();
    assert!(numbers.len() <= 1, "{numbers:?}");
    // Both store a value of one key, differently: refused with words.
    let d: ConductorId = id(&wire(&mut h, &[(200.0, 0.0), (240.0, 0.0)], json!({})), "conductor");
    h.run("setprop", json!({"conductor": d.hex(), "key": "cross_section", "value": "1.5"})).unwrap();
    let head = h.head();
    let m = wire_r(&mut h, &[(100.0, 0.0), (200.0, 0.0)], json!({})).unwrap_err();
    assert!(m.contains("different cross-sections (2.5 and 1.5)"), "{m}");
    assert_eq!(h.head(), head);
}

/// Review of #65, finding 1 (design 032 §8.4): renaming a continuation re-counts the cables of its
/// old and new name groups. W04 continued, then page 115's A1 renamed Z1: that core breaks in two.
#[test]
fn renaming_a_continuation_recounts_the_cables_cores() {
    let mut f = pack();
    f.h.run("cableslash", json!({"from": [50, -5], "to": [50, 25], "tag": "-W04"})).unwrap();
    let cores = |h: &mut H, k: ecad_model::CableId| h.run("cablecores", json!({"cable": k.hex()})).unwrap()["cores"].as_array().map_or(0, Vec::len);
    assert_eq!(cores(&mut f.h, f.cable), 3);
    let doc = f.h.doc();
    let a1 = doc
        .continuations_named("A1")
        .into_iter()
        .find(|c| doc.representations_of(ecad_model::Subject::Component(*c)).any(|(_, r)| r.page == f.p115))
        .expect("A1 on page 115");
    f.h.run("setprop", json!({"component": a1.hex(), "key": "name", "value": "Z1"})).unwrap();
    assert_eq!(cores(&mut f.h, f.cable), 4, "the A1 core is two cores now");
    f.h.cmdline("u");
    assert_eq!(cores(&mut f.h, f.cable), 3, "undo joins it again");
}

/// Review of #65, finding 3: wire labels are drawn in lane T's wire-label style (a bold weight
/// gives the bold text style), and their placement box is as wide as that font draws them.
#[test]
fn wire_labels_take_the_wire_label_text_style() {
    let mut h = H::new(0x3216);
    let page = h.first_page();
    wire(&mut h, &[(0.0, 0.0), (60.0, 0.0)], json!({}));
    let regular = h.p(|p| ecad_cadcraft::wires::labels_on_page(p.ledger.document(), page));
    assert_eq!(regular.len(), 1);
    h.run("textstyle", json!({"kind": "wire_label", "weight": "bold"})).unwrap();
    let (bold, width) = h.p(|p| {
        let doc = p.ledger.document();
        let l = ecad_cadcraft::wires::labels_on_page(doc, page);
        let w = l.first().map(|x| ecad_cadcraft::text_style::text_width(doc, ecad_cadcraft::text_style::TextKind::WireLabel, &x.text));
        (l, w)
    });
    let l = &bold[0];
    assert!((l.width - width.unwrap()).abs() < 1e-9, "the box is the font's width");
    let look = h.p(|p| ecad_cadcraft::text_style::Look::new(p.ledger.document()));
    let e = ecad_cadcraft::wires::label_entity(&look, l, cadcraft_engine::doc::Handle(1));
    let cadcraft_engine::doc::EntityKind::Text(t) = &e.kind else { panic!("{e:?}") };
    assert_eq!(t.style, ecad_cadcraft::text_style::STYLE_BOLD);
}

/// Review of #65, finding 5: the quiet ends (a cable reference end) come from the document, the
/// same with no projected page as the page map gives, and a wire ending there stays open.
#[test]
fn quiet_ends_come_from_the_document() {
    let mut h = H::new(0x32_17);
    let page = h.first_page();
    for y in [0.0, 5.0, 10.0] {
        terminal(&mut h, (0.0, y), "X01");
        wire(&mut h, &[(0.0, y), (60.0, y)], json!({}));
    }
    let k: ecad_model::CableId = id(&h.run("cableslash", json!({"from": [30, -3], "to": [30, 13]})).unwrap(), "cable");
    h.run("cableref", json!({"cable": k.hex(), "at": [65, 5]})).unwrap();
    // Continued on another page: its members leave page 1 at the reference (brief 010 §6.4).
    let other: PageId = id(&h.run("newpage", json!({"title": "Field"})).unwrap(), "page");
    h.activate(other);
    h.run("cablecontinue", json!({"cable": k.hex(), "at": [20, 5], "toward": [40, 5]})).unwrap();
    h.activate(page);
    let doc = h.doc();
    // No project, no page map: the document alone.
    let quiet = ecad_cadcraft::wires::quiet_ends_of(&doc, page, &std::collections::BTreeSet::new(), 1e-6);
    let shown = h.p(|p| p.pages.get(&page).map(|v| v.map.quiet_ends().to_vec()).unwrap_or_default());
    assert!(
        !quiet.is_empty(),
        "the ends at the reference are quiet: {quiet:?} shown {shown:?} markers {}",
        ecad_cadcraft::references::markers_on_page(&doc, page).len()
    );
    for q in &shown {
        assert!(quiet.iter().any(|x| x.near(*q, 1e-9)), "{q:?} not in {quiet:?}");
    }
    // A wire from a quiet end stays a wire of its own.
    let at = quiet[0];
    let before = h.doc().conductors.len();
    let r = wire(&mut h, &[(at.x, at.y), (at.x + 20.0, at.y - 30.0)], json!({}));
    assert_eq!(r["created"], json!(true), "{r}");
    assert_eq!(h.doc().conductors.len(), before + 1);
}

// ---------------------------------------------------------------- update maps (§8, §12 item 17)

/// The pages a command marked for redrawing (`None`: every page).
fn marked(h: &mut H, cmd: &str, params: Value) -> Option<std::collections::BTreeSet<PageId>> {
    let n = h.p(|p| p.change_count());
    h.run(cmd, params).unwrap();
    h.p(|p| p.changes_since(n))
}

/// The kept Checks rows and cable rows equal a rebuild from the document (§8.3, §8.4).
fn kept_equals_rebuild(h: &mut H, what: &str) {
    h.cfg(|p| {
        p.checks();
        let doc = p.ledger.document();
        let kept_checks = p.checks.as_ref().map(|c| c.sorted(doc)).unwrap_or_default();
        let fresh = ecad_cadcraft::checks::Checks::build(doc, p.tolerance).sorted(doc);
        assert_eq!(kept_checks, fresh, "{what}: Checks differ from a rebuild");
        p.outputs();
        let doc = p.ledger.document();
        let kept = p.outputs.as_ref().map(|o| o.cables().into_iter().cloned().collect::<Vec<_>>()).unwrap_or_default();
        let rebuilt = ecad_cadcraft::outputs::Outputs::build(doc);
        let fresh: Vec<_> = rebuilt.cables().into_iter().cloned().collect();
        assert_eq!(kept, fresh, "{what}: cable rows differ from a rebuild");
    });
}

/// §8.1 and §8.2: a route op on a page marks that page only; a fold marks its page only (not
/// every page); a change on another page leaves this one; wiring settings mark every page.
#[test]
fn label_and_mark_updates_reach_only_the_pages_they_name() {
    let mut f = pack();
    let p115 = f.p115;
    f.h.activate(f.p100);
    let m = marked(&mut f.h, "wire", json!({"points": [[200, 0], [240, 0]]}));
    assert_eq!(m, Some([f.p100].into()), "a route on page 100");
    f.h.run("wire", json!({"points": [[260, 0], [300, 0]]})).unwrap();
    let m = marked(&mut f.h, "wire", json!({"points": [[240, 0], [260, 0]]}));
    assert_eq!(m, Some([f.p100].into()), "a fold: its page only");
    let m = marked(&mut f.h, "wiresettings", json!({"label_rule": "each"}));
    assert!(m.is_none() || m.as_ref().is_some_and(|s| s.contains(&p115)), "settings: every page, {m:?}");
    // The looks-connected marks of page 100 follow its projection: none on page 115 change.
    f.h.activate(p115);
    let before = f.h.p(|p| p.pages.get(&p115).map(|v| v.map.looks_connected().to_vec()).unwrap_or_default());
    f.h.activate(f.p100);
    f.h.run("wire", json!({"points": [[300, 0], [300, -20]], "join": false})).unwrap();
    f.h.activate(p115);
    let after = f.h.p(|p| p.pages.get(&p115).map(|v| v.map.looks_connected().to_vec()).unwrap_or_default());
    assert_eq!(before, after, "page 115's marks are untouched by page 100");
}

/// §8.3 and §8.4: after each op of the rows (members, a slash, a member's end, a continuation's
/// name) and after an unrelated op, the kept Checks and cable rows equal a rebuild; the unrelated
/// op changes no cable row.
#[test]
fn chain_updates_in_checks_and_outputs_match_a_rebuild() {
    let mut f = pack();
    kept_equals_rebuild(&mut f.h, "start");
    f.h.run("cableslash", json!({"from": [50, -5], "to": [50, 25], "tag": "-W04"})).unwrap();
    kept_equals_rebuild(&mut f.h, "slash continuing the cable");
    let doc = f.h.doc();
    let a2 = doc
        .continuations_named("A2")
        .into_iter()
        .find(|c| doc.representations_of(ecad_model::Subject::Component(*c)).any(|(_, r)| r.page == f.p115))
        .unwrap();
    f.h.run("setprop", json!({"component": a2.hex(), "key": "name", "value": "Z2"})).unwrap();
    kept_equals_rebuild(&mut f.h, "continuation renamed");
    f.h.run("disconnect", json!({"conductor": f.second[0].hex(), "end": "A"})).unwrap();
    kept_equals_rebuild(&mut f.h, "a member's end detached");
    // Unrelated: a wire far away on page 115 changes no cable row.
    f.h.run("wire", json!({"points": [[400, 400], [440, 400]]})).unwrap();
    let changed = f.h.p(|p| p.outputs.as_ref().is_some_and(|o| o.last_cables_changed));
    assert!(!changed, "an unrelated wire changes no cable row");
    kept_equals_rebuild(&mut f.h, "unrelated wire");
}

/// Review of #65, finding 1, at the update map itself (§8.4): the rename op alone (no numbering
/// pass, which would also bring the cable in) re-counts the cable's cores.
#[test]
fn the_outputs_map_takes_a_continuation_rename_to_its_cables() {
    let mut f = pack();
    f.h.run("cableslash", json!({"from": [50, -5], "to": [50, 25], "tag": "-W04"})).unwrap();
    let doc = f.h.doc();
    let a1 = doc
        .continuations_named("A1")
        .into_iter()
        .find(|c| doc.representations_of(ecad_model::Subject::Component(*c)).any(|(_, r)| r.page == f.p115))
        .unwrap();
    let cable = f.cable;
    let (kept, fresh) = f.h.cfg(|p| {
        p.outputs();
        let before = p.ledger.document().clone();
        let ops = vec![ecad_model::Op::SetProperty {
            object: ecad_model::ObjectRef::Component(a1),
            key: "name".into(),
            before: Some("A1".into()),
            after: Some("Z1".into()),
        }];
        let scope = p.outputs.as_ref().unwrap().scope(&before, &ops);
        p.ledger.commit(ecad_model::Author::User, "rename", ops.clone()).unwrap();
        let doc = p.ledger.document();
        p.outputs.as_mut().unwrap().update(doc, &ops, scope);
        let kept = p.outputs.as_ref().unwrap().cable_row(cable).cloned();
        let fresh = ecad_cadcraft::outputs::Outputs::build(doc).cable_row(cable).cloned();
        (kept, fresh)
    });
    assert_eq!(kept.as_ref().map(|r| r.cores), Some(4), "{kept:?}");
    assert_eq!(kept, fresh);
}
