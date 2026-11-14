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
