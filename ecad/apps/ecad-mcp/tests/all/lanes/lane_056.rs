//! Brief 056 (the PLC signal list) through MCP (brief 011 rule, design 056 §10):
//! `plcchannel {tag, safe}`, `plcmaketags`, `plcimport` and `plccardtype` record one transaction
//! by `Author::Agent(<client>)` and are refused with writes off; `plcimport {dry_run: true}`,
//! `plcexport` and `iolist` work with writes off and record nothing.

use ecad_mcp::policy;
use ecad_model::Author;
use serde_json::{Value, json};

use crate::common::writer;
use crate::mcp_contracts::{AGENT, Client};

const TAGS: &str = "name,address,type,comment\nPump_Run,Q0.0,Bool,Pump running\nSpare_1,Q0.3,Bool,Spare one\n";

fn by_agent(c: &mut Client, command: &str, params: Value) -> Value {
    let n = c.log().len();
    let v = c.exec(command, params);
    let log = c.log();
    assert_eq!(log.len(), n + 1, "{command}: one transaction: {v}");
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
fn n_writes_are_the_agents_one_transaction_each_and_refused_with_writes_off() {
    let mut c = writer(5661);
    by_agent(&mut c, "place", json!({"symbol": "plccpu", "at": [50, 300]}));
    by_agent(&mut c, "place", json!({"symbol": "plcdq", "at": [100, 300]}));
    by_agent(&mut c, "plcchannel", json!({"card": "-A02", "channel": 0, "tag": "Pump_On", "text": "Pump on", "safe": "hold"}));
    by_agent(&mut c, "plcchannel", json!({"card": "-A02", "channel": 1, "text": "Mixer on"}));
    let v = by_agent(&mut c, "plcmaketags", json!({"card": "-A02"}));
    assert_eq!(v["made"], 1, "{v}");
    let v = by_agent(&mut c, "plcimport", json!({"text": TAGS, "name": "tags.csv"}));
    assert_eq!(v["tags"], 2, "Pump_Run at %Q0.0 and Spare_1 at %Q0.3: {v}");
    by_agent(&mut c, "plccardtype", json!({"card": "-A02", "type": "AO"}));

    refused_with_writes_off("plcchannel", json!({"card": "-A02", "channel": 0, "tag": "x"}));
    refused_with_writes_off("plcchannel", json!({"card": "-A02", "channel": 0, "safe": "on"}));
    refused_with_writes_off("plcmaketags", json!({"card": "-A02"}));
    refused_with_writes_off("plcimport", json!({"text": TAGS}));
    refused_with_writes_off("plcimport", json!({"text": TAGS, "dry_run": false}));
    refused_with_writes_off("plccardtype", json!({"card": "-A02", "type": "DI"}));
}

#[test]
fn n_reads_work_with_writes_off_and_record_nothing() {
    let mut r = crate::mcp_contracts::reader();
    let log = r.log();
    let v = r.exec("plcimport", json!({"text": TAGS, "dry_run": true}));
    assert_eq!(v["written"], false, "{v}");
    assert_eq!(v["io"], 2, "{v}");
    let v = r.exec("plcexport", json!({}));
    assert_eq!(v["rows"], 0, "{v}");
    assert!(v["text"].as_str().is_some_and(|t| t.starts_with("name,address,type,comment")), "{v}");
    r.exec("iolist", json!({}));
    assert_eq!(r.log(), log, "reads record nothing");
}

#[test]
fn n_a_file_path_is_refused_from_an_agent() {
    let mut c = writer(5662);
    let log = c.log();
    let m = c.refused("execute", json!({"command": "plcexport", "params": {"path": "tags.csv"}}));
    assert!(!m.is_empty());
    let m = c.refused("execute", json!({"command": "plcimport", "params": {"path": "tags.csv"}}));
    assert!(!m.is_empty());
    assert_eq!(c.log(), log);
}
