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

/// `textstyle`, `toposition`, `spagetype`: a text, wire or location style, a deleted component's
/// record and a cable's property reach no cable's references but that cable's (before, each made a
/// commit derive every reference of the project twice). A standard and any other style catalogue
/// may come to carry sheets, so they still reach every cable.
#[test]
fn ops_that_read_no_reference_touch_no_cable() {
    use ecad_model::style::{StyleLayer, StyleRecord, StyleRef};
    use ecad_model::{CableId, Device, DeviceId, Op};
    let doc = ecad_model::Document::default();
    let touched = |ops: Vec<Op>| ecad_cadcraft::references::cables_touched(&doc, &ops);
    let style = |catalogue: ecad_model::StyleCatalogue| Op::SetStyle {
        layer: StyleLayer::Project,
        catalogue,
        id: StyleRef::builtin("tag").unwrap(),
        before: None,
        after: Some(StyleRecord::empty(catalogue)),
    };
    for catalogue in [ecad_model::StyleCatalogue::Text, ecad_model::StyleCatalogue::Wire, ecad_model::StyleCatalogue::Location] {
        assert_eq!(touched(vec![style(catalogue)]), Some(Default::default()), "a {catalogue} style");
    }
    assert_eq!(touched(vec![style(ecad_model::StyleCatalogue::Frame)]), None, "a frame style reaches every cable");
    assert_eq!(touched(vec![Op::SetStandard { before: None, after: None }]), None, "a standard reaches every cable");
    let gone = Op::DeleteDevice { id: DeviceId(7), device: Device::default() };
    assert_eq!(touched(vec![gone]), Some(Default::default()), "a deleted component");
    let k = CableId(9);
    let mut typed_cable = ecad_model::Cable::default();
    typed_cable.properties.insert("type".into(), "5G1.5".into());
    let typed = Op::ModifyCable { id: k, before: ecad_model::Cable::default(), after: typed_cable };
    assert_eq!(touched(vec![typed]), Some([k].into()), "a cable's own property");
}

/// A cable slashed on page Supply with a reference there, and its other end on page Field:
/// (harness, cable, Supply, Field). The other end is written as `cablecontinue` writes it
/// ([`H::reference_end`]): a continuation needs wires leaving the page through the reference,
/// which form only with lane F step F4, and these tests are about the references, not the wires.
fn paired_cable(seed: u64) -> (H, ecad_model::CableId, ecad_model::PageId, ecad_model::PageId) {
    let mut h = H::new(seed);
    let supply = h.first_page();
    for y in [0.0, 5.0, 10.0] {
        h.run("wire", json!({"points": [[0.0, y], [60.0, y]]})).unwrap();
    }
    let c1: ecad_model::CableId = id(&h.run("cableslash", json!({"from": [28, -3], "to": [32, 18], "tag": "C1"})).unwrap(), "cable");
    h.run("cableref", json!({"cable": c1.hex(), "at": [65, 5]})).unwrap();
    let field: ecad_model::PageId = id(&h.run("newpage", json!({"title": "Field"})).unwrap(), "page");
    h.activate(field);
    h.reference_end(c1, field, (20.0, 5.0));
    (h, c1, supply, field)
}

/// Review of #109, 1: a change of the project's standard re-traces every cable's references, and
/// each reference's zone afterwards is the one a fresh derivation gives.
#[test]
fn a_standard_change_retraces_every_cable_reference() {
    let (mut h, c1, supply, _) = paired_cable(0x53_03);
    let zone = |doc: &ecad_model::Document| {
        ecad_cadcraft::references::all(doc)
            .into_iter()
            .filter(|r| r.cable == c1 && r.page == supply)
            .map(|r| (r.column, r.row, r.column_label, r.row_label, r.text))
            .collect::<Vec<_>>()
    };
    let before = zone(&h.doc());
    assert!(before.first().is_some_and(|z| z.0.is_some()), "the reference on Supply has a zone: {before:?}");
    h.run("standard", json!({"new": "House"})).unwrap();
    let ops = h.p(|p| p.ledger.log().last().map(|t| t.ops.clone()).unwrap_or_default());
    assert!(ops.iter().any(|op| matches!(op, ecad_model::Op::SetStandard { .. })), "the commit set the standard: {ops:?}");
    let doc = h.doc();
    assert_eq!(ecad_cadcraft::references::cables_touched(&doc, &ops), None, "a standard re-traces every cable");
    let shown: Vec<_> = ecad_cadcraft::references::markers_on_page(&doc, supply).into_iter().map(|m| m.text).collect();
    let fresh = zone(&doc);
    assert_eq!(shown, fresh.iter().map(|z| z.4.clone()).collect::<Vec<_>>(), "the page shows the fresh zone");
    assert_eq!(fresh, before, "this standard carries no sheet, so the zone is unchanged");
}

/// `wire`, `joinwires`: a page's reference markers come from the references of the cables drawn
/// on it, and equal the markers taken from every reference of the project.
#[test]
fn a_page_s_markers_equal_those_of_every_reference() {
    let (mut h, _, supply, field) = paired_cable(0x53_02);
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
