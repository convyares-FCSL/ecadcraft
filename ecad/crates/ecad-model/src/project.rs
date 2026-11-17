//! The project's own data (design 072 §5.3): the location and function lists and the project's
//! default location and function.
//!
//! # Semantics
//! - The project holds one list of locations (`+CP1`) and one of functions (`=F1`). Each entry has
//!   a stable id, allocated when it is made and recorded in the op, a **code** unique (ignoring
//!   case) within its list and starting with its list's sign, an optional **tag** and any number
//!   of **descriptions** (description 1, description 2, …).
//! - Pages, the project and (lane B) devices refer to an entry by its id, so renaming a code is one
//!   edit of the list. An id with no entry (an entry a merge removed) shows `?`.
//! - The whole project changes through one op, [`crate::Op::ModifyProject`].
//!
//! This module excludes what a code means for a device (ecad-electrical's `locations` and
//! `designations`) and the commands.
#![deny(clippy::wildcard_enum_match_arm)]

use serde::{Deserialize, Serialize};

use crate::id::{FunctionId, LocationId};
use crate::text::Text;

/// Most characters of a list code (with its sign) and of a tag.
pub const MAX_CODE: usize = 16;
/// Most characters of one description of a list entry.
pub const MAX_LIST_DESCRIPTION: usize = 128;
/// Most descriptions one entry holds.
pub const MAX_DESCRIPTIONS: usize = 8;

/// A list code (`+CP1`, `=F1`) or tag.
pub type ListCode = Text<MAX_CODE>;
/// One description of a list entry.
pub type ListDescription = Text<MAX_LIST_DESCRIPTION>;

/// One entry of the location or function list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListEntry<I> {
    pub id: I,
    pub code: ListCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<ListCode>,
    /// Description 1, description 2, … in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub descriptions: Vec<ListDescription>,
}

impl<I> ListEntry<I> {
    /// An entry with `code` and nothing else.
    pub fn new(id: I, code: ListCode) -> Self {
        ListEntry { id, code, tag: None, descriptions: Vec::new() }
    }

    /// Description 1, if any.
    pub fn description(&self) -> Option<&ListDescription> {
        self.descriptions.first()
    }
}

/// A location (`+CP1`).
pub type Location = ListEntry<LocationId>;
/// A function (`=F1`).
pub type Function = ListEntry<FunctionId>;

/// The project's own data (design 072 §5.3). Every field is left out of records while empty.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    /// The location list, in the order entries were made.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<Location>,
    /// The function list, in the order entries were made.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<Function>,
    /// The location of every page that sets none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<LocationId>,
    /// The function of every page that sets none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<FunctionId>,
}

impl Project {
    /// Whether nothing is set.
    pub fn is_empty(&self) -> bool {
        *self == Project::default()
    }

    /// The location `id`, if the list has it.
    pub fn location(&self, id: LocationId) -> Option<&Location> {
        self.locations.iter().find(|e| e.id == id)
    }

    /// The function `id`, if the list has it.
    pub fn function(&self, id: FunctionId) -> Option<&Function> {
        self.functions.iter().find(|e| e.id == id)
    }

    /// Why the project is not well formed, if it is not: an id twice in a list, a code twice
    /// (ignoring case), a code without its list's sign (`+`, `=`), or too many descriptions.
    pub fn problem(&self) -> Option<String> {
        list_problem(&self.locations, '+', "location").or_else(|| list_problem(&self.functions, '=', "function"))
    }
}

fn list_problem<I: Copy + Ord + std::fmt::Display>(list: &[ListEntry<I>], sign: char, what: &str) -> Option<String> {
    let mut ids = std::collections::BTreeSet::new();
    let mut codes = std::collections::BTreeSet::new();
    for e in list {
        if !ids.insert(e.id) {
            return Some(format!("{what} {} is listed twice", e.id));
        }
        if !codes.insert(e.code.as_str().to_uppercase()) {
            return Some(format!("{what} code {} is listed twice", e.code));
        }
        if !e.code.as_str().starts_with(sign) || e.code.as_str().len() == sign.len_utf8() {
            return Some(format!("{what} code {} does not start with {sign} and a name", e.code));
        }
        if e.descriptions.len() > MAX_DESCRIPTIONS {
            return Some(format!("{what} {} has more than {MAX_DESCRIPTIONS} descriptions", e.code));
        }
    }
    None
}
