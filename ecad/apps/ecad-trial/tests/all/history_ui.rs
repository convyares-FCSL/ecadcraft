//! Brief 020a headless in the trial (design 020 §13 checks A–D, L, M): the History tab on the
//! demo project, New version through its dialog, a row's details, Show and Back to current,
//! Branch from here with the branch bar and the window title, Save and switch back, and Revisions
//! off main. Screenshots (`020-*.png`) are written when `ECAD_TESTER_SHOTS` names a folder and a
//! wgpu adapter exists; they are for the owner's approval, never committed.

use std::path::PathBuf;
use std::sync::Arc;

use ecad_trial::history_panel::{self, Dialog, Do};
use ecad_trial::project_panel;
use eframe::egui::{self, Key};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use crate::checks_ui::demo_file;
use crate::mouse_tester::{Deltas, doc, harness_with, ok, shot};

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

fn click(h: &mut H, label: &str) {
    // UI rev 3: the panels' tabs are reached through the strip and the inspector.
    crate::project_panel::click(h, label);
}

/// Replace the text of the field labelled `label`.
fn type_into(h: &mut H, label: &str, text: &str) {
    h.get_by_label(label).focus();
    h.run_steps(1);
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.get_by_label(label).type_text(text);
    h.run_steps(2);
}

fn last_label(h: &mut H) -> String {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.log().last().map(|t| t.label.clone()).unwrap_or_default()).unwrap()
}

#[test]
fn the_history_tab_makes_a_version_shows_it_and_branches_from_it() {
    let dir = std::env::temp_dir().join(format!("ecad-020-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = demo_file(&dir);
    let deltas: Deltas = Arc::default();
    let mut h = harness_with(Some(deltas.clone()));
    h.state_mut().cad.session.viewport_px = (1280.0, 800.0);
    h.state_mut().close_project();
    h.run_steps(2);
    h.state_mut().open_project(&path).unwrap();
    h.run_steps(3);
    assert_eq!(ecad_trial::window_title(h.state_mut().branch_info().as_ref()), ecad_trial::TITLE, "one branch: the title is as before");

    // A: the History tab, New version through its dialog.
    click(&mut h, history_panel::HISTORY_TAB);
    assert_eq!(h.state().panel.tab, project_panel::Tab::History);
    let kinds: Vec<_> = h.state().panel.history.graph().nodes.iter().map(|n| n.kind).collect();
    assert!(kinds.contains(&ecad_cadcraft::history::NodeKind::Now) && kinds.contains(&ecad_cadcraft::history::NodeKind::Root), "{kinds:?}");
    click(&mut h, "ecad.history.newversion");
    assert!(matches!(h.state().panel.history.dialog, Some(Dialog::NewVersion { .. })));
    type_into(&mut h, "ecad.newversion.name", "Before change");
    click(&mut h, "ecad.newversion.ok");
    assert_eq!(last_label(&mut h), "Create version \"Before change\"");
    assert!(h.state().panel.history.dialog.is_none());
    h.state_mut().save().unwrap();
    h.run_steps(3);
    let rows: Vec<String> = h.state().panel.history.graph().nodes.iter().map(|n| n.label.clone()).collect();
    assert!(rows.iter().any(|r| r == "Before change"), "{rows:?}");
    save(&h, &deltas, "020-history.png");

    // The details of the version, then Show (read only) and Back to current.
    click(&mut h, "Version Before change on main");
    assert!(h.state().panel.history.selected.is_some());
    save(&h, &deltas, "020-details.png");
    let head = doc(&mut h).versions.len();
    click(&mut h, "ecad.history.show");
    let bar = h.state().panel.revision.clone().unwrap_or_default();
    assert!(bar.starts_with("Version \"Before change\" · main"), "{bar}");
    save(&h, &deltas, "020-point.png");
    click(&mut h, project_panel::BACK_TO_CURRENT);
    assert!(h.state().panel.revision.is_none());
    assert_eq!(doc(&mut h).versions.len(), head, "showing recorded nothing");

    // B: Branch from here, the bar and the title.
    click(&mut h, "Version Before change on main");
    click(&mut h, "ecad.history.branchfrom");
    type_into(&mut h, "ecad.newbranch.name", "Option B");
    save(&h, &deltas, "020-newbranch.png");
    click(&mut h, "ecad.newbranch.ok");
    h.run_steps(3);
    let info = h.state_mut().branch_info().unwrap();
    assert_eq!(info.name, "Option B");
    assert_eq!(info.bar.as_deref(), Some("Branch \"Option B\" · from Before change"));
    assert!(ecad_trial::window_title(Some(&info)).ends_with("demo.ecad · Option B"));
    save(&h, &deltas, "020-branchbar.png");

    // An edit on the branch; Save and switch back to main: main is unchanged.
    let wires = doc(&mut h).conductors.len();
    ok(&mut h, r#"wire {"points":[[0,150],[30,150]]}"#);
    assert_eq!(doc(&mut h).conductors.len(), wires + 1);
    // M: Revisions on the branch: Issue is disabled with its reason.
    h.state_mut().open_revisions();
    h.run_steps(2);
    assert_eq!(h.state().revisions.as_ref().and_then(|r| r.off_main.clone()).as_deref(), Some("Option B"));
    // Switch to main from Revisions: the edit is unsaved, so Save and switch asks first.
    click(&mut h, "ecad.revisions.switchmain");
    assert!(matches!(&h.state().panel.history.dialog, Some(Dialog::Switch { branch, .. }) if branch == "main"));
    click(&mut h, "ecad.switchbranch.ok");
    assert_eq!(h.state_mut().branch_info().unwrap().name, "main");
    // Revisions, still open, now allows issuing.
    assert_eq!(h.state().revisions.as_ref().map(|r| r.off_main.clone()), Some(None));
    h.state_mut().revisions = None;
    assert_eq!(doc(&mut h).conductors.len(), wires, "main is as it was");
    // C: Restore the version after an edit on main: one step, undone by u.
    ok(&mut h, r#"wire {"points":[[0,160],[30,160]]}"#);
    h.state_mut().history_action(Do::AskRestore(json!({"version": "Before change"})));
    h.run_steps(2);
    assert!(matches!(&h.state().panel.history.dialog, Some(Dialog::Restore { question, .. }) if question.contains("as it was at")));
    click(&mut h, "ecad.restore.ok");
    assert_eq!(last_label(&mut h), "Restore \"Before change\"");
    assert_eq!(doc(&mut h).conductors.len(), wires);
    ok(&mut h, "u");
    assert_eq!(doc(&mut h).conductors.len(), wires + 1);
    // A typed switch refreshes the title, bar and History as a click does (reviewer finding 1).
    ok(&mut h, r#"switchbranch {"branch":"Option B","save":true}"#);
    h.run_steps(3);
    let info = h.state_mut().branch_info().unwrap();
    assert_eq!(info.name, "Option B");
    assert!(info.bar.is_some());
    assert!(ecad_trial::window_title(Some(&info)).ends_with("· Option B"));
    let g = h.state().panel.history.graph().clone();
    let cur = g.branches.iter().find(|b| b.current).map(|b| b.name.clone());
    assert_eq!(cur.as_deref(), Some("Option B"), "the graph's current branch");
    // A typed rename too.
    ok(&mut h, r#"renamebranch {"branch":"Option B","name":"Option Z"}"#);
    h.run_steps(3);
    assert_eq!(h.state_mut().branch_info().unwrap().name, "Option Z");
    assert!(ecad_trial::window_title(h.state_mut().branch_info().as_ref()).ends_with("· Option Z"));
    let _ = std::fs::remove_dir_all(&dir);
}
