//! Page types: the `page` catalogue's typed record.
//!
//! What a type brings to its pages is resolved in `crate::document::sheet` (sheet, frame, zones, checks)
//! and `ecad-electrical` (`styles::page`).

#![deny(clippy::wildcard_enum_match_arm)]

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::document::sheet::{PROJECT_SHEET, SheetSpec};
use crate::objects::styles::StyleRef;

crate::word_enum! {
    /// Which generated kinds a type's pages may be.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub enum Holds {
        /// Drawn pages only.
        None,
        /// Cover, revision history, contents.
        Front,
        /// BOM, terminal plan, cable schedule, PLC racks, I/O list.
        Report,
    }
}

/// The sheet a page type gives: the project's, or its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeSheet {
    Project,
    Sheet(SheetSpec),
}

/// The frame a page type gives: the project's choice, or a frame style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeFrame {
    Project,
    Style(StyleRef),
}

/// The title block a page type draws: its frame's own, or another frame style's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeBlock {
    Frame,
    Style(StyleRef),
}

/// The word meaning "the frame's own block".
pub const OWN_BLOCK: &str = "frame";

impl TypeSheet {
    /// `project` (any case) or a sheet (`A3`, `A2 12x8`, …).
    pub fn parse(text: &str) -> Option<TypeSheet> {
        if text.trim().eq_ignore_ascii_case(PROJECT_SHEET) {
            return Some(TypeSheet::Project);
        }
        SheetSpec::parse(text).ok().map(TypeSheet::Sheet)
    }

    /// Its stored text.
    pub fn text(&self) -> String {
        match self {
            TypeSheet::Project => PROJECT_SHEET.to_string(),
            TypeSheet::Sheet(s) => s.text(),
        }
    }
}

impl TypeFrame {
    /// `project` (any case) or a frame style's ID.
    pub fn parse(text: &str) -> Option<TypeFrame> {
        if text.trim().eq_ignore_ascii_case(PROJECT_SHEET) {
            return Some(TypeFrame::Project);
        }
        StyleRef::parse(text).map(TypeFrame::Style)
    }

    /// Its stored text.
    pub fn text(&self) -> String {
        match self {
            TypeFrame::Project => PROJECT_SHEET.to_string(),
            TypeFrame::Style(id) => id.text(),
        }
    }
}

impl TypeBlock {
    /// `frame` (any case) or a frame style's ID.
    pub fn parse(text: &str) -> Option<TypeBlock> {
        if text.trim().eq_ignore_ascii_case(OWN_BLOCK) {
            return Some(TypeBlock::Frame);
        }
        StyleRef::parse(text).map(TypeBlock::Style)
    }

    /// Its stored text.
    pub fn text(&self) -> String {
        match self {
            TypeBlock::Frame => OWN_BLOCK.to_string(),
            TypeBlock::Style(id) => id.text(),
        }
    }
}

macro_rules! text_serde {
    ($t:ty, $what:literal) => {
        impl Serialize for $t {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.text())
            }
        }

        impl<'de> Deserialize<'de> for $t {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = String::deserialize(d)?;
                <$t>::parse(&text).ok_or_else(|| serde::de::Error::custom(format!("{text:?} is not {}", $what)))
            }
        }
    };
}

text_serde!(TypeSheet, "a page type's sheet (project or a sheet)");
text_serde!(TypeFrame, "a page type's frame (project or a frame style)");
text_serde!(TypeBlock, "a page type's title block (frame or a frame style)");

/// A page type: what a standard or a project says about one. `None` inherits.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageType {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<TypeSheet>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<TypeFrame>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_block: Option<TypeBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zones: Option<bool>,
    /// Checks and references.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holds: Option<Holds>,
}

impl PageType {
    /// The fields it sets, as `(key, stored text)`, in field order.
    pub fn entries(&self) -> Vec<(&'static str, String)> {
        let flag = |b: bool| crate::naming::words::flag_word(b).to_string();
        let mut out = Vec::new();
        if let Some(v) = &self.sheet {
            out.push(("sheet", v.text()));
        }
        if let Some(v) = &self.frame {
            out.push(("frame", v.text()));
        }
        if let Some(v) = &self.title_block {
            out.push(("title_block", v.text()));
        }
        if let Some(v) = self.zones {
            out.push(("zones", flag(v)));
        }
        if let Some(v) = self.checks {
            out.push(("checks", flag(v)));
        }
        if let Some(v) = self.holds {
            out.push(("holds", crate::naming::words::Word::word(v).to_string()));
        }
        out
    }
}
