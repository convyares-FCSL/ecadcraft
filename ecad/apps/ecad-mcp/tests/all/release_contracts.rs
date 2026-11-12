//! Brief 027 (design 027 §10, check G): through `execute`, `prerelease` works with writes off,
//! records nothing and lists an `orphan-reference` row; `release` of the pending revision records
//! `Issue Rev B` with `rev.<n>.released` as the agent, writes the files and a manifest whose hashes
//! match, and is refused with writes off; with an error and no `accept` it refuses with the way
//! out, with `accept: "errors"` it succeeds; `release {rev: "A"}` with writes off writes the folder
//! and records nothing; an existing folder without `existing` refuses before any transaction; a
//! `folder` is refused (agents name no files).

use std::path::Path;

use ecad_mcp::{EcadMcp, Options};
use ecad_model::{Author, ObjectRef, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, TempDir};

/// A project saved as `<dir>/demo.ecad`: three loose wires, cable C1 with one reference end (an
/// orphan), an issued Rev A and a pending Rev B; returns the writer and the file.
fn project(dir: &TempDir) -> (Client, String) {
    let path = dir.file("demo.ecad");
    let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
    for y in [0, 5, 10] {
        c.exec("wire", json!({"points": [[60, y], [120, y]]}));
    }
    let cable = c.exec("cableslash", json!({"from": [88, -3], "to": [92, 18], "tag": "C1"}));
    c.exec("cableref", json!({"cable": cable["cable"], "at": [120, 5]}));
    c.exec("projectprops", json!({"drawing_number": "D-1001"}));
    c.exec("revision", json!({"description": "First issue", "drawn": "CVA"}));
    c.exec("issuerevision", json!({}));
    c.exec("revision", json!({"description": "Terminals added", "drawn": "CVA"}));
    c.ok("save", json!({}));
    (c, path)
}

/// The temporary folder itself.
fn root(dir: &TempDir) -> std::path::PathBuf {
    Path::new(&dir.file("demo.ecad")).parent().unwrap().to_path_buf()
}

fn manifest(dir: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(dir.join("release.json")).unwrap()).unwrap()
}

#[test]
fn release_records_the_issue_as_the_agent_and_prerelease_reads() {
    let dir = TempDir::new("027-go");
    let (mut c, _) = project(&dir);
    let before = c.log().len();
    let v = c.exec("prerelease", json!({}));
    assert_eq!(c.log().len(), before, "prerelease records nothing");
    let orphans = v["groups"].as_array().unwrap().iter().find(|g| g["group"] == "orphans").cloned().unwrap();
    assert_eq!(orphans["rows"][0]["kind"], "orphan-reference", "{v}");
    assert!(v.get("refusal").is_none(), "{v}");
    // A folder is a file: refused in every mode.
    let m = c.refused("execute", json!({"command": "release", "params": {"folder": "elsewhere"}}));
    assert!(m.contains("`folder` names a file"), "{m}");
    assert_eq!(c.log().len(), before);
    // Release: one transaction, as the agent, with FP1; the project is saved.
    let r = c.exec("release", json!({}));
    let log = c.log();
    assert_eq!(log.len(), before + 1, "{r}");
    let t = log.last().unwrap();
    assert_eq!(t.label, "Issue Rev B");
    assert_eq!(t.author, Author::Agent(AGENT.into()));
    assert!(
        t.ops.iter().any(|o| matches!(o, Op::SetProperty { object: ObjectRef::Project, key, after: Some(a), .. }
        if key == "rev.2.released" && a == "release/Rev B")),
        "{:?}",
        t.ops
    );
    assert_eq!(r["saved"], true);
    let out = root(&dir).join("release").join("Rev B");
    assert_eq!(r["folder"], json!(out.to_string_lossy()));
    for f in manifest(&out)["files"].as_array().unwrap() {
        let bytes = std::fs::read(out.join(f["name"].as_str().unwrap())).unwrap();
        assert_eq!(f["sha256"], ecad_cadcraft::release::sha256_hex(&bytes), "{f}");
    }
    // An existing folder without `existing`: refused before any transaction.
    c.exec("revision", json!({"description": "Again", "drawn": "CVA", "rev": "C"}));
    std::fs::create_dir_all(root(&dir).join("release").join("Rev C")).unwrap();
    let before = c.log().len();
    let m = c.refused("execute", json!({"command": "release", "params": {}}));
    assert!(m.contains("exists (This folder was not written by a release"), "{m}");
    assert!(m.contains("Give existing: \"add-missing\", \"replace\""), "{m}");
    assert_eq!(c.log().len(), before, "nothing issued");
    let r = c.exec("release", json!({"existing": "keep-both"}));
    assert!(r["folder"].as_str().unwrap().ends_with("Rev C (2)"), "{r}");
    assert_eq!(c.log().len(), before + 1);
}

#[test]
fn errors_need_accept_through_mcp() {
    let dir = TempDir::new("027-err");
    let (mut c, _) = project(&dir);
    c.exec("place", json!({"symbol": "continuation", "at": [300, 100], "name": "24V"}));
    c.exec("place", json!({"symbol": "continuation", "at": [300, 140], "rotation": 180, "name": "0V"}));
    c.exec("wire", json!({"points": [[300, 100], [300, 140]]}));
    let before = c.log().len();
    let pre = c.exec("prerelease", json!({}));
    assert!(pre["refusal"].as_str().unwrap().starts_with("1 error is open"), "{pre}");
    let m = c.refused("execute", json!({"command": "release", "params": {}}));
    assert!(m.contains("give accept: \"errors\" to release with it"), "{m}");
    assert_eq!(c.log().len(), before);
    c.exec("release", json!({"accept": "errors"}));
    assert_eq!(c.log().len(), before + 1);
    let out = root(&dir).join("release").join("Rev B");
    assert_eq!(manifest(&out)["checks"]["accepted_errors"], true);
}

#[test]
fn with_writes_off_release_of_the_pending_revision_is_refused_and_an_issued_one_is_written() {
    let dir = TempDir::new("027-off");
    let (c, path) = project(&dir);
    drop(c);
    let (s, _) = EcadMcp::open(&path, Options { write: false, agent: None }).unwrap();
    let mut r = Client::new(s);
    let before = r.log();
    r.exec("prerelease", json!({}));
    r.exec("prerelease", json!({"rev": "A"}));
    let m = r.refused("execute", json!({"command": "release", "params": {}}));
    assert!(m.contains("read-only"), "{m}");
    let v = r.exec("release", json!({"rev": "A"}));
    let a = root(&dir).join("release").join("Rev A");
    assert_eq!(v["folder"], json!(a.to_string_lossy()));
    assert_eq!(v["transactions"], json!([]));
    assert!(a.join("D-1001 Rev A.pdf").exists());
    assert_eq!(manifest(&a)["rev"], "A");
    // Again: the same CSVs, byte for byte.
    let csv = std::fs::read(a.join("D-1001 Rev A wires.csv")).unwrap();
    let v = r.exec("release", json!({"rev": "A", "existing": "replace"}));
    assert_eq!(v["compare"]["different"], json!([]), "{v}");
    assert_eq!(std::fs::read(a.join("D-1001 Rev A wires.csv")).unwrap(), csv);
    // The pending one cannot be named.
    let m = r.refused("execute", json!({"command": "release", "params": {"rev": "B"}}));
    assert!(m.contains("Rev B is not issued yet"), "{m}");
    assert_eq!(r.log(), before, "nothing recorded with writes off");
}
