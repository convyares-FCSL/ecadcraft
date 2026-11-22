//! PLC stations, modules and channels (design 072 §3): the data of the device groups
//! [`crate::DeviceType::PlcRack`] and [`crate::DeviceType::PlcModule`].
//!
//! # Semantics
//! - A **station** ([`PlcRack`]) is on a network with an address; its head is a typed choice, a
//!   CPU or a coupler ([`RackHead`]). Usually one station has the CPU; the others have couplers.
//! - A **module** ([`PlcModule`]) names its station by ID (`rack`), its slot, and what it does
//!   ([`ModuleFunction`], an enum with data in generic names, never a vendor's). Safety-rated,
//!   intrinsically safe (Ex i) and a calibration certificate are [`ModuleAttributes`], never
//!   functions: a safe digital input is still [`ModuleFunction::DigitalIn`].
//! - A module's **channels** live in its channel groups ([`ChannelGroup`]): the signal they carry,
//!   how many, and each channel's own text, typed address, PLC tag and safe state by index. A
//!   module with several groups ([`ModuleFunction::Mixed`]) numbers its channels across them in
//!   order ([`PlcModule::channel`]).
//! - Everything is strict on read: an unknown word or field, or invalid text, is a read error.
//!
//! This module holds the stored data and the reads every caller shares. Address blocks, the I/O
//! list, the station overview and the Checks are in `ecad-electrical`'s `plc` module.
#![deny(clippy::wildcard_enum_match_arm)]

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::DeviceId;
use crate::naming::text::Text;
use crate::geometry::units::RatedVoltage;

/// The highest slot number.
pub const MAX_SLOT: u16 = 999;
/// The most channels a group has.
pub const MAX_CHANNELS: u16 = 256;
/// The highest bus node number of a station's network address.
pub const MAX_NODE: u16 = 65_535;
/// The highest byte of a pinned address start.
pub const MAX_BYTE: u32 = 65_535;
/// The largest pinned block size, in bytes.
pub const MAX_SIZE: u32 = 4_096;

/// A channel description.
pub type ChannelText = Text<80>;
/// A channel's typed address.
pub type ChannelAddress = Text<32>;
/// A channel's PLC tag (its symbol name in the PLC program).
pub type PlcTag = Text<128>;

// ------------------------------------------------------------------------------------- station

/// A PLC station (device group 10): on a network, with an address there, and a CPU or a coupler
/// at its head (design 072 §3, F19′).
///
/// # Example
/// ```
/// use crate::objects::plc::{PlcRack, RackHead, Protocol};
/// let s = PlcRack { head: RackHead::Coupler { protocol: Protocol::Ethercat }, ..PlcRack::default() };
/// assert_eq!(serde_json::to_string(&s).unwrap(), r#"{"head":{"coupler":{"protocol":"ethercat"}}}"#);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlcRack {
    /// The network it is on (`Line 1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<Text<32>>,
    /// Its address there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<NetAddress>,
    pub head: RackHead,
    /// How its modules' channels are addressed; `None` follows the project's
    /// `plc.address.style`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address_style: Option<AddressStyle>,
}

/// What heads a station.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RackHead {
    /// A CPU (usually one per project), with its family (`S7-1500`, written by the person).
    Cpu {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        family: Option<Text<32>>,
    },
    /// A fieldbus coupler.
    Coupler { protocol: Protocol },
}

impl Default for RackHead {
    fn default() -> Self {
        RackHead::Cpu { family: None }
    }
}

impl RackHead {
    /// Whether the head is a CPU.
    pub fn is_cpu(&self) -> bool {
        match self {
            RackHead::Cpu { .. } => true,
            RackHead::Coupler { .. } => false,
        }
    }
}

/// A station's address on its network: an IPv4 address or a bus node number.
///
/// # Semantics
/// Written as text: four dotted numbers are an IP address (`192.168.0.10`), a whole number a node
/// (`3`). Anything else is refused on input and a read error in a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NetAddress {
    /// An IPv4 address (`core::net`: plain data, no system access).
    Ip(core::net::Ipv4Addr),
    Node(u16),
}

impl NetAddress {
    /// The address `text` names, if it is one.
    ///
    /// # Example
    /// ```
    /// use crate::objects::plc::NetAddress;
    /// assert_eq!(NetAddress::parse("3"), Some(NetAddress::Node(3)));
    /// assert!(matches!(NetAddress::parse("192.168.0.10"), Some(NetAddress::Ip(_))));
    /// assert_eq!(NetAddress::parse("3a"), None);
    /// ```
    pub fn parse(text: &str) -> Option<NetAddress> {
        let t = text.trim();
        if t.contains('.') {
            return t.parse::<core::net::Ipv4Addr>().ok().map(NetAddress::Ip);
        }
        crate::naming::words::uint(t, u32::from(MAX_NODE)).filter(|_| t.len() <= 5).and_then(|n| u16::try_from(n).ok()).map(NetAddress::Node)
    }
}

impl fmt::Display for NetAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetAddress::Ip(a) => write!(f, "{a}"),
            NetAddress::Node(n) => write!(f, "{n}"),
        }
    }
}

impl Serialize for NetAddress {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for NetAddress {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        match NetAddress::parse(&text) {
            Some(a) if a.to_string() == text => Ok(a),
            _ => Err(serde::de::Error::custom(format!("`{}` is not a network address (an IP address or a node number)", text.escape_debug()))),
        }
    }
}

crate::word_enum! {
    /// A network or link protocol: a coupler's fieldbus, a communication module's protocol.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Protocol {
        Ethercat,
        Profinet,
        EthernetIp,
        ModbusTcp,
        ModbusRtu,
        Profibus,
        Canopen,
        Devicenet,
        CcLink,
        Powerlink,
        AsInterface,
        Serial,
        Other,
    }
}

impl Protocol {
    /// As a person reads it.
    pub fn label(self) -> &'static str {
        match self {
            Protocol::Ethercat => "EtherCAT",
            Protocol::Profinet => "PROFINET",
            Protocol::EthernetIp => "EtherNet/IP",
            Protocol::ModbusTcp => "Modbus TCP",
            Protocol::ModbusRtu => "Modbus RTU",
            Protocol::Profibus => "PROFIBUS",
            Protocol::Canopen => "CANopen",
            Protocol::Devicenet => "DeviceNet",
            Protocol::CcLink => "CC-Link",
            Protocol::Powerlink => "POWERLINK",
            Protocol::AsInterface => "AS-Interface",
            Protocol::Serial => "Serial",
            Protocol::Other => "Other",
        }
    }
}

/// A coupler's fieldbus.
pub type Fieldbus = Protocol;

crate::word_enum! {
    /// How a station's channels are addressed (design 045 §5.2): `I0.0` (`byte.bit`), `%IX0.0`
    /// (`iec`), `%I0.0` (`percent`), by slot (`slot`), or not at all (`none`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum AddressStyle {
        #[strum(serialize = "byte.bit")]
        ByteBit,
        Iec,
        Percent,
        Slot,
        None,
    }
}

// -------------------------------------------------------------------------------------- module

/// A module in a PLC station (device group 11).
///
/// # Example
/// ```
/// use crate::objects::plc::{ChannelGroup, ModuleFunction, PlcModule, Signal};
/// let m = PlcModule::new(ModuleFunction::DigitalIn(ChannelGroup::new(Signal::Dc24v, 16)));
/// assert_eq!(m.channel_count(), 16);
/// assert_eq!(serde_json::to_string(&m).unwrap(), r#"{"function":{"digital_in":{"signal":"dc24v","count":16}}}"#);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlcModule {
    /// Its station.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rack: Option<DeviceId>,
    /// Its slot there, 0–[`MAX_SLOT`].
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "slot_in_range")]
    pub slot: Option<u16>,
    pub function: ModuleFunction,
    #[serde(default, skip_serializing_if = "ModuleAttributes::is_none")]
    pub attributes: ModuleAttributes,
    /// Its pinned address block, when the person set one.
    #[serde(default, skip_serializing_if = "AddressPin::is_none")]
    pub address: AddressPin,
    /// The supply it takes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supply: Option<RatedVoltage>,
}

impl Default for PlcModule {
    fn default() -> Self {
        PlcModule::new(ModuleFunction::DigitalIn(ChannelGroup::new(Signal::Dc24v, 0)))
    }
}

/// A pinned address block: its first byte (0–[`MAX_BYTE`]) and its size in bytes
/// (1–[`MAX_SIZE`]), each when set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddressPin {
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "start_in_range")]
    pub start: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "size_in_range")]
    pub size: Option<u32>,
}

fn slot_in_range<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u16>, D::Error> {
    let v = Option::<u16>::deserialize(d)?;
    match v {
        Some(n) if n > MAX_SLOT => Err(serde::de::Error::custom(format!("slot {n} is over {MAX_SLOT}"))),
        v => Ok(v),
    }
}

fn count_in_range<'de, D: Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    let n = u16::deserialize(d)?;
    if n > MAX_CHANNELS { Err(serde::de::Error::custom(format!("{n} channels is over {MAX_CHANNELS}"))) } else { Ok(n) }
}

fn start_in_range<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
    match Option::<u32>::deserialize(d)? {
        Some(n) if n > MAX_BYTE => Err(serde::de::Error::custom(format!("start byte {n} is over {MAX_BYTE}"))),
        v => Ok(v),
    }
}

fn size_in_range<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
    match Option::<u32>::deserialize(d)? {
        Some(n) if n == 0 || n > MAX_SIZE => Err(serde::de::Error::custom(format!("block size {n} is not 1 to {MAX_SIZE}"))),
        v => Ok(v),
    }
}

impl AddressPin {
    /// Whether nothing is pinned.
    pub fn is_none(&self) -> bool {
        self.start.is_none() && self.size.is_none()
    }
}

/// Properties of a module or its channels, never functions (owner, 2026-11-17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleAttributes {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub safety_rated: bool,
    /// Ex i.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub intrinsically_safe: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub calibration_certificate: bool,
}

impl ModuleAttributes {
    /// Whether no attribute is set.
    pub fn is_none(&self) -> bool {
        !self.safety_rated && !self.intrinsically_safe && !self.calibration_certificate
    }
}

/// What a module does (owner, 2026-11-17): generic names, never a vendor's (design 072 §3, F41).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleFunction {
    DigitalIn(ChannelGroup),
    DigitalOut(ChannelGroup),
    AnalogIn(ChannelGroup),
    AnalogOut(ChannelGroup),
    /// Several groups on one module.
    Mixed(Vec<ChannelGroup>),
    /// Encoders, SSI, counters.
    Position(Position),
    /// Stepper, servo and DC motor drives.
    Drive(Drive),
    /// Gateways, serial interfaces, sub-buses, IO-Link masters.
    Communication(Communication),
    /// Data acquisition: power measurement, vibration, strain.
    Measurement(Measurement),
    SafetyLogic(SafetyLogic),
    /// Power feeds, potential distribution, fusing, end caps, shield.
    System(SystemModule),
}

/// A position module's data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub kind: PositionKind,
    pub channels: ChannelGroup,
}

crate::word_enum! {
    /// What a position module reads.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum PositionKind {
        Encoder,
        Ssi,
        Counter,
    }
}

/// A drive module's data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Drive {
    pub kind: DriveKind,
    pub axes: u8,
}

crate::word_enum! {
    /// What a drive module drives.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum DriveKind {
        Stepper,
        Servo,
        DcMotor,
    }
}

/// A communication module's data (deviation from design 072 §3, overseer 2026-11-17): what kind
/// of link it is decides whether it has channels, so a wrong combination cannot be built. An
/// IO-Link master always has its channels (its ports, signal [`Signal::IoLink`]); any other
/// communication module has a protocol and ports and never channels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Communication {
    /// A gateway, serial interface or sub-bus on a protocol.
    Link(CommLink),
    /// An IO-Link master.
    IoLinkMaster(IoLinkMaster),
}

/// A communication link's data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommLink {
    pub protocol: Protocol,
    pub role: CommRole,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<Port>,
}

/// An IO-Link master's data: its ports are its channels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IoLinkMaster {
    pub channels: ChannelGroup,
}

crate::word_enum! {
    /// What a communication module is to its station.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum CommRole {
        Gateway,
        Serial,
        Fieldbus,
    }
}

/// A communication port, with its name as printed on the module.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Port {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<Text<32>>,
}

/// A measurement module's data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Measurement {
    pub kind: MeasurementKind,
    pub channels: ChannelGroup,
}

crate::word_enum! {
    /// What a measurement module measures.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum MeasurementKind {
        Power,
        Vibration,
        Strain,
    }
}

/// A safety logic module's data: its own safe inputs and outputs, when it has any (design 072
/// §3). Unlike a communication module's, both cases are one kind of module, so the option is the
/// data, not a combination to rule out.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafetyLogic {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channels: Option<ChannelGroup>,
}

/// A system module's data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemModule {
    pub kind: SystemKind,
}

crate::word_enum! {
    /// What a system module is.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum SystemKind {
        /// A power supply module making the station's supply.
        PowerSupply,
        /// A power feed into the station's potential groups.
        PowerFeed,
        PotentialDistribution,
        Fusing,
        EndCap,
        Shield,
    }
}

/// One group of channels carrying one signal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelGroup {
    pub signal: Signal,
    /// How many channels the group has, 0–[`MAX_CHANNELS`].
    #[serde(deserialize_with = "count_in_range")]
    pub count: u16,
    /// Each channel's own data, by index in the group; a channel with none is not listed.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub channels: BTreeMap<u16, Channel>,
}

impl ChannelGroup {
    /// `count` channels of `signal`, none with data of its own.
    pub fn new(signal: Signal, count: u16) -> ChannelGroup {
        ChannelGroup { signal, count, channels: BTreeMap::new() }
    }
}

crate::word_enum! {
    /// The signal a channel group carries.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Signal {
        Dc24v,
        Ac230v,
        Relay,
        Current4To20,
        Voltage0To10,
        Rtd,
        Thermocouple,
        IoLink,
    }
}

impl Signal {
    /// As a person reads it: `24 V`, `relay`, `4–20 mA`.
    pub fn label(self) -> &'static str {
        match self {
            Signal::Dc24v => "24 V",
            Signal::Ac230v => "230 V AC",
            Signal::Relay => "relay",
            Signal::Current4To20 => "4\u{2013}20 mA",
            Signal::Voltage0To10 => "0\u{2013}10 V",
            Signal::Rtd => "RTD",
            Signal::Thermocouple => "thermocouple",
            Signal::IoLink => "IO-Link",
        }
    }
}

/// One channel's own data (design 045 §4.6, 056 §4).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Channel {
    /// Its description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<ChannelText>,
    /// An address typed over the automatic one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<ChannelAddress>,
    /// Its PLC tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<PlcTag>,
    /// What an output does when the PLC stops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe: Option<SafeState>,
}

impl Channel {
    /// Whether it holds nothing (and so is not listed).
    pub fn is_empty(&self) -> bool {
        self.text.is_none() && self.address.is_none() && self.tag.is_none() && self.safe.is_none()
    }
}

/// What an output does when the PLC stops: a digital output `off`, `on` or `hold`; an analogue
/// output `hold`, `min`, `max` or a value (`4 mA`).
///
/// # Semantics
/// Written as its word, or the value's text. A text that is one of the words in another case is a
/// read error (the words are stored in lower case).
///
/// # Example
/// ```
/// use crate::objects::plc::SafeState;
/// assert_eq!(SafeState::parse("Hold"), Some(SafeState::Hold));
/// assert_eq!(SafeState::parse("4 mA").map(|s| s.to_string()), Some("4 mA".to_string()));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SafeState {
    Off,
    On,
    Hold,
    Min,
    Max,
    Value(Text<16>),
}

impl SafeState {
    /// The words, as stored.
    pub const WORDS: [&'static str; 5] = ["off", "on", "hold", "min", "max"];

    /// The word of a worded state; `None` for a value.
    pub fn word(&self) -> Option<&'static str> {
        match self {
            SafeState::Off => Some("off"),
            SafeState::On => Some("on"),
            SafeState::Hold => Some("hold"),
            SafeState::Min => Some("min"),
            SafeState::Max => Some("max"),
            SafeState::Value(_) => None,
        }
    }

    /// The state `text` names: a word in any case, else a value of 1 to 16 characters.
    pub fn parse(text: &str) -> Option<SafeState> {
        let t = text.trim();
        let worded = [SafeState::Off, SafeState::On, SafeState::Hold, SafeState::Min, SafeState::Max];
        let word = worded.into_iter().find(|s| s.word().is_some_and(|w| w.eq_ignore_ascii_case(t)));
        word.or_else(|| Text::new(t).ok().map(SafeState::Value))
    }
}

impl fmt::Display for SafeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SafeState::Value(v) => f.write_str(v.as_str()),
            SafeState::Off | SafeState::On | SafeState::Hold | SafeState::Min | SafeState::Max => f.write_str(self.word().unwrap_or_default()),
        }
    }
}

impl Serialize for SafeState {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SafeState {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        match SafeState::parse(&text) {
            Some(s) if s.to_string() == text => Ok(s),
            _ => Err(serde::de::Error::custom(format!("`{}` is not a safe state (off, on, hold, min, max or a value)", text.escape_debug()))),
        }
    }
}

impl ModuleFunction {
    /// Its channel groups, in order.
    pub fn groups(&self) -> Vec<&ChannelGroup> {
        match self {
            ModuleFunction::DigitalIn(g) | ModuleFunction::DigitalOut(g) | ModuleFunction::AnalogIn(g) | ModuleFunction::AnalogOut(g) => vec![g],
            ModuleFunction::Mixed(v) => v.iter().collect(),
            ModuleFunction::Position(p) => vec![&p.channels],
            ModuleFunction::Measurement(m) => vec![&m.channels],
            ModuleFunction::Communication(Communication::IoLinkMaster(m)) => vec![&m.channels],
            ModuleFunction::Communication(Communication::Link(_)) => Vec::new(),
            ModuleFunction::SafetyLogic(s) => s.channels.iter().collect(),
            ModuleFunction::Drive(_) | ModuleFunction::System(_) => Vec::new(),
        }
    }

    /// Its channel groups, in order, to change.
    pub fn groups_mut(&mut self) -> Vec<&mut ChannelGroup> {
        match self {
            ModuleFunction::DigitalIn(g) | ModuleFunction::DigitalOut(g) | ModuleFunction::AnalogIn(g) | ModuleFunction::AnalogOut(g) => vec![g],
            ModuleFunction::Mixed(v) => v.iter_mut().collect(),
            ModuleFunction::Position(p) => vec![&mut p.channels],
            ModuleFunction::Measurement(m) => vec![&mut m.channels],
            ModuleFunction::Communication(Communication::IoLinkMaster(m)) => vec![&mut m.channels],
            ModuleFunction::Communication(Communication::Link(_)) => Vec::new(),
            ModuleFunction::SafetyLogic(s) => s.channels.iter_mut().collect(),
            ModuleFunction::Drive(_) | ModuleFunction::System(_) => Vec::new(),
        }
    }

    /// Its function's word (`digital_in`), for lists and messages.
    pub fn word(&self) -> &'static str {
        match self {
            ModuleFunction::DigitalIn(_) => "digital_in",
            ModuleFunction::DigitalOut(_) => "digital_out",
            ModuleFunction::AnalogIn(_) => "analog_in",
            ModuleFunction::AnalogOut(_) => "analog_out",
            ModuleFunction::Mixed(_) => "mixed",
            ModuleFunction::Position(_) => "position",
            ModuleFunction::Drive(_) => "drive",
            ModuleFunction::Communication(_) => "communication",
            ModuleFunction::Measurement(_) => "measurement",
            ModuleFunction::SafetyLogic(_) => "safety_logic",
            ModuleFunction::System(_) => "system",
        }
    }
}

impl crate::Document {
    /// The modules naming station `rack` (whether or not it is a station now), in ID order: the
    /// index's station → modules map, never a walk of the project.
    pub fn plc_modules(&self, rack: DeviceId) -> impl Iterator<Item = DeviceId> + '_ {
        crate::document::index::referrers(&self.index.station_modules, &rack)
    }
}

impl PlcModule {
    /// A module doing `function`, in no station, with nothing pinned.
    pub fn new(function: ModuleFunction) -> PlcModule {
        PlcModule { rack: None, slot: None, function, attributes: ModuleAttributes::default(), address: AddressPin::default(), supply: None }
    }

    /// How many channels it has, over all its groups.
    pub fn channel_count(&self) -> u32 {
        self.function.groups().iter().map(|g| u32::from(g.count)).sum()
    }

    /// The group channel `i` (counted over the module) is in, and its index there. A channel past
    /// the last group's count is the last group's.
    fn locate(&self, i: u32) -> Option<(usize, u16)> {
        let groups = self.function.groups();
        let mut first = 0u32;
        for (n, g) in groups.iter().enumerate() {
            let end = first.saturating_add(u32::from(g.count));
            if i < end || n + 1 == groups.len() {
                return u16::try_from(i.checked_sub(first)?).ok().map(|x| (n, x));
            }
            first = end;
        }
        None
    }

    /// The signal of channel `i`.
    pub fn signal_of(&self, i: u32) -> Option<Signal> {
        let (g, _) = self.locate(i)?;
        self.function.groups().get(g).map(|x| x.signal)
    }

    /// Channel `i`'s own data, if it has any.
    pub fn channel(&self, i: u32) -> Option<&Channel> {
        let (g, x) = self.locate(i)?;
        self.function.groups().get(g)?.channels.get(&x)
    }

    /// Every channel with data of its own, by its index over the module.
    pub fn channels(&self) -> Vec<(u32, &Channel)> {
        let mut out = Vec::new();
        let mut first = 0u32;
        for g in self.function.groups() {
            out.extend(g.channels.iter().map(|(i, c)| (first.saturating_add(u32::from(*i)), c)));
            first = first.saturating_add(u32::from(g.count));
        }
        out
    }

    /// Change channel `i`'s own data with `f`; a channel left with none is dropped. `false` when
    /// the module has no channel group.
    pub fn edit_channel(&mut self, i: u32, f: impl FnOnce(&mut Channel)) -> bool {
        let Some((g, x)) = self.locate(i) else { return false };
        let mut groups = self.function.groups_mut();
        let Some(group) = groups.get_mut(g) else { return false };
        let ch = group.channels.entry(x).or_default();
        f(ch);
        if ch.is_empty() {
            group.channels.remove(&x);
        }
        true
    }
}
