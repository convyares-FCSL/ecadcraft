//! A potential's supply: the voltage, AC or DC, the phases and the frequency of what feeds it.
//!
//! # Semantics
//! - A row of the potentials table ([`super::wiring::PotentialDef`]) may carry a [`Supply`]; none
//!   means not known, and nothing is inferred from the potential's name.
//! - The nominal voltage of an AC three-phase supply is its line-to-line voltage (`400 V`, as
//!   IEC 60038 names a 230/400 V system by 400 V); of an AC single-phase supply, the voltage
//!   between its line and neutral (`230 V`); of a DC supply, the voltage between its poles.
//! - [`Supply::line_to_line`] and [`Supply::line_to_neutral`] give what a three-phase and a
//!   single-phase load on it see; a load's rating is compared with them, never the other way
//!   round.
//!
//! This module excludes tracing a supply through a transformer or a power supply (a potential's
//! supply is the one typed on it) and the checks that compare a device with it
//! (`ecad-electrical`'s checks).
#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::geometry::units::{Hertz, Volts};
use crate::objects::devices::Phases;

/// The highest nominal voltage of a supply, in volts (1 MV).
pub const MAX_VOLTS: f64 = 1_000_000.0;
/// The highest supply frequency, in hertz (1 kHz).
const MAX_HERTZ: f64 = 1_000.0;

/// What feeds a potential.
///
/// # Example
/// ```
/// use ecad_model::connectivity::supply::Supply;
/// use ecad_model::geometry::units::{Hertz, Volts};
/// use ecad_model::objects::devices::Phases;
/// let s = Supply::Ac { nominal: Volts(400.0), phases: Phases::Three, frequency: Some(Hertz(50.0)) };
/// assert_eq!(s.to_string(), "400 V AC 3~ 50 Hz");
/// assert_eq!(s.line_to_neutral().map(|v| v.0.round()), Some(231.0));
/// assert_eq!(
///     serde_json::to_string(&s).unwrap(),
///     r#"{"current":"ac","nominal":400.0,"phases":"three","frequency":50.0}"#
/// );
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "current", rename_all = "snake_case", deny_unknown_fields)]
pub enum Supply {
    /// Alternating current.
    Ac {
        /// Line-to-line on three phases, line-to-neutral on one.
        nominal: Volts,
        phases: Phases,
        /// Not known when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        frequency: Option<Hertz>,
    },
    /// Direct current.
    Dc {
        /// Between the poles.
        nominal: Volts,
    },
}

impl Supply {
    /// The nominal voltage as typed.
    pub fn nominal(&self) -> Volts {
        match self {
            Supply::Ac { nominal, .. } | Supply::Dc { nominal } => *nominal,
        }
    }

    /// The frequency, when AC and known.
    pub fn frequency(&self) -> Option<Hertz> {
        match self {
            Supply::Ac { frequency, .. } => *frequency,
            Supply::Dc { .. } => None,
        }
    }

    /// The phases, when AC.
    pub fn phases(&self) -> Option<Phases> {
        match self {
            Supply::Ac { phases, .. } => Some(*phases),
            Supply::Dc { .. } => None,
        }
    }

    /// The voltage between two lines: the nominal of an AC three-phase supply; none otherwise.
    pub fn line_to_line(&self) -> Option<Volts> {
        match self {
            Supply::Ac { nominal, phases: Phases::Three, .. } => Some(*nominal),
            Supply::Ac { phases: Phases::Single, .. } | Supply::Dc { .. } => None,
        }
    }

    /// The voltage between a line and neutral: the nominal of an AC single-phase supply, the
    /// nominal divided by √3 on three phases; none for DC.
    pub fn line_to_neutral(&self) -> Option<Volts> {
        match self {
            Supply::Ac { nominal, phases: Phases::Single, .. } => Some(*nominal),
            Supply::Ac { nominal, phases: Phases::Three, .. } => Some(Volts(nominal.0 / 3f64.sqrt())),
            Supply::Dc { .. } => None,
        }
    }

    /// What is wrong with it, in words: a voltage or frequency that is not a number above zero, or
    /// beyond [`MAX_VOLTS`] or 1 kHz.
    pub fn problem(&self) -> Option<String> {
        let v = self.nominal().0;
        if !(v.is_finite() && v > 0.0 && v <= MAX_VOLTS) {
            return Some(format!("A supply's voltage is more than 0 and at most {MAX_VOLTS} V."));
        }
        match self.frequency() {
            Some(Hertz(f)) if !(f.is_finite() && f > 0.0 && f <= MAX_HERTZ) => {
                Some(format!("A supply's frequency is more than 0 and at most {MAX_HERTZ} Hz."))
            }
            Some(_) | None => None,
        }
    }
}

/// `400 V AC 3~ 50 Hz`, `230 V AC 1~`, `24 V DC`.
impl fmt::Display for Supply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Supply::Ac { nominal, phases, frequency } => {
                let n = match phases {
                    Phases::Single => 1,
                    Phases::Three => 3,
                };
                write!(f, "{nominal} AC {n}~")?;
                if let Some(hz) = frequency {
                    write!(f, " {hz}")?;
                }
                Ok(())
            }
            Supply::Dc { nominal } => write!(f, "{nominal} DC"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_voltages_follow_the_phases() {
        let three = Supply::Ac { nominal: Volts(400.0), phases: Phases::Three, frequency: None };
        let one = Supply::Ac { nominal: Volts(230.0), phases: Phases::Single, frequency: Some(Hertz(60.0)) };
        let dc = Supply::Dc { nominal: Volts(24.0) };
        assert_eq!(three.line_to_line(), Some(Volts(400.0)));
        assert_eq!(one.line_to_line(), None);
        assert_eq!(one.line_to_neutral(), Some(Volts(230.0)));
        assert_eq!((dc.line_to_line(), dc.line_to_neutral()), (None, None));
        assert_eq!(dc.to_string(), "24 V DC");
        assert_eq!(one.to_string(), "230 V AC 1~ 60 Hz");
    }

    #[test]
    fn a_supply_out_of_range_is_named() {
        assert!(Supply::Dc { nominal: Volts(0.0) }.problem().is_some());
        assert!(Supply::Dc { nominal: Volts(f64::NAN) }.problem().is_some());
        let hz = Supply::Ac { nominal: Volts(230.0), phases: Phases::Single, frequency: Some(Hertz(-50.0)) };
        assert!(hz.problem().is_some());
        assert_eq!(Supply::Dc { nominal: Volts(24.0) }.problem(), None);
    }

    #[test]
    fn a_dc_supply_refuses_phases() {
        assert!(serde_json::from_str::<Supply>(r#"{"current":"dc","nominal":24.0,"phases":"three"}"#).is_err());
        let s: Supply = serde_json::from_str(r#"{"current":"dc","nominal":24}"#).unwrap();
        assert_eq!(s, Supply::Dc { nominal: Volts(24.0) });
    }
}
