//! Lane 072 C (design 072 §3; acceptance A9, A11, A19): PLC stations with a CPU or coupler head,
//! modules with a typed function and attributes, and their channels, stored and edited as fields.

use ecad_model::objects::plc::{AddressStyle, Communication, ModuleFunction, NetAddress, PlcModule, PlcRack, Protocol, RackHead, Signal, SystemKind};
use ecad_model::{DeviceId, DeviceType, Document};
use serde_json::json;

use crate::common::{H, id};

fn placed(h: &mut H, symbol: &str, at: [f64; 2]) -> DeviceId {
    let v = h.run("place", json!({"symbol": symbol, "at": at})).unwrap_or_else(|e| panic!("{symbol}: {e}"));
    id::<DeviceId>(&v, "component")
}

fn station(doc: &Document, c: DeviceId) -> &PlcRack {
    match &doc.devices[&c].device_type {
        DeviceType::PlcRack(r) => r,
        other => panic!("not a station: {other:?}"),
    }
}

fn module(doc: &Document, c: DeviceId) -> &PlcModule {
    match &doc.devices[&c].device_type {
        DeviceType::PlcModule(m) => m,
        other => panic!("not a module: {other:?}"),
    }
}

/// A11: a CPU card is a station with a CPU head, a head module a station with a coupler; neither
/// is a module.
#[test]
fn cpu_and_head_module_cards_are_stations() {
    let mut h = H::new(0x72_c1);
    let cpu = placed(&mut h, "plccpu", [40.0, 40.0]);
    let im = placed(&mut h, "plcim", [140.0, 40.0]);
    let doc = h.doc();
    assert!(station(&doc, cpu).head.is_cpu());
    assert_eq!(station(&doc, im).head, RackHead::Coupler { protocol: Protocol::Profinet });
    assert_eq!(ecad_electrical::plc::rack::racks(&doc).len(), 2);
}

/// A19: each of today's PLC starters gives its module function (design 072 §3 mapping), with no
/// attribute set.
#[test]
fn each_plc_starter_gives_its_function() {
    let mut h = H::new(0x72_c2);
    placed(&mut h, "plccpu", [20.0, 20.0]);
    let cases: [(&str, fn(&ModuleFunction) -> bool); 9] = [
        ("plcdi", |f| matches!(f, ModuleFunction::DigitalIn(g) if g.signal == Signal::Dc24v && g.count == 16)),
        ("plcdq", |f| matches!(f, ModuleFunction::DigitalOut(g) if g.signal == Signal::Dc24v && g.count == 16)),
        ("plcai", |f| matches!(f, ModuleFunction::AnalogIn(g) if g.signal == Signal::Current4To20 && g.count == 4)),
        ("plcao", |f| matches!(f, ModuleFunction::AnalogOut(g) if g.count == 4)),
        ("plcrtd", |f| matches!(f, ModuleFunction::AnalogIn(g) if g.signal == Signal::Rtd)),
        (
            "plciol",
            |f| matches!(f, ModuleFunction::Communication(Communication::IoLinkMaster(m)) if m.channels.signal == Signal::IoLink && m.channels.count == 4),
        ),
        ("plcps", |f| matches!(f, ModuleFunction::System(s) if s.kind == SystemKind::PowerSupply)),
        ("plcpm", |f| matches!(f, ModuleFunction::System(s) if s.kind == SystemKind::PowerFeed)),
        ("plccm", |f| matches!(f, ModuleFunction::Communication(Communication::Link(_)))),
    ];
    for (n, (symbol, ok)) in cases.iter().enumerate() {
        let c = placed(&mut h, symbol, [60.0 + 40.0 * n as f64, 80.0]);
        let doc = h.doc();
        let m = module(&doc, c);
        assert!(ok(&m.function), "{symbol}: {:?}", m.function);
        assert!(m.attributes.is_none(), "{symbol}: no attribute");
        assert!(m.rack.is_some(), "{symbol}: placed in the only station");
    }
}

/// A new card's channel count is the project's for its type when it is made, and its own after.
#[test]
fn a_new_card_takes_the_project_channel_count() {
    let mut h = H::new(0x72_c3);
    h.run("setdefault", json!({"key": "plc.di.channels", "value": "8"})).unwrap();
    let c = placed(&mut h, "plcdi", [40.0, 40.0]);
    assert_eq!(module(&h.doc(), c).channel_count(), 8);
    h.run("setdefault", json!({"key": "plc.di.channels", "value": "32"})).unwrap();
    assert_eq!(module(&h.doc(), c).channel_count(), 8, "a made card keeps its own count");
}

/// A9: `setprop` on a module's and a station's keys changes the typed fields; a value that does not
/// read is refused and changes nothing.
#[test]
fn setprop_writes_the_fields() {
    let mut h = H::new(0x72_c4);
    let cpu = placed(&mut h, "plccpu", [40.0, 40.0]);
    let di = placed(&mut h, "plcdi", [140.0, 40.0]);
    let set = |h: &mut H, c: DeviceId, key: &str, value: &str| h.run("setprop", json!({"component": c.hex(), "key": key, "value": value}));
    set(&mut h, di, "plc.ch.3.text", "Pump running").unwrap();
    set(&mut h, di, "plc.ch.3.tag", "Pump_Run").unwrap();
    set(&mut h, di, "plc.safety_rated", "yes").unwrap();
    set(&mut h, di, "plc.ex_i", "yes").unwrap();
    set(&mut h, di, "plc.channels", "8").unwrap();
    set(&mut h, di, "plc.signal", "relay").unwrap();
    set(&mut h, di, "plc.addr.start", "10").unwrap();
    set(&mut h, cpu, "plc.network", "Line 1").unwrap();
    set(&mut h, cpu, "plc.network.address", "192.168.0.10").unwrap();
    set(&mut h, cpu, "plc.cpu.family", "S7-1500").unwrap();
    set(&mut h, cpu, "plc.address.style", "iec").unwrap();
    let doc = h.doc();
    let m = module(&doc, di);
    let ch = m.channel(3).unwrap();
    assert_eq!(ch.text.as_ref().map(|t| t.as_str()), Some("Pump running"));
    assert_eq!(ch.tag.as_ref().map(|t| t.as_str()), Some("Pump_Run"));
    assert!(m.attributes.safety_rated && m.attributes.intrinsically_safe && !m.attributes.calibration_certificate);
    assert_eq!(m.channel_count(), 8);
    assert_eq!(m.signal_of(0), Some(Signal::Relay));
    assert_eq!(m.address.start, Some(10));
    assert_eq!(m.rack, Some(cpu));
    let s = station(&doc, cpu);
    assert_eq!(s.network.as_ref().map(|t| t.as_str()), Some("Line 1"));
    assert_eq!(s.address, NetAddress::parse("192.168.0.10"));
    assert_eq!(s.head, RackHead::Cpu { family: ecad_model::naming::text::Text::new("S7-1500").ok() });
    assert_eq!(s.address_style, Some(AddressStyle::Iec));
    // The bag holds none of them.
    assert!(
        !doc.devices[&di].properties.keys().any(|k| k.starts_with("plc.ch.") || k == "plc.rack" || k == "plc.slot"),
        "{:?}",
        doc.devices[&di].properties
    );

    let head = h.head();
    for (c, key, value) in [
        (cpu, "plc.network.address", "300.1.1.1"),
        (cpu, "plc.protocol", "ethercat"),
        (di, "plc.ch.1.text", "bad\u{202e}text"),
        (di, "plc.signal", "volts"),
        (di, "plc.safety_rated", "maybe"),
        (di, "plc.protocol", "profinet"),
    ] {
        assert!(set(&mut h, c, key, value).is_err(), "{key} = {value} is refused");
    }
    assert_eq!(h.head(), head, "a refused value changes nothing");
    // Clearing a channel's data drops the channel from the stored map.
    set(&mut h, di, "plc.ch.3.text", "").unwrap();
    set(&mut h, di, "plc.ch.3.tag", "").unwrap();
    assert!(module(&h.doc(), di).channels().is_empty());
}

/// Deleting a station takes its modules out of their slots; the slot command moves a module.
#[test]
fn deleting_a_station_releases_its_modules() {
    let mut h = H::new(0x72_c5);
    let cpu = placed(&mut h, "plccpu", [40.0, 40.0]);
    let di = placed(&mut h, "plcdi", [140.0, 40.0]);
    assert_eq!(module(&h.doc(), di).rack, Some(cpu));
    h.run("plcslot", json!({"card": di.hex(), "slot": "5"})).unwrap();
    assert_eq!(module(&h.doc(), di).slot, Some(5));
    h.run("devicedelete", json!({"component": cpu.hex()})).unwrap();
    let m = module(&h.doc(), di).clone();
    assert_eq!((m.rack, m.slot), (None, None));
}

/// A communication module shows in the station overview with its protocol.
#[test]
fn a_communication_module_shows_in_the_overview() {
    let mut h = H::new(0x72_c6);
    placed(&mut h, "plccpu", [40.0, 40.0]);
    let cm = placed(&mut h, "plccm", [140.0, 40.0]);
    h.run("setprop", json!({"component": cm.hex(), "key": "plc.protocol", "value": "modbus_tcp"})).unwrap();
    let v = h.run("plcracks", json!({})).unwrap().to_string();
    assert!(v.contains("Modbus TCP"), "{v}");
}

/// A card type change keeps the module's channel data and slot and changes its function.
#[test]
fn a_card_type_change_keeps_channel_data() {
    let mut h = H::new(0x72_c7);
    placed(&mut h, "plccpu", [40.0, 40.0]);
    let di = placed(&mut h, "plcdi", [140.0, 40.0]);
    h.run("setprop", json!({"component": di.hex(), "key": "plc.ch.1.text", "value": "Spare lamp"})).unwrap();
    let slot = module(&h.doc(), di).slot;
    h.run("plccardtype", json!({"card": di.hex(), "type": "dq"})).unwrap();
    let doc = h.doc();
    let m = module(&doc, di);
    assert!(matches!(m.function, ModuleFunction::DigitalOut(_)), "{:?}", m.function);
    assert_eq!(m.slot, slot);
    assert_eq!(m.channel(1).and_then(|c| c.text.as_ref()).map(|t| t.as_str()), Some("Spare lamp"));
}

/// A10: a module's address block is its own method and agrees with the kept model the Checks and
/// lists read; a station's modules and I/O count come from the index.
#[test]
fn address_blocks_and_station_reads_are_methods() {
    use ecad_electrical::plc::address::{Addressed, Station};
    let mut h = H::new(0x72_c8);
    let cpu = placed(&mut h, "plccpu", [40.0, 40.0]);
    let di = placed(&mut h, "plcdi", [140.0, 40.0]);
    let dq = placed(&mut h, "plcdq", [180.0, 40.0]);
    let ai = placed(&mut h, "plcai", [220.0, 40.0]);
    let doc = h.doc();
    let kept = ecad_electrical::reports::kept::ReportModels::build_with(&doc, &ecad_engine::styles::draw::EngineMetrics);
    for c in [di, dq, ai] {
        assert_eq!(module(&doc, c).address_block(&doc, c), kept.plc.blocks.get(&c).copied(), "{c:?}");
    }
    assert!(module(&doc, di).address_block(&doc, di).is_some_and(|b| b.input.is_some() && b.output.is_none()));
    let s = station(&doc, cpu);
    let mods: Vec<DeviceId> = s.modules(&doc, cpu).into_iter().map(|(c, _)| c).collect();
    assert_eq!(mods, [di, dq, ai], "by slot");
    assert_eq!(s.io_count(&doc, cpu), (20, 16), "16 DI + 4 AI in, 16 DQ out");
}
