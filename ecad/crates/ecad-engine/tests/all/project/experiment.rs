//! The bounded integration experiment, scenario S1-S16 (proposal §17.2). One project, two pages,
//! real CADCraft commands driven through `Session` as the UI and control channel drive it. The
//! checks of §17.4 run after every step (see `common::H::check`).

use crate::common;

use common::{H, hex, id, pts};
use ecad_engine::ProjectHandle;
use ecad_engine::history::event_log::EventKind;
use ecad_model::connectivity::derive::nets;
use ecad_model::{DeviceId, Document, PageId, Pt, RouteEnd, RouteId, Subject, ConnectionPointId, WireId};
use serde_json::{Value, json};

#[test]
fn scenario_s1_to_s16() {
    let mut h = H::new(0xEC4D);
    let p1 = h.first_page();

    // S1: raw geometry with real CADCraft commands.
    h.cmdline("line 0,0 40,0 40,20 0,20 c");
    h.run("text", json!({"at": [2, 22], "text": "K1", "height": 2.5})).unwrap();
    let lines = h.lines_on(p1);
    assert_eq!(lines.len(), 4, "LINE drew the rectangle");
    assert_eq!(h.doc().items.len(), 5);
    assert!(h.doc().representations.is_empty(), "plain geometry has no meaning (DOC-03)");

    // S2: convert to a component, add three terminals.
    let r = h.run("convertcomponent", json!({"handles": lines.iter().map(|x| x.hex()).collect::<Vec<_>>()})).unwrap();
    let c1: DeviceId = id(&r, "component");
    let mut t = Vec::new();
    for y in [15.0, 10.0, 5.0] {
        let r = h.run("addterminal", json!({"handle": lines[1].hex(), "at": [40, y]})).unwrap();
        t.push(id::<ConnectionPointId>(&r, "terminal"));
    }
    let (t1, t2, t3) = (t[0], t[1], t[2]);
    assert_eq!(h.doc().connection_points_of(c1).count(), 3);

    // S3: place a symbol rotated 90°.
    let r = h.run("place", json!({"at": [100, 10], "rotation": 90})).unwrap();
    let c2: DeviceId = id(&r, "component");
    let keys: Vec<(String, ConnectionPointId)> =
        r["terminals"].as_array().unwrap().iter().map(|x| (x["key"].as_str().unwrap().to_string(), id(x, "terminal"))).collect();
    let (s1, s2) = (keys[0].1, keys[1].1);
    assert!(h.connection_point_pos(p1, s1).near(Pt::new(90.0, 10.0), 1e-9), "A1 at (90,10): placement applied");
    assert!(h.connection_point_pos(p1, s2).near(Pt::new(110.0, 10.0), 1e-9));

    // S4: three lines: one from t1 to s1 (a wire), two from t2 and t3 into free space (routes
    // with one connection point carry no wire, design 072 §4.2).
    let r = h.run("wire", json!({"points": pts(&[(40.0, 15.0), (65.0, 15.0), (65.0, 10.0), (90.0, 10.0)])})).unwrap();
    let w1: WireId = id(&r, "conductor");
    let r2: RouteId = id(&h.run("wire", json!({"points": pts(&[(40.0, 10.0), (55.0, -10.0)])})).unwrap(), "route");
    let r3: RouteId = id(&h.run("wire", json!({"points": pts(&[(40.0, 5.0), (50.0, -20.0)])})).unwrap(), "route");
    let doc = h.doc();
    assert_eq!(doc.wires[&w1].ends, [t1, s1]);
    assert_eq!(doc.wires.len(), 1, "a line to nowhere is no wire");
    assert_eq!(doc.routes[&r2].ends, [RouteEnd::Point(t2), RouteEnd::Free]);
    assert_eq!(doc.routes[&r3].ends, [RouteEnd::Point(t3), RouteEnd::Free]);
    let ends = |h: &mut H, r: RouteId| h.doc().routes[&r].end_points().unwrap();

    // S5: reroute w1 by a vertex grip (M1).
    let nets_before = nets(&h.doc());
    let head = h.head();
    let route = h.route_handle(p1, w1);
    h.run("grip.stretch", json!({"handle": route.hex(), "index": 2, "to": [70, 18]})).unwrap();
    assert_eq!(h.head(), head + 1);
    let tx = h.p(|p| p.ledger.log().last().cloned().unwrap());
    assert!(
        tx.ops.iter().all(|o| matches!(o.name(), "ModifyItem" | "ModifyRoute")),
        "a reroute records geometry only: {:?}",
        tx.ops.iter().map(|o| o.name()).collect::<Vec<_>>()
    );
    assert_eq!(nets(&h.doc()), nets_before);

    // S6: move the drawn component (as its group pick selects it) and rotate the symbol (M2).
    let handles = h.rep_handles(p1, c1);
    assert_eq!(handles.len(), 7, "four lines and three terminal markers");
    h.run("move", json!({"handles": handles, "delta": [0, -5]})).unwrap();
    assert!(h.connection_point_pos(p1, t1).near(Pt::new(40.0, 10.0), 1e-9));
    assert!(h.route_ends(p1, w1).0.near(Pt::new(40.0, 10.0), 1e-9), "w1 end A followed t1");
    assert!(ends(&mut h, r2)[0].near(Pt::new(40.0, 5.0), 1e-9), "r2 end A followed t2");
    assert!(ends(&mut h, r2)[1].near(Pt::new(55.0, -10.0), 1e-9), "r2's free end stayed");
    let sym_item = h.symbol_item(c2);
    let sym = h.handle_of(p1, sym_item);
    h.run("rotate", json!({"handles": [sym.hex()], "base": [100, 10], "angle": 180})).unwrap();
    assert!(h.connection_point_pos(p1, s1).near(Pt::new(110.0, 10.0), 1e-6));
    assert!(h.route_ends(p1, w1).1.near(h.connection_point_pos(p1, s1), 1e-9), "w1 end B followed s1");
    assert_eq!(nets(&h.doc()), nets_before, "moving equipment does not change connectivity");

    // S7: deliberate reconnection of r3's free end: onto s2, then into free space, then back (M3).
    let s2_at = h.connection_point_pos(p1, s2);
    let route = h.line_handle(p1, r3);
    h.run("grip.stretch", json!({"handle": route.hex(), "index": 2, "to": [s2_at.x, s2_at.y]})).unwrap();
    assert_eq!(h.doc().routes[&r3].ends, [RouteEnd::Point(t3), RouteEnd::Point(s2)], "landing on s2 attached it");
    assert!(ecad_engine::wires::wire_between(&h.doc(), t3, s2).is_some(), "and made the wire t3-s2");
    let route = h.line_handle(p1, r3);
    h.run("grip.stretch", json!({"handle": route.hex(), "index": 2, "to": [80, -20]})).unwrap();
    assert_eq!(h.doc().routes[&r3].ends, [RouteEnd::Point(t3), RouteEnd::Free], "dragging into free space detached it");
    assert!(ecad_engine::wires::wire_between(&h.doc(), t3, s2).is_none(), "and took the wire away");
    assert_eq!(h.doc().routes[&r2].ends, [RouteEnd::Point(t2), RouteEnd::Free], "r2 untouched");
    let route = h.line_handle(p1, r3);
    h.run("grip.stretch", json!({"handle": route.hex(), "index": 2, "to": [s2_at.x, s2_at.y]})).unwrap();
    let w3 = ecad_engine::wires::wire_between(&h.doc(), t3, s2).expect("t3-s2 wired again");

    // S8: COPY c1 together with r2: an independent copy with preserved internal relationships.
    let before = h.doc();
    let mut sel = h.rep_handles(p1, c1);
    sel.push(h.line_handle(p1, r2).hex());
    h.run("copy", json!({"handles": sel, "delta": [0, -60]})).unwrap();
    let after = h.doc();
    assert_eq!(after.devices.len(), before.devices.len() + 1);
    let c1b = *after.devices.keys().find(|c| !before.devices.contains_key(c)).unwrap();
    let new_terms: Vec<ConnectionPointId> = after.connection_points_of(c1b).map(|(t, _)| t).collect();
    assert_eq!(new_terms.len(), 3);
    assert!(new_terms.iter().all(|t| !before.terminals.contains_key(t)), "new terminal IDs");
    let r2b = *after.routes.keys().find(|r| !before.routes.contains_key(r)).unwrap();
    let t2b = after.routes[&r2b].ends[0].point().expect("the copied line is attached");
    assert_eq!(after.terminals[&t2b].device().unwrap(), c1b, "attached to the copied component, not the original");
    assert!(h.connection_point_pos(p1, t2b).near(Pt::new(40.0, -55.0), 1e-9));
    assert_eq!(after.routes[&r2b].ends[1], RouteEnd::Free);
    for k in before.devices.keys() {
        assert_eq!(before.devices[k], after.devices[k], "originals unchanged");
    }
    assert_eq!(before.routes[&r2], after.routes[&r2]);
    assert_eq!(before.wires, after.wires, "a copied line to nowhere makes no wire");
    // COPYCLIP / PASTECLIP the symbol.
    let sym_item = h.symbol_item(c2);
    let sym = h.handle_of(p1, sym_item);
    h.run("copyclip", json!({"handles": [sym.hex()]})).unwrap();
    let base = h.s.clipboard_base;
    let before = h.doc();
    h.run("pasteclip", json!({"at": [base.x, base.y - 60.0]})).unwrap();
    let after = h.doc();
    let c2b = *after.devices.keys().find(|c| !before.devices.contains_key(c)).unwrap();
    assert_eq!(after.devices[&c2b].definition, after.devices[&c2].definition, "same embedded definition");
    let keys_b: Vec<_> = after.connection_points_of(c2b).map(|(_, t)| t.key.clone()).collect();
    assert_eq!(keys_b.len(), 2);
    assert!(keys_b.contains(&Some(ecad_model::PointKey::def("A1"))) && keys_b.contains(&Some(ecad_model::PointKey::def("A2"))));

    // S9: ERASE w1's line removes its route, and the wire goes with it (design 072 §4.4); erasing
    // r2's line leaves t2 with no route.
    let r1 = h.route_of(p1, w1);
    let route = h.route_handle(p1, w1);
    h.run("erase", json!({"handles": [route.hex()]})).unwrap();
    let doc = h.doc();
    assert!(!doc.routes.contains_key(&r1), "the line's route is erased with it");
    assert!(!doc.wires.contains_key(&w1), "t1 and s1 are no longer joined");
    assert_eq!(doc.representations_of(Subject::Route(r1)).count(), 0, "nothing shows it");
    let route = h.line_handle(p1, r2);
    h.run("erase", json!({"handles": [route.hex()]})).unwrap();
    let doc = h.doc();
    assert!(!doc.routes.contains_key(&r2));
    assert_eq!(doc.routes_at(t2).count(), 0);

    // S10: a second page; a cable spanning both pages; a non-graphical edit.
    let r = h.run("newpage", json!({"title": "2"})).unwrap();
    let p2: PageId = id(&r, "page");
    h.activate(p2);
    h.run("line", json!({"points": [[0, 0], [10, 10]]})).unwrap();
    let r = h.run("place", json!({"at": [100, 10], "rotation": 90})).unwrap();
    let ends4: Vec<ConnectionPointId> = r["terminals"].as_array().unwrap().iter().map(|x| id(x, "terminal")).collect();
    let (a4, b4) = (h.connection_point_pos(p2, ends4[0]), h.connection_point_pos(p2, ends4[1]));
    let w4: WireId =
        id(&h.run("wire", json!({"points": pts(&[(a4.x, a4.y), (a4.x, a4.y + 20.0), (b4.x, b4.y + 20.0), (b4.x, b4.y)])})).unwrap(), "conductor");
    let cable = h.run("cableadd", json!({"conductors": [w2_hex(w3), w2_hex(w4)], "tag": "-W1"})).unwrap();
    h.run("setprop", json!({"component": c1b.hex(), "key": "tag", "value": "-K2"})).unwrap();
    let i2 = h.page_index(p2).unwrap();
    assert!(h.s.doc_unsaved(i2), "the page window shows unsaved");
    assert_eq!(h.page_index(p1), None, "one page window: page 1 is not shown");
    assert!(h.p(|p| p.ledger.is_unsaved()), "head differs from the persisted position");

    // S11: project-wide undo from page 1 changes page 2.
    let w4_route = h.route_handle(p2, w4);
    h.activate(p1);
    h.run("u", Value::Null).unwrap();
    h.run("u", Value::Null).unwrap();
    h.run("u", Value::Null).unwrap();
    let doc = h.doc();
    assert!(!doc.devices[&c1b].properties.contains_key("tag"), "SETPROP undone");
    assert!(doc.cables.is_empty(), "cable undone");
    assert!(!doc.wires.contains_key(&w4), "WIRE on page 2 undone");
    // Brief 016 (AC8): the undo of the wire on page 2 brings page 2 to the front, without it.
    assert_eq!(h.p(|p| p.active_page()), Some(p2), "the undo on page 2 brought page 2 to the front");
    assert!(h.pv(|p, pg| pg.entities_of(p, p2)).iter().all(|e| e.handle != w4_route), "page 2 shows the undo");
    h.activate(p1);
    for _ in 0..3 {
        h.run("redo", Value::Null).unwrap();
    }
    assert_eq!(h.doc().cables.len(), 1);
    assert_eq!(id::<ecad_model::CableId>(&cable, "cable"), *h.doc().cables.keys().next().unwrap(), "redo recreates the same cable ID");

    // S12: close the page window showing page 2 (the desktop app's `keep_open`: no prompt, the project
    // stays open), then show it again.
    h.activate(p2);
    h.cfg(|p| p.keep_open = true);
    let i2 = h.page_index(p2).unwrap();
    assert!(!h.s.close_needs_prompt(i2), "closing the page window never prompts with keep_open");
    h.run("close", json!({"index": i2})).unwrap();
    assert!(h.p(|p| p.ledger.is_unsaved()), "the project and its unsaved work stay open");
    assert_eq!(h.s.ext.owners().len(), 1);
    h.activate(p1);
    h.run("openpage", json!({"page": p2.hex()})).unwrap();
    assert!(h.page_index(p2).is_some());
    assert!(h.pv(|p, pg| pg.entities_of(p, p2)).len() >= 2, "page 2 reprojected with its line and wire");

    // S13: save; a new edit; a failed save; save again.
    h.activate(p1);
    h.run("qsave", Value::Null).unwrap();
    assert!(!h.p(|p| p.ledger.is_unsaved()));
    let saved_log = h.p(|p| p.ledger.full_log().unwrap());
    let saved_head = h.head();
    h.run("setprop", json!({"component": c1.hex(), "key": "tag", "value": "-K1"})).unwrap();
    let i1 = h.page_index(p1).unwrap();
    assert!(h.s.doc_unsaved(i1));
    h.cfg(|p| p.store.fail_next_write());
    let e = h.run("qsave", Value::Null).unwrap_err();
    assert!(e.contains("not saved"), "{e}");
    assert!(h.p(|p| p.ledger.is_unsaved()), "still unsaved after the failure");
    assert_eq!(h.p(|p| p.ledger.persisted()), saved_head);
    // The failed save acknowledged nothing: the file still opens at the first save, and the retry
    // writes the rest, so nothing is duplicated or lost.
    let after_failure = h.p(|p| p.store.bytes());
    let mut s0 = cadcraft_engine::Session::empty();
    let first = ProjectHandle::open_from(&mut s0, "Demo", after_failure, 1).unwrap();
    assert_eq!(first.read(&mut s0, |p, _| p.ledger.full_log().unwrap()).unwrap(), saved_log, "the first save is untouched");
    h.run("qsave", Value::Null).unwrap();
    assert!(!h.p(|p| p.ledger.is_unsaved()));
    assert_eq!(h.p(|p| p.events.count(EventKind::SaveFailed)), 1);
    let medium = h.p(|p| p.store.bytes());
    let log = h.p(|p| p.ledger.full_log().unwrap());
    let mut s1 = cadcraft_engine::Session::empty();
    let reopened = ProjectHandle::open_from(&mut s1, "Demo", medium, 1).unwrap();
    assert_eq!(reopened.read(&mut s1, |p, _| p.ledger.full_log().unwrap()).unwrap(), log, "every transaction exactly once");

    // S14: undo everything, then redo everything: state and IDs restored at every point.
    let trail: Vec<Document> = h.trail.clone();
    let head = h.head();
    let edits = trail.len() - 1;
    for k in (0..edits).rev() {
        h.run("u", Value::Null).unwrap();
        assert_eq!(h.doc(), trail[k], "after undo the document equals the recorded state {k}");
    }
    assert_eq!(h.doc(), trail[0]);
    h.run("u", Value::Null).unwrap();
    assert_eq!(h.doc(), Document::default(), "back to an empty project");
    h.run("redo", Value::Null).unwrap();
    for (k, state) in trail.iter().enumerate().skip(1) {
        h.run("redo", Value::Null).unwrap();
        assert_eq!(&h.doc(), state, "after redo the document equals the recorded state {k} with the same IDs");
    }
    assert_eq!(h.head(), head + 2 * (edits as u64 + 1), "the log grew by one transaction per undo and redo");

    // S15: reopen from the in-memory project file (checkpoints are disabled, §20.8).
    h.run("qsave", Value::Null).unwrap();
    let medium = h.p(|p| p.store.bytes());
    let live = h.doc();
    let id = h.p(|p| p.id);
    let mut s2 = cadcraft_engine::Session::empty();
    let reopened = ProjectHandle::open_from(&mut s2, "Demo", medium, 1).unwrap();
    assert_eq!(reopened.read(&mut s2, |p, _| p.ledger.document().clone()).unwrap(), live);
    assert_eq!(reopened.read(&mut s2, |p, _| p.id).unwrap(), id, "the project keeps its identity");
    let report = reopened.read(&mut s2, |p, _| p.open_report.clone()).unwrap();
    assert!(report.is_some_and(|r| r.damage.is_none()));

    // S16: failure after acceptance, before projection completes (PROJ-02).
    h.activate(p1);
    let handles = h.rep_handles(p1, c1);
    let head = h.head();
    h.cfg(|p| p.faults.fail_projections = 1);
    h.run("move", json!({"handles": handles, "delta": [0, 5]})).unwrap();
    assert_eq!(h.head(), head + 1, "the accepted transaction stays");
    assert!(h.connection_point_pos(p1, t1).near(Pt::new(40.0, 15.0), 1e-9), "the document holds the move");
    assert_eq!(h.p(|p| p.events.count(EventKind::ProjectionFailed)), 1);
    assert_eq!(h.p(|p| p.events.count(EventKind::ProjectionRebuilt)), 1);
    h.check(); // drawing and map rebuilt and consistent
    let handles = h.rep_handles(p1, c1);
    h.cfg(|p| p.faults.fail_projections = 2);
    h.run("move", json!({"handles": handles, "delta": [0, -5]})).unwrap();
    assert_eq!(h.p(|p| p.events.count(EventKind::PageUnavailable)), 1);
    assert!(h.p(|p| p.pages[&p1].unavailable));
    assert!(h.p(|p| p.ledger.is_unsaved()));
    let i1 = h.page_index(p1).unwrap();
    h.s.active = i1;
    h.run("qsave", Value::Null).unwrap();
    assert!(!h.p(|p| p.ledger.is_unsaved()), "the project is still saveable");
    h.run("close", json!({"index": i1})).unwrap();
    h.activate(p2);
    h.run("openpage", json!({"page": p1.hex()})).unwrap();
    assert!(h.connection_point_pos(p1, t1).near(Pt::new(40.0, 10.0), 1e-9));
    h.check();
    // H2: no committed drawing ever differed from its projection.
    let mismatched: Vec<_> = h.p(|p| p.events.events().filter(|e| e.kind == EventKind::FidelityMismatch).cloned().collect());
    assert!(mismatched.is_empty(), "{mismatched:?}");
}

fn w2_hex(c: WireId) -> String {
    c.hex()
}

#[test]
fn snapshots_are_labelled_and_history_is_separate() {
    let mut h = H::new(9);
    h.cmdline("line 0,0 10,0 ");
    let live_before = h.doc();
    let snap = h.run("ecadinspect", json!({"seq": 1})).unwrap();
    assert_eq!(snap["kind"], "historical");
    assert_eq!(snap["seq"], 1);
    assert_eq!(snap["transactions"].as_array().unwrap().len(), 1);
    assert_eq!(h.doc(), live_before, "historical inspection did not modify the live project");
    let live = h.run("ecadinspect", json!({})).unwrap();
    assert_eq!(live["kind"], "live");
    assert_eq!(live["seq"], h.head());
    assert_eq!(live["checks"]["replay_equals_live"], true);
    assert_eq!(live["pages"][0]["projection_matches"], true);
    assert!(live["events"].as_array().unwrap().iter().any(|e| e["command"] == "line" && e["kind"] == "Accepted"));
    assert_eq!(hex(&live, "upstream"), ecad_engine::UPSTREAM_COMMIT);
}
