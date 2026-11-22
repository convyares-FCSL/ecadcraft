//! Reading a load's rating (design 072 §2.2, "Ratings"): its rated power.
//!
//! # Semantics
//! - `3 kW`, `3000 W` and `3kW` are the rated power; `0,5 kW` reads as `0.5 kW` (a decimal comma).
//! - Ambiguous text is asked about: a number without a unit (W or kW?).
//! - A voltage or a current is refused: a load's rating is its power only (its supply is its
//!   circuit's).
//! - Any other word (`see datasheet`, `NC`) is no rating: the note.
//!
//! This module excludes the protection reader (its parent) and the demand calculation.
#![deny(clippy::wildcard_enum_match_arm)]

use ecad_model::geometry::units::Watts;
use ecad_model::objects::devices::Load;
use ecad_model::objects::devices::load::MAX_POWER_WATTS;

use crate::devices::ratings::{MAX_CHARS, Read};

/// One read piece of a power rating.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum PowerTok {
    Watts(f64),
    Bare(f64),
    /// A voltage or a current: a value the group has no field for.
    Other(&'static str),
}

fn unit_token(x: f64, unit: &str) -> Option<PowerTok> {
    match unit {
        "" => Some(PowerTok::Bare(x)),
        "w" => Some(PowerTok::Watts(x)),
        "kw" => Some(PowerTok::Watts(x * 1000.0)),
        "v" | "vac" | "vdc" => Some(PowerTok::Other("voltage")),
        "a" | "ma" => Some(PowerTok::Other("current")),
        _ => None,
    }
}

/// A number with its unit (`3kW`, `5W`, `24V`), or `None` when the piece is not one.
pub(super) fn power_piece(lower: &str) -> Option<PowerTok> {
    let split = lower.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(lower.len());
    let (num, unit) = lower.split_at(split);
    unit_token(crate::devices::ratings::parse::decimal(num)?, unit)
}

/// A unit after a bare number (`3 kW`): the joined token.
pub(super) fn power_unit_after(prev: Option<PowerTok>, lower: &str) -> Option<PowerTok> {
    match prev {
        Some(PowerTok::Bare(x)) => unit_token(x, lower).filter(|t| !matches!(t, PowerTok::Bare(_))),
        // `24 V DC`: the kind of supply belongs to the voltage.
        Some(PowerTok::Other("voltage")) if matches!(lower, "ac" | "dc") => Some(PowerTok::Other("voltage")),
        Some(PowerTok::Watts(_) | PowerTok::Other(_)) | None => None,
    }
}

/// `text` with a comma between digits made a decimal point (`0,5 kW`), split into lower-case pieces.
pub(super) fn pieces(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let text: String = chars
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let digit = |j: Option<usize>| j.and_then(|j| chars.get(j)).is_some_and(char::is_ascii_digit);
            if *c == ',' && digit(i.checked_sub(1)) && digit(Some(i + 1)) { '.' } else { *c }
        })
        .collect();
    text.split(|c: char| c.is_whitespace() || c == ',' || c == ';').filter(|p| !p.is_empty()).map(str::to_lowercase).collect()
}

/// Why the powers a text gives are not one power.
pub(super) enum PowerRefusal {
    /// It gives two powers.
    TwoPowers,
    /// A number has no unit: the question to ask.
    Ask(String),
    /// Refused, in words.
    Refused(String),
}

/// The one power `toks` give, in watts to the milliwatt: refused when two or out of range, asked
/// about when a number has no unit. `group` names the device in the answer (`a load`).
pub(super) fn one_power(toks: &[PowerTok], group: &str, example: &str) -> Result<Option<Watts>, PowerRefusal> {
    let mut watts: Option<f64> = None;
    for t in toks {
        match *t {
            PowerTok::Watts(x) => {
                if watts.is_some() {
                    return Err(PowerRefusal::TwoPowers);
                }
                watts = Some(x);
            }
            PowerTok::Bare(x) => return Err(PowerRefusal::Ask(format!("a unit for {x}: W or kW?"))),
            PowerTok::Other(what) => {
                return Err(PowerRefusal::Refused(format!(
                    "{group}'s rating has no {what} (its supply is its circuit's); give its power, for example {example}"
                )));
            }
        }
    }
    watts
        .map(|w| {
            if w.is_finite() && w > 0.0 && w <= MAX_POWER_WATTS {
                Ok(Watts((w * 1000.0).round() / 1000.0))
            } else {
                Err(PowerRefusal::Refused(format!("a rated power is more than 0 and at most {MAX_POWER_WATTS} W")))
            }
        })
        .transpose()
}

/// Read `text` as the rating of a load.
///
/// # Example
/// ```
/// use ecad_electrical::devices::ratings::{Read, read_load};
/// let Read::Rating(r) = read_load("3kW") else { panic!() };
/// assert_eq!(r.to_string(), "3 kW");
/// assert!(matches!(read_load("3"), Read::Ask(_)));
/// assert!(matches!(read_load("24 V"), Read::Refused(_)));
/// assert_eq!(read_load("see datasheet"), Read::NotARating);
/// ```
pub fn read_load(text: &str) -> Read<Load> {
    let t = text.trim();
    if t.is_empty() || t.chars().nth(MAX_CHARS).is_some() {
        return Read::NotARating;
    }
    let mut toks: Vec<PowerTok> = Vec::new();
    for p in pieces(t) {
        if let Some(j) = power_unit_after(toks.last().copied(), &p) {
            toks.pop();
            toks.push(j);
            continue;
        }
        let Some(tok) = power_piece(&p) else { return Read::NotARating };
        toks.push(tok);
    }
    match one_power(&toks, "a load", "3 kW") {
        Ok(rated_power) => Read::Rating(Load { rated_power }),
        Err(PowerRefusal::TwoPowers) => Read::Refused(format!("'{t}' does not read: it gives two powers")),
        Err(PowerRefusal::Ask(q)) => Read::Ask(q),
        Err(PowerRefusal::Refused(e)) => Read::Refused(e),
    }
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
            ("3", "ask: a unit for 3: W or kW?"),
            ("24 V", "refused: a load's rating has no voltage"),
            ("24VDC 5 W", "refused: a load's rating has no voltage"),
            ("24 V DC", "refused: a load's rating has no voltage"),
            ("2 A", "refused: a load's rating has no current"),
            ("3 kW 5 W", "refused: '3 kW 5 W' does not read: it gives two powers"),
            ("0 W", "refused: a rated power"),
            ("200000 kW", "refused: a rated power"),
        ] {
            let got = shown(text);
            assert!(got.starts_with(start), "{text}: {got}");
        }
        for text in ["see datasheet", "NC", "5 hp", "", "NaN W", &"1".repeat(70)] {
            assert_eq!(shown(text), "note", "{text}");
        }
    }
}
