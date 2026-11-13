//! Tester 050 in the trial, headless (egui_kittest): design 050 §12 check 9 and brief 050b AC 6,
//! the update map's counters (042's `rebuilds` / `patches`): a `style` change patches the
//! Standards tables and rebuilds none; a commit that touches no style or registry key leaves them
//! alone; the catalogue strip names each catalogue (§9). Written from the briefs and design before
//! the implementation; the table's field names are 042's.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::mouse_tester::{harness_with, ok};

type H = Harness<'static, ecad_trial::Trial>;

fn counters(h: &H) -> (u64, u64) {
    let v = &h.state().settings.standards;
    (v.rebuilds, v.patches)
}

fn shown_standards() -> H {
    let mut h = harness_with(None);
    h.state_mut().set_cad_chrome(false);
    ok(&mut h, r#"place {"symbol":"coil","at":[100,100]}"#);
    ok(&mut h, r#"wire {"points":[[0,50],[60,50]],"potential":"24V"}"#);
    h.state_mut().open_standards(None);
    h.run_steps(3);
    h
}

/// §12.9: `style` on `tag` patches (`patches +1`, `rebuilds +0`); an unrelated commit touches
/// nothing; a standard value and `SetStandard` patch, never rebuild.
#[test]
#[ignore = "tester 050: awaiting build"]
fn a09_style_changes_patch_the_standards_tables_and_rebuild_none() {
    let mut h = shown_standards();
    let (rebuilds, patches) = counters(&h);
    assert_eq!(rebuilds, 1, "built on first show");
    ok(&mut h, r#"wire {"points":[[0,150],[40,150]]}"#);
    h.run_steps(2);
    assert_eq!(counters(&h), (rebuilds, patches), "a commit touching no style or key leaves the tables alone");
    ok(&mut h, r#"style {"catalogue":"text","id":"tag","fields":{"height":3}}"#);
    h.run_steps(2);
    assert_eq!(counters(&h), (rebuilds, patches + 1), "one patch");
    ok(&mut h, r#"standard {"new":"House"}"#);
    h.run_steps(2);
    ok(&mut h, r#"setdefault {"key":"symbol.coil.prefix","level":"standard","value":"Q"}"#);
    h.run_steps(2);
    let (r, _) = counters(&h);
    assert_eq!(r, rebuilds, "no rebuild for a standard or a standard value");
    ok(&mut h, "u");
    h.run_steps(2);
    assert_eq!(counters(&h).0, rebuilds, "undo patches too");
}

/// Brief 050b AC 6: a category change patches, rebuilds nothing.
#[test]
#[ignore = "tester 050: awaiting build"]
fn b_a_category_change_patches_and_rebuilds_none() {
    let mut h = shown_standards();
    let (rebuilds, patches) = counters(&h);
    ok(&mut h, r#"style {"catalogue":"wire","id":"dc24","fields":{"colour":"RD"}}"#);
    h.run_steps(2);
    assert_eq!(counters(&h), (rebuilds, patches + 1));
    let c = crate::mouse_tester::doc(&mut h).conductors.keys().next().copied().unwrap();
    ok(&mut h, &format!(r#"category {{"conductor":"{}","value":"signal"}}"#, c.hex()));
    h.run_steps(2);
    assert_eq!(counters(&h).0, rebuilds);
}

/// §9: the catalogue strip names Defaults, Text styles, Location boxes, Frames and (050b) Wire
/// categories; the standard header shows "IEC (built in)" and its Make button.
#[test]
#[ignore = "tester 050: awaiting build"]
fn a09_the_catalogue_strip_and_the_header_are_named() {
    let mut h = shown_standards();
    for word in ["defaults", "text", "location", "frame", "wire"] {
        let name = format!("ecad.standards.catalogue.{word}");
        assert!(h.query_by_label(&name).is_some(), "{name}");
    }
    assert!(h.query_by_label("ecad.standards.header").is_some() || h.query_by_label_contains("IEC (built in)").is_some());
    assert!(h.query_by_label("ecad.standards.header.make").is_some() || h.query_by_label_contains("Make a standard").is_some());
}
