//! Tester 036 through MCP (brief 011 rule; brief 036 AC 9; design 036 §13 M): on a project saved
//! at a path with spaces and non-ASCII letters, every new or changed write (`terminals` with type
//! and level, `place` of a level starter, `termlabel`, `toposition`, `jumper` with levels,
//! `setprop` of `term_label.*`, `rating.*`, `part_number.<code>`, offers with codes, `link.*`
//! with levels, `projectprops strip_<code>_part_number` and the registry keys) records one
//! transaction by `Author::Agent(<client>)`; hostile input is refused and records nothing;
//! `stripplan`, `bom`, `checks` and `defaults {group: "Terminals"}` read and record nothing;
//! reopened with writes off, every write is refused and the reads give what they gave before.
//! Written from the brief and design before the implementation.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::{Author, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, TempDir};

fn hex(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap().to_string()
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

fn reads(c: &mut Client) -> Value {
    json!({
        "stripplan": c.exec("stripplan", json!({"limit": 5000})),
        "bom": c.exec("bom", json!({"limit": 5000})),
        "bom_part": c.exec("bom", json!({"group": "part"})),
        "checks": c.exec("checks", json!({"kinds": ["level-twice", "position-mixed", "level-missing", "fuse-device", "no-fuse-rating", "too-many-terminals"]})),
        "defaults": c.exec("defaults", json!({"group": "Terminals"})),
    })
}

/// The strip's terminal IDs by (position label, level key).
fn level(c: &mut Client, strip: &str, pos: &str, key: &str) -> String {
    let d = c.doc();
    d.terminals
        .iter()
        .find(|(_, t)| t.component.hex() == strip && t.label.as_deref() == Some(pos) && t.key.as_deref() == Some(key))
        .map(|(id, _)| id.hex())
        .unwrap_or_else(|| panic!("-X {pos}{key} missing"))
}

#[test]
fn level_writes_by_the_agent_then_refused_with_writes_off() {
    let dir = TempDir::new("t036 Schaltpl\u{e4}ne mit Leerzeichen");
    let path = dir.file("Klemmen -X01 (L N PE).ecad");
    let (x1, x2, fuse, before) = {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        for x in [100, 110, 120, 200, 210, 220, 300, 310, 320, 400, 500, 510] {
            c.exec("wire", json!({"points": [[x, 250], [x, 150]]}));
        }
        let f = c.exec("place", json!({"symbol": "fuseterm", "at": [700, 200]}));
        let fuse = hex(&f, "component");
        c.exec("wire", json!({"points": [[700, 210], [700, 260]]}));
        c.exec("wire", json!({"points": [[700, 190], [700, 140]]}));
        c.exec("setprop", json!({"component": fuse, "key": "rating", "value": "4A"}));

        // The L fence makes the strip; each write after it one transaction by the agent.
        let n = c.log().len();
        let v = c.exec("terminals", json!({"strip": null, "type": "LNPE", "level": "L", "from": [90, 200], "to": [130, 200]}));
        one_by_agent(&mut c, n, &format!("terminals LNPE L → {v}"));
        let x1 = hex(&v, "strip");
        let n = c.log().len();
        let v = c.exec("terminals", json!({"strip": null, "type": "SEN", "level": "S", "from": [495, 200], "to": [515, 200]}));
        one_by_agent(&mut c, n, &format!("terminals SEN S → {v}"));
        let x2 = hex(&v, "strip");
        let writes: Vec<(&str, Value, &[&str])> = vec![
            ("terminals", json!({"strip": x1, "type": "LNPE", "level": "N", "from": [190, 200], "to": [230, 200]}), &[]),
            ("place", json!({"symbol": "termlnpe.3", "at": [300, 200], "device": x1}), &[]),
            ("terminals", json!({"strip": x1, "type": "LNPE", "level": "PE", "position": "2", "from": [305, 200], "to": [315, 200]}), &[]),
            ("setprop", json!({"component": x1, "key": "term_label.1.1", "value": "L1"}), &["term_label.1.1"]),
            ("setprop", json!({"component": x1, "key": "part_number.lnpe", "value": "LNPE-1"}), &["part_number.lnpe"]),
            ("setprop", json!({"component": x1, "key": "terminals", "value": "3LNPE+2x3"}), &["terminals"]),
            ("setprop", json!({"component": x1, "key": "link.1", "value": "1.2-3.2"}), &["link.1"]),
            ("setprop", json!({"component": x1, "key": "terminal.level.separator", "value": "-"}), &["terminal.level.separator"]),
            ("projectprops", json!({"props": {"strip_lnpe_part_number": "LNPE-P"}}), &["strip_lnpe_part_number"]),
            ("projectprops", json!({"props": {"terminal.sen.levels": "S,P,M"}}), &["terminal.sen.levels"]),
            ("jumper", json!({"strip": x2, "positions": "1.1-2.1"}), &["link.1"]),
            ("terminals", json!({"strip": x2, "type": "FUSE", "level": "All", "from": [395, 200], "to": [405, 200]}), &[]),
            ("setprop", json!({"component": x2, "key": "rating.3", "value": "2A"}), &["rating.3"]),
            ("toposition", json!({"device": fuse, "strip": x2}), &["rating.4"]),
            ("renumber", json!({"kind": "terminals", "start": 1}), &[]),
        ];
        for (cmd, params, keys) in writes {
            let n = c.log().len();
            let v = c.exec(cmd, params.clone());
            let ops = one_by_agent(&mut c, n, &format!("{cmd} {params} → {v}"));
            for k in keys {
                assert!(sets_key(&ops, k), "{cmd} {params}: sets {k}: {ops:?}");
            }
        }
        // termlabel: position and text, each one transaction.
        let t = level(&mut c, &x1, "2", ".1");
        for params in [json!({"terminal": t, "position": "7"}), json!({"terminal": t, "text": "2.1S"}), json!({"terminal": t, "text": null})] {
            let n = c.log().len();
            let v = c.exec("termlabel", params.clone());
            one_by_agent(&mut c, n, &format!("termlabel {params} → {v}"));
        }
        // Jumper on two picked levels of one position: a cross-level link.
        let (a, b) = (level(&mut c, &x1, "1", ".1"), level(&mut c, &x1, "1", ".3"));
        let n = c.log().len();
        c.exec("jumper", json!({"terminals": [a, b]}));
        let ops = one_by_agent(&mut c, n, "jumper 1.1 → 1.3");
        assert!(
            ops.iter()
                .any(|o| matches!(o, Op::SetProperty { key, after: Some(v), .. } if key.starts_with("link.") && ["1.1-1.3", "1-1.3"].contains(&v.replace(' ', "").as_str()))),
            "{ops:?}"
        );
        // Hostile input: refused through MCP, nothing recorded.
        let n = c.log().len();
        for (cmd, params) in [
            ("setprop", json!({"component": x1, "key": "terminals", "value": "1000x2"})),
            ("setprop", json!({"component": x1, "key": "terminals", "value": "6Q3"})),
            ("setprop", json!({"component": x1, "key": "link.2", "value": "1.1-4.2"})),
            ("termlabel", json!({"terminal": t, "position": "x".repeat(10_000)})),
            ("termlabel", json!({"terminal": t, "position": "1"})),
            ("toposition", json!({"device": x1, "strip": x2})),
            ("terminals", json!({"strip": x1, "type": "Q", "level": "L", "from": [90, 200], "to": [130, 200]})),
        ] {
            let m = c.refused("execute", json!({"command": cmd, "params": params}));
            assert!(!m.is_empty(), "{cmd} {params}");
        }
        assert_eq!(c.log().len(), n, "refusals record nothing");
        // Reads record nothing.
        let n = c.log().len();
        let before = reads(&mut c);
        assert_eq!(c.log().len(), n, "reads record nothing");
        let rows = before["stripplan"]["strips"].as_array().cloned().unwrap_or_default();
        let any_row = rows.iter().flat_map(|s| s["rows"].as_array().cloned().unwrap_or_default()).find(|r| r["level_name"].is_string());
        let r = any_row.unwrap_or_else(|| panic!("stripplan rows carry level fields: {}", before["stripplan"]));
        for k in ["position", "level", "level_name", "type", "type_name", "state", "label_overridden", "editable", "breaks"] {
            assert!(r.get(k).is_some(), "stripplan row has {k}: {r}");
        }
        let keys: Vec<String> = before["defaults"]["defaults"]
            .as_array()
            .map(|a| a.iter().map(|r| r["key"].as_str().unwrap_or("").to_string()).collect())
            .unwrap_or_default();
        assert!(keys.iter().any(|k| k == "terminal.level.separator"), "{keys:?}");
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
        (x1, x2, fuse, before)
    };
    let (s, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut c = Client::new(s);
    assert!(!c.s.writes());
    let log = c.log();
    let t = level(&mut c, &x1, "1", ".1");
    for (cmd, params) in [
        ("terminals", json!({"strip": x1, "type": "LNPE", "level": "L", "from": [90, 200], "to": [130, 200]})),
        ("terminals", json!({"strip": null, "type": "x3", "level": "All", "from": [495, 200], "to": [515, 200]})),
        ("place", json!({"symbol": "termlnpe.2", "at": [500, 200], "device": x1})),
        ("termlabel", json!({"terminal": t, "position": "9"})),
        ("termlabel", json!({"terminal": t, "text": "X"})),
        ("toposition", json!({"device": fuse, "strip": x2})),
        ("jumper", json!({"strip": x1, "positions": "1.3-2.3"})),
        ("setprop", json!({"component": x1, "key": "term_label.1.2", "value": "N"})),
        ("setprop", json!({"component": x2, "key": "rating.3", "value": "6A"})),
        ("setprop", json!({"component": x1, "key": "part_number.lnpe", "value": "X"})),
        ("setprop", json!({"component": x1, "key": "terminals", "value": "4LNPE"})),
        ("setprop", json!({"component": x1, "key": "link.1", "value": "1.1-2.1"})),
        ("projectprops", json!({"props": {"strip_lnpe_part_number": "X"}})),
        ("projectprops", json!({"props": {"terminal.level.separator": ""}})),
        ("renumber", json!({"kind": "terminals", "start": 5})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd} {params}");
    }
    let after = reads(&mut c);
    for k in ["stripplan", "bom", "bom_part", "checks", "defaults"] {
        assert_eq!(after[k], before[k], "{k} after reopen with writes off");
    }
    assert_eq!(c.log(), log, "nothing recorded with writes off");
}
