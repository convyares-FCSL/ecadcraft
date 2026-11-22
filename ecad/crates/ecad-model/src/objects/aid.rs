//! Drawing aids: continuations and location boxes (design 072 §6.2, lane F).
//!
//! # Semantics
//! - An [`Aid`] is drawn but is not a device: it has no tag, no part and no rating. A
//!   representation draws it ([`crate::Subject::Aid`]).
//! - A [`Aid::Continuation`] carries a net from one place to another by its name: every
//!   continuation with one name is one net. An unnamed one joins nothing. Its caption is the text
//!   beside a run of arrows.
//! - A [`Aid::Box`] marks a location (and function) on a page; the devices inside it take its
//!   values unless their own are fixed (§6.3). No box id is stored on a device.
//!
//! This module excludes how aids are drawn and edited (ecad-electrical, ecad-engine) and the
//! nets continuations join (connectivity).

use serde::{Deserialize, Serialize};

use crate::geometry::units::Celsius;
use crate::naming::id::{FunctionId, LocationId};
use crate::naming::text::Text;
use crate::objects::styles::StyleRef;

/// Most characters of a continuation name.
pub const MAX_NAME: usize = 16;
/// Most characters of a continuation caption.
pub const MAX_CAPTION: usize = 80;

/// A continuation's name (`24V`): the net it carries.
pub type ContinuationName = Text<MAX_NAME>;
/// A continuation's caption: one line beside its run of arrows.
pub type Caption = Text<MAX_CAPTION>;

/// Something drawn to help read a page, not a device (stored intent).
///
/// # Example
/// ```
/// use ecad_model::objects::aid::{Aid, ContinuationName};
/// let a = Aid::Continuation { name: Some(ContinuationName::new("24V").unwrap()), caption: None };
/// assert_eq!(serde_json::to_string(&a).unwrap(), r#"{"continuation":{"name":"24V"}}"#);
/// assert_eq!(serde_json::from_str::<Aid>(r#"{"box":{}}"#).unwrap(), Aid::Box(Default::default()));
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Aid {
    /// A continuation arrow.
    Continuation {
        /// Its name, when it has one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<ContinuationName>,
        /// Its caption, when it has one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        caption: Option<Caption>,
    },
    /// A location box.
    Box(LocationBox),
}

/// A location box's values (design 072 §6.2).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocationBox {
    /// The location it marks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<LocationId>,
    /// The function it marks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<FunctionId>,
    /// The temperature inside the panel it marks, for ratings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel_temperature: Option<Celsius>,
    /// Its own location style (brief 050a); `None`: the project's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<StyleRef>,
}

/// An aid's kind, without its data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AidKind {
    Continuation,
    Box,
}

impl Aid {
    /// Its kind.
    pub fn kind(&self) -> AidKind {
        match self {
            Aid::Continuation { .. } => AidKind::Continuation,
            Aid::Box(_) => AidKind::Box,
        }
    }

    /// Its continuation name, when it is a named continuation.
    pub fn continuation_name(&self) -> Option<&ContinuationName> {
        match self {
            Aid::Continuation { name, .. } => name.as_ref(),
            Aid::Box(_) => None,
        }
    }

    /// Its caption, when it is a continuation with one.
    pub fn continuation_caption(&self) -> Option<&Caption> {
        match self {
            Aid::Continuation { caption, .. } => caption.as_ref(),
            Aid::Box(_) => None,
        }
    }

    /// Its box values, when it is a location box.
    pub fn as_box(&self) -> Option<&LocationBox> {
        match self {
            Aid::Box(b) => Some(b),
            Aid::Continuation { .. } => None,
        }
    }

    /// Why it is not well formed, if it is not: a panel temperature that is not a number.
    pub fn problem(&self) -> Option<String> {
        match self {
            Aid::Continuation { .. } => None,
            Aid::Box(b) => b.panel_temperature.filter(|t| !t.0.is_finite()).map(|_| "the panel temperature is not a number".into()),
        }
    }
}
