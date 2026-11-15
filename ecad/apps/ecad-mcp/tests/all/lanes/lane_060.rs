//! Brief 060 (R0): `setprop` on several components through MCP walks the single-component rules,
//! records one agent transaction, and is refused with writes off.

use ecad_mcp::{EcadMcp, Options};
use ecad_model::{Author, Op};
use serde_json::json;

use crate::mcp_contracts::{AGENT, Client, reader};

#[test]
fn setprop_on_several_connectors_through_mcp_refuses_what_one_refuses() {
    let mut c = Client::new(EcadMcp::in_memory("lane060", 60, Options { write: true, agent: None }).unwrap());
    let ids: Vec<String> = [50, 120]
        .iter()
        .map(|y| {
            let v = c.exec("connector", json!({"halves": "both", "family": "plain", "pins": 4, "layout": "row", "at": [50, y]}));
            v["component"].as_str().unwrap().to_string()
        })
        .collect();
    let before = c.log().len();
    let m = c.refused("execute", json!({"command": "setprop", "params": {"components": ids, "key": "pins", "value": "abc"}}));
    assert!(m.contains("pins"), "{m}");
    assert_eq!(c.log().len(), before, "a refusal records nothing");

    c.exec("setprop", json!({"components": ids, "key": "pins", "value": " 6 "}));
    let log = c.log();
    assert_eq!(log.len(), before + 1, "one transaction for both");
    let last = log.last().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    let set: Vec<&str> = last
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::SetProperty { key, after, .. } if key == "pins" => after.as_deref(),
            _ => None,
        })
        .collect();
    assert_eq!(set, ["6", "6"], "{:?}", last.ops);

    let mut r = reader();
    r.refused("execute", json!({"command": "setprop", "params": {"components": ids, "key": "pins", "value": "6"}}));
}
