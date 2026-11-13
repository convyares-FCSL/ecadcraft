//! Lane U's BOM defaults (brief 047, design 047 §8). Owned by lane U: append `pub static` entries
//! and list them in [`ENTRIES`].
//!
//! Read them through `bom::article_column`, `bom::lines_nested` and `bom::item_unit_metres`, not
//! by key. A part line's quantity default (1) is the meaning of an absent `qty`, not a preference,
//! and is not an entry; the guards (99 lines, 999 sub-items, 10 000 items…) are constants in
//! `bom.rs`.

use super::{Entry, Kind, Level, ProjectSource, Value};

const GROUP: &str = "BOM";
const BRIEF: &str = "047";
/// Standard and project.
const SP: &[Level] = &[Level::Standard, Level::Project];

const fn entry(key: &'static str, label: &'static str, words: &'static [&'static str], builtin: &'static str) -> Entry {
    Entry {
        key,
        label,
        group: GROUP,
        brief: BRIEF,
        kind: Kind::Choice(words),
        builtin: Value::Choice(builtin),
        levels: SP,
        project: ProjectSource::Property,
    }
}

/// `bom.column.article`: the Article number column of the BOM window, BOM page and CSV (owner UI
/// answer 12: off until the user adds it).
pub static COLUMN_ARTICLE: Entry = entry("bom.column.article", "Article number column", &["on", "off"], "off");

/// `bom.lines`: part lines merged with equal parts (`flat`) or listed under their device
/// (`nested`, By location only).
pub static LINES: Entry = entry("bom.lines", "Part lines", &["flat", "nested"], "flat");

/// `bom.item.unit`: the unit of a new item when none is given.
pub static ITEM_UNIT: Entry = entry("bom.item.unit", "Unit of a new item", &["pcs", "m"], "pcs");

/// This lane's entries, in Standards order.
pub static ENTRIES: &[&Entry] = &[&COLUMN_ARTICLE, &LINES, &ITEM_UNIT];
