//! Tester 054 in the model (FP3, design 054 §3.1 and §11): `ecad_model::sheet::sheet_of` reads the
//! page type's stored `sheet`: the page's own `sheet` → the type's project record → the type's
//! standard record (each only when it parses and is within limits) → `settings.sheet` → A3. The
//! type is the page's `page.type`, else derived from `generated` by the fixed six-word table
//! (`cover`, `revisions`, `contents` → `front`; every other kind → `report`), else `schematic`.
//! Old documents (no record) resolve exactly as before. Replay and the inverse of every op give the
//! same sheets. Written from brief and design 054 before the implementation.

use ecad_model::sheet::{SheetSpec, sheet_of};
use ecad_model::style::{StandardHeader, StyleLayer, StyleRecord, StyleRef};
use ecad_model::{Author, IdAllocator, Ledger, ObjectRef, Op, Page, PageId, StandardId};

fn rec(fields: &[(&str, &str)]) -> StyleRecord {
    StyleRecord { fields: fields.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), ..Default::default() }
}

fn set_type(layer: StyleLayer, id: StyleRef, before: Option<StyleRecord>, after: Option<StyleRecord>) -> Op {
    Op::SetStyle { layer, catalogue: "page".into(), id, before, after }
}

fn b(w: &str) -> StyleRef {
    StyleRef::Builtin(w.into())
}

fn set_page(page: PageId, key: &str, after: &str) -> Op {
    Op::SetProperty { object: ObjectRef::Page(page), key: key.into(), before: None, after: Some(after.into()) }
}

/// Pages: plain, cover, revisions, contents, BOM, terminals, and one with its own A1 sheet.
fn fixture() -> (Ledger, Vec<PageId>) {
    let mut ids = IdAllocator::seeded(54);
    let mut l = Ledger::new(54);
    let pages: Vec<PageId> = (0..7).map(|_| ids.fresh()).collect();
    let mut ops: Vec<Op> = pages.iter().enumerate().map(|(i, p)| Op::AddPage { id: *p, page: Page::new(format!("{i}")), index: i }).collect();
    for (i, g) in [(1, "cover"), (2, "revisions"), (3, "contents"), (4, "bom-location"), (5, "terminals")] {
        ops.push(set_page(pages[i], "generated", g));
    }
    ops.push(set_page(pages[6], "sheet", "A1"));
    l.commit(Author::User, "pages", ops).unwrap();
    (l, pages)
}

fn sheets(l: &Ledger, pages: &[PageId]) -> Vec<String> {
    pages.iter().map(|p| sheet_of(l.document(), *p).text()).collect()
}

/// FP3: with no record, every page resolves as before (own sheet, else settings, else A3).
#[test]
#[ignore = "tester 054: awaiting build"]
fn fp3_no_record_resolves_as_before() {
    let (l, p) = fixture();
    let s = sheets(&l, &p);
    assert!(s[..6].iter().all(|x| x.starts_with("A3")), "{s:?}");
    assert!(s[6].starts_with("A1"));
}

/// FP3: the derived type table: a Front record's sheet reaches cover, revisions and contents; a
/// Report record's reaches BOM and terminals; a Schematic record's reaches the plain page; the
/// page's own sheet wins over all.
#[test]
#[ignore = "tester 054: awaiting build"]
fn fp3_records_reach_their_derived_types_and_the_own_sheet_wins() {
    let (mut l, p) = fixture();
    l.commit(
        Author::User,
        "types",
        vec![
            set_type(StyleLayer::Project, b("front"), None, Some(rec(&[("sheet", "A2")]))),
            set_type(StyleLayer::Project, b("report"), None, Some(rec(&[("sheet", "A1")]))),
            set_type(StyleLayer::Project, b("schematic"), None, Some(rec(&[("sheet", "A0")]))),
        ],
    )
    .unwrap();
    let s = sheets(&l, &p);
    let want = ["A0", "A2", "A2", "A2", "A1", "A1", "A1"];
    for (i, w) in want.iter().enumerate() {
        assert!(s[i].starts_with(w), "page {i}: {w}, got {}", s[i]);
    }
    // The page's own sheet wins: page 6 is a schematic page with its own A1 (not A0).
    l.commit(Author::User, "own", vec![set_page(p[0], "sheet", "A3")]).unwrap();
    assert!(sheet_of(l.document(), p[0]).text().starts_with("A3"));
}

/// FP3: `page.type` on the page wins over the derived type; an own type's record is read; a
/// `page.type` naming no type falls back to the derived type.
#[test]
#[ignore = "tester 054: awaiting build"]
fn fp3_page_type_property_and_own_types() {
    let (mut l, p) = fixture();
    let own = StyleRef::Own(0x54);
    l.commit(
        Author::User,
        "own",
        vec![
            set_type(
                StyleLayer::Project,
                own.clone(),
                None,
                Some(StyleRecord { name: "Wide".into(), ..rec(&[("sheet", "A0"), ("position", "body")]) }),
            ),
            set_type(StyleLayer::Project, b("network"), None, Some(rec(&[("sheet", "A2")]))),
            set_page(p[0], "page.type", &own.text()),
            set_page(p[4], "page.type", "#0000000000000000000000000000dead"),
        ],
    )
    .unwrap();
    assert!(sheet_of(l.document(), p[0]).text().starts_with("A0"), "the own type's sheet");
    assert!(sheet_of(l.document(), p[4]).text().starts_with("A3"), "a page.type naming nothing: derived (report, no record)");
    l.commit(
        Author::User,
        "net",
        vec![Op::SetProperty { object: ObjectRef::Page(p[0]), key: "page.type".into(), before: Some(own.text()), after: Some("network".into()) }],
    )
    .unwrap();
    assert!(sheet_of(l.document(), p[0]).text().starts_with("A2"), "a built-in named by page.type");
}

/// FP3: the project record over the standard record; a malformed or out-of-limits stored sheet
/// (`A0 999x999`, `A9`, 10 000 characters, `project`) is skipped as if absent; then settings.sheet.
#[test]
#[ignore = "tester 054: awaiting build"]
fn fp3_project_then_standard_record_and_malformed_values_fall_through() {
    let (mut l, p) = fixture();
    let header = StandardHeader { id: StandardId(0x54), name: "House".into(), based_on: "iec".into(), origin: None };
    l.commit(
        Author::User,
        "std",
        vec![
            Op::SetStandard { before: None, after: Some(header) },
            set_type(StyleLayer::Standard, b("schematic"), None, Some(rec(&[("sheet", "A1")]))),
        ],
    )
    .unwrap();
    assert!(sheet_of(l.document(), p[0]).text().starts_with("A1"), "the standard's record");
    l.commit(Author::User, "proj", vec![set_type(StyleLayer::Project, b("schematic"), None, Some(rec(&[("sheet", "A2")])))]).unwrap();
    assert!(sheet_of(l.document(), p[0]).text().starts_with("A2"), "the project's record over the standard's");
    let long = "x".repeat(10_000);
    let mut before = Some(rec(&[("sheet", "A2")]));
    for bad in ["A0 999x999", "A9", long.as_str(), "", "1e308x1e308"] {
        let after = Some(rec(&[("sheet", bad)]));
        l.commit(Author::User, "bad", vec![set_type(StyleLayer::Project, b("schematic"), before.clone(), after.clone())]).unwrap();
        before = after;
        let got = sheet_of(l.document(), p[0]).text();
        assert!(got.starts_with("A1"), "{bad:.20}: skipped, the standard's A1 next; got {got}");
    }
    // A valid sheet in the record, never above the limits of 019.
    let s = sheet_of(l.document(), p[0]);
    assert!(s.check().is_ok() && SheetSpec::parse(&s.text()).is_ok());
}

/// FP3: undo, redo and replay give the same sheets; a record's inverse restores the old sheet.
#[test]
#[ignore = "tester 054: awaiting build"]
fn fp3_undo_redo_and_replay_give_the_same_sheets() {
    let (mut l, p) = fixture();
    let before = sheets(&l, &p);
    l.commit(Author::User, "front", vec![set_type(StyleLayer::Project, b("front"), None, Some(rec(&[("sheet", "A2")])))]).unwrap();
    let after = sheets(&l, &p);
    assert_ne!(after, before);
    l.undo(Author::User).unwrap();
    assert_eq!(sheets(&l, &p), before);
    l.redo(Author::User).unwrap();
    assert_eq!(sheets(&l, &p), after);
    let replayed = Ledger::replay(l.full_log().unwrap(), 54).unwrap();
    assert_eq!(sheets(&replayed, &p), after);
    assert!(replayed.document().check().is_ok());
}
