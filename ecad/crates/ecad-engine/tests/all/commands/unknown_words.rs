//! Lane 072 A1 (design 072 §9.5, F11, acceptance A4): a word this build does not know in a value
//! that stays text opens the project read-only, names the word, the setting and where it is, and
//! leaves the file exactly as it is.

use std::path::PathBuf;

use cadcraft_engine::Session;
use ecad_engine::{Persistence, ProjectHandle};
use ecad_model::{Author, ObjectRef, Op};
use serde_json::json;

use crate::common::H;

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A project file holding `text.tag.show = maybe` on the project (a file made elsewhere): its path.
fn file_with_unknown_word(dir: &TempDir, seed: u64) -> String {
    let mut h = H::new(seed);
    h.cfg(|p| {
        let op = Op::SetProperty { object: ObjectRef::Project, key: "text.tag.show".into(), before: None, after: Some("maybe".into()) };
        p.ledger.commit(Author::User, "a file made elsewhere", vec![op]).unwrap();
    });
    let path = dir.0.join("words072").display().to_string();
    let saved = saved_as(&mut h, &path);
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    saved
}

/// Save As `path`; the file the project is now bound to.
fn saved_as(h: &mut H, path: &str) -> String {
    h.cfg(|p| p.persistence = Persistence::File);
    let v = h.run("saveas", json!({"path": path})).unwrap();
    h.cfg(|p| p.path.clone()).unwrap_or_else(|| panic!("saved: {v}"))
}

#[test]
fn unknown_words_open_read_only() {
    let dir = TempDir(std::env::temp_dir().join(format!("ecad-072a-words-{}", ecad_store::disk::unique())));
    std::fs::create_dir_all(&dir.0).unwrap();
    let file = file_with_unknown_word(&dir, 0x72_a4);

    let mut s = Session::empty();
    let (project, summary) = ProjectHandle::open_file(&mut s, &file, 0x72_a5).unwrap();
    assert!(summary.contains("Opened read-only"), "{summary}");
    for named in ["`maybe`", "text.tag.show", "project"] {
        assert!(summary.contains(named), "the message names {named}: {summary}");
    }
    let mut h = H { s, project, trail: Vec::new() };
    let head = h.cfg(|p| p.ledger.head());
    h.run("projectprops", json!({"location": "+X9"})).unwrap();
    assert!(h.run("save", json!({})).is_err(), "saving a project opened for unknown words is refused");
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }

    // The file's content is as it was: same head, the word still stored. A read-only open (an agent
    // with writes off) leaves even the bytes as they are.
    let bytes = std::fs::read(&file).unwrap();
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file_read_only(&mut s, &file, 0x72_a6).unwrap();
    let mut h = H { s, project, trail: Vec::new() };
    let (again, word) = h.cfg(|p| (p.ledger.head(), p.ledger.document().display.get("text.tag.show").cloned()));
    assert_eq!((again, word.as_deref()), (head, Some("maybe")), "nothing was written");
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    assert_eq!(std::fs::read(&file).unwrap(), bytes, "a read-only open leaves the bytes as they are");
}

#[test]
fn known_words_open_writable() {
    let dir = TempDir(std::env::temp_dir().join(format!("ecad-072a-known-{}", ecad_store::disk::unique())));
    std::fs::create_dir_all(&dir.0).unwrap();
    let mut h = H::new(0x72_a6);
    h.run("setdefault", json!({"key": "text.tag.show", "level": "project", "value": "first"})).unwrap();
    let path = dir.0.join("known072").display().to_string();
    let saved = saved_as(&mut h, &path);
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    let mut s = Session::empty();
    let (_, summary) = ProjectHandle::open_file(&mut s, &saved, 0x72_a7).unwrap();
    assert!(!summary.contains("read-only"), "{summary}");
}

/// Lane 072 A2 (design 072 F5): a project symbol with the project's own prefix
/// (`tag_prefixes[project:<key>]`) and a starter's opens writable, and keeps both.
#[test]
fn a_project_symbol_with_its_own_prefix_opens_writable() {
    let dir = TempDir(std::env::temp_dir().join(format!("ecad-072a2-prefix-{}", ecad_store::disk::unique())));
    std::fs::create_dir_all(&dir.0).unwrap();
    let mut h = H::new(0x72_a8);
    let symbol = json!({
        "name": "Float switch", "key": "float1",
        "graphics": [{"kind": "polyline", "vertices": [[-2.5, -7.5], [2.5, -7.5], [2.5, 7.5], [-2.5, 7.5]], "closed": true}],
        "points": [{"key": 1, "label": "BN", "at": [0, 5]}, {"key": "2", "label": null, "at": [0, -5]}],
        "slots": [{"text": "tag", "at": [-4, 0], "h": "right", "v": "middle", "rotation": 0}],
        "defaults": {"prefix": "B", "description": "Float switch", "join": "device"},
    });
    h.run("symbolnew", symbol).unwrap();
    h.run("setdefault", json!({"key": "symbol.float1.prefix", "level": "project", "value": "BL"})).unwrap();
    h.run("setdefault", json!({"key": "symbol.coil.prefix", "level": "project", "value": "KA"})).unwrap();
    let keys = h.cfg(|p| p.ledger.document().settings.tag_prefixes.keys().map(ToString::to_string).collect::<Vec<_>>());
    assert_eq!(keys, ["starter:coil", "project:float1"]);
    let path = dir.0.join("prefix072").display().to_string();
    let saved = saved_as(&mut h, &path);
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    let mut s = Session::empty();
    let (project, summary) = ProjectHandle::open_file(&mut s, &saved, 0x72_a9).unwrap();
    assert!(!summary.contains("read-only"), "{summary}");
    let mut h = H { s, project, trail: Vec::new() };
    let float = h.cfg(|p| ecad_electrical::defaults::symbols::project_prefix(p.ledger.document(), "float1"));
    assert_eq!(float, "BL");
}

/// Lane 072 A2 (design 072 F6, F7): a symbol's text slot takes its rotation in degrees, like every
/// other command, and stores it in radians.
#[test]
fn a_slot_rotation_is_given_in_degrees_and_stored_in_radians() {
    let mut h = H::new(0x72_aa);
    let symbol = json!({
        "name": "Turned", "key": "turned1",
        "graphics": [{"kind": "polyline", "vertices": [[-2.5, -7.5], [2.5, -7.5], [2.5, 7.5], [-2.5, 7.5]], "closed": true}],
        "points": [{"key": 1, "label": null, "at": [0, 5]}, {"key": 2, "label": null, "at": [0, -5]}],
        "slots": [{"text": "tag", "at": [-4, 0], "h": "right", "v": "middle", "rotation": 90}],
        "defaults": {"prefix": "B", "join": "device"},
    });
    h.run("symbolnew", symbol).unwrap();
    let turns = h.cfg(|p| p.ledger.document().definitions.values().flat_map(|d| d.texts.iter().map(|t| t.rotation)).collect::<Vec<_>>());
    assert_eq!(turns, [ecad_model::geometry::units::Radians(std::f64::consts::FRAC_PI_2)]);
}
