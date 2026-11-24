//! Reading a load's rating: its rated power, voltage, current and phases.
//!
//! # Semantics
//! - `3 kW`, `3000 W` and `3kW` are the rated power; `0,5 kW` reads as `0.5 kW` (a decimal comma).
//! - `230 V AC`, `24 VDC` is the rated voltage, which needs AC or DC; `13 A` (or `500 mA`) the
//!   rated current; `1~` or `3~` (also `1ph`, `3ph`, `single-phase`, `three-phase`) the phases.
//! - Ambiguous text is asked about: a number without a unit, a voltage without AC or DC.
//! - Any other word (`see datasheet`, `NC`) is no rating: the note.
//!
//! This module excludes the protection reader (its parent) and the demand calculation.
#![deny(clippy::wildcard_enum_match_arm)]

use ecad_model::geometry::units::{Amps, Current, RatedVoltage, Volts, Watts};
use ecad_model::objects::devices::load::{MAX_AMPS, MAX_POWER_WATTS, MAX_VOLTS};
use ecad_model::objects::devices::{Load, Phases};

use crate::devices::ratings::parse::{number_unit, pieces};
use crate::devices::ratings::{MAX_CHARS, Problem, Read};

/// One read piece of a load rating.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok {
    Watts(f64),
    Volts(f64, Option<Current>),
    /// `AC` or `DC` (after a voltage).
    Current(Current),
    Amps(f64),
    Phases(Phases),
    Bare(f64),
}

fn unit_token(x: f64, unit: &str) -> Option<Tok> {
    let phases = |x: f64| {
        let is = |n: f64| (x - n).abs() < f64::EPSILON;
        if is(1.0) {
            Some(Tok::Phases(Phases::Single))
        } else if is(3.0) {
            Some(Tok::Phases(Phases::Three))
        } else {
            None
        }
    };
    match unit {
        "" => Some(Tok::Bare(x)),
        "w" => Some(Tok::Watts(x)),
        "kw" => Some(Tok::Watts(x * 1000.0)),
        "v" => Some(Tok::Volts(x, None)),
        "vac" => Some(Tok::Volts(x, Some(Current::Ac))),
        "vdc" => Some(Tok::Volts(x, Some(Current::Dc))),
        "a" => Some(Tok::Amps(x)),
        "ma" => Some(Tok::Amps(x / 1000.0)),
        "~" | "ph" => phases(x),
        _ => None,
    }
}

/// A piece's token: a word, or a number with its unit (`3kW`, `24VDC`, `13A`, `3~`).
fn piece_token(lower: &str) -> Option<Tok> {
    match lower {
        "ac" => return Some(Tok::Current(Current::Ac)),
        "dc" => return Some(Tok::Current(Current::Dc)),
        "single-phase" | "1-phase" => return Some(Tok::Phases(Phases::Single)),
        "three-phase" | "3-phase" => return Some(Tok::Phases(Phases::Three)),
        _ => {}
    }
    let (x, unit) = number_unit(lower)?;
    unit_token(x, unit)
}

/// The pieces of `text`, or `None` when a piece is no rating word.
fn tokens(text: &str) -> Option<Vec<Tok>> {
    let mut out: Vec<Tok> = Vec::new();
    for lower in pieces(text) {
        // A unit after a bare number: `3 kW`, `24 V`, `13 A`, `3 ~`.
        if let Some(Tok::Bare(x)) = out.last().copied()
            && let Some(t) = unit_token(x, &lower).filter(|t| !matches!(t, Tok::Bare(_)))
        {
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
    volts: Option<(f64, Option<Current>)>,
    amps: Option<f64>,
    phases: Option<Phases>,
    bare: Option<f64>,
}

fn once<T>(slot: &mut Option<T>, v: T, what: &str) -> Result<(), Problem> {
    if slot.is_some() {
        return Err(Problem::Refused(format!("it gives two {what}")));
    }
    *slot = Some(v);
    Ok(())
}

fn found(toks: &[Tok]) -> Result<Found, Problem> {
    let mut f = Found::default();
    let mut prev: Option<Tok> = None;
    for t in toks {
        match *t {
            Tok::Watts(x) => once(&mut f.watts, x, "powers")?,
            Tok::Volts(x, c) => once(&mut f.volts, (x, c), "voltages")?,
            Tok::Current(c) => match (prev, f.volts.as_mut()) {
                (Some(Tok::Volts(_, None)), Some(v)) => v.1 = Some(c),
                _ => return Err(Problem::Refused("AC or DC is given apart from its voltage; write it after it, for example 230 V AC".into())),
            },
            Tok::Amps(x) => once(&mut f.amps, x, "currents")?,
            Tok::Phases(p) => once(&mut f.phases, p, "phase counts")?,
            Tok::Bare(x) => once(&mut f.bare, x, "numbers with no unit")?,
        }
        prev = Some(*t);
    }
    Ok(f)
}

fn within(x: f64, max: f64, what: &str, unit: &str) -> Result<f64, Problem> {
    if x.is_finite() && x > 0.0 && x <= max { Ok(x) } else { Err(Problem::Refused(format!("{what} is more than 0 and at most {max} {unit}"))) }
}

/// Read `text` as the rating of a load.
///
/// # Example
/// ```
/// use ecad_electrical::devices::ratings::{Read, read_load};
/// let Read::Rating(r) = read_load("3kW 400VAC 4.3A 3~") else { panic!() };
/// assert_eq!(r.to_string(), "3 kW, 400 V AC, 4.3 A, 3~");
/// assert!(matches!(read_load("3"), Read::Ask(_)));
/// assert!(matches!(read_load("24 V"), Read::Ask(_)));
/// assert_eq!(read_load("see datasheet"), Read::NotARating);
/// ```
pub fn read_load(text: &str) -> Read<Load> {
    let t = text.trim();
    if t.is_empty() || t.chars().nth(MAX_CHARS).is_some() {
        return Read::NotARating;
    }
    let Some(toks) = tokens(t) else { return Read::NotARating };
    let f = match found(&toks) {
        Ok(f) => f,
        Err(Problem::Refused(e)) => return Read::Refused(format!("'{t}' does not read: {e}")),
        Err(Problem::Ask(q)) => return Read::Ask(q),
    };
    match build(&f) {
        Ok(l) => Read::Rating(l),
        Err(Problem::Ask(q)) => Read::Ask(q),
        Err(Problem::Refused(m)) => Read::Refused(m),
    }
}

fn build(f: &Found) -> Result<Load, Problem> {
    if let Some(x) = f.bare {
        return Err(Problem::Ask(format!("a unit for {x}: W or kW (power), V (voltage) or A (current)?")));
    }
    // Read in watts or kilowatts, kept in watts to the milliwatt.
    let rated_power = f.watts.map(|w| within(w, MAX_POWER_WATTS, "a rated power", "W").map(|w| Watts((w * 1000.0).round() / 1000.0))).transpose()?;
    let rated_voltage = match f.volts {
        None => None,
        Some((x, None)) => return Err(Problem::Ask(format!("whether {x} V is AC or DC, for example {x} V AC"))),
        Some((x, Some(current))) => Some(RatedVoltage { volts: Volts(within(x, MAX_VOLTS, "a rated voltage", "V")?), current }),
    };
    let rated_current = f.amps.map(|a| within(a, MAX_AMPS, "a rated current", "A").map(|a| Amps((a * 1000.0).round() / 1000.0))).transpose()?;
    Ok(Load { rated_power, rated_voltage, rated_current, phases: f.phases })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(text: &str) -> String {
        match read_load(text) {
            Read::Rating(s) => s.to_string(),
            Read::Ask(m) => format!("ask: {m}"),
            Read::Refused(m) => format!("refused: {m}"),
            Read::NotARating => "note".into(),
        }
    }

    #[test]
    fn load_ratings_read_into_fields_and_back() {
        for (text, want) in [
            ("3 kW", "3 kW"),
            ("3kW", "3 kW"),
            ("3000 W", "3 kW"),
            ("0,5 kW", "500 W"),
            ("5 W", "5 W"),
            ("5W", "5 W"),
            ("1.5 kW", "1.5 kW"),
            ("7.5 W", "7.5 W"),
            ("24 V DC", "24 V DC"),
            ("24VDC 5 W", "5 W, 24 V DC"),
            ("230 V AC 13 A 1~", "230 V AC, 13 A, 1~"),
            ("9 kW 400VAC 13A 3ph", "9 kW, 400 V AC, 13 A, 3~"),
            ("three-phase 9 kW", "9 kW, 3~"),
            ("500 mA", "0.5 A"),
        ] {
            let got = shown(text);
            assert_eq!(got, want, "{text}");
            // What is shown reads back to the same fields.
            assert_eq!(shown(&got), want, "{got}");
        }
    }

    #[test]
    fn ambiguous_load_text_is_asked_wrong_values_refused_and_others_kept_as_note() {
        for (text, start) in [
            ("3", "ask: a unit for 3: W or kW"),
            ("24 V", "ask: whether 24 V is AC or DC"),
            ("AC 24 V", "refused: 'AC 24 V' does not read: AC or DC is given apart"),
            ("3 kW 5 W", "refused: '3 kW 5 W' does not read: it gives two powers"),
            ("2 A 3 A", "refused: '2 A 3 A' does not read: it gives two currents"),
            ("1~ 3~", "refused: '1~ 3~' does not read: it gives two phase counts"),
            ("0 W", "refused: a rated power"),
            ("200000 kW", "refused: a rated power"),
            ("2000 V AC", "refused: a rated voltage"),
            ("0 A", "refused: a rated current"),
        ] {
            let got = shown(text);
            assert!(got.starts_with(start), "{text}: {got}");
        }
        for text in ["see datasheet", "NC", "5 hp", "2~", "", "NaN W", &"1".repeat(170)] {
            assert_eq!(shown(text), "note", "{text}");
        }
    }
}
