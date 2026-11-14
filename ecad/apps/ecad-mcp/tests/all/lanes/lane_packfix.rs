//! The drawing-pack re-run's fixes (2026-11-14) through MCP (brief 011 rule): `plcchannels` with
//! `point` and `place` of a project symbol with `tag` each record one transaction by
//! `Author::Agent(<client>)` and are refused with writes off.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::Author;
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client};

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
fn fence_point_and_project_symbol_tag_by_the_agent_and_refused_with_writes_off() {
    let mut c = Client::new(EcadMcp::in_memory("mcp packfix", 9190, Options { write: true, agent: None }).unwrap());
    by_agent(&mut c, "wire", json!({"points": [[100, 250], [100, 150]]}));
    let v = by_agent(&mut c, "plcchannels", json!({"type": "IOL", "point": "L+", "from": [94, 200], "to": [106, 200]}));
    assert_eq!(v["channels"], json!([0]), "{v}");
    by_agent(
        &mut c,
        "symbolnew",
        json!({"name": "Sensor", "key": "s1", "graphics": [{"kind": "line", "a": [-5, 0], "b": [5, 0]}],
            "points": [{"key": "1", "at": [-5, 0]}], "slots": [{"text": "tag", "at": [0, 4]}], "defaults": {"prefix": "B"}}),
    );
    let v = by_agent(&mut c, "place", json!({"symbol": "s1", "at": [200, 100], "tag": "B07"}));
    assert_eq!(v["tag"], json!("-B07"), "{v}");

    refused_with_writes_off("plcchannels", json!({"type": "IOL", "point": "L+", "from": [94, 200], "to": [106, 200]}));
    refused_with_writes_off("place", json!({"symbol": "s1", "at": [200, 100], "tag": "B07"}));
}
