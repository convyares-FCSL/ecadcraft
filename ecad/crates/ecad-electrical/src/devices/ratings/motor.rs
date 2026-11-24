//! Reading a motor's rating: its nameplate power, voltage, current, frequency, speed, power factor,
//! efficiency and its class, winding connection, duty, insulation class, IP code, and starting.
//!
//! # Semantics
//! - `7.5 kW` and `7500 W` are the rated power; `400 V` the rated voltage; `15.2 A` the rated
//!   current; `50 Hz` the rated frequency; `0,37 kW` reads as `0.37 kW` (a decimal comma).
//! - `cos φ 0.85` (also `cosφ`, `cos phi`, `pf`) is the power factor; `eff 91 %` (also `η`) the
//!   efficiency; `IE3` its class.
//! - `1450 rpm` (or `1450 1/min`) is the rated speed; `Y`, `Δ` (`star`, `delta`) or `Y/Δ` the
//!   winding connection (`Y-D` stays star-delta starting); `S1` to `S10` the duty; `class F` the
//!   insulation class; `IP55` (or `IP 55`) the degree of protection.
//! - `DOL` (or `direct`), `star-delta` (`Y-D`), `soft start` (`soft-start`) and `VFD` (`VSD`) say how
//!   it is started.
//! - Ambiguous text is asked about: a number without a unit (kW or A?), a percentage that does not
//!   say it is the efficiency. A DC voltage is refused: a motor here runs on AC.
//! - Any other word (`see datasheet`) is no rating: the note.
//!
//! This module excludes the protection reader (its parent) and the calculations from the rating.
#![deny(clippy::wildcard_enum_match_arm)]

use ecad_model::Word;
use ecad_model::geometry::units::{Amps, Hertz, Percent, Rpm, Volts, Watts};
use ecad_model::objects::devices::motor::{MAX_CURRENT_AMPS, MAX_FREQUENCY_HERTZ, MAX_POWER_WATTS, MAX_SPEED_RPM, MAX_VOLTAGE_VOLTS};
use ecad_model::objects::devices::{DutyType, EfficiencyClass, InsulationClass, IpCode, Motor, Starting, WindingConnection};

use crate::devices::ratings::parse::{number_unit, pieces};
use crate::devices::ratings::{MAX_CHARS, Problem, Read};

/// One read piece of a motor rating.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok {
    Watts(f64),
    KiloWatts(f64),
    Amps(f64),
    Percent(f64),
    Volts(f64),
    DcVolts,
    Hertz(f64),
    Bare(f64),
    /// `cos φ`, `pf`: the next number is the power factor.
    PowerFactor,
    /// `eff`, `η`: the next percentage is the efficiency.
    Efficiency,
    Starting(Starting),
    /// `soft` (before `start`).
    Soft,
    Rpm(f64),
    EfficiencyClass(EfficiencyClass),
    Connection(WindingConnection),
    Duty(DutyType),
    /// `class` (before an insulation class's letter).
    ClassWord,
    Insulation(InsulationClass),
    /// `IP` (before its numerals).
    IpWord,
    Ip(IpCode),
}

fn unit_token(x: f64, unit: &str) -> Option<Tok> {
    match unit {
        "" => Some(Tok::Bare(x)),
        "w" => Some(Tok::Watts(x)),
        "kw" => Some(Tok::KiloWatts(x)),
        "a" => Some(Tok::Amps(x)),
        "%" => Some(Tok::Percent(x)),
        "v" | "vac" => Some(Tok::Volts(x)),
        "kv" => Some(Tok::Volts(x * 1000.0)),
        "vdc" => Some(Tok::DcVolts),
        "hz" => Some(Tok::Hertz(x)),
        "rpm" | "1/min" => Some(Tok::Rpm(x)),
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
        "star" | "y" => return Some(Tok::Connection(WindingConnection::Star)),
        "delta" | "δ" => return Some(Tok::Connection(WindingConnection::Delta)),
        "star/delta" | "star_delta" | "y/δ" | "δ/y" => return Some(Tok::Connection(WindingConnection::StarDelta)),
        "class" => return Some(Tok::ClassWord),
        "ip" => return Some(Tok::IpWord),
        _ => {}
    }
    if let Some(c) = EfficiencyClass::parse(lower) {
        return Some(Tok::EfficiencyClass(c));
    }
    if let Some(d) = DutyType::parse(lower) {
        return Some(Tok::Duty(d));
    }
    if let Some(ip) = IpCode::parse(lower) {
        return Some(Tok::Ip(ip));
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
            (Some(Tok::ClassWord), u) => InsulationClass::parse(u).map(Tok::Insulation),
            (Some(Tok::IpWord), u) => IpCode::parse(&format!("ip{u}")).map(Tok::Ip),
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
    volts: Option<f64>,
    hertz: Option<f64>,
    dc: bool,
    rpm: Option<f64>,
    efficiency_class: Option<EfficiencyClass>,
    connection: Option<WindingConnection>,
    duty: Option<DutyType>,
    insulation: Option<InsulationClass>,
    ip: Option<IpCode>,
}

fn once<T>(slot: &mut Option<T>, v: T, what: &str) -> Result<(), Problem> {
    if slot.is_some() {
        return Err(Problem::Refused(format!("it gives two {what}")));
    }
    *slot = Some(v);
    Ok(())
}

/// The fields of `toks`; a marker (`cos φ`, `eff`) takes the number after it.
fn found(toks: &[Tok]) -> Result<Found, Problem> {
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
            Tok::Volts(x) => once(&mut f.volts, x, "voltages")?,
            Tok::Hertz(x) => once(&mut f.hertz, x, "frequencies")?,
            Tok::DcVolts => f.dc = true,
            Tok::PowerFactor => match toks.get(i) {
                Some(Tok::Bare(x)) => {
                    i += 1;
                    once(&mut f.power_factor, *x, "power factors")?;
                }
                _ => return Err(Problem::Ask("the power factor, for example cos φ 0.85".into())),
            },
            Tok::Efficiency => match toks.get(i) {
                Some(Tok::Percent(x)) => {
                    i += 1;
                    once(&mut f.efficiency, *x, "efficiencies")?;
                }
                _ => return Err(Problem::Ask("the efficiency in percent, for example eff 91 %".into())),
            },
            Tok::Soft => return Err(Problem::Ask("whether it has a soft starter; write soft start".into())),
            Tok::Rpm(x) => once(&mut f.rpm, x, "speeds")?,
            Tok::EfficiencyClass(c) => once(&mut f.efficiency_class, c, "efficiency classes")?,
            Tok::Connection(c) => once(&mut f.connection, c, "winding connections")?,
            Tok::Duty(d) => once(&mut f.duty, d, "duty types")?,
            Tok::Insulation(c) => once(&mut f.insulation, c, "insulation classes")?,
            Tok::Ip(ip) => once(&mut f.ip, ip, "IP codes")?,
            Tok::ClassWord => return Err(Problem::Ask("the insulation class (B, F or H), for example class F".into())),
            Tok::IpWord => return Err(Problem::Ask("the IP code, for example IP55".into())),
        }
    }
    Ok(f)
}

fn within(x: f64, max: f64, what: &str, unit: &str) -> Result<f64, Problem> {
    if x.is_finite() && x > 0.0 && x <= max { Ok(x) } else { Err(Problem::Refused(format!("{what} is more than 0 and at most {max}{unit}"))) }
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
        Err(Problem::Refused(e)) => return Read::Refused(format!("'{t}' does not read: {e}")),
        Err(Problem::Ask(q)) => return Read::Ask(q),
    };
    match build(motor, &f) {
        Ok(m) => Read::Rating(m),
        Err(Problem::Ask(q)) => Read::Ask(q),
        Err(Problem::Refused(m)) => Read::Refused(m),
    }
}

fn build(motor: &Motor, f: &Found) -> Result<Motor, Problem> {
    if f.dc {
        return Err(Problem::Refused("a motor here runs on AC; give its voltage without DC, for example 400 V".into()));
    }
    if let Some(x) = f.bare {
        return Err(Problem::Ask(format!("a unit for {x}: kW (rated power) or A (rated current)?")));
    }
    if let Some(x) = f.percent {
        return Err(Problem::Ask(format!("whether {x} % is the efficiency; write eff {x} %")));
    }
    // Read in watts or kilowatts, kept in watts to the milliwatt (0.37 kW is 370 W).
    let rated_power = f.watts.map(|w| within(w, MAX_POWER_WATTS, "a rated power", " W").map(|w| Watts((w * 1000.0).round() / 1000.0))).transpose()?;
    let rated_voltage = f.volts.map(|v| within(v, MAX_VOLTAGE_VOLTS, "a rated voltage", " V").map(Volts)).transpose()?;
    let rated_current = f.amps.map(|a| within(a, MAX_CURRENT_AMPS, "a rated current", " A").map(Amps)).transpose()?;
    let rated_frequency = f.hertz.map(|h| within(h, MAX_FREQUENCY_HERTZ, "a rated frequency", " Hz").map(Hertz)).transpose()?;
    let power_factor = f.power_factor.map(|p| within(p, 1.0, "a power factor", "")).transpose()?;
    let efficiency = f.efficiency.map(|e| within(e, 100.0, "an efficiency", " %").map(Percent)).transpose()?;
    let rated_speed = f.rpm.map(|n| within(n, MAX_SPEED_RPM, "a rated speed", " rpm").map(Rpm)).transpose()?;
    Ok(Motor {
        rated_power,
        rated_voltage,
        rated_current,
        rated_frequency,
        rated_speed,
        power_factor,
        efficiency,
        efficiency_class: f.efficiency_class,
        connection: f.connection,
        duty: f.duty,
        insulation_class: f.insulation,
        ip_rating: f.ip,
        starting: f.starting,
        ..motor.unrated()
    })
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
            ("400 V", "400 V"),
            ("400VAC", "400 V"),
            ("50 Hz", "50 Hz"),
            ("7.5 kW 400 V 15.2 A 50 Hz", "7.5 kW, 400 V, 15.2 A, 50 Hz"),
            ("1450 rpm", "1450 rpm"),
            ("1450 1/min", "1450 rpm"),
            ("IE3", "IE3"),
            ("delta", "Δ"),
            ("Y", "Y"),
            ("Y/Δ", "Y/Δ"),
            ("S3", "S3"),
            ("class F", "class F"),
            ("IP 55", "IP55"),
            ("ipx4", "IPX4"),
            (
                "7.5 kW 400 V 15.2 A 50 Hz 1460 rpm cos φ 0.85 eff 91 % IE3 Δ S1 class F IP55 star-delta",
                "7.5 kW, 400 V, 15.2 A, 50 Hz, 1460 rpm, cos φ 0.85, eff 91 %, IE3, Δ, S1, class F, IP55, star-delta",
            ),
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
            ("7.5 kW 24 VDC", "refused: a motor here runs on AC"),
            ("400 V 230 V", "refused: '400 V 230 V' does not read: it gives two voltages"),
            ("0 Hz", "refused: a rated frequency"),
            ("7.5 kW 5 kW", "refused: '7.5 kW 5 kW' does not read: it gives two powers"),
            ("DOL VFD", "refused: 'DOL VFD' does not read: it gives two ways"),
            ("cos φ 1.2", "refused: a power factor is more than 0 and at most 1"),
            ("eff 120 %", "refused: an efficiency"),
            ("0 kW", "refused: a rated power"),
            ("20000 A", "refused: a rated current"),
            ("class", "ask: the insulation class"),
            ("IP", "ask: the IP code"),
            ("0 rpm", "refused: a rated speed"),
            ("IE2 IE3", "refused: 'IE2 IE3' does not read: it gives two efficiency classes"),
            ("Y Δ", "refused: 'Y Δ' does not read: it gives two winding connections"),
        ] {
            let got = shown(text);
            assert!(got.starts_with(start), "{text}: {got}");
        }
        for text in ["see datasheet", "IE6", "S11", "IP75", "class Z", "7.5 hp", "", "NaN kW", &"1".repeat(170)] {
            assert_eq!(shown(text), "note", "{text}");
        }
    }
}
