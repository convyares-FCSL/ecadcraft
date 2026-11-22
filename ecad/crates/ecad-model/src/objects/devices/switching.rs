//! A switching device's operator and rating (design 072 §2.2, "Ratings", owner decisions
//! 2026-11-18): typed fields, never a stored display string.
//!
//! # Semantics
//! - [`Operator`] says what operates it: a coil (relays, contactors), a solid-state input (an SSR)
//!   or a hand (push buttons, selectors, switch-disconnectors). The symbol declares it.
//! - A coil or an SSR input has a rated control voltage (`24 V DC`) and a power (`3 W`).
//! - The switched side has an AC-3 rating (IEC 60947-4-1, motor switching): the rated operational
//!   current (`AC-3 9 A`) and the motor power it switches (`4 kW`).
//! - Every field may be empty (not typed yet); [`Switching::is_unrated`] says when all are.
//!   [`fmt::Display`] writes the set fields in a fixed order (`coil 24 V DC, 3 W, AC-3 9 A, 4 kW`),
//!   which is what the drawing and every command's answer show.
//!
//! - `contacts` is what the part offers ([`ContactSet`]) and `contact_table` where its contact
//!   table goes (none: the project's default); neither is part of the rating.
//!
//! This module excludes reading a typed rating text (`ecad-electrical`'s rating reader) and
//! numbering the contacts a device offers (`ecad-electrical`'s `xref`).
#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::objects::devices::contacts::{ContactSet, ContactTable};

use crate::geometry::units::{Amps, RatedVoltage, Watts};

/// The largest control voltage of a coil or SSR input, in volts.
pub const MAX_CONTROL_VOLTS: f64 = 1000.0;
/// The largest power of a coil or SSR input, in watts.
pub const MAX_CONTROL_WATTS: f64 = 1000.0;
/// The largest AC-3 rated operational current, in amperes.
pub const MAX_AC3_AMPS: f64 = 6300.0;
/// The largest AC-3 motor power, in watts (4 MW).
pub const MAX_AC3_WATTS: f64 = 4_000_000.0;

/// A switching device: what operates it and its AC-3 rating (group 3, design 072 §2.2).
///
/// # Example
/// ```
/// use crate::objects::devices::{Control, Operator, Switching};
/// use crate::geometry::units::{Amps, Current, RatedVoltage, Volts};
/// let k = Switching {
///     operator: Operator::Coil(Control { voltage: Some(RatedVoltage { volts: Volts(24.0), current: Current::Dc }), power: None }),
///     ac3_current: Some(Amps(9.0)),
///     ac3_power: None,
///     ..Switching::default()
/// };
/// assert_eq!(k.to_string(), "coil 24 V DC, AC-3 9 A");
/// assert_eq!(
///     serde_json::to_string(&k).unwrap(),
///     r#"{"operator":{"coil":{"voltage":{"volts":24.0,"current":"dc"}}},"ac3_current":9.0}"#
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Switching {
    pub operator: Operator,
    /// The AC-3 rated operational current.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ac3_current: Option<Amps>,
    /// The AC-3 motor power it switches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ac3_power: Option<Watts>,
    /// The contacts it offers (design 025 FP1).
    #[serde(default, skip_serializing_if = "ContactSet::is_empty")]
    pub contacts: ContactSet,
    /// Where its contact table goes; none takes the project's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact_table: Option<ContactTable>,
}

/// What operates a switching device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    /// A coil: relays and contactors.
    Coil(Control),
    /// A solid-state relay's control input.
    SolidState(Control),
    /// A hand: push buttons, selectors, switch-disconnectors.
    Manual,
}

impl Default for Operator {
    fn default() -> Operator {
        Operator::Coil(Control::default())
    }
}

/// The kind of an [`Operator`], without its data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OperatorKind {
    Coil,
    SolidState,
    Manual,
}

impl OperatorKind {
    /// The device as a person reads it (`relay or contactor`).
    pub fn label(self) -> &'static str {
        match self {
            OperatorKind::Coil => "relay or contactor",
            OperatorKind::SolidState => "solid-state relay",
            OperatorKind::Manual => "manual switch",
        }
    }

    /// A device operated so, with nothing rated.
    pub fn empty(self) -> Switching {
        let operator = match self {
            OperatorKind::Coil => Operator::Coil(Control::default()),
            OperatorKind::SolidState => Operator::SolidState(Control::default()),
            OperatorKind::Manual => Operator::Manual,
        };
        Switching { operator, ac3_current: None, ac3_power: None, contacts: ContactSet::default(), contact_table: None }
    }
}

/// A coil's or an SSR input's rating: `24 V DC, 3 W`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Control {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voltage: Option<RatedVoltage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<Watts>,
}

impl Switching {
    /// What operates it, without its data.
    pub fn operator_kind(&self) -> OperatorKind {
        match self.operator {
            Operator::Coil(_) => OperatorKind::Coil,
            Operator::SolidState(_) => OperatorKind::SolidState,
            Operator::Manual => OperatorKind::Manual,
        }
    }

    /// Its coil's or SSR input's rating; `None` for a manual switch.
    pub fn control(&self) -> Option<&Control> {
        match &self.operator {
            Operator::Coil(c) | Operator::SolidState(c) => Some(c),
            Operator::Manual => None,
        }
    }

    /// Whether no rating field is set (its contacts and table are not rating fields).
    pub fn is_unrated(&self) -> bool {
        *self == self.unrated()
    }

    /// The same device with nothing rated: its operator kind, contacts and table kept.
    pub fn unrated(&self) -> Switching {
        Switching { contacts: self.contacts.clone(), contact_table: self.contact_table, ..self.operator_kind().empty() }
    }

    /// Why it is not well formed, if it is not: a value out of range or not a number.
    pub fn problem(&self) -> Option<String> {
        let within = |x: f64, max: f64| x.is_finite() && x > 0.0 && x <= max;
        let control_bad = self.control().is_some_and(|c| {
            c.voltage.is_some_and(|v| !within(v.volts.0, MAX_CONTROL_VOLTS)) || c.power.is_some_and(|w| !within(w.0, MAX_CONTROL_WATTS))
        });
        let bad = control_bad
            || self.ac3_current.is_some_and(|a| !within(a.0, MAX_AC3_AMPS))
            || self.ac3_power.is_some_and(|w| !within(w.0, MAX_AC3_WATTS));
        if bad {
            return Some(format!("a rating value of a {} is out of range", self.operator_kind().label()));
        }
        self.contacts.problem().or_else(|| self.contact_table.and_then(|t| t.problem()))
    }
}

/// A motor power as written on a drawing, in kilowatts: `4 kW`, `0.37 kW`.
pub fn motor_power(w: Watts) -> String {
    format!("{} kW", w.0 / 1000.0)
}

impl fmt::Display for Switching {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        let word = match self.operator {
            Operator::Coil(_) => "coil",
            Operator::SolidState(_) => "input",
            Operator::Manual => "",
        };
        if let Some(c) = self.control() {
            let mut first = true;
            for p in [c.voltage.map(|v| v.to_string()), c.power.map(|w| w.to_string())].into_iter().flatten() {
                parts.push(if first { format!("{word} {p}") } else { p });
                first = false;
            }
        }
        if let Some(a) = self.ac3_current {
            parts.push(format!("AC-3 {a}"));
        }
        if let Some(w) = self.ac3_power {
            parts.push(motor_power(w));
        }
        f.write_str(&parts.join(", "))
    }
}
