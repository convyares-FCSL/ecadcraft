//! Location box styles: the `location` catalogue's typed record.
//!
//! How a box is drawn from it is `ecad-electrical`'s (`styles::location`).

use serde::{Deserialize, Serialize};

use crate::geometry::units::Mm;
use crate::objects::styles::StyleRef;
use crate::objects::styles::text::StyleColour;
use crate::objects::styles::wire::LineType;

/// A location box style: what a standard or a project says about one. `None` inherits.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocationStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_type: Option<LineType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_weight: Option<Mm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_colour: Option<StyleColour>,
    /// The text style of the box's label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_style: Option<StyleRef>,
    /// Reserved: stored and listed, drawn as none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<StyleColour>,
}

impl LocationStyle {
    /// The fields it sets, as `(key, stored text)`, in field order.
    pub fn entries(&self) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        if let Some(v) = self.line_type {
            out.push(("line_type", crate::naming::words::Word::word(v).to_string()));
        }
        if let Some(v) = self.line_weight {
            out.push(("line_weight", v.0.to_string()));
        }
        if let Some(v) = self.line_colour {
            out.push(("line_colour", v.to_string()));
        }
        if let Some(v) = &self.label_style {
            out.push(("label_style", v.text()));
        }
        if let Some(v) = self.fill {
            out.push(("fill", v.to_string()));
        }
        out
    }
}
