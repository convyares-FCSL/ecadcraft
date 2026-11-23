//! Title blocks through MCP: `titleblock` (new and write)
//! and `titleblockimport` (a block and a logo) each record one transaction by
//! `Author::Agent(<client>)`, are refused with writes off, and one undo restores the document;
//! `titleblocks` and `titleblock {id}` read with writes off. `path` names a file: refused.

use ecad_engine::agent::policy;
use ecad_model::Author;
use serde_json::{Value, json};

use crate::common::writer;
use crate::mcp_contracts::{AGENT, Client};

const WRITER_SEED: u64 = 55;

fn one_undoable(c: &mut Client, command: &str, params: Value, label: &str) -> Value {
    let before = c.doc();
    let n = c.log().len();
    let v = c.exec(command, params);
    let log = c.log();
    assert_eq!(log.len(), n + 1, "{command}: one transaction");
    let t = log.last().unwrap();
    assert_eq!(t.author, Author::Agent(AGENT.into()), "{command}");
    assert_eq!(t.label, label, "{command}");
    let after = c.doc();
    c.exec("undo", json!({}));
    assert_eq!(c.doc(), before, "{command}: one undo restores the document");
    c.exec("redo", json!({}));
    assert_eq!(c.doc(), after, "{command}: redo");
    v
}

fn dxf(lines: &[(f64, f64, f64, f64)]) -> String {
    let mut s = String::from("0\nSECTION\n2\nENTITIES\n");
    for (x1, y1, x2, y2) in lines {
        s.push_str(&format!("0\nLINE\n8\n0\n10\n{x1}\n20\n{y1}\n30\n0\n11\n{x2}\n21\n{y2}\n31\n0\n"));
    }
    s.push_str("0\nENDSEC\n0\nEOF\n");
    s
}

#[test]
fn the_title_block_commands_record_one_transaction_by_the_agent() {
    let mut c = writer(WRITER_SEED);
    let v = one_undoable(&mut c, "titleblock", json!({"new": {"from": "sheet", "sheet": "A3"}}), "New title block A3 title block");
    let id = v["id"].as_str().unwrap().to_string();
    let mut body = c.exec("titleblock", json!({"id": id}))["body"].clone();
    body["items"].as_array_mut().unwrap().push(json!({"kind": "field", "at": {"x": 2, "y": 2, "w": 40, "h": 8}, "bind": "project:job_no"}));
    one_undoable(&mut c, "titleblock", json!({"id": id, "body": body}), "Title block A3 title block");
    let item = c.exec("titleblock", json!({"id": id}))["body"]["items"].as_array().unwrap().iter().position(|i| i["kind"] == "logo").unwrap();
    let lines: Vec<(f64, f64, f64, f64)> = (0..10).map(|i| (f64::from(i), 0.0, f64::from(i), 5.0)).collect();
    one_undoable(&mut c, "titleblockimport", json!({"id": id, "item": item, "dxf": dxf(&lines)}), "Logo of A3 title block");
    one_undoable(
        &mut c,
        "titleblockimport",
        json!({"dxf": dxf(&[(0.0, 0.0, 100.0, 0.0), (0.0, 0.0, 0.0, 30.0)]), "sheet": "A3", "name": "Imported"}),
        "Import title block Imported",
    );
    // The reads record nothing.
    let n = c.log().len();
    let list = c.exec("titleblocks", json!({}));
    assert!(list["title_blocks"].as_array().is_some_and(|a| a.len() == 3), "the built-in and two own: {list}");
    assert_eq!(c.exec("titleblock", json!({"id": "sheet"}))["builtin"], json!(true));
    assert_eq!(c.log().len(), n);
    let m = c.refused("execute", json!({"command": "titleblockimport", "params": {"path": "logo.dxf", "sheet": "A3"}}));
    assert!(m.contains("`path` names a file"), "{m}");
    assert_eq!(c.log().len(), n);
}

#[test]
fn writes_are_refused_with_writes_off_and_the_reads_still_answer() {
    let mut r = crate::mcp_contracts::reader();
    let log = r.log();
    for (command, params) in [
        ("titleblock", json!({"new": {"from": "sheet", "sheet": "A3"}})),
        ("titleblock", json!({"id": "sheet", "body": {"size": [10, 10], "items": []}})),
        ("titleblockimport", json!({"dxf": dxf(&[(0.0, 0.0, 10.0, 0.0)]), "sheet": "A3"})),
    ] {
        let m = r.refused("execute", json!({"command": command, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{command}");
    }
    assert_eq!(r.exec("titleblocks", json!({}))["title_blocks"][0]["id"], json!("sheet"));
    assert!(r.exec("titleblock", json!({"id": "sheet", "sheet": "A2"}))["body"]["items"].is_array());
    assert_eq!(r.log(), log, "nothing recorded with writes off");
}
