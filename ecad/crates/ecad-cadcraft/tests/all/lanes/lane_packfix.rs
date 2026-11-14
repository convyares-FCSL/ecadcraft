//! The drawing-pack re-run's bugs (2026-11-14), one test or more per bug, each from a generic
//! repro: project symbols, tags, contact tables, references and PLC card drawing.

use ecad_model::{ComponentId, Document, PageId};
use serde_json::{Value, json};

use crate::common::{H, id};

/// A one-point earth symbol whose point has an empty label, prefix PE.
fn earth(h: &mut H) {
    h.run(
        "symbolnew",
        json!({
            "name": "Earth", "key": "e1",
            "graphics": [{"kind": "line", "a": [-3, 0], "b": [3, 0]}],
            "points": [{"key": "PE", "label": "", "at": [0, 0]}],
            "slots": [{"text": "tag", "at": [4, 0]}],
            "defaults": {"prefix": "PE"},
        }),
    )
    .unwrap();
}

fn placed(h: &mut H, params: Value) -> ComponentId {
    id(&h.run("place", params).unwrap(), "component")
}

fn tag(d: &Document, c: ComponentId) -> String {
    d.components.get(&c).and_then(|x| x.properties.get("tag").cloned()).unwrap_or_default()
}

/// Every text value derived on `page`, shown or not.
fn page_texts(h: &mut H, page: PageId) -> Vec<String> {
    let d = h.doc();
    ecad_cadcraft::devices::all_texts_on_page(&d, page).into_iter().map(|t| t.text.value).collect()
}

fn is_hex_id(s: &str) -> bool {
    s.len() == 32 && s.chars().all(|c| c.is_ascii_hexdigit())
}

// ---------------------------------------------------------------- bug 1

#[test]
fn bug_1_an_unlabelled_point_never_prints_its_id_after_a_merge() {
    let mut h = H::new(9101);
    let page = h.first_page();
    earth(&mut h);
    let a = placed(&mut h, json!({"symbol": "e1", "at": [50, 50]}));
    let b = placed(&mut h, json!({"symbol": "e1", "at": [80, 50]}));
    let ta = tag(&h.doc(), a);
    h.run("setprop", json!({"component": b.hex(), "key": "tag", "value": ta, "merge": true})).unwrap();
    let d = h.doc();
    assert!(!d.components.contains_key(&b), "the second earth merged into the first");
    for t in page_texts(&mut h, page) {
        assert!(!is_hex_id(&t), "a text prints a 32-digit ID: {t}");
    }
    // Both points are still named by their key, unlabelled.
    let terms: Vec<_> = d.terminals_of(a).map(|(_, t)| (t.key.clone(), t.label.clone())).collect();
    assert_eq!(terms.len(), 2);
    assert!(terms.iter().all(|(k, l)| l.is_none() && k.as_deref().is_some_and(|k| !is_hex_id(k))), "{terms:?}");
}

// ---------------------------------------------------------------- bug 2

#[test]
fn bug_2_place_names_a_new_project_symbol_device_by_its_typed_tag() {
    let mut h = H::new(9102);
    h.run(
        "symbolnew",
        json!({
            "name": "Sensor", "key": "s1",
            "graphics": [{"kind": "line", "a": [-3, 0], "b": [3, 0]}],
            "points": [{"key": "1", "at": [-5, 0]}, {"key": "2", "at": [5, 0]}],
            "slots": [{"text": "tag", "at": [0, 4]}],
            "defaults": {"prefix": "B"},
        }),
    )
    .unwrap();
    let a = placed(&mut h, json!({"symbol": "s1", "at": [100, 100], "tag": "B07"}));
    assert_eq!(tag(&h.doc(), a), "-B07");
    // Without a tag: the next free number, as before.
    let b = placed(&mut h, json!({"symbol": "s1", "at": [120, 100]}));
    assert_ne!(tag(&h.doc(), b), "-B07");
    // A tag a device holds is that device, as for built-in symbols: this symbol would join it, and
    // its points 1 and 2 are taken there.
    let e = h.run("place", json!({"symbol": "s1", "at": [140, 100], "tag": "b7"})).unwrap_err();
    assert!(e.contains("-B07 already has a connection point"), "{e}");
    // `tag` and `device` together are refused.
    assert!(h.run("place", json!({"symbol": "s1", "at": [160, 100], "tag": "B9", "device": a.hex()})).is_err());
}

// ---------------------------------------------------------------- bug 3

#[test]
fn bug_3_a_located_tag_keeps_a_separator_with_the_tag_dash_off() {
    for dash in [false, true] {
        let mut h = H::new(9103);
        let page = h.first_page();
        h.run("projectprops", json!({"tag_dash": dash})).unwrap();
        let k = placed(&mut h, json!({"symbol": "coil", "at": [100, 100]}));
        h.run("setprop", json!({"component": k.hex(), "key": "location", "value": "X1"})).unwrap();
        let bare = tag(&h.doc(), k);
        let bare = bare.trim_start_matches('-');
        let texts = page_texts(&mut h, page);
        assert!(texts.contains(&format!("+X1-{bare}")), "dash {dash}: {texts:?}");
        assert!(!texts.contains(&format!("+X1{bare}")), "dash {dash}: {texts:?}");
    }
    assert_eq!(ecad_cadcraft::locations::located_tag("+X1", "K01"), "+X1-K01");
    assert_eq!(ecad_cadcraft::locations::located_tag("+X1", "-K01"), "+X1-K01");
    assert_eq!(ecad_cadcraft::locations::located_tag("+X1", "=PWR"), "+X1=PWR");
}

// ---------------------------------------------------------------- bug 4

/// Each contact table's box on `page` (its lines and text anchors), by head.
fn table_boxes(h: &mut H, page: PageId) -> Vec<(ecad_model::Pt, ecad_model::Pt)> {
    use ecad_cadcraft::xref::Shape;
    use std::collections::BTreeMap;
    let parts = h.p(|p| ecad_cadcraft::xref::parts_on_page(p.ledger.document(), page));
    let mut boxes: BTreeMap<_, (ecad_model::Pt, ecad_model::Pt)> = BTreeMap::new();
    for part in parts.iter().filter(|p| parts.iter().any(|q| q.rep == p.rep && q.bar)) {
        let pts = match &part.shape {
            Shape::Line(a, b) => vec![*a, *b],
            Shape::Text { at, .. } => vec![*at],
        };
        for q in pts {
            let e = boxes.entry(part.rep).or_insert((q, q));
            e.0 = ecad_model::Pt::new(e.0.x.min(q.x), e.0.y.min(q.y));
            e.1 = ecad_model::Pt::new(e.1.x.max(q.x), e.1.y.max(q.y));
        }
    }
    boxes.into_values().collect()
}

#[test]
fn bug_4_tables_of_coils_drawn_close_together_do_not_overlap() {
    let mut h = H::new(9104);
    let page = h.first_page();
    let coils: Vec<ComponentId> = [100.0, 115.0, 130.0, 145.0].iter().map(|x| placed(&mut h, json!({"symbol": "coil", "at": [x, 70.0]}))).collect();
    h.run("newpage", json!({"title": "Contacts"})).unwrap();
    for (i, k) in coils.iter().enumerate() {
        let x = 60.0 + 30.0 * i as f64;
        h.run("place", json!({"symbol": "no", "at": [x, 150.0], "device": k.hex()})).unwrap();
        h.run("place", json!({"symbol": "nc", "at": [x, 100.0], "device": k.hex()})).unwrap();
    }
    let boxes = table_boxes(&mut h, page);
    assert_eq!(boxes.len(), 4, "one table per coil");
    for (i, a) in boxes.iter().enumerate() {
        for b in boxes.iter().skip(i + 1) {
            let apart = a.1.x <= b.0.x || b.1.x <= a.0.x || a.1.y <= b.0.y || b.1.y <= a.0.y;
            assert!(apart, "tables overlap: {a:?} {b:?}");
        }
    }
    // A table on its own keeps its place under its coil.
    let mut solo = H::new(9105);
    let p = solo.first_page();
    let k = placed(&mut solo, json!({"symbol": "coil", "at": [100.0, 70.0]}));
    solo.run("newpage", json!({"title": "Contacts"})).unwrap();
    solo.run("place", json!({"symbol": "no", "at": [60.0, 150.0], "device": k.hex()})).unwrap();
    let one = table_boxes(&mut solo, p);
    let top = solo.p(|p| ecad_cadcraft::xref::contacts_json(p.ledger.document(), k)).unwrap()["table"]["top"][0].as_f64().unwrap();
    assert_eq!(one.len(), 1);
    assert!(one.iter().all(|b| (b.0.x - (top - 10.0)).abs() < 1e-6), "{one:?} {top}");
    // The leftmost of the four keeps that same place: only the later ones move.
    assert!(boxes.iter().any(|b| (b.0.x - (top + 0.0 - 10.0)).abs() < 1e-6), "{boxes:?}");
}

// ---------------------------------------------------------------- bug 11

#[test]
fn bug_11_four_pole_power_contacts_show_their_coil_like_no_and_nc() {
    use ecad_cadcraft::xref::Shape;
    let mut h = H::new(9111);
    let p1 = h.first_page();
    let p2: PageId = id(&h.run("newpage", json!({"title": "Coils"})).unwrap(), "page");
    let k = placed(&mut h, json!({"symbol": "coil", "at": [100, 150]}));
    h.activate(p1);
    let power = h.run("place", json!({"symbol": "power4", "at": [100, 150], "device": k.hex()})).unwrap();
    let nc = h.run("place", json!({"symbol": "nc", "at": [200, 150], "device": k.hex()})).unwrap();
    let parts = h.p(|p| ecad_cadcraft::xref::parts_on_page(p.ledger.document(), p1));
    let text_of = |rep: ecad_model::RepresentationId| {
        parts.iter().find(|x| x.rep == rep).and_then(|x| match &x.shape {
            Shape::Text { value, .. } => Some(value.clone()),
            Shape::Line(..) => None,
        })
    };
    let nc_text = text_of(id(&nc, "representation")).expect("the NC shows its coil");
    assert_eq!(text_of(id(&power, "representation")).as_deref(), Some(nc_text.as_str()), "the power contacts show the same place");
    // The coil's table lists four power poles and the NC.
    let rows = h.p(|p| ecad_cadcraft::xref::contacts_json(p.ledger.document(), k)).unwrap();
    assert!(p2 != p1);
    assert_eq!(rows["rows"].as_array().map(Vec::len), Some(5), "{rows}");
}

// ---------------------------------------------------------------- PLC fences

/// Vertical wires at `xs` from y 250 down to 150.
fn wires(h: &mut H, xs: &[f64]) {
    for x in xs {
        h.run("wire", json!({"points": crate::common::pts(&[(*x, 250.0), (*x, 150.0)])})).unwrap();
    }
}

fn card_of(v: &Value) -> ComponentId {
    id(v, "card")
}

// ---------------------------------------------------------------- bug 5

#[test]
fn bug_5_a_card_drawn_in_several_calls_has_one_frame() {
    let mut h = H::new(9105);
    let page = h.first_page();
    wires(&mut h, &[100.0, 120.0, 140.0, 160.0]);
    let v = h.run("plcchannels", json!({"type": "DI", "first": 0, "from": [95, 200], "to": [125, 200]})).unwrap();
    let card = card_of(&v);
    h.run("plcchannels", json!({"card": card.hex(), "first": 3, "from": [155, 200], "to": [165, 200]})).unwrap();
    h.run("place", json!({"symbol": "chdi", "at": [140, 200], "device": card.hex(), "channel": 2})).unwrap();
    let runs = h.p(|p| ecad_cadcraft::plc::look::runs_of_card(p.ledger.document(), card, page));
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs.first().map(|r| (r.first, r.last)), Some((Some(0), Some(3))));
    // The placed cell's point is on the wire, on the line of the others.
    let d = h.doc();
    let ch2 = d.terminals_of(card).find(|(_, t)| t.key.as_deref() == Some(".2.in")).map(|(t, _)| t).unwrap();
    let at = h.terminal_pos(page, ch2);
    assert!((at.y - 200.0).abs() < 1e-6 && (at.x - 140.0).abs() < 1e-6, "{at:?}");
    assert!(!h.doc().ends_at(ch2).is_empty(), "channel 2 lands on its wire");
}

// ---------------------------------------------------------------- bug 7

/// Each drawn channel's address text on `page`, read with the kept PLC model as the page draws it.
fn addresses(h: &mut H, page: PageId) -> Vec<String> {
    let d = h.doc();
    h.cfg(|p| {
        let plc = p.outputs().plc.clone();
        ecad_cadcraft::devices::all_texts_on_page_with(&d, page, Some(&plc))
            .into_iter()
            .filter(|t| t.text.value.starts_with("IW"))
            .map(|t| t.text.value)
            .collect()
    })
}

#[test]
fn bug_7_raising_a_cards_channel_count_addresses_the_new_channels() {
    let mut h = H::new(9107);
    let page = h.first_page();
    super::lane_045::placed(&mut h, "plccpu", 50.0, 400.0);
    let card = super::lane_045::placed(&mut h, "plcrtd", 100.0, 400.0);
    for i in 0..4 {
        let x = 60.0 + 40.0 * f64::from(i);
        h.run("place", json!({"symbol": "chrtd", "at": [x, 300.0], "device": card.hex()})).unwrap();
    }
    assert_eq!(addresses(&mut h, page).len(), 4);
    h.run("setprop", json!({"component": card.hex(), "key": "channels", "value": "8"})).unwrap();
    assert_eq!(super::lane_045::prop(&mut h, card, "plc.channels").as_deref(), Some("8"), "`channels` is the card's count");
    assert_eq!(super::lane_045::prop(&mut h, card, "channels"), None, "no free property beside it");
    for i in 4..8 {
        let x = 60.0 + 40.0 * f64::from(i);
        h.run("place", json!({"symbol": "chrtd", "at": [x, 300.0], "device": card.hex()})).unwrap();
    }
    let all = addresses(&mut h, page);
    assert_eq!(all.len(), 8, "every channel has its address: {all:?}");
    // On a device that is not a card, `channels` stays free text.
    let k = placed(&mut h, json!({"symbol": "coil", "at": [100, 100]}));
    h.run("setprop", json!({"component": k.hex(), "key": "channels", "value": "x"})).unwrap();
    assert_eq!(super::lane_045::prop(&mut h, k, "channels").as_deref(), Some("x"));
    super::lane_045::same_as_built(&mut h);
}

/// The key of the card's point that wire `x` (vertical, through y 200) is attached to.
fn landed_on(h: &mut H, card: ComponentId, page: PageId, x: f64) -> Vec<String> {
    let d = h.doc();
    d.terminals_of(card)
        .filter(|(t, _)| !d.ends_at(*t).is_empty())
        .filter(|(t, _)| (h.terminal_pos(page, *t).x - x).abs() < 1e-6)
        .filter_map(|(_, term)| term.key.clone())
        .collect()
}

// ---------------------------------------------------------------- bug 6

#[test]
fn bug_6_a_multi_point_cell_spans_its_own_wires() {
    // (type, wires, the point keys landed on each wire left to right)
    let cases: [(&str, Vec<f64>, Vec<&str>); 3] = [
        ("AI", vec![100.0, 110.0], vec![".0.+", ".0.-"]),
        ("RTD", vec![100.0, 110.0, 120.0, 130.0], vec![".0.I+", ".0.S+", ".0.S-", ".0.I-"]),
        // Two channels: two pairs, one cell width apart.
        ("AI", vec![100.0, 110.0, 120.0, 130.0], vec![".0.+", ".0.-", ".1.+", ".1.-"]),
    ];
    for (seed, (ty, xs, want)) in (9160..).zip(cases) {
        let mut h = H::new(seed);
        let page = h.first_page();
        wires(&mut h, &xs);
        let last = xs.last().copied().unwrap_or(0.0);
        let v = h.run("plcchannels", json!({"type": ty, "from": [xs[0] - 3.0, 200.0], "to": [last + 3.0, 200.0]})).unwrap();
        let card = card_of(&v);
        let got: Vec<String> = xs.iter().flat_map(|x| landed_on(&mut h, card, page, *x)).collect();
        assert_eq!(got, want, "{ty} over {xs:?}");
        h.check();
    }
    // Spaced as cells, not as points: one cell per wire, as before.
    let mut h = H::new(9169);
    wires(&mut h, &[100.0, 120.0]);
    let v = h.run("plcchannels", json!({"type": "AI", "from": [95, 200], "to": [125, 200]})).unwrap();
    assert_eq!(v["channels"], json!([0, 1]));
    // Pairs closer than a cell width are still refused.
    let mut h = H::new(9168);
    wires(&mut h, &[100.0, 110.0, 115.0, 125.0]);
    assert!(h.run("plcchannels", json!({"type": "AI", "from": [95, 200], "to": [130, 200]})).is_err());
}

// ---------------------------------------------------------------- bug 8

#[test]
fn bug_8_cells_on_wires_running_down_are_turned_with_their_frame_above() {
    let mut h = H::new(9108);
    let page = h.first_page();
    // Wires from y 230 down to 150; the fence just under their top.
    for x in [100.0, 120.0] {
        h.run("wire", json!({"points": crate::common::pts(&[(x, 230.0), (x, 150.0)])})).unwrap();
    }
    let v = h.run("plcchannels", json!({"type": "DQ", "from": [95, 228], "to": [125, 228]})).unwrap();
    let card = card_of(&v);
    let runs = h.p(|p| ecad_cadcraft::plc::look::runs_of_card(p.ledger.document(), card, page));
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert!(runs.iter().all(|r| r.dir > 0.0 && (r.top - 228.0).abs() < 1e-6), "the frame grows up from the points: {runs:?}");
    for x in [100.0, 120.0] {
        assert_eq!(landed_on(&mut h, card, page, x).len(), 1, "wire {x} lands");
    }
    // Wires running up (sensors above): not turned, as before.
    let mut h = H::new(9109);
    let page = h.first_page();
    for x in [100.0, 120.0] {
        h.run("wire", json!({"points": crate::common::pts(&[(x, 270.0), (x, 190.0)])})).unwrap();
    }
    let v = h.run("plcchannels", json!({"type": "DI", "from": [95, 192], "to": [125, 192]})).unwrap();
    let runs = h.p(|p| ecad_cadcraft::plc::look::runs_of_card(p.ledger.document(), card_of(&v), page));
    assert!(runs.iter().all(|r| r.dir < 0.0), "{runs:?}");
}

// ---------------------------------------------------------------- bug 10

#[test]
fn bug_10_an_io_link_port_lands_c_q_by_default_and_point_chooses() {
    let mut h = H::new(9110);
    let page = h.first_page();
    wires(&mut h, &[100.0]);
    let v = h.run("plcchannels", json!({"type": "IOL", "from": [94, 200], "to": [106, 200]})).unwrap();
    assert_eq!(landed_on(&mut h, card_of(&v), page, 100.0), [".0.C/Q"]);
    // `point` chooses.
    let mut h = H::new(9111);
    let page = h.first_page();
    wires(&mut h, &[100.0]);
    let v = h.run("plcchannels", json!({"type": "IOL", "point": "l+", "from": [94, 200], "to": [106, 200]})).unwrap();
    assert_eq!(landed_on(&mut h, card_of(&v), page, 100.0), [".0.L+"]);
    let mut h = H::new(9112);
    wires(&mut h, &[100.0]);
    let e = h.run("plcchannels", json!({"type": "IOL", "point": "Q", "from": [94, 200], "to": [106, 200]})).unwrap_err();
    assert!(e.contains("L+, C/Q, L-"), "{e}");
}
