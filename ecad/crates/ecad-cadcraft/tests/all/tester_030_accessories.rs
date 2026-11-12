//! Tester 030 (terminal strip accessories and jumper links): acceptance checks from outside,
//! written from brief 030 and design 030 §12 only, before the implementation existed, then run
//! against the build. They drive only JSON commands, the Jumper tool's inputs and APIs that 028
//! already had. Defects found are kept as `#[ignore = "tester 030: defect, …"]` probes.
//!
//! Link lines are derived drawing (design §5.3). They are found as page entities mapped to a
//! `Mapped` variant that 028 did not have (by its Debug name), so the test does not depend on what
//! the variant is called.

use std::collections::BTreeMap;

use cadcraft_engine::doc::EntityKind;
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Input, Session};
use ecad_cadcraft::outputs::{Group, Outputs};
use ecad_cadcraft::{Persistence, ProjectHandle};
use ecad_model::{ComponentId, Document, PageId, TerminalId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

struct A {
    h: H,
    page: PageId,
    x1: ComponentId,
    /// `-X01:1…4` at x 100, 150, 200, 250 on y 170.
    t: [TerminalId; 4],
    coil: ComponentId,
}

fn place(h: &mut H, symbol: &str, at: (f64, f64)) -> Value {
    h.run("place", json!({"symbol": symbol, "at": [at.0, at.1]})).unwrap()
}

/// 028's fixture T after its checks B, C and G (design 030 §12): `-X01:1…4` on the first page, a
/// wire bridge 3–4, `terminals 12+2PE`, one PE terminal. `parts` adds fixture A's project defaults.
fn fixture(seed: u64, parts: bool) -> A {
    let mut h = H::new(seed);
    let page = h.first_page();
    h.run("locationbox", json!({"location": "+CP1", "corners": [[20, 150], [400, 280]]})).unwrap();
    h.run("locationbox", json!({"location": "+FIELD", "corners": [[20, 20], [400, 140]]})).unwrap();
    let k1: ComponentId = id(&place(&mut h, "coil", (100.0, 220.0)), "component");
    let k2: ComponentId = id(&place(&mut h, "coil", (150.0, 220.0)), "component");
    h.run("place", json!({"symbol": "no", "at": [200, 220], "device": k1.hex()})).unwrap();
    h.run("place", json!({"symbol": "no", "at": [250, 220], "device": k2.hex()})).unwrap();
    place(&mut h, "motor3", (110.0, 50.0));
    place(&mut h, "pbno", (250.0, 60.0));
    let routes = [
        vec![(100.0, 210.0), (100.0, 65.0)],
        vec![(150.0, 210.0), (150.0, 100.0), (110.0, 100.0), (110.0, 65.0)],
        vec![(200.0, 210.0), (200.0, 95.0), (120.0, 95.0), (120.0, 65.0)],
        vec![(250.0, 210.0), (250.0, 70.0)],
    ];
    for r in &routes {
        h.run("wire", json!({"points": pts(r)})).unwrap();
    }
    let v = h.run("cableslash", json!({"from": [90, 120], "to": [260, 120], "tag": "-W01"})).unwrap();
    let cable: ecad_model::CableId = id(&v, "cable");
    h.run("setprop", json!({"cable": cable.hex(), "key": "type", "value": "YSLY-JZ 7G1.5"})).unwrap();
    let v = h.run("terminals", json!({"from": [90, 170], "to": [260, 170]})).unwrap();
    let x1: ComponentId = id(&v, "strip");
    let t: Vec<TerminalId> = v["terminals"].as_array().unwrap().iter().map(|x| id(x, "terminal")).collect();
    assert_eq!(t.len(), 4, "{v}");
    h.run("bridge", json!({"from": t[2].hex(), "to": t[3].hex()})).unwrap();
    h.run("setprop", json!({"component": x1.hex(), "key": "terminals", "value": "12+2PE"})).unwrap();
    h.run("place", json!({"symbol": "terminalpe", "at": [360, 250], "device": x1.hex()})).unwrap();
    if parts {
        h.run(
            "projectprops",
            json!({"props": {
                "strip_end_stop_part_number": "ES-1",
                "strip_end_plate_part_number": "EP-1",
                "strip_partition_part_number": "TP-1",
                "strip_marker_part_number": "MK-1",
                "strip_marker_per_card": "10",
            }}),
        )
        .unwrap();
    }
    let coil = k1;
    A { h, page, x1, t: [t[0], t[1], t[2], t[3]], coil }
}

/// A second strip `-X02:1,2` on two wires of its own (x 320, 330).
fn second_strip(h: &mut H) -> (ComponentId, Vec<TerminalId>) {
    for x in [320.0, 330.0] {
        h.run("wire", json!({"points": pts(&[(x, 270.0), (x, 160.0)])})).unwrap();
    }
    let v = h.run("terminals", json!({"from": [310, 200], "to": [340, 200]})).unwrap();
    (id(&v, "strip"), v["terminals"].as_array().unwrap().iter().map(|x| id(x, "terminal")).collect())
}

fn same_as_built(h: &mut H) {
    let (kept, built) = h.cfg(|p| {
        let kept = p.outputs().clone();
        (kept, Outputs::build(p.ledger.document()))
    });
    assert!(kept.strips.same_as(&built.strips), "strips differ from a rebuild");
    assert_eq!(kept.bom_table(Group::Location), built.bom_table(Group::Location));
    assert_eq!(kept.bom_table(Group::Part), built.bom_table(Group::Part));
    assert_eq!(kept.wire_table(), built.wire_table());
}

fn model_parts(d: &Document) -> impl PartialEq + std::fmt::Debug + '_ {
    (&d.components, &d.terminals, &d.conductors, &d.cables, &d.pages, &d.page_order, &d.representations, &d.items, &d.properties)
}

/// One write: exactly one transaction; `u` gives back the document before it and `redo` the one
/// after it; the kept strips and reports equal a rebuild after each step.
fn step(h: &mut H, cmd: &str, params: Value) -> Value {
    let before = h.doc();
    let head = h.head();
    let v = h.run(cmd, params.clone()).unwrap_or_else(|e| panic!("{cmd} {params}: {e}"));
    assert_eq!(h.head(), head + 1, "{cmd} {params}: one transaction ({v})");
    let after = h.doc();
    same_as_built(h);
    h.cmdline("u");
    assert_eq!(model_parts(&h.doc()), model_parts(&before), "{cmd} {params}: undo");
    same_as_built(h);
    h.run("redo", Value::Null).unwrap();
    assert_eq!(model_parts(&h.doc()), model_parts(&after), "{cmd} {params}: redo");
    same_as_built(h);
    v
}

fn props(h: &mut H, c: ComponentId) -> BTreeMap<String, String> {
    h.doc().components[&c].properties.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

fn prop(h: &mut H, c: ComponentId, key: &str) -> Option<String> {
    h.doc().components[&c].properties.get(key).cloned()
}

/// A link or accessory value without spaces, for comparing canonical forms.
fn squash(v: Option<String>) -> Option<String> {
    v.map(|s| s.chars().filter(|c| !c.is_whitespace()).collect())
}

fn plan(h: &mut H, strip: ComponentId) -> Value {
    h.run("stripplan", json!({"strip": strip.hex()})).unwrap()["strips"][0].clone()
}

/// The gap rows of a strip's plan, in order: (accessory, gap).
fn gap_rows(plan: &Value) -> Vec<(String, String)> {
    plan["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "accessory")
        .map(|r| (r["accessory"].as_str().unwrap_or("?").to_string(), r["gap"].as_str().unwrap_or("?").to_string()))
        .collect()
}

/// Each row in order as `T:<label>` or `A:<accessory>`.
fn row_seq(plan: &Value) -> Vec<String> {
    plan["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            if r["kind"] == "accessory" {
                format!("A:{}", r["accessory"].as_str().unwrap_or("?"))
            } else {
                format!("T:{}", r["label"].as_str().unwrap_or("?"))
            }
        })
        .collect()
}

fn qty(v: &Value) -> u64 {
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())).unwrap_or(0)
}

/// BOM rows (by location) whose part number is `part`.
fn bom_rows(h: &mut H, part: &str) -> Vec<Value> {
    let bom = h.run("bom", json!({"limit": 5000})).unwrap();
    bom["rows"].as_array().unwrap().iter().filter(|r| r["part_number"] == part).cloned().collect()
}

fn bom_qty(h: &mut H, part: &str) -> u64 {
    bom_rows(h, part).iter().map(|r| qty(&r["qty"])).sum()
}

fn checks_of(h: &mut H, kind: &str) -> Vec<Value> {
    let v = h.run("checks", json!({"kinds": [kind]})).unwrap();
    v["checks"].as_array().cloned().unwrap_or_default()
}

/// Every Checks row of `kind` carries a fix or a go-to (an object it points at).
fn rows_have_a_way_out(h: &mut H, kind: &str) {
    let rows = ecad_cadcraft::checks::all(&h.doc(), 1e-6);
    let mine: Vec<_> = rows.iter().filter(|c| c.kind.name() == kind).collect();
    assert!(!mine.is_empty(), "no {kind} row");
    for c in mine {
        assert!(c.fix.is_some() || c.object != ecad_cadcraft::checks::Object::None, "{kind} row without a fix or go-to: {c:?}");
    }
}

fn net_number(h: &mut H, t: TerminalId) -> Value {
    h.run("net", json!({"terminal": t.hex()})).unwrap()["number"].clone()
}

fn click(h: &mut H, at: ecad_model::Pt) -> cadcraft_engine::Result<()> {
    let p = Vec2::new(at.x, at.y);
    h.s.cursor = p;
    h.s.input(Input::Point(p))
}

fn prompt(h: &H) -> String {
    h.s.current_prompt().map(|p| p.display()).unwrap_or_default()
}

fn last_echo(h: &H) -> String {
    h.s.log.last().cloned().unwrap_or_default()
}

/// `Mapped` variants that existed before 030; anything else on a page is new derived drawing.
const OLD_MAPPED: [&str; 8] = ["Item(", "Marker(", "Fan(", "RefText(", "Tag(", "Label(", "Frame(", "Xref("];

struct Drawn {
    handle: cadcraft_engine::doc::Handle,
    linetype: String,
    pts: Vec<(f64, f64)>,
}

/// The link lines on `page` (the page must be open).
fn link_lines(h: &mut H, page: PageId) -> Vec<Drawn> {
    h.activate(page);
    let ents = h.pv(|p, pg| pg.entities_of(p, page));
    let mut out = Vec::new();
    for e in ents {
        let Some(m) = h.mapped(page, e.handle) else { continue };
        let name = format!("{m:?}");
        if OLD_MAPPED.iter().any(|k| name.starts_with(k)) {
            continue;
        }
        let pts = match &e.kind {
            EntityKind::Line(l) => vec![(l.a.x, l.a.y), (l.b.x, l.b.y)],
            EntityKind::LwPolyline(p) => p.vertices.iter().map(|v| (v.p.x, v.p.y)).collect(),
            _ => continue,
        };
        out.push(Drawn { handle: e.handle, linetype: e.common.linetype.clone(), pts });
    }
    out
}

fn extent(lines: &[Drawn]) -> (f64, f64, f64, f64) {
    let all = lines.iter().flat_map(|d| d.pts.iter().copied());
    all.fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |(a, b, c, d), (x, y)| (a.min(x), b.min(y), c.max(x), d.max(y)))
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.05
}

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ecad t030 {tag} Schaltpl\u{e4}ne {}", ecad_store::disk::unique()));
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

fn has_acc_or_link_key(d: &Document) -> bool {
    d.components.values().any(|c| c.properties.keys().any(|k| k.starts_with("acc.") || k.starts_with("link.") || k.starts_with("jumper.")))
}

// ---------------------------------------------------------------------------------------------

/// Design A (AC 1, AC 8): fixture A as loaded shows no accessories: no gap rows in the plan, no
/// accessory rows on the BOM, no accessory or link keys stored.
#[test]
fn a_fixture_a_shows_no_accessories() {
    let mut a = fixture(0x7030_0001, true);
    let p = plan(&mut a.h, a.x1);
    assert!(gap_rows(&p).is_empty(), "{p}");
    for part in ["ES-1", "EP-1", "TP-1", "MK-1"] {
        assert!(bom_rows(&mut a.h, part).is_empty(), "{part} on the BOM with nothing placed");
    }
    assert!(!has_acc_or_link_key(&a.h.doc()));
    same_as_built(&mut a.h);
    a.h.check();
}

/// Design B, C, D (AC 1, AC 3): end stops and end plates at the ends, a partition after 4, markers
/// on 1…12 in one call. Plan rows in position order, BOM rows per part, each write one
/// transaction that `u` takes back exactly.
#[test]
fn b_to_d_placing_accessories_plan_bom_undo_redo() {
    let mut a = fixture(0x7030_0002, true);
    a.h.cfg(|p| {
        p.outputs();
    });
    let s = a.x1.hex();
    // B
    let v = step(&mut a.h, "accessory", json!({"strip": s, "at": "start", "items": ["end_stop"]}));
    assert_eq!(v["changed"], true, "{v}");
    step(&mut a.h, "accessory", json!({"strip": s, "at": "end", "items": ["end_plate", "end_stop"]}));
    assert_eq!(squash(prop(&mut a.h, a.x1, "acc.start")).as_deref(), Some("end_stop"));
    assert_eq!(squash(prop(&mut a.h, a.x1, "acc.end")).as_deref(), Some("end_plate,end_stop"));
    let p = plan(&mut a.h, a.x1);
    let seq = row_seq(&p);
    assert_eq!(seq.first().map(String::as_str), Some("A:end_stop"), "{seq:?}");
    assert_eq!(&seq[seq.len() - 2..], ["A:end_plate", "A:end_stop"], "{seq:?}");
    assert_eq!(gap_rows(&p), [("end_stop".into(), "start".into()), ("end_plate".into(), "end".into()), ("end_stop".into(), "end".into())]);
    assert_eq!(bom_qty(&mut a.h, "ES-1"), 2);
    assert_eq!(bom_qty(&mut a.h, "EP-1"), 1);
    for r in bom_rows(&mut a.h, "ES-1") {
        assert_eq!(r["tags"], "-X01", "{r}");
    }
    // C
    step(&mut a.h, "accessory", json!({"strip": s, "at": "4", "add": ["partition"]}));
    let seq = row_seq(&plan(&mut a.h, a.x1));
    let i4 = seq.iter().position(|x| x == "T:4").unwrap();
    assert_eq!(seq[i4 + 1], "A:partition", "{seq:?}");
    assert_eq!(bom_qty(&mut a.h, "TP-1"), 1);
    // D: markers on every position, one call, one transaction.
    let labels: Vec<String> = (1..=12).map(|n| n.to_string()).collect();
    let v = step(&mut a.h, "accessory", json!({"strip": s, "at": labels, "on": true, "items": ["marker"]}));
    let _ = v;
    for n in 1..=12 {
        assert_eq!(prop(&mut a.h, a.x1, &format!("acc.on.{n}")).as_deref(), Some("marker"));
    }
    let p = plan(&mut a.h, a.x1);
    let marked = p["rows"].as_array().unwrap().iter().filter(|r| r["marker"] == true).count();
    assert_eq!(marked, 12, "{p}");
    assert_eq!(gap_rows(&p).len(), 4, "markers are not rows: {p}");
    assert_eq!(bom_qty(&mut a.h, "MK-1"), 2, "ceil(12 / 10) cards");
    let card = &bom_rows(&mut a.h, "MK-1")[0];
    assert!(card["description"].as_str().unwrap_or("").to_lowercase().contains("card"), "{card}");
    // Clearing the marker part: no marker row; no-accessory-part counts the 12 markers.
    step(&mut a.h, "projectprops", json!({"props": {"strip_marker_part_number": null}}));
    assert!(bom_rows(&mut a.h, "MK-1").is_empty());
    let rows = checks_of(&mut a.h, "no-accessory-part");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].to_string().contains("12"), "{}", rows[0]);
    // `items: null` clears the partition.
    step(&mut a.h, "accessory", json!({"strip": s, "at": "4", "items": null}));
    assert_eq!(prop(&mut a.h, a.x1, "acc.after.4"), None);
    assert!(bom_rows(&mut a.h, "TP-1").is_empty());
    // `remove` takes one occurrence.
    step(&mut a.h, "accessory", json!({"strip": s, "at": "end", "remove": ["end_stop"]}));
    assert_eq!(squash(prop(&mut a.h, a.x1, "acc.end")).as_deref(), Some("end_plate"));
    // `{terminal}` names the strip and the label: an end plate after 2.
    step(&mut a.h, "accessory", json!({"terminal": a.t[1].hex(), "items": ["end_plate"]}));
    assert_eq!(prop(&mut a.h, a.x1, "acc.after.2").as_deref(), Some("end_plate"));
    // Typed through setprop: spaces and case ignored, stored canonical.
    step(&mut a.h, "setprop", json!({"component": s, "key": "acc.start", "value": " End_Stop , STRIP_LABEL "}));
    assert_eq!(prop(&mut a.h, a.x1, "acc.start").as_deref(), Some("end_stop,strip_label"));
    a.h.check();
}

/// AC 3: marker cards use the strip's `marker.per_card` before the project's; with no card size
/// markers are counted singly.
#[test]
fn marker_cards_follow_per_card_strip_then_project() {
    let mut a = fixture(0x7030_0003, true);
    let s = a.x1.hex();
    let labels: Vec<String> = (1..=12).map(|n| n.to_string()).collect();
    a.h.run("accessory", json!({"strip": s, "at": labels, "on": true, "items": ["marker"]})).unwrap();
    assert_eq!(bom_qty(&mut a.h, "MK-1"), 2);
    step(&mut a.h, "setprop", json!({"component": s, "key": "marker.per_card", "value": "4"}));
    assert_eq!(bom_qty(&mut a.h, "MK-1"), 3, "strip per_card 4 beats the project's 10");
    a.h.run("setprop", json!({"component": s, "key": "marker.per_card", "value": null})).unwrap();
    a.h.run("projectprops", json!({"props": {"strip_marker_per_card": null}})).unwrap();
    assert_eq!(bom_qty(&mut a.h, "MK-1"), 12, "no card size: markers counted singly");
    // A strip part number wins over the project's.
    a.h.run("setprop", json!({"component": s, "key": "marker.part_number", "value": "MK-OWN"})).unwrap();
    assert_eq!(bom_qty(&mut a.h, "MK-OWN"), 12);
    assert!(bom_rows(&mut a.h, "MK-1").is_empty());
    same_as_built(&mut a.h);
    a.h.check();
}

/// Design E (AC 2, AC 10): swapping 1 and 2 in drawing order, `renumber {kind: "terminals"}`
/// moves `acc.after.*` and `acc.on.*` with their terminals and rewrites `link.<n>` in the same
/// transaction; the preview lists the moves and writes nothing; `u` restores all.
#[test]
fn e_renumber_moves_acc_and_link_keys_in_one_transaction() {
    let mut a = fixture(0x7030_0004, true);
    let s = a.x1.hex();
    a.h.run("accessory", json!({"strip": s, "at": "1", "items": ["partition"]})).unwrap();
    a.h.run("accessory", json!({"strip": s, "at": "2", "items": ["end_plate"]})).unwrap();
    a.h.run("accessory", json!({"strip": s, "at": "1", "on": true, "items": ["marker"]})).unwrap();
    a.h.run("jumper", json!({"strip": s, "positions": "1, 3", "skip": true})).unwrap();
    assert_eq!(squash(prop(&mut a.h, a.x1, "link.1")).as_deref(), Some("1,3"));
    // Swap 1 and 2 by moving (wires follow).
    let doc = a.h.doc();
    // The strip's first symbol is of its own definition: its terminal is matched by key and its
    // `connection_points` is empty (doc.rs, brief 018); the other terminals are mapped.
    let item_of = |d: &Document, term: TerminalId| {
        let mut reps = d.representations_of(ecad_model::Subject::Component(a.x1));
        let mapped = d
            .representations_of(ecad_model::Subject::Component(a.x1))
            .find(|(_, r)| r.connection_points.contains_key(&term))
            .map(|(_, r)| r.items[0]);
        mapped.or_else(|| reps.find(|(_, r)| r.connection_points.is_empty()).map(|(_, r)| r.items[0])).unwrap()
    };
    let (i1, i2) = (item_of(&doc, a.t[0]), item_of(&doc, a.t[1]));
    let (h1, h2) = (a.h.handle_of(a.page, i1), a.h.handle_of(a.page, i2));
    a.h.run("move", json!({"handles": [h1.hex()], "delta": [60, 0]})).unwrap();
    a.h.run("move", json!({"handles": [h2.hex()], "delta": [-60, 0]})).unwrap();
    // Preview: lists the moves, writes nothing.
    let head = a.h.head();
    let pv = a.h.run("renumber", json!({"kind": "terminals", "preview": true})).unwrap();
    assert_eq!(a.h.head(), head, "the preview writes nothing");
    let text = pv.to_string();
    assert!(text.contains("partition after 1"), "preview lists the partition move: {pv}");
    assert!(text.contains("link"), "preview lists the link rewrite: {pv}");
    let before = a.h.doc();
    a.h.run("renumber", json!({"kind": "terminals"})).unwrap();
    assert_eq!(a.h.head(), head + 1, "one transaction");
    let d = a.h.doc();
    assert_eq!(d.terminals[&a.t[0]].label.as_deref(), Some("2"));
    assert_eq!(d.terminals[&a.t[1]].label.as_deref(), Some("1"));
    assert_eq!(prop(&mut a.h, a.x1, "acc.after.2").as_deref(), Some("partition"), "partition follows old 1");
    assert_eq!(prop(&mut a.h, a.x1, "acc.after.1").as_deref(), Some("end_plate"), "end plate follows old 2");
    assert_eq!(prop(&mut a.h, a.x1, "acc.on.2").as_deref(), Some("marker"));
    assert_eq!(prop(&mut a.h, a.x1, "acc.on.1"), None);
    assert_eq!(squash(prop(&mut a.h, a.x1, "link.1")).as_deref(), Some("2-3"), "the link keeps its terminals, re-collapsed");
    same_as_built(&mut a.h);
    a.h.cmdline("u");
    assert_eq!(model_parts(&a.h.doc()), model_parts(&before), "one u restores labels and keys");
    a.h.run("redo", Value::Null).unwrap();
    assert_eq!(prop(&mut a.h, a.x1, "acc.after.2").as_deref(), Some("partition"));
    a.h.check();
}

/// Design F and P, link-orphan (AC 4): every Checks row has a way out, and the fixes' effects
/// clear the row.
#[test]
fn f_orphans_and_their_fixes() {
    let mut a = fixture(0x7030_0005, true);
    let s = a.x1.hex();
    a.h.run("accessory", json!({"strip": s, "at": "10", "items": ["partition"]})).unwrap();
    a.h.run("jumper", json!({"strip": s, "positions": "3-6"})).unwrap();
    assert!(checks_of(&mut a.h, "accessory-orphan").is_empty());
    // Lower the offer: 10, 5 and 6 are gone; nothing is refused.
    a.h.run("setprop", json!({"component": s, "key": "terminals", "value": "4+1PE"})).unwrap();
    assert_eq!(prop(&mut a.h, a.x1, "acc.after.10").as_deref(), Some("partition"), "the key is kept");
    let rows = checks_of(&mut a.h, "accessory-orphan");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["which"], "-X01: partition after 10, but -X01 has no position 10", "{}", rows[0]);
    rows_have_a_way_out(&mut a.h, "accessory-orphan");
    let rows = checks_of(&mut a.h, "link-orphan");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0]["which"].as_str().unwrap_or("").contains("link 3-6"), "{}", rows[0]);
    rows_have_a_way_out(&mut a.h, "link-orphan");
    // Remove: the key cleared.
    step(&mut a.h, "setprop", json!({"component": s, "key": "acc.after.10", "value": null}));
    assert!(checks_of(&mut a.h, "accessory-orphan").is_empty());
    // Drop missing positions.
    let head = a.h.head();
    a.h.run("jumper", json!({"link": {"strip": s, "n": 1}, "positions": "3-4"})).unwrap();
    assert_eq!(a.h.head(), head + 1);
    assert!(checks_of(&mut a.h, "link-orphan").is_empty());
    a.h.cmdline("u");
    assert_eq!(checks_of(&mut a.h, "link-orphan").len(), 1);
    // Delete link.
    step(&mut a.h, "jumper", json!({"link": {"strip": s, "n": 1}, "delete": true}));
    assert_eq!(prop(&mut a.h, a.x1, "link.1"), None);
    assert!(checks_of(&mut a.h, "link-orphan").is_empty());
    // An accessory with no part number is counted (Info) with Show.
    a.h.run("accessory", json!({"strip": s, "at": "start", "items": ["strip_label"]})).unwrap();
    let rows = checks_of(&mut a.h, "no-accessory-part");
    assert_eq!(rows.len(), 1, "a strip label has no part number: {rows:?}");
    rows_have_a_way_out(&mut a.h, "no-accessory-part");
    same_as_built(&mut a.h);
    a.h.check();
}

/// Design P (AC 4, brief: a link across a partition is a warning, never a refusal): Split link at
/// 4 and Remove partition each clear the row in one transaction.
#[test]
fn p_link_across_partition_is_a_warning_with_fixes() {
    let mut a = fixture(0x7030_0006, true);
    let s = a.x1.hex();
    a.h.run("accessory", json!({"strip": s, "at": "4", "items": ["partition"]})).unwrap();
    let v = a.h.run("jumper", json!({"strip": s, "positions": "3-6"}));
    assert!(v.is_ok(), "a link across a partition is never refused: {v:?}");
    let rows = checks_of(&mut a.h, "link-across-partition");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["which"], "-X01: link 3-6 crosses the partition after 4", "{}", rows[0]);
    assert_eq!(rows[0]["severity"], "warning", "{}", rows[0]);
    rows_have_a_way_out(&mut a.h, "link-across-partition");
    // Split link at 4: two links, one transaction.
    step(&mut a.h, "jumper", json!({"link": {"strip": s, "n": 1}, "split_at": "4"}));
    let links: Vec<String> = props(&mut a.h, a.x1)
        .into_iter()
        .filter(|(k, _)| k.starts_with("link.") && k[5..].chars().all(|c| c.is_ascii_digit()))
        .map(|(_, v)| v.replace(' ', ""))
        .collect();
    assert_eq!(links.len(), 2, "{links:?}");
    assert!(links.contains(&"3-4".to_string()) && links.contains(&"5-6".to_string()), "{links:?}");
    assert!(checks_of(&mut a.h, "link-across-partition").is_empty());
    a.h.cmdline("u");
    assert_eq!(checks_of(&mut a.h, "link-across-partition").len(), 1);
    // Remove partition.
    step(&mut a.h, "accessory", json!({"strip": s, "at": "4", "remove": ["partition"]}));
    assert!(checks_of(&mut a.h, "link-across-partition").is_empty());
    same_as_built(&mut a.h);
    a.h.check();
}

/// Design G (AC 5): an accessory key recomputes only its strip; a project default recomputes only
/// the strips that inherit it; an unrelated edit recomputes none; kept = rebuilt throughout.
#[test]
fn g_update_maps_recompute_only_what_changed() {
    let mut a = fixture(0x7030_0007, true);
    let (x2, _) = second_strip(&mut a.h);
    a.h.run("accessory", json!({"strip": a.x1.hex(), "at": "start", "items": ["end_stop"]})).unwrap();
    a.h.run("accessory", json!({"strip": x2.hex(), "at": "start", "items": ["end_stop"]})).unwrap();
    a.h.run("setprop", json!({"component": x2.hex(), "key": "end_stop.part_number", "value": "ES-OWN"})).unwrap();
    a.h.cfg(|p| {
        p.outputs();
    });
    let work = |h: &mut H| h.p(|p| p.outputs.as_ref().unwrap().strips.last_work);
    let x2_before = plan(&mut a.h, x2);
    a.h.run("setprop", json!({"component": a.x1.hex(), "key": "acc.after.2", "value": "partition"})).unwrap();
    assert_eq!(work(&mut a.h), 1, "acc.* on -X01 recomputes -X01 only");
    assert_eq!(plan(&mut a.h, x2), x2_before);
    same_as_built(&mut a.h);
    // A project default: only -X01 inherits it (-X02 has its own end stop part).
    a.h.run("projectprops", json!({"props": {"strip_end_stop_part_number": "ES-2"}})).unwrap();
    assert_eq!(work(&mut a.h), 1, "only the strip that inherits the key");
    assert_eq!(bom_qty(&mut a.h, "ES-2"), 1);
    assert_eq!(bom_qty(&mut a.h, "ES-OWN"), 1);
    same_as_built(&mut a.h);
    // A strip part key recomputes that strip.
    a.h.run("setprop", json!({"component": x2.hex(), "key": "partition.maker", "value": "Acme"})).unwrap();
    assert!(work(&mut a.h) <= 1);
    // Unrelated: a coil wired to no strip (028's map recomputes the strips whose rows end on a
    // device, so the fixture's coils, wired to -X01, are not unrelated): its part number, a move.
    let free: ComponentId = id(&place(&mut a.h, "coil", (330.0, 60.0)), "component");
    a.h.run("setprop", json!({"component": free.hex(), "key": "part_number", "value": "K-99"})).unwrap();
    assert_eq!(work(&mut a.h), 0, "a coil's part number recomputes no strip");
    let item = a.h.symbol_item(free);
    let hd = a.h.handle_of(a.page, item);
    a.h.run("move", json!({"handles": [hd.hex()], "delta": [0, 5]})).unwrap();
    assert_eq!(work(&mut a.h), 0, "moving a coil recomputes no strip");
    a.h.run("projectprops", json!({"props": {"strip_jumper_bar": "6"}})).unwrap();
    assert!(work(&mut a.h) <= 2);
    same_as_built(&mut a.h);
    a.h.check();
}

/// Design L (AC 10): the Jumper tool picks 1, then 4, Enter: `link.1 = 1-4` in one transaction,
/// the same as the JSON form; 1–4 share one wire number; Skip makes a skip link; Esc writes
/// nothing; a terminal of another strip is refused and the tool carries on. The line is dashed,
/// through 1–4, with no stub; a skip link runs in its own lane.
#[test]
fn l_jumper_tool_range_skip_and_wire_numbers() {
    // The JSON form on an identical fixture, for comparing the transaction.
    let json_tx = {
        let mut b = fixture(0x7030_0008, true);
        second_strip(&mut b.h);
        b.h.run("jumper", json!({"terminals": [b.t[0].hex(), b.t[3].hex()]})).unwrap();
        b.h.p(|p| p.ledger.log().last().cloned()).unwrap()
    };
    let mut a = fixture(0x7030_0008, true);
    let (_x2, y) = second_strip(&mut a.h);
    a.h.activate(a.page);
    let at: Vec<_> = a.t.iter().map(|t| a.h.terminal_pos(a.page, *t)).collect();
    let y_at = a.h.terminal_pos(a.page, y[0]);
    let (n1, n4) = (net_number(&mut a.h, a.t[0]), net_number(&mut a.h, a.t[3]));
    assert_ne!(n1, n4, "1 and 4 are different nets before the link");
    // Esc writes nothing.
    let head = a.h.head();
    a.h.s.start("jumper").unwrap();
    // Design §7.2 shows "Select first terminal:"; the build also offers Skip here (reported).
    assert!(prompt(&a.h).starts_with("Select first terminal"), "{}", prompt(&a.h));
    click(&mut a.h, at[0]).unwrap();
    assert!(prompt(&a.h).starts_with("Next terminal of -X01"), "{}", prompt(&a.h));
    click(&mut a.h, at[1]).unwrap();
    a.h.s.input(Input::Cancel).unwrap();
    assert!(a.h.s.running.is_none());
    assert_eq!(a.h.head(), head, "Esc writes nothing");
    // Skip: 1 then 3 only.
    a.h.s.start("jumper").unwrap();
    click(&mut a.h, at[0]).unwrap();
    a.h.s.input(Input::Keyword("Skip".into())).unwrap();
    click(&mut a.h, at[2]).unwrap();
    a.h.s.input(Input::Enter).unwrap();
    assert_eq!(a.h.head(), head + 1);
    assert_eq!(squash(prop(&mut a.h, a.x1, "link.1")).as_deref(), Some("1,3"));
    a.h.cmdline("u");
    assert_eq!(a.h.head(), head + 2);
    assert_eq!(prop(&mut a.h, a.x1, "link.1"), None);
    // Range: 1, (another strip: refused, carries on), 4, Enter.
    let head = a.h.head();
    a.h.s.start("jumper").unwrap();
    click(&mut a.h, at[0]).unwrap();
    let r = click(&mut a.h, y_at);
    let said = match r {
        Err(e) => e.to_string(),
        Ok(()) => last_echo(&a.h),
    };
    assert!(said.contains("another strip"), "{said}");
    assert!(a.h.s.running.is_some(), "the tool carries on");
    click(&mut a.h, at[3]).unwrap();
    a.h.s.input(Input::Enter).unwrap();
    assert_eq!(a.h.head(), head + 1, "one transaction");
    assert_eq!(squash(prop(&mut a.h, a.x1, "link.1")).as_deref(), Some("1-4"));
    let tx = a.h.p(|p| p.ledger.log().last().cloned()).unwrap();
    assert_eq!(tx.ops, json_tx.ops, "the tool records what the JSON form records");
    assert_eq!(tx.label, json_tx.label);
    assert!(tx.label.contains("Link -X01 1-4"), "{}", tx.label);
    // One wire number on 1–4; undo splits them again.
    let n: Vec<Value> = a.t.iter().map(|t| net_number(&mut a.h, *t)).collect::<Vec<_>>();
    assert!(n.iter().all(|x| *x == n[0]) && !n[0].is_null(), "one wire number on 1-4: {n:?}");
    same_as_built(&mut a.h);
    a.h.cmdline("u");
    assert_ne!(net_number(&mut a.h, a.t[0]), net_number(&mut a.h, a.t[3]), "u splits the numbers again");
    a.h.run("redo", Value::Null).unwrap();
    // The line: dashed, through 1-4 on y 170, no stub (5…12 are not in the link).
    let lines = link_lines(&mut a.h, a.page);
    assert!(!lines.is_empty(), "the page draws the link line");
    assert!(lines.iter().any(|d| d.linetype.to_lowercase().contains("dash")), "dashed: {:?}", lines.iter().map(|d| &d.linetype).collect::<Vec<_>>());
    let (x0, y0, x1, y1) = extent(&lines);
    assert!(near(x0, at[0].x) && near(x1, at[3].x), "through 1..4, no stub: {x0}..{x1}");
    assert!(near(y0, at[0].y) && near(y1, at[0].y), "lane 0 through the centres: {y0}..{y1}");
    // A skip link 1, 3 overlaps: lane 1, 1.5 mm off the row, never crossing.
    a.h.run("jumper", json!({"terminals": [a.t[0].hex(), a.t[2].hex()], "skip": true})).unwrap();
    assert_eq!(squash(prop(&mut a.h, a.x1, "link.2")).as_deref(), Some("1,3"));
    let p = plan(&mut a.h, a.x1);
    let lanes: Vec<(u64, u64)> = p["links"].as_array().unwrap().iter().map(|l| (l["link"].as_u64().unwrap(), l["lane"].as_u64().unwrap())).collect();
    assert_eq!(lanes, [(1, 0), (2, 1)], "{p}");
    let lines = link_lines(&mut a.h, a.page);
    let ys: std::collections::BTreeSet<i64> =
        lines.iter().flat_map(|d| d.pts.windows(2).filter(|w| near(w[0].1, w[1].1)).map(|w| (w[0].1 * 100.0).round() as i64)).collect();
    let base = (at[0].y * 100.0).round() as i64;
    assert!(ys.contains(&base) && (ys.contains(&(base + 150)) || ys.contains(&(base - 150))), "two lanes 1.5 mm apart: {ys:?}");
    a.h.check();
}

/// Design M (AC 10): a link whose last terminal is drawn on another page: the first page draws the
/// line with a 4 mm stub past its last shown terminal; the other page draws only the stub; a
/// change of the link updates both pages.
#[test]
fn m_link_line_on_every_page_with_stub() {
    let mut a = fixture(0x7030_0009, true);
    let p2: PageId = id(&a.h.run("newpage", json!({"title": "Two"})).unwrap(), "page");
    a.h.activate(p2);
    a.h.run("wire", json!({"points": pts(&[(100.0, 200.0), (100.0, 100.0)])})).unwrap();
    let v = a.h.run("place", json!({"symbol": "terminal", "at": [100, 150], "device": a.x1.hex()})).unwrap();
    let doc = a.h.doc();
    // The terminal drawn on page 2 (PE terminals have numeric labels too, so pick by page).
    let far: TerminalId = ecad_model::derive::terminal_positions(&doc, p2)
        .into_iter()
        .map(|(x, _, _)| x)
        .find(|x| doc.terminals.get(x).is_some_and(|t| t.component == a.x1))
        .unwrap_or_else(|| panic!("{v}"));
    let far_label = doc.terminals[&far].label.clone().unwrap();
    a.h.run("jumper", json!({"strip": a.x1.hex(), "positions": format!("3-{far_label}")})).unwrap();
    let at3 = a.h.terminal_pos(a.page, a.t[2]);
    let at4 = a.h.terminal_pos(a.page, a.t[3]);
    // The fixture's PE terminal (label 5, drawn on page 1 at 360,250) lies between 4 and the far
    // terminal, so page 1 draws 3, 4, 5 and the stub runs past 5 (design M: "through 3–5 with a
    // stub past 5").
    let last = ecad_model::derive::terminal_positions(&a.h.doc(), a.page)
        .into_iter()
        .find(|(x, _, _)| !a.t.contains(x) && doc.terminals.get(x).is_some_and(|t| t.component == a.x1))
        .map(|(_, _, p)| p)
        .expect("the PE terminal on page 1");
    let lines = link_lines(&mut a.h, a.page);
    let (x0, _, _, _) = extent(&lines);
    assert!(near(x0, at3.x), "starts at 3 (3 is the link's first position): {x0}");
    let pts: Vec<(f64, f64)> = lines.iter().flat_map(|d| d.pts.clone()).collect();
    let dist = |p: &(f64, f64)| ((p.0 - last.x).powi(2) + (p.1 - last.y).powi(2)).sqrt();
    assert!(pts.iter().any(|p| near(dist(p), 0.0)), "the line reaches 5: {pts:?}");
    assert!(pts.iter().any(|p| near(dist(p), 4.0)), "a 4 mm stub past 5: {pts:?}");
    assert!(pts.iter().all(|p| dist(p) <= 4.05 || p.0 <= last.x), "nothing beyond the stub: {pts:?}");
    let atf = a.h.terminal_pos(p2, far);
    let lines = link_lines(&mut a.h, p2);
    assert!(!lines.is_empty(), "page 2 draws the stub");
    let (x0, y0, x1, y1) = extent(&lines);
    assert!(near(x0, atf.x) && near(x1, atf.x + 4.0) && near(y0, atf.y) && near(y1, atf.y), "only a stub to the right: {x0},{y0}..{x1},{y1}");
    // Shorten to 3-4: page 2 has no line, page 1 no stub.
    a.h.run("jumper", json!({"link": {"strip": a.x1.hex(), "n": 1}, "positions": "3-4"})).unwrap();
    assert!(link_lines(&mut a.h, p2).is_empty(), "page 2 updated when the link changed");
    let (_, _, x1, _) = extent(&link_lines(&mut a.h, a.page));
    assert!(near(x1, at4.x), "no stub now: {x1}");
    a.h.check();
}

/// Design N (AC 10): Extend, Shorten (inner position → a skip), Split, Delete are one transaction
/// and one `u` each; Erase on the selected line deletes the link and says so.
#[test]
fn n_extend_shorten_split_delete_each_one_transaction() {
    let mut a = fixture(0x7030_000a, true);
    let s = a.x1.hex();
    a.h.run("jumper", json!({"strip": s, "positions": "3-6"})).unwrap();
    let l1 = json!({"strip": s, "n": 1});
    let v = step(&mut a.h, "jumper", json!({"link": l1, "add": ["7", "8"]}));
    assert_eq!(squash(prop(&mut a.h, a.x1, "link.1")).as_deref(), Some("3-8"), "{v}");
    step(&mut a.h, "jumper", json!({"link": l1, "remove": ["5"]}));
    assert_eq!(squash(prop(&mut a.h, a.x1, "link.1")).as_deref(), Some("3-4,6-8"));
    a.h.run("setprop", json!({"component": s, "key": "link.1.part_number", "value": "JB-RD"})).unwrap();
    step(&mut a.h, "jumper", json!({"link": l1, "split_at": "4"}));
    let links: BTreeMap<String, String> =
        props(&mut a.h, a.x1).into_iter().filter(|(k, _)| k.starts_with("link.") && k[5..].chars().all(|c| c.is_ascii_digit())).collect();
    assert_eq!(links.len(), 2, "{links:?}");
    assert_eq!(squash(links.get("link.1").cloned()).as_deref(), Some("3-4"));
    let (k2, v2) = links.iter().find(|(k, _)| *k != "link.1").unwrap();
    assert_eq!(squash(Some(v2.clone())).as_deref(), Some("6-8"));
    let n2: u64 = k2[5..].parse().unwrap();
    assert!(n2 >= 2, "a new number, never reused: {k2}");
    // Delete link 1 clears its part keys too.
    step(&mut a.h, "jumper", json!({"link": l1, "delete": true}));
    let left: Vec<String> = props(&mut a.h, a.x1).into_keys().filter(|k| k.starts_with("link.1")).collect();
    assert!(left.iter().all(|k| !(k == "link.1" || k.starts_with("link.1."))), "{left:?}");
    // Erase on the line of the remaining link (3-4 drawn on page 1 is gone; 6-8 are spares, so
    // make a drawn one first).
    a.h.run("jumper", json!({"strip": s, "positions": "1-2"})).unwrap();
    let lines = link_lines(&mut a.h, a.page);
    assert!(!lines.is_empty());
    let head = a.h.head();
    let v = a.h.run("erase", json!({"handles": [lines[0].handle.hex()]})).unwrap();
    assert_eq!(a.h.head(), head + 1, "one transaction");
    let _ = v; // the message is probed by `n_erase_on_a_link_line_says_deleted_link` (defect)
    assert!(props(&mut a.h, a.x1).values().all(|v| v.replace(' ', "") != "1-2"));
    // `move` on the line is refused (never geometry).
    a.h.cmdline("u");
    let lines = link_lines(&mut a.h, a.page);
    let r = a.h.run("move", json!({"handles": [lines[0].handle.hex()], "delta": [0, 10]}));
    assert!(r.is_err() || a.h.head() == head + 2, "the line is never moved as geometry: {r:?}");
    same_as_built(&mut a.h);
    a.h.check();
}

/// Design O (AC 11): bars per link = ceil(span / bar size); bar size from the strip, else the
/// project, else 10; a link's own part number gives its own row.
#[test]
fn o_jumper_bars_on_the_bom() {
    let mut a = fixture(0x7030_000b, true);
    let s = a.x1.hex();
    a.h.run("projectprops", json!({"props": {"strip_jumper_part_number": "JB-10"}})).unwrap();
    a.h.run("jumper", json!({"strip": s, "positions": "1-12"})).unwrap();
    assert_eq!(bom_qty(&mut a.h, "JB-10"), 2, "12 positions, bar 10");
    step(&mut a.h, "setprop", json!({"component": s, "key": "jumper.bar", "value": "4"}));
    assert_eq!(bom_qty(&mut a.h, "JB-10"), 3, "strip bar 4");
    a.h.run("setprop", json!({"component": s, "key": "jumper.bar", "value": null})).unwrap();
    step(&mut a.h, "projectprops", json!({"props": {"strip_jumper_bar": "5"}}));
    assert_eq!(bom_qty(&mut a.h, "JB-10"), 3, "project bar 5");
    a.h.run("setprop", json!({"component": s, "key": "jumper.bar", "value": "12"})).unwrap();
    assert_eq!(bom_qty(&mut a.h, "JB-10"), 1, "the strip's value wins over the project's");
    // A skip link's span counts every position between its ends (spares too).
    a.h.run("jumper", json!({"strip": s, "positions": "1, 3, 5", "skip": true})).unwrap();
    assert_eq!(bom_qty(&mut a.h, "JB-10"), 2);
    step(&mut a.h, "setprop", json!({"component": s, "key": "link.2.part_number", "value": "JB-10-BU"}));
    assert_eq!(bom_qty(&mut a.h, "JB-10-BU"), 1, "its own row");
    assert_eq!(bom_qty(&mut a.h, "JB-10"), 1);
    let p = plan(&mut a.h, a.x1);
    let bars: Vec<u64> = p["links"].as_array().unwrap().iter().map(|l| l["bars"].as_u64().unwrap()).collect();
    assert_eq!(bars, [1, 1], "{p}");
    let by_part = a.h.run("bom", json!({"group": "part"})).unwrap();
    assert!(by_part.to_string().contains("JB-10-BU"));
    same_as_built(&mut a.h);
    a.h.check();
}

/// The reads that must survive a reopen.
fn reads(h: &mut H) -> Value {
    let mut checks = h.run("checks", json!({})).unwrap();
    if let Some(o) = checks.as_object_mut() {
        o.remove("revision");
    }
    json!({
        "stripplan": h.run("stripplan", json!({})).unwrap(),
        "bom": h.run("bom", json!({"limit": 5000})).unwrap(),
        "bom_part": h.run("bom", json!({"group": "part", "limit": 5000})).unwrap(),
        "wirelist": h.run("wirelist", json!({})).unwrap(),
        "checks": checks,
    })
}

/// Design J (AC 8): every accessory, part and link key, the plan, the BOM and the checks are the
/// same after save, close and reopen at a Windows-style path; open adds no transaction.
#[test]
fn j_save_reopen_keeps_everything() {
    let dir = tmp_dir("j");
    let mut a = fixture(0x7030_000c, true);
    let s = a.x1.hex();
    a.h.run("accessory", json!({"strip": s, "at": "start", "items": ["end_stop", "strip_label"]})).unwrap();
    a.h.run("accessory", json!({"strip": s, "at": "end", "items": ["end_plate", "end_stop"]})).unwrap();
    a.h.run("accessory", json!({"strip": s, "at": "PE1", "items": ["end_plate"]})).unwrap_or_default();
    a.h.run("accessory", json!({"strip": s, "at": "2", "items": ["partition"]})).unwrap();
    a.h.run("accessory", json!({"strip": s, "at": ["1", "2", "3"], "on": true, "items": ["marker"]})).unwrap();
    a.h.run("setprop", json!({"component": s, "key": "end_plate_pe.part_number", "value": "EP-PE"})).unwrap();
    a.h.run("setprop", json!({"component": s, "key": "marker.per_card", "value": "4"})).unwrap();
    a.h.run("jumper", json!({"strip": s, "positions": "1-2"})).unwrap();
    a.h.run("jumper", json!({"strip": s, "positions": "3, 7", "skip": true})).unwrap();
    a.h.run("setprop", json!({"component": s, "key": "link.2.part_number", "value": "JB-BU"})).unwrap();
    a.h.run("setprop", json!({"component": s, "key": "jumper.bar", "value": "8"})).unwrap();
    a.h.run("projectprops", json!({"props": {"strip_jumper_part_number": "JB-10", "strip_jumper_bar": "10"}})).unwrap();
    a.h.check();
    let before_reads = reads(&mut a.h);
    let before_doc = a.h.doc();
    let before_head = a.h.head();
    a.h.cfg(|p| p.persistence = Persistence::File);
    let path = dir.join("Klemmen -X01 (Rev A).ecad").display().to_string();
    a.h.run("saveas", json!({"path": path})).unwrap();
    close_all(&mut a.h);
    drop(a.h);
    let mut h2 = reopen(&path, 0x7030_000d);
    assert_eq!(h2.head(), before_head, "open adds no transaction");
    assert_eq!(model_parts(&h2.doc()), model_parts(&before_doc));
    assert_eq!(reads(&mut h2), before_reads);
    assert_eq!(h2.head(), before_head, "reads record nothing");
    same_as_built(&mut h2);
    h2.check();
    close_all(&mut h2);
    drop(h2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Design J, second half (AC 8): a 028 file (strip, offer, PE, bridge, no 030 key) reopens with no
/// transaction, shows no accessories or links, and its log is unchanged by open, read and close.
#[test]
fn j_a_028_file_opens_unchanged_and_shows_no_accessories() {
    let dir = tmp_dir("old");
    let mut a = fixture(0x7030_000e, false);
    assert!(!has_acc_or_link_key(&a.h.doc()));
    a.h.cfg(|p| p.persistence = Persistence::File);
    let path = dir.join("old 028.ecad").display().to_string();
    a.h.run("saveas", json!({"path": path})).unwrap();
    let head = a.h.head();
    let before = reads(&mut a.h);
    close_all(&mut a.h);
    drop(a.h);
    let mut h2 = reopen(&path, 0x7030_000f);
    assert_eq!(h2.head(), head);
    let p = h2.run("stripplan", json!({})).unwrap();
    assert!(p["strips"].as_array().unwrap().iter().all(|s| gap_rows(s).is_empty()), "{p}");
    assert_eq!(reads(&mut h2), before, "the same reads as before the save");
    assert!(!has_acc_or_link_key(&h2.doc()));
    assert_eq!(h2.head(), head, "reads record nothing");
    h2.check();
    close_all(&mut h2);
    drop(h2);
    let mut again = reopen(&path, 0x7030_0010);
    assert_eq!(again.head(), head);
    close_all(&mut again);
    drop(again);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Design K: hostile input is refused in words (or listed by Checks where the design says so),
/// writes nothing when refused, and never panics.
#[test]
fn k_hostile_input_refused_without_panic() {
    let mut a = fixture(0x7030_0011, true);
    let s = a.x1.hex();
    let coil = a.coil.hex();
    let head = a.h.head();
    let many: Vec<String> = (0..5000).map(|n| n.to_string()).collect();
    let refused: Vec<(&str, Value)> = vec![
        ("setprop", json!({"component": s, "key": "acc.start", "value": "partition,".repeat(1000)})),
        ("accessory", json!({"strip": s, "at": "start", "items": vec!["end_stop"; 9]})),
        ("accessory", json!({"strip": s, "at": "start", "items": ["marker"]})),
        ("accessory", json!({"strip": s, "at": "4", "on": true, "items": ["partition"]})),
        ("accessory", json!({"strip": s, "at": "4", "items": ["plate"]})),
        ("accessory", json!({"strip": s, "at": "15", "items": ["partition"]})),
        ("accessory", json!({"strip": s, "at": many, "on": true, "items": ["marker"]})),
        ("accessory", json!({"strip": coil, "at": "start", "items": ["end_stop"]})),
        ("accessory", json!({"strip": "nothex", "at": "start", "items": ["end_stop"]})),
        ("accessory", json!({"strip": s, "at": 4, "items": ["end_stop"]})),
        ("accessory", json!({"strip": s})),
        ("setprop", json!({"component": s, "key": "acc.on.4", "value": "partition"})),
        ("setprop", json!({"component": s, "key": "acc.after.4", "value": "marker"})),
        ("setprop", json!({"component": s, "key": "marker.per_card", "value": "0"})),
        ("setprop", json!({"component": s, "key": "marker.per_card", "value": "101"})),
        ("setprop", json!({"component": s, "key": "marker.per_card", "value": "1e400"})),
        ("setprop", json!({"component": s, "key": "jumper.bar", "value": "0"})),
        ("setprop", json!({"component": s, "key": "jumper.bar", "value": "1"})),
        ("setprop", json!({"component": s, "key": "jumper.bar", "value": "51"})),
        ("setprop", json!({"component": s, "key": "jumper.bar", "value": "-3"})),
        ("setprop", json!({"component": s, "key": "link.1", "value": "1\u{2013}x"})),
        ("projectprops", json!({"props": {"strip_jumper_bar": "0"}})),
        ("projectprops", json!({"props": {"strip_marker_per_card": "1000"}})),
        ("jumper", json!({"positions": "1-4"})),
        ("jumper", json!({"strip": s, "positions": "1\u{2013}x"})),
        ("jumper", json!({"strip": s, "positions": ""})),
        ("jumper", json!({"strip": coil, "positions": "1-2"})),
        ("jumper", json!({"terminals": [a.t[0].hex()]})),
        ("jumper", json!({"terminals": [a.t[0].hex(), "ffffffffffffffffffffffffffffffff"]})),
        ("jumper", json!({"link": {"strip": s, "n": 1}, "delete": true})),
        ("jumper", json!({"link": {"strip": s, "n": u64::MAX}, "add": ["1"]})),
    ];
    for (cmd, params) in refused {
        let shown = {
            let t = params.to_string();
            if t.chars().count() > 160 { format!("{}…", t.chars().take(160).collect::<String>()) } else { t }
        };
        let r = a.h.run(cmd, params);
        assert!(r.is_err(), "{cmd} {shown}: expected a refusal, got {r:?}");
        assert!(!r.unwrap_err().trim().is_empty(), "{cmd} {shown}: refused in words");
        assert_eq!(a.h.head(), head, "{cmd} {shown}: nothing written");
    }
    // The design's exact sentences for two of them.
    let e = a.h.run("accessory", json!({"strip": s, "at": "4", "items": ["plate"]})).unwrap_err();
    assert!(e.contains("\"plate\" is not an accessory"), "{e}");
    let e = a.h.run("accessory", json!({"strip": coil, "at": "start", "items": ["end_stop"]})).unwrap_err();
    assert!(e.contains("is not a terminal strip"), "{e}");
    let e = a.h.run("setprop", json!({"component": s, "key": "marker.per_card", "value": "0"})).unwrap_err();
    assert!(e.contains("is not a card size"), "{e}");
    // Refusal or a Checks row: never a panic, and kept = rebuilt either way.
    let lenient: Vec<(&str, Value)> = vec![
        ("setprop", json!({"component": s, "key": format!("acc.after.{}", "9".repeat(10_000)), "value": "partition"})),
        ("setprop", json!({"component": coil, "key": "acc.after.1", "value": "partition"})),
        ("setprop", json!({"component": coil, "key": "link.1", "value": "1-2"})),
        ("setprop", json!({"component": s, "key": "link.1", "value": "1-99999"})),
        ("setprop", json!({"component": s, "key": "link.0", "value": "1-4"})),
        ("setprop", json!({"component": s, "key": "link.1000", "value": "1-4"})),
        ("setprop", json!({"component": s, "key": "link.2", "value": "3"})),
        ("setprop", json!({"component": s, "key": "link.3", "value": ",".repeat(5000)})),
        ("setprop", json!({"component": s, "key": "link.4", "value": "1-".repeat(3000)})),
        ("jumper", json!({"strip": s, "positions": "1-99999"})),
        ("jumper", json!({"strip": s, "positions": "4-1"})),
        ("setprop", json!({"component": s, "key": "acc.on.4", "value": "marker,".repeat(8)})),
    ];
    for (cmd, params) in lenient {
        let shown = {
            let t = params.to_string();
            if t.chars().count() > 160 { format!("{}…", t.chars().take(160).collect::<String>()) } else { t }
        };
        let head = a.h.head();
        match a.h.run(cmd, params) {
            Err(e) => {
                assert!(!e.trim().is_empty(), "{cmd} {shown}");
                assert_eq!(a.h.head(), head, "{cmd} {shown}: refused but written");
            }
            Ok(_) => {
                let _ = a.h.run("checks", json!({})).unwrap();
                let _ = a.h.run("stripplan", json!({"limit": 5000})).unwrap();
                let _ = a.h.run("bom", json!({})).unwrap();
            }
        }
        same_as_built(&mut a.h);
    }
    a.h.check();
}

/// Design §7.4: "Erase on a selected link deletes the link, saying so (`Deleted link -X01 1-4.`)".
/// Defect: the erase deletes the link's keys (translate.rs) but echoes no note.
#[test]
fn n_erase_on_a_link_line_says_deleted_link() {
    let mut a = fixture(0x7030_000b, true);
    a.h.run("jumper", json!({"strip": a.x1.hex(), "positions": "1-2"})).unwrap();
    let lines = link_lines(&mut a.h, a.page);
    assert!(!lines.is_empty());
    let v = a.h.run("erase", json!({"handles": [lines[0].handle.hex()]})).unwrap();
    assert_eq!(prop(&mut a.h, a.x1, "link.1"), None, "the link is deleted");
    let said = format!("{v} {}", last_echo(&a.h));
    assert!(said.contains("Deleted link -X01 1-2"), "{said}");
}
