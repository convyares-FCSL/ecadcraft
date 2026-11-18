//! Tester 059a through MCP (design 059 §4.6, §11; the owner's rules 7 and 8): hostile and
//! oversized parameters to `cablenew`, `cableedit`, `cableremove` and `cableslash` are refused in
//! bounded words with nothing recorded; a session of cable-manager edits, undone and redone, saves,
//! reopens and replays to the same document.
//!
//! Every value here is synthetic.

use ecad_mcp::{EcadMcp, MAX_REQUEST_BYTES, Options};
use ecad_model::Document;
use serde_json::{Value, json};

use crate::mcp_contracts::{Client, TempDir};

fn writer(dir: &TempDir, name: &str) -> (Client, String) {
    let path = dir.file(name);
    (Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap()), path)
}

/// Why `m` is not plain, bounded words, if it is not.
fn unbounded(m: &str, what: &str) -> Option<String> {
    if m.trim().is_empty() {
        return Some(format!("{what}: empty words"));
    }
    if m.chars().count() > 400 {
        return Some(format!("{what}: {} chars of words: {:.120}", m.chars().count(), m));
    }
    if m.contains("panicked") {
        return Some(format!("{what}: {m}"));
    }
    None
}

#[test]
fn t1_hostile_and_oversized_parameters_are_refused_in_bounded_words_recording_nothing() {
    let dir = TempDir::new("tester059a");
    let (mut c, _) = writer(&dir, "Hostile 059.ecad");
    for x in [0, 10] {
        c.exec("wire", json!({"points": [[x, 0], [x, 50]]}));
    }
    let k = c.exec("cablenew", json!({"cores": "3G"}))["cable"].as_str().unwrap().to_string();
    let big = "x".repeat(200_000);
    let mut long_key = serde_json::Map::new();
    long_key.insert(big.clone(), json!(1));
    let long_key = Value::Object(long_key);
    let cases: Vec<(&str, Value)> = vec![
        ("cablenew", json!({"colour": 1})),
        ("cablenew", long_key.clone()),
        ("cablenew", json!({"tag": big.clone()})),
        ("cablenew", json!({"maker": big.clone()})),
        ("cablenew", json!({"cores": big.clone()})),
        ("cablenew", json!({"length": "1e400"})),
        ("cablenew", json!({"length": -3})),
        ("cablenew", json!({"shield": "lead"})),
        ("cablenew", json!({"cable_type": big.clone()})),
        ("cablenew", json!({"from": {"gland": big.clone()}})),
        ("cablenew", json!({"to": {"location": big.clone()}})),
        ("cablenew", json!({"to": {"nope": 1}})),
        ("cableedit", json!({"cable": k, "set": long_key.clone()})),
        ("cableedit", json!({"cable": big.clone(), "set": {"length": 1}})),
        ("cableedit", json!({"cable": k, "set": {"length": "x"}})),
        ("cableedit", json!({"cable": k, "set": {"cores": big.clone()}})),
        ("cableedit", json!({"cable": k})),
        ("cableedit", json!({"cable": k, "set": {}, "extra": true})),
        ("cableremove", json!({"cable": big.clone()})),
        ("cableremove", json!({"cable": k, "drawn": "unassign"})),
        ("cableremove", long_key.clone()),
        ("cableslash", json!({"from": [-5, 25], "to": [1e308, 25], "cable": k})),
        ("cableslash", json!({"from": [-5, 25], "to": [15, 25], "cable": big.clone()})),
    ];
    let mut problems = Vec::new();
    for (cmd, params) in cases {
        let n = c.log().len();
        let before: Document = c.doc();
        let r = c.call("execute", json!({"command": cmd, "params": params}));
        let what = format!("{cmd} {:.60}", params.to_string());
        if r["isError"] != json!(true) {
            problems.push(format!("{what}: accepted: {:.200}", r.to_string()));
            continue;
        }
        let m = r["content"][0]["text"].as_str().unwrap_or_default().to_string();
        problems.extend(unbounded(&m, &what));
        assert_eq!(c.log().len(), n, "{what}: nothing recorded");
        assert!(c.doc() == before, "{what}: document unchanged");
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn t2_a_request_over_the_size_limit_is_refused_before_it_reaches_the_command() {
    let dir = TempDir::new("tester059b");
    let (mut c, _) = writer(&dir, "Big 059.ecad");
    let n = c.log().len();
    let big = "x".repeat(MAX_REQUEST_BYTES + 16);
    let line = json!({"jsonrpc": "2.0", "id": 99, "method": "tools/call",
        "params": {"name": "execute", "arguments": {"command": "cablenew", "params": {"maker": big}}}})
    .to_string();
    let reply = c.s.handle_line(&line).unwrap_or_default();
    assert!(reply.len() < 2000, "the reply is bounded: {} bytes", reply.len());
    assert!(reply.contains("error") || reply.contains("isError"), "{reply:.300}");
    assert_eq!(c.log().len(), n, "nothing recorded");
}

#[test]
fn t3_cable_manager_edits_undone_and_redone_save_reopen_and_replay_the_same() {
    let dir = TempDir::new("tester059c");
    let (mut c, path) = writer(&dir, "Round 059.ecad");
    for x in [0, 10, 20] {
        c.exec("wire", json!({"points": [[x, 0], [x, 50]]}));
    }
    let ty = c.exec("setstyle", json!({"catalogue": "cable", "new": true, "name": "Synth Round", "fields": {"cores": "4G", "size": "1.5", "maker": "Synthmaker", "gland": "M20"}}));
    let a = c.exec(
        "cablenew",
        json!({"cable_type": ty["id"], "length": 12, "shield": "overall", "part_number": "SYN-R4",
               "from": {"location": "+CAB1", "function": "=PUMP", "gland": "M25"}, "to": {"location": "+FIELD", "function": "=PUMP"}}),
    )["cable"]
        .as_str()
        .unwrap()
        .to_string();
    let b = c.exec("cablenew", json!({"tag": "-W05", "cores": "BN,BK"}))["cable"].as_str().unwrap().to_string();
    let spare = c.exec("cablenew", json!({}))["cable"].as_str().unwrap().to_string();
    c.exec("cableedit", json!({"cable": a, "set": {"maker": "Othermaker", "to": {"gland": "M16"}}}));
    c.exec("cableslash", json!({"from": [-5, 25], "to": [25, 25], "cable": a}));
    c.exec("cableremove", json!({"cable": spare}));
    c.exec("u", json!({}));
    c.exec("u", json!({}));
    c.exec("redo", json!({}));
    c.exec("cableedit", json!({"cable": b, "set": {"length": "3.5", "description": "Synthetic"}}));
    let live = c.doc();
    let log = c.log();
    assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
    drop(c);
    let (sess, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut r = Client::new(sess);
    assert!(r.doc() == live, "the reopened document equals the saved one");
    assert_eq!(r.log().len(), log.len(), "the history round-trips");
    let replayed = r.s.with_session(|s, p| p.read(s, |p, _| p.ledger.replay_matches().unwrap()).unwrap()).unwrap();
    assert!(replayed, "replay of the reopened ledger equals the document");
    // The not-drawn cable stays not drawn, the assigned one drawn, the removed one present (undone).
    let doc = r.doc();
    let tags: Vec<String> = doc.cables.values().filter_map(|x| x.properties.get("tag").cloned()).collect();
    assert_eq!(doc.cables.len(), 3, "{tags:?}");
    let ka = ecad_model::CableId::parse(&a).unwrap();
    assert_eq!(doc.cables[&ka].members.len(), 3);
    assert!(doc.cables[&ka].properties.contains_key("end.from.location"));
}

/// 059 full over MCP: + Cable, edits (glands from the type, overridden per end) and Remove are each
/// one agent transaction, undone and redone exactly; a drawn cable's Remove writes nothing.
#[test]
fn t4_cable_manager_actions_are_one_transaction_each_with_exact_undo_and_redo() {
    let dir = TempDir::new("tester059d");
    let (mut c, _) = writer(&dir, "Actions 059.ecad");
    for x in [0, 10] {
        c.exec("wire", json!({"points": [[x, 0], [x, 50]]}));
    }
    let ty = c.exec("setstyle", json!({"catalogue": "cable", "new": true, "name": "Synth MCP", "fields": {"cores": "3G", "gland": "M20"}}));
    let exact = |c: &mut Client, cmd: &str, params: Value, label: &str| -> Value {
        let before = c.doc();
        let n = c.log().len();
        let v = c.exec(cmd, params.clone());
        let log = c.log();
        assert_eq!(log.len(), n + 1, "{cmd}: one transaction");
        assert_eq!(log.last().unwrap().label, label);
        assert_eq!(log.last().unwrap().author, ecad_model::Author::Agent(crate::mcp_contracts::AGENT.into()));
        let after = c.doc();
        c.exec("u", json!({}));
        assert!(c.doc() == before, "{cmd}: undo exact");
        c.exec("redo", json!({}));
        assert!(c.doc() == after, "{cmd}: redo exact");
        v
    };
    let k = exact(&mut c, "cablenew", json!({"cable_type": ty["id"]}), "Create cable")["cable"].as_str().unwrap().to_string();
    exact(&mut c, "cableedit", json!({"cable": k, "set": {"to": {"gland": "M25"}, "length": 4}}), "Edit cable");
    let doc = c.doc();
    let kid = ecad_model::CableId::parse(&k).unwrap();
    assert_eq!(ecad_cadcraft::cable_cmds::gland(&doc, kid, ecad_cadcraft::cable_cmds::Side::From).map(|g| g.0).as_deref(), Some("M20"));
    assert_eq!(ecad_cadcraft::cable_cmds::gland(&doc, kid, ecad_cadcraft::cable_cmds::Side::To).map(|g| g.0).as_deref(), Some("M25"));
    let spare = exact(&mut c, "cablenew", json!({}), "Create cable")["cable"].as_str().unwrap().to_string();
    exact(&mut c, "cableremove", json!({"cable": spare}), "Remove cable");
    exact(&mut c, "cableslash", json!({"from": [-5, 25], "to": [15, 25], "cable": k}), "Cable slash");
    let n = c.log().len();
    let m = c.refused("execute", json!({"command": "cableremove", "params": {"cable": k}}));
    assert!(m.contains("is drawn on"), "{m}");
    assert_eq!(c.log().len(), n, "nothing recorded");
    let m = c.refused("execute", json!({"command": "cableedit", "params": {"cable": k, "set": {"from": {"gland": "M\u{202e}20"}}}}));
    assert!(unbounded(&m, "gland").is_none(), "{m}");
    assert_eq!(c.log().len(), n);
}
