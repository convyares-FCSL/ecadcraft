//! Brief 031 Q2 (design 031 §6, §13 T9, T10, T15): tag blocks avoid wires and each other, a
//! moved tag keeps its place relative to its symbol, and hostile stored places draw the automatic
//! place.

use crate::common::{H, id};

use ecad_model::{ComponentId, PageId, Pt, RepresentationId, Subject};
use serde_json::{Value, json};

const SEED: u64 = 0x31_02;

fn place(h: &mut H, symbol: &str, at: [f64; 2]) -> (ComponentId, RepresentationId) {
    let v = h.run("place", json!({"symbol": symbol, "at": at})).unwrap();
    let c: ComponentId = id(&v, "component");
    let r = h.doc().representations_of(Subject::Component(c)).map(|(r, _)| r).next().unwrap();
    (c, r)
}

/// The `pagetexts` row of `rep`'s tag.
fn tag_row(h: &mut H, page: PageId, rep: RepresentationId) -> Value {
    let v = h.run("pagetexts", json!({"page": page.hex()})).unwrap();
    v["texts"].as_array().unwrap().iter().find(|t| t["kind"] == "tag" && t["representation"] == rep.hex().as_str()).unwrap().clone()
}

fn bx(row: &Value) -> (Pt, Pt) {
    let b = &row["box"];
    (Pt::new(b[0][0].as_f64().unwrap(), b[0][1].as_f64().unwrap()), Pt::new(b[1][0].as_f64().unwrap(), b[1][1].as_f64().unwrap()))
}

fn intersects(a: (Pt, Pt), b: (Pt, Pt)) -> bool {
    a.0.x < b.1.x && b.0.x < a.1.x && a.0.y < b.1.y && b.0.y < a.1.y
}

#[test]
fn a_tag_on_a_wire_moves_to_the_next_free_candidate() {
    let mut h = H::new(SEED);
    let page = h.first_page();
    let (_, r) = place(&mut h, "no", [100.0, 150.0]);
    let first = bx(&tag_row(&mut h, page, r));
    // A wire straight through the first place.
    let y = (first.0.y + first.1.y) / 2.0;
    h.run("wire", json!({"points": [[first.0.x - 30.0, y], [first.1.x + 0.5, y]]})).unwrap();
    let row = tag_row(&mut h, page, r);
    let now = bx(&row);
    assert_ne!(now, first, "moved off the wire");
    let wire = (Pt::new(first.0.x - 30.3, y - 0.3), Pt::new(first.1.x + 0.8, y + 0.3));
    assert!(!intersects(now, wire), "{now:?} still on the wire");
    assert_eq!(row["overlaps"], json!(false));
    h.check();
}

#[test]
fn neighbouring_tags_never_overlap_when_a_free_place_exists() {
    let mut h = H::new(SEED);
    let page = h.first_page();
    let reps: Vec<RepresentationId> = (0..4).map(|i| place(&mut h, "no", [100.0 + 10.0 * f64::from(i), 150.0]).1).collect();
    let boxes: Vec<(Pt, Pt)> = reps.iter().map(|r| bx(&tag_row(&mut h, page, *r))).collect();
    for (i, a) in boxes.iter().enumerate() {
        for b in boxes.iter().skip(i + 1) {
            assert!(!intersects(*a, *b), "{a:?} overlaps {b:?}");
        }
    }
    for r in &reps {
        assert_eq!(tag_row(&mut h, page, *r)["overlaps"], json!(false));
    }
}

#[test]
fn a_moved_tag_follows_its_symbol_and_reset_puts_it_back() {
    let mut h = H::new(SEED);
    let page = h.first_page();
    let (c, r) = place(&mut h, "no", [100.0, 150.0]);
    let auto = tag_row(&mut h, page, r);
    let head = h.head();
    let v = h.run("movetext", json!({"representation": r.hex(), "kind": "tag", "to": [130, 170], "justify": "LT"})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    assert_eq!(v["changed"], json!(true));
    let row = tag_row(&mut h, page, r);
    assert_eq!((row["moved"].as_bool(), row["justify"].as_str()), (Some(true), Some("LT")));
    assert_eq!((row["at"][0].as_f64(), row["at"][1].as_f64()), (Some(130.0), Some(170.0)));
    // Move the symbol 20 mm: the tag follows.
    let item = h.doc().representations[&r].items[0];
    let hd = h.handle_of(page, item);
    h.run("move", json!({"handles": [hd.hex()], "delta": [20, 0]})).unwrap();
    let row = tag_row(&mut h, page, r);
    assert!((row["at"][0].as_f64().unwrap() - 150.0).abs() < 1e-6, "{row}");
    // Renumbering keeps it (the key names the representation, not the tag).
    h.run("setprop", json!({"component": c.hex(), "key": "tag", "value": "-K77"})).unwrap();
    assert_eq!(tag_row(&mut h, page, r)["moved"], json!(true));
    // Reset: placed automatically again.
    h.run("movetext", json!({"representation": r.hex(), "kind": "tag", "reset": true})).unwrap();
    let row = tag_row(&mut h, page, r);
    assert_eq!(row["moved"], json!(false));
    assert_ne!(row["at"], auto["at"], "the symbol moved, so the automatic place moved with it");
    h.check();
}

#[test]
fn movetext_refusals() {
    let mut h = H::new(SEED);
    let (_, r) = place(&mut h, "no", [100.0, 150.0]);
    for (p, words) in [
        (json!({"representation": r.hex(), "kind": "tag", "to": [1]}), "A text position must be a finite point."),
        (json!({"representation": r.hex(), "kind": "tag", "to": [1, 2], "justify": "XX"}), "Justify is two letters"),
        (json!({"representation": r.hex(), "kind": "point_label", "to": [1, 2]}), "point_label needs the connection point's key."),
        (json!({"representation": "00000000000000000000000000000001", "kind": "tag", "to": [1, 2]}), "No representation"),
    ] {
        let e = h.run("movetext", p.clone()).unwrap_err();
        assert!(e.contains(words), "{p}: {e}");
    }
}

#[test]
fn hostile_stored_places_draw_the_automatic_place() {
    let mut h = H::new(SEED);
    let page = h.first_page();
    let (c, r) = place(&mut h, "no", [100.0, 150.0]);
    let auto = tag_row(&mut h, page, r);
    let key = format!("text.{}.tag.at", r.hex());
    for bad in ["a,b,ZZ", "1e999,0,LT", "1,2", "1,2,LT,9", &"9".repeat(10_000)] {
        let op = ecad_model::Op::SetProperty {
            object: ecad_model::ObjectRef::Component(c),
            key: key.clone(),
            before: h.doc().components[&c].properties.get(&key).cloned(),
            after: Some(bad.to_string()),
        };
        h.project.apply_ops(&mut h.s, "hostile", vec![op]).unwrap();
        let row = tag_row(&mut h, page, r);
        assert_eq!((row["moved"].as_bool(), &row["at"]), (Some(false), &auto["at"]), "{bad:.20}");
    }
}
