//! Tester 054 through MCP (brief 011 rule; design 054 §12.1 AC16). On a project saved at a path
//! with spaces and non-ASCII letters, each writing form (`newpage {type}`, `newpage {type,
//! generated, after}`, `setstyle {catalogue: "page"}` new and change, `setdefault page.type` at page
//! level, `setdefault front.cover.notes` at project level, `projectprops {props: {cover.notes}}`,
//! `folderprops`, `renumberpages`, `deletepage`) records one transaction by
//! `Author::Agent(<client>)`, and one `u` restores the document; `styles {catalogue: page}` records
//! nothing; with writes off every write is refused with `WRITES_OFF` and reads are unchanged.
//! Written from brief and design 054 before the implementation.

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

/// The command recorded one transaction by the agent; `u` restores `before` exactly; redo.
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

fn names(ops: &[Op]) -> Vec<&'static str> {
    ops.iter().map(Op::name).collect()
}

fn reads(c: &mut Client) -> Value {
    json!({
        "types": c.exec("styles", json!({"catalogue": "page"})),
        "defaults": c.exec("defaults", json!({"key": "page.type"})),
        "checks": c.exec("checks", json!({})),
    })
}

/// AC16: every writing form by the agent, one transaction, one undo; then writes off.
#[test]
#[ignore = "tester 054: awaiting build"]
fn ac16_page_type_writes_by_the_agent_then_refused_with_writes_off() {
    let dir = TempDir::new("t054 Seitentypen mit Leerzeichen \u{e4}");
    let path = dir.file("Projekt 054 (A).ecad");
    let (before, page, folder, cover) = {
        let mut c = create(&path);
        let first = c.doc().page_order[0].hex();
        let n = c.log().len();
        let rows = c.exec("styles", json!({"catalogue": "page"}));
        assert!(rows.to_string().contains("schematic"), "{rows}");
        assert_eq!(c.log().len(), n, "reads record nothing");

        let (v, ops) = one_by_agent_then_undo(&mut c, "newpage", json!({"type": "network", "after": first}));
        assert_eq!(names(&ops), ["AddPage"]);
        let page = s(&v["page"]);
        let (v, ops) = one_by_agent_then_undo(&mut c, "newpage", json!({"type": "front", "generated": "cover", "after": first}));
        assert_eq!(names(&ops), ["AddPage"]);
        let cover = s(&v["page"]);
        let (_, ops) = one_by_agent_then_undo(&mut c, "newpage", json!({"type": "front", "generated": "revisions", "after": cover}));
        assert_eq!(names(&ops), ["AddPage"]);
        let revs = c.log().last().and_then(|t| t.ops.first().cloned());
        let (v, ops) = one_by_agent_then_undo(
            &mut c,
            "setstyle",
            json!({"catalogue": "page", "new": true, "name": "Wide", "copy_of": "schematic", "fields": {"sheet": "A1"}}),
        );
        assert_eq!(names(&ops), ["SetStyle"]);
        assert!(s(&v["id"]).starts_with('#'), "{v}");
        let (_, ops) =
            one_by_agent_then_undo(&mut c, "setstyle", json!({"catalogue": "page", "id": "schematic", "fields": {"sheet": "A2", "zones": "off"}}));
        assert_eq!(names(&ops), ["SetStyle"]);
        let (_, ops) =
            one_by_agent_then_undo(&mut c, "setdefault", json!({"key": "page.type", "level": "page", "page": page, "value": "singleline"}));
        assert!(matches!(ops.as_slice(), [Op::SetProperty { object: ecad_model::ObjectRef::Page(_), .. }]), "{ops:?}");
        one_by_agent_then_undo(&mut c, "setdefault", json!({"key": "front.cover.notes", "level": "project", "value": "off"}));
        let (_, ops) = one_by_agent_then_undo(&mut c, "projectprops", json!({"props": {"cover.notes": "First note\nSecond note"}}));
        assert!(matches!(ops.as_slice(), [Op::SetProperty { object: ecad_model::ObjectRef::Project, key, .. }] if key == "cover.notes"), "{ops:?}");
        let f = s(&c.exec("newfolder", json!({"name": "Schematic"}))["folder"]);
        c.exec("movepage", json!({"page": page, "folder": f}));
        let (_, ops) = one_by_agent_then_undo(&mut c, "folderprops", json!({"folder": f, "start": "100"}));
        assert!(
            matches!(ops.as_slice(), [Op::SetProperty { object: ecad_model::ObjectRef::Project, key, .. }] if key == &format!("folder.{f}.start")),
            "FP7: {ops:?}"
        );
        let (_, ops) = one_by_agent_then_undo(&mut c, "renumberpages", json!({}));
        assert!(!ops.is_empty() && ops.iter().all(|o| o.name() == "SetProperty"), "{ops:?}");
        let revs_id = match revs {
            Some(Op::AddPage { id, .. }) => id.hex(),
            other => panic!("{other:?}"),
        };
        let (_, ops) = one_by_agent_then_undo(&mut c, "deletepage", json!({"page": revs_id}));
        assert_eq!(names(&ops), ["RemovePage"]);

        // Refusals record nothing.
        let n = c.log().len();
        for (cmd, params) in [
            ("newpage", json!({"type": "front"})),
            ("newpage", json!({"generated": "cover", "type": "report"})),
            ("folderprops", json!({"folder": f, "start": "1a"})),
            ("deletepage", json!({"page": first})),
            ("projectprops", json!({"props": {"cover.notes": "x".repeat(401)}})),
            ("setstyle", json!({"catalogue": "page", "id": "schematic", "fields": {"position": "x"}})),
            ("setdefault", json!({"key": "page.type", "level": "project", "value": "network"})),
        ] {
            let m = c.refused("execute", json!({"command": cmd, "params": params}));
            assert!(!m.is_empty(), "{cmd}");
        }
        assert_eq!(c.log().len(), n, "refusals record nothing");
        let before = reads(&mut c);
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
        (before, page, f, cover)
    };

    let (sess, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut c = Client::new(sess);
    assert!(!c.s.writes());
    let log = c.log();
    for (cmd, params) in [
        ("newpage", json!({"type": "network", "after": page})),
        ("newpage", json!({"type": "front", "generated": "contents", "after": cover})),
        ("setstyle", json!({"catalogue": "page", "id": "schematic", "fields": {"sheet": "A1"}})),
        ("setstyle", json!({"catalogue": "page", "new": true, "name": "New", "copy_of": "schematic"})),
        ("setdefault", json!({"key": "page.type", "level": "page", "page": page, "value": "network"})),
        ("setdefault", json!({"key": "front.cover.notes", "level": "project", "value": "on"})),
        ("projectprops", json!({"props": {"cover.notes": "x"}})),
        ("folderprops", json!({"folder": folder, "start": "200"})),
        ("renumberpages", json!({})),
        ("deletepage", json!({"page": cover})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd} {params}");
    }
    assert_eq!(reads(&mut c), before, "reads after reopen with writes off");
    assert_eq!(c.log(), log, "nothing recorded with writes off");
}

/// The new commands are listed with parameter docs; `newpage` documents `type` and `after`.
#[test]
#[ignore = "tester 054: awaiting build"]
fn ac16_the_new_commands_are_listed() {
    let mut c = crate::mcp_contracts::reader();
    let catalog = c.ok("list_commands", json!({}));
    let catalog = catalog.as_array().cloned().unwrap_or_default();
    for id in ["folderprops", "renumberpages", "deletepage", "newpage"] {
        let info = catalog.iter().find(|i| i["id"] == id).unwrap_or_else(|| panic!("{id} not listed"));
        assert!(info["params"].as_str().is_some_and(|p| !p.is_empty()), "{id} has parameter docs");
    }
    let np = catalog.iter().find(|i| i["id"] == "newpage").unwrap();
    let p = s(&np["params"]);
    assert!(p.contains("type") && p.contains("after"), "{p}");
}
