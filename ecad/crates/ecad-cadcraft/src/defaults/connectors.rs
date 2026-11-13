//! Brief 044's defaults (connector devices, design 044 §10): how a half is named, the BOM
//! descriptions of each family's halves, and the first answers of the `connector` command. Owned by
//! lane S.
//!
//! # Semantics
//! - `connector.half.separator` and `connector.side.socket` / `connector.side.plug`: a half's tag
//!   is the connector's tag, the separator and the side name (`-X10.S`). Connector → project →
//!   built-in. Side names equal to each other (ignoring case) count as absent at both levels: the
//!   built-in `S` and `P` are used ([`crate::connectors::side_names`]).
//! - `connector.description`: the description a new connector gets.
//! - `connector.<family>.<side>.description`: a half's BOM description when it has none of its
//!   own; `{n}` is its pin count.
//! - `connector.maker.*`: the first answers of `connector`'s prompts and the JSON form's defaults.
//!
//! Families, sizes and side keys are catalogue code ([`crate::connectors`], [`crate::symbols`]).

use super::{Entry, Kind, Level, ProjectSource, Value};

const GROUP: &str = "Connectors";
const BRIEF: &str = "044";
/// A standard (reserved), the project and the connector may override.
const SPO: &[Level] = &[Level::Standard, Level::Project, Level::Object];
/// A standard (reserved) and the project may override.
const SP: &[Level] = &[Level::Standard, Level::Project];

/// 1–3 characters that are not letters, digits, spaces, commas or colons (`.`, `-`, `/`).
fn separator_ok(t: &str) -> bool {
    let n = t.chars().count();
    (1..=3).contains(&n) && !t.chars().any(|c| c.is_alphanumeric() || c.is_whitespace() || c == ':' || c == ',' || c.is_control())
}

/// 1–4 ASCII letters and digits.
fn side_ok(t: &str) -> bool {
    let n = t.chars().count();
    (1..=4).contains(&n) && t.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Up to 64 characters (checked by the kind) with at most one `{n}`.
fn description_ok(t: &str) -> bool {
    t.matches("{n}").count() <= 1
}

pub static SEPARATOR: Entry = Entry {
    key: "connector.half.separator",
    label: "Separator between a connector's tag and its half",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Parsed { max_chars: 3, parse: separator_ok },
    builtin: Value::Text("."),
    levels: SPO,
    project: ProjectSource::Property,
};

pub static SIDE_SOCKET: Entry = Entry {
    key: "connector.side.socket",
    label: "Name of the socket half",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Parsed { max_chars: 4, parse: side_ok },
    builtin: Value::Text("S"),
    levels: SPO,
    project: ProjectSource::Property,
};

pub static SIDE_PLUG: Entry = Entry {
    key: "connector.side.plug",
    label: "Name of the plug half",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Parsed { max_chars: 4, parse: side_ok },
    builtin: Value::Text("P"),
    levels: SPO,
    project: ProjectSource::Property,
};

pub static DESCRIPTION: Entry = Entry {
    key: "connector.description",
    label: "Description of a new connector",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Text { max_chars: 64 },
    builtin: Value::Text("Connector"),
    levels: SP,
    project: ProjectSource::Property,
};

macro_rules! descriptions {
    ($($name:ident = $family:literal, $side:literal, $builtin:literal;)*) => {
        $(
            pub static $name: Entry = Entry {
                key: concat!("connector.", $family, ".", $side, ".description"),
                label: concat!("BOM description: ", $builtin),
                group: GROUP,
                brief: BRIEF,
                kind: Kind::Parsed { max_chars: 64, parse: description_ok },
                builtin: Value::Text($builtin),
                levels: SP,
                project: ProjectSource::Property,
            };
        )*

        /// The half-description entry of a family (`plain`, `dsub`, `m12`) and side (`socket`,
        /// `plug`).
        pub fn description_entry(family: &str, side: &str) -> Option<&'static Entry> {
            match (family, side) {
                $(($family, $side) => Some(&$name),)*
                _ => None,
            }
        }
    };
}

descriptions! {
    DESC_PLAIN_S = "plain", "socket", "Socket, {n}-pin";
    DESC_PLAIN_P = "plain", "plug", "Plug, {n}-pin";
    DESC_DSUB_S = "dsub", "socket", "D-sub socket, {n}-way";
    DESC_DSUB_P = "dsub", "plug", "D-sub plug, {n}-way";
    DESC_M12_S = "m12", "socket", "M12 socket, {n}-pin";
    DESC_M12_P = "m12", "plug", "M12 plug, {n}-pin";
}

pub static MAKER_HALVES: Entry = Entry {
    key: "connector.maker.halves",
    label: "Connector: halves first offered",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Choice(&["socket", "plug", "both"]),
    builtin: Value::Choice("both"),
    levels: SP,
    project: ProjectSource::Property,
};

pub static MAKER_FAMILY: Entry = Entry {
    key: "connector.maker.family",
    label: "Connector: type first offered",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Choice(&["plain", "dsub", "m12", "rj45", "usba", "usbb"]),
    builtin: Value::Choice("plain"),
    levels: SP,
    project: ProjectSource::Property,
};

pub static MAKER_PINS: Entry = Entry {
    key: "connector.maker.pins",
    label: "Connector: pins first offered",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Number { min: 1.0, max: 64.0 },
    builtin: Value::Number(4.0),
    levels: SP,
    project: ProjectSource::Property,
};

pub static MAKER_NAMES: Entry = Entry {
    key: "connector.maker.names",
    label: "Connector: pin names first offered",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Choice(&["numbers", "letters", "list"]),
    builtin: Value::Choice("numbers"),
    levels: SP,
    project: ProjectSource::Property,
};

pub static MAKER_LAYOUT: Entry = Entry {
    key: "connector.maker.layout",
    label: "Connector: drawing first offered",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Choice(&["row", "pins"]),
    builtin: Value::Choice("row"),
    levels: SP,
    project: ProjectSource::Property,
};

/// Every entry of this file, in the order `defaults` lists them.
pub static ENTRIES: &[&Entry] = &[
    &SEPARATOR,
    &SIDE_SOCKET,
    &SIDE_PLUG,
    &DESCRIPTION,
    &DESC_PLAIN_S,
    &DESC_PLAIN_P,
    &DESC_DSUB_S,
    &DESC_DSUB_P,
    &DESC_M12_S,
    &DESC_M12_P,
    &MAKER_HALVES,
    &MAKER_FAMILY,
    &MAKER_PINS,
    &MAKER_NAMES,
    &MAKER_LAYOUT,
];

/// Whether `key` is a naming key a connector or the project may hold (separator, side names):
/// what the connector index's `inherits` map watches (design 044 §11.1).
pub fn is_naming_key(key: &str) -> bool {
    key == SEPARATOR.key || key == SIDE_SOCKET.key || key == SIDE_PLUG.key
}

/// The value `setprop` stores for one of this file's object-level keys on a connector, or `None`
/// when the key is not one of them.
///
/// # Errors
/// A value the entry does not accept, in words.
pub fn setprop_value(key: &str, value: Option<&str>) -> Option<Result<Option<String>, String>> {
    let e = ENTRIES.iter().find(|e| e.key == key && e.allows(Level::Object))?;
    Some(match value.map(str::trim) {
        None | Some("") => Ok(None),
        Some(v) => match e.parse(v) {
            Some(_) => Ok(Some(v.to_string())),
            None if e.key == SEPARATOR.key => {
                Err("A half separator is 1 to 3 characters that are not letters, digits, spaces, commas or colons (. or -).".into())
            }
            None => Err(format!("`{key}` is 1 to 4 letters or digits (S, P, F, M).")),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_parse_as_designed() {
        assert_eq!(SEPARATOR.parse("-"), Some(Value::Text("-")));
        for bad in ["", "ab", "1", " ", ":", "....", "a\u{7}"] {
            assert_eq!(SEPARATOR.parse(bad), None, "{bad:?}");
        }
        assert_eq!(SIDE_SOCKET.parse("F"), Some(Value::Text("F")));
        for bad in ["", "SOCKS", "S P", "S.", "Ü"] {
            assert_eq!(SIDE_SOCKET.parse(bad), None, "{bad:?}");
        }
        assert!(DESC_DSUB_S.parse("{n}{n}").is_none());
        assert!(description_entry("m12", "plug").is_some() && description_entry("rj45", "plug").is_none());
        assert_eq!(setprop_value("connector.side.socket", Some("F")), Some(Ok(Some("F".into()))));
        assert!(setprop_value("connector.side.socket", Some(&"x".repeat(10_000))).is_some_and(|r| r.is_err()));
        assert_eq!(setprop_value("connector.maker.pins", Some("4")), None, "project only");
    }
}
