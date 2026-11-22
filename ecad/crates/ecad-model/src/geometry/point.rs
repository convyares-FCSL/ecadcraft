//! What a connection point is called on its device: [`PointKey`] (design 072 §2.7, I3).
//!
//! A point's key says what kind of point it is as data: a definition key (`A1`), a connector side,
//! a strip level, a PLC channel point or a channel cell's role. The file stores its text form
//! (`A1`, `.s`, `.1i`, `.3.in`, `.in`), read strictly: a key that starts with `.` must be one of
//! the structured forms, and a definition key never starts with `.`.
//!
//! Excludes: what a point means electrically (ecad-electrical) and where it sits (the symbol).

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::naming::words::{Side, Word};

/// Most characters in a definition key.
pub const MAX_DEF_KEY: usize = 32;

/// Highest PLC channel index a key may name.
pub const MAX_CHANNEL_INDEX: u16 = 9999;

crate::word_enum! {
    /// A PLC channel point's role (design 045 §4.1), in the order a channel lists its points.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum ChannelRole {
        In,
        Out,
        #[strum(serialize = "+")]
        Plus,
        #[strum(serialize = "-")]
        Minus,
        #[strum(serialize = "I+")]
        CurrentPlus,
        #[strum(serialize = "S+")]
        SensePlus,
        #[strum(serialize = "S-")]
        SenseMinus,
        #[strum(serialize = "I-")]
        CurrentMinus,
        #[strum(serialize = "L+")]
        SupplyPlus,
        #[strum(serialize = "C/Q")]
        Cq,
        #[strum(serialize = "L-")]
        SupplyMinus,
    }
}

impl ChannelRole {
    /// The role written exactly (`in`, `C/Q`): unlike [`Word::parse`], case and spaces count.
    pub fn exact(text: &str) -> Option<ChannelRole> {
        ChannelRole::ALL.iter().copied().find(|r| r.word() == text)
    }
}

crate::word_enum! {
    /// Which side of a breaking strip level a point is (design 036): in or out.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum LevelIo {
        In,
        Out,
    }
}

impl LevelIo {
    fn letter(self) -> char {
        match self {
            LevelIo::In => 'i',
            LevelIo::Out => 'o',
        }
    }
}

/// What a connection point is called on its device (design 072 §2.7).
///
/// # Semantics
/// - `Def`: a symbol's own key (`A1`, `13`, `1U1`); never empty, never starts with `.`.
/// - `Side`: one half of a connector pin (`.s` socket, `.p` plug).
/// - `Level`: a strip level (`.1`), or one side of a breaking level (`.1i`, `.1o`); level ≥ 1.
/// - `Channel`: a point of PLC channel `index` (`.3.in`).
/// - `Role`: a channel cell's point before it joins a card (`.in`).
///
/// # Example
/// ```
/// use crate::geometry::point::{ChannelRole, PointKey};
/// let k = PointKey::parse(".3.in").unwrap();
/// assert_eq!(k, PointKey::Channel { index: 3, role: ChannelRole::In });
/// assert_eq!(k.to_string(), ".3.in");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PointKey {
    Def(String),
    Side(Side),
    Level { level: u8, io: Option<LevelIo> },
    Channel { index: u16, role: ChannelRole },
    Role(ChannelRole),
}

/// A text that is not a point key, with the reason in words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointKeyError(pub String);

impl fmt::Display for PointKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PointKeyError {}

/// The side key letter of `side`: `s` or `p`.
fn side_letter(side: Side) -> char {
    match side {
        Side::Socket => 's',
        Side::Plug => 'p',
    }
}

impl PointKey {
    /// A definition key from code, for keys known to be valid (`A1`).
    pub fn def(key: &str) -> PointKey {
        PointKey::Def(key.to_string())
    }

    /// A point key from its text form.
    ///
    /// # Errors
    /// Empty text, a hidden character, a definition key over [`MAX_DEF_KEY`] characters, or a
    /// `.` form that is none of the structured ones (`.x`, `.0`, `.3.nope`, `.99999.in`).
    pub fn parse(text: &str) -> Result<PointKey, PointKeyError> {
        let bad = || PointKeyError(format!("`{text}` is not a connection point key"));
        if text.is_empty() || crate::naming::text::has_hidden(text) {
            return Err(bad());
        }
        let Some(rest) = text.strip_prefix('.') else {
            if text.chars().count() > MAX_DEF_KEY {
                return Err(PointKeyError(format!("`{text}` is longer than {MAX_DEF_KEY} characters")));
            }
            return Ok(PointKey::Def(text.to_string()));
        };
        match rest {
            "s" => return Ok(PointKey::Side(Side::Socket)),
            "p" => return Ok(PointKey::Side(Side::Plug)),
            _ => {}
        }
        if let Some(role) = ChannelRole::exact(rest) {
            return Ok(PointKey::Role(role));
        }
        if let Some((i, role)) = rest.split_once('.') {
            let index = digits(i).filter(|n| *n <= u32::from(MAX_CHANNEL_INDEX)).ok_or_else(bad)?;
            let role = ChannelRole::exact(role).ok_or_else(bad)?;
            return Ok(PointKey::Channel { index: u16::try_from(index).map_err(|_| bad())?, role });
        }
        let (n, io) = match rest.strip_suffix('i') {
            Some(n) => (n, Some(LevelIo::In)),
            None => match rest.strip_suffix('o') {
                Some(n) => (n, Some(LevelIo::Out)),
                None => (rest, None),
            },
        };
        let level = digits(n).and_then(|n| u8::try_from(n).ok()).filter(|n| *n >= 1).ok_or_else(bad)?;
        Ok(PointKey::Level { level, io })
    }

    /// The definition key, for a `Def` key.
    pub fn as_def(&self) -> Option<&str> {
        match self {
            PointKey::Def(k) => Some(k),
            _ => None,
        }
    }

    /// The connector side, for a `Side` key.
    pub fn side(&self) -> Option<Side> {
        match self {
            PointKey::Side(s) => Some(*s),
            _ => None,
        }
    }

    /// The channel index and role, for a `Channel` key.
    pub fn channel(&self) -> Option<(u16, ChannelRole)> {
        match self {
            PointKey::Channel { index, role } => Some((*index, *role)),
            _ => None,
        }
    }

    /// The role of a `Channel` or `Role` key.
    pub fn role(&self) -> Option<ChannelRole> {
        match self {
            PointKey::Channel { role, .. } | PointKey::Role(role) => Some(*role),
            _ => None,
        }
    }

    /// The level and its side, for a `Level` key.
    pub fn level(&self) -> Option<(u8, Option<LevelIo>)> {
        match self {
            PointKey::Level { level, io } => Some((*level, *io)),
            _ => None,
        }
    }
}

/// Up to four ASCII digits as a number.
fn digits(s: &str) -> Option<u32> {
    if s.is_empty() || s.len() > 4 || !crate::naming::words::is_digits(s) {
        return None;
    }
    s.parse().ok()
}

impl fmt::Display for PointKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PointKey::Def(k) => f.write_str(k),
            PointKey::Side(s) => write!(f, ".{}", side_letter(*s)),
            PointKey::Level { level, io: None } => write!(f, ".{level}"),
            PointKey::Level { level, io: Some(io) } => write!(f, ".{level}{}", io.letter()),
            PointKey::Channel { index, role } => write!(f, ".{index}.{}", role.word()),
            PointKey::Role(role) => write!(f, ".{}", role.word()),
        }
    }
}

impl PartialEq<str> for PointKey {
    fn eq(&self, other: &str) -> bool {
        match self {
            PointKey::Def(k) => k == other,
            _ => PointKey::parse(other).is_ok_and(|k| k == *self),
        }
    }
}

impl PartialEq<&str> for PointKey {
    fn eq(&self, other: &&str) -> bool {
        self == *other
    }
}

impl std::str::FromStr for PointKey {
    type Err = PointKeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        PointKey::parse(s)
    }
}

impl Serialize for PointKey {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for PointKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        PointKey::parse(&text).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for PointKey {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        "PointKey".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type": "string", "minLength": 1})
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn every_form_reads_back_as_written() {
        let rows: &[(&str, PointKey)] = &[
            ("A1", PointKey::def("A1")),
            ("I0.0", PointKey::def("I0.0")),
            (".s", PointKey::Side(Side::Socket)),
            (".p", PointKey::Side(Side::Plug)),
            (".1", PointKey::Level { level: 1, io: None }),
            (".2i", PointKey::Level { level: 2, io: Some(LevelIo::In) }),
            (".2o", PointKey::Level { level: 2, io: Some(LevelIo::Out) }),
            (".3.in", PointKey::Channel { index: 3, role: ChannelRole::In }),
            (".0.C/Q", PointKey::Channel { index: 0, role: ChannelRole::Cq }),
            (".L+", PointKey::Role(ChannelRole::SupplyPlus)),
            (".-", PointKey::Role(ChannelRole::Minus)),
        ];
        for (text, key) in rows {
            assert_eq!(PointKey::parse(text).as_ref(), Ok(key), "{text}");
            assert_eq!(key.to_string(), *text);
        }
    }

    #[test]
    fn malformed_keys_are_refused() {
        for text in ["", ".", ".x", ".0", ".256", ".3.nope", ".99999.in", ".-1.in", ".S", ".IN", "a\u{200b}"] {
            assert!(PointKey::parse(text).is_err(), "{text:?}");
        }
        assert!(PointKey::parse(&"x".repeat(MAX_DEF_KEY + 1)).is_err());
    }
}
