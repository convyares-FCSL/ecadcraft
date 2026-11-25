//! Make them one in a merge: a tag conflict answered `one` joins the source's device into the
//! target's with everything the device merge moves (its symbols, its connection points and the
//! wires on them), and the merge keeps what else the source drew, a location box included.

use ecad_model::{AidId, DeviceId, RouteId, Subject};
use serde_json::{Value, json};

use super::acceptance::{TempDir, raw, saved, verify};
use crate::common::{close_all, id};

fn tag_row(pv: &Value) -> Value {
    pv["conflicts"].as_array().and_then(|a| a.iter().find(|k| k["kind"] == json!("tag")).cloned()).unwrap_or_else(|| panic!("no tag row: {pv}"))
}

/// The branch draws a location box and places `-K77` with a wire on its connection point 13;
/// main places its own `-K77`. Merging with `one` joins the branch's device into main's: the
/// wire lands on main's device, the box is kept, and the project and its file check clean.
#[test]
fn make_them_one_with_a_location_box_and_a_wire_moves_everything_the_join_touches() {
    let d = TempDir::new("make-one-box");
    let (mut h, path) = saved(&d, 41);
    raw(&mut h, "branch", json!({"name": "S"})).unwrap();
    let b: AidId = id(&raw(&mut h, "locationbox", json!({"location": "+CAB9", "corners": [[880, -40], [1000, 60]]})).unwrap(), "aid");
    let x: DeviceId = id(&raw(&mut h, "place", json!({"symbol": "no", "at": [900, 0], "tag": "-K77"})).unwrap(), "component");
    let w: RouteId = id(&raw(&mut h, "wire", json!({"points": [[900, 10], [960, 10]]})).unwrap(), "route");
    assert!(h.doc().routes()[&w].ends[0].point().is_some(), "the wire lands on -K77:13");
    raw(&mut h, "switchbranch", json!({"branch": "main", "save": true})).unwrap();
    let y: DeviceId = id(&raw(&mut h, "place", json!({"symbol": "no", "at": [700, 0], "tag": "-K77"})).unwrap(), "component");
    let pv = raw(&mut h, "merge", json!({"from": "S", "preview": true})).unwrap();
    let k = tag_row(&pv);
    assert!(k["choices"].as_array().unwrap().contains(&json!("one")), "{k}");
    let cid = k["id"].as_str().unwrap().to_string();
    let r = raw(&mut h, "merge", json!({"from": "S", "choices": {cid: "one"}})).unwrap();
    assert_eq!(r["merged"], json!(true), "{r}");
    let doc = h.doc();
    assert!(!doc.devices().contains_key(&x), "the branch's -K77 joined main's");
    assert_eq!(doc.representations_of(Subject::Device(y)).count(), 2, "one device with both symbols");
    let end = doc.routes()[&w].ends[0].point().expect("the wire still lands on a connection point");
    assert_eq!(doc.connection_points()[&end].owner, y.into(), "the wire lands on main's -K77");
    assert!(doc.aids().contains_key(&b), "the location box is kept");
    h.check();
    close_all(&mut h.s);
    drop(h);
    verify(&path);
}

/// `-K76` and a wire on its connection point 13 are drawn before the fork; the branch renames it
/// `-K77` and main places its own `-K77`. Neither side changed the wire, so only the join moves
/// it: merging with `one` still lands it on main's device.
#[test]
fn make_them_one_moves_a_wire_drawn_before_the_fork() {
    let d = TempDir::new("make-one-old-wire");
    let (mut h, path) = saved(&d, 42);
    let x: DeviceId = id(&raw(&mut h, "place", json!({"symbol": "no", "at": [900, 0], "tag": "-K76"})).unwrap(), "component");
    let w: RouteId = id(&raw(&mut h, "wire", json!({"points": [[900, 10], [960, 10]]})).unwrap(), "route");
    assert!(h.doc().routes()[&w].ends[0].point().is_some(), "the wire lands on -K76:13");
    raw(&mut h, "branch", json!({"name": "S"})).unwrap();
    raw(&mut h, "setprop", json!({"component": x.hex(), "key": "tag", "value": "-K77"})).unwrap();
    raw(&mut h, "switchbranch", json!({"branch": "main", "save": true})).unwrap();
    let y: DeviceId = id(&raw(&mut h, "place", json!({"symbol": "no", "at": [700, 0], "tag": "-K77"})).unwrap(), "component");
    let pv = raw(&mut h, "merge", json!({"from": "S", "preview": true})).unwrap();
    let k = tag_row(&pv);
    assert!(k["choices"].as_array().unwrap().contains(&json!("one")), "{k}");
    let cid = k["id"].as_str().unwrap().to_string();
    let r = raw(&mut h, "merge", json!({"from": "S", "choices": {cid: "one"}})).unwrap();
    assert_eq!(r["merged"], json!(true), "{r}");
    let doc = h.doc();
    assert!(!doc.devices().contains_key(&x), "the branch's -K77 joined main's");
    let end = doc.routes()[&w].ends[0].point().expect("the wire still lands on a connection point");
    assert_eq!(doc.connection_points()[&end].owner, y.into(), "the wire lands on main's -K77");
    h.check();
    close_all(&mut h.s);
    drop(h);
    verify(&path);
}
