//! Tester contracts for brief 020a (history, versions, restore, branches) through the typed
//! commands: branch switching never loses or misplaces unsaved work, undo is per branch, a
//! second (read-only) opener sees branches created after it opened and writes nothing, a branch
//! from an old version starts at that version, Restore is one undoable step that leaves version
//! names and `rev.*` alone, and the file verifies after all of it.
#![cfg(not(target_arch = "wasm32"))]

use crate::common::{H, pts, small_project};

use std::path::PathBuf;

use cadcraft_engine::Session;
use ecad_cadcraft::{Persistence, ProjectHandle};
use ecad_model::Document;
use serde_json::{Value, json};

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-t020a-{name}-{}", ecad_store::disk::unique()));
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

fn close_all(s: &mut Session) {
    while !s.docs.is_empty() {
        s.execute("close", &json!({"index": 0})).unwrap();
    }
}

/// A saved file project (small fixture) at `path`.
fn saved(d: &TempDir, seed: u64) -> (H, String) {
    let (mut h, ..) = small_project(seed);
    h.cfg(|p| p.persistence = Persistence::File);
    let path = d.path("p.ecad");
    h.run("saveas", json!({"path": path})).unwrap();
    (h, path)
}

/// A branch action: the harness's one-transaction check does not apply (the head is the other
/// branch's afterwards).
fn raw(h: &mut H, cmd: &str, params: Value) -> Result<Value, String> {
    h.s.execute(cmd, &params).map_err(|e| e.to_string())
}

fn wire(h: &mut H, y: f64) {
    h.run("wire", json!({"points": pts(&[(200.0, y), (260.0, y)])})).unwrap();
}

fn current(h: &mut H) -> String {
    h.p(|p| p.file.v2().unwrap().branch_record().name.clone())
}

fn verify(path: &str) {
    let v = ecad_file::v2::verify::verify(&ecad_file::v2::Store::open(std::path::Path::new(path), false).unwrap());
    assert!(v.ok, "{:#?}", v.lines);
}

/// Without versions (history) and revision fields: what Restore compares.
fn restorable(d: &Document) -> Document {
    let mut d = d.clone();
    d.versions.clear();
    d.properties.retain(|k, _| !k.starts_with("rev."));
    d
}

// ------------------------------------------------------------------ switching and undo

#[test]
fn undo_and_redo_are_per_branch_across_save_and_switch() {
    let d = TempDir::new("undo");
    let (mut h, _path) = saved(&d, 2001);
    wire(&mut h, 10.0);
    wire(&mut h, 20.0);
    h.run("qsave", Value::Null).unwrap();
    let main_doc = h.doc();
    let main_before_last = h.p(|p| p.ledger.state_at(p.ledger.head() - 1).unwrap());
    raw(&mut h, "branch", json!({"name": "X"})).unwrap();
    assert_eq!(current(&mut h), "X");
    assert_eq!(h.doc(), main_doc, "the branch starts at main's head");
    let head = h.head();
    let _ = h.s.execute("u", &Value::Null);
    assert_eq!(h.doc(), main_doc, "undo on a new branch never reaches main's steps");
    assert_eq!(h.head(), head, "nothing of the branch's own to undo");
    wire(&mut h, 30.0);
    let x_doc = h.doc();
    raw(&mut h, "switchbranch", json!({"branch": "main", "save": true})).unwrap();
    assert_eq!(h.doc(), main_doc, "main does not carry X's edit");
    // `u` on main undoes main's own last step, not X's.
    h.run("u", Value::Null).unwrap();
    assert_eq!(h.doc(), main_before_last);
    h.run("redo", Value::Null).unwrap();
    assert_eq!(h.doc(), main_doc);
    // Undo on main, unsaved; Save and switch saves it on main, not on X.
    h.run("u", Value::Null).unwrap();
    raw(&mut h, "switchbranch", json!({"branch": "X", "save": true})).unwrap();
    assert_eq!(h.doc(), x_doc, "X is as it was left");
    h.run("u", Value::Null).unwrap();
    assert_eq!(h.doc(), main_doc, "X's undo takes back X's own step");
    h.run("redo", Value::Null).unwrap();
    assert_eq!(h.doc(), x_doc);
    raw(&mut h, "switchbranch", json!({"branch": "main", "save": true})).unwrap();
    assert_eq!(h.doc(), main_before_last, "main's saved undo stayed on main");
    h.run("redo", Value::Null).unwrap();
    assert_eq!(h.doc(), main_doc, "main's redo top survived the round trip through X");
}

#[test]
fn branch_from_an_old_version_with_unsaved_edits_saves_them_on_main_and_starts_at_the_version() {
    let d = TempDir::new("oldversion");
    let (mut h, path) = saved(&d, 2002);
    wire(&mut h, 10.0);
    h.run("version", json!({"name": "V1"})).unwrap();
    let at_v1 = h.doc();
    h.run("qsave", Value::Null).unwrap();
    wire(&mut h, 20.0);
    wire(&mut h, 30.0);
    let main_live = h.doc();
    assert!(h.p(|p| p.unsaved()));
    let r = raw(&mut h, "branch", json!({"name": "From V1", "from": {"version": "V1"}})).unwrap();
    assert_eq!(r["switched"], json!(true), "{r}");
    assert_eq!(restorable(&h.doc()), restorable(&at_v1), "the branch starts at V1's state");
    assert!(!h.doc().conductors.is_empty());
    assert!(!h.p(|p| p.unsaved()), "nothing unsaved right after switching");
    // The parent is untouched and kept its unsaved edits (they were saved on main first).
    raw(&mut h, "switchbranch", json!({"branch": "main"})).unwrap();
    assert_eq!(h.doc(), main_live, "main kept its edits");
    raw(&mut h, "switchbranch", json!({"branch": "From V1"})).unwrap();
    wire(&mut h, 40.0);
    let branch_doc = h.doc();
    h.run("qsave", Value::Null).unwrap();
    // Reopen lands on the last-used branch.
    close_all(&mut h.s);
    drop(h);
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, &path, 91).unwrap();
    let mut h2 = H { s, project, trail: Vec::new() };
    assert_eq!(current(&mut h2), "From V1");
    assert_eq!(h2.doc(), branch_doc);
    h2.run("u", Value::Null).unwrap();
    assert_eq!(restorable(&h2.doc()), restorable(&at_v1), "undo after reopen is the branch's own step");
    raw(&mut h2, "switchbranch", json!({"branch": "main", "save": true})).unwrap();
    assert_eq!(h2.doc(), main_live);
    close_all(&mut h2.s);
    drop(h2);
    verify(&path);
}

#[test]
fn switching_with_unsaved_edits_and_no_save_is_refused_and_keeps_them() {
    let d = TempDir::new("refuse");
    let (mut h, _path) = saved(&d, 2003);
    raw(&mut h, "branch", json!({"name": "B", "switch": false})).unwrap();
    wire(&mut h, 10.0);
    let live = h.doc();
    let e = raw(&mut h, "switchbranch", json!({"branch": "B"})).unwrap_err();
    assert!(e.contains("unsaved") && e.contains("\"save\": true"), "{e}");
    assert_eq!(current(&mut h), "main");
    assert_eq!(h.doc(), live, "the unsaved edit is still there");
    assert!(h.p(|p| p.unsaved()));
    // Archive of the branch shown saves first and goes to its parent.
    raw(&mut h, "switchbranch", json!({"branch": "B", "save": true})).unwrap();
    wire(&mut h, 50.0);
    let b_doc = h.doc();
    raw(&mut h, "archivebranch", json!({"branch": "B"})).unwrap();
    assert_eq!(current(&mut h), "main");
    assert_eq!(h.doc(), live);
    raw(&mut h, "reopenbranch", json!({"branch": "B"})).unwrap();
    raw(&mut h, "switchbranch", json!({"branch": "B"})).unwrap();
    assert_eq!(h.doc(), b_doc, "archiving the shown branch saved its edit first");
}

// ------------------------------------------------------------------ second opener

/// A second session (read-only: the first holds the lock) opened before a branch was created
/// sees it, can show it, and writes nothing (design 020 §3.5, §13 L).
#[test]
fn a_second_opener_sees_a_branch_created_after_it_opened_and_writes_nothing() {
    let d = TempDir::new("second");
    let (mut h, path) = saved(&d, 2004);
    let main_doc = h.doc();
    let mut s = Session::empty();
    let (project, summary) = ProjectHandle::open_file(&mut s, &path, 92).unwrap();
    assert!(summary.contains("read-only"), "{summary}");
    let mut h2 = H { s, project, trail: Vec::new() };
    // The writer creates a branch and saves an edit on it.
    raw(&mut h, "branch", json!({"name": "Late"})).unwrap();
    wire(&mut h, 10.0);
    h.run("qsave", Value::Null).unwrap();
    let late_doc = h.doc();
    // The reader lists and shows it.
    let list = h2.run("branches", json!({})).unwrap();
    assert_eq!(list["branches"].as_array().unwrap().len(), 2, "{list}");
    let e = raw(&mut h2, "branch", json!({"name": "Mine"})).unwrap_err();
    assert!(e.contains("read-only"), "{e}");
    raw(&mut h2, "switchbranch", json!({"branch": "Late"})).unwrap();
    assert_eq!(h2.doc(), late_doc);
    raw(&mut h2, "switchbranch", json!({"branch": "main"})).unwrap();
    assert_eq!(h2.doc(), main_doc);
    // The reader wrote no `meta.active`: the writer's file still reopens on Late.
    close_all(&mut h2.s);
    close_all(&mut h.s);
    drop((h, h2));
    let o = ecad_file::v2::disk::DiskProjectV2::open(std::path::Path::new(&path), 1, "probe").unwrap();
    assert_eq!(o.project.project().branch_record().name, "Late", "the read-only session changed meta.active");
    drop(o);
    verify(&path);
}

/// Design 020 §3.3: on an unwritable file with unsaved changes, switching asks for Save As; the
/// typed command must not drop the read-only session's unsaved edits by switching anyway.
#[test]
fn a_read_only_session_with_unsaved_edits_is_not_switched_away_from_them() {
    let d = TempDir::new("rounsaved");
    let (mut h, path) = saved(&d, 2005);
    raw(&mut h, "branch", json!({"name": "B", "switch": false})).unwrap();
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, &path, 93).unwrap();
    let mut h2 = H { s, project, trail: Vec::new() };
    h2.run("newpage", json!({"title": "mine"})).unwrap();
    let mine = h2.doc();
    assert!(h2.p(|p| p.unsaved()));
    let r = raw(&mut h2, "switchbranch", json!({"branch": "B"}));
    let r2 = if r.is_ok() { None } else { Some(raw(&mut h2, "switchbranch", json!({"branch": "B", "save": true}))) };
    assert!(
        r.is_err() && r2.as_ref().is_some_and(Result::is_err),
        "switched a read-only session with unsaved edits: {r:?} / {r2:?}; work lost: {}",
        h2.doc() != mine
    );
    assert_eq!(h2.doc(), mine, "the unsaved edits are still shown");
    drop(h);
}

// ------------------------------------------------------------------ restore

#[test]
fn restore_is_one_undoable_step_that_keeps_versions_and_revision_fields() {
    let d = TempDir::new("restore");
    let (mut h, path) = saved(&d, 2006);
    wire(&mut h, 10.0);
    h.run("version", json!({"name": "Before"})).unwrap();
    let before = h.doc();
    // A page renamed while its objects stay on it (a RemovePage + AddPage pair).
    h.run("pageprops", json!({"title": "Renamed"})).unwrap();
    h.run("newpage", json!({"title": "Extra"})).unwrap();
    wire(&mut h, 20.0);
    h.run("issuerevision", json!({"rev": "A", "description": "first issue", "drawn": "CV"})).unwrap();
    wire(&mut h, 30.0);
    let live = h.doc();
    let rev_keys: Vec<(String, String)> =
        live.properties.iter().filter(|(k, _)| k.starts_with("rev.")).map(|(k, v)| (k.clone(), v.clone())).collect();
    assert!(!rev_keys.is_empty(), "Rev A wrote rev.* fields");
    let log_len = h.p(|p| p.ledger.full_log().unwrap().len());
    let r = h.run("restore", json!({"version": "Before"})).unwrap();
    assert!(r["objects"].as_u64().unwrap() >= 3, "{r}");
    let after = h.doc();
    assert_eq!(restorable(&after), restorable(&before), "the state equals the version's");
    assert_eq!(after.versions, live.versions, "version names untouched (Before and Rev A)");
    let rev_after: Vec<(String, String)> =
        after.properties.iter().filter(|(k, _)| k.starts_with("rev.")).map(|(k, v)| (k.clone(), v.clone())).collect();
    assert_eq!(rev_after, rev_keys, "rev.* untouched");
    assert_eq!(h.p(|p| p.ledger.full_log().unwrap().len()), log_len + 1, "one transaction, history intact");
    h.run("u", Value::Null).unwrap();
    assert_eq!(h.doc(), live, "undo brings everything back");
    h.run("redo", Value::Null).unwrap();
    assert_eq!(h.doc(), after);
    h.run("qsave", Value::Null).unwrap();
    // Restore to the project's very first point, then reopen and verify.
    h.run("restore", json!({"seq": 1})).unwrap();
    let first = h.p(|p| p.ledger.state_at(1).unwrap());
    assert_eq!(restorable(&h.doc()), restorable(&first));
    h.run("qsave", Value::Null).unwrap();
    let saved_doc = h.doc();
    close_all(&mut h.s);
    drop(h);
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, &path, 94).unwrap();
    let mut h2 = H { s, project, trail: Vec::new() };
    assert_eq!(h2.doc(), saved_doc);
    h2.run("u", Value::Null).unwrap();
    assert_eq!(h2.doc(), after, "undo of a restore survives reopening");
    close_all(&mut h2.s);
    drop(h2);
    verify(&path);
}

#[test]
fn restore_on_a_branch_reaches_its_ancestry_before_the_fork_only() {
    let d = TempDir::new("restorebranch");
    let (mut h, path) = saved(&d, 2007);
    wire(&mut h, 10.0);
    h.run("version", json!({"name": "Base"})).unwrap();
    let base = h.doc();
    wire(&mut h, 20.0);
    raw(&mut h, "branch", json!({"name": "B"})).unwrap();
    raw(&mut h, "switchbranch", json!({"branch": "main"})).unwrap();
    wire(&mut h, 30.0);
    h.run("version", json!({"name": "After fork"})).unwrap();
    raw(&mut h, "switchbranch", json!({"branch": "B", "save": true})).unwrap();
    wire(&mut h, 40.0);
    let e = h.run("restore", json!({"version": "After fork", "branch": "main"})).unwrap_err();
    assert!(e.contains("another branch") || e.contains("branch from here"), "{e}");
    let b_live = h.doc();
    h.run("restore", json!({"version": "Base", "branch": "main"})).unwrap();
    assert_eq!(restorable(&h.doc()), restorable(&base), "a point of the parent before the fork restores");
    h.run("u", Value::Null).unwrap();
    assert_eq!(h.doc(), b_live);
    h.run("qsave", Value::Null).unwrap();
    close_all(&mut h.s);
    drop(h);
    verify(&path);
}

// ------------------------------------------------------------------ revisions

#[test]
fn revisions_are_refused_off_main_with_a_way_forward() {
    let d = TempDir::new("rev");
    let (mut h, _path) = saved(&d, 2008);
    raw(&mut h, "branch", json!({"name": "Option B"})).unwrap();
    let head = h.head();
    let e = h.run("issuerevision", json!({"rev": "A", "description": "x", "drawn": "CV"})).unwrap_err();
    assert!(e.contains("issued on main") && e.contains("Merge") && e.contains("Option B"), "{e}");
    assert_eq!(h.head(), head, "nothing recorded");
}

/// Review of PR #48: Save As from a read-only session that switched branch keeps the work on that
/// branch. A read-only switch writes no `meta.active`, so the copy used to open on `main` and
/// save the branch's pending steps there.
#[test]
fn save_as_after_a_read_only_switch_keeps_the_branch() {
    let d = TempDir::new("rosaveas");
    let (mut h, path) = saved(&d, 2099);
    let main_doc = h.doc();
    raw(&mut h, "branch", json!({"name": "B", "switch": false})).unwrap();
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, &path, 95).unwrap();
    let mut h2 = H { s, project, trail: Vec::new() };
    raw(&mut h2, "switchbranch", json!({"branch": "B"})).unwrap();
    assert_eq!(current(&mut h2), "B");
    h2.run("newpage", json!({"title": "mine"})).unwrap();
    let mine = h2.doc();
    let other = d.path("rescued.ecad");
    let r = raw(&mut h2, "saveas", json!({"path": other}));
    assert!(r.is_ok(), "{r:?}");
    assert_eq!(current(&mut h2), "B");
    close_all(&mut h2.s);
    drop(h2);
    let o = ecad_file::v2::disk::DiskProjectV2::open(std::path::Path::new(&other), 1, "probe").unwrap();
    assert_eq!(o.project.project().branch_record().name, "B");
    let opened_doc = o.ledger.document().clone();
    drop(o);
    verify(&other);
    verify(&path);
    assert_eq!(opened_doc, mine, "the rescued file opens on B with the edits");
    assert_ne!(opened_doc, main_doc);
    drop(h);
}
