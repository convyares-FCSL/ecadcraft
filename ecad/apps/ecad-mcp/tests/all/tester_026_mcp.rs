//! Tester 026 (numbering) through MCP: the two-transaction exception of `issuerevision
//! {provisional: "finalise"}` and nothing else, and merges with `Choice::MakeOne`, renames on both
//! sides, provisional duplicates and fixed flags. A probe that found a bug is
//! `#[ignore = "tester 026: <bug>"]` and fails until it is fixed.

use std::collections::BTreeSet;

use ecad_mcp::{EcadMcp, Options};
use ecad_model::{ComponentId, Document, tag};
use serde_json::{Value, json};

use crate::mcp_contracts::{Client, TempDir};

fn writer() -> Client {
    Client::new(EcadMcp::in_memory("t026", 0x0E_CAD0_7026, Options { write: true, agent: None }).unwrap())
}

fn place(c: &mut Client, symbol: &str, at: [f64; 2], tag: Option<&str>) -> String {
    let mut p = json!({"symbol": symbol, "at": at});
    if let Some(t) = tag {
        p["tag"] = json!(t);
    }
    c.exec("place", p)["component"].as_str().unwrap().to_string()
}

fn tag_of(doc: &Document, comp: &str) -> Option<String> {
    doc.components.get(&ComponentId::parse(comp).unwrap()).and_then(|x| x.properties.get("tag").cloned())
}

fn assert_unique(doc: &Document, step: &str) {
    let mut seen = BTreeSet::new();
    for t in doc.components.values().filter_map(|c| c.properties.get("tag")).chain(doc.cables.values().filter_map(|c| c.properties.get("tag"))) {
        assert!(seen.insert(tag::key(t)), "{step}: duplicate tag {t}");
    }
}

fn revision(c: &mut Client, rev: &str) {
    for (f, v) in [("rev", rev), ("description", "Issue"), ("drawn", "CV")] {
        c.exec("revision", json!({f: v}));
    }
}

fn exec_call(id: u64, command: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": "execute", "arguments": {"command": command, "params": params}}})
}

fn batch(c: &mut Client, msgs: Vec<Value>) -> Vec<Value> {
    let reply = c.s.handle_line(&Value::Array(msgs).to_string()).expect("a reply");
    serde_json::from_str::<Value>(&reply).unwrap().as_array().unwrap().clone()
}

fn is_error(v: &Value) -> bool {
    v["result"]["isError"] == json!(true) || v.get("error").is_some()
}

/// A project with two provisional coils and a filled revision row.
fn provisional_project() -> (Client, String, String) {
    let mut c = writer();
    c.exec("numbering", json!({"provisional": true}));
    let a = place(&mut c, "coil", [100.0, 100.0], None);
    let b = place(&mut c, "coil", [200.0, 100.0], None);
    revision(&mut c, "A");
    (c, a, b)
}

// ---------------------------------------------------------------- 3. the two-transaction exception

/// In one request line, an edit before `issuerevision {provisional: "finalise"}` exceeds the cap
/// (1 + 2 > 2): the issue's two transactions are taken back and the design is as before it.
/// The gate takes an issue back with `u`, which an issue refuses (it needs `undoissue`), so the
/// call is refused but Finalise and the issue stay ("The project is NOT as before").
#[test]
fn an_edit_before_issue_finalise_in_one_line_is_refused_and_taken_back() {
    let (mut c, a, b) = provisional_project();
    let replies = batch(
        &mut c,
        vec![exec_call(1, "wire", json!({"points": [[0, 0], [60, 0]]})), exec_call(2, "issuerevision", json!({"provisional": "finalise"}))],
    );
    assert!(!is_error(&replies[0]), "{}", replies[0]);
    assert!(is_error(&replies[1]), "{}", replies[1]);
    let m = replies[1]["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string();
    assert!(m.contains("taken back"), "{m}");
    let doc = c.doc();
    assert_eq!((tag_of(&doc, &a).as_deref(), tag_of(&doc, &b).as_deref()), (Some("-K?1"), Some("-K?2")), "Finalise was taken back");
    assert!(doc.versions.is_empty(), "the issue was taken back: {:?}", doc.versions);
}

/// A plain `issuerevision` after an edit on the same line goes over the cap of one (019's gap,
/// reachable before 026): the issue is taken back with `undoissue`, so nothing stays issued.
#[test]
fn an_edit_before_a_plain_issue_in_one_line_is_refused_and_taken_back() {
    let mut c = writer();
    revision(&mut c, "A");
    let replies = batch(&mut c, vec![exec_call(1, "wire", json!({"points": [[0, 0], [60, 0]]})), exec_call(2, "issuerevision", json!({}))]);
    assert!(!is_error(&replies[0]), "{}", replies[0]);
    assert!(is_error(&replies[1]), "{}", replies[1]);
    let m = replies[1]["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string();
    assert!(m.contains("taken back") && !m.contains("NOT as before"), "{m}");
    assert!(c.doc().versions.is_empty(), "the issue was taken back");
}

/// After `issuerevision {provisional: "finalise"}` (two transactions), any further write on the
/// same line is refused and taken back; the issue and Finalise stay.
#[test]
fn an_edit_after_issue_finalise_in_one_line_is_refused() {
    let (mut c, a, _) = provisional_project();
    let replies = batch(
        &mut c,
        vec![exec_call(1, "issuerevision", json!({"provisional": "finalise"})), exec_call(2, "wire", json!({"points": [[0, 0], [60, 0]]}))],
    );
    assert!(!is_error(&replies[0]), "{}", replies[0]);
    assert!(is_error(&replies[1]), "{}", replies[1]);
    let doc = c.doc();
    assert_eq!(tag_of(&doc, &a).as_deref(), Some("-K01"));
    assert_eq!(doc.versions.len(), 1);
    assert!(doc.conductors.is_empty(), "the wire was taken back");
}

/// Only `issuerevision` with `provisional: "finalise"` gets the second transaction: the same
/// parameter on other commands, `"issue"`, and other spellings get one.
#[test]
fn the_exception_is_only_issuerevision_with_finalise() {
    for (cmd, params) in [
        ("issuerevision", json!({"provisional": "issue"})),
        ("finalise", json!({"provisional": "finalise"})),
        ("renumber", json!({"provisional": "finalise"})),
        ("numbering", json!({"provisional": true})),
    ] {
        assert_eq!(ecad_mcp::policy::transactions_allowed(cmd, &params), ecad_mcp::MAX_TRANSACTIONS_PER_CALL, "{cmd} {params}");
    }
    assert_eq!(ecad_mcp::policy::transactions_allowed("issuerevision", &json!({"provisional": " Finalize "})), 2);
    assert_eq!(ecad_mcp::policy::transactions_allowed("issuerevision", &json!({"provisional": true})), 1);
    // An alias of the command is resolved to its id before the check.
    let (mut c, a, _) = provisional_project();
    let n = c.log().len();
    let r = c.call("execute", json!({"command": "ISSUEREVISION", "params": {"provisional": "finalise"}}));
    assert_ne!(r["isError"], json!(true), "{r}");
    assert_eq!(c.log().len(), n + 2);
    assert_eq!(tag_of(&c.doc(), &a).as_deref(), Some("-K01"));
}

/// The issue refused after the exception's gate: nothing is recorded, and with nothing
/// provisional, `"finalise"` records the issue alone.
#[test]
fn issue_finalise_that_is_refused_records_nothing_and_without_provisional_records_one() {
    let mut c = writer();
    c.exec("numbering", json!({"provisional": true}));
    let a = place(&mut c, "coil", [100.0, 100.0], None);
    let n = c.log().len();
    let m = c.refused("execute", json!({"command": "issuerevision", "params": {"provisional": "finalise"}}));
    assert_eq!(c.log().len(), n, "fields missing: nothing written ({m})");
    assert_eq!(tag_of(&c.doc(), &a).as_deref(), Some("-K?1"));
    c.exec("finalise", json!({}));
    revision(&mut c, "A");
    let n = c.log().len();
    c.exec("issuerevision", json!({"provisional": "finalise"}));
    assert_eq!(c.log().len(), n + 1, "nothing provisional: the issue alone");
    // Undo through MCP after the two-transaction form: one step each.
    let b = place(&mut c, "coil", [300.0, 100.0], None);
    revision(&mut c, "B");
    c.exec("issuerevision", json!({"provisional": "finalise"}));
    assert_eq!(tag_of(&c.doc(), &b).as_deref(), Some("-K02"));
    c.exec("undoissue", json!({}));
    assert_eq!(tag_of(&c.doc(), &b).as_deref(), Some("-K02"), "the first undo takes back the issue only");
    c.exec("u", json!({}));
    assert_eq!(tag_of(&c.doc(), &b).as_deref(), Some("-K?1"), "the second takes back Finalise");
}

// ---------------------------------------------------------------- 4. merge and update from main

/// A file project with `-K01` (coil, X), saved; then a branch.
fn merge_project(dir: &TempDir, name: &str) -> (Client, String) {
    let path = dir.file(name);
    let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
    let x = place(&mut c, "coil", [0.0, 0.0], Some("-K01"));
    c.exec("wire", json!({"points": [[0, 40], [60, 40]]}));
    c.ok("save", json!({}));
    (c, x)
}

fn conflicts(pv: &Value) -> Vec<Value> {
    pv["conflicts"].as_array().cloned().unwrap_or_default()
}

/// Both sides rename the same device differently, and both add a new device with the same tag:
/// the rename is a property conflict, the new pair a tag conflict with `one`; any answer leaves
/// the tags unique.
#[test]
fn merge_with_the_same_device_renamed_on_both_sides_and_a_tag_pair_made_one() {
    let dir = TempDir::new("t026-merge-rename");
    let (mut c, x) = merge_project(&dir, "rename.ecad");
    c.exec("branch", json!({"name": "Option R"}));
    c.exec("setprop", json!({"component": x, "key": "tag", "value": "-K07"}));
    place(&mut c, "coil", [100.0, 100.0], Some("-K05"));
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
    c.exec("setprop", json!({"component": x, "key": "tag", "value": "-K08"}));
    place(&mut c, "no", [200.0, 100.0], Some("-K05"));
    let pv = c.exec("merge", json!({"from": "Option R", "preview": true}));
    let ks = conflicts(&pv);
    let mut choices = serde_json::Map::new();
    for k in &ks {
        let id = k["id"].as_str().unwrap().to_string();
        let opts: Vec<String> =
            k["choices"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
        let pick = if opts.iter().any(|o| o == "one") { "one".to_string() } else { "branch".to_string() };
        choices.insert(id, json!(pick));
    }
    assert!(ks.iter().any(|k| k["kind"] == json!("tag")), "{pv}");
    let r = c.exec("merge", json!({"from": "Option R", "choices": Value::Object(choices)}));
    assert_eq!(r["merged"], json!(true), "{r}");
    let doc = c.doc();
    assert_unique(&doc, "after the merge");
    assert_eq!(tag_of(&doc, &x).as_deref(), Some("-K07"), "the branch's rename was chosen");
}

/// The branch renames X to `-K05` while main adds a new device `-K05`: a tag clash between an
/// existing and a new device; `one` or a renumber choice, never two `-K05`.
#[test]
fn merge_where_one_side_renames_into_the_others_new_tag() {
    for pick in ["one", "branch-renumber", "main-renumber"] {
        let dir = TempDir::new(&format!("t026-merge-into-{pick}"));
        let (mut c, x) = merge_project(&dir, "into.ecad");
        c.exec("branch", json!({"name": "Option S"}));
        c.exec("setprop", json!({"component": x, "key": "tag", "value": "-K05"}));
        c.exec("switchbranch", json!({"branch": "main", "save": true}));
        place(&mut c, "no", [200.0, 100.0], Some("-K05"));
        let pv = c.exec("merge", json!({"from": "Option S", "preview": true}));
        let ks = conflicts(&pv);
        let Some(k) = ks.iter().find(|k| k["kind"] == json!("tag")) else {
            // No tag row: the merge must still not make a duplicate.
            let r = c.call("execute", json!({"command": "merge", "params": {"from": "Option S"}}));
            assert_unique(&c.doc(), &format!("{pick}: no tag row, {r}"));
            continue;
        };
        let opts: Vec<String> = k["choices"].as_array().unwrap().iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
        let choice = match pick {
            "one" => "one".to_string(),
            "branch-renumber" => opts.iter().find(|o| o.contains("branch") || o.as_str() == "source").cloned().unwrap_or_else(|| opts[0].clone()),
            _ => opts
                .iter()
                .find(|o| o.contains("main") || o.as_str() == "target")
                .cloned()
                .unwrap_or_else(|| opts[opts.len().saturating_sub(1)].clone()),
        };
        let id = k["id"].as_str().unwrap().to_string();
        let mut ch = serde_json::Map::new();
        ch.insert(id, json!(choice));
        for other in ks.iter().filter(|o| o["kind"] != json!("tag")) {
            ch.insert(other["id"].as_str().unwrap().to_string(), json!("branch"));
        }
        let r = c.call("execute", json!({"command": "merge", "params": {"from": "Option S", "choices": Value::Object(ch)}}));
        assert_unique(&c.doc(), &format!("{pick} ({choice}): {r}"));
    }
}

/// Update from main with `one`: main's device joins the branch's; tags stay unique on the branch.
#[test]
fn update_from_main_with_one_joins_and_keeps_tags_unique() {
    let dir = TempDir::new("t026-update-one");
    let (mut c, _x) = merge_project(&dir, "update.ecad");
    c.exec("branch", json!({"name": "Option U"}));
    let bk = place(&mut c, "coil", [100.0, 100.0], Some("-K05"));
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
    let mk = place(&mut c, "no", [200.0, 100.0], Some("-K05"));
    c.ok("save", json!({}));
    c.exec("switchbranch", json!({"branch": "Option U", "save": true}));
    let pv = c.exec("merge", json!({"from": "main", "preview": true}));
    let k = conflicts(&pv).into_iter().find(|k| k["kind"] == json!("tag")).unwrap_or_else(|| panic!("{pv}"));
    let id = k["id"].as_str().unwrap().to_string();
    let r = c.exec("merge", json!({"from": "main", "choices": {id: "one"}}));
    assert_eq!(r["merged"], json!(true), "{r}");
    let doc = c.doc();
    assert_unique(&doc, "after the update");
    let bid = ComponentId::parse(&bk).unwrap();
    assert!(doc.components.contains_key(&bid), "the branch keeps its device");
    assert!(!doc.components.contains_key(&ComponentId::parse(&mk).unwrap()), "main's device joined the branch's");
    assert_eq!(doc.representations_of(ecad_model::Subject::Component(bid)).count(), 2);
}

/// Both sides add `-K?1`: no conflict row, the branch's gets the next provisional number.
#[test]
fn provisional_duplicates_in_a_merge_are_renumbered_silently() {
    let dir = TempDir::new("t026-merge-prov");
    let (mut c, _x) = merge_project(&dir, "prov.ecad");
    c.exec("numbering", json!({"provisional": true}));
    c.ok("save", json!({}));
    c.exec("branch", json!({"name": "Option P"}));
    let bk = place(&mut c, "coil", [100.0, 100.0], None);
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
    let mk = place(&mut c, "coil", [200.0, 100.0], None);
    let doc = c.doc();
    assert_eq!(tag_of(&doc, &mk).as_deref(), Some("-K?1"));
    let pv = c.exec("merge", json!({"from": "Option P", "preview": true}));
    assert!(!conflicts(&pv).iter().any(|k| k["kind"] == json!("tag")), "{pv}");
    c.exec("merge", json!({"from": "Option P"}));
    let doc = c.doc();
    assert_unique(&doc, "provisional merge");
    assert_eq!(tag_of(&doc, &mk).as_deref(), Some("-K?1"));
    assert_eq!(tag_of(&doc, &bk).as_deref(), Some("-K?2"));
}

/// A merge can join a branch's `tag_fixed` with main's provisional tag on one device (each side
/// changed one property). Finalise then skips it as fixed, so a provisional tag survives
/// Finalise, and `issuerevision {provisional: "finalise"}` issues it without saying so.
#[test]
fn a_merged_fixed_flag_on_a_provisional_tag_does_not_survive_finalise() {
    let dir = TempDir::new("t026-merge-fixed-prov");
    let (mut c, x) = merge_project(&dir, "fixedprov.ecad");
    c.exec("branch", json!({"name": "Option F"}));
    c.exec("setprop", json!({"component": x, "key": "tag_fixed", "value": "yes"}));
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
    c.exec("numbering", json!({"provisional": true}));
    c.exec("setprop", json!({"component": x, "key": "tag", "value": "-K?"}));
    assert_eq!(tag_of(&c.doc(), &x).as_deref(), Some("-K?1"));
    let pv = c.exec("merge", json!({"from": "Option F", "preview": true}));
    let mut ch = serde_json::Map::new();
    for k in conflicts(&pv) {
        ch.insert(k["id"].as_str().unwrap().to_string(), json!("main"));
    }
    c.exec("merge", json!({"from": "Option F", "choices": Value::Object(ch)}));
    c.exec("finalise", json!({}));
    let doc = c.doc();
    let t = tag_of(&doc, &x).unwrap_or_default();
    let fixed = doc.components[&ComponentId::parse(&x).unwrap()].properties.get("tag_fixed").cloned();
    assert!(!tag::is_provisional(&t), "Finalise left {t} (tag_fixed {fixed:?})");
}
