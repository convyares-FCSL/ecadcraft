//! MCP contract: `describe_command` for the wire commands, `wire` merging and
//! folding, `joinwires`, `cableslash` continuing a cable, `cablecontinue`'s refusal and
//! `wiresettings` through `execute` record the agent's transaction (or none), and are refused with
//! writes off.

use ecad_engine::agent::policy;
use ecad_model::{Author, Op};
use serde_json::{Value, json};

use crate::common::writer;
use crate::mcp_contracts::{AGENT, Client, reader};

const WRITER_SEED: u64 = 32;

fn last(c: &mut Client) -> ecad_model::Transaction {
    let t = c.log().last().cloned().unwrap();
    assert_eq!(t.author, Author::Agent(AGENT.into()));
    t
}

fn hex(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap().to_string()
}

/// FP-W3 (design 072: routes draw, wires connect): `wire` that merges records one `Wire`
/// transaction that grows the existing line (no new route), and says `created: false` with that
/// line; a line between two open ends removes the folded line.
#[test]
fn wire_merging_through_execute() {
    let mut c = writer(WRITER_SEED);
    let first = c.exec("wire", json!({"points": [[0, 0], [50, 0]]}));
    let r = c.exec("wire", json!({"points": [[50, 0], [50, -20]]}));
    assert_eq!(r["created"], json!(false), "{r}");
    assert_eq!(r["route"], first["route"]);
    assert_eq!(r["conductor"], Value::Null, "its ends are free: no wire yet");
    let t = last(&mut c);
    assert_eq!(t.label, "Wire");
    assert!(!t.ops.iter().any(|o| matches!(o, Op::AddRoute { .. })), "{:?}", t.ops);
    // Both ends: the folded line is reported and no longer exists.
    let other = c.exec("wire", json!({"points": [[100, -20], [140, -20]]}));
    let r = c.exec("wire", json!({"points": [[50, -20], [100, -20]]}));
    assert_eq!(r["removed"], json!([hex(&other, "route")]), "{r}");
    let t = last(&mut c);
    assert!(t.ops.iter().any(|o| matches!(o, Op::RemoveRoute { id, .. } if id.hex() == hex(&other, "route"))));
}

/// `joinwires`: one `Join wires` transaction by the agent.
#[test]
fn joinwires_through_execute() {
    let mut c = writer(WRITER_SEED);
    c.exec("wire", json!({"points": [[0, 0], [50, 0]]}));
    c.exec("wire", json!({"points": [[50, 0], [50, -20]], "join": false}));
    let n = c.log().len();
    let r = c.exec("joinwires", json!({"at": [50, 0]}));
    assert_eq!(r["how"], json!("merged"), "{r}");
    assert_eq!(c.log().len(), n + 1);
    assert_eq!(last(&mut c).label, "Join wires");
}

#[test]
fn with_writes_off_the_wire_commands_are_refused() {
    let mut c = reader();
    let before = c.log();
    for (cmd, params) in [("wire", json!({"points": [[0, 0], [5, 0]], "join": false})), ("joinwires", json!({"at": [0, 0]}))] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd}");
    }
    assert_eq!(c.log(), before, "nothing recorded");
}

/// Design 032 §3.1 through `execute`, as design 072 §4.4 (F5) has it: `-W04` slashed on the first
/// page over lines to continuations; the second page's lines from their partners make each core
/// one wire across the pages. Returns the cable.
fn pack(c: &mut Client) -> String {
    for (i, y) in [0, 10, 20].into_iter().enumerate() {
        c.exec("place", json!({"symbol": "terminal_block", "at": [0, y], "tag": "X01"}));
        c.exec("place", json!({"symbol": "continuation", "at": [100, y], "name": format!("A{}", i + 1)}));
        c.exec("wire", json!({"points": [[0, y], [100, y]]}));
    }
    let k = hex(&c.exec("cableslash", json!({"from": [50, -5], "to": [50, 25], "tag": "-W04"})), "cable");
    let page = hex(&c.exec("newpage", json!({"title": "115"})), "page");
    c.exec("openpage", json!({"page": page}));
    for (i, y) in [0, 10, 20].into_iter().enumerate() {
        c.exec("place", json!({"symbol": "continuation", "at": [0, y], "name": format!("A{}", i + 1)}));
        c.exec("place", json!({"symbol": "terminal_block", "at": [100, y], "tag": "X05"}));
        c.exec("wire", json!({"points": [[0, y], [100, y]]}));
    }
    k
}

/// `cableslash` with an existing cable's tag (F5): one `Cable slash` transaction with a
/// `LinkRepresentation`, no `CreateCable` and no `ModifyCable` (each core is already one wire
/// through the continuations); `cablecontinue` on such a cable is refused and records nothing.
#[test]
fn cableslash_continuing_and_cablecontinue_through_execute() {
    let mut c = writer(WRITER_SEED);
    let k = pack(&mut c);
    let n = c.log().len();
    let m = c.refused("execute", json!({"command": "cablecontinue", "params": {"cable": k, "at": [20, 80], "toward": [40, 80]}}));
    assert!(m.contains("-W04"), "{m}");
    assert_eq!(c.log().len(), n, "a refusal records nothing");
    let r = c.exec("cableslash", json!({"from": [50, -5], "to": [50, 25], "tag": "-W04"}));
    assert_eq!(r["continued"], json!(true), "{r}");
    assert_eq!(r["cores"].as_array().map_or(0, Vec::len), 3, "{r}");
    let t = last(&mut c);
    assert_eq!(t.label, "Cable slash");
    assert!(!t.ops.iter().any(|o| matches!(o, Op::ModifyCable { .. })), "no new member: {:?}", t.ops);
    assert!(t.ops.iter().any(|o| matches!(o, Op::LinkRepresentation { .. })));
    assert!(!t.ops.iter().any(|o| matches!(o, Op::CreateCable { .. })));
}
/// `wiresettings {label_rule: "each", label_min_length: 10}`: one `SetProjectSettings`
/// transaction by the agent.
#[test]
fn wiresettings_label_rule_through_execute() {
    let mut c = writer(WRITER_SEED);
    let r = c.exec("wiresettings", json!({"label_rule": "each", "label_min_length": 10}));
    assert_eq!(r["label_rule"], json!("each"), "{r}");
    assert_eq!(r["label_min_length"], json!(10.0));
    let t = last(&mut c);
    assert!(matches!(&t.ops[..], [Op::SetProjectSettings { .. }]), "{:?}", t.ops);
}

/// `describe_command` (writes off): each wire command's parameters text, params schema and reply schema; a command
/// that is not offered is refused.
#[test]
fn describe_command_shows_the_wire_commands_params_and_reply() {
    let mut c = reader();
    for id in [
        "wire",
        "joinwires",
        "nets",
        "net",
        "wirenumber",
        "potential",
        "category",
        "wiresettings",
        "renumberwires",
        "checks",
        "connect",
        "disconnect",
    ] {
        let d = c.ok("describe_command", json!({"id": id}));
        assert_eq!(d["id"], json!(id));
        assert!(d["params"].is_object() && d["reply"].is_object(), "{id}: {d}");
        assert!(!d["help"].as_str().unwrap_or_default().is_empty(), "{id}: {d}");
    }
    assert_eq!(c.ok("describe_command", json!({"id": "net"}))["agent"], json!("read"));
    assert_eq!(c.ok("describe_command", json!({"id": "wire"}))["agent"], json!("write"));
    assert!(c.refused("describe_command", json!({"id": "lisp"})).contains("lisp"));
}

/// The wire commands' replies through `execute` read as their types: what an agent gets is what the type says.
#[test]
fn the_wire_commands_replies_read_as_their_types() {
    use ecad_electrical::wires::commands::nets::{NetsReply, RenumberwiresReply, WiresettingsReply};
    use ecad_engine::wires::commands::{JoinwiresReply, WireReply};
    let mut c = writer(WRITER_SEED);
    let r: WireReply = serde_json::from_value(c.exec("wire", json!({"points": [[0, 0], [50, 0]]}))).unwrap();
    assert!(r.created);
    c.exec("wire", json!({"points": [[50, 0], [50, -20]], "join": false}));
    let j: JoinwiresReply = serde_json::from_value(c.exec("joinwires", json!({"at": [50, 0]}))).unwrap();
    assert!(matches!(j, JoinwiresReply::Merged { .. }), "{j:?}");
    let _: WiresettingsReply = serde_json::from_value(c.exec("wiresettings", json!({}))).unwrap();
    let _: RenumberwiresReply = serde_json::from_value(c.exec("renumberwires", json!({}))).unwrap();
    let _: NetsReply = serde_json::from_value(c.exec("nets", json!({"all": true}))).unwrap();
}
