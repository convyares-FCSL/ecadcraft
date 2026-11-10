//! Contract tests for brief 023 (upstream sync to CADCraft 0.19.0). CADCraft 0.16.0 to 0.19.0
//! brought command paths that change a page in new ways: BREAK and TRIM keep the object's handle
//! for one piece (#2545), OOPS brings back erased objects with their old handles, `-ARRAY` is the
//! classic command-line array, and MINSERT inserts an array of block references. On an ECAD page
//! each one is recorded properly or refused with a reason, and the project stays consistent
//! (proposal §17.4).

use crate::common;

use cadcraft_engine::doc::{Handle, Space};
use common::{H, pts, small_project};
use ecad_inspect::EventKind;
use ecad_model::{ConductorId, PageId, TxKind};
use serde_json::{Value, json};

/// What a refusal must leave as it was.
struct Before {
    head: u64,
    doc: ecad_model::Document,
    unsaved: bool,
    model: cadcraft_engine::doc::EntityStore,
}

fn before(h: &mut H) -> Before {
    let page = active(h);
    h.pv(|p, pg| Before {
        head: p.ledger.head(),
        doc: p.ledger.document().clone(),
        unsaved: p.ledger.is_unsaved(),
        model: pg.drawing_of(p, page).unwrap().space(&Space::Model).unwrap().clone(),
    })
}

fn active(h: &mut H) -> PageId {
    let uid = h.s.docs[h.s.active].uid;
    h.p(|p| p.page_of(uid).unwrap())
}

/// `r` is an error naming `why`, nothing was recorded, and the page is drawn as before.
fn refused(h: &mut H, b: &Before, what: &str, r: Result<Value, String>, why: &str) {
    h.check();
    let page = active(h);
    let e = r.expect_err(what);
    assert!(e.contains(why), "{what}: {e}");
    h.pv(|p, pg| {
        assert_eq!(p.ledger.head(), b.head, "{what}: nothing appended");
        assert_eq!(p.ledger.document(), &b.doc, "{what}: document unchanged");
        assert_eq!(p.ledger.is_unsaved(), b.unsaved, "{what}: unsaved state unchanged");
        assert!(pg.drawing_of(p, page).unwrap().space(&Space::Model).unwrap() == &b.model, "{what}: drawing restored");
    });
    assert!(h.s.state().unwrap().undo.is_empty(), "{what}: no engine undo step");
}

/// The last transaction's label and op names.
fn last_tx(h: &mut H) -> (String, Vec<&'static str>) {
    h.p(|p| p.ledger.log().last().map(|t| (t.label.clone(), t.ops.iter().map(|o| o.name()).collect())).unwrap())
}

/// The small project plus a plain line from (50, 30) to (80, 30), its handle, and the wire.
fn fixture() -> (H, Handle, ConductorId) {
    let (mut h, _, _, w) = small_project(23);
    let page = h.first_page();
    h.run("line", json!({"points": pts(&[(50.0, 30.0), (80.0, 30.0)])})).unwrap();
    let plain = h.lines_on(page).into_iter().last().unwrap();
    (h, plain, w)
}

/// Two cutting edges across x = 60 and x = 70, from y = -5 to 35; their handles.
fn edges(h: &mut H) -> [String; 2] {
    let page = h.first_page();
    h.run("line", json!({"points": pts(&[(60.0, -5.0), (60.0, 35.0)])})).unwrap();
    h.run("line", json!({"points": pts(&[(70.0, -5.0), (70.0, 35.0)])})).unwrap();
    let ls = h.lines_on(page);
    let n = ls.len();
    [ls[n - 2].hex(), ls[n - 1].hex()]
}

const SPLIT: &str = "replaces objects that have an electrical meaning";

#[test]
fn break_and_trim_that_split_a_wire_are_refused() {
    let (mut h, _, w) = fixture();
    let page = h.first_page();
    let wire = h.route_handle(page, w);
    let b = before(&mut h);
    let r = h.run("break", json!({"handle": wire.hex(), "p1": [60, 10], "p2": [70, 10]}));
    refused(&mut h, &b, "BREAK of a wire", r, SPLIT);
    let e = edges(&mut h);
    let b = before(&mut h);
    let r = h.run("trim", json!({"handle": wire.hex(), "pick": [65, 10], "edges": e}));
    refused(&mut h, &b, "TRIM of a wire's middle", r, SPLIT);
    assert_eq!(h.route_handle(page, w), wire, "the wire keeps its route");
    assert!(h.p(|p| p.events.count(EventKind::Refused)) >= 2);
}

#[test]
fn trim_and_break_of_plain_drawing_keep_the_handle_and_its_item() {
    for cmd in ["trim", "break"] {
        let (mut h, plain, _) = fixture();
        let page = h.first_page();
        let e = edges(&mut h);
        let item = match h.mapped(page, plain) {
            Some(ecad_cadcraft::project::Mapped::Item(id)) => id,
            other => panic!("the plain line is an item: {other:?}"),
        };
        let lines = h.lines_on(page).len();
        let head = h.head();
        let r = match cmd {
            "trim" => h.run("trim", json!({"handle": plain.hex(), "pick": [65, 30], "edges": e})),
            _ => h.run("break", json!({"handle": plain.hex(), "p1": [60, 30], "p2": [70, 30]})),
        };
        let handles = r.unwrap()["handles"].clone();
        assert_eq!(handles[0], json!(plain.hex()), "{cmd}: the object keeps its handle");
        assert_eq!(h.head(), head + 1, "{cmd}: one transaction");
        assert_eq!(last_tx(&mut h), (cmd.to_ascii_uppercase(), vec!["ModifyItem", "AddItem"]), "{cmd}");
        assert!(
            matches!(h.mapped(page, plain), Some(ecad_cadcraft::project::Mapped::Item(id)) if id == item),
            "{cmd}: the kept piece is the same item"
        );
        // Undo takes both pieces back to the one line.
        h.cmdline("u");
        assert_eq!(h.lines_on(page).len(), lines, "{cmd}: undo leaves one line again");
        assert!(matches!(h.mapped(page, plain), Some(ecad_cadcraft::project::Mapped::Item(id)) if id == item));
    }
}

#[test]
fn oops_brings_back_plain_drawing_as_one_transaction() {
    let (mut h, plain, _) = fixture();
    let page = h.first_page();
    let doc = h.doc();
    h.run("erase", json!({"handles": [plain.hex()]})).unwrap();
    assert!(h.mapped(page, plain).is_none());
    let head = h.head();
    let r = h.run("oops", json!({})).unwrap();
    assert_eq!(r["restored"], json!(1));
    assert_eq!(h.head(), head + 1);
    assert_eq!(last_tx(&mut h), ("OOPS".into(), vec!["AddItem"]));
    assert!(matches!(h.mapped(page, plain), Some(ecad_cadcraft::project::Mapped::Item(_))), "back with its handle, as an item");
    // The same geometry as before the erase.
    let items = |d: &ecad_model::Document| {
        d.items.values().filter(|i| i.page == page).map(|i| format!("{:?}", i.kind)).collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(items(&h.doc()), items(&doc));
    // Typed at the command line too.
    h.run("erase", json!({"handles": [plain.hex()]})).unwrap();
    let head = h.head();
    h.cmdline("oops");
    assert_eq!(h.head(), head + 1);
}

#[test]
fn oops_of_objects_with_a_meaning_is_refused_with_a_reason() {
    for what in ["wire", "component line", "symbol", "wire and plain line"] {
        let (mut h, plain, w) = fixture();
        let page = h.first_page();
        let doc = h.doc();
        let c1 = *doc.components.keys().next().unwrap();
        let c2 = *doc.components.keys().nth(1).unwrap();
        let target: Vec<String> = match what {
            "wire" => vec![h.route_handle(page, w).hex()],
            "component line" => vec![h.rep_handles(page, c1)[0].clone()],
            "symbol" => {
                let item = h.symbol_item(c2);
                vec![h.handle_of(page, item).hex()]
            }
            _ => vec![h.route_handle(page, w).hex(), plain.hex()],
        };
        h.run("erase", json!({"handles": target})).unwrap();
        let b = before(&mut h);
        let r = h.run("oops", json!({}));
        refused(&mut h, &b, what, r, "use UNDO");
        // Typed: refused before it starts, with the same reason on the command line.
        let b = before(&mut h);
        let typed = h.s.cmdline("oops").map(|()| json!(null)).map_err(|e| e.to_string());
        let typed = typed.and_then(|v| match h.s.log.iter().rev().take(3).find(|l| l.contains("use UNDO")) {
            Some(m) => Err(m.clone()),
            None => Ok(v),
        });
        refused(&mut h, &b, what, typed, "use UNDO");
        // UNDO brings it back.
        h.cmdline("u");
        assert_eq!(h.doc(), doc, "{what}: undo brings it back");
    }
}

#[test]
fn oops_after_undo_or_on_another_page_is_refused() {
    // After UNDO the drawing changed under what OOPS kept.
    let (mut h, plain, _) = fixture();
    h.run("erase", json!({"handles": [plain.hex()]})).unwrap();
    h.cmdline("u");
    let b = before(&mut h);
    let r = h.run("oops", json!({}));
    refused(&mut h, &b, "OOPS after UNDO", r, "use UNDO");
    // On another page shown in the same window.
    let (mut h, plain, _) = fixture();
    let p1 = h.first_page();
    h.run("erase", json!({"handles": [plain.hex()]})).unwrap();
    h.run("newpage", json!({"title": "2"})).unwrap();
    let p2 = h.doc().page_order[1];
    h.activate(p2);
    let b = before(&mut h);
    let r = h.run("oops", json!({}));
    refused(&mut h, &b, "OOPS on another page", r, "use UNDO");
    // Back on the page it was erased from, the page was drawn again: refused as well, never a
    // silent nothing.
    h.activate(p1);
    let b = before(&mut h);
    let r = h.run("oops", json!({}));
    refused(&mut h, &b, "OOPS after switching pages", r, "use UNDO");
}

#[test]
fn classic_array_copies_are_recorded_as_copies() {
    let cases: [(&str, fn(&mut H, Handle, ConductorId) -> Value, &[&str]); 3] = [
        ("plain line", |_, plain, _| json!({"handles": [plain.hex()], "rows": 2, "cols": 2, "rowSpacing": 10, "colSpacing": 50}), &["AddItem"]),
        (
            "symbol",
            |h, _, _| {
                let c2 = *h.doc().components.keys().nth(1).unwrap();
                let item = h.symbol_item(c2);
                let page = h.first_page();
                json!({"handles": [h.handle_of(page, item).hex()], "rows": 1, "cols": 2, "colSpacing": 60})
            },
            &["CreateComponent", "LinkRepresentation"],
        ),
        (
            "wire",
            |h, _, w| {
                let page = h.first_page();
                json!({"handles": [h.route_handle(page, w).hex()], "rows": 2, "cols": 1, "rowSpacing": 30})
            },
            &["CreateConductor", "LinkRepresentation"],
        ),
    ];
    for (what, params, expect) in cases {
        let (mut h, plain, w) = fixture();
        let p = params(&mut h, plain, w);
        let head = h.head();
        let components = h.doc().components.len();
        let conductors = h.doc().conductors.len();
        h.run("-array", p).unwrap();
        assert_eq!(h.head(), head + 1, "{what}: one transaction");
        let (label, ops) = last_tx(&mut h);
        assert_eq!(label, "-ARRAY", "{what}");
        for e in expect {
            assert!(ops.contains(e), "{what}: {ops:?}");
        }
        let doc = h.doc();
        match what {
            "symbol" => assert_eq!(doc.components.len(), components + 1, "a copied device"),
            "wire" => assert_eq!(doc.conductors.len(), conductors + 1, "a copied wire"),
            _ => assert_eq!((doc.components.len(), doc.conductors.len()), (components, conductors)),
        }
        h.check();
        // One undo takes the whole array back.
        h.cmdline("u");
        assert_eq!(h.head(), head + 2);
        assert!(matches!(h.p(|p| p.ledger.log().last().map(|t| t.kind)), Some(TxKind::Undo { .. })));
        h.check();
    }
}

#[test]
fn minsert_on_an_ecad_page_is_refused_with_a_reason() {
    let (mut h, _, _) = fixture();
    let page = h.first_page();
    let name = h.pv(|p, pg| pg.drawing_of(p, page).unwrap().blocks.values().map(|b| b.name.clone()).find(|n| n.starts_with("ECAD$"))).unwrap();
    let b = before(&mut h);
    let r = h.run("minsert", json!({"name": name, "at": [200, 50], "rows": 2, "columns": 2, "rowSpacing": 10, "columnSpacing": 20}));
    refused(&mut h, &b, "MINSERT of an ECAD symbol", r, "not record");
    // A block of one's own cannot be made on an ECAD page, so MINSERT has nothing else to insert.
    h.cmdline("line 200,0 210,0 ");
    let l = h.lines_on(page).into_iter().last().unwrap();
    let b = before(&mut h);
    let r = h.run("block", json!({"name": "B1", "handles": [l.hex()], "base": [200, 0]}));
    refused(&mut h, &b, "BLOCK", r, "block definitions");
}
