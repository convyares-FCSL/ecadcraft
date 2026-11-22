//! Reading a typed rating into a group's typed fields (design 072 §2.2, "Ratings", owner decisions
//! 2026-11-18): one reader per group (protection here, switching in [`read_switching`], sensors in
//! [`read_sensor`], motors in [`read_motor`], loads in [`read_load`], indicators in
//! [`read_indicator`], power supplies and transformers in [`read_power_supply`], a strip's fuse positions in [`with_position_rating`]), used by `setprop` (the
//! UI, the CLI and MCP alike) and by a symbol's default rating.
//!
//! # Semantics
//! - Text that reads as the group's rating fills its fields: `B16` on an MCB is curve B, 16 A.
//! - Ambiguous text is asked about, never guessed ([`Read::Ask`]): a bare `16` on an MCB needs its
//!   curve; a bare `30` on an RCD needs its unit.
//! - Values the group does not have are refused ([`Read::Refused`]): a curve on a fuse.
//! - Text with a word that is no rating at all ([`Read::NotARating`]) goes to the device's note
//!   with the typed fields left empty, so nothing typed is lost.
//! - What was read is shown back from the fields (the group's `Display`),
//!   and reading that text again gives the same fields.
//!
//! This module excludes the checks on a rating (the Checks rows read only the typed fields).
#![deny(clippy::wildcard_enum_match_arm)]

use ecad_model::Device;

use crate::error::CmdError;
use ecad_model::Word;
use ecad_model::geometry::units::{Amps, KiloAmps, MilliAmps};
use ecad_model::objects::devices::protection::{MAX_AMPS, MAX_BREAKING_KA, MAX_RESIDUAL_MA};
use ecad_model::objects::devices::{Breaker, Fuse, FuseClass, Poles, Protection, ProtectionKind, Rcbo, Rcd, TripCurve};
use ecad_model::objects::devices::{DeviceType, Load, RatingNote};

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
}

/// The pieces of `text`, or `None` when a piece is no rating word.
fn tokens(text: &str) -> Option<Vec<Tok>> {
    // A comma between digits is a decimal comma (`6,3 A`); any other separates pieces.
    let chars: Vec<char> = text.chars().collect();
    let text: String = chars
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let digit = |j: Option<usize>| j.and_then(|j| chars.get(j)).is_some_and(char::is_ascii_digit);
            if *c == ',' && digit(i.checked_sub(1)) && digit(Some(i + 1)) { '.' } else { *c }
        })
        .collect();
    let mut out: Vec<Tok> = Vec::new();
    for piece in text.split(|c: char| c.is_whitespace() || c == ',' || c == ';').filter(|p| !p.is_empty()) {
        let lower = piece.to_ascii_lowercase();
        // A unit after a bare number: `16 A`, `30 mA`, `6 kA`.
        if let Some(Tok::Bare(x)) = out.last().copied() {
            let unit = match lower.as_str() {
                "a" => Some(Tok::Amps(x)),
                "ma" => Some(Tok::MilliAmps(x)),
                "ka" => Some(Tok::KiloAmps(x)),
                _ => None,
            };
            if let Some(t) = unit {
                out.pop();
                out.push(t);
                continue;
            }
        }
        out.extend(piece_tokens(piece, &lower)?);
    }
    Some(out)
}

/// One piece's tokens: a word, a number with its unit, or a curve letter with its current (`C16`).
fn piece_tokens(piece: &str, lower: &str) -> Option<Vec<Tok>> {
    if lower == "curve" {
        return Some(vec![Tok::CurveWord]);
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
    if let Some(rest) = piece.get(1..)
        && rest.starts_with(|d: char| d.is_ascii_digit())
        && let Some(c) = piece.get(..1).and_then(TripCurve::parse)
    {
        return match number_token(rest)? {
            Tok::Amps(x) | Tok::Bare(x) => Some(vec![Tok::Curve(c), Tok::Amps(x)]),
            Tok::MilliAmps(_) | Tok::KiloAmps(_) | Tok::Curve(_) | Tok::Class(_) | Tok::Poles(_) | Tok::CurveWord => None,
        };
    }
    number_token(piece).map(|t| vec![t])
}

/// A number with an optional unit: `16`, `16A`, `30mA`, `6kA`, `6,3A`.
fn number_token(piece: &str) -> Option<Tok> {
    let split = piece.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ',')).unwrap_or(piece.len());
    let (num, unit) = piece.split_at(split);
    let x = crate::devices::ratings::parse::decimal(num)?;
    match unit.to_ascii_lowercase().as_str() {
        "" => Some(Tok::Bare(x)),
        "a" => Some(Tok::Amps(x)),
        "ma" => Some(Tok::MilliAmps(x)),
        "ka" => Some(Tok::KiloAmps(x)),
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
    for t in toks {
        match *t {
            Tok::Amps(x) => once(&mut f.amps, x, "rated currents")?,
            Tok::Bare(x) => once(&mut f.bare, x, "numbers with no unit")?,
            Tok::MilliAmps(x) => once(&mut f.milli, x, "residual currents")?,
            Tok::KiloAmps(x) => once(&mut f.kilo, x, "breaking capacities")?,
            Tok::Curve(c) => once(&mut f.curve, c, "curves")?,
            Tok::Class(c) => once(&mut f.class, c, "fuse classes")?,
            Tok::Poles(p) => once(&mut f.poles, p, "pole counts")?,
            Tok::CurveWord => {}
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
            if rated.is_none() {
                return Err(Problem::Ask(AMPS_Q.into()));
            }
            let fuse = Fuse { rated_current: rated, class: f.class, breaking };
            Ok(if kind == ProtectionKind::FuseTerminalBlock { Protection::FuseTerminalBlock(fuse) } else { Protection::Fuse(fuse) })
        }
        ProtectionKind::Mcb | ProtectionKind::Rcbo => {
            if f.class.is_some() {
                return Err(no("fuse class"));
            }
            if residual.is_some() && kind == ProtectionKind::Mcb {
                return Err(Problem::Refused("an MCB has no residual current; an RCBO has".into()));
            }
            match (rated, f.curve) {
                (_, None) => return Err(Problem::Ask(CURVE_Q.into())),
                (None, Some(_)) => return Err(Problem::Ask(AMPS_Q.into())),
                (Some(_), Some(_)) => {}
            }
            Ok(if kind == ProtectionKind::Mcb {
                Protection::Mcb(Breaker { rated_current: rated, curve: f.curve, breaking, poles: f.poles, ..Breaker::default() })
            } else {
                Protection::Rcbo(Rcbo { rated_current: rated, curve: f.curve, residual, breaking, poles: f.poles, ..Rcbo::default() })
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
            Ok(Protection::Rcd(Rcd { rated_current: rated, residual, poles: f.poles, ..Rcd::default() }))
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
///
/// # Errors
/// Ambiguous or refused text, in words naming `who` and what is missing; a fuse bank's own rating
/// (its ratings are per fuse).
pub fn rated(t: &DeviceType, who: &str, text: Option<&str>) -> Result<(DeviceType, Option<RatingNote>), CmdError> {
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
/// `fuse_positions` are the strip's fuse position labels (design 072 §2.5).
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
    key.strip_prefix(ecad_model::objects::devices::RATING_OF_PREFIX).and(d.value(key)).map(|r| format!("Read: {r}."))
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
        ] {
            let got = shown(kind, text);
            assert!(got.starts_with(start), "{text}: {got}");
        }
        for text in ["see datasheet", "C16 Siemens", "", "curve", "1e9 A", "NaN", &"1".repeat(70)] {
            assert_eq!(shown(K::Mcb, text), "note", "{text}");
        }
    }
}
