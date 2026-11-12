//! Brief 032, MCP contract (the ECAD rule of brief 011, design 032 §12): `wire` merging and
//! folding, `joinwires`, `cableslash` continuing a cable, `cablecontinue`'s refusal and
//! `wiresettings` through `execute` record the agent's transaction (or none), and are refused with
//! writes off.

use ecad_mcp::policy;
use ecad_model::{Author, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, reader};

fn writer() -> Client {
    Client::new(ecad_mcp::EcadMcp::in_memory("mcp", 32, ecad_mcp::Options { write: true, agent: None }).unwrap())
}

fn last(c: &mut Client) -> ecad_model::Transaction {
    let t = c.log().last().cloned().unwrap();
    assert_eq!(t.author, Author::Agent(AGENT.into()));
    t
}

fn hex(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap().to_string()
}

/// FP-W3: `wire` that merges records one `Wire` transaction with no `CreateConductor`, and says
/// `created: false` with the existing wire; a wire between two open ends removes the folded wire.
#[test]
fn wire_merging_through_execute() {
    let mut c = writer();
    let first = c.exec("wire", json!({"points": [[0, 0], [50, 0]]}));
    let r = c.exec("wire", json!({"points": [[50, 0], [50, -20]]}));
    assert_eq!(r["created"], json!(false), "{r}");
    assert_eq!(r["conductor"], first["conductor"]);
    let t = last(&mut c);
    assert_eq!(t.label, "Wire");
    assert!(!t.ops.iter().any(|o| matches!(o, Op::CreateConductor { .. })), "{:?}", t.ops);
    // Both ends: the folded wire is reported and no longer exists.
    let other = c.exec("wire", json!({"points": [[100, -20], [140, -20]]}));
    let r = c.exec("wire", json!({"points": [[50, -20], [100, -20]]}));
    assert_eq!(r["removed"], json!([hex(&other, "conductor")]), "{r}");
    let t = last(&mut c);
    assert!(t.ops.iter().any(|o| matches!(o, Op::DeleteConductor { id, .. } if id.hex() == hex(&other, "conductor"))));
}

/// `joinwires`: one `Join wires` transaction by the agent.
#[test]
fn joinwires_through_execute() {
    let mut c = writer();
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

/// Design 032 §3.1 through `execute`: `-W04` slashed on the first page, its cores ending on
/// continuations; the second page's wires continue from them. Returns the cable.
fn pack(c: &mut Client) -> String {
    for (i, y) in [0, 10, 20].into_iter().enumerate() {
        c.exec("place", json!({"symbol": "terminal", "at": [0, y], "tag": "X01"}));
        c.exec("place", json!({"symbol": "continuation", "at": [100, y], "name": format!("A{}", i + 1)}));
        c.exec("wire", json!({"points": [[0, y], [100, y]]}));
    }
    let k = hex(&c.exec("cableslash", json!({"from": [50, -5], "to": [50, 25], "tag": "-W04"})), "cable");
    let page = hex(&c.exec("newpage", json!({"title": "115"})), "page");
    c.exec("openpage", json!({"page": page}));
    for (i, y) in [0, 10, 20].into_iter().enumerate() {
        c.exec("place", json!({"symbol": "continuation", "at": [0, y], "name": format!("A{}", i + 1)}));
        c.exec("place", json!({"symbol": "terminal", "at": [100, y], "tag": "X05"}));
        c.exec("wire", json!({"points": [[0, y], [100, y]]}));
    }
    k
}

/// `cableslash` continuing a cable: one `Cable slash` transaction with `AddCableMember` and
/// `LinkRepresentation`, no `CreateCable`; `cablecontinue` on it before is refused with §3.4's
/// words and records nothing.
#[test]
fn cableslash_continuing_and_cablecontinue_through_execute() {
    let mut c = writer();
    let k = pack(&mut c);
    let n = c.log().len();
    let m = c.refused("execute", json!({"command": "cablecontinue", "params": {"cable": k, "at": [20, 80], "toward": [40, 80]}}));
    assert!(m.contains("-W04's cores end on continuations."), "{m}");
    assert_eq!(c.log().len(), n, "a refusal records nothing");
    let r = c.exec("cableslash", json!({"from": [50, -5], "to": [50, 25], "tag": "-W04"}));
    assert_eq!(r["continued"], json!(true), "{r}");
    let t = last(&mut c);
    assert_eq!(t.label, "Cable slash");
    assert_eq!(t.ops.iter().filter(|o| matches!(o, Op::AddCableMember { .. })).count(), 3);
    assert!(t.ops.iter().any(|o| matches!(o, Op::LinkRepresentation { .. })));
    assert!(!t.ops.iter().any(|o| matches!(o, Op::CreateCable { .. })));
}
/// `wiresettings {label_rule: "each", label_min_length: 10}`: one `SetProjectSettings`
/// transaction by the agent.
#[test]
fn wiresettings_label_rule_through_execute() {
    let mut c = writer();
    let r = c.exec("wiresettings", json!({"label_rule": "each", "label_min_length": 10}));
    assert_eq!(r["label_rule"], json!("each"), "{r}");
    assert_eq!(r["label_min_length"], json!(10.0));
    let t = last(&mut c);
    assert!(matches!(&t.ops[..], [Op::SetProjectSettings { .. }]), "{:?}", t.ops);
}
