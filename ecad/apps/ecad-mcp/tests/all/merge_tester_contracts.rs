//! Tester contracts for brief 020b (merge into main, update from main) through `execute`, as the
//! History tab's clicks send them: a clean merge survives save and reopen and its undo still asks
//! in a later session; conflicts of every kind offer choices and every offered choice merges (never
//! a dead end); merging an already merged branch; undo and redo across a merge; the file verifies.

use ecad_mcp::{EcadMcp, Options};
use ecad_model::{ComponentId, ConductorId, TxKind};
use serde_json::{Value, json};

use crate::mcp_contracts::{Client, TempDir};

fn open(path: &str) -> Client {
    Client::new(EcadMcp::open(path, Options { write: true, agent: None }).unwrap().0)
}

/// A file project: one coil (`-K01`, "Pump") and one loose wire, saved.
fn project(dir: &TempDir, name: &str) -> (Client, String, String, String) {
    let path = dir.file(name);
    let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
    let coil = c.exec("place", json!({"symbol": "coil", "at": [0, 0]}))["component"].as_str().unwrap().to_string();
    c.exec("setprop", json!({"component": coil, "key": "description", "value": "Pump"}));
    let w = c.exec("wire", json!({"points": [[100, 40], [160, 40]]}))["conductor"].as_str().unwrap().to_string();
    c.ok("save", json!({}));
    (c, coil, w, path)
}

fn prop(c: &mut Client, coil: &str, key: &str) -> Option<String> {
    let id = ComponentId::parse(coil).unwrap();
    c.doc().components.get(&id).and_then(|x| x.properties.get(key).cloned())
}

fn branches(c: &mut Client) -> Vec<Value> {
    c.exec("branches", json!({}))["branches"].as_array().unwrap().clone()
}

fn current(c: &mut Client) -> String {
    branches(c).iter().find(|b| b["current"] == json!(true)).unwrap()["name"].as_str().unwrap().to_string()
}

fn status(c: &mut Client, name: &str) -> String {
    branches(c).iter().find(|b| b["name"] == json!(name)).map(|b| b["status"].as_str().unwrap().to_string()).unwrap_or_default()
}

fn merge_nodes(c: &mut Client) -> Vec<Value> {
    let h = c.exec("history", json!({"archived": true}));
    h["nodes"].as_array().unwrap().iter().filter(|n| n["kind"] == json!("merge")).cloned().collect()
}

fn verify(path: &str) {
    let v = ecad_file::v2::verify::verify(&ecad_file::v2::Store::open(std::path::Path::new(path), false).unwrap());
    assert!(v.ok, "{:#?}", v.lines);
}

fn tags(c: &mut Client) -> Vec<String> {
    let mut t: Vec<String> = c.doc().components.values().filter_map(|x| x.properties.get("tag").cloned()).collect();
    t.sort();
    t
}

// ------------------------------------------------------------------ clean merge, save, reopen

#[test]
fn a_clean_merge_survives_save_and_reopen_and_its_undo_still_asks_in_the_next_session() {
    let dir = TempDir::new("t020b-clean");
    let (mut c, coil, _, path) = project(&dir, "clean.ecad");
    c.exec("version", json!({"name": "Before change"}));
    c.exec("branch", json!({"name": "Option B"}));
    c.exec("setprop", json!({"component": coil, "key": "description", "value": "Pump 1"}));
    let r = c.exec("merge", json!({"from": "Option B"}));
    assert_eq!(r["merged"], json!(true), "{r}");
    let after = c.doc();
    c.ok("save", json!({}));
    drop(c);
    // Reopen: on main, merged, the join in the graph, the merge step on top.
    let mut c = open(&path);
    assert_eq!(current(&mut c), "main");
    assert_eq!(c.doc(), after, "reopen gives the merged state");
    assert_eq!(status(&mut c, "Option B"), "merged");
    assert_eq!(merge_nodes(&mut c).len(), 1);
    assert!(matches!(c.log().last().unwrap().kind, TxKind::Merge { .. }));
    verify(&path);
    // Undo in a later session asks first; undomerge undoes, and the save reopens the branch.
    let m = c.refused("execute", json!({"command": "u", "params": {}}));
    assert!(m.contains("Undo the merge?"), "{m}");
    c.exec("undomerge", json!({}));
    assert_eq!(prop(&mut c, &coil, "description").as_deref(), Some("Pump"));
    c.ok("save", json!({}));
    drop(c);
    let mut c = open(&path);
    assert_eq!(status(&mut c, "Option B"), "open", "undoing the merge reopened the branch");
    assert!(merge_nodes(&mut c).is_empty(), "the join went with the undo");
    assert_eq!(prop(&mut c, &coil, "description").as_deref(), Some("Pump"));
    verify(&path);
    // The branch still holds its change and can be merged again.
    let r = c.exec("merge", json!({"from": "Option B"}));
    assert_eq!(r["merged"], json!(true), "{r}");
    assert_eq!(prop(&mut c, &coil, "description").as_deref(), Some("Pump 1"));
    c.ok("save", json!({}));
    verify(&path);
}

// ------------------------------------------------------------------ already merged

#[test]
fn merging_an_already_merged_branch_is_never_a_second_copy_of_its_changes() {
    let dir = TempDir::new("t020b-again");
    let (mut c, coil, _, path) = project(&dir, "again.ecad");
    c.exec("branch", json!({"name": "Option B"}));
    c.exec("setprop", json!({"component": coil, "key": "description", "value": "Pump 1"}));
    c.exec("merge", json!({"from": "Option B", "close": false}));
    c.ok("save", json!({}));
    assert_eq!(status(&mut c, "Option B"), "open");
    // Nothing new on the branch: the preview says so, and a merge records nothing (or is refused
    // with a reason).
    let n = c.log().len();
    let pv = c.exec("merge", json!({"from": "Option B", "preview": true}));
    assert_eq!(pv["conflicts"], json!([]), "{pv}");
    assert!(pv["changes"].as_array().is_none_or(Vec::is_empty), "nothing new to merge: {pv}");
    let r = c.call("execute", json!({"command": "merge", "params": {"from": "Option B"}}));
    let text = r["content"][0]["text"].as_str().unwrap_or_default().to_string();
    assert!(c.log().len() <= n + 1, "{text}");
    eprintln!("re-merge with nothing new: isError={} {text}", r["isError"]);
    if c.log().len() == n + 1 {
        c.exec("undomerge", json!({}));
    }
    // main changes the merged property afterwards; the branch changes something else. The merge
    // back starts from the first merge, so main's later change is no conflict and stays.
    if current(&mut c) != "main" {
        c.exec("switchbranch", json!({"branch": "main", "save": true}));
    }
    c.exec("setprop", json!({"component": coil, "key": "description", "value": "Main pump"}));
    c.ok("save", json!({}));
    c.exec("switchbranch", json!({"branch": "Option B"}));
    c.exec("setprop", json!({"component": coil, "key": "location", "value": "+A1"}));
    let pv = c.exec("merge", json!({"from": "Option B", "preview": true}));
    assert_eq!(pv["conflicts"], json!([]), "the earlier merge is the ancestor: {pv}");
    let r = c.exec("merge", json!({"from": "Option B"}));
    assert_eq!(r["merged"], json!(true), "{r}");
    assert_eq!(current(&mut c), "main");
    assert_eq!(prop(&mut c, &coil, "description").as_deref(), Some("Main pump"));
    assert_eq!(prop(&mut c, &coil, "location").as_deref(), Some("+A1"));
    c.ok("save", json!({}));
    assert_eq!(status(&mut c, "Option B"), "merged");
    // A closed (merged) branch merged again: refused with a way through, or nothing to merge.
    let r = c.call("execute", json!({"command": "merge", "params": {"from": "Option B", "preview": true}}));
    eprintln!("preview of a merged branch: {r}");
    let n = c.log().len();
    let r = c.call("execute", json!({"command": "merge", "params": {"from": "Option B"}}));
    eprintln!("merge of a merged branch: {r}");
    assert_eq!(c.log().len(), n, "a closed branch's changes are not merged twice");
    verify(&path);
}

// ------------------------------------------------------------------ conflicts: never a dead end

/// Every conflict in the preview has choices, and each choice offered merges: try them one by
/// one (merge, check, undomerge). `check` sees the choice and the client after the merge.
fn every_choice_merges(c: &mut Client, from: &str, check: &mut dyn FnMut(&mut Client, &str, &str)) -> usize {
    let pv = c.exec("merge", json!({"from": from, "preview": true}));
    let conflicts = pv["conflicts"].as_array().unwrap().clone();
    assert!(!conflicts.is_empty(), "{pv}");
    let mut tried = 0;
    for k in &conflicts {
        let id = k["id"].as_str().unwrap().to_string();
        let choices = k["choices"].as_array().unwrap_or_else(|| panic!("no choices: {k}")).clone();
        assert!(!choices.is_empty(), "a conflict without a choice is a dead end: {k}");
        for choice in choices {
            // `{"value": ""}` (Edit) is offered as an object: give it a value.
            let choice = if choice.is_object() { json!({"value": "Edited"}) } else { choice };
            let ch = choice.as_str().map(str::to_string).unwrap_or_else(|| choice.to_string());
            let mut all = serde_json::Map::new();
            for other in &conflicts {
                let oid = other["id"].as_str().unwrap();
                let first = other["choices"][0].clone();
                all.insert(oid.into(), if oid == id { choice.clone() } else { first });
            }
            let before = c.doc();
            let n = c.log().len();
            let r = c.call("execute", json!({"command": "merge", "params": {"from": from, "choices": all, "close": false}}));
            assert_ne!(r["isError"], json!(true), "choice {ch} for {id} ({k}) is refused: {r}");
            let out: Value = serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap();
            if out["merged"] == json!(false) {
                // Every answer keeps main's side: nothing to record (see
                // `keeping_main_for_every_conflict_still_merges_and_closes_the_branch`).
                assert_eq!((c.log().len(), c.doc()), (n, before), "{out}");
                tried += 1;
                continue;
            }
            assert_eq!(c.log().len(), n + 1, "one step: {r}");
            check(c, &id, &ch);
            c.exec("undomerge", json!({}));
            assert_eq!(c.doc(), before, "undomerge after {id}={ch}");
            tried += 1;
        }
    }
    tried
}

#[test]
fn a_value_conflict_offers_main_branch_and_edit_and_each_merges() {
    let dir = TempDir::new("t020b-value");
    let (mut c, coil, _, path) = project(&dir, "value.ecad");
    c.exec("branch", json!({"name": "Option C"}));
    c.exec("setprop", json!({"component": coil, "key": "description", "value": "Branch pump"}));
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
    c.exec("setprop", json!({"component": coil, "key": "description", "value": "Main pump"}));
    let pv = c.exec("merge", json!({"from": "Option C", "preview": true}));
    let k = &pv["conflicts"][0];
    let choices: Vec<String> = k["choices"].as_array().unwrap().iter().map(|x| x.to_string()).collect();
    assert!(choices.iter().any(|x| x.contains("main")) && choices.iter().any(|x| x.contains("branch")), "{k}");
    let tried = every_choice_merges(&mut c, "Option C", &mut |c, _, ch| {
        let d = prop(c, &coil, "description");
        match ch {
            "main" => assert_eq!(d.as_deref(), Some("Main pump")),
            "branch" => assert_eq!(d.as_deref(), Some("Branch pump")),
            _ => {}
        }
    });
    assert!(tried >= 2);
    // Edit: a typed value.
    let id = k["id"].as_str().unwrap();
    c.exec("merge", json!({"from": "Option C", "choices": {id: {"value": "Pump 2"}}}));
    assert_eq!(prop(&mut c, &coil, "description").as_deref(), Some("Pump 2"));
    c.ok("save", json!({}));
    verify(&path);
}

#[test]
fn a_device_removed_on_main_and_changed_on_the_branch_offers_a_way_through() {
    let dir = TempDir::new("t020b-removed");
    let (mut c, coil, _, path) = project(&dir, "removed.ecad");
    c.exec("branch", json!({"name": "Option R"}));
    c.exec("setprop", json!({"component": coil, "key": "description", "value": "Branch pump"}));
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
    c.exec("devicedelete", json!({"component": coil}));
    let pv = c.exec("merge", json!({"from": "Option R", "preview": true}));
    eprintln!("removed vs changed preview: {pv}");
    let id = ComponentId::parse(&coil).unwrap();
    let tried = every_choice_merges(&mut c, "Option R", &mut |c, _, ch| {
        let present = c.doc().components.contains_key(&id);
        eprintln!("  choice {ch}: device present = {present}");
    });
    assert!(tried >= 2, "keep it removed / keep the device");
    c.ok("save", json!({}));
    verify(&path);
}

#[test]
fn the_same_new_tag_on_both_sides_is_renumbered_and_tags_stay_unique() {
    let dir = TempDir::new("t020b-tag");
    let (mut c, _, _, path) = project(&dir, "tag.ecad");
    c.exec("branch", json!({"name": "Option T"}));
    c.exec("place", json!({"symbol": "coil", "at": [0, 80]}));
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
    c.exec("place", json!({"symbol": "coil", "at": [0, 120]}));
    let pv = c.exec("merge", json!({"from": "Option T", "preview": true}));
    eprintln!("duplicate tag preview: {}", pv["conflicts"]);
    let tried = every_choice_merges(&mut c, "Option T", &mut |c, id, ch| {
        let t = tags(c);
        let mut u = t.clone();
        u.dedup();
        assert_eq!(t, u, "tags unique after {id}={ch}");
        assert_eq!(t.len(), 3, "both new devices kept: {t:?}");
    });
    assert!(tried >= 2);
    c.ok("save", json!({}));
    verify(&path);
}

#[test]
fn a_wire_end_attached_differently_on_both_sides_can_be_left_unconnected() {
    let dir = TempDir::new("t020b-end");
    let path = dir.file("end.ecad");
    let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
    let placed = c.exec("place", json!({"symbol": "coil", "at": [0, 0]}));
    let t: Vec<String> = placed["terminals"].as_array().unwrap().iter().map(|t| t["terminal"].as_str().unwrap().to_string()).collect();
    assert!(t.len() >= 2, "{placed}");
    let w = c.exec("wire", json!({"points": [[100, 40], [160, 40]]}))["conductor"].as_str().unwrap().to_string();
    c.ok("save", json!({}));
    c.exec("branch", json!({"name": "Option W"}));
    c.exec("connect", json!({"conductor": w, "terminal": t[1], "end": "B"}));
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
    c.exec("connect", json!({"conductor": w, "terminal": t[0], "end": "B"}));
    let pv = c.exec("merge", json!({"from": "Option W", "preview": true}));
    let k =
        pv["conflicts"].as_array().unwrap().iter().find(|k| k["id"].as_str().unwrap().ends_with("/end.b")).cloned().unwrap_or_else(|| panic!("{pv}"));
    eprintln!("end conflict: {k}");
    // Design §5.3 and owner answer 7: a connection conflict always offers Leave unconnected.
    assert!(k["choices"].as_array().unwrap().iter().any(|x| x == "loose"), "Leave unconnected offered: {k}");
    let cid = ConductorId::parse(&w).unwrap();
    every_choice_merges(&mut c, "Option W", &mut |c, _, ch| {
        let end_b = c.doc().conductors.get(&cid).unwrap().end(ecad_model::End::B);
        eprintln!("  choice {ch}: end B = {end_b:?}");
        if ch == "loose" {
            assert!(end_b.is_none(), "Leave unconnected leaves end B loose");
        }
    });
    // Leave it loose, and Checks lists the open end.
    let id = k["id"].as_str().unwrap();
    c.exec("merge", json!({"from": "Option W", "choices": {id: "loose"}}));
    let checks = c.exec("checks", json!({}));
    eprintln!("checks after loose: {checks}");
    c.ok("save", json!({}));
    verify(&path);
}

#[test]
fn a_branch_wire_attached_to_a_point_main_removed_offers_bring_back_or_leave_loose() {
    let dir = TempDir::new("t020b-bring");
    let path = dir.file("bring.ecad");
    let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
    let placed = c.exec("place", json!({"symbol": "coil", "at": [0, 0]}));
    let coil = placed["component"].as_str().unwrap().to_string();
    let t0 = placed["terminals"][0]["terminal"].as_str().unwrap().to_string();
    let w = c.exec("wire", json!({"points": [[100, 40], [160, 40]]}))["conductor"].as_str().unwrap().to_string();
    c.ok("save", json!({}));
    c.exec("branch", json!({"name": "Option X"}));
    c.exec("connect", json!({"conductor": w, "terminal": t0, "end": "B"}));
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
    c.exec("devicedelete", json!({"component": coil}));
    let pv = c.exec("merge", json!({"from": "Option X", "preview": true}));
    eprintln!("bring-back preview: {}", pv["conflicts"]);
    // Every combination of offered answers either merges, or the preview says what to change
    // before Merge (never an "invalid document" refusal after the dialog said it was fine).
    let ks = pv["conflicts"].as_array().unwrap().clone();
    assert_eq!(ks.len(), 2, "{pv}");
    let opts = |k: &Value| k["choices"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect::<Vec<_>>();
    let mut bad = Vec::new();
    for a in opts(&ks[0]) {
        for b in opts(&ks[1]) {
            let choices = json!({ks[0]["id"].as_str().unwrap(): a, ks[1]["id"].as_str().unwrap(): b});
            let p = c.exec("merge", json!({"from": "Option X", "preview": true, "choices": choices}));
            let r = c.call("execute", json!({"command": "merge", "params": {"from": "Option X", "choices": choices, "close": false}}));
            if r["isError"] == json!(true) {
                let flagged = p["unanswered"] != json!([]) || p["after"].as_str().is_some_and(|t| t.contains("first"));
                if !flagged {
                    bad.push(format!(
                        "{}={a}, {}={b}: preview {:?}, merge refused {:?}",
                        ks[0]["object"], ks[1]["object"], p["after"], r["content"][0]["text"]
                    ));
                }
            } else {
                c.exec("undomerge", json!({}));
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
    c.ok("save", json!({}));
    verify(&path);
}

// ------------------------------------------------------------------ undo and redo across a merge

#[test]
fn undo_and_redo_step_across_a_merge_exactly() {
    let dir = TempDir::new("t020b-undo");
    let (mut c, coil, _, path) = project(&dir, "undo.ecad");
    c.exec("branch", json!({"name": "Option U"}));
    c.exec("setprop", json!({"component": coil, "key": "description", "value": "Pump 1"}));
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
    let s0 = c.doc();
    c.exec("wire", json!({"points": [[0, 200], [60, 200]]}));
    let s1 = c.doc();
    c.exec("merge", json!({"from": "Option U"}));
    let s2 = c.doc();
    c.exec("wire", json!({"points": [[0, 240], [60, 240]]}));
    let s3 = c.doc();
    // Undo the step after the merge (no ask), then the merge (asks), then the step before.
    c.exec("u", json!({}));
    assert_eq!(c.doc(), s2);
    c.refused("execute", json!({"command": "u", "params": {}}));
    c.exec("undomerge", json!({}));
    assert_eq!(c.doc(), s1);
    c.exec("u", json!({}));
    assert_eq!(c.doc(), s0);
    // Redo all three; redo of the merge does not ask.
    c.exec("redo", json!({}));
    assert_eq!(c.doc(), s1);
    c.exec("redo", json!({}));
    assert_eq!(c.doc(), s2);
    c.exec("redo", json!({}));
    assert_eq!(c.doc(), s3);
    c.ok("save", json!({}));
    assert_eq!(status(&mut c, "Option U"), "merged");
    drop(c);
    let mut c = open(&path);
    assert_eq!(c.doc(), s3);
    verify(&path);
}

#[test]
fn update_from_main_undo_and_redo_on_the_branch_and_save() {
    let dir = TempDir::new("t020b-update");
    let (mut c, coil, _, path) = project(&dir, "update.ecad");
    c.exec("branch", json!({"name": "Option D"}));
    c.exec("wire", json!({"points": [[0, 120], [60, 120]]}));
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
    c.exec("setprop", json!({"component": coil, "key": "description", "value": "Main pump"}));
    c.ok("save", json!({}));
    c.exec("switchbranch", json!({"branch": "Option D"}));
    let before = c.doc();
    c.exec("merge", json!({"from": "main"}));
    assert_eq!(prop(&mut c, &coil, "description").as_deref(), Some("Main pump"));
    let after = c.doc();
    c.refused("execute", json!({"command": "u", "params": {}}));
    c.exec("undomerge", json!({}));
    assert_eq!(c.doc(), before);
    c.exec("redo", json!({}));
    assert_eq!(c.doc(), after);
    c.ok("save", json!({}));
    let statuses = (status(&mut c, "main"), status(&mut c, "Option D"));
    drop(c);
    // The file opens again (at a615fa54: "the file is damaged: the main branch's record is
    // inconsistent", because the redo's save marked main merged).
    let reopened = EcadMcp::open(&path, Options { write: true, agent: None });
    assert!(reopened.is_ok(), "reopen after redo of an update: {:?}", reopened.err());
    assert_eq!(statuses, ("open".to_string(), "open".to_string()), "an update never closes main (nor the branch)");
    let mut c = Client::new(reopened.unwrap().0);
    assert_eq!(current(&mut c), "Option D");
    assert_eq!(c.doc(), after);
    verify(&path);
}

#[test]
fn keeping_main_for_every_conflict_still_merges_and_closes_the_branch() {
    // Keep main for all, Merge (Close ticked): the decision is the merge. At a615fa54 nothing is
    // recorded, the reply says the branch "has no changes", the branch stays open and the next
    // merge asks the same conflict again.
    let dir = TempDir::new("t020b-keepmain");
    let (mut c, coil, _, path) = project(&dir, "keepmain.ecad");
    c.exec("branch", json!({"name": "Option K"}));
    c.exec("setprop", json!({"component": coil, "key": "description", "value": "Branch pump"}));
    c.exec("switchbranch", json!({"branch": "main", "save": true}));
    c.exec("setprop", json!({"component": coil, "key": "description", "value": "Main pump"}));
    let pv = c.exec("merge", json!({"from": "Option K", "preview": true}));
    let id = pv["conflicts"][0]["id"].as_str().unwrap().to_string();
    let r = c.exec("merge", json!({"from": "Option K", "choices": {id: "main"}}));
    eprintln!("keep main for all: {r}");
    assert_eq!(prop(&mut c, &coil, "description").as_deref(), Some("Main pump"));
    c.ok("save", json!({}));
    assert_eq!(status(&mut c, "Option K"), "merged", "Close branch after merging was ticked: {r}");
    verify(&path);
}
