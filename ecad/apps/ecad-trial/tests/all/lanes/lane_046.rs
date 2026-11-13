//! Brief 046 in the trial (design 046 §8.1, §11): Pages ▸ New page ▸ **Terminal strips** makes a
//! graphic `terminals` page and opens it. Screenshots of fixture G's strip page (paper and dark)
//! and of a long strip's second sheet are written when `ECAD_TESTER_SHOTS` names a folder and a
//! wgpu adapter exists; they are for the owner's approval, never committed.

use std::sync::Arc;

use ecad_model::{ComponentId, PageId};
use ecad_trial::{Trial, project_panel};
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

fn hex_id<T: From<u128>>(v: &Value, key: &str) -> T {
    let s = v[key].as_str().unwrap_or_else(|| panic!("{key} in {v}"));
    let hex = s.rsplit('-').next().unwrap_or(s);
    T::from(u128::from_str_radix(hex, 16).unwrap())
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

/// Fixture G (design 046 §11), as `ecad-cadcraft`'s `lane_046` builds it.
fn fixture_g(h: &mut H) -> ComponentId {
    exec(h, "locationbox", json!({"location": "+CP1", "corners": [[20, 150], [400, 280]]}));
    exec(h, "locationbox", json!({"location": "+FIELD", "corners": [[20, 20], [400, 140]]}));
    let k1: ComponentId = hex_id(&exec(h, "place", json!({"symbol": "coil", "at": [100, 220]})), "component");
    let k2: ComponentId = hex_id(&exec(h, "place", json!({"symbol": "coil", "at": [150, 220]})), "component");
    exec(h, "place", json!({"symbol": "no", "at": [200, 220], "device": k1.hex()}));
    exec(h, "place", json!({"symbol": "no", "at": [250, 220], "device": k2.hex()}));
    let m1: ComponentId = hex_id(&exec(h, "place", json!({"symbol": "motor3", "at": [110, 50]})), "component");
    exec(h, "setprop", json!({"component": m1.hex(), "key": "description", "value": "Pump"}));
    exec(h, "place", json!({"symbol": "pbno", "at": [250, 60]}));
    for r in [
        json!([[100, 210], [100, 65]]),
        json!([[150, 210], [150, 100], [110, 100], [110, 65]]),
        json!([[200, 210], [200, 95], [120, 95], [120, 65]]),
        json!([[250, 210], [250, 70]]),
    ] {
        exec(h, "wire", json!({"points": r}));
    }
    let v = exec(h, "cableslash", json!({"from": [90, 120], "to": [260, 120], "tag": "-W01"}));
    let cable = v["cable"].as_str().unwrap().to_string();
    exec(h, "setprop", json!({"cable": cable, "key": "type", "value": "YSLY-JZ 7G1.5"}));
    let v = exec(h, "terminals", json!({"from": [90, 170], "to": [260, 170]}));
    let x1: ComponentId = hex_id(&v, "strip");
    let terms: Vec<String> = v["terminals"].as_array().unwrap().iter().map(|t| t["terminal"].as_str().unwrap().to_string()).collect();
    for x in [300, 340] {
        exec(h, "wire", json!({"points": [[x, 260], [x, 160]]}));
    }
    let s = x1.hex();
    exec(h, "terminals", json!({"strip": s, "type": "PE", "level": "All", "from": [290, 200], "to": [310, 200]}));
    let f = exec(h, "terminals", json!({"strip": s, "type": "FUSE", "level": "All", "from": [330, 200], "to": [350, 200]}));
    let fuse = f["terminals"][0]["label"].as_str().unwrap_or("6").to_string();
    exec(h, "setprop", json!({"component": s, "key": "terminals", "value": "12+2PE+3LNPE+2FUSE"}));
    exec(h, "setprop", json!({"component": s, "key": "link.1", "value": "1-4"}));
    exec(h, "setprop", json!({"component": s, "key": format!("rating.{fuse}"), "value": "2A"}));
    exec(h, "bridge", json!({"from": terms[2], "to": terms[3]}));
    exec(h, "accessory", json!({"strip": s, "at": "start", "items": ["end_stop"]}));
    exec(h, "accessory", json!({"strip": s, "at": "end", "items": ["end_stop"]}));
    exec(h, "accessory", json!({"strip": s, "at": "2", "items": ["partition"]}));
    exec(h, "accessory", json!({"strip": s, "at": ["1", "2", "3"], "on": true, "items": ["marker"]}));
    x1
}

#[test]
fn new_page_terminal_strips_makes_and_opens_a_graphic_page() {
    let deltas: crate::mouse_tester::Deltas = Arc::default();
    let mut h = Harness::builder().with_size(vec2(1920.0, 1080.0)).renderer(crate::mouse_tester::Collect(deltas.clone())).build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        trial(),
    );
    h.run_steps(3);
    let x1 = fixture_g(&mut h);
    let pages = doc(&mut h).page_order.len();
    h.state_mut().panel_action(project_panel::Action::NewStripPage);
    h.run_steps(4);
    let d = doc(&mut h);
    assert_eq!(d.page_order.len(), pages + 1, "one new page");
    let page: PageId = *d.page_order.last().unwrap();
    assert_eq!(d.pages[&page].title, ecad_cadcraft::strip_page::TITLE);
    assert!(ecad_cadcraft::strip_page::is_graphic(&d, page));
    let Some(out) = shots() else { return };
    let _ = std::fs::create_dir_all(&out);
    view(&mut h, 0.0, 0.0, 420.0, 297.0);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("046-strip-page.png"));
    h.state_mut().sheet_command("ui.sheet dark");
    h.run_steps(4);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("046-strip-page-dark.png"));
    h.state_mut().sheet_command("ui.sheet paper");
    h.run_steps(2);
    // A detail: the cable box and the far device.
    view(&mut h, 20.0, 60.0, 150.0, 200.0);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("046-strip-detail.png"));
    // A 45-position strip's second sheet, with a link over the break.
    exec(&mut h, "setprop", json!({"component": x1.hex(), "key": "terminals", "value": "45"}));
    exec(&mut h, "setprop", json!({"component": x1.hex(), "key": "link.2", "value": "30-45"}));
    view(&mut h, 470.0, 0.0, 890.0, 297.0);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("046-strip-sheet2.png"));
    view(&mut h, 0.0, 0.0, 420.0, 297.0);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("046-strip-sheet1.png"));
}
