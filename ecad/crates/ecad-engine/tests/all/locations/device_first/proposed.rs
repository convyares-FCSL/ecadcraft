//! Brief 052d (design 052 §5.3–§5.6, §5.8, §10.17–21): placements, moves, boxes, pages and the
//! project propose a function and a location. Unset devices take them; a device with another
//! value is listed (Leave by default) and never changed silently.

use crate::common::{H, id};

use ecad_engine::locations;
use ecad_model::{DeviceId, Document};
use serde_json::{Value, json};

const SEED: u64 = 522;

fn loc(d: &Document, c: DeviceId) -> Option<String> {
    locations::resolve(d, c).value
}

fn func(d: &Document, c: DeviceId) -> Option<String> {
    locations::resolve_function(d, c).value
}

fn conflicts(h: &mut H) -> Value {
    h.run("conflicts", json!({})).unwrap()["conflicts"].clone()
}

fn place(h: &mut H, symbol: &str, at: [i32; 2]) -> (DeviceId, Value) {
    let v = h.run("place", json!({"symbol": symbol, "at": at})).unwrap();
    (id(&v, "component"), v)
}

/// §10.17: a new device placed in a box takes its location in the same transaction; one undo
/// removes both.
#[test]
fn a_new_device_in_a_box_takes_its_location_in_one_step() {
    let mut h = H::new(SEED);
    h.run("locationbox", json!({"location": "+CP1", "corners": [[20, 100], [200, 220]]})).unwrap();
    let head = h.head();
    let (k, v) = place(&mut h, "coil", [75, 180]);
    assert_eq!(h.head(), head + 1, "one transaction");
    assert_eq!(loc(&h.doc(), k).as_deref(), Some("+CP1"));
    assert_eq!(v["location"]["set"][0]["value"], "+CP1", "{v}");
    h.cmdline("u");
    assert!(!h.doc().devices.contains_key(&k), "one undo removes the device and its location");
    // A device placed outside every box, with no page or project value: no location.
    let (k2, _) = place(&mut h, "coil", [300, 50]);
    assert_eq!(loc(&h.doc(), k2), None);
    // JSON `keep` proposes nothing; a code sets it.
    let v = h.run("place", json!({"symbol": "coil", "at": [75, 160], "location": "keep"})).unwrap();
    assert_eq!(loc(&h.doc(), id(&v, "component")), None);
    let v = h.run("place", json!({"symbol": "coil", "at": [75, 140], "location": "+CP7"})).unwrap();
    assert_eq!(loc(&h.doc(), id(&v, "component")).as_deref(), Some("+CP7"));
}

/// §10.18, §10.19: moving an unset device's symbol into a box sets it in a follow-up transaction
/// (`u` clears it, `u` again moves back); a device with another value is listed, defaulting to
/// Leave, and Move sets it.
#[test]
fn a_move_into_a_box_proposes_its_location() {
    let mut h = H::new(SEED);
    let page = h.first_page();
    h.run("locationbox", json!({"location": "+CP1", "corners": [[200, 100], [380, 220]]})).unwrap();
    let (k, _) = place(&mut h, "coil", [75, 180]);
    assert_eq!(loc(&h.doc(), k), None);
    let item = h.symbol_item(k);
    let hd = h.handle_of(page, item);
    let head = h.head();
    h.run("move", json!({"handles": [hd.hex()], "delta": [200, 0]})).unwrap();
    assert_eq!(h.head(), head + 2, "the move, then its follow-up");
    assert_eq!(loc(&h.doc(), k).as_deref(), Some("+CP1"));
    let x = |h: &mut H| match &h.doc().items[&item].kind {
        ecad_model::ItemKind::Symbol(sym) => sym.placement.origin.x,
        ecad_model::ItemKind::Primitive(_) => f64::NAN,
    };
    h.cmdline("u");
    assert_eq!(loc(&h.doc(), k), None, "u undoes the follow-up first");
    assert!((x(&mut h) - 275.0).abs() < 1e-9, "the move is still there");
    h.cmdline("u");
    assert!((x(&mut h) - 75.0).abs() < 1e-9, "u again moves it back");
    // -K05 at +CP2 with all its symbols moved into +CP1: asked, Leave by default.
    let (k5, _) = place(&mut h, "coil", [75, 120]);
    h.run("setlocation", json!({"component": k5.hex(), "value": "+CP2"})).unwrap();
    let item = h.symbol_item(k5);
    let hd = h.handle_of(page, item);
    let head = h.head();
    h.run("move", json!({"handles": [hd.hex()], "delta": [200, 0]})).unwrap();
    assert_eq!(h.head(), head + 1, "only the move: nothing is set");
    assert_eq!(loc(&h.doc(), k5).as_deref(), Some("+CP2"));
    let c = conflicts(&mut h);
    assert_eq!(c["location"]["conflicts"][0]["proposed"], "+CP1", "{c}");
    assert_eq!(c["location"]["conflicts"][0]["all_symbols"], true, "{c}");
    // Leave writes nothing; the list is gone.
    h.run("conflicts", json!({"answer": "leave"})).unwrap();
    assert_eq!(h.head(), head + 1);
    assert_eq!(conflicts(&mut h)["location"]["conflicts"], json!([]));
    // Move again (back and in) and answer Move: one transaction sets it.
    h.run("move", json!({"handles": [hd.hex()], "delta": [-200, 0]})).unwrap();
    h.run("move", json!({"handles": [hd.hex()], "delta": [200, 0]})).unwrap();
    let head = h.head();
    h.run("conflicts", json!({"answer": "move"})).unwrap();
    assert_eq!(h.head(), head + 1);
    assert_eq!(loc(&h.doc(), k5).as_deref(), Some("+CP1"));
    // Moving a symbol out of every box never prompts.
    h.run("move", json!({"handles": [hd.hex()], "delta": [-200, 0]})).unwrap();
    assert_eq!(conflicts(&mut h)["location"]["conflicts"], json!([]));
    assert_eq!(loc(&h.doc(), k5).as_deref(), Some("+CP1"));
}

/// §10.20, §5.6: a box drawn around devices sets the unset ones and lists the others; a box given
/// another entry lists the devices at its old value only.
#[test]
fn a_box_proposes_when_drawn_and_when_given_another_entry() {
    let mut h = H::new(SEED);
    let (a, _) = place(&mut h, "coil", [50, 150]);
    let (b, _) = place(&mut h, "coil", [100, 150]);
    let (c, _) = place(&mut h, "coil", [150, 150]);
    h.run("setlocation", json!({"component": b.hex(), "value": "+CP1"})).unwrap();
    h.run("setlocation", json!({"component": c.hex(), "value": "+CP2"})).unwrap();
    let v = h.run("locationbox", json!({"location": "+CP1", "corners": [[20, 100], [200, 220]]})).unwrap();
    assert_eq!(v["message"], "Location box +CP1: 3 devices inside; 1 set to +CP1, 1 already +CP1, 1 kept.", "{v}");
    assert_eq!(v["conflicts"][0]["from"], "+CP2", "{v}");
    let d = h.doc();
    assert_eq!(loc(&d, a).as_deref(), Some("+CP1"));
    assert_eq!(loc(&d, c).as_deref(), Some("+CP2"), "never changed silently");
    // Give the box +CP3: devices at +CP1 are listed, the one at +CP2 is not.
    let bx: ecad_model::AidId = id(&v, "aid");
    h.run("setprop", json!({"aids": [bx.hex()], "key": "location", "value": "+CP3"})).unwrap();
    let list = conflicts(&mut h)["location"]["conflicts"].clone();
    let tags: Vec<&str> = list.as_array().unwrap().iter().filter_map(|x| x["tag"].as_str()).collect();
    assert_eq!(tags.len(), 2, "{list}");
    assert!(list.as_array().unwrap().iter().all(|x| x["location"] == "+CP1"), "{list}");
    // A page value proposes for the symbols outside every box.
    let (e, _) = place(&mut h, "coil", [300, 50]);
    h.run("pageprops", json!({"location": "+CP9"})).unwrap();
    assert_eq!(loc(&h.doc(), e).as_deref(), Some("+CP9"));
}

/// §10.21, §5.8: a function box around a location box: a new device inside takes both in one
/// transaction; a device with another function is listed for its function only.
#[test]
fn a_function_box_around_a_location_box_proposes_both() {
    let mut h = H::new(SEED);
    let v = h.run("functionbox", json!({"function": "=F1", "corners": [[10, 90], [260, 230]]})).unwrap();
    assert!(v["message"].as_str().is_some_and(|m| m.starts_with("Function box =F1")), "{v}");
    h.run("locationbox", json!({"location": "+CP1", "corners": [[20, 100], [200, 220]]})).unwrap();
    let d = h.doc();
    let labels: Vec<String> = locations::labels_on_page(&d, h.first_page()).into_iter().map(|(_, t, _)| t).collect();
    assert!(labels.contains(&"=F1".to_string()) && labels.contains(&"+CP1".to_string()), "{labels:?}");
    let head = h.head();
    let (k, _) = place(&mut h, "coil", [75, 180]);
    assert_eq!(h.head(), head + 1);
    let d = h.doc();
    assert_eq!((func(&d, k).as_deref(), loc(&d, k).as_deref()), (Some("=F1"), Some("+CP1")));
    // A device at =F2 +CP1 joins a symbol inside: one row, the function.
    let (m, _) = place(&mut h, "coil", [400, 50]);
    h.run("setfunction", json!({"component": m.hex(), "value": "=F2"})).unwrap();
    h.run("setlocation", json!({"component": m.hex(), "value": "+CP1"})).unwrap();
    let v = h.run("place", json!({"symbol": "no", "at": [120, 160], "device": m.hex()})).unwrap();
    assert_eq!(v["function"]["conflicts"].as_array().map(Vec::len), Some(1), "{v}");
    assert_eq!(v["location"]["conflicts"], json!([]), "{v}");
    assert_eq!(func(&h.doc(), m).as_deref(), Some("=F2"));
    // JSON `move`: the conflicting function is moved in the same transaction.
    let v = h.run("place", json!({"symbol": "no", "at": [140, 160], "device": m.hex(), "function": "move"})).unwrap();
    assert_eq!(func(&h.doc(), m).as_deref(), Some("=F1"), "{v}");
}

/// §5.3: a connector half placed where the connector has another location is asked about with
/// Half, which sets only that half's `location.<side>`; JSON `location: "half"` answers it.
#[test]
fn a_connector_half_answers_half() {
    let mut h = H::new(SEED);
    let v = h.run("place", json!({"symbol": "pins", "at": [300, 40]})).unwrap();
    let x: DeviceId = id(&v, "component");
    h.run("setlocation", json!({"component": x.hex(), "value": "+CP1"})).unwrap();
    h.run("locationbox", json!({"location": "+FIELD", "corners": [[20, 100], [200, 220]]})).unwrap();
    let v = h.run("place", json!({"symbol": "pinp", "device": x.hex(), "at": [75, 180]})).unwrap();
    assert_eq!(v["location"]["conflicts"][0]["proposed"], "+FIELD", "{v}");
    let head = h.head();
    h.run("conflicts", json!({"answer": "half"})).unwrap();
    assert_eq!(h.head(), head + 1);
    let d = h.doc();
    assert_eq!(loc(&d, x).as_deref(), Some("+CP1"), "the connector keeps its location");
    assert_eq!(locations::half_location(&d, x, ecad_electrical::symbols::Side::Plug).value.as_deref(), Some("+FIELD"));
    // JSON: the half answered in the place's own transaction.
    let v = h.run("place", json!({"symbol": "pins", "at": [320, 40]})).unwrap();
    let y: DeviceId = id(&v, "component");
    h.run("setlocation", json!({"component": y.hex(), "value": "+CP1"})).unwrap();
    h.run("place", json!({"symbol": "pinp", "device": y.hex(), "at": [75, 140], "location": "half"})).unwrap();
    let d = h.doc();
    assert_eq!(locations::half_location(&d, y, ecad_electrical::symbols::Side::Plug).value.as_deref(), Some("+FIELD"));
    assert_eq!(loc(&d, y).as_deref(), Some("+CP1"));
}

/// §5.3: the other place paths propose too: a level terminal (its strip), a PLC channel (its
/// card) and a connector placed in a box take the box's location.
#[test]
fn levels_channels_and_connectors_propose_too() {
    let mut h = H::new(SEED);
    h.run("locationbox", json!({"location": "+CP1", "corners": [[20, 20], [400, 280]]})).unwrap();
    let v = h.run("place", json!({"symbol": "termlnpe.1", "at": [60, 200]})).unwrap();
    assert_eq!(loc(&h.doc(), id(&v, "component")).as_deref(), Some("+CP1"), "{v}");
    let v = h.run("place", json!({"symbol": "di", "at": [150, 200]})).unwrap();
    assert_eq!(loc(&h.doc(), id(&v, "component")).as_deref(), Some("+CP1"), "{v}");
    let v = h.run("place", json!({"symbol": "pins", "at": [250, 200]})).unwrap();
    assert_eq!(loc(&h.doc(), id(&v, "component")).as_deref(), Some("+CP1"), "{v}");
}
