//! Lane 072 D (design 072 §4.2; brief 011): a wire's size, colours and rating inputs set through
//! MCP's `execute` record one `ModifyWire` by the agent; a value that does not read, and a key a
//! wire does not have, are refused and record nothing; with writes off the command is refused.

use ecad_mcp::{EcadMcp, Options};
use ecad_model::units::Amps;
use ecad_model::wire::Basis;
use ecad_model::{Author, Op, Wire};
use serde_json::json;

use crate::mcp_contracts::{AGENT, Client, reader};

fn last_modified(c: &mut Client) -> (Author, Wire) {
    let log = c.log();
    let last = log.last().unwrap();
    let after = last
        .ops
        .iter()
        .find_map(|o| match o {
            Op::ModifyWire { after, .. } => Some(after.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("a ModifyWire: {:?}", last.ops));
    (last.author.clone(), after)
}

#[test]
fn a_wires_values_set_through_mcp_record_one_modify_wire() {
    let mut c = Client::new(EcadMcp::in_memory("lane072d", 72, Options { write: true, agent: None }).unwrap());
    let v = c.exec("wire", json!({"points": [[0, 0], [50, 0]]}));
    let w = v["conductor"].as_str().unwrap().to_string();

    let before = c.log().len();
    c.exec("setprop", json!({"conductor": w, "key": "size", "value": "2.5 mm²"}));
    assert_eq!(c.log().len(), before + 1);
    let (author, after) = last_modified(&mut c);
    assert_eq!(author, Author::Agent(AGENT.into()));
    assert_eq!(after.size.map(|s| s.to_string()).as_deref(), Some("2.5"));

    c.exec("setprop", json!({"conductor": w, "key": "colours", "value": "GNYE"}));
    let (_, after) = last_modified(&mut c);
    assert_eq!(after.colours.map(|c| c.to_string()).as_deref(), Some("GN/YE"));

    c.exec("rating", json!({"wire": w, "basis": "manual", "manual": "16 A", "load": "4 A"}));
    let (author, after) = last_modified(&mut c);
    assert_eq!(author, Author::Agent(AGENT.into()));
    assert_eq!((after.rating.basis, after.rating.manual, after.load), (Some(Basis::Manual), Some(Amps(16.0)), Some(Amps(4.0))));

    let before = c.log().len();
    for (key, value, words) in [("size", "thick", "wire size"), ("colours", "tartan", "colour list"), ("tag", "W1", "no property")] {
        let m = c.refused("execute", json!({"command": "setprop", "params": {"conductor": w, "key": key, "value": value}}));
        assert!(m.contains(words), "{key}: {m}");
    }
    assert_eq!(c.log().len(), before, "a refusal records nothing");

    let mut r = reader();
    r.refused("execute", json!({"command": "setprop", "params": {"conductor": w, "key": "size", "value": "2.5"}}));
}
