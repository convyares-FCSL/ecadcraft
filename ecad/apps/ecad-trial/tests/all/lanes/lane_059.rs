//! Lane 059 (the cable manager): Project ▸ Cables' + Cable, the not-drawn state, the editable
//! Details pane and Remove (design 059 §5), headless on the demo project. With `ECAD_TESTER_SHOTS`
//! set, writes `059-list.png`, `059-undrawn-details.png` and `059-remove-refused.png`.

use ecad_trial::Trial;
use ecad_trial::shell::ListsView;
use ecad_trial::shell::state::Workspace;
use eframe::egui::{self, Key, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::mouse_tester::{Collect, Deltas};
use crate::project_panel::{doc, ok, rev3_trial as trial};

type H = Harness<'static, Trial>;

fn harness(deltas: Deltas) -> H {
    let mut h = Harness::builder().with_size(vec2(1920.0, 1080.0)).with_step_dt(1.0 / 60.0).renderer(Collect(deltas)).build_ui_state(
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
    h.run_steps(5);
    h
}

fn demo(h: &mut H, dir: &std::path::Path) {
    let path = crate::checks_ui::demo_file(dir);
    h.state_mut().close_project();
    h.run_steps(2);
    h.state_mut().open_project(&path).unwrap();
    h.run_steps(3);
    h.state_mut().set_workspace(Workspace::Lists);
    h.state_mut().show_project_tab(ListsView::Cables);
    h.run_steps(6);
}

fn selected_cable(h: &H) -> Option<ecad_model::CableId> {
    let v = &h.state().reports.as_ref()?.cables;
    v.selected().and_then(|k| v.ids.get(k).copied())
}

fn shot(h: &H, deltas: &Deltas, name: &str) {
    if let Some(out) = crate::project_panel::shots() {
        let _ = crate::mouse_tester::shot(h, deltas, &out.join(name));
    }
}

#[test]
fn plus_cable_builds_a_cable_not_drawn_and_its_details_edit_it() {
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = harness(deltas.clone());
    let dir = std::env::temp_dir().join(format!("ecad-059-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    demo(&mut h, &dir);
    let before = doc(&mut h).cables.len();
    // + Cable: a cable at once, selected, its details open, no dialog.
    h.get_by_label(ecad_trial::lists::workspace::ADD_NAME).click();
    h.run_steps(8);
    let d = doc(&mut h);
    assert_eq!(d.cables.len(), before + 1);
    let k = selected_cable(&h).expect("the new cable is selected");
    assert!(d.cables[&k].members.is_empty());
    assert!(h.state().reports.as_ref().unwrap().cables.details, "its details are open");
    assert!(h.query_all_by_label_contains("not drawn").next().is_some(), "the not drawn badge");
    assert!(h.query_all_by_label_contains("1 not drawn").next().is_some(), "the footer counts it");
    assert!(h.query_by_label("ecad.project.detail.cable.tag").is_some(), "the Tag box");
    assert!(h.query_all_by_label_contains("ecad.project.detail.cable.drawn").next().is_some(), "Not drawn yet");
    // Typing a length commits one cableedit on Enter.
    let n = h.get_by_label("ecad.project.detail.cable.length");
    n.focus();
    h.run_steps(1);
    h.get_by_label("ecad.project.detail.cable.length").type_text("12");
    h.run_steps(1);
    h.key_press(Key::Enter);
    h.run_steps(6);
    assert_eq!(doc(&mut h).cables[&k].properties.get("length").map(String::as_str), Some("12"));
    // A bad value is refused: nothing written, the words shown.
    let n = h.get_by_label("ecad.project.detail.cable.cores");
    n.focus();
    h.run_steps(1);
    h.get_by_label("ecad.project.detail.cable.cores").type_text("4Q");
    h.run_steps(1);
    h.key_press(Key::Enter);
    h.run_steps(6);
    assert!(!doc(&mut h).cables[&k].properties.contains_key("cores"));
    assert!(h.state().reports.as_ref().unwrap().cables.edit.error.is_some(), "the refusal is shown");
    // A planned end and a shield, through the commands the boxes run.
    ok(
        &mut h,
        &format!(
            r#"cableedit {{"cable":"{}","set":{{"cores":"4G","shield":"overall","from":{{"location":"+CAB1"}},"to":{{"location":"+FIELD","gland":"M20"}}}}}}"#,
            k.hex()
        ),
    );
    h.run_steps(8);
    assert!(h.query_all_by_label_contains("+FIELD (planned)").next().is_some(), "the list shows the planned end");
    shot(&h, &deltas, "059-undrawn-details.png");
    // The chevron's menu: New cable, then the cables not drawn.
    h.get_by_label(ecad_trial::lists::workspace::ADD_MENU).click();
    h.run_steps(4);
    assert!(h.query_all_by_label_contains(ecad_trial::lists::cables::NEW_CABLE).next().is_some());
    assert!(h.query_all_by_label_contains(ecad_trial::lists::cables::ASSIGN_HEADER).next().is_some());
    shot(&h, &deltas, "059-list.png");
    h.key_press(Key::Escape);
    h.run_steps(3);
    // Remove: a cable not drawn goes at once; undo brings it back.
    h.state_mut().cable_ask(k, ecad_trial::lists::cable_editor::Ask::Remove);
    h.run_steps(4);
    assert!(!doc(&mut h).cables.contains_key(&k));
    ok(&mut h, "u");
    h.run_steps(4);
    assert!(doc(&mut h).cables.contains_key(&k));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn removing_a_drawn_cable_is_refused_with_a_go_to_per_page() {
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = harness(deltas.clone());
    let dir = std::env::temp_dir().join(format!("ecad-059-rm-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    demo(&mut h, &dir);
    let d = doc(&mut h);
    let w01 = d.cables_tagged("-W01")[0];
    let k = h.state().reports.as_ref().unwrap().cables.ids.iter().position(|c| *c == w01).unwrap();
    h.state_mut().reports.as_mut().unwrap().cables.table.selection.set([k]);
    h.state_mut().reports.as_mut().unwrap().cables.details = true;
    h.run_steps(8);
    let head = doc(&mut h).cables.len();
    h.get_by_label(ecad_trial::lists::cable_editor::REMOVE).click();
    h.run_steps(6);
    assert_eq!(doc(&mut h).cables.len(), head, "nothing removed");
    let refused = h.state().reports.as_ref().unwrap().cables.edit.refused.clone().expect("refused");
    assert!(refused.0.contains("-W01 is drawn on") && refused.0.contains("Delete it on its page."), "{refused:?}");
    assert!(!refused.1.is_empty(), "a Go to per page");
    assert!(h.query_by_label("ecad.lists.cables.remove.refused").is_some());
    shot(&h, &deltas, "059-remove-refused.png");
    let _ = std::fs::remove_dir_all(&dir);
}
