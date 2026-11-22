//! Lane 057 (wire and cable rating checks): the Rating sections of Project ▸ Wires and Cables
//! (boards 2.05 and 2.06, design 057 §2 and §9), on the demo project. With `ECAD_TESTER_SHOTS`
//! set, writes `057-2.05.png` and `057-2.06.png` for the side-by-sides.
//!
//! **Every rating value here is synthetic** (made up for the test; none is from a standard).

use ecad_app::App;
use ecad_app::shell::ListsView;
use ecad_app::shell::state::Workspace;
use eframe::egui::{self, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::canvas::mouse_acceptance::{Collect, Deltas};
use crate::pages::project_panel::{doc, ok, rev3_app as new_app};

type H = Harness<'static, App>;

/// The synthetic rating table (not from any standard).
const SYNTHETIC: &str =
    "# synthetic values for tests; not from any standard\\ncapacity,E,pvc,3,1.5,20\\ngrouping,E,3,0.90\\nresistance,5,1.5,10.0\\n";

fn harness(deltas: Deltas) -> H {
    let mut h = Harness::builder().with_size(vec2(1920.0, 1080.0)).with_step_dt(1.0 / 60.0).renderer(Collect(deltas)).build_ui_state(
        |ui, t: &mut App| {
            let r = ui.ctx().content_rect();
            ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
                ui.set_clip_rect(r);
                t.logic(&ui.ctx().clone());
                t.ui(ui);
            });
        },
        new_app(),
    );
    h.run_steps(5);
    h
}

fn show(h: &mut H, v: ListsView) {
    h.state_mut().set_workspace(Workspace::Lists);
    h.state_mut().show_project_tab(v);
    h.run_steps(6);
}

/// The demo, rated: Power L rated 27 A at 30 °C (synthetic), panel 40 °C, Q01 4 A, the L wire
/// from Q01:2 bundled 0.80; -W01 on the synthetic table, method E, 35 °C, 3 grouped, 12 m,
/// protected by Q01.
fn rated(h: &mut H, dir: &std::path::Path) -> (ecad_model::WireId, ecad_model::CableId) {
    let path = crate::checks::ui::demo_file(dir);
    h.state_mut().close_project();
    h.run_steps(2);
    h.state_mut().open_project(&path).unwrap();
    h.run_steps(3);
    let d = doc(h);
    let q01 = d.devices.iter().find(|(_, c)| c.tag().is_some_and(|t| t.trim_start_matches('-') == "Q01")).map(|(id, _)| *id).unwrap();
    let tag = d.devices[&q01].tag().unwrap().to_string();
    // The L wire leaving Q01:2 (30, 110) downwards.
    let wire = d
        .wires
        .iter()
        .find(|(_, c)| {
            c.potential_name() == Some("L")
                && c.ends.iter().any(|t| d.connection_points.get(t).is_some_and(|x| x.device() == Some(q01) && x.label.as_deref() == Some("2")))
        })
        .map(|(id, _)| *id)
        .unwrap();
    let w01 = d.cables_tagged("-W01")[0];
    ok(h, &format!(r#"setprop {{"component":"{}","key":"rating","value":"4A C"}}"#, q01.hex()));
    ok(h, r#"setstyle {"catalogue":"wire","id":"l","fields":{"rated_current":"27","rated_at":"30","max_temperature":"70"}}"#);
    ok(h, r#"setdefault {"key":"rating.panel_temperature","level":"project","value":"40"}"#);
    ok(h, &format!(r#"rating {{"wire":"{}","bundling":"0.80"}}"#, wire.hex()));
    ok(h, &format!(r#"ratingtable {{"name":"Test (synthetic)","source":"made up for tests","csv":"{SYNTHETIC}"}}"#));
    let d = doc(h);
    let table = ecad_electrical::styles::rows(&d, &ecad_electrical::styles::ratingtable::CATALOGUE)[0].id.text();
    ok(h, &format!(r#"setdefault {{"key":"rating.table","level":"project","value":"{table}"}}"#));
    ok(h, &format!(r#"setprop {{"cable":"{}","key":"length","value":"12 m"}}"#, w01.hex()));
    ok(h, &format!(r#"setprop {{"cable":"{}","key":"type","value":"H07RN-F 4G1.5"}}"#, w01.hex()));
    ok(h, &format!(r#"rating {{"cable":"{}","basis":"table","method":"E","ambient":35,"grouped":3,"protected_by":"{tag}"}}"#, w01.hex()));
    (wire, w01)
}

#[test]
fn the_rating_sections_show_boards_2_05_and_2_06() {
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = harness(deltas.clone());
    let dir = std::env::temp_dir().join(format!("ecad-057-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (wire, w01) = rated(&mut h, &dir);
    // 2.05: the wire, its Rating section after Made as.
    show(&mut h, ListsView::Wires);
    let k = h.state().reports.as_ref().unwrap().wires.source.ids.iter().position(|c| *c == wire).unwrap();
    h.state_mut().reports.as_mut().unwrap().wires.table.selection.set([k]);
    h.state_mut().reports.as_mut().unwrap().wires.table.reveal(&[k]);
    h.run_steps(8);
    assert!(h.query_by_label_contains("Rated: 27 A at 30 \u{b0}C").is_some(), "Rated row");
    assert!(h.query_by_label_contains("Panel: \u{d7} 0.87 40 \u{b0}C").is_some(), "Panel row");
    assert!(h.query_by_label_contains("Bundled: \u{d7} 0.80").is_some(), "Bundled row");
    assert!(h.query_by_label_contains("Carries: 18.8 A").is_some(), "Carries row");
    if let Some(out) = crate::pages::project_panel::shots() {
        let _ = crate::canvas::mouse_acceptance::shot(&h, &deltas, &out.join("057-2.05.png"));
    }
    // 2.06: the cable, its Rating section after Part.
    show(&mut h, ListsView::Cables);
    let k = h.state().reports.as_ref().unwrap().cables.ids.iter().position(|c| *c == w01).unwrap();
    h.state_mut().reports.as_mut().unwrap().cables.table.selection.set([k]);
    h.run_steps(8);
    assert!(h.query_by_label_contains("Table: 20 A 1.5 mm\u{b2} \u{b7} E \u{b7} Test (synthetic)").is_some(), "Table row");
    assert!(h.query_by_label_contains("Carries: 16.9 A").is_some(), "Carries row");
    assert!(h.query_by_label_contains("Volt drop: 0.2 %").is_some(), "Volt drop row");
    if let Some(out) = crate::pages::project_panel::shots() {
        let _ = crate::canvas::mouse_acceptance::shot(&h, &deltas, &out.join("057-2.06.png"));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Board 2.06's drawn cable (owner, added to 057): each core in its cable type's colour, the
/// Colour column filled, Cores only hides the drawing, Swap ends mirrors it.
#[test]
fn the_cable_is_drawn_end_to_end_in_its_core_colours() {
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = harness(deltas.clone());
    let dir = std::env::temp_dir().join(format!("ecad-057-cable-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (_, w01) = rated(&mut h, &dir);
    ok(&mut h, r#"setstyle {"catalogue":"cable","new":true,"name":"H07RN-F 4G1.5","fields":{"size":"1.5","core_colours":"BN,BK,GY,GNYE"}}"#);
    let d = doc(&mut h);
    let ty = ecad_electrical::styles::rows(&d, &ecad_electrical::styles::cable::CATALOGUE)[0].id.text();
    ok(&mut h, &format!(r#"rating {{"cable":"{}","cable_type":"{ty}"}}"#, w01.hex()));
    show(&mut h, ListsView::Cables);
    let k = h.state().reports.as_ref().unwrap().cables.ids.iter().position(|c| *c == w01).unwrap();
    h.state_mut().reports.as_mut().unwrap().cables.table.selection.set([k]);
    h.run_steps(8);
    // Owner: a plain list until Details opens the selected cable below it.
    assert!(h.query_by_label_contains("drawn end to end").is_none(), "a plain list");
    if let Some(out) = crate::pages::project_panel::shots() {
        let _ = crate::canvas::mouse_acceptance::shot(&h, &deltas, &out.join("057-cable-view-v2-list.png"));
    }
    h.get_by_label(ecad_app::workspace::frame::DETAILS).click();
    h.run_steps(8);
    assert!(h.query_by_label_contains("-W01 drawn end to end, 4 cores").is_some(), "the drawing");
    let r = h.state().reports.as_ref().unwrap();
    let colours: Vec<Option<String>> = r.cables.cores.rows.keys.iter().map(|i| r.cables.cores.colour(*i)).collect();
    assert_eq!(colours, ["BN", "BK", "GY", "GNYE"].map(|c| Some(c.to_string())).to_vec());
    let drawing = r.cables.drawing(k);
    assert_eq!(drawing.cores.iter().map(|c| c.colour.clone().unwrap_or_default()).collect::<Vec<_>>(), ["BN", "BK", "GY", "GNYE"]);
    // Owner rule: the demo's unwired GNYE core is an open end, an error, until ticked Not used.
    let r = h.state().reports.as_ref().unwrap();
    let gnye = r.cables.cores.rows.keys.iter().copied().find(|i| r.cables.cores.rows.text(*i, "Core") == "GNYE").unwrap();
    assert!(r.cables.cores.is_open(gnye));
    let open_rows = |h: &mut H| doc_checks(h).into_iter().filter(|c| c.kind == ecad_electrical::checks::Kind::CoreOpen).count();
    assert_eq!(open_rows(&mut h), 1);
    assert!(h.query_by_label_contains("Field end").is_some(), "the board's columns");
    // v3: the row's fixes and the panel's buttons, the shared splitter.
    let fix = doc_checks(&mut h).into_iter().find(|c| c.kind == ecad_electrical::checks::Kind::CoreOpen).and_then(|c| c.fix);
    assert_eq!(fix, Some(ecad_electrical::checks::Fix::CoreOpen(w01, 0)));
    assert!(h.query_by_label(ecad_app::checks::panel::GO_TO_OPEN_END).is_some(), "the panel's Go to the open end");
    assert!(h.query_by_label(ecad_app::cables::list::SPLIT).is_some(), "the splitter");
    if let Some(out) = crate::pages::project_panel::shots() {
        let _ = crate::canvas::mouse_acceptance::shot(&h, &deltas, &out.join("057-cable-view-v3.png"));
    }
    if let Some(out) = crate::pages::project_panel::shots() {
        let _ = crate::canvas::mouse_acceptance::shot(&h, &deltas, &out.join("057-cable-view-v2-open.png"));
    }
    // The GNYE row is the last of the four.
    h.get_all_by_label(ecad_app::cables::list::NOT_USED_TOGGLE).last().unwrap().click();
    h.run_steps(6);
    assert!(h.state().reports.as_ref().unwrap().cables.cores.is_unused(gnye), "ticked");
    assert_eq!(open_rows(&mut h), 0, "a ticked core is no error");
    let d = doc(&mut h);
    assert!(d.cables[&w01].cores.iter().any(|m| m.label_str() == Some("GNYE") && m.is_spare()), "the core is stored Spare");
    if let Some(out) = crate::pages::project_panel::shots() {
        let _ = crate::canvas::mouse_acceptance::shot(&h, &deltas, &out.join("057-cable-view-v2-ticked.png"));
    }
    ok(&mut h, "u");
    assert_eq!(open_rows(&mut h), 1, "one undo takes the tick back");
    // Cores only hides it; Swap ends keeps it, mirrored.
    h.get_by_label("Cores only").click();
    h.run_steps(4);
    assert!(h.query_by_label_contains("drawn end to end").is_none());
    h.get_by_label("Cable view").click();
    h.get_by_label("Swap ends").click();
    h.run_steps(4);
    assert!(h.state().reports.as_ref().unwrap().cables.swapped);
    assert!(h.query_by_label_contains("drawn end to end").is_some());
    // Esc closes the details; the list stays.
    h.key_press(egui::Key::Escape);
    h.run_steps(4);
    assert!(!h.state().reports.as_ref().unwrap().cables.details);
    let _ = std::fs::remove_dir_all(&dir);
}

fn doc_checks(h: &mut H) -> Vec<ecad_electrical::checks::Check> {
    let d = doc(h);
    ecad_engine::checks::all(&d, 0.01)
}
