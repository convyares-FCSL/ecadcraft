//! Brief 036, MCP contract (the ECAD rule of brief 011, design 036 §13 M): `terminals` with a
//! type and level, `place` of a level starter, `termlabel`, `toposition`, `jumper` with levels,
//! `setprop` of `term_label.*`, `rating.*`, `part_number.lnpe`, `terminals` with type codes, and
//! `projectprops strip_lnpe_part_number` through `execute` record the agent's transaction and are
//! refused with writes off; `stripplan`, `bom`, `checks` and `defaults {group: "Terminals"}` work
//! with writes off and record nothing.

use ecad_mcp::policy;
use ecad_model::{Author, ObjectRef, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, reader};

fn writer() -> Client {
    Client::new(ecad_mcp::EcadMcp::in_memory("mcp", 36, ecad_mcp::Options { write: true, agent: None }).unwrap())
}

fn hex(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap().to_string()
}

fn last_by_agent(c: &mut Client) -> (String, Vec<Op>) {
    let last = c.log().last().cloned().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    (last.label, last.ops)
}

fn set(ops: &[Op], key: &str) -> Option<String> {
    ops.iter().find_map(|o| match o {
        Op::SetProperty { object: ObjectRef::Component(_) | ObjectRef::Project, key: k, after, .. } if k == key => after.clone(),
        _ => None,
    })
}

#[test]
fn multi_level_terminal_commands_through_execute() {
    let mut c = writer();
    for (x0, y0) in [(10, 0), (10, 100)] {
        for i in 0..3 {
            let x = x0 + 10 * i;
            c.exec("wire", json!({"points": [[x, y0], [x, y0 + 40]]}));
        }
    }
    // Levels by fence: L across the first three wires, N across the next three.
    let l = c.exec("terminals", json!({"type": "LNPE", "level": "L", "from": [0, 20], "to": [40, 20]}));
    assert_eq!(last_by_agent(&mut c).0, "Place terminal level");
    let strip = hex(&l, "strip");
    c.exec("terminals", json!({"strip": strip, "type": "LNPE", "level": "N", "from": [0, 120], "to": [40, 120]}));
    let (label, ops) = last_by_agent(&mut c);
    assert_eq!(label, "Place terminal level");
    assert!(ops.iter().any(|o| matches!(o, Op::AddTerminal { terminal, .. } if terminal.key.as_deref() == Some(".2"))), "level keys .n");
    // A level starter placed on its own joins the strip at the next free position.
    let p = c.exec("place", json!({"symbol": "termlnpe.3", "at": [200, 200], "device": strip}));
    assert_eq!(p["position"], "1", "{p}");
    assert_eq!(last_by_agent(&mut c).0, "Place terminal level");
    // Labels.
    let t = hex(&l["terminals"][0], "terminal");
    c.exec("termlabel", json!({"terminal": t, "text": "1L"}));
    assert_eq!(set(&last_by_agent(&mut c).1, "term_label.1.1").as_deref(), Some("1L"));
    c.exec("termlabel", json!({"terminal": t, "position": "10"}));
    let (label, ops) = last_by_agent(&mut c);
    assert_eq!(label, "Relabel position");
    assert_eq!(set(&ops, "term_label.10.1").as_deref(), Some("1L"), "the level's label moves with its position");
    c.exec("setprop", json!({"component": strip, "key": "term_label.2.2", "value": "2N"}));
    assert_eq!(set(&last_by_agent(&mut c).1, "term_label.2.2").as_deref(), Some("2N"));
    // Links per level, by the Jumper and by setprop.
    let n2 = hex(&c.exec("stripplan", json!({"strip": strip}))["strips"][0]["rows"][1], "terminal");
    let n3 = c.exec("stripplan", json!({"strip": strip}))["strips"][0]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["label"] == "3.N")
        .map(|r| hex(r, "terminal"))
        .unwrap();
    c.exec("jumper", json!({"terminals": [n2, n3]}));
    assert_eq!(set(&last_by_agent(&mut c).1, "link.1").as_deref(), Some("2.2-3.2"));
    c.exec("setprop", json!({"component": strip, "key": "link.2", "value": "2.1-2.2"}));
    assert_eq!(set(&last_by_agent(&mut c).1, "link.2").as_deref(), Some("2.1-2.2"));
    // Offers by type, part per type, a project default.
    c.exec("setprop", json!({"component": strip, "key": "terminals", "value": "2+4LNPE"}));
    assert_eq!(set(&last_by_agent(&mut c).1, "terminals").as_deref(), Some("2+4LNPE"));
    c.exec("setprop", json!({"component": strip, "key": "part_number.lnpe", "value": "LNPE-4"}));
    assert_eq!(set(&last_by_agent(&mut c).1, "part_number.lnpe").as_deref(), Some("LNPE-4"));
    c.exec("projectprops", json!({"props": {"strip_lnpe_part_number": "LNPE-X"}}));
    assert_eq!(set(&last_by_agent(&mut c).1, "strip_lnpe_part_number").as_deref(), Some("LNPE-X"));
    // A fuse position on a wire, its rating, and a fuse device turned into a position.
    let w = c.exec("wire", json!({"points": [[300, 0], [300, 80]]}));
    let _ = hex(&w, "conductor");
    let f = c.exec("terminals", json!({"strip": strip, "type": "FUSE", "from": [290, 40], "to": [310, 40]}));
    let fpos = f["terminals"][0]["position"].as_str().unwrap().to_string();
    c.exec("setprop", json!({"component": strip, "key": format!("rating.{fpos}"), "value": "2A"}));
    assert_eq!(set(&last_by_agent(&mut c).1, &format!("rating.{fpos}")).as_deref(), Some("2A"));
    let dev = c.exec("place", json!({"symbol": "fuseterm", "at": [250, 300]}));
    let dev = hex(&dev, "component");
    c.exec("toposition", json!({"device": dev, "strip": strip}));
    let (label, ops) = last_by_agent(&mut c);
    assert!(label.contains("\u{2192} -X01:"), "{label}");
    assert!(ops.iter().any(|o| matches!(o, Op::DeleteComponent { .. })));
    // The reads record nothing.
    let n = c.log().len();
    let plan = c.exec("stripplan", json!({"strip": strip}));
    let s = &plan["strips"][0];
    assert!(s["rows"].as_array().unwrap().iter().any(|r| r["label"] == "10.L" || r["label"] == "1L"), "{s}");
    assert!(plan["fields"].as_array().unwrap().iter().any(|f| f["key"] == "position" && f["editable"] == true));
    let bom = c.exec("bom", json!({}));
    assert!(bom["rows"].as_array().unwrap().iter().any(|r| r["part_number"] == "LNPE-4"), "{bom}");
    c.exec("checks", json!({"kinds": ["level-twice", "position-mixed", "level-missing", "fuse-device", "no-fuse-rating"]}));
    let d = c.exec("defaults", json!({"group": "Terminals"}));
    assert!(d.to_string().contains("terminal.level.separator"), "{d}");
    assert_eq!(c.log().len(), n, "reads record nothing");
}

#[test]
fn with_writes_off_the_level_writes_are_refused_and_the_reads_work() {
    let mut c = reader();
    let before = c.log();
    for (cmd, params) in [
        ("terminals", json!({"type": "LNPE", "level": "L", "from": [0, 0], "to": [10, 0]})),
        ("place", json!({"symbol": "termlnpe.2", "at": [0, 0]})),
        ("termlabel", json!({"terminal": "1", "text": "1L"})),
        ("toposition", json!({"device": "1"})),
        ("jumper", json!({"terminals": ["1", "2"]})),
        ("setprop", json!({"component": "1", "key": "term_label.1.1", "value": "x"})),
        ("setprop", json!({"component": "1", "key": "rating.1", "value": "2A"})),
        ("setprop", json!({"component": "1", "key": "part_number.lnpe", "value": "P"})),
        ("setprop", json!({"component": "1", "key": "terminals", "value": "12+4x3"})),
        ("projectprops", json!({"props": {"strip_lnpe_part_number": "P"}})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd}");
    }
    c.exec("stripplan", json!({}));
    c.exec("bom", json!({}));
    c.exec("checks", json!({}));
    c.exec("defaults", json!({"group": "Terminals"}));
    assert_eq!(c.log(), before, "nothing recorded");
}
