//! Brief 050a through MCP (brief 011 rule, design 050 §6.8): `standard` (`new`, `name`,
//! `remove`), `style`, `deletestyle`, `promote` and `setdefault` at `standard` each record one
//! transaction by `Author::Agent(<client>)`, are refused with writes off, and one undo restores
//! the document; `styles` and `standard {}` read with writes off. `standard {from}` names another
//! project file, which agents may not read (the server's file rule): refused in words.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::Author;
use serde_json::json;

use crate::mcp_contracts::{AGENT, Client};

fn writer() -> Client {
    Client::new(EcadMcp::in_memory("mcp 050", 50, Options { write: true, agent: None }).unwrap())
}

/// Run `command` as the agent: one transaction labelled `label`, and one undo restores the
/// document as it was.
fn one_undoable(c: &mut Client, command: &str, params: serde_json::Value, label: &str) -> serde_json::Value {
    let before = c.doc();
    let n = c.log().len();
    let v = c.exec(command, params.clone());
    let log = c.log();
    assert_eq!(log.len(), n + 1, "{command}: one transaction");
    let t = log.last().unwrap();
    assert_eq!(t.author, Author::Agent(AGENT.into()), "{command}");
    assert_eq!(t.label, label, "{command}");
    let after = c.doc();
    c.exec("undo", json!({}));
    assert_eq!(c.doc(), before, "{command}: one undo restores the document");
    c.exec("redo", json!({}));
    assert_eq!(c.doc(), after, "{command}: redo");
    v
}

#[test]
fn the_style_commands_record_one_transaction_by_the_agent() {
    let mut c = writer();
    let k = c.exec("place", json!({"symbol": "coil", "at": [100, 100]}))["component"].as_str().unwrap().to_string();
    one_undoable(&mut c, "standard", json!({"new": "House"}), "Make standard House");
    one_undoable(&mut c, "standard", json!({"name": "House 2026"}), "Rename standard House 2026");
    one_undoable(
        &mut c,
        "setdefault",
        json!({"key": "symbol.coil.prefix", "level": "standard", "value": "Q"}),
        "Default Tag prefix: Contactor / relay coil",
    );
    one_undoable(&mut c, "setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"colour": "RD"}}), "Text style Tag");
    let v = one_undoable(
        &mut c,
        "setstyle",
        json!({"catalogue": "text", "new": true, "name": "Tag large", "fields": {"height": 5}}),
        "New text style Tag large",
    );
    let large = v["id"].as_str().unwrap().to_string();
    c.exec("setdefault", json!({"key": "text.tag.style", "level": "object", "component": k, "value": large}));
    one_undoable(&mut c, "promote", json!({"catalogue": "text", "id": large}), "Promote Tag large");
    one_undoable(&mut c, "deletestyle", json!({"catalogue": "text", "id": large, "replace_with": "tag"}), "Delete text style Tag large");
    one_undoable(&mut c, "standard", json!({"remove": true, "replace_with": {"text": "tag"}}), "Back to IEC from House 2026");
    // The reads record nothing.
    let n = c.log().len();
    assert_eq!(c.exec("standard", json!({}))["name"], json!("IEC (built in)"));
    assert!(c.exec("styles", json!({"catalogue": "text", "users": true}))["styles"].as_array().is_some_and(|a| a.len() == 20));
    assert_eq!(c.log().len(), n);
    // Another project file is not the agent's to read.
    let m = c.refused("execute", json!({"command": "standard", "params": {"from": "other.ecad"}}));
    assert!(m.contains("`from` names a file"), "{m}");
    assert_eq!(c.log().len(), n);
}

#[test]
fn writes_are_refused_with_writes_off_and_the_reads_still_answer() {
    let mut r = crate::mcp_contracts::reader();
    let log = r.log();
    for (command, params) in [
        ("standard", json!({"new": "House"})),
        ("setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"colour": "RD"}})),
        ("deletestyle", json!({"catalogue": "text", "id": "#0123456789abcdef0123456789abcdef"})),
        ("promote", json!({"catalogue": "text", "id": "tag"})),
        ("setdefault", json!({"key": "symbol.coil.prefix", "level": "standard", "value": "Q"})),
    ] {
        let m = r.refused("execute", json!({"command": command, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{command}");
    }
    assert_eq!(r.exec("standard", json!({}))["name"], json!("IEC (built in)"));
    assert_eq!(r.exec("styles", json!({"catalogue": "frame"}))["styles"][0]["id"], json!("sheet"));
    assert_eq!(r.log(), log, "nothing recorded with writes off");
}

/// Brief 050b (design 050 §6.8): `category` and `wire {category}` by the agent.
#[test]
fn category_and_wire_with_a_category_by_the_agent() {
    let mut c = writer();
    let w = c.exec("wire", json!({"points": [[10, 150], [60, 150]], "potential": "24V"}))["conductor"].as_str().unwrap().to_string();
    one_undoable(&mut c, "category", json!({"conductor": w, "value": "signal"}), "Category Signal");
    let n = c.log().len();
    let v = c.exec("wire", json!({"points": [[10, 170], [60, 170]], "category": "24 V DC"}));
    assert_eq!(c.log().len(), n + 1, "one transaction");
    assert_eq!(c.log().last().unwrap().author, Author::Agent(AGENT.into()));
    let id = v["conductor"].as_str().unwrap();
    let d = c.doc();
    let cond = d.conductors.iter().find(|(k, _)| k.hex() == id).unwrap().1;
    assert_eq!(cond.properties.get("wire.category").map(String::as_str), Some("dc24"));
    let mut r = crate::mcp_contracts::reader();
    let m = r.refused("execute", json!({"command": "category", "params": {"conductor": w, "value": "signal"}}));
    assert_eq!(m, policy::WRITES_OFF);
}
