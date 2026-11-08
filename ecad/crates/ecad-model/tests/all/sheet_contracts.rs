//! Brief 019: page and project properties (FP1), sheet settings (FP2), folders and their
//! invariants I19–I21 (FP3), book order and page numbers, and references on the frame's grid.
//! Table-driven where the cases share a shape.

use ecad_model::book::{Book, FOLDER_KEY, NUMBER_KEY};
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
    let ops = pages
        .iter()
        .enumerate()
        .map(|(i, p)| Op::AddPage { id: *p, page: Page::new(format!("{i}")), index: i })
        .collect();
    ledger.commit(Author::User, "pages", ops).unwrap();
    F { ids, ledger, pages }
}

fn folder(name: &str, parent: Option<FolderId>) -> Folder {
    Folder { name: name.into(), parent }
}

fn set_page(page: PageId, key: &str, before: Option<&str>, after: Option<&str>) -> Op {
    Op::SetProperty {
        object: ObjectRef::Page(page),
        key: key.into(),
        before: before.map(str::to_string),
        after: after.map(str::to_string),
    }
}

#[test]
fn page_and_project_properties_set_and_undo() {
    let mut f = fixture();
    let p = f.pages[0];
    f.ledger
        .commit(
            Author::User,
            "props",
            vec![
                set_page(p, NUMBER_KEY, None, Some("100")),
                Op::SetProperty {
                    object: ObjectRef::Project,
                    key: "title".into(),
                    before: None,
                    after: Some("P7".into()),
                },
            ],
        )
        .unwrap();
    let doc = f.ledger.document();
    assert_eq!(doc.pages[&p].properties.get(NUMBER_KEY).map(String::as_str), Some("100"));
    assert_eq!(doc.properties.get("title").map(String::as_str), Some("P7"));
    assert_eq!(Book::of(doc).number(doc, p), "100");
    assert_eq!(Book::of(doc).number(doc, f.pages[1]), "2", "unnumbered pages show their position");
    f.ledger.undo(Author::User).unwrap();
    assert!(f.ledger.document().properties.is_empty());
    assert!(f.ledger.document().pages[&p].properties.is_empty());
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
                set_page(p0, FOLDER_KEY, None, Some(&b.hex())),
                set_page(p1, FOLDER_KEY, None, Some(&a.hex())),
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
        ("a page names a missing folder", |f, a, _| vec![set_page(f.pages[0], FOLDER_KEY, None, Some(&a.hex()))]),
        ("a page's folder is not an ID", |f, _, _| vec![set_page(f.pages[0], FOLDER_KEY, None, Some("nope"))]),
        ("a parent is missing", |_, a, b| vec![Op::AddFolder { id: a, folder: folder("A", Some(b)), index: 0 }]),
        ("a folder inside itself", |_, a, b| {
            vec![
                Op::AddFolder { id: a, folder: folder("A", Some(b)), index: 0 },
                Op::AddFolder { id: b, folder: folder("B", Some(a)), index: 1 },
            ]
        }),
        ("removing a folder a page is in", |f, a, _| {
            vec![
                Op::AddFolder { id: a, folder: folder("A", None), index: 0 },
                set_page(f.pages[0], FOLDER_KEY, None, Some(&a.hex())),
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
    f.ledger.commit(Author::User, "sheet", vec![set_page(p, ecad_model::sheet::SHEET_KEY, None, Some("A2"))]).unwrap();
    let doc = f.ledger.document();
    assert_eq!(ecad_model::sheet::sheet_of(doc, p).columns, 12);
    assert_eq!(ecad_model::sheet::sheet_of(doc, f.pages[1]).columns, 8);
    let g = ecad_model::sheet::Grid::of(doc, f.pages[1]);
    let place = ecad_model::derive::place_on(&g, "105", ecad_model::Pt::new(120.0, 85.0)).unwrap();
    assert_eq!(place.text(ecad_model::ReferenceStyle::PageColumnRow), "105-C-4");
    assert_eq!(place.text(ecad_model::ReferenceStyle::PageColumn), "/105.C");
}
