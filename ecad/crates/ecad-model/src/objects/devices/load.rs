//! A load's rating: its rated power,
//! a typed field, never a stored display string.
//!
//! # Semantics
//! - A load (a heater, a valve or solenoid, a brake) has one rating field: its rated power, kept in
//!   watts. Below 1 kW it is shown in watts (`5 W`), from 1 kW in kilowatts (`3 kW`); the rating
//!   reader reads both back.
//! - The field may be empty (not typed yet): [`Load::is_unrated`].
//!
//! This module excludes reading a typed rating text (`ecad-electrical`'s rating reader) and the
//! demand calculation.
#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::geometry::units::Watts;

/// The largest rated power of a load, in watts (100 MW).
pub const MAX_POWER_WATTS: f64 = 100_000_000.0;

/// A consumer with a rated power.
///
/// # Example
/// ```
/// use ecad_model::objects::devices::Load;
/// use ecad_model::geometry::units::Watts;
/// assert_eq!(Load { rated_power: Some(Watts(3000.0)) }.to_string(), "3 kW");
/// assert_eq!(Load { rated_power: Some(Watts(5.0)) }.to_string(), "5 W");
/// assert_eq!(serde_json::to_string(&Load { rated_power: Some(Watts(5.0)) }).unwrap(), r#"{"rated_power":5.0}"#);
/// assert_eq!(serde_json::to_string(&Load::default()).unwrap(), "{}");
/// ```
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Load {
    /// The rated power.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_power: Option<Watts>,
}

/// A load's or an indicator's power as shown: watts below 1 kW (`5 W`), kilowatts from it (`3 kW`).
pub(crate) fn load_power(w: Watts) -> String {
    if w.0 < 1000.0 { w.to_string() } else { format!("{} kW", w.0 / 1000.0) }
}

impl Load {
    /// Whether no rating field is set.
    pub fn is_unrated(&self) -> bool {
        self.rated_power.is_none()
    }

    /// The same load with nothing rated.
    pub fn unrated(&self) -> Load {
        Load::default()
    }

    /// Why it is not well formed, if it is not: a power out of range or not a number.
    pub fn problem(&self) -> Option<String> {
        let bad = self.rated_power.is_some_and(|w| !(w.0.is_finite() && w.0 > 0.0 && w.0 <= MAX_POWER_WATTS));
        bad.then(|| "a load's rated power is out of range".to_string())
    }
}

impl fmt::Display for Load {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(w) = self.rated_power {
            f.write_str(&load_power(w))?;
        }
        Ok(())
    }
}
