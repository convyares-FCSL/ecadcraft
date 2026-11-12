//! Brief 025, MCP contract (the ECAD rule of brief 011): `setprop` with `contacts` and `mirror`,
//! `mirrormove` and a `place` join through `execute` record the agent's transaction and are
//! refused with writes off; `xrefs`, `contacts` and `refgoto` work with writes off and record
//! nothing.

use ecad_mcp::policy;
use ecad_model::{Author, ObjectRef, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, reader};

fn writer() -> Client {
    Client::new(ecad_mcp::EcadMcp::in_memory("mcp", 25, ecad_mcp::Options { write: true, agent: None }).unwrap())
}

fn hex(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap().to_string()
}

fn last_by_agent(c: &mut Client) -> Vec<Op> {
    let last = c.log().last().cloned().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    last.ops
}

#[test]
fn cross_reference_commands_through_execute() {
    let mut c = writer();
    let coil = c.exec("place", json!({"symbol": "coil", "at": [75, 180]}));
    let k1 = hex(&coil, "component");
    c.exec("place", json!({"symbol": "power3", "at": [125, 180], "device": k1}));
    // The offer: one `setprop`, by the agent.
    c.exec("setprop", json!({"component": k1, "key": "contacts", "value": "3P+1NO+1NC"}));
    assert!(
        matches!(&last_by_agent(&mut c)[..], [Op::SetProperty { object: ObjectRef::Component(_), key, after: Some(v), .. }] if key == "contacts" && v == "3P+1NO+1NC")
    );
    // A join takes the offered labels.
    let no = c.exec("place", json!({"symbol": "no", "at": [100, 100], "device": k1}));
    assert_eq!(no["offer_slot"], json!(true));
    assert!(last_by_agent(&mut c).iter().any(|o| matches!(o, Op::AddTerminal { terminal, .. } if terminal.label.as_deref() == Some("13"))));
    // The table's placement, by `setprop` and by Move Mirror Table.
    c.exec("setprop", json!({"component": k1, "key": "mirror", "value": "below"}));
    assert!(matches!(&last_by_agent(&mut c)[..], [Op::SetProperty { key, after: Some(v), .. }] if key == "mirror" && v == "below"));
    let m = c.exec("mirrormove", json!({"component": k1, "at": [90, 150]}));
    assert_eq!(m["mirror"], json!("at 15,-30"), "{m}");
    let last = c.log().last().cloned().unwrap();
    assert_eq!(last.label, "Move contact table");
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    // The reads record nothing.
    let n = c.log().len();
    let x = c.exec("xrefs", json!({"component": k1}));
    assert_eq!(x["references"].as_array().unwrap().len(), 8, "{x}");
    let t = c.exec("contacts", json!({"component": k1}));
    assert_eq!(t["offer"], json!("3P+1NO+1NC"));
    assert_eq!(t["rows"].as_array().unwrap().len(), 5);
    let g = c.exec("refgoto", json!({"source": hex(&no, "representation")}));
    assert_eq!(g["target"], json!(hex(&coil, "representation")));
    assert_eq!(c.log().len(), n, "reads record nothing");
}

#[test]
fn with_writes_off_the_writes_are_refused_and_the_reads_work() {
    let mut c = reader();
    let before = c.log();
    for (cmd, params) in [
        ("setprop", json!({"component": "1", "key": "contacts", "value": "3P"})),
        ("setprop", json!({"component": "1", "key": "mirror", "value": "off"})),
        ("mirrormove", json!({"component": "1", "placement": "off"})),
        ("place", json!({"symbol": "no", "at": [0, 0], "device": "1"})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd}");
    }
    let x = c.exec("xrefs", json!({}));
    assert_eq!(x["references"], json!([]));
    let m = c.refused("execute", json!({"command": "contacts", "params": {"component": "1"}}));
    assert!(m.contains("no such component"), "{m}");
    let m = c.refused("execute", json!({"command": "refgoto", "params": {"source": "1"}}));
    assert!(m.contains("no such reference"), "{m}");
    assert_eq!(c.log(), before, "nothing recorded");
}
