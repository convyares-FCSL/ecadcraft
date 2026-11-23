//! MCP contract: `setlocation`,
//! `setfunction`, `designationadd`, `designationedit`, `designationmerge` and
//! `designationremove` through `execute` record the agent's transaction and are refused with
//! writes off; `designations` reads with writes off.

use ecad_engine::agent::policy;
use ecad_model::{Author, Op};
use serde_json::{Value, json};

use crate::common::writer;
use crate::mcp_contracts::{AGENT, Client, reader};

pub(super) const WRITER_SEED: u64 = 52;

fn last_by_agent(c: &mut Client) -> (String, Vec<Op>) {
    let last = c.log().last().cloned().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    (last.label, last.ops)
}

fn hex(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap().to_string()
}

#[test]
fn location_and_list_commands_through_execute() {
    let mut c = writer(WRITER_SEED);
    let k = hex(&c.exec("place", json!({"symbol": "coil", "at": [50, 50]})), "component");
    let v = c.exec("setlocation", json!({"component": k, "value": "+CP1"}));
    assert_eq!(v["location"], "+CP1");
    let (label, ops) = last_by_agent(&mut c);
    assert_eq!(label, "Set location +CP1");
    assert!(
        ops.iter().any(|o| matches!(o, Op::ModifyProject { after, .. } if after.locations.iter().any(|e| e.code == "+CP1"))),
        "entry added (design 072 §5.3)"
    );
    assert!(
        ops.iter().any(|o| matches!(o, Op::ModifyDevice { after, .. } if matches!(after.location, ecad_model::document::book::Place::Manual(_))))
    );
    c.exec("setfunction", json!({"component": k, "value": "=F1"}));
    assert_eq!(last_by_agent(&mut c).0, "Set function =F1");
    c.exec("designationadd", json!({"aspect": "location", "code": "+CP2", "description": "Field"}));
    assert_eq!(last_by_agent(&mut c).0, "Add location +CP2");
    c.exec("designationedit", json!({"aspect": "location", "code": "+CP1", "to": "+CP10"}));
    assert_eq!(last_by_agent(&mut c).0, "Rename location +CP1 → +CP10");
    c.exec("designationmerge", json!({"aspect": "location", "from": "+CP10", "into": "+CP2"}));
    assert_eq!(last_by_agent(&mut c).0, "Merge location +CP10 into +CP2 (1 device)");
    c.exec("designationadd", json!({"aspect": "function", "code": "=SPARE"}));
    c.exec("designationremove", json!({"aspect": "function", "code": "=SPARE"}));
    assert_eq!(last_by_agent(&mut c).0, "Remove function =SPARE");
    let v = c.exec("designations", json!({}));
    assert_eq!(v["locations"]["entries"].as_array().map(Vec::len), Some(1), "{v}");
    assert_eq!(v["functions"]["entries"][0]["devices"], 1, "{v}");
}

/// Brief 052d (design 052 §10.22): `place` with `location` / `function` modes, `locationbox` and
/// `functionbox` returning `conflicts` with their aspect, and `conflicts {answer: "move"}`; `place`
/// never prompts.
#[test]
fn proposals_through_execute() {
    let mut c = writer(WRITER_SEED);
    let k = hex(&c.exec("place", json!({"symbol": "coil", "at": [300, 40]})), "component");
    c.exec("setlocation", json!({"component": k, "value": "+CP2"}));
    let v = c.exec("locationbox", json!({"location": "+CP1", "corners": [[20, 100], [200, 220]]}));
    assert_eq!(last_by_agent(&mut c).0, "Location box +CP1");
    assert_eq!(v["conflicts"], json!([]), "{v}");
    // A new device inside: set in the place's own transaction.
    let n = hex(&c.exec("place", json!({"symbol": "coil", "at": [75, 180]})), "component");
    let (label, ops) = last_by_agent(&mut c);
    assert_eq!(label, "Place symbol");
    assert!(
        ops.iter().any(|o| matches!(o, Op::ModifyDevice { after, .. } if matches!(after.location, ecad_model::document::book::Place::Proposed(_)))),
        "{ops:?}"
    );
    // Joining -K (at +CP2) inside the box: `move` moves it, `keep` for function proposes nothing.
    let v = c.exec("place", json!({"symbol": "no", "at": [120, 160], "device": k, "location": "move", "function": "keep"}));
    assert_eq!(v["location"]["conflicts"][0]["proposed"], "+CP1", "{v}");
    let (_, ops) = last_by_agent(&mut c);
    assert!(
        ops.iter().any(|o| matches!(o, Op::ModifyDevice { after, .. } if matches!(after.location, ecad_model::document::book::Place::Proposed(_)))),
        "moved in the same transaction"
    );
    // A function box around them: unset devices take =F1; the result carries each aspect.
    let v = c.exec("functionbox", json!({"function": "=F1", "corners": [[10, 90], [260, 230]]}));
    assert_eq!(last_by_agent(&mut c).0, "Function box =F1");
    assert_eq!(v["function"]["set"].as_array().map(Vec::len), Some(2), "{v}");
    // A device with =F2 inside a new =F3 box is a conflict with its aspect; Move sets it.
    c.exec("setfunction", json!({"component": n, "value": "=F2"}));
    let v = c.exec("functionbox", json!({"function": "=F3", "corners": [[60, 170], [90, 190]]}));
    assert_eq!(v["conflicts"][0]["aspect"], "function", "{v}");
    c.exec("conflicts", json!({"answer": "move"}));
    assert_eq!(last_by_agent(&mut c).0, "Set function =F3");
}

#[test]
fn with_writes_off_the_location_writes_are_refused_and_the_list_reads() {
    let mut c = reader();
    let before = c.log();
    for (cmd, params) in [
        ("setlocation", json!({"component": "1", "value": "+CP1"})),
        ("setfunction", json!({"component": "1", "value": "=F1"})),
        ("designationadd", json!({"aspect": "location", "code": "+CP1"})),
        ("designationedit", json!({"aspect": "location", "code": "+CP1", "to": "+CP2"})),
        ("designationmerge", json!({"aspect": "location", "from": "+CP1", "into": "+CP2"})),
        ("designationremove", json!({"aspect": "location", "code": "+CP1"})),
        ("place", json!({"symbol": "coil", "at": [0, 0], "location": "move", "function": "keep"})),
        ("locationbox", json!({"location": "+CP1", "corners": [[0, 0], [20, 20]], "proposal": "propose"})),
        ("functionbox", json!({"function": "=F1", "corners": [[0, 0], [20, 20]], "proposal": "propose"})),
        ("conflicts", json!({"answer": "move"})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd}");
    }
    c.exec("designations", json!({}));
    c.exec("conflicts", json!({}));
    assert_eq!(c.log().len(), before.len());
}

mod tester;
