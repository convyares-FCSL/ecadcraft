//! Brief 032 acceptance (design 032 §12): wires that meet end to end join, the reverse T, the way
//! out (`join: false`), `joinwires`, a cable continued across pages, and the wire label rule.
//! Driven through the commands' JSON forms.

use ecad_cadcraft::checks::{Fix, Kind};
use ecad_model::{PageId, Pt, RouteEnd, RouteId, WireId};
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

fn route(h: &mut H, r: RouteId) -> Vec<Pt> {
    h.p(|p| p.ledger.document().routes.get(&r).map(|x| x.points.clone()).unwrap_or_default())
}

fn same(a: &[Pt], b: &[(f64, f64)]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(p, (x, y))| p.near(Pt::new(*x, *y), 1e-9))
}

fn terminal(h: &mut H, at: (f64, f64), tag: &str) -> ecad_model::TerminalId {
    let r = h.run("place", json!({"symbol": "terminal", "at": [at.0, at.1], "tag": tag})).unwrap();
    id(&r["terminals"][0], "terminal")
}

/// §12 item 1, the regression: a line drawn from another line's free end joins it: one route
/// (design 072 §4.3).
#[test]
fn a_wire_from_an_open_end_joins_it() {
    let mut h = H::new(0x32_01);
    let first: RouteId = id(&wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({})), "route");
    let r = wire(&mut h, &[(50.0, 0.0), (50.0, -20.0)], json!({}));
    let second: RouteId = id(&r, "route");
    let doc = h.doc();
    assert_eq!(doc.routes.len(), 1, "one route: {r}");
    assert_eq!(second, first, "the drawn line belongs to the existing route: {r}");
    assert_eq!(r["created"], json!(false), "{r}");
    assert_eq!(r["joined"][0]["how"], json!("merged"), "{r}");
    let v = route(&mut h, first);
    assert!(same(&v, &[(0.0, 0.0), (50.0, 0.0), (50.0, -20.0)]), "{v:?}");
}

/// §12 item 2: end to end in one line drops the middle vertex; drawn onto end A, the route keeps
/// its end letters.
#[test]
fn a_straight_join_drops_the_middle_vertex() {
    let mut h = H::new(0x32_02);
    let c: RouteId = id(&wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({})), "route");
    wire(&mut h, &[(50.0, 0.0), (80.0, 0.0)], json!({}));
    assert_eq!(h.doc().routes.len(), 1);
    let v = route(&mut h, c);
    assert!(same(&v, &[(0.0, 0.0), (80.0, 0.0)]), "{v:?}");
    wire(&mut h, &[(-20.0, 0.0), (0.0, 0.0)], json!({}));
    let v = route(&mut h, c);
    assert!(same(&v, &[(-20.0, 0.0), (80.0, 0.0)]), "{v:?}");
    assert_eq!(h.doc().routes.len(), 1);
}

/// §12 items 3 and 12: a line between two free ends folds them into one route; the far landing is
/// on the survivor; `u` undoes it in one step.
#[test]
fn both_ends_fold_two_wires_into_one() {
    let mut h = H::new(0x32_03);
    let x = terminal(&mut h, (100.0, 0.0), "X1");
    let a: RouteId = id(&wire(&mut h, &[(0.0, 0.0), (40.0, 0.0)], json!({})), "route");
    let c: RouteId = id(&wire(&mut h, &[(60.0, 10.0), (100.0, 0.0)], json!({})), "route");
    assert_eq!(h.doc().routes[&c].ends[1], RouteEnd::Point(x));
    let before = h.doc();
    let head = h.head();
    let r = wire(&mut h, &[(40.0, 0.0), (60.0, 10.0)], json!({}));
    assert_eq!(h.head(), head + 1);
    let doc = h.doc();
    assert_eq!(doc.routes.len(), 1, "{r}");
    assert_eq!(r["route"], json!(a.hex()));
    assert_eq!(r["removed"], json!([c.hex()]));
    assert!(!doc.routes.contains_key(&c));
    assert_eq!(doc.routes[&a].ends, [RouteEnd::Free, RouteEnd::Point(x)]);
    let v = route(&mut h, a);
    assert!(same(&v, &[(0.0, 0.0), (40.0, 0.0), (60.0, 10.0), (100.0, 0.0)]), "{v:?}");
    h.cmdline("u");
    assert_eq!(h.doc().routes, before.routes);
}

/// §12 item 6 (design 072 §4.3): joining a line's own two ends would close it into a loop:
/// refused. (Drawing onto a route's two ends lands on it there, a T.)
#[test]
fn a_loop_is_refused() {
    let mut h = H::new(0x32_06);
    wire(&mut h, &[(0.0, 0.0), (40.0, 0.0), (40.0, 20.0), (0.0, 20.0), (0.0, 0.0)], json!({"join": false}));
    let head = h.head();
    let m = h.run("joinwires", json!({"at": [0, 0]})).unwrap_err();
    assert!(m.contains("close it into a loop"), "{m}");
    assert_eq!(h.head(), head);
}

/// §12 items 8 and 12: the reverse T: a free end on the new line's interior lands on it (design
/// 072 §4.3), unsplit.
#[test]
fn an_open_end_on_the_new_wire_joins_it() {
    let mut h = H::new(0x32_08);
    let c: RouteId = id(&wire(&mut h, &[(30.0, 20.0), (30.0, -10.0)], json!({})), "route");
    let head = h.head();
    let r = wire(&mut h, &[(0.0, -10.0), (60.0, -10.0)], json!({}));
    let doc = h.doc();
    assert_eq!(doc.routes.len(), 2, "{r}");
    assert_eq!(r["tees"][0]["at"], json!([30.0, -10.0]), "{r}");
    let w: RouteId = id(&r, "route");
    assert_eq!(doc.routes[&c].ends[1].route(), Some(w), "{r}");
    assert_eq!(h.head(), head + 1);
    h.cmdline("u");
    assert_eq!(h.doc().routes.len(), 1);
}

/// §12 item 9: `join: false` leaves both ends free; Checks lists Looks connected with Join and two
/// Open end rows; the old name `junction` is refused (lane 072 D).
#[test]
fn join_false_leaves_the_ends_open() {
    {
        let key = "join";
        let mut h = H::new(0x32_09);
        terminal(&mut h, (0.0, 0.0), "X1");
        terminal(&mut h, (50.0, -20.0), "X1");
        wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({}));
        wire(&mut h, &[(50.0, 0.0), (50.0, -20.0)], json!({key: false}));
        let doc = h.doc();
        assert_eq!(doc.routes.len(), 2, "{key}");
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
    assert!(m.contains("`junction` is not a parameter"), "{m}");
}

/// §12 items 11 and 12: `joinwires` merges ends that meet; nothing to join is refused.
#[test]
fn joinwires_repairs_ends_that_meet() {
    let mut h = H::new(0x32_11);
    let a: RouteId = id(&wire(&mut h, &[(0.0, 0.0), (50.0, 0.0)], json!({})), "route");
    wire(&mut h, &[(50.0, 0.0), (50.0, -20.0)], json!({"join": false}));
    let head = h.head();
    let r = h.run("joinwires", json!({"at": [50, 0]})).unwrap();
    assert_eq!(h.head(), head + 1);
    assert_eq!(r["how"], json!("merged"), "{r}");
    let doc = h.doc();
    assert_eq!(doc.routes.len(), 1);
    let survivor: RouteId = id(&r, "route");
    let v = route(&mut h, survivor);
    assert!(same(&v, &[(0.0, 0.0), (50.0, 0.0), (50.0, -20.0)]) || same(&v, &[(50.0, -20.0), (50.0, 0.0), (0.0, 0.0)]), "{v:?}");
    let _ = a;
    let all = ecad_cadcraft::checks::all(&doc, 1e-6);
    assert!(all.iter().all(|c| c.kind != Kind::LooksConnected), "{all:?}");
    let m = h.run("joinwires", json!({"at": [500, 500]})).unwrap_err();
    assert!(m.contains("There are no wire ends to join here."), "{m}");
    h.cmdline("u");
    assert_eq!(h.doc().routes.len(), 2);
}

/// §12 item 11: `joinwires` on a free end lying on a route's interior lands it there (a T).
#[test]
fn joinwires_on_a_tee_places_a_junction() {
    let mut h = H::new(0x32_12);
    let main: RouteId = id(&wire(&mut h, &[(0.0, 0.0), (60.0, 0.0)], json!({})), "route");
    let tee: RouteId = id(&wire(&mut h, &[(30.0, 30.0), (30.0, 0.0)], json!({"join": false})), "route");
    let r = h.run("joinwires", json!({"at": [30, 0]})).unwrap();
    assert_eq!(r["how"], json!("tee"), "{r}");
    assert_eq!(h.doc().routes[&tee].ends[1].route(), Some(main));
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
    assert_eq!(h.doc().routes.len(), 2, "join off: two lines");
    h.s.start("wire").unwrap();
    h.s.cmdline("0,0").unwrap();
    h.s.cmdline("-30,0").unwrap();
    h.cmdline("");
    assert_eq!(h.doc().routes.len(), 2, "joining is on again when the tool starts");
}

// ---------------------------------------------------------------- Q8: a cable across pages

/// Design 032 §3.1, as design 072 §4.4 (F5) has it: page 100 lines from `-X01:1…3` to
/// continuations A1…A3, slashed as `-W04`; page 115 continuations A1…A3 and lines on to
/// `-X05:1…3`. Continuations are named points, so each core is one wire from `-X01:n` to `-X05:n`.
struct Pack {
    h: H,
    p100: PageId,
    p115: PageId,
    cable: ecad_model::CableId,
    /// The three wires, `-X01:n` to `-X05:n`: the cable's cores.
    wires: Vec<WireId>,
}

fn pack() -> Pack {
    let mut h = H::new(0x32_20);
    let p100 = h.first_page();
    for (i, y) in [0.0, 10.0, 20.0].into_iter().enumerate() {
        terminal(&mut h, (0.0, y), "X01");
        h.run("place", json!({"symbol": "continuation", "at": [100, y], "name": format!("A{}", i + 1)})).unwrap();
        let r = wire(&mut h, &[(0.0, y), (100.0, y)], json!({}));
        assert!(r["conductor"].is_null(), "no partner yet: no wire, {r}");
    }
    let r = h.run("cableslash", json!({"from": [50, -5], "to": [50, 25], "tag": "-W04"})).unwrap();
    let cable: ecad_model::CableId = id(&r, "cable");
    let p115: PageId = id(&h.run("newpage", json!({"title": "115"})).unwrap(), "page");
    h.activate(p115);
    let mut wires: Vec<WireId> = Vec::new();
    for (i, y) in [0.0, 10.0, 20.0].into_iter().enumerate() {
        h.run("place", json!({"symbol": "continuation", "at": [0, y], "name": format!("A{}", i + 1)})).unwrap();
        terminal(&mut h, (100.0, y), "X05");
        wires.push(id(&wire(&mut h, &[(0.0, y), (100.0, y)], json!({})), "conductor"));
    }
    for (i, w) in wires.iter().enumerate() {
        h.run("cablecore", json!({"conductor": w.hex(), "core": format!("{}", i + 1)})).unwrap();
    }
    let doc = h.doc();
    let members: Vec<WireId> = doc.cables[&cable].cores.iter().filter_map(|m| m.wire()).collect();
    assert_eq!(members, wires, "each core moved onto its wire");
    Pack { h, p100, p115, cable, wires }
}

/// Design 072 §4.4 (F5, superseding design 032 §3.3): slashing on page 115 with the cable's tag
/// shows the cable there. Each core is already one wire through the continuations, so the cable
/// keeps its three members; the result names them. Erasing that slash keeps them too (they run on
/// page 100).
#[test]
fn a_cable_is_shown_on_another_page_by_its_tag() {
    let mut f = pack();
    let before = f.h.doc().cables.len();
    let head = f.h.head();
    let r = f.h.run("cableslash", json!({"from": [50, -5], "to": [50, 25], "tag": "-W04"})).unwrap();
    assert_eq!(f.h.head(), head + 1);
    assert_eq!(r["continued"], json!(true), "{r}");
    assert_eq!(r["cable"], json!(f.cable.hex()));
    let doc = f.h.doc();
    assert_eq!(doc.cables.len(), before, "no new cable");
    let members: Vec<WireId> = doc.cables[&f.cable].cores.iter().filter_map(|m| m.wire()).collect();
    assert_eq!(members, f.wires, "no new member");
    let cores = r["cores"].as_array().unwrap();
    assert_eq!(cores.len(), 3);
    for (i, (c, w)) in cores.iter().zip(&f.wires).enumerate() {
        assert_eq!(c["conductor"], json!(w.hex()));
        assert_eq!(c["core"], json!(format!("{}", i + 1)));
    }
    let slashes = doc.representations_of(ecad_model::Subject::Cable(f.cable)).filter(|(_, r)| r.role == ecad_model::Role::Slash).count();
    assert_eq!(slashes, 2, "one slash on each page");
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
    // Erasing the page-115 slash: the cable keeps its members, which run on page 100 too.
    let rep: ecad_model::RepresentationId = id(&r, "representation");
    let item: ecad_model::ItemId = id(&r, "item");
    let handle = f.h.handle_of(f.p115, item);
    f.h.run("erase", json!({"handles": [handle.hex()]})).unwrap();
    let doc = f.h.doc();
    assert!(!doc.representations.contains_key(&rep));
    let left: Vec<WireId> = doc.cables[&f.cable].cores.iter().filter_map(|m| m.wire()).collect();
    assert_eq!(left, f.wires);
    let _ = f.p100;
}

/// F5 (overseer, 2026-11-20): the refusals of a slash with an existing cable's tag. A crossed wire
/// that is not one of its cores; a page that already has its slash; a page where none of its wires
/// runs (no empty slash). Nothing is written.
#[test]
fn showing_a_cable_by_its_tag_is_refused_with_the_reason() {
    let mut f = pack();
    // A wire on page 115 that is not a core of -W04.
    f.h.points_at(&[(0.0, 40.0), (100.0, 40.0)]);
    wire(&mut f.h, &[(0.0, 40.0), (100.0, 40.0)], json!({}));
    let head = f.h.head();
    let m = f.h.run("cableslash", json!({"from": [50, -5], "to": [50, 45], "tag": "-W04"})).unwrap_err();
    assert!(m.contains("is not a core of -W04."), "{m}");
    assert_eq!(f.h.head(), head);
    // Already slashed on page 100.
    f.h.activate(f.p100);
    let m = f.h.run("cableslash", json!({"from": [60, -5], "to": [60, 25], "cable": f.cable.hex()})).unwrap_err();
    assert!(m.contains("-W04 already has a slash on this page."), "{m}");
    // A page where none of -W04's wires runs.
    let p116: PageId = id(&f.h.run("newpage", json!({"title": "116"})).unwrap(), "page");
    f.h.activate(p116);
    f.h.points_at(&[(0.0, 0.0), (100.0, 0.0)]);
    wire(&mut f.h, &[(0.0, 0.0), (100.0, 0.0)], json!({}));
    let head = f.h.head();
    let m = f.h.run("cableslash", json!({"from": [50, -5], "to": [50, 5], "tag": "-W04"})).unwrap_err();
    assert!(m.contains("-W04 has no wire on this page"), "{m}");
    assert_eq!(f.h.head(), head);
    assert_eq!(f.h.doc().representations_of(ecad_model::Subject::Cable(f.cable)).filter(|(_, r)| r.page == p116).count(), 0);
}

/// F5: `cablecontinue` takes a core's loose line on to another page; a cable whose cores are each
/// one wire, connected at both ends, has nothing to continue and is refused, with nothing written.
#[test]
fn cablecontinue_of_cores_connected_at_both_ends_is_refused() {
    let mut f = pack();
    let head = f.h.head();
    let m = f.h.run("cablecontinue", json!({"cable": f.cable.hex(), "at": [20, 80], "toward": [40, 80]})).unwrap_err();
    assert!(m.contains("-W04"), "{m}");
    assert_eq!(f.h.head(), head);
}

// ---------------------------------------------------------------- Q4: the wire label rule

/// §12 item 15: a 24V bus of 100 mm between two points with four 5 mm drops to `-X01:1…4`
/// (design 072 §4.3: each drop is a T on the bus, its wire joining from the bus's nearer end).
fn bus() -> (H, PageId) {
    let mut h = H::new(0x32_40);
    let page = h.first_page();
    h.points_at(&[(0.0, 0.0), (100.0, 0.0)]);
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
    // Design 072: the bus is one wire and each drop a wire of its own (a T splits nothing).
    assert_eq!(h.doc().wires.len(), 5);
    assert_eq!(labels(&mut h, page).len(), 5, "each, 0: every wire");
    h.run("wiresettings", json!({"label_min_length": 10})).unwrap();
    let l = labels(&mut h, page);
    assert_eq!(l.len(), 1, "each, 10: the bus, no 5 mm drop");
    // A drop of the net with its own cross-section is labelled too (owner Q7).
    h.run("wiresettings", json!({"label_rule": "net"})).unwrap();
    let d = h.doc();
    let drop = d
        .routes_on(page)
        .find(|r| {
            let v = &d.routes[r].points;
            v.len() == 2 && v[0].x == 20.0 && v[0].y == 0.0
        })
        .and_then(|r| ecad_cadcraft::wires::wire_of_route(&d, r));
    let drop = drop.expect("the first drop");
    h.run("setprop", json!({"conductor": drop.hex(), "key": "size", "value": "2.5"})).unwrap();
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
    // The label rule's built-in is `net` (data since brief 066).
    let rule = &ecad_cadcraft::defaults::wires::WIRING_LABEL_RULE;
    assert_eq!(rule.builtin(), ecad_cadcraft::defaults::Value::Choice(ecad_model::wiring::LABEL_RULE_NET));
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
                    after: Some("no".into()),
                }],
            )
            .unwrap();
    });
    wire(&mut h, &[(50.0, 0.0), (50.0, -20.0)], json!({}));
    assert_eq!(h.doc().routes.len(), 2, "wire.join off: the ends stay open (two lines)");
}

/// Review of #65, finding 1, as design 072 §4.4 (F5) has it: renaming page 115's A1 to Z1 breaks
/// the pair, so the A1 core's wire goes and the core stays on its lines; undo puts it back on the
/// wire. The cable keeps three cores throughout.
#[test]
fn renaming_a_continuation_leaves_the_core_on_its_lines() {
    let mut f = pack();
    f.h.run("cableslash", json!({"from": [50, -5], "to": [50, 25], "tag": "-W04"})).unwrap();
    let cores = |h: &mut H, k: ecad_model::CableId| h.run("cablecores", json!({"cable": k.hex()})).unwrap()["cores"].as_array().map_or(0, Vec::len);
    assert_eq!(cores(&mut f.h, f.cable), 3);
    let doc = f.h.doc();
    let a1 = doc
        .continuations_named("A1")
        .into_iter()
        .find(|c| doc.representations_of(ecad_model::Subject::Aid(*c)).any(|(_, r)| r.page == f.p115))
        .expect("A1 on page 115");
    f.h.run("setprop", json!({"aids": [a1.hex()], "key": "name", "value": "Z1"})).unwrap();
    let doc = f.h.doc();
    assert_eq!(doc.cables[&f.cable].cores.len(), 3, "still three cores: {:?}", doc.cables[&f.cable].cores);
    assert!(!doc.wires.contains_key(&f.wires[0]), "the A1 wire went with the pair");
    assert!(matches!(doc.cables[&f.cable].cores[0].position, ecad_model::CorePosition::Route { .. }), "{:?}", doc.cables[&f.cable].cores[0]);
    f.h.cmdline("u");
    assert_eq!(cores(&mut f.h, f.cable), 3);
    assert_eq!(f.h.doc().cables[&f.cable].cores[0].wire(), Some(f.wires[0]), "undo puts the core back on its wire");
}

/// Review of #65, finding 3: wire labels are drawn in lane T's wire-label style (a bold weight
/// gives the bold text style), and their placement box is as wide as that font draws them.
#[test]
fn wire_labels_take_the_wire_label_text_style() {
    let mut h = H::new(0x3216);
    let page = h.first_page();
    h.points_at(&[(0.0, 0.0), (60.0, 0.0)]);
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

/// Review of #65, finding 5: the quiet ends (the ends at a cable reference of the wires that
/// leave for another page) come from the document, the same with no projected page as the page
/// map gives. With named points (design 072 §4.4, F4) such an end lands on its core's point on
/// the reference, so a line drawn from it lands on that point too, never on the wire's line.
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
    // Continued on another page and landed on terminals there: its wires leave page 1 at the
    // reference (brief 010 §6.4).
    let other: PageId = id(&h.run("newpage", json!({"title": "Field"})).unwrap(), "page");
    h.activate(other);
    let v = h.run("cablecontinue", json!({"cable": k.hex(), "at": [20, 5], "toward": [40, 5]})).unwrap();
    for (s, y) in v["routes"].as_array().cloned().unwrap_or_default().iter().zip([0.0, 5.0, 10.0]) {
        let t = terminal(&mut h, (40.0, y), "X02");
        h.run("connect", json!({"route": s["route"], "end": s["free_end"], "terminal": t.hex()})).unwrap();
    }
    h.activate(page);
    let doc = h.doc();
    assert_eq!(doc.wires.len(), 3, "a wire per core across the pages");
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
    // A line from a quiet end lands on the core's named point there.
    let at = quiet[0];
    let r = wire(&mut h, &[(at.x, at.y), (at.x + 20.0, at.y - 30.0)], json!({}));
    let line: RouteId = id(&r, "route");
    let doc = h.doc();
    let first = doc.routes[&line].ends[0];
    assert!(first.point().is_some_and(|t| doc.named_points(t).is_some()), "{first:?}");
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
        let kept_checks = p.kept_checks().map(|c| c.sorted(doc)).unwrap_or_default();
        let fresh = ecad_cadcraft::project::engine_views().checks(doc, p.tolerance).sorted(doc);
        assert_eq!(kept_checks, fresh, "{what}: Checks differ from a rebuild");
        p.outputs();
        let doc = p.ledger.document();
        let kept = p.kept_outputs().map(|o| o.cables().into_iter().cloned().collect::<Vec<_>>()).unwrap_or_default();
        let rebuilt = ecad_cadcraft::outputs::kept::Outputs::build_with(doc, &ecad_cadcraft::text_style::EngineMetrics);
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
        .find(|c| doc.representations_of(ecad_model::Subject::Aid(*c)).any(|(_, r)| r.page == f.p115))
        .unwrap();
    f.h.run("setprop", json!({"aids": [a2.hex()], "key": "name", "value": "Z2"})).unwrap();
    kept_equals_rebuild(&mut f.h, "continuation renamed");
    let line = f.h.route_of(f.p115, f.wires[2]);
    f.h.run("disconnect", json!({"route": line.hex(), "end": "A"})).unwrap();
    kept_equals_rebuild(&mut f.h, "a member's end detached");
    // Unrelated: a wire far away on page 115 changes no cable row.
    f.h.run("wire", json!({"points": [[400, 400], [440, 400]]})).unwrap();
    let changed = f.h.p(|p| p.kept_outputs().is_some_and(|o| o.last_cables_changed));
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
        .find(|c| doc.representations_of(ecad_model::Subject::Aid(*c)).any(|(_, r)| r.page == f.p115))
        .unwrap();
    let cable = f.cable;
    let (kept, fresh) = f.h.cfg(|p| {
        p.outputs();
        let before = p.ledger.document().clone();
        let before_aid = p.ledger.document().aids.get(&a1).cloned().unwrap();
        let after_aid = ecad_cadcraft::wires::continuation_aid("Z1", None).unwrap();
        let ops = vec![ecad_model::Op::ModifyAid { id: a1, before: before_aid, after: after_aid }];
        let pending = p.views_mut().begin(&before, &ops);
        p.ledger.commit(ecad_model::Author::User, "rename", ops.clone()).unwrap();
        // F5: the commit's rewrite pass adds to the rename (the broken pair's wire goes, its core
        // stays on its lines); the update gets the whole transaction, as the project gives it.
        let all = p.ledger.log().last().map(|t| t.ops.clone()).unwrap_or_default();
        assert!(all.len() > ops.len(), "the rewrite extends the rename");
        let doc = p.ledger.document().clone();
        p.views_mut().finish(&doc, &all, pending);
        let kept = p.kept_outputs().unwrap().cable_row(cable).cloned();
        let fresh = p.derived().detached(&doc).outputs().cable_row(cable).cloned();
        (kept, fresh)
    });
    assert_eq!(kept.as_ref().map(|r| r.cores), Some(2), "F5: two cores are wires, the third is on its lines: {kept:?}");
    assert_eq!(kept, fresh);
}
