//! Lane 072 D, step F2c (design 072 §4.3, owner ruling 2026-11-18): the owner's loose-line
//! terminal workflow. A line drawn from a device's point into free space carries no wire; Place
//! Terminals across it lands a terminal on the line, splitting it: the piece from the device to the
//! terminal lands at both ends and becomes a wire, the piece beyond stays loose (no wire, a
//! `no-wire-yet` row). Drawing on from the loose end to a second device's point forms the second
//! wire. Driven through the commands' JSON forms.

use std::collections::BTreeSet;

use ecad_model::{ConnectionPointId, DeviceId, RouteEnd, RouteId};
use serde_json::{Value, json};

use crate::common::{H, id};

const SEED: u64 = 0x72_d7;

/// The `no-wire-yet` rows of the checks.
fn no_wire_yet(h: &mut H) -> Vec<Value> {
    let v = h.run("checks", json!({"kinds": ["no-wire-yet"]})).unwrap();
    v["checks"].as_array().cloned().unwrap_or_default()
}

/// Each wire's two ends, as unordered pairs.
fn wire_ends(h: &mut H) -> BTreeSet<BTreeSet<ConnectionPointId>> {
    h.doc().wires().values().map(|w| w.ends.iter().copied().collect()).collect()
}

#[test]
fn place_terminal_blocks_on_a_loose_line_then_draw_on_to_a_second_device() {
    let mut h = H::new(SEED);
    // Two devices, each with one connection point (separate devices: one `points_at` each).
    let a = h.points_at(&[(0.0, 0.0)])[0];
    let b = h.points_at(&[(120.0, 0.0)])[0];

    // 1. A line from -A's point into free space: a route, no wire, one no-wire-yet row.
    let v = h.run("wire", json!({"points": [[0, 0], [60, 0]]})).unwrap();
    assert!(v["conductor"].is_null(), "a line with a free end carries no wire: {v}");
    let line: RouteId = id(&v, "route");
    assert!(h.doc().wires().is_empty());
    assert_eq!(no_wire_yet(&mut h).len(), 1);
    h.check();

    // 2. Place Terminals across the loose line: one transaction, one terminal on the route.
    let head = h.head();
    let v = h.run("terminalblocks", json!({"from": [30, 10], "to": [30, -10]})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction: {v}");
    let strip: DeviceId = id(&v, "strip");
    let terms = v["connection_points"].as_array().cloned().unwrap_or_default();
    assert_eq!(terms.len(), 1, "one connection_point: {v}");
    let t: ConnectionPointId = id(&terms[0], "connection_point");
    let doc = h.doc();
    assert_eq!(doc.connection_points()[&t].device().unwrap(), strip);
    // The route is split at the terminal: one piece from -A's point to the terminal, one from the
    // terminal to the free end.
    let pieces: Vec<(RouteId, [RouteEnd; 2])> = doc.routes().iter().map(|(r, x)| (*r, x.ends)).collect();
    assert_eq!(pieces.len(), 2, "split in two: {pieces:?}");
    let landed = pieces.iter().find(|(_, e)| e.contains(&RouteEnd::Point(a)) && e.contains(&RouteEnd::Point(t)));
    assert!(landed.is_some(), "a piece from -A to the connection_point: {pieces:?}");
    let (beyond, _) =
        *pieces.iter().find(|(_, e)| e.contains(&RouteEnd::Point(t)) && e.contains(&RouteEnd::Free)).unwrap_or_else(|| panic!("{pieces:?}"));
    assert!(pieces.iter().any(|(r, _)| *r == line), "the line keeps its id on one piece: {pieces:?}");
    // The landed piece is a wire (both its ends land); the piece beyond is not.
    assert_eq!(wire_ends(&mut h), BTreeSet::from([BTreeSet::from([a, t])]), "one wire, -A to the terminal");
    let rows = no_wire_yet(&mut h);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["object"], json!({"route": beyond.hex()}), "the piece beyond: {rows:?}");
    assert_eq!(rows[0]["at"], json!([60.0, 0.0]), "at its free end: {rows:?}");
    h.check();

    // 3. Draw on from the loose end to -B's point: the second wire forms, terminal to -B, and no
    // line is left without a wire.
    let head = h.head();
    let v = h.run("wire", json!({"points": [[60, 0], [120, 0]]})).unwrap();
    assert_eq!(h.head(), head + 1, "one commit: {v}");
    assert_eq!(wire_ends(&mut h), BTreeSet::from([BTreeSet::from([a, t]), BTreeSet::from([t, b])]), "-A to the terminal, the terminal to -B");
    assert!(no_wire_yet(&mut h).is_empty(), "every line lands at both ends");
    let doc = h.doc();
    let nets: BTreeSet<_> = doc.wires().values().map(|w| w.net).collect();
    assert_eq!(nets.len(), 1, "one net through the terminal");
    h.check();
}
