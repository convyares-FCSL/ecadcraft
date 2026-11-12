//! Tester 032 through MCP (design 032 §12, FP-W3): each changed command driven through `execute`
//! records one transaction by `Author::Agent(<client>)` with the ops the design names, and is
//! refused with writes off. Written from the brief and design before the implementation.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::{Author, ConductorId, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, TempDir};

fn hex(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().to_string()
}

fn one_by_agent(c: &mut Client, before: usize, what: &str) -> (String, Vec<Op>) {
    let log = c.log();
    assert_eq!(log.len(), before + 1, "{what}: one transaction");
    let last = log.last().cloned().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()), "{what}: {}", last.label);
    (last.label, last.ops)
}

fn conductor(h: &str) -> ConductorId {
    ConductorId::from(u128::from_str_radix(h, 16).unwrap())
}

fn place_terminal(c: &mut Client, at: (f64, f64), tag: &str) -> (f64, f64) {
    let r = c.exec("place", json!({"symbol": "terminal", "at": [at.0, at.1], "tag": tag}));
    let t = hex(&r["terminals"][0], "terminal");
    let d = c.doc();
    let tid = ecad_model::TerminalId::from(u128::from_str_radix(&t, 16).unwrap());
    let pos = d
        .page_order
        .iter()
        .find_map(|pg| ecad_model::derive::terminal_positions(&d, *pg).into_iter().find(|(x, _, _)| *x == tid).map(|(_, _, p)| p))
        .unwrap();
    (pos.x, pos.y)
}

/// The §3.1 case through MCP: page 1 `-X1…3` → arrows A1…A3 slashed `-W04`; page 2 arrows → `-X51…53`.
fn w04(c: &mut Client) -> (String, String) {
    for i in 0..3 {
        let x = 100.0 + 20.0 * f64::from(i);
        let tp = place_terminal(c, (x, 100.0), &format!("X{}", i + 1));
        c.exec("wire", json!({"points": [[tp.0, tp.1], [tp.0, 40.0]]}));
        c.exec("place", json!({"symbol": "continuation", "at": [tp.0, 40.0], "name": format!("A{}", i + 1)}));
    }
    let cable = hex(&c.exec("cableslash", json!({"from": [90, 70], "to": [150, 70], "tag": "-W04"})), "cable");
    let p2 = hex(&c.exec("newpage", json!({"title": "Field"})), "page");
    c.exec("openpage", json!({"page": p2}));
    for i in 0..3 {
        let x = 100.0 + 20.0 * f64::from(i);
        let tp = place_terminal(c, (x, 100.0), &format!("X5{}", i + 1));
        c.exec("place", json!({"symbol": "continuation", "at": [tp.0, 40.0], "name": format!("A{}", i + 1), "rotation": 180}));
        c.exec("wire", json!({"points": [[tp.0, 40.0], [tp.0, tp.1]]}));
    }
    (cable, p2)
}

#[test]
#[ignore = "tester 032: awaiting build"]
fn wire_joinwires_cableslash_cablecontinue_wiresettings_by_the_agent_then_refused_with_writes_off() {
    let dir = TempDir::new("t032 Leitungen ä");
    let path = dir.file("Netze (W).ecad");
    let (survivor, cable, p2) = {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());

        // `wire` merging (case 1): one transaction labelled Wire, no CreateConductor.
        let a = c.exec("wire", json!({"points": [[0, 0], [50, 0]]}));
        let first = hex(&a, "conductor");
        let n = c.log().len();
        let b = c.exec("wire", json!({"points": [[50, 0], [50, -20]]}));
        let (label, ops) = one_by_agent(&mut c, n, "wire merge");
        assert_eq!(label, "Wire");
        assert!(!ops.iter().any(|o| matches!(o, Op::CreateConductor { .. })), "{ops:?}");
        assert_eq!(b["created"], json!(false), "{b}");
        assert_eq!(hex(&b, "conductor"), first, "{b}");
        assert_eq!(b["joined"][0]["how"], "merged", "{b}");

        // Case 3 through execute: `removed` names the folded conductor, which no longer exists.
        let l = place_terminal(&mut c, (0.0, 200.0), "X7");
        let r = place_terminal(&mut c, (150.0, 200.0), "X8");
        let left = hex(&c.exec("wire", json!({"points": [[l.0, l.1], [50, 200]]})), "conductor");
        let right = hex(&c.exec("wire", json!({"points": [[100, 200], [r.0, r.1]]})), "conductor");
        let n = c.log().len();
        let v = c.exec("wire", json!({"points": [[50, 200], [100, 200]]}));
        let (label, ops) = one_by_agent(&mut c, n, "wire fold");
        assert_eq!(label, "Wire");
        assert_eq!(v["created"], json!(false), "{v}");
        assert_eq!(hex(&v, "conductor"), left, "{v}");
        assert_eq!(v["removed"], json!([right]), "{v}");
        assert!(ops.iter().any(|o| matches!(o, Op::DeleteConductor { .. })), "{ops:?}");
        assert!(!c.doc().conductors.contains_key(&conductor(&right)), "the folded conductor is gone");

        // A refused merge records nothing.
        c.exec("wire", json!({"points": [[0, 300], [50, 300]], "potential": "0V"}));
        let n = c.log().len();
        let m = c.refused("execute", json!({"command": "wire", "params": {"points": [[50, 300], [50, 340]], "potential": "24V"}}));
        assert!(m.contains("That wire carries 0V and this one 24V"), "{m}");
        assert_eq!(c.log().len(), n);

        // `joinwires`: one transaction "Join wires".
        c.exec("wire", json!({"points": [[200, 0], [250, 0]]}));
        c.exec("wire", json!({"points": [[250, 0], [250, -20]], "join": false}));
        let n = c.log().len();
        let j = c.exec("joinwires", json!({"at": [250, 0]}));
        let (label, _) = one_by_agent(&mut c, n, "joinwires");
        assert_eq!(label, "Join wires");
        assert_eq!(j["how"], "merged", "{j}");
        let survivor = hex(&j, "conductor");

        // `wiresettings`: one SetProjectSettings transaction.
        let n = c.log().len();
        let s = c.exec("wiresettings", json!({"label_rule": "each", "label_min_length": 10}));
        let (_, ops) = one_by_agent(&mut c, n, "wiresettings");
        assert!(ops.iter().all(|o| matches!(o, Op::SetProjectSettings { .. })) && !ops.is_empty(), "{ops:?}");
        assert_eq!(s["label_rule"], "each", "{s}");
        assert_eq!(s["label_min_length"], json!(10.0), "{s}");

        // `cablecontinue` before the second slash: the §3.4 refusal, no transaction.
        let (cable, p2) = w04(&mut c);
        let n = c.log().len();
        let m = c.refused("execute", json!({"command": "cablecontinue", "params": {"cable": cable, "at": [300, 70], "toward": [350, 70]}}));
        assert!(m.contains("-W04's cores end on continuations"), "{m}");
        assert!(m.contains("slash the wires there with tag -W04"), "{m}");
        assert_eq!(c.log().len(), n);

        // `cableslash` continuing: one transaction "Cable slash", AddCableMember and
        // LinkRepresentation, no CreateCable.
        let n = c.log().len();
        let v = c.exec("cableslash", json!({"from": [90, 70], "to": [150, 70], "tag": "-W04"}));
        let (label, ops) = one_by_agent(&mut c, n, "cableslash continue");
        assert_eq!(label, "Cable slash");
        assert_eq!(v["continued"], json!(true), "{v}");
        assert_eq!(ops.iter().filter(|o| matches!(o, Op::AddCableMember { .. })).count(), 3, "{ops:?}");
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
    let s = c.exec("wiresettings", json!({}));
    assert_eq!(s["label_rule"], "each", "{s}");
    assert_eq!(c.exec("checks", json!({})), reads_before);
    let _ = c.exec("net", json!({"conductor": survivor}));
    let _ = c.exec("cablecores", json!({"cable": cable}));
    let _ = p2;
    assert_eq!(c.log(), log, "nothing recorded with writes off");
}
