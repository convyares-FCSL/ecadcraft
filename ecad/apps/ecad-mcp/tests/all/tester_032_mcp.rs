//! Tester 032 through MCP (design 032 §12, FP-W3): each changed command driven through `execute`
//! records one transaction by `Author::Agent(<client>)` with the ops the design names, and is
//! refused with writes off. Written from the brief and design before the implementation.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::{Op, Transaction};
use serde_json::{Value, json};

use crate::common::assert_one_by_agent;
use crate::mcp_contracts::{Client, TempDir};

fn hex(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().to_string()
}

fn place_connection_point(c: &mut Client, at: (f64, f64), tag: &str) -> (f64, f64) {
    let r = c.exec("place", json!({"symbol": "terminal", "at": [at.0, at.1], "tag": tag}));
    let t = hex(&r["terminals"][0], "terminal");
    let d = c.doc();
    let tid = ecad_model::ConnectionPointId::from(u128::from_str_radix(&t, 16).unwrap());
    let pos = d
        .page_order
        .iter()
        .find_map(|pg| ecad_model::connectivity::derive::connection_point_positions(&d, *pg).into_iter().find(|(x, _, _)| *x == tid).map(|(_, _, p)| p))
        .unwrap();
    (pos.x, pos.y)
}

/// The §3.1 case through MCP: page 1 `-X1…3` → arrows A1…A3 slashed `-W04`; page 2 arrows → `-X51…53`.
fn w04(c: &mut Client) -> (String, String) {
    for i in 0..3 {
        let x = 100.0 + 20.0 * f64::from(i);
        let tp = place_connection_point(c, (x, 100.0), &format!("X{}", i + 1));
        // The continuation first, so the line lands on its point (design 072 §4.4, F5).
        c.exec("place", json!({"symbol": "continuation", "at": [tp.0, 40.0], "name": format!("A{}", i + 1)}));
        c.exec("wire", json!({"points": [[tp.0, tp.1], [tp.0, 40.0]]}));
    }
    let cable = hex(&c.exec("cableslash", json!({"from": [90, 70], "to": [150, 70], "tag": "-W04"})), "cable");
    let p2 = hex(&c.exec("newpage", json!({"title": "Field"})), "page");
    c.exec("openpage", json!({"page": p2}));
    for i in 0..3 {
        let x = 100.0 + 20.0 * f64::from(i);
        let tp = place_connection_point(c, (x, 100.0), &format!("X5{}", i + 1));
        c.exec("place", json!({"symbol": "continuation", "at": [tp.0, 40.0], "name": format!("A{}", i + 1), "rotation": 180}));
        c.exec("wire", json!({"points": [[tp.0, 40.0], [tp.0, tp.1]]}));
    }
    (cable, p2)
}

#[test]
fn wire_joinwires_cableslash_cablecontinue_wiresettings_by_the_agent_then_refused_with_writes_off() {
    let dir = TempDir::new("t032 Leitungen ä");
    let path = dir.file("Netze (W).ecad");
    let (survivor, cable, p2) = {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());

        // `wire` merging (case 1): one transaction labelled Wire that grows the line (design 072:
        // routes draw, wires connect), no new route.
        let a = c.exec("wire", json!({"points": [[0, 0], [50, 0]]}));
        let first = hex(&a, "route");
        let n = c.log().len();
        let b = c.exec("wire", json!({"points": [[50, 0], [50, -20]]}));
        let Transaction { label, ops, .. } = assert_one_by_agent(&mut c, n, "wire merge");
        assert_eq!(label, "Wire");
        assert!(!ops.iter().any(|o| matches!(o, Op::AddRoute { .. })), "{ops:?}");
        assert_eq!(b["created"], json!(false), "{b}");
        assert_eq!(hex(&b, "route"), first, "{b}");
        assert_eq!(b["joined"][0]["how"], "merged", "{b}");

        // Case 3 through execute: `removed` names the folded line, which no longer exists; with
        // both ends on terminals the line now carries a wire.
        let l = place_connection_point(&mut c, (0.0, 200.0), "X7");
        let r = place_connection_point(&mut c, (150.0, 200.0), "X8");
        let left = hex(&c.exec("wire", json!({"points": [[l.0, l.1], [50, 200]]})), "route");
        let right = hex(&c.exec("wire", json!({"points": [[100, 200], [r.0, r.1]]})), "route");
        let n = c.log().len();
        let v = c.exec("wire", json!({"points": [[50, 200], [100, 200]]}));
        let Transaction { label, ops, .. } = assert_one_by_agent(&mut c, n, "wire fold");
        assert_eq!(label, "Wire");
        assert_eq!(v["created"], json!(false), "{v}");
        assert_eq!(hex(&v, "route"), left, "{v}");
        assert_eq!(v["removed"], json!([right]), "{v}");
        assert!(ops.iter().any(|o| matches!(o, Op::RemoveRoute { .. })), "{ops:?}");
        assert!(ops.iter().any(|o| matches!(o, Op::CreateWire { .. })), "the wire X7–X8 forms: {ops:?}");
        assert!(!c.doc().routes.contains_key(&ecad_model::RouteId::parse(&right).unwrap()), "the folded line is gone");

        // A refused join records nothing: a 24V line landing on a 0V wire (a T; a line with a free
        // end carries no wire, so no potential, design 072).
        c.points_at(&[(0.0, 300.0), (50.0, 300.0), (25.0, 340.0)]);
        c.exec("wire", json!({"points": [[0, 300], [50, 300]], "potential": "0V"}));
        let n = c.log().len();
        let m = c.refused("execute", json!({"command": "wire", "params": {"points": [[25, 340], [25, 300]], "potential": "24V"}}));
        assert!(m.contains("That wire carries 0V and this one 24V"), "{m}");
        assert_eq!(c.log().len(), n);

        // `joinwires`: one transaction "Join wires".
        c.exec("wire", json!({"points": [[200, 0], [250, 0]]}));
        c.exec("wire", json!({"points": [[250, 0], [250, -20]], "join": false}));
        let n = c.log().len();
        let j = c.exec("joinwires", json!({"at": [250, 0]}));
        let Transaction { label, .. } = assert_one_by_agent(&mut c, n, "joinwires");
        assert_eq!(label, "Join wires");
        assert_eq!(j["how"], "merged", "{j}");
        let survivor = hex(&j, "route");

        // `wiresettings`: one SetProjectSettings transaction.
        let n = c.log().len();
        let s = c.exec("wiresettings", json!({"label_rule": "each", "label_min_length": 10}));
        let Transaction { label: _, ops, .. } = assert_one_by_agent(&mut c, n, "wiresettings");
        assert!(ops.iter().all(|o| matches!(o, Op::SetProjectSettings { .. })) && !ops.is_empty(), "{ops:?}");
        assert_eq!(s["label_rule"], "each", "{s}");
        assert_eq!(s["label_min_length"], json!(10.0), "{s}");

        // `cablecontinue` on cores that are each one wire through the continuation pairs (F5):
        // refused, naming the cable, no transaction.
        let (cable, p2) = w04(&mut c);
        let n = c.log().len();
        let m = c.refused("execute", json!({"command": "cablecontinue", "params": {"cable": cable, "at": [300, 70], "toward": [350, 70]}}));
        assert!(m.contains("-W04"), "{m}");
        assert_eq!(c.log().len(), n);

        // `cableslash` with -W04's tag on page 2 (F5): one transaction "Cable slash", a
        // LinkRepresentation, no CreateCable and no new member.
        let n = c.log().len();
        let v = c.exec("cableslash", json!({"from": [90, 70], "to": [150, 70], "tag": "-W04"}));
        let Transaction { label, ops, .. } = assert_one_by_agent(&mut c, n, "cableslash continue");
        assert_eq!(label, "Cable slash");
        assert_eq!(v["continued"], json!(true), "{v}");
        assert!(!ops.iter().any(|o| matches!(o, Op::ModifyCable { .. })), "no new member: {ops:?}");
        assert_eq!(v["cores"].as_array().map_or(0, Vec::len), 3, "{v}");
        assert!(ops.iter().any(|o| matches!(o, Op::LinkRepresentation { .. })), "{ops:?}");
        assert!(!ops.iter().any(|o| matches!(o, Op::CreateCable { .. })), "{ops:?}");

        // `u` through MCP undoes it.
        c.exec("u", Value::Null);
        c.exec("redo", Value::Null);
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
        (survivor, cable, p2)
    };
    // Reopened with writes off: every changed command is refused, nothing recorded.
    let (s, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut c = Client::new(s);
    assert!(!c.s.writes());
    let log = c.log();
    let reads_before = c.exec("checks", json!({}));
    for (cmd, params) in [
        ("wire", json!({"points": [[0, 0], [0, 40]]})),
        ("wire", json!({"points": [[500, 0], [550, 0]], "join": false})),
        ("joinwires", json!({"at": [0, 0]})),
        ("cableslash", json!({"from": [90, 60], "to": [150, 60], "cable": cable})),
        ("cablecontinue", json!({"cable": cable, "at": [300, 70], "toward": [350, 70]})),
        ("wiresettings", json!({"label_rule": "net"})),
        ("wiresettings", json!({"label_min_length": 5})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd} {params}");
    }
    // Reads still work and record nothing.
    // `wiresettings {}` is not a read-only form in the MCP policy (true since brief 022, not 032),
    // so the stored settings are read from the document.
    let w = serde_json::to_value(&c.doc().settings.wiring).unwrap();
    assert_eq!(w["label_rule"], "each", "{w}");
    assert_eq!(w["label_min_length"], json!(10.0), "{w}");
    assert_eq!(c.exec("checks", json!({})), reads_before);
    assert!(c.doc().routes.contains_key(&ecad_model::RouteId::parse(&survivor).unwrap()), "the joined line is saved");
    let _ = c.exec("cablecores", json!({"cable": cable}));
    let _ = p2;
    assert_eq!(c.log(), log, "nothing recorded with writes off");
}
