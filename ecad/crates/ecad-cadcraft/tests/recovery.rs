//! Recovery after the ledger accepted a transaction (PROJ-02), checked without handle numbers.
//!
//! The normal path is checked by handle (`FidelityMismatch`, see `experiment.rs`). Here every
//! rebuilt page is checked by content with `verify_page`, and the tests add their own independent
//! comparison: the document is unchanged by recovery, and the rebuilt drawing, read back into
//! ECAD terms, equals what the document says (geometry, styles, symbol placement, terminal
//! positions, relationships). Selection may be lost; IDs and content must survive.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use cadcraft_engine::doc::{Drawing, EntityKind, Space};
use common::{H, small_project};
use ecad_cadcraft::verify::verify_page;
use ecad_cadcraft::{HandleMap, project_page};
use ecad_inspect::EventKind;
use ecad_model::{Document, PageId};
use serde_json::{Value, json};

/// Everything a page shows, keyed by stable ECAD IDs only: items (geometry, style, placement),
/// drawn terminal markers, symbol terminal positions.
fn content_by_id(doc: &Document, page: PageId, d: &Drawing, map: &HandleMap) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let model = d.space(&Space::Model).unwrap();
    for id in doc.items.iter().filter(|(_, i)| i.page == page).map(|(id, _)| *id) {
        let e = model.get(map.item(id).expect("item mapped")).expect("item drawn");
        let item = ecad_cadcraft::convert::item_from(doc, page, e).expect("readable");
        out.insert(format!("item {id}"), serde_json::to_string(&item).unwrap());
    }
    for (rid, r) in doc.representations.iter().filter(|(_, r)| r.page == page) {
        for t in r.connection_points.keys() {
            let e = model.get(map.marker(*rid, *t).expect("marker mapped")).expect("marker drawn");
            let p = ecad_cadcraft::convert::marker_position(e).expect("a marker");
            out.insert(format!("marker {rid}/{t}"), format!("{:.9},{:.9}", p.x, p.y));
        }
    }
    out
}

fn page_state(h: &mut H, page: PageId) -> (Document, Arc<Drawing>, HandleMap) {
    h.pv(|p, pg| (p.ledger.document().clone(), pg.drawing_of(p, page).unwrap(), p.pages[&page].map.clone()))
}

#[test]
fn a_rebuilt_page_is_verified_by_content_not_handles() {
    let (mut h, c1, _c2, _w) = small_project(31);
    let page = h.first_page();
    let handles = h.rep_handles(page, c1);
    h.s.set_selection(h.s.doc().unwrap().model.iter().map(|e| e.handle).collect());
    h.cfg(|p| p.faults.fail_projections = 1);
    h.run("move", json!({"handles": handles, "delta": [0, 5]})).unwrap();
    assert_eq!(h.p(|p| p.events.count(EventKind::ProjectionRebuilt)), 1);
    assert_eq!(h.p(|p| p.events.count(EventKind::RecoveryVerified)), 1, "the rebuild was verified");
    assert_eq!(h.p(|p| p.events.count(EventKind::FidelityMismatch)), 0, "no handle comparison after a rebuild");
    let (doc, d, map) = page_state(&mut h, page);
    let v = verify_page(&doc, page, &d, &map);
    assert!(v.ok(), "{:?}", v.problems);
    assert!(v.items > 0 && v.terminals >= 3 && v.attached_ends == 2, "{v:?}");
    // Independent comparison: a projection with completely different handle numbers shows the
    // same content per stable ID.
    let mut far = Drawing::new_metric();
    far.bump_handseed(cadcraft_engine::doc::Handle(0x7000));
    let (other, other_map) = project_page(&doc, page, &far, &HandleMap::default()).unwrap();
    assert!(other_map.handles().all(|x| map.handles().all(|y| x != y)), "no handle in common");
    assert_eq!(content_by_id(&doc, page, &d, &map), content_by_id(&doc, page, &other, &other_map));
    assert!(verify_page(&doc, page, &other, &other_map).ok());
    h.check();
}

#[cfg(feature = "fault-injection")]
#[test]
fn owner_recovery_rebuilds_every_open_page_and_keeps_the_document() {
    let (mut h, ..) = small_project(32);
    let p1 = h.first_page();
    h.run("newpage", json!({"title": "2"})).unwrap();
    h.run("wire", json!({"points": [[0, 0], [30, 0]]})).unwrap();
    let p2 = h.doc().page_order[1];
    let (doc_before, d1, m1) = page_state(&mut h, p1);
    let (_, d2, m2) = page_state(&mut h, p2);
    let head = h.head();
    h.cfg(|p| p.faults.panic_in_commit = true);
    assert!(h.run("line", json!({"points": [[0, 50], [10, 50]]})).is_err());
    assert_eq!(h.head(), head, "the failed command recorded nothing");
    assert_eq!(h.doc(), doc_before, "stable IDs and content are unchanged");
    assert_eq!(h.p(|p| p.events.count(EventKind::Recovered)), 1);
    assert_eq!(h.p(|p| p.events.count(EventKind::RecoveryVerified)), 2, "both open pages verified");
    for (page, before, before_map) in [(p1, d1, m1), (p2, d2, m2)] {
        let (doc, d, map) = page_state(&mut h, page);
        assert!(verify_page(&doc, page, &d, &map).ok());
        assert_eq!(content_by_id(&doc, page, &d, &map), content_by_id(&doc_before, page, &before, &before_map));
    }
    h.check();
}

#[test]
fn verification_catches_each_kind_of_disagreement() {
    let (mut h, c1, c2, w) = small_project(33);
    let page = h.first_page();
    let (doc, d, map) = page_state(&mut h, page);
    assert!(verify_page(&doc, page, &d, &map).ok());
    let tamper = |f: &dyn Fn(&mut Drawing)| {
        let mut x = (*d).clone();
        f(&mut x);
        verify_page(&doc, page, &x, &map)
    };
    let edit = |x: &mut Drawing, h: cadcraft_engine::doc::Handle, f: &dyn Fn(&mut cadcraft_engine::doc::Entity)| {
        let mut e = (**x.model.get(h).unwrap()).clone();
        f(&mut e);
        x.model.replace(e);
    };
    let sym = map.item(doc.representations_of(ecad_model::Subject::Component(c2)).next().unwrap().1.items[0]).unwrap();
    let route = map.item(ecad_model::derive::route_on_page(&doc, w, page).unwrap().0).unwrap();
    let (rid, r) = doc.representations_of(ecad_model::Subject::Component(c1)).next().unwrap();
    let marker = map.marker(rid, *r.connection_points.keys().next().unwrap()).unwrap();
    let line = map.item(r.items[0]).unwrap();
    let cases: Vec<(&str, ecad_cadcraft::verify::Verification)> = vec![
        (
            "geometry",
            tamper(&|x| {
                edit(x, line, &|e| {
                    if let EntityKind::Line(l) = &mut e.kind {
                        l.b.x += 1.0;
                    }
                })
            }),
        ),
        ("style", tamper(&|x| edit(x, line, &|e| e.common.color = cadcraft_engine::doc::color::Color::Index(1)))),
        (
            "symbol placement",
            tamper(&|x| {
                edit(x, sym, &|e| {
                    if let EntityKind::Insert(i) = &mut e.kind {
                        i.rotation += 0.5;
                    }
                })
            }),
        ),
        (
            "terminal marker",
            tamper(&|x| {
                edit(x, marker, &|e| {
                    if let EntityKind::Point(p) = &mut e.kind {
                        p.p.y += 1.0;
                    }
                })
            }),
        ),
        (
            "route end off its terminal",
            tamper(&|x| {
                edit(x, route, &|e| match &mut e.kind {
                    EntityKind::Line(l) => l.b.y += 1.0,
                    EntityKind::LwPolyline(p) => {
                        if let Some(v) = p.vertices.last_mut() {
                            v.p.y += 1.0;
                        }
                    }
                    _ => {}
                })
            }),
        ),
        ("group", tamper(&|x| x.groups.iter_mut().for_each(|g| g.members.truncate(1)))),
        (
            "missing entity",
            tamper(&|x| {
                x.model.remove(line);
            }),
        ),
        (
            "symbol block",
            tamper(&|x| x.blocks.values_mut().for_each(|b| Arc::make_mut(b).entities = Default::default())),
        ),
    ];
    for (what, v) in cases {
        assert!(!v.ok(), "{what}: not detected");
    }
}

#[test]
fn an_unavailable_page_is_a_labelled_read_only_placeholder() {
    let (mut h, c1, ..) = small_project(34);
    let p1 = h.first_page();
    h.run("newpage", json!({"title": "2"})).unwrap();
    let p2 = h.doc().page_order[1];
    h.activate(p1);
    let handles = h.rep_handles(p1, c1);
    h.cfg(|p| p.faults.fail_projections = 2);
    h.run("move", json!({"handles": handles, "delta": [0, 5]})).unwrap();
    assert!(h.p(|p| p.pages[&p1].unavailable));
    let i1 = h.page_index(p1).unwrap();
    // Labelled: one text naming the problem and the way out; the tab title says so too.
    let texts: Vec<String> = h.s.docs[i1]
        .doc
        .model
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::Text(t) => Some(t.value.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, vec![ecad_cadcraft::project::PLACEHOLDER_TEXT.to_string()]);
    assert!(h.s.docs[i1].title.ends_with(ecad_cadcraft::project::UNAVAILABLE_SUFFIX));
    // Read only: CADCraft edits, ECAD commands and undo are refused; nothing is recorded.
    let head = h.head();
    let shown = h.s.docs[i1].doc.clone();
    h.s.active = i1;
    let text = h.s.docs[i1].doc.model.iter().next().unwrap().handle;
    for (cmd, params) in [
        ("line", json!({"points": [[0, 0], [5, 5]]})),
        ("erase", json!({"handles": [text.hex()]})),
        ("move", json!({"handles": [text.hex()], "delta": [1, 1]})),
        ("wire", json!({"points": [[0, 0], [5, 0]]})),
        ("u", Value::Null),
    ] {
        let r = h.run(cmd, params);
        assert!(r.is_err(), "{cmd} must be refused on a placeholder");
        assert!(Arc::ptr_eq(&h.s.docs[i1].doc, &shown) || h.s.docs[i1].doc.model == shown.model, "{cmd}");
    }
    assert_eq!(h.head(), head);
    // Edits on another page do not revive or touch the placeholder.
    h.activate(p2);
    h.run("wire", json!({"points": [[0, 0], [30, 0]]})).unwrap();
    assert!(h.p(|p| p.pages[&p1].unavailable));
    assert!(h.s.docs[i1].doc.model == shown.model);
    // The project still saves, from the placeholder page too.
    h.s.active = i1;
    h.run("qsave", Value::Null).unwrap();
    assert!(!h.p(|p| p.ledger.is_unsaved()));
    // Close and reopen: the page is drawn from the document again.
    h.run("close", json!({"index": i1})).unwrap();
    h.run("openpage", json!({"page": p1.hex()})).unwrap();
    assert!(!h.p(|p| p.pages[&p1].unavailable));
    let i1 = h.page_index(p1).unwrap();
    assert!(!h.s.docs[i1].title.ends_with(ecad_cadcraft::project::UNAVAILABLE_SUFFIX));
    let (doc, d, map) = page_state(&mut h, p1);
    assert!(verify_page(&doc, p1, &d, &map).ok());
    h.check();
}
