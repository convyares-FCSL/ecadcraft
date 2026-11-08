//! Brief 022 acceptance (design 022 §15): nets on the page, labels, stable numbers, the Checks
//! list and its incremental upkeep. Driven through the commands' JSON forms.

use ecad_cadcraft::checks::{Checks, Fix, Kind};
use ecad_cadcraft::wires::{self, LABEL_LAYER};
use ecad_model::{Author, ConductorId, Document, ObjectRef, Op, PageId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

fn net_of(doc: &Document, c: ConductorId) -> Option<ecad_model::NetId> {
    doc.connectivity().net_of_conductor(c)
}

fn number(doc: &Document, c: ConductorId) -> Option<String> {
    doc.conductors.get(&c)?.properties.get("wire_number").cloned()
}

fn wire(h: &mut H, points: &[(f64, f64)], extra: Value) -> ConductorId {
    let mut params = json!({"points": pts(points)});
    if let (Some(o), Some(e)) = (params.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            o.insert(k.clone(), v.clone());
        }
    }
    id(&h.run("wire", params).unwrap(), "conductor")
}

fn terminal(h: &mut H, at: (f64, f64), tag: &str) -> ecad_model::TerminalId {
    let r = h.run("place", json!({"symbol": "terminal", "at": [at.0, at.1], "tag": tag})).unwrap();
    id(&r["terminals"][0], "terminal")
}

fn rows(doc: &Document, ch: &Checks) -> Vec<String> {
    ch.sorted(doc).iter().map(|c| format!("{:?} {:?} {} {}", c.kind, c.object, c.which, c.locator(doc))).collect()
}

/// §15 B: a T without a junction is two nets and a Checks row with Add junction; with the
/// junction it is one net and one number; a crossing is two nets.
#[test]
fn a_tee_needs_a_junction_and_a_crossing_never_connects() {
    let mut h = H::new(0x22_01);
    let a = terminal(&mut h, (0.0, 0.0), "X1");
    let b = terminal(&mut h, (60.0, 0.0), "X1");
    let _ = (a, b);
    let main = wire(&mut h, &[(0.0, 0.0), (60.0, 0.0)], json!({}));
    let tee = wire(&mut h, &[(30.0, 30.0), (30.0, 0.0)], json!({"junction": false}));
    let doc = h.doc();
    assert_ne!(net_of(&doc, main), net_of(&doc, tee), "a T without a junction is not connected");
    let all = ecad_cadcraft::checks::all(&doc, 1e-6);
    let looks: Vec<_> = all.iter().filter(|c| c.kind == Kind::LooksConnected).collect();
    assert_eq!(looks.len(), 1, "{all:?}");
    let Some(Fix::AddJunction(at)) = looks[0].fix else { panic!("no fix: {looks:?}") };
    assert!(at.near(ecad_model::Pt::new(30.0, 0.0), 1e-9));
    // Add junction (the row's fix), then the T is one net with one number.
    let head = h.head();
    let j = h.run("junction", json!({"at": [at.x, at.y]})).unwrap();
    assert_eq!(h.head(), head + 1);
    let doc = h.doc();
    let second: ConductorId = id(&j, "conductor");
    assert_eq!(net_of(&doc, main), net_of(&doc, tee));
    assert_eq!(net_of(&doc, main), net_of(&doc, second));
    assert_eq!(number(&doc, main), number(&doc, tee));
    assert_eq!(number(&doc, main), number(&doc, second));
    assert!(ecad_cadcraft::checks::all(&doc, 1e-6).iter().all(|c| c.kind != Kind::LooksConnected));
    // `u` undoes it in one step.
    h.cmdline("u");
    let doc = h.doc();
    assert_ne!(net_of(&doc, main), net_of(&doc, tee));
    // The refusals.
    h.cmdline("redo");
    let m = h.run("junction", json!({"at": [30, 0]})).unwrap_err();
    assert!(m.contains("There is a junction here already."), "{m}");
    let m = h.run("junction", json!({"at": [0, 0]})).unwrap_err();
    assert!(m.contains("not its end"), "{m}");
    // A crossing: two nets.
    let v = wire(&mut h, &[(45.0, 20.0), (45.0, -20.0)], json!({"junction": false}));
    let doc = h.doc();
    assert_ne!(net_of(&doc, v), net_of(&doc, main));
}

/// §15 C: every route shows its label; a net's conductors share the number; the text is never
/// upside down at 0, 90, 180, 270, 45 and 135°.
#[test]
fn every_wire_shows_its_number_cross_section_and_colour_never_upside_down() {
    let mut h = H::new(0x22_02);
    let page = h.first_page();
    let mut ws = Vec::new();
    for (i, (dx, dy)) in
        [(20.0, 0.0), (0.0, 20.0), (-20.0, 0.0), (0.0, -20.0), (20.0, 20.0), (-20.0, 20.0)].into_iter().enumerate()
    {
        let x0 = 100.0 * i as f64;
        ws.push(wire(&mut h, &[(x0, 0.0), (x0 + dx, dy)], json!({"potential": "24V"})));
    }
    let doc = h.doc();
    let labels = wires::labels_on_page(&doc, page);
    assert_eq!(labels.len(), ws.len());
    for l in &labels {
        let deg = l.rotation.to_degrees();
        assert!(deg > -90.0 - 1e-9 && deg <= 90.0 + 1e-9, "{deg}");
        assert!(l.text.ends_with(" 0.5 Blue") && l.text.starts_with("24V-0"), "{}", l.text);
    }
    // Vertical text reads bottom to top, to the left of the line.
    let up = labels.iter().find(|l| l.conductor == ws[1]).unwrap();
    assert!((up.rotation.to_degrees() - 90.0).abs() < 1e-9 && up.at.x < 100.0);
    // A stored colour and cross-section win over the potential's defaults.
    h.run("setprop", json!({"conductor": ws[0].hex(), "key": "colour", "value": "Red"})).unwrap();
    h.run("setprop", json!({"conductor": ws[0].hex(), "key": "cross_section", "value": "18 AWG"})).unwrap();
    let doc = h.doc();
    let l = wires::labels_on_page(&doc, page).into_iter().find(|l| l.conductor == ws[0]).unwrap();
    assert!(l.text.ends_with(" 18 AWG Red"), "{}", l.text);
    // The labels are drawn on their reserved layer and mapped to their wire.
    let n = h.pv(|p, pg| pg.entities_of(p, page)).iter().filter(|e| e.common.layer == LABEL_LAYER).count();
    assert_eq!(n, ws.len());
    // `setprop` refuses the numbering keys.
    let m = h.run("setprop", json!({"conductor": ws[0].hex(), "key": "wire_number", "value": "X"})).unwrap_err();
    assert!(m.contains("use wirenumber"), "{m}");
    let m = h.run("setprop", json!({"conductor": ws[0].hex(), "key": "potential", "value": "0V"})).unwrap_err();
    assert!(m.contains("use potential"), "{m}");
}

/// §15 D and E: adding a wire never renumbers another net; a wire added into a net gets its
/// number; a fixed number survives adds, deletes, merges and undo; a used number is refused.
#[test]
fn adding_a_wire_never_renumbers_and_a_fixed_number_survives() {
    let mut h = H::new(0x22_03);
    let t: Vec<_> = (0..6).map(|i| terminal(&mut h, (20.0 * i as f64, 0.0), "X1")).collect();
    let _ = &t;
    let a = wire(&mut h, &[(0.0, 0.0), (20.0, 0.0)], json!({}));
    let b = wire(&mut h, &[(40.0, 0.0), (60.0, 0.0)], json!({}));
    let numbers = |h: &mut H| {
        let doc = h.doc();
        doc.conductors.keys().map(|c| (*c, number(&doc, *c))).collect::<std::collections::BTreeMap<_, _>>()
    };
    let before = numbers(&mut h);
    let c = wire(&mut h, &[(80.0, 0.0), (100.0, 30.0)], json!({}));
    let after = numbers(&mut h);
    for (k, v) in &before {
        assert_eq!(after.get(k), Some(v), "a new net renumbered {k}");
    }
    assert_eq!(number(&h.doc(), c).as_deref(), Some("03"));
    let into = wire(&mut h, &[(20.0, 0.0), (20.0, 30.0)], json!({}));
    assert_eq!(number(&h.doc(), into), number(&h.doc(), a));
    // Fix 24V-99 on b's net; add, merge, delete and undo around it.
    h.run("wirenumber", json!({"conductor": b.hex(), "value": "24V-99"})).unwrap();
    let joined = wire(&mut h, &[(60.0, 0.0), (60.0, 30.0)], json!({}));
    assert_eq!(number(&h.doc(), joined).as_deref(), Some("24V-99"));
    let merge = wire(&mut h, &[(20.0, 0.0), (40.0, 0.0)], json!({}));
    let doc = h.doc();
    for w in [a, b, into, joined, merge] {
        assert_eq!(number(&doc, w).as_deref(), Some("24V-99"), "{w}");
    }
    h.run("ecaddelete", json!({"conductor": merge.hex()})).unwrap();
    assert_eq!(number(&h.doc(), b).as_deref(), Some("24V-99"));
    h.cmdline("u");
    h.cmdline("u");
    assert_eq!(number(&h.doc(), b).as_deref(), Some("24V-99"));
    // A used number is offered choices, never written silently.
    let m = h.run("wirenumber", json!({"conductor": c.hex(), "value": "24V-99"})).unwrap_err();
    assert!(m.contains("24V-99 is used by another net"), "{m}");
    // Delete a middle conductor: the bigger part keeps the number (§5.2, split rules).
    let mid = wire(&mut h, &[(80.0, 0.0), (100.0, 0.0)], json!({}));
    let tail = wire(&mut h, &[(100.0, 0.0), (100.0, -30.0)], json!({}));
    let n = number(&h.doc(), c);
    h.run("ecaddelete", json!({"conductor": mid.hex()})).unwrap();
    let doc = h.doc();
    assert_eq!(number(&doc, c), n, "the part with the lower conductor ID keeps it on a tie");
    assert_ne!(number(&doc, tail), n);
}

/// §15 F: one seeded fault per Checks kind appears once, at the right page and point; Number
/// them works, and `u` undoes it.
#[test]
fn each_seeded_fault_appears_once_where_it_is() {
    use ecad_cadcraft::checks::Object;
    let mut h = H::new(0x22_04);
    let page = h.first_page();
    // Open end, and its net's single connection point.
    let x = terminal(&mut h, (0.0, 0.0), "X1");
    let open = wire(&mut h, &[(0.0, 0.0), (0.0, 30.0)], json!({}));
    // Loose wire, and a T onto it with no junction (looks connected).
    let loose = wire(&mut h, &[(50.0, 0.0), (50.0, 30.0)], json!({}));
    terminal(&mut h, (70.0, 15.0), "X4");
    let tee = wire(&mut h, &[(70.0, 15.0), (50.0, 15.0)], json!({"junction": false}));
    // Unconnected device points.
    h.run("place", json!({"symbol": "lamp", "at": [200, 100], "tag": "H9"})).unwrap();
    // A junction with two wires only.
    terminal(&mut h, (600.0, 0.0), "X5");
    terminal(&mut h, (640.0, 0.0), "X5");
    wire(&mut h, &[(600.0, 0.0), (640.0, 0.0)], json!({}));
    h.run("junction", json!({"at": [620, 0]})).unwrap();
    // Mixed potentials in one net.
    for i in 0..3 {
        terminal(&mut h, (400.0 + 20.0 * i as f64, 0.0), "X2");
    }
    wire(&mut h, &[(400.0, 0.0), (420.0, 0.0)], json!({"potential": "24V"}));
    wire(&mut h, &[(420.0, 0.0), (440.0, 0.0)], json!({"potential": "N"}));
    // Two nets sharing a fixed number (a fixed net split).
    for i in 0..4 {
        terminal(&mut h, (500.0, 20.0 * i as f64), "X3");
    }
    let s1 = wire(&mut h, &[(500.0, 0.0), (500.0, 20.0)], json!({}));
    let mid = wire(&mut h, &[(500.0, 20.0), (500.0, 40.0)], json!({}));
    wire(&mut h, &[(500.0, 40.0), (500.0, 60.0)], json!({}));
    h.run("wirenumber", json!({"conductor": s1.hex(), "value": "S-1"})).unwrap();
    h.run("ecaddelete", json!({"conductor": mid.hex()})).unwrap();
    // A net joining 24V and 0V through continuations; the 0V one has no partner.
    h.run("place", json!({"symbol": "continuation", "at": [700, 0], "name": "24V"})).unwrap();
    h.run("place", json!({"symbol": "continuation", "at": [700, 40], "rotation": 180, "name": "0V"})).unwrap();
    h.run("place", json!({"symbol": "continuation", "at": [300, 0], "name": "24V"})).unwrap();
    wire(&mut h, &[(700.0, 0.0), (700.0, 40.0)], json!({}));
    // One net, two fixed numbers.
    for (x, y) in [(800.0, 0.0), (800.0, 30.0), (840.0, 0.0), (840.0, 30.0)] {
        terminal(&mut h, (x, y), "X6");
    }
    let fa = wire(&mut h, &[(800.0, 0.0), (800.0, 30.0)], json!({}));
    let fb = wire(&mut h, &[(840.0, 0.0), (840.0, 30.0)], json!({}));
    h.run("wirenumber", json!({"conductor": fa.hex(), "value": "A-1"})).unwrap();
    h.run("wirenumber", json!({"conductor": fb.hex(), "value": "A-2"})).unwrap();
    wire(&mut h, &[(800.0, 0.0), (840.0, 0.0)], json!({}));
    // Unnumbered: a conductor committed without the pass (a project older than 022); and an end
    // drawn away from its connection point. Both are written past the commands.
    let old = h.cfg(|p| {
        let w: ConductorId = p.ids.fresh();
        p.ledger
            .commit(Author::User, "old", vec![Op::CreateConductor { id: w, conductor: Default::default() }])
            .unwrap();
        let doc = p.ledger.document();
        let (item, _) = ecad_model::derive::route_on_page(doc, fa, page).unwrap();
        let before = doc.items[&item].clone();
        let mut after = before.clone();
        if let ecad_model::ItemKind::Primitive(prim) = &before.kind {
            let moved = prim.with_path_end(ecad_model::End::B, ecad_model::Pt::new(805.0, 30.0)).unwrap();
            after.kind = ecad_model::ItemKind::Primitive(moved);
        }
        p.ledger.commit(Author::User, "away", vec![Op::ModifyItem { id: item, before, after }]).unwrap();
        w
    });
    let doc = h.doc();
    let all = ecad_cadcraft::checks::all(&doc, 1e-6);
    let count = |k: Kind| all.iter().filter(|c| c.kind == k).count();
    let has = |k: Kind, o: Object| all.iter().any(|c| c.kind == k && c.object == o);
    let listing = rows(&doc, &Checks::build(&doc, 1e-6));
    assert_eq!(count(Kind::OpenEnd), 2, "{listing:#?}");
    assert!(has(Kind::OpenEnd, Object::Conductor(open)) && has(Kind::OpenEnd, Object::Conductor(tee)));
    assert_eq!(count(Kind::LooseWire), 2, "{listing:#?}");
    assert!(has(Kind::LooseWire, Object::Conductor(loose)) && has(Kind::LooseWire, Object::Conductor(old)));
    assert_eq!(count(Kind::LooksConnected), 1, "{listing:#?}");
    assert!(has(Kind::LooksConnected, Object::Conductor(tee)));
    assert_eq!(count(Kind::SinglePoint), 2, "{listing:#?}");
    assert!(has(Kind::SinglePoint, Object::Terminal(x)));
    assert_eq!(count(Kind::ThinJunction), 1, "{listing:#?}");
    assert_eq!(count(Kind::LonelyContinuation), 1, "{listing:#?}");
    assert_eq!(count(Kind::MixedPotentials), 1, "{listing:#?}");
    assert_eq!(count(Kind::JoinsPotentials), 1, "{listing:#?}");
    assert_eq!(count(Kind::TwoFixed), 1, "{listing:#?}");
    assert_eq!(count(Kind::SharedNumber), 2, "{listing:#?}");
    assert_eq!(count(Kind::DrawnAway), 1, "{listing:#?}");
    assert!(has(Kind::DrawnAway, Object::Conductor(fa)));
    assert_eq!(count(Kind::Unnumbered), 1, "{listing:#?}");
    assert_eq!(count(Kind::Unconnected), 2, "the lamp's X1 and X2: {listing:#?}");
    // Rows point at their page and point, in book order (errors first).
    let lamp = all.iter().find(|c| c.kind == Kind::Unconnected).unwrap();
    assert_eq!(lamp.page, Some(page));
    assert_eq!(lamp.locator(&doc), "/1.6");
    assert_eq!(all.first().map(|c| c.severity()), Some(ecad_cadcraft::checks::Severity::Error));
    // Put the drawn-away end back (written past the commands, it is undone the same way).
    h.cfg(|p| p.ledger.undo(Author::User).unwrap());
    // Number them, then `u`.
    let head = h.head();
    h.run("renumberwires", json!({"scope": "unnumbered"})).unwrap();
    assert_eq!(h.head(), head + 1);
    assert!(ecad_cadcraft::checks::all(&h.doc(), 1e-6).iter().all(|c| c.kind != Kind::Unnumbered));
    h.cmdline("u");
    assert!(ecad_cadcraft::checks::all(&h.doc(), 1e-6).iter().any(|c| c.kind == Kind::Unnumbered));
}

/// Design 022 §11: the kept Checks list equals a fresh build after every step, and an edit that
/// touches no connectivity leaves every row untouched.
#[test]
fn the_kept_checks_list_follows_every_edit_and_equals_a_fresh_build() {
    let mut h = H::new(0x22_05);
    // Shown once: built whole.
    h.cfg(|p| {
        p.checks();
    });
    let compare = |h: &mut H, step: &str| {
        let (kept, fresh) = h.cfg(|p| {
            let doc = p.ledger.document().clone();
            let kept = rows(&doc, p.checks());
            (kept, rows(&doc, &Checks::build(&doc, p.tolerance)))
        });
        assert_eq!(kept, fresh, "after {step}");
    };
    let steps: Vec<(&str, Value)> = vec![
        ("place", json!({"symbol": "terminal", "at": [0, 0], "tag": "X1"})),
        ("place", json!({"symbol": "terminal", "at": [40, 0], "tag": "X1"})),
        ("wire", json!({"points": [[0, 0], [40, 0]]})),
        ("wire", json!({"points": [[20, 30], [20, 0]], "junction": false})),
        ("junction", json!({"at": [20, 0]})),
        ("wire", json!({"points": [[100, 0], [100, 40]], "potential": "24V"})),
        ("place", json!({"symbol": "continuation", "at": [100, 40], "name": "24V"})),
        ("place", json!({"symbol": "continuation", "at": [200, 40], "name": "24V"})),
        ("wire", json!({"points": [[200, 40], [200, 0]], "potential": "0V"})),
        ("place", json!({"symbol": "lamp", "at": [300, 50], "tag": "H1"})),
        ("wire", json!({"points": [[300, 60], [300, 100]]})),
        ("place", json!({"symbol": "lamp", "at": [400, 50], "tag": "H2"})),
        ("newpage", json!({"title": "2"})),
    ];
    for (cmd, params) in steps {
        h.run(cmd, params.clone()).unwrap_or_else(|e| panic!("{cmd} {params}: {e}"));
        compare(&mut h, cmd);
    }
    for _ in 0..4 {
        h.cmdline("u");
        compare(&mut h, "undo");
    }
    for _ in 0..2 {
        h.cmdline("redo");
        compare(&mut h, "redo");
    }
    // A move of a lamp drags its wire's end: drawn-away and looks-connected rows follow.
    let page = h.first_page();
    h.activate(page);
    let doc = h.doc();
    let w = *doc.conductors.keys().next().unwrap();
    h.run("ecaddelete", json!({"conductor": w.hex()})).unwrap();
    compare(&mut h, "delete");
    // An unrelated edit (a device property) changes no row and re-checks no net.
    let before = h.cfg(|p| p.checks().clone());
    let lamp =
        h.doc().components.iter().find(|(_, c)| c.properties.get("tag").is_some_and(|t| t == "-H02")).map(|(k, _)| *k);
    if let Some(lamp) = lamp {
        h.run("setprop", json!({"component": lamp.hex(), "key": "description", "value": "Spare"})).unwrap();
    }
    let after = h.cfg(|p| p.checks().clone());
    let doc = h.doc();
    assert_eq!(rows(&doc, &before), rows(&doc, &after));
    let _ = PageId(0);
    let _ = ObjectRef::Conductor(w);
}
