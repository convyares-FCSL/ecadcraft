//! Tester 044 (connector devices) through MCP (brief 011 rule; brief 044 AC 9; design 044 §12.1
//! and row 14): on a project saved at a path with spaces and non-ASCII letters, `connector`
//! (row and pins forms), `place` of a pin starter joining a connector, `pinlabel`, `setprop` of the
//! FP4 keys and `setdefault` of the `connector` area each record one transaction by
//! `Author::Agent(<client>)` with §4.6's ops; §6.1's refusals and hostile input record nothing;
//! reads record nothing and name pins `-X01.S:3`; reopened with writes off, every write is
//! refused and the reads give what they gave before. Written from the brief and design before the
//! implementation.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::{Author, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, TempDir};

fn conn(v: &Value) -> String {
    ["component", "connector", "device"].iter().find_map(|k| v[*k].as_str().map(str::to_string)).unwrap_or_else(|| panic!("no connector ID in {v}"))
}

fn one_by_agent(c: &mut Client, before: usize, what: &str) -> Vec<Op> {
    let log = c.log();
    assert_eq!(log.len(), before + 1, "{what}: one transaction");
    let last = log.last().cloned().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()), "{what}: {}", last.label);
    last.ops
}

fn sides(ops: &[Op]) -> Vec<(String, String)> {
    ops.iter()
        .filter_map(|o| match o {
            Op::AddTerminal { terminal, .. } => Some((terminal.key.clone().unwrap_or_default(), terminal.label.clone().unwrap_or_default())),
            _ => None,
        })
        .collect()
}

/// The connector's terminal ID by side key and pin label.
fn pin(c: &mut Client, x: &str, side: &str, label: &str) -> String {
    let d = c.doc();
    d.terminals
        .iter()
        .find(|(_, t)| t.component.hex() == x && t.key.as_deref() == Some(side) && t.label.as_deref() == Some(label))
        .map(|(id, _)| id.hex())
        .unwrap_or_else(|| panic!("{x} {side}:{label} missing"))
}

fn reads(c: &mut Client, x: &str) -> Value {
    json!({
        "bom": c.exec("bom", json!({"limit": 5000})),
        "bom_part": c.exec("bom", json!({"group": "part"})),
        "wirelist": c.exec("wirelist", json!({"limit": 5000})),
        "xrefs": c.exec("xrefs", json!({"component": x})),
        "devices": c.exec("devices", json!({"tag": "-X"})),
        "checks": c.exec("checks", json!({"kinds": ["connector-pin-twice", "connector-family", "connector-size", "connector-lone-pin", "connector-over"]})),
        "defaults": c.exec("defaults", json!({})),
    })
}

#[test]
#[ignore = "tester 044: awaiting build"]
fn connector_writes_by_the_agent_then_refused_with_writes_off() {
    let dir = TempDir::new("t044 Steckverbinder mit Leerzeichen \u{e4}");
    let path = dir.file("Stecker -X01 (S P).ecad");
    let (x1, x2, before) = {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        for x in [100, 110, 120] {
            c.exec("wire", json!({"points": [[x, 250], [x, 150]]}));
        }

        // (1) The row form: one transaction, §4.6's ops.
        let n = c.log().len();
        let v = c.exec("connector", json!({"halves": "both", "family": "plain", "pins": 4, "layout": "row", "at": [250, 100]}));
        let ops = one_by_agent(&mut c, n, &format!("connector row → {v}"));
        let x1 = conn(&v);
        assert_eq!(ops.iter().filter(|o| matches!(o, Op::CreateComponent { .. })).count(), 1, "{ops:?}");
        let embeds: Vec<&ecad_model::Definition> =
            ops.iter().filter_map(|o| if let Op::EmbedDefinition { definition, .. } = o { Some(definition) } else { None }).collect();
        assert_eq!(embeds.len(), 2, "socket4 and plug4: {ops:?}");
        assert!(
            embeds
                .iter()
                .all(|d| d.origin.as_ref().is_some_and(|o| o.library_seq == 2) && d.bridges == vec![vec![".s".to_string(), ".p".to_string()]])
        );
        let mut got = sides(&ops);
        got.sort();
        let mut want: Vec<(String, String)> = [".p", ".s"].iter().flat_map(|k| (1..=4).map(move |i| (k.to_string(), i.to_string()))).collect();
        want.sort();
        assert_eq!(got, want, "AddTerminal per pin per side");
        assert!(
            ops.iter()
                .filter_map(|o| if let Op::LinkRepresentation { rep, .. } = o { Some(rep) } else { None })
                .all(|r| !r.connection_points.is_empty()),
            "explicit points"
        );

        // (1) The pins form across the three wires: one transaction, the wires split.
        let n = c.log().len();
        let v = c.exec("connector", json!({"halves": "both", "family": "m12", "pins": 4, "layout": "pins", "line": [[90, 200], [130, 200]]}));
        let ops = one_by_agent(&mut c, n, &format!("connector pins → {v}"));
        let x2 = conn(&v);
        assert_eq!(sides(&ops).len(), 6, "pins 1–3 on both sides: {ops:?}");
        assert!(ops.iter().any(|o| matches!(o, Op::CreateConductor { .. })), "the wires are split: {ops:?}");

        // (2) `place {symbol: pinpair, device}` joins and takes the next free pin (4).
        let n = c.log().len();
        let v = c.exec("place", json!({"symbol": "pinpair", "device": x2, "at": [300, 200]}));
        let ops = one_by_agent(&mut c, n, &format!("place pinpair → {v}"));
        let mut got = sides(&ops);
        got.sort();
        assert_eq!(got, [(".p".to_string(), "4".to_string()), (".s".to_string(), "4".to_string())], "next free pin");
        assert!(!ops.iter().any(|o| matches!(o, Op::CreateComponent { .. })), "joins, no new device");

        // (3) pinlabel, setprop of FP4 keys, setdefault of the connector area: one each.
        let s3 = pin(&mut c, &x1, ".s", "3");
        let writes: Vec<(&str, Value)> = vec![
            ("pinlabel", json!({"device": x1, "pin": "3", "to": "PE"})),
            ("pinlabel", json!({"terminal": s3, "to": "GND"})),
            ("setprop", json!({"component": x1, "key": "half.s.part_number", "value": "SKT-4"})),
            ("setprop", json!({"component": x1, "key": "half.p.location", "value": "+CP1"})),
            ("setprop", json!({"component": x2, "key": "pins", "value": "5"})),
            ("setdefault", json!({"key": "connector.side.socket", "level": "project", "value": "F"})),
            ("setdefault", json!({"key": "connector.side.plug", "level": "object", "component": x2, "value": "M"})),
            ("connector", json!({"halves": "socket", "family": "dsub", "pins": 9, "layout": "row", "at": [250, 50]})),
        ];
        for (cmd, params) in writes {
            let n = c.log().len();
            let v = c.exec(cmd, params.clone());
            one_by_agent(&mut c, n, &format!("{cmd} {params} → {v}"));
        }
        let d = c.doc();
        let t = d.terminals.iter().find(|(id, _)| id.hex() == s3).map(|(_, t)| t.clone()).expect("S:3 kept its ID");
        assert_eq!(t.label.as_deref(), Some("GND"), "pinlabel keeps the terminal's ID");

        // (4) §6.1's refusals and hostile input: nothing recorded.
        let k = conn(&c.exec("place", json!({"symbol": "coil", "at": [350, 100]})));
        let n = c.log().len();
        for (cmd, params, words) in [
            ("connector", json!({"device": x1, "halves": "plug", "layout": "row", "at": [250, 150]}), "already has a plug half"),
            ("place", json!({"symbol": "pins", "device": x2, "pin": "2", "at": [300, 150]}), "is drawn on"),
            ("place", json!({"symbol": "socket4", "device": k, "at": [300, 150]}), "is not a connector."),
            ("connector", json!({"family": "dsub", "pins": 10, "layout": "row", "at": [250, 150]}), "A D-sub has 9, 15 or 25 pins."),
            ("connector", json!({"pins": 4, "names": ["L", "N", "PE"], "layout": "row", "at": [250, 150]}), "Give 4 pin names; got 3."),
            ("connector", json!({"pins": 4, "layout": "row"}), "connector needs at"),
            ("connector", json!({"pins": 100_000, "layout": "pins", "line": [[90, 200], [130, 200]]}), ""),
            ("connector", json!({"pins": 2, "names": ["A\u{7}", "B"], "layout": "row", "at": [250, 150]}), ""),
            ("pinlabel", json!({"device": x1, "pin": "4", "to": "GND"}), "already has a pin GND"),
            ("pinlabel", json!({"device": x1, "pin": "4", "to": "x".repeat(10_000)}), ""),
            ("setprop", json!({"component": x1, "key": "half.x.maker", "value": "Acme"}), "has no half \"x\""),
        ] {
            let m = c.refused("execute", json!({"command": cmd, "params": params}));
            assert!(!m.is_empty() && m.contains(words), "{cmd} {params}: {m}");
        }
        assert_eq!(c.log().len(), n, "refusals record nothing");

        // Reads record nothing and name the halves.
        let n = c.log().len();
        let before = reads(&mut c, &x1);
        assert_eq!(c.log().len(), n, "reads record nothing");
        let bom = before["bom"].to_string();
        assert!(bom.contains("-X01.F") && bom.contains("-X01.P") && bom.contains("SKT-4"), "{bom}");
        let keys: Vec<String> = before["defaults"]["defaults"]
            .as_array()
            .map(|a| a.iter().map(|r| r["key"].as_str().unwrap_or("").to_string()).collect())
            .unwrap_or_default();
        assert!(keys.iter().any(|k| k == "connector.side.socket"), "{keys:?}");
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
        (x1, x2, before)
    };

    // (5) Writes off: every write refused, nothing recorded, reads unchanged.
    let (s, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut c = Client::new(s);
    assert!(!c.s.writes());
    let log = c.log();
    let p1 = pin(&mut c, &x1, ".p", "1");
    for (cmd, params) in [
        ("connector", json!({"halves": "both", "family": "plain", "pins": 4, "layout": "row", "at": [250, 150]})),
        ("connector", json!({"device": x2, "halves": "socket", "layout": "pins", "line": [[90, 200], [130, 200]]})),
        ("place", json!({"symbol": "pinpair", "device": x2, "at": [300, 150]})),
        ("pinlabel", json!({"device": x1, "pin": "1", "to": "Z"})),
        ("pinlabel", json!({"terminal": p1, "to": "Z"})),
        ("setprop", json!({"component": x1, "key": "half.p.maker", "value": "Acme"})),
        ("setdefault", json!({"key": "connector.half.separator", "level": "project", "value": "-"})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd} {params}");
    }
    let after = reads(&mut c, &x1);
    for k in ["bom", "bom_part", "wirelist", "xrefs", "devices", "checks", "defaults"] {
        assert_eq!(after[k], before[k], "{k} after reopen with writes off");
    }
    assert_eq!(c.log(), log, "nothing recorded with writes off");
}

/// `connector` and `pinlabel` are listed with their docs (every ECAD command reaches MCP through
/// `execute`, design §0: no ecad-mcp code change).
#[test]
#[ignore = "tester 044: awaiting build"]
fn connector_and_pinlabel_are_listed() {
    let mut c = crate::mcp_contracts::reader();
    let catalog = c.ok("list_commands", json!({}));
    let catalog = catalog.as_array().cloned().unwrap_or_default();
    for id in ["connector", "pinlabel"] {
        let info = catalog.iter().find(|i| i["id"] == id).unwrap_or_else(|| panic!("{id} not listed"));
        assert!(info["params"].as_str().is_some_and(|p| !p.is_empty()), "{id} has parameter docs");
    }
}
