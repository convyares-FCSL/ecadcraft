//! Lane 059 through MCP (brief 011 rule; design 059 §4.6): `cablenew`, `cableedit`, `cableremove`
//! and `cableslash` in assign mode record one transaction by the agent, undone by one `u`; their
//! refusals (cores in use, drawn, more wires than cores, an unknown key) write nothing; with
//! writes off every one is refused. A cable not drawn round-trips through the file (FD1).

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::{Author, Document, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, TempDir};

fn one_by_agent(c: &mut Client, cmd: &str, params: Value, label: &str) -> (Value, Vec<Op>) {
    let before: Document = c.doc();
    let n = c.log().len();
    let v = c.exec(cmd, params.clone());
    let log = c.log();
    assert_eq!(log.len(), n + 1, "{cmd} {params}: one transaction");
    let t = log.last().unwrap();
    assert_eq!(t.author, Author::Agent(AGENT.into()), "{cmd}");
    assert_eq!(t.label, label, "{cmd}");
    let ops = t.ops.clone();
    c.exec("u", json!({}));
    assert!(c.doc() == before, "{cmd} {params}: one undo restores the document");
    c.exec("redo", json!({}));
    (v, ops)
}

fn refused_writes_nothing(c: &mut Client, cmd: &str, params: Value, words: &str) {
    let n = c.log().len();
    let m = c.refused("execute", json!({"command": cmd, "params": params}));
    assert!(m.contains(words), "{cmd} {params}: {m}");
    assert_eq!(c.log().len(), n, "{cmd} {params}: nothing recorded");
}

#[test]
fn the_cable_manager_commands_are_agent_transactions_and_refused_with_writes_off() {
    let dir = TempDir::new("lane059");
    let path = dir.file("Cables 059.ecad");
    {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        // Four wires to slash, and a cable not drawn with 3 cores.
        for x in [0, 10, 20, 30] {
            c.exec("wire", json!({"points": [[x, 0], [x, 50]]}));
        }
        let (v, ops) = one_by_agent(&mut c, "cablenew", json!({"cores": "3G", "to": {"gland": "M20"}}), "Create cable");
        assert!(matches!(ops.as_slice(), [Op::CreateCable { cable, .. }] if cable.cores.is_empty()), "{ops:?}");
        let k = v["cable"].as_str().unwrap().to_string();
        let (_, ops) = one_by_agent(&mut c, "cableedit", json!({"cable": k, "set": {"length": 12}}), "Edit cable");
        assert_eq!(ops.len(), 1, "{ops:?}");
        refused_writes_nothing(&mut c, "cableedit", json!({"cable": k, "set": {"colour": 1}}), "is not a parameter of this command");
        // More wires than free cores: refused.
        refused_writes_nothing(&mut c, "cableslash", json!({"from": [-5, 25], "to": [35, 25], "cable": k}), "free cores");
        // Three wires: assigned.
        let (v, ops) = one_by_agent(&mut c, "cableslash", json!({"from": [-5, 25], "to": [25, 25], "cable": k}), "Cable slash");
        assert_eq!(v["assigned"], json!(true), "{v}");
        assert_eq!(ops.iter().filter(|o| matches!(o, Op::AddCableMember { .. })).count(), 3);
        refused_writes_nothing(&mut c, "cableedit", json!({"cable": k, "set": {"cores": "2G"}}), "has wires on cores");
        refused_writes_nothing(&mut c, "cableremove", json!({"cable": k}), "is drawn on");
        refused_writes_nothing(&mut c, "cableremove", json!({"cable": k, "drawn": "unassign"}), "is not a parameter of this command");
        let (v, _) = one_by_agent(&mut c, "cablenew", json!({}), "Create cable");
        let spare = v["cable"].as_str().unwrap().to_string();
        let (_, ops) = one_by_agent(&mut c, "cableremove", json!({"cable": spare}), "Remove cable");
        assert!(matches!(ops.as_slice(), [Op::DeleteCable { .. }]), "{ops:?}");
        // One cable not drawn stays, to round-trip through the file.
        c.exec("cablenew", json!({"tag": "-W09", "from": {"location": "+CAB1"}}));
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
    }
    let (sess, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut c = Client::new(sess);
    let doc = c.doc();
    let w09 =
        doc.cables.values().find(|x| x.properties.get("tag").map(String::as_str) == Some("-W09")).expect("FD1: the cable not drawn round-trips");
    assert!(w09.cores.is_empty());
    assert!(w09.properties.contains_key("end.from.location"));
    let log = c.log();
    for (cmd, params) in [
        ("cablenew", json!({})),
        ("cableedit", json!({"cable": "-W09", "set": {"length": 5}})),
        ("cableremove", json!({"cable": "-W09"})),
        ("cableslash", json!({"from": [-5, 40], "to": [35, 40], "cable": "-W09"})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd}");
    }
    assert_eq!(c.log(), log, "nothing recorded with writes off");
}
