//! ECAD-SEAM(owner): Tool Sets tabs supplied by an embedding application. Without any, Drafting
//! and Modeling are unchanged; with one, it appears beside them, shows its own groups, and a click
//! on one of its tools is queued for the embedder exactly once instead of being started.

use cadcraft_engine::Session;
use cadcraft_ui_egui::palettes::{ExtraTool, ExtraToolGroup, ExtraToolTab};
use cadcraft_ui_egui::{CadApp, Services};
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

/// A Drafting tool shown once in the palette (not also on a toolbar).
const DRAFTING_TOOL: &str = "Revision Cloud";

fn harness(tabs: Vec<ExtraToolTab>) -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.run("ui.sample", serde_json::json!({})).unwrap();
    app.ui.in_window_menu = false;
    app.integrated_titlebar = true;
    app.extra_tool_tabs = tabs;
    let mut h = Harness::builder().with_size(vec2(1600.0, 1000.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);
    h
}

fn tab(name: &str) -> ExtraToolTab {
    ExtraToolTab {
        name: name.into(),
        groups: vec![
            ExtraToolGroup {
                name: "First Group".into(),
                tools: vec![ExtraTool { id: "test.first".into(), label: "First Tool".into(), tooltip: "Does the first thing".into() }],
            },
            ExtraToolGroup {
                name: "Second Group".into(),
                tools: vec![ExtraTool { id: "test.second".into(), label: "Second Tool".into(), tooltip: "Does the second".into() }],
            },
        ],
    }
}

#[test]
fn without_extra_tabs_drafting_and_modeling_are_unchanged() {
    let mut h = harness(Vec::new());
    let drafting = h.get_by_label("Drafting").rect();
    let modeling = h.get_by_label("Modeling").rect();
    assert!((drafting.width() - 92.0).abs() < 0.5 && (modeling.width() - 92.0).abs() < 0.5, "tab widths as before");
    assert!(h.query_by_label("Embedder").is_none());
    h.get_by_label("Line").click();
    h.run_steps(3);
    assert_eq!(h.state().session.running.as_ref().unwrap().id, "line", "built-in tools still start their command");
    assert!(h.state_mut().take_tool_requests().is_empty(), "nothing is queued");
}

#[test]
fn an_extra_tab_shows_its_groups_and_queues_clicks_once() {
    let mut h = harness(vec![tab("Embedder")]);
    assert!(h.query_by_label(DRAFTING_TOOL).is_some(), "Drafting is still the tab shown first");
    assert!(h.query_by_label("First Tool").is_none(), "the extra tab's tools are not in Drafting");

    h.get_by_label("Embedder").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.toolset_tab, "Embedder");
    assert!(h.query_by_label(DRAFTING_TOOL).is_none(), "the tab replaces the built-in groups");
    assert!(h.query_by_label("First Group").is_some() && h.query_by_label("Second Group").is_some());

    let selection = h.state().session.selection();
    h.get_by_label("Second Tool").click();
    h.run_steps(3);
    assert!(h.state().session.running.is_none(), "CADCraft does not start an embedder tool");
    assert_eq!(h.state().session.selection(), selection, "the selection is kept");
    assert_eq!(h.state_mut().take_tool_requests(), vec!["test.second".to_string()]);
    h.run_steps(3);
    assert!(h.state_mut().take_tool_requests().is_empty(), "each click is reported exactly once");

    // A group header collapses its group like a built-in one.
    h.get_by_label("First Group").click();
    h.run_steps(3);
    assert!(h.query_by_label("First Tool").is_none());
    assert!(h.query_by_label("Second Tool").is_some());

    // Back to Drafting: the built-in palette returns.
    h.get_by_label("Drafting").click();
    h.run_steps(3);
    assert!(h.query_by_label(DRAFTING_TOOL).is_some());
    assert!(h.query_by_label("Second Tool").is_none());
}

#[test]
fn an_extra_tab_named_like_a_built_in_one_is_ignored() {
    let h = harness(vec![tab("Drafting")]);
    assert!(h.query_by_label("First Tool").is_none());
    assert!(h.query_by_label(DRAFTING_TOOL).is_some());
}
