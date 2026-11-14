//! Tester 045 through MCP (brief 011 rule; briefs 045a AC 7, 045b AC 7; design 045 §13 R; FP1
//! condition 1 for merge): on a project saved at a path with spaces and non-ASCII letters, every
//! PLC write (`place` of a card body with rack and slot, `plcslot`, `setprop plc.*`, `setdefault
//! plc.*`, `place` of a cell with device and channel, `plcchannels`, `plcchannel`, `plcaddress`)
//! records one transaction by `Author::Agent(<client>)`; reopened with writes off each is refused
//! and records nothing, and `plcracks`, `checks` and `defaults {group: "PLC"}` read and record
//! nothing. The merge preview names a rack by its tag, never by its hex ID. Written from the briefs
//! and design before the implementation; ignored until phase 2.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::{Author, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, TempDir};

fn hex(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_else(|| panic!("no {key} in {v}")).to_string()
}

fn one_by_agent(c: &mut Client, before: usize, what: &str) -> Vec<Op> {
    let log = c.log();
    assert_eq!(log.len(), before + 1, "{what}: one transaction");
    let last = log.last().cloned().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()), "{what}: {}", last.label);
    last.ops
}

fn sets_key(ops: &[Op], key: &str) -> bool {
    ops.iter().any(|o| matches!(o, Op::SetProperty { key: k, .. } if k == key))
}

const KINDS_A: [&str; 4] = ["slot-twice", "rack-missing", "slot-range", "unracked"];
const KINDS_B: [&str; 7] = ["channel-twice", "channel-over", "channel-type", "address-overlap", "address-twice", "address-overflow", "no-address"];

/// The PLC reads; `b`: with 045b's Checks kinds too.
fn reads(c: &mut Client, b: bool) -> Value {
    let mut kinds: Vec<&str> = KINDS_A.to_vec();
    if b {
        kinds.extend(KINDS_B);
    }
    json!({
        "plcracks": c.exec("plcracks", json!({})),
        "checks": c.exec("checks", json!({"kinds": kinds})),
        "defaults": c.exec("defaults", json!({"group": "PLC"})),
    })
}

/// The writes of 045a, each checked as one agent transaction. Returns the IDs used.
struct Ids {
    rack: String,
    di: String,
    ai: String,
}

fn writes_a(c: &mut Client) -> Ids {
    let n = c.log().len();
    let rack = c.exec("place", json!({"symbol": "plccpu", "at": [100, 200]}));
    one_by_agent(c, n, "place plccpu");
    let rack = hex(&rack, "component");
    // Brief 051: the pack's 20 mm pitch.
    for x in [100, 120, 140, 160] {
        c.exec("wire", json!({"points": [[x, 450], [x, 350]]}));
    }
    let n = c.log().len();
    let di = c.exec("place", json!({"symbol": "plcdi", "at": [140, 200], "rack": rack, "slot": 2}));
    let ops = one_by_agent(c, n, "place card body with rack and slot");
    assert!(sets_key(&ops, "plc.rack") && sets_key(&ops, "plc.slot"), "{ops:?}");
    let di = hex(&di, "component");
    let n = c.log().len();
    let ai = hex(&c.exec("place", json!({"symbol": "plcai", "at": [180, 200]})), "component");
    one_by_agent(c, n, "place card body, defaults");
    let n = c.log().len();
    let v = c.exec("plcslot", json!({"card": ai, "slot": 5}));
    one_by_agent(c, n, "plcslot");
    assert!(v.to_string().contains("-A01"), "replies name the rack by tag: {v}");
    let n = c.log().len();
    c.exec("plcslot", json!({"card": ai, "slot": 2, "swap": true}));
    one_by_agent(c, n, "plcslot swap");
    let n = c.log().len();
    c.exec("setprop", json!({"component": di, "key": "plc.channels", "value": "8"}));
    one_by_agent(c, n, "setprop plc.channels");
    let n = c.log().len();
    c.exec("setdefault", json!({"key": "plc.slots", "level": "project", "value": 16}));
    one_by_agent(c, n, "setdefault plc.slots");
    Ids { rack, di, ai }
}

/// The writes of 045b, each checked as one agent transaction.
fn writes_b(c: &mut Client, ids: &Ids) {
    let (rack, di, ai) = (&ids.rack, &ids.di, &ids.ai);
    let n = c.log().len();
    let v = c.exec("plcchannels", json!({"card": di, "from": [90, 400], "to": [170, 400]}));
    let ops = one_by_agent(c, n, "plcchannels fence");
    assert!(ops.iter().filter(|o| matches!(o, Op::AddTerminal { .. })).count() >= 4, "{v}");
    let n = c.log().len();
    c.exec("plcchannels", json!({"card": di, "rest": true, "at": [300, 400]}));
    one_by_agent(c, n, "plcchannels rest");
    let n = c.log().len();
    c.exec("place", json!({"symbol": "chai", "at": [100, 300], "device": ai, "channel": 2}));
    one_by_agent(c, n, "place cell with device and channel");
    let n = c.log().len();
    c.exec("plcchannel", json!({"card": di, "channel": 1, "text": "Door closed", "address": "I9.9"}));
    let ops = one_by_agent(c, n, "plcchannel");
    assert!(sets_key(&ops, "plc.ch.1.text") && sets_key(&ops, "plc.ch.1.address"), "{ops:?}");
    let n = c.log().len();
    c.exec("plcchannel", json!({"card": ai, "channel": 2, "pins": {"+": "21"}}));
    one_by_agent(c, n, "plcchannel pins");
    for (what, p) in [
        ("plcaddress style", json!({"rack": rack, "style": "iec"})),
        ("plcaddress starts", json!({"rack": rack, "input_start": 10, "output_start": 20})),
        ("plcaddress fix", json!({"rack": rack, "fix": true})),
        ("plcaddress clear", json!({"rack": rack, "clear": true})),
        ("plcaddress card", json!({"card": ai, "start": 40, "size": 16})),
    ] {
        let n = c.log().len();
        c.exec("plcaddress", p);
        one_by_agent(c, n, what);
    }
    let n = c.log().len();
    c.exec("setdefault", json!({"key": "plc.address.style", "level": "object", "component": rack, "value": "slot"}));
    one_by_agent(c, n, "setdefault plc.address.style");
}

/// 045a AC 7: the a-writes by the agent; reopened with writes off they are refused, and
/// `plcracks`, `checks` and `defaults {group: "PLC"}` read and record nothing.
#[test]
fn plc_rack_writes_by_the_agent_then_refused_with_writes_off() {
    let dir = TempDir::new("t045a Schaltpl\u{e4}ne mit Leerzeichen");
    let path = dir.file("SPS -A01 (Rack).ecad");
    let (ids, before) = {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        let ids = writes_a(&mut c);
        let r = reads(&mut c, false);
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
        (ids, r)
    };
    let (s, _) = EcadMcp::open(&path, Options { write: false, agent: None }).unwrap();
    let mut r = Client::new(s);
    let log = r.log();
    assert_eq!(reads(&mut r, false), before, "the reads give what they gave before the save");
    let keys = r.exec("defaults", json!({"group": "PLC"}));
    for k in ["plc.slots", "plc.slot.first", "plc.card.caption"] {
        assert!(keys.to_string().contains(k), "{k}: {keys}");
    }
    for (cmd, p) in [
        ("place", json!({"symbol": "plcdq", "at": [220, 200], "rack": ids.rack, "slot": 3})),
        ("plcslot", json!({"card": ids.di, "slot": 7})),
        ("plcslot", json!({"card": ids.ai, "rack": null})),
        ("setprop", json!({"component": ids.di, "key": "plc.channels", "value": "16"})),
        ("setdefault", json!({"key": "plc.slots", "level": "project", "value": 8})),
    ] {
        let m = r.refused("execute", json!({"command": cmd, "params": p}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd}");
    }
    assert_eq!(r.log(), log, "nothing recorded with writes off");
}

#[test]
fn plc_writes_by_the_agent_then_refused_with_writes_off() {
    let dir = TempDir::new("t045 Schaltpl\u{e4}ne mit Leerzeichen");
    let path = dir.file("SPS -A01 (Rack).ecad");
    let (ids, before) = {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        let ids = writes_a(&mut c);
        writes_b(&mut c, &ids);
        let r = reads(&mut c, true);
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
        (ids, r)
    };
    let (s, _) = EcadMcp::open(&path, Options { write: false, agent: None }).unwrap();
    let mut r = Client::new(s);
    let log = r.log();
    assert_eq!(reads(&mut r, true), before, "the reads give what they gave before the save");
    let keys = r.exec("defaults", json!({"group": "PLC"}));
    for k in ["plc.slots", "plc.address.style", "plc.pins", "plc.spare.text"] {
        assert!(keys.to_string().contains(k), "{k}: {keys}");
    }
    for (cmd, p) in [
        ("place", json!({"symbol": "plcdq", "at": [220, 200], "rack": ids.rack, "slot": 3})),
        ("plcslot", json!({"card": ids.di, "slot": 7})),
        ("setprop", json!({"component": ids.di, "key": "plc.channels", "value": "16"})),
        ("setdefault", json!({"key": "plc.slots", "level": "project", "value": 8})),
        ("place", json!({"symbol": "chdi", "at": [300, 300], "device": ids.di, "channel": 7})),
        ("plcchannels", json!({"card": ids.di, "rest": true})),
        ("plcchannel", json!({"card": ids.di, "channel": 0, "text": "x"})),
        ("plcaddress", json!({"rack": ids.rack, "fix": true})),
        ("plcaddress", json!({"card": ids.ai, "start": null})),
    ] {
        let m = r.refused("execute", json!({"command": cmd, "params": p}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd}");
    }
    assert_eq!(r.log(), log, "nothing recorded with writes off");
}

/// Hostile PLC params through MCP: a JSON-RPC result with `isError`, never a panic, nothing written.
#[test]
fn hostile_plc_params_are_refused_and_record_nothing() {
    let mut c = Client::new(EcadMcp::in_memory("mcp t045", 0x7045_0001, Options { write: true, agent: None }).unwrap());
    let rack = hex(&c.exec("place", json!({"symbol": "plccpu", "at": [0, 0]})), "component");
    let card = hex(&c.exec("place", json!({"symbol": "plcdi", "at": [40, 0]})), "component");
    let n = c.log().len();
    for (cmd, p) in [
        ("plcslot", json!({"card": card, "slot": -1})),
        ("plcslot", json!({"card": card, "slot": "99999999999999999999"})),
        ("plcslot", json!({"card": rack, "slot": 1})),
        ("plcslot", json!({"card": "not hex"})),
        ("plcchannel", json!({"card": card, "channel": 999, "text": "x"})),
        ("plcchannel", json!({"card": card, "channel": 0, "text": "x".repeat(10_000)})),
        ("plcaddress", json!({"card": card, "start": 70000})),
        ("plcaddress", json!({"rack": rack, "style": "vendor"})),
        ("plcchannels", json!({"card": card, "from": [0, 0]})),
        ("setprop", json!({"component": card, "key": "plc.slot", "value": "1000"})),
    ] {
        let r = c.call("execute", json!({"command": cmd, "params": p}));
        assert_eq!(r["isError"], json!(true), "{cmd} {p}: {r}");
    }
    assert_eq!(c.log().len(), n, "a refusal records nothing");
}

/// FP1 condition 1 (merge and diff): two branches move one card to two different racks; the merge
/// preview's changes and its conflict name the racks by tag, never by hex ID; the merge records
/// `plc.rack` as an ID and the card ends in the chosen rack.
#[test]
fn fp1_merge_preview_names_racks_by_tag() {
    let dir = TempDir::new("t045 merge");
    let path = dir.file("merge plc.ecad");
    let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
    let a01 = hex(&c.exec("place", json!({"symbol": "plccpu", "at": [0, 0]})), "component");
    let a20 = hex(&c.exec("place", json!({"symbol": "plcim", "at": [200, 0], "tag": "-A20"})), "component");
    let a30 = hex(&c.exec("place", json!({"symbol": "plcim", "at": [400, 0], "tag": "-A30"})), "component");
    let card = hex(&c.exec("place", json!({"symbol": "plcdi", "at": [40, 0], "rack": a01, "slot": 1})), "component");
    let other = hex(&c.exec("place", json!({"symbol": "plcdq", "at": [80, 0], "rack": a01, "slot": 2})), "component");
    c.ok("save", json!({}));
    c.exec("branch", json!({"name": "Option R"}));
    c.exec("plcslot", json!({"card": card, "rack": a20, "slot": 1}));
    c.exec("plcslot", json!({"card": other, "rack": a30, "slot": 4}));
    c.ok("save", json!({}));
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
    c.exec("plcslot", json!({"card": card, "rack": a30, "slot": 1}));
    let pv = c.exec("merge", json!({"from": "Option R", "preview": true}));
    let mut shown = pv.clone();
    if let Some(o) = shown.as_object_mut() {
        o.remove("ancestor_point");
    }
    let text = shown.to_string().to_lowercase();
    for (t, id) in [("-A01", &a01), ("-A20", &a20), ("-A30", &a30)] {
        assert!(!text.contains(&id.to_lowercase()), "the merge preview shows {t}'s hex ID: {pv}");
    }
    let conflicts = pv["conflicts"].as_array().cloned().unwrap_or_default();
    let k =
        conflicts.iter().find(|k| k["id"].as_str().is_some_and(|i| i.contains("plc.rack"))).unwrap_or_else(|| panic!("a plc.rack conflict: {pv}"));
    assert!(k["main"].as_str().is_some_and(|m| m.contains("-A30")), "{k}");
    assert!(k["branch"].as_str().is_some_and(|b| b.contains("-A20")), "{k}");
    let changes = pv["changes"].to_string();
    assert!(changes.contains("-A30") || changes.contains("-A02"), "the other card's move names its rack by tag: {changes}");
    // Choose the branch's rack: the stored value is the ID again.
    let id = hex(k, "id");
    c.exec("merge", json!({"from": "Option R", "choices": {id: "branch"}}));
    let d = c.doc();
    let cid = ecad_model::ComponentId::parse(&card).unwrap();
    assert_eq!(d.components[&cid].properties.get("plc.rack").map(String::as_str), Some(a20.as_str()));
}
