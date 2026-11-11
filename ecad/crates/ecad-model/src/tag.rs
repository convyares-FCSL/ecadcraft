//! Device tags (brief 018): what makes two tags the same, and how a typed tag is written.
//!
//! A component's tag is its stored `tag` property. By the concept spec a tag **is** an identity:
//! the same tag anywhere means the same device. Two tags are the same when their [`key`]s are
//! equal: case is ignored, a leading `-` is optional, and in the usual form (letters, then a
//! number) leading zeros of the number are ignored, so `-K1`, `k01` and `-K001` are one tag.
//!
//! IEC 81346 is the default, never the law: any letters are accepted (`-SR01`, `-TB01`, `N01`),
//! and a tag that does not have the usual form is kept as typed (in upper case) and compared as a
//! whole.
//!
//! This module holds no index and reads no document; [`crate::Document::components_tagged`] finds
//! the components holding a tag through the document's index.

/// The property key a component's tag is stored under.
pub const TAG_KEY: &str = "tag";

/// Most characters in a tag.
pub const MAX_TAG_CHARS: usize = 40;

/// Most digits in a tag's number (larger numbers are compared as text).
pub const MAX_TAG_DIGITS: usize = 9;

/// The highest number a tag counts with ([`MAX_TAG_DIGITS`] nines).
pub const MAX_TAG_NUMBER: u64 = 999_999_999;

/// Most digits a project may pad tag numbers to.
pub const MAX_PADDING: u8 = 6;

/// The property key of a device's or cable's fixed flag (brief 026, FP1): `"yes"`, absent when not
/// fixed. Renumber and Finalise never change a fixed tag.
pub const FIXED_KEY: &str = "tag_fixed";

/// The project property turning provisional tags on (brief 026, FP2): `"yes"`, absent = off.
pub const PROVISIONAL_SETTING: &str = "numbering.provisional";

/// The project property prefix of a high-water reset (brief 026, FP4): `numbering.high.<P>` =
/// the highest number of prefix `P` in use after a renumber. [`note_ops`] reads it as "the mark of
/// `P` is now this", so new tags follow the renumbered set.
pub const HIGH_RESET_PREFIX: &str = "numbering.high.";

/// Refusal of a tag that cannot be stored.
pub const BAD_TAG: &str = "A tag is 1 to 40 printable characters, for example -K01, -SR01 or -TB01.";

/// The usual form of a tag: its letter prefix (upper case, without `-`) and its number.
///
/// # Semantics
/// After trimming and dropping one leading `-`: one or more ASCII letters, then 1 to
/// [`MAX_TAG_DIGITS`] ASCII digits, nothing else. `None` for anything else.
///
/// # Example
/// ```
/// assert_eq!(ecad_model::tag::parse("-k01"), Some(("K".to_string(), 1)));
/// assert_eq!(ecad_model::tag::parse("TB12"), Some(("TB".to_string(), 12)));
/// assert_eq!(ecad_model::tag::parse("-K1A"), None);
/// ```
pub fn parse(tag: &str) -> Option<(String, u64)> {
    let t = tag.trim();
    let t = t.strip_prefix('-').unwrap_or(t);
    let split = t.find(|c: char| !c.is_ascii_alphabetic())?;
    let (letters, digits) = t.split_at(split);
    if letters.is_empty() || digits.is_empty() || digits.len() > MAX_TAG_DIGITS {
        return None;
    }
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = digits.parse::<u64>().ok()?;
    Some((letters.to_ascii_uppercase(), n))
}

/// The provisional form of a tag (brief 026, FP3): its prefix (upper case, without `-`) and its
/// provisional number, for `-K?3`; `None` for anything else. `-K?` (no number) is `Some(("K", 0))`:
/// the request for the next provisional number, never a stored tag.
///
/// # Example
/// ```
/// assert_eq!(ecad_model::tag::provisional("-k?3"), Some(("K".to_string(), 3)));
/// assert_eq!(ecad_model::tag::provisional("-W?"), Some(("W".to_string(), 0)));
/// assert_eq!(ecad_model::tag::provisional("-K03"), None);
/// ```
pub fn provisional(tag: &str) -> Option<(String, u64)> {
    let t = tag.trim();
    let t = t.strip_prefix('-').unwrap_or(t);
    let (letters, digits) = t.split_once('?')?;
    if letters.is_empty() || !letters.bytes().all(|b| b.is_ascii_alphabetic()) || digits.len() > MAX_TAG_DIGITS {
        return None;
    }
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = if digits.is_empty() { 0 } else { digits.parse::<u64>().ok()? };
    Some((letters.to_ascii_uppercase(), n))
}

/// The written provisional tag `-K?3` (never padded).
pub fn format_provisional(prefix: &str, n: u64) -> String {
    format!("-{}?{n}", prefix.trim_start_matches('-').to_ascii_uppercase())
}

/// Whether `tag` is a stored provisional tag (`-K?3`, number at least 1).
pub fn is_provisional(tag: &str) -> bool {
    provisional(tag).is_some_and(|(_, n)| n > 0)
}

/// The letter prefix of a tag in the usual or the provisional form.
pub fn prefix_of(tag: &str) -> Option<String> {
    parse(tag).or_else(|| provisional(tag)).map(|(p, _)| p)
}

/// The highest number each tag prefix has had in a history (brief 018: the next automatic number
/// is the highest + 1, so a deleted tag is never given again). Version 2 snapshots carry it
/// (brief 021, the `model:meta` chunk), so a project opened from a snapshot keeps it.
pub type HighWater = std::collections::BTreeMap<String, u64>;

/// Count tag `t` in `high`.
pub fn note(high: &mut HighWater, t: &str) {
    if let Some((prefix, n)) = parse(t) {
        let e = high.entry(prefix).or_insert(0);
        *e = (*e).max(n);
    }
}

/// Count every tag `ops` write or remove (undone ones included), in order.
///
/// # Semantics
/// Device and cable tags share one mark per prefix (brief 026, Q7): `CreateComponent`,
/// `DeleteComponent`, `CreateCable`, `DeleteCable` and any `SetProperty` of `tag` count (FP7). A
/// `SetProperty` of project property `numbering.high.<P>` to a number sets the mark of `P` to
/// it (FP4: overwrite, not max), so new tags follow a renumbered set; a later tag write raises it
/// again as usual.
pub fn note_ops(high: &mut HighWater, ops: &[crate::Op]) {
    for op in ops {
        match op {
            crate::Op::CreateComponent { component, .. } | crate::Op::DeleteComponent { component, .. } => {
                if let Some(t) = component.properties.get(TAG_KEY) {
                    note(high, t);
                }
            }
            crate::Op::CreateCable { cable, .. } | crate::Op::DeleteCable { cable, .. } => {
                if let Some(t) = cable.properties.get(TAG_KEY) {
                    note(high, t);
                }
            }
            crate::Op::SetProperty { object: crate::ObjectRef::Project, key, after: Some(v), .. } if key.starts_with(HIGH_RESET_PREFIX) => {
                let prefix = key[HIGH_RESET_PREFIX.len()..].to_ascii_uppercase();
                if let Ok(n) = v.trim().parse::<u64>()
                    && !prefix.is_empty()
                    && prefix.bytes().all(|b| b.is_ascii_alphabetic())
                {
                    high.insert(prefix, n.min(MAX_TAG_NUMBER));
                }
            }
            crate::Op::SetProperty { key, before, after, .. } if key == TAG_KEY => {
                for t in before.iter().chain(after) {
                    note(high, t);
                }
            }
            _ => {}
        }
    }
}

/// The identity of a tag: equal keys are the same tag.
///
/// # Example
/// ```
/// use ecad_model::tag::key;
/// assert_eq!(key("-K1"), key("k01"));
/// assert_ne!(key("-K1"), key("-K10"));
/// ```
pub fn key(tag: &str) -> String {
    match parse(tag) {
        Some((prefix, n)) => format!("{prefix}{n}"),
        None => {
            let t = tag.trim();
            t.strip_prefix('-').unwrap_or(t).to_uppercase()
        }
    }
}

/// The written form of `prefix` and `n`: `-`, the prefix in upper case, the number padded with
/// zeros to `padding` digits.
///
/// # Example
/// ```
/// assert_eq!(ecad_model::tag::format("K", 1, 2), "-K01");
/// assert_eq!(ecad_model::tag::format("k", 123, 2), "-K123");
/// ```
pub fn format(prefix: &str, n: u64, padding: u8) -> String {
    let width = usize::from(padding.min(MAX_PADDING));
    format!("-{}{n:0width$}", prefix.trim_start_matches('-').to_ascii_uppercase())
}

/// A typed tag as it is stored.
///
/// # Semantics
/// Trimmed and in upper case. In the usual form ([`parse`]) it is written by [`format`] with the
/// project's padding (`k5` → `-K05` with padding 2). Anything else is kept as typed, with `-`
/// added when it starts with a letter.
///
/// # Errors
/// [`BAD_TAG`] when it is empty, longer than [`MAX_TAG_CHARS`] characters, holds a control
/// character, or is nothing but `-` (its [`key`] would be empty or `-`).
pub fn normalise(text: &str, padding: u8) -> Result<String, String> {
    let t = text.trim();
    let n = t.chars().count();
    if !(1..=MAX_TAG_CHARS).contains(&n) || t.chars().any(char::is_control) || t.chars().all(|c| c == '-' || c.is_whitespace()) {
        return Err(BAD_TAG.into());
    }
    if let Some((prefix, number)) = parse(t) {
        return Ok(format(&prefix, number, padding));
    }
    if let Some((prefix, number)) = provisional(t) {
        return Ok(if number == 0 { format!("-{prefix}?") } else { format_provisional(&prefix, number) });
    }
    let upper = t.to_uppercase();
    if upper.starts_with(|c: char| c.is_alphabetic()) { Ok(format!("-{upper}")) } else { Ok(upper) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_tag_table() {
        let same = [("-K1", "k01"), ("-K01", "K1"), ("-TB01", "tb1"), ("N01", "-N1"), ("-K 1", "-k 1")];
        for (a, b) in same {
            assert_eq!(key(a), key(b), "{a} and {b}");
        }
        let different = [("-K1", "-K10"), ("-K1", "-Q1"), ("-K1", "-KA1"), ("-K1A", "-K1")];
        for (a, b) in different {
            assert_ne!(key(a), key(b), "{a} and {b}");
        }
    }

    #[test]
    fn normalise_table() {
        let rows = [
            ("k5", 2, "-K05"),
            ("-K1", 1, "-K1"),
            ("-k123", 2, "-K123"),
            ("sr1", 2, "-SR01"),
            ("  tb01 ", 3, "-TB001"),
            ("-K1A", 2, "-K1A"),
            ("=PWR", 2, "=PWR"),
            ("main switch", 2, "-MAIN SWITCH"),
        ];
        for (input, pad, want) in rows {
            assert_eq!(normalise(input, pad).as_deref(), Ok(want), "{input}");
        }
        for bad in ["", "   ", "a\u{7}", &"K".repeat(41), "-", "--", " - - "] {
            assert!(normalise(bad, 2).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn provisional_table() {
        let rows = [
            ("-K?3", Some(("K", 3))),
            ("k?03", Some(("K", 3))),
            ("-W?", Some(("W", 0))),
            ("-K3", None),
            ("-?3", None),
            ("-K?3A", None),
            ("-K??3", None),
        ];
        for (t, want) in rows {
            assert_eq!(provisional(t), want.map(|(p, n)| (p.to_string(), n)), "{t}");
        }
        assert_eq!(normalise("k?03", 2).as_deref(), Ok("-K?3"));
        assert_eq!(normalise("-k?", 2).as_deref(), Ok("-K?"));
        assert_eq!(key("-K?3"), key("k?3"));
        assert_ne!(key("-K?3"), key("-K3"));
        assert!(is_provisional("-K?1") && !is_provisional("-K?") && !is_provisional("-K1"));
    }

    #[test]
    fn high_reset_overwrites_then_tags_raise() {
        use crate::{ObjectRef, Op};
        let set = |k: &str, v: &str| Op::SetProperty { object: ObjectRef::Project, key: k.into(), before: None, after: Some(v.into()) };
        let tag = |v: &str| Op::SetProperty { object: ObjectRef::Project, key: TAG_KEY.into(), before: None, after: Some(v.into()) };
        let mut high = HighWater::new();
        note_ops(&mut high, &[tag("-K300"), set("numbering.high.K", "40")]);
        assert_eq!(high.get("K"), Some(&40));
        note_ops(&mut high, &[set("numbering.high.K", "x"), set("numbering.high.", "3"), set("numbering.high.K1", "3")]);
        assert_eq!(high.get("K"), Some(&40));
        note_ops(&mut high, &[tag("-K41")]);
        assert_eq!(high.get("K"), Some(&41));
    }

    #[test]
    fn hostile_numbers_are_text() {
        let long = format!("-K{}", "9".repeat(20));
        assert_eq!(parse(&long), None);
        assert_eq!(key(&long), long.trim_start_matches('-'));
        assert_eq!(format("K", u64::MAX, 255).len(), 2 + u64::MAX.to_string().len());
    }
}
