//! A load's rating: its rated power, voltage, current and phases,
//! typed fields, never a stored display string.
//!
//! # Semantics
//! - A load (a heater, a valve or solenoid, a brake) is rated by its nameplate: its rated power,
//!   kept in watts (below 1 kW shown in watts, `5 W`, from 1 kW in kilowatts, `3 kW`), its rated
//!   voltage with AC or DC (a heater or a valve coil is chosen for its supply: `24 V DC`,
//!   `230 V AC`), its rated current (for protection and cable sizing without guessing a power
//!   factor) and its phases ([`Phases`], shown `1~` or `3~`: a three-phase heater is wired and
//!   protected differently). The rating reader reads each back.
//! - Every field may be empty (not typed yet): [`Load::is_unrated`].
//!
//! This module excludes reading a typed rating text (`ecad-electrical`'s rating reader) and the
//! demand calculation.
#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::DeviceType;
use crate::objects::devices::RatingField;

use crate::geometry::units::{Amps, RatedVoltage, Watts};
use crate::objects::devices::Phases;

/// The largest rated power of a load, in watts (100 MW).
pub const MAX_POWER_WATTS: f64 = 100_000_000.0;
/// The largest rated voltage of a load, in volts.
pub const MAX_VOLTS: f64 = 1000.0;
/// The largest rated current of a load, in amperes.
pub const MAX_AMPS: f64 = 10_000.0;

/// A consumer with a rated power.
///
/// # Example
/// ```
/// use ecad_model::objects::devices::Load;
/// use ecad_model::geometry::units::Watts;
/// assert_eq!(Load { rated_power: Some(Watts(3000.0)), ..Load::default() }.to_string(), "3 kW");
/// assert_eq!(Load { rated_power: Some(Watts(5.0)), ..Load::default() }.to_string(), "5 W");
/// assert_eq!(serde_json::to_string(&Load { rated_power: Some(Watts(5.0)), ..Load::default() }).unwrap(), r#"{"rated_power":5.0}"#);
/// assert_eq!(serde_json::to_string(&Load::default()).unwrap(), "{}");
/// ```
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Load {
    /// The rated power.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_power: Option<Watts>,
    /// The rated voltage, AC or DC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_voltage: Option<RatedVoltage>,
    /// The rated current.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_current: Option<Amps>,
    /// Its supply phases.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phases: Option<Phases>,
}

/// A load's or an indicator's power as shown: watts below 1 kW (`5 W`), kilowatts from it (`3 kW`).
pub(crate) fn load_power(w: Watts) -> String {
    if w.0 < 1000.0 { w.to_string() } else { format!("{} kW", w.0 / 1000.0) }
}

impl Load {
    /// Whether no rating field is set.
    pub fn is_unrated(&self) -> bool {
        *self == Load::default()
    }

    /// The same load with nothing rated.
    pub fn unrated(&self) -> Load {
        Load::default()
    }

    /// Why it is not well formed, if it is not: a value out of range or not a number.
    pub fn problem(&self) -> Option<String> {
        let within = |x: f64, max: f64| x.is_finite() && x > 0.0 && x <= max;
        let bad = self.rated_power.is_some_and(|w| !within(w.0, MAX_POWER_WATTS))
            || self.rated_voltage.is_some_and(|v| !within(v.volts.0, MAX_VOLTS))
            || self.rated_current.is_some_and(|a| !within(a.0, MAX_AMPS));
        bad.then(|| "a rating value of a load is out of range".to_string())
    }
}

impl fmt::Display for Load {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&super::join_parts(&DeviceType::Load(self.clone()).rating_parts()))
    }
}

/// A load's rating fields, in the rating line's order.
pub(super) static FIELDS: &[RatingField] = &[
    rating_field!("power", "Rated power", Plate, Watts, DeviceType::Load(l) => l.rated_power),
    rating_field!("voltage", "Rated voltage", Plate, RatedVoltage, DeviceType::Load(l) => l.rated_voltage),
    rating_field!("current", "Rated current", Plate, Amps, DeviceType::Load(l) => l.rated_current),
    rating_field!("phases", "Phases", Plate, Phases, DeviceType::Load(l) => l.phases),
];
