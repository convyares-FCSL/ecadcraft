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
//! Types, codes, level counts and bridges are catalogue code ([`crate::terminal_types`]).

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

fn names_ok(t: &str, n: usize) -> bool {
    let parts: Vec<&str> = t.split(',').map(str::trim).collect();
    parts.len() == n && parts.iter().all(|p| !p.is_empty() && p.chars().count() <= 6)
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
        pub static ENTRIES: &[&Entry] = &[&SEPARATOR, &LEVELS_X2, &LEVELS_X2B, &LEVELS_X3, &LEVELS_LNPE, &LEVELS_SEN, $(&$name),*];
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
            None if e.key.ends_with(".levels") => Err(format!("`{key}` is the level names, comma-separated, one per level, each 1 to 6 characters.")),
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
    }
}
