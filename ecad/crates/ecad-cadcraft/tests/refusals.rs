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
    let lines = h.lines_on(h.first_page());
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

fn before(h: &H) -> Before {
    let page = h.first_page();
    let p = h.p();
    Before {
        head: p.ledger.head(),
        doc: p.ledger.document().clone(),
        map: p.pages[&page].map.clone(),
        unsaved: p.ledger.is_unsaved(),
        model: p.drawing_of(&h.s, page).unwrap().space(&Space::Model).unwrap().clone(),
    }
}

fn unchanged(h: &H, b: &Before, what: &str) {
    let page = h.first_page();
    let p = h.p();
    assert_eq!(p.ledger.head(), b.head, "{what}: nothing appended");
    assert_eq!(p.ledger.document(), &b.doc, "{what}: document unchanged");
    assert_eq!(p.pages[&page].map, b.map, "{what}: map unchanged");
    assert_eq!(p.ledger.is_unsaved(), b.unsaved, "{what}: unsaved state unchanged");
    assert!(p.drawing_of(&h.s, page).unwrap().space(&Space::Model).unwrap() == &b.model, "{what}: drawing restored");
}

#[test]
fn escape_during_move_records_nothing() {
    let (mut h, ..) = fixture();
    let b = before(&h);
    h.cmdline("move all  0,0");
    assert!(h.s.running.is_some());
    h.s.cancel();
    h.check();
    unchanged(&h, &b, "Esc during MOVE");
}

#[test]
fn escape_during_line_keeps_the_segments_drawn() {
    let (mut h, ..) = fixture();
    let head = h.head();
    h.cmdline("line 0,50 10,50 10,60");
    assert!(h.s.running.is_some());
    h.s.cancel();
    h.check();
    assert_eq!(h.head(), head + 1, "one transaction for the segments CADCraft kept");
    let tx = h.p().ledger.log().last().cloned().unwrap();
    assert_eq!(tx.ops.iter().filter(|o| o.name() == "AddItem").count(), 2);
}

#[test]
fn replacing_or_unsupported_edits_are_refused() {
    let (mut h, _c1, c2, w) = fixture();
    let page = h.first_page();
    let cases: Vec<(&str, Value)> = vec![
        ("explode", json!({"handles": [h.handle_of(page, h.symbol_item(c2)).hex()]})),
        ("break", json!({"handle": h.route_handle(page, w).hex(), "p1": [60, 10], "p2": [70, 10]})),
        ("hatch", json!({"points": [[0, 40], [10, 40], [10, 50]]})),
        ("layer.new", json!({"name": "NEW"})),
    ];
    for (cmd, params) in cases {
        let b = before(&h);
        let r = h.run(cmd, params);
        h.check();
        unchanged(&h, &b, cmd);
        assert!(r.is_err(), "{cmd} must report the refusal");
    }
    assert!(h.p().events.count(EventKind::Refused) >= 3);
}

#[test]
fn connecting_to_a_deleted_terminal_fails_cleanly() {
    let (mut h, c1, _c2, w) = fixture();
    let t = h.doc().terminals_of(c1).next().map(|(t, _)| t).unwrap();
    h.run("ecaddelete", json!({"component": c1.hex()})).unwrap();
    let b = before(&h);
    let r = h.run("connect", json!({"conductor": w.hex(), "end": "A", "terminal": t.hex()}));
    assert!(r.is_err());
    unchanged(&h, &b, "connect to a deleted terminal");
    assert_eq!(h.doc().conductors[&w].ends[0], None, "deleting the component detached the wire explicitly");
}

#[cfg(feature = "fault-injection")]
#[test]
fn injected_panics_are_contained() {
    let (mut h, c1, ..) = fixture();
    // In an ECAD command, before it commits.
    let b = before(&h);
    h.p().faults.panic_in_command = true;
    assert!(h.run("setprop", json!({"component": c1.hex(), "key": "tag", "value": "-K1"})).is_err());
    unchanged(&h, &b, "panic in an ECAD command");
    h.run("setprop", json!({"component": c1.hex(), "key": "tag", "value": "-K1"})).unwrap();
    // In the owner's commit callback.
    let b = before(&h);
    h.p().faults.panic_in_commit = true;
    assert!(h.run("line", json!({"points": [[0, 80], [10, 80]]})).is_err());
    h.check();
    // Recovery rebuilds every page from the document with fresh handles (PROJ-02), so the map
    // changes; the document, history and unsaved state do not.
    assert_eq!(h.head(), b.head, "panic in the commit callback: nothing appended");
    assert_eq!(h.doc(), b.doc, "panic in the commit callback: document unchanged");
    assert_eq!(h.p().events.count(EventKind::Recovered), 1);
    h.run("line", json!({"points": [[0, 80], [10, 80]]})).unwrap();
}

#[test]
fn ecad_edits_wait_for_a_running_command() {
    let (mut h, c1, ..) = fixture();
    h.cmdline("move all  0,0");
    let b = before(&h);
    let r = h.run("setprop", json!({"component": c1.hex(), "key": "tag", "value": "-K1"}));
    assert!(r.unwrap_err().contains("TX-05"));
    h.s.cancel();
    unchanged(&h, &b, "SETPROP during MOVE");
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
    h.p().store.medium.fail_next_append = true;
    app.answer_close(Choice::Save);
    assert_eq!(app.session.docs.len(), 1, "save failed: the project stays open");
    assert!(app.session.has_unsaved_work());
    app.request_close(Some(0));
    app.answer_close(Choice::Save);
    assert!(app.session.docs.is_empty(), "saved, then closed");
    assert!(!h.p().ledger.is_unsaved());
    assert!(app.session.ext.owners().is_empty());
}

#[test]
fn reopening_with_the_creation_seed_never_reissues_ids() {
    let mut h = H::new(4242);
    h.run("newpage", json!({"title": "2"})).unwrap();
    h.run("qsave", Value::Null).unwrap();
    let medium = h.p().store.medium.clone();
    let mut s = cadcraft_engine::Session::empty();
    let reopened =
        ecad_cadcraft::Project::open_from(&mut s, "Demo", ecad_store::TestStore::new(medium), 4242, false).unwrap();
    let before = reopened.lock().unwrap().ledger.document().clone();
    s.execute("newpage", &json!({"title": "3"})).unwrap();
    s.execute("wire", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    let after = reopened.lock().unwrap().ledger.document().clone();
    assert_eq!(after.pages.len(), before.pages.len() + 1, "a new page, not a duplicate of an existing ID");
    assert!(before.pages.keys().all(|p| after.pages.contains_key(p)));
}

#[test]
fn oversized_inputs_are_refused_before_collecting() {
    let (mut h, ..) = fixture();
    let many: Vec<String> = (0..10_001).map(|i| format!("{i:032x}")).collect();
    let b = before(&h);
    assert!(h.run("cableadd", json!({"conductors": many})).unwrap_err().contains("at most"));
    unchanged(&h, &b, "oversized cableadd");
}
