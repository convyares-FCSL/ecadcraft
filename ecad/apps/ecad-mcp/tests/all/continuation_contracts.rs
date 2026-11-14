//! Brief 049 through MCP (brief 011 rule, design 049 §10.1): `contcaption` and `place` with a
//! caption each record one transaction by `Author::Agent(<client>)` and are refused with writes
//! off; `xrefs` and `refgoto` read an arrow's other ends and record nothing; hostile captions are
//! errors in words.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::Author;
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client};

fn writer() -> Client {
    Client::new(EcadMcp::in_memory("mcp 049", 49, Options { write: true, agent: None }).unwrap())
}

/// One transaction since `n`, by the agent, labelled `label`.
fn one_by_agent(c: &mut Client, n: usize, label: &str) {
    let log = c.log();
    assert_eq!(log.len(), n + 1, "one transaction for {label}");
    let t = log.last().unwrap();
    assert_eq!(t.author, Author::Agent(AGENT.into()));
    assert_eq!(t.label, label);
}

fn hex(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap().to_string()
}

#[test]
fn contcaption_and_place_with_a_caption_by_the_agent() {
    let mut c = writer();
    let n = c.log().len();
    let a = c.exec("place", json!({"symbol": "continuation", "at": [100, 150], "name": "24V", "caption": "TO CONTROL SUPPLY"}));
    assert_eq!(a["caption"], "TO CONTROL SUPPLY");
    one_by_agent(&mut c, n, "Continuation");
    let b = c.exec("place", json!({"symbol": "continuation", "at": [300, 150], "name": "24V"}));

    let n = c.log().len();
    let v = c.exec("contcaption", json!({"continuations": [hex(&a, "component"), hex(&b, "component")], "caption": "TO CONTROL SUPPLY"}));
    assert_eq!((v["changed"].clone(), v["unchanged"].clone()), (json!(1), json!(1)), "{v}");
    one_by_agent(&mut c, n, "Caption TO CONTROL SUPPLY");

    let n = c.log().len();
    c.exec("contcaption", json!({"continuations": [hex(&a, "component")], "caption": null}));
    one_by_agent(&mut c, n, "Remove caption");

    // Reading: each arrow lists the other; Go to opens it. Nothing is recorded.
    let n = c.log().len();
    let v = c.exec("xrefs", json!({"component": hex(&a, "component")}));
    let rows = v["references"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{v}");
    assert_eq!((rows[0]["kind"].clone(), rows[0]["target"].clone()), (json!("continuation"), json!(hex(&b, "representation"))));
    let v = c.exec("refgoto", json!({"source": hex(&a, "representation")}));
    assert_eq!(v["target"], json!(hex(&b, "representation")));
    assert_eq!(c.log().len(), n, "reading records nothing");

    // Hostile captions: errors in words, nothing written.
    for bad in [json!("x".repeat(10_000)), json!("A\nB"), json!(42)] {
        let m = c.refused("execute", json!({"command": "contcaption", "params": {"continuations": [hex(&a, "component")], "caption": bad}}));
        assert!(m.contains("caption"), "{m}");
    }
    let k = c.exec("place", json!({"symbol": "coil", "at": [200, 50]}));
    let n = c.log().len();
    let m = c.refused("execute", json!({"command": "contcaption", "params": {"continuations": [hex(&k, "component")], "caption": "X"}}));
    assert!(m.starts_with("Only continuation arrows have a caption;"), "{m}");
    assert_eq!(c.log().len(), n);
}

#[test]
fn caption_writes_are_refused_with_writes_off() {
    let mut r = crate::mcp_contracts::reader();
    let log = r.log();
    let any = "0123456789abcdef0123456789abcdef";
    for (command, params) in [
        ("contcaption", json!({"continuations": [any], "caption": "X"})),
        ("place", json!({"symbol": "continuation", "at": [0, 0], "name": "24V", "caption": "X"})),
    ] {
        let m = r.refused("execute", json!({"command": command, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{command}");
    }
    let v = r.exec("xrefs", json!({}));
    assert!(v["references"].is_array(), "{v}");
    assert_eq!(r.log(), log, "nothing recorded with writes off");
}
