//! Tester 059a (brief 059, design 059 §2–§9, §11; the owner's rules 1–8 of the tester brief): the
//! cable manager's model, commands and checks, run through the JSON forms and the command line.
//!
//! Cable types, makers and part numbers here are synthetic (made-up names and values).
//!
//! - r1: `cablenew` makes a cable not drawn, the next free -W, editable; type, cores, length, maker
//!   and part come from the cable type and are overridden per cable.
//! - r2: `cableremove` refuses a drawn cable with its pages (the Go to); removes one not drawn in
//!   one transaction; undo and redo are exact.
//! - r3: reducing cores below those assigned is refused, naming the cores in use.
//! - r4: each end's plan is checked against that same end only; ends differing is normal.
//! - r5: not drawn is Info, Warning in the release check.
//! - r6: the assign rule (crossing order, core labels, no `core-open` while not drawn, "not
//!   assigned").
//! - r7: typed params and bounded words, hostile and oversized input (JSON forms and the command
//!   line; MCP in `ecad-mcp`'s `lane_059_tester`).
//! - r8: atomic, exact undo and redo, replay; update maps touch only that cable.

use ecad_cadcraft::cable_cmds::{self, CableField, Side, Source};
use ecad_cadcraft::checks::{self, Severity};
use ecad_cadcraft::designations::Aspect;
use ecad_model::{CableId, DeviceId, Document, Op, WireId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

// ---------------------------------------------------------------- helpers

fn checks_of(h: &mut H, kind: &str) -> Vec<Value> {
    h.run("checks", json!({"kinds": [kind], "limit": 2000})).unwrap_or_else(|e| panic!("checks {kind}: {e}"))["checks"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn text(v: &Value) -> String {
    v["text"].as_str().unwrap_or_default().to_string()
}

fn last_tx(h: &mut H) -> (String, Vec<Op>) {
    h.p(|p| p.ledger.log().last().map(|t| (t.label.clone(), t.ops.clone())).unwrap_or_default())
}

/// `n` vertical wires at x = 0, 10, 20…, from y 0 to 50, between connection points of one device.
fn wires(h: &mut H, n: usize) -> Vec<WireId> {
    let ends: Vec<(f64, f64)> = (0..n).flat_map(|i| [(10.0 * i as f64, 0.0), (10.0 * i as f64, 50.0)]).collect();
    h.points_at(&ends);
    (0..n)
        .map(|i| {
            let x = 10.0 * i as f64;
            id(&h.run("wire", json!({"points": pts(&[(x, 0.0), (x, 50.0)])})).unwrap(), "conductor")
        })
        .collect()
}

fn new_cable(h: &mut H, p: Value) -> CableId {
    id(&h.run("cablenew", p).unwrap_or_else(|e| panic!("cablenew: {e}")), "cable")
}

fn slash_across(h: &mut H, from: (f64, f64), to: (f64, f64), cable: Option<CableId>) -> Result<Value, String> {
    let mut p = json!({"from": [from.0, from.1], "to": [to.0, to.1]});
    if let Some(k) = cable {
        p["cable"] = json!(k.hex());
    }
    h.run("cableslash", p)
}

/// A refusal that writes nothing: the error, with head and document unchanged.
fn refused(h: &mut H, cmd: &str, params: Value) -> String {
    let head = h.head();
    let before = h.doc();
    let e = match h.run(cmd, params.clone()) {
        Ok(v) => panic!("{cmd} {:.200} should be refused: {v}", params.to_string()),
        Err(e) => e,
    };
    assert_eq!(h.head(), head, "{cmd} {:.200}: nothing recorded", params.to_string());
    assert!(h.doc() == before, "{cmd} {:.200}: document unchanged", params.to_string());
    e
}

/// The words are plain and bounded: short, one paragraph, no echo of a huge input.
fn bounded(e: &str, what: &str) {
    if let Some(p) = unbounded(e, what) {
        panic!("{p}");
    }
}

/// Why the words are not plain and bounded, if they are not.
fn unbounded(e: &str, what: &str) -> Option<String> {
    if e.trim().is_empty() {
        return Some(format!("{what}: empty words"));
    }
    if e.chars().count() > 400 {
        return Some(format!("{what}: {} chars of words: {:.120}", e.chars().count(), e));
    }
    if e.contains("panicked") || e.contains("RUST_BACKTRACE") {
        return Some(format!("{what}: {e}"));
    }
    None
}

/// Two placed devices at (0, 100) and (0, -100), in `+CAB1` and `+JB1`, joined by one wire; returns
/// (a, b, the slash's y and x).
fn two_devices(h: &mut H, loc_a: &str, loc_b: &str) -> (DeviceId, DeviceId, f64, f64) {
    let a = id::<DeviceId>(&h.run("place", json!({"at": [0, 100], "rotation": 90})).unwrap(), "component");
    let b = id::<DeviceId>(&h.run("place", json!({"at": [0, -100], "rotation": 90})).unwrap(), "component");
    h.run("setlocation", json!({"component": a.hex(), "value": loc_a})).unwrap();
    h.run("setlocation", json!({"component": b.hex(), "value": loc_b})).unwrap();
    let doc = h.doc();
    let page = h.first_page();
    let tp = ecad_model::derive::terminal_positions(&doc, page);
    let pos = |c: DeviceId| {
        let (rid, _) = doc.representations_of(ecad_model::Subject::Device(c)).next().unwrap();
        tp.iter().find(|(_, r, _)| *r == rid).unwrap().2
    };
    let (pa, pb) = (pos(a), pos(b));
    h.run("wire", json!({"points": pts(&[(pa.x, pa.y), (pa.x, 0.0), (pb.x, 0.0), (pb.x, pb.y)])})).unwrap();
    (a, b, pa.y / 2.0, pa.x)
}

fn synth_type(h: &mut H, name: &str, fields: Value) -> String {
    let v = h.run("setstyle", json!({"catalogue": "cable", "new": true, "name": name, "fields": fields})).unwrap();
    v["id"].as_str().unwrap().to_string()
}

fn tag_of(doc: &Document, k: CableId) -> Option<String> {
    doc.cables.get(&k).and_then(|c| c.properties.get("tag").cloned())
}

// ---------------------------------------------------------------- r1: cablenew

#[test]
fn r1_cablenew_takes_the_next_free_w_after_a_drawn_cable_and_the_tag_is_editable() {
    let mut h = H::new(59_101);
    wires(&mut h, 2);
    let w1: CableId = id(&slash_across(&mut h, (-5.0, 25.0), (15.0, 25.0), None).unwrap(), "cable");
    assert_eq!(tag_of(&h.doc(), w1).as_deref(), Some("-W01"));
    let r = h.run("cablenew", json!({})).unwrap();
    let k: CableId = id(&r, "cable");
    assert_eq!(r["tag"], json!("-W02"), "{r}");
    assert!(!cable_cmds::is_drawn(&h.doc(), k));
    assert!(cable_cmds::not_drawn(&h.doc()).contains(&k));
    // Editable: a typed tag is stored and fixed.
    h.run("cableedit", json!({"cable": "-W02", "set": {"tag": "-W07"}})).unwrap();
    let doc = h.doc();
    assert_eq!(tag_of(&doc, k).as_deref(), Some("-W07"));
    // The next free -W after -W07.
    let r = h.run("cablenew", json!({})).unwrap();
    let t = r["tag"].as_str().unwrap();
    assert!(t != "-W01" && t != "-W07", "a free tag: {t}");
    // A tag a cable holds is refused, by edit and by new.
    let e = refused(&mut h, "cableedit", json!({"cable": "-W07", "set": {"tag": "-W01"}}));
    assert!(e.contains("-W01") && e.contains("(cable)"), "{e}");
    let e = refused(&mut h, "cablenew", json!({"tag": "-W07"}));
    assert!(e.contains("already"), "{e}");
}

#[test]
fn r1_type_cores_length_maker_and_part_come_from_the_type_and_are_overridden_per_cable() {
    let mut h = H::new(59_102);
    let ty = synth_type(
        &mut h,
        "Synth Flex",
        json!({"cores": "4G", "size": "1.5", "maker": "Synthmaker", "part_number": "SYN-0004", "description": "Synthetic four core", "shield": "overall"}),
    );
    let k = new_cable(&mut h, json!({"cable_type": ty, "length": 12.5}));
    let doc = h.doc();
    let props = &doc.cables[&k].properties;
    assert!(props.get("type").is_some(), "the type's notation fills `type`: {props:?}");
    assert_eq!(cable_cmds::cores_shown(&doc, k).map(|c| (c.0, c.1)), Some(("4G".to_string(), Source::Type)));
    assert_eq!(cable_cmds::part_value(&doc, k, CableField::Maker), Some(("Synthmaker".into(), Source::Type)));
    assert_eq!(cable_cmds::part_value(&doc, k, CableField::PartNumber), Some(("SYN-0004".into(), Source::Type)));
    assert_eq!(cable_cmds::part_value(&doc, k, CableField::Description).map(|v| v.1), Some(Source::Type));
    assert_eq!(cable_cmds::shield(&doc, k), Some((cable_cmds::Shield::Overall, Source::Type)));
    assert_eq!(cable_cmds::read(&doc, k, CableField::Length).as_deref(), Some("12.5"));
    // Overridden per cable, in one transaction.
    h.run(
        "cableedit",
        json!({"cable": k.hex(), "set": {"cores": "7G", "maker": "Othermaker", "part_number": "OTH-7", "shield": "none", "type": "Hand 7G1.5", "length": 30}}),
    )
    .unwrap();
    let (label, _) = last_tx(&mut h);
    assert_eq!(label, "Edit cable");
    let doc = h.doc();
    assert_eq!(cable_cmds::cores_shown(&doc, k).map(|c| (c.0, c.1)), Some(("7G".to_string(), Source::Here)));
    assert_eq!(cable_cmds::part_value(&doc, k, CableField::Maker), Some(("Othermaker".into(), Source::Here)));
    assert_eq!(cable_cmds::part_value(&doc, k, CableField::PartNumber), Some(("OTH-7".into(), Source::Here)));
    assert_eq!(cable_cmds::shield(&doc, k).map(|s| s.1), Some(Source::Here));
    assert_eq!(doc.cables[&k].properties.get("type").map(String::as_str), Some("Hand 7G1.5"));
    // null clears back to the type's value.
    h.run("cableedit", json!({"cable": k.hex(), "set": {"cores": null, "maker": null}})).unwrap();
    let doc = h.doc();
    assert_eq!(cable_cmds::cores_shown(&doc, k).map(|c| c.1), Some(Source::Type));
    assert_eq!(cable_cmds::part_value(&doc, k, CableField::Maker), Some(("Synthmaker".into(), Source::Type)));
    // Everything at once in cablenew, ends included.
    let k2 = new_cable(
        &mut h,
        json!({"tag": "-W20", "type": "Synth 3G1", "cores": "BN,BK,GNYE", "length": "8 m", "shield": "pairs", "maker": "M", "part_number": "P",
               "description": "D", "from": {"location": "+CAB1", "function": "=PUMP", "gland": "M20"}, "to": {"location": "+FIELD", "function": "=PUMP", "gland": "M25"}}),
    );
    let doc = h.doc();
    assert_eq!(tag_of(&doc, k2).as_deref(), Some("-W20"));
    assert_eq!(cable_cmds::plan_words(&doc, k2, Side::From).as_deref(), Some("=PUMP +CAB1"));
    assert_eq!(cable_cmds::plan_words(&doc, k2, Side::To).as_deref(), Some("=PUMP +FIELD"));
    assert_eq!(cable_cmds::gland(&doc, k2, Side::To).map(|g| g.0).as_deref(), Some("M25"));
    assert!(cable_cmds::read(&doc, k2, CableField::Length).is_some(), "`8 m` is a length");
}

#[test]
fn r1_clearing_the_tag_never_leaves_a_cable_without_one() {
    let mut h = H::new(59_103);
    let k = new_cable(&mut h, json!({}));
    let r = h.run("cableedit", json!({"cable": k.hex(), "set": {"tag": null}}));
    let doc = h.doc();
    assert!(tag_of(&doc, k).is_some_and(|t| !t.trim().is_empty()), "tag null: {r:?}; the cable has no tag now: {:?}", doc.cables[&k].properties);
    let r = h.run("cableedit", json!({"cable": k.hex(), "set": {"tag": ""}}));
    let doc = h.doc();
    assert!(tag_of(&doc, k).is_some_and(|t| !t.trim().is_empty()), "tag \"\": {r:?}; the cable has no tag now");
}

// ---------------------------------------------------------------- r2: cableremove

#[test]
fn r2_removing_a_drawn_cable_is_refused_with_its_pages_and_an_undrawn_one_is_removed_exactly() {
    let mut h = H::new(59_201);
    wires(&mut h, 2);
    let w: CableId = id(&slash_across(&mut h, (-5.0, 25.0), (15.0, 25.0), None).unwrap(), "cable");
    let page = h.first_page();
    // Drawn: refused, by id and by tag, with the page in the words and its id in the error.
    for c in [json!(w.hex()), json!("-W01")] {
        let e = refused(&mut h, "cableremove", json!({"cable": c}));
        assert!(e.contains("-W01 is drawn on") && e.contains("Delete it on its page."), "{e}");
        bounded(&e, "Drawn");
    }
    match cable_cmds::drawn(&h.doc(), w) {
        Some(cable_cmds::Drawn { tag, pages }) => {
            assert_eq!(tag, "-W01");
            assert_eq!(pages.len(), 1, "{pages:?}");
            assert_eq!(pages[0].0, page, "the Go to's page");
            assert!(!pages[0].1.is_empty());
        }
        other => panic!("{other:?}"),
    }
    // Not drawn: one transaction, one DeleteCable of exactly the cable; undo and redo exact.
    let k = new_cable(&mut h, json!({"cores": "4G", "length": 5, "to": {"location": "+FIELD", "gland": "M20"}, "maker": "Synthmaker"}));
    let before = h.doc();
    let kept = before.cables[&k].clone();
    h.run("cableremove", json!({"cable": k.hex()})).unwrap();
    let (label, ops) = last_tx(&mut h);
    assert_eq!(label, "Remove cable");
    match ops.as_slice() {
        [Op::DeleteCable { id, cable }] => {
            assert_eq!(*id, k);
            assert!(*cable == kept, "the op holds the whole cable");
        }
        other => panic!("{other:?}"),
    }
    let after = h.doc();
    assert!(!after.cables.contains_key(&k));
    assert!(checks_of(&mut h, "cable-not-drawn").is_empty(), "its row goes with it");
    h.run("u", json!({})).unwrap();
    assert!(h.doc() == before, "undo restores the document exactly");
    assert_eq!(checks_of(&mut h, "cable-not-drawn").len(), 1, "its row comes back");
    h.run("redo", json!({})).unwrap();
    assert!(h.doc() == after, "redo removes it again exactly");
    // Gone: a second remove names no such cable, nothing written.
    let e = refused(&mut h, "cableremove", json!({"cable": k.hex()}));
    bounded(&e, "NoSuchCable");
}

#[test]
fn r2_a_cable_drawn_on_two_pages_names_both() {
    let mut h = H::new(59_202);
    wires(&mut h, 2);
    let w: CableId = id(&slash_across(&mut h, (-5.0, 25.0), (15.0, 25.0), None).unwrap(), "cable");
    let p2 = h.run("newpage", json!({"title": "Second"})).unwrap();
    let p2 = ecad_model::PageId::parse(p2["page"].as_str().or(p2["id"].as_str()).unwrap_or_default());
    let Some(p2) = p2 else { return };
    h.activate(p2);
    // A representation of the cable on page 2 (a continuation slash), if 032 makes one here.
    wires(&mut h, 2);
    if slash_across(&mut h, (-5.0, 25.0), (15.0, 25.0), Some(w)).is_err() {
        // Continuing needs a continuation (032); without one this case is not reached here and
        // the two-page Go to stays unexercised by this file (single page: r2 above).
        return;
    }
    let e = refused(&mut h, "cableremove", json!({"cable": w.hex()}));
    assert!(e.contains(" and "), "both pages: {e}");
    match cable_cmds::drawn(&h.doc(), w) {
        Some(cable_cmds::Drawn { pages, .. }) => assert_eq!(pages.len(), 2, "{pages:?}"),
        other => panic!("{other:?}"),
    }
}

// ---------------------------------------------------------------- r3: cores in use

#[test]
fn r3_reducing_cores_below_those_assigned_is_refused_naming_them() {
    let mut h = H::new(59_301);
    wires(&mut h, 3);
    let k = new_cable(&mut h, json!({"cores": "BN,BK,GY,GNYE"}));
    slash_across(&mut h, (-5.0, 25.0), (25.0, 25.0), Some(k)).unwrap();
    let doc = h.doc();
    let labels: Vec<Option<String>> = doc.cables[&k].cores.iter().map(|m| m.label_str().map(str::to_string)).collect();
    // Design 072 §4.5: GNYE, offered and on no line, is stored too.
    assert_eq!(labels, vec![Some("BN".into()), Some("BK".into()), Some("GY".into()), Some("GNYE".into())]);
    // A list without GY, or with too few: refused, naming the cores in use.
    for (cores, words) in
        [("BN,BK", "has wires on cores BN, BK and GY"), ("BN,BK,GNYE", "has wires on cores BN, BK and GY"), ("2G", "has wires on cores")]
    {
        let e = refused(&mut h, "cableedit", json!({"cable": k.hex(), "set": {"cores": cores}}));
        assert!(e.contains(words), "{cores}: {e}");
        bounded(&e, cores);
    }
    // Through the type notation when the offer comes from it.
    let e = refused(&mut h, "cableedit", json!({"cable": k.hex(), "set": {"cores": null, "type": "2G1.5"}}));
    assert!(e.contains("has wires on cores"), "{e}");
    // Not below: allowed (a spare core removed, a core added).
    h.run("cableedit", json!({"cable": k.hex(), "set": {"cores": "BN,BK,GY"}})).unwrap();
    h.run("cableedit", json!({"cable": k.hex(), "set": {"cores": "BN,BK,GY,GNYE,WH"}})).unwrap();
}

#[test]
fn r3_a_cable_not_drawn_may_take_any_offer() {
    let mut h = H::new(59_302);
    let k = new_cable(&mut h, json!({"cores": "7G"}));
    h.run("cableedit", json!({"cable": k.hex(), "set": {"cores": "2X"}})).unwrap();
    h.run("cableedit", json!({"cable": k.hex(), "set": {"cores": null}})).unwrap();
}

// ---------------------------------------------------------------- r4: planned ends

#[test]
fn r4_ends_planned_and_drawn_in_different_places_raise_nothing() {
    let mut h = H::new(59_401);
    let (_, _, y, x) = two_devices(&mut h, "+CAB1", "+CAB2");
    let k = new_cable(&mut h, json!({"from": {"location": "+CAB1"}, "to": {"location": "+CAB2"}}));
    slash_across(&mut h, (x - 5.0, y), (x + 5.0, y), Some(k)).unwrap();
    assert!(checks_of(&mut h, "cable-end-off-plan").is_empty(), "two ends in two places is normal");
    // An end with no plan raises nothing.
    h.run("cableedit", json!({"cable": k.hex(), "set": {"to": {"location": null}}})).unwrap();
    assert!(checks_of(&mut h, "cable-end-off-plan").is_empty());
}

#[test]
fn r4_a_cable_not_drawn_with_plans_raises_no_off_plan_row() {
    let mut h = H::new(59_402);
    new_cable(&mut h, json!({"from": {"location": "+CAB1", "function": "=PUMP"}, "to": {"location": "+FIELD", "function": "=FAN"}}));
    assert!(checks_of(&mut h, "cable-end-off-plan").is_empty());
}

/// The field end planned `+CAB1` (as the cabinet end) but drawn at `+JB1`: that end differs from
/// its own plan, so it warns, even though another end is drawn at `+CAB1`.
#[test]
fn r4_an_end_drawn_off_its_own_plan_warns_even_when_the_other_end_matches_that_plan() {
    let mut h = H::new(59_403);
    let (_, _, y, x) = two_devices(&mut h, "+CAB1", "+JB1");
    let k = new_cable(&mut h, json!({"from": {"location": "+CAB1"}, "to": {"location": "+CAB1"}}));
    slash_across(&mut h, (x - 5.0, y), (x + 5.0, y), Some(k)).unwrap();
    let rows = checks_of(&mut h, "cable-end-off-plan");
    assert_eq!(rows.len(), 1, "the field end is drawn at +JB1, planned +CAB1: {rows:?}");
    assert!(text(&rows[0]).contains("field end"), "{rows:?}");
}

/// Each end's plan has a function too (Q4): a drawn end whose device's function differs from that
/// end's planned function is off its plan.
#[test]
fn r4_an_end_drawn_off_its_planned_function_warns() {
    let mut h = H::new(59_404);
    let (a, b, y, x) = two_devices(&mut h, "+CAB1", "+JB1");
    h.run("setfunction", json!({"component": a.hex(), "value": "=PUMP"})).unwrap();
    h.run("setfunction", json!({"component": b.hex(), "value": "=FAN"})).unwrap();
    let k = new_cable(&mut h, json!({"from": {"location": "+CAB1", "function": "=PUMP"}, "to": {"location": "+JB1", "function": "=PUMP"}}));
    slash_across(&mut h, (x - 5.0, y), (x + 5.0, y), Some(k)).unwrap();
    let rows = checks_of(&mut h, "cable-end-off-plan");
    assert_eq!(rows.len(), 1, "the field end's device is =FAN, planned =PUMP: {rows:?}");
    assert!(text(&rows[0]).contains("field end"), "{rows:?}");
}

// ---------------------------------------------------------------- r5: not drawn severity

#[test]
fn r5_not_drawn_is_info_and_a_warning_in_the_release_check() {
    let mut h = H::new(59_501);
    let ty = synth_type(&mut h, "Synth Four", json!({"cores": "4G", "size": "1.5"}));
    let k = new_cable(&mut h, json!({"cable_type": ty}));
    let rows = checks_of(&mut h, "cable-not-drawn");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["severity"], json!("info"));
    assert!(checks_of(&mut h, "core-open").is_empty(), "a type giving 4G raises no core-open while not drawn");
    let release = h.cfg(ecad_cadcraft::release::check_rows);
    let row: Vec<&checks::Check> = release.iter().filter(|c| c.kind == checks::Kind::CableNotDrawn).collect();
    assert_eq!(row.len(), 1);
    assert_eq!(row[0].severity(), Severity::Info);
    assert_eq!(row[0].release_severity(), Severity::Warning);
    let (_, warnings, _) = ecad_electrical::release::counts(&release);
    assert!(warnings >= 1);
    // The pre-release check (read only) shows it as a warning.
    let pre = h.run("prerelease", json!({})).unwrap();
    let s = pre.to_string();
    assert!(s.contains("is not in the drawing yet"), "{s:.2000}");
    // Assigned: gone.
    wires(&mut h, 2);
    slash_across(&mut h, (-5.0, 25.0), (15.0, 25.0), Some(k)).unwrap();
    assert!(checks_of(&mut h, "cable-not-drawn").is_empty());
}

// ---------------------------------------------------------------- r6: assign

#[test]
fn r6_assign_follows_the_crossing_order_and_the_core_labels() {
    let mut h = H::new(59_601);
    let ws = wires(&mut h, 3);
    let k = new_cable(&mut h, json!({"cores": "BN,BK,GY,GNYE"}));
    // Right to left: the rightmost wire is crossed first.
    let r = slash_across(&mut h, (25.0, 25.0), (-5.0, 25.0), Some(k)).unwrap();
    assert_eq!(r["assigned"], json!(true), "{r}");
    let doc = h.doc();
    let members: Vec<(WireId, Option<String>)> =
        doc.cables[&k].cores.iter().filter_map(|m| Some((m.wire()?, m.label_str().map(str::to_string)))).collect();
    assert_eq!(members, vec![(ws[2], Some("BN".into())), (ws[1], Some("BK".into())), (ws[0], Some("GY".into()))]);
    // The spare core reads "not assigned".
    let open = checks_of(&mut h, "core-open");
    let spare: Vec<&Value> = open.iter().filter(|c| text(c).contains("core GNYE")).collect();
    assert_eq!(spare.len(), 1, "{open:?}");
    assert!(text(spare[0]).contains("is not assigned"), "{open:?}");
}

#[test]
fn r6_assign_without_an_offer_numbers_1_to_n() {
    let mut h = H::new(59_602);
    wires(&mut h, 3);
    let k = new_cable(&mut h, json!({}));
    let r = slash_across(&mut h, (-5.0, 25.0), (25.0, 25.0), Some(k)).unwrap();
    assert_eq!(r["cores"], json!(["1", "2", "3"]), "{r}");
}

#[test]
fn r6_cableadd_onto_a_cable_not_drawn_takes_its_free_cores_in_order() {
    let mut h = H::new(59_603);
    let ws = wires(&mut h, 2);
    let k = new_cable(&mut h, json!({"cores": "3G"}));
    h.run("cableadd", json!({"conductors": [ws[0].hex(), ws[1].hex()], "cable": k.hex()})).unwrap();
    let doc = h.doc();
    let labels: Vec<Option<String>> = doc.cables[&k].cores.iter().map(|m| m.label_str().map(str::to_string)).collect();
    // Design 072 §4.5: the stored cores 1 and 2 are placed; GNYE stays stored, on no line.
    assert_eq!(labels, vec![Some("1".into()), Some("2".into()), Some("GNYE".into())]);
    assert_eq!(doc.cables[&k].cores[2].position, ecad_model::CorePosition::Unplaced);
    assert!(checks_of(&mut h, "cable-not-drawn").is_empty());
}

#[test]
fn r6_a_cable_not_drawn_skips_core_open_and_a_drawn_cable_slashed_twice_here_is_refused() {
    let mut h = H::new(59_604);
    wires(&mut h, 2);
    let k = new_cable(&mut h, json!({"cores": "5G"}));
    assert!(checks_of(&mut h, "core-open").is_empty());
    assert!(checks_of(&mut h, "no-core-count").is_empty());
    slash_across(&mut h, (-5.0, 25.0), (15.0, 25.0), Some(k)).unwrap();
    let open = checks_of(&mut h, "core-open");
    let spare = open.iter().filter(|c| text(c).contains("is not assigned")).count();
    assert_eq!(spare, 3, "3 spare cores read not assigned (the 2 wired ones are open at their loose ends): {open:?}");
    // Now drawn: a second slash on this page is continue mode, refused here (032).
    let e = refused(&mut h, "cableslash", json!({"from": [-5, 40], "to": [15, 40], "cable": k.hex()}));
    bounded(&e, "slash again");
}

// ---------------------------------------------------------------- r7: typed params, hostile input

fn hostile_new() -> Vec<(Value, &'static str)> {
    let big = "x".repeat(100_000);
    let mut long_key = serde_json::Map::new();
    long_key.insert(big.clone(), json!(1));
    vec![
        (json!({"colour": 1}), "is not a parameter of this command"),
        (Value::Object(long_key), "is not a parameter of this command"),
        (json!({"from": {"colour": "red"}}), "is not a parameter of this command"),
        (json!({"to": {"location": "+F", "extra": 1}}), "is not a parameter of this command"),
        (json!({"tag": big.clone()}), ""),
        (json!({"tag": "-W\u{0007}1"}), ""),
        (json!({"tag": {"a": 1}}), ""),
        (json!({"type": big.clone()}), "at most 64"),
        (json!({"type": "a\nb"}), ""),
        (json!({"maker": big.clone()}), "at most 64"),
        (json!({"part_number": "x".repeat(65)}), "at most 64"),
        (json!({"description": [1, 2]}), ""),
        (json!({"cores": big.clone()}), "Cores"),
        (json!({"cores": "4X1.5"}), "Cores"),
        (json!({"cores": "0G"}), "Cores"),
        (json!({"cores": "99999G"}), "Cores"),
        (json!({"length": "x"}), "Length must be"),
        (json!({"length": -1}), "Length must be"),
        (json!({"length": 0.05}), "Length must be"),
        (json!({"length": 10_000.5}), "Length must be"),
        (json!({"length": "NaN"}), "Length must be"),
        (json!({"length": "inf"}), "Length must be"),
        (json!({"length": "1e400"}), "Length must be"),
        (json!({"length": true}), ""),
        (json!({"shield": "lead"}), "Shield is"),
        (json!({"shield": 3}), ""),
        (json!({"cable_type": "H07X"}), "No cable type"),
        (json!({"cable_type": big.clone()}), "No cable type"),
        (json!({"cable_type": "#00000000000000000000000000000000"}), ""),
        (json!({"from": {"gland": big.clone()}}), "at most 16"),
        (json!({"from": {"gland": "M\u{0000}20"}}), ""),
        (json!({"to": {"location": big.clone()}}), ""),
        (json!({"to": {"function": "\u{202e}=PUMP"}}), ""),
        (json!({"from": "+CAB1"}), ""),
        (json!([1, 2, 3]), ""),
        (json!("cablenew"), ""),
    ]
}

#[test]
fn r7_cablenew_refuses_hostile_and_oversized_input_in_bounded_words_writing_nothing() {
    let mut h = H::new(59_701);
    let mut accepted = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    for (p, words) in hostile_new() {
        let head = h.head();
        let before = h.doc();
        match h.run("cablenew", p.clone()) {
            Ok(v) => {
                // Accepted only when what was stored is within bounds.
                let k: CableId = id(&v, "cable");
                let doc = h.doc();
                let props = &doc.cables[&k].properties;
                assert!(
                    props.values().all(|v| v.chars().count() <= 64 && !v.chars().any(char::is_control)),
                    "{:.120}: stored {props:?}",
                    p.to_string()
                );
                accepted.push(format!("{:.80}", p.to_string()));
                h.run("u", json!({})).unwrap();
            }
            Err(e) => {
                assert_eq!(h.head(), head, "{:.120}: nothing recorded", p.to_string());
                assert!(h.doc() == before, "{:.120}: document unchanged", p.to_string());
                problems.extend(unbounded(&e, &format!("{:.60}", p.to_string())));
                if !e.contains(words) {
                    problems.push(format!("{:.60}: expected {words:?} in {e:.200}", p.to_string()));
                }
            }
        }
    }
    eprintln!("accepted: {accepted:?}");
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn r7_cableedit_and_cableremove_refuse_hostile_input_in_bounded_words() {
    let mut h = H::new(59_702);
    let k = new_cable(&mut h, json!({"cores": "4G"}));
    let big = "x".repeat(100_000);
    let mut problems: Vec<String> = Vec::new();
    for (set, _) in hostile_new() {
        if !set.is_object() {
            continue;
        }
        let head = h.head();
        let before = h.doc();
        if let Err(e) = h.run("cableedit", json!({"cable": k.hex(), "set": set.clone()})) {
            assert_eq!(h.head(), head);
            assert!(h.doc() == before);
            problems.extend(unbounded(&e, &format!("cableedit {:.60}", set.to_string())));
        } else {
            let doc = h.doc();
            let props = &doc.cables[&k].properties;
            assert!(
                props.values().all(|v| v.chars().count() <= 64 && !v.chars().any(char::is_control)),
                "{:.120}: stored {props:?}",
                set.to_string()
            );
            h.run("u", json!({})).unwrap();
        }
    }
    for p in [
        json!({"cable": big.clone(), "set": {"length": 1}}),
        json!({"cable": k.hex()}),
        json!({"set": {"length": 1}}),
        json!({"cable": k.hex(), "set": {"length": 1}, "also": 2}),
        json!({"cable": 12, "set": {}}),
        json!({"cable": k.hex(), "set": "length=1"}),
        Value::Null,
    ] {
        let e = refused(&mut h, "cableedit", p.clone());
        problems.extend(unbounded(&e, &format!("cableedit {:.60}", p.to_string())));
    }
    for p in [json!({"cable": big.clone()}), json!({}), json!({"cable": k.hex(), "force": true}), json!({"cable": null})] {
        let e = refused(&mut h, "cableremove", p.clone());
        problems.extend(unbounded(&e, &format!("cableremove {:.60}", p.to_string())));
    }
    // An unknown key 100 000 characters long: refused, words bounded.
    let mut set = serde_json::Map::new();
    set.insert(big.clone(), json!(1));
    let e = refused(&mut h, "cableedit", json!({"cable": k.hex(), "set": Value::Object(set)}));
    problems.extend(unbounded(&e, "cableedit long unknown key"));
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn r7_the_command_line_refuses_the_same_input_writing_nothing() {
    let mut h = H::new(59_703);
    let k = new_cable(&mut h, json!({}));
    let big = "x".repeat(50_000);
    let lines = [
        "cablenew {\"colour\": 1}".to_string(),
        format!("cablenew {{\"{big}\": 1}}"),
        format!("cablenew {{\"maker\": \"{big}\"}}"),
        "cablenew {\"length\": \"abc\"}".to_string(),
        "cablenew {\"cores\": ".to_string(),
        "cablenew {not json}".to_string(),
        format!("cableedit {{\"cable\": \"{}\", \"set\": {{\"length\": 1e999}}}}", k.hex()),
        format!("cableedit {{\"cable\": \"{}\", \"set\": {{\"cores\": \"{big}\"}}}}", k.hex()),
        format!("cableremove {{\"cable\": \"{big}\"}}"),
    ];
    let mut problems: Vec<String> = Vec::new();
    for line in &lines {
        let head = h.head();
        let before = h.doc();
        let r = h.s.cmdline(line);
        assert_eq!(h.head(), head, "{:.80}: nothing recorded", line);
        assert!(h.doc() == before, "{:.80}: document unchanged", line);
        if let Err(e) = r {
            problems.extend(unbounded(&e.to_string(), &format!("{line:.60}")));
        }
        h.check();
    }
    assert!(problems.is_empty(), "{problems:#?}");
    // A good line works and is one transaction.
    let head = h.head();
    h.cmdline(&format!("cableedit {{\"cable\": \"{}\", \"set\": {{\"length\": 7}}}}", k.hex()));
    assert_eq!(h.head(), head + 1);
    // Typed with no parameters at the command line: one cable, no prompt left running.
    let n = h.doc().cables.len();
    h.cmdline("cablenew");
    assert!(h.s.running.is_none(), "programmatic calls never prompt");
    assert_eq!(h.doc().cables.len(), n + 1);
}

#[test]
fn r7_a_file_with_hostile_cable_values_reads_as_absent_without_a_panic() {
    let mut h = H::new(59_704);
    let k = new_cable(&mut h, json!({}));
    let big = "x".repeat(100_000);
    for (key, v) in [
        ("length", "1e400".to_string()),
        ("length", "-0".to_string()),
        ("cores", "4X1.5".to_string()),
        ("type", big.clone()),
        ("maker", big.clone()),
        ("part_number", "a\u{0000}b".to_string()),
        ("gland.from", "\u{202e}M20".to_string()),
        ("end.to.function", big.clone()),
        ("cable_type", "#ffffffffffffffffffffffffffffffff".to_string()),
    ] {
        h.run("setprop", json!({"cable": k.hex(), "key": key, "value": v})).unwrap();
    }
    // Design 072 §4.5: the shield is a typed field, never a property; a word it is not is refused.
    assert!(h.run("setprop", json!({"cable": k.hex(), "key": "shield", "value": "tartan"})).is_err());
    assert!(!h.doc().cables[&k].properties.contains_key("shield"));
    let doc = h.doc();
    assert!(cable_cmds::read(&doc, k, CableField::Length).is_none());
    assert!(cable_cmds::part_value(&doc, k, CableField::Maker).is_none());
    assert!(cable_cmds::plan_words(&doc, k, Side::To).is_none());
    let rows = checks_of(&mut h, "cable-bad-value");
    assert!(!rows.is_empty());
    for r in &rows {
        bounded(&text(r), "cable-bad-value row");
    }
    // The report and pickers read it without a panic.
    let _ = cable_cmds::not_drawn(&doc);
    let _ = cable_cmds::cores_shown(&doc, k);
    let _ = h.run("report", json!({"kind": "cables"}));
    // Assign onto it: a bad offer is refused or numbered, never a panic.
    wires(&mut h, 2);
    let _ = slash_across(&mut h, (-5.0, 25.0), (15.0, 25.0), Some(k));
}

/// Typed params are an object of named fields: a positional array is not a parameter object, and a
/// remove must never be reached by one (serde reads a struct from a sequence unless told not to).
#[test]
fn r7_positional_array_params_are_refused() {
    let mut h = H::new(59_705);
    let k = new_cable(&mut h, json!({}));
    let e = refused(&mut h, "cableremove", json!([k.hex()]));
    bounded(&e, "cableremove [id]");
    let e = refused(&mut h, "cableedit", json!([k.hex(), {"length": 3}]));
    bounded(&e, "cableedit [id, set]");
}

/// A planned code or a text value holding a bidi override or a zero-width character is refused
/// (it would show reordered or look-alike in lists and Checks); control characters already are.
#[test]
fn r7_format_characters_are_refused_like_control_characters() {
    let mut h = H::new(59_706);
    for p in [json!({"to": {"function": "\u{202e}=PUMP"}}), json!({"maker": "Synth\u{202e}maker"}), json!({"from": {"location": "+CAB\u{200b}1"}})] {
        let e = refused(&mut h, "cablenew", p.clone());
        bounded(&e, &p.to_string());
    }
}

// ---------------------------------------------------------------- r8: ops, undo, update maps

#[test]
fn r8_every_command_is_one_transaction_with_exact_undo_and_redo() {
    let mut h = H::new(59_801);
    let ws = wires(&mut h, 3);
    let steps: Vec<(&str, Box<dyn Fn(CableId) -> Value>)> = vec![
        (
            "cableedit",
            Box::new(
                |k: CableId| json!({"cable": k.hex(), "set": {"length": 12, "shield": "overall", "from": {"location": "+CAB1", "function": "=PUMP", "gland": "M20"}}}),
            ),
        ),
        ("cableedit", Box::new(|k: CableId| json!({"cable": k.hex(), "set": {"tag": "-W09", "maker": "Synthmaker", "cores": "BN,BK,GY,GNYE"}}))),
        ("cableslash", Box::new(|k: CableId| json!({"from": [-5, 25], "to": [25, 25], "cable": k.hex()}))),
        ("cableedit", Box::new(|k: CableId| json!({"cable": k.hex(), "set": {"from": {"location": null}, "length": null}}))),
    ];
    let d0 = h.doc();
    let k = new_cable(&mut h, json!({"to": {"location": "+FIELD"}}));
    let d1 = h.doc();
    let mut states = vec![d0, d1];
    for (cmd, p) in &steps {
        let head = h.head();
        h.run(cmd, p(k)).unwrap_or_else(|e| panic!("{cmd}: {e}"));
        assert_eq!(h.head(), head + 1, "{cmd}: one transaction");
        states.push(h.doc());
    }
    assert_eq!(h.doc().cables[&k].cores.iter().filter_map(|m| m.wire()).collect::<Vec<_>>(), ws);
    // Undo all the way back, each step exact; then redo all, each step exact.
    for i in (0..states.len() - 1).rev() {
        h.run("u", json!({})).unwrap();
        assert!(h.doc() == states[i], "undo to state {i}");
    }
    for (i, s) in states.iter().enumerate().skip(1) {
        h.run("redo", json!({})).unwrap();
        assert!(h.doc() == *s, "redo to state {i}");
    }
    // Replay of the ledger equals the document (H::check does it after every step too).
    assert!(h.p(|p| p.ledger.replay_matches().unwrap()));
}

#[test]
fn r8_a_refusal_in_the_middle_of_several_fields_writes_none_of_them() {
    let mut h = H::new(59_802);
    let k = new_cable(&mut h, json!({}));
    let e = refused(
        &mut h,
        "cableedit",
        json!({"cable": k.hex(), "set": {"length": 12, "maker": "Good", "from": {"location": "+CAB1", "gland": "M20"}, "shield": "lead"}}),
    );
    assert!(e.contains("Shield"), "{e}");
    // A new planned code adds a list entry: refused later, that entry is not written either.
    let e = refused(&mut h, "cablenew", json!({"from": {"location": "+NEWPLACE"}, "length": "x"}));
    assert!(e.contains("Length"), "{e}");
    let doc = h.doc();
    assert!(ecad_cadcraft::designations::code_of(&doc, Aspect::Location, "+NEWPLACE").is_none(), "no stray list entry");
}

fn scope_words(h: &mut H, before: &Document, ops: &[Op]) -> (String, String) {
    let after = h.doc();
    h.cfg(|p| {
        let c = p.checks();
        let a = format!("{:?} {:?}", c.scope(before, ops), c.scope(&after, ops));
        let scope = |d: &Document| {
            let mut s = ecad_cadcraft::outputs::Scope::default();
            s.collect(d, ops);
            s
        };
        let o = p.kept_outputs().map(|_| format!("{:?} {:?}", scope(before), scope(&after))).unwrap_or_default();
        (a, o)
    })
}

#[test]
fn r8_a_cable_edit_touches_only_that_cables_views() {
    let mut h = H::new(59_803);
    // Another cable, drawn, with members, and a device: none of them is in the edit's scope.
    let (a, b, y, x) = two_devices(&mut h, "+CAB1", "+JB1");
    let other: CableId = id(&slash_across(&mut h, (x - 5.0, y), (x + 5.0, y), None).unwrap(), "cable");
    let k = new_cable(&mut h, json!({"cores": "4G"}));
    h.cfg(|p| {
        p.outputs();
        p.checks();
    });
    let before = h.doc();
    h.run("cableedit", json!({"cable": k.hex(), "set": {"length": 12, "to": {"gland": "M20"}, "shield": "pairs"}})).unwrap();
    let (_, ops) = last_tx(&mut h);
    let (c, o) = scope_words(&mut h, &before, &ops);
    assert!(c.contains("all: false"), "checks scope: {c}");
    assert!(!c.contains(&format!("{other:?}")), "the other cable is not re-checked: {c}");
    assert!(!c.contains(&format!("{a:?}")) && !c.contains(&format!("{b:?}")), "no device is re-checked: {c}");
    assert!(c.contains("wires: {}") && c.contains("devices: {}"), "an undrawn cable's edit reaches no wire or device: {c}");
    assert!(c.contains(&format!("{k:?}")), "the cable itself is re-checked: {c}");
    assert!(!o.contains(&format!("{other:?}")), "outputs: the other cable's row is not patched: {o}");
    assert!(o.contains(&format!("{k:?}")), "outputs: its row is patched: {o}");
    // The kept rows equal a fresh build afterwards.
    let doc = h.doc();
    let (kept, built, row, fresh) = h.cfg(|p| {
        let tol = p.tolerance;
        let kept = p.checks().sorted(&doc);
        let built = checks::Checks::build(&doc, tol).sorted(&doc);
        let row = p.outputs().cable_row(k).cloned();
        let fresh = ecad_cadcraft::outputs::kept::Outputs::build_with(&doc, &ecad_cadcraft::text_style::EngineMetrics).cable_row(k).cloned();
        (kept, built, row, fresh)
    });
    assert!(kept == built, "kept checks equal a rebuild");
    assert!(row == fresh, "kept cable row equals a rebuild: {row:?} vs {fresh:?}");
    assert!(row.is_some(), "a cable not drawn has a row");
}

#[test]
fn r8_kept_rows_follow_new_remove_assign_and_undo() {
    let mut h = H::new(59_804);
    wires(&mut h, 2);
    let k = new_cable(&mut h, json!({"cores": "3G", "to": {"location": "+FIELD"}}));
    let k2 = new_cable(&mut h, json!({}));
    slash_across(&mut h, (-5.0, 25.0), (15.0, 25.0), Some(k)).unwrap();
    h.run("cableremove", json!({"cable": k2.hex()})).unwrap();
    h.run("u", json!({})).unwrap();
    h.run("u", json!({})).unwrap();
    h.run("redo", json!({})).unwrap();
    let doc = h.doc();
    let (kept, built, rows, fresh) = h.cfg(|p| {
        let tol = p.tolerance;
        let kept = p.checks().sorted(&doc);
        let built = checks::Checks::build(&doc, tol).sorted(&doc);
        let o = p.outputs();
        let rows = (o.cable_row(k).cloned(), o.cable_row(k2).cloned(), o.cable_count());
        let f = ecad_cadcraft::outputs::kept::Outputs::build_with(&doc, &ecad_cadcraft::text_style::EngineMetrics);
        let fresh = (f.cable_row(k).cloned(), f.cable_row(k2).cloned(), f.cable_count());
        (kept, built, rows, fresh)
    });
    assert!(kept == built, "kept checks equal a rebuild");
    assert!(rows == fresh, "kept cable rows equal a rebuild");
}

// ================================================================ 059 full (059b, 059c, 059d)

/// A cable slashed on page 1 with a reference there, and its other reference end on page 2:
/// drawn on two pages. Returns (cable, page 1, page 2). The end on page 2 is written as
/// `cablecontinue` writes it ([`H::reference_end`]): a continuation needs wires leaving the page
/// through the reference, which form only with lane F step F4.
fn on_two_pages(h: &mut H) -> (CableId, ecad_model::PageId, ecad_model::PageId) {
    let p1 = h.first_page();
    for y in [0.0, 5.0, 10.0] {
        h.run("wire", json!({"points": [[0.0, y], [60.0, y]]})).unwrap();
    }
    let w: CableId = id(&h.run("cableslash", json!({"from": [28, -3], "to": [32, 18]})).unwrap(), "cable");
    h.run("cableref", json!({"cable": w.hex(), "at": [65, 5]})).unwrap();
    let p2: ecad_model::PageId = id(&h.run("newpage", json!({"title": "Field"})).unwrap(), "page");
    h.activate(p2);
    h.reference_end(w, p2, (20.0, 5.0));
    (w, p1, p2)
}

#[test]
fn f1_removing_a_cable_drawn_on_two_pages_names_both_with_a_go_to_each() {
    let mut h = H::new(59_901);
    let (w, p1, p2) = on_two_pages(&mut h);
    let e = refused(&mut h, "cableremove", json!({"cable": w.hex()}));
    assert!(e.contains("is drawn on") && e.contains(" and ") && e.contains("Delete it on its page."), "{e}");
    bounded(&e, "Drawn, two pages");
    match cable_cmds::drawn(&h.doc(), w) {
        Some(cable_cmds::Drawn { pages, .. }) => {
            let ids: Vec<ecad_model::PageId> = pages.iter().map(|p| p.0).collect();
            assert_eq!(ids, vec![p1, p2], "one Go to per page, in book order: {pages:?}");
            assert!(pages.iter().all(|p| !p.1.is_empty()));
            assert!(e.contains(&pages[0].1) && e.contains(&pages[1].1), "the words name both places: {e} / {pages:?}");
        }
        other => panic!("{other:?}"),
    }
}

/// Page `page`'s location set to `code`.
fn page_at(h: &mut H, page: ecad_model::PageId, code: &str) {
    h.run("pageprops", json!({"page": page.hex(), "location": code})).unwrap_or_else(|e| panic!("pageprops: {e}"));
}

#[test]
fn f2_the_picker_lists_not_drawn_first_matches_marked_then_drawn_not_slashed_here() {
    let mut h = H::new(59_902);
    let p1 = h.first_page();
    // A drawn cable slashed on page 1 (absent from page 1's picker), and one on page 2.
    wires(&mut h, 2);
    let here: CableId = id(&slash_across(&mut h, (-5.0, 25.0), (15.0, 25.0), None).unwrap(), "cable");
    let p2: ecad_model::PageId = id(&h.run("newpage", json!({"title": "Other"})).unwrap(), "page");
    h.activate(p2);
    wires(&mut h, 1);
    let elsewhere: CableId = id(&slash_across(&mut h, (-5.0, 25.0), (5.0, 25.0), None).unwrap(), "cable");
    h.activate(p1);
    // Not drawn: -W10 plans nothing, -W11 plans its field end at +CAB1, -W12 its cabinet end.
    let a = new_cable(&mut h, json!({"tag": "-W10"}));
    let b = new_cable(&mut h, json!({"tag": "-W11", "to": {"location": "+CAB1"}}));
    let c = new_cable(&mut h, json!({"tag": "-W12", "from": {"location": "+CAB1"}, "type": "Synth 4G1"}));
    page_at(&mut h, p1, "+CAB1");
    let items = ecad_electrical::cmd::cable::picker(&h.doc(), p1);
    let order: Vec<CableId> = items.iter().map(|i| i.cable).collect();
    assert!(!order.contains(&here), "a cable slashed on this page is absent: {items:?}");
    assert_eq!(order, vec![b, c, a, elsewhere], "matches, then other not drawn, then drawn: {items:?}");
    assert!(items[0].matches && items[1].matches && !items[2].matches && !items[3].matches, "{items:?}");
    assert!(!items[0].drawn && !items[2].drawn && items[3].drawn);
    assert!(items[1].line.contains("Synth 4G1") && items[1].line.contains("+CAB1"), "{:?}", items[1].line);
    // A cable once assigned here leaves the picker of this page.
    for x in [200.0, 210.0] {
        h.wire_pp(&[(x, 0.0), (x, 50.0)]).unwrap();
    }
    slash_across(&mut h, (195.0, 25.0), (215.0, 25.0), Some(a)).unwrap();
    let items = ecad_electrical::cmd::cable::picker(&h.doc(), p1);
    assert!(!items.iter().any(|i| i.cable == a), "slashed here now: absent");
}

fn typed(h: &mut H, tokens: &[&str]) {
    for t in tokens {
        h.cmdline(t);
    }
}

#[test]
fn f3_the_slash_tool_assigns_a_picked_cable_not_drawn_in_crossing_order() {
    let mut h = H::new(59_903);
    let ws = wires(&mut h, 3);
    let k = new_cable(&mut h, json!({"tag": "-W05", "cores": "BN,BK,GY,GNYE"}));
    let head = h.head();
    typed(&mut h, &["cableslash", "Cable", "-W05", "25,25", "-5,25"]);
    assert!(h.s.running.is_none(), "the slash finished");
    assert_eq!(h.head(), head + 1, "one transaction");
    let (label, ops) = last_tx(&mut h);
    assert_eq!(label, "Cable slash");
    // One ModifyCable for the cable, placing three more cores on wires.
    let placed = |c: &ecad_model::Cable| c.cores.iter().filter(|m| m.wire().is_some()).count() as isize;
    let added: Vec<isize> = ops
        .iter()
        .filter_map(|o| match o {
            Op::ModifyCable { id, before, after } if *id == k => Some(placed(after) - placed(before)),
            _ => None,
        })
        .collect();
    assert_eq!(added, vec![3], "{:?}", ops);
    let doc = h.doc();
    let members: Vec<(WireId, Option<String>)> =
        doc.cables[&k].cores.iter().filter_map(|m| Some((m.wire()?, m.label_str().map(str::to_string)))).collect();
    assert_eq!(members, vec![(ws[2], Some("BN".into())), (ws[1], Some("BK".into())), (ws[0], Some("GY".into()))]);
    h.run("u", json!({})).unwrap();
    assert!(!cable_cmds::is_drawn(&h.doc(), k), "one undo: not drawn again");
}

#[test]
fn f3_the_slash_tool_makes_a_new_cable_with_the_next_free_w_by_default_and_by_new() {
    let mut h = H::new(59_904);
    wires(&mut h, 4);
    new_cable(&mut h, json!({"tag": "-W01"}));
    new_cable(&mut h, json!({"tag": "-W02"}));
    let before: Vec<CableId> = h.doc().cables.keys().copied().collect();
    typed(&mut h, &["cableslash", "-5,25", "5,25"]);
    let doc = h.doc();
    let made: Vec<CableId> = doc.cables.keys().filter(|k| !before.contains(k)).copied().collect();
    assert_eq!(made.len(), 1);
    assert_eq!(tag_of(&doc, made[0]).as_deref(), Some("-W03"), "default: New cable, next free -W");
    let before: Vec<CableId> = doc.cables.keys().copied().collect();
    typed(&mut h, &["cableslash", "Cable", "New", "15,25", "35,25"]);
    let doc = h.doc();
    let made: Vec<CableId> = doc.cables.keys().filter(|k| !before.contains(k)).copied().collect();
    assert_eq!(made.len(), 1);
    assert_eq!(tag_of(&doc, made[0]).as_deref(), Some("-W04"));
    // The not-drawn -W01 and -W02 stay not drawn.
    assert_eq!(cable_cmds::not_drawn(&doc).len(), 2);
}

#[test]
fn f3_esc_after_picking_writes_nothing_and_a_cable_slashed_here_is_refused_at_the_pick() {
    let mut h = H::new(59_905);
    wires(&mut h, 2);
    new_cable(&mut h, json!({"tag": "-W05", "cores": "3G"}));
    let head = h.head();
    let before = h.doc();
    typed(&mut h, &["cableslash", "Cable", "-W05", "-5,25"]);
    h.s.input(cadcraft_engine::Input::Cancel).unwrap();
    assert!(h.s.running.is_none());
    assert_eq!(h.head(), head, "Esc writes nothing");
    assert!(h.doc() == before);
    // Slashed here: the pick is refused (032: one slash per page), nothing written.
    typed(&mut h, &["cableslash", "Cable", "-W05", "-5,25", "15,25"]);
    let head = h.head();
    h.cmdline("cableslash");
    h.cmdline("Cable");
    let r = h.s.cmdline("-W05");
    let _ = h.s.input(cadcraft_engine::Input::Cancel);
    assert_eq!(h.head(), head, "nothing written: {r:?}");
}

#[test]
fn f3_more_wires_than_free_cores_at_the_tool_is_refused_and_writes_nothing() {
    let mut h = H::new(59_906);
    wires(&mut h, 4);
    new_cable(&mut h, json!({"tag": "-W05", "cores": "2X"}));
    let head = h.head();
    h.cmdline("cableslash");
    h.cmdline("Cable");
    h.cmdline("-W05");
    h.cmdline("-5,25");
    let _ = h.s.cmdline("35,25");
    let _ = h.s.input(cadcraft_engine::Input::Cancel);
    assert_eq!(h.head(), head, "refused: nothing written");
}

// ---------------------------------------------------------------- 059d glands

#[test]
fn f4_glands_default_from_the_cable_type_and_are_overridden_per_end() {
    let mut h = H::new(59_907);
    let ty = synth_type(&mut h, "Synth Gland", json!({"cores": "3G", "size": "1.5", "gland": "M20"}));
    let k = new_cable(&mut h, json!({"cable_type": ty}));
    let doc = h.doc();
    for s in Side::BOTH {
        assert_eq!(cable_cmds::gland(&doc, k, s), Some(("M20".into(), Source::Type)), "{s:?}");
    }
    h.run("cableedit", json!({"cable": k.hex(), "set": {"to": {"gland": "M25"}}})).unwrap();
    let (label, ops) = last_tx(&mut h);
    assert_eq!(label, "Edit cable");
    assert_eq!(ops.len(), 1, "one SetProperty: {ops:?}");
    let doc = h.doc();
    assert_eq!(cable_cmds::gland(&doc, k, Side::From), Some(("M20".into(), Source::Type)));
    assert_eq!(cable_cmds::gland(&doc, k, Side::To), Some(("M25".into(), Source::Here)));
    let row = h.cfg(|p| p.outputs().cable_row(k).map(|r| r.glands.clone()));
    assert_eq!(row, Some([Some("M20".to_string()), Some("M25".to_string())]));
    // The type's gland changes: the from end follows, the overridden end does not.
    h.run("setstyle", json!({"catalogue": "cable", "id": ty, "fields": {"gland": "M16"}})).unwrap();
    let doc = h.doc();
    assert_eq!(cable_cmds::gland(&doc, k, Side::From).map(|g| g.0).as_deref(), Some("M16"));
    assert_eq!(cable_cmds::gland(&doc, k, Side::To).map(|g| g.0).as_deref(), Some("M25"));
    let row = h.cfg(|p| p.outputs().cable_row(k).map(|r| r.glands.clone()));
    assert_eq!(row, Some([Some("M16".to_string()), Some("M25".to_string())]), "the list row follows the type");
    // null clears the override: back to the type's.
    h.run("cableedit", json!({"cable": k.hex(), "set": {"to": {"gland": null}}})).unwrap();
    assert_eq!(cable_cmds::gland(&h.doc(), k, Side::To), Some(("M16".into(), Source::Type)));
}

#[test]
fn f4_gland_input_is_bounded_on_the_cable_and_on_the_cable_type() {
    let mut h = H::new(59_908);
    let k = new_cable(&mut h, json!({}));
    let big = "x".repeat(100_000);
    for g in [
        json!("M20 and seventeen"),
        json!(big.clone()),
        json!("M\u{202e}20"),
        json!("M\u{200b}20"),
        json!("M\u{0007}20"),
        json!([20]),
        json!({"m": 20}),
    ] {
        let e = refused(&mut h, "cableedit", json!({"cable": k.hex(), "set": {"from": {"gland": g.clone()}}}));
        bounded(&e, &format!("gland {:.40}", g.to_string()));
    }
    // Exactly 16 characters is a gland.
    h.run("cableedit", json!({"cable": k.hex(), "set": {"from": {"gland": "M20x1.5 nickel A"}}})).unwrap();
    // The cable type's gland field: the same bounds.
    let ty = synth_type(&mut h, "Synth Bound", json!({"cores": "3G"}));
    let mut problems = Vec::new();
    for g in ["M20 and seventeen".to_string(), big.clone(), "M\u{202e}20".to_string(), "M\u{200b}20".to_string()] {
        let head = h.head();
        match h.run("setstyle", json!({"catalogue": "cable", "id": ty, "fields": {"gland": g.clone()}})) {
            Ok(_) => {
                let k2 = new_cable(&mut h, json!({"cable_type": ty}));
                let shown = cable_cmds::gland(&h.doc(), k2, Side::From);
                if shown.is_some() {
                    problems.push(format!("type gland {:?} accepted and shown: {shown:?}", g.chars().take(30).collect::<String>()));
                }
                h.run("u", json!({})).unwrap();
                h.run("u", json!({})).unwrap();
            }
            Err(e) => {
                assert_eq!(h.head(), head);
                problems.extend(unbounded(&e, "type gland"));
            }
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn f4_no_gland_is_info_for_a_drawn_cable_only() {
    let mut h = H::new(59_909);
    let k = new_cable(&mut h, json!({"cores": "3G"}));
    assert!(checks_of(&mut h, "no-gland").is_empty(), "not drawn: no gland row");
    wires(&mut h, 2);
    slash_across(&mut h, (-5.0, 25.0), (15.0, 25.0), Some(k)).unwrap();
    let rows = checks_of(&mut h, "no-gland");
    assert_eq!(rows.len(), 2, "one per end: {rows:?}");
    assert!(rows.iter().all(|r| r["severity"] == json!("info")), "{rows:?}");
    h.run("cableedit", json!({"cable": k.hex(), "set": {"from": {"gland": "M20"}, "to": {"gland": "M20"}}})).unwrap();
    assert!(checks_of(&mut h, "no-gland").is_empty());
}
