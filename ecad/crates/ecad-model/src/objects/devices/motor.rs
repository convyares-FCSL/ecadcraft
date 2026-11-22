//! A motor's phases and rating (design 072 §2.2 row 5, "Ratings", owner decisions 2026-11-18):
//! typed fields, never a stored display string.
//!
//! # Semantics
//! - [`Phases`] says whether it is a single-phase or a three-phase motor. The symbol declares it.
//! - Its rating is its nameplate: the rated power (kept in watts, shown in kilowatts: `7.5 kW`),
//!   the rated current (`15.2 A`), the power factor (`cos φ 0.85`), the efficiency (`eff 91 %`)
//!   and how it is started ([`Starting`]: direct on line, star-delta, soft starter, VFD).
//! - Every rating field may be empty (not typed yet); [`Motor::is_unrated`] says when all are.
//!   [`fmt::Display`] writes the set fields in a fixed order
//!   (`7.5 kW, 15.2 A, cos φ 0.85, eff 91 %, star-delta`), which is what the drawing and every
//!   command's answer show.
//!
//! This module excludes reading a typed rating text (`ecad-electrical`'s rating reader) and the
//! calculations from the rating (full-load and starting current, protection and cable sizing).
#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::objects::devices::switching::motor_power;
use crate::geometry::units::{Amps, Percent, Watts};

/// The largest rated power of a motor, in watts (100 MW).
pub const MAX_POWER_WATTS: f64 = 100_000_000.0;
/// The largest rated current of a motor, in amperes.
pub const MAX_CURRENT_AMPS: f64 = 10_000.0;

/// A motor: its phases and its nameplate rating (group 5, design 072 §2.2).
///
/// # Example
/// ```
/// use crate::objects::devices::{Motor, Phases, Starting};
/// use crate::geometry::units::{Amps, Watts};
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
    /// The rated (full-load) current.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_current: Option<Amps>,
    /// The power factor at rated load (cos φ), more than 0 and at most 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_factor: Option<f64>,
    /// The efficiency at rated load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub efficiency: Option<Percent>,
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

impl Phases {
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
        Motor { phases, rated_power: None, rated_current: None, power_factor: None, efficiency: None, starting: None }
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
            || self.rated_current.is_some_and(|a| !within(a.0, MAX_CURRENT_AMPS))
            || self.power_factor.is_some_and(|p| !within(p, 1.0))
            || self.efficiency.is_some_and(|e| !within(e.0, 100.0));
        bad.then(|| format!("a rating value of a {} is out of range", self.phases.label()))
    }
}

impl fmt::Display for Motor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = [
            self.rated_power.map(motor_power),
            self.rated_current.map(|a| a.to_string()),
            self.power_factor.map(|p| format!("cos φ {p}")),
            self.efficiency.map(|e| format!("eff {e}")),
            self.starting.map(|s| s.label().to_string()),
        ]
        .into_iter()
        .flatten()
        .collect();
        f.write_str(&parts.join(", "))
    }
}
