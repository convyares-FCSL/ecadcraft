//! Terminal block levels through MCP: on a project saved
//! at a path with spaces and non-ASCII letters, every new or changed write (`connection_points` with type
//! and level, `place` of a level starter, `terminalblocklabel`, `toposition`, `jumper` with levels,
//! `setprop` of `block_label.*`, `rating.*`, `part_number.<code>`, offers with codes, `link.*`
//! with levels, `projectprops strip_<code>_part_number` and the registry keys) records one
//! transaction by `Author::Agent(<client>)`; hostile input is refused and records nothing;
//! `stripplan`, `bom`, `checks` and `defaults {group: "Terminals"}` read and record nothing;
//! reopened with writes off, every write is refused and the reads give what they gave before.

use ecad_engine::agent::policy;
use ecad_mcp::{EcadMcp, Options};
use ecad_model::Op;
use serde_json::{Value, json};

use crate::common::assert_one_by_agent;
use crate::mcp_contracts::{Client, TempDir};

fn hex(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap().to_string()
}

fn sets_key(ops: &[Op], key: &str) -> bool {
    // Design 072 §2.5: a fuse position's rating is the strip's typed field (a `ModifyDevice`).
    ops.iter().any(|o| match o {
        Op::SetProperty { key: k, .. } => k == key,
        Op::ModifyDevice { before, after, .. } => before.value(key) != after.value(key),
        _ => false,
    })
}

fn reads(c: &mut Client) -> Value {
    json!({
        "stripplan": c.exec("stripplan", json!({"limit": 5000})),
        "bom": c.exec("bom", json!({"limit": 5000})),
        "bom_part": c.exec("bom", json!({"group": "part"})),
        "checks": c.exec("checks", json!({"kinds": ["level-twice", "position-mixed", "level-missing", "fuse-device", "no-fuse-rating", "too-many-terminal-blocks"]})),
        "defaults": c.exec("defaults", json!({"group": "Terminals"})),
    })
}

/// The strip's terminal IDs by (position label, level key).
fn level(c: &mut Client, strip: &str, pos: &str, key: &str) -> String {
    let d = c.doc();
    d.connection_points()
        .iter()
        .find(|(_, t)| {
            t.owner.subject_hex() == strip && t.label.as_deref() == Some(pos) && t.key.as_ref().map(ToString::to_string).as_deref() == Some(key)
        })
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
        let f = c.exec("place", json!({"symbol": "fuseterminalblock", "at": [700, 200]}));
        let fuse = hex(&f, "component");
        c.exec("wire", json!({"points": [[700, 210], [700, 260]]}));
        c.exec("wire", json!({"points": [[700, 190], [700, 140]]}));
        c.exec("setprop", json!({"component": fuse, "key": "rating", "value": "4A"}));

        // The L fence makes the strip; each write after it one transaction by the agent.
        let n = c.log().len();
        let v = c.exec("terminalblocks", json!({"strip": null, "type": "LNPE", "level": "L", "from": [90, 200], "to": [130, 200]}));
        assert_one_by_agent(&mut c, n, &format!("terminals LNPE L → {v}"));
        let x1 = hex(&v, "strip");
        let n = c.log().len();
        let v = c.exec("terminalblocks", json!({"strip": null, "type": "SEN", "level": "S", "from": [495, 200], "to": [515, 200]}));
        assert_one_by_agent(&mut c, n, &format!("terminals SEN S → {v}"));
        let x2 = hex(&v, "strip");
        let writes: Vec<(&str, Value, &[&str])> = vec![
            ("terminalblocks", json!({"strip": x1, "type": "LNPE", "level": "N", "from": [190, 200], "to": [230, 200]}), &[]),
            ("place", json!({"symbol": "termlnpe.3", "at": [300, 200], "device": x1}), &[]),
            ("terminalblocks", json!({"strip": x1, "type": "LNPE", "level": "PE", "position": "2", "from": [305, 200], "to": [315, 200]}), &[]),
            ("setprop", json!({"component": x1, "key": "block_label.1.1", "value": "L1"}), &["block_label.1.1"]),
            ("setprop", json!({"component": x1, "key": "part_number.lnpe", "value": "LNPE-1"}), &["part_number.lnpe"]),
            ("setprop", json!({"component": x1, "key": "terminal_blocks", "value": "3LNPE+2x3"}), &["terminal_blocks"]),
            ("setprop", json!({"component": x1, "key": "jumper_bar.1", "value": "1.2-3.2"}), &["jumper_bar.1"]),
            ("setprop", json!({"component": x1, "key": "terminal_block.level.separator", "value": "-"}), &["terminal_block.level.separator"]),
            ("projectprops", json!({"props": {"strip_lnpe_part_number": "LNPE-P"}}), &["strip_lnpe_part_number"]),
            ("projectprops", json!({"props": {"terminal_block.sen.levels": "S,P,M"}}), &["terminal_block.sen.levels"]),
            ("jumper", json!({"strip": x2, "positions": "1.1-2.1"}), &["jumper_bar.1"]),
            ("terminalblocks", json!({"strip": x2, "type": "FUSE", "level": "All", "from": [395, 200], "to": [405, 200]}), &[]),
            ("setprop", json!({"component": x2, "key": "rating.3", "value": "2A"}), &["rating.3"]),
            ("toposition", json!({"device": fuse, "strip": x2}), &["rating.4"]),
            ("renumber", json!({"kind": "terminal_blocks", "start": 1}), &[]),
        ];
        for (cmd, params, keys) in writes {
            let n = c.log().len();
            let v = c.exec(cmd, params.clone());
            let ops = assert_one_by_agent(&mut c, n, &format!("{cmd} {params} → {v}")).ops;
            for k in keys {
                assert!(sets_key(&ops, k), "{cmd} {params}: sets {k}: {ops:?}");
            }
        }
        // terminalblocklabel: position and text, each one transaction.
        let t = level(&mut c, &x1, "2", ".1");
        for params in [
            json!({"connection_point": t, "position": "7"}),
            json!({"connection_point": t, "text": "2.1S"}),
            json!({"connection_point": t, "text": null}),
        ] {
            let n = c.log().len();
            let v = c.exec("terminalblocklabel", params.clone());
            assert_one_by_agent(&mut c, n, &format!("terminalblocklabel {params} → {v}"));
        }
        // Jumper on two picked levels of one position: a cross-level link.
        let (a, b) = (level(&mut c, &x1, "1", ".1"), level(&mut c, &x1, "1", ".3"));
        let n = c.log().len();
        c.exec("jumper", json!({"connection_points": [a, b]}));
        let ops = assert_one_by_agent(&mut c, n, "jumper 1.1 → 1.3").ops;
        assert!(
            ops.iter()
                .any(|o| matches!(o, Op::SetProperty { key, after: Some(v), .. } if key.starts_with("jumper_bar.") && ["1.1-1.3", "1-1.3"].contains(&v.replace(' ', "").as_str()))),
            "{ops:?}"
        );
        // Hostile input: refused through MCP, nothing recorded.
        let n = c.log().len();
        for (cmd, params) in [
            ("setprop", json!({"component": x1, "key": "terminal_blocks", "value": "1000x2"})),
            ("setprop", json!({"component": x1, "key": "terminal_blocks", "value": "6Q3"})),
            ("setprop", json!({"component": x1, "key": "jumper_bar.2", "value": "1.1-4.2"})),
            ("terminalblocklabel", json!({"connection_point": t, "position": "x".repeat(10_000)})),
            ("terminalblocklabel", json!({"connection_point": t, "position": "1"})),
            ("toposition", json!({"device": x1, "strip": x2})),
            ("terminalblocks", json!({"strip": x1, "type": "Q", "level": "L", "from": [90, 200], "to": [130, 200]})),
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
        assert!(keys.iter().any(|k| k == "terminal_block.level.separator"), "{keys:?}");
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
        (x1, x2, fuse, before)
    };
    let (s, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut c = Client::new(s);
    assert!(!c.s.writes());
    let log = c.log();
    let t = level(&mut c, &x1, "1", ".1");
    for (cmd, params) in [
        ("terminalblocks", json!({"strip": x1, "type": "LNPE", "level": "L", "from": [90, 200], "to": [130, 200]})),
        ("terminalblocks", json!({"strip": null, "type": "x3", "level": "All", "from": [495, 200], "to": [515, 200]})),
        ("place", json!({"symbol": "termlnpe.2", "at": [500, 200], "device": x1})),
        ("terminalblocklabel", json!({"connection_point": t, "position": "9"})),
        ("terminalblocklabel", json!({"connection_point": t, "text": "X"})),
        ("toposition", json!({"device": fuse, "strip": x2})),
        ("jumper", json!({"strip": x1, "positions": "1.3-2.3"})),
        ("setprop", json!({"component": x1, "key": "block_label.1.2", "value": "N"})),
        ("setprop", json!({"component": x2, "key": "rating.3", "value": "6A"})),
        ("setprop", json!({"component": x1, "key": "part_number.lnpe", "value": "X"})),
        ("setprop", json!({"component": x1, "key": "terminal_blocks", "value": "4LNPE"})),
        ("setprop", json!({"component": x1, "key": "jumper_bar.1", "value": "1.1-2.1"})),
        ("projectprops", json!({"props": {"strip_lnpe_part_number": "X"}})),
        ("projectprops", json!({"props": {"terminal_block.level.separator": ""}})),
        ("renumber", json!({"kind": "terminal_blocks", "start": 5})),
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
