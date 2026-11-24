//! A potential's supply: its system (three-phase, single-phase or DC), its voltages as typed, and
//! its frequency.
//!
//! # Semantics
//! - A row of the potentials table ([`super::wiring::PotentialDef`]) may carry a [`Supply`]; none
//!   means not known, and nothing is inferred from the potential's name.
//! - Every voltage is the one the person typed; none is computed from another. A three-phase
//!   supply has its line-to-line voltage and may have its line-to-neutral voltage
//!   (`400/230 V 3~`); a single-phase supply has its line-to-neutral voltage (`230 V 1~`); a DC
//!   supply the voltage between its poles (`24 V DC`).
//! - The text form ([`fmt::Display`]) is the one people type: `400/230 V 3~ 50 Hz`, `400 V 3~`,
//!   `230 V 1~ 50 Hz`, `24 V DC`.
//!
//! This module excludes reading the typed text (`ecad-electrical`), tracing a supply through a
//! transformer or a power supply, and the checks that compare a device with it.
#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::geometry::units::{Hertz, Volts};

/// The highest voltage of a supply, in volts (1 MV).
pub const MAX_VOLTS: f64 = 1_000_000.0;
/// The highest supply frequency, in hertz (1 kHz).
const MAX_HERTZ: f64 = 1_000.0;

/// What feeds a potential.
///
/// # Example
/// ```
/// use ecad_model::connectivity::supply::Supply;
/// use ecad_model::geometry::units::{Hertz, Volts};
/// let s = Supply::Ac3 { line_to_line: Volts(400.0), line_to_neutral: Some(Volts(230.0)), frequency: Some(Hertz(50.0)) };
/// assert_eq!(s.to_string(), "400/230 V 3~ 50 Hz");
/// assert_eq!(
///     serde_json::to_string(&s).unwrap(),
///     r#"{"system":"ac3","line_to_line":400.0,"line_to_neutral":230.0,"frequency":50.0}"#
/// );
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "system", rename_all = "snake_case", deny_unknown_fields)]
pub enum Supply {
    /// Three-phase AC.
    Ac3 {
        line_to_line: Volts,
        /// Not known when absent: never computed from the line-to-line voltage.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line_to_neutral: Option<Volts>,
        /// Not known when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        frequency: Option<Hertz>,
    },
    /// Single-phase AC, between a line and neutral.
    Ac1 {
        line_to_neutral: Volts,
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
    /// The frequency, when AC and known.
    pub fn frequency(&self) -> Option<Hertz> {
        match self {
            Supply::Ac3 { frequency, .. } | Supply::Ac1 { frequency, .. } => *frequency,
            Supply::Dc { .. } => None,
        }
    }

    /// The voltages it has, as typed.
    fn voltages(&self) -> [Option<Volts>; 2] {
        match self {
            Supply::Ac3 { line_to_line, line_to_neutral, .. } => [Some(*line_to_line), *line_to_neutral],
            Supply::Ac1 { line_to_neutral, .. } => [Some(*line_to_neutral), None],
            Supply::Dc { nominal } => [Some(*nominal), None],
        }
    }

    /// What is wrong with it, in words: a voltage or frequency that is not a number above zero, or
    /// beyond [`MAX_VOLTS`] or 1 kHz; a line-to-neutral voltage not below the line-to-line.
    pub fn problem(&self) -> Option<String> {
        let ok = |x: f64, max: f64| x.is_finite() && x > 0.0 && x <= max;
        if self.voltages().into_iter().flatten().any(|v| !ok(v.0, MAX_VOLTS)) {
            return Some(format!("A supply's voltage is more than 0 and at most {MAX_VOLTS} V."));
        }
        if self.frequency().is_some_and(|f| !ok(f.0, MAX_HERTZ)) {
            return Some(format!("A supply's frequency is more than 0 and at most {MAX_HERTZ} Hz."));
        }
        match self {
            Supply::Ac3 { line_to_line, line_to_neutral: Some(ln), .. } if ln.0 >= line_to_line.0 => {
                Some("A supply's line-to-neutral voltage is below its line-to-line voltage.".into())
            }
            Supply::Ac3 { .. } | Supply::Ac1 { .. } | Supply::Dc { .. } => None,
        }
    }
}

/// `400/230 V 3~ 50 Hz`, `400 V 3~`, `230 V 1~ 50 Hz`, `24 V DC`.
impl fmt::Display for Supply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = crate::geometry::units::number_text;
        match self {
            Supply::Ac3 { line_to_line, line_to_neutral, .. } => {
                write!(f, "{}", n(line_to_line.0, ""))?;
                if let Some(ln) = line_to_neutral {
                    write!(f, "/{}", n(ln.0, ""))?;
                }
                write!(f, " V 3~")?;
            }
            Supply::Ac1 { line_to_neutral, .. } => write!(f, "{} 1~", n(line_to_neutral.0, "V"))?,
            Supply::Dc { nominal } => write!(f, "{} DC", n(nominal.0, "V"))?,
        }
        if let Some(hz) = self.frequency() {
            write!(f, " {}", n(hz.0, "Hz"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_form_writes_what_was_typed() {
        let three = Supply::Ac3 { line_to_line: Volts(400.0), line_to_neutral: None, frequency: None };
        let one = Supply::Ac1 { line_to_neutral: Volts(230.0), frequency: Some(Hertz(60.0)) };
        assert_eq!(three.to_string(), "400 V 3~");
        assert_eq!(one.to_string(), "230 V 1~ 60 Hz");
        assert_eq!(Supply::Dc { nominal: Volts(24.0) }.to_string(), "24 V DC");
    }

    #[test]
    fn a_supply_out_of_range_is_named() {
        assert!(Supply::Dc { nominal: Volts(0.0) }.problem().is_some());
        assert!(Supply::Dc { nominal: Volts(f64::NAN) }.problem().is_some());
        assert!(Supply::Ac1 { line_to_neutral: Volts(230.0), frequency: Some(Hertz(-50.0)) }.problem().is_some());
        let upside_down = Supply::Ac3 { line_to_line: Volts(230.0), line_to_neutral: Some(Volts(400.0)), frequency: None };
        assert!(upside_down.problem().is_some());
        assert_eq!(Supply::Dc { nominal: Volts(24.0) }.problem(), None);
    }

    #[test]
    fn a_dc_supply_refuses_ac_fields() {
        assert!(serde_json::from_str::<Supply>(r#"{"system":"dc","nominal":24.0,"frequency":50}"#).is_err());
        let s: Supply = serde_json::from_str(r#"{"system":"dc","nominal":24}"#).unwrap();
        assert_eq!(s, Supply::Dc { nominal: Volts(24.0) });
    }
}
