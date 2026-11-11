//! Brief 024 acceptance (design 024 §13): device locations and location boxes, the location
//! prefix, the reports, generated pages and Print Book. Driven through the commands' JSON forms.

use ecad_cadcraft::checks::{Fix, Kind};
use ecad_cadcraft::locations::{self, Source};
use ecad_cadcraft::outputs::{Group, Outputs};
use ecad_model::{ComponentId, ConductorId, Document, PageId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

fn place(h: &mut H, symbol: &str, at: (f64, f64)) -> ComponentId {
    let r = h.run("place", json!({"symbol": symbol, "at": [at.0, at.1]})).unwrap();
    id(&r, "component")
}

fn loc(doc: &Document, c: ComponentId) -> (Option<String>, &'static str) {
    let l = locations::resolve(doc, c);
    (l.value, l.source.word())
}

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

/// The kept models equal a fresh build (the update map left nothing behind).
fn same_as_built(h: &mut H) {
    let (kept, built) = h.cfg(|p| {
        let kept = p.outputs().clone();
        (kept, Outputs::build(p.ledger.document()))
    });
    assert_eq!(kept.bom_table(Group::Location), built.bom_table(Group::Location));
    assert_eq!(kept.bom_table(Group::Part), built.bom_table(Group::Part));
    assert_eq!(kept.wire_table(), built.wire_table());
    assert_eq!(kept.cable_table(), built.cable_table());
    let mut a = kept.checks();
    let mut b = built.checks();
    a.sort_by_key(|c| format!("{c:?}"));
    b.sort_by_key(|c| format!("{c:?}"));
    assert_eq!(a, b);
}

/// §13 A: own value, page, project; one transaction for several; clear back to inherited.
#[test]
fn a_device_location_is_its_own_then_the_page_then_the_project() {
    let mut h = H::new(0x24_01);
    let k = place(&mut h, "coil", (100.0, 150.0));
    let q = place(&mut h, "coil", (150.0, 150.0));
    let doc = h.doc();
    assert_eq!(loc(&doc, k), (None, "none"));
    h.run("projectprops", json!({"location": "CP1"})).unwrap();
    assert_eq!(loc(&h.doc(), k), (s("+CP1"), "project"));
    h.run("pageprops", json!({"location": "+FIELD"})).unwrap();
    assert_eq!(loc(&h.doc(), k), (s("+FIELD"), "page"));
    let head = h.head();
    h.run("setprop", json!({"components": [k.hex(), q.hex()], "key": "location", "value": "CP3"})).unwrap();
    assert_eq!(h.head(), head + 1, "several devices: one transaction");
    assert_eq!(loc(&h.doc(), k), (s("+CP3"), "device"));
    assert_eq!(loc(&h.doc(), q), (s("+CP3"), "device"));
    h.run("setprop", json!({"component": k.hex(), "key": "location", "value": null})).unwrap();
    assert_eq!(loc(&h.doc(), k), (s("+FIELD"), "page"));
    h.cmdline("u");
    assert_eq!(loc(&h.doc(), k), (s("+CP3"), "device"));
    // `devices` reports what is shown and where it comes from.
    let v = h.run("devices", json!({})).unwrap();
    let all = v["devices"].as_array().unwrap();
    assert!(all.iter().all(|d| d["location_from"] == "device" && d["location_shown"] == "+CP3"), "{v}");
    same_as_built(&mut h);
}

/// §13 B: a box gives its location live; moving it or a symbol updates at once; nested boxes:
/// the inner wins; an own differing value is kept, flagged, and fixed in one transaction.
#[test]
fn location_boxes_are_live_and_the_innermost_wins() {
    let mut h = H::new(0x24_02);
    h.run("projectprops", json!({"location": "+CP1"})).unwrap();
    let page = h.first_page();
    h.run("pageprops", json!({"location": "+FIELD"})).unwrap();
    // Keep the models from here on, so every edit goes through the update map.
    h.cfg(|p| {
        p.outputs();
    });
    let m = place(&mut h, "coil", (100.0, 100.0));
    let x = place(&mut h, "coil", (140.0, 100.0));
    let head = h.head();
    let r = h.run("locationbox", json!({"location": "JB1", "corners": [[80, 80], [160, 130]]})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    assert_eq!(r["inside"], 2, "{r}");
    let boxc: ComponentId = id(&r, "component");
    let doc = h.doc();
    assert_eq!(loc(&doc, m), (s("+JB1"), "box"));
    let l = locations::resolve(&doc, m);
    assert_eq!(l.source, Source::Box { component: boxc, page });
    assert!(l.shown(&doc).contains("from box on 1"), "{}", l.shown(&doc));
    // Nothing is written on the devices.
    assert!(!doc.components[&m].properties.contains_key("location"));
    same_as_built(&mut h);
    // Move the box off them: the page's location at once; `u` brings it back.
    let item = h.run("devices", json!({})).unwrap();
    let _ = item;
    let item: ecad_model::ItemId = id(&r, "item");
    let hd = h.handle_of(page, item);
    h.run("move", json!({"handles": [hd.hex()], "delta": [0, 200]})).unwrap();
    assert_eq!(loc(&h.doc(), m), (s("+FIELD"), "page"));
    same_as_built(&mut h);
    h.cmdline("u");
    assert_eq!(loc(&h.doc(), m), (s("+JB1"), "box"));
    same_as_built(&mut h);
    // A polyline box nested inside: the inner wins for the device inside it only.
    h.run("locationbox", json!({"location": "+JB2", "points": [[90, 90], [120, 90], [125, 105], [120, 120], [90, 120]]})).unwrap();
    let doc = h.doc();
    assert_eq!(loc(&doc, m).0, s("+JB2"));
    assert_eq!(loc(&doc, x).0, s("+JB1"));
    same_as_built(&mut h);
    // An own value that differs: kept, flagged, one-click fix.
    h.run("setprop", json!({"component": x.hex(), "key": "location", "value": "+CP9"})).unwrap();
    assert_eq!(loc(&h.doc(), x), (s("+CP9"), "device"));
    let checks = ecad_cadcraft::checks::all(&h.doc(), 1e-6);
    let own: Vec<_> = checks.iter().filter(|c| c.kind == Kind::OwnLocation).collect();
    assert_eq!(own.len(), 1, "{checks:?}");
    assert!(own[0].which.contains("is +CP9 but sits in box +JB1"), "{}", own[0].which);
    assert_eq!(own[0].fix, Some(Fix::UseBoxLocation(x)));
    same_as_built(&mut h);
    h.run("setprop", json!({"component": x.hex(), "key": "location", "value": null})).unwrap();
    assert_eq!(loc(&h.doc(), x), (s("+JB1"), "box"));
    // Erasing the box deletes it with its outline.
    let hd = h.handle_of(page, item);
    h.run("erase", json!({"handles": [hd.hex()]})).unwrap();
    let doc = h.doc();
    assert!(!doc.components.contains_key(&boxc), "the box component goes with its outline");
    assert_eq!(loc(&doc, x), (s("+FIELD"), "page"));
    same_as_built(&mut h);
}

/// §3.5: refusals keep the way on.
#[test]
fn a_box_needs_room_and_a_closed_polyline() {
    let mut h = H::new(0x24_03);
    let e = h.run("locationbox", json!({"location": "+A", "corners": [[0, 0], [5, 5]]})).unwrap_err();
    assert!(e.contains("at least 10 x 10 mm"), "{e}");
    let e = h.run("locationbox", json!({"location": "+A", "corners": [[0, 0], [50, 50]], "points": [[0, 0]]})).unwrap_err();
    assert!(e.contains("exactly one"), "{e}");
    // An existing closed polyline becomes a box; an open one is refused.
    let new_item = |h: &mut H, before: &std::collections::BTreeSet<ecad_model::ItemId>| *h.doc().items.keys().find(|i| !before.contains(i)).unwrap();
    let before: std::collections::BTreeSet<_> = h.doc().items.keys().copied().collect();
    h.run("pline", json!({"vertices": pts(&[(0.0, 0.0), (60.0, 0.0), (60.0, 40.0), (0.0, 40.0)]), "closed": true})).unwrap();
    let item = new_item(&mut h, &before);
    let made = h.run("locationbox", json!({"location": "+JB9", "item": item.hex()})).unwrap();
    let doc = h.doc();
    assert_eq!(doc.items[&item].style.linetype, "DASHED");
    assert!(doc.components.contains_key(&id::<ComponentId>(&made, "component")));
    // The same outline again: it belongs to a box now.
    let e = h.run("locationbox", json!({"location": "+JB8", "item": item.hex()})).unwrap_err();
    assert!(e.contains("belongs to another location box"), "{e}");
    let before: std::collections::BTreeSet<_> = h.doc().items.keys().copied().collect();
    h.run("pline", json!({"vertices": pts(&[(100.0, 0.0), (160.0, 0.0), (160.0, 40.0)])})).unwrap();
    let open = new_item(&mut h, &before);
    let e = h.run("locationbox", json!({"location": "+JB7", "item": open.hex()})).unwrap_err();
    assert!(e.contains("Pick a closed polyline"), "{e}");
}

/// §13 C: a symbol drawn away from its device's location shows the prefix; the device keeps its
/// main symbol's location.
#[test]
fn a_contact_away_from_its_coil_shows_the_location_prefix() {
    let mut h = H::new(0x24_04);
    h.run("projectprops", json!({"location": "+CP1"})).unwrap();
    let p1 = h.first_page();
    let k = place(&mut h, "coil", (100.0, 150.0));
    let r = h.run("newpage", json!({"title": "Field"})).unwrap();
    let p2: PageId = id(&r, "page");
    h.run("locationbox", json!({"location": "+JB1", "corners": [[50, 50], [200, 200]]})).unwrap();
    h.run("place", json!({"symbol": "no", "at": [100, 100], "device": k.hex()})).unwrap();
    let doc = h.doc();
    assert_eq!(loc(&doc, k).0, s("+CP1"), "main-symbol rule");
    let tag = ecad_cadcraft::devices::tag_of(&doc, k).unwrap().to_string();
    let texts2: Vec<String> = ecad_cadcraft::devices::texts_on_page(&doc, p2).into_iter().map(|t| t.value).collect();
    assert!(texts2.contains(&format!("+CP1{tag}")), "{texts2:?}");
    let texts1: Vec<String> = ecad_cadcraft::devices::texts_on_page(&doc, p1).into_iter().map(|t| t.value).collect();
    assert!(texts1.contains(&tag), "{texts1:?}");
    assert!(ecad_cadcraft::checks::all(&doc, 1e-6).iter().all(|c| c.kind != Kind::OwnLocation));
}

fn wire(h: &mut H, points: &[(f64, f64)]) -> ConductorId {
    id(&h.run("wire", json!({"points": pts(points)})).unwrap(), "conductor")
}

/// §13 E, F: the BOM holds devices only; the wire report names both ends; flags are counted;
/// CSV is Excel-safe.
#[test]
fn the_reports_keep_wires_off_the_bom_and_flag_missing_values() {
    let mut h = H::new(0x24_05);
    h.cfg(|p| {
        p.outputs();
    });
    h.run("projectprops", json!({"location": "+CP1"})).unwrap();
    let a = h.run("place", json!({"symbol": "terminal", "at": [0, 0], "tag": "X1"})).unwrap();
    let b = h.run("place", json!({"symbol": "terminal", "at": [60, 0], "tag": "X2"})).unwrap();
    let (a, b): (ComponentId, ComponentId) = (id(&a, "component"), id(&b, "component"));
    let w = wire(&mut h, &[(0.0, 0.0), (60.0, 0.0)]);
    h.run("setprop", json!({"component": a.hex(), "key": "maker", "value": "Phoenix"})).unwrap();
    h.run("setprop", json!({"component": a.hex(), "key": "part_number", "value": "UT 2.5"})).unwrap();
    same_as_built(&mut h);
    let bom = h.run("bom", json!({})).unwrap();
    let rows = bom["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "two devices, no wire: {bom}");
    assert!(rows.iter().all(|r| r["location"] == "+CP1"), "{bom}");
    assert!(bom["flagged"].as_u64().unwrap() >= 2, "X2 has no maker or part number: {bom}");
    let doc = h.doc();
    let (ta, tb) = (ecad_cadcraft::devices::tag_of(&doc, a).unwrap().to_string(), ecad_cadcraft::devices::tag_of(&doc, b).unwrap().to_string());
    let flagged_row = rows.iter().find(|r| r["tags"] == tb.as_str()).unwrap();
    assert!(flagged_row["flags"].as_array().unwrap().contains(&json!("maker")));
    let wires = h.run("wirelist", json!({})).unwrap();
    let wr = &wires["rows"][0];
    assert_eq!(wr["conductor"], w.hex());
    assert!(wr["from"].as_str().unwrap().starts_with(&format!("{ta}:")), "{wires}");
    assert!(wr["to"].as_str().unwrap().starts_with(&format!("{tb}:")), "{wires}");
    assert_eq!(wr["from_location"], "+CP1");
    // A location change at an end reaches the wire row (update map).
    h.run("setprop", json!({"component": b.hex(), "key": "location", "value": "+FIELD"})).unwrap();
    let wires = h.run("wirelist", json!({})).unwrap();
    assert_eq!(wires["rows"][0]["to_location"], "+FIELD");
    same_as_built(&mut h);
    // A cable over the wire: one row, its locations.
    h.run("cableslash", json!({"from": [30, 10], "to": [30, -10]})).unwrap();
    let cables = h.run("cablelist", json!({})).unwrap();
    let c = &cables["rows"][0];
    assert_eq!(c["cores"], 1, "{cables}");
    assert_eq!(c["from_location"], "+CP1");
    assert_eq!(c["to_location"], "+FIELD");
    assert!(c["used"].is_null());
    same_as_built(&mut h);
    // CSV, Excel-safe.
    let dir = std::env::temp_dir().join(format!("ecad-024-{}", std::process::id()));
    let path = dir.join("bom.csv");
    let r = h.run("bom", json!({"path": path.to_string_lossy()})).unwrap();
    assert_eq!(r["rows"], 2);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with('\u{feff}'));
    assert!(text.contains(&format!("'+CP1,1,'{ta},")), "{text}");
    assert!(text.contains("\r\n"));
    let _ = std::fs::remove_dir_all(&dir);
    // Nothing recorded by reading.
    let head = h.head();
    h.run("bom", json!({"group": "part"})).unwrap();
    h.run("wirelist", json!({})).unwrap();
    assert_eq!(h.head(), head);
}

/// §13 D, E: generated pages are added in one transaction, refuse drawing with a way on, follow
/// edits, overflow onto more sheets and count in Total; Print Book prints them.
#[test]
fn generated_pages_follow_the_project_and_print() {
    ecad_cadcraft::printbook::install_io();
    let mut h = H::new(0x24_06);
    h.run("projectprops", json!({"title": "Pump panel", "location": "+CP1"})).unwrap();
    place(&mut h, "coil", (100.0, 150.0));
    let head = h.head();
    let r = h.run("newpage", json!({"generated": "bom-location"})).unwrap();
    assert_eq!(h.head(), head + 1);
    let bom_page: PageId = id(&r, "page");
    let doc = h.doc();
    assert_eq!(doc.pages[&bom_page].title, "BOM by location");
    let e = h.run("place", json!({"symbol": "coil", "at": [100, 100]})).unwrap_err();
    assert!(e.contains("is generated (BOM by location)") && e.contains("add one after it with +"), "{e}");
    let e = h.run("pageprops", json!({"generated": "cover"})).unwrap_err();
    assert!(e.contains("kind is set when it is made"), "{e}");
    // The page shows the BOM; 60 devices overflow to two sheets and Total counts them.
    let first = h.first_page();
    h.activate(first);
    for i in 0..60 {
        place(&mut h, "coil", (20.0 + f64::from(i % 10) * 35.0, 60.0 + f64::from(i / 10) * 30.0));
    }
    let doc = h.doc();
    assert_eq!(ecad_cadcraft::generated::sheets(&doc, bom_page, Some(&Outputs::build(&doc))), 2);
    let parts = ecad_cadcraft::generated::parts_of(&doc, bom_page, None);
    let texts: Vec<String> = parts
        .iter()
        .filter_map(|p| match &p.kind {
            cadcraft_engine::doc::EntityKind::Text(t) => Some(t.value.clone()),
            _ => None,
        })
        .collect();
    assert!(texts.iter().any(|t| t == "BOM by location (2/2)"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "+CP1 (continued)"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "3"), "Total counts sheets: {texts:?}");
    // Print Book: one PDF, every sheet, in book order.
    let v = h.run("printbook", json!({})).unwrap();
    assert_eq!(v["sheets"], 3, "{}", v["message"]);
    assert_eq!(v["generated"], 2);
    let data = v["files"][0]["data"].as_str().unwrap();
    assert!(data.starts_with("JVBER"), "a PDF");
    let v = h.run("printbook", json!({"scope": {"pages": "1, 7"}})).unwrap();
    assert_eq!(v["sheets"], 1);
    assert_eq!(v["unknown"], json!(["7"]));
}

/// The cover, contents and revision list show the project and follow it.
#[test]
fn cover_contents_and_revision_list_show_the_project() {
    let mut h = H::new(0x24_07);
    h.run("projectprops", json!({"title": "Pump panel", "drawing_number": "D-100"})).unwrap();
    let r = h.run("newpage", json!({"generated": "cover"})).unwrap();
    let cover: PageId = id(&r, "page");
    let r = h.run("newpage", json!({"generated": "contents"})).unwrap();
    let contents: PageId = id(&r, "page");
    let r = h.run("newpage", json!({"generated": "revisions"})).unwrap();
    let revs: PageId = id(&r, "page");
    let doc = h.doc();
    let text = |p: PageId| -> Vec<String> {
        ecad_cadcraft::generated::parts_of(&doc, p, None)
            .into_iter()
            .filter_map(|p| match p.kind {
                cadcraft_engine::doc::EntityKind::Text(t) => Some(t.value),
                _ => None,
            })
            .collect()
    };
    let c = text(cover);
    assert!(c.contains(&"Pump panel".to_string()) && c.contains(&"D-100".to_string()) && c.contains(&"Not issued".to_string()), "{c:?}");
    assert!(c.contains(&"4 sheets".to_string()), "{c:?}");
    let t = text(contents);
    assert!(t.contains(&"Cover".to_string()) && t.contains(&"Contents".to_string()) && t.contains(&"Revision list".to_string()), "{t:?}");
    let r = text(revs);
    assert!(r.contains(&"No revisions issued yet.".to_string()), "{r:?}");
}

#[allow(dead_code)]
fn _unused(_: Value) {}

/// Tester 024, bug 2 (owner ruling): PEDIT Open of a box outline is refused with the way out, and
/// the box keeps giving its location.
#[test]
fn opening_a_box_outline_is_refused_with_the_way_out() {
    let mut h = H::new(0x24_08);
    let page = h.first_page();
    let k = place(&mut h, "coil", (100.0, 100.0));
    let r = h.run("locationbox", json!({"location": "+JB1", "corners": [[80, 80], [160, 140]]})).unwrap();
    let item: ecad_model::ItemId = id(&r, "item");
    let hd = h.handle_of(page, item);
    let head = h.head();
    let e = h.run("pedit", json!({"handle": hd.hex(), "option": "open"})).unwrap_err();
    assert!(e.contains("cannot be opened") && e.contains("erase the box"), "{e}");
    assert_eq!(h.head(), head, "nothing recorded");
    assert_eq!(loc(&h.doc(), k).0, s("+JB1"));
}
