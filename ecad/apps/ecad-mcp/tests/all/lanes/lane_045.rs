//! Brief 045 (PLC racks and cards) through MCP (brief 011 rule): each write records one
//! transaction by `Author::Agent(<client>)` and is refused with writes off; the reads work with
//! writes off and record nothing.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::Author;
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client};

fn writer(seed: u64) -> Client {
    Client::new(EcadMcp::in_memory("mcp 045", seed, Options { write: true, agent: None }).unwrap())
}

/// One write: one more transaction, by the agent.
fn by_agent(c: &mut Client, command: &str, params: Value) -> Value {
    let n = c.log().len();
    let v = c.exec(command, params);
    let log = c.log();
    assert_eq!(log.len(), n + 1, "{command}: one transaction");
    assert_eq!(log.last().map(|t| t.author.clone()), Some(Author::Agent(AGENT.into())), "{command}");
    v
}

fn refused_with_writes_off(command: &str, params: Value) {
    let mut r = crate::mcp_contracts::reader();
    let log = r.log();
    let m = r.refused("execute", json!({"command": command, "params": params}));
    assert_eq!(m, policy::WRITES_OFF, "{command}");
    assert_eq!(r.log(), log, "{command}: nothing recorded with writes off");
}

#[test]
fn a7_rack_and_slot_writes_by_the_agent_and_refused_with_writes_off() {
    let mut c = writer(4571);
    let cpu = by_agent(&mut c, "place", json!({"symbol": "plccpu", "at": [50, 200]}));
    let v = by_agent(&mut c, "place", json!({"symbol": "plcdi", "at": [100, 200], "rack": "-A01", "slot": 3}));
    assert_eq!((v["rack"].clone(), v["slot"].clone()), (cpu["component"].clone(), json!(3)), "{v}");
    let card = v["component"].as_str().unwrap().to_string();
    by_agent(&mut c, "plcslot", json!({"card": card, "slot": 5}));
    by_agent(&mut c, "setprop", json!({"component": card, "key": "plc.channels", "value": "32"}));
    by_agent(&mut c, "setdefault", json!({"key": "plc.slots", "level": "project", "value": "12"}));
    // Reads: no transaction.
    let n = c.log().len();
    let r = c.exec("plcracks", json!({}));
    assert_eq!(r["racks"][0]["slots"], 12);
    assert_eq!(c.log().len(), n);

    refused_with_writes_off("plcslot", json!({"card": "-A02", "slot": 2}));
    refused_with_writes_off("place", json!({"symbol": "plcdi", "at": [0, 0], "rack": "-A01", "slot": 1}));
    refused_with_writes_off("setprop", json!({"component": card, "key": "plc.slot", "value": "2"}));
    refused_with_writes_off("setdefault", json!({"key": "plc.slots", "level": "project", "value": "8"}));
}

#[test]
fn a7_reads_work_with_writes_off() {
    let mut r = crate::mcp_contracts::reader();
    let log = r.log();
    let v = r.exec("plcracks", json!({}));
    assert!(v["racks"].as_array().is_some_and(Vec::is_empty), "{v}");
    let d = r.exec("defaults", json!({"group": "PLC"}));
    let keys: Vec<&str> = d["defaults"].as_array().into_iter().flatten().filter_map(|e| e["key"].as_str()).collect();
    assert!(keys.contains(&"plc.slots") && keys.contains(&"plc.card.caption"), "{d}");
    r.exec("checks", json!({}));
    assert_eq!(r.log(), log, "reads record nothing");
}

#[test]
fn b7_channel_and_address_writes_by_the_agent_and_refused_with_writes_off() {
    let mut c = writer(4572);
    by_agent(&mut c, "place", json!({"symbol": "plccpu", "at": [50, 300]}));
    let card = by_agent(&mut c, "place", json!({"symbol": "plcdi", "at": [100, 300]}));
    let card = card["component"].as_str().unwrap().to_string();
    for i in 0..4 {
        let x = 100 + 10 * i;
        by_agent(&mut c, "wire", json!({"points": [[x, 250], [x, 150]]}));
    }
    let v = by_agent(&mut c, "plcchannels", json!({"card": card, "from": [95, 200], "to": [135, 200]}));
    assert_eq!(v["channels"], json!([0, 1, 2, 3]), "{v}");
    by_agent(&mut c, "place", json!({"symbol": "chdi", "at": [200, 200], "device": card}));
    by_agent(&mut c, "plcchannel", json!({"card": card, "channel": 1, "text": "Door closed", "address": "I9.9"}));
    by_agent(&mut c, "plcaddress", json!({"rack": "-A01", "style": "iec"}));
    by_agent(&mut c, "plcchannels", json!({"card": card, "rest": true}));

    refused_with_writes_off("plcchannels", json!({"type": "DI", "from": [0, 0], "to": [10, 0]}));
    refused_with_writes_off("plcchannel", json!({"card": "-A02", "channel": 0, "text": "x"}));
    refused_with_writes_off("plcaddress", json!({"rack": "-A01", "fix": true}));
    refused_with_writes_off("place", json!({"symbol": "chdi", "at": [0, 0], "device": "-A02", "channel": 3}));
}

#[test]
fn c5_iolist_works_with_writes_off() {
    let mut r = crate::mcp_contracts::reader();
    let log = r.log();
    let v = r.exec("iolist", json!({}));
    assert!(v["rows"].as_array().is_some_and(Vec::is_empty), "{v}");
    assert_eq!(r.log(), log, "a read records nothing");
}
