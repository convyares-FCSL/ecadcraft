//! A power supply's or a transformer's rating (design 072 §2.2 row 1, "Ratings", overseer rulings
//! 2026-11-18): typed fields per kind, never a stored display string.
//!
//! # Semantics
//! - Which kind it is ([`PowerSupply::Psu`] or [`PowerSupply::Transformer`]) is its starter's,
//!   stored when the device is created; the rating text never changes it. Each kind carries only
//!   its own fields, so a transformer can hold neither a DC voltage nor watts.
//! - A power supply has an input and an output [`RatedVoltage`] (each AC or DC), an output current
//!   and an output power. A transformer has an input and an output voltage (always AC) and a rated
//!   apparent power in [`VoltAmps`]. A transformer's ratio is worked out from its two voltages
//!   ([`Transformer::ratio`]), never stored.
//! - Every field may be empty (not typed yet); [`PowerSupply::is_unrated`] says when all are.
//!   [`fmt::Display`] writes what the rating reader reads back: both voltages as `in / out`
//!   (`230 V AC / 24 V DC, 5 A, 120 W`), one alone with `in` or `out` before it, except a power
//!   supply's DC output, which is its output alone (`24 V DC`).
//!
//! This module excludes reading a typed rating text (`ecad-electrical`'s rating reader) and the
//! supply and demand calculations.
#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::objects::devices::load::load_power;
use crate::geometry::units::{Amps, Current, RatedVoltage, VoltAmps, Watts};

/// The largest input or output voltage, in volts (low voltage).
pub const MAX_VOLTS: f64 = 1000.0;
/// The largest output current of a power supply, in amps.
pub const MAX_AMPS: f64 = 1000.0;
/// The largest output power of a power supply, in watts.
pub const MAX_POWER_WATTS: f64 = 100_000.0;
/// The largest rated power of a transformer, in volt-amperes.
pub const MAX_VOLT_AMPS: f64 = 10_000_000.0;

/// A power supply or a transformer, each with its own fields (group 1, design 072 §2.2). Batteries
/// come later.
///
/// # Example
/// ```
/// use crate::objects::devices::{PowerSupply, Psu, Transformer};
/// use crate::geometry::units::{Amps, Current, RatedVoltage, VoltAmps, Volts};
/// let dc24 = RatedVoltage { volts: Volts(24.0), current: Current::Dc };
/// let psu = PowerSupply::Psu(Psu { output: Some(dc24), output_current: Some(Amps(5.0)), ..Psu::default() });
/// assert_eq!(psu.to_string(), "24 V DC, 5 A");
/// assert_eq!(serde_json::to_string(&psu).unwrap(), r#"{"psu":{"output":{"volts":24.0,"current":"dc"},"output_current":5.0}}"#);
/// let ac = |v| Some(RatedVoltage { volts: Volts(v), current: Current::Ac });
/// let t = Transformer { input: ac(400.0), output: ac(24.0), rated_power: Some(VoltAmps(250.0)) };
/// assert_eq!(PowerSupply::Transformer(t).to_string(), "400 V AC / 24 V AC, 250 VA");
/// assert_eq!(t.ratio(), Some(400.0 / 24.0));
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerSupply {
    /// G: a power supply (AC/DC, DC/DC).
    Psu(Psu),
    /// T: a transformer.
    Transformer(Transformer),
}

impl Default for PowerSupply {
    fn default() -> PowerSupply {
        PowerSupply::Psu(Psu::default())
    }
}

/// A power supply's rating.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Psu {
    /// Its input voltage, AC or DC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<RatedVoltage>,
    /// Its output voltage, AC or DC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<RatedVoltage>,
    /// Its rated output current.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_current: Option<Amps>,
    /// Its rated output power.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_power: Option<Watts>,
}

/// A transformer's rating. Both voltages are AC.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transformer {
    /// Its primary voltage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<RatedVoltage>,
    /// Its secondary voltage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<RatedVoltage>,
    /// Its rated apparent power.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_power: Option<VoltAmps>,
}

impl Transformer {
    /// Its ratio, input over output (`400/24`), when both voltages are set.
    pub fn ratio(&self) -> Option<f64> {
        match (self.input, self.output) {
            (Some(i), Some(o)) if o.volts.0 > 0.0 => Some(i.volts.0 / o.volts.0),
            (Some(_) | None, Some(_) | None) => None,
        }
    }
}

/// A transformer's power as shown: VA below 1 kVA (`250 VA`), kVA from it (`2.5 kVA`).
pub fn volt_amps(va: VoltAmps) -> String {
    if va.0 < 1000.0 { va.to_string() } else { format!("{} kVA", va.0 / 1000.0) }
}

impl PowerSupply {
    /// The same kind with nothing rated.
    pub fn unrated(&self) -> PowerSupply {
        match self {
            PowerSupply::Psu(_) => PowerSupply::Psu(Psu::default()),
            PowerSupply::Transformer(_) => PowerSupply::Transformer(Transformer::default()),
        }
    }

    /// Whether no rating field is set.
    pub fn is_unrated(&self) -> bool {
        *self == self.unrated()
    }

    /// The kind as a person reads it (`power supply`).
    pub fn label(&self) -> &'static str {
        match self {
            PowerSupply::Psu(_) => "power supply",
            PowerSupply::Transformer(_) => "transformer",
        }
    }

    /// Why it is not well formed, if it is not: a value out of range or not a number, or a
    /// transformer with a DC voltage.
    pub fn problem(&self) -> Option<String> {
        let within = |x: f64, max: f64| x.is_finite() && x > 0.0 && x <= max;
        let volts_bad = |v: Option<RatedVoltage>| v.is_some_and(|v| !within(v.volts.0, MAX_VOLTS));
        match self {
            PowerSupply::Psu(p) => {
                let bad = volts_bad(p.input)
                    || volts_bad(p.output)
                    || p.output_current.is_some_and(|a| !within(a.0, MAX_AMPS))
                    || p.output_power.is_some_and(|w| !within(w.0, MAX_POWER_WATTS));
                bad.then(|| "a rating value of a power supply is out of range".to_string())
            }
            PowerSupply::Transformer(t) => {
                if [t.input, t.output].iter().flatten().any(|v| v.current == Current::Dc) {
                    return Some("a transformer's voltages are AC".to_string());
                }
                let bad = volts_bad(t.input) || volts_bad(t.output) || t.rated_power.is_some_and(|va| !within(va.0, MAX_VOLT_AMPS));
                bad.then(|| "a rating value of a transformer is out of range".to_string())
            }
        }
    }
}

/// The voltages as the reader reads them back: `in / out`, `in X`, `out X`, or a power supply's DC
/// output alone.
fn voltages(input: Option<RatedVoltage>, output: Option<RatedVoltage>, dc_output_alone: bool) -> Option<String> {
    match (input, output) {
        (Some(i), Some(o)) => Some(format!("{i} / {o}")),
        (Some(i), None) => Some(format!("in {i}")),
        (None, Some(o)) if dc_output_alone && o.current == Current::Dc => Some(o.to_string()),
        (None, Some(o)) => Some(format!("out {o}")),
        (None, None) => None,
    }
}

impl fmt::Display for PowerSupply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = match self {
            PowerSupply::Psu(p) => [voltages(p.input, p.output, true), p.output_current.map(|a| a.to_string()), p.output_power.map(load_power)]
                .into_iter()
                .flatten()
                .collect(),
            PowerSupply::Transformer(t) => [voltages(t.input, t.output, false), t.rated_power.map(volt_amps)].into_iter().flatten().collect(),
        };
        f.write_str(&parts.join(", "))
    }
}
