//! Lane 072 D (overseer ruling 1, 2026-11-18): the junction symbol is gone; a T is a line landing on
//! a line. A file that still holds a junction (a tagless component of `kind = junction`) is read
//! strictly: it opens read-only, naming the word, and the file is kept as it is (pre-v1, no
//! conversion).

use std::path::PathBuf;

use cadcraft_engine::Session;
use ecad_cadcraft::{Persistence, ProjectHandle};
use ecad_model::connectivity::KIND;
use ecad_model::{Author, Device, DeviceId, Op};
use serde_json::json;

use crate::common::H;

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn close_all(h: &mut H) {
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
}

#[test]
fn a_file_holding_a_junction_opens_read_only() {
    let dir = TempDir(std::env::temp_dir().join(format!("ecad-072d-junction-{}", ecad_store::disk::unique())));
    std::fs::create_dir_all(&dir.0).unwrap();
    // A file made by an earlier build: a junction component on its own.
    let mut h = H::new(0x72_d7);
    h.cfg(|p| {
        let mut junction = Device::default();
        junction.properties.insert(KIND.into(), "junction".into());
        let op = Op::CreateDevice { id: DeviceId(0x72_d7), device: junction };
        p.ledger.commit(Author::User, "an earlier build's junction", vec![op]).unwrap();
    });
    h.cfg(|p| p.persistence = Persistence::File);
    let path = dir.0.join("junction072d").display().to_string();
    let v = h.run("saveas", json!({"path": path})).unwrap();
    let file = h.cfg(|p| p.path.clone()).unwrap_or_else(|| panic!("saved: {v}"));
    close_all(&mut h);

    let mut s = Session::empty();
    let (project, summary) = ProjectHandle::open_file(&mut s, &file, 0x72_d8).unwrap();
    assert!(summary.contains("Opened read-only"), "{summary}");
    for named in ["`junction`", "`kind`", "continuation"] {
        assert!(summary.contains(named), "the message names {named}: {summary}");
    }
    let mut h = H { s, project, trail: Vec::new() };
    assert!(h.run("save", json!({})).is_err(), "saving is refused");
    // `place` no longer offers it.
    let e = h.run("place", json!({"symbol": "junction", "at": [10, 10]})).unwrap_err();
    assert_eq!(e, "No symbol \"junction\". Type List to see them.");
    close_all(&mut h);

    // The junction is still stored as it was: nothing converted it.
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file_read_only(&mut s, &file, 0x72_d9).unwrap();
    let mut h = H { s, project, trail: Vec::new() };
    let kind = h.cfg(|p| p.ledger.document().devices.get(&DeviceId(0x72_d7)).and_then(|d| d.properties.get(KIND).cloned()));
    assert_eq!(kind.as_deref(), Some("junction"), "the file is kept as it is");
    close_all(&mut h);
}
