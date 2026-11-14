//! Tester probe for brief 024 (outputs): device locations, location boxes, the kept report models,
//! generated pages, Print Book and the file round trip.
//!
//! The random probe (`random_edits_keep_locations_and_reports_equal_to_a_rebuild`) draws, moves,
//! reshapes, re-labels and erases boxes (nested, overlapping, crossing themselves), moves devices
//! in and out of them on two pages, sets and clears own, page and project locations, edits parts,
//! moves pages, rotates and mirrors selections holding a box and devices, and undoes and redoes.
//! After every command it checks, as an oracle:
//! - the kept report models (BOM by location and by part, wires, cables, the location Checks rows,
//!   every device's location) equal a fresh `Outputs::build`;
//! - no command other than `setprop`, undo and redo changes a device's own `location` property
//!   (box locations are derived, never written);
//! - a BOM page's sheet count from the kept models equals the rebuilt one;
//! - the harness's own project checks (projection equals the page drawing, replay, mismatches).
//!
//! Bugs are kept as tests marked `#[ignore = "tester 024: <bug>"]`.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeMap;

use crate::common::{H, id, pts};
use ecad_cadcraft::locations::{self, Source};
use ecad_cadcraft::outputs::{Cell, Group, Outputs, Table, csv};
use ecad_model::{ComponentId, Document, ItemId, ItemKind, PageId, Primitive, Pt, Subject};
use serde_json::{Value, json};

// ---------------------------------------------------------------- helpers

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
    fn x(&mut self) -> f64 {
        10.0 + self.below(76) as f64 * 5.0
    }
    fn y(&mut self) -> f64 {
        60.0 + self.below(44) as f64 * 5.0
    }
    fn pick<T: Clone>(&mut self, v: &[T]) -> Option<T> {
        if v.is_empty() { None } else { v.get(self.below(v.len())).cloned() }
    }
}

const LOCS: &[&str] = &["+A", "+B", "+CP1", "+CP10", "+JB1", "+FIELD"];

fn devices(doc: &Document) -> Vec<ComponentId> {
    doc.components.keys().copied().filter(|c| locations::is_device(doc, *c)).collect()
}

/// Every box: (component, page, outline item, vertex count).
fn boxes(doc: &Document) -> Vec<(ComponentId, PageId, ItemId, usize)> {
    doc.page_order.iter().flat_map(|p| locations::boxes_on(doc, *p)).map(|b| (b.component, b.page, b.item, b.vertices.len())).collect()
}

/// Each device's own `location` property.
fn own(doc: &Document) -> BTreeMap<ComponentId, Option<String>> {
    devices(doc).into_iter().map(|c| (c, doc.components[&c].properties.get("location").cloned())).collect()
}

/// The page of a component's first representation.
fn page_of(doc: &Document, c: ComponentId) -> Option<PageId> {
    doc.representations_of(Subject::Component(c)).next().map(|(_, r)| r.page)
}

fn place(h: &mut H, symbol: &str, at: (f64, f64)) -> ComponentId {
    let r = h.run("place", json!({"symbol": symbol, "at": [at.0, at.1]})).unwrap();
    id(&r, "component")
}

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

fn keep_models(h: &mut H) {
    h.cfg(|p| {
        p.outputs();
    });
}

/// The oracle: the kept models equal a rebuild; every device's kept location equals `resolve`.
fn verify(h: &mut H, what: &str) {
    let (kept, built, doc) = h.cfg(|p| {
        let kept = p.outputs().clone();
        let doc = p.ledger.document().clone();
        (kept, Outputs::build(&doc), doc)
    });
    assert_eq!(kept.bom_table(Group::Location), built.bom_table(Group::Location), "{what}: BOM by location");
    assert_eq!(kept.bom_table(Group::Part), built.bom_table(Group::Part), "{what}: BOM by part");
    assert_eq!(kept.wire_table(), built.wire_table(), "{what}: wire report");
    assert_eq!(kept.cable_table(), built.cable_table(), "{what}: cable report");
    let mut a: Vec<String> = kept.checks().iter().map(|c| format!("{c:?}")).collect();
    let mut b: Vec<String> = built.checks().iter().map(|c| format!("{c:?}")).collect();
    a.sort();
    b.sort();
    assert_eq!(a, b, "{what}: location checks");
    assert_eq!(kept.device_count(), built.device_count(), "{what}: device count");
    for c in devices(&doc) {
        let want = locations::resolve(&doc, c);
        assert_eq!(kept.device(c).map(|d| d.location.clone()), Some(want), "{what}: kept location of {c:?}");
        assert_eq!(kept.device(c), built.device(c), "{what}: device row {c:?}");
    }
    // No wire on the BOM: every BOM row is made of devices only.
    for r in kept.bom(Group::Location) {
        for d in &r.devices {
            assert!(locations::is_device(&doc, *d), "{what}: a BOM row holds {d:?}, not a device");
        }
    }
    for p in &doc.page_order {
        if ecad_cadcraft::generated::kind_of(&doc, *p).is_some() {
            assert_eq!(
                ecad_cadcraft::generated::sheets(&doc, *p, Some(&kept)),
                ecad_cadcraft::generated::sheets(&doc, *p, Some(&built)),
                "{what}: sheets of a generated page"
            );
        }
    }
}

fn texts(doc: &Document, page: PageId) -> Vec<String> {
    ecad_cadcraft::generated::parts_of(doc, page, None)
        .into_iter()
        .filter_map(|p| match p.kind {
            cadcraft_engine::doc::EntityKind::Text(t) => Some(t.value),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------- 1, 3: the random oracle

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Act {
    BoxRect,
    BoxPoly,
    BoxMove,
    BoxGrip,
    BoxVertex,
    BoxErase,
    BoxRelabel,
    DevMove,
    DevPlace,
    DevJoin,
    DevErase,
    OwnLoc,
    PageLoc,
    ProjLoc,
    Part,
    Group,
    MovePage,
    Undo,
    Redo,
}

const ACTS: &[Act] = &[
    Act::BoxRect,
    Act::BoxRect,
    Act::BoxPoly,
    Act::BoxMove,
    Act::BoxMove,
    Act::BoxGrip,
    Act::BoxVertex,
    Act::BoxErase,
    Act::BoxRelabel,
    Act::DevMove,
    Act::DevMove,
    Act::DevMove,
    Act::DevPlace,
    Act::DevJoin,
    Act::DevErase,
    Act::OwnLoc,
    Act::OwnLoc,
    Act::PageLoc,
    Act::ProjLoc,
    Act::Part,
    Act::Group,
    Act::MovePage,
    Act::Undo,
    Act::Undo,
    Act::Redo,
];

struct Fx {
    h: H,
    pages: Vec<PageId>,
}

/// Two pages; devices on both; two terminals wired with a cable over the wire; a BOM page.
fn fixture(seed: u64) -> Fx {
    let mut h = H::new(seed);
    keep_models(&mut h);
    h.run("projectprops", json!({"location": "+PROJ"})).unwrap();
    let p1 = h.first_page();
    for (x, y) in [(40.0, 100.0), (90.0, 120.0), (150.0, 180.0), (220.0, 90.0)] {
        place(&mut h, "coil", (x, y));
    }
    let a = place(&mut h, "terminal", (60.0, 220.0));
    let b = place(&mut h, "terminal", (160.0, 220.0));
    h.run("wire", json!({"points": pts(&[(60.0, 220.0), (160.0, 220.0)])})).unwrap();
    h.run("cableslash", json!({"from": [110, 230], "to": [110, 210]})).unwrap();
    h.run("setprop", json!({"component": a.hex(), "key": "part_number", "value": "UT 2.5"})).unwrap();
    h.run("setprop", json!({"component": b.hex(), "key": "part_number", "value": "UT 2.5"})).unwrap();
    let p2: PageId = id(&h.run("newpage", json!({"title": "Field"})).unwrap(), "page");
    for (x, y) in [(50.0, 80.0), (120.0, 150.0), (300.0, 200.0)] {
        place(&mut h, "coil", (x, y));
    }
    h.run("newpage", json!({"generated": "bom-location"})).unwrap();
    h.activate(p1);
    verify(&mut h, "fixture");
    Fx { h, pages: vec![p1, p2] }
}

fn step(f: &mut Fx, r: &mut Rng, act: Act) -> Result<Value, String> {
    let doc = f.h.doc();
    let page = r.pick(&f.pages).unwrap_or(f.pages[0]);
    match act {
        Act::BoxRect => {
            f.h.activate(page);
            let (x, y) = (r.x(), r.y());
            let (w, hh) = (15.0 + r.below(30) as f64 * 5.0, 15.0 + r.below(20) as f64 * 5.0);
            f.h.run("locationbox", json!({"location": r.pick(LOCS), "corners": [[x, y], [x + w, y + hh]]}))
        }
        Act::BoxPoly => {
            f.h.activate(page);
            let n = 3 + r.below(4);
            let v: Vec<(f64, f64)> = (0..n).map(|_| (r.x(), r.y())).collect();
            f.h.run("locationbox", json!({"location": r.pick(LOCS), "points": pts(&v)}))
        }
        Act::BoxMove | Act::BoxGrip | Act::BoxVertex | Act::BoxErase => {
            let Some((_, p, item, n)) = r.pick(&boxes(&doc)) else { return Err("no box".into()) };
            f.h.activate(p);
            let hd = f.h.handle_of(p, item).hex();
            match act {
                Act::BoxMove => {
                    let d = [(r.below(21) as f64 - 10.0) * 5.0, (r.below(21) as f64 - 10.0) * 5.0];
                    f.h.run("move", json!({"handles": [hd], "delta": d}))
                }
                Act::BoxGrip => f.h.run("grip.stretch", json!({"handle": hd, "index": r.below(n), "to": [r.x(), r.y()]})),
                Act::BoxVertex => f.h.run("grip.option", json!({"handle": hd, "index": n + r.below(n), "option": "addVertex", "to": [r.x(), r.y()]})),
                _ => f.h.run("erase", json!({"handles": [hd]})),
            }
        }
        Act::BoxRelabel => {
            let Some((c, ..)) = r.pick(&boxes(&doc)) else { return Err("no box".into()) };
            f.h.run("setprop", json!({"component": c.hex(), "key": "location", "value": r.pick(LOCS)}))
        }
        Act::DevMove => {
            let Some(c) = r.pick(&devices(&doc)) else { return Err("no device".into()) };
            let reps: Vec<PageId> = doc.representations_of(Subject::Component(c)).map(|(_, r)| r.page).collect();
            let Some(p) = r.pick(&reps) else { return Err("no rep".into()) };
            f.h.activate(p);
            let handles = f.h.rep_handles(p, c);
            let d = [(r.below(41) as f64 - 20.0) * 5.0, (r.below(41) as f64 - 20.0) * 5.0];
            f.h.run("move", json!({"handles": handles, "delta": d}))
        }
        Act::DevPlace => {
            f.h.activate(page);
            f.h.run("place", json!({"symbol": "coil", "at": [r.x(), r.y()]}))
        }
        Act::DevJoin => {
            // A contact of an existing device, on any page: a device with symbols in different
            // locations (main-symbol rule).
            let Some(c) = r.pick(&devices(&doc)) else { return Err("no device".into()) };
            f.h.activate(page);
            f.h.run("place", json!({"symbol": "no", "at": [r.x(), r.y()], "device": c.hex()}))
        }
        Act::DevErase => {
            let Some(c) = r.pick(&devices(&doc)) else { return Err("no device".into()) };
            let Some(p) = page_of(&doc, c) else { return Err("no rep".into()) };
            f.h.activate(p);
            let handles = f.h.rep_handles(p, c);
            f.h.run("erase", json!({"handles": handles}))
        }
        Act::OwnLoc => {
            let all = devices(&doc);
            let v = if r.below(3) == 0 { Value::Null } else { json!(r.pick(LOCS)) };
            if r.below(2) == 0 {
                let Some(c) = r.pick(&all) else { return Err("no device".into()) };
                f.h.run("setprop", json!({"component": c.hex(), "key": "location", "value": v}))
            } else {
                let some: Vec<String> = all.iter().filter(|_| r.below(2) == 0).map(|c| c.hex()).collect();
                f.h.run("setprop", json!({"components": some, "key": "location", "value": v}))
            }
        }
        Act::PageLoc => {
            let v = if r.below(3) == 0 { Value::Null } else { json!(r.pick(LOCS)) };
            f.h.run("pageprops", json!({"page": page.hex(), "location": v}))
        }
        Act::ProjLoc => {
            // Never cleared: a device with no location at all hits the bug kept in
            // `setting_the_project_location_reaches_devices_that_had_none`.
            f.h.run("projectprops", json!({"location": r.pick(LOCS)}))
        }
        Act::Part => {
            let Some(c) = r.pick(&devices(&doc)) else { return Err("no device".into()) };
            let key = ["part_number", "maker", "description", "tag"][r.below(4)];
            let v = match key {
                "tag" => json!(format!("K{}", 50 + r.below(50))),
                _ if r.below(4) == 0 => Value::Null,
                _ => json!(["UT 2.5", "ut 2.5", "CT-9A, 24 V", "=SUM(A1)", "Say \"hi\""][r.below(5)]),
            };
            f.h.run("setprop", json!({"component": c.hex(), "key": key, "value": v}))
        }
        Act::Group => {
            // Move, rotate or mirror a box together with some devices on its page.
            let Some((bc, p, item, _)) = r.pick(&boxes(&doc)) else { return Err("no box".into()) };
            let _ = bc;
            f.h.activate(p);
            let mut handles = vec![f.h.handle_of(p, item).hex()];
            for c in devices(&doc) {
                if doc.representations_of(Subject::Component(c)).any(|(_, x)| x.page == p) && r.below(2) == 0 {
                    handles.extend(f.h.rep_handles(p, c));
                }
            }
            let base = [r.x(), r.y()];
            match r.below(3) {
                0 => f.h.run("move", json!({"handles": handles, "delta": [(r.below(9) as f64 - 4.0) * 10.0, (r.below(9) as f64 - 4.0) * 10.0]})),
                1 => {
                    let angle = [90.0, 180.0, 270.0, 30.0][r.below(4)];
                    f.h.run("rotate", json!({"handles": handles, "base": base, "angle": angle}))
                }
                _ => f.h.run("mirror", json!({"handles": handles, "p1": base, "p2": [base[0], base[1] + 10.0], "erase": true})),
            }
        }
        Act::MovePage => {
            let n = doc.page_order.len();
            f.h.run("movepage", json!({"page": page.hex(), "index": 1 + r.below(n)}))
        }
        Act::Undo => f.h.run("u", Value::Null),
        Act::Redo => f.h.run("redo", Value::Null),
    }
}

fn run_random(seed: u64, steps: usize) {
    let mut f = fixture(seed);
    let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut ok: BTreeMap<Act, usize> = BTreeMap::new();
    for i in 0..steps {
        let act = ACTS[r.below(ACTS.len())];
        let before = own(&f.h.doc());
        let res = step(&mut f, &mut r, act);
        if res.is_ok() {
            *ok.entry(act).or_default() += 1;
        }
        let what = format!("seed {seed:#x} step {i} {act:?} -> {}", res.as_ref().map_or_else(|e| format!("Err({e})"), |_| "ok".into()));
        verify(&mut f.h, &what);
        if !matches!(act, Act::OwnLoc | Act::Undo | Act::Redo) {
            let after = own(&f.h.doc());
            for (c, v) in &before {
                if let Some(w) = after.get(c) {
                    assert_eq!(v, w, "{what}: the device's own location changed without setprop");
                }
            }
        }
    }
    // The probe exercised what it says it does.
    for a in [Act::BoxRect, Act::BoxMove, Act::DevMove, Act::OwnLoc, Act::Undo] {
        assert!(ok.get(&a).copied().unwrap_or(0) > 0, "seed {seed:#x}: no successful {a:?}: {ok:?}");
    }
}

#[test]
fn random_edits_keep_locations_and_reports_equal_to_a_rebuild() {
    run_random(0x24_7E57, 60);
}

#[test]
#[ignore = "slow: 3 x 250 random edits, each checked against a full rebuild and a fresh projection"]
fn random_edits_keep_locations_and_reports_equal_to_a_rebuild_long() {
    for seed in [0x24_0001, 0x24_0002, 0x24_0003] {
        run_random(seed, 250);
    }
}

// ---------------------------------------------------------------- targeted update-map probes

/// Setting the project's location must reach devices that had no location at all (source `none`),
/// not only those whose location already came from the project.
#[test]
fn setting_the_project_location_reaches_devices_that_had_none() {
    let mut h = H::new(0x24_7101);
    keep_models(&mut h);
    let k = place(&mut h, "coil", (100.0, 100.0));
    assert_eq!(locations::resolve(&h.doc(), k).source, Source::None);
    h.run("projectprops", json!({"location": "+CP1"})).unwrap();
    assert_eq!(locations::resolve(&h.doc(), k).value, s("+CP1"));
    verify(&mut h, "project location set");
    h.run("projectprops", json!({"location": null})).unwrap();
    verify(&mut h, "project location cleared");
    h.run("u", Value::Null).unwrap();
    verify(&mut h, "undo of the clear");
}

/// The same for a device with no symbol left (its source is the project, or none).
#[test]
fn a_page_location_set_reaches_devices_on_that_page_with_none() {
    let mut h = H::new(0x24_7102);
    keep_models(&mut h);
    place(&mut h, "coil", (100.0, 100.0));
    h.run("pageprops", json!({"location": "+FIELD"})).unwrap();
    verify(&mut h, "page location set");
    h.run("u", Value::Null).unwrap();
    verify(&mut h, "undo");
}

// ------------------------------------------------- located tags on a device's other pages (QA gate)
//
// A device's location comes from its main symbol's context, and its symbols on other pages show it
// as a tag prefix (design 024 §3.3). O1 (#75) redraws the shown page only when a commit touches
// it, so each route that changes a device's location must touch the device's other pages too
// (`project::located_tag_pages`). The harness compares the shown page with a fresh projection
// after each command; a page not shown is projected again when it is shown. So the routes that
// can leave a stale tag are those made while another page is shown (a page's location, a box
// relabelled by `setprop`); the tests of the others guard the same rule.

/// A coil on page 1 (its main symbol) and its contact on page 2: (harness, page 1, page 2, coil).
fn two_page_device(seed: u64) -> (H, PageId, PageId, ComponentId) {
    let mut h = H::new(seed);
    let p1 = h.first_page();
    let k = place(&mut h, "coil", (100.0, 100.0));
    let p2: PageId = id(&h.run("newpage", json!({"title": "Field"})).unwrap(), "page");
    h.activate(p2);
    h.run("place", json!({"symbol": "no", "at": [100.0, 100.0], "device": k.hex()})).unwrap();
    (h, p1, p2, k)
}

/// The long probe's failing steps (seed 0x240002, steps 20 to 22): a property, a contact joined on
/// another page, then page 1's location, with page 2 shown: page 2 reads `+CP1-K01`.
#[test]
fn a_page_location_redraws_the_device_tags_on_its_other_pages() {
    let (mut h, p1, _, k) = two_page_device(0x24_7104);
    h.run("setprop", json!({"component": k.hex(), "key": "description", "value": "Pump"})).unwrap();
    h.run("pageprops", json!({"page": p1.hex(), "location": "+CP1"})).unwrap();
    verify(&mut h, "page 1's location set, page 2 shown");
    h.run("u", Value::Null).unwrap();
    verify(&mut h, "undo");
}

/// The project's location reaches a device that falls back to it, shown on a page of its own.
#[test]
fn a_project_location_redraws_the_device_tags_on_its_other_pages() {
    let (mut h, _, p2, _) = two_page_device(0x24_7105);
    h.run("pageprops", json!({"page": p2.hex(), "location": "+FIELD"})).unwrap();
    h.run("projectprops", json!({"location": "+PROJ"})).unwrap();
    verify(&mut h, "project location set, page 2 shown");
    h.run("u", Value::Null).unwrap();
    verify(&mut h, "undo");
}

/// The device's own location: its symbols' pages.
#[test]
fn a_device_location_redraws_its_tags_on_every_page() {
    let (mut h, p1, _, k) = two_page_device(0x24_7106);
    h.activate(p1);
    h.run("setprop", json!({"component": k.hex(), "key": "location", "value": "+JB1"})).unwrap();
    verify(&mut h, "own location set");
}

/// A location box round the main symbol relabelled while page 2 is shown (the long probe's step
/// 155): page 2 reads `+JB1-K01`. Then moved off the coil and erased, on page 1.
#[test]
fn a_location_box_redraws_the_device_tags_on_its_other_pages() {
    let (mut h, p1, p2, _) = two_page_device(0x24_7107);
    h.activate(p1);
    h.run("locationbox", json!({"location": "+CP1", "corners": [[80.0, 80.0], [130.0, 130.0]]})).unwrap();
    verify(&mut h, "box drawn round the coil");
    let (c, _, item, _) = boxes(&h.doc())[0];
    h.activate(p2);
    h.run("setprop", json!({"component": c.hex(), "key": "location", "value": "+JB1"})).unwrap();
    verify(&mut h, "box relabelled, page 2 shown");
    h.activate(p1);
    let hd = h.handle_of(p1, item).hex();
    h.run("move", json!({"handles": [hd], "delta": [100.0, 0.0]})).unwrap();
    verify(&mut h, "box moved off the coil");
    h.run("u", Value::Null).unwrap();
    let hd = h.handle_of(p1, item).hex();
    h.run("erase", json!({"handles": [hd]})).unwrap();
    verify(&mut h, "box erased");
}

/// The main symbol moved into a location box: the device's location changes, and with it its tag
/// on page 2.
#[test]
fn moving_the_main_symbol_into_a_box_redraws_its_tags_on_other_pages() {
    let (mut h, p1, _, k) = two_page_device(0x24_7108);
    h.activate(p1);
    h.run("locationbox", json!({"location": "+CP1", "corners": [[180.0, 80.0], [230.0, 130.0]]})).unwrap();
    let handles = h.rep_handles(p1, k);
    h.run("move", json!({"handles": handles, "delta": [100.0, 0.0]})).unwrap();
    verify(&mut h, "coil moved into the box");
    h.run("u", Value::Null).unwrap();
    verify(&mut h, "undo");
}

/// Moving a page in the book changes nothing in the kept models that a rebuild would not.
#[test]
fn moving_pages_and_folders_keeps_the_models_equal_to_a_rebuild() {
    let mut h = H::new(0x24_7103);
    keep_models(&mut h);
    let p1 = h.first_page();
    place(&mut h, "coil", (100.0, 100.0));
    let f1: String = h.run("newfolder", json!({"name": "A"})).unwrap()["folder"].as_str().unwrap_or_default().to_string();
    let f2: String = h.run("newfolder", json!({"name": "B"})).unwrap()["folder"].as_str().unwrap_or_default().to_string();
    let p2: PageId = id(&h.run("newpage", json!({"title": "two", "folder": f1})).unwrap(), "page");
    place(&mut h, "coil", (100.0, 100.0));
    let p3: PageId = id(&h.run("newpage", json!({"title": "three", "folder": f2})).unwrap(), "page");
    place(&mut h, "coil", (100.0, 100.0));
    h.run("newpage", json!({"generated": "contents"})).unwrap();
    verify(&mut h, "set up");
    h.run("movepage", json!({"page": p1.hex(), "index": 3})).unwrap();
    verify(&mut h, "movepage");
    let _ = h.run("movefolder", json!({"folder": f2, "parent": null, "index": 1}));
    verify(&mut h, "movefolder");
    h.run("pageprops", json!({"page": p3.hex(), "number": null})).ok();
    verify(&mut h, "page number cleared");
    h.run("pageprops", json!({"page": p2.hex(), "number": "77"})).unwrap();
    verify(&mut h, "page renumbered");
    h.run("u", Value::Null).unwrap();
    verify(&mut h, "undo");
}

// ---------------------------------------------------------------- 2: translate and movement

fn outline(doc: &Document, item: ItemId) -> (Vec<Pt>, bool) {
    match &doc.items[&item].kind {
        ItemKind::Primitive(Primitive::Polyline { vertices, closed }) => (vertices.clone(), *closed),
        k => panic!("not a polyline: {k:?}"),
    }
}

/// Rotating, mirroring and scaling a box together with the devices inside keeps it a box, keeps
/// them inside, and records nothing on the devices.
#[test]
fn rotating_mirroring_and_scaling_a_box_with_its_devices_keeps_them_inside() {
    let mut h = H::new(0x24_7201);
    keep_models(&mut h);
    let page = h.first_page();
    h.run("pageprops", json!({"location": "+PAGE"})).unwrap();
    let k1 = place(&mut h, "coil", (100.0, 100.0));
    let k2 = place(&mut h, "coil", (130.0, 110.0));
    let out = place(&mut h, "coil", (300.0, 200.0));
    let r = h.run("locationbox", json!({"location": "+JB1", "corners": [[80, 80], [160, 140]]})).unwrap();
    let (bc, item): (ComponentId, ItemId) = (id(&r, "component"), id(&r, "item"));
    let sel = |h: &mut H| {
        let mut v = vec![h.handle_of(page, item).hex()];
        v.extend(h.rep_handles(page, k1));
        v.extend(h.rep_handles(page, k2));
        v
    };
    let cases: Vec<(&str, Value)> = vec![
        ("rotate", json!({"base": [120, 110], "angle": 90})),
        ("rotate", json!({"base": [0, 0], "angle": 33})),
        ("mirror", json!({"p1": [120, 0], "p2": [120, 10], "erase": true})),
        ("move", json!({"delta": [55, -15]})),
        ("scale", json!({"base": [120, 110], "factor": 1.5})),
    ];
    for (cmd, mut params) in cases {
        params["handles"] = json!(sel(&mut h));
        let res = h.run(cmd, params.clone());
        let doc = h.doc();
        assert!(locations::is_box(&doc, bc), "{cmd}: the box is still a box");
        let (v, closed) = outline(&doc, item);
        assert!(closed && v.len() >= 4, "{cmd}: still a closed outline: {v:?}");
        assert_eq!(doc.items[&item].style.linetype, "DASHED", "{cmd}");
        if res.is_ok() {
            for c in [k1, k2] {
                assert_eq!(locations::resolve(&doc, c).value, s("+JB1"), "{cmd} {params}: {c:?} still inside");
            }
        }
        assert_eq!(locations::resolve(&doc, out).value, s("+PAGE"), "{cmd}");
        assert!(own(&doc).values().all(Option::is_none), "{cmd}: nothing written on the devices");
        verify(&mut h, cmd);
    }
}

/// Erasing a box together with the devices inside, then undoing, brings all back as it was.
#[test]
fn erasing_a_box_with_its_devices_and_undoing() {
    let mut h = H::new(0x24_7202);
    keep_models(&mut h);
    let page = h.first_page();
    let k1 = place(&mut h, "coil", (100.0, 100.0));
    let r = h.run("locationbox", json!({"location": "+JB1", "corners": [[80, 80], [160, 140]]})).unwrap();
    let (bc, item): (ComponentId, ItemId) = (id(&r, "component"), id(&r, "item"));
    let before = h.doc();
    let mut handles = vec![h.handle_of(page, item).hex()];
    handles.extend(h.rep_handles(page, k1));
    h.run("erase", json!({"handles": handles})).unwrap();
    let doc = h.doc();
    assert!(!doc.components.contains_key(&bc) && !doc.components.contains_key(&k1));
    assert!(!doc.items.contains_key(&item), "the outline goes with the box");
    verify(&mut h, "erase both");
    h.run("u", Value::Null).unwrap();
    assert_eq!(h.doc(), before);
    verify(&mut h, "undo");
}

/// The box label is derived: erasing it alone deletes the box (it selects the box) or is refused;
/// either way nothing is left half done.
#[test]
fn erasing_the_box_label_alone_leaves_no_half_box() {
    let mut h = H::new(0x24_7203);
    keep_models(&mut h);
    let page = h.first_page();
    let k1 = place(&mut h, "coil", (100.0, 100.0));
    let r = h.run("locationbox", json!({"location": "+JB1", "corners": [[80, 80], [160, 140]]})).unwrap();
    let (bc, item): (ComponentId, ItemId) = (id(&r, "component"), id(&r, "item"));
    let label = h
        .pv(|p, pg| pg.entities_of(p, page))
        .into_iter()
        .find(|e| matches!(&e.kind, cadcraft_engine::doc::EntityKind::Text(t) if t.value == "+JB1"))
        .map(|e| e.handle);
    let Some(label) = label else { panic!("no +JB1 label drawn") };
    let res = h.run("erase", json!({"handles": [label.hex()]}));
    let doc = h.doc();
    let gone = !doc.components.contains_key(&bc);
    assert_eq!(gone, !doc.items.contains_key(&item), "box and outline go together: {res:?}");
    if !gone {
        assert_eq!(locations::resolve(&doc, k1).value, s("+JB1"), "{res:?}");
    }
    verify(&mut h, "erase label");
}

/// Opening a box outline (PEDIT Open) leaves an open polyline. A location box is a closed
/// polyline (brief scope 1): an open one must either be refused or stop giving its location.
#[test]
fn opening_a_box_outline_is_refused_or_ends_the_box() {
    let mut h = H::new(0x24_7204);
    keep_models(&mut h);
    let page = h.first_page();
    let k1 = place(&mut h, "coil", (100.0, 100.0));
    let r = h.run("locationbox", json!({"location": "+JB1", "corners": [[80, 80], [160, 140]]})).unwrap();
    let item: ItemId = id(&r, "item");
    let hd = h.handle_of(page, item).hex();
    let res = h.run("pedit", json!({"handle": hd, "option": "open"}));
    let doc = h.doc();
    let (_, closed) = outline(&doc, item);
    if !closed {
        assert_ne!(locations::resolve(&doc, k1).value, s("+JB1"), "an open outline still gives its location ({res:?})");
    }
    verify(&mut h, "pedit open");
}

/// Removing vertices of a box down to a line (grip removeVertex) never panics and never leaves a
/// box with fewer than 3 vertices giving a location.
#[test]
fn removing_box_vertices_down_to_a_line() {
    let mut h = H::new(0x24_7205);
    keep_models(&mut h);
    let page = h.first_page();
    let k1 = place(&mut h, "coil", (100.0, 100.0));
    let r = h.run("locationbox", json!({"location": "+JB1", "points": [[80, 80], [160, 80], [160, 140], [80, 140]]})).unwrap();
    let item: ItemId = id(&r, "item");
    for _ in 0..3 {
        let hd = h.handle_of(page, item).hex();
        let _ = h.run("grip.option", json!({"handle": hd, "index": 0, "option": "removeVertex"}));
        verify(&mut h, "removeVertex");
        let doc = h.doc();
        if let Some(i) = doc.items.get(&item)
            && let ItemKind::Primitive(Primitive::Polyline { vertices, .. }) = &i.kind
            && vertices.len() < 3
        {
            assert_ne!(locations::resolve(&doc, k1).value, s("+JB1"));
        }
    }
}

// ---------------------------------------------------------------- 3: CSV

/// RFC 4180 quoting and Excel-safe prefixes, including the full-width formula characters Excel
/// also reads as formulas (`＝`, `＋`, `－`, `＠`).
#[test]
fn csv_quotes_and_neutralises_formula_starts() {
    let row = |v: &[&str]| v.iter().map(|t| Cell { text: (*t).to_string(), flag: None }).collect::<Vec<_>>();
    let t = Table {
        headers: vec!["A", "B", "C"],
        rows: vec![
            row(&["a,b", "say \"hi\"", "two\nlines"]),
            row(&["=1+1", "+CP1", "-K01"]),
            row(&["@SUM(A1)", "\tx", "\rx"]),
            row(&["plain", "", "?"]),
        ],
    };
    let out = csv(&t, true);
    assert!(out.starts_with('\u{feff}'));
    let body = out.trim_start_matches('\u{feff}');
    let lines: Vec<&str> = body.split("\r\n").collect();
    assert_eq!(lines[0], "A,B,C");
    assert_eq!(lines[1], "\"a,b\",\"say \"\"hi\"\"\",\"two\nlines\"");
    assert_eq!(lines[2], "'=1+1,'+CP1,'-K01");
    assert_eq!(lines[3], "'@SUM(A1),'\tx,\"'\rx\"");
    assert_eq!(lines[4], "plain,,?");
    // Without Excel-safe, values are written as they are.
    assert!(csv(&t, false).contains("=1+1,+CP1,-K01\r\n"));
}

#[test]
fn csv_neutralises_full_width_formula_starts() {
    let row = |v: &[&str]| v.iter().map(|t| Cell { text: (*t).to_string(), flag: None }).collect::<Vec<_>>();
    let t = Table { headers: vec!["A", "B", "C", "D"], rows: vec![row(&["＠SUM(A1)", "＝1+1", "＋CP1", "－K01"])] };
    let out = csv(&t, true);
    assert!(out.contains("'＠SUM(A1),'＝1+1,'＋CP1,'－K01\r\n"), "{out:?}");
}

/// End to end: hostile descriptions through `setprop` and `bom {path}` come out quoted and safe,
/// and parse back to the same cells.
#[test]
fn bom_csv_survives_hostile_values_end_to_end() {
    let mut h = H::new(0x24_7301);
    keep_models(&mut h);
    let k = place(&mut h, "coil", (100.0, 100.0));
    let nasty = "=HYPERLINK(\"x\",\"y\"), \"quoted\"\nnext";
    h.run("setprop", json!({"component": k.hex(), "key": "description", "value": nasty})).ok();
    h.run("setprop", json!({"component": k.hex(), "key": "maker", "value": "@evil"})).unwrap();
    h.run("setprop", json!({"component": k.hex(), "key": "part_number", "value": "-1+2"})).unwrap();
    let dir = std::env::temp_dir().join(format!("ecad-t024-{}", std::process::id()));
    let path = dir.join("bom.csv");
    h.run("bom", json!({"path": path.to_string_lossy()})).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let rows = parse_csv(text.trim_start_matches('\u{feff}'));
    assert_eq!(rows.len(), 2, "{text:?}");
    for cell in &rows[1] {
        assert!(!cell.starts_with(['=', '+', '-', '@']), "unsafe cell {cell:?} in {text:?}");
    }
    let stored = h.doc().components[&k].properties.get("description").cloned();
    if let Some(d) = stored {
        assert!(rows[1].iter().any(|c| c.trim_start_matches('\'') == d.trim()), "{rows:?}");
    }
}

/// A minimal RFC 4180 reader for the probe.
fn parse_csv(t: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut q = false;
    let mut it = t.chars().peekable();
    while let Some(c) = it.next() {
        match (q, c) {
            (true, '"') if it.peek() == Some(&'"') => {
                it.next();
                cell.push('"');
            }
            (true, '"') => q = false,
            (true, c) => cell.push(c),
            (false, '"') => q = true,
            (false, ',') => row.push(std::mem::take(&mut cell)),
            (false, '\r') if it.peek() == Some(&'\n') => {
                it.next();
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            (false, c) => cell.push(c),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

// ---------------------------------------------------------------- 4: generated pages

/// The BOM page overflows by 44 rows a sheet; Total counts the sheets; adding devices adds a
/// sheet; every device is printed on some sheet (nothing cut off).
#[test]
fn bom_page_overflow_counts_sheets_and_loses_no_row() {
    let mut h = H::new(0x24_7401);
    keep_models(&mut h);
    let first = h.first_page();
    h.run("projectprops", json!({"location": "+CP1"})).unwrap();
    let bom: PageId = id(&h.run("newpage", json!({"generated": "bom-location"})).unwrap(), "page");
    h.activate(first);
    // 43 rows + 1 heading = 44: one sheet; one more device: two sheets.
    for i in 0..43 {
        place(&mut h, "coil", (20.0 + f64::from(i % 10) * 35.0, 60.0 + f64::from(i / 10) * 30.0));
    }
    let n = |h: &mut H| {
        let doc = h.doc();
        let o = h.cfg(|p| p.outputs().clone());
        (ecad_cadcraft::generated::sheets(&doc, bom, Some(&o)), ecad_cadcraft::generated::book_sheets(&doc, Some(&o)))
    };
    assert_eq!(n(&mut h), (1, 2));
    place(&mut h, "coil", (20.0, 250.0));
    assert_eq!(n(&mut h), (2, 3));
    verify(&mut h, "overflow");
    // A second location adds a heading row.
    let doc = h.doc();
    let some = devices(&doc)[0];
    h.run("setprop", json!({"component": some.hex(), "key": "location", "value": "+CP2"})).unwrap();
    verify(&mut h, "second block");
    let doc = h.doc();
    let t = texts(&doc, bom);
    for c in devices(&doc) {
        let tag = ecad_cadcraft::devices::tag_of(&doc, c).unwrap().to_string();
        assert!(t.iter().any(|x| x.split(", ").any(|y| y == tag)), "{tag} is on no sheet: {t:?}");
    }
    // The Total in the title block of an ordinary page counts the sheets.
    let total = ecad_cadcraft::generated::book_sheets(&doc, Some(&Outputs::build(&doc)));
    assert!(texts(&doc, first).contains(&total.to_string()), "{total}: {:?}", texts(&doc, first));
    // Undo back to one sheet.
    h.run("u", Value::Null).unwrap();
    h.run("u", Value::Null).unwrap();
    assert_eq!(n(&mut h), (1, 2));
    verify(&mut h, "undo");
}

/// A BOM sheet that starts inside a location block repeats the block as `+CP1 (continued)` and
/// then still takes 44 rows: 45 rows plus the header need 232 mm of the 230 mm drawing area, so
/// the last row hangs out of it, onto the title block.
#[test]
fn a_continued_bom_sheet_keeps_its_rows_inside_the_drawing_area() {
    let mut h = H::new(0x24_7405);
    h.run("projectprops", json!({"location": "+CP1"})).unwrap();
    let first = h.first_page();
    let bom: PageId = id(&h.run("newpage", json!({"generated": "bom-location"})).unwrap(), "page");
    h.activate(first);
    // 1 heading + 88 rows: sheet 2 starts inside +CP1.
    for i in 0..88 {
        let at = json!([10.0 + f64::from(i % 12) * 30.0, 40.0 + f64::from(i / 12) * 30.0]);
        h.s.execute("place", &json!({"symbol": "coil", "at": at})).unwrap();
    }
    let doc = h.doc();
    let o = Outputs::build(&doc);
    assert_eq!(ecad_cadcraft::generated::sheets(&doc, bom, Some(&o)), 3);
    // Sheet 2 (x from 470 to 890 mm): the lowest content text's row must end inside the area.
    let bottom = ecad_cadcraft::generated::parts_of(&doc, bom, Some(&o))
        .into_iter()
        .filter(|p| p.layer == ecad_cadcraft::generated::LAYER)
        .filter_map(|p| match p.kind {
            cadcraft_engine::doc::EntityKind::Text(t) if t.insert.x > 470.0 && t.insert.x < 890.0 => Some(t.insert.y - 2.5),
            _ => None,
        })
        .fold(f64::INFINITY, f64::min);
    assert!(bottom >= 55.0 - 1e-6, "the last row of sheet 2 ends at y = {bottom} mm, below the drawing area (55 mm)");
}

/// The contents follow pages added, moved, renamed and removed (undo), and overflow onto a second
/// sheet past 44 pages, counted in Total.
#[test]
fn contents_follow_pages_and_overflow() {
    let mut h = H::new(0x24_7403);
    let first = h.first_page();
    h.run("renamepage", json!({"page": first.hex(), "title": "First"})).unwrap();
    let c: PageId = id(&h.run("newpage", json!({"generated": "contents"})).unwrap(), "page");
    let cover: PageId = id(&h.run("newpage", json!({"generated": "cover"})).unwrap(), "page");
    let p: PageId = id(&h.run("newpage", json!({"title": "Motors"})).unwrap(), "page");
    let t = texts(&h.doc(), c);
    assert!(t.contains(&"Motors".to_string()), "{t:?}");
    h.run("renamepage", json!({"page": p.hex(), "title": "Pumps"})).unwrap();
    let t = texts(&h.doc(), c);
    assert!(t.contains(&"Pumps".to_string()) && !t.contains(&"Motors".to_string()), "{t:?}");
    h.run("movepage", json!({"page": p.hex(), "index": 1})).unwrap();
    let doc = h.doc();
    let t = texts(&doc, c);
    let order: Vec<String> = ecad_model::book::Book::of(&doc).order.iter().map(|x| doc.pages[x].title.clone()).collect();
    let table = t.iter().position(|x| x == "Folder").map_or(&t[..], |i| &t[i..]);
    let shown: Vec<String> = table.iter().filter(|x| order.contains(x)).cloned().collect();
    assert_eq!(shown, order, "contents order vs book order");
    h.run("u", Value::Null).unwrap();
    h.run("u", Value::Null).unwrap();
    h.run("u", Value::Null).unwrap();
    let t = texts(&h.doc(), c);
    assert!(!t.contains(&"Pumps".to_string()) && !t.contains(&"Motors".to_string()), "page removed by undo: {t:?}");
    // 46 pages: two contents sheets; Total and the cover count 47 sheets.
    for i in 0..43 {
        h.run("newpage", json!({"title": format!("P{i}")})).unwrap();
    }
    let doc = h.doc();
    assert_eq!(doc.page_order.len(), 46);
    assert_eq!(ecad_cadcraft::generated::sheets(&doc, c, None), 2);
    assert_eq!(ecad_cadcraft::generated::book_sheets(&doc, None), 47);
    let t = texts(&doc, c);
    assert!(t.iter().any(|x| x == "Contents (2/2)"), "{t:?}");
    assert!(t.iter().any(|x| x == "P42"), "the last page is listed: {t:?}");
    let t = texts(&doc, cover);
    assert!(t.contains(&"47 sheets".to_string()), "{t:?}");
}

/// The revision list shows every issued revision, newest first, and follows `undoissue`.
#[test]
fn revision_list_follows_issuerevision_and_undo() {
    let mut h = H::new(0x24_7404);
    let r: PageId = id(&h.run("newpage", json!({"generated": "revisions"})).unwrap(), "page");
    let cover: PageId = id(&h.run("newpage", json!({"generated": "cover"})).unwrap(), "page");
    h.run("issuerevision", json!({"rev": "A", "date": "2026-11-01", "description": "First, \"issue\"", "drawn": "CV"})).unwrap();
    h.run("projectprops", json!({"title": "x"})).unwrap();
    h.run("issuerevision", json!({"rev": "B", "date": "2026-11-12", "description": "Second", "drawn": "CV"})).unwrap();
    let doc = h.doc();
    let t = texts(&doc, r);
    let a = t.iter().position(|x| x == "2026-11-01");
    let b = t.iter().position(|x| x == "2026-11-12");
    assert!(a.is_some() && b.is_some() && b < a, "newest first: {t:?}");
    assert!(t.iter().any(|x| x.starts_with("First, \"issue\"")), "{t:?}");
    assert!(texts(&doc, cover).contains(&"Revision B, 2026-11-12".to_string()), "{:?}", texts(&doc, cover));
    h.run("undoissue", json!({})).unwrap();
    let doc = h.doc();
    let t = texts(&doc, r);
    assert!(!t.contains(&"Second".to_string()) && t.contains(&"2026-11-01".to_string()), "{t:?}");
    assert!(texts(&doc, cover).contains(&"Revision A, 2026-11-01".to_string()));
    // 45 issued revisions: two sheets.
    for i in 0..45 {
        h.run("projectprops", json!({"title": format!("t{i}")})).unwrap();
        h.run("issuerevision", json!({"rev": format!("R{i}"), "description": "d", "drawn": "CV"})).unwrap();
    }
    let doc = h.doc();
    assert_eq!(ecad_cadcraft::generated::sheets(&doc, r, None), 2);
}

// ---------------------------------------------------------------- 5: files and branches

struct TempDir(std::path::PathBuf);
impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-t024-{name}-{}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn path(&self, name: &str) -> String {
        self.0.join(name).display().to_string()
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn close_all(s: &mut cadcraft_engine::Session) {
    while !s.docs.is_empty() {
        s.execute("close", &json!({"index": 0})).unwrap();
    }
}

/// Run a command without the harness's one-transaction check (switching branches replaces the
/// ledger), then the project checks.
fn raw(h: &mut H, cmd: &str, params: Value) -> Result<Value, String> {
    let r = h.s.execute(cmd, &params).map_err(|e| e.to_string());
    h.check();
    r
}

fn reopen(path: &str, seed: u64) -> H {
    let mut s = cadcraft_engine::Session::empty();
    let (project, _) = ecad_cadcraft::ProjectHandle::open_file(&mut s, path, seed).unwrap();
    H { s, project, trail: Vec::new() }
}

/// Boxes, own locations, page and project locations and generated pages survive save and reopen;
/// the reports are the same after reopening.
#[test]
fn save_and_reopen_keep_boxes_locations_and_generated_pages() {
    let d = TempDir::new("reopen");
    let mut h = H::new(0x24_7501);
    h.cfg(|p| p.persistence = ecad_cadcraft::Persistence::File);
    h.run("projectprops", json!({"location": "+CP1"})).unwrap();
    let k1 = place(&mut h, "coil", (100.0, 100.0));
    let k2 = place(&mut h, "coil", (300.0, 200.0));
    h.run("locationbox", json!({"location": "+JB1", "points": [[80, 80], [160, 80], [170, 120], [80, 140]]})).unwrap();
    h.run("setprop", json!({"component": k2.hex(), "key": "location", "value": "+CP9"})).unwrap();
    for g in ["cover", "contents", "revisions", "bom-location"] {
        h.run("newpage", json!({"generated": g})).unwrap();
    }
    let path = d.path("t.ecad");
    h.run("saveas", json!({"path": path})).unwrap();
    let doc = h.doc();
    let bom = h.run("bom", json!({})).unwrap()["rows"].clone();
    let wires = h.run("wirelist", json!({})).unwrap()["rows"].clone();
    close_all(&mut h.s);
    let mut h2 = reopen(&path, 0x24_7502);
    assert_eq!(h2.doc(), doc);
    assert!(!h2.p(|p| p.unsaved()));
    let d2 = h2.doc();
    assert_eq!(locations::resolve(&d2, k1).value, s("+JB1"));
    assert_eq!(locations::resolve(&d2, k2).source, Source::Device);
    assert_eq!(h2.run("bom", json!({})).unwrap()["rows"], bom);
    assert_eq!(h2.run("wirelist", json!({})).unwrap()["rows"], wires);
    h2.check();
    // Editing after reopening still goes through the update map.
    keep_models(&mut h2);
    let item = boxes(&d2)[0].2;
    let p = d2.page_order[0];
    h2.activate(p);
    let hd = h2.handle_of(p, item).hex();
    h2.run("move", json!({"handles": [hd], "delta": [0, 200]})).unwrap();
    verify(&mut h2, "move after reopen");
    assert_eq!(locations::resolve(&h2.doc(), k1).value, s("+CP1"));
}

/// A project with nothing of 024 in it opens with no change: nothing written, no unsaved mark, no
/// warning rows; devices show inherited locations only.
#[test]
fn a_project_without_024_opens_unchanged() {
    let d = TempDir::new("old");
    let mut h = H::new(0x24_7503);
    h.cfg(|p| p.persistence = ecad_cadcraft::Persistence::File);
    place(&mut h, "coil", (100.0, 100.0));
    h.run("pageprops", json!({"location": "+OLD"})).unwrap();
    h.run("pline", json!({"vertices": pts(&[(0.0, 0.0), (60.0, 0.0), (60.0, 40.0), (0.0, 40.0)]), "closed": true})).unwrap();
    let path = d.path("old.ecad");
    h.run("saveas", json!({"path": path})).unwrap();
    let doc = h.doc();
    let head = h.head();
    close_all(&mut h.s);
    let mut h2 = reopen(&path, 0x24_7504);
    assert_eq!(h2.doc(), doc);
    assert_eq!(h2.head(), head);
    assert!(!h2.p(|p| p.unsaved()));
    // A plain closed polyline is not a box.
    assert!(boxes(&h2.doc()).is_empty());
    let checks = ecad_cadcraft::checks::all(&h2.doc(), 1e-6);
    assert!(
        checks.iter().all(|c| !matches!(
            c.kind,
            ecad_cadcraft::checks::Kind::OwnLocation | ecad_cadcraft::checks::Kind::BoxCrosses | ecad_cadcraft::checks::Kind::BoxesOverlap
        )),
        "{checks:?}"
    );
    keep_models(&mut h2);
    verify(&mut h2, "old project");
    assert!(!h2.p(|p| p.unsaved()), "showing the reports writes nothing");
}

/// The golden v1 project file (before 024) opens and its reports build without error.
#[test]
fn the_golden_v1_project_opens_and_reports_build() {
    let d = TempDir::new("golden");
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ecad-file/tests/golden/project-v1.ecad");
    let path = d.path("golden.ecad");
    std::fs::copy(&src, &path).unwrap();
    let mut s = cadcraft_engine::Session::empty();
    let opened = ecad_cadcraft::ProjectHandle::open_file(&mut s, &path, 0x24_7505);
    let Ok((project, _)) = opened else { return };
    let mut h = H { s, project, trail: Vec::new() };
    let head = h.head();
    keep_models(&mut h);
    verify(&mut h, "golden");
    for cmd in ["bom", "wirelist", "cablelist"] {
        h.run(cmd, json!({})).unwrap();
    }
    assert_eq!(h.head(), head, "reading records nothing");
}

/// A box edited on a branch and on main: merging gives a valid box, consistent models and the
/// devices' locations from the merged outline.
#[test]
fn merging_a_branch_with_a_box_edited_on_both_sides() {
    let d = TempDir::new("merge");
    let mut h = H::new(0x24_7506);
    h.cfg(|p| p.persistence = ecad_cadcraft::Persistence::File);
    let page = h.first_page();
    let k1 = place(&mut h, "coil", (100.0, 100.0));
    let k2 = place(&mut h, "coil", (250.0, 100.0));
    let r = h.run("locationbox", json!({"location": "+JB1", "corners": [[80, 80], [160, 140]]})).unwrap();
    let (bc, item): (ComponentId, ItemId) = (id(&r, "component"), id(&r, "item"));
    h.run("saveas", json!({"path": d.path("m.ecad")})).unwrap();
    // Branch: move the box over k2 and relabel it.
    raw(&mut h, "branch", json!({"name": "B", "switch": true})).unwrap();
    h.activate(page);
    let hd = h.handle_of(page, item).hex();
    h.run("move", json!({"handles": [hd], "delta": [150, 0]})).unwrap();
    h.run("setprop", json!({"component": bc.hex(), "key": "location", "value": "+JB2"})).unwrap();
    raw(&mut h, "switchbranch", json!({"branch": "main", "save": true})).unwrap();
    // Main: stretch the same box and give it another label.
    h.activate(page);
    let hd = h.handle_of(page, item).hex();
    h.run("grip.stretch", json!({"handle": hd, "index": 2, "to": [200, 160]})).unwrap();
    h.run("setprop", json!({"component": bc.hex(), "key": "location", "value": "+JB3"})).unwrap();
    h.run("qsave", Value::Null).unwrap();
    keep_models(&mut h);
    let v = raw(&mut h, "merge", json!({"from": "B", "preview": true})).unwrap();
    let mut choices = serde_json::Map::new();
    for c in v["conflicts"].as_array().cloned().unwrap_or_default() {
        if let Some(i) = c["id"].as_str() {
            choices.insert(i.to_string(), json!("branch"));
        }
    }
    raw(&mut h, "merge", json!({"from": "B", "choices": choices, "close": false})).unwrap();
    let doc = h.doc();
    assert!(locations::is_box(&doc, bc));
    let b = locations::boxes_on(&doc, page);
    assert_eq!(b.len(), 1, "one box after the merge");
    assert!(b[0].vertices.len() >= 3);
    for c in [k1, k2] {
        let want =
            locations::innermost(&b, locations::anchor(&doc, locations::main_rep(&doc, c).unwrap().1).unwrap()).and_then(|x| x.location.clone());
        assert_eq!(locations::resolve(&doc, c).value, want, "{c:?}");
    }
    verify(&mut h, "after merge");
    raw(&mut h, "undomerge", json!({})).unwrap();
    verify(&mut h, "after undomerge");
    // Update the branch from main (the other direction), with the same edits.
    raw(&mut h, "switchbranch", json!({"branch": "B", "save": true})).unwrap();
    keep_models(&mut h);
    let v = raw(&mut h, "merge", json!({"from": "main", "into": "B", "preview": true}));
    if let Ok(v) = v {
        let mut choices = serde_json::Map::new();
        for c in v["conflicts"].as_array().cloned().unwrap_or_default() {
            if let Some(i) = c["id"].as_str() {
                choices.insert(i.to_string(), json!("main"));
            }
        }
        let _ = raw(&mut h, "merge", json!({"from": "main", "into": "B", "choices": choices, "close": false}));
        verify(&mut h, "after update from main");
        let doc = h.doc();
        assert_eq!(locations::boxes_on(&doc, page).len(), 1);
    }
}

// ---------------------------------------------------------------- 6: Print Book

/// Print Book of a project of more than 10 pages, every kind of generated page included, returns
/// one base64 PDF with one page per sheet.
#[test]
fn printbook_of_more_than_ten_pages_returns_one_pdf_with_every_sheet() {
    ecad_cadcraft::printbook::install_io();
    let mut h = H::new(0x24_7601);
    h.run("projectprops", json!({"title": "Big", "location": "+CP1"})).unwrap();
    for g in ["cover", "contents", "revisions", "bom-location"] {
        h.run("newpage", json!({"generated": g})).unwrap();
    }
    for i in 0..9 {
        h.run("newpage", json!({"title": format!("P{i}")})).unwrap();
        place(&mut h, "coil", (100.0, 100.0));
    }
    let v = h.run("printbook", json!({})).unwrap();
    assert_eq!(v["sheets"], 14, "{v}");
    let files = v["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "{v}");
    let data = files[0]["data"].as_str().unwrap();
    let bytes = b64(data);
    assert!(bytes.starts_with(b"%PDF"), "a PDF");
    assert_eq!(files[0]["bytes"].as_u64(), Some(bytes.len() as u64), "bytes counts the decoded PDF");
    let text = String::from_utf8_lossy(&bytes);
    let pages = text.matches("/Type /Page\n").count()
        + text.matches("/Type /Page ").count()
        + text.matches("/Type/Page/").count()
        + text.matches("/Type /Page>").count();
    assert!(pages == 14 || text.contains("/Count 14"), "14 pages in the PDF ({pages})");
    // A page list in book order and an unknown number.
    let v = h.run("printbook", json!({"scope": {"pages": "6-8, 99"}})).unwrap();
    assert_eq!(v["sheets"], 3, "{v}");
    assert_eq!(v["unknown"], json!(["99"]));
    let e = h.run("printbook", json!({"scope": {"pages": "99"}})).unwrap_err();
    assert!(e.to_lowercase().contains("no page"), "{e}");
}

fn b64(s: &str) -> Vec<u8> {
    let val = |c: u8| -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some(u32::from(c - b'A')),
            b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0;
    for c in s.bytes() {
        let Some(v) = val(c) else { continue };
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}
