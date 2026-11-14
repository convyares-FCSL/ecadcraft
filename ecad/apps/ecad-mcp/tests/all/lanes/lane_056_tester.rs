//! Tester 056 through MCP (brief 056 AC 10, design 056 §10, §17; brief 011 rule): on a project
//! saved at a path with spaces and non-ASCII letters, `plcchannel {tag, safe}`, `plcmaketags`,
//! `plcimport` and `plctake` record one transaction by `Author::Agent(<client>)`; reopened with
//! writes off each is refused with `WRITES_OFF` and records nothing, while `plcimport {dry_run}`,
//! `plcexport`, `plcsync` and `iolist` work and record nothing. Two branches that both imported a
//! table merge with one whole-value conflict on the snapshot (FP2). Written from the brief and
//! design before the implementation; ignored until phase 2.

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

/// A CPU, a DQ card in slot 1 with channels 0–3 drawn to four lamps.
fn build(c: &mut Client) -> String {
    c.exec("place", json!({"symbol": "plccpu", "at": [50, 400]}));
    let dq = hex(&c.exec("place", json!({"symbol": "plcdq", "at": [100, 400]})), "component");
    for i in 0..4 {
        let x = 100 + 20 * i;
        c.exec("place", json!({"symbol": "lamp", "at": [x, 260]}));
        c.exec("wire", json!({"points": [[x, 250], [x, 150]]}));
    }
    c.exec("plcchannels", json!({"card": dq, "from": [95, 200], "to": [175, 200]}));
    dq
}

fn address(c: &mut Client, ch: u64) -> String {
    let v = c.exec("iolist", json!({"card": "-A02", "limit": 100}));
    let r = v["rows"].as_array().and_then(|r| r.iter().find(|r| r["channel"] == json!(ch)).cloned()).unwrap_or_default();
    format!("%{}", r["address"].as_str().unwrap_or_default().trim_start_matches('%'))
}

fn reads(c: &mut Client) -> Value {
    json!({
        "iolist": c.exec("iolist", json!({"limit": 100})),
        "plcsync": c.exec("plcsync", json!({})),
        "plcexport": c.exec("plcexport", json!({})),
        "dry": c.exec("plcimport", json!({"text": "name,address,type,comment\nX,%I8.0,Bool,\n", "dry_run": true})),
    })
}

#[test]
#[ignore = "tester 056: awaiting build"]
fn plc_signal_writes_by_the_agent_then_refused_with_writes_off() {
    let dir = TempDir::new("t056 Schaltpl\u{e4}ne mit Leerzeichen");
    let path = dir.file("SPS Signale (Rack).ecad");
    let (dq, before) = {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        let dq = build(&mut c);
        let n = c.log().len();
        c.exec("plcchannel", json!({"card": dq, "channel": 0, "tag": "Fan_On", "safe": "hold"}));
        let ops = one_by_agent(&mut c, n, "plcchannel tag + safe");
        assert!(sets_key(&ops, "plc.ch.0.tag") && sets_key(&ops, "plc.ch.0.safe"), "{ops:?}");
        for ch in 1..4 {
            c.exec("plcchannel", json!({"card": dq, "channel": ch, "text": format!("Lamp {ch} on")}));
        }
        let n = c.log().len();
        c.exec("plcmaketags", json!({"card": dq}));
        one_by_agent(&mut c, n, "plcmaketags");
        let (a0, a1) = (address(&mut c, 0), address(&mut c, 1));
        let table = format!("name,address,type,comment\nFan_On,{a0},Bool,\nLamp_A,{a1},Bool,\nDoor_Sw,%I8.0,Bool,\n");
        let n = c.log().len();
        let dry = c.exec("plcimport", json!({"text": table, "dry_run": true}));
        assert_eq!(c.log().len(), n, "a dry run records nothing: {dry}");
        c.exec("plcimport", json!({"text": table}));
        let ops = one_by_agent(&mut c, n, "plcimport");
        assert!(sets_key(&ops, "plc.sync.table"), "{ops:?}");
        assert_eq!(c.log().last().map(|t| t.label.clone()).as_deref(), Some("Import PLC tags"));
        let n = c.log().len();
        c.exec("plctake", json!({"items": [{"card": dq, "channel": 1}]}));
        let ops = one_by_agent(&mut c, n, "plctake");
        assert!(sets_key(&ops, "plc.ch.1.tag"), "{ops:?}");
        let r = reads(&mut c);
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
        (dq, r)
    };
    let (s, _) = EcadMcp::open(&path, Options { write: false, agent: None }).unwrap();
    let mut r = Client::new(s);
    let log = r.log();
    assert_eq!(reads(&mut r), before, "reads work with writes off and give what they gave before the save");
    for (cmd, p) in [
        ("plcchannel", json!({"card": dq, "channel": 2, "tag": "X"})),
        ("plcchannel", json!({"card": dq, "channel": 2, "safe": "on"})),
        ("plcmaketags", json!({"card": dq})),
        ("plcimport", json!({"text": "name,address,type,comment\nX,%I8.0,Bool,\n"})),
        ("plctake", json!({"all": true})),
    ] {
        let m = r.refused("execute", json!({"command": cmd, "params": p}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd}");
    }
    assert_eq!(r.log(), log, "nothing recorded with writes off");
}

/// Hostile params through MCP: `isError`, never a panic, nothing written.
#[test]
#[ignore = "tester 056: awaiting build"]
fn hostile_plc_signal_params_are_refused_and_record_nothing() {
    let mut c = Client::new(EcadMcp::in_memory("mcp t056", 0x7056_0001, Options { write: true, agent: None }).unwrap());
    let dq = build(&mut c);
    let n = c.log().len();
    for (cmd, p) in [
        ("plcchannel", json!({"card": dq, "channel": 0, "tag": "x".repeat(10_000)})),
        ("plcchannel", json!({"card": dq, "channel": 0, "safe": "x".repeat(10_000)})),
        ("plcchannel", json!({"card": dq, "channel": 0, "safe": 42})),
        ("plcchannel", json!({"card": dq, "channel": 999, "tag": "X"})),
        ("plcmaketags", json!({"card": "not hex"})),
        ("plcmaketags", json!({})),
        ("plcimport", json!({})),
        ("plcimport", json!({"text": "PK\u{3}\u{4}"})),
        ("plcimport", json!({"text": "name,address,type,comment\n", "format": "xlsx"})),
        ("plcimport", json!({"path": "Z:\\no such dir\\tags.csv"})),
        ("plctake", json!({"items": [{"card": dq, "channel": -1}]})),
        ("plctake", json!({"items": "all"})),
        ("plcsync", json!({"state": "broken"})),
        ("plcexport", json!({"format": "tia"})),
    ] {
        let r = c.call("execute", json!({"command": cmd, "params": p}));
        assert_eq!(r["isError"], json!(true), "{cmd} {p}: {r}");
    }
    assert_eq!(c.log().len(), n, "a refusal records nothing");
}

/// FP2: two branches that each imported a table merge with one conflict on the snapshot, named in
/// words; choosing a side keeps that side's table whole.
#[test]
#[ignore = "tester 056: awaiting build"]
fn fp2_two_imports_merge_as_one_whole_value() {
    let dir = TempDir::new("t056 merge");
    let path = dir.file("merge plc tags.ecad");
    let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
    build(&mut c);
    let a0 = address(&mut c, 0);
    c.ok("save", json!({}));
    c.exec("branch", json!({"name": "PLC B"}));
    let theirs = format!("name,address,type,comment\nFrom_B,{a0},Bool,\nB2,%I8.0,Bool,\n");
    c.exec("plcimport", json!({"text": theirs}));
    c.ok("save", json!({}));
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
    let ours = format!("name,address,type,comment\nFrom_A,{a0},Bool,\n");
    c.exec("plcimport", json!({"text": ours}));
    let pv = c.exec("merge", json!({"from": "PLC B", "preview": true}));
    let conflicts = pv["conflicts"].as_array().cloned().unwrap_or_default();
    let k: Vec<&Value> = conflicts.iter().filter(|k| k.to_string().contains("plc.sync")).collect();
    assert_eq!(k.len(), 1, "one conflict for the snapshot, not one per row or per key: {pv}");
    let id = hex(k[0], "id");
    c.exec("merge", json!({"from": "PLC B", "choices": {id: "branch"}}));
    let d = c.doc();
    let table = d.properties.get("plc.sync.table").cloned().unwrap_or_default();
    assert!(table.contains("From_B") && table.contains("B2") && !table.contains("From_A"), "the branch's table whole: {table}");
}
