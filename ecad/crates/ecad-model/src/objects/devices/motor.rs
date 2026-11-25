//! A motor's phases and rating: typed fields, never a stored display string.
//!
//! # Semantics
//! - [`Phases`] says whether it is a single-phase or a three-phase motor. The symbol declares it.
//! - Its rating is its nameplate: the rated power (kept in watts, shown in kilowatts: `7.5 kW`),
//!   the rated voltage (`400 V`: line-to-line on three phases, line-to-neutral on one), the rated
//!   current (`15.2 A`), the rated frequency (`50 Hz`), the rated speed (`1450 rpm`), the power
//!   factor (`cos φ 0.85`), the efficiency (`eff 91 %`) and its class ([`EfficiencyClass`], `IE3`:
//!   the legal minimum, IEC 60034-30-1), the winding connection ([`WindingConnection`]: `Δ` for
//!   star-delta starting at the supply voltage), the duty ([`DutyType`], `S1`: an S2 or S3 motor must
//!   not run continuously), the insulation class ([`InsulationClass`], `class F`: a drive or a hot
//!   room needs F or above) and the degree of protection ([`IpCode`]); and how it is started
//!   ([`Starting`]: direct on line, star-delta, soft starter, VFD).
//! - Every rating field may be empty (not typed yet); [`Motor::is_unrated`] says when all are.
//!   [`fmt::Display`] writes the set fields in a fixed order
//!   (`7.5 kW, 400 V, 15.2 A, 50 Hz, 1450 rpm, cos φ 0.85, eff 91 %, IE3, Δ, S1, class F, IP55,
//!   star-delta`), which is what the drawing and every command's answer show.
//! - The rated voltage is never taken from the circuit: a supply on the motor's potentials only
//!   suggests it (`ecad-electrical`), and a check compares the two.
//!
//! This module excludes reading a typed rating text (`ecad-electrical`'s rating reader) and the
//! calculations from the rating (full-load and starting current, protection and cable sizing).
#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::DeviceType;
use crate::objects::devices::RatingField;

use crate::geometry::units::{Amps, Hertz, Percent, Rpm, Volts, Watts};
use crate::objects::devices::IpCode;
use crate::objects::devices::switching::motor_power;

/// The largest rated power of a motor, in watts (100 MW).
pub const MAX_POWER_WATTS: f64 = 100_000_000.0;
/// The largest rated current of a motor, in amperes.
pub const MAX_CURRENT_AMPS: f64 = 10_000.0;
/// The largest rated voltage of a motor, in volts.
pub const MAX_VOLTAGE_VOLTS: f64 = 100_000.0;
/// The largest rated frequency of a motor, in hertz.
pub const MAX_FREQUENCY_HERTZ: f64 = 1_000.0;
/// The largest rated speed of a motor, in revolutions per minute.
pub const MAX_SPEED_RPM: f64 = 100_000.0;

/// A motor: its phases and its nameplate rating.
///
/// # Example
/// ```
/// use ecad_model::objects::devices::{Motor, Phases, Starting};
/// use ecad_model::geometry::units::{Amps, Watts};
/// let m = Motor { rated_power: Some(Watts(7500.0)), rated_current: Some(Amps(15.2)), starting: Some(Starting::StarDelta), ..Motor::empty(Phases::Three) };
/// assert_eq!(m.to_string(), "7.5 kW, 15.2 A, star-delta");
/// assert_eq!(
///     serde_json::to_string(&m).unwrap(),
///     r#"{"phases":"three","rated_power":7500.0,"rated_current":15.2,"starting":"star_delta"}"#
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Motor {
    pub phases: Phases,
    /// The rated (shaft) power.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_power: Option<Watts>,
    /// The rated voltage: line-to-line on three phases, line-to-neutral on one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_voltage: Option<Volts>,
    /// The rated (full-load) current.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_current: Option<Amps>,
    /// The rated frequency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_frequency: Option<Hertz>,
    /// The rated speed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_speed: Option<Rpm>,
    /// The power factor at rated load (cos φ), more than 0 and at most 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_factor: Option<f64>,
    /// The efficiency at rated load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub efficiency: Option<Percent>,
    /// The efficiency class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub efficiency_class: Option<EfficiencyClass>,
    /// How its windings are connected at its rated voltage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<WindingConnection>,
    /// Its duty type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duty: Option<DutyType>,
    /// Its insulation class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insulation_class: Option<InsulationClass>,
    /// Its degree of protection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip_rating: Option<IpCode>,
    /// How it is started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starting: Option<Starting>,
}

crate::word_enum! {
    /// A motor's supply phases.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    pub enum Phases {
        Single,
        #[default]
        Three,
    }
}

crate::word_enum! {
    /// How a motor is started.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Starting {
        /// Direct on line (DOL).
        Direct,
        StarDelta,
        /// A soft starter.
        Soft,
        /// A variable-frequency drive.
        Vfd,
    }
}

impl Starting {
    /// As written on a drawing (`DOL`, `star-delta`, `soft start`, `VFD`); the rating reader reads
    /// each back.
    pub fn label(self) -> &'static str {
        match self {
            Starting::Direct => "DOL",
            Starting::StarDelta => "star-delta",
            Starting::Soft => "soft start",
            Starting::Vfd => "VFD",
        }
    }
}

crate::word_enum! {
    /// How a motor's windings are connected at its rated voltage: star (`Y`), delta (`Δ`), or
    /// either (`Y/Δ`: six terminals, `Δ 400 V / Y 690 V`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum WindingConnection {
        Star,
        Delta,
        StarDelta,
    }
}

impl WindingConnection {
    /// As written on a nameplate: `Y`, `Δ`, `Y/Δ`.
    pub fn label(self) -> &'static str {
        match self {
            WindingConnection::Star => "Y",
            WindingConnection::Delta => "Δ",
            WindingConnection::StarDelta => "Y/Δ",
        }
    }
}

crate::word_enum! {
    /// A motor's duty type (IEC 60034-1): `S1` continuous, `S2` short-time, `S3` intermittent
    /// periodic, up to `S10`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum DutyType {
        S1,
        S2,
        S3,
        S4,
        S5,
        S6,
        S7,
        S8,
        S9,
        S10,
    }
}

impl DutyType {
    /// As written on a nameplate: `S1`.
    pub fn label(self) -> &'static str {
        match self {
            DutyType::S1 => "S1",
            DutyType::S2 => "S2",
            DutyType::S3 => "S3",
            DutyType::S4 => "S4",
            DutyType::S5 => "S5",
            DutyType::S6 => "S6",
            DutyType::S7 => "S7",
            DutyType::S8 => "S8",
            DutyType::S9 => "S9",
            DutyType::S10 => "S10",
        }
    }
}

crate::word_enum! {
    /// A motor's insulation class (IEC 60034-1): `B` (130 °C), `F` (155 °C), `H` (180 °C).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum InsulationClass {
        B,
        F,
        H,
    }
}

impl InsulationClass {
    /// Its letter: `F`.
    pub fn letter(self) -> &'static str {
        match self {
            InsulationClass::B => "B",
            InsulationClass::F => "F",
            InsulationClass::H => "H",
        }
    }
}

crate::word_enum! {
    /// A motor's efficiency class (IEC 60034-30-1): `IE1` to `IE5`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum EfficiencyClass {
        Ie1,
        Ie2,
        Ie3,
        Ie4,
        Ie5,
    }
}

impl EfficiencyClass {
    /// As written on a nameplate: `IE3`.
    pub fn label(self) -> &'static str {
        match self {
            EfficiencyClass::Ie1 => "IE1",
            EfficiencyClass::Ie2 => "IE2",
            EfficiencyClass::Ie3 => "IE3",
            EfficiencyClass::Ie4 => "IE4",
            EfficiencyClass::Ie5 => "IE5",
        }
    }
}

impl Phases {
    /// As written on a drawing: `1~`, `3~`.
    pub fn symbol(self) -> &'static str {
        match self {
            Phases::Single => "1~",
            Phases::Three => "3~",
        }
    }

    /// The motor as a person reads it (`three-phase motor`).
    pub fn label(self) -> &'static str {
        match self {
            Phases::Single => "single-phase motor",
            Phases::Three => "three-phase motor",
        }
    }
}

impl Motor {
    /// A motor with `phases`, with nothing rated.
    pub fn empty(phases: Phases) -> Motor {
        Motor {
            phases,
            rated_power: None,
            rated_voltage: None,
            rated_current: None,
            rated_frequency: None,
            rated_speed: None,
            power_factor: None,
            efficiency: None,
            efficiency_class: None,
            connection: None,
            duty: None,
            insulation_class: None,
            ip_rating: None,
            starting: None,
        }
    }

    /// The same motor with nothing rated.
    pub fn unrated(&self) -> Motor {
        Motor::empty(self.phases)
    }

    /// Whether no rating field is set.
    pub fn is_unrated(&self) -> bool {
        *self == self.unrated()
    }

    /// Why it is not well formed, if it is not: a value out of range or not a number.
    pub fn problem(&self) -> Option<String> {
        let within = |x: f64, max: f64| x.is_finite() && x > 0.0 && x <= max;
        let bad = self.rated_power.is_some_and(|w| !within(w.0, MAX_POWER_WATTS))
            || self.rated_voltage.is_some_and(|v| !within(v.0, MAX_VOLTAGE_VOLTS))
            || self.rated_current.is_some_and(|a| !within(a.0, MAX_CURRENT_AMPS))
            || self.rated_frequency.is_some_and(|f| !within(f.0, MAX_FREQUENCY_HERTZ))
            || self.rated_speed.is_some_and(|n| !within(n.0, MAX_SPEED_RPM))
            || self.power_factor.is_some_and(|p| !within(p, 1.0))
            || self.efficiency.is_some_and(|e| !within(e.0, 100.0));
        bad.then(|| format!("a rating value of a {} is out of range", self.phases.label()))
    }
}

impl fmt::Display for Motor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&super::join_parts(&DeviceType::Motor(self.clone()).rating_parts()))
    }
}

/// A motor's rating fields, in the rating line's order.
pub(super) static FIELDS: &[RatingField] = &[
    rating_field!("power", "Rated power", Plate, Watts, show motor_power, after [], DeviceType::Motor(m) => m.rated_power),
    rating_field!("voltage", "Rated voltage", Plate, Volts, DeviceType::Motor(m) => m.rated_voltage),
    rating_field!("current", "Rated current", Plate, Amps, DeviceType::Motor(m) => m.rated_current),
    rating_field!("frequency", "Rated frequency", Plate, Hertz, DeviceType::Motor(m) => m.rated_frequency),
    rating_field!("speed", "Rated speed", Plate, Rpm, DeviceType::Motor(m) => m.rated_speed),
    rating_field!(
        "power_factor",
        "Power factor",
        Plate,
        f64,
        show |p: f64| format!("cos φ {p}"),
        after ["cos φ", "cosφ", "cos phi"],
        DeviceType::Motor(m) => m.power_factor
    ),
    rating_field!("efficiency", "Efficiency", Plate, Percent, show |e: Percent| format!("eff {e}"), after ["eff"], DeviceType::Motor(m) => m.efficiency),
    rating_field!("efficiency_class", "Efficiency class", Plate, EfficiencyClass, DeviceType::Motor(m) => m.efficiency_class),
    rating_field!("connection", "Connection", Plate, WindingConnection, DeviceType::Motor(m) => m.connection),
    rating_field!("duty", "Duty", Plate, DutyType, DeviceType::Motor(m) => m.duty),
    rating_field!(
        "insulation_class",
        "Insulation class",
        Plate,
        InsulationClass,
        show |c: InsulationClass| format!("class {}", c.letter()),
        after ["class"],
        DeviceType::Motor(m) => m.insulation_class
    ),
    rating_field!("ip_rating", "IP rating", Plate, IpCode, DeviceType::Motor(m) => m.ip_rating),
    rating_field!("starting", "Starting", Design, Starting, DeviceType::Motor(m) => m.starting),
];
