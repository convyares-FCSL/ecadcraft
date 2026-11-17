//! The readers of the few word shapes every stored text uses (design 072 §9.1): a yes/no value and
//! a whole number, and the characters no typed text may hold. Every copy of these rules goes through here, so a file reads the same in every
//! crate.
//!
//! It also holds [`Word`] and [`crate::word_enum!`], through which every enum stored as a word
//! gets its `word()`, `parse()` and serde forms from one attribute (strum).
//!
//! This module holds only the readers. What a word means for one setting (its choice list, its
//! default) lives with that setting.
#![deny(clippy::wildcard_enum_match_arm)]

/// The words read as yes, in lowercase. [`flag_word`] writes the first.
pub const YES_WORDS: [&str; 4] = ["yes", "true", "1", "on"];
/// The words read as no, in lowercase. [`flag_word`] writes the first.
pub const NO_WORDS: [&str; 4] = ["no", "false", "0", "off"];
/// The two words a yes/no value is stored as, yes first: the choices a person picks from.
pub const FLAG_WORDS: &[&str] = &[YES_WORDS[0], NO_WORDS[0]];

/// The most digits [`uint`] and [`uint_canonical`] read: enough for any `u32`.
pub const MAX_UINT_DIGITS: usize = 10;

/// A yes/no value: the one boolean rule (design 072 Part I).
///
/// # Semantics
/// Surrounding whitespace is ignored and case does not matter. `yes`, `true`, `1`, `on` are yes;
/// `no`, `false`, `0`, `off` are no; anything else (`y`, `n`, an empty text) is `None`.
///
/// # Example
/// ```
/// use ecad_model::words::flag;
/// assert_eq!(flag(" Yes "), Some(true));
/// assert_eq!(flag("off"), Some(false));
/// assert_eq!(flag("y"), None);
/// ```
pub fn flag(text: &str) -> Option<bool> {
    let t = text.trim();
    if YES_WORDS.iter().any(|w| t.eq_ignore_ascii_case(w)) {
        Some(true)
    } else if NO_WORDS.iter().any(|w| t.eq_ignore_ascii_case(w)) {
        Some(false)
    } else {
        None
    }
}

/// The word a yes/no value is written as: `yes` or `no`.
pub fn flag_word(value: bool) -> &'static str {
    if value { YES_WORDS[0] } else { NO_WORDS[0] }
}

/// Whether `text` is one or more ASCII digits and nothing else.
pub fn is_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

/// A whole number of at most `max`, as a person may type it.
///
/// # Semantics
/// Surrounding whitespace is ignored; the rest must be 1–[`MAX_UINT_DIGITS`] ASCII digits
/// (leading zeros allowed: `007` is 7). No sign, no separators. `None` past `max`.
///
/// # Example
/// ```
/// use ecad_model::words::uint;
/// assert_eq!(uint(" 07 ", 99), Some(7));
/// assert_eq!(uint("100", 99), None);
/// assert_eq!(uint("+1", 99), None);
/// ```
pub fn uint(text: &str, max: u32) -> Option<u32> {
    digits_value(text.trim()).filter(|n| *n <= max)
}

/// A whole number of at most `max` in its one written form: digits with no leading zero (`0` itself
/// is allowed) and no whitespace. For values stored as text whose spelling must be unique, so two
/// texts never name the same number.
///
/// # Example
/// ```
/// use ecad_model::words::uint_canonical;
/// assert_eq!(uint_canonical("12", 99), Some(12));
/// assert_eq!(uint_canonical("0", 99), Some(0));
/// assert_eq!(uint_canonical("012", 99), None);
/// assert_eq!(uint_canonical(" 12", 99), None);
/// ```
pub fn uint_canonical(text: &str, max: u32) -> Option<u32> {
    if text.len() > 1 && text.starts_with('0') {
        return None;
    }
    digits_value(text).filter(|n| *n <= max)
}

fn digits_value(t: &str) -> Option<u32> {
    if t.len() > MAX_UINT_DIGITS || !is_digits(t) {
        return None;
    }
    t.parse::<u32>().ok()
}

/// The invisible format characters a typed text never holds besides control characters (design 072
/// §9.4): soft hyphen, the Arabic letter mark, the Mongolian vowel separator, zero-width and joiner
/// characters and the directional marks, the bidi embeddings and overrides, word joiners and
/// invisible operators, the bidi isolates and deprecated format characters, the byte-order mark and
/// the interlinear annotation marks. Inclusive ranges of code points.
pub const HIDDEN_RANGES: &[(u32, u32)] = &[
    (0x00AD, 0x00AD),
    (0x061C, 0x061C),
    (0x180E, 0x180E),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x2064),
    (0x2066, 0x206F),
    (0xFEFF, 0xFEFF),
    (0xFFF9, 0xFFFB),
];

/// Whether `c` is a character no text a person typed may hold: a control character or one of
/// [`HIDDEN_RANGES`]. The one hidden-character rule (G14: no other copy).
pub fn is_hidden(c: char) -> bool {
    c.is_control() || HIDDEN_RANGES.iter().any(|(lo, hi)| (*lo..=*hi).contains(&u32::from(c)))
}

crate::word_enum! {
    /// A connector half (brief 044, design 072 §2.2, §2.6): the socket (female contacts) or the
    /// plug (male contacts).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Side {
        Socket,
        Plug,
    }
}

impl Side {
    /// The other half.
    pub fn other(self) -> Side {
        match self {
            Side::Socket => Side::Plug,
            Side::Plug => Side::Socket,
        }
    }
}

/// An enum whose values are stored as words (design 072 §9.1): `word()` writes one, `parse()`
/// reads one or fails. Declared with [`crate::word_enum!`], never by hand, so a variant's word is
/// written in exactly one place (its strum attribute, or its name in snake case).
///
/// # Semantics
/// [`Word::parse`] ignores surrounding whitespace and ASCII case and returns `None` for any other
/// text: there is no fallback variant. Serde writes [`Word::word`] and refuses an unknown word with
/// an error naming it and the allowed words.
pub trait Word: Copy + Sized + 'static {
    /// Every value, in declaration order.
    const ALL: &'static [Self];
    /// Every value's word, in the order of [`Word::ALL`].
    const WORDS: &'static [&'static str];

    /// The value's word.
    fn word(self) -> &'static str;

    /// The value a word names.
    fn parse(text: &str) -> Option<Self>;
}

/// The message of an unknown word: the word and the words allowed.
pub fn unknown_word_message(word: &str, what: &str, allowed: &[&str]) -> String {
    format!("unknown {what} word `{word}` (allowed: {})", allowed.join(", "))
}

#[doc(hidden)]
#[cfg(feature = "schema")]
pub use schemars as __schemars;
#[doc(hidden)]
pub use serde as __serde;

/// Declares an enum whose values are stored as words, and implements [`Word`], `Serialize`,
/// `Deserialize` and (with ecad-model's `schema` feature) `JsonSchema` for it.
///
/// The words are snake case of the variant names unless a variant says otherwise with
/// `#[strum(serialize = "…")]`. The crate using it depends on `strum`.
///
/// # Example
/// ```
/// use ecad_model::words::Word;
/// ecad_model::word_enum! {
///     #[derive(Debug, Clone, Copy, PartialEq, Eq)]
///     pub enum Corner {
///         TopLeft,
///         #[strum(serialize = "br")]
///         BottomRight,
///     }
/// }
/// assert_eq!(Corner::TopLeft.word(), "top_left");
/// assert_eq!(Corner::parse(" BR "), Some(Corner::BottomRight));
/// assert_eq!(Corner::parse("bottom_right"), None);
/// assert_eq!(Corner::WORDS, &["top_left", "br"]);
/// ```
#[macro_export]
macro_rules! word_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(::strum::IntoStaticStr, ::strum::EnumString, ::strum::VariantArray, ::strum::VariantNames)]
        #[strum(serialize_all = "snake_case", ascii_case_insensitive)]
        $vis enum $name {
            $( $(#[$vmeta])* $variant ),*
        }

        impl $crate::words::Word for $name {
            const ALL: &'static [Self] = <Self as ::strum::VariantArray>::VARIANTS;
            const WORDS: &'static [&'static str] = <Self as ::strum::VariantNames>::VARIANTS;

            fn word(self) -> &'static str {
                self.into()
            }

            fn parse(text: &str) -> Option<Self> {
                <Self as ::core::str::FromStr>::from_str(text.trim()).ok()
            }
        }

        impl $crate::words::__serde::Serialize for $name {
            fn serialize<S: $crate::words::__serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str($crate::words::Word::word(*self))
            }
        }

        impl<'de> $crate::words::__serde::Deserialize<'de> for $name {
            fn deserialize<D: $crate::words::__serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = <::std::string::String as $crate::words::__serde::Deserialize>::deserialize(d)?;
                <Self as $crate::words::Word>::parse(&text).ok_or_else(|| {
                    <D::Error as $crate::words::__serde::de::Error>::custom($crate::words::unknown_word_message(
                        &text,
                        stringify!($name),
                        <Self as $crate::words::Word>::WORDS,
                    ))
                })
            }
        }

        $crate::__word_schema!($name);
    };
}

/// The JSON Schema of a [`Word`] enum: a string, one of its words.
#[doc(hidden)]
#[cfg(feature = "schema")]
#[macro_export]
macro_rules! __word_schema {
    ($name:ident) => {
        impl $crate::words::__schemars::JsonSchema for $name {
            fn inline_schema() -> bool {
                true
            }

            fn schema_name() -> ::std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }

            fn json_schema(_: &mut $crate::words::__schemars::SchemaGenerator) -> $crate::words::__schemars::Schema {
                $crate::words::__schemars::json_schema!({
                    "type": "string",
                    "enum": <$name as $crate::words::Word>::WORDS,
                })
            }
        }
    };
}

/// Without ecad-model's `schema` feature a [`Word`] enum has no schema.
#[doc(hidden)]
#[cfg(not(feature = "schema"))]
#[macro_export]
macro_rules! __word_schema {
    ($name:ident) => {};
}
