//! What feeds a device, and whether a motor's rating agrees with it.
//!
//! # Semantics
//! - A device is fed by the supply typed on the potentials of the nets its connection points are
//!   on ([`ecad_model::connectivity::supply`]). Potentials with no supply are left out; when the
//!   supplied ones disagree, the device's supply is not known and nothing is compared.
//! - A three-phase motor's rated voltage is compared with the supply's line-to-line voltage, a
//!   single-phase motor's with its line-to-neutral voltage, within ±5 % (zone A of IEC 60034-1). The
//!   rated frequency is compared when both it and the supply's are known.
//! - A motor with no rated voltage is offered the supply's (`suggested_voltage`); the offer is
//!   taken only by the person, never written by itself.
//!
//! This module excludes tracing through a transformer or a power supply: a supply is the one typed
//! on the potential.

use std::collections::BTreeSet;

use ecad_model::connectivity::supply::Supply;
use ecad_model::geometry::units::Volts;
use ecad_model::objects::devices::{Motor, Phases};
use ecad_model::{DeviceId, DeviceType, Document};

/// The setprop key of a motor's rated voltage (`400 V`; empty clears it).
pub(crate) const RATED_VOLTAGE_KEY: &str = "rated_voltage";

/// The share a rated voltage may differ from the supply's by (zone A of IEC 60034-1).
const VOLTAGE_TOLERANCE: f64 = 0.05;
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
    /// Where an offer comes from: `From L1: 400 V AC 3~ 50 Hz`.
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

/// The voltage a motor of `phases` sees on `supply`, or why it cannot run on it.
fn seen(phases: Phases, supply: &Supply) -> Result<Volts, &'static str> {
    match (phases, supply) {
        (_, Supply::Dc { .. }) => Err("a DC supply"),
        (Phases::Three, s) => s.line_to_line().ok_or("a single-phase supply"),
        (Phases::Single, s) => s.line_to_neutral().ok_or("a supply with no neutral voltage"),
    }
}

/// The rated voltage to offer a motor that has none: the voltage it sees on its supply.
pub(crate) fn suggested_voltage(motor: &Motor, fed: &Fed) -> Option<Volts> {
    match motor.rated_voltage {
        Some(_) => None,
        None => seen(motor.phases, &fed.supply).ok(),
    }
}

/// How motor `tag`'s rating disagrees with what feeds it, in words; `None` when it agrees or a
/// value is not known.
pub(crate) fn motor_mismatch(tag: &str, motor: &Motor, fed: &Fed) -> Option<String> {
    let from = format!("{} ({})", fed.potential, fed.supply);
    if let Some(rated) = motor.rated_voltage {
        match seen(motor.phases, &fed.supply) {
            Err(why) => return Some(format!("{tag} is a {} on {why}, {from}", motor.phases.label())),
            Ok(v) if (rated.0 - v.0).abs() > v.0 * VOLTAGE_TOLERANCE => {
                let between = match motor.phases {
                    Phases::Three => "line-to-line",
                    Phases::Single => "line-to-neutral",
                };
                return Some(format!("{tag} is rated {rated}; its supply gives {} {between}, {from}", Volts(v.0.round())));
            }
            Ok(_) => {}
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

#[cfg(test)]
mod tests {
    use super::*;
    use ecad_model::geometry::units::Hertz;

    fn fed(supply: Supply) -> Fed {
        Fed { potential: "L1".into(), supply }
    }
    const THREE: Supply = Supply::Ac { nominal: Volts(400.0), phases: Phases::Three, frequency: Some(Hertz(50.0)) };

    #[test]
    fn a_motor_is_compared_with_the_voltage_its_phases_see() {
        let m = |phases, v: f64| Motor { rated_voltage: Some(Volts(v)), ..Motor::empty(phases) };
        assert_eq!(motor_mismatch("-M1", &m(Phases::Three, 400.0), &fed(THREE)), None);
        assert_eq!(motor_mismatch("-M1", &m(Phases::Three, 415.0), &fed(THREE)), None, "within 5 %");
        assert_eq!(motor_mismatch("-M1", &m(Phases::Single, 230.0), &fed(THREE)), None, "line-to-neutral");
        assert_eq!(
            motor_mismatch("-M1", &m(Phases::Three, 230.0), &fed(THREE)).as_deref(),
            Some("-M1 is rated 230 V; its supply gives 400 V line-to-line, L1 (400 V AC 3~ 50 Hz)")
        );
        let one = Supply::Ac { nominal: Volts(230.0), phases: Phases::Single, frequency: None };
        assert_eq!(
            motor_mismatch("-M1", &m(Phases::Three, 400.0), &fed(one)).as_deref(),
            Some("-M1 is a three-phase motor on a single-phase supply, L1 (230 V AC 1~)")
        );
        assert!(motor_mismatch("-M1", &m(Phases::Three, 24.0), &fed(Supply::Dc { nominal: Volts(24.0) })).is_some());
    }

    #[test]
    fn frequencies_are_compared_only_when_both_are_known() {
        let at = |hz: f64| Motor { rated_frequency: Some(Hertz(hz)), ..Motor::empty(Phases::Three) };
        assert!(motor_mismatch("-M1", &at(60.0), &fed(THREE)).is_some_and(|w| w.contains("rated 60 Hz")));
        assert_eq!(motor_mismatch("-M1", &at(50.0), &fed(THREE)), None);
        let unknown = Supply::Ac { nominal: Volts(400.0), phases: Phases::Three, frequency: None };
        assert_eq!(motor_mismatch("-M1", &at(60.0), &fed(unknown)), None);
    }

    #[test]
    fn only_a_motor_with_no_rated_voltage_is_offered_one() {
        assert_eq!(suggested_voltage(&Motor::empty(Phases::Three), &fed(THREE)), Some(Volts(400.0)));
        assert_eq!(suggested_voltage(&Motor::empty(Phases::Single), &fed(THREE)), Some(Volts(230.0)));
        let rated = Motor { rated_voltage: Some(Volts(690.0)), ..Motor::empty(Phases::Three) };
        assert_eq!(suggested_voltage(&rated, &fed(THREE)), None);
        assert_eq!(suggested_voltage(&Motor::empty(Phases::Three), &fed(Supply::Dc { nominal: Volts(24.0) })), None);
    }
}
