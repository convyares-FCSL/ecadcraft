//! The trial harness without a window: labelling, saving that reports unavailable persistence,
//! the Cancel/Discard close dialog (one page, two pages, quitting), snapshot export, and frames
//! laid out headless. What these tests cannot show is how it looks and feels on a display.

use std::path::PathBuf;

use ecad_trial::{Answer, SNAPSHOT_NOTE, Trial};
use serde_json::{Value, json};

fn trial(snapshots: Option<PathBuf>) -> Trial {
    ecad_cadcraft::install();
    Trial::new(snapshots).unwrap()
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ecad-trial-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn edit(t: &mut Trial) {
    t.cad.run("wire", json!({"points": [[0, 0], [40, 0]]})).unwrap();
}

fn one_frame(t: &mut Trial, ctx: &eframe::egui::Context) {
    let mut out = ctx.run_ui(eframe::egui::RawInput::default(), |ui| {
        t.logic(ui.ctx());
        t.ui(ui);
    });
    out.textures_delta.clear();
}

#[test]
fn starts_with_one_labelled_project_and_nothing_to_lose() {
    let mut t = trial(None);
    assert_eq!(t.cad.session.docs.len(), 1);
    assert!(t.project_open());
    assert!(t.cad.session.log.iter().any(|l| l.contains("this project cannot be saved")));
    assert_eq!(t.status(), Some((1, 0)));
    // An untouched project closes without a question.
    t.cad.request_close(None);
    t.after_logic();
    assert!(t.pending_close().is_none());
    assert!(t.cad.session.docs.is_empty() && !t.project_open());
    t.new_project().unwrap();
    assert!(t.project_open());
}

#[test]
fn save_and_save_as_report_that_persistence_is_unavailable() {
    let mut t = trial(None);
    edit(&mut t);
    let target = temp_dir("save").join("project.ecad");
    // The menu / Ctrl+S path (no file name yet: Save becomes Save As, which reaches the project).
    for (cmd, params) in [
        ("qsave", Value::Null),
        ("saveas", Value::Null),
        ("saveas", json!({"path": target.to_string_lossy()})),
        ("qsave", json!({"path": target.to_string_lossy()})),
    ] {
        t.dismiss_notice();
        let r = t.cad.run(cmd, params.clone());
        t.after_logic();
        let notice = t.notice().unwrap_or_default().to_string();
        assert!(notice.contains("Saving is not available"), "{cmd} {params}: notice {notice:?}, result {r:?}");
        assert!(t.cad.session.doc_unsaved(0), "{cmd}: never marked saved");
        assert_eq!(t.cad.session.docs[0].path, None, "{cmd}: no file name recorded");
    }
    assert!(!target.exists() && !target.parent().is_some_and(|d| d.exists()), "nothing written");
    assert_eq!(t.status(), Some((1, 1)), "the edit is still unsaved");
}

#[test]
fn closing_an_edited_project_offers_cancel_or_discard() {
    let mut t = trial(None);
    edit(&mut t);
    t.cad.request_close(None);
    t.after_logic();
    assert!(t.pending_close().is_some(), "the trial asks");
    assert!(t.cad.close_prompt().is_none(), "CADCraft's Save prompt is not shown");
    // Asking again while the question is open does not stack a second prompt.
    t.cad.request_close(None);
    t.after_logic();
    assert!(t.cad.close_prompt().is_none());
    t.answer(Answer::Cancel);
    assert!(t.pending_close().is_none());
    assert!(t.project_open() && t.cad.session.doc_unsaved(0), "cancel keeps the project and its edit");
    t.cad.request_close(None);
    t.after_logic();
    t.answer(Answer::Discard);
    assert!(!t.project_open() && t.cad.session.docs.is_empty(), "discard closes it");
    assert!(t.cad.session.log.iter().any(|l| l.contains("discarded")));
}

#[test]
fn a_page_tab_closes_quietly_and_the_last_page_asks() {
    let mut t = trial(None);
    edit(&mut t);
    t.cad.run("newpage", json!({"title": "2"})).unwrap();
    assert_eq!(t.cad.session.docs.len(), 2);
    t.cad.request_close(None); // page 2, the active tab
    t.after_logic();
    assert!(t.pending_close().is_none(), "another page is still open: no question");
    assert_eq!(t.cad.session.docs.len(), 1);
    assert_eq!(t.status(), Some((1, 2)), "the project and both edits are still there");
    t.cad.request_close(None);
    t.after_logic();
    assert!(t.pending_close().is_some(), "closing the last page asks");
    t.answer(Answer::Cancel);
    assert_eq!(t.cad.session.docs.len(), 1);
}

#[test]
fn quitting_with_an_edited_project_asks_then_quits_on_discard() {
    let mut t = trial(None);
    edit(&mut t);
    assert!(t.cad.request_quit(), "the window close is held");
    t.after_logic();
    assert!(t.pending_close().is_some());
    t.answer(Answer::Cancel);
    assert!(!t.cad.quit_requested && t.project_open(), "cancel keeps the app and the project");
    assert!(t.cad.request_quit());
    t.after_logic();
    t.answer(Answer::Discard);
    assert!(t.cad.quit_requested, "discard lets the app quit");
}

#[test]
fn snapshots_are_exported_as_diagnostics_only() {
    let dir = temp_dir("snapshots");
    let mut t = trial(Some(dir.clone()));
    edit(&mut t);
    let p = t.export_snapshot("manual").unwrap();
    let snap: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    assert_eq!(snap["format"], "ecad-inspect/1");
    assert_eq!(snap["seq"], snap["head"]);
    assert!(t.cad.session.log.iter().any(|l| l.contains(SNAPSHOT_NOTE)));
    // Discarding exports one more first; the project is still not saved anywhere.
    t.cad.request_close(None);
    t.after_logic();
    t.answer(Answer::Discard);
    let files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).map(|e| e.file_name()).collect();
    assert_eq!(files.len(), 2, "{files:?}");
    assert!(files.iter().any(|f| f.to_string_lossy().ends_with("-discarded.json")));
    assert!(files.iter().all(|f| f.to_string_lossy().starts_with("ecad-trial-")));
    // Without a folder, export is refused rather than written somewhere unexpected.
    let mut plain = trial(None);
    assert!(plain.export_snapshot("manual").is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn frames_lay_out_headless_with_and_without_dialogs() {
    let ctx = eframe::egui::Context::default();
    let mut t = trial(Some(temp_dir("frames")));
    one_frame(&mut t, &ctx);
    edit(&mut t);
    let _ = t.cad.run("qsave", Value::Null);
    one_frame(&mut t, &ctx);
    assert!(t.notice().is_some(), "the save notice is up");
    t.dismiss_notice();
    t.cad.request_close(None);
    one_frame(&mut t, &ctx);
    assert!(t.pending_close().is_some(), "the close dialog is up");
    one_frame(&mut t, &ctx);
    t.answer(Answer::Discard);
    one_frame(&mut t, &ctx);
    assert!(!t.project_open(), "after discard the banner offers a new project");
}

/// The README's manual trial, typed through the banner's command field. Mouse steps (window
/// selection, dragging) are replaced by their JSON forms; how they look on screen is not checked.
#[test]
fn the_manual_trial_script_runs_headless() {
    use cadcraft_engine::doc::EntityKind;
    let mut t = trial(None);
    let p = t.project().unwrap().clone();
    let doc = |t: &mut Trial| p.read(&mut t.cad.session, |p, _| p.ledger.document().clone()).unwrap();
    let ok = |t: &mut Trial, line: &str| {
        let e = t.submit_command(line);
        assert!(e.is_none(), "{line}: {e:?}");
    };
    // 1. Draw.
    ok(&mut t, "line 0,0 40,0 40,20 0,20 c");
    let lines: Vec<_> = t.cad.session.doc().unwrap().model.iter().map(|e| e.handle).collect();
    assert_eq!(lines.len(), 4);
    // 2. Convert (the window selection is set directly) and add a terminal on the right edge.
    t.cad.session.set_selection(lines.clone());
    ok(&mut t, "convertcomponent");
    let right = t
        .cad
        .session
        .doc()
        .unwrap()
        .model
        .iter()
        .find(|e| matches!(&e.kind, EntityKind::Line(l) if l.a.x == 40.0 && l.b.x == 40.0))
        .map(|e| e.handle)
        .unwrap();
    ok(&mut t, &format!("list {{\"handles\":[\"{}\"]}}", right.hex()));
    ok(&mut t, &format!("addterminal {{\"handle\":\"{}\",\"at\":[40,10]}}", right.hex()));
    // 3. Place, rotated 90 degrees: the coil's terminals land at 90,0 and 110,0.
    ok(&mut t, r#"place {"at":[100,0],"rotation":90}"#);
    // 4. Wire from the rectangle's terminal to A1.
    ok(&mut t, r#"wire {"points":[[40,10],[70,10],[70,0],[90,0]]}"#);
    assert!(
        t.cad
            .session
            .log
            .iter()
            .rev()
            .take(3)
            .any(|l| l.contains("\"attached\"") && l.matches("terminal").count() == 2)
    );
    let d = doc(&mut t);
    let (cid, c) = d.conductors.iter().next().unwrap();
    assert!(c.ends.iter().all(Option::is_some), "both ends attached");
    let page = d.page_order[0];
    // 5. Connected movement: move the rectangle (its group), then rotate the symbol.
    let (rid, rep) = d
        .representations
        .iter()
        .find(|(_, r)| r.form == ecad_model::Form::Drawn && matches!(r.subject, ecad_model::Subject::Component(_)))
        .unwrap();
    let rid = *rid;
    let mut moved: Vec<String> = Vec::new();
    for i in &rep.items {
        moved.push(p.read(&mut t.cad.session, |p, _| p.pages[&page].map.item(*i).unwrap().hex()).unwrap());
    }
    for term in rep.connection_points.keys() {
        moved.push(p.read(&mut t.cad.session, |p, _| p.pages[&page].map.marker(rid, *term).unwrap().hex()).unwrap());
    }
    ok(&mut t, &format!("move {{\"handles\":{},\"delta\":[0,-5]}}", serde_json::to_string(&moved).unwrap()));
    let route_start =
        |d: &ecad_model::Document| ecad_model::derive::route_on_page(d, *cid, page).unwrap().1.path_ends().unwrap();
    assert_eq!(route_start(&doc(&mut t)).0, ecad_model::Pt::new(40.0, 5.0), "the wire end followed the terminal");
    let sym =
        d.items.iter().find(|(_, i)| matches!(i.kind, ecad_model::ItemKind::Symbol(_))).map(|(id, _)| *id).unwrap();
    let sym = p.read(&mut t.cad.session, |p, _| p.pages[&page].map.item(sym).unwrap().hex()).unwrap();
    ok(&mut t, &format!("rotate {{\"handles\":[\"{sym}\"],\"base\":[100,0],\"angle\":180}}"));
    let end = route_start(&doc(&mut t)).1;
    assert!((end.x - 110.0).abs() < 1e-9 && end.y.abs() < 1e-9, "the wire end followed A1: {end:?}");
    assert!(ecad_model::derive::mismatches(&doc(&mut t), 1e-9).is_empty());
    // 6. Undo/redo three steps each way; UNDO options are refused.
    let before = doc(&mut t);
    for _ in 0..3 {
        ok(&mut t, "u");
    }
    assert_ne!(doc(&mut t), before);
    for _ in 0..3 {
        ok(&mut t, "redo");
    }
    assert_eq!(doc(&mut t), before, "redo returns to the same document, IDs included");
    let n = t.cad.session.log.len();
    let _ = t.submit_command(r#"undo {"mode":"mark"}"#);
    assert!(t.cad.session.log[n..].iter().any(|l| l.contains("not supported")));
    // 7. Save: a notice, still unsaved.
    ok(&mut t, "qsave");
    assert!(t.notice().is_some_and(|m| m.contains("Saving is not available")));
    t.dismiss_notice();
    // 8. Two pages: a quiet tab close, then reopen with the content back.
    ok(&mut t, r#"newpage {"title":"2"}"#);
    ok(&mut t, r#"wire {"points":[[0,0],[30,0]]}"#);
    assert_eq!(t.cad.session.docs.len(), 2);
    let p2 = doc(&mut t).page_order[1];
    t.cad.request_close(None);
    t.after_logic();
    assert!(t.pending_close().is_none() && t.cad.session.docs.len() == 1);
    ok(&mut t, &format!("openpage {{\"page\":\"{}\"}}", p2.hex()));
    assert_eq!(t.cad.session.docs.len(), 2);
    assert_eq!(t.cad.session.doc().unwrap().model.len(), 1, "page 2's wire is drawn again");
    // 10. Close everything: one question, for the last page.
    t.cad.request_close_all();
    t.after_logic();
    assert!(t.pending_close().is_some());
    t.answer(Answer::Discard);
    assert!(!t.project_open());
}

/// Spaces in the command field are inputs, as in a script line: a trailing space ends a command,
/// and an empty line is an Enter while one runs.
#[test]
fn the_command_field_keeps_trailing_enters() {
    let mut t = trial(None);
    assert!(t.submit_command("line 0,0 10,0 ").is_none());
    assert!(t.cad.session.running.is_none(), "the trailing space ended LINE");
    assert!(t.submit_command("line 0,5 10,5").is_none());
    assert!(t.cad.session.running.is_some(), "LINE waits for its next point");
    assert!(t.submit_command("").is_none());
    assert!(t.cad.session.running.is_none(), "an empty line is Enter");
    assert_eq!(t.cad.session.doc().unwrap().model.len(), 2);
    assert!(t.submit_command("   ").is_none(), "blank with nothing running does nothing");
}
