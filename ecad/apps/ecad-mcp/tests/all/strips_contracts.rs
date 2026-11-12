//! Brief 028, MCP contract (the ECAD rule of brief 011, design 028 §10): `place` of a terminal
//! on a wire, `terminals`, `bridge`, `setprop` with `terminals` and `cores`, and `newpage` of a
//! terminal plan through `execute` record the agent's transaction and are refused with writes
//! off; `stripplan`, `cablecores`, `cablelist` and `checks` work with writes off and record
//! nothing.

use ecad_mcp::policy;
use ecad_model::{Author, ObjectRef, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, reader};

fn writer() -> Client {
    Client::new(ecad_mcp::EcadMcp::in_memory("mcp", 28, ecad_mcp::Options { write: true, agent: None }).unwrap())
}

fn hex(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap().to_string()
}

fn last_by_agent(c: &mut Client) -> (String, Vec<Op>) {
    let last = c.log().last().cloned().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    (last.label, last.ops)
}

#[test]
fn terminal_strip_commands_through_execute() {
    let mut c = writer();
    let mut wires = Vec::new();
    for x in [100, 120, 140] {
        let w = c.exec("wire", json!({"points": [[x, 200], [x, 100]]}));
        wires.push(hex(&w, "conductor"));
    }
    let slash = c.exec("cableslash", json!({"from": [90, 120], "to": [150, 120], "tag": "-W01"}));
    let cable = hex(&slash, "cable");
    // A terminal on a wire: split in one transaction, by the agent.
    let t = c.exec("place", json!({"symbol": "terminal", "at": [100, 170]}));
    assert!(t["split"].is_object(), "{t}");
    let (label, ops) = last_by_agent(&mut c);
    assert_eq!(label, "Place terminal");
    assert!(ops.iter().any(|o| matches!(o, Op::CreateConductor { .. })));
    let strip = hex(&t, "component");
    // Place Terminals on the other two.
    let f = c.exec("terminals", json!({"strip": strip, "from": [110, 170], "to": [150, 170]}));
    assert_eq!(f["message"], "Placed -X01:2\u{2026}3 on 2 wires.", "{f}");
    assert_eq!(last_by_agent(&mut c).0, "Place terminals");
    let terms: Vec<String> = f["terminals"].as_array().unwrap().iter().map(|x| hex(x, "terminal")).collect();
    // A bridge.
    let b = c.exec("bridge", json!({"from": terms[0], "to": terms[1]}));
    assert_eq!(b["bridge"], true);
    assert_eq!(last_by_agent(&mut c).0, "Bridge -X01:2\u{2013}3");
    // Terminal numbering through the planner (026's fourth kind): preview reads, apply records.
    let n = c.log().len();
    let pv = c.exec("renumber", json!({"kind": "terminals", "preview": true}));
    assert_eq!(pv["rows"].as_array().unwrap().len(), 3, "{pv}");
    assert_eq!(c.log().len(), n, "a preview records nothing");
    c.exec("renumber", json!({"kind": "terminals", "start": 5}));
    let (label, ops) = last_by_agent(&mut c);
    assert_eq!(label, "Renumber terminals");
    assert!(ops.iter().any(|o| matches!(o, Op::AddTerminal { terminal, .. } if terminal.label.as_deref() == Some("5"))));
    // The offers.
    c.exec("setprop", json!({"component": strip, "key": "terminals", "value": "8+1PE"}));
    assert!(
        matches!(&last_by_agent(&mut c).1[..], [Op::SetProperty { object: ObjectRef::Component(_), key, after: Some(v), .. }] if key == "terminals" && v == "8+1PE")
    );
    c.exec("setprop", json!({"cable": cable, "key": "cores", "value": "5G"}));
    assert!(
        matches!(&last_by_agent(&mut c).1[..], [Op::SetProperty { object: ObjectRef::Cable(_), key, after: Some(v), .. }] if key == "cores" && v == "5G")
    );
    // A terminal plan page.
    let page = c.exec("newpage", json!({"generated": "terminals", "strip": strip}));
    assert!(page["page"].is_string(), "{page}");
    assert_eq!(last_by_agent(&mut c).0, "New page");
    // The reads record nothing.
    let n = c.log().len();
    let plan = c.exec("stripplan", json!({}));
    assert_eq!(plan["strips"][0]["offered"], "8+1PE", "{plan}");
    assert_eq!(plan["strips"][0]["rows"].as_array().unwrap().len(), 8);
    let cores = c.exec("cablecores", json!({"cable": cable}));
    assert_eq!(cores["used"], 3, "{cores}");
    assert_eq!(cores["spare"], json!(["4", "GNYE"]));
    let list = c.exec("cablelist", json!({}));
    assert_eq!(list["rows"][0]["spare_text"], "2 (4, GNYE)", "{list}");
    let checks = c.exec("checks", json!({"kinds": ["too-many-terminals", "too-many-cores", "core-twice", "no-core-count", "terminal-twice"]}));
    assert_eq!(checks["checks"], json!([]), "{checks}");
    assert_eq!(c.log().len(), n, "reads record nothing");
}

#[test]
fn with_writes_off_the_strip_writes_are_refused_and_the_reads_work() {
    let mut c = reader();
    let before = c.log();
    for (cmd, params) in [
        ("place", json!({"symbol": "terminal", "at": [0, 0]})),
        ("terminals", json!({"from": [0, 0], "to": [10, 0]})),
        ("bridge", json!({"from": "1", "to": "2"})),
        ("setprop", json!({"component": "1", "key": "terminals", "value": "12"})),
        ("setprop", json!({"cable": "1", "key": "cores", "value": "7G"})),
        ("newpage", json!({"generated": "terminals"})),
        ("renumber", json!({"kind": "terminals"})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd}");
    }
    let plan = c.exec("stripplan", json!({}));
    assert_eq!(plan["strips"], json!([]));
    let m = c.refused("execute", json!({"command": "cablecores", "params": {"cable": "1"}}));
    assert!(m.contains("no such cable"), "{m}");
    c.exec("cablelist", json!({}));
    c.exec("checks", json!({}));
    assert_eq!(c.log(), before, "nothing recorded");
}
