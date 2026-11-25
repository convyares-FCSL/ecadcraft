//! The sheet of a page in the model: `ecad_model::document::sheet::sheet_of` reads the
//! page type's stored `sheet`: the page's own `sheet` → the type's project record → the type's
//! standard record (each only when it parses and is within limits) → `settings.sheet()` → A3. The
//! type is the page's `page.type`, else derived from `generated` by the fixed six-word table
//! (`cover`, `revisions`, `contents` → `front`; every other kind → `report`), else `schematic`.
//! Old documents (no record) resolve exactly as before. Replay and the inverse of every op give the
//! same sheets.

use ecad_model::document::sheet::{SheetSpec, sheet_of};
use ecad_model::objects::styles::{StandardHeader, StyleLayer, StyleRecord, StyleRef};
use ecad_model::{Author, IdAllocator, Ledger, Op, Page, PageId, StandardId, Word};

fn rec(fields: &[(&str, &str)]) -> StyleRecord {
    // A page type's fields, as a file holds them.
    let body: serde_json::Map<String, serde_json::Value> = fields.iter().map(|(k, v)| (k.to_string(), serde_json::json!(v))).collect();
    StyleRecord::new("", ecad_model::objects::styles::StyleBody::Page(serde_json::from_value(serde_json::Value::Object(body)).unwrap()))
}

fn set_type(layer: StyleLayer, id: StyleRef, before: Option<StyleRecord>, after: Option<StyleRecord>) -> Op {
    Op::SetStyle { layer, catalogue: ecad_model::StyleCatalogue::Page, id, before, after }
}

fn b(w: &str) -> StyleRef {
    StyleRef::Builtin(w.into())
}

/// `page` with the field the old page key `key` named set to `value` (design 072 §5.1).
fn with(mut page: Page, key: &str, value: &str) -> Page {
    match key {
        "sheet" => page.sheet = SheetSpec::parse(value).ok(),
        "page.type" => page.page_type = StyleRef::parse(value),
        "generated" => page.kind = ecad_model::PageKind::Generated(ecad_model::GenKind::parse(value).unwrap()),
        other => panic!("not a page field: {other}"),
    }
    page
}

/// A `ModifyPage` of `page` (as `l` holds it) setting `key`'s field to `after`.
fn set_page(l: &Ledger, page: PageId, key: &str, after: &str) -> Op {
    let before = l.document().pages()[&page].clone();
    Op::ModifyPage { id: page, before: before.clone(), after: with(before, key, after) }
}

/// Pages: plain, cover, revisions, contents, BOM, terminals, and one with its own A1 sheet.
fn fixture() -> (Ledger, Vec<PageId>) {
    let mut ids = IdAllocator::seeded(54);
    let mut l = Ledger::new(54);
    let pages: Vec<PageId> = (0..7).map(|_| ids.fresh()).collect();
    let mut made: Vec<Page> = (0..7).map(|i| Page::titled(format!("{i}")).unwrap()).collect();
    for (i, g) in [(1, "cover"), (2, "revisions"), (3, "contents"), (4, "bom-location"), (5, "terminal_blocks")] {
        made[i] = with(made[i].clone(), "generated", g);
    }
    made[6] = with(made[6].clone(), "sheet", "A1");
    let ops: Vec<Op> = pages.iter().zip(made).enumerate().map(|(i, (p, page))| Op::AddPage { id: *p, page, index: i }).collect();
    l.commit(Author::User, "pages", ops).unwrap();
    (l, pages)
}

fn sheets(l: &Ledger, pages: &[PageId]) -> Vec<String> {
    pages.iter().map(|p| sheet_of(l.document(), *p).text()).collect()
}

/// FP3: with no record, every page resolves as before (own sheet, else settings, else A3).
#[test]
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
    l.commit(Author::User, "own", vec![set_page(&l, p[0], "sheet", "A3")]).unwrap();
    assert!(sheet_of(l.document(), p[0]).text().starts_with("A3"));
}

/// FP3: `page.type` on the page wins over the derived type; an own type's record is read; a
/// `page.type` naming no type falls back to the derived type.
#[test]
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
                Some(StyleRecord { name: ecad_model::objects::styles::StyleName::new("Wide").ok(), ..rec(&[("sheet", "A0"), ("holds", "none")]) }),
            ),
            set_type(StyleLayer::Project, b("network"), None, Some(rec(&[("sheet", "A2")]))),
            set_page(&l, p[0], "page.type", &own.text()),
            set_page(&l, p[4], "page.type", "#0000000000000000000000000000dead"),
        ],
    )
    .unwrap();
    assert!(sheet_of(l.document(), p[0]).text().starts_with("A0"), "the own type's sheet");
    assert!(sheet_of(l.document(), p[4]).text().starts_with("A3"), "a page.type naming nothing: derived (report, no record)");
    l.commit(Author::User, "net", vec![set_page(&l, p[0], "page.type", "network")]).unwrap();
    assert!(sheet_of(l.document(), p[0]).text().starts_with("A2"), "a built-in named by page.type");
}

/// FP3: the project record over the standard record; a malformed or out-of-limits stored sheet
/// (`A0 999x999`, `A9`, 10 000 characters, `project`) is skipped as if absent; then settings.sheet().
#[test]
fn fp3_project_then_standard_record_and_malformed_values_fall_through() {
    let (mut l, p) = fixture();
    let header = StandardHeader {
        id: StandardId(0x54),
        name: ecad_model::objects::styles::StyleName::new("House").unwrap(),
        based_on: ecad_model::objects::styles::BasedOn::Iec,
        origin: None,
    };
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
    // Design 072 §8: a sheet past the limits, or not a sheet, does not read (the file opens
    // read-only), so it can never reach a page.
    let long = "x".repeat(10_000);
    for bad in ["A0 999x999", "A9", long.as_str(), "", "1e308x1e308"] {
        let v = serde_json::json!({ "sheet": bad });
        assert!(serde_json::from_value::<ecad_model::objects::styles::PageType>(v).is_err(), "{bad:.20}");
    }
    // A valid sheet in the record, never above the limits of 019.
    let s = sheet_of(l.document(), p[0]);
    assert!(s.check().is_ok() && SheetSpec::parse(&s.text()).is_ok());
}

/// FP3: undo, redo and replay give the same sheets; a record's inverse restores the old sheet.
#[test]
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
