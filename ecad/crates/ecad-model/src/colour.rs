//! Wire colours (design 072 §4.6): the one primitive wires, cable cores, wire types and cable
//! types share.
//!
//! A [`ConductorColour`] is an IEC 60757 base colour. The variant is the identity; its forms (name,
//! 2-letter code, 3-letter code, display hex) are data, in `defaults/colours.toml` embedded at
//! build time ([`COLOURS`]) and read once. A project stores the 2-letter code; reading accepts any
//! form in any case. A two-colour conductor is an ordered list (`[Green, Yellow]`, written `GN/YE`),
//! never a colour of its own.
//!
//! This module excludes indicator (lamp) colours, which are a different enum, and the drawing
//! colours of wires on a sheet.
#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;
use std::sync::OnceLock;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The embedded data file.
pub const COLOURS: &str = include_str!("../defaults/colours.toml");
/// Its name, for messages.
pub const FILE: &str = "colours.toml";

/// An IEC 60757 base colour of a conductor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, strum::IntoStaticStr, strum::VariantArray)]
#[strum(serialize_all = "snake_case")]
pub enum ConductorColour {
    Black,
    Brown,
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Violet,
    Grey,
    White,
    Pink,
    Gold,
    Turquoise,
    Silver,
}

/// A colour for display: red, green and blue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// `#RRGGBB`.
    pub fn parse(text: &str) -> Option<Rgb> {
        let h = text.strip_prefix('#').filter(|h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit()))?;
        let byte = |i: usize| h.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok());
        Some(Rgb(byte(0)?, byte(2)?, byte(4)?))
    }
}

/// Every form of one colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColourForms {
    /// `Blue`.
    pub name: String,
    /// `BU` (IEC 60757): the stored form.
    pub code2: String,
    /// `BLU`.
    pub code3: String,
    /// The swatch colour.
    pub hex: Rgb,
}

crate::word_enum! {
    /// Which form drawings, labels and reports show (the `wiring.colour_style` setting).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub enum ColourWords {
        #[default]
        Code2,
        Code3,
        Name,
    }
}

impl ConductorColour {
    /// Every colour, in IEC 60757's order.
    pub const ALL: &'static [ConductorColour] = <Self as strum::VariantArray>::VARIANTS;

    /// The colour's key in [`COLOURS`] (`turquoise`).
    pub fn key(self) -> &'static str {
        self.into()
    }

    /// Its forms, from [`COLOURS`].
    pub fn forms(self) -> &'static ColourForms {
        // The discriminant is the colour's place in the table (one table entry per variant, in
        // declaration order), so the index is always in range.
        &forms_table()[self as usize]
    }

    /// The form `words` chooses.
    pub fn shown(self, words: ColourWords) -> &'static str {
        let f = self.forms();
        match words {
            ColourWords::Code2 => &f.code2,
            ColourWords::Code3 => &f.code3,
            ColourWords::Name => &f.name,
        }
    }

    /// The colour any of its forms names, in any case, spaces around ignored.
    ///
    /// # Example
    /// ```
    /// use ecad_model::colour::ConductorColour;
    /// assert_eq!(ConductorColour::parse("bu"), Some(ConductorColour::Blue));
    /// assert_eq!(ConductorColour::parse(" BLU "), Some(ConductorColour::Blue));
    /// assert_eq!(ConductorColour::parse("blue"), Some(ConductorColour::Blue));
    /// assert_eq!(ConductorColour::parse("GNYE"), None);
    /// ```
    pub fn parse(text: &str) -> Option<ConductorColour> {
        let t = text.trim();
        Self::ALL.iter().copied().find(|c| {
            let f = c.forms();
            [&f.code2, &f.code3, &f.name].iter().any(|w| w.eq_ignore_ascii_case(t))
        })
    }
}

impl fmt::Display for ConductorColour {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.forms().code2)
    }
}

/// Stored as its 2-letter code.
impl Serialize for ConductorColour {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.forms().code2)
    }
}

/// Any form is read; anything else is an error naming it and the stored codes.
impl<'de> Deserialize<'de> for ConductorColour {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        ConductorColour::parse(&text).ok_or_else(|| {
            let codes: Vec<&str> = ConductorColour::ALL.iter().map(|c| c.forms().code2.as_str()).collect();
            serde::de::Error::custom(crate::words::unknown_word_message(&text, "ConductorColour", &codes))
        })
    }
}

#[cfg(feature = "schema")]
impl schemars::JsonSchema for ConductorColour {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ConductorColour".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let codes: Vec<&str> = ConductorColour::ALL.iter().map(|c| c.forms().code2.as_str()).collect();
        schemars::json_schema!({"type": "string", "enum": codes})
    }
}

/// The forms of every colour, one per colour in [`ConductorColour::ALL`]'s order, read once.
fn forms_table() -> &'static [ColourForms; 14] {
    static T: OnceLock<[ColourForms; 14]> = OnceLock::new();
    T.get_or_init(|| load(COLOURS))
}

/// Read the forms from `text`. A colour whose table is missing or damaged (not reachable in a
/// tested build) takes its key as name and `?` codes, never a crash.
pub fn load(text: &str) -> [ColourForms; 14] {
    let table = toml::from_str::<toml::Table>(text).ok();
    let one = |c: ConductorColour| {
        let t = table.as_ref().and_then(|t| t.get(c.key())).and_then(toml::Value::as_table);
        let get = |field: &str| t.and_then(|t| t.get(field)).and_then(toml::Value::as_str);
        ColourForms {
            name: get("name").unwrap_or(c.key()).to_string(),
            code2: get("code2").unwrap_or("??").to_string(),
            code3: get("code3").unwrap_or("???").to_string(),
            hex: get("hex").and_then(Rgb::parse).unwrap_or(Rgb(0x80, 0x80, 0x80)),
        }
    };
    use ConductorColour as C;
    [
        one(C::Black),
        one(C::Brown),
        one(C::Red),
        one(C::Orange),
        one(C::Yellow),
        one(C::Green),
        one(C::Blue),
        one(C::Violet),
        one(C::Grey),
        one(C::White),
        one(C::Pink),
        one(C::Gold),
        one(C::Turquoise),
        one(C::Silver),
    ]
}
