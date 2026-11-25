//! Frame styles: the `frame` catalogue's typed record, with
//! its title block.
//!
//! How a frame is drawn is `ecad-electrical`'s (`styles::frame`); which frame a page uses, its
//! sheet and margins are resolved in `crate::document::sheet`.

#![deny(clippy::wildcard_enum_match_arm)]

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::document::sheet::{Margins, SheetSpec, margins_text, parse_frame_sheet, parse_margins};
use crate::geometry::units::Mm;
use crate::objects::styles::text::StyleColour;
use crate::objects::title_block::TitleBlockLayout;

crate::word_enum! {
    /// Where a frame's block sits.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum BlockAt {
        /// The block's left part stretches to the frame's left edge (built-ins).
        #[default]
        BottomFull,
        /// The block keeps its width against the frame's bottom-right corner.
        BottomRight,
    }
}

/// The sheet a frame is drawn for: the page's own, or one sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameSheet {
    Page,
    Sheet(SheetSpec),
}

impl FrameSheet {
    /// The word meaning "the page's own sheet".
    pub const PAGE: &'static str = "page";

    /// `page` (any case) or a sheet.
    pub fn parse(text: &str) -> Option<FrameSheet> {
        if text.trim().eq_ignore_ascii_case(FrameSheet::PAGE) {
            return Some(FrameSheet::Page);
        }
        parse_frame_sheet(text).map(FrameSheet::Sheet)
    }

    /// Its stored text.
    pub fn text(&self) -> String {
        match self {
            FrameSheet::Page => FrameSheet::PAGE.to_string(),
            FrameSheet::Sheet(s) => s.text(),
        }
    }
}

impl Serialize for FrameSheet {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.text())
    }
}

impl<'de> Deserialize<'de> for FrameSheet {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        FrameSheet::parse(&text).ok_or_else(|| serde::de::Error::custom(format!("{text:?} is not a frame's sheet (page or a sheet)")))
    }
}

/// A frame's margins as stored: `l,r,b,t` in millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameMargins(pub Margins);

impl Serialize for FrameMargins {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&margins_text(&self.0))
    }
}

impl<'de> Deserialize<'de> for FrameMargins {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        parse_margins(&text)
            .map(FrameMargins)
            .ok_or_else(|| serde::de::Error::custom(format!("{text:?} is not margins (four millimetre values, l,r,b,t)")))
    }
}

/// A frame style: what a standard or a project says about one. `None` inherits.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_weight: Option<Mm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_weight: Option<Mm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_colour: Option<StyleColour>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid_labels: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<FrameSheet>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margins: Option<FrameMargins>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub centre_marks: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<BlockAt>,
    /// The frame's own title block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_block: Option<TitleBlockLayout>,
}

impl FrameStyle {
    /// The fields it sets, as `(key, stored text)`, in field order (the title block is not a
    /// field: it is edited in the title block editor).
    pub fn entries(&self) -> Vec<(&'static str, String)> {
        let flag = |b: bool| crate::naming::words::flag_word(b).to_string();
        let mut out = Vec::new();
        if let Some(v) = self.border_weight {
            out.push(("border_weight", v.0.to_string()));
        }
        if let Some(v) = self.inner_weight {
            out.push(("inner_weight", v.0.to_string()));
        }
        if let Some(v) = self.line_colour {
            out.push(("line_colour", v.to_string()));
        }
        if let Some(v) = self.grid_labels {
            out.push(("grid_labels", flag(v)));
        }
        if let Some(v) = &self.sheet {
            out.push(("sheet", v.text()));
        }
        if let Some(v) = &self.margins {
            out.push(("margins", margins_text(&v.0)));
        }
        if let Some(v) = self.centre_marks {
            out.push(("centre_marks", flag(v)));
        }
        if let Some(v) = self.block {
            out.push(("block", crate::naming::words::Word::word(v).to_string()));
        }
        out
    }
}
