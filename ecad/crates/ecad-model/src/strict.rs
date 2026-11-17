//! Strict reading (design 072 §9.4, F11): what makes a project open read-only because of a word.
//!
//! A typed field with an unknown word fails its record's decode and is reported as damage. A word
//! inside a value that stays text (a defaults choice, a `tag_prefixes` key) and a property that
//! became a struct field are found after the project is read, by the electrical rules' words
//! check; each one found is an [`UnknownWord`]. The project then opens read-only and its file is
//! never changed.
//!
//! This module holds only the record of what was found. The check itself needs the defaults
//! registry and the symbol list, so it lives in ecad-electrical (`words_check`).
#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;

use crate::id::{CableId, ComponentId, PageId, WireId};

/// Where a stored word was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WordPlace {
    /// The project's own properties.
    Project,
    /// The project's settings (`tag_prefixes`).
    Settings,
    /// The project's standard's values.
    Standard,
    Page(PageId),
    Device(ComponentId),
    Wire(WireId),
    Cable(CableId),
}

impl fmt::Display for WordPlace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WordPlace::Project => f.write_str("the project's properties"),
            WordPlace::Settings => f.write_str("the project's settings"),
            WordPlace::Standard => f.write_str("the project's standard"),
            WordPlace::Page(id) => write!(f, "page {}", id.hex()),
            WordPlace::Device(id) => write!(f, "device {}", id.hex()),
            WordPlace::Wire(id) => write!(f, "wire {}", id.hex()),
            WordPlace::Cable(id) => write!(f, "cable {}", id.hex()),
        }
    }
}

/// One word this build does not read, or one property that is now a struct field.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnknownWord {
    /// `word` is not one of the words `setting` allows.
    Value {
        place: WordPlace,
        /// The setting's key, as stored (`text.tag.show`, or `text.<rep>.tag.show` for a symbol's
        /// own value), or the settings field (`tag_prefixes`).
        setting: String,
        word: String,
        /// The words the setting allows.
        allowed: Vec<&'static str>,
    },
    /// A property bag holds `key`, which became a struct field ([`crate::keys::RESERVED`]).
    ReservedKey { place: WordPlace, key: String },
}

impl UnknownWord {
    /// Where it was found.
    pub fn place(&self) -> WordPlace {
        match self {
            UnknownWord::Value { place, .. } | UnknownWord::ReservedKey { place, .. } => *place,
        }
    }
}

impl fmt::Display for UnknownWord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnknownWord::Value { place, setting, word, allowed } if allowed.is_empty() => {
                write!(f, "unknown word `{word}` in setting `{setting}` of {place}")
            }
            UnknownWord::Value { place, setting, word, allowed } => {
                write!(f, "unknown word `{word}` in setting `{setting}` of {place} (allowed: {})", allowed.join(", "))
            }
            UnknownWord::ReservedKey { place, key } => {
                write!(f, "property `{key}` of {place} is no longer a property; this build stores it as a field")
            }
        }
    }
}
