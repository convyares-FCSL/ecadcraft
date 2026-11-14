//! Brief 053, the performance batch (v0.2.1): the work the large-tier misses hid, counted.

use std::sync::atomic::Ordering;

use serde_json::json;

use crate::common::{H, id};

/// `bomcols`: a BOM page of several sheets lays its table out once per draw, not once per sheet
/// (it took 1.5 s on the large tier's 117 sheets).
#[test]
fn a_bom_page_lays_its_table_out_once_per_draw() {
    let mut h = H::new(0x53_01);
    for i in 0..95 {
        h.run("bomitem", json!({"description": format!("Item {i}"), "qty": 1})).unwrap();
    }
    let page: ecad_model::PageId = id(&h.run("newpage", json!({"generated": "bom-location"})).unwrap(), "page");
    let (sheets, groupings) = h.cfg(|p| {
        let o = p.outputs().clone();
        let doc = p.ledger.document();
        let before = o.half_groupings.load(Ordering::Relaxed);
        std::hint::black_box(ecad_cadcraft::generated::parts_of(doc, page, Some(&o)));
        (o.bom_sheets(), o.half_groupings.load(Ordering::Relaxed) - before)
    });
    assert!(sheets >= 3, "the page needs several sheets, it has {sheets}");
    assert_eq!(groupings, 1, "the BOM rows were grouped {groupings} times for {sheets} sheets");
}

/// `wire`, `joinwires`: a page's reference markers come from the references of the cables drawn
/// on it, and equal the markers taken from every reference of the project.
#[test]
fn a_page_s_markers_equal_those_of_every_reference() {
    let mut h = H::new(0x53_02);
    let supply = h.first_page();
    for y in [0.0, 5.0, 10.0] {
        h.run("wire", json!({"points": [[0.0, y], [60.0, y]]})).unwrap();
    }
    let c1: ecad_model::CableId = id(&h.run("cableslash", json!({"from": [28, -3], "to": [32, 18], "tag": "C1"})).unwrap(), "cable");
    h.run("cableref", json!({"cable": c1.hex(), "at": [65, 5]})).unwrap();
    let field: ecad_model::PageId = id(&h.run("newpage", json!({"title": "Field"})).unwrap(), "page");
    h.activate(field);
    h.run("cablecontinue", json!({"cable": c1.hex(), "at": [20, 5], "toward": [40, 5]})).unwrap();
    let doc = h.doc();
    let every = ecad_cadcraft::references::all(&doc);
    for page in [supply, field] {
        let markers = ecad_cadcraft::references::markers_on_page(&doc, page);
        assert!(!markers.is_empty(), "a marker on each page");
        assert_eq!(markers, ecad_cadcraft::references::markers_from(&doc, every.clone(), page));
        let on_page: Vec<_> = every.iter().filter(|r| r.page == page).cloned().collect();
        assert_eq!(ecad_cadcraft::references::references_on_page(&doc, page), on_page);
        let items: Vec<_> = doc.items.iter().filter(|(_, i)| i.page == page).map(|(id, _)| *id).collect();
        assert_eq!(doc.items_on(page).map(|(id, _)| id).collect::<Vec<_>>(), items, "the page index lists the page's items in ID order");
    }
}
