//! Brief 042 (U4c) through MCP (brief 011 rule): `setdefault` records one transaction by
//! `Author::Agent(<client>)`, a refused level gives the reason and the allowed levels, and with
//! writes off it is refused and records nothing.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::Author;
use serde_json::json;

use crate::mcp_contracts::{AGENT, Client};

#[test]
fn setdefault_by_the_agent_then_refused_with_writes_off() {
    let mut c = Client::new(EcadMcp::in_memory("mcp 042", 42, Options { write: true, agent: None }).unwrap());
    let n = c.log().len();
    let v = c.exec("setdefault", json!({"key": "symbol.coil.prefix", "level": "project", "value": "Q"}));
    assert_eq!((v["value"].clone(), v["from"].clone(), v["changed"].clone()), (json!("Q"), json!("project"), json!(true)), "{v}");
    let log = c.log();
    assert_eq!(log.len(), n + 1, "one transaction");
    assert_eq!(log.last().map(|t| t.author.clone()), Some(Author::Agent(AGENT.into())));
    assert_eq!(c.doc().settings.tag_prefixes.get("coil").map(String::as_str), Some("Q"));
    // A level the entry does not allow: the reason and the levels it does.
    let m = c.refused("execute", json!({"command": "setdefault", "params": {"key": "text.tag.height", "level": "page", "value": 3}}));
    assert!(m.contains("cannot be set at page level") && m.contains("It can be set at: project"), "{m}");
    assert_eq!(c.log().len(), n + 1, "a refusal records nothing");

    let mut r = crate::mcp_contracts::reader();
    let log = r.log();
    let m = r.refused("execute", json!({"command": "setdefault", "params": {"key": "symbol.coil.prefix", "value": "Q"}}));
    assert_eq!(m, policy::WRITES_OFF);
    assert_eq!(r.log(), log, "nothing recorded with writes off");
}
