//! Tester 050 (style model and wire categories) through MCP (brief 011 rule; design 050 §6.8 and
//! §12 checks 8, 10 and 18; the overseer's FP1–FP3 ruling: merge tests). On a project saved at a
//! path with spaces and non-ASCII letters, each new or changed writing command (`standard` new,
//! name, from, remove; `style`; `deletestyle`; `promote`; `setdefault` at `standard`; 050b's
//! `category` and `wire {category}`) records one transaction by `Author::Agent(<client>)`, and one
//! `u` restores the document; `styles` and `standard {}` record nothing; with writes off every
//! write is refused and reads are unchanged. Branches: the same style changed differently on two
//! branches is one conflict worded with the style's name; different styles, different standard
//! values and a concurrent drawing edit merge cleanly; the standard header is one unit.
//!
//! Written from the briefs and design before the implementation.

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

fn own_id(v: &Value) -> String {
    let i = ["id", "style"].iter().map(|k| s(&v[*k])).find(|x| x.starts_with('#')).unwrap_or_else(|| panic!("no own style ID in {v}"));
    assert_eq!(i.len(), 33, "{i}");
    i
}

fn styles(c: &mut Client, catalogue: &str) -> Vec<Value> {
    match c.exec("styles", json!({"catalogue": catalogue})) {
        Value::Array(a) => a,
        other => other["styles"].as_array().cloned().unwrap_or_default(),
    }
}

fn field(c: &mut Client, catalogue: &str, which: &str, key: &str) -> String {
    let rows = styles(c, catalogue);
    let r = rows.iter().find(|r| s(&r["id"]) == which || s(&r["name"]) == which).unwrap_or_else(|| panic!("{which} in {rows:?}"));
    s(&r["fields"][key]["value"])
}

/// The command recorded one transaction by the agent; `u` restores `before` exactly.
fn one_by_agent_then_undo(c: &mut Client, cmd: &str, params: Value) -> (Value, Vec<Op>) {
    let before: Document = c.doc();
    let n = c.log().len();
    let v = c.exec(cmd, params.clone());
    let log = c.log();
    assert_eq!(log.len(), n + 1, "{cmd} {params}: one transaction");
    let last = log.last().cloned().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()), "{cmd}: {}", last.label);
    let ops = last.ops.clone();
    c.exec("u", json!({}));
    assert!(c.doc() == before, "{cmd} {params}: one undo restores the document");
    c.exec("redo", json!({}));
    (v, ops)
}

fn reads(c: &mut Client) -> Value {
    json!({
        "standard": c.exec("standard", json!({})),
        "styles": c.exec("styles", json!({"users": true})),
        "defaults": c.exec("defaults", json!({})),
        "checks": c.exec("checks", json!({})),
    })
}

fn merge_preview(c: &mut Client, from: &str) -> Value {
    c.exec("merge", json!({"from": from, "preview": true}))
}

// ---------------------------------------------------------------------------- 050a

/// §6.8: every 050a writing command, by the agent, one transaction, one undo; then writes off.
#[test]
fn a10_style_model_writes_by_the_agent_then_refused_with_writes_off() {
    let dir = TempDir::new("t050 Hausstandard mit Leerzeichen \u{e4}");
    let other = dir.file("Panel Std (other).ecad");
    {
        let mut o = create(&other);
        o.exec("standard", json!({"new": "Panel Std"}));
        o.exec("setdefault", json!({"key": "symbol.coil.prefix", "level": "standard", "value": "Q"}));
        o.ok("save", json!({}));
    }
    let path = dir.file("Projekt 050 (A).ecad");
    let before = {
        let mut c = create(&path);
        let k = c.exec("place", json!({"symbol": "coil", "at": [100, 100]}))["component"].as_str().unwrap().to_string();

        // Reads record nothing.
        let n = c.log().len();
        let r = c.exec("standard", json!({}));
        assert_eq!(s(&r["name"]), "IEC (built in)");
        // 031's kinds, 049's continuation caption and 051's two.
        assert_eq!(styles(&mut c, "text").len(), 23);
        assert_eq!(c.log().len(), n, "reads record nothing");

        let (_, ops) = one_by_agent_then_undo(&mut c, "standard", json!({"new": "House"}));
        assert_eq!(ops.iter().map(Op::name).collect::<Vec<_>>(), ["SetStandard"]);
        one_by_agent_then_undo(&mut c, "standard", json!({"name": "House 2026"}));
        let (_, ops) = one_by_agent_then_undo(&mut c, "setdefault", json!({"key": "symbol.coil.prefix", "level": "standard", "value": "Q"}));
        assert!(matches!(ops.as_slice(), [Op::SetProperty { object: ecad_model::ObjectRef::Standard, .. }]), "{ops:?}");
        let (_, ops) = one_by_agent_then_undo(&mut c, "setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"colour": "RD", "height": 3}}));
        assert_eq!(ops.iter().map(Op::name).collect::<Vec<_>>(), ["SetStyle"]);
        let (v, _) = one_by_agent_then_undo(
            &mut c,
            "setstyle",
            json!({"catalogue": "text", "new": true, "name": "Tag large", "copy_of": "tag", "fields": {"height": 5}}),
        );
        let large = own_id(&v);
        one_by_agent_then_undo(&mut c, "setdefault", json!({"key": "text.tag.style", "level": "object", "component": k, "value": large}));
        one_by_agent_then_undo(&mut c, "deletestyle", json!({"catalogue": "text", "id": large, "replace_with": "tag"}));
        // The style is gone after redo of the delete: make another for promote.
        let (v, _) = one_by_agent_then_undo(&mut c, "setstyle", json!({"catalogue": "text", "new": true, "name": "Promoted", "copy_of": "tag"}));
        let p = own_id(&v);
        one_by_agent_then_undo(&mut c, "promote", json!({"catalogue": "text", "id": p}));
        // Agents cannot read other files: MCP refuses `from` (brief 050a deviation), recording nothing.
        let n = c.log().len();
        let m = c.refused("execute", json!({"command": "standard", "params": {"from": other}}));
        assert!(m.contains("`from` names a file"), "{m}");
        assert_eq!(c.log().len(), n);
        let (v, _) = one_by_agent_then_undo(&mut c, "standard", json!({"remove": true}));
        assert!(!v.is_null());
        c.exec("standard", json!({"new": "Final"}));
        c.exec("setstyle", json!({"catalogue": "location", "id": "box", "layer": "standard", "fields": {"line_type": "continuous"}}));

        // Refusals record nothing.
        let n = c.log().len();
        for (cmd, params) in [
            ("setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"height": 11}})),
            ("deletestyle", json!({"catalogue": "text", "id": "tag"})),
            ("standard", json!({"new": "Twice"})),
            ("promote", json!({"catalogue": "text", "id": "tag"})),
            ("setstyle", json!({"catalogue": "text", "new": true, "name": "x".repeat(10_000), "copy_of": "tag"})),
        ] {
            let m = c.refused("execute", json!({"command": cmd, "params": params}));
            assert!(!m.is_empty(), "{cmd}");
        }
        assert_eq!(c.log().len(), n, "refusals record nothing");
        let before = reads(&mut c);
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
        before
    };

    // Writes off: every write refused, nothing recorded, reads unchanged.
    let (sess, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut c = Client::new(sess);
    assert!(!c.s.writes());
    let log = c.log();
    for (cmd, params) in [
        ("standard", json!({"name": "Renamed"})),
        ("standard", json!({"remove": true})),
        ("setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"height": 3}})),
        ("setstyle", json!({"catalogue": "text", "new": true, "name": "New", "copy_of": "tag"})),
        ("deletestyle", json!({"catalogue": "text", "id": "#00000000000000000000000000000001", "replace_with": "tag"})),
        ("promote", json!({"key": "symbol.coil.prefix"})),
        ("setdefault", json!({"key": "symbol.coil.prefix", "level": "standard", "value": "Z"})),
        ("textstyle", json!({"kind": "tag", "height": 3})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd} {params}");
    }
    assert_eq!(reads(&mut c), before, "reads after reopen with writes off");
    assert_eq!(c.log(), log, "nothing recorded with writes off");
}

/// The new commands are listed with parameter docs.
#[test]
fn a10_the_new_commands_are_listed() {
    let mut c = crate::mcp_contracts::reader();
    let catalog = c.ok("list_commands", json!({}));
    let catalog = catalog.as_array().cloned().unwrap_or_default();
    for id in ["standard", "setstyle", "deletestyle", "promote", "styles", "category"] {
        let info = catalog.iter().find(|i| i["id"] == id).unwrap_or_else(|| panic!("{id} not listed"));
        assert!(info["params"].as_str().is_some_and(|p| !p.is_empty()), "{id} has parameter docs");
    }
}

/// A file project with a standard and a coil, saved; a branch "Option B".
fn branched(dir: &TempDir, name: &str) -> (Client, String) {
    let path = dir.file(name);
    let mut c = create(&path);
    c.exec("standard", json!({"new": "House"}));
    c.exec("place", json!({"symbol": "coil", "at": [0, 0]}));
    c.ok("save", json!({}));
    c.exec("branch", json!({"name": "Option B"}));
    (c, path)
}

fn to_main(c: &mut Client) {
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
}

fn verify(path: &str) {
    let v = ecad_file::v2::verify::verify(&ecad_file::v2::Store::open(std::path::Path::new(path), false).unwrap());
    assert!(v.ok, "{:#?}", v.lines);
}

/// §3.5, FP3 ruling: the same style record changed differently on both branches is one conflict,
/// worded with the style's name, whose every choice merges; the result survives save and reopen.
#[test]
fn fp3_the_same_style_on_both_branches_is_one_conflict_named_by_the_style() {
    let dir = TempDir::new("t050-merge-same");
    let path = dir.file("same.ecad");
    let mut c = create(&path);
    c.exec("standard", json!({"new": "House"}));
    let large = own_id(&c.exec("setstyle", json!({"catalogue": "text", "new": true, "name": "Tag large", "copy_of": "tag"})));
    c.ok("save", json!({}));
    c.exec("branch", json!({"name": "Option B"}));
    c.exec("setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"height": 4}}));
    c.exec("setstyle", json!({"catalogue": "text", "id": large, "fields": {"height": 6}}));
    to_main(&mut c);
    c.exec("setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"height": 3}}));
    c.exec("setstyle", json!({"catalogue": "text", "id": large, "fields": {"height": 7}}));
    let pv = merge_preview(&mut c, "Option B");
    let ks = pv["conflicts"].as_array().cloned().unwrap_or_default();
    assert_eq!(ks.len(), 2, "one unit per record, one conflict each: {pv}");
    let words = |k: &Value| s(&k["text"]);
    assert!(ks.iter().any(|k| words(k).contains("Tag large")), "an own style's conflict is worded with its name, not its ID: {ks:?}");
    let ids: Vec<String> = ks.iter().map(|k| s(&k["id"])).collect();
    for (choice, want) in [("branch", "4"), ("main", "3")] {
        let choices: serde_json::Map<String, Value> = ids.iter().map(|i| (i.clone(), json!(choice))).collect();
        let r = c.exec("merge", json!({"from": "Option B", "choices": choices}));
        assert_eq!(r["merged"], json!(true), "{choice}: {r}");
        assert!(field(&mut c, "text", "tag", "height").starts_with(want), "{choice}");
        c.exec("undomerge", json!({}));
    }
    let choices: serde_json::Map<String, Value> = ids.iter().map(|i| (i.clone(), json!("branch"))).collect();
    c.exec("merge", json!({"from": "Option B", "choices": choices}));
    let want = c.doc();
    c.ok("save", json!({}));
    drop(c);
    verify(&path);
    let mut c = open(&path);
    assert!(c.doc() == want, "reopen gives the merged state");
    // Last, so the checks above report on their own: a built-in's override is named too.
    assert!(ks.iter().any(|k| words(k).contains("Tag") && !words(k).contains("large")), "the built-in's by its name Tag: {ks:?}");
}

/// Different styles, a style and a concurrent drawing edit, and different standard values merge
/// with no conflict; the same standard value changed differently is one conflict (values merge
/// per key, like project properties).
#[test]
fn fp3_different_styles_values_and_edits_merge_cleanly() {
    let dir = TempDir::new("t050-merge-clean");
    let (mut c, path) = branched(&dir, "clean.ecad");
    c.exec("setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"colour": "RD"}}));
    let own = own_id(&c.exec("setstyle", json!({"catalogue": "location", "new": true, "name": "Field box", "copy_of": "box", "layer": "standard"})));
    c.exec("setdefault", json!({"key": "symbol.coil.prefix", "level": "standard", "value": "Q"}));
    to_main(&mut c);
    c.exec("setstyle", json!({"catalogue": "frame", "id": "sheet", "fields": {"border_weight": 0.7}}));
    c.exec("wire", json!({"points": [[100, 40], [160, 40]]}));
    c.exec("setdefault", json!({"key": "wiring.label_rule", "level": "standard", "value": "each"}));
    let pv = merge_preview(&mut c, "Option B");
    assert_eq!(pv["conflicts"], json!([]), "{pv}");
    let r = c.exec("merge", json!({"from": "Option B"}));
    assert_eq!(r["merged"], json!(true), "{r}");
    assert!(field(&mut c, "text", "tag", "colour").contains("RD"));
    assert!(field(&mut c, "frame", "sheet", "border_weight").starts_with("0.7"));
    assert!(styles(&mut c, "location").iter().any(|r| s(&r["id"]) == own));
    let d = c.doc();
    assert_eq!(d.conductors.len(), 1, "main's wire kept");
    let std = d.standard.as_ref().unwrap();
    assert_eq!(std.values.get("symbol.coil.prefix").map(String::as_str), Some("Q"));
    assert_eq!(std.values.get("wiring.label_rule").map(String::as_str), Some("each"));
    // The same key on both branches: one conflict.
    c.exec("branch", json!({"name": "Option C"}));
    c.exec("setdefault", json!({"key": "symbol.coil.prefix", "level": "standard", "value": "R"}));
    to_main(&mut c);
    c.exec("setdefault", json!({"key": "symbol.coil.prefix", "level": "standard", "value": "S"}));
    let pv = merge_preview(&mut c, "Option C");
    assert_eq!(pv["conflicts"].as_array().map(Vec::len), Some(1), "{pv}");
    c.ok("save", json!({}));
    verify(&path);
}

/// §3.5: the standard header is one unit: renamed differently on both branches, one conflict;
/// a standard made on a branch arrives whole.
#[test]
fn fp3_the_standard_header_is_one_unit() {
    let dir = TempDir::new("t050-merge-header");
    let (mut c, _) = branched(&dir, "header.ecad");
    c.exec("standard", json!({"name": "Branch name"}));
    to_main(&mut c);
    c.exec("standard", json!({"name": "Main name"}));
    let pv = merge_preview(&mut c, "Option B");
    assert_eq!(pv["conflicts"].as_array().map(Vec::len), Some(1), "{pv}");
    // A standard made only on a branch merges in whole.
    let dir2 = TempDir::new("t050-merge-new-std");
    let path = dir2.file("n.ecad");
    let mut c = create(&path);
    c.ok("save", json!({}));
    c.exec("branch", json!({"name": "Std"}));
    c.exec("standard", json!({"new": "From branch"}));
    c.exec("setstyle", json!({"catalogue": "text", "id": "tag", "layer": "standard", "fields": {"height": 4}}));
    let want = c.doc().standard.clone();
    to_main(&mut c);
    assert!(c.doc().standard.is_none());
    let r = c.exec("merge", json!({"from": "Std"}));
    assert_eq!(r["merged"], json!(true), "{r}");
    assert!(c.doc().standard == want, "header, values and records arrive together");
}

// ---------------------------------------------------------------------------- 050b

/// AC 5 (050b), §6.8: `category` and `wire {category}` by the agent, one transaction each, one
/// undo; refused with writes off.
#[test]
fn b_category_writes_by_the_agent_then_refused_with_writes_off() {
    let dir = TempDir::new("t050b Kategorien \u{e4}");
    let path = dir.file("Leitungen (B).ecad");
    let (w, before) = {
        let mut c = create(&path);
        let w = c.exec("wire", json!({"points": [[0, 20], [60, 20]], "potential": "24V"}))["conductor"].as_str().unwrap().to_string();
        assert_eq!(styles(&mut c, "wire").len(), 10);
        let (_, ops) = one_by_agent_then_undo(&mut c, "category", json!({"conductor": w, "value": "signal"}));
        assert!(matches!(ops.as_slice(), [Op::SetProperty { key, .. }] if key == "wire.category"), "{ops:?}");
        one_by_agent_then_undo(&mut c, "wire", json!({"points": [[0, 60], [60, 60]], "category": "dc24"}));
        one_by_agent_then_undo(
            &mut c,
            "setstyle",
            json!({"catalogue": "wire", "id": "dc24", "fields": {"colour": "RD", "part_number": "W-05", "allowance": 1.5}}),
        );
        one_by_agent_then_undo(&mut c, "category", json!({"net": w, "value": "analogue"}));
        let n = c.log().len();
        for params in [json!({"conductor": w, "value": "nope"}), json!({"conductor": w, "value": "x".repeat(10_000)}), json!({"value": "dc24"})] {
            c.refused("execute", json!({"command": "category", "params": params}));
        }
        let m = c.refused(
            "execute",
            json!({"command": "wiresettings", "params": {"potentials": [{"name": "24V", "prefix": "24V-", "numbered": true, "colour": "RD"}]}}),
        );
        assert!(m.contains("set on wire categories now"), "{m}");
        assert_eq!(c.log().len(), n);
        let before = json!({
            "wirelist": c.exec("wirelist", json!({"limit": 5000})),
            "bom": c.exec("bom", json!({"limit": 5000})),
            "nets": c.exec("nets", json!({})),
            "styles": c.exec("styles", json!({"catalogue": "wire", "users": true})),
        });
        assert!(before["wirelist"].to_string().contains("category"), "{}", before["wirelist"]);
        c.ok("save", json!({}));
        (w, before)
    };
    let (sess, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut c = Client::new(sess);
    let log = c.log();
    for (cmd, params) in [
        ("category", json!({"conductor": w, "value": "signal"})),
        ("wire", json!({"points": [[0, 90], [60, 90]], "category": "dc24"})),
        ("setstyle", json!({"catalogue": "wire", "id": "pe", "fields": {"line_weight": 0.5}})),
    ] {
        assert_eq!(c.refused("execute", json!({"command": cmd, "params": params})), policy::WRITES_OFF, "{cmd}");
    }
    let after = json!({
        "wirelist": c.exec("wirelist", json!({"limit": 5000})),
        "bom": c.exec("bom", json!({"limit": 5000})),
        "nets": c.exec("nets", json!({})),
        "styles": c.exec("styles", json!({"catalogue": "wire", "users": true})),
    });
    assert_eq!(after, before);
    assert_eq!(c.log(), log);
}

/// §12.18: merge across `wire.category` and category records: a category record changed
/// differently on both branches is one conflict named by the category; a conductor's category on
/// one branch and the category's colour on the other merge cleanly.
#[test]
fn b18_category_merges() {
    let dir = TempDir::new("t050b-merge");
    let path = dir.file("cat.ecad");
    let mut c = create(&path);
    let w = c.exec("wire", json!({"points": [[0, 20], [60, 20]], "potential": "24V"}))["conductor"].as_str().unwrap().to_string();
    c.ok("save", json!({}));
    c.exec("branch", json!({"name": "Option B"}));
    c.exec("category", json!({"conductor": w, "value": "signal"}));
    c.exec("setstyle", json!({"catalogue": "wire", "id": "dc24", "fields": {"colour": "VT"}}));
    to_main(&mut c);
    c.exec("setstyle", json!({"catalogue": "wire", "id": "pe", "fields": {"line_weight": 0.5}}));
    c.exec("setstyle", json!({"catalogue": "wire", "id": "dc24", "fields": {"colour": "RD"}}));
    let pv = merge_preview(&mut c, "Option B");
    let ks = pv["conflicts"].as_array().cloned().unwrap_or_default();
    assert_eq!(ks.len(), 1, "{pv}");
    assert!(ks[0].to_string().contains("24 V DC"), "{}", ks[0]);
    let id = s(&ks[0]["id"]);
    let r = c.exec("merge", json!({"from": "Option B", "choices": {id: "main"}}));
    assert_eq!(r["merged"], json!(true), "{r}");
    let d = c.doc();
    let cond = d.conductors.iter().find(|(k, _)| k.hex() == w).map(|(_, x)| x.clone()).unwrap();
    assert_eq!(cond.properties.get("wire.category").map(String::as_str), Some("signal"), "the branch's category came");
    assert!(field(&mut c, "wire", "dc24", "colour").contains("RD"));
    assert!(field(&mut c, "wire", "pe", "line_weight").starts_with("0.5"));
    c.ok("save", json!({}));
    verify(&path);
}
