//! Brief 025 headless in the trial (design 025 §10, screenshots for the owner's approval): fixture
//! X built through the ECAD command field, the coil's table with a spare and an over row, the
//! contact texts on page 105, the Properties palette with the coil selected, and the Checks row.
//! Screenshots are written when `ECAD_TESTER_SHOTS` names a folder and a wgpu adapter exists;
//! they are never committed.

use std::path::PathBuf;
use std::sync::Arc;

use ecad_model::PageId;
use ecad_trial::checks_panel;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

use crate::mouse_tester::{Deltas, harness_with, shot};

type H = Harness<'static, ecad_trial::Trial>;

fn out() -> Option<PathBuf> {
    let dir = std::env::var_os("ECAD_TESTER_SHOTS").map(PathBuf::from)?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn save(h: &H, deltas: &Deltas, name: &str) {
    if let Some(dir) = out()
        && shot(h, deltas, &dir.join(name)).is_none()
    {
        eprintln!("no wgpu adapter: {name} skipped");
    }
}

fn run(h: &mut H, cmd: &str, params: Value) -> Value {
    let v = h.state_mut().cad.run(cmd, params.clone()).unwrap_or_else(|e| panic!("{cmd} {params}: {e}"));
    h.run_steps(2);
    v
}

/// Show the box (x0, y0)-(x1, y1) in the part of the window the panels leave visible.
fn view(h: &mut H, x0: f64, y0: f64, x1: f64, y1: f64) {
    let height = (y1 - y0).max((x1 - x0) * 800.0 / 520.0);
    let per_px = height / 800.0;
    if let Ok(st) = h.state_mut().cad.session.state_mut() {
        let centre = cadcraft_engine::geom::Vec2::new((x0 + x1) / 2.0 - 70.0 * per_px, (y0 + y1) / 2.0);
        let _ = st.set_view(cadcraft_engine::View { center: centre, height, twist: 0.0 });
    }
    h.run_steps(3);
}

fn hex(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap().to_string()
}

#[test]
fn the_coil_table_the_contact_texts_properties_and_checks() {
    let deltas: Deltas = Arc::default();
    let mut h = harness_with(Some(deltas.clone()));
    h.state_mut().cad.session.viewport_px = (1280.0, 800.0);
    h.run_steps(3);
    let p100: PageId = {
        let t = h.state_mut();
        let p = t.project().unwrap().clone();
        p.read(&mut t.cad.session, |p, _| p.ledger.document().page_order[0]).unwrap()
    };
    run(&mut h, "pageprops", json!({"page": p100.hex(), "number": "100", "title": "Supply"}));
    let coil = run(&mut h, "place", json!({"symbol": "coil", "at": [75, 180]}));
    let k1 = hex(&coil, "component");
    run(&mut h, "wire", json!({"points": [[75, 170], [75, 150]]}));
    run(&mut h, "wire", json!({"points": [[40, 150], [200, 150]]}));
    run(&mut h, "place", json!({"symbol": "power3", "at": [125, 180], "device": k1}));
    run(&mut h, "setprop", json!({"component": k1, "key": "contacts", "value": "3P+2NO+1NC"}));
    let p105 = hex(&run(&mut h, "newpage", json!({"title": "Motor control", "number": "105"})), "page");
    run(&mut h, "place", json!({"symbol": "no", "at": [100, 150], "device": k1}));
    run(&mut h, "place", json!({"symbol": "nc", "at": [100, 100], "device": k1}));
    run(&mut h, "place", json!({"symbol": "no", "at": [150, 150], "device": k1}));
    run(&mut h, "place", json!({"symbol": "no", "at": [200, 150], "device": k1}));
    let mcb = run(&mut h, "place", json!({"symbol": "mcb1", "at": [250, 150]}));
    run(&mut h, "place", json!({"symbol": "nc", "at": [300, 150], "device": hex(&mcb, "component")}));

    // Page 105: every contact shows its coil's or its MCB's place.
    view(&mut h, 80.0, 70.0, 320.0, 175.0);
    let texts: Vec<String> = h
        .state()
        .cad
        .session
        .doc()
        .unwrap()
        .model
        .iter()
        .filter_map(|e| match &e.kind {
            cadcraft_engine::doc::EntityKind::Text(t) if e.common.layer == ecad_cadcraft::devices::TAG_LAYER => Some(t.value.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(texts.iter().filter(|t| *t == "100/C2").count(), 4, "{texts:?}");
    save(&h, &deltas, "025-contacts.png");

    // Page 100: the table under the rail, with the spare row gone and one over row.
    run(&mut h, "openpage", json!({"page": p100.hex()}));
    view(&mut h, 40.0, 110.0, 160.0, 200.0);
    save(&h, &deltas, "025-coil-table.png");

    // The coil selected: its Properties section.
    let coil_handle = {
        let t = h.state_mut();
        let p = t.project().unwrap().clone();
        let rep: ecad_model::RepresentationId = ecad_model::RepresentationId(u128::from_str_radix(&hex(&coil, "representation"), 16).unwrap());
        p.read(&mut t.cad.session, |p, _| {
            let item = p.ledger.document().representations[&rep].items[0];
            p.pages[&p100].map.item(item).unwrap()
        })
        .unwrap()
    };
    h.state_mut().cad.session.set_selection(vec![coil_handle]);
    h.run_steps(4);
    let rows: Vec<(String, String)> =
        h.state().props.lock().unwrap().view.sections.iter().flat_map(|s| s.rows.iter().map(|r| (r.label.clone(), r.value.clone()))).collect();
    assert!(rows.iter().any(|(l, v)| l == "Contacts offered" && v == "3P+2NO+1NC"), "{rows:?}");
    assert!(rows.iter().any(|(l, v)| l == "Contacts" && v.contains("43/44 NO") && v.contains("beyond the part")), "{rows:?}");
    save(&h, &deltas, "025-props.png");
    h.state_mut().cad.session.set_selection(Vec::new());
    h.run_steps(2);

    // The Checks tab: one too-many-contacts row; a click goes to page 105.
    crate::project_panel::click(&mut h, checks_panel::CHECKS_TAB);
    // UI rev 3: Checks is the inspector's tab; the whole list is Project ▸ Check list.
    assert_eq!(h.state().shell_state().right_tab, ecad_trial::shell::RightTab::Checks);
    h.state_mut().set_workspace(ecad_trial::shell::Workspace::Lists);
    h.state_mut().shell.lists = ecad_trial::shell::ListsView::Checks;
    h.run_steps(3);
    let rows = h.state().panel.checks.rows().to_vec();
    let row = rows.iter().find(|r| r.check.kind == ecad_cadcraft::checks::Kind::TooManyContacts).expect("the too-many-contacts row").clone();
    h.get_by_label(&checks_panel::row_label(&row)).click();
    h.run_steps(3);
    assert_eq!(h.state().panel.active.map(|p| p.hex()), Some(p105));
    save(&h, &deltas, "025-checks.png");
}
