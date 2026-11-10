//! Brief 020a, MCP contract (the ECAD rule of brief 011): every history and branch command
//! through `execute` records the same transaction (or branch row) as the click, by the agent,
//! and is refused with writes off; the read forms work with writes off and record nothing.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::{Author, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, TempDir, reader};

/// A project file created through MCP with writes on, one wire drawn and saved.
fn project(dir: &TempDir) -> Client {
    let path = dir.file("history.ecad");
    let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
    c.exec("wire", json!({"points": [[0, 0], [60, 0]]}));
    c.ok("save", json!({}));
    c
}

fn conductors(c: &mut Client) -> usize {
    c.doc().conductors.len()
}

#[test]
fn version_restore_and_branches_through_execute_record_the_agents_steps() {
    let dir = TempDir::new("history");
    let mut c = project(&dir);
    // A version: one step, by the agent, naming the head.
    let head = c.log().len() as u64;
    let v = c.exec("version", json!({"name": "Before change", "description": "one wire"}));
    assert_eq!(v["at"], json!(head));
    let last = c.log().last().cloned().unwrap();
    assert_eq!(last.label, "Create version \"Before change\"");
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    assert!(
        matches!(&last.ops[..], [Op::NameVersion { name, at, description: Some(d) }] if name == "Before change" && *at == head && d == "one wire")
    );
    let m = c.refused("execute", json!({"command": "version", "params": {"name": "before CHANGE"}}));
    assert!(m.contains("already a version"), "{m}");
    let m = c.refused("execute", json!({"command": "version", "params": {"name": "Rev B"}}));
    assert!(m.contains("names revisions"), "{m}");
    // Two more wires, then Restore: one step back to the version's state, undoable.
    c.exec("wire", json!({"points": [[0, 20], [60, 20]]}));
    c.exec("wire", json!({"points": [[0, 40], [60, 40]]}));
    assert_eq!(conductors(&mut c), 3);
    let info = c.exec("pointinfo", json!({"version": "Before change"}));
    assert_eq!(info["restore"], json!(true), "{info}");
    let r = c.exec("restore", json!({"version": "Before change"}));
    assert!(r["objects"].as_u64().unwrap() >= 2, "{r}");
    assert_eq!(conductors(&mut c), 1);
    let last = c.log().last().cloned().unwrap();
    assert_eq!(last.label, "Restore \"Before change\"");
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    assert!(c.doc().versions.contains_key("Before change"), "versions are not restored away");
    let m = c.refused("execute", json!({"command": "restore", "params": {"version": "Before change"}}));
    assert!(m.contains("already as it was"), "{m}");
    c.exec("u", json!({}));
    assert_eq!(conductors(&mut c), 3, "Undo takes the restore back");
    // A branch from the version: saved first, written at once, switched to.
    let b = c.exec("branch", json!({"name": "Option B", "from": {"version": "Before change"}}));
    assert_eq!(b["switched"], json!(true), "{b}");
    assert_eq!(c.ok("save", json!({}))["unsaved"], json!(false));
    assert_eq!(conductors(&mut c), 1, "the branch starts at the version's state");
    let list = c.exec("branches", json!({}));
    let rows = list["branches"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1]["name"], json!("Option B"));
    assert_eq!(rows[1]["current"], json!(true));
    let created_by = c.s.with_session(|s, p| p.read(s, |p, _| p.file.v2().unwrap().branch_record().created_by.clone()).unwrap()).unwrap();
    assert_eq!(created_by.as_deref(), Some(AGENT));
    // An edit on the branch; switching with it unsaved is refused, then saved and switched.
    c.exec("wire", json!({"points": [[0, 80], [60, 80]]}));
    let m = c.refused("execute", json!({"command": "switchbranch", "params": {"branch": "main"}}));
    assert!(m.contains("unsaved") && m.contains("\"save\": true"), "{m}");
    let sw = c.exec("switchbranch", json!({"branch": "main", "save": true}));
    assert_eq!(sw["switched"], json!(true));
    assert_eq!(conductors(&mut c), 3, "main is as it was");
    // Rename, archive and reopen.
    c.exec("renamebranch", json!({"branch": "Option B", "name": "Option C"}));
    c.exec("archivebranch", json!({"branch": "Option C"}));
    let h = c.exec("history", json!({}));
    assert_eq!(h["branches"].as_array().unwrap().len(), 1, "archived branches are hidden: {h}");
    let h = c.exec("history", json!({"archived": true}));
    assert_eq!(h["branches"].as_array().unwrap().len(), 2);
    c.exec("reopenbranch", json!({"branch": "Option C"}));
    let m = c.refused("execute", json!({"command": "archivebranch", "params": {"branch": "main"}}));
    assert!(m.contains("release line"), "{m}");
    // The graph names the version and both heads; steps list the newest first.
    let h = c.exec("history", json!({}));
    let kinds: Vec<&str> = h["nodes"].as_array().unwrap().iter().map(|n| n["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"version") && kinds.iter().filter(|k| **k == "now").count() == 2, "{h}");
    let steps = c.exec("steps", json!({"limit": 2}));
    let s = steps["steps"].as_array().unwrap();
    assert_eq!(s.len(), 2);
    assert!(s[0]["seq"].as_u64() > s[1]["seq"].as_u64());
    // Revisions are issued on main only.
    c.exec("switchbranch", json!({"branch": "Option C", "save": true}));
    let m = c.refused("execute", json!({"command": "issuerevision", "params": {"rev": "A", "description": "x", "drawn": "CV"}}));
    assert!(m.contains("issued on main"), "{m}");
}

#[test]
fn with_writes_off_the_history_reads_and_every_branch_write_is_refused() {
    let mut c = reader();
    for (cmd, params) in [("history", json!({})), ("branches", json!({})), ("steps", json!({})), ("closepoint", json!({}))] {
        let v: Value = c.exec(cmd, params);
        assert!(v.is_object(), "{cmd}: {v}");
    }
    let before = c.log().len();
    for (cmd, params) in [
        ("version", json!({"name": "V"})),
        ("restore", json!({"seq": 0})),
        ("branch", json!({"name": "B"})),
        ("renamebranch", json!({"branch": "main", "name": "x"})),
        ("switchbranch", json!({"branch": "main"})),
        ("archivebranch", json!({"branch": "main"})),
        ("reopenbranch", json!({"branch": "main"})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd}");
    }
    assert_eq!(c.log().len(), before, "nothing recorded");
}
