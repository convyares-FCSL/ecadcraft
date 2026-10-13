//! The trial harness without a window: Save, Save As and Open through the trial's own file
//! dialog, the Save/Discard/Cancel close dialog (one page, two pages, quitting), read-only
//! projects, snapshot export, and frames laid out headless. What these tests cannot show is how it
//! looks and feels on a display.

use std::path::PathBuf;

use ecad_trial::{Answer, DialogKind, SNAPSHOT_NOTE, Trial};
use serde_json::{Value, json};

fn trial(snapshots: Option<PathBuf>) -> Trial {
    ecad_cadcraft::install();
    Trial::new(snapshots).unwrap()
}

/// A fresh folder under the system temp folder, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-trial-{name}-{}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn path(&self, name: &str) -> String {
        self.0.join(name).display().to_string()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn edit(t: &mut Trial) {
    t.cad.run("wire", json!({"points": [[0, 0], [40, 0]]})).unwrap();
}

fn doc(t: &mut Trial) -> ecad_model::Document {
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.document().clone()).unwrap()
}

fn one_frame(t: &mut Trial, ctx: &eframe::egui::Context) {
    let mut out = ctx.run_ui(eframe::egui::RawInput::default(), |ui| {
        t.logic(ui.ctx());
        t.ui(ui);
    });
    out.textures_delta.clear();
}

/// Save through the dialog the way a person does: Save (Ctrl+S), type a path, confirm.
fn save_via_dialog(t: &mut Trial, path: &str) {
    let _ = t.cad.run("qsave", Value::Null);
    t.after_logic();
    assert!(matches!(t.dialog(), Some((DialogKind::SaveAs, ..))), "Save of a new project asks for a file name");
    t.set_dialog_path(path);
    t.confirm_dialog();
    assert!(t.dialog().is_none(), "saved: {:?}", t.dialog());
}

#[test]
fn starts_with_one_new_project_and_nothing_to_lose() {
    let mut t = trial(None);
    assert_eq!(t.cad.session.docs.len(), 1);
    assert!(t.project_open());
    assert!(t.cad.session.log.iter().any(|l| l.contains("Save As writes the project to a .ecad file")));
    let s = t.status().unwrap();
    assert_eq!((s.pages, s.unsaved, s.file.path), (1, 0, None));
    // An untouched project closes without a question.
    t.cad.request_close(None);
    t.after_logic();
    assert!(t.pending_close().is_none());
    assert!(t.cad.session.docs.is_empty() && !t.project_open());
    t.new_project().unwrap();
    assert!(t.project_open());
    assert!(t.new_project().is_err(), "one project at a time");
}

#[test]
fn save_asks_for_a_file_name_once_then_saves_to_it() {
    let d = TempDir::new("save");
    let mut t = trial(None);
    edit(&mut t);
    // The dialog suggests the default folder.
    let _ = t.cad.run("qsave", Value::Null);
    t.after_logic();
    let (kind, suggested, _) = t.dialog().unwrap();
    assert_eq!(kind, DialogKind::SaveAs);
    assert_eq!(suggested, "~/ecad-projects/untitled.ecad");
    t.cancel_dialog();
    assert!(t.status().unwrap().unsaved > 0, "cancel saves nothing");
    // Saved to a chosen file (the extension is added).
    save_via_dialog(&mut t, &d.path("demo"));
    let path = d.path("demo.ecad");
    let s = t.status().unwrap();
    assert_eq!((s.name.as_str(), s.unsaved, s.file.path.as_deref()), ("demo", 0, Some(path.as_str())));
    assert!(!t.cad.session.doc_unsaved(0));
    assert!(t.cad.session.log.iter().any(|l| l.starts_with("Saved:")));
    // Later saves go to the same file, with no dialog.
    edit(&mut t);
    assert_eq!(t.status().unwrap().unsaved, 1);
    t.save().unwrap();
    assert!(t.dialog().is_none());
    assert_eq!(t.status().unwrap().unsaved, 0);
    // Save As from the menu asks again, suggesting the current file.
    let _ = t.cad.run("ui.saveas", Value::Null);
    t.after_logic();
    assert_eq!(t.dialog().map(|d| d.1.to_string()), Some(path));
    t.cancel_dialog();
    // Exports (no project file) still get no file dialog.
    assert!(t.cad.run("exportpdf", Value::Null).is_ok());
    t.after_logic();
    assert!(t.dialog().is_none());
}

#[test]
fn save_as_over_an_existing_file_asks_once_more() {
    let d = TempDir::new("replace");
    let other = d.path("other.ecad");
    std::fs::write(&other, b"someone else's file\n").unwrap();
    let mut t = trial(None);
    edit(&mut t);
    t.request_save_as();
    t.set_dialog_path(&other);
    t.confirm_dialog();
    let (_, _, error) = t.dialog().unwrap();
    assert!(error.unwrap_or_default().contains("already exists"), "{error:?}");
    assert_eq!(std::fs::read(&other).unwrap(), b"someone else's file\n");
    t.confirm_dialog();
    assert!(t.dialog().is_none());
    assert_eq!(t.status().unwrap().file.path.as_deref(), Some(other.as_str()));
}

#[test]
fn a_failed_save_keeps_the_dialog_open_with_the_reason() {
    let mut t = trial(None);
    edit(&mut t);
    t.request_save_as();
    t.set_dialog_path("/nonexistent-folder-for-ecad-tests/p.ecad");
    t.confirm_dialog();
    let (_, _, error) = t.dialog().unwrap();
    let error = error.unwrap_or_default().to_string();
    assert!(error.contains("not saved") && error.contains("unchanged"), "{error}");
    assert!(t.notice().is_none(), "reported once, in the dialog");
    assert_eq!(t.status().unwrap().unsaved, 1);
    t.set_dialog_path("x.dxf");
    t.confirm_dialog();
    assert!(t.dialog().unwrap().2.unwrap_or_default().contains(".ecad"));
}

#[test]
fn closing_with_unsaved_changes_offers_save_discard_or_cancel() {
    let d = TempDir::new("close");
    let mut t = trial(None);
    edit(&mut t);
    t.cad.request_close(None);
    t.after_logic();
    assert!(t.pending_close().is_some(), "the trial asks");
    assert!(t.cad.close_prompt().is_none(), "CADCraft's own prompt is not shown");
    t.answer(Answer::Cancel);
    assert!(t.project_open() && t.cad.session.doc_unsaved(0), "cancel keeps the project and its edit");
    // Save on a project with no file: the Save As dialog, then the close goes on.
    t.cad.request_close(None);
    t.after_logic();
    t.answer(Answer::Save);
    assert!(t.project_open(), "still open while the file name is asked");
    t.set_dialog_path(&d.path("closing.ecad"));
    t.confirm_dialog();
    assert!(!t.project_open(), "saved, then closed");
    // Reopen, edit, close with Save: saved to its file without a dialog.
    t.open_project(&d.path("closing.ecad")).unwrap();
    edit(&mut t);
    t.cad.request_close(None);
    t.after_logic();
    t.answer(Answer::Save);
    assert!(t.dialog().is_none() && !t.project_open());
    t.open_project(&d.path("closing.ecad")).unwrap();
    assert_eq!(doc(&mut t).conductors.len(), 2, "both wires were saved");
    // Discard: closed; the file keeps its last saved state.
    edit(&mut t);
    t.cad.request_close(None);
    t.after_logic();
    t.answer(Answer::Discard);
    assert!(!t.project_open());
    assert!(t.cad.session.log.iter().any(|l| l.contains("keeps its last saved state")));
    t.open_project(&d.path("closing.ecad")).unwrap();
    assert_eq!(doc(&mut t).conductors.len(), 2);
}

#[test]
fn cancelling_the_save_as_of_a_close_keeps_the_project_open() {
    let mut t = trial(None);
    edit(&mut t);
    t.cad.request_close(None);
    t.after_logic();
    t.answer(Answer::Save);
    t.cancel_dialog();
    assert!(t.project_open() && t.pending_close().is_none());
    assert!(t.cad.session.log.iter().any(|l| l.contains("Close cancelled")));
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
    let s = t.status().unwrap();
    assert_eq!((s.pages, s.unsaved), (1, 2), "the project and both edits are still there");
    t.cad.request_close(None);
    t.after_logic();
    assert!(t.pending_close().is_some(), "closing the last page asks");
    t.answer(Answer::Cancel);
    assert_eq!(t.cad.session.docs.len(), 1);
}

#[test]
fn quitting_with_unsaved_changes_asks_then_saves_or_discards() {
    let d = TempDir::new("quit");
    let mut t = trial(None);
    edit(&mut t);
    assert!(t.cad.request_quit(), "the window close is held");
    t.after_logic();
    assert!(t.pending_close().is_some());
    t.answer(Answer::Cancel);
    assert!(!t.cad.quit_requested && t.project_open(), "cancel keeps the app and the project");
    assert!(t.cad.request_quit());
    t.after_logic();
    t.answer(Answer::Save);
    t.set_dialog_path(&d.path("quit.ecad"));
    t.confirm_dialog();
    assert!(t.cad.quit_requested, "saved, then the app quits");
    assert!(std::path::Path::new(&d.path("quit.ecad")).exists());
    let mut t = trial(None);
    edit(&mut t);
    assert!(t.cad.request_quit());
    t.after_logic();
    t.answer(Answer::Discard);
    assert!(t.cad.quit_requested, "discard lets the app quit");
}

#[test]
fn open_reopens_a_saved_project_with_its_history() {
    let d = TempDir::new("open");
    let path = d.path("p.ecad");
    let mut t = trial(None);
    edit(&mut t);
    t.cad.run("newpage", json!({"title": "2"})).unwrap();
    save_via_dialog(&mut t, &path);
    let saved = doc(&mut t);
    assert!(t.open_project(&path).is_err(), "close the open project first");
    t.cad.request_close_all();
    t.after_logic();
    assert!(!t.project_open());
    // Opening through the dialog.
    t.request_open();
    t.set_dialog_path(&path);
    t.confirm_dialog();
    assert!(t.dialog().is_none());
    assert!(t.notice().is_none(), "a clean open needs no dialog");
    assert_eq!(doc(&mut t), saved, "same pages, objects and IDs");
    let s = t.status().unwrap();
    assert_eq!((s.name.as_str(), s.unsaved, s.pages), ("p", 0, 1));
    // History came back: undo removes page 2, then the wire.
    t.submit_command("u");
    assert_eq!(doc(&mut t).page_order.len(), 1);
    // A file that is not a project is refused in the dialog.
    t.cad.request_close_all();
    t.after_logic();
    t.answer(Answer::Discard);
    let junk = d.path("junk.ecad");
    std::fs::write(&junk, b"hello\n").unwrap();
    t.request_open();
    t.set_dialog_path(&junk);
    t.confirm_dialog();
    assert!(t.dialog().unwrap().2.unwrap_or_default().contains("could not open"));
    // Starting the app with a file that cannot be opened starts a new project and says why.
    let t2 = Trial::with_file(None, &junk).unwrap();
    assert!(t2.project_open());
    assert!(t2.notice().unwrap_or_default().contains("could not be opened"));
}

#[test]
fn a_second_window_on_the_same_file_is_read_only() {
    let d = TempDir::new("second");
    let path = d.path("shared.ecad");
    let mut first = trial(None);
    edit(&mut first);
    save_via_dialog(&mut first, &path);
    let mut second = Trial::with_file(None, &path).unwrap();
    assert!(second.notice().unwrap_or_default().contains("read-only"), "{:?}", second.notice());
    assert!(second.status().unwrap().file.read_only.is_some());
    second.dismiss_notice();
    edit(&mut second);
    let before = std::fs::read(&path).unwrap();
    assert!(second.save().is_err());
    assert!(second.notice().unwrap_or_default().contains("another session is editing it"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    // Closing with Save asks for another file name.
    second.dismiss_notice();
    second.cad.request_close(None);
    second.after_logic();
    second.answer(Answer::Save);
    assert!(matches!(second.dialog(), Some((DialogKind::SaveAs, ..))));
    second.set_dialog_path(&d.path("mine.ecad"));
    second.confirm_dialog();
    assert!(!second.project_open());
    assert!(std::path::Path::new(&d.path("mine.ecad")).exists());
}

#[test]
fn snapshots_are_exported_as_diagnostics_only() {
    let dir = TempDir::new("snapshots");
    let snaps = dir.0.join("snaps");
    let mut t = trial(Some(snaps.clone()));
    edit(&mut t);
    let p = t.export_snapshot("manual").unwrap();
    let snap: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    assert_eq!(snap["format"], "ecad-inspect/1");
    assert_eq!(snap["seq"], snap["head"]);
    assert!(t.cad.session.log.iter().any(|l| l.contains(SNAPSHOT_NOTE)));
    // Discarding exports one more first.
    t.cad.request_close(None);
    t.after_logic();
    t.answer(Answer::Discard);
    let files: Vec<_> = std::fs::read_dir(&snaps).unwrap().filter_map(Result::ok).map(|e| e.file_name()).collect();
    assert_eq!(files.len(), 2, "{files:?}");
    assert!(files.iter().any(|f| f.to_string_lossy().ends_with("-discarded.json")));
    assert!(files.iter().all(|f| f.to_string_lossy().starts_with("ecad-trial-")));
    // Without a folder, export is refused rather than written somewhere unexpected.
    let mut plain = trial(None);
    assert!(plain.export_snapshot("manual").is_err());
}

#[test]
fn frames_lay_out_headless_with_every_dialog() {
    let d = TempDir::new("frames");
    let ctx = eframe::egui::Context::default();
    let mut t = trial(Some(d.0.join("snaps")));
    one_frame(&mut t, &ctx);
    edit(&mut t);
    let _ = t.cad.run("qsave", Value::Null);
    one_frame(&mut t, &ctx);
    assert!(t.dialog().is_some(), "the Save As dialog is up");
    t.set_dialog_path("/nonexistent-folder-for-ecad-tests/p");
    t.confirm_dialog();
    one_frame(&mut t, &ctx);
    t.cancel_dialog();
    t.cad.request_close(None);
    one_frame(&mut t, &ctx);
    assert!(t.pending_close().is_some(), "the close dialog is up");
    one_frame(&mut t, &ctx);
    t.answer(Answer::Discard);
    one_frame(&mut t, &ctx);
    assert!(!t.project_open(), "after discard the banner offers New and Open");
    t.request_open();
    one_frame(&mut t, &ctx);
    t.cancel_dialog();
    t.new_project().unwrap();
    t.save().unwrap_or_default();
    one_frame(&mut t, &ctx);
}

/// The quick-start guide's manual trial, typed through the banner's command field. Mouse steps
/// (window selection, dragging) are replaced by their JSON forms; how they look on screen is not
/// checked. Keep in step with `ecad/docs/quick-start.md`.
#[test]
fn the_quick_start_trial_runs_headless() {
    use cadcraft_engine::doc::EntityKind;
    let d = TempDir::new("quickstart");
    let mut t = trial(None);
    let p = t.project().unwrap().clone();
    let ok = |t: &mut Trial, line: &str| {
        let e = t.submit_command(line);
        assert!(e.is_none(), "{line}: {e:?}");
    };
    // 1. Draw a rectangle and convert it into a component with a terminal.
    ok(&mut t, "line 0,0 40,0 40,20 0,20 c");
    let lines: Vec<_> = t.cad.session.doc().unwrap().model.iter().map(|e| e.handle).collect();
    assert_eq!(lines.len(), 4);
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
    ok(&mut t, &format!("addterminal {{\"handle\":\"{}\",\"at\":[40,10]}}", right.hex()));
    // 2. Place a second component (a coil symbol), rotated: its terminals land at 90,0 and 110,0.
    ok(&mut t, r#"place {"at":[100,0],"rotation":90}"#);
    // 3. Connect the two terminals with a wire.
    ok(&mut t, r#"wire {"points":[[40,10],[70,10],[70,0],[90,0]]}"#);
    let dc = doc(&mut t);
    let (cid, c) = dc.conductors.iter().next().unwrap();
    let cid = *cid;
    assert!(c.ends.iter().all(Option::is_some), "both ends attached");
    let page = dc.page_order[0];
    // 4. Connected movement: move the rectangle; the wire end follows.
    let (rid, rep) = dc
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
        |d: &ecad_model::Document| ecad_model::derive::route_on_page(d, cid, page).unwrap().1.path_ends().unwrap();
    assert_eq!(route_start(&doc(&mut t)).0, ecad_model::Pt::new(40.0, 5.0), "the wire end followed the terminal");
    assert!(ecad_model::derive::mismatches(&doc(&mut t), 1e-9).is_empty());
    // 5. Undo/redo.
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
    // 6. Two pages: a quiet tab close, then reopen with the content back.
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
    // 7. Save, close and reopen: connectivity, IDs and history survive.
    let path = d.path("quick-start.ecad");
    save_via_dialog(&mut t, &path);
    let saved = doc(&mut t);
    let history = p.read(&mut t.cad.session, |p, _| p.ledger.log().to_vec()).unwrap();
    t.cad.request_close_all();
    t.after_logic();
    assert!(t.pending_close().is_none(), "nothing unsaved: no question");
    assert!(!t.project_open());
    t.open_project(&path).unwrap();
    assert_eq!(doc(&mut t), saved);
    let reopened = t.project().unwrap().clone();
    assert_eq!(reopened.read(&mut t.cad.session, |p, _| p.ledger.log().to_vec()).unwrap(), history);
    assert!(ecad_model::derive::mismatches(&doc(&mut t), 1e-9).is_empty(), "still connected");
    ok(&mut t, "u");
    assert_eq!(doc(&mut t).page_order.len(), 2, "undo reaches back into the saved history");
    ok(&mut t, "redo");
    assert_eq!(doc(&mut t), saved);
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
