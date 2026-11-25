//! Reading a switching device's rating: a coil's or SSR input's
//! control voltage and power, the AC-3 rating and its operational voltage, the thermal current
//! and the insulation voltage.
//!
//! # Semantics
//! - `coil 24 V DC, 3 W` fills the control; `24VDC` and `24 V DC` read alike.
//! - `AC-3 9 A` is the AC-3 rated current; a current without `AC-3` is asked about (it may be a
//!   thermal or AC-1 current). `4 kW` is the AC-3 motor power.
//! - A voltage without AC or DC is asked about; so is a number without a unit.
//! - `Ue 400 V` is the operational voltage, `Ith 20 A` the thermal current and `Ui 690 V` the
//!   insulation voltage: each is read only after its letters, so a voltage alone stays the
//!   control's and a current alone is asked about. `AC` after `Ue` or `Ui` is allowed.
//! - A manual switch has no control: a voltage or power on it is refused.
//!
//! This module excludes the protection reader (its parent) and the contacts a device offers.
#![deny(clippy::wildcard_enum_match_arm)]

use ecad_model::geometry::units::{Amps, Current, RatedVoltage, Volts, Watts};
use ecad_model::objects::devices::switching::{
    MAX_AC3_AMPS, MAX_AC3_WATTS, MAX_CONTROL_VOLTS, MAX_CONTROL_WATTS, MAX_INSULATION_VOLTS, MAX_OPERATIONAL_VOLTS,
};
use ecad_model::objects::devices::{Control, Operator, OperatorKind, Switching};

use crate::devices::ratings::parse::{number_unit, pieces};
use crate::devices::ratings::{MAX_CHARS, Problem, Read};

/// One read piece of a switching rating.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok {
    Volts(f64),
    Watts(f64),
    KiloWatts(f64),
    Amps(f64),
    Bare(f64),
    Current(Current),
    /// `AC-3` (or `AC3`).
    Ac3,
    /// `coil` or `input` (before the voltage).
    Filler,
    /// `Ue`: the next voltage is the operational voltage.
    Ue,
    /// `Ui`: the next voltage is the insulation voltage.
    Ui,
    /// `Ith`: the next current is the thermal current.
    Ith,
}

fn unit_token(x: f64, unit: &str) -> Option<Tok> {
    match unit {
        "" => Some(Tok::Bare(x)),
        "v" => Some(Tok::Volts(x)),
        "w" => Some(Tok::Watts(x)),
        "kw" => Some(Tok::KiloWatts(x)),
        "a" => Some(Tok::Amps(x)),
        _ => None,
    }
}

/// A piece's tokens: a word, or a number with its unit (`24V`, `24VDC`, `9A`, `4kW`).
fn piece_tokens(lower: &str) -> Option<Vec<Tok>> {
    match lower {
        "coil" | "input" => return Some(vec![Tok::Filler]),
        "ac" => return Some(vec![Tok::Current(Current::Ac)]),
        "dc" => return Some(vec![Tok::Current(Current::Dc)]),
        "ac-3" | "ac3" => return Some(vec![Tok::Ac3]),
        "ue" => return Some(vec![Tok::Ue]),
        "ui" => return Some(vec![Tok::Ui]),
        "ith" => return Some(vec![Tok::Ith]),
        _ => {}
    }
    let (x, unit) = number_unit(lower)?;
    // A voltage with its kind run on: `24vdc`, `230vac`.
    for (suffix, current) in [("vac", Current::Ac), ("vdc", Current::Dc)] {
        if unit == suffix {
            return Some(vec![Tok::Volts(x), Tok::Current(current)]);
        }
    }
    unit_token(x, unit).map(|t| vec![t])
}

/// The pieces of `text`, or `None` when a piece is no rating word.
fn tokens(text: &str) -> Option<Vec<Tok>> {
    let mut out: Vec<Tok> = Vec::new();
    for lower in pieces(text) {
        // A unit after a bare number: `24 V`, `9 A`, `4 kW`, `24 VDC`.
        if let Some(Tok::Bare(x)) = out.last().copied() {
            let run_on = match lower.as_str() {
                "vac" => Some(vec![Tok::Volts(x), Tok::Current(Current::Ac)]),
                "vdc" => Some(vec![Tok::Volts(x), Tok::Current(Current::Dc)]),
                u => unit_token(x, u).filter(|t| !matches!(t, Tok::Bare(_))).map(|t| vec![t]),
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

/// What one text gives, sorted by field.
#[derive(Default)]
struct Found {
    volts: Option<f64>,
    current: Option<Current>,
    watts: Option<f64>,
    kilowatts: Option<f64>,
    amps: Option<f64>,
    bare: Option<f64>,
    ac3: bool,
    /// After `Ue`, `Ith` and `Ui`.
    operational_volts: Option<f64>,
    thermal_amps: Option<f64>,
    insulation_volts: Option<f64>,
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
    let mut i = 0;
    while let Some(t) = toks.get(i) {
        i += 1;
        match *t {
            Tok::Volts(x) => once(&mut f.volts, x, "voltages")?,
            Tok::Current(c) => once(&mut f.current, c, "kinds of voltage (AC or DC)")?,
            Tok::Watts(x) => once(&mut f.watts, x, "coil powers")?,
            Tok::KiloWatts(x) => once(&mut f.kilowatts, x, "motor powers")?,
            Tok::Amps(x) => once(&mut f.amps, x, "currents")?,
            Tok::Bare(x) => once(&mut f.bare, x, "numbers with no unit")?,
            Tok::Ac3 => f.ac3 = true,
            Tok::Filler => {}
            Tok::Ue | Tok::Ui => {
                let Some(Tok::Volts(x)) = toks.get(i).copied() else {
                    let letters = if *t == Tok::Ue { "Ue 400 V" } else { "Ui 690 V" };
                    return Err(Problem::Ask(format!("the voltage after its letters, for example {letters}")));
                };
                i += 1;
                // `Ue 400 V AC`: AC is part of the voltage, not the control's.
                if toks.get(i) == Some(&Tok::Current(Current::Ac)) {
                    i += 1;
                }
                if *t == Tok::Ue {
                    once(&mut f.operational_volts, x, "operational voltages")?;
                } else {
                    once(&mut f.insulation_volts, x, "insulation voltages")?;
                }
            }
            Tok::Ith => {
                let Some(Tok::Amps(x)) = toks.get(i).copied() else {
                    return Err(Problem::Ask("the thermal current, for example Ith 20 A".into()));
                };
                i += 1;
                once(&mut f.thermal_amps, x, "thermal currents")?;
            }
        }
    }
    Ok(f)
}

fn within(x: f64, max: f64, what: &str, unit: &str) -> Result<f64, Problem> {
    if x.is_finite() && x > 0.0 && x <= max { Ok(x) } else { Err(Problem::Refused(format!("{what} is more than 0 and at most {max} {unit}"))) }
}

/// Read `text` as the rating of a switching device operated by `kind`.
///
/// # Example
/// ```
/// use ecad_electrical::devices::ratings::{Read, read_switching};
/// use ecad_model::objects::devices::OperatorKind;
/// let Read::Rating(k) = read_switching(OperatorKind::Coil, "24VDC AC-3 9A") else { panic!() };
/// assert_eq!(k.to_string(), "coil 24 V DC, AC-3 9 A");
/// assert!(matches!(read_switching(OperatorKind::Coil, "24 V"), Read::Ask(_)));
/// assert_eq!(read_switching(OperatorKind::Coil, "see datasheet"), Read::NotARating);
/// ```
pub fn read_switching(kind: OperatorKind, text: &str) -> Read<Switching> {
    let t = text.trim();
    if t.is_empty() || t.chars().nth(MAX_CHARS).is_some() {
        return Read::NotARating;
    }
    let Some(toks) = tokens(t) else { return Read::NotARating };
    if toks.iter().all(|t| *t == Tok::Filler) {
        return Read::NotARating;
    }
    let f = match found(&toks) {
        Ok(f) => f,
        Err(Problem::Refused(e)) => return Read::Refused(format!("'{t}' does not read: {e}")),
        Err(Problem::Ask(q)) => return Read::Ask(q),
    };
    match build(kind, &f) {
        Ok(s) => Read::Rating(s),
        Err(Problem::Ask(q)) => Read::Ask(q),
        Err(Problem::Refused(m)) => Read::Refused(m),
    }
}

fn build(kind: OperatorKind, f: &Found) -> Result<Switching, Problem> {
    let ask = |q: String| Err(Problem::Ask(q));
    if let Some(x) = f.bare {
        return ask(match kind {
            OperatorKind::Coil | OperatorKind::SolidState => {
                format!("a unit for {x}: V (control voltage), W (power), A (AC-3 current) or kW (motor power)?")
            }
            OperatorKind::Manual => format!("a unit for {x}: A (AC-3 current) or kW (motor power)?"),
        });
    }
    let control = f.volts.is_some() || f.current.is_some() || f.watts.is_some();
    if control && kind == OperatorKind::Manual {
        return Err(Problem::Refused("a manual switch has no coil; its rating is AC-3 (AC-3 9 A, 4 kW)".into()));
    }
    let voltage = match (f.volts, f.current) {
        (Some(v), Some(c)) => Some(RatedVoltage { volts: Volts(within(v, MAX_CONTROL_VOLTS, "a control voltage", "V")?), current: c }),
        (Some(v), None) => return ask(format!("whether {v} V is AC or DC, for example {v} V DC")),
        (None, Some(c)) => return ask(format!("the voltage, for example 24 V {}", c.label())),
        (None, None) => None,
    };
    let power = f.watts.map(|w| within(w, MAX_CONTROL_WATTS, "a coil power", "W").map(Watts)).transpose()?;
    let ac3_current = match (f.amps, f.ac3) {
        (Some(a), true) => Some(Amps(within(a, MAX_AC3_AMPS, "an AC-3 current", "A")?)),
        (Some(a), false) => return ask(format!("whether {a} A is the AC-3 rated current; write AC-3 {a} A")),
        (None, _) => None,
    };
    // Read in kilowatts, kept in watts to the milliwatt (0.37 kW is 370 W).
    let ac3_power =
        f.kilowatts.map(|k| within(k * 1000.0, MAX_AC3_WATTS, "a motor power", "W").map(|w| Watts((w * 1000.0).round() / 1000.0))).transpose()?;
    if f.ac3 && ac3_current.is_none() && ac3_power.is_none() {
        return ask("the AC-3 current or motor power, for example AC-3 9 A or 4 kW".into());
    }
    let operational_voltage = f.operational_volts.map(|v| within(v, MAX_OPERATIONAL_VOLTS, "an operational voltage", "V").map(Volts)).transpose()?;
    let thermal_current = f.thermal_amps.map(|a| within(a, MAX_AC3_AMPS, "a thermal current", "A").map(Amps)).transpose()?;
    let insulation_voltage = f.insulation_volts.map(|v| within(v, MAX_INSULATION_VOLTS, "an insulation voltage", "V").map(Volts)).transpose()?;
    let operator = match kind {
        OperatorKind::Coil => Operator::Coil(Control { voltage, power }),
        OperatorKind::SolidState => Operator::SolidState(Control { voltage, power }),
        OperatorKind::Manual => Operator::Manual,
    };
    Ok(Switching { operator, ac3_current, ac3_power, operational_voltage, thermal_current, insulation_voltage, ..Switching::default() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use OperatorKind as K;

    fn shown(kind: K, text: &str) -> String {
        match read_switching(kind, text) {
            Read::Rating(s) => s.to_string(),
            Read::Ask(m) => format!("ask: {m}"),
            Read::Refused(m) => format!("refused: {m}"),
            Read::NotARating => "note".into(),
        }
    }

    #[test]
    fn switching_ratings_read_into_fields_and_back() {
        for (kind, text, want) in [
            (K::Coil, "24 V DC", "coil 24 V DC"),
            (K::Coil, "24VDC", "coil 24 V DC"),
            (K::Coil, "230 VAC 4 W", "coil 230 V AC, 4 W"),
            (K::Coil, "coil 24 V DC, 3 W", "coil 24 V DC, 3 W"),
            (K::Coil, "24V DC AC-3 9A 4kW", "coil 24 V DC, AC-3 9 A, 4 kW"),
            (K::Coil, "AC3 12 A", "AC-3 12 A"),
            (K::Coil, "0,37 kW", "0.37 kW"),
            (K::SolidState, "24 V DC", "input 24 V DC"),
            (K::Manual, "AC-3 25 A", "AC-3 25 A"),
            (K::Manual, "7.5 kW", "7.5 kW"),
            (K::Coil, "24 V DC AC-3 9 A 4 kW Ue 400 V Ith 20 A Ui 690 V", "coil 24 V DC, AC-3 9 A, 4 kW, Ue 400 V, Ith 20 A, Ui 690 V"),
            (K::Coil, "Ui 690V Ue 400 V AC 230 V AC", "coil 230 V AC, Ue 400 V, Ui 690 V"),
            (K::Manual, "Ith 10 A, Ui 250 V", "Ith 10 A, Ui 250 V"),
            (K::Manual, "ITH 16A", "Ith 16 A"),
        ] {
            let got = shown(kind, text);
            assert_eq!(got, want, "{text}");
            // What is shown reads back to the same fields.
            assert_eq!(shown(kind, &got), want, "{got}");
        }
    }

    #[test]
    fn ambiguous_switching_text_is_asked_and_wrong_values_refused() {
        for (kind, text, start) in [
            (K::Coil, "24", "ask: a unit for 24"),
            (K::Coil, "24 V", "ask: whether 24 V is AC or DC"),
            (K::Coil, "DC", "ask: the voltage"),
            (K::Coil, "9 A", "ask: whether 9 A is the AC-3"),
            (K::Coil, "AC-3", "ask: the AC-3 current"),
            (K::Manual, "24", "ask: a unit for 24: A"),
            (K::Manual, "24 V DC", "refused: a manual switch has no coil"),
            (K::Coil, "24 V DC 230 V AC", "refused: '24 V DC 230 V AC' does not read: it gives two voltages"),
            (K::Coil, "2000 V DC", "refused: a control voltage"),
            (K::Coil, "AC-3 0 A", "refused: an AC-3 current"),
            (K::Coil, "Ue", "ask: the voltage after its letters, for example Ue 400 V"),
            (K::Coil, "Ui 9 A", "ask: the voltage after its letters, for example Ui 690 V"),
            (K::Manual, "Ith 400 V", "ask: the thermal current"),
            (K::Manual, "Ue 2000 V", "refused: an operational voltage"),
            (K::Manual, "Ui 0 V", "refused: an insulation voltage"),
            (K::Manual, "Ith 20 A Ith 25 A", "refused: 'Ith 20 A Ith 25 A' does not read: it gives two thermal currents"),
        ] {
            let got = shown(kind, text);
            assert!(got.starts_with(start), "{text}: {got}");
        }
        for text in ["see datasheet", "24 V AC/DC", "5 VA", "", "coil", "NaN V DC", &"1".repeat(170)] {
            assert_eq!(shown(K::Coil, text), "note", "{text}");
        }
    }
}
