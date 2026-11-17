//! Tag prefixes as a project stores them (design 072 F5): the prefix itself ([`TagPrefix`]) and
//! the symbol it is for ([`PrefixOf`]), a built-in starter or a project symbol, said in the data
//! rather than worked out by looking the name up.
//!
//! This module checks the form only. Whether a starter word names a symbol of the catalogue is
//! the electrical crate's (it holds the catalogue), checked when a project is opened.
#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Most letters in a tag prefix.
pub const MAX_PREFIX_LETTERS: usize = 10;

/// Most characters of a symbol's key (a starter's word or a project symbol's key, design 048 §2.4).
pub const MAX_SYMBOL_KEY: usize = 16;

/// A tag prefix (IEC 81346): 1 to [`MAX_PREFIX_LETTERS`] ASCII letters, upper case, without `-`.
///
/// # Example
/// ```
/// use ecad_model::prefix::TagPrefix;
/// assert_eq!(TagPrefix::typed(" -ka ").map(|p| p.to_string()), Some("KA".to_string()));
/// assert!(TagPrefix::typed("K1").is_none());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TagPrefix(Box<str>);

impl TagPrefix {
    /// A prefix as a person types it: trimmed, an optional leading `-`, any case.
    pub fn typed(text: &str) -> Option<TagPrefix> {
        let t = text.trim();
        let t = t.strip_prefix('-').unwrap_or(t);
        TagPrefix::stored(&t.to_ascii_uppercase())
    }

    /// A prefix exactly as stored: 1 to [`MAX_PREFIX_LETTERS`] upper-case ASCII letters.
    pub fn stored(text: &str) -> Option<TagPrefix> {
        ((1..=MAX_PREFIX_LETTERS).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_uppercase())).then(|| TagPrefix(text.into()))
    }

    /// The letters.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TagPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for TagPrefix {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

/// Strict: a file holds the stored form only.
impl<'de> Deserialize<'de> for TagPrefix {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        TagPrefix::stored(&text).ok_or_else(|| {
            serde::de::Error::custom(format!("tag prefix `{}` is not 1 to {MAX_PREFIX_LETTERS} upper-case letters", crate::text::clip(&text, 24)))
        })
    }
}

/// The symbol a project's tag prefix is for, written `starter:<word>` or `project:<key>`.
///
/// # Semantics
/// A starter's word is 1 to [`MAX_SYMBOL_KEY`] of `a-z`, `0-9` and `.` (`coil`, `term2.1`); a
/// project symbol's key follows design 048 §2.4: 1 to [`MAX_SYMBOL_KEY`] ASCII letters, digits,
/// `-` and `_`, starting with a letter or digit.
///
/// # Example
/// ```
/// use ecad_model::prefix::PrefixOf;
/// let p = PrefixOf::parse("project:valve-x").unwrap();
/// assert_eq!(p, PrefixOf::Project("valve-x".into()));
/// assert_eq!(PrefixOf::Starter("coil".into()).to_string(), "starter:coil");
/// assert!(PrefixOf::parse("cable").is_none());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrefixOf {
    /// A built-in starter symbol, by its word.
    Starter(Box<str>),
    /// A symbol made in the project, by its key.
    Project(Box<str>),
}

const STARTER_TAG: &str = "starter:";
const PROJECT_TAG: &str = "project:";

impl PrefixOf {
    /// Whether `word` has a starter word's form.
    pub fn starter_form(word: &str) -> bool {
        (1..=MAX_SYMBOL_KEY).contains(&word.len()) && word.bytes().all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'.'))
    }

    /// Whether `key` has a project symbol key's form (design 048 §2.4).
    pub fn project_form(key: &str) -> bool {
        (1..=MAX_SYMBOL_KEY).contains(&key.len())
            && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            && key.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric())
    }

    /// The written form read back; `None` for anything else.
    pub fn parse(text: &str) -> Option<PrefixOf> {
        if let Some(w) = text.strip_prefix(STARTER_TAG) {
            return PrefixOf::starter_form(w).then(|| PrefixOf::Starter(w.into()));
        }
        let k = text.strip_prefix(PROJECT_TAG)?;
        PrefixOf::project_form(k).then(|| PrefixOf::Project(k.into()))
    }
}

impl fmt::Display for PrefixOf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrefixOf::Starter(w) => write!(f, "{STARTER_TAG}{w}"),
            PrefixOf::Project(k) => write!(f, "{PROJECT_TAG}{k}"),
        }
    }
}

impl Serialize for PrefixOf {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

/// Strict: `starter:<word>` or `project:<key>` in their forms; anything else is a read error.
impl<'de> Deserialize<'de> for PrefixOf {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        PrefixOf::parse(&text)
            .ok_or_else(|| serde::de::Error::custom(format!("prefix key `{}` is not starter:<word> or project:<key>", crate::text::clip(&text, 40))))
    }
}

#[cfg(feature = "schema")]
mod schema {
    use std::borrow::Cow;

    use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};

    impl JsonSchema for super::TagPrefix {
        fn schema_name() -> Cow<'static, str> {
            "TagPrefix".into()
        }
        fn json_schema(_: &mut SchemaGenerator) -> Schema {
            json_schema!({"type": "string", "pattern": "^[A-Z]{1,10}$"})
        }
    }

    impl JsonSchema for super::PrefixOf {
        fn schema_name() -> Cow<'static, str> {
            "PrefixOf".into()
        }
        fn json_schema(_: &mut SchemaGenerator) -> Schema {
            json_schema!({"type": "string", "pattern": "^(starter:[a-z0-9.]{1,16}|project:[A-Za-z0-9][A-Za-z0-9_-]{0,15})$"})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_and_keys_read_only_their_stored_forms() {
        for (text, ok) in [("KA", true), ("ka", false), ("-K", false), ("K1", false), ("", false), ("ABCDEFGHIJK", false)] {
            assert_eq!(serde_json::from_value::<TagPrefix>(serde_json::json!(text)).is_ok(), ok, "{text}");
        }
        for (text, ok) in [
            ("starter:coil", true),
            ("starter:term2.1", true),
            ("project:Valve_2", true),
            ("cable", false),
            ("coil", false),
            ("starter:Coil", false),
            ("project:-x", false),
            ("project:", false),
            ("starter:abcdefghijklmnopq", false),
        ] {
            let read = serde_json::from_value::<PrefixOf>(serde_json::json!(text));
            assert_eq!(read.is_ok(), ok, "{text}");
            if let Ok(p) = read {
                assert_eq!(serde_json::to_value(&p).ok(), Some(serde_json::json!(text)));
            }
        }
    }
}
