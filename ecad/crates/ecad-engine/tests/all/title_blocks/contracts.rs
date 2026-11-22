//! Brief 055 (design 055 §12.2): title blocks: the house block, frames with bodies, bindings and
//! their update map, sheet resolution through the frame, the checks, DXF logos and import, and
//! hostile bodies.

use cadcraft_engine::doc::EntityKind;
use ecad_engine::pages::frame::{self, FrameInput};
use ecad_model::document::book::Book;
use ecad_model::document::sheet::{self, SheetSpec};
use ecad_model::objects::title_block::{Binding, BlockItem, IfEmpty, Rect, TextDraw, TitleBlockLayout};
use ecad_model::{PageId, Primitive, Pt};
use serde_json::{Value, json};
use {ecad_electrical::title_blocks::house, ecad_engine::title_blocks::dxf};

use crate::common::H;

fn texts(h: &mut H, page: PageId) -> Vec<(String, Option<u8>, f64, f64)> {
    h.pv(|p, pg| pg.frame_entities_of(p, page))
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::Text(t) => {
                let aci = match e.common.color {
                    cadcraft_engine::doc::color::Color::Index(i) => Some(i),
                    _ => None,
                };
                let at = t.align_pt.unwrap_or(t.insert);
                Some((t.value.clone(), aci, at.x, at.y))
            }
            _ => None,
        })
        .collect()
}

fn text_values(h: &mut H, page: PageId) -> Vec<String> {
    texts(h, page).into_iter().map(|t| t.0).collect()
}

fn new_frame(h: &mut H, sheet: &str) -> String {
    let v = h.run("titleblock", json!({"new": {"from": "sheet", "sheet": sheet}})).unwrap();
    v["id"].as_str().unwrap().to_string()
}

fn body_of(h: &mut H, id: &str) -> Value {
    h.run("titleblock", json!({"id": id})).unwrap()["body"].clone()
}

fn frame_parts(h: &mut H, page: PageId) -> Vec<frame::Part> {
    let doc = h.doc();
    frame::parts(&FrameInput::of(&doc, page, &Book::of(&doc)))
}

/// Row A: every page shows the house block: revision grid with ECR, a red status, function,
/// location, page/next/of, title, drawing number, Rev; captions are our own words.
#[test]
fn the_house_block_is_on_every_page() {
    let mut h = H::new(0x0551);
    let page = h.first_page();
    h.run("projectprops", json!({"title": "Pump station", "drawing_number": "PS-E-002", "status": "RELEASED", "customer": "Northfield"})).unwrap();
    let t = texts(&mut h, page);
    let has = |s: &str| t.iter().any(|x| x.0 == s);
    for caption in ["ECR", "Rev", "Description", "Drawn", "Status", "Function", "Location", "Page", "Next", "Of", "Title", "Drawing no.", "Size"] {
        assert!(has(caption), "{caption} in {t:?}");
    }
    for value in ["Pump station", "PS-E-002", "Northfield", "A3", "1"] {
        assert!(has(value), "{value} in {t:?}");
    }
    let status = t.iter().find(|x| x.0 == "RELEASED").unwrap();
    assert_eq!(status.1, Some(ecad_engine::title_blocks::layout::RED_ACI), "the status is red");
    // Nothing moves: the keep-out strip is still 40 mm.
    let doc = h.doc();
    assert_eq!(sheet::keep_out(&doc, page), sheet::TITLE_BLOCK_MM);
}

/// The house block keeps its bounds on every preset, and its revision table widens with the sheet.
#[test]
fn the_house_block_fits_every_preset() {
    let doc = ecad_model::Document::default();
    let text = frame::FrameText::of(&doc);
    let mut last = 0.0;
    for (name, ..) in sheet::PRESETS {
        let spec = SheetSpec::preset(name).unwrap();
        let w = ecad_electrical::title_blocks::frame_width(&spec, &sheet::Margins::default());
        let l = house::house(w, &text);
        l.check().unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(l.size, (w, 40.0), "{name}");
        let table = l.items.iter().find_map(|i| if let BlockItem::RevTable { at, .. } = i { Some(at.w) } else { None }).unwrap();
        assert!(table > last, "{name}: the table widens");
        last = table;
        for i in &l.items {
            let b = i.bounds().unwrap();
            assert!(b.x >= -1e-9 && b.right() <= w + 1e-9 && b.top() <= 40.0 + 1e-9, "{name}: {i:?}");
        }
    }
}

/// Rows B and C: a frame made from the built-in returns the house items, draws the same, and a
/// field moved and saved moves on the page; one undo puts it back.
#[test]
fn a_frame_from_the_built_in_draws_the_same_and_follows_its_body() {
    let mut h = H::new(0x0552);
    let page = h.first_page();
    h.run("projectprops", json!({"drawing_number": "DN-1"})).unwrap();
    let before = frame_parts(&mut h, page);
    let id = new_frame(&mut h, "A3");
    let body = body_of(&mut h, &id);
    let doc = h.doc();
    let want = house::house(400.0, &frame::FrameText::of(&doc));
    assert_eq!(body["items"].as_array().unwrap().len(), want.items.len());
    h.run("setdefault", json!({"key": "print.frame.style", "level": "project", "value": id})).unwrap();
    assert_eq!(frame_parts(&mut h, page), before, "choosing the copy changes nothing visible");
    // Move the drawing number field 10 mm to the right.
    let at = |t: &[(String, Option<u8>, f64, f64)]| t.iter().find(|x| x.0 == "DN-1").map(|x| (x.2, x.3)).unwrap();
    let was = at(&texts(&mut h, page));
    let mut body = body;
    for item in body["items"].as_array_mut().unwrap() {
        if item["bind"] == "project:drawing_number" {
            let x = item["at"]["x"].as_f64().unwrap();
            item["at"]["x"] = json!(x + 10.0);
        }
    }
    let head = h.head();
    h.run("titleblock", json!({"id": id, "body": body})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    let now = at(&texts(&mut h, page));
    assert!((now.0 - was.0 - 10.0).abs() < 1e-6 && (now.1 - was.1).abs() < 1e-9, "{was:?} → {now:?}");
    h.cmdline("u");
    assert_eq!(at(&texts(&mut h, page)), was, "undo puts it back");
    h.check();
}

/// Row D: a field bound to a user's project key shows it; that key redraws the frame, a key no
/// field reads rebuilds nothing.
#[test]
fn a_bound_project_key_redraws_and_an_unread_one_does_not() {
    let mut h = H::new(0x0553);
    let page = h.first_page();
    let id = new_frame(&mut h, "A3");
    let mut body = body_of(&mut h, &id);
    body["items"].as_array_mut().unwrap().push(json!({
        "kind": "field", "at": {"x": 2, "y": 2, "w": 40, "h": 8}, "bind": "project:job_no", "label": "Job", "draw": {"size": 2.5}
    }));
    h.run("titleblock", json!({"id": id, "body": body})).unwrap();
    h.run("setdefault", json!({"key": "print.frame.style", "level": "project", "value": id})).unwrap();
    let _ = texts(&mut h, page);
    let builds = h.p(|p| p.frame_builds);
    h.run("projectprops", json!({"props": {"unrelated": "x"}})).unwrap();
    let _ = texts(&mut h, page);
    assert_eq!(h.p(|p| p.frame_builds), builds, "a key no field reads rebuilds no frame");
    h.run("projectprops", json!({"props": {"job_no": "J-7"}})).unwrap();
    assert!(text_values(&mut h, page).contains(&"J-7".to_string()));
    assert_eq!(h.p(|p| p.frame_builds), builds + 1);
    let v = h.run("titleblocks", json!({})).unwrap();
    assert!(v["keys"].as_array().unwrap().contains(&json!("job_no")), "{v}");
}

/// Row E: an own A2 frame gives its pages A2 paper and zones; a page whose own sheet is A3 gets a
/// warning whose fix clears it.
#[test]
fn an_a2_frame_decides_its_pages_sheet() {
    let mut h = H::new(0x0554);
    let page = h.first_page();
    let id = new_frame(&mut h, "A2");
    h.run("setdefault", json!({"key": "print.frame.style", "level": "project", "value": id})).unwrap();
    let doc = h.doc();
    assert_eq!(sheet::sheet_of(&doc, page), SheetSpec::preset("A2").unwrap());
    assert_eq!(sheet::Grid::of(&doc, page).spec.columns, 12, "A2 zones");
    assert!(text_values(&mut h, page).contains(&"L".to_string()) || text_values(&mut h, page).contains(&"H".to_string()));
    h.run("pageprops", json!({"sheet": "A3"})).unwrap();
    let doc = h.doc();
    assert_eq!(sheet::sheet_of(&doc, page).width, 594, "the frame wins");
    let rows = h.run("checks", json!({"kinds": ["frame-sheet-differs"]})).unwrap();
    let rows = rows["rows"].as_array().or(rows["checks"].as_array()).cloned().unwrap_or_default();
    assert_eq!(rows.len(), 1, "{rows:?}");
    let which = rows[0].to_string();
    assert!(which.contains("is A3, but its frame") && which.contains("is A2; the frame wins."), "{which}");
    // The fix: clear the page's sheet.
    h.run("pageprops", json!({"page": page.hex(), "sheet": Value::Null})).unwrap();
    let rows = h.run("checks", json!({"kinds": ["frame-sheet-differs"]})).unwrap();
    let rows = rows["rows"].as_array().or(rows["checks"].as_array()).cloned().unwrap_or_default();
    assert!(rows.is_empty());
}

/// `sheet_of`'s order: frame sheet, page sheet, project sheet, A3.
#[test]
fn sheets_resolve_frame_then_page_then_project() {
    let mut h = H::new(0x0555);
    let page = h.first_page();
    assert_eq!(sheet::sheet_of(&h.doc(), page), SheetSpec::default());
    h.run("projectprops", json!({"sheet": "A1"})).unwrap();
    assert_eq!(sheet::sheet_of(&h.doc(), page).width, 841);
    h.run("pageprops", json!({"sheet": "A2"})).unwrap();
    assert_eq!(sheet::sheet_of(&h.doc(), page).width, 594);
    let id = new_frame(&mut h, "A0");
    h.run("setdefault", json!({"key": "print.frame.style", "level": "page", "value": id})).unwrap();
    assert_eq!(sheet::sheet_of(&h.doc(), page).width, 1189);
    assert_eq!(sheet::own_sheet(&h.doc(), page).width, 594);
}

/// Checks: a binding its source does not have is refused (design 072 §5.4: it does not read); a
/// value too long for its box is cut and listed.
#[test]
fn unknown_bindings_and_cut_values_are_listed() {
    let mut h = H::new(0x0556);
    let id = new_frame(&mut h, "A3");
    let mut body = body_of(&mut h, &id);
    let good = body.clone();
    body["items"].as_array_mut().unwrap().push(json!({"kind": "field", "at": {"x": 2, "y": 2, "w": 40, "h": 8}, "bind": "print:weather"}));
    let e = h.run("titleblock", json!({"id": id, "body": body})).unwrap_err();
    assert!(e.contains("weather"), "{e}");
    h.run("titleblock", json!({"id": id, "body": good})).unwrap();
    h.run("setdefault", json!({"key": "print.frame.style", "level": "project", "value": id})).unwrap();
    h.run("projectprops", json!({"drawing_number": "X".repeat(120)})).unwrap();
    let v = h.run("checks", json!({"kinds": ["title-binding-unknown", "title-value-cut"]})).unwrap();
    let s = v.to_string();
    assert!(!s.contains("cannot fill"), "{s}");
    assert!(s.contains("Drawing no. is cut in the title block on 1 page."), "{s}");
}

/// Hostile bodies (row J): too many items, NaN and huge numbers, long texts, unknown items and
/// sources draw without a panic; commands refuse them in words.
#[test]
fn hostile_bodies_draw_and_are_refused_by_commands() {
    let mut h = H::new(0x0557);
    let page = h.first_page();
    let doc = h.doc();
    let mut input = FrameInput::of(&doc, page, &Book::of(&doc));
    let field = |x: f64| BlockItem::Field {
        at: Rect::new(x, 1.0, 10.0, 5.0),
        bind: Binding::Print(ecad_model::objects::title_block::PrintField::Sheet),
        label: Some("é".repeat(10_000)),
        draw: TextDraw::default(),
        if_empty: IfEmpty::Text("t".repeat(100)),
    };
    let mut items: Vec<BlockItem> = (0..10_000).map(|i| field(f64::from(i))).collect();
    items.push(BlockItem::Line { a: (f64::NAN, 0.0), b: (1e308, f64::INFINITY), weight: Default::default() });
    items.push(BlockItem::Logo {
        at: Rect::new(1.0, 1.0, 5.0, 5.0),
        name: String::new(),
        prims: vec![Primitive::Circle { center: Pt::new(f64::NAN, 0.0), radius: 1.0 }; 5000],
    });
    let l = TitleBlockLayout { size: (f64::NAN, 1e308), items };
    assert!(l.check().is_err());
    input.block.layout = std::sync::Arc::new(l);
    let (_, drawn) = frame::drawn(&input);
    assert!(drawn.skipped >= 10_000 - ecad_model::objects::title_block::MAX_ITEMS, "{drawn:?}");
    // The commands refuse in words; nothing is written.
    let id = new_frame(&mut h, "A3");
    let head = h.head();
    let many: Vec<Value> = (0..600).map(|_| json!({"kind": "line", "a": [0, 0], "b": [1, 1]})).collect();
    for (body, want) in [
        (json!({"size": [100, 40], "items": many}), "at most 512 items"),
        (json!({"size": [100, 4000], "items": []}), "1 to 400 mm high"),
        (
            json!({"size": [100, 40], "items": [{"kind": "text", "at": {"x": 0, "y": 0, "w": 10, "h": 5}, "text": "x".repeat(300)}]}),
            "longer than 256",
        ),
        (json!({"size": [100, 40], "items": [{"kind": "rev_table", "at": {"x": 0, "y": 0, "w": 10, "h": 5}, "rows": 30}]}), "1 to 20 rows"),
        (json!({"size": [100, 40], "items": [{"kind": "hologram"}]}), "does not read"),
        (json!({"size": [100, 40], "items": [{"kind": "field", "at": {"x": 0, "y": 0, "w": 10, "h": 5}, "bind": "weather:today"}]}), "weather"),
        (json!({"size": "big"}), "does not read as a title block"),
    ] {
        let e = h.run("titleblock", json!({"id": id, "body": body})).unwrap_err();
        assert!(e.contains(want), "{want}: {e}");
    }
    assert_eq!(h.head(), head, "refusals record nothing");
    let e = h.run("titleblock", json!({"id": "sheet", "body": {"size": [10, 10], "items": []}})).unwrap_err();
    assert!(e.contains("Built-in"), "{e}");
}

// ------------------------------------------------------------------------------- DXF

/// A synthetic ASCII DXF (R12 entities section) of `lines` (x1, y1, x2, y2) and `texts`
/// (x, y, height, text). Made by the test; never a customer's file.
pub(crate) fn dxf(lines: &[(f64, f64, f64, f64)], texts: &[(f64, f64, f64, &str)]) -> String {
    let mut s = String::from("0\nSECTION\n2\nENTITIES\n");
    for (x1, y1, x2, y2) in lines {
        s.push_str(&format!("0\nLINE\n8\n0\n10\n{x1}\n20\n{y1}\n30\n0\n11\n{x2}\n21\n{y2}\n31\n0\n"));
    }
    for (x, y, ht, t) in texts {
        s.push_str(&format!("0\nTEXT\n8\n0\n10\n{x}\n20\n{y}\n30\n0\n40\n{ht}\n1\n{t}\n"));
    }
    s.push_str("0\nENDSEC\n0\nEOF\n");
    s
}

/// Row F: a 120-shape logo fits its box keeping its proportions; 3 400 shapes and a non-DXF are
/// refused in words.
#[test]
fn logos_fit_their_box_and_bad_files_are_refused() {
    let lines: Vec<(f64, f64, f64, f64)> = (0..120).map(|i| (f64::from(i), 0.0, f64::from(i), 50.0)).collect();
    let prims = dxf::logo(dxf(&lines, &[]).as_bytes()).unwrap();
    assert_eq!(prims.len(), 120);
    let many: Vec<(f64, f64, f64, f64)> = (0..3400).map(|i| (f64::from(i), 0.0, f64::from(i), 1.0)).collect();
    assert_eq!(dxf::logo(dxf(&many, &[]).as_bytes()).unwrap_err(), "The logo has 3 400 shapes; 2 000 at most. Simplify it first.");
    assert!(dxf::logo(b"\x89PNG\r\n\x1a\nnot a dxf").unwrap_err().starts_with("This file is not a DXF we can read"));
    assert_eq!(dxf::logo(dxf(&[], &[]).as_bytes()).unwrap_err(), dxf::EMPTY);
    // Into a frame's logo item: fitted, centred, proportions kept (119 × 50 into 76 × 10).
    let mut h = H::new(0x0558);
    let page = h.first_page();
    let id = new_frame(&mut h, "A3");
    h.run("setdefault", json!({"key": "print.frame.style", "level": "project", "value": id})).unwrap();
    let body = body_of(&mut h, &id);
    let item = body["items"].as_array().unwrap().iter().position(|i| i["kind"] == "logo").unwrap();
    let head = h.head();
    h.run("titleblockimport", json!({"id": id, "item": item, "dxf": dxf(&lines, &[])})).unwrap();
    assert_eq!(h.head(), head + 1);
    let doc = h.doc();
    let (parts, _) = frame::drawn(&FrameInput::of(&doc, page, &Book::of(&doc)));
    let ys: Vec<f64> = parts
        .iter()
        .filter_map(|p| match &p.kind {
            // The logo's lines: vertical, in the logo box (zone ticks and centre marks are in the margins).
            EntityKind::Line(l)
                if (l.a.x - l.b.x).abs() < 1e-9
                    && l.a.y.min(l.b.y) > 20.0
                    && l.a.y.max(l.b.y) < 50.0
                    && (1.0..10.5).contains(&(l.a.y - l.b.y).abs()) =>
            {
                Some((l.a.y - l.b.y).abs())
            }
            _ => None,
        })
        .collect();
    assert_eq!(ys.len(), 120, "every shape drawn");
    assert!(ys.iter().all(|h| (h - 10.0).abs() < 1e-6), "fitted to the box's height: {ys:?}");
}

/// Row G: Import DXF makes a frame of line and text items at 1:1; too large is refused in words.
#[test]
fn a_dxf_title_block_imports_at_full_size() {
    let mut h = H::new(0x0559);
    let lines = [(0.0, 0.0, 180.0, 0.0), (180.0, 0.0, 180.0, 40.0), (180.0, 40.0, 0.0, 40.0), (0.0, 40.0, 0.0, 0.0), (90.0, 0.0, 90.0, 40.0)];
    let v =
        h.run("titleblockimport", json!({"dxf": dxf(&lines, &[(5.0, 30.0, 3.5, "DRAWING NO"), (95.0, 30.0, 2.5, "TITLE")]), "sheet": "A3"})).unwrap();
    let id = v["id"].as_str().unwrap().to_string();
    let body = body_of(&mut h, &id);
    assert_eq!(body["size"], json!([180.0, 40.0]));
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.iter().filter(|i| i["kind"] == "line").count(), 5);
    assert_eq!(items.iter().filter(|i| i["kind"] == "text").count(), 2);
    let big = [(0.0, 0.0, 260.0, 200.0)];
    let e = h.run("titleblockimport", json!({"dxf": dxf(&big, &[]), "sheet": "A3"})).unwrap_err();
    assert_eq!(e, "The drawing is 260 × 200 mm; the block area of A3 is 400 × 140 mm. Scale it in the DXF first.");
}

/// The texts drawn on `page` now (the kept references as shown, not a fresh derivation).
fn drawn_texts(h: &mut H, page: PageId) -> Vec<String> {
    h.activate(page);
    h.s.doc()
        .unwrap()
        .model
        .iter()
        .filter(|e| e.common.layer != frame::LAYER)
        .filter_map(|e| match &e.kind {
            EntityKind::Text(t) => Some(t.value.clone()),
            _ => None,
        })
        .collect()
}

/// The texts a fresh derivation puts on `page` (contact references and tables, brief 025).
fn fresh_reference_texts(h: &mut H, page: PageId) -> Vec<String> {
    let doc = h.doc();
    ecad_engine::references::draw::devices::parts_on_page(&doc, page)
        .into_iter()
        .filter_map(|p| match p.shape {
            ecad_electrical::references::devices::table::Shape::Text { value, .. } => Some(value),
            ecad_electrical::references::devices::table::Shape::Line(..) => None,
        })
        .collect()
}

/// Review of #110, 1: a frame decides its pages' grids (FP4), so changing the project's frame to
/// an A2 frame, or a frame's own sheet, re-traces the contact references through the update map:
/// each page shows what a fresh derivation says.
#[test]
fn references_follow_the_frames_sheet() {
    let mut h = H::new(0x055A);
    let coils = h.first_page();
    h.run("place", json!({"symbol": "coil", "at": [330, 150], "tag": "K1"})).unwrap();
    let contacts: PageId = crate::common::id(&h.run("newpage", json!({"title": "Contacts"})).unwrap(), "page");
    h.activate(contacts);
    h.run("place", json!({"symbol": "no", "at": [100, 150], "tag": "K1"})).unwrap();
    let before = fresh_reference_texts(&mut h, contacts);
    assert!(!before.is_empty(), "the contact names its coil's place");
    let check = |h: &mut H, what: &str| {
        for p in [coils, contacts] {
            let want = fresh_reference_texts(h, p);
            let shown = drawn_texts(h, p);
            for t in &want {
                assert!(shown.contains(t), "{what}, page {p:?}: {t} not drawn; drawn: {shown:?}");
            }
        }
    };
    let id = new_frame(&mut h, "A2");
    h.activate(contacts);
    h.run("setdefault", json!({"key": "print.frame.style", "level": "project", "value": id})).unwrap();
    assert_ne!(fresh_reference_texts(&mut h, contacts), before, "the A2 zones differ from the A3 ones");
    check(&mut h, "A2 frame");
    // A frame's own sheet changed in place reaches its pages too.
    h.activate(contacts);
    h.run("titleblock", json!({"id": id, "fields": {"sheet": "A1"}})).unwrap();
    check(&mut h, "A1 sheet");
    h.check();
}

/// Review of #110, 2: switching one standard for another redraws the frame.
#[test]
fn a_standard_switch_redraws_the_frame() {
    let page = PageId(0);
    let a = ecad_model::StandardHeader {
        id: ecad_model::StandardId(1),
        name: ecad_model::objects::styles::StyleName::new("A").unwrap(),
        based_on: ecad_model::objects::styles::BasedOn::Iec,
        origin: None,
    };
    let b = ecad_model::StandardHeader {
        id: ecad_model::StandardId(2),
        name: ecad_model::objects::styles::StyleName::new("B").unwrap(),
        based_on: ecad_model::objects::styles::BasedOn::Iec,
        origin: None,
    };
    let doc = ecad_model::Document::default();
    let shown = FrameInput::of(&doc, page, &Book::of(&doc));
    let op = ecad_model::Op::SetStandard { before: Some(a), after: Some(b) };
    assert!(frame::touches(&[op], page, &shown));
}

/// Review of #110, 1: the update maps of the cable references and the Devices list follow a
/// frame choice and a frame's sheet, margins or block height; a frame's weight changes none.
#[test]
fn the_update_maps_follow_frame_choices_and_frame_geometry() {
    use crate::common::{applied, devices_touched};
    use ecad_model::{ObjectRef, Op};
    let mut h = H::new(0x055B);
    let page = h.first_page();
    for y in [0.0, 5.0] {
        h.run("wire", json!({"points": [[60.0, y], [380.0, y]]})).unwrap();
    }
    let c1: ecad_model::CableId = crate::common::id(&h.run("cableslash", json!({"from": [88, -3], "to": [92, 12], "tag": "C1"})).unwrap(), "cable");
    h.run("cablereference", json!({"cable": c1.hex(), "at": [380, 5]})).unwrap();
    let coil: ecad_model::DeviceId =
        crate::common::id(&h.run("place", json!({"symbol": "coil", "at": [200, 150], "tag": "K1"})).unwrap(), "component");
    let id = new_frame(&mut h, "A2");
    let doc = h.doc();
    let choice = |object| Op::SetProperty { object, key: "print.frame.style".into(), before: None, after: Some(id.clone()) };
    // The project's frame: every cable, every place.
    assert_eq!(crate::common::cables_touched(&doc, &[choice(ObjectRef::Project)]), None);
    let ops = [choice(ObjectRef::Project)];
    assert!(devices_touched(&doc, &applied(&doc, &ops), &ops).pages);
    // A page's frame or borrowed block: that page's cables and devices.
    let borrowed = {
        let before = doc.pages()[&page].clone();
        let after = ecad_model::Page { title_block: ecad_model::StyleRef::parse(&id), ..before.clone() };
        Op::ModifyPage { id: page, before, after }
    };
    let frame = Op::SetProperty { object: ObjectRef::Page(page), key: "print.frame.style".into(), before: None, after: Some(id.clone()) };
    for (key, op) in [("print.frame.style", frame), ("title_block", borrowed)] {
        assert!(crate::common::cables_touched(&doc, std::slice::from_ref(&op)).is_some_and(|s| s.contains(&c1)), "{key}");
        let ops = [op];
        assert!(devices_touched(&doc, &applied(&doc, &ops), &ops).devices.contains(&coil), "{key}");
    }
    // A frame record: its sheet reaches its pages; its weight reaches nothing.
    let own = ecad_model::StyleRef::parse(&id).unwrap();
    let rec = doc.styles().get(&ecad_model::StyleCatalogue::Frame).and_then(|m| m.get(&own)).cloned().unwrap();
    let set = |after: ecad_model::StyleRecord| Op::SetStyle {
        layer: ecad_model::StyleLayer::Project,
        catalogue: ecad_model::StyleCatalogue::Frame,
        id: own.clone(),
        before: Some(rec.clone()),
        after: Some(after),
    };
    let mut heavier = rec.clone();
    ecad_electrical::styles::frame::set(frame_of(&mut heavier), "border_weight", Some("0.7")).unwrap();
    assert_eq!(crate::common::cables_touched(&doc, &[set(heavier)]), Some(Default::default()), "a weight moves no reference");
    let mut a1 = rec.clone();
    ecad_electrical::styles::frame::set(frame_of(&mut a1), "sheet", Some("A1")).unwrap();
    let mut used = doc.clone();
    used.edit_parts(|parts| {
        parts.display.insert("print.frame.style".into(), id.clone());
    });
    assert!(crate::common::cables_touched(&used, &[set(a1)]).is_some_and(|s| s.contains(&c1)), "its pages' cables");
}

/// Tester 055 (§12.1 J, §1.3): a title block whose size is past the bounds opens, keeps the
/// built-in's strip, and is reported in Checks.
#[test]
fn an_out_of_bounds_block_size_is_reported() {
    let mut doc = ecad_model::Document::default();
    let title_block = Some(TitleBlockLayout { size: (1e308, 1e308), items: Vec::new() });
    let rec = ecad_model::StyleRecord::new(
        "Huge",
        ecad_model::objects::styles::StyleBody::Frame(ecad_model::objects::styles::FrameStyle { title_block, ..Default::default() }),
    );
    doc.edit_parts(|parts| {
        parts.styles.entry(ecad_model::StyleCatalogue::Frame).or_default().insert(ecad_model::StyleRef::Own(5), rec);
    });
    let rows = ecad_electrical::title_blocks::checks::rows(&doc);
    assert!(rows.iter().any(|r| r.which.contains("Huge's title block size is out of bounds")), "{rows:?}");
}

fn frame_of(r: &mut ecad_model::StyleRecord) -> &mut ecad_model::objects::styles::FrameStyle {
    match &mut r.body {
        ecad_model::objects::styles::StyleBody::Frame(f) => f,
        other => panic!("not a frame: {other:?}"),
    }
}
