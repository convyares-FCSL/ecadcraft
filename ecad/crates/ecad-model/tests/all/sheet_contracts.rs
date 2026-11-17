//! Brief 019: page and project properties (FP1), sheet settings (FP2), folders and their
//! invariants I19–I21 (FP3), book order and page numbers, and references on the frame's grid.
//! Table-driven where the cases share a shape.

use ecad_model::book::Book;
use ecad_model::{Author, Folder, FolderId, IdAllocator, Ledger, ModelError, ObjectRef, Op, Page, PageId};

struct F {
    ids: IdAllocator,
    ledger: Ledger,
    pages: Vec<PageId>,
}

fn fixture() -> F {
    let mut ids = IdAllocator::seeded(19);
    let mut ledger = Ledger::new(19);
    let pages: Vec<PageId> = (0..3).map(|_| ids.fresh()).collect();
    let ops = pages.iter().enumerate().map(|(i, p)| Op::AddPage { id: *p, page: Page::titled(format!("{i}")).unwrap(), index: i }).collect();
    ledger.commit(Author::User, "pages", ops).unwrap();
    F { ids, ledger, pages }
}

fn folder(name: &str, parent: Option<FolderId>) -> Folder {
    Folder { name: name.into(), parent }
}

/// A `ModifyPage` of `page` (as `f` holds it) with `change` applied.
fn modify(f: &F, page: PageId, change: impl FnOnce(&mut Page)) -> Op {
    let before = f.ledger.document().pages[&page].clone();
    let mut after = before.clone();
    change(&mut after);
    Op::ModifyPage { id: page, before, after }
}

#[test]
fn page_and_project_properties_set_and_undo() {
    let mut f = fixture();
    let p = f.pages[0];
    let before = f.ledger.document().pages[&p].clone();
    let after = ecad_model::Page { number: ecad_model::PageNo::parse("100"), ..before.clone() };
    f.ledger
        .commit(
            Author::User,
            "props",
            vec![
                Op::ModifyPage { id: p, before, after },
                Op::SetProperty { object: ObjectRef::Project, key: "title".into(), before: None, after: Some("P7".into()) },
            ],
        )
        .unwrap();
    let doc = f.ledger.document();
    assert_eq!(doc.pages[&p].number.as_ref().map(ecad_model::PageNo::as_str), Some("100"));
    assert_eq!(doc.properties.get("title").map(String::as_str), Some("P7"));
    assert_eq!(Book::of(doc).number(doc, p), "100");
    assert_eq!(Book::of(doc).number(doc, f.pages[1]), "2", "unnumbered pages show their position");
    f.ledger.undo(Author::User).unwrap();
    assert!(f.ledger.document().properties.is_empty());
    assert!(f.ledger.document().pages[&p].number.is_none());
    assert!(f.ledger.replay_matches().unwrap());
}

#[test]
fn folders_order_pages_first_then_sub_folders() {
    let mut f = fixture();
    let (a, b): (FolderId, FolderId) = (f.ids.fresh(), f.ids.fresh());
    let [p0, p1, p2] = [f.pages[0], f.pages[1], f.pages[2]];
    f.ledger
        .commit(
            Author::User,
            "folders",
            vec![
                Op::AddFolder { id: a, folder: folder("A", None), index: 0 },
                Op::AddFolder { id: b, folder: folder("B", Some(a)), index: 1 },
                modify(&f, p0, |p| p.folder = Some(b)),
                modify(&f, p1, |p| p.folder = Some(a)),
            ],
        )
        .unwrap();
    let doc = f.ledger.document();
    let book = Book::of(doc);
    // Top level: p2; then A: p1; then B: p0.
    assert_eq!(book.order, [p2, p1, p0]);
    assert_eq!(book.next(p1), Some(p0));
    assert_eq!(doc.pages_in_folder(b), [p0]);
    assert_eq!(doc.subfolders(a), [b]);
    doc.check().unwrap();
    doc.check_index().unwrap();
}

/// I19–I21: each refused transaction leaves the document as it was.
#[test]
fn folder_invariants_refuse_bad_states() {
    type Case = (&'static str, fn(&F, FolderId, FolderId) -> Vec<Op>);
    let cases: Vec<Case> = vec![
        ("a page names a missing folder", |f, a, _| vec![modify(f, f.pages[0], |p| p.folder = Some(a))]),
        ("a parent is missing", |_, a, b| vec![Op::AddFolder { id: a, folder: folder("A", Some(b)), index: 0 }]),
        ("a folder inside itself", |_, a, b| {
            vec![Op::AddFolder { id: a, folder: folder("A", Some(b)), index: 0 }, Op::AddFolder { id: b, folder: folder("B", Some(a)), index: 1 }]
        }),
        ("removing a folder a page is in", |f, a, _| {
            vec![
                Op::AddFolder { id: a, folder: folder("A", None), index: 0 },
                modify(f, f.pages[0], |p| p.folder = Some(a)),
                Op::RemoveFolder { id: a, folder: folder("A", None), index: 0 },
            ]
        }),
    ];
    for (what, ops) in cases {
        let mut f = fixture();
        let (a, b): (FolderId, FolderId) = (f.ids.fresh(), f.ids.fresh());
        let before = f.ledger.document().clone();
        let ops = ops(&f, a, b);
        let e = f.ledger.commit(Author::User, what, ops).unwrap_err();
        assert!(matches!(e, ModelError::Invariant(_)), "{what}: {e:?}");
        assert_eq!(f.ledger.document(), &before, "{what}");
    }
}

#[test]
fn folders_nest_at_most_eight_deep() {
    let mut f = fixture();
    let mut parent = None;
    let mut ops = Vec::new();
    for i in 0..8 {
        let id: FolderId = f.ids.fresh();
        ops.push(Op::AddFolder { id, folder: folder(&format!("L{i}"), parent), index: i });
        parent = Some(id);
    }
    f.ledger.commit(Author::User, "eight", ops).unwrap();
    let id: FolderId = f.ids.fresh();
    let e = f.ledger.commit(Author::User, "nine", vec![Op::AddFolder { id, folder: folder("L8", parent), index: 8 }]);
    assert!(matches!(e, Err(ModelError::Invariant(_))), "{e:?}");
}

#[test]
fn references_and_sheets_read_from_page_and_project() {
    let mut f = fixture();
    let p = f.pages[0];
    f.ledger.commit(Author::User, "sheet", vec![modify(&f, p, |pg| pg.sheet = ecad_model::SheetSpec::parse("A2").ok())]).unwrap();
    let doc = f.ledger.document();
    assert_eq!(ecad_model::sheet::sheet_of(doc, p).columns, 12);
    assert_eq!(ecad_model::sheet::sheet_of(doc, f.pages[1]).columns, 8);
    let g = ecad_model::sheet::Grid::of(doc, f.pages[1]);
    let place = ecad_model::derive::place_on(&g, "105", ecad_model::Pt::new(130.0, 120.0)).unwrap();
    // UI rev 3: rows lettered, columns numbered, and the zone's letter first in every style.
    assert_eq!(place.text(ecad_model::ReferenceStyle::PageZone), "105/D3");
    assert_eq!(place.text(ecad_model::ReferenceStyle::PageColumnRow), "105-D-3");
    assert_eq!(place.text(ecad_model::ReferenceStyle::PageColumn), "/105.3", "the column: the current path");
    assert_eq!(ecad_model::ReferenceStyle::default(), ecad_model::ReferenceStyle::PageZone, "2/B5 for a new project");
    // A grid lettered across the top (a project saved with `letters numbers`) reads as before.
    let mut old = ecad_model::sheet::SheetSpec::default();
    (old.column_labels, old.row_labels) = (ecad_model::GridLabels::Letters, ecad_model::GridLabels::Numbers);
    let g = ecad_model::sheet::Grid::new(old, ecad_model::LengthUnit::Millimetre);
    let place = ecad_model::derive::place_on(&g, "105", ecad_model::Pt::new(130.0, 120.0)).unwrap();
    assert_eq!(place.text(ecad_model::ReferenceStyle::PageColumnRow), "105-C-4");
    assert_eq!(place.text(ecad_model::ReferenceStyle::PageColumn), "/105.C");
    assert_eq!(place.text(ecad_model::ReferenceStyle::PageZone), "105/C4");
}
