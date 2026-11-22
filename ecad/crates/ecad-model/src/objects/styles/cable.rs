//! Cable types (brief 057, brief 059, design 072 §4.6, §8, F37): the `cable` catalogue's typed
//! record, and the named core colour schemes a type may take its core colours from.
//!
//! The schemes' colours are built-in data (`ecad-electrical`'s `defaults/core_schemes.toml`);
//! which colours a cable's cores get is worked out there (`styles::cable`).

#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::geometry::units::Mm;
use crate::naming::text::Text;
use crate::objects::styles::wire::ConductorRating;
use crate::objects::wire::{Colours, WireSize};

crate::word_enum! {
    /// A cable's screen.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Shield {
        None,
        Overall,
        Pairs,
        #[strum(serialize = "overall+pairs")]
        OverallAndPairs,
    }
}

crate::word_enum! {
    /// A named core colour scheme (design 072 F37): how a cable type's cores are coloured, by
    /// core count. The colours are built-in data.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub enum CoreScheme {
        /// Power cables: brown, black, grey, blue and green/yellow by core count (HD 308);
        /// numbered black cores from six cores up.
        Harmonised,
        /// Black cores printed with numbers, and a green/yellow earth core.
        Numbered,
        /// Data cables: DIN 47100's sequence without repeated colours.
        Din47100,
        /// Control cables: ICEA method 1, a base colour with a tracer.
        IceaK2,
    }
}

/// The most cores a type's explicit colour list names.
pub const MAX_CORES: usize = 64;

/// A cable type's explicit core colours, in core order: `BN,BK,GY,GN/YE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreColours(Vec<Colours>);

impl CoreColours {
    /// A list of 1 to [`MAX_CORES`] cores.
    pub fn new(list: Vec<Colours>) -> Option<CoreColours> {
        (!list.is_empty() && list.len() <= MAX_CORES).then_some(CoreColours(list))
    }

    /// The cores `text` names, comma separated, each in any form [`Colours::parse`] reads.
    pub fn parse(text: &str) -> Option<CoreColours> {
        CoreColours::new(text.split(',').map(Colours::parse).collect::<Option<Vec<_>>>()?)
    }

    /// The cores' colours, in order.
    pub fn list(&self) -> &[Colours] {
        &self.0
    }
}

impl fmt::Display for CoreColours {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.iter().map(ToString::to_string).collect::<Vec<_>>().join(","))
    }
}

impl Serialize for CoreColours {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for CoreColours {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        CoreColours::parse(&text)
            .filter(|c| c.to_string() == text)
            .ok_or_else(|| serde::de::Error::custom(format!("{text:?} is not a list of core colours (BN,BK,GY,GN/YE)")))
    }
}

/// The longest core offer (`4G`, `BN,BK,GY,GN/YE`).
pub const MAX_OFFER: usize = 256;
/// The longest gland size.
pub const MAX_GLAND: usize = 16;
/// The longest part number, maker or description.
pub const MAX_PART_TEXT: usize = 64;

/// A cable type: what a standard or a project says about one. `None` inherits.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CableType {
    /// The cores' size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<WireSize>,
    #[serde(default, skip_serializing_if = "ConductorRating::is_empty")]
    pub rating: ConductorRating,
    /// The cores' colours, in core order; when set, the scheme is not used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core_colours: Option<CoreColours>,
    /// The scheme the cores' colours come from, by core count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core_scheme: Option<CoreScheme>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_number: Option<Text<MAX_PART_TEXT>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maker: Option<Text<MAX_PART_TEXT>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Text<MAX_PART_TEXT>>,
    /// The core offer a cable of this type gets when it has none of its own (`4G`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cores: Option<Text<MAX_OFFER>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shield: Option<Shield>,
    /// The gland at both ends (`M20`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gland: Option<Text<MAX_GLAND>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outer_diameter: Option<Mm>,
}

impl CableType {
    /// The fields it sets, as `(key, stored text)`, in field order.
    pub fn entries(&self) -> Vec<(&'static str, String)> {
        let r = &self.rating;
        let word = |w: &dyn Fn() -> &'static str| w().to_string();
        let mut out: Vec<(&'static str, String)> = Vec::new();
        let mut put = |k: &'static str, v: Option<String>| {
            if let Some(v) = v {
                out.push((k, v));
            }
        };
        put("size", self.size.map(|v| v.to_string()));
        put("material", r.material.map(|v| word(&|| crate::naming::words::Word::word(v))));
        put("max_temperature", r.max_temperature.map(|v| v.0.to_string()));
        put("conductor_class", r.conductor_class.map(|v| word(&|| crate::naming::words::Word::word(v))));
        put("rated_current", r.rated_current.map(|v| v.0.to_string()));
        put("rated_at", r.rated_at.map(|v| v.0.to_string()));
        put("core_colours", self.core_colours.as_ref().map(ToString::to_string));
        put("core_scheme", self.core_scheme.map(|v| word(&|| crate::naming::words::Word::word(v))));
        put("part_number", self.part_number.as_ref().map(ToString::to_string));
        put("maker", self.maker.as_ref().map(ToString::to_string));
        put("description", self.description.as_ref().map(ToString::to_string));
        put("cores", self.cores.as_ref().map(ToString::to_string));
        put("shield", self.shield.map(|v| word(&|| crate::naming::words::Word::word(v))));
        put("gland", self.gland.as_ref().map(ToString::to_string));
        put("outer_diameter", self.outer_diameter.map(|v| v.0.to_string()));
        out
    }
}
