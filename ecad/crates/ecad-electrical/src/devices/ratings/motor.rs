//! Reading a motor's rating: its nameplate power, current, power
//! factor, efficiency and starting.
//!
//! # Semantics
//! - `7.5 kW` and `7500 W` are the rated power; `15.2 A` the rated current; `0,37 kW` reads as
//!   `0.37 kW` (a decimal comma).
//! - `cos φ 0.85` (also `cosφ`, `cos phi`, `pf`) is the power factor; `eff 91 %` (also `η`) the
//!   efficiency.
//! - `DOL` (or `direct`), `star-delta` (`Y-D`), `soft start` (`soft-start`) and `VFD` (`VSD`) say how
//!   it is started.
//! - Ambiguous text is asked about: a number without a unit (kW or A?), a percentage that does not
//!   say it is the efficiency. A voltage is refused: a motor's rating has no voltage field.
//! - Any other word (`IE3`, `see datasheet`) is no rating: the note.
//!
//! This module excludes the protection reader (its parent) and the calculations from the rating.
#![deny(clippy::wildcard_enum_match_arm)]

use ecad_model::geometry::units::{Amps, Percent, Watts};
use ecad_model::objects::devices::motor::{MAX_CURRENT_AMPS, MAX_POWER_WATTS};
use ecad_model::objects::devices::{Motor, Starting};

use crate::devices::ratings::parse::{number_unit, pieces};
use crate::devices::ratings::{MAX_CHARS, Read};

/// One read piece of a motor rating.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok {
    Watts(f64),
    KiloWatts(f64),
    Amps(f64),
    Percent(f64),
    Volts,
    Bare(f64),
    /// `cos φ`, `pf`: the next number is the power factor.
    PowerFactor,
    /// `eff`, `η`: the next percentage is the efficiency.
    Efficiency,
    Starting(Starting),
    /// `soft` (before `start`).
    Soft,
}

fn unit_token(x: f64, unit: &str) -> Option<Tok> {
    match unit {
        "" => Some(Tok::Bare(x)),
        "w" => Some(Tok::Watts(x)),
        "kw" => Some(Tok::KiloWatts(x)),
        "a" => Some(Tok::Amps(x)),
        "%" => Some(Tok::Percent(x)),
        "v" | "vac" | "vdc" => Some(Tok::Volts),
        _ => None,
    }
}

/// A piece's token: a word, or a number with its unit (`7.5kW`, `15A`, `91%`).
fn piece_token(lower: &str) -> Option<Tok> {
    match lower {
        "cosφ" | "cos" | "φ" | "phi" | "cosphi" | "pf" => return Some(Tok::PowerFactor),
        "eff" | "η" => return Some(Tok::Efficiency),
        "dol" | "direct" => return Some(Tok::Starting(Starting::Direct)),
        "star-delta" | "stardelta" | "y-d" | "y/d" => return Some(Tok::Starting(Starting::StarDelta)),
        "soft-start" | "softstart" => return Some(Tok::Starting(Starting::Soft)),
        "soft" => return Some(Tok::Soft),
        "vfd" | "vsd" => return Some(Tok::Starting(Starting::Vfd)),
        _ => {}
    }
    let (x, unit) = number_unit(lower)?;
    unit_token(x, unit)
}

/// The pieces of `text`, or `None` when a piece is no rating word.
fn tokens(text: &str) -> Option<Vec<Tok>> {
    let mut out: Vec<Tok> = Vec::new();
    for lower in pieces(text) {
        let run_on = match (out.last().copied(), lower.as_str()) {
            // A unit after a bare number: `7.5 kW`, `15 A`, `91 %`.
            (Some(Tok::Bare(x)), u) => unit_token(x, u).filter(|t| !matches!(t, Tok::Bare(_))),
            (Some(Tok::Soft), "start" | "starter") => Some(Tok::Starting(Starting::Soft)),
            // `cos φ` and `cos phi` are one marker.
            (Some(Tok::PowerFactor), "φ" | "phi") => Some(Tok::PowerFactor),
            _ => None,
        };
        if let Some(t) = run_on {
            out.pop();
            out.push(t);
            continue;
        }
        out.push(piece_token(&lower)?);
    }
    Some(out)
}

/// What one text gives, sorted by field.
#[derive(Default)]
struct Found {
    watts: Option<f64>,
    amps: Option<f64>,
    power_factor: Option<f64>,
    efficiency: Option<f64>,
    percent: Option<f64>,
    bare: Option<f64>,
    starting: Option<Starting>,
    volts: bool,
}

fn once<T>(slot: &mut Option<T>, v: T, what: &str) -> Result<(), Read<Motor>> {
    if slot.is_some() {
        return Err(Read::Refused(format!("it gives two {what}")));
    }
    *slot = Some(v);
    Ok(())
}

/// The fields of `toks`; a marker (`cos φ`, `eff`) takes the number after it.
fn found(toks: &[Tok]) -> Result<Found, Read<Motor>> {
    let mut f = Found::default();
    let mut i = 0;
    while let Some(t) = toks.get(i) {
        i += 1;
        match *t {
            Tok::Watts(x) => once(&mut f.watts, x, "powers")?,
            Tok::KiloWatts(x) => once(&mut f.watts, x * 1000.0, "powers")?,
            Tok::Amps(x) => once(&mut f.amps, x, "currents")?,
            Tok::Percent(x) => once(&mut f.percent, x, "percentages")?,
            Tok::Bare(x) => once(&mut f.bare, x, "numbers with no unit")?,
            Tok::Starting(s) => once(&mut f.starting, s, "ways of starting")?,
            Tok::Volts => f.volts = true,
            Tok::PowerFactor => match toks.get(i) {
                Some(Tok::Bare(x)) => {
                    i += 1;
                    once(&mut f.power_factor, *x, "power factors")?;
                }
                _ => return Err(Read::Ask("the power factor, for example cos φ 0.85".into())),
            },
            Tok::Efficiency => match toks.get(i) {
                Some(Tok::Percent(x)) => {
                    i += 1;
                    once(&mut f.efficiency, *x, "efficiencies")?;
                }
                _ => return Err(Read::Ask("the efficiency in percent, for example eff 91 %".into())),
            },
            Tok::Soft => return Err(Read::Ask("whether it has a soft starter; write soft start".into())),
        }
    }
    Ok(f)
}

fn within(x: f64, max: f64, what: &str, unit: &str) -> Result<f64, Read<Motor>> {
    if x.is_finite() && x > 0.0 && x <= max { Ok(x) } else { Err(Read::Refused(format!("{what} is more than 0 and at most {max}{unit}"))) }
}

/// Read `text` as the rating of `motor` (its phases kept, its nameplate read).
///
/// # Example
/// ```
/// use ecad_electrical::devices::ratings::{Read, read_motor};
/// use ecad_model::objects::devices::{Motor, Phases};
/// let m = Motor::empty(Phases::Three);
/// let Read::Rating(r) = read_motor(&m, "7,5kW 15.2A DOL") else { panic!() };
/// assert_eq!(r.to_string(), "7.5 kW, 15.2 A, DOL");
/// assert!(matches!(read_motor(&m, "7.5"), Read::Ask(_)));
/// assert_eq!(read_motor(&m, "see datasheet"), Read::NotARating);
/// ```
pub fn read_motor(motor: &Motor, text: &str) -> Read<Motor> {
    let t = text.trim();
    if t.is_empty() || t.chars().nth(MAX_CHARS).is_some() {
        return Read::NotARating;
    }
    let Some(toks) = tokens(t) else { return Read::NotARating };
    let f = match found(&toks) {
        Ok(f) => f,
        Err(Read::Refused(e)) => return Read::Refused(format!("'{t}' does not read: {e}")),
        Err(other) => return other,
    };
    match build(motor, &f) {
        Ok(m) => Read::Rating(m),
        Err(r) => r,
    }
}

fn build(motor: &Motor, f: &Found) -> Result<Motor, Read<Motor>> {
    if f.volts {
        return Err(Read::Refused(
            "a motor's rating has no voltage (its supply is its circuit's); give its power and current, for example 7.5 kW, 15.2 A".into(),
        ));
    }
    if let Some(x) = f.bare {
        return Err(Read::Ask(format!("a unit for {x}: kW (rated power) or A (rated current)?")));
    }
    if let Some(x) = f.percent {
        return Err(Read::Ask(format!("whether {x} % is the efficiency; write eff {x} %")));
    }
    // Read in watts or kilowatts, kept in watts to the milliwatt (0.37 kW is 370 W).
    let rated_power = f.watts.map(|w| within(w, MAX_POWER_WATTS, "a rated power", " W").map(|w| Watts((w * 1000.0).round() / 1000.0))).transpose()?;
    let rated_current = f.amps.map(|a| within(a, MAX_CURRENT_AMPS, "a rated current", " A").map(Amps)).transpose()?;
    let power_factor = f.power_factor.map(|p| within(p, 1.0, "a power factor", "")).transpose()?;
    let efficiency = f.efficiency.map(|e| within(e, 100.0, "an efficiency", " %").map(Percent)).transpose()?;
    Ok(Motor { rated_power, rated_current, power_factor, efficiency, starting: f.starting, ..motor.unrated() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecad_model::objects::devices::Phases;

    fn shown(text: &str) -> String {
        match read_motor(&Motor::empty(Phases::Three), text) {
            Read::Rating(s) => s.to_string(),
            Read::Ask(m) => format!("ask: {m}"),
            Read::Refused(m) => format!("refused: {m}"),
            Read::NotARating => "note".into(),
        }
    }

    #[test]
    fn motor_ratings_read_into_fields_and_back() {
        for (text, want) in [
            ("7.5 kW", "7.5 kW"),
            ("7,5kW", "7.5 kW"),
            ("0,37 kW", "0.37 kW"),
            ("370 W", "0.37 kW"),
            ("15.2 A", "15.2 A"),
            ("7.5kW 15.2A", "7.5 kW, 15.2 A"),
            ("cos φ 0.85", "cos φ 0.85"),
            ("cosφ 0.85", "cos φ 0.85"),
            ("cos phi 0,85", "cos φ 0.85"),
            ("PF 0.85", "cos φ 0.85"),
            ("eff 91 %", "eff 91 %"),
            ("η 91%", "eff 91 %"),
            ("DOL", "DOL"),
            ("direct", "DOL"),
            ("Y-D", "star-delta"),
            ("soft start", "soft start"),
            ("soft-start", "soft start"),
            ("VSD", "VFD"),
            ("7.5 kW, 15.2 A, cos φ 0.85, eff 91 %, star-delta", "7.5 kW, 15.2 A, cos φ 0.85, eff 91 %, star-delta"),
        ] {
            let got = shown(text);
            assert_eq!(got, want, "{text}");
            // What is shown reads back to the same fields.
            assert_eq!(shown(&got), want, "{got}");
        }
        // The phases are kept.
        let Read::Rating(m) = read_motor(&Motor::empty(Phases::Single), "0.37 kW") else { panic!() };
        assert_eq!((m.phases, m.rated_power), (Phases::Single, Some(Watts(370.0))));
    }

    #[test]
    fn ambiguous_motor_text_is_asked_wrong_values_refused_and_others_kept_as_note() {
        for (text, start) in [
            ("7.5", "ask: a unit for 7.5: kW (rated power) or A"),
            ("91 %", "ask: whether 91 % is the efficiency"),
            ("cos φ", "ask: the power factor"),
            ("eff 0.91", "ask: the efficiency in percent"),
            ("soft", "ask: whether it has a soft starter"),
            ("7.5 kW 400 V", "refused: a motor's rating has no voltage"),
            ("7.5 kW 5 kW", "refused: '7.5 kW 5 kW' does not read: it gives two powers"),
            ("DOL VFD", "refused: 'DOL VFD' does not read: it gives two ways"),
            ("cos φ 1.2", "refused: a power factor is more than 0 and at most 1"),
            ("eff 120 %", "refused: an efficiency"),
            ("0 kW", "refused: a rated power"),
            ("20000 A", "refused: a rated current"),
        ] {
            let got = shown(text);
            assert!(got.starts_with(start), "{text}: {got}");
        }
        for text in ["see datasheet", "IE3", "7.5 hp", "", "NaN kW", &"1".repeat(70)] {
            assert_eq!(shown(text), "note", "{text}");
        }
    }
}
