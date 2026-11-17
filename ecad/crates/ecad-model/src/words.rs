//! The readers of the few word shapes every stored text uses (design 072 §9.1): a yes/no value and
//! a whole number. Every copy of these rules goes through here, so a file reads the same in every
//! crate.
//!
//! This module holds only the readers. What a word means for one setting (its choice list, its
//! default) lives with that setting.
#![deny(clippy::wildcard_enum_match_arm)]

/// The words read as yes, in lowercase. [`flag_word`] writes the first.
pub const YES_WORDS: [&str; 4] = ["yes", "true", "1", "on"];
/// The words read as no, in lowercase. [`flag_word`] writes the first.
pub const NO_WORDS: [&str; 4] = ["no", "false", "0", "off"];

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
