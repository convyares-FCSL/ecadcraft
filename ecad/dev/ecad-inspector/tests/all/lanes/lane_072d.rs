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
    let point = |label: &str| Terminal { component: k, label: Some(label.into()), key: None };
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
        (0, &["ModifyWire", "colours", "GN/YE", "size", "rating", "load"][..]),
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
