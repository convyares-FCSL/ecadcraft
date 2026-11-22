//! A terminal strip's typed data (design 072 §2.5): the rating of each of its fuse positions.
//!
//! # Semantics
//! `fuses` holds a fuse's typed rating by position label (`ConnectionPoint.label`); a position not listed
//! has no rating. Nothing stores the shown text: [`TerminalStrip::fuse_rating`] builds it from the
//! fields (`6.3 A, T`). A stored fuse with no field set, or a value out of range, is not well formed
//! ([`TerminalStrip::problem`]).
//!
//! This module excludes reading a typed rating text (`ecad-electrical`'s rating reader), which
//! positions are fuse positions (the strip's drawn symbols say that), and what a terminal accepts.
#![deny(clippy::wildcard_enum_match_arm)]

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::objects::devices::protection::{Fuse, Protection};

/// Most characters of a position label that holds a fuse rating (`termlabel`'s bound).
pub const MAX_POSITION: usize = 16;

/// A strip position's label (`1`, `PE1`).
pub type PositionLabel = crate::naming::text::Text<MAX_POSITION>;

/// A strip of DIN-rail terminals (group 8, design 072 §2.5).
///
/// # Example
/// ```
/// use ecad_model::objects::devices::{Fuse, PositionLabel, TerminalStrip};
/// use ecad_model::geometry::units::Amps;
/// let mut s = TerminalStrip::default();
/// s.fuses.insert(PositionLabel::new("3").unwrap(), Fuse { rated_current: Some(Amps(4.0)), ..Fuse::default() });
/// assert_eq!(s.fuse_rating("3").as_deref(), Some("4 A"));
/// assert_eq!(serde_json::to_string(&s).unwrap(), r#"{"fuses":{"3":{"rated_current":4.0}}}"#);
/// ```
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalStrip {
    /// Each fuse position's rating, by position label.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fuses: BTreeMap<PositionLabel, Fuse>,
}

impl TerminalStrip {
    /// The fuse rating of position `position`, if it has one.
    pub fn fuse(&self, position: &str) -> Option<&Fuse> {
        self.fuses.iter().find(|(l, _)| l.as_str() == position).map(|(_, f)| f)
    }

    /// The fuse rating of position `position` as shown (`6.3 A, T`), if it has one.
    pub fn fuse_rating(&self, position: &str) -> Option<String> {
        self.fuse(position).map(ToString::to_string).filter(|t| !t.is_empty())
    }

    /// Why it is not well formed, if it is not: a fuse with no field set, or a value out of range.
    pub fn problem(&self) -> Option<String> {
        for (p, f) in &self.fuses {
            if *f == Fuse::default() {
                return Some(format!("fuse position {p} holds a rating with no value"));
            }
            if let Some(m) = Protection::Fuse(f.clone()).problem() {
                return Some(format!("fuse position {p}: {m}"));
            }
        }
        None
    }
}

impl fmt::Display for TerminalStrip {
    /// Each rated position: `1: 4 A; 2: 6.3 A, T`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.fuses.iter().map(|(p, x)| format!("{p}: {x}")).collect();
        f.write_str(&parts.join("; "))
    }
}
