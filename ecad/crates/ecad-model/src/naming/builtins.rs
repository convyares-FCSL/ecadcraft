//! The built-in project settings this crate computes with, as data (brief 066c, design 066 §2.2;
//! overseer ruling 2026-11-16, option D).
//!
//! # Semantics
//! - The values live in `ecad-model/defaults/settings.toml`, embedded at build time ([`SETTINGS`]).
//!   A project setting left unchosen (`None` in [`crate::ProjectSettings`]) reads its value from
//!   [`builtins`]; a chosen one is kept, even when it equals the built-in.
//! - **One source.** The defaults registry in `ecad-electrical` loads the same text for these keys'
//!   built-ins, so the registry and this crate never disagree (a test checks it).
//! - Parsed once, on first use. A value missing or out of range in the file (not reachable in a
//!   tested build) takes its safe value: the low end of its range or its first word; never a crash.
//!
//! Excluded: kinds, ranges and labels (the registry's), the standard level (this crate reads the
//! project's own value or the built-in, as before brief 066), and reading any file from disk.

use std::sync::OnceLock;

use crate::document::doc::ReferenceStyle;
use crate::document::sheet::SheetSpec;
use crate::geometry::colour::ColourWords;
use crate::naming::words::Word as _;

/// The embedded data file.
pub const SETTINGS: &str = include_str!("../../defaults/settings.toml");
/// Its name, for messages.
pub const FILE: &str = "settings.toml";

/// Registry key of the tag padding.
pub const TAG_PADDING: &str = "tags.padding";
/// Registry key of the wire number digits.
pub const WIRE_DIGITS: &str = "wiring.digits";
/// Registry key of the colour style.
pub const COLOUR_STYLE: &str = "wiring.colour_style";
/// Registry key of the page number step.
pub const PAGE_NUMBER_STEP: &str = "page.number_step";
/// Registry key of the project sheet.
pub const SHEET: &str = "page.sheet";
/// Registry key of the reference style.
pub const REFERENCE_STYLE: &str = "print.reference_style";

/// Every key [`SETTINGS`] holds. These live only here (owner, 2026-11-16: what the lower crate
/// needs goes in the lower crate); the registry's entries for them read [`get`].
pub const KEYS: &[&str] =
    &[TAG_PADDING, crate::naming::tag::DASH_KEY, crate::naming::tag::CASE_KEY, WIRE_DIGITS, COLOUR_STYLE, PAGE_NUMBER_STEP, SHEET, REFERENCE_STYLE];

/// A built-in value of [`KEYS`], as the registry reads it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BuiltinValue {
    Number(f64),
    Word(&'static str),
}

/// The built-in value of `key`, if [`SETTINGS`] holds it. The sheet is structured
/// ([`Builtins::sheet`]) and has no single value here.
pub fn get(key: &str) -> Option<BuiltinValue> {
    let b = builtins();
    Some(match key {
        TAG_PADDING => BuiltinValue::Number(f64::from(b.tag_padding)),
        WIRE_DIGITS => BuiltinValue::Number(f64::from(b.wire_digits)),
        PAGE_NUMBER_STEP => BuiltinValue::Number(f64::from(b.page_number_step)),
        COLOUR_STYLE => BuiltinValue::Word(b.colour_style.word()),
        REFERENCE_STYLE => BuiltinValue::Word(b.reference_style.word()),
        k if k == crate::naming::tag::DASH_KEY => BuiltinValue::Word(b.tag_dash),
        k if k == crate::naming::tag::CASE_KEY => BuiltinValue::Word(b.tag_case),
        _ => return None,
    })
}

/// The most a page number may step by (a guard, not a preference).
pub const MAX_PAGE_NUMBER_STEP: u32 = 1000;

/// The built-in project settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Builtins {
    pub tag_padding: u8,
    /// A word of [`crate::naming::tag::DASH_WORDS`].
    pub tag_dash: &'static str,
    /// A word of [`crate::naming::tag::CASE_WORDS`].
    pub tag_case: &'static str,
    pub wire_digits: u8,
    pub colour_style: ColourWords,
    pub page_number_step: u32,
    pub sheet: SheetSpec,
    pub reference_style: ReferenceStyle,
}

/// The built-in project settings, read from [`SETTINGS`] once.
pub fn builtins() -> &'static Builtins {
    static B: OnceLock<Builtins> = OnceLock::new();
    B.get_or_init(|| load(SETTINGS))
}

/// Read the built-ins from `text`; each value missing or out of range takes its safe value.
pub fn load(text: &str) -> Builtins {
    let table = toml::from_str::<toml::Table>(text).ok();
    let values = table.as_ref().and_then(|t| t.get("values")).and_then(toml::Value::as_table);
    let get = |key: &str| values.and_then(|v| v.get(key));
    let int = |key: &str, lo: i64, hi: i64| get(key).and_then(toml::Value::as_integer).filter(|n| (lo..=hi).contains(n));
    let word =
        |key: &str, words: &'static [&'static str]| get(key).and_then(toml::Value::as_str).and_then(|s| words.iter().copied().find(|w| *w == s));
    let small = |key: &str, max: u8| int(key, 1, i64::from(max)).and_then(|n| u8::try_from(n).ok()).unwrap_or(1);
    Builtins {
        tag_padding: small(TAG_PADDING, crate::naming::tag::MAX_PADDING),
        tag_dash: word(crate::naming::tag::DASH_KEY, crate::naming::tag::DASH_WORDS).unwrap_or(crate::naming::tag::DASH_WORDS[0]),
        tag_case: word(crate::naming::tag::CASE_KEY, crate::naming::tag::CASE_WORDS).unwrap_or(crate::naming::tag::CASE_WORDS[0]),
        wire_digits: small(WIRE_DIGITS, crate::connectivity::wiring::MAX_DIGITS),
        colour_style: word(COLOUR_STYLE, crate::connectivity::wiring::COLOUR_STYLE_WORDS)
            .and_then(<ColourWords as crate::naming::words::Word>::parse)
            .unwrap_or_default(),
        page_number_step: int(PAGE_NUMBER_STEP, 1, i64::from(MAX_PAGE_NUMBER_STEP)).and_then(|n| u32::try_from(n).ok()).unwrap_or(1),
        sheet: get(SHEET).and_then(toml::Value::as_str).and_then(|s| SheetSpec::parse(s).ok()).unwrap_or_default(),
        reference_style: word(REFERENCE_STYLE, ReferenceStyle::WORDS).and_then(ReferenceStyle::parse).unwrap_or(ReferenceStyle::PageZone),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_ins_are_today_s_values() {
        let b = builtins();
        assert_eq!((b.tag_padding, b.tag_dash, b.tag_case, b.wire_digits), (2, "yes", "upper", 2));
        assert_eq!((b.colour_style, b.page_number_step, b.reference_style), (ColourWords::Code2, 1, ReferenceStyle::PageZone));
        assert_eq!(b.sheet, SheetSpec::default());
    }

    #[test]
    fn the_file_holds_exactly_its_keys() {
        let table: toml::Table = toml::from_str(SETTINGS).unwrap_or_default();
        let mut got: Vec<&str> =
            table.get("values").and_then(toml::Value::as_table).map(|t| t.keys().map(String::as_str).collect()).unwrap_or_default();
        let mut want = KEYS.to_vec();
        got.sort_unstable();
        want.sort_unstable();
        assert_eq!(got, want);
    }

    #[test]
    fn a_damaged_file_gives_safe_values_never_a_crash() {
        for text in ["", "not toml [[[", "[values]\n\"tags.padding\" = 99\n\"page.sheet\" = \"B9\"\n\"tags.dash\" = 7\n"] {
            let b = load(text);
            assert_eq!((b.tag_padding, b.tag_dash, b.page_number_step), (1, "yes", 1), "{text:?}");
            assert!(b.sheet.check().is_ok());
        }
    }
}
