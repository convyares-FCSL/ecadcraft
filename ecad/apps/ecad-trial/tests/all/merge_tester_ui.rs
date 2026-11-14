//! Tester UI checks for brief 020b in the trial, following quick start §5J steps 9–13 by clicks:
//! the branch bar's Merge into main… and Update from main… are visible and work (Cancel writes
//! nothing), Keep main for all, a Now row's right-click menu, F2 renames, the graph reads the next
//! 200 rows at the end of the list, and a version's details name the devices it added.

use std::path::PathBuf;

use ecad_cadcraft::history::NodeKind;
use ecad_trial::history_panel;
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

use crate::checks_ui::demo_file;
use crate::mouse_tester::{doc, harness_with, ok};

type H = Harness<'static, ecad_trial::Trial>;

fn click(h: &mut H, label: &str) {
    // UI rev 3: the panels' tabs are reached through the strip and the inspector.
    crate::project_panel::click(h, label);
}

fn exists(h: &H, label: &str) -> bool {
    h.query_by_label(label).is_some()
}

fn log_len(h: &mut H) -> usize {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.log().len()).unwrap()
}

fn last_label(h: &mut H) -> String {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.log().last().map(|t| t.label.clone()).unwrap_or_default()).unwrap()
}

fn branch(h: &mut H) -> String {
    h.state_mut().branch_info().unwrap().name
}

fn echoed(h: &H, part: &str) -> bool {
    h.state().cad.session.log.iter().rev().take(10).any(|l| l.contains(part))
}

fn run(h: &mut H, cmd: &str, params: Value) -> Value {
    let t = h.state_mut();
    let r = t.cad.session.execute(cmd, &params).unwrap();
    h.run_steps(2);
    r
}

fn status(h: &mut H, name: &str) -> String {
    let b = run(h, "branches", json!({}));
    b["branches"].as_array().unwrap().iter().find(|x| x["name"] == json!(name)).map(|x| x["status"].as_str().unwrap().to_string()).unwrap_or_default()
}

fn opened(tag: &str) -> (H, PathBuf, ecad_model::ComponentId) {
    let dir = std::env::temp_dir().join(format!("ecad-020b-tui-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = demo_file(&dir);
    let mut h = harness_with(None);
    h.state_mut().cad.session.viewport_px = (1280.0, 800.0);
    h.state_mut().close_project();
    h.run_steps(2);
    h.state_mut().open_project(&path).unwrap();
    h.run_steps(3);
    click(&mut h, history_panel::HISTORY_TAB);
    let k1 = doc(&mut h).components.iter().find(|(_, c)| c.properties.get("tag").is_some_and(|t| t == "-K01")).map(|(k, _)| *k).unwrap();
    (h, dir, k1)
}

fn setdesc(h: &mut H, k1: ecad_model::ComponentId, v: &str) {
    ok(h, &format!(r#"setprop {{"component":"{}","key":"description","value":"{v}"}}"#, k1.hex()));
}

#[test]
fn quick_start_5j_merge_cancel_merge_and_update_by_clicks() {
    let (mut h, dir, k1) = opened("merge");
    // On main the branch bar's merge buttons are not there (main merges into nothing).
    assert!(!exists(&h, "ecad.project.branch.merge"), "no Merge into on main");
    // Step 9: a branch, a change and a wire; Merge into main… in the branch bar.
    ok(&mut h, r#"branch {"name":"Option B"}"#);
    h.run_steps(3);
    assert!(exists(&h, "ecad.project.branch.merge") && exists(&h, "ecad.project.branch.update"), "both buttons visible on a branch");
    setdesc(&mut h, k1, "Pump 1");
    ok(&mut h, r#"wire {"points":[[0,150],[30,150]]}"#);
    let n = log_len(&mut h);
    click(&mut h, "ecad.project.branch.merge");
    let d = h.state().merge.clone().expect("the merge dialog is open");
    eprintln!("5J.9 preview: title={} header={} counts={} note={:?}", d.preview["title"], d.preview["header"], d.preview["counts"], d.switch_note);
    assert_eq!(d.preview["title"], json!("MERGE \"Option B\" INTO main"));
    assert!(
        d.switch_note.as_deref().is_some_and(|s| s.contains("switches to main") && s.contains("saving \"Option B\" first")),
        "{:?}",
        d.switch_note
    );
    assert!(
        d.preview["counts"].as_str().unwrap().starts_with("2 changes from the branch")
            && d.preview["counts"].as_str().unwrap().ends_with("0 conflicts"),
        "{}",
        d.preview["counts"]
    );
    let changes: Vec<String> =
        d.preview["changes"].as_array().unwrap().iter().map(|c| format!("{} {}", c["sign"].as_str().unwrap(), c["text"].as_str().unwrap())).collect();
    eprintln!("5J.9 changes: {changes:?}");
    assert!(changes.iter().any(|c| c.contains("-K01 · Description") && c.contains("Pump 1")), "{changes:?}");
    assert!(d.close_branch, "Close branch ticked by default");
    // Cancel writes nothing and stays on the branch.
    click(&mut h, "ecad.merge.cancel");
    assert!(h.state().merge.is_none());
    assert_eq!((branch(&mut h), log_len(&mut h)), ("Option B".to_string(), n), "Cancel: nothing written, still on the branch");
    // Merge.
    click(&mut h, "ecad.project.branch.merge");
    click(&mut h, "ecad.merge.ok");
    assert!(h.state().merge.is_none(), "{:?}", h.state().merge.as_ref().and_then(|d| d.error.clone()));
    assert_eq!(branch(&mut h), "main");
    assert!(
        echoed(&h, "Merged \"Option B\" into main (2 changes). Save to keep it."),
        "{:?}",
        h.state().cad.session.log.iter().rev().take(4).collect::<Vec<_>>()
    );
    assert!(echoed(&h, "\"Option B\" was saved; now on main."), "{:?}", h.state().cad.session.log.iter().rev().take(4).collect::<Vec<_>>());
    let g = h.state().panel.history.graph().clone();
    assert!(g.nodes.iter().any(|n| n.kind == NodeKind::Merge && n.label == "Merged \"Option B\""));
    h.state_mut().save().unwrap();
    h.run_steps(2);
    assert_eq!(status(&mut h, "Option B"), "merged");
    // Step 12: Update from main… on a branch after main changed.
    ok(&mut h, r#"branch {"name":"Option D"}"#);
    ok(&mut h, r#"wire {"points":[[0,170],[30,170]]}"#);
    ok(&mut h, r#"switchbranch {"branch":"main","save":true}"#);
    setdesc(&mut h, k1, "Main pump");
    ok(&mut h, r#"switchbranch {"branch":"Option D","save":true}"#);
    h.run_steps(3);
    click(&mut h, "ecad.project.branch.update");
    let d = h.state().merge.clone().expect("the update dialog is open");
    eprintln!("5J.12 preview: title={} header={} counts={}", d.preview["title"], d.preview["header"], d.preview["counts"]);
    assert!(d.preview["title"].as_str().unwrap().starts_with("UPDATE \"Option D\" FROM main"), "{}", d.preview["title"]);
    assert!(d.preview["header"].as_str().unwrap().starts_with("Brings main's changes since"), "{}", d.preview["header"]);
    assert!(!exists(&h, "ecad.merge.close"), "no Close box on an update");
    click(&mut h, "ecad.merge.ok");
    assert!(h.state().merge.is_none(), "{:?}", h.state().merge.as_ref().and_then(|d| d.error.clone()));
    assert_eq!(branch(&mut h), "Option D");
    assert!(last_label(&mut h).starts_with("Update from main"), "{}", last_label(&mut h));
    assert!(echoed(&h, "Updated \"Option D\" from main"), "{:?}", h.state().cad.session.log.last());
    let desc = doc(&mut h).components.get(&k1).and_then(|c| c.properties.get("description").cloned());
    assert_eq!(desc.as_deref(), Some("Main pump"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn quick_start_5j_keep_main_for_all_then_merge() {
    let (mut h, dir, k1) = opened("allmain");
    ok(&mut h, r#"branch {"name":"Option C"}"#);
    setdesc(&mut h, k1, "Branch pump");
    ok(&mut h, r#"switchbranch {"branch":"main","save":true}"#);
    setdesc(&mut h, k1, "Main pump");
    ok(&mut h, r#"switchbranch {"branch":"Option C","save":true}"#);
    h.run_steps(3);
    click(&mut h, "ecad.project.branch.merge");
    click(&mut h, "ecad.merge.allmain");
    assert_eq!(h.state().merge.as_ref().unwrap().preview["unanswered"], json!([]));
    let n = log_len(&mut h);
    click(&mut h, "ecad.merge.ok");
    let still_open = h.state().merge.as_ref().map(|d| (d.error.clone(), d.preview["counts"].clone()));
    eprintln!(
        "Keep main for all + Merge: dialog {:?}; branch {}; steps {} -> {}; echo {:?}",
        still_open,
        branch(&mut h),
        n,
        log_len(&mut h),
        h.state().cad.session.log.iter().rev().take(3).collect::<Vec<_>>()
    );
    h.state_mut().save().unwrap();
    h.run_steps(2);
    assert_eq!(status(&mut h, "Option C"), "merged", "Keep main for all with Close branch ticked closes the branch");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn now_row_right_click_f2_rename_and_graph_paging() {
    let (mut h, dir, _) = opened("menu");
    ok(&mut h, r#"branch {"name":"Option B"}"#);
    h.run_steps(3);
    // F2 on a branch with the History tab shown: the Rename dialog.
    h.key_press(egui::Key::F2);
    h.run_steps(3);
    let dlg = format!("{:?}", h.state().panel.history.dialog);
    assert!(dlg.starts_with("Some(Rename"), "{dlg}");
    h.state_mut().panel.history.dialog = None;
    h.run_steps(2);
    // Right-click on main's Now row: Switch to "main"; on the branch's own Now row: Merge into
    // main…, Update from main…, Rename, Archive.
    h.get_by_label("Now on main").click_secondary();
    h.run_steps(3);
    assert!(exists(&h, "Switch to \"main\""), "main's Now row menu");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    h.get_by_label_contains("Now on Option B").click_secondary();
    h.run_steps(3);
    for item in ["Merge into main…", "Update from main…", "Rename branch…", "Archive branch…"] {
        assert!(exists(&h, item), "{item} in the Now row menu");
    }
    h.get_by_label("Merge into main…").click();
    h.run_steps(3);
    assert!(h.state().merge.is_some(), "the menu's Merge into main… opens the dialog");
    click(&mut h, "ecad.merge.cancel");
    // Paging: more than 200 rows on main.
    ok(&mut h, r#"switchbranch {"branch":"main","save":true}"#);
    for i in 0..230 {
        let y = 200 + i;
        ok(&mut h, &format!(r#"wire {{"points":[[0,{y}],[10,{y}]]}}"#));
        ok(&mut h, &format!(r#"version {{"name":"v{i}"}}"#));
    }
    h.run_steps(3);
    let g = h.state().panel.history.graph().clone();
    eprintln!("paging: {} rows, more = {}", g.nodes.len(), g.more);
    assert!(g.more && g.nodes.len() <= 210, "the first page: {} rows, more {}", g.nodes.len(), g.more);
    let first = g.nodes.len();
    // Scroll the graph to its end: the next page is read.
    let row = h.get_by_label_contains("Now on main").rect().center();
    for _ in 0..80 {
        h.event(egui::Event::PointerMoved(row));
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Page,
            delta: egui::vec2(0.0, -5.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(1);
    }
    h.run_steps(4);
    let g = h.state().panel.history.graph().clone();
    eprintln!("paging after scroll: {} rows, more = {}", g.nodes.len(), g.more);
    assert!(g.nodes.len() > first, "scrolled to the end, the next 200 rows are read");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_versions_details_name_the_devices_it_added() {
    let (mut h, dir, _) = opened("names");
    ok(&mut h, r#"version {"name":"Before"}"#);
    h.state_mut().save().unwrap();
    let placed = run(&mut h, "place", json!({"symbol": "coil", "at": [60, 150]}));
    let c = ecad_model::ComponentId::parse(placed["component"].as_str().unwrap()).unwrap();
    let tag = doc(&mut h).components.get(&c).and_then(|x| x.properties.get("tag").cloned()).unwrap();
    ok(&mut h, r#"version {"name":"After"}"#);
    h.state_mut().save().unwrap();
    let info = run(&mut h, "pointinfo", json!({"version": "After"}));
    eprintln!("pointinfo After: {info}");
    let s = info.to_string();
    assert!(s.contains(&format!("{tag} added")), "the summary names {tag}: {s}");
    let _ = std::fs::remove_dir_all(&dir);
}
