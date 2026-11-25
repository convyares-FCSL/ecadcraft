//! A report page of several sheets shows one sheet at a time in the page window, at the origin:
//! `openpage` with a sheet picks it, a go-to into a later sheet shows that sheet, an edit keeps the
//! sheet shown, and Print Book still draws every sheet.

use cadcraft_engine::doc::EntityKind;
use ecad_electrical::reports::kept::ReportModels;
use ecad_model::{PageId, Pt};
use serde_json::json;

use crate::common::{H, id};

/// The texts of the page window while `page` is shown, and the rightmost x of any of them.
fn shown(h: &mut H, page: PageId) -> (Vec<String>, f64) {
    h.pv(|p, pg| {
        let uid = p.pages.get(&page).and_then(|v| v.uid).expect("page shown");
        let d = &pg.doc(uid).expect("its drawing").doc;
        let mut texts = Vec::new();
        let mut right = f64::NEG_INFINITY;
        for e in d.model.iter() {
            if let EntityKind::Text(t) = &e.kind {
                texts.push(t.value.clone());
                right = right.max(t.insert.x);
            }
        }
        (texts, right)
    })
}

fn has(texts: &[String], words: &str) -> bool {
    texts.iter().any(|t| t.contains(words))
}

/// A project whose BOM page runs to three sheets, the BOM page shown.
fn three_sheets(seed: u64) -> (H, PageId) {
    let mut h = H::new(seed);
    let first = h.first_page();
    let bom: PageId = id(&h.run("newpage", json!({"generated": "bom-location", "title": "Parts"})).unwrap(), "page");
    h.activate(first);
    for i in 0..88 {
        let at = json!([10.0 + f64::from(i % 12) * 30.0, 40.0 + f64::from(i / 12) * 30.0]);
        h.s.execute("place", &json!({"symbol": "coil", "at": at})).unwrap();
    }
    h.activate(bom);
    assert_eq!(h.p(|p| p.shown_sheet(bom)), (0, 3));
    (h, bom)
}

#[test]
fn a_report_page_shows_one_sheet_and_openpage_picks_it() {
    let (mut h, bom) = three_sheets(0x0c47_0001);
    let width = 420.0;
    let (texts, right) = shown(&mut h, bom);
    assert!(has(&texts, "Parts (1/3)") && !has(&texts, "(2/3)"), "{texts:?}");
    assert!(right < width, "sheet 1 drawn alone, at the origin: {right}");
    h.check();

    let reply = h.run("openpage", json!({"page": bom.hex(), "sheet": 2})).unwrap();
    assert_eq!((reply["sheet"].as_u64(), reply["sheets"].as_u64()), (Some(2), Some(3)), "{reply}");
    let (texts, right) = shown(&mut h, bom);
    assert!(has(&texts, "Parts (2/3)") && !has(&texts, "(1/3)") && !has(&texts, "(3/3)"), "{texts:?}");
    assert!(right < width, "sheet 2 drawn at the origin: {right}");
    h.check();

    // An edit that changes the BOM keeps the sheet shown.
    let field = h.first_page();
    h.activate(field);
    h.s.execute("place", &json!({"symbol": "coil", "at": [400.0, 280.0]})).unwrap();
    h.activate(bom);
    assert_eq!(h.p(|p| p.shown_sheet(bom)), (1, 3));
    assert!(has(&shown(&mut h, bom).0, "Parts (2/3)"));
    h.check();

    // Past the last sheet shows the last; nothing is recorded.
    let head = h.head();
    let reply = h.run("openpage", json!({"page": bom.hex(), "sheet": 9})).unwrap();
    assert_eq!(reply["sheet"].as_u64(), Some(3), "{reply}");
    assert!(has(&shown(&mut h, bom).0, "Parts (3/3)"));
    assert_eq!(h.head(), head);
    h.check();

    // Print Book still draws every sheet side by side.
    let doc = h.doc();
    let o = ReportModels::build_with(&doc, &ecad_engine::styles::draw::LOOK);
    let all: Vec<String> = ecad_engine::pages::generated::parts_of(&doc, bom, Some(&o))
        .into_iter()
        .filter_map(|p| match p.kind {
            EntityKind::Text(t) => Some(t.value),
            _ => None,
        })
        .collect();
    assert!(["Parts (1/3)", "Parts (2/3)", "Parts (3/3)"].iter().all(|w| has(&all, w)), "{all:?}");
    assert_eq!(ecad_electrical::pages::generated::sheet_windows_of(&doc, bom, Some(&o)).len(), 3);
}

#[test]
fn a_go_to_into_a_later_sheet_shows_that_sheet_at_its_own_point() {
    let (mut h, bom) = three_sheets(0x0c47_0002);
    // Sheet 2 of the side-by-side layout starts at 420 + 50 mm.
    let (sheet, at) = h.cfg(|p| p.sheet_at(bom, Pt::new(470.0 + 25.0, 100.0)));
    assert_eq!(sheet, 1);
    assert!((at.x - 25.0).abs() < 1e-9 && (at.y - 100.0).abs() < 1e-9, "{at:?}");
    h.run("openpage", json!({"page": bom.hex(), "sheet": sheet + 1})).unwrap();
    assert_eq!(h.p(|p| p.shown_sheet(bom)), (1, 3));
    assert!(has(&shown(&mut h, bom).0, "Parts (2/3)"));
    // A point on the first sheet shows the first.
    let (sheet, at) = h.cfg(|p| p.sheet_at(bom, Pt::new(30.0, 100.0)));
    assert!(sheet == 0 && (at.x - 30.0).abs() < 1e-9, "{sheet} {at:?}");
    h.run("openpage", json!({"page": bom.hex(), "sheet": sheet + 1})).unwrap();
    assert_eq!(h.p(|p| p.shown_sheet(bom)), (0, 3));
    h.check();
}
