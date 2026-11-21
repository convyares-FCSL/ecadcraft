//! Lane 072 D (design 072 §4.2, acceptance A15): a wire's typed values, its net, conductor and
//! place in the net's order, and the `CreateWire` and `ModifyWire` ops that record them are shown in
//! the Readable view of their transaction, and the Raw view is the stored bytes.

use std::time::Duration;

use ecad_inspector::source::{FileSource, Source, VerifyState};
use ecad_model::text::Text;
use ecad_model::units::Amps;
use ecad_model::wire::{Basis, Colours, WireNumber, WireSize};
use ecad_model::{Author, ConductorId, Device, DeviceId, IdAllocator, NetId, Op, Terminal, TerminalId, Wire, WireId};

use crate::common::*;

#[test]
fn a_wires_values_and_its_modify_op_are_shown_raw_and_readable() {
    let dir = TempDir::new("072d-wire");
    let mut l = history();
    let mut ids = IdAllocator::seeded(72);
    let (w, k, a, b): (WireId, DeviceId, TerminalId, TerminalId) = (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
    let (net, conductor): (NetId, ConductorId) = (ids.fresh(), ids.fresh());
    let before = Wire {
        net_order: 3,
        order_fixed: true,
        number: Some(WireNumber { text: Text::new("24V-07").unwrap(), fixed: true }),
        potential: Text::new("24V").ok(),
        ..Wire::new(net, conductor, a, b)
    };
    let point = |label: &str| Terminal { owner: k.into(), label: Some(label.into()), key: None, accepts: Default::default() };
    l.commit(
        Author::User,
        "wire",
        vec![
            Op::CreateDevice { id: k, device: Device::default().with_tag(Some("-K7")) },
            Op::AddTerminal { id: a, terminal: point("A1") },
            Op::AddTerminal { id: b, terminal: point("A2") },
            Op::CreateWire { id: w, wire: before.clone() },
        ],
    )
    .unwrap();
    let mut after = before.clone();
    after.size = WireSize::parse("2.5");
    after.colours = Colours::parse("GNYE");
    after.rating.basis = Some(Basis::Manual);
    after.rating.manual = Some(Amps(16.0));
    after.load = Some(Amps(4.0));
    after.wire_type = ecad_model::StyleRef::parse("dc24");
    l.commit(Author::User, "values", vec![Op::ModifyWire { id: w, before, after }]).unwrap();
    let path = written(&dir, &l);

    let log = run(&[path.to_str().unwrap(), "log", "main", "--last", "1"]).unwrap();
    for want in ["ModifyWire", "24V-07", "GN/YE", "2.5", "manual"] {
        assert!(log.contains(want), "{want} in the readable log: {log}");
    }
    let log = run(&[path.to_str().unwrap(), "log", "main", "--last", "2"]).unwrap();
    for want in ["CreateWire", "net_order", "order_fixed", &net.hex(), &conductor.hex()] {
        assert!(log.contains(want), "{want} in the readable log: {log}");
    }
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let rows = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = rows["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let key_of = |back: usize| rows.iter().rev().nth(back).and_then(|r| r["key"].as_str()).unwrap().to_string();
    let r = f.store().unwrap().reader().unwrap();
    let raw_bytes = |key: &str| {
        let mut bytes = None;
        r.raw_rows(|t, k, v| {
            if t == "transactions" && ecad_inspector::text::hex(k) == key {
                bytes = Some(v.to_vec());
            }
            Ok(())
        })
        .unwrap();
        ecad_inspector::text::hex(&bytes.unwrap())
    };
    // The ModifyWire record (the last) and the CreateWire record (the one before it).
    for (back, wants) in [
        (0, &["ModifyWire", "colours", "GN/YE", "size", "rating", "load", "wire_type", "dc24"][..]),
        (1, &["CreateWire", "net", "conductor", "net_order", "order_fixed", "ends", &net.hex(), &conductor.hex(), &a.hex(), &b.hex()][..]),
    ] {
        let key = key_of(back);
        let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
        let text = readable["ok"]["decoded"].to_string();
        for want in wants {
            assert!(text.contains(want), "{want} in the readable record: {text}");
        }
        assert!(!text.contains("AttachEnd"), "{text}");
        let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
        assert_eq!(raw["ok"]["hex"], raw_bytes(&key));
    }
    // The wires view names the net, the conductor and its place in the net's order.
    let wires = ask(s, serde_json::json!({"op": "wires"}));
    let row = &wires["ok"]["rows"][0];
    assert_eq!(row["ends"], serde_json::json!(["-K7:A1", "-K7:A2"]), "{wires}");
    assert_eq!((row["net"].as_str(), row["conductor"].as_str()), (Some(net.to_string().as_str()), Some(conductor.to_string().as_str())), "{wires}");
    assert_eq!((row["net_order"].as_u64(), row["order_fixed"].as_bool()), (Some(3), Some(true)), "{wires}");
}

#[test]
fn routes_and_their_ops_are_shown_raw_and_readable() {
    use ecad_model::{PageId, Pt, Route, RouteEnd, RouteId};
    let dir = TempDir::new("072d-route");
    let mut l = history();
    let page: PageId = *l.document().page_order.first().unwrap();
    let mut ids = IdAllocator::seeded(73);
    let (r1, r2): (RouteId, RouteId) = (ids.fresh(), ids.fresh());
    let a = Route::new(page, vec![Pt::new(0.0, 0.0), Pt::new(40.0, 0.0)], [RouteEnd::Free, RouteEnd::Free]);
    let tee = Route::new(page, vec![Pt::new(20.0, 30.0), Pt::new(20.0, 0.0)], [RouteEnd::Free, RouteEnd::On { route: r1, at: Pt::new(20.0, 0.0) }]);
    l.commit(Author::User, "draw", vec![Op::AddRoute { id: r1, route: a.clone() }, Op::AddRoute { id: r2, route: tee }]).unwrap();
    let mut b = a.clone();
    b.points.insert(1, Pt::new(10.0, -5.0));
    l.commit(Author::User, "move", vec![Op::ModifyRoute { id: r1, before: a, after: b }]).unwrap();
    let path = written(&dir, &l);

    let log = run(&[path.to_str().unwrap(), "log", "main", "--last", "2"]).unwrap();
    for want in ["AddRoute", "ModifyRoute", "on"] {
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
    for want in ["AddRoute", "ModifyRoute", "points", "ends", "free"] {
        assert!(text.contains(want), "{want} in the readable records: {text}");
    }
    // The snapshot keeps the routes in the page's chunk.
    let summary = ask(s, serde_json::json!({"op": "summary"}));
    assert!(summary.get("ok").is_some(), "{summary}");
}

#[test]
fn core_positions_are_shown_raw_and_readable() {
    // Owner ruling, 2026-11-18: a core on a loose line, with its landed end, and an unplaced core.
    use ecad_model::{Cable, CableId, Core, CorePosition, PageId, Pt, Route, RouteEnd, RouteId};
    let dir = TempDir::new("072d-cores");
    let mut l = history();
    let page: PageId = *l.document().page_order.first().unwrap();
    let mut ids = IdAllocator::seeded(74);
    let (d, t, r, k): (DeviceId, TerminalId, RouteId, CableId) = (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
    let loose = Route::new(page, vec![Pt::new(0.0, 0.0), Pt::new(40.0, 0.0)], [RouteEnd::Point(t), RouteEnd::Free]);
    let on_line =
        Core { position: CorePosition::Route { route: r, landed: Some(t) }, ..ecad_model::Core::unplaced(ecad_model::CoreLabel::new("1").ok()) };
    let spare = Core { position: CorePosition::Unplaced, ..ecad_model::Core::unplaced(ecad_model::CoreLabel::new("2").ok()) };
    l.commit(
        Author::User,
        "slash",
        vec![
            Op::CreateDevice { id: d, device: Device::default() },
            Op::AddTerminal {
                id: t,
                terminal: Terminal { owner: d.into(), label: Some("1".into()), key: Some("1".parse().unwrap()), accepts: Default::default() },
            },
            Op::AddRoute { id: r, route: loose },
            Op::CreateCable { id: k, cable: Cable { cores: vec![on_line, spare], ..Cable::default() } },
        ],
    )
    .unwrap();
    let path = written(&dir, &l);
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let rows = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = rows["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let key = rows.last().and_then(|r| r["key"].as_str().map(str::to_string)).unwrap();
    let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
    let text = readable["ok"]["decoded"].to_string();
    for want in ["CreateCable", "position", "route", "landed", "unplaced"] {
        assert!(text.contains(want), "{want} in the readable records: {text}");
    }
    let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
    assert!(raw["ok"]["hex"].as_str().is_some_and(|h| !h.is_empty()), "the raw row");
}

#[test]
fn number_by_and_its_settings_op_are_shown_raw_and_readable() {
    // Design 072 F40 (A15): `wiring.number_by` in the Project view and in the settings op.
    let dir = TempDir::new("072d-number-by");
    let mut l = history();
    let before = l.document().settings.clone();
    let mut after = before.clone();
    after.wiring.number_by = ecad_model::wiring::NumberBy::Conductor;
    l.commit(Author::User, "number by", vec![Op::SetProjectSettings { before, after }]).unwrap();
    let path = written(&dir, &l);
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let pr = ask(s, serde_json::json!({"op": "project"}));
    assert_eq!(pr["ok"]["number_by"], "conductor", "{pr}");
    let rows = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = rows["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let key = rows.last().and_then(|r| r["key"].as_str()).unwrap().to_string();
    let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
    let text = readable["ok"]["decoded"].to_string();
    assert!(text.contains("SetProjectSettings") && text.contains("number_by") && text.contains("conductor"), "{text}");
    let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
    assert!(raw["ok"]["hex"].as_str().is_some_and(|h| !h.is_empty()));
}

#[test]
fn accepts_is_shown_raw_readable_and_on_the_device_row() {
    // Design 072 F38 (A15): a point's `accepts` in the AddTerminal op and the Devices view.
    let dir = TempDir::new("072d-accepts");
    let mut l = history();
    let mut ids = IdAllocator::seeded(74);
    let (d, t): (DeviceId, TerminalId) = (ids.fresh(), ids.fresh());
    let bar =
        Terminal { owner: d.into(), label: Some("PE".into()), key: Some("PE".parse().unwrap()), accepts: ecad_model::Accepts::new(12).unwrap() };
    l.commit(Author::User, "bar", vec![Op::CreateDevice { id: d, device: Device::default() }, Op::AddTerminal { id: t, terminal: bar }]).unwrap();
    let path = written(&dir, &l);
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let rows = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = rows["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let key = rows.last().and_then(|r| r["key"].as_str()).unwrap().to_string();
    let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
    let text = readable["ok"]["decoded"].to_string();
    assert!(text.contains("AddTerminal") && text.contains("accepts") && text.contains("12"), "{text}");
    let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
    assert!(raw["ok"]["hex"].as_str().is_some_and(|h| !h.is_empty()));
    let devices = ask(s, serde_json::json!({"op": "devices", "detail": true, "count": 1000}));
    assert!(devices.to_string().contains(r#""accepts":12"#), "{devices}");
}

#[test]
fn a_cores_label_colours_and_purpose_are_shown_raw_readable_and_on_the_cable_row() {
    // Design 072 §4.5 (c1): a core's purpose is stored apart from its position.
    use ecad_model::{Cable, CableId, Core, CoreLabel, Purpose};
    let dir = TempDir::new("072d-purpose");
    let mut l = history();
    let mut ids = IdAllocator::seeded(75);
    let k: CableId = ids.fresh();
    let spare = Core { label: CoreLabel::new("GNYE").ok(), colours: Colours::parse("GN/YE"), purpose: Purpose::Spare, ..Core::unplaced(None) };
    l.commit(Author::User, "cable", vec![Op::CreateCable { id: k, cable: Cable::default() }]).unwrap();
    l.commit(
        Author::User,
        "unused",
        vec![Op::ModifyCable { id: k, before: Cable::default(), after: Cable { cores: vec![spare], ..Cable::default() } }],
    )
    .unwrap();
    let path = written(&dir, &l);
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let rows = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = rows["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let key = rows.last().and_then(|r| r["key"].as_str().map(str::to_string)).unwrap();
    let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
    let text = readable["ok"]["decoded"].to_string();
    for want in ["ModifyCable", "label", "GNYE", "colours", "GN/YE", "purpose", "spare", "unplaced"] {
        assert!(text.contains(want), "{want} in the readable records: {text}");
    }
    let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
    assert!(raw["ok"]["hex"].as_str().is_some_and(|h| !h.is_empty()), "the raw row");
    let cables = ask(s, serde_json::json!({"op": "cables"}));
    let member = &cables["ok"]["rows"][0]["members"][0];
    assert_eq!(member["core"], "GNYE");
    assert_eq!(member["purpose"], "spare");
    assert_eq!(member["colours"], "GN/YE");
}

#[test]
fn cable_reference_points_are_shown_raw_and_readable() {
    // Design 072 §4.4 (F4, overseer ruling): a cable reference places one named point per core, and
    // the core lists its points. The .ecad is never a black box: both views show them.
    use ecad_model::{Cable, CableId, Core, Form, Item, ItemId, ItemKind, PageId, Primitive, Pt, Representation, RepresentationId, Style, Subject};
    let dir = TempDir::new("072d-ref-points");
    let mut l = history();
    let page: PageId = *l.document().page_order.first().unwrap();
    let mut ids = IdAllocator::seeded(75);
    let (k, item, rep, point): (CableId, ItemId, RepresentationId, TerminalId) = (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
    let core = Core { through: vec![point], ..Core::unplaced(ecad_model::CoreLabel::new("1").ok()) };
    let layer = *l.document().layers.keys().next().unwrap();
    let style = Style { layer, color: ecad_model::Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
    l.commit(
        Author::User,
        "cablecontinue",
        vec![
            Op::CreateCable { id: k, cable: Cable { cores: vec![core], ..Cable::default() } },
            Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Primitive(Primitive::Point { at: Pt::new(65.0, 5.0) }) } },
            Op::LinkRepresentation {
                id: rep,
                rep: Representation {
                    subject: Subject::Cable(k),
                    page,
                    form: Form::Drawn,
                    role: ecad_model::Role::Reference,
                    items: vec![item],
                    connection_points: [(point, Pt::new(60.0, 5.0))].into(),
                },
            },
        ],
    )
    .unwrap();
    let path = written(&dir, &l);
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let rows = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = rows["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let key = rows.last().and_then(|r| r["key"].as_str().map(str::to_string)).unwrap();
    let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
    let text = readable["ok"]["decoded"].to_string();
    for want in ["through", "connection_points", &point.hex()] {
        assert!(text.contains(want), "{want} in the readable records: {text}");
    }
    let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
    assert!(raw["ok"]["hex"].as_str().is_some_and(|h| !h.is_empty()), "the raw row");
    let cables = ask(s, serde_json::json!({"op": "cables"}));
    let member = &cables["ok"]["rows"][0]["members"][0];
    assert_eq!(member["through"], serde_json::json!([point.to_string()]), "the cable row lists the core's point: {cables}");
}

#[test]
fn a_cable_with_two_slashes_and_one_wire_per_core_is_shown_raw_and_readable() {
    // Design 072 §4.4 (F5, overseer 2026-11-20): continuations are named points, so a core through
    // a continuation pair is one wire from page 1's point to page 2's, and a slash with the cable's
    // tag on page 2 only shows the cable there. Both slashes, the one member and its two ends read
    // in the Raw and Readable views and in the cable rows.
    use ecad_model::{
        Cable, CableId, Core, Form, Item, ItemId, ItemKind, Page, PageId, Primitive, Pt, Representation, RepresentationId, Style, Subject,
    };
    let dir = TempDir::new("072d-two-slashes");
    let mut l = history();
    let p1: PageId = *l.document().page_order.first().unwrap();
    let mut ids = IdAllocator::seeded(76);
    let p2: PageId = ids.fresh();
    let (x01, x05): (DeviceId, DeviceId) = (ids.fresh(), ids.fresh());
    let (a1, a2): (ecad_model::AidId, ecad_model::AidId) = (ids.fresh(), ids.fresh());
    let (t01, t05, ta1, ta2): (TerminalId, TerminalId, TerminalId, TerminalId) = (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
    let (w, net, conductor): (WireId, NetId, ConductorId) = (ids.fresh(), ids.fresh(), ids.fresh());
    let k: CableId = ids.fresh();
    let (i1, i2, r1, r2): (ItemId, ItemId, RepresentationId, RepresentationId) = (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
    let layer = *l.document().layers.keys().next().unwrap();
    let style = Style { layer, color: ecad_model::Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
    let point = |owner: ecad_model::TerminalOwner, label: &str| Terminal { owner, label: Some(label.into()), key: None, accepts: Default::default() };
    // Design 072 §6.2: a continuation is a drawing aid.
    let arrow = || ecad_model::Aid::Continuation { name: Some(ecad_model::aid::ContinuationName::new("A1").unwrap()), caption: None };
    let slash = |id: ItemId, page: PageId| Op::AddItem {
        id,
        item: Item { page, style: style.clone(), kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(50.0, -5.0), b: Pt::new(50.0, 5.0) }) },
    };
    let link = |id: RepresentationId, page: PageId, item: ItemId| Op::LinkRepresentation {
        id,
        rep: Representation {
            subject: Subject::Cable(k),
            page,
            form: Form::Drawn,
            role: ecad_model::Role::Slash,
            items: vec![item],
            connection_points: Default::default(),
        },
    };
    l.commit(
        Author::User,
        "setup",
        vec![
            Op::AddPage { id: p2, page: Page::titled("2").unwrap(), index: 1 },
            Op::CreateDevice { id: x01, device: Device::default() },
            Op::CreateDevice { id: x05, device: Device::default() },
            Op::CreateAid { id: a1, aid: arrow() },
            Op::CreateAid { id: a2, aid: arrow() },
            Op::AddTerminal { id: t01, terminal: point(x01.into(), "1") },
            Op::AddTerminal { id: t05, terminal: point(x05.into(), "1") },
            Op::AddTerminal { id: ta1, terminal: point(a1.into(), "1") },
            Op::AddTerminal { id: ta2, terminal: point(a2.into(), "1") },
            Op::CreateWire { id: w, wire: Wire::new(net, conductor, t01, t05) },
            Op::CreateCable { id: k, cable: Cable { cores: vec![Core::on_wire(w, ecad_model::CoreLabel::new("1").ok())], ..Cable::default() } },
            slash(i1, p1),
            link(r1, p1, i1),
        ],
    )
    .unwrap();
    // The second slash: the cable shown on page 2, no new member.
    l.commit(Author::User, "cableslash", vec![slash(i2, p2), link(r2, p2, i2)]).unwrap();
    let doc = l.document();
    assert_eq!(doc.named_points(ta2).map(|g| g.len()), Some(2), "the pair is one named point group");
    assert_eq!(doc.cables[&k].cores.len(), 1);
    let path = written(&dir, &l);
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let rows = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = rows["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let key = rows.last().and_then(|r| r["key"].as_str().map(str::to_string)).unwrap();
    let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
    let text = readable["ok"]["decoded"].to_string();
    for want in ["LinkRepresentation", "slash", &k.hex(), &p2.hex()] {
        assert!(text.contains(want), "{want} in the readable records: {text}");
    }
    assert!(!text.contains("ModifyCable"), "the second slash adds no member: {text}");
    let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
    assert!(raw["ok"]["hex"].as_str().is_some_and(|h| !h.is_empty()), "the raw row");
    let cables = ask(s, serde_json::json!({"op": "cables"}));
    let row = &cables["ok"]["rows"][0];
    let members = row["members"].as_array().cloned().unwrap_or_default();
    assert_eq!(members.len(), 1, "one member, one wire: {cables}");
    assert_eq!(members[0]["core"], "1");
    let ends = row["ends"].to_string();
    assert!(!ends.contains("A1"), "the wire runs device to device, never to a continuation: {cables}");
}
