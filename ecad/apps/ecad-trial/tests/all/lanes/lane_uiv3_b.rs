//! UI rev 3, section B: the inspector (boards 1.05 Device, 1.06 Layout, 1.07 Checks). The demo
//! project's contactor coil -K01 on page 2, as on the boards: the header, the Device tab's
//! folding sections, the Layout tab's placement and texts, the Checks tab's scopes, and the
//! screenshots for the side-by-side with the boards (written only when `ECAD_TESTER_SHOTS` names a
//! folder and a wgpu adapter exists).

use crate::project_panel::{doc, rev3_trial as trial};
use ecad_model::{RepresentationId, Subject};
use ecad_trial::Trial;
use ecad_trial::shell::right;
use ecad_trial::shell::state::RightTab;
use eframe::egui::{self, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

type H = Harness<'static, Trial>;

/// One demo project open at a time: the demo's copies are one project to the store's lock.
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn harness(deltas: Option<crate::mouse_tester::Deltas>) -> (H, std::path::PathBuf, std::sync::MutexGuard<'static, ()>) {
    let guard = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    ecad_trial::canvas::register_sheet_faces();
    let dir = std::env::temp_dir().join(format!("ecad-uiv3-b-{}-{}", std::process::id(), rand_suffix()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = crate::checks_ui::demo_file(&dir);
    let mut b = Harness::builder().with_size(vec2(1920.0, 1080.0));
    if let Some(d) = deltas {
        b = b.renderer(crate::mouse_tester::Collect(d));
    }
    // The whole window, as the app's root `Ui` (the harness keeps an 8 px margin round its own).
    let mut h = b.build_ui_state(
        |ui, t: &mut Trial| {
            let r = ui.ctx().content_rect();
            ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
                ui.set_clip_rect(r);
                t.logic(&ui.ctx().clone());
                t.ui(ui);
            });
        },
        trial(),
    );
    h.run_steps(4);
    h.state_mut().close_project();
    h.run_steps(2);
    h.state_mut().open_project(&path).unwrap();
    h.run_steps(4);
    (h, dir, guard)
}

/// A suffix so tests running at once use their own folders.
fn rand_suffix() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
}

/// The demo's -K01 coil (page 2).
fn coil(h: &mut H) -> RepresentationId {
    let d = doc(h);
    let k1 = d.components.iter().find(|(_, c)| c.properties.get("tag").is_some_and(|t| t.ends_with("K01"))).map(|(id, _)| *id).expect("-K01");
    d.representations_of(Subject::Component(k1))
        .find(|(_, r)| ecad_cadcraft::devices::rep_starter(&d, r).is_some_and(|s| s.key == "coil"))
        .map(|(id, _)| id)
        .expect("its coil")
}

fn select_coil(h: &mut H, tab: RightTab) {
    let rep = coil(h);
    h.state_mut().goto_symbols(&[rep]).unwrap();
    h.state_mut().open_inspector(tab);
    h.run_steps(5);
}

#[test]
fn the_device_tab_has_the_boards_header_status_and_folding_sections() {
    let (mut h, dir, _one) = harness(None);
    select_coil(&mut h, RightTab::Device);
    let d = h.state().settings.inspector.device.clone().expect("the device's details");
    assert_eq!(d.tag, "-K01");
    assert_eq!(d.description, "Pump contactor");
    assert!(d.symbol_words.starts_with("coil"), "{}", d.symbol_words);
    // Header: the tag, then its words, a Row button and collapse.
    assert!(h.query_by_label(right::HEAD_TITLE).is_some() && h.query_by_label(right::HEAD_SUB).is_some());
    assert!(h.query_by_label(right::ROW).is_some() && h.query_by_label(right::COLLAPSE).is_some());
    // The status of its own checks, then the five sections in the boards' order.
    assert!(h.query_by_label(right::OWN_CHECKS).is_some());
    let ys: Vec<f32> =
        ["identity", "parts", "drawn", "connections", "notes"].iter().map(|id| h.get_by_label(&right::section_name(id)).rect().top()).collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "in order: {ys:?}");
    // Where it is drawn: the coil first (this symbol), then its contacts.
    assert!(d.drawn.first().is_some_and(|r| r.this && r.pins.starts_with("Coil")), "{:?}", d.drawn);
    assert!(d.drawn.len() > 1, "its contacts: {:?}", d.drawn);
    // Connections: A1 and A2 on their wires.
    assert!(d.connections.iter().any(|c| c.pin == "A1") && d.connections.iter().any(|c| c.pin == "A2"), "{:?}", d.connections);
    // Description is set here (an input); Notes and attributes starts folded.
    assert!(h.query_by_label("ecad.props.device.description").is_some());
    // A folded section shows only its summary: Identity folded hides its fields.
    h.get_by_label(&right::section_name("identity")).click();
    h.run_steps(3);
    assert!(h.query_by_label("ecad.props.device.description").is_none(), "folded");
    h.get_by_label(&right::section_name("identity")).click();
    h.run_steps(3);
    assert!(h.query_by_label("ecad.props.device.description").is_some(), "open again");
    // A row of Where it is drawn goes to that symbol.
    let contact = d.drawn.iter().position(|r| !r.this && r.rep.is_some()).expect("a drawn contact");
    h.get_by_label(&format!("{}.{contact}", right::DRAWN_ROW)).click();
    h.run_steps(4);
    let now = h.state().settings.inspector.device.clone().expect("still a device");
    assert_eq!(now.rep, d.drawn[contact].rep, "the contact is selected");
    // Row opens the device's row in Project ▸ Devices.
    h.get_by_label(right::ROW).click();
    h.run_steps(3);
    assert_eq!(h.state().shell_state().workspace, ecad_trial::shell::Workspace::Lists);
    assert_eq!(h.state().panel.devices.selected, d.component);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_layout_tab_reads_the_placement_and_sets_the_texts_and_contact_table() {
    let (mut h, dir, _one) = harness(None);
    select_coil(&mut h, RightTab::Layout);
    for id in ["placement", "texts", "contacts", "line"] {
        assert!(h.query_by_label(&right::section_name(id)).is_some(), "{id}");
    }
    // Rotation is read from the drawing (not set here).
    let rot_name = format!("{}.0\u{b0}", right::ROTATION);
    let rot = h.get_by_label(&rot_name);
    let n = rot.accesskit_node();
    assert!(
        n.is_disabled() && (n.is_selected() == Some(true) || n.toggled() == Some(egui::accesskit::Toggled::True)),
        "0° read from the drawing: {:?} {:?} {:?}",
        n.is_disabled(),
        n.is_selected(),
        n.toggled()
    );
    // Text on the sheet: the tag's shown box and its size list.
    assert!(h.query_by_label("ecad.inspector.text.tag.show").is_some());
    assert!(h.query_by_label("ecad.inspector.style.text.tag.style").is_some());
    // The tag's shown box hides the tag on this symbol (one `setdefault`), Reset brings it back.
    h.get_by_label("ecad.inspector.text.tag.show").click();
    h.run_steps(4);
    let row = h.state().settings.inspector.rows.iter().find(|r| r.key == "text.tag.show").cloned().unwrap();
    assert_eq!(row.value, "Hidden");
    h.get_by_label("ecad.inspector.reset.text.tag.show").click();
    h.run_steps(4);
    let row = h.state().settings.inspector.rows.iter().find(|r| r.key == "text.tag.show").cloned().unwrap();
    assert_ne!(row.value, "Hidden");
    // The contact table under the coil: Right sets it on the device.
    h.get_by_label(&format!("{}.right", ecad_trial::shell::right::CONTACT_TABLE)).click();
    h.run_steps(4);
    let k1 = h.state().settings.inspector.device.as_ref().and_then(|d| d.component).unwrap();
    assert_eq!(doc(&mut h).components[&k1].properties.get(ecad_cadcraft::xref::MIRROR_KEY).map(String::as_str), Some("right"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_checks_tab_switches_between_this_page_and_the_project() {
    let (mut h, dir, _one) = harness(None);
    select_coil(&mut h, RightTab::Checks);
    assert!(h.query_by_label(&format!("{}.this page", right::SCOPE)).is_some());
    let rows = h.state().panel.checks.rows().to_vec();
    assert!(!rows.is_empty(), "the demo has checks");
    let page = h.state().panel.active;
    // This page: the cards of this page's checks, the rest under Elsewhere.
    for r in &rows {
        let label = ecad_trial::checks_panel::row_label(r);
        assert!(h.query_by_label(&label).is_some(), "{label} (on this page: {})", r.check.page == page);
    }
    if rows.iter().any(|r| r.check.page != page) {
        assert!(h.query_by_label(right::ELSEWHERE).is_some());
    }
    h.get_by_label(&format!("{}.project", right::SCOPE)).click();
    h.run_steps(3);
    assert!(h.state().settings.inspector.checks_project);
    assert!(h.query_by_label(right::ELSEWHERE).is_none(), "Project lists every check as a card");
    // A card's go-to shows its check.
    h.get_by_label(&ecad_trial::checks_panel::row_label(&rows[0])).click();
    h.run_steps(3);
    if rows[0].check.page.is_some() {
        assert_eq!(h.state().panel.active, rows[0].check.page);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Boards 1.05, 1.06 and 1.07 at 1920 × 1080 for the side-by-side; nothing is asserted about the
/// pictures.
#[test]
fn screenshots_of_boards_1_05_to_1_07() {
    let Some(out) = crate::project_panel::shots() else { return };
    let deltas: crate::mouse_tester::Deltas = std::sync::Arc::default();
    let (mut h, dir, _one) = harness(Some(deltas.clone()));
    select_coil(&mut h, RightTab::Device);
    h.state_mut().fit_sheet();
    h.run_steps(6);
    for (tab, name) in
        [(RightTab::Device, "uiv3-B-1.05-ours.png"), (RightTab::Layout, "uiv3-B-1.06-ours.png"), (RightTab::Checks, "uiv3-B-1.07-ours.png")]
    {
        h.state_mut().open_inspector(tab);
        h.run_steps(6);
        let _ = crate::mouse_tester::shot(&h, &deltas, &out.join(name));
    }
    // 1.11: placing a contactor coil, the picker folded.
    h.state_mut().cad.session.set_selection(Vec::new());
    h.state_mut().open_inspector(RightTab::Device);
    h.state_mut().start_pick(ecad_trial::search::Pick::Symbol("coil")).unwrap();
    h.run_steps(4);
    h.event(egui::Event::PointerMoved(egui::pos2(1100.0, 420.0)));
    h.run_steps(4);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("uiv3-B-1.11-ours.png"));
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    // Nothing selected: the page.
    h.state_mut().cad.session.set_selection(Vec::new());
    h.state_mut().open_inspector(RightTab::Device);
    h.run_steps(6);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("uiv3-B-nothing-selected-ours.png"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Board 1.11: while Place Symbol runs, Device describes the device about to be placed: its next
/// tag in the header, its symbol, and what the click changes. Placing it ends that.
#[test]
fn placing_a_coil_shows_the_device_about_to_be_placed() {
    let (mut h, dir, _one) = harness(None);
    h.state_mut().open_inspector(RightTab::Device);
    h.state_mut().start_pick(ecad_trial::search::Pick::Symbol("coil")).unwrap();
    h.run_steps(4);
    assert_eq!(h.state().picker_shown(), ecad_trial::shell::placing::Shown::Chip);
    let sub = h.query_by_label(right::HEAD_SUB).is_some();
    assert!(sub, "the symbol and 'not placed yet'");
    assert!(h.query_by_label(right::CHANGES).is_some(), "what this click changes");
    assert!(h.query_by_label("ecad.inspector.placing.tag").is_some());
    assert!(h.query_by_label(right::ROW).is_none(), "nothing to open a row of yet");
    // Esc ends the tool: the inspector goes back to the selection.
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(h.query_by_label(right::CHANGES).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
