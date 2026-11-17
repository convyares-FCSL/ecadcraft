//! Lane 072 C (design 072 §3; acceptance A2, A3, A4): PLC stations and modules round-trip, read
//! strictly, and the keys they replaced are reserved.

use ecad_model::Word;
use ecad_model::keys::is_reserved;
use ecad_model::plc::{
    AddressStyle, Channel, ChannelGroup, CommRole, Communication, ModuleFunction, NetAddress, PlcModule, PlcRack, Protocol, RackHead, SafeState,
    Signal,
};
use ecad_model::text::Text;
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
    round_trip(&DeviceType::PlcModule(PlcModule::new(ModuleFunction::Communication(Communication {
        protocol: Protocol::IoLink,
        role: CommRole::Master,
        ports: Vec::new(),
        channels: Some(ChannelGroup::new(Signal::IoLink, 4)),
    }))));
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
    let mut none = PlcModule::new(ModuleFunction::Drive(ecad_model::plc::Drive { kind: ecad_model::plc::DriveKind::Servo, axes: 2 }));
    assert!(!none.edit_channel(0, |c| c.text = Text::new("x").ok()));
}

/// A3: the property keys the fields replaced are reserved.
#[test]
fn the_replaced_keys_are_reserved() {
    for k in ["plc.rack", "plc.slot", "plc.channels", "plc.addr.start", "plc.addr.size", "plc.kind", "plc.ch.3.text", "plc.ch.0.tag"] {
        assert!(is_reserved(k), "{k}");
    }
    assert!(!is_reserved("plc.slots"), "a registry entry stays a value");
}
