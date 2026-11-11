//! Brief 020b, MCP contract (the ECAD rule of brief 011): `merge` (into the parent, and update
//! from the parent) and `undomerge` through `execute` record one merge transaction by the agent;
//! the preview reads and records nothing, also with writes off; a conflict without a choice is
//! refused naming it and how to choose; plain undo of a merge asks first.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::{Author, TxKind};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, TempDir};

/// A project file with one device (`-K01`, description "Pump") and one wire, saved.
fn project(dir: &TempDir, name: &str) -> (Client, String, String) {
    let path = dir.file(name);
    let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
    let coil = c.exec("place", json!({"symbol": "coil", "at": [0, 0]}))["component"].as_str().unwrap().to_string();
    c.exec("setprop", json!({"component": coil, "key": "description", "value": "Pump"}));
    c.exec("wire", json!({"points": [[0, 40], [60, 40]]}));
    c.ok("save", json!({}));
    (c, coil, path)
}

fn description(c: &mut Client, coil: &str) -> Option<String> {
    let id = ecad_model::ComponentId::parse(coil).unwrap();
    c.doc().components.get(&id).and_then(|x| x.properties.get("description").cloned())
}

fn current(c: &mut Client) -> String {
    c.exec("branches", json!({}))["branches"].as_array().unwrap().iter().find(|b| b["current"] == json!(true)).unwrap()["name"]
        .as_str()
        .unwrap()
        .to_string()
}

fn status(c: &mut Client, name: &str) -> String {
    c.exec("branches", json!({}))["branches"].as_array().unwrap().iter().find(|b| b["name"] == json!(name)).unwrap()["status"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn merge_into_main_and_undo_merge_through_execute_record_the_agents_merge() {
    let dir = TempDir::new("merge");
    let (mut c, coil, _) = project(&dir, "merge.ecad");
    c.exec("version", json!({"name": "Before change"}));
    c.exec("branch", json!({"name": "Option B"}));
    c.exec("setprop", json!({"component": coil, "key": "description", "value": "Pump 1"}));
    c.exec("wire", json!({"points": [[0, 80], [60, 80]]}));
    let wires = c.doc().conductors.len();
    // The preview reads: nothing recorded, still on the branch.
    let before = c.log().len();
    let pv = c.exec("merge", json!({"from": "Option B", "preview": true}));
    assert_eq!(c.log().len(), before);
    assert_eq!(current(&mut c), "Option B");
    assert_eq!(pv["conflicts"], json!([]), "{pv}");
    assert!(pv["changes"].as_array().unwrap().len() >= 2, "{pv}");
    assert!(pv["header"].as_str().unwrap().starts_with("Based on version \"Before change\""), "{pv}");
    assert!(pv["counts"].as_str().unwrap().contains("0 conflicts"), "{pv}");
    // Merge: switches to main (saving the branch first), then one merge step by the agent.
    let r = c.exec("merge", json!({"from": "Option B"}));
    assert_eq!(r["merged"], json!(true), "{r}");
    assert_eq!(current(&mut c), "main");
    let last = c.log().last().cloned().unwrap();
    assert!(matches!(last.kind, TxKind::Merge { .. }), "{:?}", last.kind);
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    assert_eq!(last.label, "Merge \"Option B\" (based on version \"Before change\")");
    assert_eq!(r["ecadTransactions"].as_array().map(Vec::len), Some(1), "one transaction counted: {r}");
    assert_eq!(description(&mut c, &coil).as_deref(), Some("Pump 1"));
    assert_eq!(c.doc().conductors.len(), wires);
    c.ok("save", json!({}));
    assert_eq!(status(&mut c, "Option B"), "merged", "Close branch is on by default");
    let h = c.exec("history", json!({"archived": true}));
    let merge = h["nodes"].as_array().unwrap().iter().find(|n| n["kind"] == json!("merge")).cloned().unwrap_or_else(|| panic!("{h}"));
    assert_eq!(merge["label"], json!("Merged \"Option B\""));
    assert!(merge["from"].is_object(), "{merge}");
    // Plain undo asks first; undomerge undoes, and the branch reopens with the save.
    let m = c.refused("execute", json!({"command": "u", "params": {}}));
    assert!(m.contains("undomerge") && m.contains("Undo the merge?"), "{m}");
    assert_eq!(description(&mut c, &coil).as_deref(), Some("Pump 1"), "the refused undo changed nothing");
    c.exec("undomerge", json!({}));
    assert_eq!(description(&mut c, &coil).as_deref(), Some("Pump"));
    assert_eq!(c.log().last().unwrap().author, Author::Agent(AGENT.into()));
    c.ok("save", json!({}));
    assert_eq!(status(&mut c, "Option B"), "open");
    let m = c.refused("execute", json!({"command": "undomerge", "params": {}}));
    assert!(m.contains("not a merge"), "{m}");
    // Redo merges again without asking; saved, the branch is closed again.
    c.exec("redo", json!({}));
    assert_eq!(description(&mut c, &coil).as_deref(), Some("Pump 1"));
    c.ok("save", json!({}));
    assert_eq!(status(&mut c, "Option B"), "merged");
}

#[test]
fn a_conflict_is_listed_refused_without_a_choice_and_merged_with_one() {
    let dir = TempDir::new("merge-conflict");
    let (mut c, coil, _) = project(&dir, "conflict.ecad");
    c.exec("branch", json!({"name": "Option C"}));
    c.exec("setprop", json!({"component": coil, "key": "description", "value": "Branch pump"}));
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
    c.exec("setprop", json!({"component": coil, "key": "description", "value": "Main pump"}));
    let pv = c.exec("merge", json!({"from": "Option C", "preview": true}));
    let conflicts = pv["conflicts"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1, "{pv}");
    let k = &conflicts[0];
    let id = k["id"].as_str().unwrap().to_string();
    assert_eq!(id, format!("{coil}/prop.description"));
    assert_eq!((k["main"].as_str().unwrap(), k["branch"].as_str().unwrap()), ("Main pump", "Branch pump"));
    assert_eq!(k["editable"], json!(true));
    assert_eq!(pv["unanswered"], json!([id]));
    // Without a choice: refused, naming it and how to choose; nothing recorded.
    let before = c.log().len();
    let m = c.refused("execute", json!({"command": "merge", "params": {"from": "Option C"}}));
    assert!(m.contains(&id) && m.contains("\"branch\""), "{m}");
    assert_eq!(c.log().len(), before);
    // A typed value is a choice too.
    let pv = c.exec("merge", json!({"from": "Option C", "preview": true, "choices": {id.clone(): {"value": "Pump 2"}}}));
    assert_eq!(pv["unanswered"], json!([]), "{pv}");
    let r = c.exec("merge", json!({"from": "Option C", "choices": {id: "branch"}}));
    assert_eq!(r["conflicts"], json!(1), "{r}");
    assert_eq!(description(&mut c, &coil).as_deref(), Some("Branch pump"));
    assert_eq!(c.log().len(), before + 1);
}

#[test]
fn update_from_main_is_a_merge_on_the_branch_and_the_next_merge_back_starts_from_it() {
    let dir = TempDir::new("merge-update");
    let (mut c, coil, _) = project(&dir, "update.ecad");
    c.exec("branch", json!({"name": "Option D"}));
    c.exec("wire", json!({"points": [[0, 120], [60, 120]]}));
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
    c.exec("setprop", json!({"component": coil, "key": "description", "value": "Main pump"}));
    c.ok("save", json!({}));
    c.exec("switchbranch", json!({"branch": "Option D"}));
    // From the branch, `from: "main"` updates the branch shown.
    let pv = c.exec("merge", json!({"from": "main", "preview": true}));
    assert_eq!(pv["update"], json!(true), "{pv}");
    assert!(pv["title"].as_str().unwrap().starts_with("UPDATE \"Option D\" FROM main"), "{pv}");
    let r = c.exec("merge", json!({"from": "main"}));
    assert_eq!(r["merged"], json!(true), "{r}");
    assert_eq!(current(&mut c), "Option D", "an update runs on the branch");
    let last = c.log().last().cloned().unwrap();
    assert!(last.label.starts_with("Update from main"), "{}", last.label);
    assert!(matches!(last.kind, TxKind::Merge { .. }));
    assert_eq!(description(&mut c, &coil).as_deref(), Some("Main pump"));
    c.ok("save", json!({}));
    assert_eq!(status(&mut c, "Option D"), "open", "an update never closes the branch");
    // The same description changed on main again, and on the branch: the merge back compares
    // against the update, so main's earlier change is not a conflict.
    c.exec("setprop", json!({"component": coil, "key": "description", "value": "Branch pump"}));
    c.ok("save", json!({}));
    let pv = c.exec("merge", json!({"from": "Option D", "preview": true}));
    assert_eq!(pv["conflicts"], json!([]), "the update is the ancestor: {pv}");
    assert!(pv["ancestor"].as_str().unwrap().starts_with("main, step"), "the update's main head (design 020 §6.1): {pv}");
    let r = c.exec("merge", json!({"from": "Option D"}));
    assert_eq!(r["merged"], json!(true));
    assert_eq!(description(&mut c, &coil).as_deref(), Some("Branch pump"));
    // Merging into anything but the parent is refused, saying why.
    c.exec("branch", json!({"name": "E", "switch": false}));
    c.exec("branch", json!({"name": "F", "switch": false}));
    let m = c.refused("execute", json!({"command": "merge", "params": {"from": "E", "into": "F"}}));
    assert!(m.contains("parent only"), "{m}");
}

#[test]
fn with_writes_off_the_merge_preview_reads_and_merge_is_refused() {
    let dir = TempDir::new("merge-readonly");
    let (mut c, coil, path) = project(&dir, "ro.ecad");
    c.exec("branch", json!({"name": "Option B"}));
    c.exec("setprop", json!({"component": coil, "key": "description", "value": "Pump 1"}));
    c.ok("save", json!({}));
    drop(c);
    let (s, _) = EcadMcp::open(&path, Options { write: false, agent: None }).unwrap();
    let mut c = Client::new(s);
    let before = c.log().len();
    let pv: Value = c.exec("merge", json!({"from": "Option B", "into": "main", "preview": true}));
    assert!(pv["changes"].as_array().is_some_and(|v| !v.is_empty()), "{pv}");
    for (cmd, params) in [("merge", json!({"from": "Option B", "into": "main"})), ("undomerge", json!({}))] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd}");
    }
    assert_eq!(c.log().len(), before, "nothing recorded");
}

fn exec_call(id: u64, command: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": "execute", "arguments": {"command": command, "params": params}}})
}

/// Reviewer 020b: a merge as the second transaction of a request line is taken back by
/// `undomerge` (a plain undo of a merge asks first); `close: false` undone says nothing about
/// reopening; an Edit value goes through the property's rules; Verify checks the merge rows.
#[test]
fn a_merge_over_the_cap_is_taken_back_close_false_undo_and_edit_rules() {
    let dir = TempDir::new("merge-review");
    let (mut c, coil, path) = project(&dir, "review.ecad");
    let _ = &path;
    c.exec("branch", json!({"name": "Option R"}));
    c.exec("setprop", json!({"component": coil, "key": "description", "value": "Pump R"}));
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
    let before = c.doc();
    let reply = c.s.handle_line(
        &Value::Array(vec![exec_call(1, "wire", json!({"points": [[0, 300], [60, 300]]})), exec_call(2, "merge", json!({"from": "Option R"}))])
            .to_string(),
    );
    let replies: Vec<Value> = serde_json::from_str::<Value>(&reply.unwrap()).unwrap().as_array().unwrap().clone();
    let m = replies[1]["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string();
    assert!(m.contains("taken back") && !m.contains("NOT as before"), "{m}");
    assert_eq!(c.doc().conductors.len(), before.conductors.len() + 1, "only the wire stays");
    assert_eq!(description(&mut c, &coil).as_deref(), Some("Pump"));
    // close:false: the branch stays open, and undoing says nothing about reopening it.
    c.exec("merge", json!({"from": "Option R", "close": false}));
    let q = c.refused("execute", json!({"command": "u", "params": {}}));
    assert!(!q.contains("reopened"), "{q}");
    let u = c.exec("undomerge", json!({}));
    assert!(!u["message"].as_str().unwrap().contains("open again"), "{u}");
    // An Edit value is checked as the property's own edit: a tag Edit is normalised.
    c.exec("setprop", json!({"component": coil, "key": "description", "value": "Main pump"}));
    let pv = c.exec("merge", json!({"from": "Option R", "preview": true}));
    let id = pv["conflicts"][0]["id"].as_str().unwrap().to_string();
    let pv = c.exec("merge", json!({"from": "Option R", "preview": true, "choices": {id.clone(): {"value": ""}}}));
    assert!(pv["problem"].as_str().is_some_and(|p| p.contains("cannot be empty")), "{pv}");
    let r = c.call("execute", json!({"command": "merge", "params": {"from": "Option R", "choices": {id.clone(): {"value": ""}}}}));
    assert_eq!(r["isError"], json!(true), "{r}");
    c.exec("merge", json!({"from": "Option R", "choices": {id: {"value": "Pump 9"}}}));
    assert_eq!(description(&mut c, &coil).as_deref(), Some("Pump 9"));
    c.ok("save", json!({}));
    drop(c);
    let v = ecad_file::v2::verify::verify(&ecad_file::v2::Store::open(std::path::Path::new(&path), false).unwrap());
    assert!(v.ok, "{:#?}", v.lines);
}
