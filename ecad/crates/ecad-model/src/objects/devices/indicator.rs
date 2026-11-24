//! An indicator's signal, colour and power: typed fields, never a stored display string.
//!
//! # Semantics
//! - [`SignalType`] says how it signals (a steady or flashing lamp, a rotating beacon, a buzzer, a
//!   meter). It is a starter default stored when the device is created (a lamp: steady), and the
//!   rating text may change it (`flashing`); it is not a rating field.
//! - Its rating is its colour ([`IndicatorColour`], IEC 60073: red, yellow, green, blue, white;
//!   kept apart from a wire's colour), its rated voltage with AC or DC (a lamp
//!   is bought by voltage: `24 V DC`) and its power, kept in watts and shown like a load's (`2 W`).
//! - Every rating field may be empty (not typed yet); [`Indicator::is_unrated`] says when all are.
//!   [`fmt::Display`] writes the set fields in a fixed order, the signal when it is not steady
//!   (`red, flashing, 24 V DC, 2 W`).
//!
//! This module excludes reading a typed rating text (`ecad-electrical`'s rating reader) and the
//! demand calculation.
#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::geometry::units::{RatedVoltage, Watts};
use crate::objects::devices::load::load_power;

/// The largest power of an indicator, in watts.
pub const MAX_POWER_WATTS: f64 = 10_000.0;
/// The largest rated voltage of an indicator, in volts.
pub const MAX_VOLTS: f64 = 1000.0;

/// An indicator: how it signals, its colour, its rated voltage and its power.
///
/// # Example
/// ```
/// use ecad_model::objects::devices::{Indicator, IndicatorColour, SignalType};
/// use ecad_model::geometry::units::{Current, RatedVoltage, Volts, Watts};
/// let v = RatedVoltage { volts: Volts(24.0), current: Current::Dc };
/// let i = Indicator { colour: Some(IndicatorColour::Red), rated_voltage: Some(v), power: Some(Watts(2.0)), ..Indicator::empty(SignalType::Flashing) };
/// assert_eq!(i.to_string(), "red, flashing, 24 V DC, 2 W");
/// assert_eq!(
///     serde_json::to_string(&i).unwrap(),
///     r#"{"signal":"flashing","colour":"red","rated_voltage":{"volts":24.0,"current":"dc"},"power":2.0}"#
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Indicator {
    pub signal: SignalType,
    /// Its colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour: Option<IndicatorColour>,
    /// Its rated voltage, AC or DC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_voltage: Option<RatedVoltage>,
    /// Its rated power.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<Watts>,
}

crate::word_enum! {
    /// How an indicator signals.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    pub enum SignalType {
        /// A lamp that is lit.
        #[default]
        Steady,
        Flashing,
        /// A rotating beacon.
        Rotating,
        /// A buzzer or horn.
        Audible,
        Meter,
    }
}

crate::word_enum! {
    /// An indicator's colour (IEC 60073).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum IndicatorColour {
        Red,
        Yellow,
        Green,
        Blue,
        White,
    }
}

impl IndicatorColour {
    /// As written on a drawing (`red`); the rating reader reads it back.
    pub fn label(self) -> &'static str {
        match self {
            IndicatorColour::Red => "red",
            IndicatorColour::Yellow => "yellow",
            IndicatorColour::Green => "green",
            IndicatorColour::Blue => "blue",
            IndicatorColour::White => "white",
        }
    }
}

impl SignalType {
    /// As written in a rating (`flashing`); the rating reader reads it back.
    pub fn word(self) -> &'static str {
        match self {
            SignalType::Steady => "steady",
            SignalType::Flashing => "flashing",
            SignalType::Rotating => "rotating",
            SignalType::Audible => "audible",
            SignalType::Meter => "meter",
        }
    }

    /// The indicator as a person reads it (`flashing indicator`).
    pub fn label(self) -> &'static str {
        match self {
            SignalType::Steady => "indicator lamp",
            SignalType::Flashing => "flashing indicator",
            SignalType::Rotating => "rotating beacon",
            SignalType::Audible => "audible indicator",
            SignalType::Meter => "meter",
        }
    }
}

impl Indicator {
    /// An indicator that signals as `signal`, with nothing rated.
    pub fn empty(signal: SignalType) -> Indicator {
        Indicator { signal, colour: None, rated_voltage: None, power: None }
    }

    /// The same indicator with nothing rated.
    pub fn unrated(&self) -> Indicator {
        Indicator::empty(self.signal)
    }

    /// Whether no rating field is set.
    pub fn is_unrated(&self) -> bool {
        *self == self.unrated()
    }

    /// Why it is not well formed, if it is not: a value out of range or not a number.
    pub fn problem(&self) -> Option<String> {
        let within = |x: f64, max: f64| x.is_finite() && x > 0.0 && x <= max;
        let bad = self.power.is_some_and(|w| !within(w.0, MAX_POWER_WATTS)) || self.rated_voltage.is_some_and(|v| !within(v.volts.0, MAX_VOLTS));
        bad.then(|| format!("a rating value of a {} is out of range", self.signal.label()))
    }
}

impl fmt::Display for Indicator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&super::join_parts(&self.rating_parts()))
    }
}

impl Indicator {
    /// Its set rating fields as shown, each with its field word.
    pub fn rating_parts(&self) -> Vec<(&'static str, String)> {
        let signal = (self.signal != SignalType::Steady).then(|| self.signal.word().to_string());
        [
            ("colour", self.colour.map(|c| c.label().to_string())),
            ("signal", signal),
            ("voltage", self.rated_voltage.map(|v| v.to_string())),
            ("power", self.power.map(load_power)),
        ]
        .into_iter()
        .filter_map(|(w, v)| v.map(|v| (w, v)))
        .collect()
    }
}
