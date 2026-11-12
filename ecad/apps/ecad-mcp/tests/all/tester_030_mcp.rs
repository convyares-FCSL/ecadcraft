//! Tester 030 through MCP (brief 011 rule, brief 030 AC 6, design 030 §10): on a project saved at
//! a path with spaces and non-ASCII letters, every accessory and jumper write records one
//! transaction by `Author::Agent(<client>)`; reopened with writes off, every write with real IDs
//! is refused and `stripplan`, `bom`, `checks` and the renumber preview give what they gave before
//! the save and record nothing. Written from the brief and design before the implementation.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::{Author, ObjectRef, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, TempDir};

fn hex(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap().to_string()
}

/// The command added exactly one transaction, by the agent; its label and ops.
fn one_by_agent(c: &mut Client, before: usize, what: &str) -> (String, Vec<Op>) {
    let log = c.log();
    assert_eq!(log.len(), before + 1, "{what}: one transaction");
    let last = log.last().cloned().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()), "{what}: {}", last.label);
    (last.label, last.ops)
}

fn sets_key(ops: &[Op], key: &str) -> bool {
    ops.iter().any(|o| matches!(o, Op::SetProperty { key: k, .. } if k == key))
}

fn reads(c: &mut Client) -> Value {
    let mut preview = c.exec("renumber", json!({"kind": "terminals", "preview": true, "start": 5}));
    preview.as_object_mut().map(|o| o.remove("change_count"));
    json!({
        "stripplan": c.exec("stripplan", json!({})),
        "bom": c.exec("bom", json!({})),
        "bom_part": c.exec("bom", json!({"group": "part"})),
        "checks": c.exec("checks", json!({"kinds": ["accessory-orphan", "no-accessory-part", "link-across-partition", "link-orphan"]})),
        "preview": preview,
    })
}

#[test]
#[ignore = "tester 030: awaiting build"]
fn accessory_and_jumper_writes_by_the_agent_then_refused_with_writes_off() {
    let dir = TempDir::new("t030 Schaltpl\u{e4}ne mit Leerzeichen");
    let path = dir.file("Klemmen -X01 (A).ecad");
    let (strip, terms, before) = {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        for x in [100, 110, 120, 130, 140, 150, 160, 170] {
            c.exec("wire", json!({"points": [[x, 200], [x, 100]]}));
        }
        let f = c.exec("terminals", json!({"from": [90, 170], "to": [180, 170]}));
        let strip = hex(&f, "strip");
        let terms: Vec<String> = f["terminals"].as_array().unwrap().iter().map(|x| hex(x, "terminal")).collect();
        assert_eq!(terms.len(), 8, "{f}");

        // Every write of AC 6, each one transaction by the agent.
        let writes: Vec<(&str, Value, &[&str])> = vec![
            ("projectprops", json!({"props": {"strip_end_stop_part_number": "ES-1"}}), &["strip_end_stop_part_number"]),
            ("projectprops", json!({"props": {"strip_jumper_bar": "6"}}), &["strip_jumper_bar"]),
            ("accessory", json!({"strip": strip, "at": "start", "items": ["end_stop"]}), &["acc.start"]),
            ("accessory", json!({"strip": strip, "at": "end", "add": ["end_plate", "end_stop"]}), &["acc.end"]),
            ("accessory", json!({"strip": strip, "at": "end", "remove": ["end_plate"]}), &["acc.end"]),
            ("accessory", json!({"strip": strip, "at": "end", "items": null}), &["acc.end"]),
            ("accessory", json!({"strip": strip, "at": ["1", "2", "3"], "on": true, "items": ["marker"]}), &["acc.on.1", "acc.on.2", "acc.on.3"]),
            ("setprop", json!({"component": strip, "key": "acc.after.4", "value": "partition"}), &["acc.after.4"]),
            ("setprop", json!({"component": strip, "key": "acc.on.4", "value": "marker"}), &["acc.on.4"]),
            ("setprop", json!({"component": strip, "key": "partition.part_number", "value": "TP-1"}), &["partition.part_number"]),
            ("jumper", json!({"terminals": [terms[0], terms[1]]}), &["link.1"]),
            ("jumper", json!({"strip": strip, "positions": "3-4"}), &["link.2"]),
            ("jumper", json!({"terminals": [terms[0], terms[2]], "skip": true}), &["link.3"]),
            ("jumper", json!({"link": {"strip": strip, "n": 2}, "add": ["5", "6", "7", "8"]}), &["link.2"]),
            ("jumper", json!({"link": {"strip": strip, "n": 2}, "remove": ["8"]}), &["link.2"]),
            ("jumper", json!({"link": {"strip": strip, "n": 2}, "split_at": "4"}), &["link.2"]),
            ("jumper", json!({"link": {"strip": strip, "n": 3}, "delete": true}), &["link.3"]),
            ("setprop", json!({"component": strip, "key": "link.1", "value": "1-3"}), &["link.1"]),
            ("setprop", json!({"component": strip, "key": "jumper.bar", "value": "4"}), &["jumper.bar"]),
            ("setprop", json!({"component": strip, "key": "link.1.part_number", "value": "JB-RD"}), &["link.1.part_number"]),
            // Renumber with accessory keys and links: labels 1…8 become 5…12.
            ("renumber", json!({"kind": "terminals", "start": 5}), &["acc.after.8", "acc.on.8", "link.1", "link.2"]),
        ];
        for (cmd, params, keys) in writes {
            let n = c.log().len();
            let v = c.exec(cmd, params.clone());
            let (label, ops) = one_by_agent(&mut c, n, &format!("{cmd} {params} → {v}"));
            for k in keys {
                assert!(sets_key(&ops, k), "{cmd} {params}: sets {k} ({label}): {ops:?}");
            }
            assert!(
                ops.iter()
                    .filter(|o| matches!(o, Op::SetProperty { key, .. } if key.starts_with("link.") || key.starts_with("acc.")))
                    .all(|o| matches!(o, Op::SetProperty { object: ObjectRef::Component(_), .. })),
                "accessory and link keys are strip properties: {ops:?}"
            );
        }
        // Renumber rewrote the links with their terminals.
        let d = c.doc();
        let sp = ecad_model::ComponentId::from(u128::from_str_radix(&strip, 16).unwrap());
        let links: Vec<String> = d.components[&sp]
            .properties
            .iter()
            .filter(|(k, _)| k.starts_with("link.") && k[5..].chars().all(|c| c.is_ascii_digit()))
            .map(|(_, v)| v.replace(' ', ""))
            .collect();
        assert!(links.contains(&"5-7".to_string()), "link 1-3 → 5-7: {links:?}");
        // A link that joins two nets records the numbering pass's ops in the same transaction.
        let n = c.log().len();
        c.exec("jumper", json!({"terminals": [terms[3], terms[2]]}));
        let (_, ops) = one_by_agent(&mut c, n, "jumper 4-3");
        assert!(ops.iter().any(|o| matches!(o, Op::SetProperty { key, .. } if key.starts_with("link."))), "{ops:?}");
        // `u` through MCP undoes it.
        c.exec("u", Value::Null);
        assert_eq!(c.log().len(), n + 2);
        // Reads record nothing.
        let n = c.log().len();
        let before = reads(&mut c);
        assert_eq!(c.log().len(), n, "reads record nothing");
        let plan = &before["stripplan"]["strips"][0];
        assert!(plan["links"].as_array().is_some_and(|l| !l.is_empty()), "stripplan returns links: {plan}");
        assert!(plan["rows"].as_array().unwrap().iter().any(|r| r["kind"] == "accessory"), "{plan}");
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
        (strip, terms, before)
    };
    let (s, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut c = Client::new(s);
    assert!(!c.s.writes());
    let log = c.log();
    for (cmd, params) in [
        ("accessory", json!({"strip": strip, "at": "start", "items": ["end_plate"]})),
        ("accessory", json!({"strip": strip, "at": ["5", "6"], "on": true, "items": ["marker"]})),
        ("accessory", json!({"terminal": terms[0], "items": ["partition"]})),
        ("setprop", json!({"component": strip, "key": "acc.after.5", "value": "partition"})),
        ("setprop", json!({"component": strip, "key": "acc.on.5", "value": "marker"})),
        ("setprop", json!({"component": strip, "key": "end_stop.part_number", "value": "X"})),
        ("setprop", json!({"component": strip, "key": "marker.per_card", "value": "5"})),
        ("setprop", json!({"component": strip, "key": "link.1", "value": "5-8"})),
        ("setprop", json!({"component": strip, "key": "link.1.maker", "value": "X"})),
        ("setprop", json!({"component": strip, "key": "jumper.bar", "value": "10"})),
        ("projectprops", json!({"props": {"strip_end_plate_part_number": "EP-1"}})),
        ("projectprops", json!({"props": {"strip_jumper_bar": "8"}})),
        ("renumber", json!({"kind": "terminals", "start": 1})),
        ("jumper", json!({"terminals": [terms[0], terms[3]]})),
        ("jumper", json!({"strip": strip, "positions": "11-12"})),
        ("jumper", json!({"link": {"strip": strip, "n": 1}, "add": ["9"]})),
        ("jumper", json!({"link": {"strip": strip, "n": 1}, "remove": ["6"]})),
        ("jumper", json!({"link": {"strip": strip, "n": 1}, "split_at": "6"})),
        ("jumper", json!({"link": {"strip": strip, "n": 1}, "delete": true})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd} {params}");
    }
    let after = reads(&mut c);
    for k in ["stripplan", "bom", "bom_part", "checks", "preview"] {
        assert_eq!(after[k], before[k], "{k} after reopen with writes off");
    }
    assert_eq!(c.log(), log, "nothing recorded with writes off");
}
