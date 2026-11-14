//! Brief 051 (PLC visuals) through MCP (brief 011 rule, design 051 §7): `plcpage` and a fence
//! record one transaction by `Author::Agent(<client>)` and are refused with writes off; `plcracks`
//! returns the tiles' data with writes off and records nothing.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::Author;
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client};

fn writer(seed: u64) -> Client {
    Client::new(EcadMcp::in_memory("mcp 051", seed, Options { write: true, agent: None }).unwrap())
}

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
fn q_plcpage_and_a_fence_by_the_agent_and_refused_with_writes_off() {
    let mut c = writer(5151);
    by_agent(&mut c, "place", json!({"symbol": "plccpu", "at": [50, 300]}));
    let di = by_agent(&mut c, "place", json!({"symbol": "plcdi", "at": [100, 300]}));
    let di = di["component"].as_str().unwrap().to_string();
    let ai = by_agent(&mut c, "place", json!({"symbol": "plcai", "at": [140, 300]}));
    let ai = ai["component"].as_str().unwrap().to_string();
    for i in 0..4 {
        let x = 100 + 20 * i;
        by_agent(&mut c, "wire", json!({"points": [[x, 250], [x, 150]]}));
    }
    by_agent(&mut c, "plcchannels", json!({"card": di, "from": [95, 200], "to": [175, 200]}));
    let v = by_agent(&mut c, "plcpage", json!({"card": ai}));
    assert_eq!(v["channels"], json!([0, 1, 2, 3]), "{v}");
    // The tiles' data, with no transaction.
    let n = c.log().len();
    let r = c.exec("plcracks", json!({}));
    assert_eq!(c.log().len(), n);
    let ai_row = &r["racks"][0]["cards"][1];
    assert_eq!(ai_row["pages"].as_array().map(Vec::len), Some(2), "{ai_row}");
    assert!(ai_row["range"]["inputs"].is_string(), "{ai_row}");
    assert_eq!(ai_row["checks"], 0);

    refused_with_writes_off("plcpage", json!({"card": "-A03"}));
    refused_with_writes_off("plcchannels", json!({"card": "-A02", "from": [0, 0], "to": [10, 0]}));
}

#[test]
fn q_plcracks_with_writes_off_records_nothing() {
    let mut r = crate::mcp_contracts::reader();
    let log = r.log();
    let v = r.exec("plcracks", json!({}));
    assert!(v["racks"].as_array().is_some_and(Vec::is_empty), "{v}");
    assert_eq!(r.log(), log);
}
