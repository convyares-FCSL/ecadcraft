//! Reading an indicator's rating (design 072 §2.2, "Ratings"): its colour, how it signals, its
//! rated voltage and its power.
//!
//! # Semantics
//! - A colour is read in any form of IEC 60757 (`red`, `RD`, `RED`); only an indicator's colours
//!   (IEC 60073: red, yellow, green, blue, white) are taken, another conductor colour is refused.
//! - `steady`, `flashing`, `rotating`, `audible` and `meter` say how it signals; without one the
//!   indicator keeps its own.
//! - `24 V DC` (also `24VDC`) is the rated voltage; a voltage without AC or DC is asked about. `2 W` and `0,5 kW` are the power; a bare number is asked about (V or W?).
//! - A current is refused: an indicator's rating has no current field.
//! - Any other word (`see datasheet`, `LED`) is no rating: the note.
//!
//! This module excludes the protection reader (its parent) and the demand calculation.
#![deny(clippy::wildcard_enum_match_arm)]

use ecad_model::geometry::colour::ConductorColour;
use ecad_model::objects::devices::indicator::{MAX_POWER_WATTS, MAX_VOLTS};
use ecad_model::objects::devices::{Indicator, IndicatorColour, SignalType};
use ecad_model::geometry::units::{Current, RatedVoltage, Volts, Watts};

use crate::devices::ratings::load::pieces;
use crate::devices::ratings::{MAX_CHARS, Read};

/// One read piece of an indicator rating.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok {
    Volts(f64),
    Watts(f64),
    Bare(f64),
    Current(Current),
    Amps,
    Signal(SignalType),
    Colour(ConductorColour),
}

fn unit_tokens(x: f64, unit: &str) -> Option<Vec<Tok>> {
    match unit {
        "" => Some(vec![Tok::Bare(x)]),
        "v" => Some(vec![Tok::Volts(x)]),
        "vac" => Some(vec![Tok::Volts(x), Tok::Current(Current::Ac)]),
        "vdc" => Some(vec![Tok::Volts(x), Tok::Current(Current::Dc)]),
        "w" => Some(vec![Tok::Watts(x)]),
        "kw" => Some(vec![Tok::Watts(x * 1000.0)]),
        "a" | "ma" => Some(vec![Tok::Amps]),
        _ => None,
    }
}

/// A piece's tokens: a word, or a number with its unit (`24V`, `24VDC`, `2W`).
fn piece_tokens(lower: &str) -> Option<Vec<Tok>> {
    let word = match lower {
        "ac" => Some(Tok::Current(Current::Ac)),
        "dc" => Some(Tok::Current(Current::Dc)),
        "steady" => Some(Tok::Signal(SignalType::Steady)),
        "flashing" => Some(Tok::Signal(SignalType::Flashing)),
        "rotating" => Some(Tok::Signal(SignalType::Rotating)),
        "audible" => Some(Tok::Signal(SignalType::Audible)),
        "meter" => Some(Tok::Signal(SignalType::Meter)),
        _ => ConductorColour::parse(lower).map(Tok::Colour),
    };
    if let Some(w) = word {
        return Some(vec![w]);
    }
    let split = lower.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(lower.len());
    let (num, unit) = lower.split_at(split);
    unit_tokens(crate::devices::ratings::parse::decimal(num)?, unit)
}

/// The pieces of `text`, or `None` when a piece is no rating word.
fn tokens(text: &str) -> Option<Vec<Tok>> {
    let mut out: Vec<Tok> = Vec::new();
    for p in pieces(text) {
        // A unit after a bare number: `24 V`, `2 W`, `24 VDC`.
        if let Some(Tok::Bare(x)) = out.last().copied()
            && let Some(t) = unit_tokens(x, &p).filter(|t| !matches!(t.as_slice(), [Tok::Bare(_)]))
        {
            out.pop();
            out.extend(t);
            continue;
        }
        out.extend(piece_tokens(&p)?);
    }
    Some(out)
}

/// What one text gives, sorted by field.
#[derive(Default)]
struct Found {
    volts: Option<f64>,
    current: Option<Current>,
    watts: Option<f64>,
    bare: Option<f64>,
    signal: Option<SignalType>,
    colour: Option<ConductorColour>,
    amps: bool,
}

fn once<T>(slot: &mut Option<T>, v: T, what: &str) -> Result<(), Read<Indicator>> {
    if slot.is_some() {
        return Err(Read::Refused(format!("it gives two {what}")));
    }
    *slot = Some(v);
    Ok(())
}

fn found(toks: &[Tok]) -> Result<Found, Read<Indicator>> {
    let mut f = Found::default();
    for t in toks {
        match *t {
            Tok::Volts(x) => once(&mut f.volts, x, "voltages")?,
            Tok::Current(c) => once(&mut f.current, c, "kinds of current")?,
            Tok::Watts(x) => once(&mut f.watts, x, "powers")?,
            Tok::Bare(x) => once(&mut f.bare, x, "numbers with no unit")?,
            Tok::Signal(s) => once(&mut f.signal, s, "ways of signalling")?,
            Tok::Colour(c) => once(&mut f.colour, c, "colours")?,
            Tok::Amps => f.amps = true,
        }
    }
    Ok(f)
}

fn within(x: f64, max: f64, what: &str, unit: &str) -> Result<f64, Read<Indicator>> {
    if x.is_finite() && x > 0.0 && x <= max { Ok(x) } else { Err(Read::Refused(format!("{what} is more than 0 and at most {max} {unit}"))) }
}

/// An indicator's colour for a conductor colour, when it is one (IEC 60073).
fn indicator_colour(c: ConductorColour) -> Option<IndicatorColour> {
    match c {
        ConductorColour::Red => Some(IndicatorColour::Red),
        ConductorColour::Yellow => Some(IndicatorColour::Yellow),
        ConductorColour::Green => Some(IndicatorColour::Green),
        ConductorColour::Blue => Some(IndicatorColour::Blue),
        ConductorColour::White => Some(IndicatorColour::White),
        ConductorColour::Black
        | ConductorColour::Brown
        | ConductorColour::Orange
        | ConductorColour::Violet
        | ConductorColour::Grey
        | ConductorColour::Pink
        | ConductorColour::Gold
        | ConductorColour::Turquoise
        | ConductorColour::Silver => None,
    }
}

/// Read `text` as the rating of `indicator` (its signal kept unless the text says one).
///
/// # Example
/// ```
/// use crate::devices::ratings::{Read, read_indicator};
/// use ecad_model::objects::devices::{Indicator, SignalType};
/// let lamp = Indicator::empty(SignalType::Steady);
/// let Read::Rating(r) = read_indicator(&lamp, "RD 24VDC 2W") else { panic!() };
/// assert_eq!(r.to_string(), "red, 24 V DC, 2 W");
/// assert!(matches!(read_indicator(&lamp, "24 V"), Read::Ask(_)));
/// assert_eq!(read_indicator(&lamp, "see datasheet"), Read::NotARating);
/// ```
pub fn read_indicator(indicator: &Indicator, text: &str) -> Read<Indicator> {
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
    match build(indicator, &f) {
        Ok(i) => Read::Rating(i),
        Err(r) => r,
    }
}

fn build(indicator: &Indicator, f: &Found) -> Result<Indicator, Read<Indicator>> {
    if f.amps {
        return Err(Read::Refused("an indicator's rating has no current; give its colour, voltage and power, for example red, 24 V DC, 2 W".into()));
    }
    if let Some(x) = f.bare {
        return Err(Read::Ask(format!("a unit for {x}: V (rated voltage) or W (power)?")));
    }
    let colour = match f.colour {
        None => None,
        Some(c) => Some(
            indicator_colour(c)
                .ok_or_else(|| Read::Refused(format!("an indicator is red, yellow, green, blue or white (IEC 60073), not {}", c.forms().name)))?,
        ),
    };
    let rated_voltage = match (f.volts, f.current) {
        (Some(v), Some(c)) => Some(RatedVoltage { volts: Volts(within(v, MAX_VOLTS, "a rated voltage", "V")?), current: c }),
        (Some(v), None) => return Err(Read::Ask(format!("whether {v} V is AC or DC, for example {v} V DC"))),
        (None, Some(c)) => return Err(Read::Ask(format!("the voltage, for example 24 V {}", c.label()))),
        (None, None) => None,
    };
    // Read in watts or kilowatts, kept in watts to the milliwatt.
    let power = f.watts.map(|w| within(w, MAX_POWER_WATTS, "a power", "W").map(|w| Watts((w * 1000.0).round() / 1000.0))).transpose()?;
    Ok(Indicator { signal: f.signal.unwrap_or(indicator.signal), colour, rated_voltage, power })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown_on(signal: SignalType, text: &str) -> String {
        match read_indicator(&Indicator::empty(signal), text) {
            Read::Rating(s) => s.to_string(),
            Read::Ask(m) => format!("ask: {m}"),
            Read::Refused(m) => format!("refused: {m}"),
            Read::NotARating => "note".into(),
        }
    }

    fn shown(text: &str) -> String {
        shown_on(SignalType::Steady, text)
    }

    #[test]
    fn indicator_ratings_read_into_fields_and_back() {
        for (text, want) in [
            ("red", "red"),
            ("RD", "red"),
            ("GN", "green"),
            ("yel", "yellow"),
            ("blue", "blue"),
            ("WH", "white"),
            ("24 V DC", "24 V DC"),
            ("24VDC", "24 V DC"),
            ("230 VAC", "230 V AC"),
            ("230V AC", "230 V AC"),
            ("2 W", "2 W"),
            ("0,5W", "0.5 W"),
            ("flashing", "flashing"),
            ("red flashing", "red, flashing"),
            ("24 V DC 2 W", "24 V DC, 2 W"),
            ("RD, rotating, 24 V DC, 2 W", "red, rotating, 24 V DC, 2 W"),
        ] {
            let got = shown(text);
            assert_eq!(got, want, "{text}");
            // What is shown reads back to the same fields.
            assert_eq!(shown(&got), want, "{got}");
        }
        // The signal is the indicator's own unless the text says one.
        let Read::Rating(i) = read_indicator(&Indicator::empty(SignalType::Flashing), "green") else { panic!() };
        assert_eq!((i.signal, i.colour), (SignalType::Flashing, Some(IndicatorColour::Green)));
        let Read::Rating(i) = read_indicator(&Indicator::empty(SignalType::Flashing), "steady") else { panic!() };
        assert_eq!(i.signal, SignalType::Steady);
    }

    #[test]
    fn ambiguous_indicator_text_is_asked_wrong_values_refused_and_others_kept_as_note() {
        for (text, start) in [
            ("24", "ask: a unit for 24: V (rated voltage) or W (power)?"),
            ("24 V", "ask: whether 24 V is AC or DC"),
            ("DC", "ask: the voltage, for example 24 V DC"),
            ("red 0.1 A", "refused: an indicator's rating has no current"),
            ("black", "refused: an indicator is red, yellow, green, blue or white"),
            ("red green", "refused: 'red green' does not read: it gives two colours"),
            ("flashing steady", "refused: 'flashing steady' does not read: it gives two ways"),
            ("2 W 3 W", "refused: '2 W 3 W' does not read: it gives two powers"),
            ("2000 V DC", "refused: a rated voltage"),
            ("0 W", "refused: a power"),
        ] {
            let got = shown(text);
            assert!(got.starts_with(start), "{text}: {got}");
        }
        for text in ["see datasheet", "LED", "amber", "", "NaN W", &"1".repeat(70)] {
            assert_eq!(shown(text), "note", "{text}");
        }
    }
}
