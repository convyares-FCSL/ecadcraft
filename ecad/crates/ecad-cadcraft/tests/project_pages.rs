//! Project-wide behaviour seen from any page: Save and Save As, unsupported UNDO options, and the
//! lock boundary of `ProjectHandle`.

mod common;

use common::small_project;
use ecad_store::Medium;
use serde_json::{Value, json};

#[test]
fn save_from_either_page_saves_the_whole_project() {
    let (mut h, c1, ..) = small_project(41);
    let p1 = h.first_page();
    h.run("newpage", json!({"title": "2"})).unwrap();
    let p2 = h.doc().page_order[1];
    h.run("wire", json!({"points": [[0, 0], [30, 0]]})).unwrap();
    // Save from page 2 after edits on both pages.
    h.activate(p1);
    h.run("setprop", json!({"component": c1.hex(), "key": "tag", "value": "-K1"})).unwrap();
    h.activate(p2);
    h.run("qsave", Value::Null).unwrap();
    let (head, persisted, len) = h.p(|p| (p.ledger.head(), p.ledger.persisted(), p.ledger.log().len()));
    assert_eq!(persisted, head);
    let stored = h.p(|p| p.store.medium.read_all().unwrap());
    let records = stored.iter().filter(|b| **b == b'\n').count();
    assert_eq!(records, len + 1, "header plus every transaction, including page 1's edit");
    let (i1, i2) = (h.page_index(p1).unwrap(), h.page_index(p2).unwrap());
    assert!(!h.s.doc_unsaved(i1) && !h.s.doc_unsaved(i2), "both tabs clean");
    // An edit on page 2, saved from page 1.
    h.run("wire", json!({"points": [[0, 10], [30, 10]]})).unwrap();
    assert!(h.s.doc_unsaved(i1) && h.s.doc_unsaved(i2), "both tabs show the project's unsaved state");
    h.activate(p1);
    h.run("qsave", Value::Null).unwrap();
    assert!(!h.p(|p| p.ledger.is_unsaved()));
    h.check();
}

#[test]
fn save_as_updates_every_page_path() {
    let (mut h, ..) = small_project(42);
    let p1 = h.first_page();
    h.run("newpage", json!({"title": "2"})).unwrap();
    let p2 = h.doc().page_order[1];
    h.activate(p1);
    h.run("saveas", json!({"path": "first.ecad"})).unwrap();
    let paths = |h: &mut common::H| {
        let (a, b) = (h.page_index(p1).unwrap(), h.page_index(p2).unwrap());
        (h.s.docs[a].path.clone(), h.s.docs[b].path.clone(), h.p(|p| p.path.clone()))
    };
    let first = Some("first.ecad".to_string());
    assert_eq!(paths(&mut h), (first.clone(), first.clone(), first));
    h.activate(p2);
    h.run("saveas", json!({"path": "second.ecad"})).unwrap();
    let second = Some("second.ecad".to_string());
    assert_eq!(paths(&mut h), (second.clone(), second.clone(), second.clone()));
    // A page opened later carries the project's path as well.
    let i2 = h.page_index(p2).unwrap();
    h.run("close", json!({"index": i2})).unwrap();
    h.run("openpage", json!({"page": p2.hex()})).unwrap();
    assert_eq!(paths(&mut h), (second.clone(), second.clone(), second));
    h.check();
}

#[test]
fn unsupported_undo_options_are_reported_on_ecad_pages() {
    let (mut h, ..) = small_project(43);
    let head = h.head();
    for mode in ["mark", "back", "begin", "end", "auto", "control"] {
        let e = h.run("undo", json!({"mode": mode, "on": true, "control": "all"})).unwrap_err();
        assert!(e.contains("not supported for this document"), "{mode}: {e}");
    }
    for kw in ["mark", "back", "begin", "end", "auto", "control"] {
        let before = h.s.log.len();
        h.cmdline(&format!("undo {kw} "));
        assert!(h.s.log[before..].iter().any(|l| l.contains("not supported for this document")), "{kw}");
    }
    assert_eq!(h.head(), head, "nothing was undone or recorded");
    h.run("undo", json!({"count": 1})).unwrap();
    assert_eq!(h.head(), head + 1, "a count still undoes through the project (an undo transaction)");
    h.check();
}

#[test]
fn a_project_handle_only_works_with_its_own_session() {
    let (mut h, ..) = small_project(44);
    let mut other = cadcraft_engine::Session::empty();
    assert_eq!(h.project.read(&mut other, |_, _| ()).err().as_deref(), Some(ecad_cadcraft::NOT_ATTACHED));
    assert_eq!(h.project.snapshot(&mut other, None).err().as_deref(), Some(ecad_cadcraft::NOT_ATTACHED));
    assert!(h.project.read(&mut h.s, |p, _| p.ledger.head()).is_ok());
}
