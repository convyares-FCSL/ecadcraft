//! Tester 027 (release) through MCP: `release {provisional: "finalise"}` records two
//! transactions as the agent (as `issuerevision` may), a release taken back by the per-call cap
//! leaves the file on disk agreeing with the reply, and `rev` given as odd text with writes off

use std::path::{Path, PathBuf};

use ecad_mcp::{EcadMcp, Options};
use ecad_model::Author;
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, TempDir};

fn root(dir: &TempDir) -> PathBuf {
    Path::new(&dir.file("demo.ecad")).parent().unwrap().to_path_buf()
}

/// Saved `<dir>/demo.ecad`: one wire, drawing number D-1001, Rev A issued, Rev B pending; with
/// `provisional`, two provisional coils.
fn project(dir: &TempDir, provisional: bool) -> (Client, String) {
    let path = dir.file("demo.ecad");
    let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
    c.exec("wire", json!({"points": [[60, 0], [120, 0]]}));
    c.exec("projectprops", json!({"drawing_number": "D-1001"}));
    c.exec("revision", json!({"description": "First issue", "drawn": "CVA"}));
    c.exec("issuerevision", json!({}));
    if provisional {
        c.exec("numbering", json!({"provisional": true}));
        c.exec("place", json!({"symbol": "coil", "at": [250, 150]}));
        c.exec("place", json!({"symbol": "coil", "at": [300, 150]}));
    }
    c.exec("revision", json!({"description": "Terminals added", "drawn": "CVA"}));
    c.ok("save", json!({}));
    (c, path)
}

fn exec_call(id: u64, command: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": "execute", "arguments": {"command": command, "params": params}}})
}

fn batch(c: &mut Client, msgs: Vec<Value>) -> Vec<Value> {
    let reply = c.s.handle_line(&Value::Array(msgs).to_string()).expect("a reply");
    serde_json::from_str::<Value>(&reply).unwrap().as_array().unwrap().clone()
}

fn is_error(v: &Value) -> bool {
    v["result"]["isError"] == json!(true) || v.get("error").is_some()
}

/// The versions named in the file on disk, read by a fresh read-only server.
fn versions_on_disk(path: &str) -> Vec<String> {
    let (s, _) = EcadMcp::open(path, Options { write: false, agent: None }).unwrap();
    let mut r = Client::new(s);
    r.doc().versions.keys().cloned().collect()
}

/// AC 2 and AC 6 together: 026's finalise choice through MCP. `issuerevision {provisional:
/// "finalise"}` may record two transactions in one call (tester 026); `release` with the same
/// choice records the same two and must be allowed them, as the agent.
#[test]
fn release_with_finalise_records_two_transactions_as_the_agent() {
    let dir = TempDir::new("t027-fin");
    let (mut c, path) = project(&dir, true);
    let before = c.log().len();
    let r = c.call("execute", json!({"command": "release", "params": {"provisional": "finalise"}}));
    assert_ne!(r["isError"], json!(true), "{r}");
    let log = c.log();
    let new: Vec<(String, Author)> = log.iter().skip(before).map(|t| (t.label.clone(), t.author.clone())).collect();
    assert_eq!(new, [("Finalise numbering".to_string(), Author::Agent(AGENT.into())), ("Issue Rev B".to_string(), Author::Agent(AGENT.into()))]);
    assert!(versions_on_disk(&path).iter().any(|v| v == "Rev B"));
}

/// Whatever the reply, the file on disk and the release folder agree with what the agent was
/// told: a release reported refused and taken back leaves no issued Rev B in the saved file.
#[test]
fn a_release_taken_back_by_the_cap_is_not_left_saved_on_disk() {
    let dir = TempDir::new("t027-cap");
    let (mut c, path) = project(&dir, false);
    let replies = batch(&mut c, vec![exec_call(1, "wire", json!({"points": [[0, 40], [60, 40]]})), exec_call(2, "release", json!({}))]);
    assert!(!is_error(&replies[0]), "{}", replies[0]);
    let taken_back = is_error(&replies[1]);
    let in_memory = c.doc().versions.contains_key("Rev B");
    let on_disk = versions_on_disk(&path).iter().any(|v| v == "Rev B");
    assert_eq!(in_memory, !taken_back, "{}", replies[1]);
    assert_eq!(on_disk, in_memory, "file on disk vs memory after: {}", replies[1]);
}

/// The same with finalise alone on the line (no other edit): over the cap of one today.
#[test]
fn a_release_with_finalise_is_never_left_half_taken_back() {
    let dir = TempDir::new("t027-fincap");
    let (mut c, path) = project(&dir, true);
    let r = c.call("execute", json!({"command": "release", "params": {"provisional": "finalise"}}));
    let refused = r["isError"] == json!(true);
    let in_memory = c.doc().versions.contains_key("Rev B");
    let on_disk = versions_on_disk(&path).iter().any(|v| v == "Rev B");
    assert_eq!(in_memory, !refused, "{r}");
    assert_eq!(on_disk, in_memory, "file on disk vs memory: {r}");
}

/// AC 6: with writes off, `rev` as empty, padded or lower-case text never issues the pending
/// revision; each is refused or writes an issued revision, recording nothing.
#[test]
fn with_writes_off_odd_rev_text_records_nothing() {
    let dir = TempDir::new("t027-off");
    let (c, path) = project(&dir, false);
    drop(c);
    let (s, _) = EcadMcp::open(&path, Options { write: false, agent: None }).unwrap();
    let mut r = Client::new(s);
    let before = r.log();
    for rev in ["", " ", " B ", "b", "B", " a "] {
        let _ = r.call("execute", json!({"command": "release", "params": {"rev": rev}}));
        assert_eq!(r.log(), before, "rev {rev:?}");
        assert!(!r.doc().versions.contains_key("Rev B"), "rev {rev:?}");
    }
    // Nothing named Rev B on disk either.
    assert!(!root(&dir).join("release").join("Rev B").exists());
    assert!(versions_on_disk(&path).iter().all(|v| v != "Rev B"));
}
