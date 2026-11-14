//! Brief 048 (symbol builder) through MCP (brief 011 rule, design 048 §12.1): `symbolnew`,
//! `symboledit`, `symbolupdate` and `makesymbol` each record one transaction by
//! `Author::Agent(<client>)` with the expected ops, and are refused with writes off with the head
//! unchanged; `symbolimpact` reads only and is allowed with writes off.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::{Author, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client};

fn writer() -> Client {
    Client::new(EcadMcp::in_memory("mcp 048", 48, Options { write: true, agent: None }).unwrap())
}

fn one_by_agent(c: &mut Client, before: usize) -> Vec<Op> {
    let log = c.log();
    assert_eq!(log.len(), before + 1, "one transaction");
    let tx = log.last().unwrap();
    assert_eq!(tx.author, Author::Agent(AGENT.into()));
    tx.ops.clone()
}

fn float(extra: Value) -> Value {
    let mut p = json!({
        "name": "Float switch", "key": "float1",
        "graphics": [{"kind": "polyline", "vertices": [[-2.5, -7.5], [2.5, -7.5], [2.5, 7.5], [-2.5, 7.5]], "closed": true}],
        "points": [{"key": "1", "label": "BN", "at": [0, 5]}, {"key": "2", "label": "BU", "at": [0, -5]}],
        "slots": [{"text": "tag", "at": [-4, 0], "h": "right", "v": "middle"}],
        "defaults": {"prefix": "B", "description": "Float switch"},
    });
    if let (Some(o), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    p
}

#[test]
fn symbol_commands_by_the_agent() {
    let mut c = writer();
    let n = c.log().len();
    let v = c.exec("symbolnew", float(json!({})));
    let ops = one_by_agent(&mut c, n);
    assert!(matches!(ops.as_slice(), [Op::EmbedDefinition { .. }]), "{ops:?}");
    let def = v["definition"].as_str().unwrap().to_string();

    let p1 = c.exec("place", json!({"symbol": "float1", "at": [100, 100]}));
    c.exec("place", json!({"symbol": "float1", "at": [150, 100]}));
    assert_eq!(p1["tag"], "-B01");

    // The impact reads only.
    let n = c.log().len();
    let moved = json!([{"key": "1", "label": "BN", "at": [0, 10]}, {"key": "2", "label": "BU", "at": [0, -5]}]);
    let i = c.exec("symbolimpact", float(json!({"definition": def, "points": moved})));
    assert_eq!(i["copies"], 2);
    assert_eq!(c.log().len(), n, "symbolimpact records nothing");

    // An edit updating one copy: the new version, the copy's item, its component's definition.
    let rep = p1["representation"].as_str().unwrap();
    let v2 = c.exec("symboledit", float(json!({"definition": def, "points": moved, "update": "selected", "copies": [rep]})));
    let ops = one_by_agent(&mut c, n);
    assert_eq!(v2["version"], 2);
    assert_eq!(ops.iter().filter(|o| matches!(o, Op::EmbedDefinition { .. })).count(), 1);
    assert_eq!(ops.iter().filter(|o| matches!(o, Op::SetComponentDefinition { .. })).count(), 1);
    assert_eq!(ops.iter().filter(|o| matches!(o, Op::ModifyItem { .. })).count(), 1);

    // The other copy is brought up to date: v1 is then unused and removed (FP8).
    let n = c.log().len();
    let u = c.exec("symbolupdate", json!({"lineage": v2["lineage"]}));
    let ops = one_by_agent(&mut c, n);
    assert_eq!(u["updated"], 1);
    assert!(ops.iter().any(|o| matches!(o, Op::RemoveDefinition { .. })), "{ops:?}");

    // Make symbol from two lines, replacing them.
    let before = line_handles(&mut c);
    for (a, b) in [([200, 200], [210, 200]), ([210, 200], [210, 210])] {
        c.exec("line", json!({"points": [a, b]}));
    }
    let handles: Vec<String> = line_handles(&mut c).into_iter().filter(|h| !before.contains(h)).collect();
    assert_eq!(handles.len(), 2);
    let n = c.log().len();
    let m = c.exec(
        "makesymbol",
        json!({"handles": handles, "name": "Corner", "key": "corner", "defaults": {"prefix": "A"},
            "points": [{"key": "1", "at": [200, 200]}], "slots": [{"text": "tag", "at": [195, 205]}]}),
    );
    let ops = one_by_agent(&mut c, n);
    assert_eq!(m["tag"], "-A01");
    assert_eq!(ops.iter().filter(|o| matches!(o, Op::RemoveItem { .. })).count(), 2);
    assert_eq!(ops.iter().filter(|o| matches!(o, Op::CreateComponent { .. })).count(), 1);
}

fn line_handles(c: &mut Client) -> Vec<String> {
    let r = c.exec("entities", json!({"type": "line"}));
    r["entities"].as_array().into_iter().flatten().filter_map(|e| e["handle"].as_str().map(str::to_string)).collect()
}

#[test]
fn writes_off_refuses_symbol_writes_and_allows_the_impact() {
    let mut r = crate::mcp_contracts::reader();
    let log = r.log();
    for (cmd, params) in [
        ("symbolnew", float(json!({}))),
        ("symboledit", float(json!({"definition": "00", "update": "all"}))),
        ("symbolupdate", json!({"lineage": "00"})),
        ("makesymbol", json!({"handles": [], "name": "x", "key": "x"})),
    ] {
        let m = r.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd}");
    }
    // `symbolimpact` passes the gate (and is refused by the engine only for its unknown symbol).
    assert!(policy::check("symbolimpact", &json!({}), false, None).is_ok());
    assert_eq!(r.log(), log, "nothing recorded with writes off");
}
