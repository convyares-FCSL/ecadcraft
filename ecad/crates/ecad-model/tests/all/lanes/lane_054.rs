//! Brief 054 (FP3, design 054 §3.1): a page's sheet reads its page type's stored sheet, between
//! its own sheet and the project's; old files with no type record resolve exactly as before.

use ecad_model::sheet::{SheetSpec, derived_page_type, page_type_of, sheet_of};
use ecad_model::{Author, IdAllocator, Ledger, Op, Page, PageId, StyleLayer, StyleRecord, StyleRef};

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
    let mut made: Vec<Page> = (0..4).map(|i| Page::titled(format!("{i}")).unwrap()).collect();
    made[1].sheet = SheetSpec::parse("A1").ok();
    made[2].kind = ecad_model::PageKind::Generated(ecad_model::GenKind::Cover);
    made[3].page_type = StyleRef::builtin("network");
    let ops: Vec<Op> = pages.iter().zip(made).enumerate().map(|(i, (p, page))| Op::AddPage { id: *p, page, index: i }).collect();
    ledger.commit(Author::User, "pages", ops).unwrap();
    let a3 = SheetSpec::default();
    let before: Vec<SheetSpec> = pages.iter().map(|p| sheet_of(ledger.document(), *p)).collect();
    assert_eq!(before, [a3, SheetSpec::preset("A1").unwrap(), a3, a3], "no record: as before types");
    ledger.commit(Author::User, "schematic A2", vec![set_style("schematic", Some(record("A2")), None)]).unwrap();
    let doc = ledger.document();
    let a2 = SheetSpec::preset("A2").unwrap();
    assert_eq!(sheet_of(doc, pages[0]), a2, "a schematic page takes its type's sheet");
    assert_eq!(sheet_of(doc, pages[1]).text(), "A1", "its own sheet wins");
    assert_eq!(sheet_of(doc, pages[2]), a3, "a cover is a Cover sheet page");
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
    use ecad_model::{GenKind, PageKind, Word};
    assert_eq!(derived_page_type(PageKind::Schematic), "schematic");
    for k in ["cover", "revisions", "contents"] {
        assert_eq!(derived_page_type(PageKind::generated(GenKind::parse(k).unwrap())), "front", "{k}");
    }
    for k in ["bom-location", "terminals", "cables", "plcrack", "iolist"] {
        assert_eq!(derived_page_type(PageKind::generated(GenKind::parse(k).unwrap())), "report", "{k}");
    }
    let ledger = Ledger::new(1);
    let doc = ledger.document();
    let mut page = ecad_model::Page::titled("p").unwrap();
    page.kind = PageKind::generated(GenKind::IoList);
    page.page_type = Some(StyleRef::Own(0xff));
    assert_eq!(page_type_of(doc, &page), StyleRef::Builtin("report".into()), "a type that does not exist reads as absent");
    page.page_type = StyleRef::builtin("panel");
    assert_eq!(page_type_of(doc, &page), StyleRef::Builtin("panel".into()));
    page.page_type = None;
    assert_eq!(page_type_of(doc, &page), StyleRef::Builtin("report".into()));
}
