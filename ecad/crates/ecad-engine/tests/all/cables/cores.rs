//! Lane 072 D, step c (owner rule, 2026-11-18): a core whose line has one end landed stays on the
//! line and the checks say so; reading never works a core's place out, so a stored place the line
//! no longer gives (a file made elsewhere) is reported, not changed.

use ecad_model::{Author, CableId, ConnectionPointId, Core, CorePosition, Op, Purpose, RouteId};
use serde_json::{Value, json};

use crate::common::{H, id};

fn rows(h: &mut H, kind: &str) -> Vec<Value> {
    let v = h.run("checks", json!({"kinds": [kind]})).unwrap();
    v["checks"].as_array().cloned().unwrap_or_default()
}

/// A device point at (0, 0), a line from it to (60, 0) and a slash across it: the cable and line.
fn slashed(h: &mut H) -> (ConnectionPointId, RouteId, CableId) {
    let a = h.points_at(&[(0.0, 0.0)])[0];
    let v = h.run("wire", json!({"points": [[0, 0], [60, 0]]})).unwrap();
    let route = id(&v, "route");
    let r = h.run("cableslash", json!({"from": [30, -3], "to": [30, 3]})).unwrap();
    (a, route, id(&r, "cable"))
}

#[test]
fn a_core_landed_at_one_end_is_reported_until_the_other_end_lands() {
    let mut h = H::new(0x72_dc);
    let (a, route, k) = slashed(&mut h);
    assert_eq!(h.doc().cables()[&k].cores[0].position, CorePosition::Route { route, landed: Some(a) });
    let found = rows(&mut h, "core-one-end");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["text"], json!("Core landed at one end only"));
    assert_eq!(found[0]["object"], json!({"cable": k.hex()}));
    assert!(found[0]["which"].as_str().unwrap_or_default().contains("only"), "{}", found[0]);
    assert!(rows(&mut h, "core-place-stale").is_empty());
    h.check();

    // The other end lands: the command moves the core onto the wire and the row goes.
    h.points_at(&[(120.0, 0.0)]);
    h.run("wire", json!({"points": [[60, 0], [120, 0]]})).unwrap();
    assert!(h.doc().cables()[&k].cores[0].wire().is_some());
    assert!(rows(&mut h, "core-one-end").is_empty());
    h.check();
}

#[test]
fn a_stored_place_the_line_no_longer_gives_is_reported_not_changed() {
    let mut h = H::new(0x72_dd);
    let (_, route, k) = slashed(&mut h);
    // A file made elsewhere: the core says no end has landed, though one has.
    let stale = CorePosition::Route { route, landed: None };
    h.cfg(|p| {
        let old = p.ledger.document().cables()[&k].clone();
        let mut new = old.clone();
        new.cores[0] = Core { position: stale, ..old.cores[0].clone() };
        let ops = vec![Op::ModifyCable { id: k, before: old, after: new }];
        p.ledger.commit(Author::User, "a file made elsewhere", ops).unwrap();
    });
    assert_eq!(h.doc().cables()[&k].cores[0].position, stale, "nothing moved it back");
    let found = rows(&mut h, "core-place-stale");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["severity"], json!("error"));
    assert!(found[0]["which"].as_str().unwrap_or_default().contains("its line is landed at"), "{}", found[0]);
    assert!(rows(&mut h, "core-one-end").is_empty(), "a stale core is reported once");
    h.check();
}

#[test]
fn ticking_not_used_stores_the_cores_purpose_not_a_property() {
    // Design 072 §4.5 (c1): Not used is the stored core's purpose, apart from where it is.
    let mut h = H::new(0x72_de);
    let (_, _, k) = slashed(&mut h);
    h.points_at(&[(120.0, 0.0)]);
    h.run("wire", json!({"points": [[60, 0], [120, 0]]})).unwrap();
    h.run("setprop", json!({"cable": k.hex(), "key": "cores", "value": "2"})).unwrap();
    let open = rows(&mut h, "core-open");
    assert_eq!(open.len(), 1, "core 2 is offered and on no line: {open:?}");

    h.run("setprop", json!({"cable": k.hex(), "key": "unused.2", "value": "yes"})).unwrap();
    let doc = h.doc();
    let cab = &doc.cables()[&k];
    assert!(!cab.properties.keys().any(|key| key.starts_with("unused.")), "nothing stored under the key word");
    let two = cab.cores.iter().find(|m| m.label_str() == Some("2")).expect("core 2 stored");
    assert_eq!((two.purpose, two.position), (Purpose::Spare, CorePosition::Unplaced));
    assert!(cab.cores[0].wire().is_some() && cab.cores[0].purpose == Purpose::InUse, "the placed core is untouched");
    assert!(rows(&mut h, "core-open").is_empty());
    h.check();

    // Ticking it again changes nothing; unticking keeps the offered core, in use and unplaced.
    let again = h.run("setprop", json!({"cable": k.hex(), "key": "unused.2", "value": "yes"})).unwrap();
    assert_eq!(again["changed"], json!(false));
    h.run("setprop", json!({"cable": k.hex(), "key": "unused.2", "value": null})).unwrap();
    let two = h.doc().cables()[&k].cores.iter().find(|m| m.label_str() == Some("2")).cloned().expect("offered core 2 stays");
    assert_eq!((two.purpose, two.position), (Purpose::InUse, CorePosition::Unplaced));
    assert_eq!(rows(&mut h, "core-open").len(), 1);
    // A core the cable does not have is refused.
    assert!(h.run("setprop", json!({"cable": k.hex(), "key": "unused.9", "value": "yes"})).is_err());
    h.check();
}
