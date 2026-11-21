//! Lane 072 F, F1b (design 072 §6.2; brief 011): a continuation is a drawing aid. Placed through
//! MCP's `execute` it records `CreateAid` with its point owned by the aid; `setprop {aids}` and
//! `contcaption` record one `ModifyAid` each; `xrefs {aid}` lists its other end; all by the
//! agent, and every one refused with writes off.

use ecad_mcp::{EcadMcp, Options};
use ecad_model::{Aid, Author, Op};
use serde_json::json;

use crate::mcp_contracts::{AGENT, Client, reader};

fn last_aid(c: &mut Client) -> (Author, Vec<Op>) {
    let log = c.log();
    let last = log.last().unwrap();
    (last.author.clone(), last.ops.clone())
}

#[test]
fn continuations_through_mcp_are_aids() {
    let mut c = Client::new(EcadMcp::in_memory("lane072f", 0x72f, Options { write: true, agent: None }).unwrap());
    let a = c.exec("place", json!({"symbol": "continuation", "at": [20, 20], "name": "24V"}));
    let a = a["aid"].as_str().unwrap().to_string();
    let (author, ops) = last_aid(&mut c);
    assert_eq!(author, Author::Agent(AGENT.into()));
    assert!(ops.iter().any(|o| matches!(o, Op::CreateAid { aid: Aid::Continuation { .. }, .. })), "{ops:?}");
    assert!(ops.iter().any(|o| matches!(o, Op::AddTerminal { terminal, .. } if terminal.owner.aid().is_some())), "{ops:?}");
    assert!(!ops.iter().any(|o| matches!(o, Op::CreateDevice { .. })), "no device: {ops:?}");
    let b = c.exec("place", json!({"symbol": "continuation", "at": [80, 20], "name": "24V"}))["aid"].as_str().unwrap().to_string();

    c.exec("setprop", json!({"aids": [a], "key": "name", "value": "0V"}));
    let (author, ops) = last_aid(&mut c);
    assert_eq!(author, Author::Agent(AGENT.into()));
    match ops.as_slice() {
        [Op::ModifyAid { after, .. }] => assert_eq!(after.continuation_name().map(|n| n.as_str()), Some("0V")),
        other => panic!("one ModifyAid: {other:?}"),
    }
    c.exec("setprop", json!({"aids": [a], "key": "name", "value": "24V"}));

    c.exec("contcaption", json!({"continuations": [a, b], "caption": "TO PSU"}));
    let (author, ops) = last_aid(&mut c);
    assert_eq!(author, Author::Agent(AGENT.into()));
    assert_eq!(ops.iter().filter(|o| matches!(o, Op::ModifyAid { after, .. } if after.continuation_caption().is_some())).count(), 2, "{ops:?}");

    let x = c.exec("xrefs", json!({"aid": a}));
    assert!(x.to_string().contains("continuation"), "{x}");

    let before = c.log();
    let m = c.refused("execute", json!({"command": "setprop", "params": {"aids": [a], "key": "colour", "value": "red"}}));
    assert!(m.contains("name") && m.contains("caption"), "{m}");
    assert_eq!(c.log(), before, "nothing recorded");

    let mut r = reader();
    r.refused("execute", json!({"command": "place", "params": {"symbol": "continuation", "at": [20, 20], "name": "24V"}}));
    r.refused("execute", json!({"command": "setprop", "params": {"aids": [a], "key": "name", "value": "0V"}}));
    r.refused("execute", json!({"command": "contcaption", "params": {"continuations": [a], "caption": "X"}}));
}
