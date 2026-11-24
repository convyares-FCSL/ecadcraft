//! A sensor's kind and rating: typed fields, never a stored display string.
//!
//! # Semantics
//! - [`Sensing`] says what it senses (proximity, float level, position, a guard, temperature) and
//!   [`Output`] what it gives a PLC input (a volt-free contact, a PNP or NPN switched output, a
//!   thermocouple's signal). The symbol declares both.
//! - A PNP or NPN sensor is powered: its rating is its supply voltage (`24 V DC`). A contact or a
//!   thermocouple has no supply and no typed rating; text typed on it is the device's note.
//! - [`Sensor::is_unrated`] says when the supply is empty; [`fmt::Display`] writes it (`24 V DC`),
//!   which is what the drawing and every command's answer show.
//!
//! This module excludes reading a typed rating text (`ecad-electrical`'s rating reader) and the
//! check that a sensor's output suits its PLC channel.
#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::DeviceType;
use crate::objects::devices::RatingField;

use crate::geometry::units::RatedVoltage;
use crate::naming::words::Word;

/// The largest supply voltage of a sensor, in volts.
pub const MAX_SUPPLY_VOLTS: f64 = 1000.0;

/// A sensor: what it senses, what it outputs and its supply.
///
/// # Example
/// ```
/// use ecad_model::objects::devices::{Output, Sensing, Sensor};
/// use ecad_model::geometry::units::{Current, RatedVoltage, Volts};
/// let b = Sensor { sensing: Sensing::Proximity, output: Output::Pnp, supply: Some(RatedVoltage { volts: Volts(24.0), current: Current::Dc }), ..Sensor::default() };
/// assert_eq!(b.to_string(), "24 V DC");
/// assert_eq!(
///     serde_json::to_string(&b).unwrap(),
///     r#"{"sensing":"proximity","output":"pnp","supply":{"volts":24.0,"current":"dc"}}"#
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sensor {
    pub sensing: Sensing,
    pub output: Output,
    /// A powered sensor's supply voltage; always empty on a contact or a thermocouple.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supply: Option<RatedVoltage>,
    /// The contacts a switching sensor offers (limit, pressure and float switches).
    #[serde(default, skip_serializing_if = "super::ContactSet::is_empty")]
    pub contacts: crate::objects::devices::ContactSet,
}

crate::word_enum! {
    /// What a sensor senses.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    pub enum Sensing {
        #[default]
        Proximity,
        /// A liquid's level (a float switch).
        Float,
        /// A position (a limit or roller switch).
        Limit,
        /// A guard or door (an interlock switch).
        Interlock,
        Temperature,
    }
}

crate::word_enum! {
    /// What a sensor gives a PLC input.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    pub enum Output {
        /// A volt-free contact (NO or NC).
        #[default]
        Contact,
        /// A switched output sourcing current (PNP).
        Pnp,
        /// A switched output sinking current (NPN).
        Npn,
        /// A thermocouple's millivolt signal.
        Thermocouple,
    }
}

impl Output {
    /// Whether a sensor with this output is powered (and so has a supply voltage).
    pub fn is_powered(self) -> bool {
        match self {
            Output::Pnp | Output::Npn => true,
            Output::Contact | Output::Thermocouple => false,
        }
    }
}

impl Sensing {
    /// The sensor as a person reads it (`float switch`).
    pub fn label(self) -> &'static str {
        match self {
            Sensing::Proximity => "proximity switch",
            Sensing::Float => "float switch",
            Sensing::Limit => "position switch",
            Sensing::Interlock => "interlock switch",
            Sensing::Temperature => "temperature sensor",
        }
    }
}

impl Sensor {
    /// A sensor sensing `sensing` with `output`, with nothing rated.
    pub fn empty(sensing: Sensing, output: Output) -> Sensor {
        Sensor { sensing, output, supply: None, contacts: crate::objects::devices::ContactSet::default() }
    }

    /// The same sensor with nothing rated (its contacts kept).
    pub fn unrated(&self) -> Sensor {
        Sensor { contacts: self.contacts.clone(), ..Sensor::empty(self.sensing, self.output) }
    }

    /// Whether no rating field is set.
    pub fn is_unrated(&self) -> bool {
        self.supply.is_none()
    }

    /// Why it is not well formed, if it is not: a supply on an unpowered sensor, or a value out of
    /// range or not a number.
    pub fn problem(&self) -> Option<String> {
        if let Some(p) = self.contacts.problem() {
            return Some(p);
        }
        let v = self.supply?;
        if !self.output.is_powered() {
            return Some(format!("a {} with a {} output has no supply", self.sensing.label(), self.output.word()));
        }
        let ok = v.volts.0.is_finite() && v.volts.0 > 0.0 && v.volts.0 <= MAX_SUPPLY_VOLTS;
        (!ok).then(|| format!("the supply of a {} is out of range", self.sensing.label()))
    }
}

impl fmt::Display for Sensor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&super::join_parts(&DeviceType::Sensor(self.clone()).rating_parts()))
    }
}

/// A sensor's rating fields.
pub(super) static FIELDS: &[RatingField] = &[rating_field!("supply", "Supply voltage", Plate, RatedVoltage, DeviceType::Sensor(s) => s.supply)];
