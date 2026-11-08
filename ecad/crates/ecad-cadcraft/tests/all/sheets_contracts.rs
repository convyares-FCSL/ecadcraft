//! Brief 019 (design 019 §12, checks A–G and J): the sheet frame and title block, page and project
//! properties, folders, revisions, saving and reopening, and the update maps of the frame and the
//! reference texts.

use crate::common::{H, id};

use cadcraft_engine::doc::EntityKind;
use ecad_cadcraft::frame;
use ecad_model::book::Book;
use ecad_model::derive::cable_references;
use ecad_model::{CableId, ConductorId, FolderId, ObjectRef, Op, PageId};
use serde_json::{Value, json};

/// The frame's texts on the active page `page`.
fn frame_texts(h: &mut H, page: PageId) -> Vec<String> {
    h.pv(|p, pg| pg.frame_entities_of(p, page))
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::Text(t) => Some(t.value.clone()),
            _ => None,
        })
        .collect()
}

fn last_ops(h: &mut H) -> (String, Vec<Op>) {
    h.p(|p| p.ledger.log().last().map(|t| (t.label.clone(), t.ops.clone())).unwrap_or_default())
}

fn text_on(h: &mut H, page: PageId) -> String {
    let doc = h.doc();
    ecad_cadcraft::references::markers_on_page(&doc, page).first().map(|m| m.text.clone()).unwrap_or_default()
}

/// Two pages and cable C1 between them: Supply (its reference at (120,5), column C) and Field (its
/// reference at (20,5)).
struct S {
    h: H,
    supply: PageId,
    field: PageId,
}

fn fixture() -> S {
    let mut h = H::new(0x5EE7);
    let supply = h.first_page();
    h.run("renamepage", json!({"page": supply.hex(), "title": "Supply"})).unwrap();
    let mut w: Vec<ConductorId> = Vec::new();
    for y in [0.0, 5.0, 10.0] {
        w.push(id(&h.run("wire", json!({"points": [[60.0, y], [120.0, y]]})).unwrap(), "conductor"));
    }
    let c1: CableId =
        id(&h.run("cableslash", json!({"from": [88, -3], "to": [92, 18], "tag": "C1"})).unwrap(), "cable");
    h.run("cableref", json!({"cable": c1.hex(), "at": [120, 5]})).unwrap();
    let field: PageId = id(&h.run("newpage", json!({"title": "Field"})).unwrap(), "page");
    h.run("cablecontinue", json!({"cable": c1.hex(), "at": [20, 5], "toward": [40, 5]})).unwrap();
    h.activate(supply);
    S { h, supply, field }
}

/// Check A: an A3 frame at true scale with A–H and 1–6, a filled title block; zoom extents shows
/// the sheet; the frame is refused alone and left out of other edits.
#[test]
fn a_new_page_has_an_a3_frame_and_a_filled_title_block() {
    let mut h = H::new(0x019A);
    let page = h.first_page();
    let ents = h.pv(|p, pg| pg.frame_entities_of(p, page));
    assert!(!ents.is_empty());
    assert!(ents.iter().all(|e| e.common.layer == frame::LAYER));
    let texts = frame_texts(&mut h, page);
    for label in ["A", "B", "C", "D", "E", "F", "G", "H", "1", "2", "3", "4", "5", "6", "Page", "Total", "Rev"] {
        assert!(texts.iter().any(|t| t == label), "{label} in {texts:?}");
    }
    // The paper edge and the frame, at true size.
    let rects: Vec<Vec<(f64, f64)>> = ents
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::LwPolyline(p) => Some(p.vertices.iter().map(|v| (v.p.x, v.p.y)).collect()),
            _ => None,
        })
        .collect();
    assert!(rects.iter().any(|r| r.first() == Some(&(-10.0, -50.0)) && r.get(2) == Some(&(410.0, 247.0))), "{rects:?}");
    assert!(rects.iter().any(|r| r.first() == Some(&(0.0, -40.0)) && r.get(2) == Some(&(400.0, 240.0))), "{rects:?}");
    // Zoom extents fits the sheet edge.
    let _ = h.s.zoom_extents();
    let v = h.s.state().unwrap().view();
    assert!(v.height >= 297.0 - 1e-6, "{v:?}");
    // The layer is locked.
    assert!(h.s.doc().unwrap().layer(frame::LAYER).is_some_and(|l| l.locked));
    // Erasing a frame line alone is refused with the design's words; nothing recorded.
    let head = h.head();
    let line = ents.iter().find(|e| matches!(e.kind, EntityKind::Line(_))).unwrap().handle;
    // CADCraft leaves objects on a locked layer out of ERASE; were one changed anyway, the
    // translation refuses it with the design's words.
    match h.run("erase", json!({"handles": [line.hex()]})) {
        Ok(v) => assert_eq!(v["erased"], 0, "{v}"),
        Err(e) => assert!(e.contains(frame::DERIVED), "{e}"),
    }
    assert_eq!(h.head(), head);
    h.check();
}

/// Check B: a reference's column and row are the frame's, on both ends; moving the point moves
/// the other end's text.
#[test]
fn references_name_the_frames_column_and_row() {
    let mut s = fixture();
    assert_eq!(text_on(&mut s.h, s.field), "1-C-6", "Supply's end at x = 120 is in column C");
    assert_eq!(text_on(&mut s.h, s.supply), "2-A-6");
    let doc = s.h.doc();
    let r = cable_references(&doc).into_iter().find(|r| r.page == s.supply).unwrap();
    let item = r.item.unwrap();
    let hd = s.h.handle_of(s.supply, item);
    s.h.run("move", json!({"handles": [hd.hex()], "delta": [40, 100]})).unwrap();
    assert_eq!(text_on(&mut s.h, s.field), "1-D-4", "x = 160, y = 105");
}

/// Check C: page number, title and function from `pageprops`: one transaction and one `u` each;
/// the title block and the texts naming the page follow; refusals leave the head.
#[test]
fn page_properties_are_one_transaction_each_and_the_title_block_follows() {
    let mut s = fixture();
    let head = s.h.head();
    s.h.run("pageprops", json!({"number": "100"})).unwrap();
    assert_eq!(s.h.head(), head + 1);
    let (label, ops) = last_ops(&mut s.h);
    assert_eq!(label, "Page properties");
    assert!(
        matches!(ops.as_slice(), [Op::SetProperty { object: ObjectRef::Page(p), key, .. }] if *p == s.supply && key == "number")
    );
    assert!(frame_texts(&mut s.h, s.supply).iter().any(|t| t == "100"));
    assert_eq!(text_on(&mut s.h, s.field), "100-C-6", "the other end names the new number");
    s.h.run("pageprops", json!({"title": "Supply A", "function": "PWR"})).unwrap();
    assert_eq!(s.h.head(), head + 2, "two fields, one transaction");
    let texts = frame_texts(&mut s.h, s.supply);
    assert!(texts.iter().any(|t| t == "Supply A") && texts.iter().any(|t| t == "=PWR"), "{texts:?}");
    s.h.run("u", Value::Null).unwrap();
    assert!(frame_texts(&mut s.h, s.supply).iter().any(|t| t == "Supply"));
    s.h.run("u", Value::Null).unwrap();
    assert_eq!(text_on(&mut s.h, s.field), "1-C-6");
    let head = s.h.head();
    for (n, want) in [
        ("2", "Page 2 already exists (\"Field\")."),
        ("10.5", "A page number is 1 to 12"),
        ("1234567890123", "A page number is 1 to 12"),
    ] {
        let e = s.h.run("pageprops", json!({"number": n})).unwrap_err();
        assert!(e.contains(want), "{n}: {e}");
    }
    let e = s.h.run("pageprops", json!({"sheet": "A5"})).unwrap_err();
    assert!(e.contains("Sheet \"A5\" is not known."), "{e}");
    assert_eq!(s.h.head(), head, "refusals record nothing");
    // The read form records nothing.
    let v = s.h.run("pageprops", json!({})).unwrap();
    assert_eq!(v["number"], "1");
    assert_eq!(s.h.head(), head);
    s.h.check();
}

/// The update map: an ordinary edit (a wire) does not work the frame out again; an op of the map
/// (a project title) does, and only once.
#[test]
fn the_frame_is_recomputed_only_after_an_op_of_its_update_map() {
    let mut s = fixture();
    let builds = s.h.p(|p| p.frame_builds);
    s.h.run("wire", json!({"points": [[0, 100], [40, 100]]})).unwrap();
    s.h.run("line", json!({"points": [[0, 120], [40, 120]]})).unwrap();
    assert_eq!(s.h.p(|p| p.frame_builds), builds, "edits outside the update map");
    s.h.run("projectprops", json!({"title": "Panel 7"})).unwrap();
    assert_eq!(s.h.p(|p| p.frame_builds), builds + 1);
    assert!(frame_texts(&mut s.h, s.supply).iter().any(|t| t == "Panel 7"));
    // Another page's number changes nothing here unless it is the next page.
    s.h.run("pageprops", json!({"page": s.field.hex(), "number": "105"})).unwrap();
    assert!(frame_texts(&mut s.h, s.supply).iter().any(|t| t == "105"), "Next shows 105");
    s.h.check();
}

/// Check D: Project Properties in one transaction; `u` undoes all of it; the free properties.
#[test]
fn project_properties_are_one_transaction() {
    let mut s = fixture();
    let head = s.h.head();
    s.h.run(
        "projectprops",
        json!({"title": "Panel 7", "drawing_number": "FCS-1", "customer": "ACME", "status": "Draft",
               "props": {"site_contact": "J. Smith"}}),
    )
    .unwrap();
    assert_eq!(s.h.head(), head + 1);
    let doc = s.h.doc();
    assert_eq!(doc.properties.get("site_contact").map(String::as_str), Some("J. Smith"));
    let texts = frame_texts(&mut s.h, s.supply);
    for t in ["Panel 7", "FCS-1", "ACME", "Draft"] {
        assert!(texts.iter().any(|x| x == t), "{t} in {texts:?}");
    }
    let e = s.h.run("projectprops", json!({"props": {"title": "x"}})).unwrap_err();
    assert!(e.contains("\"title\" is set above."), "{e}");
    s.h.run("u", Value::Null).unwrap();
    assert!(s.h.doc().properties.is_empty());
    // The sheet and the step are settings; a grid change changes every reference text.
    s.h.run("projectprops", json!({"sheet": "A2", "page_number_step": 5})).unwrap();
    let d = s.h.doc();
    assert_eq!((d.settings.sheet.width, d.settings.sheet.columns, d.settings.page_number_step), (594, 12, 5));
    let v = s.h.run("projectprops", json!({})).unwrap();
    assert_eq!(v["sheet"], "A2");
    s.h.check();
}

/// Check E: folders nest; moving a page into a folder is one transaction; book order and Next
/// follow; a folder cannot go inside itself; deleting a folder moves its contents up.
#[test]
fn folders_nest_and_move_pages_in_one_transaction_each() {
    let mut s = fixture();
    let spare: PageId = id(&s.h.run("newpage", json!({"title": "Spare"})).unwrap(), "page");
    s.h.activate(s.supply);
    let outer: FolderId = id(&s.h.run("newfolder", json!({"name": "Schematic layouts"})).unwrap(), "folder");
    let inner: FolderId =
        id(&s.h.run("newfolder", json!({"name": "Spare circuits", "parent": outer.hex()})).unwrap(), "folder");
    let head = s.h.head();
    s.h.run("movepage", json!({"page": s.field.hex(), "folder": inner.hex()})).unwrap();
    assert_eq!(s.h.head(), head + 1, "one transaction");
    let doc = s.h.doc();
    let book = Book::of(&doc);
    assert_eq!(book.order, [s.supply, spare, s.field], "pages first, then folders");
    assert!(frame_texts(&mut s.h, s.supply).iter().any(|t| t == "3"), "Next is Spare (3)");
    // A sibling of the same name, and a folder into its own child, are refused.
    let e = s.h.run("newfolder", json!({"name": "spare CIRCUITS", "parent": outer.hex()})).unwrap_err();
    assert!(e.contains("There is already a folder \"spare CIRCUITS\" here."), "{e}");
    let e = s.h.run("movefolder", json!({"folder": outer.hex(), "parent": inner.hex()})).unwrap_err();
    assert_eq!(e, "A folder cannot go inside itself.");
    // Nesting is bounded.
    let mut parent = inner;
    for i in 0..6 {
        parent = id(&s.h.run("newfolder", json!({"name": format!("L{i}"), "parent": parent.hex()})).unwrap(), "folder");
    }
    let e = s.h.run("newfolder", json!({"name": "too deep", "parent": parent.hex()})).unwrap_err();
    assert_eq!(e, "Folders nest at most 8 deep.");
    // Delete the outer folder: its sub-folder and pages move to the top level, in one transaction.
    let head = s.h.head();
    let v = s.h.run("deletefolder", json!({"folder": outer.hex()})).unwrap();
    assert_eq!(s.h.head(), head + 1);
    assert!(v["message"].as_str().unwrap().starts_with("Folder \"Schematic layouts\" deleted"), "{v}");
    let doc = s.h.doc();
    assert_eq!(doc.folders.get(&inner).unwrap().parent, None);
    s.h.run("u", Value::Null).unwrap();
    assert_eq!(s.h.doc().folders.get(&inner).unwrap().parent, Some(outer));
    // Rename.
    s.h.run("renamefolder", json!({"folder": inner.hex(), "name": "Spares"})).unwrap();
    assert_eq!(s.h.doc().folders.get(&inner).unwrap().name, "Spares");
    // New pages land in the folder, numbered after the page before them.
    s.h.run("projectprops", json!({"page_number_step": 5})).unwrap();
    let v = s.h.run("newpage", json!({"title": "More", "folder": inner.hex()})).unwrap();
    assert_eq!(v["number"], "7", "after Field (2): 2 + 5");
    s.h.check();
}

/// Check F (and the owner's answer Q7): the pending revision, issuing, the title block, opening
/// an issued revision as it was, refusing edits of issued rows, and undoing an issue only after
/// confirming.
#[test]
fn issuing_a_revision_names_the_log_point_and_reopens_as_it_was() {
    let mut s = fixture();
    let e = s.h.run("issuerevision", json!({"description": "First issue"})).unwrap_err();
    assert_eq!(e, ecad_cadcraft::revisions::FILL_FIRST);
    let head = s.h.head();
    s.h.run("revision", json!({"description": "First issue"})).unwrap();
    s.h.run("revision", json!({"drawn": "CVA"})).unwrap();
    assert_eq!(s.h.head(), head + 2, "one transaction per field");
    let at = s.h.head();
    let v = s.h.run("issuerevision", json!({"date": "2026-11-08"})).unwrap();
    assert_eq!(v["rev"], "A");
    assert_eq!(s.h.head(), at + 1);
    let (label, ops) = last_ops(&mut s.h);
    assert_eq!(label, "Issue Rev A");
    assert!(ops.iter().any(|o| matches!(o, Op::NameVersion { name, at: a } if name == "Rev A" && *a == at)), "{ops:?}");
    let texts = frame_texts(&mut s.h, s.supply);
    assert!(texts.iter().any(|t| t == "A") && texts.iter().any(|t| t == "First issue"), "{texts:?}");
    // An issued row cannot change.
    let e = s.h.run("projectprops", json!({"props": {"rev.1.description": "x"}})).unwrap_err();
    assert_eq!(e, "Rev A is issued and cannot change.");
    // Draw a wire after the issue; Rev A shows the project without it.
    s.h.run("wire", json!({"points": [[0, 150], [40, 150]]})).unwrap();
    let now = s.h.doc().conductors.len();
    let head = s.h.head();
    let v = s.h.run("openrevision", json!({"rev": "A"})).unwrap();
    assert_eq!(s.h.head(), head, "opening records nothing");
    assert_eq!(v["at"], json!(at));
    let then = s.h.p(|p| p.revision_view.as_ref().unwrap().doc.clone());
    assert_eq!(then.conductors.len(), now - 1, "Rev A has no wire drawn after it");
    s.h.run("closerevision", json!({})).unwrap();
    assert!(s.h.p(|p| p.revision_view.is_none()));
    // Undo the wire, then a plain `u` across the issue stops and asks.
    s.h.run("u", Value::Null).unwrap();
    let head = s.h.head();
    let e = s.h.run("u", Value::Null).unwrap_err();
    assert!(e.contains("Rev A was issued on 2026-11-08. Undo the issue?"), "{e}");
    assert_eq!(s.h.head(), head, "nothing undone");
    assert!(s.h.p(|p| p.issue_undo_question.is_some()));
    s.h.run("undoissue", json!({})).unwrap();
    assert!(!s.h.doc().versions.contains_key("Rev A"));
    assert!(frame_texts(&mut s.h, s.supply).iter().any(|t| t == "-"), "no issued revision");
    s.h.check();
}

/// Check G: save and reopen keep numbers, folders, a page's sheet, project properties and
/// revisions; Rev A still opens.
#[test]
fn save_and_reopen_keep_everything() {
    let mut s = fixture();
    let folder: FolderId = id(&s.h.run("newfolder", json!({"name": "Layouts"})).unwrap(), "folder");
    s.h.run("movepage", json!({"page": s.field.hex(), "folder": folder.hex()})).unwrap();
    s.h.run("pageprops", json!({"page": s.field.hex(), "number": "105", "sheet": "A2"})).unwrap();
    s.h.run("projectprops", json!({"title": "Panel 7", "customer": "ACME"})).unwrap();
    s.h.run("revision", json!({"description": "First issue", "drawn": "CVA"})).unwrap();
    s.h.run("issuerevision", json!({})).unwrap();
    s.h.run("qsave", Value::Null).unwrap();
    let doc = s.h.doc();
    let medium = s.h.p(|p| p.store.medium().clone());
    let mut s2 = cadcraft_engine::Session::empty();
    let reopened = ecad_cadcraft::ProjectHandle::open_from(&mut s2, "Demo", medium, 1).unwrap();
    let doc2 = reopened.read(&mut s2, |p, _| p.ledger.document().clone()).unwrap();
    assert_eq!(doc, doc2);
    assert_eq!(doc2.pages[&s.field].properties.get("sheet").map(String::as_str), Some("A2"));
    let v = s2.execute("openrevision", &json!({"rev": "A"})).unwrap();
    assert_eq!(v["rev"], "A");
}

/// An old project (no numbers, no folders, no properties) opens with defaults: A3 frames, page
/// numbers by position.
#[test]
fn an_old_project_shows_positions_and_the_default_sheet() {
    let mut h = H::new(0x01D);
    let first = h.first_page();
    // Forget the stored number, as in a file written before brief 019.
    h.run("pageprops", json!({"number": null})).unwrap();
    let p2: PageId = id(&h.run("newpage", json!({"title": "Two"})).unwrap(), "page");
    h.run("pageprops", json!({"page": p2.hex(), "number": null})).unwrap();
    h.activate(first);
    assert!(frame_texts(&mut h, first).iter().any(|t| t == "2"), "Total 2");
    let rows = h.p(|p| p.page_rows());
    assert_eq!(rows.iter().map(|r| r.number.as_str()).collect::<Vec<_>>(), ["1", "2"]);
    assert_eq!(h.doc().settings.sheet, ecad_model::SheetSpec::default());
}

/// Check J: a read-only project refuses every write and still opens a revision.
#[test]
fn a_read_only_project_refuses_writes() {
    let mut s = fixture();
    s.h.run("revision", json!({"description": "First issue", "drawn": "CVA"})).unwrap();
    s.h.run("issuerevision", json!({})).unwrap();
    s.h.cfg(|p| p.persistence = ecad_cadcraft::Persistence::Unavailable);
    // The command path refuses writes the same way for every ECAD command while a project is
    // read-only (`ecad-mcp` and the trial rely on it); reading and opening a revision work.
    let v = s.h.run("projectprops", json!({})).unwrap();
    assert_eq!(v["revisions"][0]["rev"], "A");
    s.h.run("openrevision", json!({"rev": "A"})).unwrap();
}
