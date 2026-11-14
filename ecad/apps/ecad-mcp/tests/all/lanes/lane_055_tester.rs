//! Tester 055 through MCP (brief 011 rule; design 055 §9 and §12.1 K and L). On a project saved at
//! a path with spaces and non-ASCII letters, each writing form (`titleblock` new, `titleblock`
//! write, `titleblockimport` new frame and logo) records one transaction by
//! `Author::Agent(<client>)`, and one `u` restores the document; `titleblocks` and `titleblock {id}`
//! record nothing; with writes off every write is refused with `WRITES_OFF` and reads are
//! unchanged. Branches: the same frame changed on two branches is one conflict worded with the
//! frame's name; different frames merge cleanly; the merged result survives save and reopen.
//!
//! Written from brief 055 and design 055 before the implementation. Body JSON is read tolerantly
//! and items written are copies of items the command returned.

use ecad_mcp::{EcadMcp, Options, policy};
use ecad_model::{Author, Document, Op};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, TempDir};

fn s(v: &Value) -> String {
    match v {
        Value::String(x) => x.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn create(path: &str) -> Client {
    Client::new(EcadMcp::create(path, Options { write: true, agent: None }).unwrap())
}

fn open(path: &str) -> Client {
    Client::new(EcadMcp::open(path, Options { write: true, agent: None }).unwrap().0)
}

fn id_of(v: &Value) -> String {
    ["id", "style"].iter().map(|k| s(&v[*k])).find(|x| x.starts_with('#')).unwrap_or_else(|| panic!("no own frame ID in {v}"))
}

fn body_of(v: &Value) -> Value {
    for b in [&v["body"]["title_block"], &v["body"]["TitleBlock"], &v["body"], v] {
        if b["items"].is_array() {
            return b.clone();
        }
    }
    panic!("no body with items in {v}")
}

/// The inner fields object of item `i` of `body`, mutable (internally or externally tagged).
fn item_mut(body: &mut Value, i: usize) -> &mut Value {
    let item = &mut body["items"][i];
    if ["kind", "type", "item"].iter().any(|k| item[*k].is_string()) {
        return item;
    }
    let key = item.as_object().and_then(|m| m.keys().next().cloned()).unwrap();
    &mut item[key]
}

/// The index of the first item whose JSON contains `needle`.
fn index_of(body: &Value, needle: &str) -> usize {
    body["items"].as_array().unwrap().iter().position(|i| i.to_string().contains(needle)).unwrap_or_else(|| panic!("{needle} in {body}"))
}

/// The command recorded one transaction by the agent; `u` restores `before` exactly; redo.
fn one_by_agent_then_undo(c: &mut Client, cmd: &str, params: Value) -> (Value, Vec<Op>) {
    let before: Document = c.doc();
    let n = c.log().len();
    let v = c.exec(cmd, params.clone());
    let log = c.log();
    assert_eq!(log.len(), n + 1, "{cmd}: one transaction");
    let last = log.last().cloned().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()), "{cmd}: {}", last.label);
    let ops = last.ops.clone();
    c.exec("u", json!({}));
    assert!(c.doc() == before, "{cmd}: one undo restores the document");
    c.exec("redo", json!({}));
    (v, ops)
}

fn reads(c: &mut Client) -> Value {
    json!({
        "titleblocks": c.exec("titleblocks", json!({})),
        "sheet": c.exec("titleblock", json!({"id": "sheet"})),
        "checks": c.exec("checks", json!({})),
    })
}

fn logo_dxf(n: usize) -> String {
    let mut out = String::from("0\nSECTION\n2\nENTITIES\n");
    for i in 0..n {
        let x = i as f64 * 0.5;
        out.push_str(&format!("0\nLINE\n8\n0\n10\n{x}\n20\n0\n30\n0\n11\n{x}\n21\n10\n31\n0\n"));
    }
    out.push_str("0\nENDSEC\n0\nEOF\n");
    out
}

fn block_dxf() -> String {
    let mut out = String::from("0\nSECTION\n2\nENTITIES\n");
    for (a, b) in [((0.0, 0.0), (300.0, 0.0)), ((300.0, 0.0), (300.0, 30.0)), ((300.0, 30.0), (0.0, 30.0)), ((0.0, 30.0), (0.0, 0.0))] {
        out.push_str(&format!("0\nLINE\n8\n0\n10\n{}\n20\n{}\n30\n0\n11\n{}\n21\n{}\n31\n0\n", a.0, a.1, b.0, b.1));
    }
    out.push_str("0\nTEXT\n8\n0\n10\n5\n20\n20\n30\n0\n40\n2.5\n1\nTITLE\n0\nENDSEC\n0\nEOF\n");
    out
}

/// §12.1 L: every writing form by the agent, one transaction, one undo; reads record nothing;
/// refusals record nothing; then with writes off every write is refused and reads are unchanged.
#[test]
#[ignore = "tester 055: awaiting build"]
fn l_title_block_writes_by_the_agent_then_refused_with_writes_off() {
    let dir = TempDir::new("t055 Schriftfeld mit Leerzeichen \u{e4}");
    let path = dir.file("Projekt 055 (A).ecad");
    let (before, own) = {
        let mut c = create(&path);
        let n = c.log().len();
        let rows = c.exec("titleblocks", json!({}));
        assert!(rows.to_string().contains("sheet"), "the built-in is listed: {rows}");
        c.exec("titleblock", json!({"id": "sheet"}));
        assert_eq!(c.log().len(), n, "reads record nothing");

        let (v, ops) = one_by_agent_then_undo(&mut c, "titleblock", json!({"new": {"from": "sheet", "sheet": "A3"}}));
        assert_eq!(ops.iter().map(Op::name).collect::<Vec<_>>(), ["SetStyle"]);
        let own = id_of(&v);
        let (v, _) = one_by_agent_then_undo(&mut c, "titleblock", json!({"new": {"from": "blank", "sheet": "A2", "name": "Blank A2"}}));
        let _blank = id_of(&v);
        let read = c.exec("titleblock", json!({"id": own}));
        let mut body = body_of(&read);
        let i = index_of(&body, "drawing_number");
        let x = item_mut(&mut body, i)["at"]["x"].as_f64().unwrap();
        item_mut(&mut body, i)["at"]["x"] = json!(x - 10.0);
        let (_, ops) = one_by_agent_then_undo(&mut c, "titleblock", json!({"id": own, "body": body}));
        assert_eq!(ops.iter().map(Op::name).collect::<Vec<_>>(), ["SetStyle"], "a write is one SetStyle");
        let (v, _) = one_by_agent_then_undo(&mut c, "titleblockimport", json!({"dxf": block_dxf(), "sheet": "A3", "name": "Imported"}));
        let _imported = id_of(&v);
        let body = body_of(&c.exec("titleblock", json!({"id": own})));
        let logo = body["items"].as_array().unwrap().iter().position(|i| i.to_string().to_lowercase().contains("logo")).expect("a logo box");
        one_by_agent_then_undo(&mut c, "titleblockimport", json!({"id": own, "item": logo, "dxf": logo_dxf(120)}));
        one_by_agent_then_undo(&mut c, "setdefault", json!({"key": "print.frame.style", "level": "project", "value": own}));

        // Refusals record nothing.
        let n = c.log().len();
        for (cmd, params) in [
            ("titleblock", json!({"id": "sheet", "body": body})),
            ("titleblock", json!({"new": {"from": "sheet", "sheet": "A9"}})),
            ("titleblock", json!({"id": own, "body": {"size": [3000, 40], "items": []}})),
            ("titleblockimport", json!({"id": own, "item": logo, "dxf": logo_dxf(3_400)})),
            ("titleblockimport", json!({"dxf": "not a dxf", "sheet": "A3"})),
            // Agents cannot read files: a path is refused as for 050a's `standard {from}`, or not
            // found; either way nothing is written.
            ("titleblockimport", json!({"path": "C:\\Windows\\win.ini", "sheet": "A3"})),
        ] {
            let m = c.refused("execute", json!({"command": cmd, "params": params}));
            assert!(!m.is_empty(), "{cmd}");
        }
        assert_eq!(c.log().len(), n, "refusals record nothing");
        let before = reads(&mut c);
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
        (before, own)
    };

    let (sess, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut c = Client::new(sess);
    assert!(!c.s.writes());
    let log = c.log();
    let body = body_of(&c.exec("titleblock", json!({"id": own})));
    for (cmd, params) in [
        ("titleblock", json!({"new": {"from": "sheet", "sheet": "A3"}})),
        ("titleblock", json!({"id": own, "body": body})),
        ("titleblockimport", json!({"dxf": block_dxf(), "sheet": "A3"})),
        ("titleblockimport", json!({"id": own, "item": 0, "dxf": logo_dxf(3)})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd} {params}");
    }
    assert_eq!(reads(&mut c), before, "reads after reopen with writes off");
    assert_eq!(c.log(), log, "nothing recorded with writes off");
}

/// §9: the new commands are listed with parameter docs.
#[test]
#[ignore = "tester 055: awaiting build"]
fn l_the_new_commands_are_listed() {
    let mut c = crate::mcp_contracts::reader();
    let catalog = c.ok("list_commands", json!({}));
    let catalog = catalog.as_array().cloned().unwrap_or_default();
    for id in ["titleblocks", "titleblock", "titleblockimport"] {
        let info = catalog.iter().find(|i| i["id"] == id).unwrap_or_else(|| panic!("{id} not listed"));
        assert!(info["params"].as_str().is_some_and(|p| !p.is_empty()), "{id} has parameter docs");
    }
}

fn to_main(c: &mut Client) {
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
}

fn verify(path: &str) {
    let v = ecad_file::v2::verify::verify(&ecad_file::v2::Store::open(std::path::Path::new(path), false).unwrap());
    assert!(v.ok, "{:#?}", v.lines);
}

/// Move item `needle` of frame `id` by `dx` mm.
fn nudge(c: &mut Client, id: &str, needle: &str, dx: f64) {
    let mut body = body_of(&c.exec("titleblock", json!({"id": id})));
    let i = index_of(&body, needle);
    let x = item_mut(&mut body, i)["at"]["x"].as_f64().unwrap();
    item_mut(&mut body, i)["at"]["x"] = json!(x + dx);
    c.exec("titleblock", json!({"id": id, "body": body}));
}

/// §12.1 K: two branches editing the same frame (different items) are one conflict worded with
/// the frame's name; each choice merges; two branches editing different frames merge cleanly;
/// the merged result survives save and reopen and verify passes.
#[test]
#[ignore = "tester 055: awaiting build"]
fn k_a_frames_record_is_one_merge_unit_named_by_the_frame() {
    let dir = TempDir::new("t055-merge");
    let path = dir.file("merge.ecad");
    let mut c = create(&path);
    let a3 = id_of(&c.exec("titleblock", json!({"new": {"from": "sheet", "sheet": "A3", "name": "A3 Northfield"}})));
    let a2 = id_of(&c.exec("titleblock", json!({"new": {"from": "sheet", "sheet": "A2", "name": "A2 Northfield"}})));
    c.ok("save", json!({}));
    c.exec("branch", json!({"name": "Option B"}));
    nudge(&mut c, &a3, "drawing_number", 10.0);
    nudge(&mut c, &a2, "drawing_number", 5.0);
    to_main(&mut c);
    nudge(&mut c, &a3, "customer", -10.0);
    let pv = c.exec("merge", json!({"from": "Option B", "preview": true}));
    let ks = pv["conflicts"].as_array().cloned().unwrap_or_default();
    assert_eq!(ks.len(), 1, "one unit per frame record, one conflict: {pv}");
    assert!(s(&ks[0]["text"]).contains("A3 Northfield"), "worded with the frame's name: {ks:?}");
    let kid = s(&ks[0]["id"]);
    for choice in ["branch", "main"] {
        let choices: serde_json::Map<String, Value> = [(kid.clone(), json!(choice))].into_iter().collect();
        let r = c.exec("merge", json!({"from": "Option B", "choices": choices}));
        assert_eq!(r["merged"], json!(true), "{choice}: {r}");
        c.exec("undomerge", json!({}));
    }
    let choices: serde_json::Map<String, Value> = [(kid, json!("branch"))].into_iter().collect();
    c.exec("merge", json!({"from": "Option B", "choices": choices}));
    let want = c.doc();
    c.ok("save", json!({}));
    drop(c);
    verify(&path);
    let mut c = open(&path);
    assert!(c.doc() == want, "reopen gives the merged state");
}
