//! Brief 030, MCP contract (the ECAD rule of brief 011, design 030 §10): `accessory`, `jumper`
//! (make, extend, shorten, split, delete), `setprop` of `acc.*`, `link.<n>`, `jumper.bar` and
//! accessory part keys, `projectprops` with `strip_*` and `renumber {kind: "terminals"}` with
//! accessory keys and links, through `execute`, record the agent's transaction and are refused
//! with writes off; `stripplan`, `bom`, `checks` and the renumber preview work with writes off and
//! record nothing.

use ecad_mcp::policy;
use ecad_model::{Author, ObjectRef, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, reader};

fn writer() -> Client {
    Client::new(ecad_mcp::EcadMcp::in_memory("mcp", 30, ecad_mcp::Options { write: true, agent: None }).unwrap())
}

fn last_by_agent(c: &mut Client) -> (String, Vec<Op>) {
    let last = c.log().last().cloned().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    (last.label, last.ops)
}

fn sets(ops: &[Op], key: &str) -> bool {
    ops.iter().any(|o| matches!(o, Op::SetProperty { object: ObjectRef::Component(_), key: k, .. } if k == key))
}

#[test]
fn accessory_and_jumper_commands_through_execute() {
    let mut c = writer();
    for x in [100, 120, 140, 160] {
        c.exec("wire", json!({"points": [[x, 200], [x, 100]]}));
    }
    let f = c.exec("terminals", json!({"from": [90, 150], "to": [170, 150]}));
    let strip = f["strip"].as_str().unwrap().to_string();
    let terms: Vec<String> = f["terminals"].as_array().unwrap().iter().map(|x| x["terminal"].as_str().unwrap().to_string()).collect();
    let one = |c: &mut Client, cmd: &str, params: Value, key: &str| {
        let n = c.log().len();
        c.exec(cmd, params.clone());
        assert_eq!(c.log().len(), n + 1, "{cmd} {params}: one transaction");
        let (_, ops) = last_by_agent(c);
        assert!(sets(&ops, key), "{cmd} {params} sets {key}: {ops:?}");
    };
    one(&mut c, "accessory", json!({"strip": strip, "at": "start", "items": ["end_stop"]}), "acc.start");
    one(&mut c, "accessory", json!({"strip": strip, "at": "2", "add": ["partition"]}), "acc.after.2");
    one(&mut c, "accessory", json!({"strip": strip, "at": "2", "remove": ["partition"]}), "acc.after.2");
    one(&mut c, "accessory", json!({"strip": strip, "at": ["1", "2", "3"], "on": true, "items": ["marker"]}), "acc.on.3");
    one(&mut c, "accessory", json!({"strip": strip, "at": "3", "on": true, "items": null}), "acc.on.3");
    one(&mut c, "setprop", json!({"component": strip, "key": "acc.after.4", "value": "partition"}), "acc.after.4");
    one(&mut c, "setprop", json!({"component": strip, "key": "acc.on.4", "value": "marker"}), "acc.on.4");
    one(&mut c, "setprop", json!({"component": strip, "key": "partition.part_number", "value": "TP-1"}), "partition.part_number");
    let n = c.log().len();
    c.exec("projectprops", json!({"props": {"strip_end_stop_part_number": "ES-1", "strip_jumper_bar": "4"}}));
    assert_eq!(c.log().len(), n + 1);
    let (_, ops) = last_by_agent(&mut c);
    assert!(ops.iter().any(|o| matches!(o, Op::SetProperty { object: ObjectRef::Project, key, .. } if key == "strip_end_stop_part_number")));
    // Jumpers: from terminals (a range), from positions with skip, then edits.
    one(&mut c, "jumper", json!({"terminals": [terms[0], terms[2]]}), "link.1");
    let (_, ops) = last_by_agent(&mut c);
    assert!(
        ops.iter().any(|o| matches!(o, Op::SetProperty { object: ObjectRef::Conductor(_), key, .. } if key == "wire_number")),
        "wire numbers in the link's transaction"
    );
    one(&mut c, "jumper", json!({"strip": strip, "positions": "2, 4", "skip": true}), "link.2");
    let l1 = json!({"strip": strip, "n": 1});
    one(&mut c, "jumper", json!({"link": l1, "add": "4"}), "link.1");
    one(&mut c, "jumper", json!({"link": l1, "remove": "4"}), "link.1");
    one(&mut c, "jumper", json!({"link": l1, "add": "4"}), "link.1");
    one(&mut c, "jumper", json!({"link": l1, "split_at": "2"}), "link.3");
    one(&mut c, "jumper", json!({"link": {"strip": strip, "n": 3}, "delete": true}), "link.3");
    one(&mut c, "setprop", json!({"component": strip, "key": "link.1", "value": "1-4"}), "link.1");
    one(&mut c, "setprop", json!({"component": strip, "key": "jumper.bar", "value": "2"}), "jumper.bar");
    // Renumber moves the keys and rewrites the links, in its one transaction.
    let n = c.log().len();
    let pv = c.exec("renumber", json!({"kind": "terminals", "start": 3, "preview": true}));
    assert!(pv["accessories"].to_string().contains("link 1-4 \u{2192} 3-6"), "{pv}");
    assert_eq!(c.log().len(), n, "a preview records nothing");
    c.exec("renumber", json!({"kind": "terminals", "start": 3}));
    assert_eq!(c.log().len(), n + 1);
    let (label, ops) = last_by_agent(&mut c);
    assert_eq!(label, "Renumber terminals");
    assert!(sets(&ops, "acc.after.6") && sets(&ops, "link.1"), "{ops:?}");
    // The reads record nothing.
    let n = c.log().len();
    let plan = c.exec("stripplan", json!({}));
    let s0 = &plan["strips"][0];
    assert_eq!(s0["links"][0]["positions"], "3-6", "{plan}");
    assert_eq!(s0["links"][0]["bars"], 2, "4 positions, bars of 2");
    let bom = c.exec("bom", json!({}));
    assert!(bom["rows"].as_array().unwrap().iter().any(|r| r["accessory"] == "end_stop" && r["part_number"] == "ES-1"), "{bom}");
    c.exec("checks", json!({"kinds": ["accessory-orphan", "no-accessory-part", "link-across-partition", "link-orphan"]}));
    assert_eq!(c.log().len(), n, "reads record nothing");
}

#[test]
fn with_writes_off_the_accessory_writes_are_refused_and_the_reads_work() {
    let mut c = reader();
    let before = c.log();
    for (cmd, params) in [
        ("accessory", json!({"strip": "1", "at": "start", "items": ["end_stop"]})),
        ("jumper", json!({"strip": "1", "positions": "1-4"})),
        ("jumper", json!({"link": {"strip": "1", "n": 1}, "delete": true})),
        ("setprop", json!({"component": "1", "key": "acc.after.4", "value": "partition"})),
        ("setprop", json!({"component": "1", "key": "link.1", "value": "1-4"})),
        ("setprop", json!({"component": "1", "key": "jumper.bar", "value": "4"})),
        ("projectprops", json!({"props": {"strip_jumper_bar": "4"}})),
        ("renumber", json!({"kind": "terminals"})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd}");
    }
    let plan = c.exec("stripplan", json!({"accessories": true}));
    assert_eq!(plan["strips"], json!([]));
    c.exec("bom", json!({}));
    c.exec("checks", json!({"kinds": ["link-orphan"]}));
    c.exec("renumber", json!({"kind": "terminals", "preview": true}));
    assert_eq!(c.log(), before, "nothing recorded");
}
