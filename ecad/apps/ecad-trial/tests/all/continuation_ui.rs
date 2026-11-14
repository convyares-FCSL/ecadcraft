//! Brief 049 in the trial (review of #85, 3): continuation arrows on a page as drawn. Screenshots of
//! an arrow's other-end text, a caption run under its bracket, and the run split in two are written
//! when `ECAD_TESTER_SHOTS` names a folder and a wgpu adapter exists; they are for review, never
//! committed.

use std::sync::Arc;

use ecad_model::PageId;
use ecad_trial::Trial;
use eframe::egui::vec2;
use egui_kittest::Harness;
use serde_json::{Value, json};

use crate::project_panel::{doc, shots, trial};

type H = Harness<'static, Trial>;

fn exec(h: &mut H, cmd: &str, params: Value) -> Value {
    let v = h.state_mut().cad.session.execute(cmd, &params).unwrap_or_else(|e| panic!("{cmd} {params}: {e}"));
    h.run_steps(2);
    v
}

/// Look at model x0..x1 by y0..y1 (millimetres).
fn view(h: &mut H, x0: f64, y0: f64, x1: f64, y1: f64) {
    let (w, hpx) = (1920.0, 1080.0);
    let height = (y1 - y0).max((x1 - x0) * hpx / w) * 1.08;
    if let Ok(st) = h.state_mut().cad.session.state_mut() {
        let centre = cadcraft_engine::geom::Vec2::new((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let _ = st.set_view(cadcraft_engine::View { center: centre, height, twist: 0.0 });
    }
    h.run_steps(4);
}

fn page_of(v: &Value) -> PageId {
    let s = v["page"].as_str().unwrap();
    PageId(u128::from_str_radix(s.rsplit('-').next().unwrap_or(s), 16).unwrap())
}

#[test]
#[ignore = "slow: a 1920×1080 trial with three pages, about 4 s; run by name for the screenshots"]
fn continuation_arrows_draw_their_other_ends_and_captions() {
    let deltas: crate::mouse_tester::Deltas = Arc::default();
    let mut h = Harness::builder().with_size(vec2(1920.0, 1080.0)).renderer(crate::mouse_tester::Collect(deltas.clone())).build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        trial(),
    );
    h.run_steps(3);
    let first = doc(&mut h).page_order[0];
    // Two more pages with the other ends of three supply arrows.
    let p2 = page_of(&exec(&mut h, "newpage", json!({"title": "Control"})));
    let p3 = page_of(&exec(&mut h, "newpage", json!({"title": "Outputs"})));
    for (page, x) in [(p2, 120.0), (p3, 200.0)] {
        exec(&mut h, "openpage", json!({"page": page.hex()}));
        for (i, name) in ["24V", "0V", "L1"].into_iter().enumerate() {
            exec(&mut h, "place", json!({"symbol": "continuation", "at": [x + 5.0 * i as f64, 150.0], "name": name}));
        }
    }
    // Page 1: three wires down to three arrows 5 mm apart, under one caption.
    exec(&mut h, "openpage", json!({"page": first.hex()}));
    let mut arrows = Vec::new();
    for (i, name) in ["24V", "0V", "L1"].into_iter().enumerate() {
        let x = 100.0 + 5.0 * i as f64;
        exec(&mut h, "wire", json!({"points": [[x, 200.0], [x, 150.0]]}));
        let v = exec(&mut h, "place", json!({"symbol": "continuation", "at": [x, 150.0], "name": name}));
        arrows.push(v["component"].as_str().unwrap().to_string());
    }
    let d = doc(&mut h);
    let on_first = ecad_cadcraft::xref::parts_on_page(&d, first);
    assert_eq!(on_first.len(), 3, "one other-end text per arrow");
    exec(&mut h, "contcaption", json!({"continuations": arrows, "caption": "TO CONTROL SUPPLY"}));
    let d = doc(&mut h);
    assert_eq!(ecad_cadcraft::xref::parts_on_page(&d, first).iter().filter(|p| p.index == 1).count(), 1, "one bracket");
    let Some(out) = shots() else { return };
    let _ = std::fs::create_dir_all(&out);
    view(&mut h, 80.0, 105.0, 135.0, 160.0);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("049-run.png"));
    view(&mut h, 95.0, 125.0, 105.0, 155.0);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("049-reference.png"));
    h.state_mut().sheet_command("ui.sheet dark");
    h.run_steps(4);
    view(&mut h, 80.0, 105.0, 135.0, 160.0);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("049-run-dark.png"));
    h.state_mut().sheet_command("ui.sheet paper");
    h.run_steps(2);
    // The L1 arrow deleted and placed again 45 mm to the right with the same caption: two
    // brackets, the caption under each.
    exec(&mut h, "ecaddelete", json!({"component": arrows[2]}));
    exec(&mut h, "wire", json!({"points": [[155.0, 200.0], [155.0, 150.0]]}));
    exec(&mut h, "place", json!({"symbol": "continuation", "at": [155.0, 150.0], "name": "L1", "caption": "TO CONTROL SUPPLY"}));
    let d = doc(&mut h);
    assert_eq!(ecad_cadcraft::xref::parts_on_page(&d, first).iter().filter(|p| p.index == 1).count(), 2, "two brackets");
    view(&mut h, 80.0, 105.0, 175.0, 160.0);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("049-split.png"));
}
