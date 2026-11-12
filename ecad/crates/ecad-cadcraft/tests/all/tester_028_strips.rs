//! Tester 028 (terminal strips and cable schedule): acceptance checks from outside, written from
//! brief 028 and design 028 §12 only. Save, reopen and replay with every new property (Windows
//! path with spaces and non-ASCII letters), undo and redo of every new write, terminal numbering
//! with fixed and provisional strip tags, the generated pages' update maps, an old file, and
//! hostile input. A probe that found a bug was `#[ignore = "tester 028: <bug>"]` and failed until it
//! was fixed (all three are fixed and run now).

use cadcraft_engine::Session;
use ecad_cadcraft::outputs::{Group, Outputs};
use ecad_cadcraft::{Persistence, ProjectHandle};
use ecad_model::{CableId, ComponentId, ConductorId, Document, PageId, TerminalId, tag};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

struct T {
    h: H,
    page: PageId,
    k2: ComponentId,
    wires: [ConductorId; 4],
    cable: CableId,
}

fn place(h: &mut H, symbol: &str, at: (f64, f64)) -> Value {
    h.run("place", json!({"symbol": symbol, "at": [at.0, at.1]})).unwrap()
}

/// Fixture T of design 028 §12 (the same geometry as the worker's, built here independently).
fn fixture(seed: u64) -> T {
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
    let mut wires = Vec::new();
    for r in &routes {
        let v = h.run("wire", json!({"points": pts(r)})).unwrap();
        wires.push(id::<ConductorId>(&v, "conductor"));
    }
    let v = h.run("cableslash", json!({"from": [90, 120], "to": [260, 120], "tag": "-W01"})).unwrap();
    let cable: CableId = id(&v, "cable");
    h.run("setprop", json!({"cable": cable.hex(), "key": "type", "value": "YSLY-JZ 7G1.5"})).unwrap();
    T { h, page, k2, wires: [wires[0], wires[1], wires[2], wires[3]], cable }
}

fn same_as_built(h: &mut H) {
    let (kept, built) = h.cfg(|p| {
        let kept = p.outputs().clone();
        (kept, Outputs::build(p.ledger.document()))
    });
    assert!(kept.strips.same_as(&built.strips), "strips differ from a rebuild");
    assert_eq!(kept.cable_table(), built.cable_table());
    assert_eq!(kept.wire_table(), built.wire_table());
    assert_eq!(kept.bom_table(Group::Location), built.bom_table(Group::Location));
}

fn fence(h: &mut H, y: f64) -> (ComponentId, Vec<TerminalId>) {
    let v = h.run("terminals", json!({"from": [90, y], "to": [260, y]})).unwrap();
    let terms = v["terminals"].as_array().unwrap().iter().map(|x| id(x, "terminal")).collect();
    (id(&v, "strip"), terms)
}

/// Every read the brief names, as one JSON value (the same before and after a reopen).
fn reads(h: &mut H, cable: CableId) -> Value {
    let mut checks = h.run("checks", json!({})).unwrap();
    // Checks rows carry no per-session IDs we compare on; keep them whole.
    if let Some(o) = checks.as_object_mut() {
        o.remove("revision");
    }
    json!({
        "stripplan": h.run("stripplan", json!({})).unwrap(),
        "cablelist": h.run("cablelist", json!({})).unwrap(),
        "cablecores": h.run("cablecores", json!({"cable": cable.hex()})).unwrap(),
        "bom": h.run("bom", json!({})).unwrap(),
        "wirelist": h.run("wirelist", json!({})).unwrap(),
        "checks": checks,
    })
}

fn model_parts(d: &Document) -> impl PartialEq + std::fmt::Debug + '_ {
    (&d.components, &d.terminals, &d.conductors, &d.cables, &d.pages, &d.page_order, &d.representations, &d.items)
}

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    // Spaces and non-ASCII letters, as on a Windows desktop ("C:\Users\…\Schaltpläne 2026").
    let dir = std::env::temp_dir().join(format!("ecad t028 {tag} Schaltpl\u{e4}ne {}", ecad_store::disk::unique()));
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

/// AC 10 (design K): every new property (`terminals`, `cores`, `part_number.pe`, `maker.pe`, page
/// `generated: terminals|cables` and `strip`), a bridge, a PE terminal and renumbered labels survive
/// save, close and reopen at a Windows-style path; every read is the same; replay equals live.
#[test]
fn k_save_reopen_and_replay_keep_every_new_property() {
    let dir = tmp_dir("k");
    let mut t = fixture(0x7028_0001);
    let (x1, terms) = fence(&mut t.h, 170.0);
    t.h.run("bridge", json!({"from": terms[2].hex(), "to": terms[3].hex()})).unwrap();
    t.h.run("setprop", json!({"component": x1.hex(), "key": "terminals", "value": "12+2PE"})).unwrap();
    t.h.run("place", json!({"symbol": "terminalpe", "at": [360, 250], "device": x1.hex()})).unwrap();
    for (k, v) in [("part_number", "FT-2.5"), ("maker", "Acme"), ("part_number.pe", "PE-2.5"), ("maker.pe", "Acme PE")] {
        t.h.run("setprop", json!({"component": x1.hex(), "key": k, "value": v})).unwrap();
    }
    t.h.run("setprop", json!({"cable": t.cable.hex(), "key": "cores", "value": "BN,BK,GY,BU,GNYE"})).unwrap();
    t.h.run("renumber", json!({"kind": "terminals", "start": 3})).unwrap();
    let plan_page: PageId = id(&t.h.run("newpage", json!({"generated": "terminals", "strip": x1.hex()})).unwrap(), "page");
    let cables_page: PageId = id(&t.h.run("newpage", json!({"generated": "cables"})).unwrap(), "page");
    t.h.check();
    let before_reads = reads(&mut t.h, t.cable);
    let before_doc = t.h.doc();
    let before_head = t.h.head();
    // Sanity on what is being kept.
    let plan = &before_reads["stripplan"]["strips"][0];
    assert_eq!(plan["offered"], "12+2PE", "{plan}");
    assert!(plan["rows"].as_array().unwrap().iter().any(|r| r["bridge_group"].is_number()), "{plan}");
    let pe_row = before_reads["bom"]["rows"].as_array().unwrap().iter().find(|r| r["tags"] == "-X01" && r["qty"] == 2).cloned();
    assert!(pe_row.as_ref().is_some_and(|r| r.to_string().contains("PE-2.5")), "PE BOM row carries part_number.pe: {pe_row:?}");
    assert_eq!(before_reads["cablecores"]["source"], "cores");

    t.h.cfg(|p| p.persistence = Persistence::File);
    let path = dir.join("Plan -X01 (Rev A).ecad").display().to_string();
    t.h.run("saveas", json!({"path": path})).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    close_all(&mut t.h);
    drop(t.h);

    let mut h2 = reopen(&path, 0x7028_0002);
    assert_eq!(h2.head(), before_head, "open adds no transaction");
    let after_doc = h2.doc();
    assert_eq!(model_parts(&after_doc), model_parts(&before_doc));
    assert_eq!(ecad_cadcraft::generated::strip_of(&after_doc, plan_page), Some(x1));
    assert_eq!(ecad_cadcraft::generated::kind_of(&after_doc, cables_page), Some(ecad_cadcraft::generated::GenKind::Cables));
    let after_reads = reads(&mut h2, t.cable);
    assert_eq!(after_reads, before_reads, "reads after reopen");
    same_as_built(&mut h2);
    h2.check();
    close_all(&mut h2);
    drop(h2);
    // A second open shows the same log (the redb store rewrites its own pages on open, so the
    // bytes are not compared; the records are).
    let _ = bytes;
    let mut h3 = reopen(&path, 0x7028_0003);
    assert_eq!(h3.head(), before_head);
    assert_eq!(model_parts(&h3.doc()), model_parts(&before_doc));
    close_all(&mut h3);
    drop(h3);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every new write is one transaction; `u` gives back the document before it exactly and `redo`
/// the one after it, and the kept strips and schedule equal a rebuild after each step.
#[test]
fn undo_and_redo_of_every_strip_write_are_exact() {
    let mut t = fixture(0x7028_0003);
    t.h.cfg(|p| {
        p.outputs();
    });
    let x1: ComponentId = id(&place(&mut t.h, "terminal", (100.0, 170.0)), "component");
    let steps: Vec<(&str, Value)> = vec![
        ("terminals", json!({"strip": x1.hex(), "from": [140, 170], "to": [260, 170]})),
        ("terminals", json!({"strip": x1.hex(), "pe": true, "from": [90, 190], "to": [160, 190]})),
        ("setprop", json!({"component": x1.hex(), "key": "terminals", "value": "8+2PE"})),
        ("setprop", json!({"component": x1.hex(), "key": "part_number.pe", "value": "PE-1"})),
        ("setprop", json!({"component": x1.hex(), "key": "maker.pe", "value": "Acme"})),
        ("setprop", json!({"cable": t.cable.hex(), "key": "cores", "value": "5X"})),
        ("renumber", json!({"kind": "terminals", "start": 7})),
        ("newpage", json!({"generated": "terminals"})),
        ("newpage", json!({"generated": "cables"})),
        ("setprop", json!({"cable": t.cable.hex(), "key": "cores", "value": null})),
        ("setprop", json!({"component": x1.hex(), "key": "terminals", "value": null})),
    ];
    for (cmd, params) in steps {
        let before = t.h.doc();
        let head = t.h.head();
        let v = t.h.run(cmd, params.clone()).unwrap_or_else(|e| panic!("{cmd} {params}: {e}"));
        assert_eq!(t.h.head(), head + 1, "{cmd} {params}: one transaction ({v})");
        let after = t.h.doc();
        same_as_built(&mut t.h);
        t.h.cmdline("u");
        assert_eq!(model_parts(&t.h.doc()), model_parts(&before), "{cmd} {params}: undo");
        same_as_built(&mut t.h);
        t.h.run("redo", Value::Null).unwrap();
        assert_eq!(model_parts(&t.h.doc()), model_parts(&after), "{cmd} {params}: redo");
        same_as_built(&mut t.h);
    }
    // The bridge, the last write before a terminal is removed by undo.
    let plan = t.h.run("stripplan", json!({})).unwrap();
    let ids: Vec<TerminalId> = plan["strips"][0]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["state"] == "drawn" && r["type"] != "pe")
        .map(|r| id(r, "terminal"))
        .collect();
    t.h.activate(t.page);
    let before = t.h.doc();
    t.h.run("bridge", json!({"from": ids[0].hex(), "to": ids[1].hex()})).unwrap();
    t.h.cmdline("u");
    assert_eq!(model_parts(&t.h.doc()), model_parts(&before), "bridge undo");
    same_as_built(&mut t.h);
    t.h.check();
}

fn labels(h: &mut H, terms: &[TerminalId]) -> Vec<String> {
    let d = h.doc();
    terms.iter().map(|x| d.terminals[x].label.clone().unwrap_or_default()).collect()
}

/// AC 3 with 026's tag rules: a fixed strip tag stays and its terminals are still renumbered; a
/// provisional strip's terminals renumber and Finalise leaves the labels alone; a renumber of
/// devices or `all` never relabels terminals; one strip by selection leaves the other alone; a
/// renumber that changes nothing records nothing.
#[test]
fn terminal_renumber_with_fixed_and_provisional_strip_tags() {
    let mut t = fixture(0x7028_0004);
    let (x1, a) = fence(&mut t.h, 170.0);
    let (x2, b) = fence(&mut t.h, 190.0);
    assert_eq!(labels(&mut t.h, &a), ["1", "2", "3", "4"]);
    // Already in order: nothing recorded.
    let head = t.h.head();
    t.h.run("renumber", json!({"kind": "terminals"})).unwrap();
    assert_eq!(t.h.head(), head, "a renumber that changes nothing records nothing");
    // Fixed strip tag.
    t.h.run("setprop", json!({"component": x1.hex(), "key": "tag_fixed", "value": "yes"})).unwrap();
    t.h.run("renumber", json!({"kind": "terminals", "start": 21})).unwrap();
    assert_eq!(labels(&mut t.h, &a), ["21", "22", "23", "24"]);
    assert_eq!(labels(&mut t.h, &b), ["21", "22", "23", "24"]);
    let d = t.h.doc();
    assert_eq!(d.components[&x1].properties.get("tag").map(String::as_str), Some("-X01"));
    assert_eq!(d.components[&x1].properties.get("tag_fixed").map(String::as_str), Some("yes"));
    // One strip by selection; the other keeps its labels.
    t.h.run("renumber", json!({"kind": "terminals", "scope": "selection", "strips": [x2.hex()]})).unwrap();
    assert_eq!(labels(&mut t.h, &a), ["21", "22", "23", "24"]);
    assert_eq!(labels(&mut t.h, &b), ["1", "2", "3", "4"]);
    // Devices and `all` leave terminal labels alone.
    for kind in ["devices", "all"] {
        t.h.run("renumber", json!({"kind": kind})).unwrap();
        assert_eq!(labels(&mut t.h, &a), ["21", "22", "23", "24"], "{kind}");
    }
    same_as_built(&mut t.h);
    t.h.check();
}

/// AC 3 with 026's provisional tags: with provisional tags on, a new strip from Place Terminals
/// gets a provisional tag like any new device (Place on a wire does); its terminals renumber;
/// Finalise gives the strip a final tag and leaves the labels alone; undo and redo of both.
#[test]
fn provisional_strip_tags_from_place_terminals_and_finalise() {
    let mut t = fixture(0x7028_000c);
    fence(&mut t.h, 170.0);
    t.h.run("numbering", json!({"provisional": true})).unwrap();
    // Control: Place on a wire makes a provisional strip.
    let v = place(&mut t.h, "terminal", (100.0, 190.0));
    let p_tag = v["tag"].as_str().unwrap_or_default().to_string();
    assert!(tag::is_provisional(&p_tag), "Place terminal on a wire in provisional mode: {v}");
    for x in [320.0, 330.0, 340.0] {
        t.h.run("wire", json!({"points": pts(&[(x, 270.0), (x, 160.0)])})).unwrap();
    }
    let v = t.h.run("terminals", json!({"from": [310, 200], "to": [350, 200]})).unwrap();
    let x3: ComponentId = id(&v, "strip");
    let c: Vec<TerminalId> = v["terminals"].as_array().unwrap().iter().map(|x| id(x, "terminal")).collect();
    let tag3 = t.h.doc().components[&x3].properties.get("tag").cloned().unwrap_or_default();
    assert!(tag::is_provisional(&tag3), "a strip placed in provisional mode: {tag3}");
    t.h.run("renumber", json!({"kind": "terminals", "scope": "selection", "strips": [c[0].hex()], "start": 5})).unwrap();
    assert_eq!(labels(&mut t.h, &c), ["5", "6", "7"]);
    t.h.run("finalise", json!({})).unwrap();
    assert_eq!(labels(&mut t.h, &c), ["5", "6", "7"], "Finalise does not touch terminal labels");
    let tag3 = t.h.doc().components[&x3].properties.get("tag").cloned().unwrap_or_default();
    assert!(!tag::is_provisional(&tag3), "{tag3}");
    // Undo the finalise and the renumber, redo both.
    t.h.cmdline("u");
    t.h.cmdline("u");
    assert_eq!(labels(&mut t.h, &c), ["1", "2", "3"]);
    t.h.run("redo", Value::Null).unwrap();
    t.h.run("redo", Value::Null).unwrap();
    assert_eq!(labels(&mut t.h, &c), ["5", "6", "7"]);
    same_as_built(&mut t.h);
    t.h.check();
}

/// AC 3: swapping two terminals in drawing order (3 ↔ 4 by moving), renumber exchanges their
/// labels; wires and cable cores stay with their conductors; the plan's rows follow.
#[test]
fn a_swap_by_renumber_keeps_wires_and_cores() {
    let mut t = fixture(0x7028_0005);
    let (x1, terms) = fence(&mut t.h, 170.0);
    let doc = t.h.doc();
    let item_of = |d: &Document, term: TerminalId| {
        d.representations_of(ecad_model::Subject::Component(x1))
            .find(|(_, r)| r.connection_points.contains_key(&term))
            .map(|(_, r)| r.items[0])
            .unwrap()
    };
    let (i3, i4) = (item_of(&doc, terms[2]), item_of(&doc, terms[3]));
    let (h3, h4) = (t.h.handle_of(t.page, i3), t.h.handle_of(t.page, i4));
    // Exchange their places: 3 right by 50, 4 left by 50 (wires follow).
    t.h.run("move", json!({"handles": [h3.hex()], "delta": [60, 0]})).unwrap();
    t.h.run("move", json!({"handles": [h4.hex()], "delta": [-60, 0]})).unwrap();
    let members_before = t.h.doc().cables[&t.cable].members.clone();
    let ends: Vec<_> = terms.iter().map(|x| t.h.doc().ends_at(*x)).collect();
    t.h.run("renumber", json!({"kind": "terminals"})).unwrap();
    assert_eq!(labels(&mut t.h, &terms), ["1", "2", "4", "3"]);
    let d = t.h.doc();
    assert_eq!(terms.iter().map(|x| d.ends_at(*x)).collect::<Vec<_>>(), ends, "wires stay attached");
    assert_eq!(d.cables[&t.cable].members, members_before, "cores unchanged");
    let plan = t.h.run("stripplan", json!({})).unwrap();
    let rows = plan["strips"][0]["rows"].as_array().unwrap();
    assert_eq!(rows[2]["terminal"], json!(terms[3].hex()), "row 3 is the moved terminal: {plan}");
    same_as_built(&mut t.h);
    t.h.check();
}

/// The update maps (AC 9, design §11): with a terminal plan page on screen, an edit that touches
/// no strip (a coil's part number far away) neither recomputes a strip nor rebuilds the page; an
/// edit to the strip (its offer) does both, once. A cable change touches only the strips its
/// members land on.
#[test]
fn generated_pages_and_strips_follow_only_related_edits() {
    let mut t = fixture(0x7028_0006);
    let (x1, _) = fence(&mut t.h, 170.0);
    // A second strip on wires no cable crosses.
    for x in [320.0, 330.0] {
        t.h.run("wire", json!({"points": pts(&[(x, 270.0), (x, 160.0)])})).unwrap();
    }
    let v = t.h.run("terminals", json!({"from": [310, 200], "to": [340, 200]})).unwrap();
    let x2: ComponentId = id(&v, "strip");
    let far: ComponentId = id(&place(&mut t.h, "coil", (370.0, 260.0)), "component");
    t.h.cfg(|p| {
        p.outputs();
    });
    let plan_page: PageId = id(&t.h.run("newpage", json!({"generated": "terminals"})).unwrap(), "page");
    t.h.activate(plan_page);
    let builds = t.h.p(|p| p.frame_builds);
    t.h.activate(plan_page);
    assert_eq!(t.h.p(|p| p.frame_builds), builds, "control: showing the page again with no edit rebuilds nothing");
    let x2_plan = |h: &mut H| h.run("stripplan", json!({"strip": x2.hex()})).unwrap();
    let x2_before = x2_plan(&mut t.h);

    // Unrelated: a part number of a coil that no strip touches.
    t.h.run("setprop", json!({"component": far.hex(), "key": "part_number", "value": "K-99"})).unwrap();
    let strips_work = t.h.p(|p| p.outputs.as_ref().unwrap().strips.last_work);
    assert_eq!(strips_work, 0, "an unrelated edit recomputes no strip");
    t.h.activate(plan_page);
    let unrelated = t.h.p(|p| p.frame_builds) - builds;

    // Related: the strip's offer adds spare rows on the page.
    let builds = t.h.p(|p| p.frame_builds);
    t.h.run("setprop", json!({"component": x1.hex(), "key": "terminals", "value": "10"})).unwrap();
    assert_eq!(t.h.p(|p| p.outputs.as_ref().unwrap().strips.last_work), 1, "only -X01");
    t.h.activate(plan_page);
    assert_eq!(t.h.p(|p| p.frame_builds) - builds, 1, "the plan page rebuilds once for its strip");

    // A cable change: only -X01 (on -W01's wires), never -X02.
    t.h.run("setprop", json!({"cable": t.cable.hex(), "key": "cores", "value": "4G"})).unwrap();
    assert_eq!(t.h.p(|p| p.outputs.as_ref().unwrap().strips.last_work), 1, "only the strip on the cable's members");
    assert_eq!(x2_plan(&mut t.h), x2_before, "-X02's rows unchanged");
    same_as_built(&mut t.h);
    t.h.check();
    assert_eq!(unrelated, 0, "an unrelated edit rebuilt the terminal plan page {unrelated} time(s)");
}

/// The same for the cable schedule page: an edit away from cables leaves it; a core change on
/// `-W01` rebuilds it once.
#[test]
fn the_cable_schedule_page_follows_only_cable_edits() {
    let mut t = fixture(0x7028_0007);
    let far: ComponentId = id(&place(&mut t.h, "coil", (370.0, 260.0)), "component");
    t.h.cfg(|p| {
        p.outputs();
    });
    let page: PageId = id(&t.h.run("newpage", json!({"generated": "cables"})).unwrap(), "page");
    t.h.activate(page);
    let builds = t.h.p(|p| p.frame_builds);
    t.h.activate(page);
    assert_eq!(t.h.p(|p| p.frame_builds), builds, "control: no edit, no rebuild");
    t.h.run("setprop", json!({"component": far.hex(), "key": "part_number", "value": "K-99"})).unwrap();
    t.h.activate(page);
    let unrelated = t.h.p(|p| p.frame_builds) - builds;
    let builds = t.h.p(|p| p.frame_builds);
    t.h.run("setprop", json!({"cable": t.cable.hex(), "key": "cores", "value": "4G"})).unwrap();
    t.h.activate(page);
    assert_eq!(t.h.p(|p| p.frame_builds) - builds, 1, "a core change rebuilds the schedule page once");
    t.h.check();
    assert_eq!(unrelated, 0, "an unrelated edit rebuilt the cable schedule page {unrelated} time(s)");
}

/// AC 10, second half: a project written without any 028 property (an 018-style strip joined by
/// clicking, no offer, no cores, no generated page) opens with no transaction, shows its strip in
/// the plan with no spares, and leaves its log as it was.
#[test]
fn an_old_project_opens_unchanged_and_shows_its_strips() {
    let dir = tmp_dir("old");
    let mut h = H::new(0x7028_0008);
    let x1: ComponentId = id(&place(&mut h, "terminal", (300.0, 250.0)), "component");
    h.run("place", json!({"symbol": "terminal", "at": [310, 250], "device": x1.hex()})).unwrap();
    let k: ComponentId = id(&place(&mut h, "coil", (300.0, 200.0)), "component");
    let _ = k;
    h.cfg(|p| p.persistence = Persistence::File);
    let path = dir.join("old 018.ecad").display().to_string();
    h.run("saveas", json!({"path": path})).unwrap();
    let d = h.doc();
    for key in ["cores", "terminals", "part_number.pe", "maker.pe", "strip", "generated"] {
        assert!(d.components.values().all(|c| !c.properties.contains_key(key)), "an 018-style project stores no {key}");
        assert!(d.pages.values().all(|p| !p.properties.contains_key(key)), "an 018-style project stores no page {key}");
    }
    let bytes = std::fs::read(&path).unwrap();
    let head = h.head();
    close_all(&mut h);
    drop(h);
    let mut h2 = reopen(&path, 0x7028_0009);
    assert_eq!(h2.head(), head);
    let plan = h2.run("stripplan", json!({})).unwrap();
    let s = &plan["strips"][0];
    assert_eq!(s["tag"], "-X01", "{plan}");
    assert_eq!(s["rows"].as_array().unwrap().len(), 2, "{plan}");
    assert_eq!(s["spare"], 0, "{plan}");
    assert_eq!(h2.head(), head, "reads record nothing");
    h2.check();
    close_all(&mut h2);
    drop(h2);
    let _ = bytes;
    let mut again = reopen(&path, 0x7028_000d);
    assert_eq!(again.head(), head, "the log is unchanged by open, read and close");
    close_all(&mut again);
    drop(again);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Hostile input beyond the worker's: refusals with a way forward, never a panic, nothing written.
#[test]
fn hostile_strip_input_is_refused_or_checked_without_panic() {
    let mut t = fixture(0x7028_000b);
    let (x1, terms) = fence(&mut t.h, 170.0);
    let head = t.h.head();
    let refused: Vec<(&str, Value)> = vec![
        ("setprop", json!({"component": x1.hex(), "key": "terminals", "value": "1000"})),
        ("setprop", json!({"component": x1.hex(), "key": "terminals", "value": "-1"})),
        ("setprop", json!({"component": x1.hex(), "key": "terminals", "value": "18446744073709551616"})),
        ("setprop", json!({"component": x1.hex(), "key": "terminals", "value": "2PE+3"})),
        ("setprop", json!({"cable": t.cable.hex(), "key": "cores", "value": "101"})),
        ("setprop", json!({"cable": t.cable.hex(), "key": "cores", "value": "0"})),
        ("setprop", json!({"cable": t.cable.hex(), "key": "cores", "value": "99999999999999999999G"})),
        ("setprop", json!({"cable": t.cable.hex(), "key": "cores", "value": ",".repeat(5000)})),
        ("setprop", json!({"cable": t.cable.hex(), "key": "cores", "value": "A,".repeat(200)})),
        ("renumber", json!({"kind": "terminals", "start": u64::MAX})),
        ("bridge", json!({"from": terms[0].hex(), "to": "ffffffffffffffffffffffffffffffff"})),
        ("bridge", json!({"from": "zz", "to": terms[1].hex()})),
        ("terminals", json!({"from": [90, 170], "to": [f64::MAX, 170]})),
        ("terminals", json!({"from": [90, 170]})),
        ("stripplan", json!({"strip": "nothex"})),
        ("cablecores", json!({"cable": x1.hex()})),
    ];
    for (cmd, params) in refused {
        let r = t.h.run(cmd, params.clone());
        assert!(r.is_err(), "{cmd} {params}: expected a refusal, got {r:?}");
        assert!(!r.unwrap_err().is_empty());
        assert_eq!(t.h.head(), head, "{cmd} {params}: nothing written");
    }
    // Accepted at the edge: 999 positions, 100 cores; reads with odd limits.
    t.h.run("setprop", json!({"component": x1.hex(), "key": "terminals", "value": "999"})).unwrap();
    let p = t.h.run("stripplan", json!({"limit": 5000})).unwrap();
    assert_eq!(p["strips"][0]["rows"].as_array().map(Vec::len), Some(999), "{}", p["strips"][0]["spare"]);
    let p = t.h.run("stripplan", json!({"limit": 0})).unwrap_or_else(|e| json!({"err": e}));
    let _ = p;
    let r = t.h.run("stripplan", json!({"limit": 9_999_999}));
    if let Ok(v) = r {
        let n: usize = v["strips"].as_array().unwrap().iter().map(|s| s["rows"].as_array().map_or(0, Vec::len)).sum();
        assert!(n <= 5000, "limit is capped at 5000, got {n} rows");
    }
    t.h.run("setprop", json!({"cable": t.cable.hex(), "key": "cores", "value": "100"})).unwrap();
    let cc = t.h.run("cablecores", json!({"cable": t.cable.hex()})).unwrap();
    assert_eq!(cc["cores"].as_array().unwrap().len(), 100);
    // A bridge from a terminal to a coil's connection point: a plain wire, not a bridge.
    let k2_terms: Vec<TerminalId> = t.h.doc().terminals_of(t.k2).map(|(x, _)| x).collect();
    let r = t.h.run("bridge", json!({"from": terms[0].hex(), "to": k2_terms[0].hex()}));
    if let Ok(v) = &r {
        assert_eq!(v["bridge"], false, "{v}");
    }
    same_as_built(&mut t.h);
    t.h.check();
    let _ = t.wires;
}
