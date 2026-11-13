//! Lane S's defaults (034, component library). Owned by lane S: append `pub static` entries and list them in
//! [`ENTRIES`]. Lane T created this file empty (design 031 §3.5) and does not edit it again.
//!
//! # Semantics
//! One entry per tagged symbol: its built-in tag prefix, `symbol.<key>.prefix` (design 034 §2.7,
//! overseer rulings 2026-11-12). The project level is the project's `tag_prefixes` setting, keyed
//! by symbol key as Tag Settings writes it: one project path, never a project property too. This
//! table is the only place a symbol's built-in prefix is written;
//! [`crate::symbols::Starter::prefix`] reads it. Ratings have no default (ruling: nothing is
//! registered until a family needs a real value). Labels, quiet points and text anchors are
//! catalogue code in [`crate::symbols`], not entries.

use ecad_model::{Document, ProjectSettings};

use super::{Entry, Kind, Level, ProjectSource, Scope, Value};

const GROUP: &str = "Tag prefixes";
const BRIEF: &str = "034";
/// A standard (reserved) and the project may override a prefix.
const SP: &[Level] = &[Level::Standard, Level::Project];

/// A prefix as Tag Settings accepts it ([`crate::devices::valid_prefix`]): 1 to
/// [`crate::devices::MAX_PREFIX_LETTERS`] letters, with or without a leading `-`.
fn prefix_ok(t: &str) -> bool {
    crate::devices::valid_prefix(t).is_some()
}

const PREFIX: Kind = Kind::Parsed { max_chars: crate::devices::MAX_PREFIX_LETTERS + 1, parse: prefix_ok };

/// One prefix entry per symbol key: `NAME = "key", "built-in prefix", "symbol name";`. Each entry
/// reads its project level from `settings.tag_prefixes[key]`.
macro_rules! prefixes {
    ($($name:ident = $key:literal, $prefix:literal, $symbol:literal;)*) => {
        $(
            pub static $name: Entry = Entry {
                key: concat!("symbol.", $key, ".prefix"),
                label: concat!("Tag prefix: ", $symbol),
                group: GROUP,
                brief: BRIEF,
                kind: PREFIX,
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

        /// Every entry of this file, in the order `defaults` lists them.
        pub static ENTRIES: &[&Entry] = &[$(&$name),*];

        /// The prefix entry of symbol `key` (its catalogue key).
        pub fn prefix_entry(key: &str) -> Option<&'static Entry> {
            match key {
                $($key => Some(&$name),)*
                // Brief 036: the multi-level terminal starters register theirs as `terminal.<key>.prefix`.
                _ => super::terminals::prefix_entry(key),
            }
        }
    };
}

prefixes! {
    COIL = "coil", "K", "Contactor / relay coil";
    NO = "no", "K", "NO contact";
    NC = "nc", "K", "NC contact";
    POWER3 = "power3", "K", "Power contacts, 3-pole";
    MCB1 = "mcb1", "Q", "MCB 1-pole";
    MCB2 = "mcb2", "Q", "MCB 2-pole";
    FUSE = "fuse", "F", "Fuse";
    PSU = "psu", "G", "Power supply";
    PBNO = "pbno", "S", "Push button, NO";
    PBNC = "pbnc", "S", "Push button, NC";
    SEL = "sel", "S", "Selector switch, 2-position";
    LAMP = "lamp", "H", "Indicator lamp";
    MOTOR3 = "motor3", "M", "Motor, 3-phase";
    MOTOR1 = "motor1", "M", "Motor, single-phase";
    TRAFO = "trafo", "T", "Transformer";
    TERMINAL = "terminal", "X", "Terminal";
    TERMINALPE = "terminalpe", "X", "PE terminal";
    DI = "di", "A", "PLC digital input";
    DO = "do", "A", "PLC digital output";
    CO = "co", "K", "Change-over contact";
    SWCO = "swco", "S", "Switch, change-over";
    POWER4 = "power4", "K", "Power contacts, 4-pole";
    RCBO2 = "rcbo2", "Q", "RCBO, 1P+N";
    RCD2 = "rcd2", "Q", "RCD, 2-pole";
    RCD4 = "rcd4", "Q", "RCD, 4-pole";
    SSRIN = "ssrin", "K", "SSR input";
    SSROUT = "ssrout", "K", "SSR output";
    FUSETERM = "fuseterm", "F", "Fuse terminal";
    FUSEBANK4 = "fusebank4", "F", "Fuse bank, 4-way";
    FUSEBANK6 = "fusebank6", "F", "Fuse bank, 6-way";
    FUSEBANK8 = "fusebank8", "F", "Fuse bank, 8-way";
    FUSEBANK10 = "fusebank10", "F", "Fuse bank, 10-way";
    FUSEBANK12 = "fusebank12", "F", "Fuse bank, 12-way";
    PROX3 = "prox3", "B", "Proximity switch, 3-wire PNP";
    FLOATNO = "floatno", "B", "Float switch, NO";
    FLOATNC = "floatnc", "B", "Float switch, NC";
    ROLLERNO = "rollerno", "B", "Position switch with roller, NO";
    ROLLERNC = "rollernc", "B", "Position switch with roller, NC";
    INTERLOCKNC = "interlocknc", "B", "Interlock switch, NC";
    INTERLOCKNO = "interlockno", "B", "Interlock switch, NO";
    VALVE = "valve", "MB", "Solenoid valve";
    VALVE2 = "valve2", "MB", "Dual solenoid valve";
    TC = "tc", "B", "Thermocouple";
    HEATER = "heater", "E", "Heater";
    SOCKET2 = "socket2", "X", "Socket, 2-pin";
    SOCKET3 = "socket3", "X", "Socket, 3-pin";
    SOCKET4 = "socket4", "X", "Socket, 4-pin";
    SOCKET6 = "socket6", "X", "Socket, 6-pin";
    SOCKET8 = "socket8", "X", "Socket, 8-pin";
    SOCKET12 = "socket12", "X", "Socket, 12-pin";
    PLUG2 = "plug2", "X", "Plug, 2-pin";
    PLUG3 = "plug3", "X", "Plug, 3-pin";
    PLUG4 = "plug4", "X", "Plug, 4-pin";
    PLUG6 = "plug6", "X", "Plug, 6-pin";
    PLUG8 = "plug8", "X", "Plug, 8-pin";
    PLUG12 = "plug12", "X", "Plug, 12-pin";
    DSUB9S = "dsub9s", "X", "D-sub socket, 9-way";
    DSUB15S = "dsub15s", "X", "D-sub socket, 15-way";
    DSUB25S = "dsub25s", "X", "D-sub socket, 25-way";
    DSUB9P = "dsub9p", "X", "D-sub plug, 9-way";
    DSUB15P = "dsub15p", "X", "D-sub plug, 15-way";
    DSUB25P = "dsub25p", "X", "D-sub plug, 25-way";
    M12S4 = "m12s4", "X", "M12 socket, 4-pin";
    M12S5 = "m12s5", "X", "M12 socket, 5-pin";
    M12S8 = "m12s8", "X", "M12 socket, 8-pin";
    M12P4 = "m12p4", "X", "M12 plug, 4-pin";
    M12P5 = "m12p5", "X", "M12 plug, 5-pin";
    M12P8 = "m12p8", "X", "M12 plug, 8-pin";
    RJ45 = "rj45", "X", "RJ45 bulkhead";
    USBA = "usba", "X", "USB-A bulkhead";
    USBB = "usbb", "X", "USB-B bulkhead";
    // Brief 044 (design 044 §10): the connector pins, 034's pattern.
    PINPAIR = "pinpair", "X", "Connector pin pair";
    PINS = "pins", "X", "Connector socket pin";
    PINP = "pinp", "X", "Connector plug pin";
    PLCCPU = "plccpu", "A", "PLC CPU";
    PLCPS = "plcps", "A", "PLC power supply";
    PLCDI = "plcdi", "A", "PLC digital input card";
    PLCDQ = "plcdq", "A", "PLC digital output card";
    PLCAI = "plcai", "A", "PLC analogue input card";
    PLCAO = "plcao", "A", "PLC analogue output card";
    PLCRTD = "plcrtd", "A", "PLC RTD card";
    PLCIOL = "plciol", "A", "PLC IO-Link master";
}

// ---------------------------------------------------------------- the builder (brief 048)

/// The point grid connection points of a new symbol snap to (design 048 §7.1, owner answer 6):
/// 5 mm, 2.5 mm allowed for dense pin rows. The key uses `_`, as every registry key (the key rule).
pub static POINT_GRID: Entry = Entry {
    key: "symbol.point_grid",
    label: "Symbol connection point grid",
    group: "Symbols",
    brief: "048",
    kind: Kind::Length { min: 1.0, max: 10.0 },
    builtin: Value::Length(crate::symbol_cmds::POINT_GRID_MM),
    levels: SP,
    project: ProjectSource::Property,
};

/// Brief 048's entries (listed in [`super::all`]).
pub static BUILDER_ENTRIES: &[&Entry] = &[&POINT_GRID];

/// The point grid of `doc`, millimetres.
pub fn point_grid(doc: &Document) -> f64 {
    super::resolve(doc, &POINT_GRID, Scope::project()).number()
}

// ---------------------------------------------------------------- family entries (brief 048, FP6)

/// A field of a symbol's family entries, `symbol.<key>.<field>` (design 048 §7.1): one entry per
/// symbol the project can place. A starter's prefix is its static entry above; the other fields,
/// and every field of a project symbol, resolve through [`resolve_for`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Prefix,
    Description,
    Rating,
    Join,
}

impl Field {
    pub const ALL: [Field; 4] = [Field::Prefix, Field::Description, Field::Rating, Field::Join];

    /// The last part of its key.
    pub fn word(self) -> &'static str {
        match self {
            Field::Prefix => "prefix",
            Field::Description => "description",
            Field::Rating => "rating",
            Field::Join => "join",
        }
    }

    /// Its words on the Standards screen and in `defaults`.
    pub fn label(self) -> &'static str {
        match self {
            Field::Prefix => "Tag prefix",
            Field::Description => "Description",
            Field::Rating => "Rating",
            Field::Join => "Joins a device as",
        }
    }

    /// Most characters of a value (design 048 §2.4).
    pub fn max_chars(self) -> usize {
        match self {
            Field::Prefix => crate::devices::MAX_PREFIX_LETTERS + 1,
            Field::Description => 200,
            Field::Rating => 40,
            Field::Join => 6,
        }
    }

    /// Whether a project level may override it (`join` is the definition's only).
    pub fn overridable(self) -> bool {
        self != Field::Join
    }

    /// Whether `value` is a valid value of this field, trimmed.
    pub fn accepts(self, value: &str) -> bool {
        let v = value.trim();
        let plain = !v.is_empty() && v.chars().count() <= self.max_chars() && !v.chars().any(char::is_control);
        plain
            && match self {
                Field::Prefix => crate::devices::valid_prefix(v).is_some(),
                Field::Join => v.eq_ignore_ascii_case("device") || v.eq_ignore_ascii_case("part"),
                Field::Description | Field::Rating => true,
            }
    }
}

/// The key `symbol.<key>.<field>` of a family entry.
pub fn family_key(symbol: &str, field: Field) -> String {
    format!("symbol.{symbol}.{}", field.word())
}

/// A family entry's key split into the symbol's key and the field, when `key` names one of a
/// symbol the project can place (a starter, or a project symbol's newest version).
pub fn family(doc: &Document, key: &str) -> Option<(String, Field)> {
    let rest = key.trim().strip_prefix("symbol.")?;
    let (symbol, field) = rest.rsplit_once('.')?;
    let field = Field::ALL.into_iter().find(|f| f.word() == field)?;
    if let Some(s) = crate::symbols::by_key(symbol) {
        // A starter's prefix is its static entry; a starter has no `join`.
        return (field != Field::Join && field != Field::Prefix && s.kind != crate::symbols::Kind::Tagless).then(|| (s.key.to_string(), field));
    }
    let v = crate::symbols::project_by_key(doc, symbol)?;
    Some((v.key()?.to_string(), field))
}

/// The family entries `defaults` lists: every field of every project symbol (newest version), by
/// name; (symbol key, field).
pub fn family_lines(doc: &Document) -> Vec<(String, Field)> {
    crate::symbols::project_symbols(doc)
        .into_iter()
        .filter_map(|v| v.key().map(str::to_string))
        .flat_map(|k| Field::ALL.into_iter().map(move |f| (k.clone(), f)))
        .collect()
}

/// One family entry as `defaults` lists it (the fields of an ordinary entry's line).
pub fn family_json(doc: &Document, symbol: &str, field: Field) -> serde_json::Value {
    let r = resolve_for(doc, symbol, field);
    let builtin = own_value(doc, symbol, field).map(|(v, _)| v);
    serde_json::json!({
        "key": family_key(symbol, field),
        "label": format!("{}: {symbol}", field.label()),
        "group": "Project symbols",
        "brief": "048",
        "kind": "text",
        "builtin": builtin,
        "levels": if field.overridable() { serde_json::json!(["standard", "project"]) } else { serde_json::json!([]) },
        "value": r.as_ref().map(|(v, _)| v.clone()),
        "from": r.map(|(_, s)| s.as_str()),
    })
}

/// The project property a family entry's project level is stored under (description and rating);
/// a prefix is stored in `settings.tag_prefixes[key]` (one project path, as 034).
pub fn family_property(symbol: &str, field: Field) -> Option<String> {
    matches!(field, Field::Description | Field::Rating).then(|| family_key(symbol, field))
}

/// The value of `symbol`'s `field` and where it came from (design 048 §7.1): the project's
/// override, else the definition's own value of a project symbol ([`Source::Definition`](super::Source::Definition)),
/// else a starter's built-in (its prefix, its name as description). `None` when nothing gives one.
/// A hostile stored value counts as absent.
pub fn resolve_for(doc: &Document, symbol: &str, field: Field) -> Option<(String, super::Source)> {
    project_value(doc, symbol, field).map(|v| (v, super::Source::Project)).or_else(|| own_value(doc, symbol, field))
}

/// The project's override of `symbol`'s `field`, if it holds a valid one.
pub fn project_value(doc: &Document, symbol: &str, field: Field) -> Option<String> {
    if !field.overridable() {
        return None;
    }
    match field {
        Field::Prefix => doc.settings.tag_prefixes.get(symbol).and_then(|v| crate::devices::valid_prefix(v)),
        _ => family_property(symbol, field).and_then(|k| doc.properties.get(&k)).filter(|v| field.accepts(v)).map(|v| v.trim().to_string()),
    }
}

/// `field` of definition `d` itself (a project symbol's own default), if valid.
pub fn definition_value(d: &ecad_model::Definition, field: Field) -> Option<String> {
    match field {
        Field::Join => Some(crate::symbols::join_of(d).word().to_string()),
        Field::Prefix => d.defaults.get("prefix").and_then(|p| crate::devices::valid_prefix(p)),
        _ => d.defaults.get(field.word()).filter(|v| field.accepts(v)).map(|v| v.trim().to_string()),
    }
}

/// `symbol`'s own value of `field`, without the project's override: a starter's built-in, or a
/// project symbol's definition value.
pub fn own_value(doc: &Document, symbol: &str, field: Field) -> Option<(String, super::Source)> {
    let ok = |v: &str| field.accepts(v).then(|| v.trim().to_string());
    if let Some(s) = crate::symbols::by_key(symbol) {
        return match field {
            Field::Prefix => prefix_entry(s.key).map(|e| (builtin_prefix(e).to_string(), super::Source::Builtin)).filter(|(p, _)| !p.is_empty()),
            Field::Description => Some((s.name.to_string(), super::Source::Builtin)),
            Field::Rating | Field::Join => None,
        };
    }
    let v = crate::symbols::project_by_key(doc, symbol)?;
    let own = v.def.defaults.get(field.word()).and_then(|x| ok(x));
    let own = match field {
        Field::Prefix => own.and_then(|p| crate::devices::valid_prefix(&p)),
        Field::Join => Some(crate::symbols::join_of(v.def).word().to_string()),
        _ => own,
    };
    own.map(|x| (x, super::Source::Definition))
}

/// The tag prefix a new device of symbol `key` gets in this project (letters, upper case, without
/// `-`): the project's `tag_prefixes`, else the built-in. Empty for a tagless symbol (junction,
/// continuation) or an unknown key. A hostile stored value that is not a prefix counts as absent.
/// A project symbol's key resolves through [`resolve_for`] (brief 048).
pub fn prefix(doc: &Document, key: &str) -> String {
    let Some(entry) = prefix_entry(key) else {
        return resolve_for(doc, key, Field::Prefix).map(|(p, _)| p).unwrap_or_default();
    };
    let resolved = super::resolve(doc, entry, Scope::project()).text();
    crate::devices::valid_prefix(&resolved).unwrap_or_else(|| builtin_prefix(entry).to_string())
}

/// The built-in prefix of an entry of this file (empty for any other entry).
pub fn builtin_prefix(entry: &Entry) -> &'static str {
    match entry.builtin {
        Value::Text(t) => t,
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbols::{Kind as SymbolKind, STARTERS};

    #[test]
    fn every_tagged_symbol_has_one_prefix_entry_and_no_other() {
        for s in STARTERS {
            let e = prefix_entry(s.key);
            assert_eq!(e.is_some(), s.kind != SymbolKind::Tagless, "{}", s.key);
            // Brief 036 (overseer ruling): its starters' prefixes are in the `terminal` area.
            let area = if crate::terminal_types::is_level_starter(s.key) { "terminal" } else { "symbol" };
            if let Some(e) = e {
                assert_eq!(e.key, format!("{area}.{}.prefix", s.key));
                let list = if area == "terminal" { super::super::terminals::ENTRIES } else { ENTRIES };
                assert!(list.iter().any(|x| std::ptr::eq(*x, e)), "{}", s.key);
            }
        }
        assert_eq!(
            ENTRIES.len(),
            STARTERS.iter().filter(|s| s.kind != SymbolKind::Tagless && !crate::terminal_types::is_level_starter(s.key)).count()
        );
    }

    #[test]
    fn the_project_level_is_tag_prefixes_and_hostile_values_fall_back() {
        let mut doc = Document::default();
        assert_eq!(prefix(&doc, "fusebank8"), "F");
        doc.settings.tag_prefixes.insert("fusebank8".into(), "FB".into());
        assert_eq!(prefix(&doc, "fusebank8"), "FB");
        assert_eq!(super::super::resolve(&doc, &FUSEBANK8, Scope::project()).from, super::super::Source::Project);
        // A project property of the same key is never read: one project path.
        doc.settings.tag_prefixes.clear();
        doc.properties.insert("symbol.fusebank8.prefix".into(), "ZZ".into());
        assert_eq!(prefix(&doc, "fusebank8"), "F");
        let long = "x".repeat(10_000);
        for bad in ["", "K1", long.as_str(), "K\u{7}", "-"] {
            doc.settings.tag_prefixes.insert("fusebank8".into(), bad.into());
            assert_eq!(prefix(&doc, "fusebank8"), "F", "{bad:?}");
        }
        assert_eq!(prefix(&doc, "junction"), "");
        assert_eq!(prefix(&doc, "nonsense"), "");
    }
}
