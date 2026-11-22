//! Text styles (design 050 §4.1, design 072 §8): the `text` catalogue's typed record, and the
//! colour a style field takes ([`StyleColour`], shared by text, location and frame styles).
//!
//! What a text style means (cap heights, the project font, how a colour draws) is
//! `ecad-electrical`'s (`text_style`, `styles::text`).

#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::geometry::colour::{ConductorColour, Rgb};
use crate::geometry::units::Mm;
use crate::naming::text::Text;

crate::word_enum! {
    /// A text weight.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Weight {
        Regular,
        Bold,
    }
}

/// The colour a style draws in: none (the ink), one base colour, or a display colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleColour {
    /// The drawing's ink.
    None,
    /// One base colour (IEC 60757), stored by its 2-letter code.
    Code(ConductorColour),
    /// A display colour, stored as `#rrggbb`.
    Rgb(Rgb),
}

impl StyleColour {
    /// The colour `text` names: `none`, one colour in any of its forms, or `#rrggbb`; any case.
    pub fn parse(text: &str) -> Option<StyleColour> {
        let t = text.trim();
        if t.eq_ignore_ascii_case("none") {
            return Some(StyleColour::None);
        }
        Rgb::parse(t).map(StyleColour::Rgb).or_else(|| ConductorColour::parse(t).map(StyleColour::Code))
    }
}

impl fmt::Display for StyleColour {
    /// Its stored form: `none`, `BU`, `#c62828`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StyleColour::None => f.write_str("none"),
            StyleColour::Code(c) => f.write_str(&c.forms().code2),
            StyleColour::Rgb(Rgb(r, g, b)) => write!(f, "#{r:02x}{g:02x}{b:02x}"),
        }
    }
}

impl Serialize for StyleColour {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for StyleColour {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        // Strict: the stored form only.
        StyleColour::parse(&text)
            .filter(|c| c.to_string() == text)
            .ok_or_else(|| serde::de::Error::custom(format!("{text:?} is not a style colour (none, a 2-letter colour code or #rrggbb)")))
    }
}

/// The longest font name.
pub(crate) const MAX_FONT: usize = 64;

/// The font a text style uses: the project's (`text.font`) or a named one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Font {
    Project,
    Named(Text<MAX_FONT>),
}

impl Font {
    /// The word meaning the project's font.
    pub const PROJECT: &'static str = "project";

    /// The font `text` names: `project` (any case), else a font name.
    pub fn parse(text: &str) -> Option<Font> {
        if text.trim().eq_ignore_ascii_case(Font::PROJECT) {
            return Some(Font::Project);
        }
        Text::new(text).ok().map(Font::Named)
    }

    /// Its stored text.
    pub fn as_str(&self) -> &str {
        match self {
            Font::Project => Font::PROJECT,
            Font::Named(t) => t.as_str(),
        }
    }
}

impl Serialize for Font {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Font {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        if text == Font::PROJECT {
            return Ok(Font::Project);
        }
        // Strict: a name is stored trimmed and valid (`Text`'s own read).
        let t: Text<MAX_FONT> = serde::de::Deserialize::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(&text))?;
        Ok(Font::Named(t))
    }
}

/// A text style (design 050 §4.1): what a standard or a project says about one. `None` inherits.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<Font>,
    /// Cap height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<Mm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<Weight>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour: Option<StyleColour>,
}

impl TextStyle {
    /// The fields it sets, as `(key, stored text)`, in field order.
    pub fn entries(&self) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        if let Some(v) = &self.font {
            out.push(("font", v.as_str().to_string()));
        }
        if let Some(v) = self.height {
            out.push(("height", v.0.to_string()));
        }
        if let Some(v) = self.weight {
            out.push(("weight", crate::naming::words::Word::word(v).to_string()));
        }
        if let Some(v) = self.colour {
            out.push(("colour", v.to_string()));
        }
        out
    }
}
