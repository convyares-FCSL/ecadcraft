//! Lane 072 D, step c (owner rule, 2026-11-18): a core whose line has one end landed stays on the
//! line and the checks say so; reading never works a core's place out, so a stored place the line
//! no longer gives (a file made elsewhere) is reported, not changed.

use ecad_model::{Author, CableId, CableMember, CorePosition, Op, RouteId, TerminalId};
use serde_json::{Value, json};

use crate::common::{H, id};

fn rows(h: &mut H, kind: &str) -> Vec<Value> {
    let v = h.run("checks", json!({"kinds": [kind]})).unwrap();
    v["checks"].as_array().cloned().unwrap_or_default()
}

/// A device point at (0, 0), a line from it to (60, 0) and a slash across it: the cable and line.
fn slashed(h: &mut H) -> (TerminalId, RouteId, CableId) {
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
    assert_eq!(h.doc().cables[&k].members[0].position, CorePosition::Route { route, landed: Some(a) });
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
    assert!(h.doc().cables[&k].members[0].wire().is_some());
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
        let old = p.ledger.document().cables[&k].members[0].clone();
        let new = CableMember { position: stale, ..old.clone() };
        let ops = vec![Op::RemoveCableMember { cable: k, index: 0, member: old }, Op::AddCableMember { cable: k, index: 0, member: new }];
        p.ledger.commit(Author::User, "a file made elsewhere", ops).unwrap();
    });
    assert_eq!(h.doc().cables[&k].members[0].position, stale, "nothing moved it back");
    let found = rows(&mut h, "core-place-stale");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["severity"], json!("error"));
    assert!(found[0]["which"].as_str().unwrap_or_default().contains("its line is landed at"), "{}", found[0]);
    assert!(rows(&mut h, "core-one-end").is_empty(), "a stale core is reported once");
    h.check();
}
