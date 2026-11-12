//! Brief 026, MCP contract (the ECAD rule of brief 011): `renumber` (preview and apply),
//! `finalise`, `setprop` tag with each `conflict`, `setprop tag_fixed`, `numbering`,
//! `issuerevision` with `provisional`, and the merge choice `one`, each through `execute`: the
//! transaction it records, `Author::Agent(<client>)`, and that the writes are refused with writes
//! off while the previews and the settings read answer.

use ecad_mcp::{EcadMcp, Options};
use ecad_model::sheet::Grid;
use ecad_model::{Author, ComponentId, ObjectRef, Op, PageId};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, TempDir, reader};

const SEED: u64 = 0x0E_CAD0_0026;

fn writer() -> Client {
    Client::new(EcadMcp::in_memory("numbering", SEED, Options { write: true, agent: None }).unwrap())
}

/// The point at the middle of column `col` (0-based) and row `row` of the first page's frame.
fn cell(c: &mut Client, col: u32, row: u32) -> [f64; 2] {
    let doc = c.doc();
    let page: PageId = doc.page_order[0];
    let g = Grid::of(&doc, page);
    [g.frame_left() + (f64::from(col) + 0.5) * g.column_width(), g.frame_top() - (f64::from(row) + 0.5) * g.row_height()]
}

fn coil(c: &mut Client, at: [f64; 2], tag: Option<&str>) -> String {
    let mut p = json!({"symbol": "coil", "at": at});
    if let Some(t) = tag {
        p["tag"] = json!(t);
    }
    c.exec("place", p)["component"].as_str().unwrap().to_string()
}

fn coil_at(c: &mut Client, col: u32, row: u32, tag: Option<&str>) -> String {
    let at = cell(c, col, row);
    coil(c, at, tag)
}

fn tag_of(c: &mut Client, comp: &str) -> Option<String> {
    let id = ComponentId::parse(comp).unwrap();
    c.doc().components.get(&id).and_then(|x| x.properties.get("tag").cloned())
}

fn last_is_agents(c: &mut Client) {
    assert_eq!(c.log().last().unwrap().author, Author::Agent(AGENT.into()));
}

#[test]
fn renumber_previews_and_applies_one_transaction_by_the_agent() {
    let mut c = writer();
    let (a, cc) = (cell(&mut c, 0, 1), cell(&mut c, 2, 1));
    let k07 = coil(&mut c, a, Some("-K07"));
    let k01 = coil(&mut c, cc, Some("-K01"));
    // The preview reads: nothing recorded.
    let before = c.log().len();
    let pv = c.exec("renumber", json!({"kind": "devices", "scope": "all", "preview": true}));
    assert_eq!(c.log().len(), before);
    assert_eq!(pv["changes"], json!(2), "{pv}");
    let rows = pv["rows"].as_array().unwrap();
    assert_eq!((rows[0]["old"].as_str(), rows[0]["new"].as_str()), (Some("-K07"), Some("-K01")), "{pv}");
    assert_eq!(rows[0]["mark"], json!("→"));
    // Apply on the preview's change count: one transaction by the agent.
    let r = c.exec("renumber", json!({"kind": "devices", "scope": "all", "based_on": pv["change_count"]}));
    assert_eq!(r["renumbered"], json!(2), "{r}");
    assert_eq!(c.log().len(), before + 1);
    last_is_agents(&mut c);
    let last = c.log().last().cloned().unwrap();
    assert_eq!(last.label, "Renumber devices");
    assert!(
        last.ops.iter().any(|o| matches!(o, Op::SetProperty { object: ObjectRef::Project, key, .. } if key == "numbering.high.K")),
        "{:?}",
        last.ops
    );
    assert_eq!((tag_of(&mut c, &k07).as_deref(), tag_of(&mut c, &k01).as_deref()), (Some("-K01"), Some("-K02")));
    // A new coil continues after the renumbered set (the high-water reset).
    let k3 = coil_at(&mut c, 4, 1, None);
    assert_eq!(tag_of(&mut c, &k3).as_deref(), Some("-K03"));
    // One `u` restores every tag.
    c.exec("u", json!({}));
    c.exec("u", json!({}));
    assert_eq!((tag_of(&mut c, &k07).as_deref(), tag_of(&mut c, &k01).as_deref()), (Some("-K07"), Some("-K01")));
}

#[test]
fn apply_on_a_stale_preview_is_refused_and_records_nothing() {
    let mut c = writer();
    let k = coil_at(&mut c, 0, 1, Some("-K05"));
    let pv = c.exec("renumber", json!({"preview": true}));
    c.exec("setprop", json!({"component": k, "key": "description", "value": "Pump"}));
    let before = c.log().len();
    let m = c.refused("execute", json!({"command": "renumber", "params": {"based_on": pv["change_count"]}}));
    assert!(m.contains("changed since the preview"), "{m}");
    assert_eq!(c.log().len(), before);
}

#[test]
fn setprop_tag_conflicts_and_tag_fixed_record_their_transactions() {
    let mut c = writer();
    let k4 = coil_at(&mut c, 0, 1, Some("-K04"));
    // A contact, so that it can later join the coil (018: one coil per device).
    let at = cell(&mut c, 1, 1);
    let k2 = c.exec("place", json!({"symbol": "no", "at": at, "tag": "-K02"}))["component"].as_str().unwrap().to_string();
    // Without `conflict`: refused with all three ways, nothing recorded.
    let before = c.log().len();
    let m = c.refused("execute", json!({"command": "setprop", "params": {"component": k4, "key": "tag", "value": "-K02"}}));
    assert!(m.contains("\"swap\"") && m.contains("\"merge\"") && m.contains("\"next\""), "{m}");
    assert_eq!(c.log().len(), before);
    // Swap: one transaction, both exchanged, the typed one fixed.
    let r = c.exec("setprop", json!({"component": k4, "key": "tag", "value": "-K02", "conflict": "swap"}));
    assert_eq!(r["swapped"], json!(true), "{r}");
    assert_eq!(c.log().len(), before + 1);
    last_is_agents(&mut c);
    assert_eq!(c.log().last().unwrap().label, "Swap tags");
    assert_eq!((tag_of(&mut c, &k4).as_deref(), tag_of(&mut c, &k2).as_deref()), (Some("-K02"), Some("-K04")));
    let fixed = |c: &mut Client, k: &str| c.doc().components[&ComponentId::parse(k).unwrap()].properties.get("tag_fixed").cloned();
    assert_eq!(fixed(&mut c, &k4).as_deref(), Some("yes"));
    // Next: the next free tag, fixed.
    let r = c.exec("setprop", json!({"component": k2, "key": "tag", "value": "-K02", "conflict": "next"}));
    assert_eq!(r["value"], json!("-K05"), "{r}");
    last_is_agents(&mut c);
    // Merge: the same device (018's path).
    let r = c.exec("setprop", json!({"component": k2, "key": "tag", "value": "-K02", "conflict": "merge"}));
    assert_eq!(r["merged"], json!(true), "{r}");
    last_is_agents(&mut c);
    assert!(!c.doc().components.contains_key(&ComponentId::parse(&k2).unwrap()));
    // tag_fixed: released and fixed again, each one transaction.
    let n = c.log().len();
    c.exec("setprop", json!({"component": k4, "key": "tag_fixed", "value": null}));
    assert_eq!(fixed(&mut c, &k4), None);
    c.exec("setprop", json!({"component": k4, "key": "tag_fixed", "value": "yes"}));
    assert_eq!(c.log().len(), n + 2);
    last_is_agents(&mut c);
}

#[test]
fn cable_tags_share_the_namespace_and_swap() {
    let mut c = writer();
    for y in [0.0, 10.0, 20.0, 30.0] {
        c.exec("wire", json!({"points": [[0, y], [60, y]]}));
    }
    let w1 = c.exec("cableslash", json!({"from": [20, -3], "to": [20, 13]}))["cable"].as_str().unwrap().to_string();
    let w2 = c.exec("cableslash", json!({"from": [40, 17], "to": [40, 33]}))["cable"].as_str().unwrap().to_string();
    let cable_tag = |c: &mut Client, k: &str| c.doc().cables[&ecad_model::CableId::parse(k).unwrap()].properties.get("tag").cloned();
    assert_eq!((cable_tag(&mut c, &w1).as_deref(), cable_tag(&mut c, &w2).as_deref()), (Some("-W01"), Some("-W02")));
    // A device may not take a cable's tag, except by the next free one.
    let k = coil_at(&mut c, 3, 3, None);
    let m = c.refused("execute", json!({"command": "setprop", "params": {"component": k, "key": "tag", "value": "-W01", "conflict": "swap"}}));
    assert!(m.contains("cable"), "{m}");
    c.exec("setprop", json!({"cable": w2, "key": "tag", "value": "-W01", "conflict": "swap"}));
    last_is_agents(&mut c);
    assert_eq!((cable_tag(&mut c, &w1).as_deref(), cable_tag(&mut c, &w2).as_deref()), (Some("-W02"), Some("-W01")));
}

#[test]
fn numbering_finalise_and_issue_with_provisional() {
    let mut c = writer();
    let n0 = c.log().len();
    let r = c.exec("numbering", json!({}));
    assert_eq!(r["provisional"], json!(false));
    assert_eq!(c.log().len(), n0, "reading records nothing");
    c.exec("numbering", json!({"provisional": true}));
    assert_eq!(c.log().len(), n0 + 1);
    last_is_agents(&mut c);
    assert_eq!(c.log().last().unwrap().label, "Set numbering");
    let a = coil_at(&mut c, 2, 1, None);
    let b = coil_at(&mut c, 0, 1, None);
    assert_eq!((tag_of(&mut c, &a).as_deref(), tag_of(&mut c, &b).as_deref()), (Some("-K?1"), Some("-K?2")));
    let checks = c.exec("checks", json!({"kinds": ["provisional-tag"]}));
    assert_eq!(checks["checks"].as_array().map(Vec::len), Some(2), "{checks}");
    // Finalise: drawing order (b is in column A).
    let pv = c.exec("finalise", json!({"preview": true}));
    assert_eq!(pv["changes"], json!(2), "{pv}");
    // Issue without `provisional`: refused with both ways.
    for (f, v) in [("rev", "A"), ("description", "First"), ("drawn", "CV")] {
        c.exec("revision", json!({f: v}));
    }
    let m = c.refused("execute", json!({"command": "issuerevision", "params": {}}));
    assert!(m.contains("\"finalise\"") && m.contains("\"issue\""), "{m}");
    let n = c.log().len();
    c.exec("issuerevision", json!({"provisional": "finalise"}));
    let log = c.log();
    assert_eq!(log.len(), n + 2, "Finalise, then the issue");
    assert_eq!(log[n].label, "Finalise numbering");
    assert_eq!(log[n].author, Author::Agent(AGENT.into()));
    assert_eq!((tag_of(&mut c, &b).as_deref(), tag_of(&mut c, &a).as_deref()), (Some("-K01"), Some("-K02")));
    // Issue anyway: one transaction.
    coil_at(&mut c, 4, 1, None);
    for (f, v) in [("rev", "B"), ("description", "Second"), ("drawn", "CV")] {
        c.exec("revision", json!({f: v}));
    }
    let n = c.log().len();
    let r = c.exec("issuerevision", json!({"provisional": "issue"}));
    assert_eq!(c.log().len(), n + 1, "{r}");
    assert!(r["message"].as_str().unwrap().contains("1 provisional tag issued"), "{r}");
    // Finalise after the issue: one transaction by the agent.
    let r = c.exec("finalise", json!({}));
    assert_eq!(r["renumbered"], json!(1), "{r}");
    last_is_agents(&mut c);
}

#[test]
fn writes_are_refused_with_writes_off_and_the_previews_read() {
    let mut c = reader();
    let n = c.log().len();
    c.exec("renumber", json!({"preview": true}));
    c.exec("finalise", json!({"preview": true}));
    c.exec("numbering", json!({}));
    for (cmd, params) in [
        ("renumber", json!({})),
        ("finalise", json!({})),
        ("numbering", json!({"provisional": true})),
        ("setprop", json!({"component": "00000000000000000000000000000001", "key": "tag_fixed", "value": "yes"})),
        ("issuerevision", json!({"provisional": "issue"})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert!(m.contains("writes off") || m.contains("read-only") || m.contains("Refused"), "{cmd}: {m}");
    }
    assert_eq!(c.log().len(), n);
}

#[test]
fn merge_choice_one_makes_two_devices_one_in_the_merge_transaction() {
    let dir = TempDir::new("merge-one");
    let path = dir.file("one.ecad");
    let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
    c.exec("wire", json!({"points": [[0, 40], [60, 40]]}));
    c.ok("save", json!({}));
    c.exec("branch", json!({"name": "Option E"}));
    let branch_k = coil(&mut c, [100.0, 100.0], Some("-K05"));
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
    let main_k = c.exec("place", json!({"symbol": "no", "at": [200, 100], "tag": "-K05"}))["component"].as_str().unwrap().to_string();
    let pv = c.exec("merge", json!({"from": "Option E", "preview": true}));
    let k = pv["conflicts"].as_array().unwrap().iter().find(|k| k["kind"] == json!("tag")).cloned().unwrap_or_else(|| panic!("{pv}"));
    assert!(k["choices"].as_array().unwrap().contains(&json!("one")), "{k}");
    let id = k["id"].as_str().unwrap().to_string();
    let n = c.log().len();
    let r = c.exec("merge", json!({"from": "Option E", "choices": {id: "one"}}));
    assert_eq!(r["merged"], json!(true), "{r}");
    assert_eq!(c.log().len(), n + 1);
    last_is_agents(&mut c);
    let doc = c.doc();
    let main_id = ComponentId::parse(&main_k).unwrap();
    assert!(doc.components.contains_key(&main_id));
    assert!(!doc.components.contains_key(&ComponentId::parse(&branch_k).unwrap()), "the branch's device joined main's");
    let reps = doc.representations_of(ecad_model::Subject::Component(main_id)).count();
    assert_eq!(reps, 2, "one device with both symbols");
}

#[allow(dead_code)]
fn _value(_: Value) {}
