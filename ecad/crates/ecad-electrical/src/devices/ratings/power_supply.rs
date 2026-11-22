//! Reading a power supply's or a transformer's rating (design 072 §2.2 row 1, overseer rulings
//! 2026-11-18): its input and output voltages, a power supply's output current and power, a
//! transformer's VA.
//!
//! # Semantics
//! - `230 V AC / 24 V DC` (also `230VAC/24VDC`) is input / output, and so is `in 230 V AC out 24 V
//!   DC`. `400/24 V` gives both sides the one unit.
//! - One voltage alone is read only where it cannot be anything else: a power supply's DC voltage
//!   is its output (its DC side). A power supply's AC voltage alone, a transformer's voltage alone
//!   and two voltages with no `/` or `in`/`out` are asked about (input or output?).
//! - A power supply's voltage needs AC or DC, or it is asked about. A transformer is AC on both
//!   sides: its voltages need no AC, and DC is refused.
//! - A power supply's output current is `5 A` (or `500 mA`) and its output power `120 W` (or
//!   `1,2 kW`); both may be given, and they are not checked against each other yet. A
//!   transformer's power is `250 VA` (or `2,5 kVA`); W or a current on a transformer is refused,
//!   VA on a power supply too. A current or a power on the input side is refused.
//! - A bare number is asked about (which unit?). Any other word (`see datasheet`) is no rating: the
//!   note.
//!
//! This module excludes the protection reader (its parent) and the supply calculations.
#![deny(clippy::wildcard_enum_match_arm)]

use ecad_model::device::power_supply::{MAX_AMPS, MAX_POWER_WATTS, MAX_VOLT_AMPS, MAX_VOLTS};
use ecad_model::device::{PowerSupply, Psu, Transformer};
use ecad_model::units::{Amps, Current, RatedVoltage, VoltAmps, Volts, Watts};

use super::load::pieces;
use super::{MAX_CHARS, Read};

/// Which side of the supply a value is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Input,
    Output,
}

impl Side {
    fn word(self) -> &'static str {
        match self {
            Side::Input => "input",
            Side::Output => "output",
        }
    }
}

/// One read piece of a supply rating.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok {
    Volts(f64),
    Current(Current),
    Amps(f64),
    Watts(f64),
    VoltAmps(f64),
    Bare(f64),
    Side(Side),
    Slash,
}

fn unit_tokens(x: f64, unit: &str) -> Option<Vec<Tok>> {
    match unit {
        "" => Some(vec![Tok::Bare(x)]),
        "v" => Some(vec![Tok::Volts(x)]),
        "vac" => Some(vec![Tok::Volts(x), Tok::Current(Current::Ac)]),
        "vdc" => Some(vec![Tok::Volts(x), Tok::Current(Current::Dc)]),
        "a" => Some(vec![Tok::Amps(x)]),
        "ma" => Some(vec![Tok::Amps(x / 1000.0)]),
        "w" => Some(vec![Tok::Watts(x)]),
        "kw" => Some(vec![Tok::Watts(x * 1000.0)]),
        "va" => Some(vec![Tok::VoltAmps(x)]),
        "kva" => Some(vec![Tok::VoltAmps(x * 1000.0)]),
        _ => None,
    }
}

/// A piece's tokens: a word, or a number with its unit (`24V`, `24VDC`, `5A`, `250VA`).
fn piece_tokens(lower: &str) -> Option<Vec<Tok>> {
    let word = match lower {
        "ac" => Some(Tok::Current(Current::Ac)),
        "dc" => Some(Tok::Current(Current::Dc)),
        "in" | "input" => Some(Tok::Side(Side::Input)),
        "out" | "output" => Some(Tok::Side(Side::Output)),
        "/" => Some(Tok::Slash),
        _ => None,
    };
    if let Some(w) = word {
        return Some(vec![w]);
    }
    let split = lower.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(lower.len());
    let (num, unit) = lower.split_at(split);
    unit_tokens(crate::rating::parse::decimal(num)?, unit)
}

/// The pieces of `text` (a `/` its own piece), or `None` when a piece is no rating word.
fn tokens(text: &str) -> Option<Vec<Tok>> {
    let mut out: Vec<Tok> = Vec::new();
    for p in pieces(&text.replace('/', " / ")) {
        // A unit after a bare number: `24 V`, `5 A`, `24 VDC`.
        if let Some(Tok::Bare(x)) = out.last().copied()
            && let Some(t) = unit_tokens(x, &p).filter(|t| !matches!(t.as_slice(), [Tok::Bare(_)]))
        {
            out.pop();
            out.extend(t);
            continue;
        }
        out.extend(piece_tokens(&p)?);
    }
    // `400/24 V`: the number before the `/` takes the voltage's unit after it.
    for i in 0..out.len().saturating_sub(2) {
        if let [Tok::Bare(x), Tok::Slash, Tok::Volts(_)] = out[i..i + 3] {
            out[i] = Tok::Volts(x);
        }
    }
    Some(out)
}

/// A voltage as written, with the side it was given on (if any).
#[derive(Debug, Clone, Copy)]
struct Voltage {
    volts: f64,
    current: Option<Current>,
    side: Option<Side>,
}

/// What one text gives, sorted by field.
#[derive(Default)]
struct Found {
    voltages: Vec<Voltage>,
    amps: Option<f64>,
    watts: Option<f64>,
    volt_amps: Option<f64>,
    bare: Option<f64>,
    /// A current or a power given on the input side.
    input_value: bool,
    /// AC or DC with no voltage before it.
    lone_current: Option<Current>,
}

type Err = Read<PowerSupply>;

fn once<T>(slot: &mut Option<T>, v: T, what: &str) -> Result<(), Err> {
    if slot.is_some() {
        return Err(Read::Refused(format!("it gives two {what}")));
    }
    *slot = Some(v);
    Ok(())
}

fn found(toks: &[Tok]) -> Result<Found, Err> {
    let mut f = Found::default();
    let mut side: Option<Side> = None;
    let mut slashed = false;
    let mut prev: Option<Tok> = None;
    for t in toks {
        match *t {
            Tok::Volts(x) => f.voltages.push(Voltage { volts: x, current: None, side }),
            Tok::Current(c) => match (prev, f.voltages.last_mut()) {
                (Some(Tok::Volts(_)), Some(v)) if v.current.is_none() => v.current = Some(c),
                _ => {
                    if f.lone_current.is_none() {
                        f.lone_current = Some(c);
                    }
                }
            },
            Tok::Amps(x) => {
                f.input_value |= side == Some(Side::Input);
                once(&mut f.amps, x, "currents")?;
            }
            Tok::Watts(x) => {
                f.input_value |= side == Some(Side::Input);
                once(&mut f.watts, x, "powers")?;
            }
            Tok::VoltAmps(x) => {
                f.input_value |= side == Some(Side::Input);
                once(&mut f.volt_amps, x, "powers")?;
            }
            Tok::Bare(x) => once(&mut f.bare, x, "numbers with no unit")?,
            Tok::Side(s) => side = Some(s),
            Tok::Slash => {
                if slashed {
                    return Err(Read::Refused("it gives two '/'".into()));
                }
                slashed = true;
                for v in f.voltages.iter_mut() {
                    match v.side {
                        None | Some(Side::Input) => v.side = Some(Side::Input),
                        Some(Side::Output) => return Err(Read::Refused("an output voltage is before the '/' (input / output)".into())),
                    }
                }
                side = Some(Side::Output);
            }
        }
        prev = Some(*t);
    }
    Ok(f)
}

fn within(x: f64, max: f64, what: &str, unit: &str) -> Result<f64, Err> {
    if x.is_finite() && x > 0.0 && x <= max { Ok(x) } else { Err(Read::Refused(format!("{what} is more than 0 and at most {max} {unit}"))) }
}

/// Kept to the thousandth (a milliamp, a milliwatt).
fn milli(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// Read `text` as the rating of `supply` (its kind kept: a power supply stays one).
///
/// # Example
/// ```
/// use ecad_electrical::rating::reader::{Read, read_power_supply};
/// use ecad_model::device::{PowerSupply, Psu, Transformer};
/// let psu = PowerSupply::Psu(Psu::default());
/// let Read::Rating(r) = read_power_supply(&psu, "230VAC/24VDC 5A") else { panic!() };
/// assert_eq!(r.to_string(), "230 V AC / 24 V DC, 5 A");
/// let trafo = PowerSupply::Transformer(Transformer::default());
/// let Read::Rating(r) = read_power_supply(&trafo, "400/24 V 250 VA") else { panic!() };
/// assert_eq!(r.to_string(), "400 V AC / 24 V AC, 250 VA");
/// assert!(matches!(read_power_supply(&psu, "230 V AC"), Read::Ask(_)));
/// assert_eq!(read_power_supply(&psu, "see datasheet"), Read::NotARating);
/// ```
pub fn read_power_supply(supply: &PowerSupply, text: &str) -> Read<PowerSupply> {
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
    match build(supply, &f) {
        Ok(s) => Read::Rating(s),
        Err(r) => r,
    }
}

/// The input and output voltages, each with its side settled (never guessed).
fn sides(supply: &PowerSupply, f: &Found) -> Result<(Option<Voltage>, Option<Voltage>), Err> {
    let mut input = None;
    let mut output = None;
    let unsided: Vec<Voltage> = f.voltages.iter().copied().filter(|v| v.side.is_none()).collect();
    for v in f.voltages.iter().filter(|v| v.side.is_some()) {
        let slot = if v.side == Some(Side::Input) { &mut input } else { &mut output };
        let word = v.side.map_or("", Side::word);
        once(slot, *v, &format!("{word} voltages"))?;
    }
    match unsided.as_slice() {
        [] => {}
        [v] if input.is_none() && output.is_none() => {
            // A power supply's DC side is its output; anything else alone is asked about.
            let dc_psu = matches!(supply, PowerSupply::Psu(_)) && v.current == Some(Current::Dc);
            if !dc_psu {
                return Err(Read::Ask(format!("whether {} V is the input or the output, for example in {} V AC or out 24 V DC", v.volts, v.volts)));
            }
            output = Some(*v);
        }
        [v] => {
            // The other side is named: this one is the side left.
            let slot = if input.is_none() { &mut input } else { &mut output };
            once(slot, *v, "voltages for one side")?;
        }
        [..] => return Err(Read::Ask("which voltage is the input and which the output, for example 230 V AC / 24 V DC".into())),
    }
    Ok((input, output))
}

fn build(supply: &PowerSupply, f: &Found) -> Result<PowerSupply, Err> {
    let kind = supply.label();
    if let Some(x) = f.bare {
        let units = match supply {
            PowerSupply::Psu(_) => "V, A or W",
            PowerSupply::Transformer(_) => "V or VA",
        };
        return Err(Read::Ask(format!("a unit for {x}: {units}?")));
    }
    if f.input_value {
        return Err(Read::Refused(format!("a {kind}'s current and power are its output's, not its input's")));
    }
    if let (Some(c), true) = (f.lone_current, f.voltages.is_empty()) {
        return Err(Read::Ask(format!("the voltage, for example 24 V {}", c.label())));
    }
    if f.lone_current.is_some() {
        return Err(Read::Refused("AC or DC is given apart from its voltage; write it after it, for example 24 V DC".into()));
    }
    let (input, output) = sides(supply, f)?;
    match supply {
        PowerSupply::Psu(_) => {
            if f.volt_amps.is_some() {
                return Err(Read::Refused("a power supply's output power is in W, for example 120 W".into()));
            }
            let voltage = |v: Option<Voltage>, side: Side| -> Result<Option<RatedVoltage>, Err> {
                let Some(v) = v else { return Ok(None) };
                let Some(current) = v.current else {
                    return Err(Read::Ask(format!("whether the {} {} V is AC or DC, for example {} V DC", side.word(), v.volts, v.volts)));
                };
                Ok(Some(RatedVoltage { volts: Volts(within(v.volts, MAX_VOLTS, "a voltage", "V")?), current }))
            };
            let output_current = f.amps.map(|a| within(a, MAX_AMPS, "an output current", "A").map(|a| Amps(milli(a)))).transpose()?;
            let output_power = f.watts.map(|w| within(w, MAX_POWER_WATTS, "an output power", "W").map(|w| Watts(milli(w)))).transpose()?;
            Ok(PowerSupply::Psu(Psu { input: voltage(input, Side::Input)?, output: voltage(output, Side::Output)?, output_current, output_power }))
        }
        PowerSupply::Transformer(_) => {
            if f.watts.is_some() {
                return Err(Read::Refused("a transformer is rated in VA, not W, for example 250 VA".into()));
            }
            if f.amps.is_some() {
                return Err(Read::Refused("a transformer's rating has no current; give its voltages and VA, for example 400/24 V 250 VA".into()));
            }
            let voltage = |v: Option<Voltage>| -> Result<Option<RatedVoltage>, Err> {
                let Some(v) = v else { return Ok(None) };
                if v.current == Some(Current::Dc) {
                    return Err(Read::Refused("a transformer is AC on both sides, not DC".into()));
                }
                Ok(Some(RatedVoltage { volts: Volts(within(v.volts, MAX_VOLTS, "a voltage", "V")?), current: Current::Ac }))
            };
            let rated_power = f.volt_amps.map(|va| within(va, MAX_VOLT_AMPS, "a rated power", "VA").map(|va| VoltAmps(milli(va)))).transpose()?;
            Ok(PowerSupply::Transformer(Transformer { input: voltage(input)?, output: voltage(output)?, rated_power }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown_on(supply: &PowerSupply, text: &str) -> String {
        match read_power_supply(supply, text) {
            Read::Rating(s) => s.to_string(),
            Read::Ask(m) => format!("ask: {m}"),
            Read::Refused(m) => format!("refused: {m}"),
            Read::NotARating => "note".into(),
        }
    }

    fn psu(text: &str) -> String {
        shown_on(&PowerSupply::Psu(Psu::default()), text)
    }

    fn trafo(text: &str) -> String {
        shown_on(&PowerSupply::Transformer(Transformer::default()), text)
    }

    #[test]
    fn supply_ratings_read_into_fields_and_back() {
        for (text, want) in [
            ("24 V DC", "24 V DC"),
            ("24VDC", "24 V DC"),
            ("24 V DC 5 A", "24 V DC, 5 A"),
            ("24VDC 500mA", "24 V DC, 0.5 A"),
            ("24 V DC 120 W", "24 V DC, 120 W"),
            ("24 V DC 5 A 120 W", "24 V DC, 5 A, 120 W"),
            ("230 V AC / 24 V DC", "230 V AC / 24 V DC"),
            ("230VAC/24VDC", "230 V AC / 24 V DC"),
            ("230VAC / 24VDC, 10A", "230 V AC / 24 V DC, 10 A"),
            ("in 230 V AC out 24 V DC", "230 V AC / 24 V DC"),
            ("out 24 V DC in 230 V AC", "230 V AC / 24 V DC"),
            ("in 230 V AC", "in 230 V AC"),
            ("out 24 V AC", "out 24 V AC"),
            ("in 24 V DC / 12 V DC", "24 V DC / 12 V DC"),
            ("1,2 kW 24 V DC", "24 V DC, 1.2 kW"),
            ("5 A", "5 A"),
        ] {
            let got = psu(text);
            assert_eq!(got, want, "{text}");
            assert_eq!(psu(&got), want, "{got}");
        }
        for (text, want) in [
            ("400/24 V", "400 V AC / 24 V AC"),
            ("400/24 V 250 VA", "400 V AC / 24 V AC, 250 VA"),
            ("400 V AC / 24 V AC", "400 V AC / 24 V AC"),
            ("230V/24V", "230 V AC / 24 V AC"),
            ("in 400 V out 230 V", "400 V AC / 230 V AC"),
            ("out 24 V", "out 24 V AC"),
            ("2,5 kVA", "2.5 kVA"),
            ("250VA", "250 VA"),
        ] {
            let got = trafo(text);
            assert_eq!(got, want, "{text}");
            assert_eq!(trafo(&got), want, "{got}");
        }
        let Read::Rating(PowerSupply::Transformer(t)) = read_power_supply(&PowerSupply::Transformer(Transformer::default()), "400/24 V") else {
            panic!()
        };
        assert_eq!(t.ratio(), Some(400.0 / 24.0));
    }

    #[test]
    fn ambiguous_supply_text_is_asked_wrong_values_refused_and_others_kept_as_note() {
        for (text, start) in [
            ("230 V AC", "ask: whether 230 V is the input or the output"),
            ("24 V", "ask: whether 24 V is the input or the output"),
            ("230 V AC 24 V DC", "ask: which voltage is the input"),
            ("in 230 V / 24 V DC", "ask: whether the input 230 V is AC or DC"),
            ("24", "ask: a unit for 24: V, A or W?"),
            ("DC", "ask: the voltage, for example 24 V DC"),
            ("24 V DC 250 VA", "refused: a power supply's output power is in W"),
            ("in 230 V AC 2 A", "refused: a power supply's current and power are its output's"),
            ("24 V DC 5 A 6 A", "refused: '24 V DC 5 A 6 A' does not read: it gives two currents"),
            ("out 24 V DC out 12 V DC", "refused: it gives two output voltages"),
            ("24 V DC / 12 V DC / 5 V DC", "refused: '"),
            ("2000 V DC", "refused: a voltage"),
            ("24 V DC 0 A", "refused: an output current"),
        ] {
            let got = psu(text);
            assert!(got.starts_with(start), "{text}: {got}");
        }
        for (text, start) in [
            ("24 V AC", "ask: whether 24 V is the input or the output"),
            ("24", "ask: a unit for 24: V or VA?"),
            ("400/24 V DC", "refused: a transformer is AC on both sides"),
            ("250 W", "refused: a transformer is rated in VA, not W"),
            ("400/24 V 10 A", "refused: a transformer's rating has no current"),
        ] {
            let got = trafo(text);
            assert!(got.starts_with(start), "{text}: {got}");
        }
        for text in ["see datasheet", "SITOP", "", "NaN V", &"1".repeat(70)] {
            assert_eq!(psu(text), "note", "{text}");
            assert_eq!(trafo(text), "note", "{text}");
        }
    }
}
