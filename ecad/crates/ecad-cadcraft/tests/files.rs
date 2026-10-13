//! Project files on disk through the adapter (persistence stage 2a): Save As, Save, close and
//! reopen keep connectivity, IDs and history; a second opener is read-only; paths are checked.
#![cfg(not(target_arch = "wasm32"))]

mod common;

use std::path::PathBuf;

use cadcraft_engine::Session;
use common::{H, pts, small_project};
use ecad_cadcraft::{Persistence, ProjectHandle, normalize};
use ecad_model::Pt;
use serde_json::{Value, json};

/// A fresh folder under the system temp folder, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-cadcraft-{name}-{}", ecad_store::disk::unique()));
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

/// Close every open document (the engine's CLOSE, as the window's close prompts end in).
fn close_all(s: &mut Session) {
    while !s.docs.is_empty() {
        s.execute("close", &json!({"index": 0})).unwrap();
    }
}

#[test]
fn save_close_and_reopen_keep_connectivity_ids_and_history() {
    let d = TempDir::new("reopen");
    let (mut h, c1, _c2, w) = small_project(31);
    h.cfg(|p| p.persistence = Persistence::File);
    let first = h.first_page();
    h.run("newpage", json!({"title": "2"})).unwrap();
    h.run("wire", json!({"points": pts(&[(0.0, 0.0), (30.0, 0.0)])})).unwrap();
    h.activate(first);

    // Save before the project has a file: refused, nothing written.
    let e = h.run("qsave", Value::Null).unwrap_err();
    assert!(e.contains("no file yet"), "{e}");
    assert!(h.p(|p| p.unsaved()));

    // Save As with a bare name: `.ecad` is added, the project and its pages are bound to it.
    let r = h.run("saveas", json!({"path": d.path("demo")})).unwrap();
    let path = d.path("demo.ecad");
    assert_eq!(r["path"], path);
    assert!(!h.p(|p| p.unsaved()));
    assert_eq!(h.p(|p| p.name.clone()), "demo");
    let i = h.page_index(first).unwrap();
    assert_eq!(h.s.docs[i].path.as_deref(), Some(path.as_str()));
    assert!(h.s.docs[i].title.starts_with("demo - page"), "{}", h.s.docs[i].title);
    assert!(!h.s.doc_unsaved(i));

    // Connected movement, then Save to the same file.
    let before_move = h.doc();
    let handles = h.rep_handles(first, c1);
    h.run("move", json!({"handles": handles, "delta": [0, 5]})).unwrap();
    assert!(h.p(|p| p.unsaved()));
    h.run("qsave", Value::Null).unwrap();
    assert!(!h.p(|p| p.unsaved()));
    h.check();
    let live = h.doc();
    let log = h.p(|p| p.ledger.log().to_vec());
    let wire_end = h.route_ends(first, w).0;
    assert!(wire_end.near(Pt::new(40.0, 15.0), 1e-9), "the wire end followed the move");

    // Closing the project lets go of the file: another session could now write it.
    close_all(&mut h.s);
    let probe = ecad_file::DiskProject::open(std::path::Path::new(&path), 1, "probe").unwrap();
    assert!(probe.project.read_only().is_none(), "the writer lock was released on close");
    drop(probe);

    // Reopen in a fresh session.
    let mut s = Session::empty();
    let (project, summary) = ProjectHandle::open_file(&mut s, &path, 77).unwrap();
    assert!(summary.starts_with(&format!("Opened {path}")), "{summary}");
    assert!(!summary.contains("read-only"), "{summary}");
    let mut h2 = H { s, project, trail: Vec::new() };
    assert_eq!(h2.doc(), live, "the same objects, IDs, connections and pages");
    assert_eq!(h2.p(|p| p.ledger.log().to_vec()), log, "the same history");
    assert!(!h2.p(|p| p.unsaved()));
    assert_eq!(h2.doc().page_order.len(), 2);
    h2.check();
    // Connectivity survives: moving the component again still drags the wire end.
    let handles = h2.rep_handles(first, c1);
    h2.run("move", json!({"handles": handles, "delta": [0, 5]})).unwrap();
    assert!(h2.route_ends(first, w).0.near(Pt::new(40.0, 20.0), 1e-9));
    h2.check();
    // History survives: undo steps back through the move made before saving.
    h2.run("u", Value::Null).unwrap();
    assert_eq!(h2.doc(), live);
    h2.run("u", Value::Null).unwrap();
    assert_eq!(h2.doc(), before_move);
    h2.run("redo", Value::Null).unwrap();
    assert_eq!(h2.doc(), live);
    assert!(h2.p(|p| p.unsaved()), "undo and redo are new transactions to save");
    h2.run("qsave", Value::Null).unwrap();
    assert!(!h2.p(|p| p.unsaved()));
}

#[test]
fn a_second_session_opens_read_only_and_can_save_a_copy() {
    let d = TempDir::new("second");
    let (mut h, ..) = small_project(32);
    h.cfg(|p| p.persistence = Persistence::File);
    let path = d.path("shared.ecad");
    h.run("saveas", json!({"path": path})).unwrap();

    let mut s = Session::empty();
    let (project, summary) = ProjectHandle::open_file(&mut s, &path, 78).unwrap();
    assert!(summary.contains("Opened read-only") && summary.contains("another session is editing it"), "{summary}");
    let mut h2 = H { s, project, trail: Vec::new() };
    assert!(h2.p(|p| p.file_status().read_only.is_some()));
    h2.run("newpage", json!({"title": "mine"})).unwrap();
    let before = std::fs::read(&path).unwrap();
    let e = h2.run("qsave", Value::Null).unwrap_err();
    assert!(e.contains("another session is editing it") && e.contains("Save As"), "{e}");
    assert_eq!(std::fs::read(&path).unwrap(), before, "the other session's file is untouched");
    // Save As to another file keeps the work and the project ID.
    let id = h.p(|p| p.id);
    let copy = d.path("mine.ecad");
    h2.run("saveas", json!({"path": copy})).unwrap();
    assert!(!h2.p(|p| p.unsaved()));
    assert_eq!(h2.p(|p| p.id), id);
    assert!(h2.p(|p| p.file_status().read_only.is_none()));
}

#[test]
fn file_names_are_checked_and_completed() {
    assert!(normalize("  ").is_err());
    assert!(normalize("plan.dxf").unwrap_err().contains(".ecad"));
    assert_eq!(normalize("a/b").unwrap(), "a/b.ecad");
    assert_eq!(normalize("a/B.ECAD").unwrap(), "a/B.ECAD");
    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
    {
        assert_eq!(normalize("~/p").unwrap(), format!("{}/p.ecad", home.trim_end_matches('/')));
    }
    let (mut h, ..) = small_project(33);
    h.cfg(|p| p.persistence = Persistence::File);
    assert!(h.run("saveas", json!({"path": ""})).unwrap_err().contains("needs a file name"));
    assert!(h.run("saveas", json!({"path": "x.dwg"})).unwrap_err().contains(".ecad"));
    let e = h.run("saveas", json!({"path": "/nonexistent-folder-for-ecad-tests/p.ecad"})).unwrap_err();
    assert!(e.contains("not saved") && e.contains("unchanged"), "{e}");
    assert!(h.p(|p| p.unsaved() && p.path.is_none()));
}

#[test]
fn opening_a_file_that_is_not_a_project_reports_why() {
    let d = TempDir::new("notproject");
    let p = d.path("notes.ecad");
    std::fs::write(&p, b"hello\n").unwrap();
    let mut s = Session::empty();
    let e = ProjectHandle::open_file(&mut s, &p, 1).err().unwrap();
    assert!(e.contains("could not open") && e.contains("header"), "{e}");
    assert!(s.docs.is_empty());
    assert!(ProjectHandle::open_file(&mut s, &d.path("missing"), 1).is_err());
}
