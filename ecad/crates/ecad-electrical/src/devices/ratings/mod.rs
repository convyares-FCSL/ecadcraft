//! Reading a typed rating into a group's typed fields:
//! one reader per group (protection here, switching in [`read_switching`], sensors in
//! [`read_sensor`], motors in [`read_motor`], loads in [`read_load`], indicators in
//! [`read_indicator`], power supplies and transformers in [`read_power_supply`], a strip's fuse positions in [`with_position_rating`]), used by `setprop` (the
//! UI, the CLI and MCP alike) and by a symbol's default rating.
//!
//! # Semantics
//! - Text that reads as the group's rating fills its fields: `B16` on an MCB is curve B, 16 A.
//!   A protective device also reads its rated voltage with AC or DC (`230 V AC`), a fuse its size
//!   (`10x38`, `NH00`), an RCD or RCBO its residual type after `type` (`type A`) and an RCD its
//!   conditional short-circuit current after `Inc` (`Inc 10 kA`).
//! - Ambiguous text is asked about, never guessed ([`Read::Ask`]): a bare `16` on an MCB needs its
//!   curve; a bare `30` on an RCD needs its unit.
//! - Values the group does not have are refused ([`Read::Refused`]): a curve on a fuse.
//! - Text with a word that is no rating at all ([`Read::NotARating`]) goes to the device's note
//!   with the typed fields left empty, so nothing typed is lost.
//! - What was read is shown back from the fields (the group's `Display`),
//!   and reading that text again gives the same fields.
//! - The rating line on the drawing ([`line`]) shows the fields the group's template names
//!   (`rating.template.<group>`, by default the plate and design fields), in its order; answers
//!   and the stored text keep every field.
//!
//! This module excludes the checks on a rating (the Checks rows read only the typed fields).
#![deny(clippy::wildcard_enum_match_arm)]

use ecad_model::Device;

use crate::error::CmdError;
use ecad_model::Word;
use ecad_model::geometry::units::{Amps, Current, KiloAmps, MilliAmps, RatedVoltage, Volts};
use ecad_model::objects::devices::protection::{MAX_AMPS, MAX_BREAKING_KA, MAX_RESIDUAL_MA, MAX_VOLTS};
use ecad_model::objects::devices::{Breaker, Fuse, FuseClass, FuseSize, Poles, Protection, ProtectionKind, Rcbo, Rcd, ResidualType, TripCurve};
use ecad_model::objects::devices::{DeviceType, FieldSource, Load, RatingNote};

pub mod dataset;
pub(crate) mod indicator;
pub(crate) mod load;
pub(crate) mod motor;
pub mod parse;
pub(crate) mod power_supply;
pub(crate) mod sensor;
pub(crate) mod switching;
pub use indicator::read_indicator;
pub use load::read_load;
pub use motor::read_motor;
pub use power_supply::read_power_supply;
pub use sensor::read_sensor;
pub use switching::read_switching;

/// Most characters of a typed rating (the note's bound).
pub(crate) const MAX_CHARS: usize = ecad_model::objects::devices::RatingNote::MAX;

/// What a rating text reads as.
#[derive(Debug, Clone, PartialEq)]
pub enum Read<T> {
    /// The group's typed rating.
    Rating(T),
    /// Ambiguous or incomplete: what is missing, as a question (`the curve (B, C, D, K or Z)?`).
    Ask(String),
    /// A value the group does not have, or a value out of range.
    Refused(String),
    /// Not a rating: the text is the device's note.
    NotARating,
}

/// One read piece of a rating text.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok {
    Amps(f64),
    MilliAmps(f64),
    KiloAmps(f64),
    /// A number with no unit.
    Bare(f64),
    Curve(TripCurve),
    Class(FuseClass),
    Poles(Poles),
    /// The word `curve` (before its letter).
    CurveWord,
    /// A voltage, with AC or DC when run on (`230vac`).
    Volts(f64, Option<Current>),
    /// `AC` or `DC` (after a voltage).
    Current(Current),
    Size(FuseSize),
    /// The word `type` (before a residual type).
    TypeWord,
    Residual(ResidualType),
    /// `Inc`: the next current in kA is the conditional short-circuit current.
    IncWord,
}

/// The pieces of `text`, or `None` when a piece is no rating word.
fn tokens(text: &str) -> Option<Vec<Tok>> {
    let mut out: Vec<Tok> = Vec::new();
    for lower in parse::pieces(text) {
        // A unit after a bare number: `16 A`, `30 mA`, `6 kA`, `230 V`.
        if let Some(Tok::Bare(x)) = out.last().copied() {
            let unit = match lower.as_str() {
                "a" => Some(Tok::Amps(x)),
                "ma" => Some(Tok::MilliAmps(x)),
                "ka" => Some(Tok::KiloAmps(x)),
                "v" => Some(Tok::Volts(x, None)),
                "vac" => Some(Tok::Volts(x, Some(Current::Ac))),
                "vdc" => Some(Tok::Volts(x, Some(Current::Dc))),
                _ => None,
            };
            if let Some(t) = unit {
                out.pop();
                out.push(t);
                continue;
            }
        }
        // A residual type after `type`: `type A`, `type AC`.
        if out.last() == Some(&Tok::TypeWord)
            && let Some(r) = ResidualType::parse(&lower)
        {
            out.pop();
            out.push(Tok::Residual(r));
            continue;
        }
        out.extend(piece_tokens(&lower)?);
    }
    Some(out)
}

/// One piece's tokens: a word, a number with its unit, or a curve letter with its current (`C16`).
fn piece_tokens(lower: &str) -> Option<Vec<Tok>> {
    let word = match lower {
        "curve" => Some(Tok::CurveWord),
        "type" => Some(Tok::TypeWord),
        "inc" => Some(Tok::IncWord),
        "ac" => Some(Tok::Current(Current::Ac)),
        "dc" => Some(Tok::Current(Current::Dc)),
        _ => None,
    };
    if let Some(w) = word {
        return Some(vec![w]);
    }
    if let Some(s) = FuseSize::read(lower) {
        return Some(vec![Tok::Size(s)]);
    }
    if let Some(p) = Poles::parse(lower) {
        return Some(vec![Tok::Poles(p)]);
    }
    if let Some(c) = TripCurve::parse(lower) {
        return Some(vec![Tok::Curve(c)]);
    }
    if let Some(c) = FuseClass::parse(lower) {
        return Some(vec![Tok::Class(c)]);
    }
    // A curve letter with its current: `C16`, `B6A`.
    if let Some(rest) = lower.get(1..)
        && rest.starts_with(|d: char| d.is_ascii_digit())
        && let Some(c) = lower.get(..1).and_then(TripCurve::parse)
    {
        return match number_token(rest)? {
            Tok::Amps(x) | Tok::Bare(x) => Some(vec![Tok::Curve(c), Tok::Amps(x)]),
            Tok::MilliAmps(_)
            | Tok::KiloAmps(_)
            | Tok::Curve(_)
            | Tok::Class(_)
            | Tok::Poles(_)
            | Tok::CurveWord
            | Tok::Volts(..)
            | Tok::Current(_)
            | Tok::Size(_)
            | Tok::TypeWord
            | Tok::Residual(_)
            | Tok::IncWord => None,
        };
    }
    number_token(lower).map(|t| vec![t])
}

/// A lower-case number with an optional unit: `16`, `16a`, `30ma`, `6ka`, `230v`, `24vdc`.
fn number_token(lower: &str) -> Option<Tok> {
    let (x, unit) = parse::number_unit(lower)?;
    match unit {
        "" => Some(Tok::Bare(x)),
        "a" => Some(Tok::Amps(x)),
        "ma" => Some(Tok::MilliAmps(x)),
        "ka" => Some(Tok::KiloAmps(x)),
        "v" => Some(Tok::Volts(x, None)),
        "vac" => Some(Tok::Volts(x, Some(Current::Ac))),
        "vdc" => Some(Tok::Volts(x, Some(Current::Dc))),
        _ => None,
    }
}

/// What one text gives, sorted by field.
#[derive(Default)]
struct Found {
    amps: Option<f64>,
    bare: Option<f64>,
    milli: Option<f64>,
    kilo: Option<f64>,
    curve: Option<TripCurve>,
    class: Option<FuseClass>,
    poles: Option<Poles>,
    volts: Option<(f64, Option<Current>)>,
    size: Option<FuseSize>,
    residual_type: Option<ResidualType>,
    /// After `Inc`.
    inc: Option<f64>,
}

fn once<T>(slot: &mut Option<T>, v: T, what: &str) -> Result<(), Problem> {
    if slot.is_some() {
        return Err(Problem::Refused(String::from("it gives two ") + what));
    }
    *slot = Some(v);
    Ok(())
}

fn found(toks: &[Tok]) -> Result<Found, Problem> {
    let mut f = Found::default();
    let mut i = 0;
    while let Some(t) = toks.get(i) {
        let prev = i.checked_sub(1).and_then(|j| toks.get(j)).copied();
        i += 1;
        match *t {
            Tok::Amps(x) => once(&mut f.amps, x, "rated currents")?,
            Tok::Bare(x) => once(&mut f.bare, x, "numbers with no unit")?,
            Tok::MilliAmps(x) => once(&mut f.milli, x, "residual currents")?,
            Tok::KiloAmps(x) => once(&mut f.kilo, x, "breaking capacities")?,
            Tok::Curve(c) => once(&mut f.curve, c, "curves")?,
            Tok::Class(c) => once(&mut f.class, c, "fuse classes")?,
            Tok::Poles(p) => once(&mut f.poles, p, "pole counts")?,
            Tok::CurveWord => {}
            Tok::Volts(x, c) => once(&mut f.volts, (x, c), "rated voltages")?,
            Tok::Current(c) => match (prev, f.volts.as_mut()) {
                (Some(Tok::Volts(_, None)), Some(v)) => v.1 = Some(c),
                _ => return Err(Problem::Refused("AC or DC is given apart from its voltage; write it after it, for example 230 V AC".into())),
            },
            Tok::Size(s) => once(&mut f.size, s, "fuse sizes")?,
            Tok::Residual(r) => once(&mut f.residual_type, r, "residual types")?,
            Tok::TypeWord => return Err(Problem::Ask("the residual type (AC, A, F or B), for example type A".into())),
            Tok::IncWord => match toks.get(i) {
                Some(Tok::KiloAmps(x)) => {
                    i += 1;
                    once(&mut f.inc, *x, "conditional short-circuit currents")?;
                }
                _ => return Err(Problem::Ask("the conditional short-circuit current in kA, for example Inc 10 kA".into())),
            },
        }
    }
    Ok(f)
}

fn amps(x: f64) -> Result<Amps, Problem> {
    (x.is_finite() && x > 0.0 && x <= MAX_AMPS)
        .then_some(Amps(x))
        .ok_or_else(|| Problem::Refused(format!("a rated current is more than 0 and at most {MAX_AMPS} A")))
}

fn kilo(x: f64) -> Result<KiloAmps, Problem> {
    (x.is_finite() && x > 0.0 && x <= MAX_BREAKING_KA)
        .then_some(KiloAmps(x))
        .ok_or_else(|| Problem::Refused(format!("a breaking capacity is more than 0 and at most {MAX_BREAKING_KA} kA")))
}

fn inc(x: f64) -> Result<KiloAmps, Problem> {
    (x.is_finite() && x > 0.0 && x <= MAX_BREAKING_KA)
        .then_some(KiloAmps(x))
        .ok_or_else(|| Problem::Refused(format!("a conditional short-circuit current is more than 0 and at most {MAX_BREAKING_KA} kA")))
}

/// A rated voltage with AC or DC; one without either is asked about.
fn rated_voltage(v: Option<(f64, Option<Current>)>) -> Result<Option<RatedVoltage>, Problem> {
    match v {
        None => Ok(None),
        Some((x, None)) => Err(Problem::Ask(format!("whether {x} V is AC or DC, for example {x} V AC"))),
        Some((x, Some(current))) if x.is_finite() && x > 0.0 && x <= MAX_VOLTS => Ok(Some(RatedVoltage { volts: Volts(x), current })),
        Some((_, Some(_))) => Err(Problem::Refused(format!("a rated voltage is more than 0 and at most {MAX_VOLTS} V"))),
    }
}

fn milli(x: f64) -> Result<MilliAmps, Problem> {
    (x.is_finite() && x > 0.0 && x <= MAX_RESIDUAL_MA)
        .then_some(MilliAmps(x))
        .ok_or_else(|| Problem::Refused(format!("a residual current is more than 0 and at most {MAX_RESIDUAL_MA} mA")))
}

/// Read `text` as the rating of a protective device of `kind` (a fuse bank: one fuse's).
///
/// # Example
/// ```
/// use ecad_electrical::devices::ratings::{Read, read_protection};
/// use ecad_model::objects::devices::ProtectionKind;
/// let Read::Rating(p) = read_protection(ProtectionKind::Mcb, "B16") else { panic!() };
/// assert_eq!(p.to_string(), "curve B, 16 A");
/// assert!(matches!(read_protection(ProtectionKind::Mcb, "16"), Read::Ask(_)));
/// assert_eq!(read_protection(ProtectionKind::Mcb, "see datasheet"), Read::NotARating);
/// ```
pub fn read_protection(kind: ProtectionKind, text: &str) -> Read<Protection> {
    let t = text.trim();
    if t.is_empty() || t.chars().nth(MAX_CHARS).is_some() {
        return Read::NotARating;
    }
    let Some(toks) = tokens(t) else { return Read::NotARating };
    if toks.iter().all(|t| matches!(t, Tok::CurveWord)) {
        return Read::NotARating;
    }
    let f = match found(&toks) {
        Ok(f) => f,
        Err(Problem::Refused(e)) => return Read::Refused(format!("'{t}' does not read: {e}")),
        Err(Problem::Ask(q)) => return Read::Ask(q),
    };
    match build(kind, f) {
        Ok(p) => Read::Rating(p),
        Err(Problem::Ask(m)) => Read::Ask(m),
        Err(Problem::Refused(m)) => Read::Refused(m),
    }
}

enum Problem {
    Ask(String),
    Refused(String),
}

const CURVE_Q: &str = "the curve (B, C, D, K or Z), for example C16";
const AMPS_Q: &str = "the rated current, for example 16 A";

fn build(kind: ProtectionKind, mut f: Found) -> Result<Protection, Problem> {
    let who = kind.label();
    let no = |what: &str| Problem::Refused(format!("{what} is not a rating of a {who}"));
    // A bare number: the rated current, except on an RCD, where it could be the residual.
    if let Some(x) = f.bare.take() {
        match kind {
            ProtectionKind::Rcd => return Err(Problem::Ask(format!("{x}: A (rated current) or mA (residual current)?"))),
            ProtectionKind::Fuse | ProtectionKind::FuseTerminalBlock | ProtectionKind::FuseBank | ProtectionKind::Mcb | ProtectionKind::Rcbo => {
                if f.amps.is_some() {
                    return Err(Problem::Refused("it gives two rated currents".into()));
                }
                f.amps = Some(x);
            }
        }
    }
    let rated = f.amps.map(amps).transpose()?;
    let breaking = f.kilo.map(kilo).transpose()?;
    let residual = f.milli.map(milli).transpose()?;
    let rated_voltage = rated_voltage(f.volts)?;
    let conditional_short_circuit = f.inc.map(inc).transpose()?;
    if conditional_short_circuit.is_some() && kind != ProtectionKind::Rcd {
        return Err(no("conditional short-circuit current (Inc)"));
    }
    if f.size.is_some() && !matches!(kind, ProtectionKind::Fuse | ProtectionKind::FuseTerminalBlock | ProtectionKind::FuseBank) {
        return Err(no("fuse size"));
    }
    match kind {
        ProtectionKind::Fuse | ProtectionKind::FuseTerminalBlock | ProtectionKind::FuseBank => {
            if f.curve.is_some() {
                return Err(Problem::Refused("a fuse has no curve; its class is F, M, T, gG or aM".into()));
            }
            if residual.is_some() {
                return Err(no("residual current"));
            }
            if f.poles.is_some() {
                return Err(no("poles"));
            }
            if f.residual_type.is_some() {
                return Err(no("residual type"));
            }
            if rated.is_none() {
                return Err(Problem::Ask(AMPS_Q.into()));
            }
            let fuse = Fuse { rated_current: rated, class: f.class, size: f.size, breaking, rated_voltage };
            Ok(if kind == ProtectionKind::FuseTerminalBlock { Protection::FuseTerminalBlock(fuse) } else { Protection::Fuse(fuse) })
        }
        ProtectionKind::Mcb | ProtectionKind::Rcbo => {
            if f.class.is_some() {
                return Err(no("fuse class"));
            }
            if (residual.is_some() || f.residual_type.is_some()) && kind == ProtectionKind::Mcb {
                return Err(Problem::Refused("an MCB has no residual current; an RCBO has".into()));
            }
            match (rated, f.curve) {
                (_, None) => return Err(Problem::Ask(CURVE_Q.into())),
                (None, Some(_)) => return Err(Problem::Ask(AMPS_Q.into())),
                (Some(_), Some(_)) => {}
            }
            Ok(if kind == ProtectionKind::Mcb {
                Protection::Mcb(Breaker { rated_current: rated, curve: f.curve, breaking, poles: f.poles, rated_voltage, ..Breaker::default() })
            } else {
                Protection::Rcbo(Rcbo {
                    rated_current: rated,
                    curve: f.curve,
                    residual,
                    residual_type: f.residual_type,
                    breaking,
                    poles: f.poles,
                    rated_voltage,
                    ..Rcbo::default()
                })
            })
        }
        ProtectionKind::Rcd => {
            if f.curve.is_some() {
                return Err(Problem::Refused("an RCD has no curve; an RCBO has".into()));
            }
            if f.class.is_some() {
                return Err(no("fuse class"));
            }
            if breaking.is_some() {
                return Err(no("breaking capacity"));
            }
            if residual.is_none() {
                return Err(Problem::Ask("the residual current, for example 30 mA".into()));
            }
            Ok(Protection::Rcd(Rcd {
                rated_current: rated,
                residual,
                residual_type: f.residual_type,
                conditional_short_circuit,
                poles: f.poles,
                rated_voltage,
                ..Rcd::default()
            }))
        }
    }
}

/// Read `text` as one fuse of a bank.
pub(crate) fn read_fuse(text: &str) -> Read<Fuse> {
    match read_protection(ProtectionKind::Fuse, text) {
        Read::Rating(Protection::Fuse(f)) => Read::Rating(f),
        Read::Rating(_) => Read::NotARating,
        Read::Ask(m) => Read::Ask(m),
        Read::Refused(m) => Read::Refused(m),
        Read::NotARating => Read::NotARating,
    }
}

// ---------------------------------------------------------------- on a device

/// What `text` sets on a device of type `t`: its typed rating, or the note (`None` clears both).
/// The text gives the nameplate and design fields; the datasheet fields are set one at a time
/// (`rating.<word>`) and kept.
///
/// # Errors
/// Ambiguous or refused text, in words naming `who` and what is missing; a fuse bank's own rating
/// (its ratings are per fuse).
pub fn rated(t: &DeviceType, who: &str, text: Option<&str>) -> Result<(DeviceType, Option<RatingNote>), CmdError> {
    let (mut new, note) = read_rated(t, who, text)?;
    for f in t.group().rating_fields().iter().filter(|f| f.source == FieldSource::Datasheet) {
        // The same group's own field: it is either written or not on this kind.
        let _ = (f.set)(&mut new, (f.get)(t).as_deref());
    }
    Ok((new, note))
}

/// [`rated`] before the datasheet fields are carried over.
fn read_rated(t: &DeviceType, who: &str, text: Option<&str>) -> Result<(DeviceType, Option<RatingNote>), CmdError> {
    let text = text.map(str::trim).filter(|s| !s.is_empty());
    let note = |s: &str| RatingNote::new(s).map_err(|e| CmdError::refused(note_refusal(&e)));
    match t {
        DeviceType::Protection(p) => {
            let kind = p.kind();
            if kind == ProtectionKind::FuseBank {
                return match text {
                    None => Ok((t.clone(), None)),
                    Some(_) => Err(CmdError::refused(format!("{who} is a fuse bank: its ratings are per fuse, rating.<n> (rating.1 4 A)."))),
                };
            }
            let Some(text) = text else { return Ok((DeviceType::Protection(kind.empty()), None)) };
            match read_protection(kind, text) {
                Read::Rating(p) => Ok((DeviceType::Protection(p), None)),
                Read::Ask(q) => Err(CmdError::refused(format!("{who}'s rating '{text}' needs {q}"))),
                Read::Refused(m) => Err(CmdError::refused(format!("{who}'s rating '{text}': {m}."))),
                Read::NotARating => Ok((DeviceType::Protection(kind.empty()), Some(note(text)?))),
            }
        }
        DeviceType::Switching(s) => {
            let kind = s.operator_kind();
            let Some(text) = text else { return Ok((DeviceType::Switching(kind.empty()), None)) };
            match read_switching(kind, text) {
                Read::Rating(s) => Ok((DeviceType::Switching(s), None)),
                Read::Ask(q) => Err(CmdError::refused(format!("{who}'s rating '{text}' needs {q}"))),
                Read::Refused(m) => Err(CmdError::refused(format!("{who}'s rating '{text}': {m}."))),
                Read::NotARating => Ok((DeviceType::Switching(kind.empty()), Some(note(text)?))),
            }
        }
        DeviceType::Sensor(s) => {
            let Some(text) = text else { return Ok((DeviceType::Sensor(s.unrated()), None)) };
            match read_sensor(s, text) {
                Read::Rating(s) => Ok((DeviceType::Sensor(s), None)),
                Read::Ask(q) => Err(CmdError::refused(format!("{who}'s rating '{text}' needs {q}"))),
                Read::Refused(m) => Err(CmdError::refused(format!("{who}'s rating '{text}': {m}."))),
                Read::NotARating => Ok((DeviceType::Sensor(s.unrated()), Some(note(text)?))),
            }
        }
        DeviceType::Motor(m) => {
            let Some(text) = text else { return Ok((DeviceType::Motor(m.unrated()), None)) };
            match read_motor(m, text) {
                Read::Rating(m) => Ok((DeviceType::Motor(m), None)),
                Read::Ask(q) => Err(CmdError::refused(format!("{who}'s rating '{text}' needs {q}"))),
                Read::Refused(m) => Err(CmdError::refused(format!("{who}'s rating '{text}': {m}."))),
                Read::NotARating => Ok((DeviceType::Motor(m.unrated()), Some(note(text)?))),
            }
        }
        DeviceType::Load(_) => {
            let Some(text) = text else { return Ok((DeviceType::Load(Load::default()), None)) };
            match read_load(text) {
                Read::Rating(l) => Ok((DeviceType::Load(l), None)),
                Read::Ask(q) => Err(CmdError::refused(format!("{who}'s rating '{text}' needs {q}"))),
                Read::Refused(m) => Err(CmdError::refused(format!("{who}'s rating '{text}': {m}."))),
                Read::NotARating => Ok((DeviceType::Load(Load::default()), Some(note(text)?))),
            }
        }
        DeviceType::Indicator(i) => {
            let Some(text) = text else { return Ok((DeviceType::Indicator(i.unrated()), None)) };
            match read_indicator(i, text) {
                Read::Rating(i) => Ok((DeviceType::Indicator(i), None)),
                Read::Ask(q) => Err(CmdError::refused(format!("{who}'s rating '{text}' needs {q}"))),
                Read::Refused(m) => Err(CmdError::refused(format!("{who}'s rating '{text}': {m}."))),
                Read::NotARating => Ok((DeviceType::Indicator(i.unrated()), Some(note(text)?))),
            }
        }
        DeviceType::PowerSupply(p) => {
            let Some(text) = text else { return Ok((DeviceType::PowerSupply(p.unrated()), None)) };
            match read_power_supply(p, text) {
                Read::Rating(p) => Ok((DeviceType::PowerSupply(p), None)),
                Read::Ask(q) => Err(CmdError::refused(format!("{who}'s rating '{text}' needs {q}"))),
                Read::Refused(m) => Err(CmdError::refused(format!("{who}'s rating '{text}': {m}."))),
                Read::NotARating => Ok((DeviceType::PowerSupply(p.unrated()), Some(note(text)?))),
            }
        }
        // Groups with no rating of their own (a strip's are its fuse positions'): the text is the
        // note.
        DeviceType::TerminalStrip(_) | DeviceType::Connector(_) | DeviceType::PlcRack(_) | DeviceType::PlcModule(_) | DeviceType::General => {
            Ok((t.clone(), text.map(note).transpose()?))
        }
    }
}

fn note_refusal(e: &ecad_model::naming::text::TextError) -> String {
    match e {
        ecad_model::naming::text::TextError::TooLong { .. } => format!("A rating is at most {MAX_CHARS} characters."),
        ecad_model::naming::text::TextError::Hidden { .. } | ecad_model::naming::text::TextError::Lines => {
            "A rating is one line with no control or hidden characters.".into()
        }
        ecad_model::naming::text::TextError::Empty => "A rating is not empty.".into(),
    }
}

/// Device `d` with its rating set from `text` (`None` clears it). Its contacts and contact table
/// are not rating fields and are kept.
///
/// # Errors
/// [`rated`]'s.
pub(crate) fn with_rating(d: &Device, who: &str, text: Option<&str>) -> Result<Device, CmdError> {
    use crate::devices::traits::DeviceKind;
    let (mut device_type, rating_note) = rated(&d.device_type, who, text)?;
    if let (Some(old), Some(new)) = (d.device_type.as_contacts(), device_type.as_contacts_mut()) {
        *new.contacts_mut() = old.contacts().clone();
    }
    if let (DeviceType::Switching(old), DeviceType::Switching(new)) = (&d.device_type, &mut device_type) {
        new.contact_table = old.contact_table;
    }
    Ok(Device { device_type, rating_note, ..d.clone() })
}

/// Fuse bank `d` with output `n`'s rating set from `text` (`None` clears it); `outputs` are the
/// bank's output labels.
///
/// # Errors
/// Not a fuse bank, no such output, or text that does not read as a fuse's rating.
pub(crate) fn with_output_rating(
    d: &Device, who: &str, outputs: &std::collections::BTreeSet<String>, n: &str, text: Option<&str>,
) -> Result<Device, CmdError> {
    let shown: String = n.chars().take(12).collect();
    let DeviceType::Protection(Protection::FuseBank(bank)) = &d.device_type else {
        return Err(CmdError::refused(format!("rating.{shown} is for a fuse bank's fuses; {who} is not a fuse bank.")));
    };
    let mut bank = bank.clone();
    let text = text.map(str::trim).filter(|s| !s.is_empty());
    let out = n.parse::<u8>().ok().filter(|_| outputs.contains(n));
    match (out, text) {
        (Some(o), None) => {
            bank.outputs.remove(&o);
        }
        // Clearing an output the bank no longer has (an older symbol) is always allowed.
        (None, None) => {
            if let Ok(o) = n.parse::<u8>() {
                bank.outputs.remove(&o);
            }
        }
        (None, Some(_)) => return Err(CmdError::refused(format!("{who} has no fuse {shown}."))),
        (Some(o), Some(t)) => match read_fuse(t) {
            Read::Rating(f) => {
                bank.outputs.insert(o, f);
            }
            Read::Ask(q) => return Err(CmdError::refused(format!("{who}:{o}'s rating '{t}' needs {q}"))),
            Read::Refused(m) => return Err(CmdError::refused(format!("{who}:{o}'s rating '{t}': {m}."))),
            Read::NotARating => return Err(CmdError::refused(format!("{who}:{o}'s rating '{t}' is not a fuse rating; write it as 4 A or 6.3 A T."))),
        },
    }
    Ok(Device { device_type: DeviceType::Protection(Protection::FuseBank(bank)), ..d.clone() })
}

/// Terminal strip `d` with fuse position `p`'s rating set from `text` (`None` clears it);
/// `fuse_positions` are the strip's fuse position labels.
///
/// # Semantics
/// The text is read as one fuse ([`read_fuse`]); text that is no fuse rating is refused, never
/// kept as a note (a position has no note). Clearing always works, also on a position that is no
/// longer a fuse position.
///
/// # Errors
/// Not a terminal strip, not a fuse position, or text that does not read as a fuse's rating.
pub(crate) fn with_position_rating(
    d: &Device, who: &str, fuse_positions: &std::collections::BTreeSet<String>, p: &str, text: Option<&str>,
) -> Result<Device, CmdError> {
    // Capped before any lookup: only the first 12 characters are ever shown.
    let max = crate::symbols::rules::MAX_FUSE_LABEL;
    let shown: String = p.chars().take(max).collect();
    let more = if p.chars().nth(max).is_some() { "…" } else { "" };
    let DeviceType::TerminalStrip(strip) = &d.device_type else {
        return Err(CmdError::refused(format!("rating.{shown}{more} is for a strip's fuse positions; {who} is not a terminal strip.")));
    };
    let mut strip = strip.clone();
    match text.map(str::trim).filter(|s| !s.is_empty()) {
        None => strip.fuses.retain(|l, _| l.as_str() != p),
        Some(t) => {
            let not_fuse = || format!("{who}:{shown}{more} is not a fuse position; rating.<position> is the rating of a fuse position.");
            let label = ecad_model::objects::devices::PositionLabel::new(p).ok().filter(|_| fuse_positions.contains(p));
            let label = label.ok_or_else(|| CmdError::refused(not_fuse()))?;
            let unread = || CmdError::refused(format!("{who}:{label}'s rating '{t}' is not a fuse rating; write it as 4 A or 6.3 A T."));
            match read_fuse(t) {
                Read::Rating(f) if f == Fuse::default() => return Err(unread()),
                Read::Rating(f) => {
                    strip.fuses.insert(label, f);
                }
                Read::Ask(q) => return Err(CmdError::refused(format!("{who}:{label}'s rating '{t}' needs {q}"))),
                Read::Refused(m) => return Err(CmdError::refused(format!("{who}:{label}'s rating '{t}': {m}."))),
                Read::NotARating => return Err(unread()),
            }
        }
    }
    Ok(Device { device_type: DeviceType::TerminalStrip(strip), ..d.clone() })
}

/// Device `d`'s rating line on the drawing: its set fields that the project's template for its group
/// names (`rating.template.<group>`), in that order; its note when no rating field is set.
pub fn line(doc: &ecad_model::Document, d: &Device) -> Option<String> {
    let Some(entry) = crate::defaults::rating::template_entry(d.device_type.group()) else { return d.rating_shown() };
    if d.device_type.rating().is_none() {
        return d.rating_note.as_ref().map(ToString::to_string);
    }
    let text = ecad_model::registry::resolve(doc, entry, ecad_model::registry::Scope::project()).text();
    let words = crate::defaults::rating::template_words(d.device_type.group(), &text).unwrap_or_else(|| d.device_type.group().default_template());
    d.device_type.rating_line(&words)
}

/// What a command that set `key` on `d` says it read: `Read: curve B, 16 A.`, or that the text
/// was kept as a note; `None` for any other key, or nothing set.
pub fn echo(d: &Device, key: &str) -> Option<String> {
    if key == ecad_model::objects::devices::RATING_KEY {
        if let Some(r) = d.device_type.rating() {
            return Some(format!("Read: {r}."));
        }
        return d.rating_note.as_ref().map(|n| {
            if d.device_type.has_rating_fields() {
                format!("'{n}' is not a rating ECAD reads; kept as a note (the device has no rating).")
            } else {
                format!("Kept as a note: {n}.")
            }
        });
    }
    let rest = key.strip_prefix(ecad_model::objects::devices::RATING_OF_PREFIX)?;
    if let Some(f) = d.device_type.group().rating_field(rest) {
        return Some(match (f.get)(&d.device_type) {
            Some(v) => format!("{}: {v}.", f.label),
            None => format!("{}: not set.", f.label),
        });
    }
    d.value(key).map(|r| format!("Read: {r}."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ProtectionKind as K;

    fn shown(kind: K, text: &str) -> String {
        match read_protection(kind, text) {
            Read::Rating(p) => p.to_string(),
            Read::Ask(m) => format!("ask: {m}"),
            Read::Refused(m) => format!("refused: {m}"),
            Read::NotARating => "note".into(),
        }
    }

    #[test]
    fn ratings_read_into_fields_and_back() {
        for (kind, text, want) in [
            (K::Mcb, "B16", "curve B, 16 A"),
            (K::Mcb, "C 16 A", "curve C, 16 A"),
            (K::Mcb, "16A C", "curve C, 16 A"),
            (K::Mcb, "C16 6kA 2P", "curve C, 16 A, 6 kA, 2P"),
            (K::Mcb, "curve B, 16 A", "curve B, 16 A"),
            (K::Rcbo, "13A C 30mA", "curve C, 13 A, 30 mA"),
            (K::Rcd, "40A 30mA 4P", "40 A, 30 mA, 4P"),
            (K::Rcd, "30 mA", "30 mA"),
            (K::Fuse, "6.3 A T", "6.3 A, T"),
            (K::Fuse, "4A", "4 A"),
            (K::Fuse, "4", "4 A"),
            (K::Fuse, "16 A gG 50 kA", "16 A, gG, 50 kA"),
            (K::Fuse, "6,3A", "6.3 A"),
            (K::Fuse, "16 A gG 10x38 400 V AC", "16 A, gG, 10x38, 400 V AC"),
            (K::Fuse, "NH00 63A gG 120kA 500VAC", "63 A, gG, NH00, 120 kA, 500 V AC"),
            (K::Fuse, "2 A F 6.3x32", "2 A, F, 6.3x32"),
            (K::FuseTerminalBlock, "6,3 A T 5x20 250 V AC", "6.3 A, T, 5x20, 250 V AC"),
            (K::Mcb, "C16 6kA 1P+N 230V AC", "curve C, 16 A, 6 kA, 1P+N, 230 V AC"),
            (K::Mcb, "B10 24 V DC", "curve B, 10 A, 24 V DC"),
            (K::Rcd, "40A 30mA type A Inc 10 kA 4P 400VAC", "40 A, 30 mA, type A, Inc 10 kA, 4P, 400 V AC"),
            (K::Rcd, "30 mA TYPE b", "30 mA, type B"),
            (K::Rcbo, "C13 30mA type AC 6kA", "curve C, 13 A, 30 mA, type AC, 6 kA"),
            (K::Rcbo, "B16 30 mA type F", "curve B, 16 A, 30 mA, type F"),
        ] {
            let got = shown(kind, text);
            assert_eq!(got, want, "{text}");
            // What is shown reads back to the same fields.
            assert_eq!(shown(kind, &got), want, "{got}");
        }
    }

    #[test]
    fn ambiguous_text_is_asked_and_wrong_values_refused() {
        for (kind, text, start) in [
            (K::Mcb, "16", "ask: the curve"),
            (K::Mcb, "16 A", "ask: the curve"),
            (K::Mcb, "C", "ask: the rated current"),
            (K::Rcd, "30", "ask: 30: A"),
            (K::Rcd, "40 A", "ask: the residual"),
            (K::Fuse, "T", "ask: the rated current"),
            (K::Fuse, "C16", "refused: a fuse has no curve"),
            (K::Mcb, "C16 30mA", "refused: an MCB has no residual"),
            (K::Mcb, "C16 B", "refused: 'C16 B' does not read: it gives two curves"),
            (K::Mcb, "C0", "refused: a rated current"),
            (K::Fuse, "7000 A", "refused: a rated current"),
            (K::Mcb, "C16 230 V", "ask: whether 230 V is AC or DC"),
            (K::Fuse, "4 A 2000 V AC", "refused: a rated voltage"),
            (K::Mcb, "C16 AC 230 V", "refused: 'C16 AC 230 V' does not read: AC or DC is given apart"),
            (K::Mcb, "C16 10x38", "refused: fuse size is not a rating of a MCB"),
            (K::Mcb, "C16 type A", "refused: an MCB has no residual"),
            (K::Fuse, "4 A type A", "refused: residual type is not a rating of a fuse"),
            (K::Rcd, "30 mA type", "ask: the residual type"),
            (K::Rcd, "30 mA type Z", "ask: the residual type"),
            (K::Rcd, "30 mA Inc", "ask: the conditional short-circuit current"),
            (K::Rcd, "30 mA Inc 0 kA", "refused: a conditional short-circuit current"),
            (K::Rcbo, "C16 30 mA Inc 6 kA", "refused: conditional short-circuit current (Inc) is not a rating of a RCBO"),
            (K::Rcd, "30 mA type A type B", "refused: '30 mA type A type B' does not read: it gives two residual types"),
        ] {
            let got = shown(kind, text);
            assert!(got.starts_with(start), "{text}: {got}");
        }
        for text in ["see datasheet", "C16 Siemens", "", "curve", "1e9 A", "NaN", &"1".repeat(70)] {
            assert_eq!(shown(K::Mcb, text), "note", "{text}");
        }
    }
}
