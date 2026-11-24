//! A protective device's rating: typed fields per kind, never a stored display string.
//!
//! # Semantics
//! Each kind holds only the fields it has: a fuse its rated current, class, size ([`FuseSize`]),
//! breaking capacity and rated voltage; a fuse terminal a fuse's; an MCB its rated current, curve,
//! breaking capacity, poles and rated voltage; an RCD its rated current, residual current and
//! type ([`ResidualType`]), conditional short-circuit current (`Inc`: an RCD breaks no fault of
//! its own, so its fault rating is with the device before it), poles and rated voltage; an RCBO an
//! MCB's fields and a residual current and type; a fuse bank a fuse per output.
//! Every field may be empty (not typed yet); [`Protection::is_unrated`] says when all are.
//! [`fmt::Display`] writes the set fields in a fixed order (`curve B, 16 A, 6 kA, 2P, 230 V AC`),
//! which is what the drawing and every command's answer show.
//!
//! This module excludes reading a typed rating text (`ecad-electrical`'s rating reader) and the
//! protection calculations.
#![deny(clippy::wildcard_enum_match_arm)]

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::DeviceType;
use crate::objects::devices::fields::FieldType;
use crate::objects::devices::{FieldSource, RatingField};

use crate::geometry::units::{Amps, KiloAmps, MilliAmps, RatedVoltage};

/// The largest rated current a protective device takes, in amperes.
pub const MAX_AMPS: f64 = 6300.0;
/// The largest residual current an RCD takes, in milliamperes.
pub const MAX_RESIDUAL_MA: f64 = 3000.0;
/// The largest breaking capacity, in kiloamperes.
pub const MAX_BREAKING_KA: f64 = 200.0;
/// The highest rated voltage of a protective device, in volts (low voltage, DC included).
pub const MAX_VOLTS: f64 = 1500.0;
/// The highest fuse output number of a fuse bank.
pub(crate) const MAX_OUTPUT: u8 = 99;

/// A protective device and its rating, by kind.
///
/// # Example
/// ```
/// use ecad_model::objects::devices::{Breaker, Protection, TripCurve};
/// use ecad_model::geometry::units::Amps;
/// let p = Protection::Mcb(Breaker { rated_current: Some(Amps(16.0)), curve: Some(TripCurve::B), ..Breaker::default() });
/// assert_eq!(p.to_string(), "curve B, 16 A");
/// assert_eq!(serde_json::to_string(&p).unwrap(), r#"{"mcb":{"rated_current":16.0,"curve":"b"}}"#);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protection {
    /// A fuse.
    Fuse(Fuse),
    /// A fuse terminal: a fuse in a DIN-rail terminal housing, set by the `fuseterminalblock` starter.
    FuseTerminalBlock(Fuse),
    /// A miniature circuit breaker.
    Mcb(Breaker),
    /// A residual current device (no overcurrent protection).
    Rcd(Rcd),
    /// A residual current breaker with overcurrent protection.
    Rcbo(Rcbo),
    /// A bank of fuses, one per output.
    FuseBank(FuseBank),
}

impl Default for Protection {
    fn default() -> Protection {
        Protection::Fuse(Fuse::default())
    }
}

/// The kind of a [`Protection`], without its data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProtectionKind {
    Fuse,
    FuseTerminalBlock,
    Mcb,
    Rcd,
    Rcbo,
    FuseBank,
}

impl ProtectionKind {
    /// The kind as a person reads it (`MCB`).
    pub fn label(self) -> &'static str {
        match self {
            ProtectionKind::Fuse => "fuse",
            ProtectionKind::FuseTerminalBlock => "fuse terminal",
            ProtectionKind::Mcb => "MCB",
            ProtectionKind::Rcd => "RCD",
            ProtectionKind::Rcbo => "RCBO",
            ProtectionKind::FuseBank => "fuse bank",
        }
    }

    /// A device of this kind with nothing rated.
    pub fn empty(self) -> Protection {
        match self {
            ProtectionKind::Fuse => Protection::Fuse(Fuse::default()),
            ProtectionKind::FuseTerminalBlock => Protection::FuseTerminalBlock(Fuse::default()),
            ProtectionKind::Mcb => Protection::Mcb(Breaker::default()),
            ProtectionKind::Rcd => Protection::Rcd(Rcd::default()),
            ProtectionKind::Rcbo => Protection::Rcbo(Rcbo::default()),
            ProtectionKind::FuseBank => Protection::FuseBank(FuseBank::default()),
        }
    }
}

impl Protection {
    /// A fuse terminal's fuse, when it is a fuse terminal (the `fuseterminalblock` starter).
    pub fn as_fuse_terminal_block(&self) -> Option<&Fuse> {
        match self {
            Protection::FuseTerminalBlock(f) => Some(f),
            Protection::Fuse(_) | Protection::Mcb(_) | Protection::Rcd(_) | Protection::Rcbo(_) | Protection::FuseBank(_) => None,
        }
    }

    /// Its kind.
    pub fn kind(&self) -> ProtectionKind {
        match self {
            Protection::Fuse(_) => ProtectionKind::Fuse,
            Protection::FuseTerminalBlock(_) => ProtectionKind::FuseTerminalBlock,
            Protection::Mcb(_) => ProtectionKind::Mcb,
            Protection::Rcd(_) => ProtectionKind::Rcd,
            Protection::Rcbo(_) => ProtectionKind::Rcbo,
            Protection::FuseBank(_) => ProtectionKind::FuseBank,
        }
    }

    /// Whether no rating field is set (a fuse bank: no output rated). Its contacts are not
    /// rating fields.
    pub fn is_unrated(&self) -> bool {
        *self == self.unrated()
    }

    /// The same device with nothing rated: its kind and contacts kept.
    pub fn unrated(&self) -> Protection {
        let mut out = self.kind().empty();
        if let (Some(c), Some(to)) = (self.contacts(), out.contacts_mut()) {
            *to = c.clone();
        }
        out
    }

    /// Its auxiliary contacts, when its kind has them (a breaker, an RCD, an RCBO).
    pub fn contacts(&self) -> Option<&crate::objects::devices::ContactSet> {
        match self {
            Protection::Mcb(b) => Some(&b.contacts),
            Protection::Rcd(r) => Some(&r.contacts),
            Protection::Rcbo(r) => Some(&r.contacts),
            Protection::Fuse(_) | Protection::FuseTerminalBlock(_) | Protection::FuseBank(_) => None,
        }
    }

    /// Its auxiliary contacts, to change, when its kind has them.
    pub fn contacts_mut(&mut self) -> Option<&mut crate::objects::devices::ContactSet> {
        match self {
            Protection::Mcb(b) => Some(&mut b.contacts),
            Protection::Rcd(r) => Some(&mut r.contacts),
            Protection::Rcbo(r) => Some(&mut r.contacts),
            Protection::Fuse(_) | Protection::FuseTerminalBlock(_) | Protection::FuseBank(_) => None,
        }
    }

    /// The rated current of the device, or of a fuse bank's output `output`.
    pub fn rated_current(&self, output: Option<u8>) -> Option<Amps> {
        match (self, output) {
            (Protection::FuseBank(b), Some(n)) => b.outputs.get(&n).and_then(|f| f.rated_current),
            (Protection::FuseBank(_), None) => None,
            (Protection::Fuse(f) | Protection::FuseTerminalBlock(f), _) => f.rated_current,
            (Protection::Mcb(b), _) => b.rated_current,
            (Protection::Rcd(r), _) => r.rated_current,
            (Protection::Rcbo(r), _) => r.rated_current,
        }
    }

    /// Why it is not well formed, if it is not: a value out of range or not a number, or a fuse
    /// bank output out of range.
    pub fn problem(&self) -> Option<String> {
        let amps = |a: Option<Amps>| a.is_some_and(|a| !(a.0.is_finite() && a.0 > 0.0 && a.0 <= MAX_AMPS));
        let ka = |k: Option<KiloAmps>| k.is_some_and(|k| !(k.0.is_finite() && k.0 > 0.0 && k.0 <= MAX_BREAKING_KA));
        let ma = |m: Option<MilliAmps>| m.is_some_and(|m| !(m.0.is_finite() && m.0 > 0.0 && m.0 <= MAX_RESIDUAL_MA));
        let volts = |v: Option<RatedVoltage>| v.is_some_and(|v| !(v.volts.0.is_finite() && v.volts.0 > 0.0 && v.volts.0 <= MAX_VOLTS));
        let fuse = |f: &Fuse| amps(f.rated_current) || ka(f.breaking) || volts(f.rated_voltage);
        let bad = match self {
            Protection::Fuse(f) | Protection::FuseTerminalBlock(f) => fuse(f),
            Protection::Mcb(b) => amps(b.rated_current) || ka(b.breaking) || volts(b.rated_voltage),
            Protection::Rcd(r) => amps(r.rated_current) || ma(r.residual) || ka(r.conditional_short_circuit) || volts(r.rated_voltage),
            Protection::Rcbo(r) => amps(r.rated_current) || ka(r.breaking) || ma(r.residual) || volts(r.rated_voltage),
            Protection::FuseBank(b) => {
                if b.outputs.keys().any(|n| *n == 0 || *n > MAX_OUTPUT) || b.ways.is_some_and(|w| w == 0 || w > MAX_OUTPUT) {
                    return Some(format!("a fuse bank's outputs are 1 to {MAX_OUTPUT}"));
                }
                if let Some(w) = b.ways
                    && b.outputs.keys().any(|n| *n > w)
                {
                    return Some(format!("a fuse bank of {w} ways rates an output past its last"));
                }
                b.outputs.values().any(fuse)
            }
        };
        if bad {
            return Some(format!("a rating value of a {} is out of range", self.kind().label()));
        }
        self.contacts().and_then(crate::objects::devices::ContactSet::problem)
    }
}

impl fmt::Display for Protection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&super::join_parts(&DeviceType::Protection(self.clone()).rating_parts()))
    }
}

/// Writes `parts` joined by `, `.
fn join(f: &mut fmt::Formatter<'_>, parts: &[Option<String>]) -> fmt::Result {
    let shown: Vec<&str> = parts.iter().flatten().map(String::as_str).collect();
    f.write_str(&shown.join(", "))
}

fn curve_words(c: Option<TripCurve>) -> Option<String> {
    c.map(|c| format!("curve {}", c.letter()))
}

fn show<T: fmt::Display>(x: Option<T>) -> Option<String> {
    x.map(|x| x.to_string())
}

fn residual_type_words(r: Option<ResidualType>) -> Option<String> {
    r.map(|r| format!("type {}", r.label()))
}

fn inc_words(k: Option<KiloAmps>) -> Option<String> {
    k.map(|k| format!("Inc {k}"))
}

/// A fuse's rating: `6.3 A, T`, `16 A, gG, 10x38, 50 kA, 400 V AC`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fuse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_current: Option<Amps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<FuseClass>,
    /// Its size, which picks the holder and the spare.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<FuseSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breaking: Option<KiloAmps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_voltage: Option<RatedVoltage>,
}

impl fmt::Display for Fuse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        join(
            f,
            &[
                show(self.rated_current),
                self.class.map(|c| c.label().to_string()),
                self.size.map(|s| s.label().to_string()),
                show(self.breaking),
                show(self.rated_voltage),
            ],
        )
    }
}

/// An MCB's rating: `curve B, 16 A, 6 kA, 2P`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Breaker {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_current: Option<Amps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve: Option<TripCurve>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breaking: Option<KiloAmps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poles: Option<Poles>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_voltage: Option<RatedVoltage>,
    /// Its auxiliary contacts.
    #[serde(default, skip_serializing_if = "super::ContactSet::is_empty")]
    pub contacts: crate::objects::devices::ContactSet,
}

impl fmt::Display for Breaker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        join(
            f,
            &[
                curve_words(self.curve),
                show(self.rated_current),
                show(self.breaking),
                self.poles.map(|p| p.label().to_string()),
                show(self.rated_voltage),
            ],
        )
    }
}

/// An RCD's rating: `40 A, 30 mA, type A, Inc 10 kA, 4P, 400 V AC`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rcd {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_current: Option<Amps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub residual: Option<MilliAmps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub residual_type: Option<ResidualType>,
    /// Its conditional short-circuit current (`Inc`), with the device named before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditional_short_circuit: Option<KiloAmps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poles: Option<Poles>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_voltage: Option<RatedVoltage>,
    /// Its auxiliary contacts.
    #[serde(default, skip_serializing_if = "super::ContactSet::is_empty")]
    pub contacts: crate::objects::devices::ContactSet,
}

impl fmt::Display for Rcd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        join(
            f,
            &[
                show(self.rated_current),
                show(self.residual),
                residual_type_words(self.residual_type),
                inc_words(self.conditional_short_circuit),
                self.poles.map(|p| p.label().to_string()),
                show(self.rated_voltage),
            ],
        )
    }
}

/// An RCBO's rating: `curve C, 13 A, 30 mA, type A, 6 kA, 2P, 230 V AC`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rcbo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_current: Option<Amps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve: Option<TripCurve>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub residual: Option<MilliAmps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub residual_type: Option<ResidualType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breaking: Option<KiloAmps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poles: Option<Poles>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_voltage: Option<RatedVoltage>,
    /// Its auxiliary contacts.
    #[serde(default, skip_serializing_if = "super::ContactSet::is_empty")]
    pub contacts: crate::objects::devices::ContactSet,
}

impl fmt::Display for Rcbo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        join(
            f,
            &[
                curve_words(self.curve),
                show(self.rated_current),
                show(self.residual),
                residual_type_words(self.residual_type),
                show(self.breaking),
                self.poles.map(|p| p.label().to_string()),
                show(self.rated_voltage),
            ],
        )
    }
}

/// A fuse bank: how many ways (outputs) it has, and each output's fuse rating, by output number
/// (`1`…).
///
/// # Semantics
/// - `ways` is taken from the head symbol when the bank is placed and read from here afterwards;
///   a head symbol that draws another count is a Checks row, never a silent change. `None` (a
///   project saved before it was stored) falls back to the head symbol.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FuseBank {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ways: Option<u8>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub outputs: BTreeMap<u8, Fuse>,
}

impl fmt::Display for FuseBank {
    /// Each rated output: `1: 4 A; 2: 6.3 A, T`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.outputs.iter().map(|(n, x)| format!("{n}: {x}")).collect();
        f.write_str(&parts.join("; "))
    }
}

crate::word_enum! {
    /// An MCB's or RCBO's tripping curve (IEC 60898-1, and K and Z).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum TripCurve {
        B,
        C,
        D,
        K,
        Z,
    }
}

impl TripCurve {
    /// Its letter: `B`.
    pub fn letter(self) -> &'static str {
        match self {
            TripCurve::B => "B",
            TripCurve::C => "C",
            TripCurve::D => "D",
            TripCurve::K => "K",
            TripCurve::Z => "Z",
        }
    }
}

crate::word_enum! {
    /// A fuse's class: a miniature fuse's speed (F fast, M medium, T time-lag) or an IEC 60269
    /// utilisation category (gG, aM).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum FuseClass {
        F,
        M,
        T,
        Gg,
        Am,
    }
}

impl FuseClass {
    /// As written on a drawing: `T`, `gG`.
    pub fn label(self) -> &'static str {
        match self {
            FuseClass::F => "F",
            FuseClass::M => "M",
            FuseClass::T => "T",
            FuseClass::Gg => "gG",
            FuseClass::Am => "aM",
        }
    }
}

crate::word_enum! {
    /// A fuse's size (IEC 60269): a cartridge's diameter by length in millimetres (`10x38`) or an
    /// NH blade fuse's size (`NH00`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum FuseSize {
        #[strum(serialize = "5x20")]
        Mm5x20,
        #[strum(serialize = "6_3x32")]
        Mm6x32,
        #[strum(serialize = "10x38")]
        Mm10x38,
        #[strum(serialize = "14x51")]
        Mm14x51,
        #[strum(serialize = "22x58")]
        Mm22x58,
        Nh000,
        Nh00,
        Nh0,
        Nh1,
        Nh2,
        Nh3,
        Nh4,
    }
}

impl FuseSize {
    /// As written on a drawing: `10x38`, `6.3x32`, `NH00`.
    pub fn label(self) -> &'static str {
        match self {
            FuseSize::Mm5x20 => "5x20",
            FuseSize::Mm6x32 => "6.3x32",
            FuseSize::Mm10x38 => "10x38",
            FuseSize::Mm14x51 => "14x51",
            FuseSize::Mm22x58 => "22x58",
            FuseSize::Nh000 => "NH000",
            FuseSize::Nh00 => "NH00",
            FuseSize::Nh0 => "NH0",
            FuseSize::Nh1 => "NH1",
            FuseSize::Nh2 => "NH2",
            FuseSize::Nh3 => "NH3",
            FuseSize::Nh4 => "NH4",
        }
    }

    /// The size `text` names, by its word or its label, case aside (`10x38`, `6.3x32`, `nh00`).
    pub fn read(text: &str) -> Option<FuseSize> {
        let t = text.trim();
        <FuseSize as crate::Word>::parse(t).or_else(|| <FuseSize as crate::Word>::ALL.iter().copied().find(|s| s.label().eq_ignore_ascii_case(t)))
    }
}

crate::word_enum! {
    /// What residual current an RCD or RCBO detects (IEC 60755, IEC 62423): `AC` sinusoidal only,
    /// `A` also pulsating DC, `F` also mixed frequencies (a single-phase drive), `B` also smooth DC
    /// (a three-phase drive, a DC load).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum ResidualType {
        Ac,
        A,
        F,
        B,
    }
}

impl ResidualType {
    /// As written on a drawing: `AC`, `A`.
    pub fn label(self) -> &'static str {
        match self {
            ResidualType::Ac => "AC",
            ResidualType::A => "A",
            ResidualType::F => "F",
            ResidualType::B => "B",
        }
    }
}

crate::word_enum! {
    /// A protective device's poles: `1P`, `1P+N`, `2P`, `3P`, `3P+N`, `4P`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Poles {
        #[strum(serialize = "1p")]
        One,
        #[strum(serialize = "1p+n")]
        OneN,
        #[strum(serialize = "2p")]
        Two,
        #[strum(serialize = "3p")]
        Three,
        #[strum(serialize = "3p+n")]
        ThreeN,
        #[strum(serialize = "4p")]
        Four,
    }
}

impl Poles {
    /// As written on a drawing: `1P+N`.
    pub fn label(self) -> &'static str {
        match self {
            Poles::One => "1P",
            Poles::OneN => "1P+N",
            Poles::Two => "2P",
            Poles::Three => "3P",
            Poles::ThreeN => "3P+N",
            Poles::Four => "4P",
        }
    }
}

/// A protective device's rating fields, in the rating line's order; each kind shows the ones it
/// has. A fuse bank's `outputs` (`1: 4 A; 2: 6.3 A, T`) is written per output (`rating.<n>`).
pub(super) static FIELDS: &[RatingField] = &[
    rating_field!(
        "curve",
        "Trip curve",
        Plate,
        TripCurve,
        show |c: TripCurve| format!("curve {}", c.letter()),
        after ["curve"],
        DeviceType::Protection(Protection::Mcb(x)) => x.curve,
        DeviceType::Protection(Protection::Rcbo(x)) => x.curve
    ),
    rating_field!(
        "current",
        "Rated current",
        Plate,
        Amps,
        DeviceType::Protection(Protection::Fuse(x) | Protection::FuseTerminalBlock(x)) => x.rated_current,
        DeviceType::Protection(Protection::Mcb(x)) => x.rated_current,
        DeviceType::Protection(Protection::Rcd(x)) => x.rated_current,
        DeviceType::Protection(Protection::Rcbo(x)) => x.rated_current
    ),
    rating_field!(
        "class",
        "Fuse class",
        Plate,
        FuseClass,
        DeviceType::Protection(Protection::Fuse(x) | Protection::FuseTerminalBlock(x)) => x.class
    ),
    rating_field!(
        "size",
        "Fuse size",
        Plate,
        FuseSize,
        DeviceType::Protection(Protection::Fuse(x) | Protection::FuseTerminalBlock(x)) => x.size
    ),
    rating_field!(
        "residual",
        "Residual current",
        Plate,
        MilliAmps,
        DeviceType::Protection(Protection::Rcd(x)) => x.residual,
        DeviceType::Protection(Protection::Rcbo(x)) => x.residual
    ),
    rating_field!(
        "residual_type",
        "Residual type",
        Plate,
        ResidualType,
        show |r: ResidualType| format!("type {}", r.label()),
        after ["type"],
        DeviceType::Protection(Protection::Rcd(x)) => x.residual_type,
        DeviceType::Protection(Protection::Rcbo(x)) => x.residual_type
    ),
    rating_field!(
        "breaking",
        "Breaking capacity",
        Plate,
        KiloAmps,
        DeviceType::Protection(Protection::Fuse(x) | Protection::FuseTerminalBlock(x)) => x.breaking,
        DeviceType::Protection(Protection::Mcb(x)) => x.breaking,
        DeviceType::Protection(Protection::Rcbo(x)) => x.breaking
    ),
    rating_field!(
        "conditional_short_circuit",
        "Conditional short-circuit current",
        Plate,
        KiloAmps,
        show |k: KiloAmps| format!("Inc {k}"),
        after ["inc"],
        DeviceType::Protection(Protection::Rcd(x)) => x.conditional_short_circuit
    ),
    rating_field!(
        "poles",
        "Poles",
        Plate,
        Poles,
        DeviceType::Protection(Protection::Mcb(x)) => x.poles,
        DeviceType::Protection(Protection::Rcd(x)) => x.poles,
        DeviceType::Protection(Protection::Rcbo(x)) => x.poles
    ),
    rating_field!(
        "voltage",
        "Rated voltage",
        Plate,
        RatedVoltage,
        DeviceType::Protection(Protection::Fuse(x) | Protection::FuseTerminalBlock(x)) => x.rated_voltage,
        DeviceType::Protection(Protection::Mcb(x)) => x.rated_voltage,
        DeviceType::Protection(Protection::Rcd(x)) => x.rated_voltage,
        DeviceType::Protection(Protection::Rcbo(x)) => x.rated_voltage
    ),
    RatingField {
        word: "outputs",
        label: "Outputs",
        source: FieldSource::Plate,
        ty: FieldType::Combined,
        get: |t| if let DeviceType::Protection(Protection::FuseBank(x)) = t { Some(x.to_string()).filter(|s| !s.is_empty()) } else { None },
        set: crate::objects::devices::combined,
    },
];
