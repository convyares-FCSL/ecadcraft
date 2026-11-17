//! Brief 052b (design 052 §5.1–§5.2, §5.7–§5.9, §6, §10.6–14a): function and location stored on
//! the device, overrides, tag prefixes, the project's list (rename, merge, remove) and its Checks.

use crate::common::{H, id};

use ecad_cadcraft::checks::{self, Kind};
use ecad_cadcraft::designations::{self, Aspect};
use ecad_cadcraft::{devices, locations};
use ecad_model::{DeviceId, Document, PageId};
use serde_json::{Value, json};

const SEED: u64 = 521;

fn page(h: &mut H, number: &str) -> PageId {
    id(&h.run("newpage", json!({"title": number, "number": number})).unwrap(), "page")
}

fn texts(d: &Document, p: PageId) -> Vec<String> {
    devices::texts_on_page(d, p).into_iter().map(|t| t.value).collect()
}

fn set(h: &mut H, c: DeviceId, aspect: &str, value: Value) -> Value {
    h.run("setlocation", json!({"component": c.hex(), "aspect": aspect, "value": value})).unwrap()
}

fn code(d: &Document, c: DeviceId) -> Option<String> {
    locations::resolve(d, c).value
}

/// A coil `-K01` on page 105 and its NO contact on page 110.
fn k01(h: &mut H) -> (DeviceId, PageId, PageId) {
    let p105 = h.first_page();
    h.run("pageprops", json!({"page": p105.hex(), "number": "105"})).unwrap();
    let k = id::<DeviceId>(&h.run("place", json!({"symbol": "coil", "at": [75, 180], "tag": "-K01"})).unwrap(), "component");
    let p110 = page(h, "110");
    h.run("place", json!({"symbol": "no", "at": [100, 150], "device": k.hex()})).unwrap();
    (k, p105, p110)
}

/// §10.6, §5.1: a box around a contact proposes; it never changes the device's stored location.
/// The contact shows the device's location as a prefix.
#[test]
fn a_box_never_changes_a_stored_location() {
    let mut h = H::new(SEED);
    let (k, _p105, p110) = k01(&mut h);
    let v = set(&mut h, k, "location", json!("+CP1"));
    assert_eq!(v["location"], "+CP1");
    assert!(v["message"].as_str().is_some_and(|m| m.contains("New location +CP1 added")), "{v}");
    h.run("locationbox", json!({"location": "+FIELD", "page": p110.hex(), "corners": [[60, 110], [140, 190]]})).unwrap();
    let d = h.doc();
    assert_eq!(code(&d, k).as_deref(), Some("+CP1"));
    assert!(texts(&d, p110).contains(&"+CP1-K01".to_string()), "{:?}", texts(&d, p110));
    // The stored value is the entry's key, not its code (FP8).
    let stored = d.devices[&k].properties.get("location").cloned().unwrap_or_default();
    assert_ne!(stored, "+CP1");
    assert_eq!(designations::code_of(&d, Aspect::Location, &stored).as_deref(), Some("+CP1"));
    // Neither the coil's page (no location) nor the box (+FIELD) is +CP1: Checks says so.
    let rows = checks::all(&d, 1e-6);
    assert!(rows.iter().any(|r| r.kind == Kind::LocationUnused && r.which.contains("-K01 is +CP1")), "{rows:?}");
}

/// §10.11, §5.8: function stored like location; prefixes `=F1-K01` and `=F1+CP1-K01`.
#[test]
fn function_is_stored_and_prefixed_like_location() {
    let mut h = H::new(SEED);
    let (k, p105, p110) = k01(&mut h);
    h.run("setfunction", json!({"component": k.hex(), "value": "=F1"})).unwrap();
    h.run("pageprops", json!({"page": p105.hex(), "function": "=F1", "location": "+CP1"})).unwrap();
    set(&mut h, k, "location", json!("+CP1"));
    let d = h.doc();
    assert!(texts(&d, p105).contains(&"-K01".to_string()), "same contexts: no prefix {:?}", texts(&d, p105));
    h.run("pageprops", json!({"page": p110.hex(), "function": "=F2"})).unwrap();
    let d = h.doc();
    assert!(texts(&d, p110).contains(&"=F1+CP1-K01".to_string()), "{:?}", texts(&d, p110));
    h.run("pageprops", json!({"page": p110.hex(), "location": "+CP1"})).unwrap();
    let d = h.doc();
    assert!(texts(&d, p110).contains(&"=F1-K01".to_string()), "{:?}", texts(&d, p110));
    let v = h.run("designations", json!({"aspect": "function"})).unwrap();
    let rows = v["functions"]["entries"].as_array().unwrap();
    let f1 = rows.iter().find(|r| r["code"] == "=F1").unwrap();
    assert_eq!(f1["devices"], 1, "{v}");
    assert_eq!(f1["pages"], 1, "{v}");
    let f2 = rows.iter().find(|r| r["code"] == "=F2").unwrap();
    assert_eq!(f2["devices"], 0, "{v}");
    // A function has no overrides.
    let e = h.run("setfunction", json!({"component": k.hex(), "value": "=F1", "type": "terminal"})).unwrap_err();
    assert!(e.contains("no overrides"), "{e}");
}

/// §10.8, §5.2: a connector's halves take `location.socket` / `location.plug`; the retired
/// `half.<s>.location` key is read as the override; the BOM rows follow.
#[test]
fn connector_halves_have_their_own_locations() {
    let mut h = H::new(SEED);
    let v = h.run("connector", json!({"halves": "both", "family": "plain", "pins": 4, "layout": "row", "at": [50, 50]})).unwrap();
    let x: DeviceId = id(&v, "component");
    set(&mut h, x, "location", json!("+CP1"));
    h.run("setlocation", json!({"component": x.hex(), "type": "plug", "value": "+FIELD"})).unwrap();
    let d = h.doc();
    let rows = ecad_cadcraft::connectors::half_rows(&d, x, &mut locations::BoxCache::default());
    let by_side: Vec<(String, Option<String>)> = rows.iter().map(|r| (r.tag.clone(), r.location.value.clone())).collect();
    assert!(by_side.iter().any(|(t, l)| t.ends_with(".S") && l.as_deref() == Some("+CP1")), "{by_side:?}");
    assert!(by_side.iter().any(|(t, l)| t.ends_with(".P") && l.as_deref() == Some("+FIELD")), "{by_side:?}");
    // The old key routes to the override.
    h.run("setprop", json!({"component": x.hex(), "key": "half.s.location", "value": "+CP2"})).unwrap();
    let d = h.doc();
    assert!(d.devices[&x].properties.contains_key("location.socket"));
    assert!(!d.devices[&x].properties.contains_key("half.s.location"));
    // The head is the device.
    let e = h.run("setlocation", json!({"component": x.hex(), "type": "head", "value": "+CP3"})).unwrap_err();
    assert!(e.contains("The head is the device"), "{e}");
}

/// §10.10, Q6: a strip's terminal in another box keeps the strip's location.
#[test]
fn a_strip_terminal_keeps_the_strip_location() {
    let mut h = H::new(SEED);
    let v = h.run("place", json!({"symbol": "terminal", "at": [50, 150]})).unwrap();
    let x: DeviceId = id(&v, "component");
    set(&mut h, x, "location", json!("+CP1"));
    h.run("locationbox", json!({"location": "+JB1", "corners": [[150, 100], [250, 200]]})).unwrap();
    let t5 = h.run("place", json!({"symbol": "terminal", "at": [200, 150], "device": x.hex()})).unwrap();
    let d = h.doc();
    let r = &d.representations[&id(&t5, "representation")];
    assert_eq!(locations::rep_location(&d, x, r).value.as_deref(), Some("+CP1"));
    let e = h.run("setlocation", json!({"component": x.hex(), "type": "terminal", "value": "+JB1"})).unwrap_err();
    assert!(e.contains("split strip is two strips"), "{e}");
}

/// §10.12: a rename is one transaction; every tag and box label follows; undo restores; a rename
/// onto an existing code is refused and writes nothing.
#[test]
fn renaming_an_entry_renames_it_everywhere() {
    let mut h = H::new(SEED);
    let (k, _p105, p110) = k01(&mut h);
    set(&mut h, k, "location", json!("+CP1"));
    h.run("locationbox", json!({"location": "+CP1", "page": p110.hex(), "corners": [[300, 100], [400, 200]]})).unwrap();
    h.run("designationadd", json!({"aspect": "location", "code": "+CP2"})).unwrap();
    let head = h.head();
    let v = h.run("designationedit", json!({"aspect": "location", "code": "+CP1", "to": "+CP10"})).unwrap();
    assert_eq!(v["code"], "+CP10");
    assert_eq!(h.head(), head + 1, "one transaction");
    let d = h.doc();
    assert_eq!(code(&d, k).as_deref(), Some("+CP10"));
    assert!(texts(&d, p110).contains(&"+CP10-K01".to_string()), "{:?}", texts(&d, p110));
    let labels: Vec<String> = locations::labels_on_page(&d, p110).into_iter().map(|(_, t, _)| t).collect();
    assert_eq!(labels, ["+CP10"]);
    h.cmdline("u");
    assert_eq!(code(&h.doc(), k).as_deref(), Some("+CP1"));
    let head = h.head();
    let e = h.run("designationedit", json!({"aspect": "location", "code": "+CP1", "to": "+cp2"})).unwrap_err();
    assert!(e.contains("+CP2 is already in the list. Merge +CP1 into it instead?"), "{e}");
    assert_eq!(h.head(), head, "nothing written");
}

/// §10.13, §10.14, §10.14a: remove only when unused; merge moves every user in one transaction
/// and undo restores both; a typed unknown code adds its entry in the same transaction.
#[test]
fn remove_merge_and_typed_codes() {
    let mut h = H::new(SEED);
    let (k, p105, _p110) = k01(&mut h);
    set(&mut h, k, "location", json!("+CP01"));
    h.run("pageprops", json!({"page": p105.hex(), "location": "+CP01"})).unwrap();
    h.run("designationadd", json!({"aspect": "location", "code": "+CP1", "description": "Main panel"})).unwrap();
    let e = h.run("designationremove", json!({"aspect": "location", "code": "+CP01"})).unwrap_err();
    assert!(e.contains("+CP01 is used by 1 device and 1 page"), "{e}");
    let head = h.head();
    h.run("designationmerge", json!({"aspect": "location", "from": "+CP01", "into": "+CP1"})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    let d = h.doc();
    assert_eq!(code(&d, k).as_deref(), Some("+CP1"));
    assert!(designations::find(&d, Aspect::Location, "+CP01").is_none());
    let v = h.run("designations", json!({"aspect": "location"})).unwrap();
    let rows = v["locations"]["entries"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{v}");
    assert_eq!((rows[0]["devices"].as_u64(), rows[0]["pages"].as_u64()), (Some(1), Some(1)), "{v}");
    h.cmdline("u");
    let d = h.doc();
    assert_eq!(code(&d, k).as_deref(), Some("+CP01"));
    assert!(designations::find(&d, Aspect::Location, "+CP1").is_some());
    // Unused now? +CP1 is: removing it works.
    h.run("designationremove", json!({"aspect": "location", "code": "+CP1"})).unwrap();
    // §10.14a: a typed code not in the list adds its entry, and one undo removes both.
    let head = h.head();
    set(&mut h, k, "location", json!("+CP4"));
    assert_eq!(h.head(), head + 1);
    assert!(designations::find(&h.doc(), Aspect::Location, "+CP4").is_some());
    h.cmdline("u");
    let d = h.doc();
    assert!(designations::find(&d, Aspect::Location, "+CP4").is_none());
    assert_eq!(code(&d, k).as_deref(), Some("+CP01"));
    // The list is changed through its commands only.
    let e = h.run("projectprops", json!({"props": {"designation.l.x.code": "+X"}})).unwrap_err();
    assert!(e.contains("designationadd"), "{e}");
}

/// §6, §7: a stored value naming no entry (a v0.2.0 code) shows `?` and raises
/// `designation-missing`; two entries with one code raise `designation-duplicate`.
#[test]
fn missing_and_duplicate_entries_are_listed() {
    let mut h = H::new(SEED);
    let (k, _, _) = k01(&mut h);
    let mut d = h.doc();
    if let Some(c) = d.devices.get_mut(&k) {
        c.properties.insert("location".into(), "+CP1".into());
    }
    // Design 072 §5.3: the ops refuse a code twice (I22); a hand-built document still shows it.
    for (id, code) in [(0xaa, "=F1"), (0xbb, "=f1")] {
        d.project.functions.push(ecad_model::Function::new(ecad_model::FunctionId(id), ecad_model::ListCode::new(code).unwrap()));
    }
    d.rebuild_index();
    assert_eq!(code(&d, k), None, "a v0.2.0 code shows ?");
    assert!(locations::resolve(&d, k).missing());
    let rows = checks::all(&d, 1e-6);
    assert!(rows.iter().any(|r| r.kind == Kind::DesignationMissing && r.which == "-K01's location is no longer in the list"), "{rows:?}");
    assert!(rows.iter().any(|r| r.kind == Kind::DesignationDuplicate && r.which == "Two functions are called =F1"), "{rows:?}");
}

/// §6: `two-heads` names both places and points at the second.
#[test]
fn two_heads_are_listed() {
    let mut h = H::new(SEED);
    let (k, _, _) = k01(&mut h);
    let other = h.run("place", json!({"symbol": "coil", "at": [200, 150]})).unwrap();
    let mut d = h.doc();
    let rid = id(&other, "representation");
    if let Some(r) = d.representations.get_mut(&rid) {
        r.subject = ecad_model::Subject::Device(k);
    }
    d.rebuild_index();
    let rows = checks::all(&d, 1e-6);
    let two: Vec<_> = rows.iter().filter(|r| r.kind == Kind::TwoHeads).collect();
    assert_eq!(two.len(), 1, "{rows:?}");
    assert!(two[0].which.starts_with("-K01 has two coils ("), "{}", two[0].which);
}
