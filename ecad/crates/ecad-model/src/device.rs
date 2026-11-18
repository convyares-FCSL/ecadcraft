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
//! the PLC groups' data is in [`crate::plc`].
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
/// assert_eq!(serde_json::to_string(&t).unwrap(), r#"{"protection":{"fuse":{}}}"#);
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
    /// X: plugs, sockets, bulkheads. (Boxed: a group with much data keeps every op small.)
    Connector(Box<Connector>),
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
            DeviceGroup::Connector => DeviceType::Connector(Box::default()),
            DeviceGroup::PlcRack => DeviceType::PlcRack(PlcRack::default()),
            DeviceGroup::PlcModule => DeviceType::PlcModule(PlcModule::default()),
            DeviceGroup::General => DeviceType::General,
        }
    }

    /// Whether its group holds a rating in typed fields (design 072 §2.2, "Ratings"): a device of
    /// such a group with none of them set has no rating, whatever its note says.
    pub fn has_rating_fields(&self) -> bool {
        match self {
            DeviceType::Protection(_) => true,
            DeviceType::PowerSupply(_)
            | DeviceType::Switching(_)
            | DeviceType::Sensor(_)
            | DeviceType::Motor(_)
            | DeviceType::Load(_)
            | DeviceType::Indicator(_)
            | DeviceType::TerminalStrip(_)
            | DeviceType::Connector(_)
            | DeviceType::PlcRack(_)
            | DeviceType::PlcModule(_)
            | DeviceType::General => false,
        }
    }

    /// Its typed rating as shown (`curve B, 16 A`), when a rating field is set.
    pub fn rating(&self) -> Option<String> {
        match self {
            DeviceType::Protection(p) => (!p.is_unrated()).then(|| p.to_string()),
            DeviceType::PowerSupply(_)
            | DeviceType::Switching(_)
            | DeviceType::Sensor(_)
            | DeviceType::Motor(_)
            | DeviceType::Load(_)
            | DeviceType::Indicator(_)
            | DeviceType::TerminalStrip(_)
            | DeviceType::Connector(_)
            | DeviceType::PlcRack(_)
            | DeviceType::PlcModule(_)
            | DeviceType::General => None,
        }
    }

    /// The same type with its rating fields empty (its kind kept).
    pub fn unrated(&self) -> DeviceType {
        match self {
            DeviceType::Protection(p) => DeviceType::Protection(p.kind().empty()),
            DeviceType::PowerSupply(_)
            | DeviceType::Switching(_)
            | DeviceType::Sensor(_)
            | DeviceType::Motor(_)
            | DeviceType::Load(_)
            | DeviceType::Indicator(_)
            | DeviceType::TerminalStrip(_)
            | DeviceType::Connector(_)
            | DeviceType::PlcRack(_)
            | DeviceType::PlcModule(_)
            | DeviceType::General => self.clone(),
        }
    }

    /// Why its data is not well formed, if it is not.
    pub fn problem(&self) -> Option<String> {
        match self {
            DeviceType::Protection(p) => p.problem(),
            DeviceType::Connector(c) => c.problem(),
            DeviceType::PowerSupply(_)
            | DeviceType::Switching(_)
            | DeviceType::Sensor(_)
            | DeviceType::Motor(_)
            | DeviceType::Load(_)
            | DeviceType::Indicator(_)
            | DeviceType::TerminalStrip(_)
            | DeviceType::PlcRack(_)
            | DeviceType::PlcModule(_)
            | DeviceType::General => None,
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

pub mod protection;
pub use protection::{Breaker, Fuse, FuseBank, FuseClass, Poles, Protection, ProtectionKind, Rcbo, Rcd, TripCurve};

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

/// A connector: its family, pins and the part of each half (group 9, design 072 §2.2, design 044).
///
/// # Semantics
/// - `family` is the one its symbol declares (a pin's symbol declares [`ConnectorFamily::Plain`]);
///   a half drawn with another family's symbol is the Checks row `connector-family`.
/// - `pins` is the pin count typed for the connector (1 to [`MAX_PINS`]); `None` takes the
///   symbols'.
/// - `pin_names` is a typed list of pin names in pin order (each 1 to [`MAX_PIN_NAME`]
///   characters, all different ignoring case, at most [`MAX_PINS`]); names not drawn yet are kept.
/// - `socket` and `plug` are each half's own part (the BOM lists a half with a part number).
/// - `ignored` are the Checks rows its person chose to ignore.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connector {
    pub family: ConnectorFamily,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pins: Option<PinCount>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pin_names: Vec<PinName>,
    #[serde(default, skip_serializing_if = "HalfPart::is_empty")]
    pub socket: HalfPart,
    #[serde(default, skip_serializing_if = "HalfPart::is_empty")]
    pub plug: HalfPart,
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub ignored: std::collections::BTreeSet<ConnectorCheck>,
}

impl Connector {
    /// A connector of `family` with nothing else set.
    pub fn of_family(family: ConnectorFamily) -> Connector {
        Connector { family, ..Connector::default() }
    }

    /// The part of half `plug` (`false`: the socket).
    pub fn half(&self, plug: bool) -> &HalfPart {
        if plug { &self.plug } else { &self.socket }
    }

    /// The part of half `plug`, to change.
    pub fn half_mut(&mut self, plug: bool) -> &mut HalfPart {
        if plug { &mut self.plug } else { &mut self.socket }
    }

    /// Whether `key` is one of its `setprop` keys (`pins`, `pin_names`, `half.<s|p>.<field>`,
    /// `ignore.<row>`).
    pub fn is_key(key: &str) -> bool {
        key == "pins" || key == "pin_names" || key.starts_with("half.") || key.starts_with("ignore.")
    }

    /// The value `setprop`'s `key` names, as text: the pin count, the pin names joined by commas, a
    /// half's `description`, `maker` or `part_number`, or `yes` for an ignored row.
    pub fn value(&self, key: &str) -> Option<String> {
        if key == "pins" {
            return self.pins.map(|n| n.get().to_string());
        }
        if key == "pin_names" {
            return (!self.pin_names.is_empty()).then(|| self.pin_names.iter().map(|n| n.as_str()).collect::<Vec<_>>().join(","));
        }
        if let Some(rest) = key.strip_prefix("half.") {
            let (side, field) = rest.split_once('.')?;
            let h = match side {
                "s" => &self.socket,
                "p" => &self.plug,
                _ => return None,
            };
            let t = match field {
                "description" => h.description.as_ref().map(|t| t.as_str()),
                "maker" => h.manufacturer.as_ref().map(|t| t.as_str()),
                "part_number" => h.part_number.as_ref().map(|t| t.as_str()),
                _ => None,
            };
            return t.map(str::to_string);
        }
        let row = <ConnectorCheck as crate::words::Word>::parse(key.strip_prefix("ignore.")?)?;
        self.ignored.contains(&row).then(|| crate::words::flag_word(true).to_string())
    }

    /// Why it is not well formed, if it is not: a pin count out of range, too many pin names, or a
    /// name twice (ignoring case).
    pub fn problem(&self) -> Option<String> {
        if let Some(n) = self.pins
            && !(1..=MAX_PINS).contains(&n.get())
        {
            return Some(format!("a connector has 1 to {MAX_PINS} pins, not {}", n.get()));
        }
        if self.pin_names.len() > usize::from(MAX_PINS) {
            return Some(format!("a connector has at most {MAX_PINS} pin names"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for n in &self.pin_names {
            if n.as_str().trim().is_empty() || !seen.insert(n.as_str().to_lowercase()) {
                return Some(format!("pin name \"{n}\" is empty or listed twice"));
            }
        }
        None
    }
}

/// Most pins a connector has (design 044 §2.3).
pub const MAX_PINS: u8 = 64;
/// Most characters of a pin name.
pub const MAX_PIN_NAME: usize = 8;
/// A pin name (`PE`, `13`).
pub type PinName = crate::text::Text<MAX_PIN_NAME>;
/// A connector's pin count.
pub type PinCount = std::num::NonZeroU8;

crate::word_enum! {
    /// A connector family: both halves of one connector have one (design 044 §1).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    pub enum ConnectorFamily {
        #[default]
        Plain,
        Dsub,
        M12,
        Rj45,
        Usba,
        Usbb,
    }
}

impl ConnectorFamily {
    /// The prompt keyword: `Plain`, `Dsub`, `M12`, `RJ45`, `USBA`, `USBB`.
    pub fn keyword(self) -> &'static str {
        match self {
            ConnectorFamily::Plain => "Plain",
            ConnectorFamily::Dsub => "Dsub",
            ConnectorFamily::M12 => "M12",
            ConnectorFamily::Rj45 => "RJ45",
            ConnectorFamily::Usba => "USBA",
            ConnectorFamily::Usbb => "USBB",
        }
    }

    /// The words a person reads: `D-sub`, `M12`…
    pub fn name(self) -> &'static str {
        match self {
            ConnectorFamily::Plain => "Plain",
            ConnectorFamily::Dsub => "D-sub",
            ConnectorFamily::M12 => "M12",
            ConnectorFamily::Rj45 => "RJ45",
            ConnectorFamily::Usba => "USB-A",
            ConnectorFamily::Usbb => "USB-B",
        }
    }

    /// The family a typed word names (case-insensitive; `d-sub`, `sub-d` and `usb-a` too).
    pub fn of_typed(text: &str) -> Option<ConnectorFamily> {
        let t: String = text.trim().chars().take(16).filter(|c| *c != '-').collect::<String>().to_ascii_lowercase();
        match t.as_str() {
            "subd" => Some(ConnectorFamily::Dsub),
            other => <ConnectorFamily as crate::words::Word>::parse(other),
        }
    }
}

crate::word_enum! {
    /// A connector Checks row its person can ignore (the row's kind word).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum ConnectorCheck {
        /// Halves of different families.
        #[strum(serialize = "connector-family")]
        Family,
        /// A half's pin count that is not one of its family's sizes.
        #[strum(serialize = "connector-size")]
        Size,
    }
}

/// One half's own part (design 044 §8): what the BOM lists for it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HalfPart {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<PartText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_number: Option<PartText>,
}

impl HalfPart {
    /// Whether nothing is set.
    pub fn is_empty(&self) -> bool {
        *self == HalfPart::default()
    }
}

pub use crate::plc::{PlcModule, PlcRack};

/// A device's rating note (design 072 §2.2, "Ratings"): text that does not read as its group's
/// rating ("see datasheet", a vendor code), never a second copy of the rating.
pub type RatingNote = crate::text::Text<64>;

/// The `setprop` key of a device's rating (its typed rating fields, or its note).
pub const RATING_KEY: &str = "rating";
/// A fuse bank's output rating: `rating.<n>`.
pub const RATING_OF_PREFIX: &str = "rating.";

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
