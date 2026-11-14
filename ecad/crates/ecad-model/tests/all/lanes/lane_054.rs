//! Brief 054 (FP3, design 054 §3.1): a page's sheet reads its page type's stored sheet, between
//! its own sheet and the project's; old files with no type record resolve exactly as before.

use ecad_model::sheet::{PAGE_TYPE_KEY, SheetSpec, derived_page_type, page_type_of, sheet_of};
use ecad_model::{Author, IdAllocator, Ledger, ObjectRef, Op, Page, PageId, StyleLayer, StyleRecord, StyleRef};

fn record(sheet: &str) -> StyleRecord {
    let mut r = StyleRecord::default();
    r.fields.insert("sheet".into(), sheet.into());
    r
}

fn set_style(id: &str, after: Option<StyleRecord>, before: Option<StyleRecord>) -> Op {
    Op::SetStyle { layer: StyleLayer::Project, catalogue: "page".into(), id: StyleRef::Builtin(id.into()), before, after }
}

#[test]
fn a_page_takes_its_types_sheet_between_its_own_and_the_projects() {
    let mut ids = IdAllocator::seeded(54);
    let mut ledger = Ledger::new(54);
    let pages: Vec<PageId> = (0..4).map(|_| ids.fresh()).collect();
    let mut ops: Vec<Op> = pages.iter().enumerate().map(|(i, p)| Op::AddPage { id: *p, page: Page::new(format!("{i}")), index: i }).collect();
    let set = |p: PageId, k: &str, v: &str| Op::SetProperty { object: ObjectRef::Page(p), key: k.into(), before: None, after: Some(v.into()) };
    ops.push(set(pages[1], "sheet", "A1"));
    ops.push(set(pages[2], "generated", "cover"));
    ops.push(set(pages[3], PAGE_TYPE_KEY, "network"));
    ledger.commit(Author::User, "pages", ops).unwrap();
    let a3 = SheetSpec::default();
    let before: Vec<SheetSpec> = pages.iter().map(|p| sheet_of(ledger.document(), *p)).collect();
    assert_eq!(before, [a3, SheetSpec::preset("A1").unwrap(), a3, a3], "no record: as before types");
    ledger.commit(Author::User, "schematic A2", vec![set_style("schematic", Some(record("A2")), None)]).unwrap();
    let doc = ledger.document();
    let a2 = SheetSpec::preset("A2").unwrap();
    assert_eq!(sheet_of(doc, pages[0]), a2, "a schematic page takes its type's sheet");
    assert_eq!(sheet_of(doc, pages[1]).text(), "A1", "its own sheet wins");
    assert_eq!(sheet_of(doc, pages[2]), a3, "a cover is a Front sheet page");
    assert_eq!(sheet_of(doc, pages[3]), a3, "a network page is not a schematic one");
    // `project` and anything unreadable fall through.
    for v in ["project", "A0 999x999", "x"] {
        let before = ledger.document().styles.get("page").and_then(|c| c.get(&StyleRef::Builtin("schematic".into()))).cloned();
        ledger.commit(Author::User, v, vec![set_style("schematic", Some(record(v)), before)]).unwrap();
        assert_eq!(sheet_of(ledger.document(), pages[0]), a3, "{v}");
    }
    ledger.undo(Author::User).unwrap();
    ledger.undo(Author::User).unwrap();
    ledger.undo(Author::User).unwrap();
    assert_eq!(sheet_of(ledger.document(), pages[0]), a2);
    assert!(ledger.replay_matches().unwrap());
}

#[test]
fn a_pages_type_comes_from_its_property_its_kind_or_schematic() {
    assert_eq!(derived_page_type(None), "schematic");
    for k in ["cover", "revisions", "contents"] {
        assert_eq!(derived_page_type(Some(k)), "front", "{k}");
    }
    for k in ["bom-location", "terminals", "cables", "plcrack", "iolist"] {
        assert_eq!(derived_page_type(Some(k)), "report", "{k}");
    }
    assert_eq!(derived_page_type(Some("nonsense")), "schematic");
    let ledger = Ledger::new(1);
    let doc = ledger.document();
    let mut props = ecad_model::Properties::new();
    props.insert(PAGE_TYPE_KEY.into(), "#000000000000000000000000000000ff".into());
    props.insert("generated".into(), "iolist".into());
    assert_eq!(page_type_of(doc, &props), StyleRef::Builtin("report".into()), "a type that does not exist reads as absent");
    props.insert(PAGE_TYPE_KEY.into(), "panel".into());
    assert_eq!(page_type_of(doc, &props), StyleRef::Builtin("panel".into()));
    props.insert(PAGE_TYPE_KEY.into(), "x".repeat(10_000));
    assert_eq!(page_type_of(doc, &props), StyleRef::Builtin("report".into()));
}
