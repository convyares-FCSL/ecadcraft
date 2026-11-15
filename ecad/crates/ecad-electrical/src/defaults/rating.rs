//! Lane 057's defaults (design 057 §6.3, FP5): the `rating` area. Standard and project levels;
//! a cable's own `rating.ambient`, `rating.method`, `rating.voltage`, `rating.system` and
//! `rating.vd_limit` (and a conductor's `rating.bundling`) are read by [`crate::rating`] from the
//! object itself, since the registry has no cable level.

use super::{Entry, Kind, Level, ProjectSource, Value};

const GROUP: &str = "Rating";
const BRIEF: &str = "057";
const SP: &[Level] = &[Level::Standard, Level::Project];

const fn entry(key: &'static str, label: &'static str, kind: Kind, builtin: Value<'static>) -> Entry {
    Entry { key, label, group: GROUP, brief: BRIEF, kind, builtin, levels: SP, project: ProjectSource::Property }
}

/// The bases a subject may use.
pub const BASES: &[&str] = &["datasheet", "table", "manual"];

fn bundling_or_none(s: &str) -> bool {
    s.trim().eq_ignore_ascii_case("none") || crate::rating::parse::bundling(s).is_some()
}

pub static WIRE_BASIS: Entry = entry("rating.wire_basis", "Panel wire basis", Kind::Choice(BASES), Value::Choice("datasheet"));
pub static CABLE_BASIS: Entry = entry("rating.cable_basis", "Field cable basis", Kind::Choice(BASES), Value::Choice("table"));
pub static PANEL_TEMPERATURE: Entry =
    entry("rating.panel_temperature", "Panel temperature (\u{b0}C)", Kind::Number { min: -40.0, max: 90.0 }, Value::Number(40.0));
pub static AMBIENT: Entry = entry("rating.ambient", "Field ambient (\u{b0}C)", Kind::Number { min: -40.0, max: 90.0 }, Value::Number(30.0));
/// The project's rating table (FP8): none until the user enters or imports one.
pub static TABLE: Entry = entry("rating.table", "Rating table", Kind::Style("ratingtable"), Value::Text("none"));
/// Not set: × 1.00 is used, with a warning (Q7).
pub static BUNDLING: Entry = entry("rating.bundling", "Bundling", Kind::Parsed { max_chars: 32, parse: bundling_or_none }, Value::Text("none"));
pub static METHOD: Entry = entry("rating.method", "Installation method", Kind::Choice(&crate::rating::dataset::METHODS), Value::Choice("E"));
pub static VOLTAGE: Entry = entry("rating.voltage", "Supply voltage (V)", Kind::Number { min: 1.0, max: 1000.0 }, Value::Number(400.0));
pub static SYSTEM: Entry = entry("rating.system", "Supply system", Kind::Choice(&["ac1", "ac3", "dc"]), Value::Choice("ac3"));
pub static VD_LIMIT: Entry = entry("rating.vd_limit", "Volt-drop limit (%)", Kind::Number { min: 0.5, max: 20.0 }, Value::Number(5.0));

/// Every entry of this file, in the order Standards ▸ Defaults lists them.
pub static ENTRIES: &[&Entry] = &[&WIRE_BASIS, &CABLE_BASIS, &PANEL_TEMPERATURE, &AMBIENT, &TABLE, &BUNDLING, &METHOD, &VOLTAGE, &SYSTEM, &VD_LIMIT];
