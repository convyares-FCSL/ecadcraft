//! A report page of several sheets in the app: the Pages tree lists its sheets under it while it
//! is shown, a click on a sheet shows that sheet, and a go-to into a later sheet shows that sheet
//! centred on its own point.

use ecad_model::PageId;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use super::project_panel::{build, doc, new_app, ok};
use ecad_app::shell::app::App;

fn shown(h: &mut Harness<'static, App>, page: PageId) -> (usize, usize) {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.shown_sheet(page)).unwrap()
}

#[test]
fn the_tree_lists_a_report_pages_sheets_and_a_go_to_shows_the_right_one() {
    let mut h = build(new_app(), None);
    h.state_mut().cad.session.viewport_px = (1280.0, 800.0);
    let first = doc(&mut h).page_order()[0];
    ok(&mut h, r#"newpage {"generated":"bom-location","title":"Parts"}"#);
    let bom = doc(&mut h).page_order()[1];
    ok(&mut h, &format!("openpage {}", json!({"page": first.hex()})));
    // One heading and 50 rows: two sheets.
    for i in 0..50 {
        ok(&mut h, &format!("place {}", json!({"symbol": "coil", "at": [10 + (i % 12) * 30, 40 + (i / 12) * 30]})));
    }
    assert!(h.query_all_by_label_contains("Sheet 2 of 2").next().is_none(), "sheets are listed only under the shown page");
    ok(&mut h, &format!("openpage {}", json!({"page": bom.hex()})));
    h.run_steps(3);
    assert!(h.query_all_by_label_contains("Sheet 1 of 2").next().is_some(), "the shown report page lists its sheets");
    h.query_all_by_label_contains("Sheet 2 of 2").next().expect("sheet 2's row").click();
    h.run_steps(3);
    assert_eq!(shown(&mut h, bom), (1, 2));

    // A go-to into the first sheet of the side-by-side layout shows it, at its own point.
    h.state_mut().goto_place(bom, ecad_model::Pt::new(30.0, 100.0));
    h.run_steps(2);
    assert_eq!(shown(&mut h, bom), (0, 2));
    // And into the second (it starts at 420 + 50 mm): the view is centred on its point there.
    h.state_mut().goto_place(bom, ecad_model::Pt::new(470.0 + 30.0, 100.0));
    h.run_steps(2);
    assert_eq!(shown(&mut h, bom), (1, 2));
    let centre = h.state().cad.session.state().map(|s| s.view().center).unwrap();
    assert!((centre.x - 30.0).abs() < 1e-6 && (centre.y - 100.0).abs() < 1e-6, "{centre:?}");
}
