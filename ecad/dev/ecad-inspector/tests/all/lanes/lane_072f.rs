//! Lane 072 F (design 072 §6.2, acceptance A15): drawing aids and the ops that change them are
//! shown in the Readable view of their transactions, and the Raw view is the stored bytes.

use std::time::Duration;

use ecad_inspector::source::{FileSource, Source, VerifyState};
use ecad_model::geometry::units::Celsius;
use ecad_model::objects::aid::{Caption, ContinuationName};
use ecad_model::{Aid, AidId, Author, IdAllocator, LocationBox, Op};

use crate::common::*;

#[test]
fn aids_and_their_ops_are_shown_raw_and_readable() {
    let dir = TempDir::new("072f-aid");
    let mut l = history();
    let mut ids = IdAllocator::seeded(0x72f);
    let (a, b): (AidId, AidId) = (ids.fresh(), ids.fresh());
    let arrow = Aid::Continuation { name: Some(ContinuationName::new("24V").unwrap()), caption: None };
    let boxed = Aid::Box(LocationBox { panel_temperature: Some(Celsius(35.0)), ..LocationBox::default() });
    l.commit(Author::User, "aids", vec![Op::CreateAid { id: a, aid: arrow.clone() }, Op::CreateAid { id: b, aid: boxed }]).unwrap();
    let captioned = Aid::Continuation { name: Some(ContinuationName::new("24V").unwrap()), caption: Some(Caption::new("To the PSU").unwrap()) };
    l.commit(Author::User, "caption", vec![Op::ModifyAid { id: a, before: arrow, after: captioned }]).unwrap();
    let path = written(&dir, &l);

    let log = run(&[path.to_str().unwrap(), "log", "main", "--last", "2"]).unwrap();
    for want in ["CreateAid", "ModifyAid", "24V", "To the PSU"] {
        assert!(log.contains(want), "{want} in the readable log: {log}");
    }
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let rows = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = rows["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let keys: Vec<String> = rows.iter().rev().take(2).filter_map(|r| r["key"].as_str().map(str::to_string)).collect();
    let mut text = String::new();
    for key in &keys {
        let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
        text.push_str(&readable["ok"]["decoded"].to_string());
        let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
        assert!(raw["ok"]["hex"].as_str().is_some_and(|h| !h.is_empty()), "the raw row of {key}");
    }
    for want in ["CreateAid", "ModifyAid", "continuation", "box", "panel_temperature", "caption"] {
        assert!(text.contains(want), "{want} in the readable records: {text}");
    }
    // The snapshot (with the aids in their own `model:aid:` chunks) reads back.
    let summary = ask(s, serde_json::json!({"op": "summary"}));
    assert!(summary.get("ok").is_some(), "{summary}");
}

/// Lane F F1b: a connection point says what owns it, a device or a drawing aid, in both views.
#[test]
fn a_points_owner_is_shown_raw_and_readable() {
    use ecad_model::{ConnectionPoint, ConnectionPointId, Device, DeviceId};
    let dir = TempDir::new("072f-owner");
    let mut l = history();
    let mut ids = IdAllocator::seeded(0x72f1);
    let (a, d): (AidId, DeviceId) = (ids.fresh(), ids.fresh());
    let (ta, td): (ConnectionPointId, ConnectionPointId) = (ids.fresh(), ids.fresh());
    let arrow = Aid::Continuation { name: Some(ContinuationName::new("24V").unwrap()), caption: None };
    let point = |owner: ecad_model::ConnectionPointOwner| ConnectionPoint { owner, label: None, key: None, accepts: Default::default() };
    l.commit(
        Author::User,
        "points",
        vec![
            Op::CreateAid { id: a, aid: arrow },
            Op::AddConnectionPoint { id: ta, connection_point: point(a.into()) },
            Op::CreateDevice { id: d, device: Device::default() },
            Op::AddConnectionPoint { id: td, connection_point: point(d.into()) },
        ],
    )
    .unwrap();
    let path = written(&dir, &l);

    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let rows = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = rows["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let key = rows.last().and_then(|r| r["key"].as_str()).map(str::to_string).expect("the last transaction");
    let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
    let text = readable["ok"]["decoded"].to_string();
    for want in ["AddConnectionPoint", "owner", "aid", "device", &a.hex(), &d.hex()] {
        assert!(text.contains(want), "{want} in the readable record: {text}");
    }
    let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
    assert!(raw["ok"]["hex"].as_str().is_some_and(|h| !h.is_empty()), "the raw row");
    let summary = ask(s, serde_json::json!({"op": "summary"}));
    assert!(summary.get("ok").is_some(), "{summary}");
}

/// F1b part 2: a continuation is drawn as an aid's symbol (`Subject::Aid`) and renamed with
/// `ModifyAid`; both read in the Raw and Readable views.
#[test]
fn a_continuation_aid_and_its_rename_are_shown_raw_and_readable() {
    use ecad_model::{
        ConnectionPoint, ConnectionPointId, Form, Item, ItemId, ItemKind, Primitive, Pt, Representation, RepresentationId, Style, Subject,
    };
    let dir = TempDir::new("072f-rename");
    let mut l = history();
    let mut ids = IdAllocator::seeded(0x72f2);
    let (a, t, i, r): (AidId, ConnectionPointId, ItemId, RepresentationId) = (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
    let page = *l.document().page_order().first().unwrap();
    let layer = *l.document().layers().keys().next().unwrap();
    let style = Style { layer, color: ecad_model::Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
    let named = |n: &str| Aid::Continuation { name: Some(ContinuationName::new(n).unwrap()), caption: None };
    l.commit(
        Author::User,
        "arrow",
        vec![
            Op::CreateAid { id: a, aid: named("24V") },
            Op::AddConnectionPoint {
                id: t,
                connection_point: ConnectionPoint { owner: a.into(), label: None, key: None, accepts: Default::default() },
            },
            Op::AddItem { id: i, item: Item { page, style, kind: ItemKind::Primitive(Primitive::Point { at: Pt::new(10.0, 10.0) }) } },
            Op::LinkRepresentation {
                id: r,
                rep: Representation {
                    subject: Subject::Aid(a),
                    page,
                    form: Form::Drawn,
                    role: ecad_model::Role::Part(Default::default()),
                    items: vec![i],
                    connection_points: Default::default(),
                },
            },
        ],
    )
    .unwrap();
    l.commit(Author::User, "rename", vec![Op::ModifyAid { id: a, before: named("24V"), after: named("0V") }]).unwrap();
    let path = written(&dir, &l);
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let rows = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = rows["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let keys: Vec<String> = rows.iter().rev().take(2).filter_map(|r| r["key"].as_str().map(str::to_string)).collect();
    assert_eq!(keys.len(), 2, "{rows:?}");
    let text =
        |key: &str| ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}))["ok"]["decoded"].to_string();
    let (rename, placed) = (text(&keys[0]), text(&keys[1]));
    for want in ["ModifyAid", "24V", "0V", &a.hex()] {
        assert!(rename.contains(want), "{want} in the rename: {rename}");
    }
    for want in ["CreateAid", "LinkRepresentation", "aid", &a.hex()] {
        assert!(placed.contains(want), "{want} in the placing: {placed}");
    }
    for key in &keys {
        let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
        assert!(raw["ok"]["hex"].as_str().is_some_and(|h| !h.is_empty()), "the raw row");
    }
}

/// F1c: a location box is drawn as an aid's outline (`Subject::Aid`, role `box`) and edited with
/// `ModifyAid`; both read in the Raw and Readable views.
#[test]
fn a_location_box_aid_and_its_edit_are_shown_raw_and_readable() {
    use ecad_model::{Form, Item, ItemId, ItemKind, LocationId, Primitive, Pt, Representation, RepresentationId, Style, Subject};
    let dir = TempDir::new("072f-box");
    let mut l = history();
    let mut ids = IdAllocator::seeded(0x72f3);
    let (a, i, r): (AidId, ItemId, RepresentationId) = (ids.fresh(), ids.fresh(), ids.fresh());
    let page = *l.document().page_order().first().unwrap();
    let layer = *l.document().layers().keys().next().unwrap();
    let style = Style { layer, color: ecad_model::Color::ByLayer, linetype: "DASHED".into(), lineweight: 25 };
    let outline = vec![Pt::new(0.0, 0.0), Pt::new(40.0, 0.0), Pt::new(40.0, 30.0), Pt::new(0.0, 30.0)];
    let boxed = Aid::Box(LocationBox { location: Some(LocationId(0xc1)), ..LocationBox::default() });
    l.commit(
        Author::User,
        "box",
        vec![
            Op::CreateAid { id: a, aid: boxed.clone() },
            Op::AddItem { id: i, item: Item { page, style, kind: ItemKind::Primitive(Primitive::Polyline { vertices: outline, closed: true }) } },
            Op::LinkRepresentation {
                id: r,
                rep: Representation {
                    subject: Subject::Aid(a),
                    page,
                    form: Form::Drawn,
                    role: ecad_model::Role::LocationBox,
                    items: vec![i],
                    connection_points: Default::default(),
                },
            },
        ],
    )
    .unwrap();
    // F1c-2: a box keeps its own location style.
    let own = ecad_model::StyleRef::Own(0x5e);
    let warmer = Aid::Box(LocationBox {
        location: Some(LocationId(0xc2)),
        panel_temperature: Some(Celsius(40.0)),
        style: Some(own.clone()),
        ..LocationBox::default()
    });
    l.commit(Author::User, "edit", vec![Op::ModifyAid { id: a, before: boxed, after: warmer }]).unwrap();
    let path = written(&dir, &l);
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let rows = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = rows["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let keys: Vec<String> = rows.iter().rev().take(2).filter_map(|r| r["key"].as_str().map(str::to_string)).collect();
    assert_eq!(keys.len(), 2, "{rows:?}");
    let text =
        |key: &str| ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}))["ok"]["decoded"].to_string();
    let (edit, placed) = (text(&keys[0]), text(&keys[1]));
    for want in ["ModifyAid", "box", "location", "panel_temperature", "style", &own.text(), &a.hex()] {
        assert!(edit.contains(want), "{want} in the edit: {edit}");
    }
    for want in ["CreateAid", "LinkRepresentation", "box", "aid", &a.hex()] {
        assert!(placed.contains(want), "{want} in the placing: {placed}");
    }
    for key in &keys {
        let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
        assert!(raw["ok"]["hex"].as_str().is_some_and(|h| !h.is_empty()), "the raw row");
    }
    // Raw: exactly the stored bytes of the edit with the style.
    let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": &keys[0], "form": "raw"}));
    let r = f.store().unwrap().reader().unwrap();
    let mut bytes = None;
    r.raw_rows(|t, k, v| {
        if t == "transactions" && ecad_inspector::text::hex(k) == keys[0] {
            bytes = Some(v.to_vec());
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(raw["ok"]["hex"], ecad_inspector::text::hex(&bytes.unwrap()), "the raw edit is its stored bytes");
    let summary = ask(s, serde_json::json!({"op": "summary"}));
    assert!(summary.get("ok").is_some(), "{summary}");
}

/// F2a: a loose part (design 072 §6.4) and its edit read in the Raw and Readable views, and the
/// snapshot (with the part in its own `model:loose_part:` chunk) reads back.
#[test]
fn a_loose_part_and_its_edit_are_shown_raw_and_readable() {
    use ecad_model::objects::loose_part::LooseDescription as Description;
    use ecad_model::objects::loose_part::{ItemUnit, LoosePart, Qty};
    use ecad_model::{LocationId, LoosePartId};
    let dir = TempDir::new("072f-loose");
    let mut l = history();
    let mut ids = IdAllocator::seeded(0x72f4);
    let a: LoosePartId = ids.fresh();
    let rail = LoosePart::new(Description::new("DIN rail").unwrap(), Qty(2000));
    l.commit(Author::User, "loose part", vec![Op::CreateLoosePart { id: a, part: rail.clone() }]).unwrap();
    let cut = LoosePart { qty: Qty(3600), unit: ItemUnit::Metres, location: Some(LocationId(0xc1)), ..rail.clone() };
    l.commit(Author::User, "edit", vec![Op::ModifyLoosePart { id: a, before: rail, after: cut }]).unwrap();
    let path = written(&dir, &l);

    let log = run(&[path.to_str().unwrap(), "log", "main", "--last", "2"]).unwrap();
    for want in ["CreateLoosePart", "ModifyLoosePart", "DIN rail", "3.6"] {
        assert!(log.contains(want), "{want} in the readable log: {log}");
    }
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let rows = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = rows["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let keys: Vec<String> = rows.iter().rev().take(2).filter_map(|r| r["key"].as_str().map(str::to_string)).collect();
    assert_eq!(keys.len(), 2, "{rows:?}");
    let text =
        |key: &str| ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}))["ok"]["decoded"].to_string();
    let (edit, made) = (text(&keys[0]), text(&keys[1]));
    for want in ["ModifyLoosePart", "DIN rail", "qty", "3.6", "unit", "location", &a.hex()] {
        assert!(edit.contains(want), "{want} in the edit: {edit}");
    }
    for want in ["CreateLoosePart", "DIN rail", "qty", &a.hex()] {
        assert!(made.contains(want), "{want} in the making: {made}");
    }
    // Raw: exactly the stored bytes of each row.
    let r = f.store().unwrap().reader().unwrap();
    let mut stored = std::collections::BTreeMap::new();
    r.raw_rows(|t, k, v| {
        if t == "transactions" {
            stored.insert(ecad_inspector::text::hex(k), ecad_inspector::text::hex(v));
        }
        Ok(())
    })
    .unwrap();
    for key in &keys {
        let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
        assert_eq!(raw["ok"]["hex"].as_str(), stored.get(key).map(String::as_str), "the raw row of {key} is its stored bytes");
    }
    let summary = ask(s, serde_json::json!({"op": "summary"}));
    assert!(summary.get("ok").is_some(), "{summary}");
}

/// F2b: a file holding a BOM item in its old device form (`kind: bom_item`) is not read by the
/// program (design 072 §6.4), but the inspector still shows it: the Raw view is the stored bytes
/// and the Readable view names the old kind.
#[test]
fn an_old_bom_item_device_is_still_shown_raw_and_readable() {
    use ecad_model::{Device, DeviceId};
    let dir = TempDir::new("072f-old-item");
    let mut l = history();
    let mut ids = IdAllocator::seeded(0x72f5);
    let c: DeviceId = ids.fresh();
    let mut old = Device::default();
    old.properties.insert("kind".into(), "bom_item".into());
    old.properties.insert("qty".into(), "2".into());
    l.commit(Author::User, "old item", vec![Op::CreateDevice { id: c, device: old }]).unwrap();
    let path = written(&dir, &l);

    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let rows = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = rows["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let key = rows.last().and_then(|r| r["key"].as_str()).unwrap_or_else(|| panic!("a transaction row: {rows:?}")).to_string();
    let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
    let text = readable["ok"]["decoded"].to_string();
    for want in ["CreateDevice", "bom_item", &c.hex()] {
        assert!(text.contains(want), "{want} in the readable row: {readable}");
    }
    let r = f.store().unwrap().reader().unwrap();
    let mut stored = None;
    r.raw_rows(|t, k, v| {
        if t == "transactions" && ecad_inspector::text::hex(k) == key {
            stored = Some(ecad_inspector::text::hex(v));
        }
        Ok(())
    })
    .unwrap();
    let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
    assert_eq!(raw["ok"]["hex"].as_str(), stored.as_deref(), "the raw row is its stored bytes");
}
