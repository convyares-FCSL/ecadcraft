//! Reading a sensor's rating (design 072 §2.2, "Ratings"): a powered sensor's supply voltage.
//!
//! # Semantics
//! - `24 V DC`, `24VDC` and `supply 24 V DC` read alike, as the supply.
//! - A voltage without AC or DC is asked about; so is a number without a unit.
//! - A contact or a thermocouple has no supply and no typed rating: whatever is typed on it is
//!   the note (a contact's switching rating, a thermocouple's type).
//! - Any other word (`10-30 V DC`, `see datasheet`) is no rating: the note.
//!
//! This module excludes the protection reader (its parent) and the check that a sensor suits its
//! PLC channel.
#![deny(clippy::wildcard_enum_match_arm)]

use ecad_model::geometry::units::{Current, RatedVoltage, Volts};
use ecad_model::objects::devices::Sensor;
use ecad_model::objects::devices::sensor::MAX_SUPPLY_VOLTS;

use crate::devices::ratings::{MAX_CHARS, Read};

/// One read piece of a supply.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok {
    Volts(f64),
    Bare(f64),
    Current(Current),
    /// `supply` (before the voltage).
    Filler,
}

/// A piece's tokens: a word, or a number with its unit (`24V`, `24VDC`).
fn piece_tokens(lower: &str) -> Option<Vec<Tok>> {
    match lower {
        "supply" => return Some(vec![Tok::Filler]),
        "ac" => return Some(vec![Tok::Current(Current::Ac)]),
        "dc" => return Some(vec![Tok::Current(Current::Dc)]),
        _ => {}
    }
    let split = lower.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(lower.len());
    let (num, unit) = lower.split_at(split);
    let x = crate::devices::ratings::parse::decimal(num)?;
    match unit {
        "" => Some(vec![Tok::Bare(x)]),
        "v" => Some(vec![Tok::Volts(x)]),
        "vac" => Some(vec![Tok::Volts(x), Tok::Current(Current::Ac)]),
        "vdc" => Some(vec![Tok::Volts(x), Tok::Current(Current::Dc)]),
        _ => None,
    }
}

/// The pieces of `text`, or `None` when a piece is no supply word.
fn tokens(text: &str) -> Option<Vec<Tok>> {
    let mut out: Vec<Tok> = Vec::new();
    for piece in text.split(|c: char| c.is_whitespace() || c == ',' || c == ';').filter(|p| !p.is_empty()) {
        let lower = piece.to_ascii_lowercase();
        // A unit after a bare number: `24 V`, `24 VDC`.
        if let Some(Tok::Bare(x)) = out.last().copied() {
            let run_on = match lower.as_str() {
                "v" => Some(vec![Tok::Volts(x)]),
                "vac" => Some(vec![Tok::Volts(x), Tok::Current(Current::Ac)]),
                "vdc" => Some(vec![Tok::Volts(x), Tok::Current(Current::Dc)]),
                _ => None,
            };
            if let Some(ts) = run_on {
                out.pop();
                out.extend(ts);
                continue;
            }
        }
        out.extend(piece_tokens(&lower)?);
    }
    Some(out)
}

/// Read `text` as the rating of `sensor` (its kind kept, its supply read).
///
/// # Example
/// ```
/// use crate::devices::ratings::{Read, read_sensor};
/// use ecad_model::objects::devices::{Output, Sensing, Sensor};
/// let prox = Sensor::empty(Sensing::Proximity, Output::Pnp);
/// let Read::Rating(b) = read_sensor(&prox, "24VDC") else { panic!() };
/// assert_eq!(b.to_string(), "24 V DC");
/// assert!(matches!(read_sensor(&prox, "24 V"), Read::Ask(_)));
/// assert_eq!(read_sensor(&prox, "see datasheet"), Read::NotARating);
/// assert_eq!(read_sensor(&Sensor::empty(Sensing::Float, Output::Contact), "24 V DC"), Read::NotARating);
/// ```
pub fn read_sensor(sensor: &Sensor, text: &str) -> Read<Sensor> {
    let t = text.trim();
    if !sensor.output.is_powered() || t.is_empty() || t.chars().nth(MAX_CHARS).is_some() {
        return Read::NotARating;
    }
    let Some(toks) = tokens(t) else { return Read::NotARating };
    if toks.iter().all(|t| matches!(t, Tok::Filler)) {
        return Read::NotARating;
    }
    let (mut volts, mut current, mut bare) = (None, None, None);
    for tok in &toks {
        let (slot, v, what) = match *tok {
            Tok::Volts(x) => (&mut volts, x, "voltages"),
            Tok::Bare(x) => (&mut bare, x, "numbers with no unit"),
            Tok::Current(c) => {
                if current.replace(c).is_some() {
                    return Read::Refused(format!("'{t}' does not read: it gives two kinds of voltage (AC or DC)"));
                }
                continue;
            }
            Tok::Filler => continue,
        };
        if slot.replace(v).is_some() {
            return Read::Refused(format!("'{t}' does not read: it gives two {what}"));
        }
    }
    if let Some(x) = bare {
        return Read::Ask(format!("a unit for {x}: the supply is a voltage, for example {x} V DC?"));
    }
    match (volts, current) {
        (Some(v), Some(c)) => {
            if !(v.is_finite() && v > 0.0 && v <= MAX_SUPPLY_VOLTS) {
                return Read::Refused(format!("a supply voltage is more than 0 and at most {MAX_SUPPLY_VOLTS} V"));
            }
            Read::Rating(Sensor { supply: Some(RatedVoltage { volts: Volts(v), current: c }), ..sensor.unrated() })
        }
        (Some(v), None) => Read::Ask(format!("whether {v} V is AC or DC, for example {v} V DC")),
        (None, Some(c)) => Read::Ask(format!("the voltage, for example 24 V {}", c.label())),
        (None, None) => Read::NotARating,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecad_model::objects::devices::{Output, Sensing};

    fn shown(sensor: &Sensor, text: &str) -> String {
        match read_sensor(sensor, text) {
            Read::Rating(s) => s.to_string(),
            Read::Ask(m) => format!("ask: {m}"),
            Read::Refused(m) => format!("refused: {m}"),
            Read::NotARating => "note".into(),
        }
    }

    #[test]
    fn sensor_supplies_read_into_fields_and_back() {
        let pnp = Sensor::empty(Sensing::Proximity, Output::Pnp);
        let npn = Sensor::empty(Sensing::Proximity, Output::Npn);
        for (s, text, want) in [
            (&pnp, "24 V DC", "24 V DC"),
            (&pnp, "24VDC", "24 V DC"),
            (&pnp, "supply 24 V DC", "24 V DC"),
            (&pnp, "24V DC", "24 V DC"),
            (&npn, "230 VAC", "230 V AC"),
            (&npn, "12.5 V DC", "12.5 V DC"),
        ] {
            let got = shown(s, text);
            assert_eq!(got, want, "{text}");
            // What is shown reads back to the same fields.
            assert_eq!(shown(s, &got), want, "{got}");
        }
        // The kind is kept.
        let Read::Rating(b) = read_sensor(&npn, "24 V DC") else { panic!() };
        assert_eq!(b.output, Output::Npn);
    }

    #[test]
    fn ambiguous_sensor_text_is_asked_wrong_values_refused_and_unpowered_kept_as_note() {
        let pnp = Sensor::empty(Sensing::Proximity, Output::Pnp);
        for (text, start) in [
            ("24", "ask: a unit for 24"),
            ("24 V", "ask: whether 24 V is AC or DC"),
            ("DC", "ask: the voltage"),
            ("24 V DC 230 V AC", "refused: '24 V DC 230 V AC' does not read: it gives two voltages"),
            ("24 V AC DC", "refused: '24 V AC DC' does not read: it gives two kinds"),
            ("2000 V DC", "refused: a supply voltage"),
            ("0 V DC", "refused: a supply voltage"),
        ] {
            let got = shown(&pnp, text);
            assert!(got.starts_with(start), "{text}: {got}");
        }
        for text in ["see datasheet", "10-30 V DC", "24 V AC/DC", "", "supply", "NaN V DC", &"1".repeat(70)] {
            assert_eq!(shown(&pnp, text), "note", "{text}");
        }
        for s in [Sensor::empty(Sensing::Float, Output::Contact), Sensor::empty(Sensing::Temperature, Output::Thermocouple)] {
            for text in ["24 V DC", "K", "250 V AC 5 A"] {
                assert_eq!(shown(&s, text), "note", "{text}");
            }
        }
    }
}
