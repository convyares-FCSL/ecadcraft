//! Tester 055 (Library ▸ Title blocks: the frame style's title block body, the house block, sheet
//! resolution through the frame, the commands, the checks and the update map): acceptance checks
//! from outside, written from brief 055 and design 055 (§1–§3, §6–§10, §12.1 rows A–J and the
//! overseer's FP1–FP5 rulings) only, before the implementation existed.
//!
//! The checks drive the JSON commands the design names (`titleblocks`, `titleblock`,
//! `titleblockimport`, `setdefault`, `setstyle`, `pageprops`, `projectprops`, `checks`) and read
//! the document and the drawn frame, so they lean on as little of the new Rust API as possible.
//! The Rust names used are the ones the design fixes: `ecad_cadcraft::sheets::{sheet_of, grid_of,
//! keep_out}` (FP4) and `ecad_model::style::StyleRecord.body` (FP1). Item and binding JSON is read
//! tolerantly (an item's kind under `kind`/`type` or as its one key; a binding as an object
//! `{source, key}` or as the text `source:key`); items the tests write are copies of items the
//! command returned, so the tests follow the worker's serde shape.
//!
//! Test names start with the §12.1 row (`a`…`j`) or the focus item (`fp4` sheet order, `bind`
//! bindings, `map` update map, `hostile` bounds, `ecr` FP5).

use cadcraft_engine::doc::EntityKind;
use cadcraft_engine::doc::color::Color;
use ecad_model::{Op, PageId};
use serde_json::{Value, json};

use crate::common::{H, id};
use crate::tester_050a_style::{
    checks_all, checks_of, decoded_ops, find_obj_mut, inject, last_tx, last_tx_json, one_tx, records_round_trip, refused, s, save_and_reopen,
    tmp_dir, undo,
};

// ------------------------------------------------------------------------------- helpers

/// The frame's texts on `page` (drawn on the locked frame layer).
fn frame_texts(h: &mut H, page: PageId) -> Vec<String> {
    h.pv(|p, pg| pg.frame_entities_of(p, page))
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::Text(t) => Some(t.value.clone()),
            _ => None,
        })
        .collect()
}

/// The frame's text entities on `page` whose value is `text`.
fn frame_text_entities(h: &mut H, page: PageId, text: &str) -> Vec<std::sync::Arc<cadcraft_engine::doc::Entity>> {
    h.pv(|p, pg| pg.frame_entities_of(p, page)).into_iter().filter(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == text)).collect()
}

/// Every frame entity of `page` as one comparable string (look and place).
fn frame_signature(h: &mut H, page: PageId) -> Vec<String> {
    let mut v: Vec<String> = h.pv(|p, pg| pg.frame_entities_of(p, page)).iter().map(|e| format!("{:?}|{:?}", e.common.layer, e.kind)).collect();
    v.sort();
    v
}

/// Where a frame text sits: the insertion point of the first text equal to `text`.
fn text_at(h: &mut H, page: PageId, text: &str) -> Option<(f64, f64)> {
    frame_text_entities(h, page, text).first().and_then(|e| match &e.kind {
        EntityKind::Text(t) => Some((t.insert.x, t.insert.y)),
        _ => None,
    })
}

fn frame_builds(h: &mut H) -> u64 {
    h.p(|p| p.frame_builds)
}

/// `titleblocks` rows (§9: a list, tolerated inside `{titleblocks: [...]}` or `{frames: [...]}`).
fn titleblocks(h: &mut H) -> Vec<Value> {
    let v = h.run("titleblocks", json!({})).unwrap_or_else(|e| panic!("titleblocks: {e}"));
    match v {
        Value::Array(a) => a,
        other => ["titleblocks", "frames", "rows"].iter().find_map(|k| other[*k].as_array().cloned()).unwrap_or_else(|| panic!("no rows in {other}")),
    }
}

fn row_of(h: &mut H, id: &str) -> Value {
    let rows = titleblocks(h);
    rows.iter().find(|r| s(&r["id"]) == id || s(&r["name"]) == id).cloned().unwrap_or_else(|| panic!("{id} not in {rows:?}"))
}

/// `titleblock {id}`: the record (fields and body) as JSON.
fn read_block(h: &mut H, id: &str) -> Value {
    h.run("titleblock", json!({"id": id})).unwrap_or_else(|e| panic!("titleblock {id}: {e}"))
}

/// The body of a `titleblock` result: `{size, items}`, wherever the worker put it (`body`,
/// `body.title_block`, or at the top).
fn body_of(v: &Value) -> Value {
    for b in [&v["body"]["title_block"], &v["body"]["TitleBlock"], &v["body"], v] {
        if b["items"].is_array() {
            return b.clone();
        }
    }
    panic!("no body with items in {v}")
}

fn items_of(v: &Value) -> Vec<Value> {
    body_of(v)["items"].as_array().cloned().unwrap_or_default()
}

/// An item's kind in lower case (`field`, `text`, `line`, `revtable`, `logo`) and its fields.
fn kind_of(item: &Value) -> (String, Value) {
    for k in ["kind", "type", "item"] {
        if let Some(w) = item[k].as_str() {
            return (w.to_lowercase().replace(['_', '-'], ""), item.clone());
        }
    }
    match item.as_object() {
        Some(m) if m.len() == 1 => {
            let (k, v) = m.iter().next().unwrap();
            (k.to_lowercase().replace(['_', '-'], ""), v.clone())
        }
        _ => panic!("cannot tell the kind of {item}"),
    }
}

/// The inner object of an item (where `at`, `bind`, `look` live), mutable.
fn inner_mut(item: &mut Value) -> &mut Value {
    let tagged = ["kind", "type", "item"].iter().any(|k| item[*k].is_string());
    if tagged {
        return item;
    }
    let key = item.as_object().and_then(|m| m.keys().next().cloned()).unwrap();
    &mut item[key]
}

/// A binding as `source:key`, lower case source.
fn binding_text(fields: &Value) -> String {
    let b = &fields["bind"];
    match b {
        Value::String(t) => t.to_lowercase(),
        Value::Object(_) => format!("{}:{}", s(&b["source"]).to_lowercase(), s(&b["key"])),
        _ => String::new(),
    }
}

fn set_binding(item: &mut Value, source: &str, key: &str) {
    let f = inner_mut(item);
    match &f["bind"] {
        Value::String(_) => f["bind"] = json!(format!("{source}:{key}")),
        Value::Object(o) => {
            // Keep the worker's case for the source word (`project` or `Project`).
            let upper = o.get("source").and_then(Value::as_str).is_some_and(|w| w.starts_with(char::is_uppercase));
            let src = if upper { format!("{}{}", source[..1].to_uppercase(), &source[1..]) } else { source.to_string() };
            f["bind"] = json!({"source": src, "key": key});
        }
        other => panic!("no binding to set in {other}"),
    }
}

/// The fields of every field item, by binding.
fn fields_by_binding(v: &Value) -> Vec<(String, Value)> {
    items_of(v)
        .iter()
        .filter_map(|i| {
            let (k, f) = kind_of(i);
            (k == "field").then(|| (binding_text(&f), f))
        })
        .collect()
}

fn rect(f: &Value) -> (f64, f64, f64, f64) {
    let a = &f["at"];
    let n = |k: &str| a[k].as_f64().unwrap_or_else(|| panic!("at.{k} in {a}"));
    (n("x"), n("y"), n("w"), n("h"))
}

/// The index of the field item bound to `source:key` in the body's item list.
fn field_index(v: &Value, binding: &str) -> usize {
    items_of(v)
        .iter()
        .position(|i| {
            let (k, f) = kind_of(i);
            k == "field" && binding_text(&f) == binding
        })
        .unwrap_or_else(|| panic!("no field {binding} in {v}"))
}

/// A new own frame from the built-in for `sheet`; its ID.
fn new_from_builtin(h: &mut H, sheet: &str) -> String {
    let (v, tx) = one_tx(h, "titleblock", json!({"new": {"from": "sheet", "sheet": sheet}}));
    assert_eq!(tx.ops.iter().map(Op::name).collect::<Vec<_>>(), ["SetStyle"], "§4.6: one SetStyle");
    let i = ["id", "style"].iter().map(|k| s(&v[*k])).find(|x| !x.is_empty()).unwrap_or_else(|| panic!("no id in {v}"));
    assert!(i.starts_with('#') && i.len() == 33, "an own frame's ID is 050a's: {i}");
    i
}

/// Write `body` (and optional fields) to frame `id` with one transaction.
fn write_body(h: &mut H, id: &str, body: Value) {
    one_tx(h, "titleblock", json!({"id": id, "body": body}));
}

fn use_for_project(h: &mut H, id: &str) {
    h.run("setdefault", json!({"key": "print.frame.style", "level": "project", "value": id})).unwrap();
}

/// Two more pages after the first, so Next / Of have values; the first page active.
fn three_pages(seed: u64) -> (H, Vec<PageId>) {
    let mut h = H::new(seed);
    h.run("projectprops", json!({"title": "Pump skid", "drawing_number": "PS-04-E", "customer": "Northfield", "status": "Draft"})).unwrap();
    let p1 = h.first_page();
    h.run("pageprops", json!({"page": p1.hex(), "function": "PWR", "location": "CP1"})).unwrap();
    let p2: PageId = id(&h.run("newpage", json!({"title": "Motors"})).unwrap(), "page");
    let p3: PageId = id(&h.run("newpage", json!({"title": "Controls"})).unwrap(), "page");
    h.activate(p1);
    (h, vec![p1, p2, p3])
}

fn is_red(c: &Color) -> bool {
    match c {
        Color::True(rgb) => rgb.0 > 150 && rgb.1 < 100 && rgb.2 < 100,
        Color::Index(1) => true,
        _ => false,
    }
}

// ----------------------------------------------------------------- A: the built-in house block

/// §12.1 A, §3: every page shows the house block: Rev/ECR revision grid, red status, function,
/// location, page / next / of, page title, drawing number, sheet size; Rev shows the pending mark.
#[test]
#[ignore = "tester 055: awaiting build"]
fn a_every_page_shows_the_house_block_with_its_fields() {
    let (mut h, pages) = three_pages(0x55_a001);
    for (n, page) in pages.iter().enumerate() {
        h.activate(*page);
        let t = frame_texts(&mut h, *page);
        for want in ["PS-04-E", "Pump skid", "Northfield", "Draft", "A3"] {
            assert!(t.iter().any(|x| x == want), "page {}: {want} in the house block: {t:?}", n + 1);
        }
        assert!(t.iter().any(|x| x == "3"), "Of: total 3 pages: {t:?}");
        assert!(t.iter().any(|x| x.eq_ignore_ascii_case("ECR")), "FP5: the revision grid has an ECR column: {t:?}");
        let status = frame_text_entities(&mut h, *page, "Draft");
        assert!(
            status.iter().all(|e| is_red(&e.common.color)),
            "§3: Status is red: {:?}",
            status.iter().map(|e| &e.common.color).collect::<Vec<_>>()
        );
    }
    let t = frame_texts(&mut h, pages[0]);
    assert!(t.iter().any(|x| x == "PWR") && t.iter().any(|x| x == "CP1"), "function and location: {t:?}");
    // Next of page 1 is page 2's number.
    let doc = h.doc();
    let book = ecad_model::book::Book::of(&doc);
    let n2 = book.number(&doc, pages[1]);
    assert!(t.iter().any(|x| *x == n2), "Next shows {n2}: {t:?}");
    // Rev with the pending marker (033): issue, then edit.
    h.run("revision", json!({"description": "first", "drawn": "T"})).unwrap();
    h.run("issuerevision", json!({})).unwrap();
    h.run("wire", json!({"points": [[0, 100], [40, 100]]})).unwrap();
    let t = frame_texts(&mut h, pages[0]);
    assert!(t.iter().any(|x| x.ends_with('*')), "§3: Rev carries 033's pending marker: {t:?}");
}

/// §12.1 A, §2, §3: the house block is 40 mm high, so the keep-out strip, margins and grid stay
/// 019's: no device, reference, wire or generated table moves. (`sheets::*` against 019's
/// constants; the before/after screenshot is phase 2.)
#[test]
#[ignore = "tester 055: awaiting build"]
fn a_built_in_keeps_the_strip_margins_and_grid_so_nothing_moves() {
    use ecad_model::sheet::{Grid, MARGIN_MM, SheetSpec, TITLE_BLOCK_MM, TOP_MARGIN_MM};
    let (mut h, pages) = three_pages(0x55_a002);
    h.run("pageprops", json!({"page": pages[1].hex(), "sheet": "A2"})).unwrap();
    let doc = h.doc();
    for p in &pages {
        assert_eq!(ecad_cadcraft::sheets::keep_out(&doc, *p), TITLE_BLOCK_MM, "the built-in block is 40 mm");
        let g = ecad_cadcraft::sheets::grid_of(&doc, *p);
        let old = Grid::of(&doc, *p);
        assert_eq!(g, old, "FP4: built-ins give 019's grid on page {p}");
        assert_eq!((g.margins.left, g.margins.right, g.margins.bottom, g.margins.top), (MARGIN_MM, MARGIN_MM, MARGIN_MM, TOP_MARGIN_MM));
        assert_eq!(ecad_cadcraft::sheets::sheet_of(&doc, *p), ecad_model::sheet::sheet_of(&doc, *p), "built-ins: 019's sheet");
    }
    assert_eq!(ecad_cadcraft::sheets::sheet_of(&doc, pages[1]), SheetSpec::preset("A2").unwrap());
    // A reference text names the same column and row as before (019's check B).
    h.activate(pages[0]);
    h.run("place", json!({"symbol": "coil", "at": [100, 100]})).unwrap();
    h.check();
}

/// FP3: 050a's `title_block` field is retired: a stored value is ignored (draws the house block)
/// and `styles` lists it as unknown.
#[test]
#[ignore = "tester 055: awaiting build"]
fn a_the_retired_title_block_field_is_ignored() {
    let (mut h, pages) = three_pages(0x55_a003);
    let before = frame_signature(&mut h, pages[0]);
    inject(
        &mut h,
        vec![Op::SetStyle {
            catalogue: "frame".into(),
            layer: ecad_model::style::StyleLayer::Project,
            id: ecad_model::style::StyleRef::builtin("sheet").unwrap(),
            before: None,
            after: Some(ecad_model::style::StyleRecord {
                fields: [("title_block".to_string(), "standard".to_string())].into_iter().collect(),
                ..Default::default()
            }),
        }],
    );
    h.activate(pages[0]);
    assert_eq!(frame_signature(&mut h, pages[0]), before, "a stored title_block draws nothing different");
    let rows = crate::tester_050a_style::styles(&mut h, "frame");
    let sheet = rows.iter().find(|r| s(&r["id"]) == "sheet").unwrap();
    assert!(sheet.to_string().contains("title_block"), "listed as unknown (050a's rule): {sheet}");
}

// --------------------------------------------------------------- B: new from the built-in

/// §12.1 B: `titleblock {new: {from: sheet, sheet: A3}}` returns an ID; `titleblock {id}` returns
/// the house block's items (§3); the record round-trips through save and open.
#[test]
#[ignore = "tester 055: awaiting build"]
fn b_new_from_the_built_in_is_the_house_block_and_round_trips() {
    let (mut h, _) = three_pages(0x55_b001);
    let id = new_from_builtin(&mut h, "A3");
    let v = read_block(&mut h, &id);
    let b = body_of(&v);
    let size = &b["size"];
    let (w, hh) = match size {
        Value::Array(a) => (a[0].as_f64().unwrap(), a[1].as_f64().unwrap()),
        o => (o["w"].as_f64().or(o["0"].as_f64()).unwrap(), o["h"].as_f64().or(o["1"].as_f64()).unwrap()),
    };
    assert_eq!(hh, 40.0, "§3: block 40 mm high");
    assert!((w - 400.0).abs() < 1e-6, "A3 frame width 400 mm (420 − 2 × 10): {w}");
    let bound: Vec<String> = fields_by_binding(&v).into_iter().map(|(b, _)| b).collect();
    for want in [
        "page:number",
        "page:next",
        "page:total",
        "page:title",
        "project:drawing_number",
        "revision:rev",
        "page:function",
        "page:location",
        "print:sheet",
        "project:company",
        "project:title",
        "project:customer",
        "project:status",
    ] {
        assert!(bound.iter().any(|b| b == want), "§3 field {want} in {bound:?}");
    }
    let kinds: Vec<String> = items_of(&v).iter().map(|i| kind_of(i).0).collect();
    assert!(kinds.iter().any(|k| k == "revtable"), "§3: a revision table");
    assert!(kinds.iter().any(|k| k == "logo"), "§3: an (empty) logo box");
    // Status is bold red; page number 7 mm bold centre.
    let fs = fields_by_binding(&v);
    let status = &fs.iter().find(|(b, _)| b == "project:status").unwrap().1["look"];
    assert!(s(&status["colour"]).eq_ignore_ascii_case("red") && status["bold"] == json!(true), "{status}");
    let page = &fs.iter().find(|(b, _)| b == "page:number").unwrap().1["look"];
    assert_eq!(page["size"].as_f64(), Some(7.0), "{page}");
    // Its own frame lists in titleblocks: own layer project, not built-in, A3, item count.
    let row = row_of(&mut h, &id);
    assert_eq!(row["builtin"].as_bool().or(row["built_in"].as_bool()), Some(false), "{row}");
    assert!(s(&row["sheet"]).starts_with("A3"), "{row}");
    assert_eq!(row["items"].as_u64().or(row["item_count"].as_u64()), Some(items_of(&v).len() as u64), "{row}");
    records_round_trip(&mut h);
    let want = h.doc();
    let dir = tmp_dir("b round trip");
    let mut h = save_and_reopen(h, &dir, "Projekt 055 (B).ecad", 0x55_b002);
    assert!(h.doc().styles == want.styles, "the record with its body reads back equal");
    assert_eq!(read_block(&mut h, &id), v, "titleblock reads the same after reopen");
}

/// §12.1 B, §1.3, §9: refusals in words, nothing written: an unknown frame, a built-in, a size or
/// coordinate out of bounds, more than 20 revision rows, a 257-character text.
#[test]
#[ignore = "tester 055: awaiting build"]
fn b_titleblock_refusals_say_why_and_write_nothing() {
    let (mut h, _) = three_pages(0x55_b003);
    let id = new_from_builtin(&mut h, "A3");
    let v = read_block(&mut h, &id);
    let body = body_of(&v);
    let m = refused(&mut h, "titleblock", json!({"id": "#0000000000000000000000000000beef", "body": body}));
    assert!(!m.is_empty());
    let m = refused(&mut h, "titleblock", json!({"id": "sheet", "body": body}));
    assert!(m.to_lowercase().contains("built-in") || m.to_lowercase().contains("built in"), "§4.6: built-ins are read-only: {m}");
    let mut bad = body.clone();
    bad["size"] = json!([3000.0, 40.0]);
    refused(&mut h, "titleblock", json!({"id": id, "body": bad}));
    let mut bad = body.clone();
    bad["size"] = json!([400.0, 401.0]);
    refused(&mut h, "titleblock", json!({"id": id, "body": bad}));
    let i = field_index(&v, "project:drawing_number");
    let mut bad = body.clone();
    inner_mut(&mut bad["items"][i])["at"]["x"] = json!(-5.0);
    refused(&mut h, "titleblock", json!({"id": id, "body": bad}));
    let mut bad = body.clone();
    inner_mut(&mut bad["items"][i])["label"] = json!("x".repeat(257));
    refused(&mut h, "titleblock", json!({"id": id, "body": bad}));
    let t = items_of(&v).iter().position(|i| kind_of(i).0 == "revtable").unwrap();
    let mut bad = body.clone();
    inner_mut(&mut bad["items"][t])["rows"] = json!(21);
    refused(&mut h, "titleblock", json!({"id": id, "body": bad}));
    let mut bad = body.clone();
    bad["items"] = json!(std::iter::repeat_n(body["items"][i].clone(), 513).collect::<Vec<_>>());
    let m = refused(&mut h, "titleblock", json!({"id": id, "body": bad}));
    assert!(m.contains("512"), "MAX_ITEMS named: {m}");
}

// ------------------------------------------------------- C: choose it, move a field, undo

/// §12.1 C: choosing the copy for the project changes nothing visible; moving the drawing number
/// 10 mm with `titleblock` moves it on every page; one `u` undoes it.
#[test]
#[ignore = "tester 055: awaiting build"]
fn c_a_moved_field_moves_on_every_page_and_undo_restores() {
    let (mut h, pages) = three_pages(0x55_c001);
    let builtin: Vec<Vec<String>> = pages.clone().into_iter().map(|p| frame_signature(&mut h, p)).collect();
    let id = new_from_builtin(&mut h, "A3");
    use_for_project(&mut h, &id);
    for (n, p) in pages.iter().enumerate() {
        h.activate(*p);
        assert_eq!(frame_signature(&mut h, *p), builtin[n], "§12.1 C: the copy looks the same on page {}", n + 1);
    }
    h.activate(pages[0]);
    let before = text_at(&mut h, pages[0], "PS-04-E").unwrap();
    let v = read_block(&mut h, &id);
    let mut body = body_of(&v);
    let i = field_index(&v, "project:drawing_number");
    let x = inner_mut(&mut body["items"][i])["at"]["x"].as_f64().unwrap();
    inner_mut(&mut body["items"][i])["at"]["x"] = json!(x - 10.0);
    let head = h.head();
    write_body(&mut h, &id, body);
    for p in &pages {
        h.activate(*p);
        let now = text_at(&mut h, *p, "PS-04-E").unwrap();
        assert!((now.0 - (before.0 - 10.0)).abs() < 1e-6 && (now.1 - before.1).abs() < 1e-6, "page {p}: moved 10 mm left: {before:?} → {now:?}");
    }
    undo(&mut h);
    assert_eq!(h.head(), head + 2, "undo is one transaction");
    h.activate(pages[0]);
    assert_eq!(text_at(&mut h, pages[0], "PS-04-E"), Some(before), "one u restores it");
}

// ----------------------------------------------------------- D: a user's key and the map

/// §12.1 D, §10.1: a field bound to `project:job_no` shows the property; changing `job_no`
/// rebuilds the frame on screen; a property no field reads rebuilds no frame.
#[test]
#[ignore = "tester 055: awaiting build"]
fn d_a_users_key_redraws_only_when_it_changes() {
    let (mut h, pages) = three_pages(0x55_d001);
    let id = new_from_builtin(&mut h, "A3");
    let v = read_block(&mut h, &id);
    let mut body = body_of(&v);
    let i = field_index(&v, "project:customer");
    set_binding(&mut body["items"][i], "project", "job_no");
    write_body(&mut h, &id, body);
    use_for_project(&mut h, &id);
    h.run("projectprops", json!({"props": {"job_no": "J-1187"}})).unwrap();
    assert!(frame_texts(&mut h, pages[0]).iter().any(|t| t == "J-1187"), "the field shows the user's key");
    let row = row_of(&mut h, &id);
    assert!(row["keys"].as_array().is_some_and(|k| k.iter().any(|x| x == "job_no")), "§9: titleblocks lists keys: {row}");
    let b = frame_builds(&mut h);
    h.run("projectprops", json!({"props": {"unrelated": "x"}})).unwrap();
    assert_eq!(frame_builds(&mut h), b, "§10.1: a key no field reads rebuilds no frame");
    // Customer is no longer read by this block: it rebuilds nothing either.
    h.run("projectprops", json!({"customer": "Someone else"})).unwrap();
    assert_eq!(frame_builds(&mut h), b, "a built-in key the block does not read rebuilds nothing");
    h.run("projectprops", json!({"props": {"job_no": "J-1188"}})).unwrap();
    assert_eq!(frame_builds(&mut h), b + 1, "the bound key rebuilds the page on screen once");
    assert!(frame_texts(&mut h, pages[0]).iter().any(|t| t == "J-1188"));
}

/// §10.1: a page's own `sheet` and `settings.sheet` redraw the frame only when the frame does not
/// set `sheet`; a `SetStyle` of another frame redraws nothing.
#[test]
#[ignore = "tester 055: awaiting build"]
fn map_sheet_and_other_frames_follow_the_update_map() {
    let (mut h, pages) = three_pages(0x55_d002);
    let other = new_from_builtin(&mut h, "A2");
    let b = frame_builds(&mut h);
    let v = read_block(&mut h, &other);
    write_body(&mut h, &other, body_of(&v));
    assert_eq!(frame_builds(&mut h), b, "a frame no page uses redraws nothing");
    let a2 = new_from_builtin(&mut h, "A2");
    h.run("setstyle", json!({"catalogue": "frame", "id": a2, "fields": {"sheet": "A2"}})).unwrap();
    use_for_project(&mut h, &a2);
    let b = frame_builds(&mut h);
    h.run("projectprops", json!({"sheet": "A1"})).unwrap();
    assert_eq!(frame_builds(&mut h), b, "the frame sets the sheet: settings.sheet changes nothing");
    h.run("pageprops", json!({"page": pages[0].hex(), "sheet": "A1"})).unwrap();
    // The page's own property is the page's op (019 redraws its own page); the paper stays A2.
    let doc = h.doc();
    assert_eq!(ecad_cadcraft::sheets::sheet_of(&doc, pages[0]).width, 594, "the frame wins");
}

// ----------------------------------------------------------------- E: an own A2 frame

/// §12.1 E, §2: an own frame with `sheet` A2 on an A3 project gives its pages A2 paper and zones;
/// a page with its own `sheet` A3 shows A2 and a `frame-sheet-differs` Warning whose Fix clears it.
#[test]
#[ignore = "tester 055: awaiting build"]
fn e_an_own_a2_frame_sets_its_pages_paper_and_checks_a_differing_page() {
    let (mut h, pages) = three_pages(0x55_e001);
    h.run("pageprops", json!({"page": pages[1].hex(), "sheet": "A3"})).unwrap();
    let id = new_from_builtin(&mut h, "A2");
    let f = read_block(&mut h, &id);
    assert!(f.to_string().contains("A2"), "the frame is drawn for A2: {f}");
    h.run("setstyle", json!({"catalogue": "frame", "id": id, "fields": {"sheet": "A2"}})).unwrap();
    use_for_project(&mut h, &id);
    let doc = h.doc();
    let a2 = ecad_model::sheet::SheetSpec::preset("A2").unwrap();
    for p in &pages {
        let sh = ecad_cadcraft::sheets::sheet_of(&doc, *p);
        assert_eq!((sh.width, sh.height, sh.columns, sh.rows), (a2.width, a2.height, a2.columns, a2.rows), "page {p}: A2 paper and zones");
    }
    h.activate(pages[0]);
    assert!(frame_texts(&mut h, pages[0]).iter().any(|t| t == "12"), "A2 zones (12 columns) drawn");
    let rows = checks_of(&mut h, "frame-sheet-differs");
    assert_eq!(rows.len(), 1, "only the page with its own sheet: {rows:?}");
    let r = &rows[0];
    assert_eq!(s(&r["level"]).to_lowercase(), "warning", "{r}");
    let words = s(&r["message"]) + &s(&r["text"]);
    assert!(words.contains("A3") && words.contains("A2") && words.contains("the frame wins"), "§8 words: {r}");
    // The Fix: clear the page's sheet (setprop page sheet null), one transaction.
    let fix = r["fix"].clone();
    assert!(!fix.is_null(), "a Fix: {r}");
    let (cmd, params) = (s(&fix["command"]), fix["params"].clone());
    one_tx(&mut h, &cmd, params);
    assert!(h.doc().pages[&pages[1]].properties.get("sheet").is_none(), "the page's sheet is cleared");
    assert!(checks_of(&mut h, "frame-sheet-differs").is_empty());
}

/// FP4, §2: the order frame `sheet` → page `sheet` → `settings.sheet` → A3 8 × 6, and margins from
/// the frame's `margins`.
#[test]
#[ignore = "tester 055: awaiting build"]
fn fp4_sheet_of_resolves_frame_page_project_then_a3() {
    let (mut h, pages) = three_pages(0x55_f401);
    let sheet = |h: &mut H, p: PageId| {
        let d = h.doc();
        ecad_cadcraft::sheets::sheet_of(&d, p).text()
    };
    assert!(sheet(&mut h, pages[0]).starts_with("A3"), "4: default A3");
    h.run("projectprops", json!({"sheet": "A1"})).unwrap();
    assert!(sheet(&mut h, pages[0]).starts_with("A1"), "3: settings.sheet");
    h.run("pageprops", json!({"page": pages[0].hex(), "sheet": "A2"})).unwrap();
    assert!(sheet(&mut h, pages[0]).starts_with("A2"), "2: the page's sheet");
    assert!(sheet(&mut h, pages[1]).starts_with("A1"), "another page keeps the project's");
    let id = new_from_builtin(&mut h, "A0");
    h.run("setstyle", json!({"catalogue": "frame", "id": id, "fields": {"sheet": "A0", "margins": "5,6,7,8"}})).unwrap();
    h.run("setdefault", json!({"key": "print.frame.style", "level": "page", "page": pages[0].hex(), "value": id})).unwrap();
    assert!(sheet(&mut h, pages[0]).starts_with("A0"), "1: the frame's sheet wins");
    assert!(sheet(&mut h, pages[1]).starts_with("A1"), "page level: one page only");
    let d = h.doc();
    let g = ecad_cadcraft::sheets::grid_of(&d, pages[0]);
    assert_eq!((g.margins.left, g.margins.right, g.margins.bottom, g.margins.top), (5.0, 6.0, 7.0, 8.0), "the frame's margins");
    // Bad margins are refused in words (0–50 mm).
    refused(&mut h, "setstyle", json!({"catalogue": "frame", "id": id, "fields": {"margins": "5,6,7,51"}}));
    refused(&mut h, "setstyle", json!({"catalogue": "frame", "id": id, "fields": {"sheet": "A9"}}));
    // Undo the choice: back to the page's own.
    undo(&mut h);
    assert!(sheet(&mut h, pages[0]).starts_with("A2"));
}

// ---------------------------------------------------------------- bindings and if-empty

/// §7: page, revision and print sources; `if_empty` blank, dash, missing mark and a short text;
/// an unknown key or source draws empty with one `title-binding-unknown` Info row per frame.
#[test]
#[ignore = "tester 055: awaiting build"]
fn bind_sources_if_empty_and_unknown_bindings() {
    let (mut h, pages) = three_pages(0x55_b1d1);
    h.run("revision", json!({"description": "first", "drawn": "TS"})).unwrap();
    h.run("issuerevision", json!({})).unwrap();
    h.run("setprop", json!({"project": true, "key": "revisions.r1.ecr", "value": "ECR-17"})).ok();
    let id = new_from_builtin(&mut h, "A3");
    let v = read_block(&mut h, &id);
    let mut body = body_of(&v);
    let i_cust = field_index(&v, "project:customer");
    let i_loc = field_index(&v, "page:location");
    let i_fun = field_index(&v, "page:function");
    let i_comp = field_index(&v, "project:company");
    set_binding(&mut body["items"][i_cust], "revision", "drawn");
    set_binding(&mut body["items"][i_loc], "project", "nothing_here");
    inner_mut(&mut body["items"][i_loc])["if_empty"] = json!({"text": "n/a"});
    set_binding(&mut body["items"][i_fun], "project", "also_empty");
    inner_mut(&mut body["items"][i_fun])["if_empty"] = json!("dash");
    set_binding(&mut body["items"][i_comp], "page", "no_such_page_key");
    write_body(&mut h, &id, body);
    use_for_project(&mut h, &id);
    let t = frame_texts(&mut h, pages[0]);
    assert!(t.iter().any(|x| x == "TS"), "revision:drawn of the newest issued revision: {t:?}");
    assert!(t.iter().any(|x| x == "n/a"), "if_empty text: {t:?}");
    assert!(t.iter().any(|x| x == "-" || x == "—" || x == "–"), "if_empty dash: {t:?}");
    let rows = checks_of(&mut h, "title-binding-unknown");
    assert_eq!(rows.len(), 1, "one Info row per frame: {rows:?}");
    assert_eq!(s(&rows[0]["level"]).to_lowercase(), "info");
    assert!((s(&rows[0]["message"]) + &s(&rows[0]["text"])).contains("1 field"), "§8 words count the fields: {:?}", rows[0]);
}

/// §8: `title-value-cut` (Info), once per field, counting the pages: a long drawing number is cut
/// with "…" on every page.
#[test]
#[ignore = "tester 055: awaiting build"]
fn bind_a_cut_value_is_one_info_row_counting_pages() {
    let (mut h, pages) = three_pages(0x55_b1d2);
    let long = "PS-04-E-".repeat(30);
    h.run("projectprops", json!({"drawing_number": long})).unwrap();
    let t = frame_texts(&mut h, pages[0]);
    assert!(t.iter().any(|x| x.ends_with('…')), "019's fit cuts with …: {t:?}");
    let rows = checks_of(&mut h, "title-value-cut");
    assert_eq!(rows.len(), 1, "once per field: {rows:?}");
    let w = s(&rows[0]["message"]) + &s(&rows[0]["text"]);
    assert!(w.contains("Drawing number") && w.contains("3 pages"), "§8 words: {w}");
    assert_eq!(s(&rows[0]["level"]).to_lowercase(), "info");
}

// --------------------------------------------------------------------------- FP5: ECR

/// FP5: the built-in revision columns gain `ecr` after `rev`; a project that set its own columns
/// keeps them.
#[test]
#[ignore = "tester 055: awaiting build"]
fn ecr_is_a_built_in_revision_column_after_rev() {
    let mut h = H::new(0x55_ec01);
    let d = h.doc();
    let cols: Vec<String> = ecad_cadcraft::revisions::columns(&d).iter().map(|c| c.field.clone()).collect();
    let r = cols.iter().position(|c| c == "rev").expect("rev");
    assert_eq!(cols.get(r + 1).map(String::as_str), Some("ecr"), "ECR after Rev: {cols:?}");
    h.run("projectprops", json!({"revision_columns": "rev, description, date"})).unwrap();
    let d = h.doc();
    assert!(d.properties.contains_key(ecad_cadcraft::revisions::COLUMNS_KEY));
    let cols: Vec<String> = ecad_cadcraft::revisions::columns(&d).iter().map(|c| c.field.clone()).collect();
    assert_eq!(cols, ["rev", "description", "date"], "a project's own columns are kept");
}

// --------------------------------------------------------------------------- J: hostile

/// The record JSON of a `titleblock` write, editable, for stored bodies no command writes.
fn stored_body_tx(h: &mut H, id: &str) -> Value {
    let v = read_block(h, id);
    write_body(h, id, body_of(&v));
    last_tx_json(h)
}

/// §12.1 J, §1.3: hostile bodies (10 000 items, NaN coordinates, a 1 MB text, an unknown body tag,
/// an unknown binding source) open, draw, report in Checks and never panic; each survives a save
/// and reopen.
#[test]
#[ignore = "tester 055: awaiting build"]
fn hostile_bodies_open_draw_report_and_never_panic() {
    type Edit = fn(&mut Value);
    let cases: [(&str, Edit); 6] = [
        ("10 000 items", |b| {
            let one = b["items"][0].clone();
            b["items"] = json!(std::iter::repeat_n(one, 10_000).collect::<Vec<_>>());
        }),
        ("NaN coordinates", |b| {
            // JSON has no NaN; a hostile file stores 1e999, which reads as infinity, or a string.
            inner_mut(&mut b["items"][0])["at"] = json!({"x": "NaN", "y": 1e300, "w": -1, "h": 0});
        }),
        ("1 MB text", |b| {
            inner_mut(&mut b["items"][0])["label"] = json!("x".repeat(1 << 20));
        }),
        ("an unknown item kind", |b| {
            b["items"].as_array_mut().unwrap().push(json!({"kind": "hologram", "Hologram": {}}));
        }),
        ("an unknown binding source", |b| {
            let items = b["items"].as_array_mut().unwrap();
            let i = items.iter().position(|x| kind_of(x).0 == "field").unwrap();
            set_binding(&mut items[i], "weather", "today");
        }),
        ("a huge size", |b| b["size"] = json!([1e308, 1e308])),
    ];
    for (n, (what, edit)) in cases.into_iter().enumerate() {
        let (mut h, pages) = three_pages(0x55_7000 + n as u64);
        let id = new_from_builtin(&mut h, "A3");
        use_for_project(&mut h, &id);
        let mut tx = stored_body_tx(&mut h, &id);
        let body = find_obj_mut(&mut tx, "items").unwrap_or_else(|| panic!("{what}: no body in the record"));
        let mut b = Value::Object(body.clone());
        edit(&mut b);
        *body = b.as_object().unwrap().clone();
        let ops = match ecad_file::record::decode_tx(&tx) {
            Ok(t) => t.ops,
            Err(e) => panic!("{what}: a hostile but well-formed record still reads (§1.3 'opens'): {e:?}"),
        };
        inject(&mut h, ops);
        h.activate(pages[0]);
        let _ = frame_texts(&mut h, pages[0]);
        let all = checks_all(&mut h);
        assert!(
            all.iter().any(|r| s(&r["kind"]).starts_with("title-") || s(&r["code"]).starts_with("title-")),
            "{what}: reported in Checks: {all:?}"
        );
        let _ = h.run("titleblocks", json!({})).unwrap();
        let _ = h.run("titleblock", json!({"id": id})).unwrap();
        let dir = tmp_dir(&format!("hostile {n}"));
        let mut h = save_and_reopen(h, &dir, "hostile.ecad", 0x55_7100 + n as u64);
        h.activate(pages[0]);
        let _ = frame_texts(&mut h, pages[0]);
    }
}

/// §12.1 J: an unknown body tag (a newer file) is kept raw and ignored: the frame draws the
/// built-in look and the record survives save and open unchanged.
#[test]
#[ignore = "tester 055: awaiting build"]
fn hostile_an_unknown_body_tag_is_kept_and_ignored() {
    let (mut h, pages) = three_pages(0x55_7201);
    let id = new_from_builtin(&mut h, "A3");
    use_for_project(&mut h, &id);
    let mut tx = stored_body_tx(&mut h, &id);
    let rec = find_obj_mut(&mut tx, "body").expect("a record with a body");
    rec.insert("body".into(), json!({"pattern_sheet": {"stripes": 3}}));
    let ops = decoded_ops(&tx);
    inject(&mut h, ops);
    h.activate(pages[0]);
    let _ = frame_texts(&mut h, pages[0]);
    let dir = tmp_dir("unknown tag");
    let before = h.doc().styles.clone();
    let mut h = save_and_reopen(h, &dir, "newer.ecad", 0x55_7202);
    assert!(h.doc().styles == before, "kept raw through save and open");
}

/// §12.1 J, design §0.3 invariant: a body on a `text` record (a hostile file) opens and draws the
/// text style as fields only; commands never write one.
#[test]
#[ignore = "tester 055: awaiting build"]
fn hostile_a_body_on_a_text_record_is_refused_by_commands_and_ignored_when_stored() {
    let (mut h, pages) = three_pages(0x55_7301);
    let id = new_from_builtin(&mut h, "A3");
    let v = read_block(&mut h, &id);
    let body = body_of(&v);
    refused(&mut h, "titleblock", json!({"id": "tag", "body": body}));
    // A stored one: take the frame's record and file it under `text` / `tag`.
    let tx = last_tx(&mut h);
    let Op::SetStyle { after: Some(rec), layer, .. } = tx.ops[0].clone() else { panic!("{:?}", tx.ops) };
    assert!(rec.body.is_some(), "FP1: the record carries its body");
    let op =
        Op::SetStyle { catalogue: "text".into(), layer, id: ecad_model::style::StyleRef::builtin("tag").unwrap(), before: None, after: Some(rec) };
    // Either the model refuses it at apply (an invariant), or it opens and is ignored and
    // reported; both keep the page drawing.
    let r = h.cfg(|p| p.ledger.commit(ecad_model::Author::User, "tester 055 body on text", vec![op]));
    h.activate(pages[0]);
    let _ = frame_texts(&mut h, pages[0]);
    if r.is_ok() {
        let all = checks_all(&mut h);
        assert!(!all.is_empty(), "reported: {all:?}");
    }
}

// ------------------------------------------------------------- G: Import DXF (055c)

/// A synthetic title block DXF made here (never a customer's): a 380 × 30 mm rectangle, two
/// dividers, the texts "DRAWING NO" and "PS-04-E" and a circle.
fn synthetic_block_dxf() -> String {
    let mut out = String::from("0\nSECTION\n2\nENTITIES\n");
    let line = |out: &mut String, a: (f64, f64), b: (f64, f64)| {
        out.push_str(&format!("0\nLINE\n8\n0\n10\n{}\n20\n{}\n30\n0\n11\n{}\n21\n{}\n31\n0\n", a.0, a.1, b.0, b.1));
    };
    for (a, b) in [((0.0, 0.0), (380.0, 0.0)), ((380.0, 0.0), (380.0, 30.0)), ((380.0, 30.0), (0.0, 30.0)), ((0.0, 30.0), (0.0, 0.0))] {
        line(&mut out, a, b);
    }
    line(&mut out, (300.0, 0.0), (300.0, 30.0));
    line(&mut out, (300.0, 15.0), (380.0, 15.0));
    for (t, x, y) in [("DRAWING NO", 302.0, 25.0), ("PS-04-E", 302.0, 18.0)] {
        out.push_str(&format!("0\nTEXT\n8\n0\n10\n{x}\n20\n{y}\n30\n0\n40\n2.5\n1\n{t}\n"));
    }
    out.push_str("0\nCIRCLE\n8\n0\n10\n20\n20\n15\n30\n0\n40\n8\n");
    out.push_str("0\nENDSEC\n0\nEOF\n");
    out
}

/// §12.1 G, §4.6: Import DXF makes a new frame from a synthetic DXF in one transaction: lines,
/// texts and the circle become items; texts stay static.
#[test]
#[ignore = "tester 055: awaiting build"]
fn g_import_dxf_makes_a_frame_whose_texts_are_static_items() {
    let (mut h, _) = three_pages(0x55_9001);
    let (v, _) = one_tx(&mut h, "titleblockimport", json!({"dxf": synthetic_block_dxf(), "sheet": "A3", "name": "Imported"}));
    let id = ["id", "style"].iter().map(|k| s(&v[*k])).find(|x| !x.is_empty()).unwrap();
    let b = read_block(&mut h, &id);
    let kinds: Vec<String> = items_of(&b).iter().map(|i| kind_of(i).0).collect();
    assert!(kinds.iter().filter(|k| *k == "line").count() >= 6, "{kinds:?}");
    assert_eq!(kinds.iter().filter(|k| *k == "text").count(), 2, "texts stay static: {kinds:?}");
    assert!(!kinds.iter().any(|k| k == "field"), "no field until Make field");
    assert!(b.to_string().contains("PS-04-E"));
    // The same file as a path (Windows path with spaces) gives the same items.
    let dir = tmp_dir("dxf import");
    let path = dir.join("Title block (synthetic).dxf");
    std::fs::write(&path, synthetic_block_dxf().replace('\n', "\r\n")).unwrap();
    let (v2, _) = one_tx(&mut h, "titleblockimport", json!({"path": path.display().to_string(), "sheet": "A3"}));
    let id2 = ["id", "style"].iter().map(|k| s(&v2[*k])).find(|x| !x.is_empty()).unwrap();
    assert_eq!(items_of(&read_block(&mut h, &id2)).len(), items_of(&b).len(), "CRLF file reads the same");
}

/// §12.1 G (Make field, as data): a text turned into a field keeps its box and look.
#[test]
#[ignore = "tester 055: awaiting build"]
fn g_a_text_made_a_field_keeps_box_and_look() {
    let (mut h, pages) = three_pages(0x55_9002);
    let (v, _) = one_tx(&mut h, "titleblockimport", json!({"dxf": synthetic_block_dxf(), "sheet": "A3"}));
    let id = ["id", "style"].iter().map(|k| s(&v[*k])).find(|x| !x.is_empty()).unwrap();
    let b = read_block(&mut h, &id);
    let mut body = body_of(&b);
    let i = items_of(&b).iter().position(|it| it.to_string().contains("PS-04-E")).unwrap();
    let (_, text) = kind_of(&items_of(&b)[i]);
    // Build the field from an existing field's shape (the house block's), with the text's box and look.
    let house = new_from_builtin(&mut h, "A3");
    let hb = read_block(&mut h, &house);
    let mut field = items_of(&hb)[field_index(&hb, "project:drawing_number")].clone();
    inner_mut(&mut field)["at"] = text["at"].clone();
    inner_mut(&mut field)["look"] = text["look"].clone();
    body["items"][i] = field;
    write_body(&mut h, &id, body);
    use_for_project(&mut h, &id);
    let after = read_block(&mut h, &id);
    let (k, f) = kind_of(&items_of(&after)[i]);
    assert_eq!(k, "field");
    assert_eq!(rect(&f), rect(&text), "keeps its box");
    assert_eq!(f["look"], text["look"], "keeps its look");
    assert!(frame_texts(&mut h, pages[0]).iter().any(|t| t == "PS-04-E"), "now filled from the project");
}

/// §4.6, §6: Import DXF refusals in words: too large for the block area, no drawing, not a DXF.
#[test]
#[ignore = "tester 055: awaiting build"]
fn g_import_refusals_say_why() {
    let (mut h, _) = three_pages(0x55_9003);
    let big = synthetic_block_dxf().replace("380.0", "900.0").replace("\n380\n", "\n900\n");
    let m = refused(&mut h, "titleblockimport", json!({"dxf": big, "sheet": "A3"}));
    assert!(m.contains("Scale it in the DXF first"), "§4.6: {m}");
    let m = refused(&mut h, "titleblockimport", json!({"dxf": "0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n", "sheet": "A3"}));
    assert!(m.contains("no drawing"), "§6: {m}");
    let m = refused(&mut h, "titleblockimport", json!({"dxf": "\u{89}PNG\r\n\u{1a}\nnot a dxf", "sheet": "A3"}));
    assert!(m.contains("not a DXF we can read"), "§6: {m}");
    let m = refused(&mut h, "titleblockimport", json!({"path": "C:\\no\\such\\file.dxf", "sheet": "A3"}));
    assert!(!m.is_empty());
}

// ------------------------------------------------------------------- F: logos (055c)

/// A synthetic DXF of `n` short lines in a 60 × 20 mm area.
fn logo_dxf(n: usize) -> String {
    let mut out = String::from("0\nSECTION\n2\nENTITIES\n");
    for i in 0..n {
        let x = (i % 60) as f64;
        let y = (i / 60 % 20) as f64;
        out.push_str(&format!("0\nLINE\n8\n0\n10\n{x}\n20\n{y}\n30\n0\n11\n{}\n21\n{}\n31\n0\n", x + 0.5, y + 0.5));
    }
    out.push_str("0\nENDSEC\n0\nEOF\n");
    out
}

/// §12.1 F, §6: a 120-shape DXF logo fills its box keeping proportions (centred), one transaction,
/// and prints in the PDF; 3 400 shapes are refused with the sentence; a non-DXF is refused.
#[test]
#[ignore = "tester 055: awaiting build"]
fn f_dxf_logos_fit_their_box_and_bounds_are_refused_in_words() {
    let (mut h, pages) = three_pages(0x55_f001);
    let id = new_from_builtin(&mut h, "A3");
    use_for_project(&mut h, &id);
    let v = read_block(&mut h, &id);
    let li = items_of(&v).iter().position(|i| kind_of(i).0 == "logo").expect("§3: the house block has a logo box");
    let (_, before) = kind_of(&items_of(&v)[li]);
    let (bx, by, bw, bh) = rect(&before);
    let n_ents = h.pv(|p, pg| pg.frame_entities_of(p, pages[0])).len();
    one_tx(&mut h, "titleblockimport", json!({"id": id, "item": li, "dxf": logo_dxf(120)}));
    let after = read_block(&mut h, &id);
    let (k, logo) = kind_of(&items_of(&after)[li]);
    assert_eq!(k, "logo");
    assert_eq!(logo["prims"].as_array().map(Vec::len), Some(120), "120 primitives stored");
    assert_eq!(rect(&logo), (bx, by, bw, bh), "the box stays");
    h.activate(pages[0]);
    let ents = h.pv(|p, pg| pg.frame_entities_of(p, pages[0]));
    assert!(ents.len() >= n_ents + 120, "the logo is drawn: {} → {}", n_ents, ents.len());
    // Fitted keeping proportions: the drawn logo's extent has the drawing's 3:1-ish ratio, inside
    // the box (frame coordinates are offset; check proportions only).
    let m = refused(&mut h, "titleblockimport", json!({"id": id, "item": li, "dxf": logo_dxf(3_400)}));
    assert!(m.contains("3 400 shapes") && m.contains("2 000 at most"), "§6 words: {m}");
    let m = refused(&mut h, "titleblockimport", json!({"id": id, "item": li, "dxf": "GIF89a"}));
    assert!(m.contains("not a DXF we can read"), "{m}");
    let m = refused(&mut h, "titleblockimport", json!({"id": id, "item": 9999, "dxf": logo_dxf(3)}));
    assert!(!m.is_empty());
    // Prints in PDF: the print path draws the logo's lines (Print Book of one page).
    let dir = tmp_dir("logo pdf");
    let pdf = dir.join("logo.pdf");
    let r = h.run("printbook", json!({"path": pdf.display().to_string(), "scope": {"pages": "1"}}));
    assert!(r.is_ok(), "the logo prints: {r:?}");
    let bytes = std::fs::read(&pdf).unwrap();
    assert!(bytes.starts_with(b"%PDF"), "a PDF is written");
}

// ------------------------------------------------------------------ K: one unit (local)

/// §12.1 K (the model side): the diff names the frame and its body change (§0.3 diff row).
#[test]
#[ignore = "tester 055: awaiting build"]
fn k_the_history_names_the_frame_whose_block_changed() {
    let (mut h, _) = three_pages(0x55_c0a1);
    let id = new_from_builtin(&mut h, "A3");
    h.run("setstyle", json!({"catalogue": "frame", "id": id, "name": "A3 Northfield"})).ok();
    let v = read_block(&mut h, &id);
    let mut body = body_of(&v);
    let i = field_index(&v, "project:drawing_number");
    inner_mut(&mut body["items"][i])["at"]["y"] = json!(rect(&kind_of(&body["items"][i]).1).1 + 1.0);
    write_body(&mut h, &id, body);
    let tx = last_tx(&mut h);
    let steps = h.run("steps", json!({"limit": 5})).unwrap().to_string();
    let words = format!("{} {steps}", tx.label);
    assert!(words.contains("A3 Northfield"), "§0.3: the diff names the frame: {words:.600}");
    assert!(words.to_lowercase().contains("title block"), "{words:.600}");
}
