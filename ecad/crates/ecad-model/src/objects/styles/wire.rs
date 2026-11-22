//! Wire types (design 072 §4.6, §8, F36): the `wire` catalogue's typed record.
//!
//! A wire type is a library entry a wire refers to: the potential it is the default for, its size,
//! colours and insulation, its BOM part, its allowance, its datasheet rating and its drawn line.
//! Every field is optional: an absent field inherits from the next layer (the standard, the
//! built-in type, the catalogue's defaults).
//!
//! What the fields mean (how a wire's look and rating are worked out) is `ecad-electrical`'s
//! (`styles::wire`, `rating`). Excluded: a wire's own overrides (lane D, `Wire`).

#![deny(clippy::wildcard_enum_match_arm)]

use serde::{Deserialize, Serialize};

use crate::geometry::colour::ConductorColour;
use crate::naming::text::Text;
use crate::geometry::units::{Amps, Celsius, Metres, Mm};
use crate::objects::wire::{Colours, WireSize};

crate::word_enum! {
    /// How a wire type's line (and a location box's outline) is drawn.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum LineType {
        Continuous,
        Dashed,
        Dashdot,
    }
}

crate::word_enum! {
    /// Which routes a wire type labels: the project's rule, once per net, each route, or none.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum RouteLabels {
        Project,
        Net,
        Each,
        None,
    }
}

crate::word_enum! {
    /// A conductor material.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Material {
        Cu,
    }
}

crate::word_enum! {
    /// A conductor class (IEC 60228): solid, stranded, flexible, highly flexible.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ConductorClass {
        #[strum(serialize = "1")]
        One,
        #[strum(serialize = "2")]
        Two,
        #[strum(serialize = "5")]
        Five,
        #[strum(serialize = "6")]
        Six,
    }
}

impl ConductorClass {
    /// Its number (1, 2, 5, 6).
    pub fn number(self) -> u8 {
        match self {
            ConductorClass::One => 1,
            ConductorClass::Two => 2,
            ConductorClass::Five => 5,
            ConductorClass::Six => 6,
        }
    }
}

crate::word_enum! {
    /// A conductor's insulation compound (design 072 F36).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Insulation {
        /// Polyvinyl chloride.
        Pvc,
        /// Cross-linked polyethylene.
        Xlpe,
        /// Ethylene propylene rubber.
        Epr,
        /// Natural or synthetic rubber.
        Rubber,
        /// Silicone rubber.
        Silicone,
        /// Polytetrafluoroethylene.
        Ptfe,
        /// A low-smoke, zero-halogen compound.
        Lszh,
    }
}

/// What a wire type's route is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineColour {
    /// The wire's insulation colour.
    Wire,
    /// The drawing's ink.
    Ink,
    /// One base colour.
    Colour(ConductorColour),
}

impl LineColour {
    /// The value `text` names: `wire`, `ink`, or one colour in any of its forms, any case.
    pub fn parse(text: &str) -> Option<LineColour> {
        let t = text.trim();
        if t.eq_ignore_ascii_case("wire") {
            Some(LineColour::Wire)
        } else if t.eq_ignore_ascii_case("ink") {
            Some(LineColour::Ink)
        } else {
            ConductorColour::parse(t).map(LineColour::Colour)
        }
    }

    /// Its stored word: `wire`, `ink`, or the colour's 2-letter code.
    pub fn word(self) -> &'static str {
        match self {
            LineColour::Wire => "wire",
            LineColour::Ink => "ink",
            LineColour::Colour(c) => c.forms().code2.as_str(),
        }
    }
}

impl Serialize for LineColour {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.word())
    }
}

impl<'de> Deserialize<'de> for LineColour {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        // Strict: the stored form only.
        LineColour::parse(&text)
            .filter(|c| c.word() == text)
            .ok_or_else(|| serde::de::Error::custom(format!("{text:?} is not a line colour (wire, ink or a 2-letter colour code)")))
    }
}

/// The longest potential name.
pub const MAX_POTENTIAL: usize = 24;
/// The longest part number, maker or description.
pub const MAX_PART_TEXT: usize = 64;

/// A conductor's datasheet rating (brief 057, design 057 §4.4): shared by wire types and cable
/// types.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConductorRating {
    /// The rated current at `rated_at`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_current: Option<Amps>,
    /// The ambient temperature the rated current is given at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rated_at: Option<Celsius>,
    /// The conductor's highest operating temperature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_temperature: Option<Celsius>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<Material>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conductor_class: Option<ConductorClass>,
}

impl ConductorRating {
    /// Whether it sets nothing.
    pub fn is_empty(&self) -> bool {
        *self == ConductorRating::default()
    }
}

/// A wire type (design 072 §4.6): what a standard, a project or the built-in data says about one
/// wire type. `None` inherits.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireType {
    /// The potential this type is the default for (one type per potential).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub potential: Option<Text<MAX_POTENTIAL>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<WireSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colours: Option<Colours>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insulation: Option<Insulation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_colour: Option<LineColour>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_type: Option<LineType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_weight: Option<Mm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_rule: Option<RouteLabels>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_number: Option<Text<MAX_PART_TEXT>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maker: Option<Text<MAX_PART_TEXT>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Text<MAX_PART_TEXT>>,
    /// Extra length per wire for the cut list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowance: Option<Metres>,
    #[serde(default, skip_serializing_if = "ConductorRating::is_empty")]
    pub rating: ConductorRating,
}

impl WireType {
    /// The fields it sets, as `(key, stored text)`, in field order: for messages and the
    /// inspector.
    pub fn entries(&self) -> Vec<(&'static str, String)> {
        let r = &self.rating;
        let mut out: Vec<(&'static str, String)> = Vec::new();
        let mut put = |k: &'static str, v: Option<String>| {
            if let Some(v) = v {
                out.push((k, v));
            }
        };
        put("potential", self.potential.as_ref().map(ToString::to_string));
        put("size", self.size.map(|v| v.to_string()));
        put("colours", self.colours.as_ref().map(ToString::to_string));
        put("insulation", self.insulation.map(|v| crate::naming::words::Word::word(v).to_string()));
        put("line_colour", self.line_colour.map(|v| v.word().to_string()));
        put("line_type", self.line_type.map(|v| crate::naming::words::Word::word(v).to_string()));
        put("line_weight", self.line_weight.map(|v| v.0.to_string()));
        put("label_rule", self.label_rule.map(|v| crate::naming::words::Word::word(v).to_string()));
        put("part_number", self.part_number.as_ref().map(ToString::to_string));
        put("maker", self.maker.as_ref().map(ToString::to_string));
        put("description", self.description.as_ref().map(ToString::to_string));
        put("allowance", self.allowance.map(|v| v.0.to_string()));
        put("rated_current", r.rated_current.map(|v| v.0.to_string()));
        put("rated_at", r.rated_at.map(|v| v.0.to_string()));
        put("max_temperature", r.max_temperature.map(|v| v.0.to_string()));
        put("material", r.material.map(|v| crate::naming::words::Word::word(v).to_string()));
        put("conductor_class", r.conductor_class.map(|v| crate::naming::words::Word::word(v).to_string()));
        out
    }
}
