//! Lane 072 D, step F2c (design 072 §4.3, owner ruling 2026-11-18): a line with a free end carries
//! no wire. It can still be slashed into a cable and continued; the cable's core then sits on the
//! line (`CorePosition::Route`), and the wire forms, with the core moving onto it, when the line's
//! other end lands on a connection point. Driven through the commands' JSON forms.

use ecad_model::{CableId, ConnectionPointId, CorePosition, PageId, RouteId, WireId};
use serde_json::{Value, json};

use crate::common::{H, id};

const SEED: u64 = 0x72_d0;

/// The `no-wire-yet` rows of the checks.
fn no_wire_yet(h: &mut H) -> Vec<Value> {
    let v = h.run("checks", json!({"kinds": ["no-wire-yet"]})).unwrap();
    v["checks"].as_array().cloned().unwrap_or_default()
}

/// One device's connection point at `at` on the active page.
fn point(h: &mut H, at: (f64, f64)) -> ConnectionPointId {
    h.points_at(&[at])[0]
}

/// The only member's position of cable `k`.
fn core_position(h: &mut H, k: CableId) -> CorePosition {
    let doc = h.doc();
    let members = &doc.cables[&k].cores;
    assert_eq!(members.len(), 1, "one core: {members:?}");
    members[0].position
}

/// A device point at (0, 0) and a line from it into free space to (60, 0): no wire.
fn loose_line(h: &mut H) -> (ConnectionPointId, RouteId) {
    let a = point(h, (0.0, 0.0));
    let v = h.run("wire", json!({"points": [[0, 0], [60, 0]]})).unwrap();
    assert!(v["conductor"].is_null(), "a line with a free end carries no wire: {v}");
    (a, id(&v, "route"))
}

/// Workflow 1: A line from a connection point into free space: no wire, and the checks say why, naming
/// the page and the free end.
#[test]
fn a_line_into_free_space_has_no_wire_and_a_no_wire_yet_row() {
    let mut h = H::new(SEED);
    let page = h.first_page();
    let (_, route) = loose_line(&mut h);
    let doc = h.doc();
    assert!(doc.wires.is_empty(), "no wire");
    assert!(doc.routes.contains_key(&route), "the line is a route");
    let rows = no_wire_yet(&mut h);
    assert_eq!(rows.len(), 1, "{rows:?}");
    let row = &rows[0];
    assert_eq!(row["object"], json!({"route": route.hex()}));
    assert_eq!(row["text"], json!("Line not connected at both ends: no wire yet"));
    assert_eq!(row["page"], json!(ecad_model::document::book::page_number(&doc, page)), "names the page");
    assert_eq!(row["at"], json!([60.0, 0.0]), "at the free end");
    h.check();
}

/// Workflow 2: A slash across a loose line makes a cable whose core sits on the line, landed at the
/// device's point; extending the line onto a second point forms the wire and moves the core onto
/// it in the same commit; one undo puts both back.
#[test]
fn a_slashed_loose_line_gets_its_wire_when_extended_to_a_point() {
    let mut h = H::new(SEED + 1);
    let (a, route) = loose_line(&mut h);
    let r = h.run("cableslash", json!({"from": [30, -3], "to": [30, 3]})).unwrap();
    let k: CableId = id(&r, "cable");
    assert_eq!(r["conductors"], json!([]), "no wires: {r}");
    assert_eq!(r["lines"], json!([route.hex()]), "the loose line: {r}");
    assert_eq!(core_position(&mut h, k), CorePosition::Route { route, landed: Some(a) });
    h.check();

    let b = point(&mut h, (120.0, 0.0));
    let before = h.doc();
    let head = h.head();
    let v = h.run("wire", json!({"points": [[60, 0], [120, 0]]})).unwrap();
    assert_eq!(h.head(), head + 1, "one commit");
    let doc = h.doc();
    assert_eq!(doc.wires.len(), 1, "the wire forms: {v}");
    let (&w, wire): (&WireId, _) = doc.wires.iter().next().unwrap();
    let mut ends = wire.ends;
    ends.sort();
    let mut want = [a, b];
    want.sort();
    assert_eq!(ends, want, "between the two points");
    assert_eq!(core_position(&mut h, k), CorePosition::Wire(w), "the core moves onto the wire");
    assert!(no_wire_yet(&mut h).is_empty(), "the no-wire-yet row is gone");
    h.check();

    h.cmdline("u");
    assert!(h.doc() == before, "one undo puts the line and the core back");
    assert!(h.doc().wires.is_empty());
    assert_eq!(core_position(&mut h, k), CorePosition::Route { route, landed: Some(a) });
    assert_eq!(no_wire_yet(&mut h).len(), 1);
    h.check();
}

/// The continued fixture: on the first page a device point and a loose line from it slashed into
/// cable `-C1` with a reference; a second page with a device point. Returns the pages, the
/// cable, its line, the points and `cablecontinue`'s result on the second page.
struct Continued {
    h: H,
    pages: [PageId; 2],
    cable: CableId,
    line: RouteId,
    a: ConnectionPointId,
    v: Value,
}

fn continued(seed: u64) -> Continued {
    let mut h = H::new(seed);
    let first = h.first_page();
    let (a, line) = loose_line(&mut h);
    let r = h.run("cableslash", json!({"from": [30, -3], "to": [30, 3], "tag": "C1"})).unwrap();
    let cable: CableId = id(&r, "cable");
    h.run("cablereference", json!({"cable": cable.hex(), "at": [65, 0]})).unwrap();
    let second: PageId = id(&h.run("newpage", json!({"title": "Field"})).unwrap(), "page");
    h.activate(second);
    let v = h.run("cablecontinue", json!({"cable": cable.hex(), "at": [20, 0], "toward": [40, 0]})).unwrap();
    Continued { h, pages: [first, second], cable, line, a, v }
}

/// Workflow 3: `cablecontinue` of a cable whose core sits on a loose line works as the slash does: the
/// core stays on the line (no wire yet); connecting the line on its own page forms the wire and
/// moves the core onto it.
#[test]
fn cablecontinue_of_a_loose_line_keeps_the_core_on_the_line_until_it_connects() {
    let mut c = continued(SEED + 2);
    assert!(c.h.doc().wires.is_empty(), "no wire yet: {}", c.v);
    assert_eq!(c.v["routes"].as_array().map(Vec::len), Some(1), "one stub: {}", c.v);
    assert_eq!(core_position(&mut c.h, c.cable), CorePosition::Route { route: c.line, landed: Some(c.a) });
    c.h.check();
    c.h.activate(c.pages[0]);
    let b = point(&mut c.h, (120.0, 0.0));
    c.h.run("wire", json!({"points": [[60, 0], [120, 0]]})).unwrap();
    let doc = c.h.doc();
    assert_eq!(doc.wires.len(), 1, "the wire forms");
    let (&w, wire) = doc.wires.iter().next().unwrap();
    assert!(wire.ends.contains(&c.a) && wire.ends.contains(&b));
    assert_eq!(core_position(&mut c.h, c.cable), CorePosition::Wire(w));
    c.h.check();
}

/// Workflow 4: Continuing a cable across two pages through its cable reference and connecting the stub to
/// a device point on the second page gives one wire across the pages, and one net.
#[test]
fn cablecontinue_across_two_pages_gives_one_wire_and_one_net() {
    let mut c = continued(SEED + 3);
    let b = point(&mut c.h, (100.0, 0.0));
    let stub = c.v["routes"][0].clone();
    c.h.run("connect", json!({"route": stub["route"], "end": stub["free_end"], "connection_point": b.hex()})).unwrap();
    let doc = c.h.doc();
    assert_eq!(doc.wires.len(), 1, "one wire across the pages");
    let (&w, wire) = doc.wires.iter().next().unwrap();
    assert!(wire.ends.contains(&c.a) && wire.ends.contains(&b), "from the first page's point to the second's");
    let pages = ecad_electrical::wires::routes::wire_pages(&doc, w);
    assert!(pages.contains(&c.pages[0]) && pages.contains(&c.pages[1]), "drawn on both pages: {pages:?}");
    assert_eq!(core_position(&mut c.h, c.cable), CorePosition::Wire(w));
    let nets = c.h.run("nets", json!({})).unwrap();
    let with_both: Vec<&Value> = nets["nets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|n| {
            let s = n.to_string();
            s.contains(&c.a.hex()) || s.contains(&b.hex()) || s.contains(&w.hex())
        })
        .collect();
    assert_eq!(with_both.len(), 1, "one net: {nets}");
    assert!(no_wire_yet(&mut c.h).is_empty());
    c.h.check();
}

/// Design 072 §4.4 (F38, A17): a connection point with more wires than it accepts is an error row
/// naming the point and both counts; it goes when a wire is taken away.
#[test]
fn a_point_with_more_wires_than_it_accepts_is_an_error() {
    let mut h = H::new(SEED ^ 0x0c);
    let pts = h.points_at(&[(0.0, 0.0), (60.0, 0.0), (0.0, 60.0), (60.0, 60.0)]);
    let over = |h: &mut H| -> Vec<Value> {
        let v = h.run("checks", json!({"kinds": ["over-count"]})).unwrap();
        v["checks"].as_array().cloned().unwrap_or_default()
    };
    h.run("wire", json!({"points": [[0, 0], [60, 0]]})).unwrap();
    h.run("wire", json!({"points": [[0, 0], [0, 60]]})).unwrap();
    assert!(over(&mut h).is_empty(), "two wires at a point that takes two");
    h.run("wire", json!({"points": [[0, 0], [60, 60]]})).unwrap();
    assert_eq!(h.doc().ends_at(pts[0]).len(), 3);
    let rows = over(&mut h);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["object"], json!({"connection_point": pts[0].hex()}));
    assert_eq!(rows[0]["severity"], json!("error"), "{:?}", rows[0]);
    assert!(rows[0].to_string().contains("takes 2 conductors; 3 are connected"), "{:?}", rows[0]);
    h.cmdline("u");
    assert!(over(&mut h).is_empty(), "the row goes with the third wire");
    h.check();
}

// ---------------------------------------------------------------- symbol tied to multiple locations

/// Design 072 §6.3 (A13): a device whose location was typed (fixed) with a symbol inside a box of
/// another location gets "Symbol tied to multiple locations" on every placed symbol, one message;
/// an unfixed device, or a box of its own location, gets none. The kept Checks agree with a
/// rebuild after each step (the update map).
#[test]
fn a_fixed_device_in_a_box_of_another_location_is_tied_to_multiple_locations() {
    use ecad_electrical::checks::Kind;
    let mut h = H::new(SEED + 30);
    let p105 = h.first_page();
    let k = id::<ecad_model::DeviceId>(&h.run("place", json!({"symbol": "coil", "at": [75, 180], "tag": "-K01"})).unwrap(), "component");
    let p110: PageId = id(&h.run("newpage", json!({"title": "110"})).unwrap(), "page");
    h.run("place", json!({"symbol": "no", "at": [100, 150], "device": k.hex()})).unwrap();
    let tied = |h: &mut H| {
        let doc = h.doc();
        let fresh: Vec<ecad_electrical::checks::Check> =
            ecad_engine::checks::all(&doc, 1e-6).into_iter().filter(|r| r.kind == Kind::TiedLocations).collect();
        let kept: Vec<ecad_electrical::checks::Check> = h.cfg(|p| {
            let doc = p.ledger.document().clone();
            p.checks().sorted(&doc).into_iter().filter(|r| r.kind == Kind::TiedLocations).collect()
        });
        assert_eq!(kept, fresh, "the kept Checks equal a rebuild");
        fresh
    };
    // A box of +FIELD around the contact, the device's location not set by hand: no row.
    h.run("locationbox", json!({"location": "+FIELD", "page": p110.hex(), "corners": [[60, 110], [140, 190]]})).unwrap();
    assert!(tied(&mut h).is_empty());
    // Typed: +CP1, fixed. The contact is in +FIELD: one row per placed symbol, one message.
    h.run("setlocation", json!({"component": k.hex(), "aspect": "location", "value": "+CP1"})).unwrap();
    assert!(h.doc().devices[&k].location_fixed, "typing sets fixed");
    let rows = tied(&mut h);
    let pages: std::collections::BTreeSet<PageId> = rows.iter().filter_map(|r| r.page).collect();
    assert_eq!(pages, [p105, p110].into_iter().collect(), "{rows:?}");
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(rows.iter().all(|r| r.which == "-K01: Symbol tied to multiple locations" && r.at.is_some()), "{rows:?}");
    assert_eq!(rows[0].kind.severity(), ecad_electrical::checks::Severity::Error);
    // The device typed as +FIELD: the box agrees, no row.
    h.run("setlocation", json!({"component": k.hex(), "aspect": "location", "value": "+FIELD"})).unwrap();
    assert!(tied(&mut h).is_empty());
}

// ---------------------------------------------------------------- F5: continuations as named points

/// Continuation `name` at `at` on the active page: its device.
fn continuation(h: &mut H, at: (f64, f64), name: &str) -> ecad_model::AidId {
    id(&h.run("place", json!({"symbol": "continuation", "at": [at.0, at.1], "name": name})).unwrap(), "aid")
}

/// The wires' end pairs, each sorted.
fn wire_ends(h: &mut H) -> Vec<[ConnectionPointId; 2]> {
    let mut out: Vec<[ConnectionPointId; 2]> = h
        .doc()
        .wires
        .values()
        .map(|w| {
            let mut e = w.ends;
            e.sort();
            e
        })
        .collect();
    out.sort();
    out
}

/// Design 072 §4.4 (F5): a continuation is a named point. A line from a device point to a
/// continuation and a line from its partner (same name, another page) to a device point make one
/// wire between the two device points; neither line ends a wire at the continuation.
#[test]
fn a_wire_runs_through_a_continuation_pair_from_device_point_to_device_point() {
    let mut h = H::new(SEED + 0x50);
    let p1 = h.first_page();
    let a = point(&mut h, (0.0, 0.0));
    continuation(&mut h, (50.0, 0.0), "T1");
    h.run("wire", json!({"points": [[0, 0], [50, 0]]})).unwrap();
    assert!(h.doc().wires.is_empty(), "a lonely continuation carries no wire");
    let p2: PageId = id(&h.run("newpage", json!({"title": "Two"})).unwrap(), "page");
    h.activate(p2);
    let k2 = continuation(&mut h, (0.0, 0.0), "T1");
    let b = point(&mut h, (50.0, 0.0));
    let r = h.run("wire", json!({"points": [[0, 0], [50, 0]]})).unwrap();
    let mut want = [a, b];
    want.sort();
    assert_eq!(wire_ends(&mut h), [want], "one wire, page 1's point to page 2's: {r}");
    let doc = h.doc();
    let t2 = doc.connection_points_of(k2).map(|(t, _)| t).next().unwrap();
    assert_eq!(doc.named_points(t2).map(|g| g.len()), Some(2), "the pair's two points");
    assert!(doc.ends_at(t2).is_empty(), "no wire ends at a continuation");
    let w: WireId = *doc.wires.keys().next().unwrap();
    assert!(
        ecad_model::connectivity::derive::route_on_page(&doc, w, p1).is_some()
            && ecad_model::connectivity::derive::route_on_page(&doc, w, p2).is_some()
    );
    h.check();
}

/// F5: renaming one of the pair takes the wire away in the same transaction (both continuations
/// are lonely then); undo brings it back with the same ends; renaming back joins them again.
#[test]
fn renaming_a_continuation_re_forms_the_wires_through_it() {
    let mut h = H::new(SEED + 0x51);
    let a = point(&mut h, (0.0, 0.0));
    continuation(&mut h, (50.0, 0.0), "T1");
    h.run("wire", json!({"points": [[0, 0], [50, 0]]})).unwrap();
    let p2: PageId = id(&h.run("newpage", json!({"title": "Two"})).unwrap(), "page");
    h.activate(p2);
    let k2 = continuation(&mut h, (0.0, 0.0), "T1");
    let b = point(&mut h, (50.0, 0.0));
    h.run("wire", json!({"points": [[0, 0], [50, 0]]})).unwrap();
    let mut want = [a, b];
    want.sort();
    assert_eq!(wire_ends(&mut h), [want]);
    let head = h.head();
    h.run("setprop", json!({"aids": [k2.hex()], "key": "name", "value": "T2"})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    assert!(wire_ends(&mut h).is_empty(), "the pair is broken: no wire");
    let lonely = h.run("checks", json!({"kinds": ["lonely-continuation"]})).unwrap();
    assert_eq!(lonely["total"], json!(2), "{lonely}");
    h.cmdline("u");
    assert_eq!(wire_ends(&mut h), [want], "undo brings the wire back");
    h.cmdline("u");
    assert!(wire_ends(&mut h).is_empty(), "before page 2's line: no wire");
    h.cmdline("redo");
    assert_eq!(wire_ends(&mut h), [want], "redo draws page 2's line again");
    h.run("setprop", json!({"aids": [k2.hex()], "key": "name", "value": "T2"})).unwrap();
    assert!(wire_ends(&mut h).is_empty());
    h.run("setprop", json!({"aids": [k2.hex()], "key": "name", "value": "t1 "})).unwrap();
    assert_eq!(wire_ends(&mut h), [want], "names compare trimmed, upper case");
    h.check();
}
