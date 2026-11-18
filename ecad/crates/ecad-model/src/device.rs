//! What a device is (design 072 §2.2): its [`DeviceType`], one of twelve groups, each with its own
//! data.
//!
//! # Semantics
//! A device's type is stored, never worked out from its symbol, its tag or a property word: the
//! symbol declares it ([`crate::Definition::device_type`]) and placing that symbol as a new device
//! copies it. Adding a symbol to an existing device never changes the type; changing it is one
//! [`crate::Op::ModifyDevice`].
//!
//! Each group exists because it has fields and methods no other group has; a group is added only
//! when a symbol needs fields or a calculation no group has. The groups start from IEC 81346-2's
//! top-level classes.
//!
//! This module holds the stored data only. The calculations on it (supply, load, protection,
//! switching…) are traits in `ecad-electrical`, implemented on these types (design 072 Part I);
//! the PLC groups' data is lane C's.
#![deny(clippy::wildcard_enum_match_arm)]

use serde::{Deserialize, Serialize};

crate::word_enum! {
    /// The group of a [`DeviceType`], without its data: for lists, filters and messages.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum DeviceGroup {
        PowerSupply,
        Protection,
        Switching,
        Sensor,
        Motor,
        Load,
        Indicator,
        TerminalStrip,
        Connector,
        PlcRack,
        PlcModule,
        General,
    }
}

impl DeviceGroup {
    /// The group's name as a person reads it (`Power supply`).
    pub fn label(self) -> &'static str {
        match self {
            DeviceGroup::PowerSupply => "Power supply",
            DeviceGroup::Protection => "Protection",
            DeviceGroup::Switching => "Switching",
            DeviceGroup::Sensor => "Sensor",
            DeviceGroup::Motor => "Motor",
            DeviceGroup::Load => "Load",
            DeviceGroup::Indicator => "Indicator",
            DeviceGroup::TerminalStrip => "Terminal strip",
            DeviceGroup::Connector => "Connector",
            DeviceGroup::PlcRack => "PLC station",
            DeviceGroup::PlcModule => "PLC module",
            DeviceGroup::General => "General",
        }
    }
}

/// What a device is: its group and the data only that group has (design 072 §2.2).
///
/// # Semantics
/// Written as the group's word with its data (`{"protection": {…}}`), or the bare word for a group
/// without data (`"general"`). An unknown group or field is a read error.
///
/// # Example
/// ```
/// use ecad_model::device::{DeviceGroup, DeviceType, Protection};
/// let t = DeviceType::Protection(Protection::default());
/// assert_eq!(t.group(), DeviceGroup::Protection);
/// assert_eq!(serde_json::to_string(&t).unwrap(), r#"{"protection":{}}"#);
/// assert_eq!(serde_json::to_string(&DeviceType::General).unwrap(), r#""general""#);
/// ```
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceType {
    /// G and T: power supplies, batteries, transformers.
    PowerSupply(PowerSupply),
    /// F, and Q's protective switches: fuses, MCBs, RCDs, RCBOs, fuse banks.
    Protection(Protection),
    /// K and S: relays, contactors, SSRs, timers, push buttons, selectors, switch-disconnectors.
    Switching(Switching),
    /// B: proximity, float, limit and interlock switches, temperature sensors.
    Sensor(Sensor),
    /// M: motors.
    Motor(Motor),
    /// E and MB: heaters, valves, solenoids and other consumers.
    Load(Load),
    /// P: lamps, beacons, buzzers, meters.
    Indicator(Indicator),
    /// X: DIN-rail terminals.
    TerminalStrip(TerminalStrip),
    /// X: plugs, sockets, bulkheads.
    Connector(Connector),
    /// K: a PLC station, with a CPU or a coupler at its head (lane C).
    PlcRack(PlcRack),
    /// K: an I/O, power or other module in a station (lane C).
    PlcModule(PlcModule),
    /// Everything else: no fields and no methods of its own.
    #[default]
    General,
}

impl DeviceType {
    /// The group, without its data.
    pub fn group(&self) -> DeviceGroup {
        match self {
            DeviceType::PowerSupply(_) => DeviceGroup::PowerSupply,
            DeviceType::Protection(_) => DeviceGroup::Protection,
            DeviceType::Switching(_) => DeviceGroup::Switching,
            DeviceType::Sensor(_) => DeviceGroup::Sensor,
            DeviceType::Motor(_) => DeviceGroup::Motor,
            DeviceType::Load(_) => DeviceGroup::Load,
            DeviceType::Indicator(_) => DeviceGroup::Indicator,
            DeviceType::TerminalStrip(_) => DeviceGroup::TerminalStrip,
            DeviceType::Connector(_) => DeviceGroup::Connector,
            DeviceType::PlcRack(_) => DeviceGroup::PlcRack,
            DeviceType::PlcModule(_) => DeviceGroup::PlcModule,
            DeviceType::General => DeviceGroup::General,
        }
    }

    /// A group's type with its default data (what a symbol declares before any value is filled
    /// in).
    pub fn of_group(group: DeviceGroup) -> DeviceType {
        match group {
            DeviceGroup::PowerSupply => DeviceType::PowerSupply(PowerSupply::default()),
            DeviceGroup::Protection => DeviceType::Protection(Protection::default()),
            DeviceGroup::Switching => DeviceType::Switching(Switching::default()),
            DeviceGroup::Sensor => DeviceType::Sensor(Sensor::default()),
            DeviceGroup::Motor => DeviceType::Motor(Motor::default()),
            DeviceGroup::Load => DeviceType::Load(Load::default()),
            DeviceGroup::Indicator => DeviceType::Indicator(Indicator::default()),
            DeviceGroup::TerminalStrip => DeviceType::TerminalStrip(TerminalStrip::default()),
            DeviceGroup::Connector => DeviceType::Connector(Connector::default()),
            DeviceGroup::PlcRack => DeviceType::PlcRack(PlcRack::default()),
            DeviceGroup::PlcModule => DeviceType::PlcModule(PlcModule::default()),
            DeviceGroup::General => DeviceType::General,
        }
    }

    /// Whether this is [`DeviceType::General`] (left out of a definition's file record).
    pub fn is_general(&self) -> bool {
        matches!(self, DeviceType::General)
    }
}

/// A power supply, battery or transformer (group 1). Its fields come with lane B step 3.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerSupply {}

/// A protective device (group 2). Its fields come with lane B step 3.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Protection {}

/// A switching device and what operates it (group 3). Its fields come with lane B step 3.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Switching {}

/// A sensor (group 4). Its fields come with lane B step 3.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sensor {}

/// A motor (group 5). Its fields come with lane B step 3.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Motor {}

/// A consumer with a power and a demand (group 6). Its fields come with lane B step 3.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Load {}

/// An indicator (group 7). Its fields come with lane B step 3.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Indicator {}

/// A strip of DIN-rail terminals (group 8, design 072 §2.5). Its fields come with lane B step 3.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalStrip {}

/// A connector half or bulkhead (group 9). Its fields come with lane B step 3.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connector {}

/// A PLC station (group 10, design 072 §3). Its fields are lane C's.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlcRack {}

/// A module in a PLC station (group 11, design 072 §3). Its fields are lane C's.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlcModule {}

/// A device's description (design 072 §2.1).
pub type Description = crate::text::Text<256>;

/// A part's number, maker, article number or type text (design 072 §2.1).
pub type PartText = crate::text::Text<64>;

/// The `setprop` key of a device's description.
pub const DESCRIPTION_KEY: &str = "description";
/// The `setprop` key of a device's part number.
pub const PART_NUMBER_KEY: &str = "part_number";
/// The `setprop` key of a device's maker (its `manufacturer` field).
pub const MAKER_KEY: &str = "maker";
/// The `setprop` key of a device's article number.
pub const ARTICLE_KEY: &str = "article_number";
/// The `setprop` key of a device's type text (a strip's type notation reads it).
pub const TYPE_KEY: &str = "type";

/// One of a device's text fields ([`crate::Device`]), named by the `setprop` key that routes to it
/// (design 072 §10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TextField {
    Description,
    PartNumber,
    Manufacturer,
    ArticleNumber,
    TypeText,
}

impl TextField {
    /// Every text field, in the device's order.
    pub const ALL: [TextField; 5] =
        [TextField::Description, TextField::PartNumber, TextField::Manufacturer, TextField::ArticleNumber, TextField::TypeText];

    /// The `setprop` key that names it.
    pub fn key(self) -> &'static str {
        match self {
            TextField::Description => DESCRIPTION_KEY,
            TextField::PartNumber => PART_NUMBER_KEY,
            TextField::Manufacturer => MAKER_KEY,
            TextField::ArticleNumber => ARTICLE_KEY,
            TextField::TypeText => TYPE_KEY,
        }
    }

    /// The field `key` names, if it names one.
    pub fn of_key(key: &str) -> Option<TextField> {
        TextField::ALL.into_iter().find(|f| f.key() == key)
    }

    /// The most characters it holds.
    pub fn max_chars(self) -> usize {
        match self {
            TextField::Description => Description::MAX,
            TextField::PartNumber | TextField::Manufacturer | TextField::ArticleNumber | TextField::TypeText => PartText::MAX,
        }
    }

    /// Its name in messages and the inspector.
    pub fn label(self) -> &'static str {
        match self {
            TextField::Description => "description",
            TextField::PartNumber => "part number",
            TextField::Manufacturer => "maker",
            TextField::ArticleNumber => "article number",
            TextField::TypeText => "type",
        }
    }
}
