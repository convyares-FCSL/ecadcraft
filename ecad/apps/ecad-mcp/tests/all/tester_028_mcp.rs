//! Tester 028 through MCP (brief 011 rule, design 028 §10): on a saved project with a strip, a
//! bridge, offers and cores, at a path with spaces and non-ASCII letters, every strip write records
//! `Author::Agent`; reopened with writes off, every write with real IDs is refused and every read
//! (`stripplan`, `cablecores`, `cablelist`, `checks`, a renumber preview) works, gives what it gave
//! before the save, and records nothing.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::{Author, ObjectRef, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, TempDir};

fn hex(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap().to_string()
}

fn by_agent(c: &mut Client) -> (String, Vec<Op>) {
    let last = c.log().last().cloned().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()), "{}", last.label);
    (last.label, last.ops)
}

fn reads(c: &mut Client, cable: &str) -> Value {
    // `change_count` is the session's edit counter (026), not part of the plan.
    let mut preview = c.exec("renumber", json!({"kind": "terminals", "preview": true, "start": 3}));
    preview.as_object_mut().map(|o| o.remove("change_count"));
    json!({
        "stripplan": c.exec("stripplan", json!({})),
        "cablecores": c.exec("cablecores", json!({"cable": cable})),
        "cablelist": c.exec("cablelist", json!({})),
        "checks": c.exec("checks", json!({"kinds": ["too-many-terminals", "terminal-twice", "too-many-cores", "core-twice", "no-core-count"]})),
        "preview": preview,
    })
}

#[test]
fn strip_writes_by_the_agent_then_refused_with_writes_off_on_the_saved_project() {
    let dir = TempDir::new("t028 Schaltpl\u{e4}ne mit Leerzeichen");
    let path = dir.file("Klemmen -X01 (A).ecad");
    let (strip, cable, terms, before) = {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        for x in [100, 120, 140, 160] {
            c.exec("wire", json!({"points": [[x, 200], [x, 100]]}));
        }
        let cable = hex(&c.exec("cableslash", json!({"from": [90, 120], "to": [170, 120], "tag": "-W01"})), "cable");
        let f = c.exec("terminals", json!({"from": [90, 170], "to": [170, 170]}));
        assert_eq!(by_agent(&mut c).0, "Place terminals");
        let strip = hex(&f, "strip");
        let terms: Vec<String> = f["terminals"].as_array().unwrap().iter().map(|x| hex(x, "terminal")).collect();
        assert_eq!(terms.len(), 4, "{f}");
        c.exec("bridge", json!({"from": terms[2], "to": terms[3]}));
        assert!(by_agent(&mut c).0.starts_with("Bridge -X01:3"));
        // PE keys of the BOM (FP4) by the agent.
        for (k, v) in [("part_number.pe", "PE-4"), ("maker.pe", "Acme")] {
            c.exec("setprop", json!({"component": strip, "key": k, "value": v}));
            let (_, ops) = by_agent(&mut c);
            assert!(
                matches!(&ops[..], [Op::SetProperty { object: ObjectRef::Component(_), key, after: Some(a), .. }] if key == k && a == v),
                "{k}: {ops:?}"
            );
        }
        c.exec("setprop", json!({"component": strip, "key": "terminals", "value": "6+1PE"}));
        by_agent(&mut c);
        c.exec("setprop", json!({"cable": cable, "key": "cores", "value": "3G"}));
        by_agent(&mut c);
        // An over core (4 members, 3 offered) is a Checks row, not a refusal.
        let ck = c.exec("checks", json!({"kinds": ["too-many-cores"]}));
        assert_eq!(ck["checks"].as_array().map(Vec::len), Some(1), "{ck}");
        c.exec("newpage", json!({"generated": "cables"}));
        assert_eq!(by_agent(&mut c).0, "New page");
        // `u` through MCP takes back the agent's page; redo puts it back.
        let n = c.log().len();
        c.exec("u", Value::Null);
        c.exec("redo", Value::Null);
        assert_eq!(c.log().len(), n + 2);
        let before = reads(&mut c, &cable);
        assert_eq!(c.log().len(), n + 2, "reads record nothing");
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
        (strip, cable, terms, before)
    };
    let (s, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut c = Client::new(s);
    assert!(!c.s.writes());
    let log = c.log();
    for (cmd, params) in [
        ("place", json!({"symbol": "terminal", "at": [100, 150]})),
        ("place", json!({"symbol": "terminalpe", "at": [300, 150], "device": strip})),
        ("terminals", json!({"strip": strip, "from": [90, 150], "to": [170, 150]})),
        ("bridge", json!({"from": terms[0], "to": terms[1]})),
        ("renumber", json!({"kind": "terminals", "start": 9})),
        ("renumber", json!({"kind": "terminals", "scope": "selection", "strips": [strip]})),
        ("setprop", json!({"component": strip, "key": "terminals", "value": "12"})),
        ("setprop", json!({"component": strip, "key": "part_number.pe", "value": "X"})),
        ("setprop", json!({"component": strip, "key": "maker.pe", "value": "X"})),
        ("setprop", json!({"cable": cable, "key": "cores", "value": "7G"})),
        ("cablecore", json!({"cable": cable, "cores": ["1", "2", "3", "GNYE"]})),
        ("newpage", json!({"generated": "terminals", "strip": strip})),
        ("newpage", json!({"generated": "cables"})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd} {params}");
    }
    let after = reads(&mut c, &cable);
    for k in ["stripplan", "cablecores", "cablelist", "checks", "preview"] {
        assert_eq!(after[k], before[k], "{k} after reopen with writes off");
    }
    assert_eq!(after["stripplan"]["strips"][0]["offered"], "6+1PE");
    assert_eq!(c.log(), log, "nothing recorded with writes off");
}
