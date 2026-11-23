//! Connectivity on the page: conductor labels,
//! continuation texts, drawing routes (lines, joins and T's) and placing continuations.
//!
//! # Semantics
//! - Labels and continuation texts are derived on every projection of a page, never stored
//!   (like tag texts): only the page on screen is projected.
//! - A person draws routes ([`ecad_model::Route`]); each is drawn by one polyline item whose
//!   vertices are the route's points. A T is a route end on another route (`RouteEnd::On`):
//!   there are no junction components. The wires come from the commit's rewrite pass
//!   (`ecad_model::connectivity::wire_order::rewrite`); the planners here write routes.
//! - A continuation is a tagless component made from the starter definition `continuation`,
//!   with one connection point keyed `1`.
//! - Labels use the project's label height and colour style (wiring settings) and are placed so
//!   none touches another label or another wire (`labels_on_page_with`).
//!
//! # Update map of the labels
//! | Change | Labels |
//! | --- | --- |
//! | `AddRoute`, `ModifyRoute`, `RemoveRoute` (incl. a merge's) | that page's labels, when it is projected |
//! | `CreateWire`, `DeleteWire` (nets join or split) | with the `net` rule, the pages where those nets have routes: their routes' pages are touched, marked stale, the page on screen re-projects |
//! | a wire number, potential, cross-section or colour | the pages of the conductors it was written on (the numbering pass's included) |
//! | wiring settings (label rule, shortest labelled wire, colour style, label height, potentials) | every page, when it is projected next |
//! | the wire-label text style or show setting | the pages of the conductors concerned |
//! | a cable switched to fan or traditional view | that cable's members' labels hide or show on the page on screen (display only, nothing recorded) |
//! | anything else | nothing |
//!
//! # Update map of the looks-connected marks
//! Route ops on P and cable reference ops on P change P's marks,
//! computed when P is projected ([`looks_connected_on_page`]); anything else changes nothing.
//!
//! This module reads the document and plans ops; it commits nothing itself.

pub mod commands;
pub mod joining;
pub mod label_rule;
pub mod labels;
pub mod placing;
pub mod routes;
pub mod words;
use self::{joining::*, label_rule::*, placing::*, routes::*};
use crate::CmdError;
use ecad_model::Noun;
use std::collections::BTreeMap;

use std::collections::BTreeSet;

use ecad_model::{
    Aid, AidId, ConductorId, ConnectionPoint, ConnectionPointId, DefinitionId, Document, End, Form, IdAllocator, Item, ItemId, ItemKind, NetId, Op,
    PageId, Placement, Primitive, Pt, Representation, RepresentationId, Route, RouteEnd, RouteId, Style, Subject, SymbolInstance, Wire, WireId,
};

use crate::devices::texts::{DerivedText, TextOf};
use crate::styles::text_style::fonts::TextMetrics;
use crate::symbols::{self, Starter};

/// The reserved layer of conductor labels (derived, plotted), like `ECAD-CABLES`.
pub const LABEL_LAYER: &str = "ECAD-WIRE-LABELS";
/// The smallest clearance from the line to the label, millimetres; it grows with the label height
/// ([`gap`]).
pub const LABEL_GAP: f64 = 0.8;
/// The least space kept between two labels, and between a label and another wire, millimetres.
pub(crate) const LABEL_CLEARANCE: f64 = 0.6;

/// The layer of a two-colour wire's dashes (derived, plotted only in colour).
pub const DASH_LAYER: &str = "ECAD-WIRE-COLOUR";

/// The clearance between a label of height `h` and its line.
pub fn gap(h: f64) -> f64 {
    (0.5 * h).max(LABEL_GAP)
}
/// `ecad.wire-label`: ACI 7, ByLayer on the label layer.
pub const LABEL_ACI: u8 = 7;
/// How near a wire a point must be to lie on it (model units, mm).
pub const ON_WIRE: f64 = 0.5;
/// Most characters of a continuation name.
pub const MAX_NAME_CHARS: usize = 16;

/// Why a conductor label alone cannot be edited.
pub const LABEL_DERIVED: &str = "The wire label is derived; change the number with Fix Wire Number, or the wire's cross-section and colour.";

/// Why a T's dot alone cannot be edited.
pub const TEE_DERIVED: &str = "A T's dot is drawn from its lines; move or erase the line instead.";

/// The radii (mm) of a T's dot: concentric circles, so it reads as filled at any lineweight
/// setting (the old junction glyph's look).
pub const TEE_DOT_RADII: [f64; 3] = [0.5, 0.33, 0.17];

/// The continuation name rule, in words.
pub(crate) const NAME_RULE: &str = "A continuation name is 1 to 16 letters, digits or + - / . _";

/// The starter of a continuation.
pub fn continuation_starter() -> &'static Starter {
    symbols::catalogue::starter(symbols::StarterKey::Continuation)
}

/// A continuation named `name` as typed, with `caption`, both checked.
///
/// # Errors
/// [`NAME_RULE`], or the caption rule ([`crate::continuations::CAPTION_RULE`]).
pub fn continuation_aid(name: &str, caption: Option<&str>) -> Result<Aid, CmdError> {
    let name = ecad_model::objects::aid::ContinuationName::new(&valid_name(name)?).map_err(|_| CmdError::refused(NAME_RULE))?;
    let caption = match crate::continuations::caption_text(caption)? {
        Some(c) => Some(ecad_model::objects::aid::Caption::new(&c).map_err(|_| CmdError::refused(crate::continuations::CAPTION_RULE))?),
        None => None,
    };
    Ok(Aid::Continuation { name: Some(name), caption })
}

/// A continuation name as typed, checked: trimmed, 1-16 letters, digits or `+ - / . _`.
///
/// # Errors
/// [`NAME_RULE`].
pub fn valid_name(text: &str) -> Result<String, CmdError> {
    let t = text.trim();
    let ok =
        !t.is_empty() && t.chars().count() <= MAX_NAME_CHARS && t.chars().all(|c| c.is_alphanumeric() || matches!(c, '+' | '-' | '/' | '.' | '_'));
    if ok { Ok(t.to_string()) } else { Err(CmdError::refused(NAME_RULE)) }
}
