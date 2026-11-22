//! Rating tables (brief 057, design 057 FP8, design 072 §8): the `ratingtable` catalogue's typed
//! record. What the rows mean (a CSV data set of current ratings) is `ecad-electrical`'s
//! (`rating::dataset`).

use serde::{Deserialize, Serialize};

use crate::naming::text::{LongText, Text};

/// The longest source text.
pub const MAX_SOURCE: usize = 200;
/// The longest rows text, in characters (the data set's own bound is in bytes, and no larger).
pub const MAX_ROWS: usize = 256 * 1024;

/// A rating table: what a standard or a project stores about one. `None` inherits.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RatingTable {
    /// Where the values come from, in the user's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Text<MAX_SOURCE>>,
    /// The CSV rows, one line each.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<LongText<MAX_ROWS>>,
}

impl RatingTable {
    /// The fields it sets, as `(key, stored text)`, in field order.
    pub fn entries(&self) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        if let Some(v) = &self.source {
            out.push(("source", v.to_string()));
        }
        if let Some(v) = &self.rows {
            out.push(("rows", v.to_string()));
        }
        out
    }
}
