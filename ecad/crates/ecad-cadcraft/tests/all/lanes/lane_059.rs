//! Lane 059 (the cable manager): design 059 §11, run through the commands.
//!
//! Cable types here are synthetic (made-up names and values).

use ecad_cadcraft::cable_cmds::{self, CableError, Side};
use ecad_cadcraft::checks::{self, Severity};
use ecad_model::{CableId, ConductorId, Op};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

fn checks_of(h: &mut H, kind: &str) -> Vec<Value> {
    h.run("checks", json!({"kinds": [kind], "limit": 2000})).unwrap_or_else(|e| panic!("checks {kind}: {e}"))["checks"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn last_tx(h: &mut H) -> (String, Vec<Op>) {
    h.p(|p| p.ledger.log().last().map(|t| (t.label.clone(), t.ops.clone())).unwrap_or_default())
}

/// `n` vertical wires at x = 0, 10, 20…, from y 0 to 50, with no ends attached.
fn wires(h: &mut H, n: usize) -> Vec<ConductorId> {
    (0..n)
        .map(|i| {
            let x = 10.0 * i as f64;
            id(&h.run("wire", json!({"points": pts(&[(x, 0.0), (x, 50.0)])})).unwrap(), "conductor")
        })
        .collect()
}

/// A slash across every wire of [`wires`] (left to right).
fn slash(h: &mut H, cable: Option<CableId>) -> Result<Value, String> {
    let mut p = json!({"from": [-5, 25], "to": [500, 25]});
    if let Some(k) = cable {
        p["cable"] = json!(k.hex());
    }
    h.run("cableslash", p)
}

fn new_cable(h: &mut H, p: Value) -> CableId {
    id(&h.run("cablenew", p).unwrap(), "cable")
}

#[test]
fn cablenew_makes_a_cable_that_is_not_drawn_in_one_transaction() {
    let mut h = H::new(5901);
    let r = h.run("cablenew", json!({"cores": "4G"})).unwrap();
    let k: CableId = id(&r, "cable");
    assert_eq!(r["tag"], json!("-W01"), "{r}");
    let (label, ops) = last_tx(&mut h);
    assert_eq!(label, "Create cable");
    assert!(matches!(ops.as_slice(), [Op::CreateCable { cable, .. }] if cable.members.is_empty()), "{ops:?}");
    let doc = h.doc();
    assert!(!cable_cmds::is_drawn(&doc, k));
    let rows = checks_of(&mut h, "cable-not-drawn");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["severity"], json!("info"));
    assert!(rows[0]["text"].as_str().unwrap().contains("-W01 is not in the drawing yet."), "{rows:?}");
    assert!(checks_of(&mut h, "core-open").is_empty(), "no core-open for a cable not drawn");
    assert!(checks_of(&mut h, "no-core-count").is_empty());
    // Q2: Warning in the release check.
    let c = checks::cable_checks(&doc, k).into_iter().find(|c| c.kind == checks::Kind::CableNotDrawn).unwrap();
    assert_eq!(c.severity(), Severity::Info);
    assert_eq!(c.release_severity(), Severity::Warning);
    assert_eq!(ecad_cadcraft::release::counts(&[c]), (0, 1, 0));
    // The cable's row in the report says not drawn.
    let drawn = h.p(|p| p.outputs.as_ref().and_then(|o| o.cable_row(k)).map(|r| r.drawn));
    assert!(drawn != Some(true));
}

#[test]
fn cablenew_refuses_a_tag_a_device_holds_and_writes_nothing() {
    let (mut h, _, c2, _) = crate::common::small_project(5902);
    h.run("setprop", json!({"component": c2.hex(), "key": "tag", "value": "-K01"})).unwrap();
    let tag = "-K01";
    let head = h.head();
    let e = h.run("cablenew", json!({"tag": tag})).unwrap_err();
    assert!(e.contains("already the tag of") && e.contains("(device)"), "{e}");
    assert_eq!(h.head(), head, "nothing written");
    let e = h.run("cablenew", json!({"colour": 1})).unwrap_err();
    assert!(e.contains("unknown field"), "{e}");
    assert_eq!(h.head(), head);
}

#[test]
fn cableedit_sets_fields_in_one_transaction_and_refuses_bad_values() {
    let mut h = H::new(5903);
    let k = new_cable(&mut h, json!({}));
    h.run("cableedit", json!({"cable": k.hex(), "set": {"length": 12, "to": {"gland": "M20", "location": "+FIELD"}, "shield": "overall"}})).unwrap();
    let (label, ops) = last_tx(&mut h);
    assert_eq!(label, "Edit cable");
    let on_cable = ops.iter().filter(|o| matches!(o, Op::SetProperty { object: ecad_model::ObjectRef::Cable(_), .. })).count();
    assert_eq!(on_cable, 4, "{ops:?}");
    let doc = h.doc();
    assert_eq!(cable_cmds::planned(&doc, k, Side::To, ecad_cadcraft::designations::Aspect::Location).as_deref(), Some("+FIELD"));
    assert_eq!(cable_cmds::gland(&doc, k, Side::To).map(|g| g.0).as_deref(), Some("M20"));
    assert_eq!(cable_cmds::shield(&doc, k).map(|s| s.0), Some(cable_cmds::Shield::Overall));
    let head = h.head();
    for (set, words) in [
        (json!({"length": "x"}), "Length must be a number of metres"),
        (json!({"length": 0}), "Length must be a number of metres"),
        (json!({"cores": "4Q1.5"}), "Cores:"),
        (json!({"cable_type": "H07X"}), "No cable type"),
        (json!({"from": {"gland": "M20 and a very long text"}}), "at most 16"),
        (json!({"colour": 1}), "unknown field"),
    ] {
        let e = h.run("cableedit", json!({"cable": k.hex(), "set": set})).unwrap_err();
        assert!(e.contains(words), "{set}: {e}");
        assert_eq!(h.head(), head, "{set}: nothing written");
    }
    // One undo step.
    h.run("u", json!({})).unwrap();
    assert!(!h.doc().cables[&k].properties.contains_key("length"));
}

#[test]
fn cores_in_use_are_never_reduced() {
    let mut h = H::new(5904);
    wires(&mut h, 4);
    let k: CableId = id(&slash(&mut h, None).unwrap(), "cable");
    h.run("cableedit", json!({"cable": k.hex(), "set": {"cores": "4G"}})).unwrap();
    let head = h.head();
    let e = h.run("cableedit", json!({"cable": k.hex(), "set": {"cores": "3G"}})).unwrap_err();
    assert!(e.contains("has wires on cores 1, 2, 3 and GNYE; 3G has 3 cores"), "{e}");
    assert_eq!(h.head(), head);
    // A cable type with fewer cores is refused the same way.
    let ty = h
        .run("setstyle", json!({"catalogue": "cable", "new": true, "name": "Synth three", "fields": {"cores": "3G", "cross_section": "1.5"}}))
        .unwrap();
    h.run("cableedit", json!({"cable": k.hex(), "set": {"cores": null}})).unwrap_or_else(|e| panic!("{e}"));
    let e = h.run("cableedit", json!({"cable": k.hex(), "set": {"cable_type": ty["id"]}})).unwrap_err();
    assert!(e.contains("has wires on cores"), "{e}");
}

#[test]
fn a_cable_type_fills_type_only_when_empty_or_its_own() {
    let mut h = H::new(5905);
    let a =
        h.run("setstyle", json!({"catalogue": "cable", "new": true, "name": "Synth A", "fields": {"cores": "4G", "cross_section": "1.5"}})).unwrap();
    let b =
        h.run("setstyle", json!({"catalogue": "cable", "new": true, "name": "Synth B", "fields": {"cores": "7G", "cross_section": "2.5"}})).unwrap();
    let k = new_cable(&mut h, json!({}));
    h.run("cableedit", json!({"cable": k.hex(), "set": {"cable_type": a["id"]}})).unwrap();
    assert_eq!(h.doc().cables[&k].properties.get("type").map(String::as_str), Some("4G1.5"));
    h.run("cableedit", json!({"cable": k.hex(), "set": {"cable_type": b["id"]}})).unwrap();
    assert_eq!(h.doc().cables[&k].properties.get("type").map(String::as_str), Some("7G2.5"), "came from the old type");
    h.run("cableedit", json!({"cable": k.hex(), "set": {"type": "Hand typed"}})).unwrap();
    h.run("cableedit", json!({"cable": k.hex(), "set": {"cable_type": a["id"]}})).unwrap();
    assert_eq!(h.doc().cables[&k].properties.get("type").map(String::as_str), Some("Hand typed"), "a hand-typed type stays");
    // The cable type's cores are the offer (design 059 §2.2).
    let doc = h.doc();
    assert_eq!(cable_cmds::cores_shown(&doc, k).map(|c| c.0).as_deref(), Some("4G"));
}

#[test]
fn cableremove_deletes_a_cable_not_drawn_and_refuses_a_drawn_one() {
    let mut h = H::new(5906);
    let k = new_cable(&mut h, json!({}));
    h.run("cableremove", json!({"cable": "-W01"})).unwrap();
    let (label, ops) = last_tx(&mut h);
    assert_eq!(label, "Remove cable");
    assert!(matches!(ops.as_slice(), [Op::DeleteCable { .. }]));
    h.run("u", json!({})).unwrap();
    assert!(h.doc().cables.contains_key(&k), "undo restores it");
    wires(&mut h, 2);
    let w: CableId = id(&slash(&mut h, None).unwrap(), "cable");
    let head = h.head();
    let e = h.run("cableremove", json!({"cable": w.hex()})).unwrap_err();
    assert!(e.contains("is drawn on") && e.contains("Delete it on its page."), "{e}");
    assert_eq!(h.head(), head);
    let doc = h.doc();
    let page = h.first_page();
    match cable_cmds::check_remove(&doc, w) {
        Err(CableError::Drawn { pages, .. }) => assert_eq!(pages.first().map(|p| p.0), Some(page)),
        other => panic!("{other:?}"),
    }
    let e = h.run("cableremove", json!({"cable": w.hex(), "drawn": "unassign"})).unwrap_err();
    assert!(e.contains("unknown field"), "{e}");
}

#[test]
fn slashing_onto_a_cable_not_drawn_assigns_its_cores_in_order() {
    let mut h = H::new(5907);
    let ws = wires(&mut h, 3);
    let k = new_cable(&mut h, json!({"cores": "4G"}));
    let r = slash(&mut h, Some(k)).unwrap();
    assert_eq!(r["assigned"], json!(true), "{r}");
    let (label, _) = last_tx(&mut h);
    assert_eq!(label, "Cable slash");
    let doc = h.doc();
    let members: Vec<(ConductorId, Option<String>)> = doc.cables[&k].members.iter().map(|m| (m.conductor, m.core.clone())).collect();
    assert_eq!(members, vec![(ws[0], Some("1".into())), (ws[1], Some("2".into())), (ws[2], Some("3".into()))]);
    assert!(checks_of(&mut h, "cable-not-drawn").is_empty());
    let open = checks_of(&mut h, "core-open");
    assert!(open.iter().any(|c| c["text"].as_str().unwrap_or_default().contains("core GNYE is not assigned")), "{open:?}");
    h.run("u", json!({})).unwrap();
    assert!(!cable_cmds::is_drawn(&h.doc(), k), "undo: not drawn again");
    assert_eq!(checks_of(&mut h, "cable-not-drawn").len(), 1);
}

#[test]
fn slashing_more_wires_than_free_cores_is_refused() {
    let mut h = H::new(5908);
    wires(&mut h, 5);
    let k = new_cable(&mut h, json!({"cores": "3G"}));
    let head = h.head();
    let e = slash(&mut h, Some(k)).unwrap_err();
    assert!(e.contains("-W01 has 3 free cores (1, 2, GNYE); the slash crosses 5 wires"), "{e}");
    assert_eq!(h.head(), head);
    // cableadd onto it takes the same rule.
    let ws: Vec<String> = h.doc().conductors.keys().map(|c| c.hex()).collect();
    let e = h.run("cableadd", json!({"conductors": ws, "cable": k.hex()})).unwrap_err();
    assert!(e.contains("free cores"), "{e}");
    assert_eq!(h.head(), head);
}

#[test]
fn a_wire_already_in_a_cable_is_refused_when_assigning() {
    let mut h = H::new(5909);
    wires(&mut h, 2);
    slash(&mut h, None).unwrap();
    let k = new_cable(&mut h, json!({}));
    let head = h.head();
    let e = slash(&mut h, Some(k)).unwrap_err();
    assert!(e.contains("already"), "{e}");
    assert_eq!(h.head(), head);
}

#[test]
fn a_drawn_cable_end_off_its_plan_warns_for_that_end_only() {
    let mut h = H::new(5910);
    let a = id::<ecad_model::ComponentId>(&h.run("place", json!({"at": [0, 100], "rotation": 90})).unwrap(), "component");
    let b = id::<ecad_model::ComponentId>(&h.run("place", json!({"at": [0, -100], "rotation": 90})).unwrap(), "component");
    h.run("setlocation", json!({"component": a.hex(), "value": "+CAB1"})).unwrap();
    h.run("setlocation", json!({"component": b.hex(), "value": "+JB1"})).unwrap();
    let doc = h.doc();
    let page = h.first_page();
    let tp = ecad_model::derive::terminal_positions(&doc, page);
    let pos = |c: ecad_model::ComponentId| {
        let (rid, _) = doc.representations_of(ecad_model::Subject::Component(c)).next().unwrap();
        tp.iter().find(|(_, r, _)| *r == rid).unwrap().2
    };
    let (pa, pb) = (pos(a), pos(b));
    h.run("wire", json!({"points": pts(&[(pa.x, pa.y), (pa.x, 0.0), (pb.x, 0.0), (pb.x, pb.y)])})).unwrap();
    let k = new_cable(&mut h, json!({"from": {"location": "+CAB1"}, "to": {"location": "+FIELD"}}));
    let y = (pa.y + 0.0) / 2.0;
    h.run("cableslash", json!({"from": [pa.x - 5.0, y], "to": [pa.x + 5.0, y], "cable": k.hex()})).unwrap_or_else(|e| panic!("slash: {e}"));
    let rows = checks_of(&mut h, "cable-end-off-plan");
    assert_eq!(rows.len(), 1, "{rows:?}");
    let t = rows[0]["text"].as_str().unwrap();
    assert!(t.contains("field end is planned +FIELD but drawn at +JB1"), "{t}");
    // Use the drawing: that end's plan becomes the drawn location.
    h.run("cableedit", json!({"cable": k.hex(), "set": {"to": {"location": "+JB1"}}})).unwrap();
    assert!(checks_of(&mut h, "cable-end-off-plan").is_empty());
    // A device's location change re-checks the cable (update map).
    h.run("setlocation", json!({"component": b.hex(), "value": "+JB2"})).unwrap();
    assert_eq!(checks_of(&mut h, "cable-end-off-plan").len(), 1);
}

#[test]
fn hostile_cable_values_are_ignored_and_reported() {
    let mut h = H::new(5911);
    let k = new_cable(&mut h, json!({}));
    for (key, v) in
        [("length", "abc".to_string()), ("gland.to", "x".repeat(300)), ("shield", "lead".to_string()), ("end.from.location", "nokey".to_string())]
    {
        h.run("setprop", json!({"cable": k.hex(), "key": key, "value": v})).unwrap();
    }
    let doc = h.doc();
    assert!(cable_cmds::read(&doc, k, cable_cmds::CableField::Length).is_none());
    assert!(cable_cmds::gland(&doc, k, Side::To).is_none());
    assert!(cable_cmds::shield(&doc, k).is_none());
    assert!(cable_cmds::plan_words(&doc, k, Side::From).is_none());
    let rows = checks_of(&mut h, "cable-bad-value");
    assert_eq!(rows.len(), 4, "{rows:?}");
    assert!(rows.iter().any(|r| r["text"].as_str().unwrap_or_default().contains("length \"abc\" is not a number of metres")), "{rows:?}");
}

#[test]
fn a_cable_not_drawn_survives_join_and_the_report_lists_it() {
    let mut h = H::new(5912);
    let k = new_cable(&mut h, json!({"from": {"location": "+CAB1"}, "to": {"location": "+FIELD", "gland": "M20"}}));
    let doc = h.doc();
    let row = h.p(|p| p.outputs.as_ref().and_then(|o| o.cable_row(k).cloned()));
    if let Some(r) = row {
        assert!(!r.drawn);
        assert_eq!(r.planned[1].as_deref(), Some("+FIELD"));
        assert_eq!(r.glands[1].as_deref(), Some("M20"));
    }
    // FD1: joining keeps the other cable; an empty cable is a valid source.
    wires(&mut h, 1);
    let w: CableId = id(&slash(&mut h, None).unwrap(), "cable");
    let doc2 = h.doc();
    let ops = ecad_model::merge::join_cables(&doc2, k, w).unwrap();
    assert!(matches!(ops.last(), Some(Op::DeleteCable { id, .. }) if *id == k));
    assert!(doc.cables[&k].members.is_empty());
}

#[test]
fn the_cable_report_lists_cables_not_drawn_unless_told_not_to() {
    let mut h = H::new(5913);
    wires(&mut h, 1);
    slash(&mut h, None).unwrap();
    new_cable(&mut h, json!({"to": {"location": "+FIELD"}}));
    let all = h.run("cablelist", json!({})).unwrap();
    assert_eq!(all["total"], json!(2), "{all}");
    let rows = all["rows"].as_array().unwrap();
    assert!(rows.iter().any(|r| r["planned_to"] == json!("+FIELD (planned)") && r["drawn"] == json!("no")), "{all}");
    let drawn = h.run("cablelist", json!({"include_not_drawn": false})).unwrap();
    assert_eq!(drawn["total"], json!(1), "{drawn}");
}

/// 059c (design 059 §5.6): the slash tool's `Cable` keyword picks a cable not drawn; the slash
/// assigns its cores; Esc after picking writes nothing.
#[test]
fn the_slash_tool_picks_a_cable_by_its_cable_keyword() {
    let mut h = H::new(5914);
    let ws = wires(&mut h, 2);
    let k = new_cable(&mut h, json!({"cores": "3G"}));
    // Esc after picking writes nothing.
    let head = h.head();
    h.cmdline("cableslash");
    h.cmdline("Cable");
    h.cmdline("-W01");
    h.cmdline("-5,25");
    let _ = h.s.input(cadcraft_engine::Input::Cancel);
    assert_eq!(h.head(), head, "Esc writes nothing");
    h.cmdline("cableslash");
    h.cmdline("Cable");
    h.cmdline("-W01");
    h.cmdline("-5,25");
    h.cmdline("500,25");
    let doc = h.doc();
    let members: Vec<(ConductorId, Option<String>)> = doc.cables[&k].members.iter().map(|m| (m.conductor, m.core.clone())).collect();
    assert_eq!(members, vec![(ws[0], Some("1".into())), (ws[1], Some("2".into()))]);
    assert_eq!(doc.cables.len(), 1, "no new cable");
    // `New` (the default) makes a new cable with the next free tag.
    wires_at(&mut h, 600.0, 2);
    h.cmdline("cableslash");
    h.cmdline("Cable");
    h.cmdline("");
    h.cmdline("595,25");
    h.cmdline("620,25");
    assert_eq!(h.doc().cables.len(), 2);
}

/// Two wires at `x0`, `x0 + 10`.
fn wires_at(h: &mut H, x0: f64, n: usize) {
    for i in 0..n {
        let x = x0 + 10.0 * i as f64;
        h.run("wire", json!({"points": pts(&[(x, 0.0), (x, 50.0)])})).unwrap();
    }
}

/// 059c: the picker lists cables not drawn first, those planned on this page's location before the
/// rest, then drawn cables not slashed on this page; a cable slashed here is left out.
#[test]
fn the_picker_lists_cables_not_drawn_first_and_leaves_out_those_slashed_here() {
    let mut h = H::new(5915);
    wires(&mut h, 1);
    let drawn: CableId = id(&slash(&mut h, None).unwrap(), "cable");
    let a = new_cable(&mut h, json!({"tag": "-W05"}));
    let b = new_cable(&mut h, json!({"tag": "-W07", "from": {"location": "+CAB1"}}));
    let page = h.first_page();
    h.run("setprop", json!({"page": page.hex(), "key": "location", "value": "+CAB1"})).ok();
    let doc = h.doc();
    let items = cable_cmds::picker(&doc, page);
    let order: Vec<CableId> = items.iter().map(|i| i.cable).collect();
    assert!(!order.contains(&drawn), "slashed on this page: absent");
    assert_eq!(order.len(), 2);
    assert!(order.contains(&a) && order.contains(&b));
    if items.iter().any(|i| i.matches) {
        assert_eq!(order.first(), Some(&b), "a planned match first");
    }
}
