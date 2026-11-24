//! What feeds a device, whether a motor's rating agrees with it, and reading a typed supply.
//!
//! # Semantics
//! - A device is fed by the supply typed on the potentials of the nets its connection points are
//!   on ([`ecad_model::connectivity::supply`]). Potentials with no supply are left out; when the
//!   supplied ones disagree, the device's supply is not known and nothing is compared.
//! - A three-phase motor's rated voltage is compared with the supply's line-to-line voltage, a
//!   single-phase motor's with its line-to-neutral voltage, within the project's tolerance
//!   (`rating.supply_tolerance`, percent). No voltage is computed from another: a single-phase motor
//!   on a three-phase supply with no line-to-neutral voltage is a row of its own. A motor on a
//!   supply of the wrong system (three-phase on single-phase, any motor on DC) is a row too. The
//!   rated frequency is compared when both it and the supply's are known.
//! - A motor with no rated voltage is offered the voltage it sees (`suggested_voltage`); the offer
//!   is taken only by the person, never written by itself.
//! - [`read`] reads the typed form: `400/230 V 3~ 50 Hz` fills both voltages of a three-phase
//!   supply, `230 V 1~` a single-phase one, `24 V DC` a DC one.
//!
//! This module excludes tracing through a transformer or a power supply: a supply is the one typed
//! on the potential.

use std::collections::BTreeSet;

use ecad_model::connectivity::supply::Supply;
use ecad_model::geometry::units::{Hertz, Volts};
use ecad_model::objects::devices::{Motor, Phases};
use ecad_model::{DeviceId, DeviceType, Document};

use crate::devices::ratings::parse::{decimal, pieces};
use crate::error::CmdError;

/// How far two frequencies may differ, in hertz, and still be the same.
const FREQUENCY_TOLERANCE: f64 = 0.5;

/// A device's supply and the potential it is typed on.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Fed {
    /// The first potential (by name) carrying it.
    pub(crate) potential: String,
    pub(crate) supply: Supply,
}

impl Fed {
    /// Where an offer comes from: `From L1: 400/230 V 3~ 50 Hz`.
    pub(crate) fn source_words(&self) -> String {
        format!("From {}: {}", self.potential, self.supply)
    }
}

/// The supply feeding device `c`, when its nets' supplied potentials agree on one. O(its
/// connection points and their nets' potentials).
pub(crate) fn fed(doc: &Document, c: DeviceId) -> Option<Fed> {
    let conn = doc.connectivity();
    let wiring = &doc.settings().wiring;
    let names: BTreeSet<&String> = doc
        .connection_points_of(c)
        .filter_map(|(t, _)| conn.net_of_connection_point(t))
        .filter_map(|n| conn.net(n))
        .flat_map(|n| n.potentials.keys())
        .collect();
    let mut found: Option<Fed> = None;
    for name in names {
        let Some(supply) = wiring.potential(name).and_then(|p| p.supply) else { continue };
        match &found {
            None => found = Some(Fed { potential: name.clone(), supply }),
            Some(f) if f.supply == supply => {}
            Some(_) => return None,
        }
    }
    found
}

/// The voltage a motor of `phases` sees on `supply`, or why it is not known or cannot run there.
fn seen(phases: Phases, supply: &Supply) -> Result<Volts, &'static str> {
    match (phases, supply) {
        (_, Supply::Dc { .. }) => Err("is on a DC supply"),
        (Phases::Three, Supply::Ac3 { line_to_line, .. }) => Ok(*line_to_line),
        (Phases::Three, Supply::Ac1 { .. }) => Err("is a three-phase motor on a single-phase supply"),
        (Phases::Single, Supply::Ac3 { line_to_neutral: Some(v), .. } | Supply::Ac1 { line_to_neutral: v, .. }) => Ok(*v),
        (Phases::Single, Supply::Ac3 { line_to_neutral: None, .. }) => Err("is single-phase: supply line-to-neutral voltage unknown"),
    }
}

/// The rated voltage to offer a motor that has none: the voltage it sees on its supply, when known.
pub(crate) fn suggested_voltage(motor: &Motor, fed: &Fed) -> Option<Volts> {
    match motor.rated_voltage {
        Some(_) => None,
        None => seen(motor.phases, &fed.supply).ok(),
    }
}

/// The project's tolerance on a motor's rated voltage, in percent.
pub(crate) fn tolerance(doc: &Document) -> f64 {
    ecad_model::registry::resolve(doc, &crate::defaults::rating::SUPPLY_TOLERANCE, ecad_model::registry::Scope::project()).number()
}

/// How motor `tag`'s rating or kind disagrees with what feeds it, in words, a rated voltage
/// within `tolerance` percent of the supply's agreeing; `None` when it agrees or a value is not
/// known.
pub(crate) fn motor_mismatch(tag: &str, motor: &Motor, fed: &Fed, tolerance: f64) -> Option<String> {
    let from = format!("{} ({})", fed.potential, fed.supply);
    match seen(motor.phases, &fed.supply) {
        Err(why) => return Some(format!("{tag} {why}, {from}")),
        Ok(v) => {
            if let Some(rated) = motor.rated_voltage
                && (rated.0 - v.0).abs() > v.0 * tolerance / 100.0
            {
                let between = match motor.phases {
                    Phases::Three => "line-to-line",
                    Phases::Single => "line-to-neutral",
                };
                return Some(format!("{tag} is rated {rated}; its supply gives {v} {between}, {from}"));
            }
        }
    }
    match (motor.rated_frequency, fed.supply.frequency()) {
        (Some(r), Some(s)) if (r.0 - s.0).abs() > FREQUENCY_TOLERANCE => Some(format!("{tag} is rated {r}; its supply is {s}, {from}")),
        _ => None,
    }
}

/// The motor of device `c`, if it is one.
pub(crate) fn motor_of(doc: &Document, c: DeviceId) -> Option<&Motor> {
    match &doc.devices().get(&c)?.device_type {
        DeviceType::Motor(m) => Some(m),
        _ => None,
    }
}

/// Read a typed supply: `400/230 V 3~ 50 Hz`, `400 V 3~`, `230 V 1~`, `24 V DC` (`3ph`, `1ph`,
/// `AC` and a decimal comma also read). Phases and system are said, never guessed.
///
/// # Errors
/// What is missing or does not read, in words.
pub(crate) fn read(text: &str) -> Result<Supply, CmdError> {
    let mut volts: Option<(f64, Option<f64>)> = None;
    let mut hertz: Option<f64> = None;
    let (mut three, mut one, mut dc) = (false, false, false);
    let words = pieces(text);
    let mut i = 0;
    while let Some(w) = words.get(i) {
        i += 1;
        match w.as_str() {
            "3~" | "3ph" | "3-phase" => three = true,
            "1~" | "1ph" | "1-phase" => one = true,
            "dc" | "vdc" => dc = true,
            "ac" | "vac" | "v" => {}
            _ => {
                let (num, mut unit) = match w.find(|c: char| c.is_ascii_alphabetic() || c == '~') {
                    Some(at) => w.split_at(at),
                    None => (w.as_str(), ""),
                };
                // A unit after a bare number: `400 V`, `50 Hz`.
                if unit.is_empty()
                    && let Some(u) = words.get(i).filter(|n| matches!(n.as_str(), "v" | "vac" | "vdc" | "hz"))
                {
                    unit = u;
                    i += 1;
                }
                match unit {
                    "hz" if hertz.is_none() => hertz = Some(decimal(num).ok_or_else(|| CmdError::refused(format!("'{w}' is not a frequency")))?),
                    "v" | "vac" | "vdc" | "" if volts.is_none() && !num.is_empty() => {
                        dc |= unit == "vdc";
                        let (a, b) = match num.split_once('/') {
                            Some((a, b)) => (a, Some(b)),
                            None => (num, None),
                        };
                        let a = decimal(a).ok_or_else(|| CmdError::refused(format!("'{w}' is not a voltage")))?;
                        let b = b.map(|b| decimal(b).ok_or_else(|| CmdError::refused(format!("'{w}' is not a voltage")))).transpose()?;
                        volts = Some((a, b));
                    }
                    "~" if num == "3" => three = true,
                    "~" if num == "1" => one = true,
                    _ => return Err(CmdError::refused(format!("'{w}' does not read as part of a supply"))),
                }
            }
        }
    }
    let (v, second) = volts.ok_or_else(|| CmdError::refused("a supply needs its voltage, for example 400/230 V 3~ 50 Hz"))?;
    let frequency = hertz.map(Hertz);
    let supply = match (dc, three, one, second) {
        (true, false, false, None) if frequency.is_none() => Supply::Dc { nominal: Volts(v) },
        (true, ..) => return Err(CmdError::refused("a DC supply has one voltage and no phases or frequency")),
        (false, true, true, _) => return Err(CmdError::refused("it says both 3~ and 1~")),
        (false, true, false, ln) => Supply::Ac3 { line_to_line: Volts(v), line_to_neutral: ln.map(Volts), frequency },
        (false, false, true, None) => Supply::Ac1 { line_to_neutral: Volts(v), frequency },
        (false, false, true, Some(_)) => return Err(CmdError::refused("a single-phase supply has one voltage, line-to-neutral")),
        (false, false, false, _) => return Err(CmdError::refused("say the phases (3~ or 1~) or DC")),
    };
    match supply.problem() {
        Some(p) => Err(CmdError::refused(p)),
        None => Ok(supply),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fed(supply: Supply) -> Fed {
        Fed { potential: "L1".into(), supply }
    }
    const THREE: Supply = Supply::Ac3 { line_to_line: Volts(400.0), line_to_neutral: Some(Volts(230.0)), frequency: Some(Hertz(50.0)) };
    const THREE_NO_N: Supply = Supply::Ac3 { line_to_line: Volts(400.0), line_to_neutral: None, frequency: None };

    #[test]
    fn a_motor_is_compared_with_the_voltage_its_phases_see() {
        let m = |phases, v: f64| Motor { rated_voltage: Some(Volts(v)), ..Motor::empty(phases) };
        assert_eq!(motor_mismatch("-M1", &m(Phases::Three, 400.0), &fed(THREE), 5.0), None);
        assert_eq!(motor_mismatch("-M1", &m(Phases::Three, 415.0), &fed(THREE), 5.0), None, "within 5 %");
        assert!(motor_mismatch("-M1", &m(Phases::Three, 415.0), &fed(THREE), 2.0).is_some(), "the tolerance is the project's");
        assert_eq!(motor_mismatch("-M1", &m(Phases::Single, 230.0), &fed(THREE), 5.0), None, "line-to-neutral as typed");
        assert_eq!(
            motor_mismatch("-M1", &m(Phases::Three, 230.0), &fed(THREE), 5.0).as_deref(),
            Some("-M1 is rated 230 V; its supply gives 400 V line-to-line, L1 (400/230 V 3~ 50 Hz)")
        );
        let one = Supply::Ac1 { line_to_neutral: Volts(230.0), frequency: None };
        assert_eq!(
            motor_mismatch("-M1", &Motor::empty(Phases::Three), &fed(one), 5.0).as_deref(),
            Some("-M1 is a three-phase motor on a single-phase supply, L1 (230 V 1~)")
        );
        assert!(motor_mismatch("-M1", &Motor::empty(Phases::Three), &fed(Supply::Dc { nominal: Volts(24.0) }), 5.0).is_some());
    }

    #[test]
    fn a_single_phase_motor_on_a_supply_with_no_line_to_neutral_voltage_is_a_row_not_a_guess() {
        assert_eq!(
            motor_mismatch("-M1", &Motor::empty(Phases::Single), &fed(THREE_NO_N), 5.0).as_deref(),
            Some("-M1 is single-phase: supply line-to-neutral voltage unknown, L1 (400 V 3~)")
        );
        assert_eq!(suggested_voltage(&Motor::empty(Phases::Single), &fed(THREE_NO_N)), None);
    }

    #[test]
    fn frequencies_are_compared_only_when_both_are_known() {
        let at = |hz: f64| Motor { rated_frequency: Some(Hertz(hz)), ..Motor::empty(Phases::Three) };
        assert!(motor_mismatch("-M1", &at(60.0), &fed(THREE), 5.0).is_some_and(|w| w.contains("rated 60 Hz")));
        assert_eq!(motor_mismatch("-M1", &at(50.0), &fed(THREE), 5.0), None);
        assert_eq!(motor_mismatch("-M1", &at(60.0), &fed(THREE_NO_N), 5.0), None);
    }

    #[test]
    fn only_a_motor_with_no_rated_voltage_is_offered_one() {
        assert_eq!(suggested_voltage(&Motor::empty(Phases::Three), &fed(THREE)), Some(Volts(400.0)));
        assert_eq!(suggested_voltage(&Motor::empty(Phases::Single), &fed(THREE)), Some(Volts(230.0)));
        let rated = Motor { rated_voltage: Some(Volts(690.0)), ..Motor::empty(Phases::Three) };
        assert_eq!(suggested_voltage(&rated, &fed(THREE)), None);
        assert_eq!(suggested_voltage(&Motor::empty(Phases::Three), &fed(Supply::Dc { nominal: Volts(24.0) })), None);
    }

    #[test]
    fn a_typed_supply_reads_into_its_fields_and_back() {
        for (text, want) in [
            ("400/230 V 3~ 50 Hz", "400/230 V 3~ 50 Hz"),
            ("400/230V 3~ 50Hz", "400/230 V 3~ 50 Hz"),
            ("400 V 3ph", "400 V 3~"),
            ("400 VAC 3~", "400 V 3~"),
            ("230 V 1~ 60 Hz", "230 V 1~ 60 Hz"),
            ("24 V DC", "24 V DC"),
            ("24VDC", "24 V DC"),
            ("12,5 V DC", "12.5 V DC"),
        ] {
            let got = read(text).ok().map(|s| s.to_string());
            assert_eq!(got.as_deref(), Some(want), "{text}");
            assert_eq!(read(want).ok().map(|s| s.to_string()).as_deref(), Some(want), "{want} reads back");
        }
        for (text, start) in [
            ("400 V", "say the phases"),
            ("50 Hz 3~", "a supply needs its voltage"),
            ("230/400 V 3~", "A supply's line-to-neutral voltage is below"),
            ("400/230 V 1~", "a single-phase supply has one voltage"),
            ("24 V DC 50 Hz", "a DC supply has one voltage"),
            ("400 V 3~ 1~", "it says both"),
            ("400 kV 3~", "'kv' does not read"),
            ("mains", "'mains' does not read"),
            ("0 V DC", "A supply's voltage"),
        ] {
            let got = read(text).err().map(|e| e.to_string()).unwrap_or_default();
            assert!(got.starts_with(start), "{text}: {got}");
        }
    }
}
