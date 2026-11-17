//! Brief 054a (design 054 §2–§4, §8.3, §9): page types, `newpage {type, after}`, folder start
//! numbers, Renumber pages, `deletepage`, and their Checks rows.

use ecad_cadcraft::checks::Kind;
use ecad_cadcraft::styles::page as ptype;
use ecad_model::book::{Book, NUMBER_KEY};
use ecad_model::sheet::{PAGE_TYPE_KEY, sheet_of};
use ecad_model::{FolderId, PageId, StyleRef};
use serde_json::{Value, json};

use crate::common::{H, id};

fn page(h: &mut H, params: Value) -> PageId {
    id(&h.run("newpage", params).unwrap(), "page")
}

fn number(h: &mut H, p: PageId) -> String {
    let d = h.doc();
    Book::of(&d).number(&d, p)
}

fn ty(h: &mut H, p: PageId) -> String {
    let d = h.doc();
    ptype::type_of(&d, p).text()
}

fn kinds(h: &mut H) -> Vec<(Kind, String)> {
    let d = h.doc();
    ecad_cadcraft::checks::all(&d, 0.01).into_iter().map(|c| (c.kind, c.what())).collect()
}

#[test]
fn the_six_built_in_types_are_listed_with_their_fields() {
    let mut h = H::new(5401);
    let s = h.run("styles", json!({"catalogue": "page"})).unwrap();
    let ids: Vec<String> = s["styles"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap_or_default().to_string()).collect();
    assert_eq!(ids, ["front", "schematic", "network", "singleline", "panel", "report", "plain"]);
    let d = h.doc();
    let look = |w: &str| ptype::look_of(&d, &StyleRef::Builtin(w.into()));
    assert_eq!(look("front").holds, ptype::Holds::Front);
    assert!(!look("front").zones && look("schematic").zones && !look("panel").zones && !look("report").zones);
    assert_eq!(look("report").holds, ptype::Holds::Report);
    assert_eq!((look("schematic").sheet, look("schematic").frame.clone(), look("schematic").title_block), (None, None, None));
    let names: Vec<String> = ptype::ordered(&d).into_iter().map(|r| r.name).collect();
    assert_eq!(names, ["Cover sheet", "Schematic", "Network", "Single-line", "Panel layout", "Report", "Plain"]);
}

#[test]
fn old_pages_resolve_their_type_without_a_property() {
    let mut h = H::new(5402);
    let first = h.first_page();
    let cover = page(&mut h, json!({"generated": "cover"}));
    let bom = page(&mut h, json!({"generated": "bom-location"}));
    let d = h.doc();
    for p in [first, cover, bom] {
        assert!(!d.pages[&p].properties.contains_key(PAGE_TYPE_KEY), "nothing written for the derived type");
    }
    assert_eq!((ty(&mut h, first), ty(&mut h, cover), ty(&mut h, bom)), ("schematic".into(), "front".into(), "report".into()));
    // The sheet is the project's, as before.
    let d = h.doc();
    assert_eq!(sheet_of(&d, first), d.settings.sheet());
}

#[test]
fn newpage_type_after_inserts_after_the_page_with_the_next_number() {
    let mut h = H::new(5403);
    let p1 = h.first_page();
    let p2 = page(&mut h, json!({"title": "two"}));
    let p3 = page(&mut h, json!({"title": "three"}));
    assert_eq!((number(&mut h, p1), number(&mut h, p2), number(&mut h, p3)), ("1".into(), "2".into(), "3".into()));
    // After page 2: right after it, number 4 (3 is taken), typed Network, titled after its type.
    let n = page(&mut h, json!({"type": "network", "after": p2.hex()}));
    let d = h.doc();
    assert_eq!(Book::of(&d).order, vec![p1, p2, n, p3], "the type never moves it");
    assert_eq!(number(&mut h, n), "4");
    assert_eq!(d.pages[&n].title, "Network");
    assert_eq!(d.pages[&n].properties.get(PAGE_TYPE_KEY).map(String::as_str), Some("network"));
    // A plain schematic page stays as small as today.
    let s = page(&mut h, json!({"type": "schematic", "after": p3.hex()}));
    assert!(!h.doc().pages[&s].properties.contains_key(PAGE_TYPE_KEY));
    // An empty folder with start 100: its first page is 100, the next 101.
    let f: FolderId = id(&h.run("newfolder", json!({"name": "Schematic"})).unwrap(), "folder");
    h.run("folderprops", json!({"folder": f.hex(), "start": "100"})).unwrap();
    let a = page(&mut h, json!({"type": "schematic", "folder": f.hex()}));
    let b = page(&mut h, json!({"type": "schematic", "after": a.hex()}));
    assert_eq!((number(&mut h, a), number(&mut h, b)), ("100".into(), "101".into()));
    // Leading zeros keep their width.
    let g: FolderId = id(&h.run("newfolder", json!({"name": "Front"})).unwrap(), "folder");
    h.run("folderprops", json!({"folder": g.hex(), "start": "01"})).unwrap();
    let c = page(&mut h, json!({"type": "front", "generated": "cover", "folder": g.hex()}));
    let r = page(&mut h, json!({"type": "front", "generated": "revisions", "after": c.hex()}));
    assert_eq!((number(&mut h, c), number(&mut h, r)), ("01".into(), "02".into()));
    assert_eq!(h.doc().pages[&r].title, ecad_cadcraft::generated::GenKind::Revisions.title());
    h.check();
}

#[test]
fn newpage_refuses_a_type_that_does_not_fit_the_kind() {
    let mut h = H::new(5404);
    let e = h.run("newpage", json!({"type": "front"})).unwrap_err();
    assert!(e.contains("A Cover sheet page is generated: give generated: \"cover\", \"revisions\" or \"contents\"."), "{e}");
    let e = h.run("newpage", json!({"generated": "cover", "type": "report"})).unwrap_err();
    assert!(e.contains("A cover page is a Cover sheet page; choose a Cover sheet type."), "{e}");
    let e = h.run("newpage", json!({"type": "nothing"})).unwrap_err();
    assert!(e.contains("No page type is called"), "{e}");
    let e = h.run("newpage", json!({"type": "schematic", "after": "00000000000000000000000000000abc"})).unwrap_err();
    assert!(e.contains("No such page to add after."), "{e}");
    // setdefault at page level keeps a generated page in a type that holds its kind.
    let cover = page(&mut h, json!({"generated": "cover"}));
    let e = h.run("setdefault", json!({"key": "page.type", "level": "page", "page": cover.hex(), "value": "report"})).unwrap_err();
    assert!(e.contains("choose a Cover sheet type"), "{e}");
    let first = h.first_page();
    h.run("setdefault", json!({"key": "page.type", "level": "page", "page": first.hex(), "value": "singleline"})).unwrap();
    assert_eq!(ty(&mut h, first), "singleline");
}

#[test]
fn a_type_sheet_reaches_its_pages_and_undo_restores_it() {
    let mut h = H::new(5405);
    let p1 = h.first_page();
    let own = page(&mut h, json!({"title": "own"}));
    h.run("pageprops", json!({"page": own.hex(), "sheet": "A1"})).unwrap();
    let cover = page(&mut h, json!({"generated": "cover"}));
    let head = h.head();
    h.run("setstyle", json!({"catalogue": "page", "id": "schematic", "fields": {"sheet": "A2"}})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    let d = h.doc();
    assert_eq!(sheet_of(&d, p1).text(), "A2", "a schematic page takes its type's sheet");
    assert_eq!(sheet_of(&d, own).text(), "A1", "a page's own sheet still wins");
    assert_eq!(sheet_of(&d, cover), d.settings.sheet(), "another type keeps the project's");
    h.run("u", Value::Null).unwrap();
    let d = h.doc();
    assert_eq!(sheet_of(&d, p1), d.settings.sheet());
    // `project` takes the project's sheet back.
    h.run("redo", Value::Null).unwrap();
    h.run("setstyle", json!({"catalogue": "page", "id": "schematic", "fields": {"sheet": "project"}})).unwrap();
    let d = h.doc();
    assert_eq!(sheet_of(&d, p1), d.settings.sheet());
    h.check();
}

#[test]
fn zones_off_draws_no_ticks_or_labels() {
    let mut h = H::new(5406);
    let p1 = h.first_page();
    let count = |h: &mut H| {
        let d = h.doc();
        ecad_cadcraft::generated::parts_of(&d, p1, None).len()
    };
    let with = count(&mut h);
    h.run("setstyle", json!({"catalogue": "page", "id": "schematic", "fields": {"zones": "off"}})).unwrap();
    let without = count(&mut h);
    // A3: 7 + 5 inner ticks top and bottom, left and right (24), and 8 + 6 labels on both sides (28).
    assert_eq!(with - without, 24 + 28, "{with} {without}");
    let d = h.doc();
    assert!(ecad_model::wiring::place(&d, p1, ecad_model::Pt::new(100.0, 100.0)).is_some(), "references keep their zones");
}

#[test]
fn renumber_pages_numbers_each_folder_from_its_start() {
    let mut h = H::new(5407);
    let mut pages = vec![h.first_page()];
    for i in 2..=10 {
        pages.push(page(&mut h, json!({"title": format!("p{i}")})));
    }
    let s: FolderId = id(&h.run("newfolder", json!({"name": "Schematic"})).unwrap(), "folder");
    let t: FolderId = id(&h.run("newfolder", json!({"name": "Terminal"})).unwrap(), "folder");
    h.run("folderprops", json!({"folder": s.hex(), "start": "100"})).unwrap();
    h.run("folderprops", json!({"folder": t.hex(), "start": "300"})).unwrap();
    for p in &pages[..6] {
        h.run("movepage", json!({"page": p.hex(), "folder": s.hex()})).unwrap();
    }
    for p in &pages[6..] {
        h.run("movepage", json!({"page": p.hex(), "folder": t.hex()})).unwrap();
    }
    let before: Vec<String> = pages.iter().map(|p| number(&mut h, *p)).collect();
    let head = h.head();
    let r = h.run("renumberpages", json!({})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    assert_eq!(r["renumbered"], 10);
    let after: Vec<String> = pages.iter().map(|p| number(&mut h, *p)).collect();
    assert_eq!(after, ["100", "101", "102", "103", "104", "105", "300", "301", "302", "303"]);
    assert_eq!(h.run("renumberpages", json!({})).unwrap()["message"], "Page numbers already in order.");
    assert_eq!(h.head(), head + 1, "nothing written the second time");
    h.run("u", Value::Null).unwrap();
    let undone: Vec<String> = pages.iter().map(|p| number(&mut h, *p)).collect();
    assert_eq!(undone, before, "one u restores every number");
    h.run("redo", Value::Null).unwrap();
    // Terminal starting at 103 meets Schematic's run: its pages skip, and Checks says so.
    h.run("folderprops", json!({"folder": t.hex(), "start": "103"})).unwrap();
    let rows = kinds(&mut h);
    assert!(rows.iter().any(|(k, w)| *k == Kind::FolderRunsOverlap && w == "Folder Schematic's numbers run into Terminal's (103)."), "{rows:?}");
    h.run("renumberpages", json!({})).unwrap();
    let after: Vec<String> = pages.iter().map(|p| number(&mut h, *p)).collect();
    assert_eq!(after, ["100", "101", "102", "107", "108", "109", "103", "104", "105", "106"]);
    // Scoped to one folder: the others keep their numbers.
    h.run("folderprops", json!({"folder": t.hex(), "start": "300"})).unwrap();
    h.run("renumberpages", json!({"folder": t.hex()})).unwrap();
    let after: Vec<String> = pages.iter().map(|p| number(&mut h, *p)).collect();
    assert_eq!(after, ["100", "101", "102", "107", "108", "109", "300", "301", "302", "303"]);
    h.check();
}

#[test]
fn folder_start_numbers_are_checked_and_go_with_their_folder() {
    let mut h = H::new(5408);
    let f: FolderId = id(&h.run("newfolder", json!({"name": "Terminal"})).unwrap(), "folder");
    for bad in [json!("1a"), json!("1234567")] {
        let e = h.run("folderprops", json!({"folder": f.hex(), "start": bad})).unwrap_err();
        assert!(e.contains("A start number is 1 to 6 digits."), "{e}");
    }
    // Brief 070: a number is not text, so the input is refused in the uniform wording.
    let e = h.run("folderprops", json!({"folder": f.hex(), "start": 12})).unwrap_err();
    assert_eq!(e, "`start` must be text, or null");
    h.run("folderprops", json!({"folder": f.hex(), "start": "300"})).unwrap();
    assert_eq!(h.run("folderprops", json!({"folder": f.hex()})).unwrap()["start"], "300");
    let key = ecad_cadcraft::page_types::start_key(f);
    let head = h.head();
    h.run("deletefolder", json!({"folder": f.hex()})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    assert!(!h.doc().properties.contains_key(&key), "deletefolder clears the start");
    h.run("u", Value::Null).unwrap();
    assert_eq!(h.doc().properties.get(&key).map(String::as_str), Some("300"));
    h.run("folderprops", json!({"folder": f.hex(), "start": null})).unwrap();
    assert!(!h.doc().properties.contains_key(&key));
}

#[test]
fn deletepage_removes_generated_pages_only_and_undo_restores_them() {
    let mut h = H::new(5409);
    let first = h.first_page();
    let rev = page(&mut h, json!({"generated": "revisions"}));
    let (title, num) = (h.doc().pages[&rev].title.clone(), number(&mut h, rev));
    let e = h.run("deletepage", json!({"page": first.hex()})).unwrap_err();
    assert!(e.contains(ecad_cadcraft::page_types::DRAWN_PAGE), "{e}");
    let head = h.head();
    let r = h.run("deletepage", json!({"page": rev.hex()})).unwrap();
    assert!(r["message"].as_str().unwrap_or_default().starts_with("Removed "), "{r}");
    assert_eq!(h.head(), head + 1);
    assert!(!h.doc().pages.contains_key(&rev));
    assert_eq!(h.p(|p| p.active_page()), Some(first), "the page shown moves to the page before it");
    h.run("u", Value::Null).unwrap();
    let d = h.doc();
    assert_eq!((d.pages[&rev].title.clone(), number(&mut h, rev)), (title, num), "same id, number and title");
    h.check();
}

#[test]
fn hostile_types_and_starts_read_as_absent_with_their_rows() {
    let mut h = H::new(5410);
    let p1 = h.first_page();
    // A page.type naming nothing reads as its derived type.
    h.run("pageprops", json!({"page": p1.hex()})).unwrap();
    h.cfg(|p| {
        let d = p.ledger.document();
        let before = d.pages[&p1].properties.get(PAGE_TYPE_KEY).cloned();
        p.ledger
            .commit(
                ecad_model::Author::User,
                "hostile",
                vec![
                    ecad_model::Op::SetProperty {
                        object: ecad_model::ObjectRef::Page(p1),
                        key: PAGE_TYPE_KEY.into(),
                        before,
                        after: Some("#000000000000000000000000000000ff".into()),
                    },
                    ecad_model::Op::SetProperty {
                        object: ecad_model::ObjectRef::Project,
                        key: "folder.0000000000000000000000000000beef.start".into(),
                        before: None,
                        after: Some("99999999999".into()),
                    },
                ],
            )
            .unwrap();
    });
    assert_eq!(ty(&mut h, p1), "schematic");
    h.run("setstyle", json!({"catalogue": "page", "id": "schematic", "fields": {"holds": "x", "sheet": "A0 999x999"}})).unwrap_err();
    // Written past the command's checks, as a hostile file would hold them.
    h.cfg(|p| {
        let mut r = ecad_model::StyleRecord::default();
        r.fields.insert("holds".into(), "x".into());
        r.fields.insert("sheet".into(), "A0 999x999".into());
        r.fields.insert("zones".into(), "maybe".into());
        p.ledger
            .commit(
                ecad_model::Author::User,
                "hostile",
                vec![ecad_model::Op::SetStyle {
                    layer: ecad_model::StyleLayer::Project,
                    catalogue: "page".into(),
                    id: StyleRef::Builtin("schematic".into()),
                    before: None,
                    after: Some(r),
                }],
            )
            .unwrap();
    });
    let d = h.doc();
    let look = ptype::look_of(&d, &StyleRef::Builtin("schematic".into()));
    assert_eq!((look.holds, look.zones, look.sheet), (ptype::Holds::None, true, None));
    assert_eq!(sheet_of(&d, p1), d.settings.sheet());
    let rows = kinds(&mut h);
    for k in [Kind::PageTypeUnknown, Kind::PageTypeField, Kind::FolderStartUnknown] {
        assert!(rows.iter().any(|(x, _)| *x == k), "{k:?}: {rows:?}");
    }
    assert!(h.run("renumberpages", json!({})).is_ok());
    let _ = NUMBER_KEY;
}

#[test]
fn cover_notes_are_written_one_per_line_and_bounded() {
    let mut h = H::new(5411);
    let head = h.head();
    h.run("projectprops", json!({"props": {"cover.notes": "a\n\n b \n"}})).unwrap();
    assert_eq!(h.head(), head + 1);
    assert_eq!(h.doc().properties.get("cover.notes").map(String::as_str), Some("a\nb"));
    let many: Vec<String> = (0..41).map(|i| format!("note {i}")).collect();
    let e = h.run("projectprops", json!({"props": {"cover.notes": many.join("\n")}})).unwrap_err();
    assert!(e.contains("The cover takes up to 40 notes of 400 characters."), "{e}");
    let e = h.run("projectprops", json!({"props": {"cover.notes": "x".repeat(401)}})).unwrap_err();
    assert!(e.contains("40 notes of 400 characters"), "{e}");
    assert_eq!(h.head(), head + 1, "nothing written");
}

// ---------------------------------------------------------------- 054b: the cover

fn cover_texts(h: &mut H, cover: PageId) -> Vec<String> {
    let d = h.doc();
    ecad_cadcraft::generated::parts_of(&d, cover, None)
        .into_iter()
        .filter_map(|p| match p.kind {
            cadcraft_engine::doc::EntityKind::Text(t) => Some(t.value),
            _ => None,
        })
        .collect()
}

#[test]
fn the_cover_draws_its_blocks_and_notes() {
    let mut h = H::new(5420);
    h.run("projectprops", json!({"title": "Pump station", "customer": "Northfield", "props": {"site": "Pump house 2", "cover.notes": "a\nb"}}))
        .unwrap();
    let cover = page(&mut h, json!({"generated": "cover"}));
    let t = cover_texts(&mut h, cover);
    for want in [
        "Pump station · Electrical documentation",
        "Northfield · Pump house 2",
        "Notes",
        "1. a",
        "2. b",
        "Wire colours",
        "Symbol legend",
        "Revisions",
    ] {
        assert!(t.iter().any(|x| x == want), "{want}: {t:?}");
    }
    assert!(t.iter().any(|x| x == "No symbols yet."), "{t:?}");
    assert!(!t.iter().any(|x| x == "Structure codes"), "off by default");
    // Toggles: one step each.
    let head = h.head();
    h.run("setdefault", json!({"key": "front.cover.notes", "level": "project", "value": "off"})).unwrap();
    h.run("setdefault", json!({"key": "front.cover.structure_codes", "level": "project", "value": "on"})).unwrap();
    assert_eq!(h.head(), head + 2);
    let t = cover_texts(&mut h, cover);
    assert!(!t.iter().any(|x| x == "Notes") && t.iter().any(|x| x == "Structure codes"), "{t:?}");
    h.run("setdefault", json!({"key": "front.cover.heading", "level": "project", "value": "Wiring"})).unwrap();
    assert!(cover_texts(&mut h, cover).iter().any(|x| x == "Pump station · Wiring"));
}

#[test]
fn the_symbol_legend_shows_symbols_used_and_redraws_only_at_zero() {
    let mut h = H::new(5421);
    let cover = page(&mut h, json!({"generated": "cover"}));
    let first = h.first_page();
    h.activate(first);
    h.run("place", json!({"symbol": "coil", "at": [100, 100]})).unwrap();
    let d = h.doc();
    let tx = d.clone();
    let _ = tx;
    let cells = ecad_cadcraft::front::legend_cells(&d);
    assert_eq!(cells.len(), 1, "{cells:?}");
    // The first copy of a definition changes the cover; a second does not.
    let last = |h: &mut H| h.p(|p| p.ledger.log().last().map(|t| t.ops.clone()).unwrap_or_default());
    let ops = last(&mut h);
    assert!(ecad_cadcraft::front::touches(&h.doc(), &ops));
    h.run("place", json!({"symbol": "coil", "at": [150, 100]})).unwrap();
    let ops = last(&mut h);
    assert!(!ecad_cadcraft::front::touches(&h.doc(), &ops), "a second coil leaves the cover");
    let name = cells.first().map(|c| c.name().to_string()).unwrap_or_default();
    assert!(cover_texts(&mut h, cover).contains(&name));
}

#[test]
fn wire_colour_lines_merge_equal_categories() {
    let l = ecad_cadcraft::front::WireLine {
        names: vec!["L1".into(), "L2".into(), "L3".into()],
        cross_section: "2.5".into(),
        colour: "BK".into(),
        label: None,
    };
    assert_eq!(l.text(), "L1, L2, L3 · 2.5 mm² · black");
    assert_eq!(ecad_cadcraft::front::colour_words("GNYE"), "green / yellow");
    assert_eq!(ecad_cadcraft::front::colour_words("orange"), "orange");
}

/// Review of #111, 1 and 2: several new copies of a definition in one transaction, and a
/// definition in use redrawn or renamed, change the cover's legend.
#[test]
fn the_legend_sees_several_new_copies_at_once_and_a_redrawn_definition() {
    use ecad_model::{ItemKind, Op};
    let mut h = H::new(5422);
    let first = h.first_page();
    page(&mut h, json!({"generated": "cover"}));
    h.activate(first);
    h.run("place", json!({"symbol": "lamp", "at": [100, 100]})).unwrap();
    h.run("place", json!({"symbol": "lamp", "at": [150, 100]})).unwrap();
    let d = h.doc();
    let copies: Vec<Op> =
        d.items.iter().filter(|(_, it)| matches!(it.kind, ItemKind::Symbol(_))).map(|(id, it)| Op::AddItem { id: *id, item: it.clone() }).collect();
    assert_eq!(copies.len(), 2);
    // Both copies in one transaction: the definition is new to the legend.
    assert!(ecad_cadcraft::front::touches(&d, &copies));
    // One of two copies alone is not.
    assert!(!ecad_cadcraft::front::touches(&d, &copies[..1]));
    // A definition in use redrawn or renamed (048's Edit symbol embeds it again).
    let (def_id, def) = d.definitions.iter().next().map(|(i, x)| (*i, x.clone())).unwrap();
    assert!(ecad_cadcraft::front::touches(&d, &[Op::EmbedDefinition { id: def_id, definition: def }]));
}

/// Design 054 §10.1: a second wire of a category in use leaves the cover; the first of a new
/// category redraws it.
#[test]
fn wire_categories_in_use_cross_zero_only() {
    let mut h = H::new(5423);
    let first = h.first_page();
    page(&mut h, json!({"generated": "cover"}));
    h.activate(first);
    let d0 = h.doc();
    let mut uses = ecad_cadcraft::front::Uses::build(&d0);
    let last = |h: &mut H| h.p(|p| p.ledger.log().last().map(|t| t.ops.clone()).unwrap_or_default());
    let w: ecad_model::ConductorId = id(&h.run("wire", json!({"points": [[0, 0], [60, 0]]})).unwrap(), "conductor");
    let ops = last(&mut h);
    uses.update(&h.doc(), &ops);
    h.run("category", json!({"conductor": w.hex(), "value": "dc24"})).unwrap();
    let ops = last(&mut h);
    assert!(uses.update(&h.doc(), &ops), "the first 24 V DC wire");
    let w2: ecad_model::ConductorId = id(&h.run("wire", json!({"points": [[0, 20], [60, 20]]})).unwrap(), "conductor");
    let ops = last(&mut h);
    uses.update(&h.doc(), &ops);
    h.run("category", json!({"conductor": w2.hex(), "value": "dc24"})).unwrap();
    let ops = last(&mut h);
    assert!(!uses.update(&h.doc(), &ops), "a second 24 V DC wire changes nothing on the cover");
}

/// Fix round (owner): the built-in Plain type has Checks and references off. Its pages get no
/// electrical Checks rows, no wire numbers and no cross-references; turning the switch on brings
/// them back.
#[test]
fn a_plain_page_gets_no_checks_numbering_or_references() {
    let mut h = H::new(5424);
    let first = h.first_page();
    // An open wire end gives a Checks row on a schematic page.
    h.run("wire", json!({"points": [[0, 0], [60, 0]]})).unwrap();
    let on_page = |h: &mut H, p: ecad_model::PageId| {
        let d = h.doc();
        ecad_cadcraft::checks::all(&d, 0.01).into_iter().filter(|c| c.page == Some(p)).count()
    };
    assert!(on_page(&mut h, first) > 0, "a schematic page is checked");
    let numbered = |h: &mut H| h.doc().conductors.values().any(|c| c.properties.contains_key(ecad_model::connectivity::WIRE_NUMBER));
    // A Plain page: drawn the same, checked and numbered not at all.
    let plain = page(&mut h, json!({"type": "plain"}));
    assert_eq!(ty(&mut h, plain), "plain");
    let d = h.doc();
    assert!(!ptype::look_of(&d, &StyleRef::Builtin("plain".into())).checks);
    assert!(!ecad_model::sheet::page_checks(&d, plain));
    let before_numbered = numbered(&mut h);
    h.run("wire", json!({"points": [[0, 40], [60, 40]]})).unwrap();
    assert_eq!(on_page(&mut h, plain), 0, "no Checks row on a Plain page");
    let d = h.doc();
    let on_plain: Vec<_> =
        d.conductors.iter().filter(|(c, _)| d.representations_of(ecad_model::Subject::Conductor(**c)).all(|(_, r)| r.page == plain)).collect();
    assert!(!on_plain.is_empty());
    assert!(on_plain.iter().all(|(_, c)| !c.properties.contains_key(ecad_model::connectivity::WIRE_NUMBER)), "no wire number on a Plain page");
    let _ = before_numbered;
    // A coil there is in no reference group: the device has no head.
    h.run("place", json!({"symbol": "coil", "at": [100, 100]})).unwrap();
    let d = h.doc();
    let coil = d.components.keys().copied().find(|c| d.representations_of(ecad_model::Subject::Component(*c)).any(|(_, r)| r.page == plain));
    if let Some(c) = coil {
        assert!(ecad_cadcraft::xref::device(&d, c).head.is_none(), "no references from a Plain page");
    }
    // The switch on: checks come back, in one step.
    let head = h.head();
    h.run("setstyle", json!({"catalogue": "page", "id": "plain", "fields": {"checks": "on"}})).unwrap();
    assert_eq!(h.head(), head + 1);
    assert!(on_page(&mut h, plain) > 0, "checked again");
    h.check();
}

/// Fix round (owner): the cover's table editor rows hide, reorder and relabel the generated
/// legend, as one bounded project property; a stale key is dropped.
#[test]
fn cover_rows_hide_reorder_and_relabel_the_generated_legend() {
    let mut h = H::new(5425);
    let first = h.first_page();
    let cover = page(&mut h, json!({"generated": "cover"}));
    h.activate(first);
    h.run("place", json!({"symbol": "coil", "at": [100, 100]})).unwrap();
    h.run("place", json!({"symbol": "lamp", "at": [150, 100]})).unwrap();
    let d = h.doc();
    let rows = ecad_cadcraft::front::legend_rows(&d);
    assert_eq!(rows.len(), 2);
    let (a, b) = (rows[0].1.clone(), rows[1].1.clone());
    // b first and relabelled, a hidden, plus a key no longer generated.
    let text = format!("{}\t1\tMy lamp\n{}\t0\t\nffff\t1\tgone", b.key, a.key);
    let head = h.head();
    h.run("projectprops", json!({"props": {"cover.symbol_legend": text}})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    let d = h.doc();
    let shown: Vec<String> = ecad_cadcraft::front::legend_rows(&d).into_iter().filter(|(_, r)| r.shown).map(|(_, r)| r.label).collect();
    assert_eq!(shown, ["My lamp"]);
    let texts: Vec<String> = ecad_cadcraft::generated::parts_of(&d, cover, None)
        .into_iter()
        .filter_map(|p| match p.kind {
            cadcraft_engine::doc::EntityKind::Text(t) => Some(t.value),
            _ => None,
        })
        .collect();
    assert!(texts.contains(&"My lamp".to_string()) && !texts.contains(&a.label), "{texts:?}");
    // Bounded: 65 rows or a 65-character label is refused.
    let many: Vec<String> = (0..65).map(|i| format!("k{i}\t1\t")).collect();
    assert!(h.run("projectprops", json!({"props": {"cover.symbol_legend": many.join("\n")}})).is_err());
    assert!(h.run("projectprops", json!({"props": {"cover.wire_colours": format!("bk/2.5\t1\t{}", "x".repeat(65))}})).is_err());
    h.run("u", Value::Null).unwrap();
    assert!(!h.doc().properties.contains_key("cover.symbol_legend"), "one u");
}

/// Design 054 §6.6 under 052 (FP8): the cover's structure codes show each stored entry key as its
/// code, never the key, and follow a rename of the entry.
#[test]
fn structure_codes_show_the_052_codes_and_follow_a_rename() {
    let mut h = H::new(5466);
    let cover = page(&mut h, json!({"generated": "cover"}));
    let first = h.first_page();
    h.activate(first);
    h.run("place", json!({"symbol": "coil", "at": [100, 100]})).unwrap();
    let c = h.doc().components.keys().copied().next().unwrap();
    h.run("setlocation", json!({"component": c.hex(), "value": "+CP1"})).unwrap();
    h.run("setdefault", json!({"key": "front.cover.structure_codes", "level": "project", "value": "on"})).unwrap();
    let key = h.doc().components.get(&c).and_then(|x| x.properties.get("location").cloned()).unwrap();
    assert_ne!(key, "+CP1", "052 stores the entry key");
    let t = cover_texts(&mut h, cover);
    assert!(t.iter().any(|x| x == "+CP1"), "{t:?}");
    assert!(!t.iter().any(|x| x.contains(&key)), "never the key: {t:?}");
    // A rename is one SetProperty on the list; the cover follows.
    h.run("designationedit", json!({"aspect": "location", "code": "+CP1", "to": "+CP2", "description": "Control panel"})).unwrap();
    let t = cover_texts(&mut h, cover);
    assert!(t.iter().any(|x| x == "+CP2 Control panel") && !t.iter().any(|x| x == "+CP1"), "{t:?}");
}
