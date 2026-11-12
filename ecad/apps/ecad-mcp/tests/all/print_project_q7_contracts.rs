//! Brief 033, MCP contracts for the pack quick fixes (design 033 §6–§8): the pending-revision
//! marker (AC9), revision columns and custom fields (AC10), the tag style and its rewrite (AC11),
//! renumbering under `typed` (AC12). Each write is the agent's transaction; writes off refuse.

use ecad_mcp::policy;
use ecad_model::{Author, ObjectRef, Op, PageId};
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, reader};

fn writer() -> Client {
    Client::new(ecad_mcp::EcadMcp::in_memory("mcp", 0x33_07, ecad_mcp::Options { write: true, agent: None }).unwrap())
}

fn last_by_agent(c: &mut Client) -> ecad_model::Transaction {
    let last = c.log().last().cloned().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()));
    last
}

/// Every text the first page's frame shows, with the project's pending flag; and the Rev box.
fn frame_texts(c: &mut Client) -> (Vec<String>, String) {
    c.s.with_session(|s, h| {
        h.read(s, |p, _| {
            let pending = ecad_cadcraft::revisions::is_pending(&p.ledger);
            assert_eq!(p.pending, pending, "the kept flag is the walk's");
            let doc = p.ledger.document();
            let page: PageId = doc.page_order[0];
            let texts = ecad_cadcraft::generated::parts_of_pending(doc, page, None, pending)
                .into_iter()
                .filter_map(|part| match part.kind {
                    cadcraft_engine::doc::EntityKind::Text(t) => Some(t.value),
                    _ => None,
                })
                .collect();
            (texts, ecad_cadcraft::revisions::rev_box(doc, pending))
        })
        .unwrap()
    })
    .unwrap()
}

fn rev(c: &mut Client) -> String {
    let (texts, shown) = frame_texts(c);
    assert!(texts.contains(&shown), "the Rev box shows {shown}: {texts:?}");
    shown
}

fn wire(c: &mut Client, y: f64) {
    c.exec("wire", json!({"points": [[20, y], [80, y]]}));
}

#[test]
fn ac9_the_pending_marker_follows_the_undo_stack() {
    let mut c = writer();
    wire(&mut c, 100.0);
    assert_eq!(rev(&mut c), "-", "no issued revision: no marker");
    c.exec("revision", json!({"description": "First issue", "drawn": "CVA"}));
    c.exec("issuerevision", json!({}));
    assert_eq!(rev(&mut c), "A");
    wire(&mut c, 110.0);
    assert_eq!(rev(&mut c), "A*", "an edit after the issue");
    c.exec("u", json!({}));
    assert_eq!(rev(&mut c), "A", "undone: the issue is on top again");
    c.exec("revision", json!({"description": "next"}));
    assert_eq!(rev(&mut c), "A", "filling in the next row is not a drawing change");
    wire(&mut c, 120.0);
    assert_eq!(rev(&mut c), "A*");
    c.exec("projectprops", json!({"revision_pending": "draft"}));
    let last = last_by_agent(&mut c);
    assert!(
        matches!(&last.ops[..], [Op::SetProperty { object: ObjectRef::Project, key, after: Some(v), .. }] if key == "revisions.pending" && v == "draft")
    );
    assert_eq!(rev(&mut c), "B (draft)");
    c.exec("projectprops", json!({"revision_pending": "off"}));
    assert_eq!(rev(&mut c), "A");
    c.exec("projectprops", json!({"revision_pending": "star"}));
    assert_eq!(rev(&mut c), "A*");
    // Print Book says so.
    let p = c.exec("printbook", json!({}));
    assert!(p["notes"].as_array().unwrap().iter().any(|n| n == "The drawing has changed since Rev A; the title block shows A*."), "{p}");
    // The cover says so too.
    let cover = c.exec("newpage", json!({"generated": "cover"}));
    let page = PageId::parse(cover["page"].as_str().unwrap()).unwrap();
    let texts =
        c.s.with_session(|s, h| {
            h.read(s, |p, _| {
                ecad_cadcraft::generated::parts_of_pending(p.ledger.document(), page, None, p.pending)
                    .into_iter()
                    .filter_map(|part| match part.kind {
                        cadcraft_engine::doc::EntityKind::Text(t) => Some(t.value),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap()
        })
        .unwrap();
    assert!(texts.iter().any(|t| t.starts_with("Revision A") && t.ends_with("(changed since)")), "{texts:?}");
    // Issuing ends it.
    c.exec("revision", json!({"drawn": "CVA"}));
    c.exec("issuerevision", json!({}));
    assert_eq!(rev(&mut c), "B");
}

#[test]
fn ac10_revision_columns_and_custom_fields() {
    let mut c = writer();
    wire(&mut c, 100.0);
    c.exec("projectprops", json!({"revision_columns": "rev, date, ecr:ECR, description, drawn"}));
    let last = last_by_agent(&mut c);
    assert_eq!(last.label, "Project properties");
    assert_eq!(last.ops.len(), 1);
    c.exec("revision", json!({"description": "First", "drawn": "CVA", "fields": {"ecr": "ECR-1"}}));
    let last = last_by_agent(&mut c);
    assert!(
        last.ops.iter().any(|o| matches!(o, Op::SetProperty { key, after: Some(v), .. } if key == "rev.1.ecr" && v == "ECR-1")),
        "{:?}",
        last.ops
    );
    c.exec("issuerevision", json!({}));
    let (texts, _) = frame_texts(&mut c);
    assert!(texts.contains(&"ECR".to_string()) && texts.contains(&"ECR-1".to_string()), "{texts:?}");
    assert!(!texts.contains(&"Checked".to_string()), "a column left out is not shown: {texts:?}");
    // An issued row never changes.
    let head = c.log().len();
    let m = c.refused("execute", json!({"command": "revision", "params": {"n": 1, "fields": {"ecr": "X"}}}));
    assert!(m.contains("Rev A is issued and cannot change."), "{m}");
    // The refusals of design 033 §7.1, each leaving the head where it was.
    for (cols, words) in [
        ("rev, ecr!, description", "\"ecr!\" is not a field name. Use 1 to 20 letters, digits or _."),
        ("rev, date", "Rev and Description must be columns."),
        ("rev, description, a, b, c, d, e, f, g, h, i", "At most 10 columns."),
        ("rev, issued, description", "\"issued\" is set by issuing."),
    ] {
        let m = c.refused("execute", json!({"command": "projectprops", "params": {"revision_columns": cols}}));
        assert!(m.contains(words), "{cols}: {m}");
    }
    assert_eq!(c.log().len(), head);
    // Wide columns are narrowed, never refused.
    let v = c.exec("projectprops", json!({"revision_columns": "rev@100, description, date@100"}));
    assert_eq!(v["narrowed"], json!(ecad_cadcraft::revisions::NARROWED), "{v}");
    // Cleared: the six built-ins again; the custom value is kept in the file.
    c.exec("projectprops", json!({"revision_columns": null}));
    let (texts, _) = frame_texts(&mut c);
    assert!(texts.contains(&"Checked".to_string()) && !texts.contains(&"ECR-1".to_string()), "{texts:?}");
    assert_eq!(c.doc().properties.get("rev.1.ecr").map(String::as_str), Some("ECR-1"));
    // The revision list page shows the columns, then Version.
    c.exec("projectprops", json!({"revision_columns": "rev, ecr:ECR, description"}));
    let list = c.exec("newpage", json!({"generated": "revisions"}));
    let page = PageId::parse(list["page"].as_str().unwrap()).unwrap();
    let doc = c.doc();
    let texts: Vec<String> = ecad_cadcraft::generated::parts_of(&doc, page, None)
        .into_iter()
        .filter_map(|part| match part.kind {
            cadcraft_engine::doc::EntityKind::Text(t) => Some(t.value),
            _ => None,
        })
        .collect();
    for t in ["ECR", "Version", "ECR-1"] {
        assert!(texts.contains(&t.to_string()), "{t}: {texts:?}");
    }
}

fn tags(c: &mut Client) -> Vec<String> {
    c.doc().components.values().filter_map(|x| x.properties.get("tag").cloned()).collect()
}

#[test]
fn ac11_the_tag_style_rewrites_in_one_undoable_step() {
    let mut c = writer();
    for x in [40, 80, 120] {
        c.exec("place", json!({"symbol": "coil", "at": [x, 150]}));
    }
    assert_eq!(tags(&mut c), ["-K01", "-K02", "-K03"]);
    let before = c.doc();
    let v = c.exec("projectprops", json!({"tag_dash": false}));
    assert_eq!(v["message"], json!("Tag style: no leading -. 3 tags rewritten."), "{v}");
    assert_eq!((v["tag_dash"].clone(), v["tag_case"].clone(), v["tag_padding"].clone()), (json!(false), json!("upper"), json!(2)));
    let last = last_by_agent(&mut c);
    assert_eq!(last.label, "Project properties");
    assert_eq!(last.ops.iter().filter(|o| matches!(o, Op::SetProperty { key, .. } if key == "tag")).count(), 3);
    assert_eq!(tags(&mut c), ["K01", "K02", "K03"]);
    // Identity is unchanged: the index still finds the device by either form.
    let doc = c.doc();
    assert_eq!(doc.components_tagged("-K01"), doc.components_tagged("K01"));
    assert_eq!(doc.components_tagged("k1").len(), 1);
    // One undo puts the setting and every tag back.
    c.exec("u", json!({}));
    let after_undo = c.doc();
    assert_eq!(after_undo.properties.get("tags.dash"), before.properties.get("tags.dash"));
    assert_eq!(tags(&mut c), ["-K01", "-K02", "-K03"]);
    // As typed: nothing rewritten; a tag typed now keeps its case.
    let v = c.exec("projectprops", json!({"tag_case": "typed"}));
    assert_eq!(v["message"], json!("Tags typed from now on keep their case; existing tags are unchanged."));
    let k1 = *c.doc().components.keys().next().unwrap();
    c.exec("setprop", json!({"component": k1.hex(), "key": "tag", "value": "M02a"}));
    assert!(tags(&mut c).contains(&"-M02a".to_string()), "{:?}", tags(&mut c));
    // Back to upper case: every tag upper-cased in the same transaction.
    let v = c.exec("projectprops", json!({"tag_case": "upper"}));
    assert_eq!(v["message"], json!("Tag style: upper case. 1 tag rewritten."), "{v}");
    assert!(tags(&mut c).contains(&"-M02A".to_string()));
}

#[test]
fn ac12_renumber_keeps_typed_letters() {
    let mut c = writer();
    c.exec("projectprops", json!({"tag_case": "typed"}));
    let a = c.exec("place", json!({"symbol": "coil", "at": [40, 150]}));
    let b = c.exec("place", json!({"symbol": "coil", "at": [80, 150]}));
    c.exec("setprop", json!({"component": a["component"], "key": "tag", "value": "k5"}));
    c.exec("setprop", json!({"component": b["component"], "key": "tag", "value": "k7"}));
    // Typing fixed them; release them so renumber may change them.
    for x in [&a, &b] {
        c.exec("setprop", json!({"component": x["component"], "key": "tag_fixed", "value": null}));
    }
    assert_eq!(tags(&mut c), ["-k05", "-k07"]);
    c.exec("renumber", json!({"kind": "devices"}));
    let mut t = tags(&mut c);
    t.sort();
    assert_eq!(t, ["-k01", "-k02"]);
}

#[test]
fn with_writes_off_the_q7_writes_are_refused() {
    let mut c = reader();
    let before = c.log();
    for (cmd, params) in [
        ("projectprops", json!({"tag_dash": false})),
        ("projectprops", json!({"tag_case": "typed"})),
        ("projectprops", json!({"revision_columns": "rev, ecr, description"})),
        ("projectprops", json!({"revision_pending": "off"})),
        ("revision", json!({"fields": {"ecr": "ECR-1"}})),
    ] {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd} {params}");
    }
    let v: Value = c.exec("projectprops", json!({}));
    assert_eq!(v["tag_dash"], json!(true));
    assert_eq!(v["revision_pending"], json!("star"));
    assert_eq!(c.log(), before);
}

#[test]
fn ac9_the_pending_marker_survives_save_and_reopen() {
    let dir = crate::mcp_contracts::TempDir::new("pending");
    let path = dir.file("pending.ecad");
    {
        let mut c = Client::new(ecad_mcp::EcadMcp::create(&path, ecad_mcp::Options { write: true, agent: None }).unwrap());
        wire(&mut c, 100.0);
        c.exec("revision", json!({"description": "First issue", "drawn": "CVA"}));
        c.exec("issuerevision", json!({}));
        wire(&mut c, 110.0);
        assert_eq!(rev(&mut c), "A*");
        c.ok("save", json!({}));
    }
    let (s, _) = ecad_mcp::EcadMcp::open(&path, ecad_mcp::Options::default()).unwrap();
    let mut c = Client::new(s);
    let (_, shown) = frame_texts(&mut c);
    assert_eq!(shown, "A*", "reopened: the same state");
}
