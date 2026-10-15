//! ECAD-SEAM(owner): Tool Sets groups supplied by an embedding application. Without any, the
//! panel is unchanged; with one, its buttons appear after the built-in groups of the Drafting tab,
//! and a click is queued for the embedder exactly once instead of being started.

use cadcraft_engine::Session;
use cadcraft_ui_egui::palettes::{ExtraTool, ExtraToolGroup};
use cadcraft_ui_egui::{CadApp, Services};
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn harness(groups: Vec<ExtraToolGroup>) -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.run("ui.sample", serde_json::json!({})).unwrap();
    app.ui.in_window_menu = false;
    app.integrated_titlebar = true;
    app.extra_tool_groups = groups;
    let mut h = Harness::builder().with_size(vec2(1600.0, 2600.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);
    h
}

fn group() -> ExtraToolGroup {
    ExtraToolGroup {
        name: "Embedder".into(),
        tools: vec![
            ExtraTool { id: "test.first".into(), label: "First Tool".into(), tooltip: "Does the first thing".into() },
            ExtraTool { id: "test.second".into(), label: "Second Tool".into(), tooltip: "Does the second".into() },
        ],
    }
}

#[test]
fn without_extra_groups_the_palette_is_unchanged() {
    let mut h = harness(Vec::new());
    assert!(h.query_by_label("Embedder").is_none());
    h.get_by_label("Line").click();
    h.run_steps(3);
    assert_eq!(h.state().session.running.as_ref().unwrap().id, "line", "built-in tools still start their command");
    assert!(h.state_mut().take_tool_requests().is_empty(), "nothing is queued");
}

#[test]
fn an_extra_tool_click_is_queued_once_and_not_started() {
    let mut h = harness(vec![group()]);
    assert!(h.query_by_label("Embedder").is_some(), "the group header is shown");
    let selection = h.state().session.selection();
    h.get_by_label("Second Tool").click();
    h.run_steps(3);
    assert!(h.state().session.running.is_none(), "CADCraft does not start an embedder tool");
    assert_eq!(h.state().session.selection(), selection, "the selection is kept");
    assert_eq!(h.state_mut().take_tool_requests(), vec!["test.second".to_string()]);
    h.run_steps(3);
    assert!(h.state_mut().take_tool_requests().is_empty(), "each click is reported exactly once");

    // The header collapses the group like a built-in one.
    h.get_by_label("Embedder").click();
    h.run_steps(3);
    assert!(h.query_by_label("First Tool").is_none());
    assert!(h.state().ui.collapsed_groups.iter().any(|g| g == "Embedder"));
}

#[test]
fn extra_groups_belong_to_the_drafting_tab() {
    let mut h = harness(vec![group()]);
    h.state_mut().ui.toolset_tab = "Modeling".into();
    h.run_steps(3);
    assert!(h.query_by_label("First Tool").is_none());
}
