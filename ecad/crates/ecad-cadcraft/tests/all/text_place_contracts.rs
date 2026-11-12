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

/// AC 3 / T10: a tag (moved or automatic) stays upright when its symbol rotates 90°, and a moved
/// tag keeps its place relative to the symbol.
#[test]
fn a_tag_stays_upright_when_its_symbol_rotates() {
    let mut h = H::new(SEED);
    let page = h.first_page();
    let (_, r) = place(&mut h, "no", [100.0, 150.0]);
    let (_, auto) = place(&mut h, "no", [160.0, 150.0]);
    h.run("movetext", json!({"representation": r.hex(), "kind": "tag", "to": [90, 160], "justify": "RT"})).unwrap();
    for rep in [r, auto] {
        let item = h.doc().representations[&rep].items[0];
        let hd = h.handle_of(page, item);
        let origin = match &h.doc().items[&item].kind {
            ecad_model::ItemKind::Symbol(s) => s.placement.origin,
            _ => unreachable!(),
        };
        h.run("rotate", json!({"handles": [hd.hex()], "base": [origin.x, origin.y], "angle": 90})).unwrap();
    }
    let tags: Vec<(String, f64)> = h
        .pv(|p, pg| pg.entities_of(p, page))
        .iter()
        .filter_map(|e| match &e.kind {
            cadcraft_engine::doc::EntityKind::Text(t) if e.common.layer == ecad_cadcraft::devices::TAG_LAYER && t.value.starts_with('-') => {
                Some((t.value.clone(), t.rotation))
            }
            _ => None,
        })
        .collect();
    assert_eq!(tags.len(), 2, "{tags:?}");
    assert!(tags.iter().all(|(_, rot)| *rot == 0.0), "upright: {tags:?}");
    // The moved tag turned with the symbol: (90, 160) is (-10, +10) from the origin (100, 150),
    // so after 90° it is at (90, 140).
    let row = tag_row(&mut h, page, r);
    assert_eq!(row["moved"], json!(true));
    let at = (row["at"][0].as_f64().unwrap(), row["at"][1].as_f64().unwrap());
    assert!((at.0 - 90.0).abs() < 1e-6 && (at.1 - 140.0).abs() < 1e-6, "{row}");
    h.check();
}

/// AC 3 / T10: a moved tag survives save and reopen.
#[test]
fn a_moved_tag_survives_save_and_reopen() {
    let dir = std::env::temp_dir().join(format!("ecad-031-moved-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = H::new(SEED);
    h.cfg(|p| p.persistence = ecad_cadcraft::Persistence::File);
    let page = h.first_page();
    let (_, r) = place(&mut h, "no", [100.0, 150.0]);
    h.run("movetext", json!({"representation": r.hex(), "kind": "tag", "to": [130, 170], "justify": "LT"})).unwrap();
    let before = tag_row(&mut h, page, r);
    let path = dir.join("moved").display().to_string();
    h.run("saveas", json!({"path": path})).unwrap();
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    let mut s = cadcraft_engine::Session::empty();
    let (project, _) = ecad_cadcraft::ProjectHandle::open_file(&mut s, &format!("{path}.ecad"), 0x31_06).unwrap();
    let mut h = H { s, project, trail: Vec::new() };
    let after = tag_row(&mut h, page, r);
    assert_eq!((after["moved"].clone(), after["at"].clone(), after["justify"].clone()), (json!(true), before["at"].clone(), json!("LT")));
    drop(h);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The entry point for other lanes: `text_style::apply` gives an entity its kind's height, style
/// name (with a kind's own font) and, for bold in the stroke font, the bold lineweight.
#[test]
fn apply_styles_an_entity_like_the_page_does() {
    use cadcraft_engine::doc::{Entity, EntityKind, HAlign, Handle, Lineweight, VAlign};
    use ecad_cadcraft::text_style::{self, TextKind};
    let mut doc = ecad_model::Document::default();
    let mut e: Entity = ecad_cadcraft::devices::text_entity_with("X", Pt::new(0.0, 0.0), 9.0, HAlign::Left, VAlign::Top, Handle(1), "0");
    text_style::apply(&doc, &mut e, TextKind::Tag);
    let EntityKind::Text(t) = &e.kind else { unreachable!() };
    assert_eq!((t.height, t.style.as_str()), (2.5, "ECAD-BOLD"));
    let stroke = text_style::Fonts::resolve(&doc).bold_lineweight;
    assert_eq!(e.common.lineweight, stroke.map_or(Lineweight::ByLayer, Lineweight::Mm100));
    doc.properties.insert("text.grid.font".into(), "Kind Font 031".into());
    text_style::apply(&doc, &mut e, TextKind::Grid);
    let EntityKind::Text(t) = &e.kind else { unreachable!() };
    assert_eq!((t.height, t.style.as_str()), (3.0, "ECAD-GRID"));
    // text_width measures one kind, the same as from a held Look.
    let look = text_style::Look::new(&doc);
    let w = text_style::width_in(look.fonts_for(TextKind::Grid), look.style(TextKind::Grid), "ABC");
    assert_eq!(text_style::text_width(&doc, TextKind::Grid, "ABC"), w);
}
