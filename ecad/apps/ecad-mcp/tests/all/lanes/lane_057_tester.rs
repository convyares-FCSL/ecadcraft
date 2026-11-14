//! Tester 057 through MCP (brief 057 AC 4, design 057 §8.3, §11 check 17; brief 011 rule): on a
//! project saved at a path with spaces and non-ASCII letters, `rating` and `ratingtable {csv}`
//! record one transaction by `Author::Agent(<client>)` and one undo restores; `ratingtable
//! {import}` is refused over MCP (it reads a file); reopened with writes off the writes are refused
//! and record nothing while `ratings` works and reads what it read before. Every table value is
//! made up (owner, Q11). Written from the brief and design before the implementation.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::{Author, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, TempDir};

/// Made-up rating rows; not from any standard.
const SYNTH: &str = "# made-up values for ECAD tests; not from any standard\ncapacity,E,pvc,3,1.5,21.0\ngrouping,E,4,0.71\nresistance,5,1.5,11.0\n";

fn s(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_else(|| panic!("no {key} in {v}")).to_string()
}

fn one_by_agent(c: &mut Client, before: usize, what: &str) -> Vec<Op> {
    let log = c.log();
    assert_eq!(log.len(), before + 1, "{what}: one transaction");
    let last = log.last().cloned().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()), "{what}: {}", last.label);
    last.ops
}

/// -Q01 (4 A) feeding a lamp; cable -W01 across the wire; returns the wire's conductor ID.
fn build(c: &mut Client) -> String {
    let q = s(&c.exec("place", json!({"symbol": "mcb1", "at": [100, 300]})), "component");
    c.exec("setprop", json!({"component": q, "key": "rating", "value": "4 A"}));
    c.exec("place", json!({"symbol": "lamp", "at": [100, 200]}));
    let w = s(&c.exec("wire", json!({"points": [[100, 290], [100, 210]]})), "conductor");
    let cab = s(&c.exec("cableslash", json!({"from": [95, 250], "to": [105, 250], "tag": "-W01"})), "cable");
    c.exec("setprop", json!({"cable": cab, "key": "type", "value": "H07RN-F 4G1.5"}));
    c.exec("setprop", json!({"cable": cab, "key": "length", "value": "12 m"}));
    w
}

fn reads(c: &mut Client) -> Value {
    json!({
        "all": c.exec("ratings", json!({})),
        "cable": c.exec("ratings", json!({"cable": "-W01"})),
        "checks": c.exec("checks", json!({})),
    })
}

#[test]
#[ignore = "tester 057: awaiting build"]
fn rating_writes_by_the_agent_then_refused_with_writes_off() {
    let dir = TempDir::new("t057 Leitungspr\u{fc}fung mit Leerzeichen");
    let path = dir.file("Ratings (Synth).ecad");
    let (w, before) = {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        let w = build(&mut c);
        let n = c.log().len();
        let t = c.exec("ratingtable", json!({"name": "Synthetic", "source": "made up for tests", "csv": SYNTH}));
        let ops = one_by_agent(&mut c, n, "ratingtable csv");
        assert!(ops.iter().all(|o| o.name() == "SetStyle"), "{ops:?}");
        let id = ["id", "style"].iter().filter_map(|k| t[*k].as_str()).next().unwrap_or_default().to_string();
        c.exec("setdefault", json!({"key": "rating.table", "level": "project", "value": id}));
        let n = c.log().len();
        c.exec("rating", json!({"cable": "-W01", "basis": "table", "method": "E", "ambient": 37, "grouped": 3, "load": "3 A"}));
        let ops = one_by_agent(&mut c, n, "rating cable");
        assert!(ops.iter().all(|o| matches!(o, Op::SetProperty { .. })), "only SetProperty: {ops:?}");
        let n = c.log().len();
        c.exec("rating", json!({"wire": w, "basis": "manual", "manual": "10 A", "bundling": 0.85}));
        one_by_agent(&mut c, n, "rating wire");
        // One undo restores the wire's rating keys.
        let d0 = c.doc();
        c.exec("rating", json!({"wire": w, "manual": "12 A"}));
        c.exec("undo", json!({}));
        assert!(c.doc() == d0, "one undo restores");
        // A file path is refused over MCP; the CSV form is the agent's way.
        let n = c.log().len();
        let r = c.call("execute", json!({"command": "ratingtable", "params": {"name": "X", "import": dir.file("x.csv")}}));
        assert_eq!(r["isError"], json!(true), "{r}");
        assert_eq!(c.log().len(), n);
        let r = reads(&mut c);
        assert!(r["cable"].to_string().contains("Synthetic"), "names the table: {r}");
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
        (w, r)
    };
    let (sx, _) = EcadMcp::open(&path, Options { write: false, agent: None }).unwrap();
    let mut r = Client::new(sx);
    let log = r.log();
    assert_eq!(reads(&mut r), before, "ratings reads with writes off, as before the save");
    for (cmd, p) in [
        ("rating", json!({"wire": w, "manual": "11 A"})),
        ("rating", json!({"cable": "-W01", "ambient": 40})),
        ("ratingtable", json!({"name": "Other", "source": "x", "csv": SYNTH})),
    ] {
        let m = r.refused("execute", json!({"command": cmd, "params": p}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd}");
    }
    assert_eq!(r.log(), log, "nothing recorded with writes off");
}

/// Hostile params: `isError`, never a panic, nothing written.
#[test]
#[ignore = "tester 057: awaiting build"]
fn hostile_rating_params_are_refused_and_record_nothing() {
    let mut c = Client::new(EcadMcp::in_memory("mcp t057", 0x7057_0001, Options { write: true, agent: None }).unwrap());
    let w = build(&mut c);
    let n = c.log().len();
    for (cmd, p) in [
        ("rating", json!({"wire": w, "manual": "1e400"})),
        ("rating", json!({"wire": w, "manual": "NaN"})),
        ("rating", json!({"wire": w, "load": -1})),
        ("rating", json!({"wire": w, "bundling": "x".repeat(10_000)})),
        ("rating", json!({"wire": "not a wire"})),
        ("rating", json!({"cable": "-W01", "ambient": 1e308})),
        ("rating", json!({"cable": "-W01", "method": "Z9"})),
        ("rating", json!({"cable": "-W01", "protected_by": "-Q99"})),
        ("rating", json!({})),
        ("ratingtable", json!({"name": "Bad", "source": "x", "csv": "capacity,Q,pvc,3,1.5,21.0\n"})),
        ("ratingtable", json!({"name": "Bad", "source": "x", "csv": "x".repeat(300 * 1024)})),
        ("ratingtable", json!({"name": "x".repeat(65), "source": "x", "csv": SYNTH})),
        ("ratingtable", json!({"name": "Bad", "export": "Z:\\no such dir\\t.csv"})),
    ] {
        let r = c.call("execute", json!({"command": cmd, "params": p}));
        assert_eq!(r["isError"], json!(true), "{cmd}: {r}");
    }
    assert_eq!(c.log().len(), n, "a refusal records nothing");
    let _ = c.exec("ratings", json!({}));
}
