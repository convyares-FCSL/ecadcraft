//! Tester 031 through MCP (brief 011 rule; brief 031 AC 4 and AC 7; design 031 §10.7): on a
//! project saved at a path with spaces and non-ASCII letters, every text write (`textstyle`,
//! `showtext` at four levels, `movetext` and its reset, `text` with `justify`) records one
//! transaction by `Author::Agent(<client>)`; `textstyle {}`, `pagetexts` and `defaults` read and
//! record nothing; `projectprops`, `pageprops` and `setprop` refuse `text.` keys with design
//! §10.6's sentence; reopened with writes off, every write is refused and the reads give what they
//! gave before. Written from the brief and design before the implementation.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::{Author, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, TempDir};

const RESERVED: &str = "is set with Text styles (textstyle, showtext, movetext).";

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

/// The representation holding the item a `place` returned.
fn rep_of(c: &mut Client, placed: &Value) -> String {
    let item = hex(placed, "item");
    let d = c.doc();
    d.representations.iter().find(|(_, r)| r.items.iter().any(|i| i.hex() == item)).map(|(id, _)| id.hex()).unwrap()
}

fn reads(c: &mut Client, page: &str) -> Value {
    json!({
        "textstyle": c.exec("textstyle", json!({})),
        "texts": c.exec("pagetexts", json!({"page": page})),
        "defaults": c.exec("defaults", json!({})),
        "defaults_text": c.exec("defaults", json!({"group": "Text", "page": page})),
        "devices": c.exec("devices", json!({})),
    })
}

#[test]
#[ignore = "tester 031: awaiting build"]
fn text_writes_by_the_agent_then_refused_with_writes_off() {
    let dir = TempDir::new("t031 Schaltpl\u{e4}ne mit Leerzeichen");
    let path = dir.file("Texte -K01 (A).ecad");
    let (page, k1, rep, before) = {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        let coil = c.exec("place", json!({"symbol": "coil", "at": [100, 200]}));
        let k1 = hex(&coil, "component");
        let rep = rep_of(&mut c, &coil);
        let d = c.doc();
        let page = d.page_order[0].hex();

        let writes: Vec<(&str, Value)> = vec![
            ("textstyle", json!({"kind": "tag", "height": 3})),
            ("textstyle", json!({"font": "X"})),
            ("textstyle", json!({"kind": "wire_label", "height": 2})),
            ("showtext", json!({"kind": "tag", "show": "off"})),
            ("showtext", json!({"kind": "tag", "show": "on", "page": page})),
            ("showtext", json!({"kind": "tag", "show": "off", "component": k1})),
            ("showtext", json!({"kind": "tag", "show": "on", "representation": rep})),
            ("movetext", json!({"representation": rep, "kind": "tag", "to": [80, 215]})),
            ("movetext", json!({"representation": rep, "kind": "tag", "reset": true})),
            ("text", json!({"at": [100, 100], "text": "MITTE", "height": 2.5, "justify": "MC"})),
        ];
        for (cmd, params) in writes {
            let n = c.log().len();
            let v = c.exec(cmd, params.clone());
            let ops = one_by_agent(&mut c, n, &format!("{cmd} {params} → {v}"));
            assert!(!ops.is_empty(), "{cmd} {params}");
        }
        // Reads record nothing, with writes on.
        let n = c.log().len();
        let before = reads(&mut c, &page);
        assert_eq!(c.log().len(), n, "reads record nothing");
        assert!(before["texts"]["texts"].as_array().is_some_and(|t| !t.is_empty()), "{}", before["texts"]);
        assert!(before["defaults"]["defaults"].as_array().is_some_and(|t| t.iter().any(|r| r["key"] == "text.tag.height")), "{}", before["defaults"]);
        // The `text.` prefix is reserved for the text commands.
        for (cmd, params) in [
            ("projectprops", json!({"props": {"text.font": "Y"}})),
            ("pageprops", json!({"page": page, "props": {"text.tag.show": "off"}})),
            ("setprop", json!({"component": k1, "key": "text.tag.show", "value": "off"})),
            ("setprop", json!({"component": k1, "key": format!("text.{rep}.tag.at"), "value": "1,1,LT"})),
        ] {
            let n = c.log().len();
            let m = c.refused("execute", json!({"command": cmd, "params": params}));
            assert!(m.contains(RESERVED), "{cmd} {params}: {m}");
            assert_eq!(c.log().len(), n, "{cmd}: refused writes nothing");
        }
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
        (page, k1, rep, before)
    };
    let (s, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut c = Client::new(s);
    assert!(!c.s.writes());
    let log = c.log();
    for (cmd, params) in [
        ("textstyle", json!({"kind": "tag", "height": 3.5})),
        ("textstyle", json!({"font": "Z"})),
        ("textstyle", json!({"kind": "wire_label", "height": 2.5})),
        ("showtext", json!({"kind": "tag", "show": "on"})),
        ("showtext", json!({"kind": "tag", "show": "off", "page": page})),
        ("showtext", json!({"kind": "tag", "show": "on", "component": k1})),
        ("showtext", json!({"kind": "tag", "show": "off", "representation": rep})),
        ("movetext", json!({"representation": rep, "kind": "tag", "to": [70, 215]})),
        ("movetext", json!({"representation": rep, "kind": "tag", "reset": true})),
        ("text", json!({"at": [50, 50], "text": "LINKS", "height": 2.5, "justify": "ML"})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd} {params}");
    }
    let after = reads(&mut c, &page);
    for k in ["textstyle", "texts", "defaults", "defaults_text", "devices"] {
        assert_eq!(after[k], before[k], "{k} after reopen with writes off");
    }
    assert_eq!(c.log(), log, "nothing recorded with writes off");
}

/// `defaults` and `textstyle {}` on a fresh in-memory project with writes off.
#[test]
#[ignore = "tester 031: awaiting build"]
fn reads_work_with_writes_off_and_record_nothing() {
    let mut c = crate::mcp_contracts::reader();
    let log = c.log();
    let ts = c.exec("textstyle", json!({}));
    assert!(ts["kinds"].as_array().is_some_and(|k| k.iter().any(|r| r["kind"] == "tag")), "{ts}");
    let d = c.exec("defaults", json!({}));
    assert!(d["defaults"].as_array().is_some_and(|a| !a.is_empty()), "{d}");
    let page = c.doc().page_order[0].hex();
    let t = c.exec("pagetexts", json!({"page": page}));
    assert_eq!(t["texts"], json!([]), "an empty page: {t}");
    let m = c.refused("execute", json!({"command": "defaults", "params": {"key": "nope"}}));
    assert!(m.contains("No default \"nope\"."), "{m}");
    assert_eq!(c.log(), log);
}
