//! Brief 004 (wires to cable, the slash gesture), written from the brief and design 004 (§4
//! crossing rules, §5 interaction, §6 JSON form, §7 drawing, §8 existing commands, §9 debugger).
//!
//! Fixture F (design §10): three parallel wires W0, W5, W10 (tagged by their y) from x 0 to 60,
//! and a plain line at y 15. The design's slash S runs from (28,-3) to (32,18).
#![cfg(not(target_arch = "wasm32"))]

use crate::common;

use cadcraft_engine::Input;
use cadcraft_engine::doc::EntityKind;
use cadcraft_engine::doc::color::Color as CColor;
use cadcraft_engine::ext::MarkShape;
use cadcraft_engine::geom::Vec2;
use common::{H, id};
use ecad_cadcraft::debug::{ObjectId, used_by};
use ecad_cadcraft::interact::{hover_lines, overlay_marks};
use ecad_cadcraft::slash::MAX_CABLE_MEMBERS;
use ecad_model::{
    Author, CableId, Color, ConductorId, Form, Item, ItemId, ItemKind, Op, PageId, Primitive, Pt, Representation, RepresentationId, Style, Subject,
    Transaction,
};
use serde_json::{Value, json};

const SEED: u64 = 40_04;

struct F {
    h: H,
    page: PageId,
    w0: ConductorId,
    w5: ConductorId,
    w10: ConductorId,
    plain: ItemId,
}

fn wire(h: &mut H, points: Value, tag: &str) -> ConductorId {
    let c: ConductorId = id(&h.run("wire", json!({ "points": points })).unwrap(), "conductor");
    h.run("setprop", json!({"conductor": c.hex(), "key": "tag", "value": tag})).unwrap();
    c
}

fn fixture() -> F {
    let mut h = H::new(SEED);
    h.s.viewport_px = (1280.0, 800.0);
    let w0 = wire(&mut h, json!([[0, 0], [60, 0]]), "W0");
    let w5 = wire(&mut h, json!([[0, 5], [60, 5]]), "W5");
    let w10 = wire(&mut h, json!([[0, 10], [60, 10]]), "W10");
    h.run("line", json!({"points": [[0, 15], [60, 15]]})).unwrap();
    let page = h.first_page();
    let d = h.doc();
    let plain = d.items.keys().copied().find(|i| d.representation_of_item(*i).is_none()).expect("the plain line");
    F { h, page, w0, w5, w10, plain }
}

fn slash(h: &mut H, from: (f64, f64), to: (f64, f64)) -> Result<Value, String> {
    h.run("cableslash", json!({"from": [from.0, from.1], "to": [to.0, to.1]}))
}

fn members(h: &mut H, k: CableId) -> Vec<ConductorId> {
    h.doc().cables[&k].members.iter().map(|m| m.conductor).collect()
}

fn history(h: &mut H) -> Vec<Transaction> {
    h.p(|p| p.ledger.log().iter().map(|t| Transaction { time_ms: None, ..t.clone() }).collect())
}

fn prompt(h: &H) -> String {
    h.s.current_prompt().map(|p| p.display()).unwrap_or_default()
}

fn click(h: &mut H, x: f64, y: f64) {
    let at = Vec2::new(x, y);
    h.s.cursor = at;
    let _ = h.s.input(Input::Point(at));
}

fn uid(h: &mut H, page: PageId) -> u64 {
    h.p(|p| p.pages[&page].uid).expect("page open")
}

/// AC1, AC4 (design check B): one transaction, ops in the §2 order, members in crossing order with
/// no core, the plain line ignored and reported, nothing else changed.
#[test]
fn a_slash_makes_one_cable_of_the_wires_it_crosses_in_one_transaction() {
    let mut f = fixture();
    let before = f.h.doc();
    let head = f.h.head();
    let r = slash(&mut f.h, (28.0, -3.0), (32.0, 18.0)).unwrap();
    assert_eq!(f.h.head(), head + 1, "one transaction");
    let tx = f.h.p(|p| p.ledger.log().last().cloned().unwrap());
    assert_eq!(tx.label, "Cable slash");
    let names: Vec<&str> = tx.ops.iter().map(Op::name).collect();
    assert_eq!(names, ["CreateCable", "AddCableMember", "AddCableMember", "AddCableMember", "AddItem", "LinkRepresentation"]);
    let k: CableId = id(&r, "cable");
    let d = f.h.doc();
    let cable = &d.cables[&k];
    assert_eq!(members(&mut f.h, k), vec![f.w0, f.w5, f.w10], "crossing order from the first point");
    assert!(cable.members.iter().all(|m| m.core.is_none()), "no core designations");
    // Brief 026: an automatic tag when none is given, and nothing else.
    assert!(cable.properties.keys().all(|k| k == "tag") && cable.properties["tag"].starts_with("-W"), "an automatic tag");
    assert_eq!(r["conductors"], json!([f.w0.hex(), f.w5.hex(), f.w10.hex()]));
    assert_eq!(r["ignored"], json!([f.plain.hex()]), "the plain line is ignored and reported");
    assert_eq!(r["touched"], json!([]));
    // Nothing that existed changed (DOC-04).
    assert_eq!(d.conductors, before.conductors, "conductor IDs and attachments unchanged");
    for (i, item) in &before.items {
        assert_eq!(d.items.get(i), Some(item), "item {i} unchanged");
    }
    for (rid, rep) in &before.representations {
        assert_eq!(d.representations.get(rid), Some(rep), "representation {rid} unchanged");
    }
    // The slash: a plain line in the wire style, the cable's drawn representation.
    let item: ItemId = id(&r, "item");
    let rid: RepresentationId = id(&r, "representation");
    let rep = &d.representations[&rid];
    assert_eq!(rep.subject, Subject::Cable(k));
    assert_eq!((rep.form, rep.role.as_str(), rep.page), (Form::Drawn, "slash", f.page));
    assert_eq!(rep.items, vec![item]);
    assert!(rep.connection_points.is_empty());
    assert_eq!(d.items[&item].kind, ItemKind::Primitive(Primitive::Line { a: Pt::new(28.0, -3.0), b: Pt::new(32.0, 18.0) }));
    let route = ecad_model::derive::route_on_page(&d, f.w0, f.page).unwrap().0;
    assert_eq!(d.items[&item].style, d.items[&route].style, "the same style as a new wire (O4)");
}

/// AC1 (design check C): the reversed slash gives the reversed order.
#[test]
fn the_order_follows_the_slash_direction() {
    let mut f = fixture();
    let r = slash(&mut f.h, (32.0, 18.0), (28.0, -3.0)).unwrap();
    let k: CableId = id(&r, "cable");
    assert_eq!(members(&mut f.h, k), vec![f.w10, f.w5, f.w0]);
}

/// `tag` names the cable, as `cableadd`'s does.
#[test]
fn a_tag_is_stored_as_the_cable_tag() {
    let mut f = fixture();
    let r = f.h.run("cableslash", json!({"from": [28, -3], "to": [32, 18], "tag": "-W1"})).unwrap();
    let k: CableId = id(&r, "cable");
    assert_eq!(f.h.doc().cables[&k].properties.get("tag").map(String::as_str), Some("-W01"));
}

/// AC7 (design check D): two clicks record exactly the JSON form's transaction; before the second
/// click the preview shows three green rings and the count.
#[test]
fn the_interactive_form_records_the_json_forms_transaction() {
    let mut json_form = fixture();
    slash(&mut json_form.h, (28.0, -3.0), (32.0, 18.0)).unwrap();
    let mut clicks = fixture();
    let h = &mut clicks.h;
    let head = h.head();
    h.s.start("cableslash").unwrap();
    assert_eq!(prompt(h), "Specify first point of cable slash:");
    click(h, 28.0, -3.0);
    assert_eq!(prompt(h), "Specify second point of cable slash or [Undo]:");
    let cursor = Vec2::new(32.0, 18.0);
    h.s.cursor = cursor;
    let preview = h.s.preview(cursor);
    let green_rings = preview.iter().filter(|e| matches!(e.kind, EntityKind::Circle(_)) && e.common.color == CColor::Index(3)).count();
    assert_eq!(green_rings, 9, "three bold rings (three circles each)");
    assert!(
        preview.iter().any(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == "-> 3 wires, 1 other object ignored")),
        "the count shows before the click"
    );
    assert!(preview.iter().any(|e| matches!(e.kind, EntityKind::Line(_))), "the rubber band");
    assert_eq!(h.head(), head, "the preview records nothing");
    click(h, 32.0, 18.0);
    assert!(h.s.running.is_none(), "the command ends after an accepted slash");
    assert_eq!(h.head(), head + 1);
    h.check();
    assert_eq!(history(h), history(&mut json_form.h), "identical transactions");
}

/// Design §5.1: Undo drops the first point, Enter at step 2 and Esc write nothing, the same point
/// twice is ignored.
#[test]
fn undo_enter_esc_and_a_repeated_point_write_nothing() {
    let mut f = fixture();
    let h = &mut f.h;
    let head = h.head();
    h.s.start("cableslash").unwrap();
    click(h, 28.0, -3.0);
    click(h, 28.0, -3.0);
    assert_eq!(prompt(h), "Specify second point of cable slash or [Undo]:", "the same point is ignored");
    h.s.cmdline("U").unwrap();
    assert_eq!(prompt(h), "Specify first point of cable slash:", "Undo drops the first point");
    click(h, 28.0, -3.0);
    h.s.input(Input::Enter).unwrap();
    assert!(h.s.running.is_none(), "Enter at step 2 ends the command");
    h.s.start("cableslash").unwrap();
    click(h, 28.0, -3.0);
    h.s.cancel();
    assert!(h.s.running.is_none());
    assert_eq!(h.head(), head);
    assert!(h.doc().cables.is_empty());
}

/// AC2 (design check E): `u` removes cable and slash in one step; `redo` brings both back with
/// the same IDs.
#[test]
fn undo_and_redo_take_the_cable_and_its_slash_together() {
    let mut f = fixture();
    let before = f.h.doc();
    let r = slash(&mut f.h, (28.0, -3.0), (32.0, 18.0)).unwrap();
    let after = f.h.doc();
    f.h.cmdline("u");
    assert!(f.h.doc() == before, "undo removes cable, members, slash item and representation");
    f.h.cmdline("redo");
    let again = f.h.doc();
    assert!(again == after, "redo restores them");
    let k: CableId = id(&r, "cable");
    let item: ItemId = id(&r, "item");
    assert!(again.cables.contains_key(&k) && again.items.contains_key(&item), "with the same IDs");
}

/// AC3 (design check F): save, close and reopen give the same cable, members, order and slash;
/// replay equals the live state.
#[test]
fn a_slash_survives_save_and_reopen() {
    let dir = std::env::temp_dir().join(format!("ecad-004-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("slash.ecad").display().to_string();
    let mut f = fixture();
    f.h.cfg(|p| p.persistence = ecad_cadcraft::Persistence::File);
    let r = slash(&mut f.h, (28.0, -3.0), (32.0, 18.0)).unwrap();
    f.h.run("saveas", json!({ "path": path })).unwrap();
    let live = f.h.doc();
    let log = f.h.p(|p| p.ledger.full_log().unwrap());
    while !f.h.s.docs.is_empty() {
        f.h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    let mut s = cadcraft_engine::Session::empty();
    let (project, _) = ecad_cadcraft::ProjectHandle::open_file(&mut s, &path, 99).unwrap();
    let mut h2 = H { s, project, trail: Vec::new() };
    assert!(h2.doc() == live, "same document after reopening");
    assert_eq!(h2.p(|p| p.ledger.full_log().unwrap()), log, "same history");
    assert!(h2.p(|p| p.ledger.replay_matches().unwrap()), "replay gives the same state");
    let k: CableId = id(&r, "cable");
    assert_eq!(members(&mut h2, k), vec![f.w0, f.w5, f.w10]);
    h2.check();
    while !h2.s.docs.is_empty() {
        h2.s.execute("close", &json!({"index": 0})).unwrap();
    }
    drop(h2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Design §4, rules C1 to C6, one fixture each (owner answer O1). `Ok` lists the members (by tag)
/// and the touched wires; `Err` a part of the refusal.
#[test]
fn the_crossing_rules() {
    type Setup = fn(&mut H);
    let none: Setup = |_| {};
    let bend: Setup = |h| {
        wire(h, json!([[0, 20], [10, 20], [10, 30]]), "B");
    };
    let u_shape: Setup = |h| {
        wire(h, json!([[0, 40], [0, 50], [20, 50], [20, 40]]), "U");
    };
    #[allow(clippy::type_complexity)]
    let cases: Vec<(&str, Setup, (f64, f64), (f64, f64), Result<(Vec<&str>, Vec<&str>), &str>)> = vec![
        ("C1 crossing the inside", none, (28.0, -3.0), (32.0, 18.0), Ok((vec!["W0", "W5", "W10"], vec![]))),
        ("C2 through a bend", bend, (7.0, 23.0), (13.0, 17.0), Ok((vec!["B"], vec![]))),
        ("C3 a slash end on the inside", none, (20.0, 0.0), (20.0, -10.0), Ok((vec!["W0"], vec![]))),
        ("C4 only a wire end", none, (60.0, -2.0), (60.0, 2.0), Err("The slash crosses no wires; nothing was created.")),
        ("C4 an end among crossings", none, (28.0, -3.0), (60.0, 10.0), Ok((vec!["W0", "W5"], vec!["W10"]))),
        ("C5 along a wire", none, (10.0, 0.0), (20.0, 0.0), Err("The slash crosses no wires; nothing was created.")),
        ("C6 one wire crossed twice", u_shape, (-5.0, 45.0), (25.0, 45.0), Ok((vec!["U"], vec![]))),
    ];
    for (name, setup, from, to, want) in cases {
        let mut f = fixture();
        setup(&mut f.h);
        let head = f.h.head();
        let got = slash(&mut f.h, from, to);
        let d = f.h.doc();
        let tag = |c: &str| {
            let c = ConductorId::from(u128::from_str_radix(c, 16).unwrap());
            d.conductors[&c].properties["tag"].clone()
        };
        let tags = |v: &Value| v.as_array().unwrap().iter().map(|c| tag(c.as_str().unwrap())).collect::<Vec<_>>();
        match want {
            Ok((members, touched)) => {
                let r = got.unwrap_or_else(|e| panic!("{name}: refused: {e}"));
                assert_eq!(tags(&r["conductors"]), members, "{name}: members");
                assert_eq!(tags(&r["touched"]), touched, "{name}: touched");
            }
            Err(message) => {
                let e = got.expect_err(name);
                assert!(e.contains(message), "{name}: {e}");
                assert_eq!(f.h.head(), head, "{name}: nothing written");
            }
        }
    }
}

/// AC5 (design checks G and H): each refusal leaves the head unchanged and says why; the
/// interactive form keeps its first point after a refusal.
#[test]
fn refusals_write_nothing_and_say_why() {
    let mut f = fixture();
    let first = slash(&mut f.h, (28.0, -3.0), (32.0, 18.0)).unwrap();
    let cable: CableId = id(&first, "cable");
    // Brief 026: the cable has an automatic tag, which the message names.
    let cable_tag = f.h.doc().cables[&cable].properties["tag"].clone();
    let cases: Vec<(&str, Value, String)> = vec![
        (
            "a wire already in a cable",
            json!({"from": [40, 3], "to": [40, 7]}),
            format!("Wire W5 is already in cable {cable_tag}; a wire can be in only one cable. Nothing was created."),
        ),
        ("empty space", json!({"from": [100, 100], "to": [110, 110]}), "The slash crosses no wires; nothing was created.".into()),
        (
            "the plain line only",
            json!({"from": [10, 13], "to": [10, 17]}),
            "The slash crosses no wires; nothing was created. It crosses 1 object that is not a wire.".into(),
        ),
        ("no from", json!({"to": [1, 1]}), "`from` [x,y] is required".into()),
        ("no to", json!({"from": [1, 1]}), "`to` [x,y] is required".into()),
        ("equal points", json!({"from": [1, 1], "to": [1, 1]}), "`from` and `to` must differ".into()),
        ("not a point", json!({"from": "x", "to": [1, 1]}), "`\"x\"` is not a finite [x,y] point".into()),
        ("NaN", json!({"from": [f64::NAN, 0], "to": [1, 1]}), "is not a finite [x,y] point".into()),
        ("beyond 1e12", json!({"from": [1e13, 0], "to": [1, 1]}), "is not a finite [x,y] point".into()),
        ("tag not a string", json!({"from": [28, -3], "to": [32, 18], "tag": 123}), "`tag` must be a string".into()),
    ];
    for (name, params, message) in cases {
        let head = f.h.head();
        let e = f.h.run("cableslash", params).expect_err(name);
        assert!(e.contains(&message), "{name}: {e}");
        assert_eq!(f.h.head(), head, "{name}: head unchanged");
    }
    // Typed JSON the parser rejects never reaches the command, and never panics.
    let head = f.h.head();
    let _ = f.h.s.cmdline(r#"cableslash {"from":[1e309,0],"to":[1,1]}"#);
    let _ = f.h.s.cmdline(r#"cableslash {"from":[NaN,0],"to":[1,1]}"#);
    assert_eq!(f.h.head(), head);
    // Interactive: the refusal is shown and the second-point prompt repeats with the first point.
    f.h.s.start("cableslash").unwrap();
    click(&mut f.h, 40.0, 3.0);
    click(&mut f.h, 40.0, 7.0);
    assert!(f.h.s.running.is_some(), "the command goes on after a refusal");
    assert_eq!(prompt(&f.h), "Specify second point of cable slash or [Undo]:");
    click(&mut f.h, f64::NAN, 0.0);
    assert_eq!(prompt(&f.h), "Specify second point of cable slash or [Undo]:");
    f.h.s.cancel();
    assert_eq!(f.h.head(), head);
    // The preview says so in red before the click.
    f.h.s.start("cableslash").unwrap();
    click(&mut f.h, 40.0, 3.0);
    let cursor = Vec2::new(40.0, 7.0);
    f.h.s.cursor = cursor;
    let preview = f.h.s.preview(cursor);
    assert!(preview.iter().any(|e| matches!(&e.kind, EntityKind::Text(t)
        if t.value == "-> refused: 1 wire already in a cable")
        && e.common.color == CColor::Index(1)));
    let red_rings = preview.iter().filter(|e| matches!(e.kind, EntityKind::Circle(_)) && e.common.color == CColor::Index(1)).count();
    assert_eq!(red_rings, 3, "one bold red ring on W5");
    // No wires is a refusal too: red text (design §5.2).
    // From the first point (40,3) to (45,4): between W0 and W5, crossing nothing.
    let empty = Vec2::new(45.0, 4.0);
    f.h.s.cursor = empty;
    let preview = f.h.s.preview(empty);
    assert!(preview.iter().any(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == "-> no wires") && e.common.color == CColor::Index(1)));
    f.h.s.cancel();
    assert_eq!(members(&mut f.h, cable), vec![f.w0, f.w5, f.w10]);
}

/// Owner answer O5: `cableadd` refuses a conductor already in a cable, as the slash does.
#[test]
fn cableadd_refuses_a_wire_already_in_a_cable() {
    let mut f = fixture();
    let r = slash(&mut f.h, (28.0, -3.0), (32.0, 18.0)).unwrap();
    let cable: CableId = id(&r, "cable");
    let head = f.h.head();
    for params in [json!({"conductors": [f.w5.hex()], "tag": "-W2"}), json!({"cable": cable.hex(), "conductors": [f.w5.hex()]})] {
        let e = f.h.run("cableadd", params).expect_err("refused");
        assert!(e.contains("Wire W5 is already in cable") && e.contains("Nothing was changed."), "{e}");
        assert_eq!(f.h.head(), head);
    }
    // A wire in no cable can still be added.
    let other = wire(&mut f.h, json!([[0, 60], [60, 60]]), "W60");
    let head = f.h.head();
    let e = f.h.run("cableadd", json!({"conductors": [other.hex(), other.hex()]})).expect_err("listed twice");
    assert!(e.contains(&format!("`conductors` lists wire {} twice", other.hex())), "{e}");
    assert_eq!(f.h.head(), head);
    f.h.run("cableadd", json!({"conductors": [other.hex()]})).unwrap();
}

/// AC6 (design check K): erasing the slash keeps the cable; `ecaddelete {cable}` keeps every
/// conductor; undo brings the cable back.
#[test]
fn erasing_the_slash_keeps_the_cable_and_deleting_the_cable_keeps_the_wires() {
    let mut f = fixture();
    let r = slash(&mut f.h, (28.0, -3.0), (32.0, 18.0)).unwrap();
    let (k, item, rid): (CableId, ItemId, RepresentationId) = (id(&r, "cable"), id(&r, "item"), id(&r, "representation"));
    let handle = f.h.handle_of(f.page, item);
    f.h.run("erase", json!({"handles": [handle.hex()]})).unwrap();
    let d = f.h.doc();
    assert_eq!(members(&mut f.h, k), vec![f.w0, f.w5, f.w10], "the cable and its members stay");
    assert!(!d.items.contains_key(&item) && !d.representations.contains_key(&rid), "only the slash went");
    f.h.cmdline("u");

    let before = f.h.doc();
    let head = f.h.head();
    let r = f.h.run("ecaddelete", json!({"cable": k.hex()})).unwrap();
    assert_eq!(f.h.head(), head + 1);
    assert_eq!(r["message"], json!(format!("Cable {} deleted; its 3 wires are kept.", k.hex())));
    let d = f.h.doc();
    assert!(d.cables.is_empty());
    assert!(!d.items.contains_key(&item) && !d.representations.contains_key(&rid), "the slash goes with it");
    assert_eq!(d.conductors, before.conductors, "every conductor stays");
    for c in [f.w0, f.w5, f.w10] {
        assert_eq!(ecad_model::derive::route_on_page(&d, c, f.page), ecad_model::derive::route_on_page(&before, c, f.page), "routes unchanged");
    }
    let tx = f.h.p(|p| p.ledger.log().last().cloned().unwrap());
    let names: Vec<&str> = tx.ops.iter().map(Op::name).collect();
    assert_eq!(names[..3], ["RemoveCableMember"; 3]);
    assert_eq!(names.last(), Some(&"DeleteCable"));
    f.h.cmdline("u");
    assert!(f.h.doc() == before, "undo brings the cable and its slash back");
    // Deleting a member keeps the cable (P7, Q4).
    f.h.run("ecaddelete", json!({"conductor": f.w5.hex()})).unwrap();
    assert_eq!(members(&mut f.h, k), vec![f.w0, f.w10]);
    assert!(f.h.run("ecaddelete", json!({"cable": "zz"})).is_err());
}

/// Design §7.2, §7.3 (checks M and N): magenta dots where the slash crosses its members, hover
/// on the slash and on a member; a moved member keeps its membership but loses its dot.
#[test]
fn the_drawing_shows_the_cable_and_hover_lists_its_wires() {
    let mut f = fixture();
    let r = slash(&mut f.h, (28.0, -3.0), (32.0, 18.0)).unwrap();
    let (k, item): (CableId, ItemId) = (id(&r, "cable"), id(&r, "item"));
    let uid = uid(&mut f.h, f.page);
    let dots = |h: &mut H| -> Vec<Pt> {
        h.p(|p| overlay_marks(p, uid))
            .into_iter()
            .filter(|m| m.shape == MarkShape::Dot && (m.color.0, m.color.1, m.color.2) == (0x7a, 0x3f, 0xe0))
            .map(|m| Pt::new(m.at.x, m.at.y))
            .collect()
    };
    let got = dots(&mut f.h);
    assert_eq!(got.len(), 3, "{got:?}");
    for want in [Pt::new(28.0 + 4.0 * 3.0 / 21.0, 0.0), Pt::new(28.0 + 4.0 * 8.0 / 21.0, 5.0), Pt::new(28.0 + 4.0 * 13.0 / 21.0, 10.0)] {
        assert!(got.iter().any(|p| p.near(want, 1e-9)), "dot at {want:?} in {got:?}");
    }
    let slash_handle = f.h.handle_of(f.page, item);
    let lines = f.h.p(|p| hover_lines(p, uid, slash_handle)).unwrap();
    assert!(lines[0].starts_with("Cable -W"), "{lines:?}");
    assert_eq!(lines[1], format!("id {}", k.hex()));
    // Design 005 §4.2: the view follows the `id` line.
    assert_eq!(lines[2..], ["View: traditional", "Wires: 3", "  W0", "  W5", "  W10"]);
    let w5 = f.h.route_handle(f.page, f.w5);
    let lines = f.h.p(|p| hover_lines(p, uid, w5)).unwrap();
    assert!(lines.iter().any(|l| l.starts_with("In cable -W")), "{lines:?}");
    // Move W5 away: membership stays, its dot goes.
    f.h.run("move", json!({"handles": [w5.hex()], "delta": [0, 50]})).unwrap();
    assert_eq!(members(&mut f.h, k), vec![f.w0, f.w5, f.w10]);
    assert_eq!(dots(&mut f.h).len(), 2);
    let lines = f.h.p(|p| hover_lines(p, uid, slash_handle)).unwrap();
    assert!(lines.contains(&"  W5".to_string()), "hover still lists W5");
}

/// Design §9 (check O): the debugger trail of a cable lists its members first, in member order.
#[test]
fn the_debugger_trail_goes_from_cable_to_its_wires_in_order() {
    let mut f = fixture();
    let r = slash(&mut f.h, (32.0, 18.0), (28.0, -3.0)).unwrap();
    let (k, rid): (CableId, RepresentationId) = (id(&r, "cable"), id(&r, "representation"));
    let d = f.h.doc();
    assert_eq!(
        used_by(&d, ObjectId::Cable(k)),
        vec![ObjectId::Conductor(f.w10), ObjectId::Conductor(f.w5), ObjectId::Conductor(f.w0), ObjectId::Representation(rid)]
    );
}

/// `n` short vertical wires at x = 0, 1, 2 …, committed in one transaction straight to the ledger
/// (a fixture: drawing them one by one would take too long). The page is redrawn by the next edit.
fn many_wires(h: &mut H, n: usize) {
    let xs: Vec<f64> = (0..n).map(|i| i as f64).collect();
    wires_at(h, &xs);
}

/// Short vertical wires from y -1 to 1 at each x, in that order (IDs ascend along `xs`), committed
/// as [`many_wires`] does.
fn wires_at(h: &mut H, xs: &[f64]) {
    h.cfg(|p| {
        let page = p.ledger.document().page_order[0];
        let style = Style { layer: p.default_layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
        let mut ops = Vec::new();
        for &x in xs {
            let (c, item, r): (ConductorId, ItemId, RepresentationId) = (p.ids.fresh(), p.ids.fresh(), p.ids.fresh());
            ops.push(Op::CreateConductor { id: c, conductor: Default::default() });
            ops.push(Op::AddItem {
                id: item,
                item: Item { page, style: style.clone(), kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(x, -1.0), b: Pt::new(x, 1.0) }) },
            });
            ops.push(Op::LinkRepresentation {
                id: r,
                rep: Representation {
                    subject: Subject::Conductor(c),
                    page,
                    form: Form::Drawn,
                    role: "route".into(),
                    items: vec![item],
                    connection_points: Default::default(),
                },
            });
        }
        p.ledger.commit(Author::User, "many wires", ops).unwrap();
    });
}

/// AC8 (design check L): more than `MAX_CABLE_MEMBERS` crossed wires are refused, exactly that
/// many are accepted, and huge coordinates never panic.
#[test]
fn hostile_slashes_never_panic() {
    assert_eq!(MAX_CABLE_MEMBERS, 10_000);
    let mut h = H::new(SEED);
    many_wires(&mut h, MAX_CABLE_MEMBERS + 1);
    let head = h.head();
    let e = h.s.execute("cableslash", &json!({"from": [-1, 0], "to": [MAX_CABLE_MEMBERS as f64 + 1.0, 0]})).unwrap_err();
    assert!(e.to_string().contains("The slash crosses more than 10000 wires; nothing was created."), "{e}");
    assert_eq!(h.head(), head);
    // One fewer: the last wire is outside the slash.
    let r = h.s.execute("cableslash", &json!({"from": [-1, 0], "to": [MAX_CABLE_MEMBERS as f64 - 0.5, 0]})).unwrap();
    assert_eq!(r["conductors"].as_array().unwrap().len(), MAX_CABLE_MEMBERS);
    assert_eq!(h.head(), head + 1);
    h.check();
    // The cap holds for the whole cable: one more wire through `cableadd` is refused.
    let cable = r["cable"].as_str().unwrap().to_string();
    // The one wire the slash did not take (IDs do not follow creation order).
    let taken: Vec<&str> = r["conductors"].as_array().unwrap().iter().map(|c| c.as_str().unwrap()).collect();
    let last = h.doc().conductors.keys().copied().find(|c| !taken.contains(&c.hex().as_str())).unwrap();
    let e = h.s.execute("cableadd", &json!({"cable": cable, "conductors": [last.hex()]})).unwrap_err();
    assert!(e.to_string().contains("a cable holds at most 10000 wires"), "{e}");
    assert_eq!(h.head(), head + 1);

    let mut f = fixture();
    let _ = f.h.run("cableslash", json!({"from": [-1e12, -1e12], "to": [1e12, 1e12]}));
    let _ = f.h.run("cableslash", json!({"from": [-1e12, 0], "to": [1e12, 1e-300]}));
    let _ = f.h.run("cableslash", json!({"from": [0, 0], "to": [1e-300, 0]}));
    f.h.s.start("cableslash").unwrap();
    click(&mut f.h, -1e12, -1e12);
    for cursor in [Vec2::new(1e12, 1e12), Vec2::new(f64::NAN, 0.0), Vec2::new(f64::INFINITY, 1.0), Vec2::new(-1e12, -1e12)] {
        f.h.s.cursor = cursor;
        let _ = f.h.s.preview(cursor);
    }
    f.h.s.cancel();
    f.h.check();
}

/// Review finding (sort order): crossings closer together than the tolerance, with wire IDs in
/// shuffled order against their positions, never panic (the order must be a total order) and give
/// the same member order every time; crossings within tolerance of each other go by wire ID.
#[test]
fn crossings_closer_than_the_tolerance_are_ordered_without_panicking() {
    const N: usize = 2_000;
    // A fixed shuffle of positions 0.6e-6 apart (tolerance 1e-6): neighbours tie, next-but-one
    // neighbours do not.
    let mut xs: Vec<f64> = (0..N).map(|i| i as f64 * 0.6e-6).collect();
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    for i in (1..N).rev() {
        state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        xs.swap(i, (state >> 33) as usize % (i + 1));
    }
    let run = || {
        let mut h = H::new(SEED);
        wires_at(&mut h, &xs);
        let r = h.s.execute("cableslash", &json!({"from": [-1, 0], "to": [1, 0]})).unwrap();
        r["conductors"].as_array().unwrap().iter().map(|c| c.as_str().unwrap().to_string()).collect::<Vec<_>>()
    };
    let first = run();
    assert_eq!(first.len(), N);
    assert_eq!(run(), first, "the same order every time");
    // Two wires at the same point: by ID.
    let mut h = H::new(SEED);
    wires_at(&mut h, &[0.5, 0.0, 0.0]);
    let r = h.s.execute("cableslash", &json!({"from": [-1, 0], "to": [1, 0]})).unwrap();
    let got: Vec<&str> = r["conductors"].as_array().unwrap().iter().map(|c| c.as_str().unwrap()).collect();
    let mut tied = got[..2].to_vec();
    tied.sort();
    assert_eq!(got[..2], tied[..], "a tie goes by wire ID");
}
