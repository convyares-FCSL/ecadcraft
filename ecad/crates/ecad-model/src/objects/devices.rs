//! What a device is: its [`DeviceType`], one of twelve groups, each with its own
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
//! switching…) are traits in `ecad-electrical`, implemented on these types;
//! the PLC groups' data is in [`crate::objects::plc`].
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

/// Where a rating field's value comes from: what decides whether the rating line on the drawing shows
/// it by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldSource {
    /// The nameplate: shown by default.
    Plate,
    /// How the device is used in this design (a motor's starting, a lamp's flashing): shown by
    /// default.
    Design,
    /// The datasheet: kept, not shown unless the template adds it.
    Datasheet,
}

/// One rating field of a group, declared once: its word (in a template, `setprop rating.<word>`,
/// the agent's view), its label, where its value comes from, its value type, and how it is read and
/// written on a device's type.
///
/// # Semantics
/// `get` is the value as the drawing shows it, `None` when it is not set or this kind of device in
/// the group has no such field. `set` writes it from text through the type's parser
/// ([`fields::FieldValue`]); empty text clears it. The upper limits are the group's
/// ([`DeviceType::problem`]).
#[derive(Debug, Clone, Copy)]
pub struct RatingField {
    pub word: &'static str,
    /// As a person reads it (`Rated voltage`).
    pub label: &'static str,
    pub source: FieldSource,
    pub ty: fields::FieldType,
    pub get: fn(&DeviceType) -> Option<String>,
    pub set: fn(&mut DeviceType, Option<&str>) -> Result<(), fields::FieldError>,
}

impl PartialEq for RatingField {
    fn eq(&self, other: &Self) -> bool {
        self.word == other.word && self.label == other.label && self.source == other.source && self.ty == other.ty
    }
}

/// The `set` of a field written only through the whole rating.
pub(crate) fn combined(_: &mut DeviceType, _: Option<&str>) -> Result<(), fields::FieldError> {
    Err(fields::FieldError::Combined)
}

/// `parts`' texts joined by `, `.
pub(crate) fn join_parts(parts: &[(&str, String)]) -> String {
    parts.iter().map(|(_, v)| v.as_str()).collect::<Vec<_>>().join(", ")
}

impl DeviceGroup {
    /// The fields its rating line can show, in their default order; empty for a group with no
    /// rating line.
    pub fn rating_fields(self) -> &'static [RatingField] {
        match self {
            DeviceGroup::PowerSupply => power_supply::FIELDS,
            DeviceGroup::Protection => protection::FIELDS,
            DeviceGroup::Switching => switching::FIELDS,
            DeviceGroup::Sensor => sensor::FIELDS,
            DeviceGroup::Motor => motor::FIELDS,
            DeviceGroup::Load => load::FIELDS,
            DeviceGroup::Indicator => indicator::FIELDS,
            DeviceGroup::TerminalStrip | DeviceGroup::Connector | DeviceGroup::PlcRack | DeviceGroup::PlcModule | DeviceGroup::General => &[],
        }
    }

    /// Its rating field with word `word`.
    pub fn rating_field(self, word: &str) -> Option<&'static RatingField> {
        self.rating_fields().iter().find(|f| f.word == word)
    }

    /// The fields its rating line shows by default: the plate and design fields, in order.
    pub fn default_template(self) -> Vec<&'static str> {
        self.rating_fields().iter().filter(|f| f.source != FieldSource::Datasheet).map(|f| f.word).collect()
    }

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

/// What a device is: its group and the data only that group has.
///
/// # Semantics
/// Written as the group's word with its data (`{"protection": {…}}`), or the bare word for a group
/// without data (`"general"`). An unknown group or field is a read error.
///
/// # Example
/// ```
/// use ecad_model::objects::devices::{DeviceGroup, DeviceType, Protection};
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
    /// K: a PLC station, with a CPU or a coupler at its head.
    PlcRack(PlcRack),
    /// K: an I/O, power or other module in a station.
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

    /// Whether its group holds a rating in typed fields: a device of
    /// such a group with none of them set has no rating, whatever its note says.
    pub fn has_rating_fields(&self) -> bool {
        match self {
            DeviceType::Protection(_) => true,
            // A push button or selector is commonly unrated: only a coil or an SSR is warned about.
            DeviceType::Switching(s) => s.operator_kind() != OperatorKind::Manual,
            // A contact or a thermocouple has no supply: only a powered sensor is warned about.
            DeviceType::Sensor(s) => s.output.is_powered(),
            DeviceType::PowerSupply(_) | DeviceType::Motor(_) | DeviceType::Load(_) | DeviceType::Indicator(_) => true,
            DeviceType::TerminalStrip(_) | DeviceType::Connector(_) | DeviceType::PlcRack(_) | DeviceType::PlcModule(_) | DeviceType::General => {
                false
            }
        }
    }

    /// Its set rating fields as shown, each with its field word ([`DeviceGroup::rating_fields`]),
    /// in the order [`DeviceType::rating`] writes them.
    pub fn rating_parts(&self) -> Vec<(&'static str, String)> {
        self.group().rating_fields().iter().filter_map(|f| (f.get)(self).map(|v| (f.word, v))).collect()
    }

    /// Its rating line by a template: the set fields named by `words`, in that order, joined by
    /// `, `; `None` when none of them is set. A word the group has no field for shows nothing.
    pub fn rating_line(&self, words: &[&str]) -> Option<String> {
        let parts = self.rating_parts();
        let shown: Vec<(&str, String)> = words.iter().filter_map(|w| parts.iter().find(|(p, _)| p == w)).map(|(w, v)| (*w, v.clone())).collect();
        (!shown.is_empty()).then(|| join_parts(&shown))
    }

    /// Its typed rating as shown (`curve B, 16 A`), when a rating field is set.
    /// A field that only says how the device is used (an indicator's flashing) is not a rating.
    pub fn rating(&self) -> Option<String> {
        let parts = self.rating_parts();
        (!parts.is_empty() && *self != self.unrated()).then(|| join_parts(&parts))
    }

    /// The same type with its rating fields empty (its kind and contacts kept).
    pub fn unrated(&self) -> DeviceType {
        match self {
            DeviceType::Protection(p) => DeviceType::Protection(p.unrated()),
            DeviceType::Switching(s) => DeviceType::Switching(s.unrated()),
            DeviceType::Sensor(s) => DeviceType::Sensor(s.unrated()),
            DeviceType::Motor(m) => DeviceType::Motor(m.unrated()),
            DeviceType::Load(l) => DeviceType::Load(l.unrated()),
            DeviceType::Indicator(i) => DeviceType::Indicator(i.unrated()),
            DeviceType::PowerSupply(p) => DeviceType::PowerSupply(p.unrated()),
            // A strip's ratings are its fuse positions'.
            DeviceType::TerminalStrip(_) => DeviceType::TerminalStrip(TerminalStrip::default()),
            DeviceType::Connector(_) | DeviceType::PlcRack(_) | DeviceType::PlcModule(_) | DeviceType::General => self.clone(),
        }
    }

    /// Why its data is not well formed, if it is not.
    pub fn problem(&self) -> Option<String> {
        match self {
            DeviceType::Protection(p) => p.problem(),
            DeviceType::Connector(c) => c.problem(),
            DeviceType::Switching(s) => s.problem(),
            DeviceType::Sensor(s) => s.problem(),
            DeviceType::TerminalStrip(s) => s.problem(),
            DeviceType::Motor(m) => m.problem(),
            DeviceType::Load(l) => l.problem(),
            DeviceType::Indicator(i) => i.problem(),
            DeviceType::PowerSupply(p) => p.problem(),
            DeviceType::PlcRack(_) | DeviceType::PlcModule(_) | DeviceType::General => None,
        }
    }

    /// Whether this is [`DeviceType::General`] (left out of a definition's file record).
    pub fn is_general(&self) -> bool {
        matches!(self, DeviceType::General)
    }

    /// A terminal strip's data, when it is one.
    pub fn as_strip(&self) -> Option<&TerminalStrip> {
        match self {
            DeviceType::TerminalStrip(s) => Some(s),
            DeviceType::PowerSupply(_)
            | DeviceType::Protection(_)
            | DeviceType::Switching(_)
            | DeviceType::Sensor(_)
            | DeviceType::Motor(_)
            | DeviceType::Load(_)
            | DeviceType::Indicator(_)
            | DeviceType::Connector(_)
            | DeviceType::PlcRack(_)
            | DeviceType::PlcModule(_)
            | DeviceType::General => None,
        }
    }

    /// A connector's data, when it is one.
    pub fn as_connector(&self) -> Option<&Connector> {
        match self {
            DeviceType::Connector(c) => Some(c),
            DeviceType::PowerSupply(_)
            | DeviceType::Protection(_)
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

    /// A protective device's data, when it is one.
    pub fn as_protection(&self) -> Option<&Protection> {
        match self {
            DeviceType::Protection(p) => Some(p),
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
}

/// A [`RatingField`] stored in plain fields of a group's data: its word, label, source and value
/// type, how it shows (the type's own way unless `show` says) and the words it may start with as
/// shown (`after`), then, per kind of device that has it, the pattern and the field.
macro_rules! rating_field {
    ($word:literal, $label:literal, $source:ident, $ty:ty, show $show:expr, after [$($p:literal),*], $($pat:pat => $place:expr),+ $(,)?) => {
        $crate::objects::devices::RatingField {
            word: $word,
            label: $label,
            source: $crate::objects::devices::FieldSource::$source,
            ty: <$ty as $crate::objects::devices::fields::FieldValue>::TYPE,
            get: |t| {
                $(if let $pat = t {
                    return $place.map($show);
                })+
                None
            },
            set: |t, v| {
                const AFTER: &[&str] = &[$($p),*];
                $(if let $pat = t {
                    return $crate::objects::devices::fields::put_after::<$ty>(AFTER, &mut $place, v);
                })+
                Err($crate::objects::devices::fields::FieldError::NotHere)
            },
        }
    };
    ($word:literal, $label:literal, $source:ident, $ty:ty, $($pat:pat => $place:expr),+ $(,)?) => {
        rating_field!($word, $label, $source, $ty, show <$ty as $crate::objects::devices::fields::FieldValue>::show, after [], $($pat => $place),+)
    };
}

pub mod contacts;
pub mod fields;
pub mod indicator;
pub mod ip_code;
pub mod load;
pub mod motor;
pub mod power_supply;
pub mod protection;
pub mod sensor;
pub mod switching;
pub mod terminal_strip;
pub use contacts::{ContactEntry, ContactLabel, ContactPoles, ContactSet, ContactTable, PoleKind};

pub use power_supply::{PowerSupply, Psu, Transformer};

pub use protection::{Breaker, Fuse, FuseBank, FuseClass, FuseSize, Poles, Protection, ProtectionKind, Rcbo, Rcd, ResidualType, TripCurve};

pub use switching::{Control, Operator, OperatorKind, Switching};

pub use sensor::{Output, Sensing, Sensor};

pub use motor::{Motor, Phases, Starting};

pub use load::Load;

pub use indicator::{Indicator, IndicatorColour, SignalType};

pub use ip_code::IpCode;

pub use terminal_strip::{PositionLabel, TerminalStrip};

/// A connector: its family, pins and the part of each half.
///
/// # Semantics
/// - `family` is the one its symbol declares (a pin's symbol declares [`ConnectorFamily::Plain`]);
///   a half drawn with another family's symbol is the Checks row `connector-family`.
/// - `pins` is the pin count typed for the connector (1 to [`MAX_PINS`]); `None` takes the
///   symbols'.
/// - `pin_names` is a typed list of pin names in pin order (each 1 to [`MAX_PIN_NAME`]
///   characters, all different ignoring case, at most [`MAX_PINS`]); names not drawn yet are kept.
/// - `socket` and `plug` are each half's own part (the BOM lists a half with a part number).
/// - `mating` is whether it has mating halves (a socket and a plug): set by its
///   starter (a half or pin) or the `connector` command, never worked out from its symbol. A
///   one-piece connector (a bulkhead, `rj45`) is Connector-typed with `mating` false.
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
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mating: bool,
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
    pub(crate) fn is_key(key: &str) -> bool {
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
        let row = <ConnectorCheck as crate::naming::words::Word>::parse(key.strip_prefix("ignore.")?)?;
        self.ignored.contains(&row).then(|| crate::naming::words::flag_word(true).to_string())
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

/// Most pins a connector has.
pub const MAX_PINS: u8 = 64;
/// Most characters of a pin name.
pub const MAX_PIN_NAME: usize = 8;
/// A pin name (`PE`, `13`).
pub type PinName = crate::naming::text::Text<MAX_PIN_NAME>;
/// A connector's pin count.
pub type PinCount = std::num::NonZeroU8;

crate::word_enum! {
    /// A connector family: both halves of one connector have one.
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
            other => <ConnectorFamily as crate::naming::words::Word>::parse(other),
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

/// One half's own part: what the BOM lists for it.
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

pub use crate::objects::plc::{PlcModule, PlcRack};

/// A device's rating note: text that does not read as its group's
/// rating ("see datasheet", a vendor code), never a second copy of the rating.
pub type RatingNote = crate::naming::text::Text<64>;

/// The `setprop` key of a device's rating (its typed rating fields, or its note).
pub const RATING_KEY: &str = "rating";
/// A fuse bank's output rating, or a strip's fuse position rating: `rating.<n>`.
pub const RATING_OF_PREFIX: &str = "rating.";

/// A device's description.
pub type Description = crate::naming::text::Text<256>;

/// A part's number, maker, article number or type text.
pub type PartText = crate::naming::text::Text<64>;

/// The `setprop` key of a device's description.
pub const DESCRIPTION_KEY: &str = "description";
/// The `setprop` key of a device's part number.
pub(crate) const PART_NUMBER_KEY: &str = "part_number";
/// The `setprop` key of a device's maker (its `manufacturer` field).
pub const MAKER_KEY: &str = "maker";
/// The `setprop` key of a device's article number.
pub const ARTICLE_KEY: &str = "article_number";
/// The `setprop` key of a device's type text (a strip's type notation reads it).
pub(crate) const TYPE_KEY: &str = "type";

/// One of a device's text fields ([`crate::Device`]), named by the `setprop` key that routes to it
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
