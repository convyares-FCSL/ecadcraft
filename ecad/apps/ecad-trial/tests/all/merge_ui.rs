//! Brief 020b headless in the trial (design 020 §13 checks E, F): on the demo project, a branch
//! changes a device's description and main changes it too; the branch bar's Merge into main…
//! opens the merge dialog with the conflict (Merge disabled until it is chosen), Keep branch
//! then Merge records one merge step on main, the graph shows the join, and a plain undo asks
//! first. Screenshots `020-merge.png` and `020-undomerge.png` are written when
//! `ECAD_TESTER_SHOTS` names a folder and a wgpu adapter exists.

use std::path::PathBuf;
use std::sync::Arc;

use ecad_cadcraft::history::NodeKind;
use ecad_trial::history_panel;
use egui_kittest::Harness;
use serde_json::json;

use crate::checks_ui::demo_file;
use crate::mouse_tester::{Deltas, doc, harness_with, ok, shot};

type H = Harness<'static, ecad_trial::Trial>;

fn save_shot(h: &H, deltas: &Deltas, name: &str) {
    let Some(dir) = std::env::var_os("ECAD_TESTER_SHOTS").map(PathBuf::from) else { return };
    let _ = std::fs::create_dir_all(&dir);
    if shot(h, deltas, &dir.join(name)).is_none() {
        eprintln!("no wgpu adapter: {name} skipped");
    }
}

fn click(h: &mut H, label: &str) {
    // UI rev 3: the panels' tabs are reached through the strip and the inspector.
    crate::project_panel::click(h, label);
}

fn last_label(h: &mut H) -> String {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.log().last().map(|t| t.label.clone()).unwrap_or_default()).unwrap()
}

fn description(h: &mut H, c: ecad_model::ComponentId) -> Option<String> {
    doc(h).components.get(&c).and_then(|x| x.properties.get("description").cloned())
}

#[test]
fn the_branch_bar_merges_into_main_through_the_dialog_and_undo_asks_first() {
    let dir = std::env::temp_dir().join(format!("ecad-020b-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = demo_file(&dir);
    let deltas: Deltas = Arc::default();
    let mut h = harness_with(Some(deltas.clone()));
    h.state_mut().cad.session.viewport_px = (1280.0, 800.0);
    h.state_mut().close_project();
    h.run_steps(2);
    h.state_mut().open_project(&path).unwrap();
    h.run_steps(3);
    click(&mut h, history_panel::HISTORY_TAB);
    let (k1, _) =
        doc(&mut h).components.iter().find(|(_, c)| c.properties.get("tag").is_some_and(|t| t == "-K01")).map(|(k, c)| (*k, c.clone())).unwrap();
    // A branch changes -K01's description; main changes it too (a conflict).
    ok(&mut h, r#"branch {"name":"Option B"}"#);
    ok(&mut h, &format!(r#"setprop {{"component":"{}","key":"description","value":"Branch pump"}}"#, k1.hex()));
    ok(&mut h, r#"wire {"points":[[0,150],[30,150]]}"#);
    ok(&mut h, r#"switchbranch {"branch":"main","save":true}"#);
    ok(&mut h, &format!(r#"setprop {{"component":"{}","key":"description","value":"Main pump"}}"#, k1.hex()));
    ok(&mut h, r#"switchbranch {"branch":"Option B","save":true}"#);
    h.run_steps(3);
    // The branch bar's Merge into main… opens the dialog, which says it switches first.
    click(&mut h, "ecad.project.branch.merge");
    let d = h.state().merge.clone().expect("the merge dialog is open");
    assert!(d.switch_note.as_deref().is_some_and(|n| n.contains("switches to main")), "{:?}", d.switch_note);
    let conflicts = d.preview["conflicts"].as_array().cloned().unwrap();
    assert_eq!(conflicts.len(), 1, "{}", d.preview);
    let id = conflicts[0]["id"].as_str().unwrap().to_string();
    assert_eq!(h.state_mut().branch_info().unwrap().name, "Option B", "nothing switched yet");
    // Merge is disabled until the conflict has a choice.
    click(&mut h, "ecad.merge.ok");
    assert!(h.state().merge.is_some(), "a disabled Merge does nothing");
    click(&mut h, &format!("ecad.merge.conflict.{id}.branch"));
    assert_eq!(h.state().merge.as_ref().unwrap().preview["unanswered"], json!([]));
    save_shot(&h, &deltas, "020-merge.png");
    click(&mut h, "ecad.merge.ok");
    assert!(h.state().merge.is_none(), "{:?}", h.state().merge.as_ref().and_then(|d| d.error.clone()));
    assert_eq!(h.state_mut().branch_info().unwrap().name, "main");
    assert!(last_label(&mut h).starts_with("Merge \"Option B\" (based on "));
    assert_eq!(description(&mut h, k1).as_deref(), Some("Branch pump"));
    // The graph shows the merge row with its join.
    h.run_steps(2);
    let g = h.state().panel.history.graph().clone();
    assert!(g.nodes.iter().any(|n| n.kind == NodeKind::Merge && n.label == "Merged \"Option B\""), "{:?}", g.nodes);
    // A plain undo asks first; Undo merge takes it back.
    let e = h.state_mut().submit_command("u");
    assert!(e.as_deref().is_some_and(|m| m.contains("undomerge")), "{e:?}");
    h.run_steps(3);
    save_shot(&h, &deltas, "020-undomerge.png");
    click(&mut h, "ecad.undomerge.ok");
    assert_eq!(description(&mut h, k1).as_deref(), Some("Main pump"));
    let _ = std::fs::remove_dir_all(&dir);
}
