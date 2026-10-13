//! Refused and cancelled cases (proposal §17.3): every refusal leaves the ledger, the map and the
//! unsaved state as they were, and restores the drawing to the last projection.

mod common;

use cadcraft_engine::doc::Space;
use common::{H, id, pts};
use ecad_inspect::EventKind;
use ecad_model::{ComponentId, ConductorId};
use serde_json::{Value, json};

/// A small project: a drawn component with a terminal, a placed symbol, a wire between them.
fn fixture() -> (H, ComponentId, ComponentId, ConductorId) {
    let mut h = H::new(77);
    h.cmdline("line 0,0 40,0 40,20 0,20 c");
    let first = h.first_page();
    let lines = h.lines_on(first);
    let c1: ComponentId = id(
        &h.run("convertcomponent", json!({"handles": lines.iter().map(|x| x.hex()).collect::<Vec<_>>()})).unwrap(),
        "component",
    );
    h.run("addterminal", json!({"handle": lines[1].hex(), "at": [40, 10]})).unwrap();
    let c2: ComponentId = id(&h.run("place", json!({"at": [100, 0]})).unwrap(), "component");
    let w: ConductorId =
        id(&h.run("wire", json!({"points": pts(&[(40.0, 10.0), (100.0, 10.0)])})).unwrap(), "conductor");
    (h, c1, c2, w)
}

struct Before {
    head: u64,
    doc: ecad_model::Document,
    map: ecad_cadcraft::HandleMap,
    unsaved: bool,
    model: cadcraft_engine::doc::EntityStore,
}

fn before(h: &mut H) -> Before {
    let page = h.first_page();
    h.pv(|p, pg| Before {
        head: p.ledger.head(),
        doc: p.ledger.document().clone(),
        map: p.pages[&page].map.clone(),
        unsaved: p.ledger.is_unsaved(),
        model: pg.drawing_of(p, page).unwrap().space(&Space::Model).unwrap().clone(),
    })
}

fn unchanged(h: &mut H, b: &Before, what: &str) {
    let page = h.first_page();
    h.pv(|p, pg| {
        assert_eq!(p.ledger.head(), b.head, "{what}: nothing appended");
        assert_eq!(p.ledger.document(), &b.doc, "{what}: document unchanged");
        assert_eq!(p.pages[&page].map, b.map, "{what}: map unchanged");
        assert_eq!(p.ledger.is_unsaved(), b.unsaved, "{what}: unsaved state unchanged");
        assert!(pg.drawing_of(p, page).unwrap().space(&Space::Model).unwrap() == &b.model, "{what}: drawing restored");
    });
}

#[test]
fn escape_during_move_records_nothing() {
    let (mut h, ..) = fixture();
    let b = before(&mut h);
    h.cmdline("move all  0,0");
    assert!(h.s.running.is_some());
    h.s.cancel();
    h.check();
    unchanged(&mut h, &b, "Esc during MOVE");
}

#[test]
fn escape_during_line_records_the_segments_kept() {
    let (mut h, ..) = fixture();
    // Esc before any segment: nothing accepted, no transaction (§20.4).
    let b = before(&mut h);
    h.cmdline("line 0,50");
    assert!(h.s.running.is_some());
    h.s.cancel();
    h.check();
    unchanged(&mut h, &b, "Esc during LINE with no segment");
    // Esc after two segments: CADCraft keeps them, and they are one transaction.
    let head = h.head();
    h.cmdline("line 0,50 10,50 10,60");
    assert!(h.s.running.is_some());
    h.s.cancel();
    h.check();
    assert_eq!(h.head(), head + 1, "one transaction for the segments CADCraft kept");
    let tx = h.p(|p| p.ledger.log().last().cloned().unwrap());
    assert_eq!(tx.ops.iter().filter(|o| o.name() == "AddItem").count(), 2);
    // The kept segments are saved and reopened as that transaction.
    h.run("qsave", Value::Null).unwrap();
    let medium = h.p(|p| p.store.medium().clone());
    let mut s = cadcraft_engine::Session::empty();
    let reopened = ecad_cadcraft::ProjectHandle::open_from(&mut s, "Demo", medium, 77).unwrap();
    assert_eq!(reopened.read(&mut s, |p, _| p.ledger.log().last().cloned()).unwrap(), Some(tx));
}

#[test]
fn replacing_or_unsupported_edits_are_refused() {
    let (mut h, _c1, c2, w) = fixture();
    let page = h.first_page();
    let sym_item = h.symbol_item(c2);
    let sym_handle = h.handle_of(page, sym_item);
    let cases: Vec<(&str, Value)> = vec![
        ("explode", json!({"handles": [sym_handle.hex()]})),
        ("break", json!({"handle": h.route_handle(page, w).hex(), "p1": [60, 10], "p2": [70, 10]})),
        ("hatch", json!({"points": [[0, 40], [10, 40], [10, 50]]})),
        ("layer.new", json!({"name": "NEW"})),
    ];
    for (cmd, params) in cases {
        let b = before(&mut h);
        let r = h.run(cmd, params);
        h.check();
        unchanged(&mut h, &b, cmd);
        assert!(r.is_err(), "{cmd} must report the refusal");
    }
    assert!(h.p(|p| p.events.count(EventKind::Refused)) >= 3);
}

#[test]
fn connecting_to_a_deleted_terminal_fails_cleanly() {
    let (mut h, c1, _c2, w) = fixture();
    let t = h.doc().terminals_of(c1).next().map(|(t, _)| t).unwrap();
    h.run("ecaddelete", json!({"component": c1.hex()})).unwrap();
    let b = before(&mut h);
    let r = h.run("connect", json!({"conductor": w.hex(), "end": "A", "terminal": t.hex()}));
    assert!(r.is_err());
    unchanged(&mut h, &b, "connect to a deleted terminal");
    assert_eq!(h.doc().conductors[&w].ends[0], None, "deleting the component detached the wire explicitly");
}

#[cfg(feature = "fault-injection")]
#[test]
fn injected_panics_are_contained() {
    let (mut h, c1, ..) = fixture();
    // In an ECAD command, before it commits.
    let b = before(&mut h);
    h.cfg(|p| p.faults.panic_in_command = true);
    assert!(h.run("setprop", json!({"component": c1.hex(), "key": "tag", "value": "-K1"})).is_err());
    unchanged(&mut h, &b, "panic in an ECAD command");
    h.run("setprop", json!({"component": c1.hex(), "key": "tag", "value": "-K1"})).unwrap();
    // In the owner's commit callback.
    let b = before(&mut h);
    h.cfg(|p| p.faults.panic_in_commit = true);
    assert!(h.run("line", json!({"points": [[0, 80], [10, 80]]})).is_err());
    h.check();
    // Recovery rebuilds every page from the document with fresh handles (PROJ-02), so the map
    // changes; the document, history and unsaved state do not.
    assert_eq!(h.head(), b.head, "panic in the commit callback: nothing appended");
    assert_eq!(h.doc(), b.doc, "panic in the commit callback: document unchanged");
    assert_eq!(h.p(|p| p.events.count(EventKind::Recovered)), 1);
    h.run("line", json!({"points": [[0, 80], [10, 80]]})).unwrap();
}

#[test]
fn ecad_edits_wait_for_a_running_command() {
    let (mut h, c1, ..) = fixture();
    h.cmdline("move all  0,0");
    let b = before(&mut h);
    let r = h.run("setprop", json!({"component": c1.hex(), "key": "tag", "value": "-K1"}));
    assert!(r.unwrap_err().contains("TX-05"));
    h.s.cancel();
    unchanged(&mut h, &b, "SETPROP during MOVE");
}

#[test]
fn group_pick_moves_the_whole_component_and_its_wire_ends() {
    let (mut h, c1, _c2, w) = fixture();
    let page = h.first_page();
    let t = h.doc().terminals_of(c1).next().map(|(t, _)| t).unwrap();
    // A typed point at "Select objects" picks the rectangle's left edge; PICKSTYLE selects the group.
    h.cmdline("move 0,10  0,0 0,-5 ");
    assert!(h.s.running.is_none());
    h.check();
    assert!(
        h.terminal_pos(page, t).near(ecad_model::Pt::new(40.0, 5.0), 1e-9),
        "the terminal moved with the component"
    );
    assert!(h.route_ends(page, w).0.near(ecad_model::Pt::new(40.0, 5.0), 1e-9), "the wire end followed (M2)");
}

#[test]
fn drag_preview_shows_following_wire_ends() {
    let (mut h, c1, ..) = fixture();
    let page = h.first_page();
    let handles = h.rep_handles(page, c1);
    h.s.set_selection(handles.iter().filter_map(|x| cadcraft_engine::doc::Handle::parse_hex(x)).collect());
    h.cmdline("move 0,0");
    let (_, extra) = h.s.drag(cadcraft_engine::geom::Vec2::new(0.0, -5.0)).expect("MOVE drags");
    let routes: Vec<_> =
        extra.iter().filter(|e| matches!(e.kind, cadcraft_engine::doc::EntityKind::LwPolyline(_))).collect();
    assert_eq!(routes.len(), 1, "the attached wire is previewed following the component");
    if let cadcraft_engine::doc::EntityKind::LwPolyline(p) = &routes[0].kind {
        assert!((p.vertices[0].p.y - 5.0).abs() < 1e-9, "end A previewed at the dragged terminal");
    }
    h.s.cancel();
}

#[test]
fn closing_flow_prompts_for_the_project_not_for_page_tabs() {
    use cadcraft_ui_egui::closing::Choice;
    let (mut h, c1, ..) = fixture();
    h.run("newpage", json!({"title": "2"})).unwrap();
    let session = std::mem::replace(&mut h.s, cadcraft_engine::Session::empty());
    let mut app =
        cadcraft_ui_egui::CadApp::new(session, cadcraft_ui_egui::Services { pick_open: None, pick_save: None });
    // A project that was never saved has no file name: Save asks for one (Save As).
    app.session.active = 0;
    app.run("saveas", json!({"path": "demo.ecad"})).unwrap();
    app.run("setprop", json!({"component": c1.hex(), "key": "tag", "value": "-K9"})).unwrap();
    assert!(
        app.session.docs.iter().all(|d| d.path.as_deref() == Some("demo.ecad")),
        "page documents carry the project path"
    );
    assert_eq!(app.session.docs.len(), 2);
    // Closing one page tab: no prompt, the project stays open and unsaved.
    app.request_close(Some(1));
    assert!(app.close_prompt().is_none(), "a page tab closes without a prompt");
    assert_eq!(app.session.docs.len(), 1);
    assert!(app.session.has_unsaved_work());
    // Closing the last page closes the project: prompt; a failed save keeps it open.
    app.request_close(Some(0));
    assert!(app.close_prompt().is_some(), "closing the project prompts");
    h.project.configure(&mut app.session, |p| p.store.medium_mut().fail_next_append = true).unwrap();
    app.answer_close(Choice::Save);
    assert_eq!(app.session.docs.len(), 1, "save failed: the project stays open");
    assert!(app.session.has_unsaved_work());
    app.request_close(Some(0));
    app.answer_close(Choice::Save);
    assert!(app.session.docs.is_empty(), "saved, then closed");
    assert!(!h.p(|p| p.ledger.is_unsaved()));
    assert!(app.session.ext.owners().is_empty());
}

#[test]
fn reopening_with_the_creation_seed_never_reissues_ids() {
    let mut h = H::new(4242);
    h.run("newpage", json!({"title": "2"})).unwrap();
    h.run("qsave", Value::Null).unwrap();
    let medium = h.p(|p| p.store.medium().clone());
    let mut s = cadcraft_engine::Session::empty();
    let reopened = ecad_cadcraft::ProjectHandle::open_from(&mut s, "Demo", medium, 4242).unwrap();
    let before = reopened.read(&mut s, |p, _| p.ledger.document().clone()).unwrap();
    s.execute("newpage", &json!({"title": "3"})).unwrap();
    s.execute("wire", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    let after = reopened.read(&mut s, |p, _| p.ledger.document().clone()).unwrap();
    assert_eq!(after.pages.len(), before.pages.len() + 1, "a new page, not a duplicate of an existing ID");
    assert!(before.pages.keys().all(|p| after.pages.contains_key(p)));
}

#[test]
fn oversized_inputs_are_refused_before_collecting() {
    let (mut h, ..) = fixture();
    let many: Vec<String> = (0..10_001).map(|i| format!("{i:032x}")).collect();
    let b = before(&mut h);
    assert!(h.run("cableadd", json!({"conductors": many})).unwrap_err().contains("at most"));
    unchanged(&mut h, &b, "oversized cableadd");
}

#[test]
fn convert_refuses_malformed_or_oversized_handle_lists() {
    let mut h = H::new(91);
    h.cmdline("line 0,0 40,0 40,20 0,20 c");
    let first = h.first_page();
    let lines: Vec<String> = h.lines_on(first).iter().map(|x| x.hex()).collect();
    let head = h.head();
    let mut typo = lines.clone();
    typo[1] = "not-a-handle".into();
    for (what, params) in [
        ("typo", json!({"handles": typo})),
        ("not a string", json!({"handles": [lines[0], 7]})),
        ("not an array", json!({"handles": lines[0]})),
        ("too many", json!({"handles": vec!["1"; 100_001]})),
    ] {
        let e = h.run("convertcomponent", params).unwrap_err();
        assert!(e.contains("handle"), "{what}: {e}");
    }
    assert_eq!(h.head(), head, "nothing was converted");
    assert!(h.run("convertcomponent", json!({"handles": lines})).is_ok());
}

#[test]
fn session_seeds_differ_between_calls() {
    let seeds: std::collections::BTreeSet<u64> = (0..1000).map(|_| ecad_cadcraft::session_seed()).collect();
    assert_eq!(seeds.len(), 1000, "a seed repeated within one millisecond");
}

/// Malformed parameters are refused before anything is recorded; nothing is silently dropped,
/// truncated or read as "not given".
#[test]
fn malformed_parameters_are_refused_not_coerced() {
    let (mut h, c1, _c2, w) = fixture();
    let (c1, w) = (c1.hex(), w.hex());
    let many: Vec<Value> = (0..10_001).map(|i| json!([i, 0])).collect();
    let cases: Vec<(&str, &str, Value)> = vec![
        ("wire", "a malformed point", json!({"points": [[0, 0], "x", [10, 0]]})),
        ("wire", "a point with a text coordinate", json!({"points": [[0, 0], [10, "y"]]})),
        ("wire", "too many points", json!({"points": many})),
        ("setprop", "a missing value", json!({"component": c1, "key": "tag"})),
        ("setprop", "a numeric value", json!({"component": c1, "key": "tag", "value": 5})),
        ("setprop", "a malformed id", json!({"component": "zz", "key": "tag", "value": "-K1"})),
        ("cableadd", "a malformed member", json!({"conductors": [w, "nope"]})),
        ("cableadd", "a non-string member", json!({"conductors": [w, 7]})),
        ("cableadd", "a malformed cable", json!({"conductors": [w], "cable": "nope"})),
        ("cableadd", "a non-string tag", json!({"conductors": [w], "tag": 3})),
        ("place", "a malformed definition", json!({"at": [0, 50], "definition": "nope"})),
        ("place", "a text rotation", json!({"at": [0, 50], "rotation": "ninety"})),
        ("place", "a numeric mirror", json!({"at": [0, 50], "mirror": 1})),
    ];
    for (cmd, what, params) in cases {
        let b = before(&mut h);
        let r = h.run(cmd, params);
        assert!(r.is_err(), "{cmd} with {what} must be refused");
        unchanged(&mut h, &b, &format!("{cmd} with {what}"));
    }
    // `null` still clears a property on purpose.
    h.run("setprop", json!({"component": c1, "key": "tag", "value": "-K1"})).unwrap();
    assert_eq!(h.run("setprop", json!({"component": c1, "key": "tag", "value": null})).unwrap()["changed"], true);
}

#[test]
fn a_damaged_file_opens_its_verified_prefix_read_only() {
    let mut h = H::new(4242);
    h.run("newpage", json!({"title": "2"})).unwrap();
    h.run("qsave", Value::Null).unwrap();
    let saved = h.p(|p| p.ledger.log().to_vec());
    let mut medium = h.p(|p| p.store.medium().clone());
    // Damage the last complete record's payload: a complete record failing its checksum.
    let bytes = medium.external();
    let end = bytes.len() - 3;
    bytes[end] ^= 0x01;
    let original = bytes.clone();
    let mut s = cadcraft_engine::Session::empty();
    let reopened = ecad_cadcraft::ProjectHandle::open_from(&mut s, "Demo", medium, 4242).unwrap();
    let report = reopened.read(&mut s, |p, _| p.open_report.clone()).unwrap().unwrap();
    assert_eq!(report.access, ecad_file::Access::ReadOnly);
    assert!(matches!(report.outcome, ecad_file::Outcome::Corrupt(ref c) if c.check == ecad_file::Check::Crc));
    let log = reopened.read(&mut s, |p, _| p.ledger.log().to_vec()).unwrap();
    assert_eq!(log, saved[..saved.len() - 1], "the verified prefix, without the damaged record");
    let e = s.execute("qsave", &Value::Null).unwrap_err().to_string();
    assert!(e.contains("read-only"), "{e}");
    let after = reopened.read(&mut s, |p, _| ecad_store::Medium::read_all(p.store.medium()).unwrap()).unwrap();
    assert_eq!(after, original, "the source is preserved");
}
