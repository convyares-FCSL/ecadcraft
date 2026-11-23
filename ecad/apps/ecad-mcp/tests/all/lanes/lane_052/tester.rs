//! Hostile and bounded input to the location, function and designation commands through `execute`
//! is refused with a message, writes nothing and never panics; an agent's merge is one
//! transaction that its own undo takes back exactly.

use ecad_model::Author;
use serde_json::{Value, json};

use super::{WRITER_SEED, hex};
use crate::common::writer;
use crate::mcp_contracts::AGENT;

/// Every hostile call is refused, the log is unchanged and no message reports a panic.
#[test]
fn hostile_location_and_list_input_is_refused_and_writes_nothing() {
    let mut c = writer(WRITER_SEED);
    let k = hex(&c.exec("place", json!({"symbol": "coil", "at": [50, 50]})), "component");
    c.exec("setlocation", json!({"component": k, "value": "+CP1"}));
    let before = c.log().len();
    let long = format!("+{}", "Z".repeat(10_000));
    let many: Vec<Value> = (0..20_000).map(|_| json!(k)).collect();
    let cases: Vec<(&str, Value)> = vec![
        ("setlocation", json!({"component": k, "value": long})),
        ("setlocation", json!({"component": k, "value": "+A\u{0}B"})),
        ("setlocation", json!({"component": k, "value": ["+CP1"]})),
        ("setlocation", json!({"component": k, "value": {"code": "+CP1"}})),
        ("setlocation", json!({"component": 7, "value": "+CP1"})),
        ("setlocation", json!({"component": "not hex", "value": "+CP1"})),
        ("setlocation", json!({"devices": many, "value": "+CP2"})),
        ("setlocation", json!({"devices": "abc", "value": "+CP2"})),
        ("setlocation", json!({"component": k, "type": "head", "value": "+CP2"})),
        ("setfunction", json!({"component": k, "type": "plug", "value": "=F2"})),
        ("designationadd", json!({"aspect": "location", "code": long})),
        ("designationadd", json!({"aspect": 1, "code": "+CP3"})),
        ("designationadd", json!({"aspect": "location", "code": null})),
        ("designationedit", json!({"aspect": "location", "code": "+CP1", "to": "+A\u{1b}[2J"})),
        ("designationedit", json!({"aspect": "location", "key": {"x": 1}, "code": "+CP4"})),
        ("designationmerge", json!({"aspect": "location", "from": "+CP1", "into": "+CP1"})),
        ("designationmerge", json!({"aspect": "location", "from": "+CP1", "into": "+NOPE"})),
        ("designationremove", json!({"aspect": "location", "code": "+CP1"})),
        ("designationremove", json!({"aspect": "location", "key": "%00"})),
    ];
    for (cmd, params) in cases {
        let m = c.refused("execute", json!({"command": cmd, "params": params.clone()}));
        let low = m.to_lowercase();
        assert!(!low.contains("internal error") && !low.contains("panick"), "{cmd}: {m}");
        assert!(m.len() < 2_000, "{cmd}: the refusal echoes the hostile input ({} bytes)", m.len());
        assert_eq!(c.log().len(), before, "{cmd} {params}: nothing written");
    }
}

/// An agent's merge records one transaction by the agent; the agent's own undo restores the
/// document exactly, and the redo gives the merged one back.
#[test]
fn an_agent_merge_undoes_exactly() {
    let mut c = writer(WRITER_SEED);
    let k = hex(&c.exec("place", json!({"symbol": "coil", "at": [50, 50]})), "component");
    c.exec("setlocation", json!({"component": k, "value": "+CP01"}));
    c.exec("designationadd", json!({"aspect": "location", "code": "+CP1"}));
    let before = c.doc();
    let n = c.log().len();
    c.exec("designationmerge", json!({"aspect": "location", "from": "+CP01", "into": "+CP1"}));
    let log = c.log();
    assert_eq!(log.len(), n + 1);
    assert_eq!(log.last().map(|t| t.author.clone()), Some(Author::Agent(AGENT.into())));
    let after = c.doc();
    c.exec("undo", json!({}));
    assert_eq!(c.doc(), before, "undo is exact");
    c.exec("redo", json!({}));
    assert_eq!(c.doc(), after, "redo is exact");
}

/// A `type` that is not text is refused; it is never read as "no type", which would set the
/// device's own location instead of the override the agent asked for.
#[test]
fn a_type_that_is_not_text_is_refused_not_ignored() {
    let mut c = writer(WRITER_SEED);
    let k = hex(&c.exec("place", json!({"symbol": "coil", "at": [50, 50]})), "component");
    let before = c.log().len();
    for t in [json!(["plug"]), json!(5), json!({"side": "plug"}), json!(true)] {
        let r = c.call("execute", json!({"command": "setlocation", "params": {"component": k, "type": t.clone(), "value": "+CP2"}}));
        assert_eq!(r["isError"], json!(true), "type {t} must be refused: {r}");
        assert_eq!(c.log().len(), before, "type {t}: nothing written");
    }
}

/// Brief 052d over MCP: hostile proposal parameters are refused and write nothing; with writes off
/// the answers that write are refused, and the list still reads.
#[test]
fn hostile_proposals_and_answers_over_mcp() {
    let mut c = writer(WRITER_SEED);
    let k = hex(&c.exec("place", json!({"symbol": "coil", "at": [50, 150], "location": "keep", "function": "keep"})), "component");
    c.exec("setlocation", json!({"component": k, "value": "+CP2"}));
    c.exec("locationbox", json!({"location": "+CP1", "corners": [[20, 100], [200, 220]]}));
    let before = c.log().len();
    let long = format!("+{}", "Z".repeat(10_000));
    let many: Vec<Value> = (0..100_001).map(|_| json!(k)).collect();
    let mut accepted: Vec<String> = Vec::new();
    for (cmd, params) in [
        ("place", json!({"symbol": "coil", "at": [80, 150], "location": long})),
        ("place", json!({"symbol": "coil", "at": [80, 150], "location": {"x": 1}})),
        ("place", json!({"symbol": "coil", "at": [80, 150], "function": "=\u{0}"})),
        ("locationbox", json!({"location": "+CP3", "corners": [[20, 100], [200, 220]], "proposal": "explode"})),
        ("conflicts", json!({"answer": "move", "devices": many})),
        ("conflicts", json!({"answer": "move", "devices": ["zz"]})),
        ("conflicts", json!({"answer": "half"})),
        ("conflicts", json!({"answer": "yes"})),
    ] {
        let r = c.call("execute", json!({"command": cmd, "params": params.clone()}));
        let text = r["content"][0]["text"].as_str().unwrap_or_default().to_lowercase();
        assert!(!text.contains("internal error") && !text.contains("panick"), "{cmd}: {text}");
        if r["isError"] != json!(true) {
            accepted.push(format!(
                "{cmd} {} -> {}",
                params.to_string().chars().take(80).collect::<String>(),
                text.chars().take(300).collect::<String>()
            ));
        }
        assert!(text.len() < 2_000, "{cmd}: the answer echoes the hostile input");
    }
    assert!(accepted.is_empty(), "accepted: {accepted:?}");
    assert_eq!(c.log().len(), before, "nothing written");
    // The conflict made by the box is still answerable by the agent: one transaction, its author.
    c.exec("conflicts", json!({"answer": "move"}));
    let log = c.log();
    assert_eq!(log.len(), before + 1);
    assert_eq!(log.last().map(|t| t.author.clone()), Some(Author::Agent(AGENT.into())));
}

/// With writes off, `conflicts` reads, and Move and Half are refused.
#[test]
fn with_writes_off_conflict_answers_are_refused() {
    let mut c = crate::mcp_contracts::reader();
    let before = c.log().len();
    c.exec("conflicts", json!({}));
    for answer in ["move", "half"] {
        let m = c.refused("execute", json!({"command": "conflicts", "params": {"answer": answer}}));
        assert_eq!(m, ecad_mcp::policy::WRITES_OFF, "{answer}");
    }
    let m = c.refused("execute", json!({"command": "place", "params": {"symbol": "coil", "at": [0, 0], "location": "move"}}));
    assert_eq!(m, ecad_mcp::policy::WRITES_OFF);
    assert_eq!(c.log().len(), before);
}
