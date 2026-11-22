//! Tester 059 full, in the app (design 059 §5): Project ▸ Cables' + Cable, an edit from the
//! Details pane and Remove are each one transaction with an exact undo and redo; Remove of a drawn
//! cable writes nothing and offers a Go to that shows its page; the slash picker lists New cable,
//! then not-drawn cables, then drawn ones. Headless on the demo project.

use ecad_app::App;
use ecad_app::lists::cable_editor::Ask;
use ecad_app::shell::ListsView;
use ecad_app::shell::state::Workspace;
use ecad_electrical::cables::CableField;
use ecad_model::{CableId, Document};
use eframe::egui::{self, Key, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use crate::mouse_tester::{Collect, Deltas};
use crate::project_panel::{doc, head, ok, rev3_app as new_app};

type H = Harness<'static, App>;

fn harness() -> H {
    let deltas: Deltas = std::sync::Arc::default();
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

fn demo(h: &mut H, tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ecad-059t-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = crate::checks_ui::demo_file(&dir);
    h.state_mut().close_project();
    h.run_steps(2);
    h.state_mut().open_project(&path).unwrap();
    h.run_steps(3);
    h.state_mut().set_workspace(Workspace::Lists);
    h.state_mut().show_project_tab(ListsView::Cables);
    h.run_steps(6);
    dir
}

fn last_label(h: &mut H) -> String {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.log().last().map(|x| x.label.clone()).unwrap_or_default()).unwrap()
}

/// `act` is one transaction labelled `label`; `u` restores `before` exactly and `redo` the after.
fn one_exact(h: &mut H, label: &str, act: impl FnOnce(&mut H)) -> Document {
    let before = doc(h);
    let n = head(h);
    act(h);
    h.run_steps(6);
    assert_eq!(head(h), n + 1, "{label}: one transaction");
    assert_eq!(last_label(h), label);
    let after = doc(h);
    ok(h, "u");
    h.run_steps(4);
    assert!(doc(h) == before, "{label}: undo is exact");
    ok(h, "redo");
    h.run_steps(4);
    assert!(doc(h) == after, "{label}: redo is exact");
    after
}

#[test]
fn t1_plus_cable_edit_and_remove_are_one_transaction_each_with_exact_undo() {
    let mut h = harness();
    let dir = demo(&mut h, "tx");
    let before: Vec<CableId> = doc(&mut h).cables.keys().copied().collect();
    let after = one_exact(&mut h, "Create cable", |h| {
        h.get_by_label(ecad_app::lists::workspace::ADD_NAME).click();
        h.run_steps(8);
        let made = h.state().reports.as_ref().and_then(|r| r.cables.selected().and_then(|i| r.cables.ids.get(i).copied()));
        assert!(made.is_some_and(|k| !before.contains(&k)), "the new cable is selected at once");
    });
    let k = *after.cables.keys().find(|k| !before.contains(k)).expect("a new cable");
    assert!(after.cables[&k].cores.is_empty(), "not drawn");
    one_exact(&mut h, "Edit cable", |h| h.state_mut().cable_ask(k, Ask::Edit(CableField::Length, json!({"length": 12}))));
    one_exact(&mut h, "Edit cable", |h| {
        h.state_mut().cable_ask(k, Ask::Edit(CableField::Gland(ecad_electrical::cables::Side::To), json!({"to": {"gland": "M20"}})))
    });
    // A bad value: nothing written, the words kept for the row.
    let n = head(&mut h);
    h.state_mut().cable_ask(k, Ask::Edit(CableField::Length, json!({"length": "x"})));
    h.run_steps(4);
    assert_eq!(head(&mut h), n);
    assert!(h.state().reports.as_ref().unwrap().cables.edit.error.is_some());
    let gone = one_exact(&mut h, "Remove cable", |h| h.state_mut().cable_ask(k, Ask::Remove));
    assert!(!gone.cables.contains_key(&k));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn t2_remove_of_a_drawn_cable_writes_nothing_and_its_go_to_shows_the_page() {
    let mut h = harness();
    let dir = demo(&mut h, "rm");
    let d = doc(&mut h);
    let w01 = d.cables_tagged("-W01")[0];
    let n = head(&mut h);
    h.state_mut().cable_ask(w01, Ask::Remove);
    h.run_steps(6);
    assert_eq!(head(&mut h), n, "nothing written");
    assert!(doc(&mut h) == d);
    let (words, pages) = h.state().reports.as_ref().unwrap().cables.edit.refused.clone().expect("refused");
    assert!(words.contains("-W01 is drawn on"), "{words}");
    assert!(!pages.is_empty());
    let page = pages[0].0;
    h.state_mut().cable_ask(w01, Ask::Go(page));
    h.run_steps(8);
    assert_eq!(h.state().panel.active, Some(page), "Go to shows the page it is drawn on");
    assert_eq!(head(&mut h), n, "Go to writes nothing");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn t3_the_slash_picker_lists_new_cable_then_not_drawn_then_drawn() {
    let mut h = harness();
    let dir = demo(&mut h, "pick");
    ok(&mut h, r#"cablenew {"tag":"-W08","cores":"4G"}"#);
    ok(&mut h, r#"cablenew {"tag":"-W07"}"#);
    let d = doc(&mut h);
    let k = d.cables_tagged("-W08")[0];
    let n = head(&mut h);
    h.state_mut().assign_in_drawing(k);
    h.run_steps(8);
    assert!(h.state().slash_running());
    let p = h.state().shell.slash_picker.clone();
    assert_eq!(p.labels.first().map(String::as_str), Some(ecad_app::shell::right::slash_picker::NEW_CABLE));
    let first_drawn = p.items.iter().position(|i| i.drawn).unwrap_or(p.items.len());
    assert!(p.items[..first_drawn].iter().all(|i| !i.drawn) && p.items[first_drawn..].iter().all(|i| i.drawn), "not drawn first: {:?}", p.labels);
    let tags: Vec<&str> = p.items[..first_drawn].iter().map(|i| i.tag.as_str()).collect();
    assert!(tags.contains(&"-W07") && tags.contains(&"-W08"), "{tags:?}");
    assert_eq!(p.items.get(p.picked.wrapping_sub(1)).map(|i| i.cable), Some(k), "-W08 picked");
    // Picking New cable instead, then Esc: nothing written.
    h.state_mut().slash_pick(0);
    h.run_steps(3);
    assert_eq!(h.state().shell.slash_picker.picked, 0);
    h.key_press(Key::Escape);
    h.run_steps(4);
    assert!(!h.state().slash_running());
    assert_eq!(head(&mut h), n, "Esc writes nothing");
    let _ = std::fs::remove_dir_all(&dir);
}
