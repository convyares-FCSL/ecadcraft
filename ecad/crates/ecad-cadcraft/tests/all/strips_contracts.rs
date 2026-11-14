//! Brief 028 acceptance (design 028 §12): terminals placed on wires, Place Terminals, bridges,
//! cores, the terminal plan and cable schedule, the BOM per terminal type, the Checks rows, and
//! the update maps. Driven through the commands' JSON forms on fixture T.

use ecad_cadcraft::checks::{Fix, Kind};
use ecad_cadcraft::outputs::{Group, Outputs};
use ecad_cadcraft::strips::Strips;
use ecad_model::{Author, CableId, ComponentId, ConductorId, End, Op, PageId, TerminalId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

/// Fixture T's objects.
struct T {
    h: H,
    page: PageId,
    k1: ComponentId,
    wires: [ConductorId; 4],
    cable: CableId,
    m1: ComponentId,
}

fn place(h: &mut H, symbol: &str, at: (f64, f64)) -> Value {
    h.run("place", json!({"symbol": symbol, "at": [at.0, at.1]})).unwrap()
}

/// Fixture T (design 028 §12): `+CP1` over the top half, `+FIELD` over the bottom half; `-K01`
/// and `-K02` with contacts in `+CP1`; four wires down to `-M01:U1`, `V1`, `W1` and `-S01:13` in
/// `+FIELD`; `-W01` (`7G1.5`) slashed across them inside `+FIELD`.
fn fixture(seed: u64) -> T {
    let mut h = H::new(seed);
    let page = h.first_page();
    h.run("locationbox", json!({"location": "+CP1", "corners": [[20, 150], [400, 280]]})).unwrap();
    h.run("locationbox", json!({"location": "+FIELD", "corners": [[20, 20], [400, 140]]})).unwrap();
    let k1: ComponentId = id(&place(&mut h, "coil", (100.0, 220.0)), "component");
    let k2: ComponentId = id(&place(&mut h, "coil", (150.0, 220.0)), "component");
    h.run("place", json!({"symbol": "no", "at": [200, 220], "device": k1.hex()})).unwrap();
    h.run("place", json!({"symbol": "no", "at": [250, 220], "device": k2.hex()})).unwrap();
    let m1: ComponentId = id(&place(&mut h, "motor3", (110.0, 50.0)), "component");
    place(&mut h, "pbno", (250.0, 60.0));
    let routes = [
        vec![(100.0, 210.0), (100.0, 65.0)],
        vec![(150.0, 210.0), (150.0, 100.0), (110.0, 100.0), (110.0, 65.0)],
        vec![(200.0, 210.0), (200.0, 95.0), (120.0, 95.0), (120.0, 65.0)],
        vec![(250.0, 210.0), (250.0, 70.0)],
    ];
    let mut wires = Vec::new();
    for r in &routes {
        let v = h.run("wire", json!({"points": pts(r)})).unwrap();
        assert_eq!(v["attached"].as_array().unwrap().len(), 2, "{v}");
        wires.push(id::<ConductorId>(&v, "conductor"));
    }
    let v = h.run("cableslash", json!({"from": [90, 120], "to": [260, 120], "tag": "-W01"})).unwrap();
    let cable: CableId = id(&v, "cable");
    h.run("setprop", json!({"cable": cable.hex(), "key": "type", "value": "YSLY-JZ 7G1.5"})).unwrap();
    let wires = [wires[0], wires[1], wires[2], wires[3]];
    T { h, page, k1, wires, cable, m1 }
}

/// The kept models equal a fresh build (the update map left nothing behind).
fn same_as_built(h: &mut H) {
    let (kept, built) = h.cfg(|p| {
        let kept = p.outputs().clone();
        (kept, Outputs::build(p.ledger.document()))
    });
    assert!(kept.strips.same_as(&built.strips), "strips differ:\nkept {:#?}\nbuilt {:#?}", kept.strips.strips, built.strips.strips);
    assert_eq!(kept.cable_table(), built.cable_table());
    assert_eq!(kept.wire_table(), built.wire_table());
    assert_eq!(kept.bom_table(Group::Location), built.bom_table(Group::Location));
    assert_eq!(kept.bom_table(Group::Part), built.bom_table(Group::Part));
}

fn strip_plan(h: &mut H) -> Value {
    h.run("stripplan", json!({})).unwrap()
}

/// A: a terminal clicked on a wire splits it: two conductors on one net, one number; `u`
/// restores the single wire and no `-X01`.
#[test]
fn a_terminal_on_a_wire_splits_it_and_undo_restores_it() {
    let mut t = fixture(0x28_01);
    t.h.cfg(|p| {
        p.outputs();
    });
    let before = t.h.doc();
    let head = t.h.head();
    let v = place(&mut t.h, "terminal", (100.0, 170.0));
    assert_eq!(t.h.head(), head + 1, "one transaction");
    assert_eq!(v["tag"], "-X01", "{v}");
    assert!(v["message"].as_str().unwrap().starts_with("Placed -X01:1 on wire"), "{v}");
    let new: ConductorId = id(&v["split"], "new");
    let x1: ComponentId = id(&v, "component");
    let term: TerminalId = id(&v["terminals"][0], "terminal");
    let doc = t.h.doc();
    assert_eq!(doc.conductors[&t.wires[0]].end(End::B), Some(term));
    assert_eq!(doc.conductors[&new].end(End::A), Some(term));
    let conn = doc.connectivity();
    assert_eq!(conn.net_of_conductor(t.wires[0]), conn.net_of_conductor(new), "one net");
    // The cable's slash crosses the second half, so it keeps the membership.
    assert!(doc.cables[&t.cable].members.iter().any(|m| m.conductor == new));
    assert!(!doc.cables[&t.cable].members.iter().any(|m| m.conductor == t.wires[0]));
    assert_eq!(v["cable_kept_by"]["conductor"], json!(new.hex()));
    assert_eq!(t.h.p(|p| p.ledger.log().last().unwrap().label.clone()), "Place terminal");
    same_as_built(&mut t.h);
    let plan = strip_plan(&mut t.h);
    let row = &plan["strips"][0]["rows"][0];
    assert_eq!(row["label"], "1");
    assert_eq!(row["inside"][0]["target"], "-K01:A2", "{plan}");
    assert_eq!(row["outside"][0]["cable"], "-W01", "{plan}");
    assert_eq!(row["outside"][0]["target"], "-M01:U1", "{plan}");
    assert_eq!(row["inside"][0]["wire"], row["outside"][0]["wire"], "one wire number");
    t.h.cmdline("u");
    let after = t.h.doc();
    assert_eq!(after.conductors, before.conductors);
    assert!(!after.components.contains_key(&x1));
    same_as_built(&mut t.h);
    // Refusals with a way forward.
    let e = t.h.run("place", json!({"symbol": "terminal", "at": [100, 210]})).unwrap_err();
    assert!(e.contains("That is the end of the wire at -K01:A2"), "{e}");
    place(&mut t.h, "terminal", (100.0, 170.0));
    let e = t.h.run("place", json!({"symbol": "terminal", "at": [100, 170]})).unwrap_err();
    assert!(e.contains("is already there. Click elsewhere on the wire."), "{e}");
}

/// B, C, E, F: Place Terminals across the four wires, a bridge, the cores and the plan.
#[test]
fn place_terminals_bridges_cores_and_the_plan() {
    let mut t = fixture(0x28_02);
    t.h.cfg(|p| {
        p.outputs();
    });
    let head = t.h.head();
    let v = t.h.run("terminals", json!({"from": [90, 170], "to": [260, 170]})).unwrap();
    assert_eq!(t.h.head(), head + 1, "one transaction");
    assert_eq!(v["message"], "Placed -X01:1\u{2026}4 on 4 wires.", "{v}");
    let terms: Vec<TerminalId> = v["terminals"].as_array().unwrap().iter().map(|x| id(x, "terminal")).collect();
    let labels: Vec<&str> = v["terminals"].as_array().unwrap().iter().map(|x| x["label"].as_str().unwrap()).collect();
    assert_eq!(labels, ["1", "2", "3", "4"]);
    let doc = t.h.doc();
    // The cable's members are the field halves (§3.1).
    for (i, x) in v["terminals"].as_array().unwrap().iter().enumerate() {
        let new: ConductorId = id(x, "new");
        assert!(doc.cables[&t.cable].members.iter().any(|m| m.conductor == new), "member {i}");
    }
    same_as_built(&mut t.h);
    // C: a bridge between 3 and 4.
    let head = t.h.head();
    let b = t.h.run("bridge", json!({"from": terms[2].hex(), "to": terms[3].hex()})).unwrap();
    assert_eq!(t.h.head(), head + 1);
    assert_eq!(b["bridge"], true);
    let bridge: ConductorId = id(&b, "conductor");
    let plan = strip_plan(&mut t.h);
    let rows = plan["strips"][0]["rows"].as_array().unwrap();
    assert_eq!(rows[2]["bridge_group"], json!(1), "{plan}");
    assert_eq!(rows[3]["bridge_group"], json!(1));
    assert_eq!(rows[0]["bridge_group"], Value::Null);
    let wl = t.h.run("wirelist", json!({})).unwrap();
    let row = wl["rows"].as_array().unwrap().iter().find(|r| r["conductor"] == json!(bridge.hex())).unwrap().clone();
    assert_eq!(row["cable"], "Bridge -X01", "{row}");
    let conn = t.h.doc();
    let c = conn.connectivity();
    let n3 = c.net_of_conductor(bridge);
    assert_eq!(n3, c.net_of_conductor(t.wires[2]), "bridged terminals share a net");
    assert_eq!(n3, c.net_of_conductor(t.wires[3]));
    let again = t.h.run("bridge", json!({"from": terms[2].hex(), "to": terms[3].hex()})).unwrap();
    assert_eq!(again["changed"], false);
    assert_eq!(again["message"], "-X01:3 and -X01:4 are already bridged.");
    same_as_built(&mut t.h);
    // E: the cores.
    let cl = t.h.run("cablelist", json!({})).unwrap();
    let r = &cl["rows"][0];
    assert_eq!(r["cores"], 7, "{cl}");
    assert_eq!(r["used"], 4);
    assert_eq!(r["spare"], json!(["5", "6", "GNYE"]));
    assert_eq!(r["spare_text"], "3 (5, 6, GNYE)");
    assert_eq!(r["from_devices"], json!(["-X01"]), "{r}");
    assert_eq!(r["to_devices"], json!(["-M01", "-S01"]), "{r}");
    // A fifth wire added to -W01 takes core 5.
    let w5 = t.h.run("wire", json!({"points": pts(&[(300.0, 200.0), (300.0, 50.0)])})).unwrap();
    let w5: ConductorId = id(&w5, "conductor");
    t.h.run("cableadd", json!({"cable": t.cable.hex(), "conductors": [w5.hex()]})).unwrap();
    let doc = t.h.doc();
    assert_eq!(doc.cables[&t.cable].members.iter().find(|m| m.conductor == w5).unwrap().core.as_deref(), Some("5"));
    let cores = t.h.run("cablecores", json!({"cable": t.cable.hex()})).unwrap();
    assert_eq!(cores["offer"], "7G");
    assert_eq!(cores["source"], "type");
    assert_eq!(cores["cores"].as_array().unwrap().len(), 7);
    assert_eq!(cores["cores"][6]["state"], "spare");
    // Three more: an eighth member gives too-many-cores; nothing is refused.
    let mut more = Vec::new();
    for x in [310.0, 320.0, 330.0] {
        let w = t.h.run("wire", json!({"points": pts(&[(x, 200.0), (x, 50.0)])})).unwrap();
        more.push(id::<ConductorId>(&w, "conductor").hex());
    }
    t.h.run("cableadd", json!({"cable": t.cable.hex(), "conductors": more})).unwrap();
    let checks = t.h.run("checks", json!({"kinds": ["too-many-cores"]})).unwrap();
    let rows = checks["checks"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{checks}");
    assert_eq!(rows[0]["which"], "-W01: 8 cores used, cable offers 7 (YSLY-JZ 7G1.5)");
    same_as_built(&mut t.h);
    // F: inside on -K0x, outside -W01 cores to -M01/-S01.
    let plan = strip_plan(&mut t.h);
    let rows = plan["strips"][0]["rows"].as_array().unwrap().clone();
    let inside: Vec<&str> = rows.iter().map(|r| r["inside"][0]["target"].as_str().unwrap_or("")).collect();
    assert_eq!(inside, ["-K01:A2", "-K02:A2", "-K01:14", "-K02:14"], "{plan}");
    let outside: Vec<&str> = rows.iter().map(|r| r["outside"][0]["target"].as_str().unwrap_or("")).collect();
    assert_eq!(outside, ["-M01:U1", "-M01:V1", "-M01:W1", "-S01:13"]);
    assert!(rows.iter().all(|r| r["outside"][0]["cable"] == "-W01"));
    assert_eq!(plan["strips"][0]["location"], "+CP1");
}

/// F (second half): a field device wired without a cable is outside by its location; moving it
/// into `+CP1` flips only its row.
#[test]
fn moving_a_field_device_flips_only_its_rows() {
    let mut t = fixture(0x28_03);
    t.h.cfg(|p| {
        p.outputs();
    });
    let v = t.h.run("terminals", json!({"from": [90, 170], "to": [260, 170]})).unwrap();
    let _ = v;
    let lamp = place(&mut t.h, "lamp", (340.0, 60.0));
    let h1: ComponentId = id(&lamp, "component");
    // A wire from -K01's coil top? Use a free wire from (330,175) to the lamp X1 (340, 70).
    let k = t.h.run("wire", json!({"points": pts(&[(330.0, 200.0), (330.0, 70.0), (340.0, 70.0)])})).unwrap();
    let _w: ConductorId = id(&k, "conductor");
    // Land -X01:5 on it inside +CP1.
    let x = t.h.run("place", json!({"symbol": "terminal", "at": [330, 170], "tag": "-X01"})).unwrap();
    assert_eq!(x["joined"], true, "{x}");
    same_as_built(&mut t.h);
    let plan = strip_plan(&mut t.h);
    let row5 = plan["strips"][0]["rows"][4].clone();
    assert_eq!(row5["label"], "5");
    assert_eq!(row5["outside"][0]["target"], "-H01:X1", "{row5}");
    let others: Vec<Value> = plan["strips"][0]["rows"].as_array().unwrap()[..4].to_vec();
    // Move -H01 into +CP1: its far side matches the strip's location, so it is inside now.
    let item = t.h.symbol_item(h1);
    let hd = t.h.handle_of(t.page, item);
    t.h.run("move", json!({"handles": [hd.hex()], "delta": [0, 180]})).unwrap();
    let work = t.h.p(|p| p.outputs.as_ref().unwrap().strips.last_work);
    assert_eq!(work, 1, "one strip recomputed");
    same_as_built(&mut t.h);
    let plan = strip_plan(&mut t.h);
    let row5 = plan["strips"][0]["rows"][4].clone();
    assert_eq!(row5["inside"].as_array().unwrap().len(), 2, "{row5}");
    assert_eq!(plan["strips"][0]["rows"].as_array().unwrap()[..4].to_vec(), others, "only row 5 changed");
}

/// G: the offer, a PE terminal, spares and the BOM per terminal type.
#[test]
fn the_offer_gives_spares_and_the_bom_counts_per_terminal_type() {
    let mut t = fixture(0x28_04);
    t.h.cfg(|p| {
        p.outputs();
    });
    let v = t.h.run("terminals", json!({"from": [90, 170], "to": [260, 170]})).unwrap();
    let x1: ComponentId = id(&v, "strip");
    let e = t.h.run("setprop", json!({"component": x1.hex(), "key": "terminals", "value": "12X"})).unwrap_err();
    // Brief 036 (FP1): the refusal names the type codes too.
    assert!(e.contains("\"12X\" is not a terminal count. Write 12, 12+2PE or 12+4x3"), "{e}");
    t.h.run("setprop", json!({"component": x1.hex(), "key": "terminals", "value": "12 + 2pe"})).unwrap();
    assert_eq!(t.h.doc().components[&x1].properties["terminals"], "12+2PE");
    t.h.run("place", json!({"symbol": "terminalpe", "at": [360, 250], "device": x1.hex()})).unwrap();
    same_as_built(&mut t.h);
    let bom = t.h.run("bom", json!({})).unwrap();
    let x_rows: Vec<&Value> = bom["rows"].as_array().unwrap().iter().filter(|r| r["tags"] == "-X01").collect();
    assert_eq!(x_rows.len(), 2, "{bom}");
    // Owner decision 2026-11-12: 12+2PE is 12 feed-through and 2 PE, 14 positions.
    assert_eq!(x_rows[0]["qty"], 12);
    assert_eq!(x_rows[1]["qty"], 2);
    assert_eq!(x_rows[1]["description"], "PE terminal");
    assert!(x_rows[1]["flags"].as_array().unwrap().contains(&json!("part_number")), "{bom}");
    let plan = strip_plan(&mut t.h);
    let s = &plan["strips"][0];
    assert_eq!(s["offered"], "12+2PE");
    assert_eq!((s["drawn"].as_u64(), s["spare"].as_u64()), (Some(5), Some(9)));
    let spare: Vec<(&str, &str)> = s["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["state"] == "spare")
        .map(|r| (r["label"].as_str().unwrap(), r["type"].as_str().unwrap()))
        .collect();
    assert_eq!(spare.len(), 9);
    assert_eq!(spare.iter().filter(|(_, t)| *t == "pe").count(), 1, "{spare:?}");
    // More drawn than offered: a Checks row with Offer n.
    t.h.run("setprop", json!({"component": x1.hex(), "key": "terminals", "value": "2+1PE"})).unwrap();
    let rows = ecad_cadcraft::checks::all(&t.h.doc(), 1e-6);
    let over: Vec<_> = rows.iter().filter(|c| c.kind == Kind::TooManyTerminals).collect();
    assert_eq!(over.len(), 1);
    assert_eq!(over[0].which, "-X01: 5 terminals drawn, strip offers 3");
    assert_eq!(over[0].fix, Some(Fix::OfferDrawn(x1)));
    same_as_built(&mut t.h);
}

/// H: core-twice and no-core-count rows, and their fixes' targets.
#[test]
fn core_rows_of_the_checks_list() {
    let mut t = fixture(0x28_05);
    let rows = ecad_cadcraft::checks::all(&t.h.doc(), 1e-6);
    assert!(rows.iter().all(|c| c.kind != Kind::NoCoreCount), "-W01 has a type");
    t.h.run("setprop", json!({"cable": t.cable.hex(), "key": "type", "value": null})).unwrap();
    let rows = ecad_cadcraft::checks::all(&t.h.doc(), 1e-6);
    let n: Vec<_> = rows.iter().filter(|c| c.kind == Kind::NoCoreCount).collect();
    assert_eq!(n.len(), 1);
    assert_eq!(n[0].which, "1 cable has no core count");
    assert_eq!(n[0].fix, Some(Fix::ShowCables));
    t.h.run("cablecore", json!({"cable": t.cable.hex(), "cores": ["1", "1", null, null]})).unwrap();
    let rows = ecad_cadcraft::checks::all(&t.h.doc(), 1e-6);
    let twice: Vec<_> = rows.iter().filter(|c| c.kind == Kind::CoreTwice).collect();
    assert_eq!(twice.len(), 1);
    assert_eq!(twice[0].which, "-W01: core 1 is used by two wires");
    assert_eq!(twice[0].fix, Some(Fix::NextFreeCore(t.wires[1])));
    assert_eq!(ecad_cadcraft::strips::next_free_label(&t.h.doc(), t.cable).as_deref(), Some("2"));
    t.h.run("setprop", json!({"cable": t.cable.hex(), "key": "cores", "value": "5X"})).unwrap();
    assert_eq!(ecad_cadcraft::strips::next_free_label(&t.h.doc(), t.cable).as_deref(), Some("5"));
    t.h.run("setprop", json!({"cable": t.cable.hex(), "key": "cores", "value": "4X"})).unwrap();
    assert_eq!(ecad_cadcraft::strips::next_free_label(&t.h.doc(), t.cable).as_deref(), Some("2"), "none spare: the next number");
    let e = t.h.run("setprop", json!({"cable": t.cable.hex(), "key": "cores", "value": "7Q"})).unwrap_err();
    assert!(e.contains("Write 7, 7G, 5X, 4x2x0.25 (pairs), or BN,BK,GY,BU,GNYE."), "{e}");
}

/// The generated pages: a terminal plan (all strips, or one) and the cable schedule.
#[test]
fn terminal_plan_and_cable_schedule_pages() {
    let mut t = fixture(0x28_06);
    let v = t.h.run("terminals", json!({"from": [90, 170], "to": [260, 170]})).unwrap();
    let x1: ComponentId = id(&v, "strip");
    let p = t.h.run("newpage", json!({"generated": "terminals", "strip": x1.hex()})).unwrap();
    let plan_page: PageId = id(&p, "page");
    let doc = t.h.doc();
    assert_eq!(ecad_cadcraft::generated::kind_of(&doc, plan_page), Some(ecad_cadcraft::generated::GenKind::Terminals));
    assert_eq!(ecad_cadcraft::generated::strip_of(&doc, plan_page), Some(x1));
    let o = Outputs::build(&doc);
    assert_eq!(ecad_cadcraft::generated::sheets(&doc, plan_page, Some(&o)), 1);
    let parts = ecad_cadcraft::generated::parts_of(&doc, plan_page, None);
    assert!(parts.len() > 20, "the plan draws its table");
    let c = t.h.run("newpage", json!({"generated": "cables"})).unwrap();
    let cables_page: PageId = id(&c, "page");
    assert_eq!(ecad_cadcraft::generated::sheets(&t.h.doc(), cables_page, Some(&o)), 1);
    let e = t.h.run("newpage", json!({"generated": "cables", "strip": x1.hex()})).unwrap_err();
    assert!(e.contains("terminal plan page"), "{e}");
    // Nothing is drawn on it.
    let e = t.h.run("wire", json!({"points": pts(&[(10.0, 10.0), (20.0, 10.0)])})).unwrap_err();
    assert!(e.contains("is generated (Cable schedule)"), "{e}");
    t.h.check();
}

/// The update map: an ordinary edit away from strips recomputes none; a terminal edit one.
#[test]
fn no_ordinary_edit_rebuilds_the_strips() {
    let mut t = fixture(0x28_07);
    t.h.cfg(|p| {
        p.outputs();
    });
    t.h.run("terminals", json!({"from": [90, 170], "to": [260, 170]})).unwrap();
    t.h.run("terminals", json!({"from": [90, 190], "to": [260, 190]})).unwrap();
    assert_eq!(t.h.p(|p| p.outputs.as_ref().unwrap().strips.len()), 2);
    // A coil far from any strip.
    place(&mut t.h, "coil", (350.0, 250.0));
    assert_eq!(t.h.p(|p| p.outputs.as_ref().unwrap().strips.last_work), 0);
    // A property of -M01 (a far end of -X01's rows).
    t.h.run("setprop", json!({"component": t.m1.hex(), "key": "tag", "value": "-M07"})).unwrap();
    assert_eq!(t.h.p(|p| p.outputs.as_ref().unwrap().strips.last_work), 1);
    let plan = strip_plan(&mut t.h);
    assert!(plan.to_string().contains("-M07:U1"));
    same_as_built(&mut t.h);
    // The model built whole equals the kept one after a page renumber (Place column).
    t.h.run("pageprops", json!({"number": "100"})).unwrap();
    same_as_built(&mut t.h);
    let plan = strip_plan(&mut t.h);
    assert!(plan["strips"][0]["rows"][0]["place"].as_str().unwrap().starts_with("100"), "{plan}");
    let _ = t.k1;
}

/// FP5 finding (design 028 §9.5, decisions 2026-11-12): relabelling wired terminals by
/// `RemoveTerminal` + `AddTerminal` of the same ID in one transaction is accepted, with or
/// without detaching their wires first; the wires stay attached, and undo restores the labels.
/// So no `RelabelTerminal` op is needed.
#[test]
fn relabelling_wired_terminals_by_remove_and_add_needs_no_new_op() {
    let mut t = fixture(0x28_08);
    let v = t.h.run("terminals", json!({"from": [90, 170], "to": [260, 170]})).unwrap();
    let terms: Vec<TerminalId> = v["terminals"].as_array().unwrap().iter().map(|x| id(x, "terminal")).collect();
    let (t3, t4) = (terms[2], terms[3]);
    let doc = t.h.doc();
    let relabel = |t: TerminalId, to: &str| {
        let before = doc.terminals[&t].clone();
        let mut after = before.clone();
        after.label = Some(to.into());
        [Op::RemoveTerminal { id: t, terminal: before }, Op::AddTerminal { id: t, terminal: after }]
    };
    // Bare: remove and add while still attached (3 ↔ 4, unique on the end state).
    let mut ops: Vec<Op> = relabel(t3, "4").into();
    ops.extend(relabel(t4, "3"));
    let ends_before: Vec<_> = doc.ends_at(t3).into_iter().chain(doc.ends_at(t4)).collect();
    t.h.cfg(|p| p.ledger.commit(Author::User, "swap labels", ops.clone())).unwrap();
    let d = t.h.doc();
    assert_eq!(d.terminals[&t3].label.as_deref(), Some("4"));
    assert_eq!(d.terminals[&t4].label.as_deref(), Some("3"));
    let ends_after: Vec<_> = d.ends_at(t3).into_iter().chain(d.ends_at(t4)).collect();
    assert_eq!(ends_before, ends_after, "the wires stay attached");
    let inverse: Vec<Op> = ops.iter().rev().map(Op::inverse).collect();
    t.h.cfg(|p| p.ledger.commit(Author::User, "undo swap", inverse)).unwrap();
    assert_eq!(t.h.doc().terminals, doc.terminals);
    // As the design wrote it: detach, remove, add, attach back.
    let mut ops = Vec::new();
    for (w, end) in doc.ends_at(t3) {
        ops.push(Op::AttachEnd { conductor: w, end, before: Some(t3), after: None });
    }
    ops.extend(relabel(t3, "9"));
    for (w, end) in doc.ends_at(t3) {
        ops.push(Op::AttachEnd { conductor: w, end, before: None, after: Some(t3) });
    }
    t.h.cfg(|p| p.ledger.commit(Author::User, "relabel", ops)).unwrap();
    let d = t.h.doc();
    assert_eq!(d.terminals[&t3].label.as_deref(), Some("9"));
    assert_eq!(d.ends_at(t3), doc.ends_at(t3));
    let built = Strips::build(&d, &Outputs::build(&d), &ecad_cadcraft::outputs::PageCx::of(&d));
    assert!(built.get(id(&v, "strip")).is_some());
}

/// Hostile input (design 028 §12 L): refusals with a way forward, no panic.
#[test]
fn hostile_strip_and_core_input_is_refused_with_a_way_forward() {
    let mut t = fixture(0x28_09);
    let v = t.h.run("terminals", json!({"from": [90, 170], "to": [260, 170]})).unwrap();
    let x1: ComponentId = id(&v, "strip");
    let terms: Vec<TerminalId> = v["terminals"].as_array().unwrap().iter().map(|x| id(x, "terminal")).collect();
    let long = "9".repeat(10_000);
    assert!(t.h.run("setprop", json!({"component": x1.hex(), "key": "terminals", "value": long})).is_err());
    assert!(t.h.run("setprop", json!({"cable": t.cable.hex(), "key": "cores", "value": "1000"})).unwrap_err().contains("at most 100"));
    let e = t.h.run("bridge", json!({"from": terms[0].hex(), "to": terms[0].hex()})).unwrap_err();
    assert!(e.contains("Pick two different terminals"), "{e}");
    let e = t.h.run("terminals", json!({"from": [0, 280], "to": [10, 280]})).unwrap_err();
    assert!(e.contains("The line crosses no wire. Draw it across the wires to land."), "{e}");
    let e = t.h.run("terminals", json!({"from": [90, 180], "to": [260, 180], "strip": t.m1.hex()})).unwrap_err();
    assert!(e.contains("not a terminal strip"), "{e}");
    // Two terminals with one label (a hostile file): an Error row with Renumber.
    let doc = t.h.doc();
    let before = doc.terminals[&terms[1]].clone();
    let mut after = before.clone();
    after.label = Some("1".into());
    t.h.cfg(|p| {
        p.ledger.commit(
            Author::User,
            "dup",
            vec![Op::RemoveTerminal { id: terms[1], terminal: before }, Op::AddTerminal { id: terms[1], terminal: after }],
        )
    })
    .unwrap();
    let rows = ecad_cadcraft::checks::all(&t.h.doc(), 1e-6);
    let twice: Vec<_> = rows.iter().filter(|c| c.kind == Kind::TerminalTwice).collect();
    assert_eq!(twice.len(), 1, "{rows:?}");
    assert_eq!(twice[0].which, "-X01:1 is drawn twice");
    assert_eq!(twice[0].fix, Some(Fix::RenumberStrip(x1)));
}

/// A fence across more than 200 wires is refused before anything is written.
#[test]
fn a_fence_across_too_many_wires_is_refused() {
    let mut h = H::new(0x28_0a);
    for i in 0..201 {
        let x = 10.0 + f64::from(i) * 1.5;
        h.run("wire", json!({"points": pts(&[(x, 0.0), (x, 50.0)])})).unwrap();
    }
    let head = h.head();
    let e = h.run("terminals", json!({"from": [0, 25], "to": [400, 25]})).unwrap_err();
    assert!(e.contains("That line crosses 201 wires; at most 200 at once. Draw a shorter line."), "{e}");
    assert_eq!(h.head(), head);
}

/// D (design 028 §12): renumber `{kind: "terminals"}` labels each strip's terminals 1…n in
/// drawing order; a preview lists the rows; one transaction, one `u`; the wires stay attached.
#[test]
fn renumbering_terminals_follows_drawing_order() {
    let mut t = fixture(0x28_0b);
    t.h.cfg(|p| {
        p.outputs();
    });
    let v = t.h.run("terminals", json!({"from": [90, 170], "to": [260, 170]})).unwrap();
    let x1: ComponentId = id(&v, "strip");
    let terms: Vec<TerminalId> = v["terminals"].as_array().unwrap().iter().map(|x| id(x, "terminal")).collect();
    // Move terminal 4's symbol left of terminal 1 (its wires follow).
    let doc = t.h.doc();
    let rep4 = doc
        .representations_of(ecad_model::Subject::Component(x1))
        .find(|(_, r)| r.connection_points.contains_key(&terms[3]))
        .map(|(_, r)| r.items[0])
        .unwrap();
    let hd = t.h.handle_of(t.page, rep4);
    t.h.run("move", json!({"handles": [hd.hex()], "delta": [-170, 0]})).unwrap();
    let ends: Vec<_> = terms.iter().map(|x| t.h.doc().ends_at(*x)).collect();
    // The preview lists every terminal, -X01:4 → -X01:1 first in drawing order.
    let pv = t.h.run("renumber", json!({"kind": "terminals", "preview": true})).unwrap();
    let rows = pv["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 4, "{pv}");
    assert!(rows.iter().any(|r| r["old"] == "-X01:4" && r["new"] == "-X01:1" && r["kind"] == "terminals"), "{pv}");
    let head = t.h.head();
    let r = t.h.run("renumber", json!({"kind": "terminals"})).unwrap();
    assert_eq!(t.h.head(), head + 1, "one transaction");
    assert_eq!(r["terminals"], 4, "{r}");
    let d = t.h.doc();
    let label = |x: TerminalId| d.terminals[&x].label.clone().unwrap();
    assert_eq!([label(terms[3]), label(terms[0]), label(terms[1]), label(terms[2])], ["1", "2", "3", "4"]);
    let after: Vec<_> = terms.iter().map(|x| d.ends_at(*x)).collect();
    assert_eq!(ends, after, "the wires stay attached");
    let ops = t.h.p(|p| p.ledger.log().last().unwrap().ops.clone());
    assert!(ops.iter().all(|o| !matches!(o, Op::AttachEnd { .. })), "Remove + Add of the same ID, no detaching");
    same_as_built(&mut t.h);
    let plan = strip_plan(&mut t.h);
    assert_eq!(plan["strips"][0]["rows"][0]["outside"][0]["target"], "-S01:13", "{plan}");
    t.h.cmdline("u");
    assert_eq!(t.h.doc().terminals[&terms[3]].label.as_deref(), Some("4"));
    same_as_built(&mut t.h);
    // The Checks fix's form: one strip by `strips`.
    let r = t.h.run("renumber", json!({"kind": "terminals", "scope": "selection", "strips": [terms[0].hex()], "start": 11})).unwrap();
    assert_eq!(r["terminals"], 4, "{r}");
    let e = t.h.run("renumber", json!({"kind": "terminals", "scope": "selection", "strips": [t.m1.hex()]})).unwrap_err();
    assert!(e.contains("not a terminal strip"), "{e}");
}

/// Review of PR #55, finding 1: with a cable schedule page and an all-strips terminal plan page in
/// the project (shown or not), an edit that touches neither makes no sorted list and rebuilds no
/// frame; the kept sheet counts equal a build.
#[test]
fn generated_page_counts_need_no_walk_on_an_unrelated_edit() {
    let mut t = fixture(0x28_0c);
    t.h.run("terminals", json!({"from": [90, 170], "to": [260, 170]})).unwrap();
    let far: ComponentId = id(&place(&mut t.h, "coil", (370.0, 260.0)), "component");
    let cables_page: PageId = id(&t.h.run("newpage", json!({"generated": "cables"})).unwrap(), "page");
    let plan_page: PageId = id(&t.h.run("newpage", json!({"generated": "terminals"})).unwrap(), "page");
    t.h.cfg(|p| {
        p.outputs();
    });
    for shown in [plan_page, cables_page, t.page] {
        t.h.activate(shown);
        let sorts = t.h.p(|p| p.outputs.as_ref().unwrap().sorts.load(std::sync::atomic::Ordering::Relaxed));
        let builds = t.h.p(|p| p.frame_builds);
        t.h.run("setprop", json!({"component": far.hex(), "key": "part_number", "value": format!("K-{}", shown.hex())})).unwrap();
        t.h.activate(shown);
        assert_eq!(t.h.p(|p| p.outputs.as_ref().unwrap().sorts.load(std::sync::atomic::Ordering::Relaxed)), sorts, "no sort");
        assert_eq!(t.h.p(|p| p.frame_builds), builds, "no frame rebuilt");
    }
    let (kept, doc) = t.h.cfg(|p| (p.outputs().clone(), p.ledger.document().clone()));
    let built = Outputs::build(&doc);
    for page in [plan_page, cables_page] {
        let s = |o: &Outputs| ecad_cadcraft::generated::sheets(&doc, page, Some(o));
        assert_eq!(s(&kept), s(&built));
    }
    assert_eq!(kept.generated_pages(), built.generated_pages());
    same_as_built(&mut t.h);
}

/// Review of PR #55, findings 3–5: `stripplan` pages by rows inside one strip and leaves absent
/// keys out; a spare core has no `from`; `terminals` on a device that is not a strip is free text.
#[test]
fn json_forms_leave_absent_keys_out_and_page_inside_a_strip() {
    let mut t = fixture(0x28_0d);
    let v = t.h.run("terminals", json!({"from": [90, 170], "to": [260, 170]})).unwrap();
    let x1: ComponentId = id(&v, "strip");
    t.h.run("setprop", json!({"component": x1.hex(), "key": "terminals", "value": "10"})).unwrap();
    let a = t.h.run("stripplan", json!({"limit": 4})).unwrap();
    assert_eq!(a["total"], 10, "{a}");
    assert_eq!(a["next"], 4);
    assert_eq!(a["strips"][0]["rows"].as_array().unwrap().len(), 4);
    let b = t.h.run("stripplan", json!({"limit": 4, "after": 4})).unwrap();
    assert_eq!(b["strips"][0]["first_row"], 4, "{b}");
    assert_eq!(b["strips"][0]["rows"][0]["label"], "5");
    assert!(b["strips"][0]["rows"][0].get("terminal").is_none(), "a spare row has no terminal key");
    assert!(b["strips"][0]["rows"][0].get("place").is_none());
    let c = t.h.run("stripplan", json!({"strip": x1.hex(), "limit": 5000, "after": 8})).unwrap();
    assert_eq!(c["strips"][0]["rows"].as_array().unwrap().len(), 2);
    assert!(c.get("next").is_none(), "{c}");
    let row0 = &a["strips"][0]["rows"][0];
    assert!(row0.get("bridge_group").is_none(), "{row0}");
    assert!(row0["inside"][0].get("cable").is_none(), "inside cells have no cable key");
    let cores = t.h.run("cablecores", json!({"cable": t.cable.hex()})).unwrap();
    let spare = cores["cores"].as_array().unwrap().iter().find(|c| c["state"] == "spare").unwrap().clone();
    assert!(spare.get("from").is_none() && spare.get("conductor").is_none(), "{spare}");
    t.h.run("setprop", json!({"cable": t.cable.hex(), "key": "type", "value": null})).unwrap();
    let cl = t.h.run("cablelist", json!({})).unwrap();
    assert!(cl["rows"][0].get("spare").is_none(), "unknown offer: no spare key: {cl}");
    let cores = t.h.run("cablecores", json!({"cable": t.cable.hex()})).unwrap();
    assert!(cores.get("offer").is_none() && cores.get("spare").is_none(), "{cores}");
    // On a coil, `terminals` is ordinary free text.
    t.h.run("setprop", json!({"component": t.k1.hex(), "key": "terminals", "value": "see sheet 4"})).unwrap();
    assert_eq!(t.h.doc().components[&t.k1].properties["terminals"], "see sheet 4");
    let e = t.h.run("setprop", json!({"component": x1.hex(), "key": "terminals", "value": "see sheet 4"})).unwrap_err();
    assert!(e.contains("is not a terminal count"), "{e}");
}
