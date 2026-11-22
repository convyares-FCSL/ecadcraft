//! The document's objects and their relationships (proposal §3 and §4).
//!
//! Stored intent (components, terminals, conductors, cables, definitions) and drawing content
//! (pages, layers, items, representations) live here. Derived results do not (see [`crate::connectivity::derive`]).

pub mod cable;
pub(crate) mod definition;
pub(crate) mod device;
pub(crate) mod document;
pub(crate) mod drawing;
pub(crate) mod settings;

use std::collections::BTreeMap;

/// Optional, free-form properties. A missing key is `Unset` (principle P7); an empty string is a
/// confirmed empty value.
pub type Properties = BTreeMap<String, String>;
