//! The value types of rating fields: each reads the text a person types and writes the value as a
//! drawing shows it.
//!
//! # Semantics
//! - [`FieldValue`] is one value type with its own parser: a quantity takes a number with its unit
//!   or a bare number in its base unit (`7.5 kW`, `7500`), a voltage with its kind needs AC or DC
//!   (`24 V DC`), an IP code its two numerals (`IP65`, `IPX4`), a choice takes its word or its label
//!   (`star_delta`, `star-delta`). A value is a
//!   finite number more than zero; the upper limits are each group's ([`crate::DeviceType::problem`]).
//! - [`FieldType`] names a value type for whoever describes a field (the agent's view, a column
//!   header): its word, its unit and an example it reads.
//! - [`put`] and [`put_after`] write a field from text (empty clears it); a refusal is the type's
//!   reason, which the caller prefixes with the field's label.
//!
//! This module excludes reading a whole rating text (`ecad-electrical`'s rating readers) and the
//! fields themselves, which each group declares beside its data.
#![deny(clippy::wildcard_enum_match_arm)]

use crate::geometry::units::{Amps, Current, Hertz, KiloAmps, MilliAmps, Percent, RatedVoltage, VoltAmps, Volts, Watts};
use crate::naming::words::Word;
use crate::objects::devices::{FuseClass, FuseSize, IndicatorColour, IpCode, Poles, ResidualType, SignalType, Starting, TripCurve};

/// What a rating field's value is, for whoever describes the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    Current,
    SmallCurrent,
    BreakingCurrent,
    Voltage,
    /// A voltage with AC or DC.
    RatedVoltage,
    Power,
    ApparentPower,
    Frequency,
    Percent,
    /// A number from 0 to 1 (a power factor).
    Ratio,
    /// A degree of protection (IEC 60529).
    IpCode,
    /// One of these words.
    Choice(&'static [&'static str]),
    /// Several values in one text, written only through the whole rating (`rating`).
    Combined,
}

impl FieldType {
    /// Its word in the agent's view (`current`, `choice`).
    pub fn word(self) -> &'static str {
        match self {
            FieldType::Current | FieldType::SmallCurrent | FieldType::BreakingCurrent => "current",
            FieldType::Voltage => "voltage",
            FieldType::RatedVoltage => "voltage_ac_dc",
            FieldType::Power => "power",
            FieldType::ApparentPower => "apparent_power",
            FieldType::Frequency => "frequency",
            FieldType::Percent => "percent",
            FieldType::Ratio => "ratio",
            FieldType::IpCode => "ip_code",
            FieldType::Choice(_) => "choice",
            FieldType::Combined => "combined",
        }
    }

    /// The unit a bare number is in (empty for a ratio, an IP code, a choice or a combined value).
    pub fn unit(self) -> &'static str {
        match self {
            FieldType::Current => Amps::SYMBOL,
            FieldType::SmallCurrent => MilliAmps::SYMBOL,
            FieldType::BreakingCurrent => KiloAmps::SYMBOL,
            FieldType::Voltage | FieldType::RatedVoltage => Volts::SYMBOL,
            FieldType::Power => Watts::SYMBOL,
            FieldType::ApparentPower => VoltAmps::SYMBOL,
            FieldType::Frequency => Hertz::SYMBOL,
            FieldType::Percent => Percent::SYMBOL,
            FieldType::Ratio | FieldType::IpCode | FieldType::Choice(_) | FieldType::Combined => "",
        }
    }

    /// A text its parser reads (`16 A`, a choice's first word); empty for a combined value.
    pub fn example(self) -> &'static str {
        match self {
            FieldType::Current => "16 A",
            FieldType::SmallCurrent => "30 mA",
            FieldType::BreakingCurrent => "10 kA",
            FieldType::Voltage => "400 V",
            FieldType::RatedVoltage => "24 V DC",
            FieldType::Power => "7.5 kW",
            FieldType::ApparentPower => "250 VA",
            FieldType::Frequency => "50 Hz",
            FieldType::Percent => "91 %",
            FieldType::Ratio => "0.85",
            FieldType::IpCode => "IP65",
            FieldType::Choice(w) => w.first().copied().unwrap_or_default(),
            FieldType::Combined => "",
        }
    }

    /// The words of a choice; empty otherwise.
    pub fn choices(self) -> &'static [&'static str] {
        match self {
            FieldType::Choice(w) => w,
            FieldType::Current
            | FieldType::SmallCurrent
            | FieldType::BreakingCurrent
            | FieldType::Voltage
            | FieldType::RatedVoltage
            | FieldType::Power
            | FieldType::ApparentPower
            | FieldType::Frequency
            | FieldType::Percent
            | FieldType::Ratio
            | FieldType::IpCode
            | FieldType::Combined => &[],
        }
    }
}

/// Why a rating field was not written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldError {
    /// This kind of device in the group has no such field (a transformer's output current).
    NotHere,
    /// A combined field: written through the whole rating.
    Combined,
    /// The text is not a value of the field's type: the type's reason, as the end of a sentence
    /// (`a current, such as 16 A`).
    Bad(String),
}

/// A rating field's value type: its own parser and how a drawing shows it.
pub trait FieldValue: Sized + Copy {
    const TYPE: FieldType;

    /// The value `text` (trimmed, not empty) gives.
    ///
    /// # Errors
    /// [`FieldError::Bad`] with the type's reason.
    fn parse(text: &str) -> Result<Self, FieldError>;

    /// The value as a drawing shows it (`16 A`).
    fn show(self) -> String;
}

/// `field` from `text`: empty clears it.
///
/// # Errors
/// [`FieldError::Bad`] with the type's reason.
pub fn put<T: FieldValue>(field: &mut Option<T>, text: Option<&str>) -> Result<(), FieldError> {
    put_after(&[], field, text)
}

/// `field` from `text`, which may start with one of `prefixes` as the drawing shows it
/// (`cos φ 0.85`, `AC-3 16 A`): empty clears it.
///
/// # Errors
/// [`FieldError::Bad`] with the type's reason.
pub fn put_after<T: FieldValue>(prefixes: &[&str], field: &mut Option<T>, text: Option<&str>) -> Result<(), FieldError> {
    let Some(t) = text.map(str::trim).filter(|t| !t.is_empty()) else {
        *field = None;
        return Ok(());
    };
    let lower = t.to_lowercase();
    let rest = prefixes.iter().find_map(|p| lower.strip_prefix(&p.to_lowercase())).unwrap_or(&lower);
    *field = Some(T::parse(rest.trim())?);
    Ok(())
}

/// `field` shown, when set.
pub fn shown<T: FieldValue>(field: Option<T>) -> Option<String> {
    field.map(T::show)
}

/// The number at the start of `text` and the rest, lower case and trimmed (`7.5 kW` → `7.5`, `kw`).
fn number_unit(text: &str) -> Option<(f64, String)> {
    let t = text.trim();
    let end = t.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(t.len());
    let x: f64 = t.get(..end)?.parse().ok()?;
    Some((x, t.get(end..)?.trim().to_lowercase()))
}

/// A number with one of `units` (lower case, with its factor to the base unit; `""` for a bare
/// number), in the base unit.
fn quantity(text: &str, units: &[(&str, f64)], reason: &str) -> Result<f64, FieldError> {
    let bad = || FieldError::Bad(reason.to_string());
    let (x, unit) = number_unit(text).ok_or_else(bad)?;
    let factor = units.iter().find(|(u, _)| *u == unit).map(|(_, k)| *k).ok_or_else(bad)?;
    let v = x * factor;
    if v.is_finite() && v > 0.0 { Ok(v) } else { Err(bad()) }
}

/// Declares a quantity's [`FieldValue`]: its type, its units with their factors, its reason and how
/// it shows.
macro_rules! quantity_value {
    ($name:ident, $ty:expr, [$($unit:literal => $k:expr),*], $reason:literal, $show:expr) => {
        impl FieldValue for $name {
            const TYPE: FieldType = $ty;

            fn parse(text: &str) -> Result<Self, FieldError> {
                quantity(text, &[("", 1.0) $(, ($unit, $k))*], $reason).map($name)
            }

            fn show(self) -> String {
                ($show)(self)
            }
        }
    };
}

quantity_value!(Amps, FieldType::Current, ["a" => 1.0, "ma" => 0.001, "ka" => 1000.0], "a current, such as 16 A", |v: Amps| v.to_string());
quantity_value!(MilliAmps, FieldType::SmallCurrent, ["ma" => 1.0, "a" => 1000.0], "a current, such as 30 mA", |v: MilliAmps| v.to_string());
quantity_value!(KiloAmps, FieldType::BreakingCurrent, ["ka" => 1.0, "a" => 0.001], "a current, such as 10 kA", |v: KiloAmps| v.to_string());
quantity_value!(Volts, FieldType::Voltage, ["v" => 1.0, "vac" => 1.0, "kv" => 1000.0], "a voltage, such as 400 V", |v: Volts| v.to_string());
quantity_value!(Watts, FieldType::Power, ["w" => 1.0, "kw" => 1000.0], "a power, such as 7.5 kW", crate::objects::devices::load::load_power);
quantity_value!(
    VoltAmps,
    FieldType::ApparentPower,
    ["va" => 1.0, "kva" => 1000.0],
    "an apparent power, such as 250 VA",
    crate::objects::devices::power_supply::volt_amps
);
quantity_value!(Hertz, FieldType::Frequency, ["hz" => 1.0], "a frequency, such as 50 Hz", |v: Hertz| v.to_string());
quantity_value!(Percent, FieldType::Percent, ["%" => 1.0], "a percentage, such as 91 %", |v: Percent| v.to_string());

/// A power factor: a number more than 0 and at most 1.
impl FieldValue for f64 {
    const TYPE: FieldType = FieldType::Ratio;

    fn parse(text: &str) -> Result<Self, FieldError> {
        match text.trim().parse::<f64>() {
            Ok(x) if x.is_finite() && x > 0.0 && x <= 1.0 => Ok(x),
            _ => Err(FieldError::Bad("a number more than 0 and at most 1, such as 0.85".into())),
        }
    }

    fn show(self) -> String {
        self.to_string()
    }
}

impl FieldValue for RatedVoltage {
    const TYPE: FieldType = FieldType::RatedVoltage;

    fn parse(text: &str) -> Result<Self, FieldError> {
        let reason = "a voltage with AC or DC, such as 24 V DC";
        let lower = text.trim().to_lowercase();
        let (rest, current) = if let Some(r) = lower.strip_suffix("ac") {
            (r, Current::Ac)
        } else if let Some(r) = lower.strip_suffix("dc") {
            (r, Current::Dc)
        } else {
            return Err(FieldError::Bad(reason.into()));
        };
        let volts = quantity(rest, &[("", 1.0), ("v", 1.0), ("kv", 1000.0)], reason)?;
        Ok(RatedVoltage { volts: Volts(volts), current })
    }

    fn show(self) -> String {
        self.to_string()
    }
}

impl FieldValue for IpCode {
    const TYPE: FieldType = FieldType::IpCode;

    fn parse(text: &str) -> Result<Self, FieldError> {
        IpCode::parse(text).ok_or_else(|| FieldError::Bad("an IP code, such as IP65 or IPX4".into()))
    }

    fn show(self) -> String {
        self.to_string()
    }
}

/// Declares a choice's [`FieldValue`]: its word or its label, shown by its label.
macro_rules! choice_value {
    ($name:ident, $label:expr) => {
        impl FieldValue for $name {
            const TYPE: FieldType = FieldType::Choice(<$name as Word>::WORDS);

            fn parse(text: &str) -> Result<Self, FieldError> {
                let label: fn($name) -> &'static str = $label;
                <$name as Word>::parse(text)
                    .or_else(|| <$name as Word>::ALL.iter().copied().find(|v| label(*v).eq_ignore_ascii_case(text.trim())))
                    .ok_or_else(|| FieldError::Bad(format!("one of {}", <$name as Word>::WORDS.join(", "))))
            }

            fn show(self) -> String {
                let label: fn($name) -> &'static str = $label;
                label(self).to_string()
            }
        }
    };
}

choice_value!(Starting, Starting::label);
choice_value!(IndicatorColour, IndicatorColour::label);
choice_value!(SignalType, SignalType::word);
choice_value!(FuseClass, FuseClass::label);
choice_value!(Poles, Poles::label);
choice_value!(TripCurve, TripCurve::letter);
choice_value!(FuseSize, FuseSize::label);
choice_value!(ResidualType, ResidualType::label);
