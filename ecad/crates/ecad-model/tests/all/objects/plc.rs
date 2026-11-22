//! Lane 072 C (design 072 §3; acceptance A2, A3, A4): PLC stations and modules round-trip, read
//! strictly, and the keys they replaced are reserved.

use ecad_model::Word;
use ecad_model::naming::keys::{Holder, is_reserved_on};
use ecad_model::objects::plc::{
    AddressStyle, Channel, ChannelGroup, CommLink, CommRole, Communication, IoLinkMaster, ModuleFunction, NetAddress, PlcModule, PlcRack, Protocol,
    RackHead, SafeState, Signal,
};
use ecad_model::naming::text::Text;
use ecad_model::{DeviceId, DeviceType};

fn round_trip(t: &DeviceType) {
    let text = serde_json::to_string(t).unwrap();
    let back: DeviceType = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{text}: {e}"));
    assert_eq!(&back, t, "{text}");
}

/// A2: stations, modules of every function, channels and attributes round-trip.
#[test]
fn stations_and_modules_round_trip() {
    round_trip(&DeviceType::PlcRack(PlcRack::default()));
    round_trip(&DeviceType::PlcRack(PlcRack {
        network: Text::new("Line 1").ok(),
        address: NetAddress::parse("10.0.0.7"),
        head: RackHead::Coupler { protocol: Protocol::Ethercat },
        address_style: Some(AddressStyle::ByteBit),
    }));
    let mut g = ChannelGroup::new(Signal::Dc24v, 16);
    g.channels.insert(
        3,
        Channel { text: Text::new("Pump").ok(), address: Text::new("I1.3").ok(), tag: Text::new("Pump_Run").ok(), safe: Some(SafeState::Off) },
    );
    let mut m = PlcModule::new(ModuleFunction::Mixed(vec![g.clone(), ChannelGroup::new(Signal::Voltage0To10, 2)]));
    m.rack = Some(DeviceId::from(7u128));
    m.slot = Some(4);
    m.attributes.safety_rated = true;
    m.attributes.calibration_certificate = true;
    m.address.start = Some(12);
    round_trip(&DeviceType::PlcModule(m));
    round_trip(&DeviceType::PlcModule(PlcModule::new(ModuleFunction::Communication(Communication::IoLinkMaster(IoLinkMaster {
        channels: ChannelGroup::new(Signal::IoLink, 4),
    })))));
    round_trip(&DeviceType::PlcModule(PlcModule::new(ModuleFunction::Communication(Communication::Link(CommLink {
        protocol: Protocol::ModbusTcp,
        role: CommRole::Gateway,
        ports: vec![ecad_model::objects::plc::Port { name: Text::new("X1").ok() }],
    })))));
    for w in Signal::WORDS.iter().chain(Protocol::WORDS).chain(AddressStyle::WORDS) {
        assert!(!w.is_empty());
    }
    assert_eq!(AddressStyle::ByteBit.word(), "byte.bit");
}

/// A4: an unknown word or field, an invalid address or a hidden character is a read error.
#[test]
fn reading_is_strict() {
    for bad in [
        r#"{"plc_rack":{"head":{"cpu":{}},"colour":"red"}}"#,
        r#"{"plc_rack":{"head":{"gateway":{}}}}"#,
        r#"{"plc_rack":{"head":{"coupler":{"protocol":"fieldbus9"}}}}"#,
        r#"{"plc_rack":{"head":{"cpu":{}},"address":"300.1.1.1"}}"#,
        r#"{"plc_rack":{"head":{"cpu":{}},"address":"03"}}"#,
        r#"{"plc_module":{"function":{"digital_in":{"signal":"volts","count":4}}}}"#,
        r#"{"plc_module":{"function":{"teleport":{}}}}"#,
        // An IO-Link master always has its channels; a link never has any.
        r#"{"plc_module":{"function":{"communication":{"io_link_master":{}}}}}"#,
        r#"{"plc_module":{"function":{"communication":{"link":{"protocol":"profinet","role":"gateway","channels":{"signal":"io_link","count":4}}}}}}"#,
        r#"{"plc_module":{"function":{"digital_in":{"signal":"dc24v","count":257}}}}"#,
        r#"{"plc_module":{"slot":1000,"function":{"digital_in":{"signal":"dc24v","count":4}}}}"#,
        r#"{"plc_module":{"function":{"digital_in":{"signal":"dc24v","count":4}},"address":{"size":0}}}"#,
        r#"{"plc_module":{"function":{"digital_in":{"signal":"dc24v","count":4,"channels":{"1":{"text":"a\u202eb"}}}}}}"#,
        r#"{"plc_module":{"function":{"digital_in":{"signal":"dc24v","count":4}},"attributes":{"rugged":true}}}"#,
    ] {
        assert!(serde_json::from_str::<DeviceType>(bad).is_err(), "{bad} is refused");
    }
}

/// Channels are numbered across a mixed module's groups; one past the last count is the last
/// group's; a cleared channel is dropped.
#[test]
fn channels_number_across_groups() {
    let mut m = PlcModule::new(ModuleFunction::Mixed(vec![ChannelGroup::new(Signal::Dc24v, 4), ChannelGroup::new(Signal::Relay, 2)]));
    assert_eq!(m.channel_count(), 6);
    assert_eq!(m.signal_of(3), Some(Signal::Dc24v));
    assert_eq!(m.signal_of(4), Some(Signal::Relay));
    assert_eq!(m.signal_of(9), Some(Signal::Relay));
    assert!(m.edit_channel(5, |c| c.text = Text::new("Horn").ok()));
    assert_eq!(m.channels().into_iter().map(|(i, _)| i).collect::<Vec<_>>(), [5]);
    assert!(m.edit_channel(5, |c| c.text = None));
    assert!(m.channels().is_empty());
    let mut none = PlcModule::new(ModuleFunction::Drive(ecad_model::objects::plc::Drive { kind: ecad_model::objects::plc::DriveKind::Servo, axes: 2 }));
    assert!(!none.edit_channel(0, |c| c.text = Text::new("x").ok()));
}

/// A3: the property keys the fields replaced are reserved.
#[test]
fn the_replaced_keys_are_reserved() {
    for k in ["plc.rack", "plc.slot", "plc.channels", "plc.addr.start", "plc.addr.size", "plc.kind", "plc.ch.3.text", "plc.ch.0.tag"] {
        assert!(is_reserved_on(Holder::Device, k), "{k}");
    }
    assert!(!is_reserved_on(Holder::Device, "plc.slots"), "a registry entry stays a value");
}

/// A station's network address: an IPv4 address in its one written form, or a node number.
#[test]
fn network_addresses_read_strictly() {
    assert_eq!(NetAddress::parse("192.168.0.10").map(|a| a.to_string()).as_deref(), Some("192.168.0.10"));
    assert_eq!(NetAddress::parse("7"), Some(NetAddress::Node(7)));
    for bad in ["300.1.1.1", "1.2.3", "1.2.3.4.5", "01.2.3.4", "65536", "x"] {
        assert_eq!(NetAddress::parse(bad), None, "{bad}");
    }
}

/// Design 072 §3, design 045 FP2 withdrawn (no compatibility before v1): a PLC module's point is
/// a channel's (`.0.in`) or its own symbol's. An old `I1.3` channel key is unknown: a commit with
/// it is refused, and a history holding it stops there when read (the project opens read-only at
/// the last good transaction). Nothing reads a channel out of its text.
#[test]
fn an_old_channel_key_on_a_module_is_a_read_error() {
    use ecad_model::{Author, Device, Ledger, Op, Terminal, ConnectionPointId, Transaction, TxKind};
    let mut l = Ledger::new(0x0007_2cf2);
    let c: DeviceId = DeviceId::from(0x072c_0001u128);
    let good: ConnectionPointId = ConnectionPointId::from(0x072c_0002u128);
    let module = Device {
        device_type: DeviceType::PlcModule(PlcModule::new(ModuleFunction::DigitalIn(ChannelGroup::new(Signal::Dc24v, 16)))),
        ..Device::default()
    };
    let point = |k: &str| Terminal { owner: c.into(), label: Some("1".into()), key: Some(k.parse().unwrap()), accepts: Default::default() };
    l.commit(Author::User, "card", vec![Op::CreateDevice { id: c, device: module }, Op::AddConnectionPoint { id: good, terminal: point(".0.in") }]).unwrap();
    for old in ["I1.3", "Q0.0", "L+"] {
        let t: ConnectionPointId = ConnectionPointId::from(0x072c_0100u128);
        let e = l.commit(Author::User, "old key", vec![Op::AddConnectionPoint { id: t, terminal: point(old) }]).unwrap_err().to_string();
        assert!(e.contains(&format!("keyed {old}")), "{old}: {e}");
    }
    assert_eq!(l.head(), 1, "nothing was committed");
    // A history written elsewhere with the old key: read up to the transaction before it.
    let mut log = l.log().to_vec();
    let bad = Transaction {
        id: ecad_model::TxId(log[0].id.0 + 1),
        seq: 2,
        author: Author::User,
        label: "old key".into(),
        kind: TxKind::Edit,
        time_ms: None,
        ops: vec![Op::AddConnectionPoint { id: ConnectionPointId::from(0x072c_0200u128), terminal: point("I1.3") }],
    };
    log.push(bad);
    let (read, failure) = Ledger::replay_prefix(log, 0x0007_2cf2);
    assert_eq!(read.head(), 1, "the last good transaction");
    let (at, e) = failure.expect("the old key is a read error");
    assert_eq!(at, 1);
    assert!(e.to_string().contains("keyed I1.3"), "{e}");
}
