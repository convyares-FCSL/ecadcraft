//! Tester 050b (wire categories): acceptance checks from outside, written from brief 050b and
//! design 050 (§5, §6.9, §10, §12 checks 11–18, §13 FP5/FP6 and the overseer's old-key ruling)
//! only, before the implementation existed.
//!
//! The checks drive `style {catalogue: "wire"}`, `category`, `wire {category}`, `potential`,
//! `wiresettings`, `setprop`, `bom`, `wirelist`, `checks` and read the document, the page drawing
//! (`wires::labels_on_page`, `look::coloured_routes`, the route's entity) and the prints. The one
//! new Rust name used is the design's `styles::wire::wire_look` (§5.3).
//!
//! Each test name starts with the §12 check (`b11`…`b18`) or the focus item it checks.

use cadcraft_engine::doc::color::Color;
use cadcraft_engine::doc::{EntityKind, Lineweight};
use ecad_cadcraft::{look, printbook, wires};
use ecad_model::{ConductorId, ObjectRef, Op, PageId};
use ecad_tokens::sheet::PAPER;
use serde_json::{Value, json};

use super::lane_050a_tester::{
    checks_all, checks_of, code_colour, decoded_ops, field, field_num, field_text, find_obj_mut, inject, inverses_restore, last_tx_json, num, one_tx,
    op_names, own_id, records_round_trip, redo, refused, rows_of, s, save_and_reopen, style_row, styles, tmp_dir, undo,
};
use crate::common::{H, id, pts};

// ------------------------------------------------------------------------------- helpers

/// The ten built-in categories of §5.2: id, name, potential, colour, cross-section, line colour,
/// line weight.
const BUILTIN: [(&str, &str, &str, &str, &str, &str, f64); 10] = [
    ("l", "Power L", "L", "BN", "2.5", "ink", 0.6),
    ("l1", "Power L1", "L1", "BN", "2.5", "ink", 0.6),
    ("l2", "Power L2", "L2", "BK", "2.5", "ink", 0.6),
    ("l3", "Power L3", "L3", "GY", "2.5", "ink", 0.6),
    ("n", "Neutral", "N", "BU", "2.5", "ink", 0.6),
    ("pe", "Protective earth", "PE", "GNYE", "2.5", "wire", 0.25),
    ("dc24", "24 V DC", "24V", "BU", "0.5", "wire", 0.25),
    ("dc0", "0 V DC", "0V", "BU", "0.5", "wire", 0.25),
    ("signal", "Signal", "", "none", "0.5", "ink", 0.25),
    ("analogue", "Analogue", "", "none", "0.5", "ink", 0.25),
];

/// A horizontal wire at height `y` on the active page, with an optional potential.
fn wire_at(h: &mut H, y: f64, potential: Option<&str>) -> ConductorId {
    let mut p = json!({"points": pts(&[(0.0, y), (60.0, y)])});
    if let Some(v) = potential {
        p["potential"] = json!(v);
    }
    id(&h.run("wire", p.clone()).unwrap_or_else(|e| panic!("wire {p}: {e}")), "conductor")
}

fn label(h: &mut H, page: PageId, c: ConductorId) -> Option<String> {
    wires::labels_on_page(&h.doc(), page).into_iter().find(|l| l.conductor == c).map(|l| l.text)
}

/// The colour `c`'s route is drawn in on screen (`None`: ink).
fn route_colour(h: &mut H, page: PageId, c: ConductorId) -> Option<Color> {
    let doc = h.doc();
    let routes = h.p(|p| look::coloured_routes(&doc, page, p.pages.get(&page).map(|v| &v.map), &PAPER));
    routes.iter().find(|r| r.conductor == c).map(|r| Color::True(super::lane_050a_tester::rgb(r.colours.solid)))
}

fn route_entity(h: &mut H, page: PageId, c: ConductorId) -> std::sync::Arc<cadcraft_engine::doc::Entity> {
    let route = h.route_handle(page, c);
    h.pv(|p, pg| pg.entities_of(p, page)).into_iter().find(|e| e.handle == route).expect("the route is drawn")
}

fn net_potential(h: &mut H, c: ConductorId) -> String {
    s(&h.run("net", json!({"conductor": c.hex()})).unwrap()["potential"])
}

fn wirelist(h: &mut H) -> Vec<Value> {
    h.run("wirelist", json!({"limit": 5000})).unwrap()["rows"].as_array().cloned().unwrap_or_default()
}

/// The wire report row of `c` (rows are in the order of `o.wires()`; the row carries no ID, so it
/// is found by its wire number).
fn wire_row(h: &mut H, c: ConductorId) -> Value {
    let number = s(&h.run("net", json!({"conductor": c.hex()})).unwrap()["number"]);
    wirelist(h).into_iter().find(|r| s(&r["wire_number"]) == number).unwrap_or_else(|| panic!("no wire row {number}"))
}

fn bom(h: &mut H) -> Vec<Value> {
    h.run("bom", json!({"limit": 5000})).unwrap()["rows"].as_array().cloned().unwrap_or_default()
}

fn look_category(h: &mut H, c: ConductorId) -> Option<String> {
    ecad_cadcraft::styles::wire::wire_look(&h.doc(), c).category.map(|r| r.to_string())
}

// ------------------------------------------------------------------ b11: the built-ins

/// §12.11, §5.2: the ten categories; a 24V wire is labelled `24V-01 0.5 BU`, drawn blue, and its
/// wire-report row shows Category "24 V DC".
#[test]
#[ignore = "tester 050: awaiting build"]
fn b11_ten_built_in_categories_and_a_24v_wire_is_dc24() {
    let mut h = H::new(0x50b_1101);
    let rows = styles(&mut h, "wire");
    assert_eq!(rows.len(), 10, "{rows:?}");
    for (id, name, pot, colour, xs, line, weight) in BUILTIN {
        let r = rows.iter().find(|r| s(&r["id"]) == id).unwrap_or_else(|| panic!("category {id}"));
        assert_eq!((s(&r["name"]), s(&r["layer"])), (name.to_string(), "builtin".to_string()), "{id}");
        assert_eq!(field_text(r, "potential"), pot, "{id}");
        assert_eq!(field_text(r, "colour"), colour, "{id}");
        assert_eq!(field_text(r, "cross_section"), xs, "{id}");
        assert_eq!(field_text(r, "line_colour"), line, "{id}");
        assert!((field_num(r, "line_weight") - weight).abs() < 1e-9, "{id}");
        assert_eq!(field_text(r, "line_type"), "continuous", "{id}");
        assert_eq!(field_text(r, "label_rule"), "project", "{id}");
        assert_eq!(num(&field(r, "allowance").0), Some(0.0), "{id}: built-in allowance 0 (question 5)");
        assert_eq!(field_text(r, "part_number"), "", "{id}");
    }
    let e = ecad_cadcraft::defaults::find("wire.category").expect("the choice entry");
    use ecad_cadcraft::defaults::Level;
    assert!(e.allows(Level::Standard) && e.allows(Level::Project) && e.allows(Level::Object) && !e.allows(Level::Page), "S P O");

    let page = h.first_page();
    let w = wire_at(&mut h, 50.0, Some("24V"));
    assert_eq!(label(&mut h, page, w).as_deref(), Some("24V-01 0.5 BU"));
    assert_eq!(route_colour(&mut h, page, w), Some(code_colour("BU")), "drawn blue (line_colour wire)");
    assert_eq!(look_category(&mut h, w).as_deref(), Some("dc24"), "from potential 24V");
    let row = wire_row(&mut h, w);
    assert_eq!(s(&row["category"]), "24 V DC", "{row}");
    let n = h.run("nets", json!({})).unwrap().to_string();
    assert!(n.contains("dc24") || n.contains("24 V DC"), "nets carries the category: {n:.400}");
    // No potential: no category, today's look (ink, 0.25, continuous).
    let plain = wire_at(&mut h, 80.0, None);
    assert_eq!(look_category(&mut h, plain), None);
    assert_eq!(route_colour(&mut h, page, plain), None, "ink");
    assert_eq!(route_entity(&mut h, page, plain).common.lineweight, Lineweight::Mm100(25));
}

/// §5.2, AC 2: power categories draw in ink at 0.6 mm; PE keeps its GNYE look.
#[test]
#[ignore = "tester 050: awaiting build"]
fn b11_power_is_ink_at_0_6_and_pe_is_green_yellow() {
    let mut h = H::new(0x50b_1102);
    let page = h.first_page();
    let l1 = wire_at(&mut h, 20.0, Some("L1"));
    let pe = wire_at(&mut h, 40.0, Some("PE"));
    assert_eq!(route_colour(&mut h, page, l1), None, "power L1 in ink");
    assert_eq!(route_entity(&mut h, page, l1).common.lineweight, Lineweight::Mm100(60));
    assert_eq!(label(&mut h, page, l1).map(|t| t.ends_with("2.5 BN")), Some(true), "the label keeps the insulation colour");
    assert_eq!(route_colour(&mut h, page, pe), Some(code_colour("GNYE")));
    assert_eq!(route_entity(&mut h, page, pe).common.lineweight, Lineweight::Mm100(25));
}

// ------------------------------------------------- b12: a category change, overrides

/// §12.12, AC 3: changing `dc24`'s colour turns every 24V wire's label and line red, except a
/// conductor with its own `colour`; Reset (clearing it) makes it follow the category again.
#[test]
#[ignore = "tester 050: awaiting build"]
fn b12_a_category_colour_reaches_every_user_but_an_overridden_one() {
    let mut h = H::new(0x50b_1201);
    let page = h.first_page();
    let a = wire_at(&mut h, 20.0, Some("24V"));
    let b = wire_at(&mut h, 40.0, Some("24V"));
    let own = wire_at(&mut h, 60.0, Some("24V"));
    h.run("setprop", json!({"conductor": own.hex(), "key": "colour", "value": "VT"})).unwrap();
    let (_, tx) = one_tx(&mut h, "style", json!({"catalogue": "wire", "id": "dc24", "fields": {"colour": "RD"}}));
    assert_eq!(op_names(&tx), ["SetStyle"], "a category is a style record, no new op");
    for w in [a, b] {
        assert!(label(&mut h, page, w).is_some_and(|t| t.ends_with(" RD")), "{:?}", label(&mut h, page, w));
        assert_eq!(route_colour(&mut h, page, w), Some(code_colour("RD")));
    }
    assert!(label(&mut h, page, own).is_some_and(|t| t.ends_with(" VT")), "own colour kept");
    assert_eq!(route_colour(&mut h, page, own), Some(code_colour("VT")));
    let row = wire_row(&mut h, own);
    assert!(row.to_string().contains("VT"), "{row}");
    // Reset: the conductor follows the category.
    h.run("setprop", json!({"conductor": own.hex(), "key": "colour", "value": null})).unwrap();
    assert!(label(&mut h, page, own).is_some_and(|t| t.ends_with(" RD")));
    undo(&mut h);
    undo(&mut h);
    assert!(label(&mut h, page, a).is_some_and(|t| t.ends_with(" BU")), "undo is one step");
}

/// §5.6: the wire report shows the resolved colour and cross-section, the category name, and a
/// marker on an overridden cell; 024's "No colour…" wording now names the category.
#[test]
#[ignore = "tester 050: awaiting build"]
fn b12_the_wire_report_shows_category_and_resolved_values() {
    let mut h = H::new(0x50b_1202);
    let w = wire_at(&mut h, 20.0, Some("24V"));
    let r = wire_row(&mut h, w);
    assert_eq!((s(&r["colour"]), s(&r["cross_section"]), s(&r["category"])), ("BU".into(), "0.5".into(), "24 V DC".into()), "{r}");
    h.run("setprop", json!({"conductor": w.hex(), "key": "cross_section", "value": "1.5"})).unwrap();
    let r = wire_row(&mut h, w);
    assert!(s(&r["cross_section"]).contains("1.5"), "{r}");
    let plain = wire_at(&mut h, 60.0, None);
    let r = wire_row(&mut h, plain);
    assert_eq!(s(&r["category"]), "", "no category: empty");
}

// ------------------------------------------ b13: categories, potentials and conductors

/// §12.13, §5.4, question 4: a category on one conductor of a 24V net changes only it and the
/// net keeps 24V; on a wire with no potential, `dc24` also sets potential 24V in one transaction;
/// on a net with another potential, the potential is never changed and the result says so.
#[test]
#[ignore = "tester 050: awaiting build"]
fn b13_category_on_a_conductor_and_the_potential_rule() {
    let mut h = H::new(0x50b_1301);
    let a = wire_at(&mut h, 20.0, Some("24V"));
    let b: ConductorId = id(&h.run("wire", json!({"points": pts(&[(60.0, 20.0), (100.0, 20.0)])})).unwrap(), "conductor");
    assert_eq!(net_potential(&mut h, b), "24V", "joined into the 24V net");
    let (_, tx) = one_tx(&mut h, "category", json!({"conductor": a.hex(), "value": "signal"}));
    assert!(
        matches!(tx.ops.as_slice(), [Op::SetProperty { object: ObjectRef::Conductor(c), key, .. }] if *c == a && key == "wire.category"),
        "{:?}",
        tx.ops
    );
    assert_eq!(look_category(&mut h, a).as_deref(), Some("signal"));
    assert_eq!(look_category(&mut h, b).as_deref(), Some("dc24"), "only that conductor");
    assert_eq!(net_potential(&mut h, a), "24V", "the net keeps 24V");
    // No potential: the category writes it, one transaction.
    let c = wire_at(&mut h, 80.0, None);
    let (_, tx) = one_tx(&mut h, "category", json!({"conductor": c.hex(), "value": "24 V DC"}));
    assert!(tx.ops.len() >= 2, "category and potential together: {:?}", tx.ops);
    assert_eq!(net_potential(&mut h, c), "24V");
    assert_eq!(look_category(&mut h, c).as_deref(), Some("dc24"));
    undo(&mut h);
    assert_eq!(net_potential(&mut h, c), "", "one undo takes both back");
    // Another potential is never changed.
    let l = wire_at(&mut h, 120.0, Some("L"));
    let (v, _) = one_tx(&mut h, "category", json!({"conductor": l.hex(), "value": "dc24"}));
    assert_eq!(net_potential(&mut h, l), "L");
    assert!(v.to_string().contains("keeps potential L"), "{v}");
    // `net` form: every conductor of the net; `conductors` form; null clears.
    one_tx(&mut h, "category", json!({"net": a.hex(), "value": "analogue"}));
    assert_eq!((look_category(&mut h, a).as_deref(), look_category(&mut h, b).as_deref()), (Some("analogue"), Some("analogue")));
    one_tx(&mut h, "category", json!({"conductors": [a.hex(), b.hex()], "value": null}));
    assert_eq!((look_category(&mut h, a).as_deref(), look_category(&mut h, b).as_deref()), (Some("dc24"), Some("dc24")), "cleared: from potential");
}

/// AC 4, §5.4: a potential change on a net moves its unassigned conductors to the new potential's
/// category, and leaves conductors with their own category alone.
#[test]
#[ignore = "tester 050: awaiting build"]
fn b13_a_potential_change_moves_only_unassigned_conductors() {
    let mut h = H::new(0x50b_1302);
    let page = h.first_page();
    let a = wire_at(&mut h, 20.0, Some("24V"));
    let b: ConductorId = id(&h.run("wire", json!({"points": pts(&[(60.0, 20.0), (100.0, 20.0)])})).unwrap(), "conductor");
    h.run("category", json!({"conductor": b.hex(), "value": "signal"})).unwrap();
    h.run("potential", json!({"conductor": a.hex(), "value": "0V"})).unwrap();
    assert_eq!(look_category(&mut h, a).as_deref(), Some("dc0"));
    assert_eq!(look_category(&mut h, b).as_deref(), Some("signal"), "own category kept");
    assert!(label(&mut h, page, a).is_some_and(|t| t.starts_with("0V-")));
}

/// §6.9: `wire {category}` gives the new conductor its category in the same transaction; the
/// prompt offers `[Potential/Category]`; a typed `C` asks for the category.
#[test]
#[ignore = "tester 050: awaiting build"]
fn b13_wire_with_a_category_and_the_prompt_keyword() {
    let mut h = H::new(0x50b_1303);
    let (v, _) = one_tx(&mut h, "wire", json!({"points": pts(&[(0.0, 20.0), (60.0, 20.0)]), "category": "dc24"}));
    let w: ConductorId = id(&v, "conductor");
    assert_eq!(h.doc().conductors[&w].properties.get("wire.category").map(String::as_str), Some("dc24"));
    assert_eq!(net_potential(&mut h, w), "24V", "no potential before: the category's is written");
    refused(&mut h, "wire", json!({"points": pts(&[(0.0, 40.0), (60.0, 40.0)]), "category": "nope"}));
    // The typed command: the prompt names the keyword.
    h.s.cmdline("wire").unwrap();
    let prompt = h.s.log.iter().rev().take(3).cloned().collect::<Vec<_>>().join(" | ");
    assert!(prompt.contains("Potential/Category"), "{prompt}");
    h.s.cmdline("c").unwrap();
    let prompt = h.s.log.iter().rev().take(3).cloned().collect::<Vec<_>>().join(" | ");
    assert!(prompt.contains("Category for new wires") && prompt.contains("auto"), "{prompt}");
    h.s.cmdline("signal").unwrap();
    h.cmdline("0,80");
    h.cmdline("60,80");
    h.cmdline("");
    let d = h.doc();
    let last = d.conductors.iter().find(|(_, c)| c.properties.get("wire.category").map(String::as_str) == Some("signal"));
    assert!(last.is_some(), "the typed wire took the category");
}

// ------------------------------------------------------ b14: one default per potential

/// §12.14: a second category for a potential is refused when written; a file with two warns
/// `category-potential-twice` and the first by ID wins.
#[test]
#[ignore = "tester 050: awaiting build"]
fn b14_two_defaults_for_one_potential() {
    let mut h = H::new(0x50b_1401);
    let m = refused(&mut h, "style", json!({"catalogue": "wire", "id": "pe", "fields": {"potential": "24V"}}));
    assert!(m.contains("already the default for 24V"), "{m}");
    refused(&mut h, "style", json!({"catalogue": "wire", "new": true, "name": "Other 24", "copy_of": "dc24"}));
    // A potential not in the table is allowed (§5.4).
    h.run(
        "style",
        json!({"catalogue": "wire", "new": true, "name": "48 V AC", "copy_of": "signal", "fields": {"potential": "48V", "line_colour": "RD"}}),
    )
    .unwrap();
    // Hostile: a file holding a second category for 24V. Built from a real new-style record
    // (`before` empty), taken back, then injected with the potential changed.
    h.run("style", json!({"catalogue": "wire", "new": true, "name": "Second 24", "copy_of": "signal", "fields": {"potential": "47V"}})).unwrap();
    let mut tx = last_tx_json(&mut h);
    undo(&mut h);
    let rec = find_obj_mut(&mut tx, "fields").and_then(|m| m.get_mut("fields")).and_then(Value::as_object_mut).expect("the record's fields");
    rec.insert("potential".into(), json!("24V"));
    let ops = decoded_ops(&tx);
    inject(&mut h, ops);
    let w = wire_at(&mut h, 20.0, Some("24V"));
    let rows = checks_of(&mut h, "category-potential-twice");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].to_string().contains("24V") && rows[0].to_string().to_lowercase().contains("warn"), "{}", rows[0]);
    // The first by ID: `dc24` (a word) against `#…` — whichever sorts first is used, steadily.
    let first = look_category(&mut h, w);
    reproject_twice(&mut h);
    assert_eq!(look_category(&mut h, w), first, "the same one every time");
}

fn reproject_twice(h: &mut H) {
    super::lane_050a_tester::reproject(h);
    super::lane_050a_tester::reproject(h);
}

// ------------------------------------------------------------------------ b15: the BOM

/// §12.15, §5.6, question 5: a category with a part number and an allowance gives one `m` row,
/// Qty = conductors × allowance rounded up to 0.5, noted "estimate"; with allowance 0 the row has
/// no Qty and the BOM check fires. A conductor with its own colour joins its effective row.
#[test]
#[ignore = "tester 050: awaiting build"]
fn b15_bom_wire_rows_and_the_allowance() {
    let mut h = H::new(0x50b_1501);
    for y in [20.0, 40.0, 60.0] {
        wire_at(&mut h, y, Some("24V"));
    }
    assert!(bom(&mut h).iter().all(|r| s(&r["unit"]) != "m"), "no part number: no wire row");
    h.run("style", json!({"catalogue": "wire", "id": "dc24", "fields": {"part_number": "W-05-BU", "allowance": 1.4}})).unwrap();
    let rows: Vec<Value> = bom(&mut h).into_iter().filter(|r| s(&r["part_number"]) == "W-05-BU").collect();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(s(&rows[0]["unit"]), "m");
    assert_eq!(num(&rows[0]["qty"]), Some(4.5), "3 × 1.4 = 4.2, rounded up to 4.5: {}", rows[0]);
    assert!(rows[0].to_string().contains("estimate"), "{}", rows[0]);
    h.run("style", json!({"catalogue": "wire", "id": "dc24", "fields": {"allowance": 1.5}})).unwrap();
    let r = bom(&mut h).into_iter().find(|r| s(&r["part_number"]) == "W-05-BU").unwrap();
    assert_eq!(num(&r["qty"]), Some(4.5));
    // An own colour: its effective row.
    let red = wire_at(&mut h, 80.0, Some("24V"));
    h.run("setprop", json!({"conductor": red.hex(), "key": "colour", "value": "RD"})).unwrap();
    let parts: Vec<Value> = bom(&mut h).into_iter().filter(|r| s(&r["part_number"]) == "W-05-BU").collect();
    assert_eq!(parts.len(), 2, "one row per (part, colour, cross-section): {parts:?}");
    // Allowance 0: no Qty, the count in the note, and the check.
    h.run("style", json!({"catalogue": "wire", "id": "dc24", "fields": {"allowance": 0}})).unwrap();
    let r = bom(&mut h).into_iter().find(|r| s(&r["part_number"]) == "W-05-BU").unwrap();
    assert_eq!(s(&r["qty"]), "", "{r}");
    let all = checks_all(&mut h).iter().map(Value::to_string).collect::<String>();
    assert!(all.contains("Wire length not set for 24 V DC"), "{all:.800}");
    // Hostile allowances are refused.
    for bad in [json!(-1), json!(101), json!("NaN"), json!(1e308)] {
        refused(&mut h, "style", json!({"catalogue": "wire", "id": "dc24", "fields": {"allowance": bad}}));
    }
}

// --------------------------------------------------------- b16: the potentials table

/// §12.16, FP6: `wiresettings` potentials no longer take a colour or cross-section; the read
/// shows none.
#[test]
#[ignore = "tester 050: awaiting build"]
fn b16_wiresettings_refuses_potential_colours() {
    let mut h = H::new(0x50b_1601);
    for k in ["colour", "cross_section"] {
        let mut row = json!({"name": "24V", "prefix": "24V-", "numbered": true});
        row[k] = json!("RD");
        let m = refused(&mut h, "wiresettings", json!({"potentials": [row]}));
        assert!(m.contains("set on wire categories now"), "{m}");
    }
    let r = h.run("wiresettings", json!({})).unwrap();
    let rows = r["potentials"].as_array().cloned().unwrap_or_default();
    assert!(!rows.is_empty() && rows.iter().all(|p| p.get("colour").is_none() && p.get("cross_section").is_none()), "{r}");
    one_tx(&mut h, "wiresettings", json!({"potentials": [{"name": "24V", "prefix": "24V-", "numbered": true}]}));
}

// ------------------------------------------------------------- b17: mono and colour print

/// §12.17, §5.5: mono print draws every wire in ink with no dashes but keeps line type and weight;
/// colour print draws line colours, with `line_colour ink` categories in ink.
#[test]
#[ignore = "tester 050: awaiting build"]
fn b17_mono_prints_ink_and_colour_prints_line_colours() {
    let mut h = H::new(0x50b_1701);
    let page = h.first_page();
    let dc = wire_at(&mut h, 20.0, Some("24V"));
    let pw = wire_at(&mut h, 40.0, Some("L1"));
    let pe = wire_at(&mut h, 60.0, Some("PE"));
    h.run("style", json!({"catalogue": "wire", "id": "dc24", "fields": {"line_type": "dashed"}})).unwrap();
    let ac = own_id(
        &h.run(
            "style",
            json!({"catalogue": "wire", "new": true, "name": "48 V AC", "copy_of": "signal", "fields": {"potential": "48V", "line_colour": "RD"}}),
        )
        .unwrap(),
    );
    let red = wire_at(&mut h, 80.0, Some("48V"));
    assert_eq!(look_category(&mut h, red).as_deref(), Some(ac.as_str()));
    let _ = (dc, pw, pe);
    let d = h.doc();
    let fan = Default::default();
    let (mono, _) = printbook::build_pending(&d, &[page], &fan, 0.01, None, false).unwrap();
    assert!(mono.model.iter().all(|e| e.common.layer != wires::DASH_LAYER), "no dashes in mono");
    let lines = |m: &cadcraft_engine::doc::Drawing| {
        m.model
            .iter()
            .filter(|e| matches!(e.kind, EntityKind::LwPolyline(_) | EntityKind::Line(_)) && e.common.layer != ecad_cadcraft::frame::LAYER)
            .cloned()
            .collect::<Vec<_>>()
    };
    let wired = lines(&mono);
    for c in [code_colour("BU"), code_colour("RD"), code_colour("GNYE")] {
        assert!(wired.iter().all(|e| e.common.color != c), "mono: every wire in ink");
    }
    assert!(wired.iter().any(|e| e.common.lineweight == Lineweight::Mm100(60)), "mono keeps the power weight");
    assert!(wired.iter().any(|e| e.common.linetype.eq_ignore_ascii_case("DASHED")), "mono keeps the line type");
    let (colour, _) = printbook::build_in(&d, &[page], &fan, 0.01, None, false, true).unwrap();
    let cols: Vec<Color> = lines(&colour).iter().map(|e| e.common.color).collect();
    assert!(cols.contains(&code_colour("RD")), "48 V AC's line colour RD prints");
    assert!(cols.contains(&code_colour("BU")), "24 V in its insulation colour");
}

// -------------------------------------------------------------- b18: the file format

/// §12.18, FP6 and the overseer's ruling: a potentials table carrying colours (an older file)
/// reads, the colours are ignored, and Checks shows one `ignored-old-setting` row for potentials,
/// which a save keeps and a `wiresettings` write clears.
#[test]
#[ignore = "tester 050: awaiting build"]
fn b18_old_potential_colours_are_ignored_and_reported() {
    let mut h = H::new(0x50b_1801);
    let page = h.first_page();
    let w = wire_at(&mut h, 20.0, Some("24V"));
    // Build the older file's settings record from a real one, with colours on two rows.
    h.run("wiresettings", json!({"digits": 3})).unwrap();
    let mut tx = last_tx_json(&mut h);
    let pots =
        find_obj_mut(&mut tx, "potentials").and_then(|m| m.get_mut("potentials")).and_then(Value::as_array_mut).expect("potentials in the record");
    let n = pots.len();
    for p in pots.iter_mut().filter(|p| p["name"] == json!("24V") || p["name"] == json!("0V")) {
        p["colour"] = json!("RD");
        p["cross_section"] = json!("1.5");
    }
    assert!(n >= 8);
    undo(&mut h);
    let ops = decoded_ops(&tx);
    inject(&mut h, ops);
    assert!(label(&mut h, page, w).is_some_and(|t| t.ends_with("0.5 BU")), "the old colours are not read: {:?}", label(&mut h, page, w));
    let rows = checks_of(&mut h, "ignored-old-setting");
    assert_eq!(rows.len(), 1, "one row for the potential kind: {rows:?}");
    assert!(rows[0].to_string().contains("Potential colours and cross-sections set in an older build"), "{}", rows[0]);
    let dir = tmp_dir("fp6");
    let mut h = save_and_reopen(h, &dir, "old potentials.ecad", 0x50b_1802);
    assert_eq!(checks_of(&mut h, "ignored-old-setting").len(), 1, "a save writes them back unchanged");
    let (v, _) = one_tx(
        &mut h,
        "wiresettings",
        json!({"potentials": [{"name": "24V", "prefix": "24V-", "numbered": true}, {"name": "0V", "prefix": "0V-", "numbered": true}]}),
    );
    assert!(checks_of(&mut h, "ignored-old-setting").is_empty(), "writing the table clears them: {v}");
    let _ = std::fs::remove_dir_all(dir);
}

/// §12.18: save, reopen, replay, records and undo across `wire.category` and category records.
#[test]
#[ignore = "tester 050: awaiting build"]
fn b18_category_records_and_choices_round_trip() {
    let mut h = H::new(0x50b_1803);
    let a = wire_at(&mut h, 20.0, Some("24V"));
    h.run("style", json!({"catalogue": "wire", "id": "dc24", "fields": {"colour": "RD", "part_number": "W-05", "allowance": 2}})).unwrap();
    let ac = own_id(
        &h.run("style", json!({"catalogue": "wire", "new": true, "name": "48 V AC", "copy_of": "signal", "fields": {"potential": "48V"}})).unwrap(),
    );
    h.run("category", json!({"conductor": a.hex(), "value": ac})).unwrap();
    h.run("setdefault", json!({"key": "wire.category", "level": "project", "value": "signal"})).unwrap();
    h.run("style", json!({"catalogue": "wire", "id": "pe", "fields": {"line_weight": 0.5}})).unwrap();
    let trail = h.trail.clone();
    for i in (0..trail.len() - 1).rev() {
        undo(&mut h);
        assert!(h.doc() == trail[i], "undo {i}");
    }
    for (i, want) in trail.iter().enumerate().skip(1) {
        redo(&mut h);
        assert!(h.doc() == *want, "redo {i}");
    }
    assert!(inverses_restore(&mut h) >= 4);
    records_round_trip(&mut h);
    let want = h.doc();
    let rows = styles(&mut h, "wire");
    let dir = tmp_dir("cat");
    let mut h = save_and_reopen(h, &dir, "Kategorien (1).ecad", 0x50b_1804);
    assert!(h.doc() == want);
    assert_eq!(styles(&mut h, "wire"), rows);
    assert_eq!(look_category(&mut h, a), Some(ac));
    let _ = std::fs::remove_dir_all(dir);
}

// ------------------------------------------------------------- labels, checks, hostile

/// §5.5: `label_rule none` draws no label; number, cross-section and colour stay in reports.
#[test]
#[ignore = "tester 050: awaiting build"]
fn b_label_rule_none_draws_no_label() {
    let mut h = H::new(0x50b_1901);
    let page = h.first_page();
    let w = wire_at(&mut h, 20.0, Some("24V"));
    h.run("style", json!({"catalogue": "wire", "id": "dc24", "fields": {"label_rule": "none"}})).unwrap();
    assert_eq!(label(&mut h, page, w), None);
    let r = wire_row(&mut h, w);
    assert_eq!(s(&r["colour"]), "BU", "{r}");
}

/// §5.7: a `wire.category` naming no category draws without one and Checks says
/// `category-unknown` (Info) with a go-to the conductor.
#[test]
#[ignore = "tester 050: awaiting build"]
fn b_an_unknown_category_draws_without_one_and_is_reported() {
    let mut h = H::new(0x50b_1902);
    let page = h.first_page();
    let w = wire_at(&mut h, 20.0, None);
    inject(
        &mut h,
        vec![Op::SetProperty {
            object: ObjectRef::Conductor(w),
            key: "wire.category".into(),
            before: None,
            after: Some(format!("#{}", "d".repeat(32))),
        }],
    );
    assert_eq!(look_category(&mut h, w), None);
    assert_eq!(route_colour(&mut h, page, w), None);
    let rows = checks_of(&mut h, "category-unknown");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].to_string().contains("Wire category not found") && rows[0].to_string().contains(&w.hex()), "{}", rows[0]);
}

/// Hostile input never panics: category values, field values and a potential of 10,000
/// characters, control characters, NaN and huge numbers; all refused or ignored.
#[test]
#[ignore = "tester 050: awaiting build"]
fn b_hostile_category_input_never_panics() {
    let mut h = H::new(0x50b_1903);
    let w = wire_at(&mut h, 20.0, Some("24V"));
    let long = "x".repeat(10_000);
    for (cmd, params) in [
        ("category", json!({"conductor": w.hex(), "value": long})),
        ("category", json!({"conductor": w.hex(), "value": "dc\u{0}24"})),
        ("category", json!({"conductor": "zz", "value": "dc24"})),
        ("category", json!({"conductors": (0..10_000).map(|_| w.hex()).collect::<Vec<_>>(), "value": "nope"})),
        ("category", json!({"value": "dc24"})),
        ("style", json!({"catalogue": "wire", "id": "dc24", "fields": {"colour": long}})),
        ("style", json!({"catalogue": "wire", "id": "dc24", "fields": {"line_weight": "NaN"}})),
        ("style", json!({"catalogue": "wire", "id": "dc24", "fields": {"line_weight": 1e308}})),
        ("style", json!({"catalogue": "wire", "id": "dc24", "fields": {"line_colour": "PURPLISH"}})),
        ("style", json!({"catalogue": "wire", "id": "dc24", "fields": {"potential": "x".repeat(25)}})),
        ("style", json!({"catalogue": "wire", "id": "dc24", "fields": {"label_rule": "sometimes"}})),
        ("wire", json!({"points": pts(&[(0.0, 90.0), (10.0, 90.0)]), "category": long})),
    ] {
        refused(&mut h, cmd, params);
    }
    // Hostile stored values fall through.
    for v in ["", "\u{7}", "box", "#zz", long.as_str()] {
        let before = h.doc().conductors[&w].properties.get("wire.category").cloned();
        inject(&mut h, vec![Op::SetProperty { object: ObjectRef::Conductor(w), key: "wire.category".into(), before, after: Some(v.to_string()) }]);
        assert_eq!(look_category(&mut h, w).as_deref(), Some("dc24"), "{v:.20}: falls to the potential's category");
    }
    let _ = rows_of(h.run("styles", json!({"catalogue": "wire", "users": true})).unwrap());
}

/// §2.5, §7.2: `styles {users}` for a category counts explicit users (own `wire.category`) and
/// implicit ones (by potential).
#[test]
#[ignore = "tester 050: awaiting build"]
fn b_users_of_a_category() {
    let mut h = H::new(0x50b_1904);
    wire_at(&mut h, 20.0, Some("24V"));
    wire_at(&mut h, 40.0, Some("24V"));
    let c = wire_at(&mut h, 60.0, None);
    h.run("category", json!({"conductor": c.hex(), "value": "dc24"})).unwrap();
    let rows = rows_of(h.run("styles", json!({"catalogue": "wire", "users": true})).unwrap());
    let r = rows.iter().find(|r| s(&r["id"]) == "dc24").unwrap();
    assert_eq!((r["users"]["explicit"].as_u64(), r["users"]["implicit"].as_u64()), (Some(1), Some(2)), "{r}");
    let _ = style_row(&mut h, "wire", "dc24");
}
