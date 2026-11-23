//! Defaults for multi-level terminal blocks: the level separator, the level
//! names of each multi-level type and each type's BOM description.
//!
//! # Semantics
//! - `terminal_block.level.separator`: what goes between a position and a level name in a shown label
//!   (`1.L`). Strip → project → built-in `.`; the word `none` is no separator (`1L`), which is how
//!   `setprop … ""` on a strip or the project stores it.
//! - `terminal_block.<code>.levels`: the level names of a multi-level type, comma-separated, exactly the
//!   type's level count, each 1–6 characters. A value with the wrong count is absent.
//! - `terminal_block.<code>.description`: a type's BOM description, project → built-in (its name).
//!
//! - `terminal_block.<key>.prefix`: the tag prefix of each starter:
//!   the `connection_point` area, following the pattern of `symbol.<key>.prefix`, not its
//!   area). The project level is the project's `tag_prefixes` setting, keyed by starter key, as for
//!   the symbol entries. A strip's tag comes from the starter that made it; later levels join it and
//!   take no prefix of their own.
//!
//! Types, codes, level counts and bridges are catalogue code ([`crate::terminal_strips::terminal_blocks`]).

use ecad_model::ProjectSettings;

use crate::defaults::{Entry, Kind, Level, ProjectSource, Slot, Value};

use crate::CmdError;

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

/// Level names a link can never misread: exactly `n` names, each 1–6
/// characters with no `.`, `,` or space, all different (ignoring case), and a name made of digits
/// only is its own level number (`1,2,3` on a three-level terminal; never `2,1,3`). So in a link
/// `1.2` is always level 2, whichever names the type has.
fn names_ok(t: &str, n: usize) -> bool {
    let parts: Vec<&str> = t.split(',').map(str::trim).collect();
    let mut seen = std::collections::BTreeSet::new();
    parts.len() == n
        && parts.iter().enumerate().all(|(i, p)| {
            let digits = ecad_model::naming::words::is_digits(p);
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
    key: "terminal_block.level.separator",
    label: "Level separator in terminal labels",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Parsed { max_chars: 4, parse: separator_ok },
    slot: Slot::new(),
    levels: SPO,
    project: ProjectSource::Property,
};

/// A prefix as Tag Settings accepts it.
fn prefix_ok(t: &str) -> bool {
    crate::devices::valid_prefix(t).is_some()
}

macro_rules! prefixes {
    ($($name:ident = $var:ident $key:literal, $symbol:literal;)*) => {
        $(
            pub static $name: Entry = Entry {
                key: concat!("terminal_block.", $key, ".prefix"),
                label: concat!("Tag prefix: ", $symbol),
                group: "Tag prefixes",
                brief: BRIEF,
                kind: Kind::Parsed { max_chars: crate::devices::MAX_PREFIX_LETTERS + 1, parse: prefix_ok },
                slot: Slot::new(),
                levels: SP,
                project: ProjectSource::Settings({
                    fn read(s: &ProjectSettings) -> Option<Value<'_>> {
                        s.starter_prefix($key).map(|p| Value::Text(p.as_str()))
                    }
                    read
                }),
            };
        )*

        /// The prefix entries of this brief's starters.
        pub static PREFIXES: &[&Entry] = &[$(&$name),*];

        /// The prefix entry of this brief's starter `key`.
        pub fn prefix_entry(key: crate::symbols::StarterKey) -> Option<&'static Entry> {
            match key {
                $(crate::symbols::StarterKey::$var => Some(&$name),)*
                _ => None,
            }
        }
    };
}

prefixes! {
    TERM2_1 = Term2L1 "term2.1", "Two-level terminal, level 1";
    TERM2_2 = Term2L2 "term2.2", "Two-level terminal, level 2";
    TERM2 = Term2 "term2", "Two-level terminal";
    TERM2B_1 = Term2bL1 "term2b.1", "Two-level terminal, bridged, level 1";
    TERM2B_2 = Term2bL2 "term2b.2", "Two-level terminal, bridged, level 2";
    TERM2B = Term2b "term2b", "Two-level terminal, bridged";
    TERM3_1 = Term3L1 "term3.1", "Three-level terminal, level 1";
    TERM3_2 = Term3L2 "term3.2", "Three-level terminal, level 2";
    TERM3_3 = Term3L3 "term3.3", "Three-level terminal, level 3";
    TERM3 = Term3 "term3", "Three-level terminal";
    TERMLNPE_1 = TermlnpeL1 "termlnpe.1", "L/N/PE terminal, level L";
    TERMLNPE_2 = TermlnpeL2 "termlnpe.2", "L/N/PE terminal, level N";
    TERMLNPE_3 = TermlnpeL3 "termlnpe.3", "L/N/PE terminal, level PE";
    TERMLNPE = Termlnpe "termlnpe", "L/N/PE terminal";
    TERMSENSOR_1 = TermsensorL1 "termsensor.1", "Sensor terminal, level S";
    TERMSENSOR_2 = TermsensorL2 "termsensor.2", "Sensor terminal, level +";
    TERMSENSOR_3 = TermsensorL3 "termsensor.3", "Sensor terminal, level -";
    TERMSENSOR = Termsensor "termsensor", "Sensor terminal";
    TERMDISC = Termdisc "termdisc", "Disconnect terminal";
    TERMINALBLOCKFUSE = Terminalblockfuse "terminalblockfuse", "Fuse terminal (strip position)";
}

macro_rules! levels {
    ($($name:ident = $code:literal, $parse:ident, $label:literal;)*) => {
        $(
            pub static $name: Entry = Entry {
                key: concat!("terminal_block.", $code, ".levels"),
                label: $label,
                group: GROUP,
                brief: BRIEF,
                kind: Kind::Parsed { max_chars: 64, parse: $parse },
                slot: Slot::new(),
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
    LEVELS_X2 = "x2", names2_ok, "Level names: two-level terminal";
    LEVELS_X2B = "x2b", names2_ok, "Level names: two-level terminal, bridged";
    LEVELS_X3 = "x3", names3_ok, "Level names: three-level terminal";
    LEVELS_LNPE = "lnpe", names3_ok, "Level names: L/N/PE terminal";
    LEVELS_SEN = "sen", names3_ok, "Level names: sensor terminal";
}

macro_rules! descriptions {
    ($($name:ident = $code:literal, $builtin:literal;)*) => {
        $(
            pub static $name: Entry = Entry {
                key: concat!("terminal_block.", $code, ".description"),
                label: concat!("BOM description: ", $builtin),
                group: GROUP,
                brief: BRIEF,
                kind: Kind::Text { max_chars: 64 },
                slot: Slot::new(),
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
            &TERMDISC, &TERMINALBLOCKFUSE,
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
/// [`crate::terminal_strips`]'s update map watches on a strip and on the project.
pub fn is_label_key(key: &str) -> bool {
    key == SEPARATOR.key || (ecad_model::naming::keys::in_area(key, ecad_model::naming::keys::Area::TerminalBlock) && key.ends_with(".levels"))
}

/// The value `setprop` stores for `key` (`""` on the separator is `none`), or `None` when the key
/// is not this file's.
///
/// # Errors
/// A value the entry does not accept, in words.
pub fn setprop_value(key: &str, value: Option<&str>) -> Option<Result<Option<String>, CmdError>> {
    let e = crate::defaults::find(key).filter(|e| ecad_model::naming::keys::in_area(e.key, ecad_model::naming::keys::Area::TerminalBlock))?;
    Some(match value {
        None => Ok(None),
        Some(v) if v.trim().is_empty() && e.key == SEPARATOR.key => Ok(Some(NO_SEPARATOR.into())),
        Some(v) if v.trim().is_empty() => Ok(None),
        Some(v) => match e.parse(v) {
            Some(_) => Ok(Some(v.trim().to_string())),
            None if e.key == SEPARATOR.key => {
                Err(CmdError::refused("A level separator is 1 to 3 characters that are not letters, digits, spaces or commas, or empty for none."))
            }
            None if e.key.ends_with(".levels") => Err(CmdError::refused(format!(
                "`{key}` is the level names, comma-separated, one per level, each 1 to 6 characters without dots or spaces, all different; \
a name of digits only must be its level's number (so 1.2 in a link is always level 2)."
            ))),
            None => Err(CmdError::refused(format!("`{key}` is 1 to 64 characters, with no control characters."))),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal_strips::terminal_blocks::TYPES;

    #[test]
    fn every_type_has_its_entries() {
        for t in TYPES {
            assert!(description_entry(t.code).is_some(), "{}", t.code);
            assert_eq!(levels_entry(t.code).is_some(), t.multi(), "{}", t.code);
            if let Some(e) = levels_entry(t.code) {
                assert_eq!(e.builtin(), Value::Text(Box::leak(t.levels.join(",").into_boxed_str())));
            }
        }
        assert_eq!(setprop_value("terminal_block.level.separator", Some("")), Some(Ok(Some("none".into()))));
        assert!(setprop_value("terminal_block.lnpe.levels", Some("L,N")).is_some_and(|r| r.is_err()));
        assert!(setprop_value("terminal_block.lnpe.levels", Some(&"x".repeat(10_000))).is_some_and(|r| r.is_err()));
        assert_eq!(setprop_value("text.tag.height", Some("3")), None);
        // Names a link could misread are refused.
        for bad in ["2,1,3", "L,N,L", "L.1,N,PE", "L N,N,PE", "1,X,1"] {
            assert!(setprop_value("terminal_block.lnpe.levels", Some(bad)).is_some_and(|r| r.is_err()), "{bad}");
        }
        assert_eq!(setprop_value("terminal_block.lnpe.levels", Some("1,N,3")), Some(Ok(Some("1,N,3".into()))));
    }
}
