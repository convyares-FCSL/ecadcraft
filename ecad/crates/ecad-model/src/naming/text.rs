//! Validated text (design 072 §9.4): every text a person types is stored as [`Text<N>`] (one line)
//! or [`LongText<N>`] (line breaks allowed), never as a raw `String`.
//!
//! A value is trimmed, 1 to `N` characters, and holds no control, hidden or bidi character
//! ([`crate::naming::words::is_hidden`]). Deserialisation is strict: a file value that is not already in
//! that form is a read error, so the project opens read-only (design 072 §9.5). Commands build
//! values with `new`, which trims and refuses with [`TextError`]'s words.
//!
//! This module excludes what a text means (a tag's rule, a code's alphabet): those are checked by
//! the type that holds the text.
#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::naming::words::is_hidden;

/// Why a text was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TextError {
    #[error("the text is empty")]
    Empty,
    #[error("the text is longer than {max} characters")]
    TooLong { max: usize },
    /// A control, hidden or bidi character at character position `at` (0-based).
    #[error("the text holds a hidden character (U+{:04X}) at position {}", u32::from(*char), at + 1)]
    Hidden { at: usize, char: char },
    #[error("the text must be one line")]
    Lines,
}

/// `text` trimmed and checked: 1..=`max` characters, no hidden character, and line breaks (`\n`)
/// only when `lines` is set.
fn checked(text: &str, max: usize, lines: bool) -> Result<&str, TextError> {
    let t = text.trim();
    if t.is_empty() {
        return Err(TextError::Empty);
    }
    if !lines && t.contains(['\n', '\r']) {
        return Err(TextError::Lines);
    }
    if let Some((at, char)) = t.chars().enumerate().find(|(_, c)| is_hidden(*c) && !(lines && *c == '\n')) {
        return Err(TextError::Hidden { at, char });
    }
    if t.chars().count() > max {
        return Err(TextError::TooLong { max });
    }
    Ok(t)
}

/// Whether `text` is one line of 1 to `max` characters (after trimming) with no hidden character:
/// the rule of [`Text`] for a bound known only at run time.
///
/// # Errors
/// The [`TextError`] that `Text::<max>::new` would give.
pub fn check(text: &str, max: usize) -> Result<(), TextError> {
    checked(text, max, false).map(|_| ())
}

/// Whether `text` holds a hidden character (a control, invisible or bidi character): the check for
/// a value that is not yet a [`Text`].
pub fn has_hidden(text: &str) -> bool {
    text.chars().any(is_hidden)
}

/// `text` without its hidden characters.
pub fn strip_hidden(text: &str) -> String {
    text.chars().filter(|c| !is_hidden(*c)).collect()
}

/// `text` without its hidden characters, cut to `max` characters with `…` when cut: a value
/// echoed in a message.
pub fn clip(text: &str, max: usize) -> String {
    let mut visible = text.chars().filter(|c| !is_hidden(*c));
    let shown: String = visible.by_ref().take(max).collect();
    if visible.next().is_some() { format!("{shown}\u{2026}") } else { shown }
}

/// Declares one validated text type.
macro_rules! text_type {
    ($(#[$doc:meta])* $name:ident, $lines:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name<const N: usize>(Box<str>);

        impl<const N: usize> $name<N> {
            /// The most characters it holds.
            pub const MAX: usize = N;

            /// `text`, trimmed, if it is a valid value.
            ///
            /// # Errors
            /// [`TextError`] says why it was refused.
            pub fn new(text: &str) -> Result<Self, TextError> {
                checked(text, N, $lines).map(|t| Self(t.into()))
            }

            /// The text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl<const N: usize> fmt::Display for $name<N> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl<const N: usize> PartialEq<str> for $name<N> {
            fn eq(&self, other: &str) -> bool {
                &*self.0 == other
            }
        }

        impl<const N: usize> PartialEq<&str> for $name<N> {
            fn eq(&self, other: &&str) -> bool {
                &*self.0 == *other
            }
        }

        impl<const N: usize> AsRef<str> for $name<N> {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl<const N: usize> Serialize for $name<N> {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.0)
            }
        }

        /// Strict: the stored text must already be trimmed and valid.
        impl<'de, const N: usize> Deserialize<'de> for $name<N> {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = String::deserialize(d)?;
                match checked(&text, N, $lines) {
                    Ok(t) if t.len() == text.len() => Ok(Self(text.into_boxed_str())),
                    Ok(_) => Err(serde::de::Error::custom(format!("text `{}` has spaces at its ends", text.escape_debug()))),
                    Err(e) => Err(serde::de::Error::custom(format!("text `{}`: {e}", text.escape_debug()))),
                }
            }
        }

        impl<const N: usize> schemars::JsonSchema for $name<N> {
            fn inline_schema() -> bool {
                true
            }

            fn schema_name() -> std::borrow::Cow<'static, str> {
                format!("{}{}", stringify!($name), N).into()
            }

            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({"type": "string", "minLength": 1, "maxLength": N})
            }
        }
    };
}

text_type!(
    /// One line of text a person typed: a description, a part number, a code.
    ///
    /// # Example
    /// ```
    /// use crate::naming::text::{Text, TextError};
    /// assert_eq!(Text::<8>::new("  Motor ").map(|t| t.to_string()), Ok("Motor".into()));
    /// assert_eq!(Text::<3>::new("Motor"), Err(TextError::TooLong { max: 3 }));
    /// assert_eq!(Text::<8>::new("a\u{202E}b"), Err(TextError::Hidden { at: 1, char: '\u{202E}' }));
    /// ```
    Text,
    false
);

text_type!(
    /// Text a person typed that may span lines (`\n`): notes, captions.
    LongText,
    true
);
