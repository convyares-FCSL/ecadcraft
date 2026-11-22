//! A wire: one physical conductor, and the typed values it carries (design 072 §4.2, §4.6, lane D).
//!
//! # Semantics
//! - [`Wire`] holds what a wire is in fields: its two ends, its size, its colours (an ordered list
//!   of [`ConductorColour`]s, `GN/YE`), its wire number with its fixed flag, its potential, its
//!   rating inputs ([`CircuitInputs`]), its load and its own wire type ([`StyleRef`]). Only display
//!   choices (its label's text choices) stay in [`Wire::display`].
//! - Every value is checked when it is made ([`WireSize::parse`], [`Colours::parse`],
//!   [`CircuitInputs`]'s bounds) and again when it is read from a file: a stored value out of its
//!   bounds, in another form than the stored one, or an unknown word is a read error.
//! - The rating inputs are shared with cables (lane D step 2): one struct, one set of bounds.
//!
//! This module excludes what the values mean for the drawing (labels, line looks) and for the
//! rating checks; ecad-electrical owns those.
#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::document::doc::Properties;
use crate::geometry::colour::{ColourWords, ConductorColour};
use crate::geometry::units::{Amps, Celsius, Mm2, Percent, Volts};
use crate::naming::id::{ConductorId, ConnectionPointId, DeviceId, NetId};
use crate::naming::text::Text;
use crate::naming::words::{is_digits, uint};
use crate::objects::styles::StyleRef;

/// Most characters of a wire number, a potential, a size or a colour list as typed.
pub const MAX_VALUE_CHARS: usize = 24;

/// One end of a wire: `ends[0]` is end A, `ends[1]` end B.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum End {
    A,
    B,
}

impl End {
    pub const BOTH: [End; 2] = [End::A, End::B];

    /// The other end.
    pub fn other(self) -> End {
        match self {
            End::A => End::B,
            End::B => End::A,
        }
    }
}

/// A wire (stored intent): one conductor between two connection points of one net (design 072
/// §4.2). A loose end is a route's, never a wire's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wire {
    /// The net it belongs to: stable through edits (F39).
    pub net: NetId,
    /// This conductor's own identity in the project: given when the wire is made, never reused.
    pub conductor: ConductorId,
    /// Its place in its net's order, 1-based (design 072 §4.4).
    pub net_order: u16,
    /// Its two connection points.
    pub ends: [ConnectionPointId; 2],
    /// The person reordered this net by hand: the rewrite keeps its order.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub order_fixed: bool,
    /// Its own wire type (a style of catalogue `wire`, design 072 §4.6), over the one its net's
    /// potential or the registry gives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wire_type: Option<StyleRef>,
    /// Its own size, over its wire type's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<WireSize>,
    /// Its own colours, over its wire type's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colours: Option<Colours>,
    /// The wire number it shows, written by the numbering pass or fixed by hand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<WireNumber>,
    /// The potential its net carries (`24V`), as the person chose it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub potential: Option<Text<MAX_VALUE_CHARS>>,
    /// Its rating inputs (brief 057).
    #[serde(default, skip_serializing_if = "CircuitInputs::is_empty")]
    pub rating: CircuitInputs,
    /// The current it carries, as typed (Ib).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load: Option<Amps>,
    /// The device named as its protection (`rating {protected_by}`), over the trace's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected_by: Option<ProtectedBy>,
    /// Display choices and defaults-registry values only (design 072 §9.2).
    #[serde(default, skip_serializing_if = "Properties::is_empty")]
    pub display: Properties,
}

impl Wire {
    /// A wire of net `net` between `a` and `b`, first in its net's order, with no values of its own.
    pub fn new(net: NetId, conductor: ConductorId, a: ConnectionPointId, b: ConnectionPointId) -> Self {
        Wire {
            net,
            conductor,
            net_order: 1,
            ends: [a, b],
            order_fixed: false,
            wire_type: None,
            size: None,
            colours: None,
            number: None,
            potential: None,
            rating: CircuitInputs::default(),
            load: None,
            protected_by: None,
            display: Properties::default(),
        }
    }

    /// The connection point at `end`.
    pub fn end(&self, end: End) -> ConnectionPointId {
        match end {
            End::A => self.ends[0],
            End::B => self.ends[1],
        }
    }

    /// The end at connection point `t`, if the wire ends there (end A first).
    pub fn end_at(&self, t: ConnectionPointId) -> Option<End> {
        End::BOTH.into_iter().find(|e| self.end(*e) == t)
    }

    /// Its values with `other`'s identity, net, order and ends: what a wire made in its place keeps.
    pub fn values_onto(&self, other: &Wire) -> Wire {
        Wire {
            net: other.net,
            conductor: other.conductor,
            net_order: other.net_order,
            ends: other.ends,
            order_fixed: other.order_fixed,
            ..self.clone()
        }
    }

    /// Whether `other` differs from it only in values (not in identity or ends): what
    /// [`crate::document::op::Op::ModifyWire`] may change, with the net and the order.
    pub fn same_wire(&self, other: &Wire) -> bool {
        self.conductor == other.conductor && self.ends == other.ends
    }

    /// Its wire number's text, if it has one.
    pub fn number_text(&self) -> Option<&str> {
        self.number.as_ref().map(|n| n.text.as_str())
    }

    /// Whether its wire number is fixed by hand.
    pub fn number_fixed(&self) -> bool {
        self.number.as_ref().is_some_and(|n| n.fixed)
    }

    /// Its potential's name, if it has one.
    pub fn potential_name(&self) -> Option<&str> {
        self.potential.as_ref().map(Text::as_str)
    }

    /// Its own values in words, in a fixed order (`("wire number", "24V-07")`), the ones it sets:
    /// for change lists, diffs and the inspector.
    pub fn values(&self) -> Vec<(&'static str, String)> {
        let mut out = vec![("net", self.net.to_string()), ("conductor", self.conductor.to_string()), ("net order", self.net_order.to_string())];
        if self.order_fixed {
            out.push(("order fixed", "yes".into()));
        }
        if let Some(n) = &self.number {
            out.push(("wire number", n.text.to_string()));
            if n.fixed {
                out.push(("wire number fixed", "yes".into()));
            }
        }
        out.extend(self.potential.as_ref().map(|p| ("potential", p.to_string())));
        out.extend(self.wire_type.as_ref().map(|t| ("wire type", t.text())));
        out.extend(self.size.map(|s| ("size", s.to_string())));
        out.extend(self.colours.as_ref().map(|c| ("colours", c.to_string())));
        out.extend(self.rating.values());
        out.extend(self.load.map(|a| ("load", a.to_string())));
        out
    }
}

/// A wire number and whether a person fixed it (design 022 §5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireNumber {
    pub text: Text<MAX_VALUE_CHARS>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fixed: bool,
}

/// A conductor size: a cross-section in mm², or an American wire gauge.
///
/// Stored as its words: `2.5` (mm²), `18 AWG`, `2/0 AWG`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WireSize {
    Mm2(Mm2),
    Awg(Awg),
}

/// The smallest and largest cross-section stored, in mm².
pub const MM2_RANGE: std::ops::RangeInclusive<f64> = 0.05..=1000.0;

impl WireSize {
    /// The size `text` names: `2.5`, `2.5 mm²`, `2.5mm2`, `18 AWG`, `AWG 18`, `4/0 AWG`.
    ///
    /// # Example
    /// ```
    /// use ecad_model::objects::wire::WireSize;
    /// assert_eq!(WireSize::parse("2.5 mm²").map(|s| s.to_string()), Some("2.5".into()));
    /// assert_eq!(WireSize::parse("awg18").map(|s| s.to_string()), Some("18 AWG".into()));
    /// assert_eq!(WireSize::parse("2/0 AWG").map(|s| s.to_string()), Some("2/0 AWG".into()));
    /// assert_eq!(WireSize::parse("thick"), None);
    /// ```
    pub fn parse(text: &str) -> Option<WireSize> {
        let t = text.trim();
        if t.chars().count() > MAX_VALUE_CHARS {
            return None;
        }
        let lower = t.to_ascii_lowercase();
        if let Some(g) = lower.strip_suffix("awg").or_else(|| lower.strip_prefix("awg")) {
            return Awg::parse(g.trim()).map(WireSize::Awg);
        }
        let n = ["mm\u{b2}", "mm2", "mm^2", "mm"].iter().find_map(|u| lower.strip_suffix(u)).unwrap_or(&lower).trim();
        let n = n.replace(',', ".");
        let (whole, frac) = n.split_once('.').unwrap_or((&n, "0"));
        if !is_digits(whole) || !is_digits(frac) {
            return None;
        }
        let x: f64 = n.parse().ok()?;
        (x.is_finite() && MM2_RANGE.contains(&x)).then_some(WireSize::Mm2(Mm2(x)))
    }

    /// The cross-section in mm², when it is given in mm².
    pub fn mm2(self) -> Option<f64> {
        match self {
            WireSize::Mm2(x) => Some(x.0),
            WireSize::Awg(_) => None,
        }
    }
}

impl fmt::Display for WireSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WireSize::Mm2(x) => write!(f, "{}", x.0),
            WireSize::Awg(g) => write!(f, "{g} AWG"),
        }
    }
}

impl Serialize for WireSize {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

/// Strict: only the stored form reads.
impl<'de> Deserialize<'de> for WireSize {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        WireSize::parse(&text)
            .filter(|s| s.to_string() == text)
            .ok_or_else(|| serde::de::Error::custom(format!("`{}` is not a wire size (`2.5`, `18 AWG`)", text.escape_debug())))
    }
}

impl schemars::JsonSchema for WireSize {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        "WireSize".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type": "string", "maxLength": MAX_VALUE_CHARS})
    }
}

/// An American wire gauge: 40 (thinnest) to 1, then `1/0` to `4/0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Awg(i8);

impl Awg {
    /// Gauge `n` (1 to 40).
    pub fn gauge(n: u8) -> Option<Awg> {
        i8::try_from(n).ok().filter(|g| (1..=40).contains(g)).map(Awg)
    }

    /// Gauge `n/0` (1 to 4).
    pub fn aught(n: u8) -> Option<Awg> {
        i8::try_from(n).ok().filter(|g| (1..=4).contains(g)).map(|g| Awg(1 - g))
    }

    fn parse(t: &str) -> Option<Awg> {
        let small = |s: &str| uint(s, 40).and_then(|n| u8::try_from(n).ok());
        if let Some(n) = t.strip_suffix("/0") {
            return small(n).and_then(Awg::aught);
        }
        if t.len() > 1 && t.chars().all(|c| c == '0') {
            return u8::try_from(t.len()).ok().and_then(Awg::aught);
        }
        small(t).and_then(|n| if n == 0 { Awg::aught(1) } else { Awg::gauge(n) })
    }
}

impl fmt::Display for Awg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 >= 1 { write!(f, "{}", self.0) } else { write!(f, "{}/0", 1 - self.0) }
    }
}

/// A wire's or a core's colours: an ordered list of base colours (green/yellow is
/// `[Green, Yellow]`), written `GN/YE` (design 072 §4.6, F35).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Colours(Vec<ConductorColour>);

/// Most base colours in one list.
pub const MAX_COLOURS: usize = 3;

impl Colours {
    /// A list of 1 to [`MAX_COLOURS`] colours.
    pub fn new(list: Vec<ConductorColour>) -> Option<Colours> {
        (!list.is_empty() && list.len() <= MAX_COLOURS).then_some(Colours(list))
    }

    /// The colours `text` names, in any form and case, separated by `/`, `-`, `+`, spaces or
    /// commas; two- or three-letter codes may run together (`GNYE`, `GRNYEL`).
    ///
    /// # Example
    /// ```
    /// use ecad_model::objects::wire::Colours;
    /// assert_eq!(Colours::parse("GNYE").map(|c| c.to_string()), Some("GN/YE".into()));
    /// assert_eq!(Colours::parse("green / yellow").map(|c| c.to_string()), Some("GN/YE".into()));
    /// assert_eq!(Colours::parse("blu").map(|c| c.to_string()), Some("BU".into()));
    /// assert_eq!(Colours::parse("GRNYEL").map(|c| c.to_string()), Some("GN/YE".into()));
    /// assert_eq!(Colours::parse("tartan"), None);
    /// ```
    pub fn parse(text: &str) -> Option<Colours> {
        let t = text.trim();
        if t.is_empty() || t.chars().count() > MAX_VALUE_CHARS {
            return None;
        }
        let mut list = Vec::new();
        for part in t.split(['/', '-', '+', ' ', ',']).filter(|p| !p.is_empty()) {
            if let Some(c) = ConductorColour::parse(part) {
                list.push(c);
                continue;
            }
            // Codes of one length run together (`GNYE`, `GRNYEL`).
            let chars: Vec<char> = part.chars().collect();
            let run = |n: usize| -> Option<Vec<ConductorColour>> {
                if !chars.len().is_multiple_of(n) {
                    return None;
                }
                chars
                    .chunks(n)
                    .map(|code| {
                        let code: String = code.iter().collect();
                        let form = |c: &ConductorColour| if n == 2 { &c.forms().code2 } else { &c.forms().code3 };
                        ConductorColour::ALL.iter().copied().find(|c| form(c).eq_ignore_ascii_case(&code))
                    })
                    .collect()
            };
            list.extend(run(2).or_else(|| run(3))?);
        }
        Colours::new(list)
    }

    /// The colours, in order.
    pub fn list(&self) -> &[ConductorColour] {
        &self.0
    }

    /// As drawings, labels and reports show it, in the project's colour words (`GN/YE`,
    /// `GRN/YEL`, `Green/Yellow`).
    pub fn shown(&self, words: ColourWords) -> String {
        self.0.iter().map(|c| c.shown(words)).collect::<Vec<_>>().join("/")
    }
}

impl fmt::Display for Colours {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.shown(ColourWords::Code2))
    }
}

impl Serialize for Colours {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

/// Strict: only the stored form (2-letter codes joined by `/`) reads.
impl<'de> Deserialize<'de> for Colours {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Colours::parse(&text)
            .filter(|c| c.to_string() == text)
            .ok_or_else(|| serde::de::Error::custom(format!("`{}` is not a colour list (`BU`, `GN/YE`)", text.escape_debug())))
    }
}

impl schemars::JsonSchema for Colours {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Colours".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type": "string", "pattern": "^[A-Z]{2}(/[A-Z]{2}){0,2}$"})
    }
}

crate::word_enum! {
    /// How a wire's or cable's current capacity Iz is found (design 057 §3).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Basis {
        /// The wire type's or cable type's rated current, derated.
        Datasheet,
        /// The project's rating table.
        Table,
        /// A value typed on the wire or cable ([`CircuitInputs::manual`]).
        Manual,
    }
}

crate::word_enum! {
    /// A cable's installation method (IEC 60364-5-52 reference methods).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum InstallMethod {
        #[strum(serialize = "A1")]
        A1,
        #[strum(serialize = "A2")]
        A2,
        #[strum(serialize = "B1")]
        B1,
        #[strum(serialize = "B2")]
        B2,
        #[strum(serialize = "C")]
        C,
        #[strum(serialize = "D1")]
        D1,
        #[strum(serialize = "D2")]
        D2,
        #[strum(serialize = "E")]
        E,
        #[strum(serialize = "F")]
        F,
        #[strum(serialize = "G")]
        G,
    }
}

crate::word_enum! {
    /// The supply a circuit is on, for the volt drop and the loaded conductors.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum SupplySystem {
        /// Single-phase AC: two loaded conductors.
        Ac1,
        /// Three-phase AC: three loaded conductors.
        Ac3,
        /// DC: two loaded conductors.
        Dc,
    }
}

impl SupplySystem {
    /// The loaded conductors of the circuit.
    pub fn loaded(self) -> u8 {
        match self {
            SupplySystem::Ac3 => 3,
            SupplySystem::Ac1 | SupplySystem::Dc => 2,
        }
    }
}

/// A wire's bundling or a cable's grouping (design 057 §2.1): a factor, or a circuit count the
/// rating table turns into one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Bundling {
    Factor(f64),
    Circuits(u8),
}

/// The bounds of every rating input (design 057 §2).
pub mod bounds {
    use std::ops::RangeInclusive;

    /// Iz typed by hand, and a load, in amperes (0 is refused).
    pub const AMPS: RangeInclusive<f64> = 0.0..=6300.0;
    /// A bundling or grouping factor.
    pub const FACTOR: RangeInclusive<f64> = 0.30..=1.00;
    /// A circuit count.
    pub const CIRCUITS: RangeInclusive<u8> = 1..=20;
    /// An ambient temperature, °C.
    pub const AMBIENT: RangeInclusive<f64> = -40.0..=90.0;
    /// A supply voltage.
    pub const VOLTS: RangeInclusive<f64> = 1.0..=1000.0;
    /// A volt-drop limit, %.
    pub const VD_LIMIT: RangeInclusive<f64> = 0.5..=20.0;
}

/// Whether `a` is a current within [`bounds::AMPS`] (above 0).
pub fn amps_ok(a: Amps) -> bool {
    a.0.is_finite() && a.0 > 0.0 && bounds::AMPS.contains(&a.0)
}

impl Bundling {
    /// Whether it is within its bounds.
    pub fn ok(self) -> bool {
        match self {
            Bundling::Factor(f) => f.is_finite() && bounds::FACTOR.contains(&f),
            Bundling::Circuits(n) => bounds::CIRCUITS.contains(&n),
        }
    }
}

/// The protective device a wire or a cable names (design 072 §4.5): a device, and the fuse or
/// pole of a multi-way device (`-F01:3` is position 3). Deleting the device clears every
/// `protected_by` that names it in the same transaction; one read from a file that names nothing
/// is a check row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedBy {
    pub device: DeviceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<u8>,
}

/// The rating inputs a wire or a cable holds of its own (design 057 §2, design 072 §4.2, §4.5):
/// each overrides the project default.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, try_from = "RawInputs")]
pub struct CircuitInputs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<Basis>,
    /// Iz typed by hand, for [`Basis::Manual`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual: Option<Amps>,
    /// A wire's bundling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundling: Option<Bundling>,
    /// A cable's installation method.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<InstallMethod>,
    /// A cable's ambient temperature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ambient: Option<Celsius>,
    /// A cable's grouped circuit count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grouped: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voltage: Option<Volts>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<SupplySystem>,
    /// A cable's volt-drop limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vd_limit: Option<Percent>,
}

impl CircuitInputs {
    /// Whether it sets nothing.
    pub fn is_empty(&self) -> bool {
        *self == CircuitInputs::default()
    }

    /// The inputs it sets, in words (`("rating basis", "manual")`), in a fixed order.
    pub fn values(&self) -> Vec<(&'static str, String)> {
        use crate::naming::words::Word;
        let mut out = Vec::new();
        out.extend(self.basis.map(|b| ("rating basis", b.word().to_string())));
        out.extend(self.manual.map(|a| ("rating manual", a.to_string())));
        out.extend(self.bundling.map(|b| {
            let t = match b {
                Bundling::Factor(f) => format!("{f:.2}"),
                Bundling::Circuits(n) => format!("{n} circuits"),
            };
            ("rating bundling", t)
        }));
        out.extend(self.method.map(|m| ("rating method", m.word().to_string())));
        out.extend(self.ambient.map(|c| ("rating ambient", c.to_string())));
        out.extend(self.grouped.map(|n| ("rating grouped", n.to_string())));
        out.extend(self.voltage.map(|v| ("rating voltage", v.to_string())));
        out.extend(self.system.map(|s| ("rating system", s.word().to_string())));
        out.extend(self.vd_limit.map(|p| ("rating volt-drop limit", p.to_string())));
        out
    }

    /// The first input out of its bounds, in words, or `None`.
    pub fn out_of_bounds(&self) -> Option<&'static str> {
        let within = |x: Option<f64>, r: &std::ops::RangeInclusive<f64>| x.is_none_or(|x| x.is_finite() && r.contains(&x));
        if !self.manual.is_none_or(amps_ok) {
            return Some("manual");
        }
        if !self.bundling.is_none_or(Bundling::ok) {
            return Some("bundling");
        }
        if !within(self.ambient.map(|c| c.0), &bounds::AMBIENT) {
            return Some("ambient");
        }
        if !self.grouped.is_none_or(|n| bounds::CIRCUITS.contains(&n)) {
            return Some("grouped");
        }
        if !within(self.voltage.map(|v| v.0), &bounds::VOLTS) {
            return Some("voltage");
        }
        if !within(self.vd_limit.map(|p| p.0), &bounds::VD_LIMIT) {
            return Some("vd_limit");
        }
        None
    }
}

/// [`CircuitInputs`] as written, checked against the bounds on reading.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawInputs {
    #[serde(default)]
    basis: Option<Basis>,
    #[serde(default)]
    manual: Option<Amps>,
    #[serde(default)]
    bundling: Option<Bundling>,
    #[serde(default)]
    method: Option<InstallMethod>,
    #[serde(default)]
    ambient: Option<Celsius>,
    #[serde(default)]
    grouped: Option<u8>,
    #[serde(default)]
    voltage: Option<Volts>,
    #[serde(default)]
    system: Option<SupplySystem>,
    #[serde(default)]
    vd_limit: Option<Percent>,
}

/// A stored rating input out of its bounds: the input's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("rating input `{0}` is out of its bounds")]
pub struct OutOfBounds(pub &'static str);

impl TryFrom<RawInputs> for CircuitInputs {
    type Error = OutOfBounds;

    fn try_from(r: RawInputs) -> Result<Self, OutOfBounds> {
        let c = CircuitInputs {
            basis: r.basis,
            manual: r.manual,
            bundling: r.bundling,
            method: r.method,
            ambient: r.ambient,
            grouped: r.grouped,
            voltage: r.voltage,
            system: r.system,
            vd_limit: r.vd_limit,
        };
        match c.out_of_bounds() {
            Some(field) => Err(OutOfBounds(field)),
            None => Ok(c),
        }
    }
}
