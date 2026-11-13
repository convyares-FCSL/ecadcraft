//! Brief 033 (design 033): the missing-data mark of generated tables (§3, AC4).

use std::collections::BTreeSet;

use cadcraft_engine::doc::EntityKind;
use cadcraft_engine::doc::color::Color;
use ecad_cadcraft::generated::{FLAGGED_ACI, FLAGGED_PRINT_ACI, LAYER, MISSING};
use ecad_cadcraft::outputs::Outputs;
use ecad_model::PageId;
use serde_json::json;

use crate::common::{H, id};

/// A project with one coil (no maker or part number: `?` in the BOM) and a BOM page.
fn bom_with_missing() -> (H, PageId) {
    let mut h = H::new(0x33_03);
    h.run("place", json!({"symbol": "coil", "at": [100, 150]})).unwrap();
    let bom: PageId = id(&h.run("newpage", json!({"generated": "bom-location"})).unwrap(), "page");
    (h, bom)
}

#[test]
fn a_missing_value_is_a_boxed_question_mark_yellow_on_screen() {
    let (mut h, bom) = bom_with_missing();
    let doc = h.doc();
    let parts = ecad_cadcraft::generated::parts_of(&doc, bom, None);
    // Missing cells are flagged; a block heading of devices without a location also reads `?`,
    // in the heading's own colour, and is not a cell.
    let marks: Vec<_> = parts.iter().filter(|p| p.aci == Some(FLAGGED_ACI) && matches!(&p.kind, EntityKind::Text(t) if t.value == MISSING)).collect();
    let texts: Vec<(String, Option<u8>)> =
        parts.iter().filter_map(|p| if let EntityKind::Text(t) = &p.kind { Some((t.value.clone(), p.aci)) } else { None }).collect();
    assert!(!marks.is_empty(), "the BOM has a missing value: {texts:?}");
    for m in &marks {
        let EntityKind::Text(t) = &m.kind else { unreachable!() };
        // A closed box in the same colour round it.
        let boxed = parts.iter().any(|p| match &p.kind {
            EntityKind::LwPolyline(l) if l.closed && p.aci == Some(FLAGGED_ACI) && l.vertices.len() == 4 => {
                let xs: Vec<f64> = l.vertices.iter().map(|v| v.p.x).collect();
                let ys: Vec<f64> = l.vertices.iter().map(|v| v.p.y).collect();
                let (x0, x1) = (xs.iter().copied().fold(f64::MAX, f64::min), xs.iter().copied().fold(f64::MIN, f64::max));
                let (y0, y1) = (ys.iter().copied().fold(f64::MAX, f64::min), ys.iter().copied().fold(f64::MIN, f64::max));
                (x0..=x1).contains(&t.insert.x) && (y0..=y1).contains(&t.insert.y)
            }
            _ => false,
        });
        assert!(boxed, "a box round the ? at {:?}", t.insert);
    }
}

#[test]
fn a_missing_value_prints_black_in_screen_colours() {
    let (mut h, bom) = bom_with_missing();
    let doc = h.doc();
    let outputs = Outputs::build(&doc);
    let (scratch, _) = ecad_cadcraft::printbook::build(&doc, &[bom], &BTreeSet::new(), 1e-6, Some(&outputs)).unwrap();
    let generated: Vec<_> = scratch.model.iter().filter(|e| e.common.layer == LAYER).collect();
    assert!(
        generated.iter().any(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == MISSING) && e.common.color == Color::Index(FLAGGED_PRINT_ACI))
    );
    assert!(generated.iter().all(|e| e.common.color != Color::Index(FLAGGED_ACI)), "no yellow left in print");
}
