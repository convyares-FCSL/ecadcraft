//! Reviewer 026 (PR #52) findings, through MCP: which side a merge renumbers or joins when one
//! side renames an existing device into the other's new tag (1), Make them one for two cables (2),
//! the two-transaction exception only for a line of one finalising `issuerevision` (3), and the
//! internal `numbering.high.<P>` never merged as a property (4).

use ecad_mcp::{EcadMcp, Options};
use ecad_model::{CableId, ComponentId, Document, Subject};
use serde_json::{Value, json};

use crate::mcp_contracts::{Client, TempDir};

fn tag_of(doc: &Document, comp: &str) -> Option<String> {
    doc.components.get(&ComponentId::parse(comp).unwrap()).and_then(|x| x.properties.get("tag").cloned())
}

fn place(c: &mut Client, symbol: &str, at: [f64; 2], tag: Option<&str>) -> String {
    let mut p = json!({"symbol": symbol, "at": at});
    if let Some(t) = tag {
        p["tag"] = json!(t);
    }
    c.exec("place", p)["component"].as_str().unwrap().to_string()
}

/// A saved file project with X = `-K01` and a wire.
fn project(dir: &TempDir, name: &str) -> (Client, String) {
    let path = dir.file(name);
    let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
    let x = place(&mut c, "coil", [0.0, 0.0], Some("-K01"));
    c.exec("wire", json!({"points": [[0, 40], [60, 40]]}));
    c.ok("save", json!({}));
    (c, x)
}

fn tag_row(pv: &Value) -> Value {
    pv["conflicts"].as_array().and_then(|a| a.iter().find(|k| k["kind"] == json!("tag")).cloned()).unwrap_or_else(|| panic!("no tag row: {pv}"))
}

/// The branch renames X to `-K05`; main adds a new device `-K05`. X's tag came from the branch,
/// so `renumber_branch` renumbers X and `renumber_main` main's new device; `one` joins X (the
/// branch's) into main's new device.
#[test]
fn a_rename_into_the_other_sides_new_tag_renumbers_and_joins_the_right_device() {
    for pick in ["renumber_branch", "renumber_main", "one"] {
        let dir = TempDir::new(&format!("r026-into-{pick}"));
        let (mut c, x) = project(&dir, "into.ecad");
        c.exec("branch", json!({"name": "Option S"}));
        c.exec("setprop", json!({"component": x, "key": "tag", "value": "-K05"}));
        c.exec("switchbranch", json!({"branch": "main", "save": true}));
        let y = place(&mut c, "no", [200.0, 100.0], Some("-K05"));
        let pv = c.exec("merge", json!({"from": "Option S", "preview": true}));
        let id = tag_row(&pv)["id"].as_str().unwrap().to_string();
        let r = c.exec("merge", json!({"from": "Option S", "choices": {id: pick}}));
        assert_eq!(r["merged"], json!(true), "{pick}: {r}");
        let doc = c.doc();
        match pick {
            "renumber_branch" => {
                assert_eq!(tag_of(&doc, &y).as_deref(), Some("-K05"), "main's new device keeps -K05");
                assert_ne!(tag_of(&doc, &x).as_deref(), Some("-K05"), "the branch's X is renumbered");
            }
            "renumber_main" => {
                assert_eq!(tag_of(&doc, &x).as_deref(), Some("-K05"), "X keeps the branch's -K05");
                assert_ne!(tag_of(&doc, &y).as_deref(), Some("-K05"), "main's new device is renumbered");
            }
            _ => {
                let yid = ComponentId::parse(&y).unwrap();
                assert!(!doc.components.contains_key(&ComponentId::parse(&x).unwrap()), "X (the branch's) joined main's");
                assert_eq!(tag_of(&doc, &y).as_deref(), Some("-K05"));
                assert_eq!(doc.representations_of(Subject::Component(yid)).count(), 2, "one device with both symbols");
            }
        }
    }
}

/// Both sides add a cable `-W01` with a slash: Make them one is offered and works; the branch's
/// cable joins main's with its slash and its wire.
#[test]
fn make_them_one_cable_in_a_merge_moves_the_slash_drawings() {
    let dir = TempDir::new("r026-cable-one");
    let (mut c, _x) = project(&dir, "cables.ecad");
    c.exec("branch", json!({"name": "Option C"}));
    c.exec("wire", json!({"points": [[0, 80], [60, 80]]}));
    let bk = c.exec("cableslash", json!({"from": [30, 75], "to": [30, 85]}))["cable"].as_str().unwrap().to_string();
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
    c.exec("wire", json!({"points": [[0, 120], [60, 120]]}));
    let mk = c.exec("cableslash", json!({"from": [30, 115], "to": [30, 125]}))["cable"].as_str().unwrap().to_string();
    let doc = c.doc();
    let mid = CableId::parse(&mk).unwrap();
    assert_eq!(doc.cables[&mid].properties.get("tag").map(String::as_str), Some("-W01"));
    let pv = c.exec("merge", json!({"from": "Option C", "preview": true}));
    let k = tag_row(&pv);
    assert!(k["choices"].as_array().unwrap().contains(&json!("one")), "{k}");
    assert!(k["unavailable"].is_null(), "{k}");
    let id = k["id"].as_str().unwrap().to_string();
    let r = c.exec("merge", json!({"from": "Option C", "choices": {id: "one"}}));
    assert_eq!(r["merged"], json!(true), "{r}");
    let doc = c.doc();
    assert!(!doc.cables.contains_key(&CableId::parse(&bk).unwrap()), "the branch's cable joined main's");
    assert_eq!(doc.cables[&mid].members.len(), 2, "both wires in one cable");
    assert_eq!(doc.representations_of(Subject::Cable(mid)).count(), 2, "both slashes show main's cable");
}

fn exec_call(id: u64, command: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": "execute", "arguments": {"command": command, "params": params}}})
}

fn is_error(v: &Value) -> bool {
    v["result"]["isError"] == json!(true) || v.get("error").is_some()
}

/// `[wire, issuerevision {provisional: "finalise"}]` with nothing provisional records two
/// transactions from two commands: refused (the exception is for one finalising issue only).
#[test]
fn an_edit_and_a_finalise_issue_with_nothing_provisional_is_refused() {
    let mut c = Client::new(EcadMcp::in_memory("r026", 0x0E_CAD0_7052, Options { write: true, agent: None }).unwrap());
    for (f, v) in [("rev", "A"), ("description", "Issue"), ("drawn", "CV")] {
        c.exec("revision", json!({f: v}));
    }
    let line = Value::Array(vec![
        exec_call(1, "wire", json!({"points": [[0, 0], [60, 0]]})),
        exec_call(2, "issuerevision", json!({"provisional": "finalise"})),
    ]);
    let reply = c.s.handle_line(&line.to_string()).expect("a reply");
    let replies: Vec<Value> = serde_json::from_str::<Value>(&reply).unwrap().as_array().unwrap().clone();
    assert!(!is_error(&replies[0]), "{}", replies[0]);
    assert!(is_error(&replies[1]), "{}", replies[1]);
    assert!(c.doc().versions.is_empty(), "the issue was taken back");
}

/// Both sides renumber: `numbering.high.K` is internal, so no conflict row names it, and the
/// merge leaves main's value (its mark is never lowered).
#[test]
fn numbering_high_is_not_merged_as_a_property() {
    let dir = TempDir::new("r026-high");
    let (mut c, _x) = project(&dir, "high.ecad");
    place(&mut c, "coil", [100.0, 0.0], None);
    c.ok("save", json!({}));
    c.exec("branch", json!({"name": "Option H"}));
    c.exec("renumber", json!({"kind": "devices", "scope": "all", "start": 20}));
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
    c.exec("renumber", json!({"kind": "devices", "scope": "all", "start": 40}));
    let main_high = c.doc().properties.get("numbering.high.K").cloned();
    assert!(main_high.is_some());
    let pv = c.exec("merge", json!({"from": "Option H", "preview": true}));
    let rows = pv["conflicts"].as_array().cloned().unwrap_or_default();
    assert!(!rows.iter().any(|k| k.to_string().contains("numbering.high")), "{pv}");
    assert!(!pv["changes"].to_string().to_ascii_lowercase().contains("numbering.high"), "{pv}");
    let mut ch = serde_json::Map::new();
    for k in &rows {
        let first = k["choices"][0].clone();
        ch.insert(k["id"].as_str().unwrap().to_string(), first);
    }
    let r = c.exec("merge", json!({"from": "Option H", "choices": Value::Object(ch)}));
    assert_eq!(r["merged"], json!(true), "{r}");
    assert_eq!(c.doc().properties.get("numbering.high.K"), main_high.as_ref(), "main's mark stays");
}
