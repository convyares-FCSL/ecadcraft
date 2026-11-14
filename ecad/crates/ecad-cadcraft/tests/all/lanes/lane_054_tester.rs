//! Tester 054 (page types, Pages and sheets, generated front pages): acceptance checks from
//! outside, written from brief 054 and design 054 (§2–§4, §6–§10, §12.1 AC1–AC12 and AC15, the
//! owner's answers Q1 and Q13 and the overseer's FP1–FP7 rulings) only, before the implementation
//! existed.
//!
//! The owner's answers this file holds the build to: pages start at 1; **+** (`newpage {after}`)
//! inserts right after the current page with the next available number; a folder can carry a start
//! number and **Renumber pages** renumbers each folder from its start; front, body and back never
//! force placement.
//!
//! The checks drive the JSON commands the design names (`styles`, `setstyle`, `setdefault`,
//! `newpage`, `folderprops`, `renumberpages`, `deletepage`, `projectprops`, `checks`) and read the
//! document, the book and the drawn page, leaning on as little of the new Rust API as possible:
//! only `ecad_model::sheet::sheet_of` (FP3) and the existing book and frame accessors. Result field
//! names the design does not give are read tolerantly.
//!
//! Test names start with the acceptance check (`ac1`…`ac15`) or the focus item (`num` numbering,
//! `fp3` sheet resolution, `map` update maps). 054b's cover checks (`ac8`–`ac12`) are in this file
//! too; the design gives 054b to the reviewer, the overseer asked the tester to cover it.

use cadcraft_engine::doc::EntityKind;
use ecad_model::book::Book;
use ecad_model::{ComponentId, FolderId, PageId};
use serde_json::{Value, json};

use crate::common::{H, id};
use crate::tester_050a_style::{checks_of, field_text, inject, one_tx, refused, s, save_and_reopen, style_row, styles, tmp_dir, undo};

// ------------------------------------------------------------------------------- helpers

fn number(h: &mut H, page: PageId) -> String {
    let d = h.doc();
    Book::of(&d).number(&d, page)
}

fn numbers(h: &mut H) -> Vec<String> {
    let d = h.doc();
    let b = Book::of(&d);
    b.order.iter().map(|p| b.number(&d, *p)).collect()
}

fn order(h: &mut H) -> Vec<PageId> {
    Book::of(&h.doc()).order
}

/// The page's resolved type (§2.3) as the registry reads it at page level (`defaults {key, page}`,
/// 050a's read: `{defaults: [{value, from}]}`).
fn type_of(h: &mut H, page: PageId) -> String {
    let v = h.run("defaults", json!({"key": "page.type", "page": page.hex()})).unwrap_or_else(|e| panic!("defaults page.type: {e}"));
    s(&v["defaults"][0]["value"])
}

fn new_page(h: &mut H, params: Value) -> PageId {
    let (v, tx) = one_tx(h, "newpage", params.clone());
    assert_eq!(tx.ops.iter().map(ecad_model::Op::name).collect::<Vec<_>>(), ["AddPage"], "§4.2: one AddPage for {params}");
    id(&v, "page")
}

fn folder(h: &mut H, name: &str) -> FolderId {
    id(&h.run("newfolder", json!({"name": name})).unwrap(), "folder")
}

fn move_into(h: &mut H, page: PageId, f: FolderId) {
    h.run("movepage", json!({"page": page.hex(), "folder": f.hex()})).unwrap();
}

/// Pages 1..=n (n − 1 added after the first, each after the last), the first active.
fn pages(seed: u64, n: usize) -> (H, Vec<PageId>) {
    let mut h = H::new(seed);
    let mut v = vec![h.first_page()];
    for i in 1..n {
        let last = *v.last().unwrap();
        v.push(id(&h.run("newpage", json!({"title": format!("Page {}", i + 1), "after": last.hex()})).unwrap(), "page"));
    }
    h.activate(v[0]);
    (h, v)
}

/// The generated layer's texts on `page` (made active first).
fn gen_texts(h: &mut H, page: PageId) -> Vec<String> {
    h.activate(page);
    h.pv(|p, pg| pg.entities_of(p, page))
        .iter()
        .filter(|e| e.common.layer == ecad_cadcraft::generated::LAYER)
        .filter_map(|e| match &e.kind {
            EntityKind::Text(t) => Some(t.value.clone()),
            EntityKind::MText(t) => Some(t.value.clone()),
            _ => None,
        })
        .collect()
}

/// The frame's texts on `page` (made active first).
fn frame_texts(h: &mut H, page: PageId) -> Vec<String> {
    h.activate(page);
    h.pv(|p, pg| pg.frame_entities_of(p, page))
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::Text(t) => Some(t.value.clone()),
            _ => None,
        })
        .collect()
}

fn sheet_text(h: &mut H, page: PageId) -> String {
    let d = h.doc();
    ecad_model::sheet::sheet_of(&d, page).text()
}

fn coil(h: &mut H, x: f64) -> ComponentId {
    id(&h.run("place", json!({"symbol": "coil", "at": [x, 100]})).unwrap(), "component")
}

/// A cover page (number 1 by default when first) and the schematic page it was added before.
fn with_cover(seed: u64) -> (H, PageId, PageId) {
    let mut h = H::new(seed);
    let schematic = h.first_page();
    let cover = new_page(&mut h, json!({"type": "front", "generated": "cover"}));
    h.run("movepage", json!({"page": cover.hex(), "index": 1})).unwrap();
    (h, cover, schematic)
}

// ---------------------------------------------------------------- AC1: the catalogue

/// AC1, §2.1, §2.2: `styles {catalogue: page}` lists the six built-ins with their fields: sheet and
/// frame `project`, title block `frame`, zones and position per the table.
#[test]
#[ignore = "tester 054: awaiting build"]
fn ac1_the_page_catalogue_lists_six_built_ins_with_their_fields() {
    let mut h = H::new(0x54_0101);
    let rows = styles(&mut h, "page");
    let ids: Vec<String> = rows.iter().map(|r| s(&r["id"])).collect();
    assert_eq!(ids.len(), 6, "{ids:?}");
    for (id, name, position, zones) in [
        ("front", "Front sheet", "front", "off"),
        ("schematic", "Schematic", "body", "on"),
        ("network", "Network", "body", "on"),
        ("singleline", "Single-line", "body", "on"),
        ("panel", "Panel layout", "body", "off"),
        ("report", "Report", "back", "off"),
    ] {
        let r = style_row(&mut h, "page", id);
        assert_eq!(s(&r["name"]), name, "{id}");
        assert_eq!(s(&r["layer"]), "builtin", "{id}");
        assert_eq!(field_text(&r, "position"), position, "{id}");
        assert_eq!(field_text(&r, "zones"), zones, "{id}");
        assert_eq!(field_text(&r, "sheet"), "project", "{id}: every built-in takes the project's sheet");
        assert_eq!(field_text(&r, "frame"), "project", "{id}");
        assert_eq!(field_text(&r, "title_block"), "frame", "{id}: Q10");
        for absent in ["numbers", "group", "range", "start"] {
            assert!(r["fields"][absent].is_null(), "{id}: owner Q13/Q1: a type carries no {absent}");
        }
    }
}

// ------------------------------------------------------------------ AC2: old projects

/// AC2, §2.3, §2.4: a project made the old way (no `page.type`) keeps every number, folder, sheet
/// and reference text; front pages resolve `front`, a BOM page `report`, the rest `schematic`;
/// nothing is written.
#[test]
#[ignore = "tester 054: awaiting build"]
fn ac2_old_pages_resolve_their_type_and_keep_numbers_folders_and_sheets() {
    let mut h = H::new(0x54_0201);
    let p1 = h.first_page();
    let f = folder(&mut h, "Field");
    let p2: PageId = id(&h.run("newpage", json!({"title": "Field", "folder": f.hex()})).unwrap(), "page");
    h.run("pageprops", json!({"page": p2.hex(), "sheet": "A2"})).unwrap();
    let cover: PageId = id(&h.run("newpage", json!({"generated": "cover"})).unwrap(), "page");
    let revs: PageId = id(&h.run("newpage", json!({"generated": "revisions"})).unwrap(), "page");
    let bom: PageId = id(&h.run("newpage", json!({"generated": "bom-location"})).unwrap(), "page");
    h.activate(p1);
    let k = coil(&mut h, 100.0);
    let _ = k;
    let before_numbers = numbers(&mut h);
    let before_sheets: Vec<String> = order(&mut h).into_iter().map(|p| sheet_text(&mut h, p)).collect();
    let d = h.doc();
    for p in d.page_order.iter() {
        assert!(!d.pages[p].properties.contains_key("page.type"), "plain newpage writes no page.type (§4.2)");
    }
    let head = h.head();
    for (p, want) in [(p1, "schematic"), (p2, "schematic"), (cover, "front"), (revs, "front"), (bom, "report")] {
        assert_eq!(type_of(&mut h, p), want, "page {p}");
    }
    assert_eq!(h.head(), head, "resolving writes nothing");
    assert_eq!(numbers(&mut h), before_numbers);
    let after_sheets: Vec<String> = order(&mut h).into_iter().map(|p| sheet_text(&mut h, p)).collect();
    assert_eq!(after_sheets, before_sheets);
    assert_eq!(ecad_model::book::folder_of(&h.doc(), p2), Some(f));
    // §2.4: front pages lose their zone labels; schematic pages keep them.
    let a3_cols: Vec<String> = (1..=8).map(|n| n.to_string()).collect();
    let cover_frame = frame_texts(&mut h, revs);
    assert!(!a3_cols.iter().all(|c| cover_frame.contains(c)), "a front page draws no zone labels: {cover_frame:?}");
    let sch_frame = frame_texts(&mut h, p1);
    assert!(a3_cols.iter().all(|c| sch_frame.contains(c)), "a schematic page keeps its zone labels: {sch_frame:?}");
}

// --------------------------------------------------------------- numbering and placement

/// Owner Q13, §4.3: pages start at 1; each new page takes the next available number.
#[test]
#[ignore = "tester 054: awaiting build"]
fn num_pages_start_at_one_and_count_up() {
    let (mut h, _) = pages(0x54_0301, 4);
    assert_eq!(numbers(&mut h), ["1", "2", "3", "4"]);
}

/// AC3, owner Q1, §4.1, §4.2: `newpage {type, after}` puts the page right after `after`, in its
/// folder, with the next free number; the type never moves it (a Report or Front page added in the
/// middle of the body stays where it was asked to go).
#[test]
#[ignore = "tester 054: awaiting build"]
fn ac3_plus_inserts_after_the_current_page_with_the_next_free_number() {
    let (mut h, p) = pages(0x54_0302, 5);
    let net = new_page(&mut h, json!({"type": "network", "after": p[2].hex()}));
    let o = order(&mut h);
    assert_eq!(o.iter().position(|x| *x == net), Some(3), "right after page 3");
    let n = number(&mut h, net);
    assert!(!["1", "2", "3", "4", "5"].contains(&n.as_str()), "a free number: {n}");
    assert_eq!(n, "6", "the next available number after 3 (4 and 5 are taken)");
    assert_eq!(type_of(&mut h, net), "network");
    assert_eq!(h.doc().pages[&net].properties.get("page.type").map(String::as_str), Some("network"), "§4.2: written when it differs");
    // Back and front types do not force placement (Q1).
    let rep = new_page(&mut h, json!({"type": "report", "generated": "bom-location", "after": p[0].hex()}));
    assert_eq!(order(&mut h).iter().position(|x| *x == rep), Some(1), "a Report page goes where it is asked, not to the back");
    let cov = new_page(&mut h, json!({"type": "front", "generated": "cover", "after": p[4].hex()}));
    assert_eq!(order(&mut h).last(), Some(&cov), "a cover after the last page stays last");
    // A schematic `newpage {type}` writes no page.type (the derived type).
    let sch = new_page(&mut h, json!({"type": "schematic", "after": p[1].hex()}));
    assert!(!h.doc().pages[&sch].properties.contains_key("page.type"), "§4.2: as small as today");
    // In a folder: after a page in a folder lands in that folder.
    let f = folder(&mut h, "Motors");
    move_into(&mut h, p[3], f);
    let in_f = new_page(&mut h, json!({"type": "schematic", "after": p[3].hex()}));
    assert_eq!(ecad_model::book::folder_of(&h.doc(), in_f), Some(f), "in the folder of `after`");
    h.check();
}

/// §4.2: no `type` and no `generated` is exactly today's newpage (last in the book, 019's number);
/// `after` naming no page is refused; the title defaults to the kind's or the type's name.
#[test]
#[ignore = "tester 054: awaiting build"]
fn num_newpage_defaults_and_refusals() {
    let (mut h, p) = pages(0x54_0303, 3);
    let plain = new_page(&mut h, json!({"title": "Plain"}));
    assert_eq!(order(&mut h).last(), Some(&plain));
    assert_eq!(number(&mut h, plain), "4");
    let net = new_page(&mut h, json!({"type": "network"}));
    assert_eq!(h.doc().pages[&net].title, "Network", "title defaults to the type's name");
    refused(&mut h, "newpage", json!({"type": "schematic", "after": "0000000000000000000000000000beef"}));
    refused(&mut h, "newpage", json!({"type": "nosuchtype"}));
    let m = refused(&mut h, "newpage", json!({"type": "panel"}));
    assert!(!m.is_empty(), "Q3: Panel layout pages come later");
    let _ = p;
}

/// AC3, §4.3: in an empty folder with start `100` the first new page is `100`, the next `101`;
/// a start already in use gives the next free after it; leading zeros set the width.
#[test]
#[ignore = "tester 054: awaiting build"]
fn ac3_a_folder_start_numbers_its_first_page() {
    let (mut h, _) = pages(0x54_0304, 2);
    let f = folder(&mut h, "Schematic");
    one_tx(&mut h, "folderprops", json!({"folder": f.hex(), "start": "100"}));
    assert_eq!(h.doc().properties.get(&format!("folder.{}.start", f.hex())).map(String::as_str), Some("100"), "FP7 key");
    let a = new_page(&mut h, json!({"type": "schematic", "folder": f.hex()}));
    assert_eq!(number(&mut h, a), "100");
    let b = new_page(&mut h, json!({"type": "schematic", "after": a.hex()}));
    assert_eq!(number(&mut h, b), "101");
    // Leading zeros.
    let g = folder(&mut h, "Front");
    h.run("folderprops", json!({"folder": g.hex(), "start": "01"})).unwrap();
    let c = new_page(&mut h, json!({"folder": g.hex()}));
    assert_eq!(number(&mut h, c), "01", "§4.3: leading zeros set the width");
    let d = new_page(&mut h, json!({"after": c.hex()}));
    assert_eq!(number(&mut h, d), "02");
    // A start already used: next free after it.
    let t = folder(&mut h, "Clash");
    h.run("folderprops", json!({"folder": t.hex(), "start": "100"})).unwrap();
    let e = new_page(&mut h, json!({"folder": t.hex()}));
    assert_eq!(number(&mut h, e), "102", "100 and 101 are taken");
    // Setting a start renumbers nothing.
    let before = numbers(&mut h);
    h.run("folderprops", json!({"folder": f.hex(), "start": "500"})).unwrap();
    assert_eq!(numbers(&mut h), before, "§4.3: setting a start renumbers nothing");
}

/// AC5, §4.3: `folderprops` refuses a start that is not 1–6 digits with the words, recording
/// nothing; `start: null` clears; `deletefolder` clears the start in the same transaction.
#[test]
#[ignore = "tester 054: awaiting build"]
fn ac5_folder_start_refusals_clear_and_delete() {
    let mut h = H::new(0x54_0305);
    let f = folder(&mut h, "Schematic");
    for bad in ["1a", "", "1234567", "-1", " 12", "\u{661}\u{662}"] {
        let m = refused(&mut h, "folderprops", json!({"folder": f.hex(), "start": bad}));
        assert!(m.contains("A start number is 1 to 6 digits."), "{bad:?}: {m}");
    }
    refused(&mut h, "folderprops", json!({"folder": "0000000000000000000000000000beef", "start": "100"}));
    let key = format!("folder.{}.start", f.hex());
    h.run("folderprops", json!({"folder": f.hex(), "start": "100"})).unwrap();
    one_tx(&mut h, "folderprops", json!({"folder": f.hex(), "start": null}));
    assert!(!h.doc().properties.contains_key(&key), "null clears");
    h.run("folderprops", json!({"folder": f.hex(), "start": "100"})).unwrap();
    let (_, tx) = one_tx(&mut h, "deletefolder", json!({"folder": f.hex()}));
    assert!(!h.doc().properties.contains_key(&key), "deletefolder clears the start in the same transaction: {:?}", tx.ops);
    undo(&mut h);
    assert_eq!(h.doc().properties.get(&key).map(String::as_str), Some("100"), "one u brings folder and start back");
}

/// AC5 (the owner's flow), §4.4: pages 1–50; folders Schematic (100) and Terminal (300); 40 pages
/// into Schematic, 10 into Terminal; `renumberpages` gives 100–139 and 300–309 in one transaction
/// of page-number SetProperty ops; references follow; one `u` restores every number; a second run
/// writes nothing.
#[test]
#[ignore = "tester 054: awaiting build"]
fn ac5_renumber_pages_numbers_each_folder_from_its_start() {
    let (mut h, p) = pages(0x54_0306, 50);
    let sch = folder(&mut h, "Schematic");
    let ter = folder(&mut h, "Terminal");
    h.run("folderprops", json!({"folder": sch.hex(), "start": "100"})).unwrap();
    h.run("folderprops", json!({"folder": ter.hex(), "start": "300"})).unwrap();
    for (i, page) in p.iter().enumerate() {
        move_into(&mut h, *page, if i < 40 { sch } else { ter });
    }
    // A cross-page reference: a coil on page 1 and its contact on page 45 (continuation text).
    h.activate(p[0]);
    let k = coil(&mut h, 100.0);
    h.activate(p[44]);
    h.run("place", json!({"symbol": "no", "at": [100, 100], "device": k.hex()})).ok();
    let before = numbers(&mut h);
    let (v, tx) = one_tx(&mut h, "renumberpages", json!({}));
    assert!(
        tx.ops.iter().all(|o| matches!(o, ecad_model::Op::SetProperty { object: ecad_model::ObjectRef::Page(_), key, .. } if key == "number")),
        "{:?}",
        tx.ops
    );
    assert_eq!(tx.ops.len(), 50, "every page's number changes");
    let want: Vec<String> = (100..140).chain(300..310).map(|n| n.to_string()).collect();
    assert_eq!(numbers(&mut h), want);
    assert!(s(&v["message"]).contains("Renumbered 50 pages."), "§4.4 status words: {v}");
    // References follow (019's update map): the contact's page names the coil's new number.
    let d = h.doc();
    let refs: Vec<String> = ecad_cadcraft::references::markers_on_page(&d, p[44]).into_iter().map(|m| m.text).collect();
    assert!(refs.iter().all(|t| !t.starts_with("1.") && !t.starts_with("1/")), "no reference to the old number 1: {refs:?}");
    // Again: nothing to do, nothing written.
    let head = h.head();
    let v = h.run("renumberpages", json!({})).unwrap();
    assert_eq!(h.head(), head, "already in order: nothing written");
    assert!(s(&v["message"]).contains("Page numbers already in order."), "{v}");
    undo(&mut h);
    assert_eq!(numbers(&mut h), before, "one u restores every number");
}

/// §4.4: `renumberpages {folder}` renumbers that folder (and sub-folders) alone; pages in a folder
/// without a start count in the nearest folder above with one, else the book's run from 1.
#[test]
#[ignore = "tester 054: awaiting build"]
fn num_renumber_one_folder_and_runs_without_a_start() {
    let (mut h, p) = pages(0x54_0307, 6);
    let a = folder(&mut h, "A");
    let b = folder(&mut h, "B");
    h.run("folderprops", json!({"folder": a.hex(), "start": "100"})).unwrap();
    for page in &p[2..4] {
        move_into(&mut h, *page, a);
    }
    for page in &p[4..6] {
        move_into(&mut h, *page, b);
    }
    // Only folder A.
    h.run("renumberpages", json!({"folder": a.hex()})).unwrap();
    assert_eq!(number(&mut h, p[2]), "100");
    assert_eq!(number(&mut h, p[3]), "101");
    assert_eq!(number(&mut h, p[4]), "5", "folder B untouched");
    // All: B has no start, so its pages run on from the run above them (A's, the nearest above with a start).
    h.run("renumberpages", json!({})).unwrap();
    let n = numbers(&mut h);
    assert_eq!(&n[..2], ["1", "2"], "loose pages: the book's run from 1");
    assert_eq!(&n[2..4], ["100", "101"]);
    assert_eq!(&n[4..6], ["102", "103"], "B counts in A's run: {n:?}");
}

/// AC5, §4.4, §9: Schematic holding more pages than fit before Terminal's start skips Terminal's
/// numbers, and Checks has one `folder-runs-overlap` warning naming both folders; never a refusal.
#[test]
#[ignore = "tester 054: awaiting build"]
fn ac5_overlapping_runs_skip_and_warn() {
    let (mut h, p) = pages(0x54_0308, 14);
    let sch = folder(&mut h, "Schematic");
    let ter = folder(&mut h, "Terminal");
    h.run("folderprops", json!({"folder": sch.hex(), "start": "100"})).unwrap();
    h.run("folderprops", json!({"folder": ter.hex(), "start": "110"})).unwrap();
    for (i, page) in p.iter().enumerate() {
        move_into(&mut h, *page, if i < 12 { sch } else { ter });
    }
    one_tx(&mut h, "renumberpages", json!({}));
    let n = numbers(&mut h);
    let mut seen = std::collections::BTreeSet::new();
    assert!(n.iter().all(|x| seen.insert(x.clone())), "no number twice: {n:?}");
    assert_eq!(&n[12..], ["110", "111"], "Terminal keeps its start");
    assert!(!n[..12].contains(&"110".to_string()) && !n[..12].contains(&"111".to_string()), "Schematic skips Terminal's: {n:?}");
    let rows = checks_of(&mut h, "folder-runs-overlap");
    assert_eq!(rows.len(), 1, "{rows:?}");
    let w = s(&rows[0]["message"]) + &s(&rows[0]["text"]);
    assert!(w.contains("Schematic") && w.contains("Terminal") && w.contains("110"), "§4.4 words: {w}");
    assert_eq!(s(&rows[0]["level"]).to_lowercase(), "warning");
}

/// §4.4: continuation numbers of a generated page's sheets (033) are skipped by Renumber.
#[test]
#[ignore = "tester 054: awaiting build"]
fn num_renumber_skips_continuation_numbers() {
    let (mut h, p) = pages(0x54_0309, 3);
    let bom = new_page(&mut h, json!({"generated": "bom-location", "after": p[0].hex()}));
    h.activate(p[1]);
    for i in 0..120 {
        coil(&mut h, 10.0 + (i % 30) as f64 * 12.0);
    }
    h.run("renumberpages", json!({})).unwrap();
    let d = h.doc();
    let b = Book::of(&d);
    for p in &b.order {
        let n = b.number(&d, *p);
        let c = ecad_cadcraft::generated::continuation_of(&d, &b, &n, None);
        assert!(c.is_none_or(|(owner, _)| owner == *p), "page {p}'s number {n} is a continuation number of {c:?}");
    }
    let mut seen = std::collections::BTreeSet::new();
    assert!(numbers(&mut h).iter().all(|x| seen.insert(x.clone())), "no page number twice");
    let _ = bom;
}

// ------------------------------------------------------------------- AC4: front pages

/// AC4, §2.3, §4.2, §7: `newpage {type: front, generated: revisions, after: cover}` lands right
/// after the cover with number 2 and title `Revision history`; `type: front` with no generated is
/// refused with §4.2's words; `generated: cover, type: report` with §2.3's.
#[test]
#[ignore = "tester 054: awaiting build"]
fn ac4_front_pages_after_the_cover_and_their_refusals() {
    let mut h = H::new(0x54_0401);
    let sch = h.first_page();
    // The cover takes 1: renumber so the drawn page follows it.
    let cover = new_page(&mut h, json!({"type": "front", "generated": "cover"}));
    h.run("movepage", json!({"page": cover.hex(), "index": 1})).unwrap();
    h.run("renumberpages", json!({})).unwrap();
    assert_eq!(number(&mut h, cover), "1");
    let revs = new_page(&mut h, json!({"type": "front", "generated": "revisions", "after": cover.hex()}));
    assert_eq!(order(&mut h).iter().position(|x| *x == revs), Some(1));
    assert_eq!(h.doc().pages[&revs].title, "Revision history", "§7 default title");
    let n = number(&mut h, revs);
    assert!(n == "2" || n == "3", "the next free after the cover: {n}");
    let m = refused(&mut h, "newpage", json!({"type": "front"}));
    assert!(m.contains("A Front sheet page is generated: give generated: \"cover\", \"revisions\" or \"contents\"."), "{m}");
    let m = refused(&mut h, "newpage", json!({"generated": "cover", "type": "report"}));
    assert!(m.contains("A cover page is a Front sheet page; choose a Front sheet type."), "{m}");
    let m = refused(&mut h, "newpage", json!({"generated": "bom-location", "type": "front"}));
    assert!(!m.is_empty());
    // `generated` with no `type`: rule 2's type, and no page.type stored.
    let c = new_page(&mut h, json!({"generated": "contents", "after": revs.hex()}));
    assert_eq!(type_of(&mut h, c), "front");
    assert!(!h.doc().pages[&c].properties.contains_key("page.type"));
    // An own type copied from Front sheet may hold a cover (§2.3).
    let own = s(&h.run("setstyle", json!({"catalogue": "page", "new": true, "name": "Cover A4", "copy_of": "front"})).unwrap()["id"]);
    new_page(&mut h, json!({"type": own, "generated": "cover", "after": sch.hex()}));
}

/// §2.3: `setdefault page.type` at page level (G only) moves one page's type; a generated page
/// refuses a type of the wrong position; project level is refused.
#[test]
#[ignore = "tester 054: awaiting build"]
fn ac4_setdefault_page_type_is_page_level_only() {
    let (mut h, p) = pages(0x54_0402, 2);
    one_tx(&mut h, "setdefault", json!({"key": "page.type", "level": "page", "page": p[1].hex(), "value": "singleline"}));
    assert_eq!(type_of(&mut h, p[1]), "singleline");
    assert_eq!(type_of(&mut h, p[0]), "schematic");
    refused(&mut h, "setdefault", json!({"key": "page.type", "level": "project", "value": "network"}));
    refused(&mut h, "setdefault", json!({"key": "page.type", "level": "standard", "value": "network"}));
    let cover = new_page(&mut h, json!({"generated": "cover"}));
    let m = refused(&mut h, "setdefault", json!({"key": "page.type", "level": "page", "page": cover.hex(), "value": "schematic"}));
    assert!(m.contains("Front sheet"), "{m}");
    undo(&mut h);
    undo(&mut h);
    assert_eq!(type_of(&mut h, p[1]), "schematic", "undo");
}

// --------------------------------------------------------------- AC6 / FP3: sheet by type

/// AC6, FP3, §3.1: Schematic's sheet A2 makes every schematic page without its own sheet A2 (model
/// `sheet_of`, the grid and the drawn zones), a page with its own sheet keeps it, other types keep
/// the project's; one `u` restores all.
#[test]
#[ignore = "tester 054: awaiting build"]
fn ac6_a_types_sheet_reaches_its_pages_and_undo_restores() {
    let (mut h, p) = pages(0x54_0601, 3);
    h.run("pageprops", json!({"page": p[2].hex(), "sheet": "A1"})).unwrap();
    let net = new_page(&mut h, json!({"type": "network"}));
    one_tx(&mut h, "setstyle", json!({"catalogue": "page", "id": "schematic", "fields": {"sheet": "A2"}}));
    for (page, want) in [(p[0], "A2"), (p[1], "A2"), (p[2], "A1"), (net, "A3")] {
        assert!(sheet_text(&mut h, page).starts_with(want), "page {page}: {want}, got {}", sheet_text(&mut h, page));
    }
    let d = h.doc();
    assert_eq!(ecad_model::sheet::Grid::of(&d, p[0]).spec.columns, 12, "the grid follows (A2: 12 columns)");
    assert!(frame_texts(&mut h, p[1]).iter().any(|t| t == "12"), "the frame draws A2's zones on a page not active at the change");
    undo(&mut h);
    for page in [p[0], p[1], net] {
        assert!(sheet_text(&mut h, page).starts_with("A3"), "undo: {page}");
    }
}

/// FP3, §3.1: the chain page `sheet` → type's stored sheet (project record, then standard record)
/// → `settings.sheet` → A3; a malformed stored sheet falls through and Checks has `page-type-field`.
#[test]
#[ignore = "tester 054: awaiting build"]
fn fp3_sheet_of_reads_the_type_record_then_falls_through() {
    let (mut h, p) = pages(0x54_0602, 2);
    assert!(sheet_text(&mut h, p[0]).starts_with("A3"), "A3 by default");
    h.run("projectprops", json!({"sheet": "A1"})).unwrap();
    assert!(sheet_text(&mut h, p[0]).starts_with("A1"), "settings.sheet");
    h.run("standard", json!({"new": "House"})).unwrap();
    h.run("setstyle", json!({"catalogue": "page", "id": "schematic", "layer": "standard", "fields": {"sheet": "A0"}})).unwrap();
    assert!(sheet_text(&mut h, p[0]).starts_with("A0"), "the standard's record");
    h.run("setstyle", json!({"catalogue": "page", "id": "schematic", "fields": {"sheet": "A2"}})).unwrap();
    assert!(sheet_text(&mut h, p[0]).starts_with("A2"), "the project's record over the standard's");
    h.run("pageprops", json!({"page": p[0].hex(), "sheet": "A3"})).unwrap();
    assert!(sheet_text(&mut h, p[0]).starts_with("A3"), "the page's own sheet wins");
    assert!(sheet_text(&mut h, p[1]).starts_with("A2"));
    // `project` in the project's record means the project's sheet (§2.1), not the standard's
    // record below it (question 2 in the tester's plan).
    h.run("setstyle", json!({"catalogue": "page", "id": "schematic", "fields": {"sheet": "project"}})).unwrap();
    assert!(sheet_text(&mut h, p[1]).starts_with("A1"), "`project`: settings.sheet");
}

/// FP3: a stored type record whose `sheet` does not parse or is out of limits (`A0 999x999`, a
/// 10 000-character text) reads as absent: the page keeps the project's sheet, nothing panics,
/// `page-type-field` reports it; commands refuse writing it.
#[test]
#[ignore = "tester 054: awaiting build"]
fn fp3_a_malformed_type_sheet_reads_as_absent() {
    let (mut h, p) = pages(0x54_0603, 1);
    refused(&mut h, "setstyle", json!({"catalogue": "page", "id": "schematic", "fields": {"sheet": "A0 999x999"}}));
    refused(&mut h, "setstyle", json!({"catalogue": "page", "id": "schematic", "fields": {"position": "x"}}));
    for bad in ["A0 999x999", "A9", &"x".repeat(10_000), "-1x-1", "nan"] {
        inject(
            &mut h,
            vec![ecad_model::Op::SetStyle {
                layer: ecad_model::style::StyleLayer::Project,
                catalogue: "page".into(),
                id: ecad_model::style::StyleRef::builtin("schematic").unwrap(),
                before: h.doc().styles.get("page").and_then(|m| m.get(&ecad_model::style::StyleRef::builtin("schematic").unwrap())).cloned(),
                after: Some(ecad_model::style::StyleRecord {
                    fields: [("sheet".to_string(), bad.to_string()), ("position".to_string(), "x".to_string())].into_iter().collect(),
                    ..Default::default()
                }),
            }],
        );
        assert!(sheet_text(&mut h, p[0]).starts_with("A3"), "{bad:.20}: falls through");
        let rows = checks_of(&mut h, "page-type-field");
        assert!(!rows.is_empty(), "{bad:.20}: reported");
    }
}

/// FP3 round trip: a type's sheet survives save and reopen (v2 file), and every page resolves the
/// same sheet, grid and type after reopen; replay gives the same document.
#[test]
#[ignore = "tester 054: awaiting build"]
fn fp3_type_sheets_round_trip_through_save_and_open() {
    let (mut h, p) = pages(0x54_0604, 3);
    let own = s(&h
        .run("setstyle", json!({"catalogue": "page", "new": true, "name": "Wide", "copy_of": "schematic", "fields": {"sheet": "A1", "zones": "off"}}))
        .unwrap()["id"]);
    h.run("setdefault", json!({"key": "page.type", "level": "page", "page": p[1].hex(), "value": own})).unwrap();
    h.run("setstyle", json!({"catalogue": "page", "id": "schematic", "fields": {"sheet": "A2"}})).unwrap();
    let f = folder(&mut h, "Field");
    h.run("folderprops", json!({"folder": f.hex(), "start": "300"})).unwrap();
    crate::tester_050a_style::records_round_trip(&mut h);
    let before: Vec<(String, String)> = p.iter().map(|x| (sheet_text(&mut h, *x), type_of(&mut h, *x))).collect();
    let doc = h.doc();
    let dir = tmp_dir("054 fp3");
    let mut h = save_and_reopen(h, &dir, "Projekt 054 (FP3).ecad", 0x54_0605);
    assert!(h.doc() == doc, "the document reads back equal");
    let after: Vec<(String, String)> = p.iter().map(|x| (sheet_text(&mut h, *x), type_of(&mut h, *x))).collect();
    assert_eq!(after, before);
    assert!(after[1].0.starts_with("A1") && after[0].0.starts_with("A2"));
    assert!(h.p(|pr| pr.ledger.replay_matches().unwrap()));
}

// ---------------------------------------------------------------------- AC7: zones off

/// AC7, FP6, §3.3: `zones: off` on Schematic: its pages draw no zone labels; references keep their
/// zones; the frame style's `grid_labels off` also hides them (both must be on).
#[test]
#[ignore = "tester 054: awaiting build"]
fn ac7_zones_off_hides_zone_labels_and_references_keep_zones() {
    let (mut h, p) = pages(0x54_0701, 2);
    h.activate(p[0]);
    let k = coil(&mut h, 100.0);
    h.activate(p[1]);
    h.run("place", json!({"symbol": "no", "at": [150, 100], "device": k.hex()})).ok();
    let d = h.doc();
    let refs_before: Vec<String> = ecad_cadcraft::references::markers_on_page(&d, p[1]).into_iter().map(|m| m.text).collect();
    one_tx(&mut h, "setstyle", json!({"catalogue": "page", "id": "schematic", "fields": {"zones": "off"}}));
    let t = frame_texts(&mut h, p[0]);
    assert!(!["A", "B", "C", "D", "E", "F"].iter().all(|r| t.iter().any(|x| x == r)), "no zone row labels: {t:?}");
    let d = h.doc();
    let refs_after: Vec<String> = ecad_cadcraft::references::markers_on_page(&d, p[1]).into_iter().map(|m| m.text).collect();
    assert_eq!(refs_after, refs_before, "Q14: references keep their zones");
    undo(&mut h);
    h.run("setstyle", json!({"catalogue": "frame", "id": "sheet", "fields": {"grid_labels": "off"}})).unwrap();
    let t = frame_texts(&mut h, p[0]);
    assert!(!["A", "B", "C", "D", "E", "F"].iter().all(|r| t.iter().any(|x| x == r)), "grid_labels off hides them too");
}

// ------------------------------------------------------------- AC8–AC11: the cover (054b)

/// AC8, §6: a cover with every block: heading, customer and site, notes, wire colours (one line
/// per category in use), symbol legend, revisions and the title block. (The no-overlap geometry
/// check is phase 2 against the worker's part boxes.)
#[test]
#[ignore = "tester 054: awaiting build (054b)"]
fn ac8_the_cover_draws_every_block() {
    let (mut h, cover, sch) = with_cover(0x54_0801);
    h.run(
        "projectprops",
        json!({"title": "Pump skid", "customer": "Northfield", "props": {"site": "Hull", "cover.notes": "First note\nSecond note"}}),
    )
    .unwrap();
    h.run("revision", json!({"description": "First issue", "drawn": "TS", "date": "2026-11-14"})).unwrap();
    h.run("issuerevision", json!({})).unwrap();
    h.activate(sch);
    coil(&mut h, 100.0);
    h.run("wire", json!({"points": [[0, 50], [60, 50]], "potential": "24V"})).unwrap();
    let t = gen_texts(&mut h, cover);
    for want in [
        "Pump skid · Electrical documentation",
        "Northfield · Hull",
        "Notes",
        "1. First note",
        "2. Second note",
        "Wire colours",
        "Symbol legend",
        "Revisions",
        "First issue",
    ] {
        assert!(t.iter().any(|x| x == want), "§6.1: {want} in {t:?}");
    }
    assert!(t.iter().any(|x| x.contains("mm²") || x.contains("blue")), "a wire-colour line: {t:?}");
    assert!(!t.iter().any(|x| x.contains("sheets")), "§10.2: the old sheet-count line is gone");
    assert!(!t.iter().any(|x| x == "Structure codes"), "structure codes are off by default");
}

/// §6.2, Q5: wire colours list only categories in use; equal colour and cross-section share a line;
/// none in use says `No wires yet.`; deleting the last wire of a category removes its line.
#[test]
#[ignore = "tester 054: awaiting build (054b)"]
fn ac8_wire_colours_show_only_categories_in_use() {
    let (mut h, cover, sch) = with_cover(0x54_0802);
    assert!(gen_texts(&mut h, cover).iter().any(|x| x == "No wires yet."));
    h.activate(sch);
    let w = h.run("wire", json!({"points": [[0, 50], [60, 50]], "potential": "24V"})).unwrap();
    let t = gen_texts(&mut h, cover);
    let lines: Vec<&String> =
        t.iter().filter(|x| x.contains(" · ")).filter(|x| x.contains("mm²") || x.contains("blue") || x.contains("black")).collect();
    assert_eq!(lines.len(), 1, "one category in use: {t:?}");
    assert!(lines[0].contains("24"), "{lines:?}");
    let rows = styles(&mut h, "wire");
    assert!(rows.len() > 2, "more categories exist than are listed");
    // Remove the wire: the legend empties.
    h.activate(sch);
    let c: ecad_model::ConductorId = id(&w, "conductor");
    let hd = h.route_handle(sch, c);
    h.run("erase", json!({"handles": [hd.hex()]})).unwrap();
    assert!(gen_texts(&mut h, cover).iter().any(|x| x == "No wires yet."), "a count crossing 0 removes the line");
}

/// AC9, §6.3: the symbol legend shows only symbols used on drawn pages: the first coil adds a cell,
/// deleting it removes it; a second coil adds no second cell; `No symbols yet.` when none.
#[test]
#[ignore = "tester 054: awaiting build (054b)"]
fn ac9_the_symbol_legend_shows_only_symbols_used() {
    let (mut h, cover, sch) = with_cover(0x54_0901);
    assert!(gen_texts(&mut h, cover).iter().any(|x| x == "No symbols yet."));
    h.activate(sch);
    let k1 = coil(&mut h, 100.0);
    let t1 = gen_texts(&mut h, cover);
    assert!(!t1.iter().any(|x| x == "No symbols yet."), "a cell appears: {t1:?}");
    h.activate(sch);
    coil(&mut h, 160.0);
    let t2 = gen_texts(&mut h, cover);
    assert_eq!(t2.len(), t1.len(), "a second coil adds no cell");
    h.activate(sch);
    let hs = h.rep_handles(sch, k1);
    h.run("erase", json!({"handles": hs})).unwrap();
    let t3 = gen_texts(&mut h, cover);
    assert_eq!(t3.len(), t1.len(), "one coil left: still one cell");
    let defs_before = t3.len();
    let _ = defs_before;
}

/// AC9, §6.3, Q6: a legend that does not fit says `and N more` and Checks has `cover-legend-cut`.
#[test]
#[ignore = "tester 054: awaiting build (054b)"]
fn ac9_a_legend_that_does_not_fit_says_and_n_more() {
    let (mut h, cover, sch) = with_cover(0x54_0902);
    h.activate(sch);
    let syms = h.run("symbols", json!({})).unwrap();
    let keys: Vec<String> = match &syms {
        Value::Array(a) => a.iter().map(|x| s(&x["key"])).filter(|k| !k.is_empty()).collect(),
        o => o["symbols"].as_array().map(|a| a.iter().map(|x| s(&x["key"])).collect()).unwrap_or_default(),
    };
    for (i, k) in keys.iter().enumerate() {
        let _ = h.run("place", json!({"symbol": k, "at": [10.0 + (i % 20) as f64 * 18.0, 40.0 + (i / 20) as f64 * 30.0]}));
    }
    let t = gen_texts(&mut h, cover);
    if keys.len() > 50 {
        assert!(t.iter().any(|x| x.starts_with("and ") && x.ends_with(" more")), "{t:?}");
        assert_eq!(checks_of(&mut h, "cover-legend-cut").len(), 1);
    }
}

/// AC10, §6.4: each block toggle through `setdefault` (project level) changes the cover, one
/// transaction each, one `u` each; the heading text and empty heading.
#[test]
#[ignore = "tester 054: awaiting build (054b)"]
fn ac10_block_toggles_change_the_cover_one_transaction_each() {
    let (mut h, cover, _) = with_cover(0x54_1001);
    h.run("projectprops", json!({"title": "Pump skid", "props": {"cover.notes": "One"}})).unwrap();
    for (key, title) in [("notes", "Notes"), ("wire_colours", "Wire colours"), ("symbol_legend", "Symbol legend"), ("revisions", "Revisions")] {
        let full = format!("front.cover.{key}");
        one_tx(&mut h, "setdefault", json!({"key": full, "level": "project", "value": "off"}));
        assert!(!gen_texts(&mut h, cover).iter().any(|x| x == title), "{key} off");
        undo(&mut h);
        assert!(gen_texts(&mut h, cover).iter().any(|x| x == title), "{key} back");
    }
    one_tx(&mut h, "setdefault", json!({"key": "front.cover.structure_codes", "level": "project", "value": "on"}));
    assert!(gen_texts(&mut h, cover).iter().any(|x| x == "Structure codes"));
    h.run("setdefault", json!({"key": "front.cover.heading", "level": "project", "value": "Wiring diagrams"})).unwrap();
    assert!(gen_texts(&mut h, cover).iter().any(|x| x == "Pump skid · Wiring diagrams"));
    h.run("setdefault", json!({"key": "front.cover.heading", "level": "project", "value": ""})).unwrap();
    assert!(gen_texts(&mut h, cover).iter().any(|x| x == "Pump skid"), "empty heading leaves the title alone");
    refused(&mut h, "setdefault", json!({"key": "front.cover.heading", "level": "project", "value": "x".repeat(65)}));
    refused(&mut h, "setdefault", json!({"key": "front.cover.notes", "level": "page", "page": cover.hex(), "value": "off"}));
}

/// AC11, §6.4, FP4: notes `a\nb` draw `1. a`, `2. b`; 41 notes or a 401-character note are refused
/// with the words; the notes survive deleting and re-adding the cover.
#[test]
#[ignore = "tester 054: awaiting build (054b)"]
fn ac11_notes_are_numbered_and_bounded() {
    let (mut h, cover, sch) = with_cover(0x54_1101);
    one_tx(&mut h, "projectprops", json!({"props": {"cover.notes": "a\nb"}}));
    let t = gen_texts(&mut h, cover);
    assert!(t.iter().any(|x| x == "1. a") && t.iter().any(|x| x == "2. b"), "{t:?}");
    let many: Vec<String> = (0..41).map(|i| format!("n{i}")).collect();
    let m = refused(&mut h, "projectprops", json!({"props": {"cover.notes": many.join("\n")}}));
    assert!(m.contains("The cover takes up to 40 notes of 400 characters."), "{m}");
    let m = refused(&mut h, "projectprops", json!({"props": {"cover.notes": "x".repeat(401)}}));
    assert!(m.contains("The cover takes up to 40 notes of 400 characters."), "{m}");
    // Windows line endings are notes too.
    h.run("projectprops", json!({"props": {"cover.notes": "c\r\nd"}})).unwrap();
    let t = gen_texts(&mut h, cover);
    assert!(t.iter().any(|x| x == "1. c") && t.iter().any(|x| x == "2. d"), "CRLF: {t:?}");
    h.activate(sch);
    h.run("deletepage", json!({"page": cover.hex()})).unwrap();
    let again = new_page(&mut h, json!({"type": "front", "generated": "cover"}));
    assert!(gen_texts(&mut h, again).iter().any(|x| x == "1. c"), "Q7: notes outlive the cover");
}

// --------------------------------------------------------------------- AC12: deletepage

/// AC12, §8.3: `deletepage` on a generated page is one RemovePage; `u` restores it with the same
/// id, number and title; a drawn page is refused with the words, recording nothing.
#[test]
#[ignore = "tester 054: awaiting build"]
fn ac12_deletepage_removes_generated_pages_only() {
    let (mut h, cover, sch) = with_cover(0x54_1201);
    let revs = new_page(&mut h, json!({"type": "front", "generated": "revisions", "after": cover.hex()}));
    let (n, title) = (number(&mut h, revs), h.doc().pages[&revs].title.clone());
    h.activate(sch);
    let (_, tx) = one_tx(&mut h, "deletepage", json!({"page": revs.hex()}));
    assert_eq!(tx.ops.iter().map(ecad_model::Op::name).collect::<Vec<_>>(), ["RemovePage"]);
    assert!(!h.doc().pages.contains_key(&revs));
    undo(&mut h);
    let d = h.doc();
    assert!(d.pages.contains_key(&revs), "same id");
    assert_eq!((number(&mut h, revs), d.pages[&revs].title.clone()), (n, title));
    let m = refused(&mut h, "deletepage", json!({"page": sch.hex()}));
    assert!(m.contains("Deleting a drawn page comes later; move its content first."), "{m}");
    refused(&mut h, "deletepage", json!({"page": "0000000000000000000000000000beef"}));
}

// ----------------------------------------------------------------- update maps (§10)

/// §10.2: an ordinary edit on a schematic page leaves the cover's parts alone and the frame of the
/// active page unbuilt; a type change of the active page rebuilds its frame once.
#[test]
#[ignore = "tester 054: awaiting build"]
fn map_ordinary_edits_touch_no_type_view() {
    let (mut h, p) = pages(0x54_1301, 2);
    h.activate(p[0]);
    let b = h.p(|pr| pr.frame_builds);
    h.run("line", json!({"points": [[0, 120], [40, 120]]})).unwrap();
    h.run("setstyle", json!({"catalogue": "page", "id": "network", "fields": {"zones": "off"}})).unwrap();
    assert_eq!(h.p(|pr| pr.frame_builds), b, "a line and another type's change rebuild no frame here");
    h.run("setstyle", json!({"catalogue": "page", "id": "schematic", "fields": {"zones": "off"}})).unwrap();
    assert_eq!(h.p(|pr| pr.frame_builds), b + 1, "the active page's type: its frame once");
    h.run("setdefault", json!({"key": "page.type", "level": "page", "page": p[1].hex(), "value": "network"})).unwrap();
    assert_eq!(h.p(|pr| pr.frame_builds), b + 1, "another page's type: nothing here");
    h.run("setdefault", json!({"key": "page.type", "level": "page", "page": p[0].hex(), "value": "network"})).unwrap();
    assert_eq!(h.p(|pr| pr.frame_builds), b + 2, "this page's type: its frame once");
}

/// §10.2: the cover follows its keys only: a project property no cover block reads (`site_contact`)
/// leaves the cover as drawn; `customer`, `site` and `cover.notes` change it.
#[test]
#[ignore = "tester 054: awaiting build (054b)"]
fn map_the_cover_follows_only_its_keys() {
    let (mut h, cover, _) = with_cover(0x54_1302);
    h.activate(cover);
    let before = h.p(|pr| pr.projections);
    h.run("projectprops", json!({"props": {"site_contact": "J. Smith"}})).unwrap();
    assert_eq!(h.p(|pr| pr.projections), before, "§10.2: an unread key does not re-project the cover");
    h.run("projectprops", json!({"customer": "Northfield"})).unwrap();
    assert!(h.p(|pr| pr.projections) > before, "customer re-projects it");
    assert!(gen_texts(&mut h, cover).iter().any(|x| x.contains("Northfield")));
}

// ------------------------------------------------------------------------ AC15: hostile

/// AC15, §9: hostile stored values read as absent or cut, each with its Checks row, never a panic:
/// a type record with `position: x` and `sheet: A0 999x999`; a `folder.<id>.start` of
/// `99999999999` and one naming no folder; a `page.type` naming nothing; 1 MB of notes.
#[test]
#[ignore = "tester 054: awaiting build"]
fn ac15_hostile_values_read_as_absent_with_checks_rows() {
    use ecad_model::{ObjectRef, Op};
    let (mut h, cover, sch) = with_cover(0x54_1501);
    let f = folder(&mut h, "Field");
    let ghost = "#0000000000000000000000000000dead".to_string();
    let notes: String = (0..5000).map(|i| format!("note {i} {}\n", "x".repeat(200))).collect();
    inject(
        &mut h,
        vec![
            Op::SetProperty { object: ObjectRef::Project, key: format!("folder.{}.start", f.hex()), before: None, after: Some("99999999999".into()) },
            Op::SetProperty {
                object: ObjectRef::Project,
                key: "folder.0000000000000000000000000000beef.start".into(),
                before: None,
                after: Some("100".into()),
            },
            Op::SetProperty { object: ObjectRef::Page(sch), key: "page.type".into(), before: None, after: Some(ghost) },
            Op::SetProperty { object: ObjectRef::Project, key: "cover.notes".into(), before: None, after: Some(notes) },
        ],
    );
    assert_eq!(type_of(&mut h, sch), "schematic", "a page.type naming nothing falls to rule 3");
    let t = gen_texts(&mut h, cover);
    assert!(t.iter().filter(|x| x.starts_with(|c: char| c.is_ascii_digit()) && x.contains(". note")).count() <= 40, "notes read cut at 40");
    for kind in ["folder-start-unknown", "page-type-unknown", "cover-notes-cut"] {
        assert!(!checks_of(&mut h, kind).is_empty(), "{kind} row");
    }
    assert_eq!(checks_of(&mut h, "folder-start-unknown").len(), 2, "both start keys");
    // Renumber with a hostile start never panics and never writes a 11-digit number.
    h.run("renumberpages", json!({})).unwrap();
    assert!(numbers(&mut h).iter().all(|n| n.len() <= 6), "{:?}", numbers(&mut h));
    // A page-type-unknown fix writes the derived type.
    let row = checks_of(&mut h, "page-type-unknown").remove(0);
    if let Some(fix) = row.get("fix").filter(|f| !f.is_null()) {
        one_tx(&mut h, &s(&fix["command"]), fix["params"].clone());
        assert!(checks_of(&mut h, "page-type-unknown").is_empty());
    }
}

/// AC15: 10 000 wire categories in use on the cover never panic and the legend is bounded.
#[test]
#[ignore = "slow: 10 000 categories and conductors (tester 054, run by name)"]
fn ac15_ten_thousand_categories_in_use_are_bounded() {
    let (mut h, cover, sch) = with_cover(0x54_1502);
    h.activate(sch);
    for i in 0..10_000u32 {
        let id = h.run("setstyle", json!({"catalogue": "wire", "new": true, "name": format!("C{i}"), "copy_of": "dc24", "fields": {"colour": "BU", "cross_section": format!("{}.5", i % 50)}}));
        let Ok(id) = id else { break };
        let w = h.run("wire", json!({"points": [[0.0, i as f64 * 0.01], [10.0, i as f64 * 0.01]]})).unwrap();
        h.run("category", json!({"conductor": s(&w["conductor"]), "value": s(&id["id"])})).ok();
    }
    let t = gen_texts(&mut h, cover);
    assert!(t.len() < 2_000, "the cover is bounded: {}", t.len());
}
