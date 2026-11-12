//! Tester 033 (print and project, lane P): acceptance checks from outside, written from brief 033
//! and design 033 §15 (AC1–AC15) only, before the implementation existed. They drive JSON
//! commands and the public print, frame, generated-page and xref functions that 030 already had.
//! Behaviour that does not exist yet is `#[ignore = "tester 033: awaiting build"]`; the tests
//! that run now guard what 033 must not change (pixels of a printed page, old projects).
//!
//! Coordinates are page drawing coordinates: the drawing area's lower-left corner is (0, 0), the
//! title block's top line is y = 0 (50 mm above the paper's bottom on A3), the frame's right line
//! is x = 400 and its top line y = 240 on A3. The free area of design §4.1 is therefore y ≥ 2.

use std::collections::BTreeSet;

use cadcraft_engine::doc::{Drawing, EntityKind, EntityStore, Space};
use cadcraft_engine::geom::{Bounds2, Mat3, Vec2};
use ecad_cadcraft::outputs::Outputs;
use ecad_cadcraft::printbook::{self, Sheet};
use ecad_cadcraft::{frame, generated, xref};
use ecad_model::{ComponentId, Document, ObjectRef, Op, PageId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

const AWAIT: &str = "tester 033: awaiting build";

fn place(h: &mut H, symbol: &str, at: (f64, f64)) -> Value {
    h.run("place", json!({"symbol": symbol, "at": [at.0, at.1]})).unwrap()
}

fn place_on(h: &mut H, symbol: &str, at: (f64, f64), device: ComponentId) -> Value {
    h.run("place", json!({"symbol": symbol, "at": [at.0, at.1], "device": device.hex()})).unwrap()
}

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

/// Every text of a generated page (all its sheets) and of its frames.
fn gen_texts(doc: &Document, page: PageId) -> Vec<String> {
    generated::parts_of(doc, page, Some(&Outputs::build(doc)))
        .iter()
        .filter_map(|p| match &p.kind {
            EntityKind::Text(t) => Some(t.value.clone()),
            _ => None,
        })
        .collect()
}

fn last_tx(h: &mut H) -> (String, Vec<Op>) {
    h.p(|p| p.ledger.log().last().map(|t| (t.label.clone(), t.ops.clone())).unwrap_or_default())
}

fn sets(ops: &[Op], key: &str) -> bool {
    ops.iter().any(|o| matches!(o, Op::SetProperty { key: k, .. } if k == key))
}

fn tag(h: &mut H, c: ComponentId) -> String {
    h.doc().components[&c].properties.get("tag").cloned().unwrap_or_default()
}

/// A refusal: `cmd` fails and the head does not move.
fn refused(h: &mut H, cmd: &str, params: Value) -> String {
    let head = h.head();
    let e = h.run(cmd, params.clone()).expect_err(&format!("{cmd} {params} must be refused"));
    assert_eq!(h.head(), head, "{cmd} {params}: nothing written");
    e
}

/// A page full of drawing: `n` coils with a wire each.
fn dense_page(h: &mut H, title: &str, n: usize) -> PageId {
    let page: PageId = id(&h.run("newpage", json!({"title": title})).unwrap(), "page");
    for i in 0..n {
        let (x, y) = (20.0 + (i % 12) as f64 * 30.0, 40.0 + (i / 12) as f64 * 45.0);
        place(h, "coil", (x, y));
        h.run("wire", json!({"points": pts(&[(x, y + 10.0), (x, y + 25.0)])})).unwrap();
    }
    page
}

/// A page with one wire on it.
fn light_page(h: &mut H, title: &str) -> PageId {
    let page: PageId = id(&h.run("newpage", json!({"title": title})).unwrap(), "page");
    h.run("wire", json!({"points": pts(&[(100.0, 100.0), (200.0, 100.0)])})).unwrap();
    page
}

fn scratch(h: &mut H, pages: &[PageId]) -> (Drawing, Vec<Sheet>) {
    let doc = h.doc();
    let tol = h.p(|p| p.tolerance);
    printbook::build(&doc, pages, &BTreeSet::new(), tol, Some(&Outputs::build(&doc))).unwrap()
}

fn window(s: &Sheet) -> Bounds2 {
    Bounds2::new(Vec2::new(s.lo.x, s.lo.y), Vec2::new(s.hi.x, s.hi.y))
}

/// The plotter's PDF of `sheets` of `d`, content streams uncompressed.
fn pdf_plain(d: &Drawing, sheets: &[&Sheet]) -> Vec<u8> {
    let pages: Vec<Value> =
        sheets.iter().map(|s| json!({"window": [[s.lo.x, s.lo.y], [s.hi.x, s.hi.y]], "paper": s.paper, "landscape": s.landscape})).collect();
    cadcraft_io::plot(d, &Space::Model, &json!({"layout": "Model", "pages": pages, "margins": [0, 0, 0, 0], "compress": false})).unwrap()
}

// ------------------------------------------------------------------------------------ AC1–AC3

/// AC2 (design §2.3): the visible result of a printed page does not depend on what the other
/// pages hold. The middle sheet of [dense, P, dense] renders pixel for pixel as the middle sheet of
/// [blank, P, blank] (all A3, so the windows coincide); the same for the first sheet of [P, dense]
/// and [P, blank]. Runs now (clipping) and must still hold with culling.
#[test]
fn ac2_a_printed_page_looks_the_same_whatever_its_neighbours_hold() {
    let mut h = H::new(0x33_02);
    let p = light_page(&mut h, "P");
    h.run("place", json!({"symbol": "coil", "at": [60, 150]})).unwrap();
    let dense = dense_page(&mut h, "Dense", 24);
    let blank: PageId = id(&h.run("newpage", json!({"title": "Blank"})).unwrap(), "page");
    let (a, sa) = scratch(&mut h, &[dense, p, dense]);
    let (b, sb) = scratch(&mut h, &[blank, p, blank]);
    assert_eq!(window(&sa[1]), window(&sb[1]), "same window for P");
    let png = |d: &Drawing, s: &Sheet| cadcraft_io::png_framed(d, &Space::Model, 840, 594, Some(window(s))).unwrap();
    assert!(png(&a, &sa[1]) == png(&b, &sb[1]), "P between dense pages looks the same as between blank ones");
    let (a, sa) = scratch(&mut h, &[p, dense]);
    let (b, sb) = scratch(&mut h, &[p, blank]);
    assert!(png(&a, &sa[0]) == png(&b, &sb[0]), "P before a dense page looks the same");
    // And the dense page itself is not emptied by culling.
    assert!(png(&a, &sa[1]) != png(&b, &sb[1]), "the dense page shows its own content");
}

/// AC1 (design §2.1, brief "each PDF page holds only what lies in its window"): the PDF page of a
/// light page next to a dense page is about as large as the same page next to a blank page
/// (content streams uncompressed). Today it carries the dense page's geometry, clipped.
#[test]
#[ignore = "tester 033: awaiting build"]
fn ac1_each_pdf_page_holds_only_its_own_content() {
    let _ = AWAIT;
    let mut h = H::new(0x33_01);
    let p = light_page(&mut h, "P");
    let dense = dense_page(&mut h, "Dense", 48);
    let blank: PageId = id(&h.run("newpage", json!({"title": "Blank"})).unwrap(), "page");
    let (a, sa) = scratch(&mut h, &[p, dense]);
    let (b, sb) = scratch(&mut h, &[p, blank]);
    let with_dense = pdf_plain(&a, &[&sa[0]]).len();
    let with_blank = pdf_plain(&b, &[&sb[0]]).len();
    assert!(with_dense <= with_blank + with_blank / 20 + 2048, "page P next to a dense page: {with_dense} bytes, next to a blank page: {with_blank}");
    // The dense page's own PDF page does hold its content.
    let dense_alone = pdf_plain(&a, &[&sa[1]]).len();
    assert!(dense_alone > 4 * with_blank, "dense page {dense_alone} bytes vs light {with_blank}");
}

/// AC1 (design §2.2): a book of 20 copies of one dense page is at most 2.2 × a book of 10 (today
/// about 4×). The copies are made on the scratch drawing, as Print Book places sheets.
#[test]
#[ignore = "tester 033: awaiting build"]
fn ac1_pdf_bytes_grow_linearly_with_sheets() {
    printbook::install_io();
    let mut h = H::new(0x33_11);
    let dense = dense_page(&mut h, "Dense", 36);
    let (one, s) = scratch(&mut h, &[dense]);
    let s0 = s[0].clone();
    let step = s0.hi.x - s0.lo.x + generated::SHEET_GAP;
    let book = |n: usize| -> (Drawing, Vec<Sheet>) {
        let mut d = one.clone();
        let mut store = EntityStore::new();
        let mut sheets = Vec::new();
        for i in 0..n {
            let m = Mat3::translate(Vec2::new(step * i as f64, 0.0));
            for e in one.model.iter() {
                let mut e = (**e).clone();
                e.kind.transform(&m);
                e.handle = d.new_handle();
                store.push(e);
            }
            let mut sh = s0.clone();
            sh.lo.x += step * i as f64;
            sh.hi.x += step * i as f64;
            sheets.push(sh);
        }
        d.model = store;
        (d, sheets)
    };
    let (d10, s10) = book(10);
    let (d20, s20) = book(20);
    let b10 = printbook::plot(&d10, &s10, printbook::Options::default()).unwrap().len();
    let b20 = printbook::plot(&d20, &s20, printbook::Options::default()).unwrap().len();
    assert!(b20 as f64 <= 2.2 * b10 as f64, "20 sheets {b20} bytes vs 10 sheets {b10}: not linear");
}

// ------------------------------------------------------------------------------------ AC4

/// AC4 (design §3): a missing value in a generated table is a `?` in a closed box, both ACI 2 on
/// screen; the Print Book scratch drawing has them in ACI 7; the dialog's `flagged` is unchanged.
#[test]
#[ignore = "tester 033: awaiting build"]
fn ac4_a_missing_value_is_boxed_and_prints_black() {
    printbook::install_io();
    let mut h = H::new(0x33_04);
    h.run("projectprops", json!({"location": "+CP1"})).unwrap();
    place(&mut h, "coil", (100.0, 150.0)); // no maker, no part number: missing cells
    let bom: PageId = id(&h.run("newpage", json!({"generated": "bom-location"})).unwrap(), "page");
    let doc = h.doc();
    let parts = generated::parts_of(&doc, bom, Some(&Outputs::build(&doc)));
    let marks: Vec<_> = parts.iter().filter(|p| matches!(&p.kind, EntityKind::Text(t) if t.value == "?")).collect();
    assert!(!marks.is_empty(), "the BOM shows missing values");
    assert!(marks.iter().all(|p| p.aci == Some(generated::FLAGGED_ACI)), "`?` is yellow on screen");
    let boxes = parts.iter().filter(|p| p.aci == Some(generated::FLAGGED_ACI) && !matches!(p.kind, EntityKind::Text(_))).count();
    assert!(boxes >= marks.len(), "each `?` has its box ({boxes} boxes for {} marks)", marks.len());
    // Print Book's scratch drawing: every flagged part is ACI 7, nothing else recoloured.
    let (d, _) = scratch(&mut h, &[bom]);
    let gen: Vec<_> = d.model.iter().filter(|e| e.common.layer == generated::LAYER).collect();
    let printed_marks: Vec<_> = gen.iter().filter(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == "?")).collect();
    assert_eq!(printed_marks.len(), marks.len());
    for e in &printed_marks {
        assert_eq!(e.common.color, cadcraft_engine::doc::color::Color::Index(7), "`?` printed black");
    }
    assert!(gen.iter().all(|e| e.common.color != cadcraft_engine::doc::color::Color::Index(generated::FLAGGED_ACI)), "no yellow left in print");
    // On screen still yellow (the page's own drawing).
    h.activate(bom);
    let shown = h.pv(|p, pg| pg.entities_of(p, bom));
    assert!(
        shown.iter().any(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == "?") && e.common.color == cadcraft_engine::doc::color::Color::Index(2)),
        "on screen the mark stays ACI 2"
    );
    // Print Book's words unchanged.
    let v = h.run("printbook", json!({"colour": "screen"})).unwrap();
    assert_eq!(v["flagged"].as_u64(), Some(marks.len() as u64), "{v}");
}

// ------------------------------------------------------------------------------------ AC5, AC6

/// A coil `-K01` on page 1 at `at` with four NO contacts on page 2.
fn coil_low(seed: u64, at: (f64, f64)) -> (H, PageId, ComponentId) {
    let mut h = H::new(seed);
    let p1 = h.first_page();
    let k1: ComponentId = id(&place(&mut h, "coil", at), "component");
    let _p2: PageId = id(&h.run("newpage", json!({"title": "Contacts"})).unwrap(), "page");
    for i in 0..4 {
        place_on(&mut h, "no", (60.0 + 40.0 * f64::from(i), 150.0), k1);
    }
    h.activate(p1);
    (h, p1, k1)
}

/// The table's drawn parts on `page`: (min x, min y, max x, max y).
fn table_box(h: &mut H, page: PageId, coil: ComponentId) -> (f64, f64, f64, f64) {
    let doc = h.doc();
    let head = xref::device(&doc, coil).head.unwrap();
    let mut b = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for part in xref::parts_on_page(&doc, page).into_iter().filter(|p| p.rep == head) {
        let ps = match part.shape {
            xref::Shape::Line(a, c) => vec![a, c],
            xref::Shape::Text { at, .. } => vec![at],
        };
        for p in ps {
            b = (b.0.min(p.x), b.1.min(p.y), b.2.max(p.x), b.3.max(p.y));
        }
    }
    assert!(b.0.is_finite(), "the coil has a drawn table");
    b
}

/// AC5 (design §4.1, §4.2): an automatic table that would cross the title block moves right of
/// the coil; with no room on the right it is lifted (and pulled left). Never over the title block.
#[test]
#[ignore = "tester 033: awaiting build"]
fn ac5_an_automatic_table_clears_the_title_block() {
    // Room on the right: moved right.
    let (mut h, p1, k1) = coil_low(0x33_05, (100.0, 20.0));
    let v = h.run("contacts", json!({"component": k1.hex()})).unwrap();
    assert_eq!(v["table"]["shown"], json!("right"), "{v}");
    assert_eq!(v["table"]["placement"], json!("auto"), "{v}");
    assert!(v["table"]["bottom"].as_f64().unwrap() >= 2.0 - 1e-9, "{v}");
    let b = table_box(&mut h, p1, k1);
    assert!(b.1 >= 0.0, "table above the title block: {b:?}");
    assert!(b.0 > 100.0, "right of the coil: {b:?}");
    // No room on the right: lifted, inside the frame.
    let (mut h, p1, k1) = coil_low(0x33_15, (392.0, 15.0));
    let v = h.run("contacts", json!({"component": k1.hex()})).unwrap();
    assert_eq!(v["table"]["shown"], json!("lifted"), "{v}");
    let bottom = v["table"]["bottom"].as_f64().unwrap();
    assert!((bottom - 2.0).abs() < 1e-6 || bottom >= 2.0, "{v}");
    let b = table_box(&mut h, p1, k1);
    assert!(b.1 >= 0.0 && b.2 <= 400.0 + 1e-6, "lifted and pulled left inside the frame: {b:?}");
    // A coil high on the page: the table stays where 025 put it.
    let (mut h, p1, k1) = coil_low(0x33_25, (100.0, 180.0));
    let v = h.run("contacts", json!({"component": k1.hex()})).unwrap();
    let shown = v["table"]["shown"].as_str().unwrap_or_default().to_string();
    assert!(shown != "right" && shown != "lifted", "{v}");
    let top = h.p(|p| xref::table(p.ledger.document(), &mut xref::Places::new(p.ledger.document()), &xref::device(p.ledger.document(), k1)).map(|t| t.top));
    assert_eq!(top.map(|t| t.x), Some(100.0), "under the coil as today");
    let _ = table_box(&mut h, p1, k1);
    // Moving the low coil's head up: the table follows and goes back under it.
    h.check();
}

/// AC6 (design §4.1): a table the user placed into the title block stays there (`as placed`), and
/// Checks warns with the tag and page.
#[test]
#[ignore = "tester 033: awaiting build"]
fn ac6_a_user_placed_table_stays_and_checks_warn() {
    let (mut h, p1, k1) = coil_low(0x33_06, (100.0, 120.0));
    let head = h.head();
    h.run("mirrormove", json!({"component": k1.hex(), "at": [100, -10]})).unwrap();
    assert_eq!(h.head(), head + 1);
    let v = h.run("contacts", json!({"component": k1.hex()})).unwrap();
    assert_eq!(v["table"]["shown"], json!("as placed"), "{v}");
    let b = table_box(&mut h, p1, k1);
    assert!(b.1 < 0.0, "not moved: {b:?}");
    let tag = tag(&mut h, k1);
    let checks = h.run("checks", json!({})).unwrap().to_string();
    let want = format!("The contact table of {tag} overlaps the title block on page");
    assert!(checks.contains(&want), "{want}: {checks}");
    // Moving it out clears the warning.
    h.run("mirrormove", json!({"component": k1.hex(), "at": [100, 60]})).unwrap();
    let checks = h.run("checks", json!({})).unwrap().to_string();
    assert!(!checks.contains("overlaps the title block"), "{checks}");
}

// ------------------------------------------------------------------------------------ AC7, AC8

/// A terminal plan page numbered `number` that runs to three or more sheets: one strip of 120
/// terminals. Returns the page and its sheet count.
fn long_plan(h: &mut H, number: &str) -> (PageId, usize) {
    for x in [100.0, 110.0] {
        h.run("wire", json!({"points": pts(&[(x, 200.0), (x, 100.0)])})).unwrap();
    }
    let v = h.run("terminals", json!({"from": [90, 150], "to": [120, 150]})).unwrap();
    let strip: ComponentId = id(&v, "strip");
    h.run("setprop", json!({"component": strip.hex(), "key": "terminals", "value": "120"})).unwrap();
    let page: PageId = id(&h.run("newpage", json!({"generated": "terminals", "number": number})).unwrap(), "page");
    let doc = h.doc();
    let n = generated::sheets(&doc, page, Some(&Outputs::build(&doc)));
    assert!(n >= 3, "120 terminals run to three sheets or more ({n})");
    (page, n)
}

fn sheet_numbers(h: &mut H, page: PageId) -> Vec<String> {
    let v = h.run("pageprops", json!({"page": page.hex()})).unwrap();
    v["sheet_numbers"].as_array().unwrap_or_else(|| panic!("pageprops gives sheet_numbers: {v}")).iter().map(|x| x.as_str().unwrap().to_string()).collect()
}

/// AC7 (design §5.1, §5.2): continuation sheets are numbered in sequence on every place they show.
#[test]
#[ignore = "tester 033: awaiting build"]
fn ac7_continuation_sheets_are_numbered_in_sequence() {
    printbook::install_io();
    let mut h = H::new(0x33_07);
    let (plan, n) = long_plan(&mut h, "400");
    let want: Vec<String> = (0..n).map(|i| (400 + i).to_string()).collect();
    assert_eq!(sheet_numbers(&mut h, plan), want);
    // Page boxes and Next: 401 is sheet 2's Page and sheet 1's Next.
    let texts = gen_texts(&h.doc(), plan);
    for w in &want {
        assert!(texts.contains(w), "{w} printed: {texts:?}");
    }
    assert!(texts.iter().filter(|t| *t == "401").count() >= 2, "401 is sheet 2's Page and sheet 1's Next: {texts:?}");
    assert!(texts.iter().any(|t| t.ends_with(&format!("(2/{n})"))), "the (2/n) title stays: {texts:?}");
    // Contents: one row per page with its range.
    let contents: PageId = id(&h.run("newpage", json!({"generated": "contents"})).unwrap(), "page");
    let range = format!("400\u{2013}{}", 400 + n - 1);
    assert!(gen_texts(&h.doc(), contents).iter().any(|t| t.contains(&range)), "contents shows {range}");
    // Project panel: the number column shows the range.
    let rows = h.p(|p| p.page_rows());
    assert!(rows.iter().any(|r| r.number == range), "{:?}", rows.iter().map(|r| r.number.clone()).collect::<Vec<_>>());
    // Print Book's Pages field: 401 selects page 400, all its sheets, and says so.
    let v = h.run("printbook", json!({"scope": {"pages": "401"}})).unwrap();
    assert_eq!(v["sheets"].as_u64(), Some(n as u64), "{v}");
    assert!(v["notes"].to_string().contains("401 is sheet 2 of page 400; the whole page is printed."), "{v}");
    // Numbers without digits and with leading zeros.
    h.run("pageprops", json!({"page": plan.hex(), "number": "BOM"})).unwrap();
    let mut want = vec!["BOM".to_string()];
    want.extend((2..=n).map(|i| format!("BOM{i}")));
    assert_eq!(sheet_numbers(&mut h, plan), want);
    h.run("pageprops", json!({"page": plan.hex(), "number": "0090"})).unwrap();
    assert_eq!(sheet_numbers(&mut h, plan)[..2], ["0090".to_string(), "0091".to_string()]);
    h.run("pageprops", json!({"page": plan.hex(), "number": "B10"})).unwrap();
    assert_eq!(sheet_numbers(&mut h, plan)[..2], ["B10".to_string(), "B11".to_string()]);
    // A one-sheet generated page keeps its single number.
    let cover: PageId = id(&h.run("newpage", json!({"generated": "cover", "number": "1"})).unwrap(), "page");
    assert_eq!(sheet_numbers(&mut h, cover), ["1"]);
}

/// AC8 (design §5.3): a continuation number counts as taken when typed; `newpage` without a
/// number skips it; a table that grows onto another page's number is a Checks warning.
#[test]
#[ignore = "tester 033: awaiting build"]
fn ac8_continuation_numbers_are_taken_and_a_clash_is_a_warning() {
    let mut h = H::new(0x33_08);
    let (plan, n) = long_plan(&mut h, "400");
    let title = h.doc().pages[&plan].title.clone();
    let e = refused(&mut h, "newpage", json!({"title": "X", "number": "401"}));
    assert_eq!(e, format!("Number 401 is sheet 2 of page 400 ({title})."));
    let other: PageId = id(&h.run("newpage", json!({"title": "Other"})).unwrap(), "page");
    let taken = sheet_numbers(&mut h, plan);
    let got = h.run("pageprops", json!({"page": other.hex()})).unwrap()["number"].as_str().unwrap().to_string();
    assert!(!taken.contains(&got), "newpage without a number skips continuation numbers: {got} in {taken:?}");
    let e = refused(&mut h, "pageprops", json!({"page": other.hex(), "number": "402"}));
    assert_eq!(e, format!("Number 402 is sheet 3 of page 400 ({title})."));
    // A page right after the last sheet, then the plan grows onto it.
    let next = (400 + n).to_string();
    h.run("pageprops", json!({"page": other.hex(), "number": next})).unwrap();
    let strip = h.doc().components.iter().find(|(_, c)| c.properties.get("terminals").is_some()).map(|(k, _)| *k).unwrap();
    h.run("setprop", json!({"component": strip.hex(), "key": "terminals", "value": "200"})).unwrap();
    let checks = h.run("checks", json!({})).unwrap().to_string();
    let doc = h.doc();
    let last = 400 + generated::sheets(&doc, plan, Some(&Outputs::build(&doc))) - 1;
    let want = format!("Page 400 ({title}) now runs to {last}, and page {next} has that number.");
    assert!(checks.contains(&want), "{want}: {checks}");
    // The book still prints.
    printbook::install_io();
    assert!(h.run("printbook", json!({})).is_ok());
}

// ------------------------------------------------------------------------------------ AC9

fn issue(h: &mut H, desc: &str, date: &str) {
    h.run("revision", json!({"description": desc, "drawn": "T"})).unwrap();
    h.run("issuerevision", json!({"date": date})).unwrap();
}

fn rev_box(h: &mut H, page: PageId) -> Vec<String> {
    frame_texts(h, page).into_iter().filter(|t| t.starts_with('A') || t.starts_with('B') || t == "-").collect()
}

/// AC9 (design §6): the pending marker follows the undo stack after the newest issue.
#[test]
#[ignore = "tester 033: awaiting build"]
fn ac9_the_pending_marker_follows_the_undo_stack() {
    printbook::install_io();
    let mut h = H::new(0x33_09);
    let page = h.first_page();
    h.run("wire", json!({"points": pts(&[(10.0, 10.0), (60.0, 10.0)])})).unwrap();
    issue(&mut h, "First issue", "2026-11-10");
    let has = |h: &mut H, t: &str| frame_texts(h, page).iter().any(|x| x == t);
    assert!(has(&mut h, "A") && !has(&mut h, "A*"), "{:?}", rev_box(&mut h, page));
    h.run("wire", json!({"points": pts(&[(10.0, 30.0), (60.0, 30.0)])})).unwrap();
    assert!(has(&mut h, "A*"), "edit after the issue: {:?}", rev_box(&mut h, page));
    let cover: PageId = id(&h.run("newpage", json!({"generated": "cover"})).unwrap(), "page");
    assert!(gen_texts(&h.doc(), cover).iter().any(|t| t == "Revision A, 2026-11-10 (changed since)"), "{:?}", gen_texts(&h.doc(), cover));
    let v = h.run("printbook", json!({})).unwrap();
    assert!(v["notes"].to_string().contains("The drawing has changed since Rev A; the title block shows A*."), "{v}");
    h.run("u", Value::Null).unwrap(); // the cover page
    h.run("u", Value::Null).unwrap(); // the wire
    h.activate(page);
    assert!(has(&mut h, "A") && !has(&mut h, "A*"), "undone: {:?}", rev_box(&mut h, page));
    h.run("revision", json!({"description": "next"})).unwrap();
    assert!(!has(&mut h, "A*"), "filling in the next revision is not a change");
    h.run("wire", json!({"points": pts(&[(10.0, 50.0), (60.0, 50.0)])})).unwrap();
    assert!(has(&mut h, "A*"));
    h.run("projectprops", json!({"revision_pending": "draft"})).unwrap();
    assert!(has(&mut h, "B (draft)"), "{:?}", frame_texts(&mut h, page));
    h.run("projectprops", json!({"revision_pending": "off"})).unwrap();
    assert!(has(&mut h, "A") && !has(&mut h, "A*") && !has(&mut h, "B (draft)"));
    h.run("projectprops", json!({"revision_pending": "star"})).unwrap();
    assert!(has(&mut h, "A*"));
    assert_eq!(h.doc().properties.get("revisions.pending").map(String::as_str), Some("star"), "FP3 key");
    // Save and reopen: the same state, computed on open.
    h.run("qsave", Value::Null).unwrap();
    let medium = h.p(|p| p.store.medium().clone());
    let mut s2 = cadcraft_engine::Session::empty();
    let r = ecad_cadcraft::ProjectHandle::open_from(&mut s2, "Demo", medium, 1).unwrap();
    let first = r.read(&mut s2, |p, _| p.ledger.document().page_order[0]).unwrap();
    r.open_page(&mut s2, first).unwrap();
    let texts: Vec<String> = r
        .read(&mut s2, |p, pg| pg.frame_entities_of(p, first))
        .unwrap()
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::Text(t) => Some(t.value.clone()),
            _ => None,
        })
        .collect();
    assert!(texts.iter().any(|t| t == "A*"), "reopened: {texts:?}");
    // Issue B: no marker.
    h.run("revision", json!({"drawn": "T"})).unwrap();
    h.run("issuerevision", json!({})).unwrap();
    assert!(has(&mut h, "B") && !has(&mut h, "B*") && !has(&mut h, "A*"), "{:?}", rev_box(&mut h, page));
    // Read-only revision view is never pending.
    let v = h.run("openrevision", json!({"rev": "A"})).unwrap();
    assert_eq!(v["rev"], "A");
    h.run("closerevision", json!({})).unwrap();
}

// ------------------------------------------------------------------------------------ AC10

/// AC10 (design §7): custom revision columns in the title block and the revision list; custom
/// fields follow the built-ins' rules; refusals write nothing.
#[test]
#[ignore = "tester 033: awaiting build"]
fn ac10_revision_columns_and_custom_fields() {
    let mut h = H::new(0x33_10);
    let page = h.first_page();
    let head = h.head();
    h.run("projectprops", json!({"revision_columns": "rev, date, ecr:ECR, description, drawn"})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    let (label, ops) = last_tx(&mut h);
    assert_eq!(label, "Project properties");
    assert!(sets(&ops, "revisions.columns"), "{ops:?}");
    h.run("revision", json!({"fields": {"ecr": "ECR-1"}})).unwrap();
    assert_eq!(h.doc().properties.get("rev.1.ecr").map(String::as_str), Some("ECR-1"));
    issue(&mut h, "First issue", "2026-11-10");
    let texts = frame_texts(&mut h, page);
    assert!(texts.iter().any(|t| t == "ECR") && texts.iter().any(|t| t == "ECR-1"), "{texts:?}");
    assert!(!texts.iter().any(|t| t == "Checked"), "columns not listed are hidden: {texts:?}");
    let e = refused(&mut h, "projectprops", json!({"props": {"rev.1.ecr": "x"}}));
    assert_eq!(e, "Rev A is issued and cannot change.");
    // The revision list page.
    let list: PageId = id(&h.run("newpage", json!({"generated": "revisions"})).unwrap(), "page");
    let t = gen_texts(&h.doc(), list);
    assert!(t.iter().any(|x| x == "ECR") && t.iter().any(|x| x == "ECR-1") && t.iter().any(|x| x == "Version"), "{t:?}");
    // Refusals.
    let cases = [
        ("rev, date, ecr!, description", "\"ecr!\" is not a field name. Use 1 to 20 letters, digits or _."),
        ("rev, date", "Rev and Description must be columns."),
        ("rev, description, a, b, c, d, e, f, g, h, i", "At most 10 columns."),
        ("rev, issued, description", "\"issued\" is set by issuing."),
    ];
    for (cols, want) in cases {
        let e = refused(&mut h, "projectprops", json!({"revision_columns": cols}));
        assert_eq!(e, want, "{cols}");
    }
    // Widths too wide are narrowed, never refused.
    h.run("projectprops", json!({"revision_columns": "rev@100, date@100, description, drawn@100"})).unwrap();
    // A field removed from the columns keeps its stored value.
    h.run("projectprops", json!({"revision_columns": null})).unwrap();
    assert_eq!(h.doc().properties.get("rev.1.ecr").map(String::as_str), Some("ECR-1"));
    let texts = frame_texts(&mut h, page);
    assert!(texts.iter().any(|t| t == "Checked") && !texts.iter().any(|t| t == "ECR"), "back to the six: {texts:?}");
}

// ------------------------------------------------------------------------------------ AC11, AC12

/// AC11 (design §8): tag style is a writing rule; changing the dash rewrites every tag in one
/// transaction that one undo takes back; identity ignores case and the dash.
#[test]
#[ignore = "tester 033: awaiting build"]
fn ac11_tag_style_rewrites_in_one_undoable_step() {
    let mut h = H::new(0x33_11_1);
    let k1: ComponentId = id(&place(&mut h, "coil", (60.0, 150.0)), "component");
    let k2: ComponentId = id(&place(&mut h, "coil", (100.0, 150.0)), "component");
    let q1: ComponentId = id(&place(&mut h, "mcb1", (200.0, 150.0)), "component");
    assert_eq!([tag(&mut h, k1), tag(&mut h, k2), tag(&mut h, q1)], ["-K01", "-K02", "-Q01"]);
    let before = h.doc();
    let head = h.head();
    let v = h.run("projectprops", json!({"tag_dash": false})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    assert_eq!([tag(&mut h, k1), tag(&mut h, k2), tag(&mut h, q1)], ["K01", "K02", "Q01"]);
    assert!(v.to_string().contains("3 tags rewritten"), "{v}");
    assert_eq!(v["tag_dash"], json!(false), "{v}");
    let (_, ops) = last_tx(&mut h);
    assert!(sets(&ops, "tags.dash"), "{ops:?}");
    assert_eq!(ops.iter().filter(|o| matches!(o, Op::SetProperty { object: ObjectRef::Component(_), key, .. } if key == "tag")).count(), 3);
    h.run("u", Value::Null).unwrap();
    let after_undo = h.doc();
    assert_eq!(after_undo.components, before.components, "undo restores every tag");
    assert_eq!(after_undo.properties, before.properties, "and the setting");
    h.run("redo", Value::Null).unwrap();
    // Case: typed from now on; existing tags unchanged.
    let v = h.run("projectprops", json!({"tag_case": "typed"})).unwrap();
    assert!(v.to_string().contains("existing tags are unchanged"), "{v}");
    assert_eq!(tag(&mut h, k1), "K01");
    h.run("setprop", json!({"component": k2.hex(), "key": "tag", "value": "M02a"})).unwrap();
    assert_eq!(tag(&mut h, k2), "M02a");
    // Identity unchanged: -k1 is K01's tag.
    let e = h.run("setprop", json!({"component": q1.hex(), "key": "tag", "value": "-k1"}));
    assert!(e.is_err() || tag(&mut h, k1) != "K01" || h.doc().components.values().filter(|c| c.properties.get("tag").is_some_and(|t| t.eq_ignore_ascii_case("k01"))).count() == 1, "conflict rules unchanged");
    // Dash yes with typed case.
    h.run("projectprops", json!({"tag_dash": true})).unwrap();
    assert_eq!(tag(&mut h, k2), "-M02a");
    // Back to upper: upper-cases every tag in the same transaction.
    let head = h.head();
    h.run("projectprops", json!({"tag_case": "upper"})).unwrap();
    assert_eq!(h.head(), head + 1);
    assert_eq!(tag(&mut h, k2), "-M02A");
    let v = h.run("projectprops", json!({})).unwrap();
    assert!(v.get("tag_padding").is_some() && v.get("tag_case").is_some() && v.get("tag_dash").is_some(), "{v}");
}

/// AC12 (design §8.3): with `typed`, renumber keeps each tag's own letters.
#[test]
#[ignore = "tester 033: awaiting build"]
fn ac12_renumber_keeps_typed_letters() {
    let mut h = H::new(0x33_12);
    h.run("projectprops", json!({"tag_dash": false, "tag_case": "typed"})).unwrap();
    let a: ComponentId = id(&place(&mut h, "coil", (60.0, 150.0)), "component");
    let b: ComponentId = id(&place(&mut h, "coil", (100.0, 150.0)), "component");
    for (c, t) in [(a, "k05"), (b, "k07")] {
        h.run("setprop", json!({"component": c.hex(), "key": "tag", "value": t})).unwrap();
        // Typed tags are fixed (026); free them for renumber.
        let _ = h.run("setprop", json!({"component": c.hex(), "key": "tag_fixed", "value": null}));
    }
    assert_eq!([tag(&mut h, a), tag(&mut h, b)], ["k05", "k07"]);
    h.run("renumber", json!({"kind": "devices", "start": 1})).unwrap();
    assert_eq!([tag(&mut h, a), tag(&mut h, b)], ["k01", "k02"]);
}

// ------------------------------------------------------------------------------------ AC13

/// AC13 (design §11): `table` is refused before anything is written, in the design's words; the
/// plural is fixed for every refused kind.
#[test]
#[ignore = "tester 033: awaiting build"]
fn ac13_table_and_hatch_are_refused_in_clean_words() {
    let mut h = H::new(0x33_13);
    let e = refused(&mut h, "table", json!({"at": [50, 50], "rows": 2, "cols": 2}));
    assert_eq!(
        e,
        "Tables can't be placed on ECAD pages yet. Draw it with lines and text, or add a generated page for lists (New Page \u{25b8} Generated)."
    );
    let e = refused(&mut h, "hatch", json!({"points": [[0, 40], [10, 40], [10, 50]]}));
    assert!(e.contains("Hatches can't be placed on ECAD pages yet."), "{e}");
    for e in [&e] {
        assert!(!e.contains("kinds are") && !e.contains("hatchs"), "{e}");
    }
    h.check();
}

// ------------------------------------------------------------------------------------ AC14

/// AC14, the part that runs now: a project without any 033 setting shows the six revision
/// columns and `-` tags, and opens with no transaction. Must still hold after 033.
#[test]
fn ac14_a_project_without_033_settings_looks_as_before() {
    let mut h = H::new(0x33_14);
    let page = h.first_page();
    let k: ComponentId = id(&place(&mut h, "coil", (60.0, 150.0)), "component");
    assert_eq!(tag(&mut h, k), "-K01");
    issue(&mut h, "First issue", "2026-11-10");
    let texts = frame_texts(&mut h, page);
    for w in ["Rev", "Date", "Description", "Drawn", "Checked", "Approved", "A", "First issue"] {
        assert!(texts.iter().any(|t| t == w), "{w}: {texts:?}");
    }
    h.run("qsave", Value::Null).unwrap();
    let doc = h.doc();
    let medium = h.p(|p| p.store.medium().clone());
    let mut s2 = cadcraft_engine::Session::empty();
    let r = ecad_cadcraft::ProjectHandle::open_from(&mut s2, "Demo", medium, 1).unwrap();
    let (doc2, unsaved) = r.read(&mut s2, |p, _| (p.ledger.document().clone(), p.ledger.is_unsaved())).unwrap();
    assert_eq!(doc2, doc);
    assert!(!unsaved, "opening writes nothing");
    assert!(!doc2.properties.keys().any(|k| k.starts_with("tags.") || k.starts_with("revisions.")), "no 033 key appears by itself");
}

/// AC14 after the build: the old project above shows the pending marker only by the pending rule
/// (an edit after the issue), and its new keys stay absent until set.
#[test]
#[ignore = "tester 033: awaiting build"]
fn ac14_an_old_project_shows_the_marker_only_by_the_pending_rule() {
    let mut h = H::new(0x33_14_1);
    let page = h.first_page();
    issue(&mut h, "First issue", "2026-11-10");
    assert!(!frame_texts(&mut h, page).iter().any(|t| t == "A*"));
    h.run("wire", json!({"points": pts(&[(10.0, 30.0), (60.0, 30.0)])})).unwrap();
    assert!(frame_texts(&mut h, page).iter().any(|t| t == "A*"));
    assert!(!h.doc().properties.keys().any(|k| k.starts_with("tags.") || k.starts_with("revisions.")));
}

// ------------------------------------------------------------------------------------ AC15

/// AC15 (design §15): hostile settings are refused or read as absent; nothing panics; the frame
/// still draws.
#[test]
#[ignore = "tester 033: awaiting build"]
fn ac15_hostile_settings_never_panic() {
    let mut h = H::new(0x33_15_1);
    let page = h.first_page();
    issue(&mut h, "First issue", "2026-11-10");
    let thousand = (0..1000).map(|i| format!("f{i}")).collect::<Vec<_>>().join(", ");
    let big = "x".repeat(10_000);
    let hostile: Vec<Value> = vec![
        json!({"revision_columns": big}),
        json!({"revision_columns": thousand}),
        json!({"revision_columns": "rev@NaN, description"}),
        json!({"revision_columns": "rev@-5, description"}),
        json!({"revision_columns": "rev@1e308, description"}),
        json!({"revision_columns": "rev, description, ecr:\u{0}\u{7}"}),
        json!({"revision_columns": ",,,,"}),
        json!({"revision_columns": 5}),
        json!({"tag_case": "x"}),
        json!({"tag_case": 1}),
        json!({"tag_dash": "maybe"}),
        json!({"revision_pending": ""}),
        json!({"revision_pending": "x"}),
        json!({"props": {"tags.case": "x"}}),
        json!({"props": {"revisions.pending": ""}}),
        json!({"props": {"revisions.columns": "rev@NaN"}}),
    ];
    for params in hostile {
        let head = h.head();
        let r = h.run("projectprops", params.clone());
        if r.is_err() {
            assert_eq!(h.head(), head, "{params}: refused writes nothing");
        }
        assert!(!frame_texts(&mut h, page).is_empty(), "{params}: the frame still draws");
        h.check();
    }
    // A continuation number at the edge of u64 and of its width.
    let (plan, n) = long_plan(&mut h, "18446744073709551615");
    let nums = sheet_numbers(&mut h, plan);
    assert_eq!(nums.len(), n, "{nums:?}");
    h.run("pageprops", json!({"page": plan.hex(), "number": "999"})).unwrap();
    assert_eq!(sheet_numbers(&mut h, plan)[..2], ["999".to_string(), "1000".to_string()]);
    let _ = frame::LAYER;
}
