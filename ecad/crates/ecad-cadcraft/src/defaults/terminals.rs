//! Brief 036's defaults (multi-level terminals, design 036 §12): the level separator, the level
//! names of each multi-level type and each type's BOM description. Owned by lane D.
//!
//! # Semantics
//! - `terminal.level.separator`: what goes between a position and a level name in a shown label
//!   (`1.L`). Strip → project → built-in `.`; the word `none` is no separator (`1L`), which is how
//!   `setprop … ""` on a strip or the project stores it.
//! - `terminal.<code>.levels`: the level names of a multi-level type, comma-separated, exactly the
//!   type's level count, each 1–6 characters. A value with the wrong count is absent.
//! - `terminal.<code>.description`: a type's BOM description, project → built-in (its name).
//!
//! - `terminal.<key>.prefix`: the tag prefix of each of this brief's starters (overseer ruling,
//!   2026-11-13: the `terminal` area, following the pattern of 034's `symbol.<key>.prefix`, not its
//!   area). The project level is the project's `tag_prefixes` setting, keyed by starter key, as for
//!   034's entries. A strip's tag comes from the starter that made it; later levels join it and
//!   take no prefix of their own.
//!
//! Types, codes, level counts and bridges are catalogue code ([`crate::terminal_types`]).

use ecad_model::ProjectSettings;

use super::{Entry, Kind, Level, ProjectSource, Value};

const GROUP: &str = "Terminals";
const BRIEF: &str = "036";
/// A standard (reserved), the project and the strip may override.
const SPO: &[Level] = &[Level::Standard, Level::Project, Level::Object];
/// A standard (reserved) and the project may override.
const SP: &[Level] = &[Level::Standard, Level::Project];

/// The stored word for "no separator".
pub const NO_SEPARATOR: &str = "none";

fn separator_ok(t: &str) -> bool {
    t.eq_ignore_ascii_case(NO_SEPARATOR) || (t.chars().count() <= 3 && !t.chars().any(|c| c.is_whitespace() || c.is_ascii_alphanumeric() || c == ','))
}

/// Level names a link can never misread (overseer ruling, M1 review): exactly `n` names, each 1–6
/// characters with no `.`, `,` or space, all different (ignoring case), and a name made of digits
/// only is its own level number (`1,2,3` on a three-level terminal; never `2,1,3`). So in a link
/// `1.2` is always level 2, whichever names the type has.
fn names_ok(t: &str, n: usize) -> bool {
    let parts: Vec<&str> = t.split(',').map(str::trim).collect();
    let mut seen = std::collections::BTreeSet::new();
    parts.len() == n
        && parts.iter().enumerate().all(|(i, p)| {
            let digits = p.bytes().all(|b| b.is_ascii_digit());
            !p.is_empty()
                && p.chars().count() <= 6
                && !p.chars().any(|c| c == '.' || c == ',' || c.is_whitespace() || c.is_control())
                && (!digits || p.parse::<usize>().ok() == Some(i + 1))
                && seen.insert(p.to_lowercase())
        })
}

fn names2_ok(t: &str) -> bool {
    names_ok(t, 2)
}

fn names3_ok(t: &str) -> bool {
    names_ok(t, 3)
}

pub static SEPARATOR: Entry = Entry {
    key: "terminal.level.separator",
    label: "Level separator in terminal labels",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Parsed { max_chars: 4, parse: separator_ok },
    builtin: Value::Text("."),
    levels: SPO,
    project: ProjectSource::Property,
};

/// A prefix as Tag Settings accepts it.
fn prefix_ok(t: &str) -> bool {
    crate::devices::valid_prefix(t).is_some()
}

macro_rules! prefixes {
    ($($name:ident = $key:literal, $prefix:literal, $symbol:literal;)*) => {
        $(
            pub static $name: Entry = Entry {
                key: concat!("terminal.", $key, ".prefix"),
                label: concat!("Tag prefix: ", $symbol),
                group: "Tag prefixes",
                brief: BRIEF,
                kind: Kind::Parsed { max_chars: crate::devices::MAX_PREFIX_LETTERS + 1, parse: prefix_ok },
                builtin: Value::Text($prefix),
                levels: SP,
                project: ProjectSource::Settings({
                    fn read(s: &ProjectSettings) -> Option<Value<'_>> {
                        s.tag_prefixes.get($key).map(|p| Value::Text(p.as_str()))
                    }
                    read
                }),
            };
        )*

        /// The prefix entries of this brief's starters.
        pub static PREFIXES: &[&Entry] = &[$(&$name),*];

        /// The prefix entry of this brief's starter `key`.
        pub fn prefix_entry(key: &str) -> Option<&'static Entry> {
            match key {
                $($key => Some(&$name),)*
                _ => None,
            }
        }
    };
}

prefixes! {
    TERM2_1 = "term2.1", "X", "Two-level terminal, level 1";
    TERM2_2 = "term2.2", "X", "Two-level terminal, level 2";
    TERM2 = "term2", "X", "Two-level terminal";
    TERM2B_1 = "term2b.1", "X", "Two-level terminal, bridged, level 1";
    TERM2B_2 = "term2b.2", "X", "Two-level terminal, bridged, level 2";
    TERM2B = "term2b", "X", "Two-level terminal, bridged";
    TERM3_1 = "term3.1", "X", "Three-level terminal, level 1";
    TERM3_2 = "term3.2", "X", "Three-level terminal, level 2";
    TERM3_3 = "term3.3", "X", "Three-level terminal, level 3";
    TERM3 = "term3", "X", "Three-level terminal";
    TERMLNPE_1 = "termlnpe.1", "X", "L/N/PE terminal, level L";
    TERMLNPE_2 = "termlnpe.2", "X", "L/N/PE terminal, level N";
    TERMLNPE_3 = "termlnpe.3", "X", "L/N/PE terminal, level PE";
    TERMLNPE = "termlnpe", "X", "L/N/PE terminal";
    TERMSENSOR_1 = "termsensor.1", "X", "Sensor terminal, level S";
    TERMSENSOR_2 = "termsensor.2", "X", "Sensor terminal, level +";
    TERMSENSOR_3 = "termsensor.3", "X", "Sensor terminal, level -";
    TERMSENSOR = "termsensor", "X", "Sensor terminal";
    TERMDISC = "termdisc", "X", "Disconnect terminal";
    TERMFUSE = "termfuse", "X", "Fuse terminal (strip position)";
}

macro_rules! levels {
    ($($name:ident = $code:literal, $parse:ident, $builtin:literal, $label:literal;)*) => {
        $(
            pub static $name: Entry = Entry {
                key: concat!("terminal.", $code, ".levels"),
                label: $label,
                group: GROUP,
                brief: BRIEF,
                kind: Kind::Parsed { max_chars: 64, parse: $parse },
                builtin: Value::Text($builtin),
                levels: SPO,
                project: ProjectSource::Property,
            };
        )*

        /// The level-names entry of a multi-level type, by registry code.
        pub fn levels_entry(code: &str) -> Option<&'static Entry> {
            match code {
                $($code => Some(&$name),)*
                _ => None,
            }
        }
    };
}

levels! {
    LEVELS_X2 = "x2", names2_ok, "1,2", "Level names: two-level terminal";
    LEVELS_X2B = "x2b", names2_ok, "1,2", "Level names: two-level terminal, bridged";
    LEVELS_X3 = "x3", names3_ok, "1,2,3", "Level names: three-level terminal";
    LEVELS_LNPE = "lnpe", names3_ok, "L,N,PE", "Level names: L/N/PE terminal";
    LEVELS_SEN = "sen", names3_ok, "S,+,-", "Level names: sensor terminal";
}

macro_rules! descriptions {
    ($($name:ident = $code:literal, $builtin:literal;)*) => {
        $(
            pub static $name: Entry = Entry {
                key: concat!("terminal.", $code, ".description"),
                label: concat!("BOM description: ", $builtin),
                group: GROUP,
                brief: BRIEF,
                kind: Kind::Text { max_chars: 64 },
                builtin: Value::Text($builtin),
                levels: SP,
                project: ProjectSource::Property,
            };
        )*

        /// The description entry of a type, by registry code.
        pub fn description_entry(code: &str) -> Option<&'static Entry> {
            match code {
                $($code => Some(&$name),)*
                _ => None,
            }
        }

        /// Every entry of this file, in the order `defaults` lists them.
        pub static ENTRIES: &[&Entry] = &[
            &SEPARATOR, &LEVELS_X2, &LEVELS_X2B, &LEVELS_X3, &LEVELS_LNPE, &LEVELS_SEN, $(&$name,)*
            &TERM2_1, &TERM2_2, &TERM2, &TERM2B_1, &TERM2B_2, &TERM2B, &TERM3_1, &TERM3_2, &TERM3_3, &TERM3,
            &TERMLNPE_1, &TERMLNPE_2, &TERMLNPE_3, &TERMLNPE, &TERMSENSOR_1, &TERMSENSOR_2, &TERMSENSOR_3, &TERMSENSOR,
            &TERMDISC, &TERMFUSE,
        ];
    };
}

descriptions! {
    DESC_FT = "ft", "Feed-through terminal";
    DESC_PE = "pe", "PE terminal";
    DESC_X2 = "x2", "Two-level terminal";
    DESC_X2B = "x2b", "Two-level terminal, bridged";
    DESC_X3 = "x3", "Three-level terminal";
    DESC_LNPE = "lnpe", "L/N/PE terminal";
    DESC_SEN = "sen", "Sensor terminal";
    DESC_DIS = "dis", "Disconnect terminal";
    DESC_FUSE = "fuse", "Fuse terminal";
}

/// Whether `key` is one of this file's keys a strip may hold (the separator, level names): what
/// [`crate::strips`]'s update map watches on a strip and on the project.
pub fn is_label_key(key: &str) -> bool {
    key == SEPARATOR.key || (key.starts_with("terminal.") && key.ends_with(".levels"))
}

/// The value `setprop` stores for `key` (`""` on the separator is `none`), or `None` when the key
/// is not this file's.
///
/// # Errors
/// A value the entry does not accept, in words.
pub fn setprop_value(key: &str, value: Option<&str>) -> Option<Result<Option<String>, String>> {
    let e = super::find(key).filter(|e| e.key.starts_with("terminal."))?;
    Some(match value {
        None => Ok(None),
        Some(v) if v.trim().is_empty() && e.key == SEPARATOR.key => Ok(Some(NO_SEPARATOR.into())),
        Some(v) if v.trim().is_empty() => Ok(None),
        Some(v) => match e.parse(v) {
            Some(_) => Ok(Some(v.trim().to_string())),
            None if e.key == SEPARATOR.key => {
                Err("A level separator is 1 to 3 characters that are not letters, digits, spaces or commas, or empty for none.".into())
            }
            None if e.key.ends_with(".levels") => Err(format!(
                "`{key}` is the level names, comma-separated, one per level, each 1 to 6 characters without dots or spaces, all different; \
a name of digits only must be its level's number (so 1.2 in a link is always level 2)."
            )),
            None => Err(format!("`{key}` is 1 to 64 characters, with no control characters.")),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal_types::TYPES;

    #[test]
    fn every_type_has_its_entries() {
        for t in TYPES {
            assert!(description_entry(t.code).is_some(), "{}", t.code);
            assert_eq!(levels_entry(t.code).is_some(), t.multi(), "{}", t.code);
            if let Some(e) = levels_entry(t.code) {
                assert_eq!(e.builtin, Value::Text(Box::leak(t.levels.join(",").into_boxed_str())));
            }
        }
        assert_eq!(setprop_value("terminal.level.separator", Some("")), Some(Ok(Some("none".into()))));
        assert!(setprop_value("terminal.lnpe.levels", Some("L,N")).is_some_and(|r| r.is_err()));
        assert!(setprop_value("terminal.lnpe.levels", Some(&"x".repeat(10_000))).is_some_and(|r| r.is_err()));
        assert_eq!(setprop_value("text.tag.height", Some("3")), None);
        // Names a link could misread are refused (overseer ruling, M1 review).
        for bad in ["2,1,3", "L,N,L", "L.1,N,PE", "L N,N,PE", "1,X,1"] {
            assert!(setprop_value("terminal.lnpe.levels", Some(bad)).is_some_and(|r| r.is_err()), "{bad}");
        }
        assert_eq!(setprop_value("terminal.lnpe.levels", Some("1,N,3")), Some(Ok(Some("1,N,3".into()))));
    }
}
