//! The BOM defaults: append `pub static` entries
//! and list them in [`ENTRIES`].
//!
//! Read them through `bom::article_column`, `bom::lines_nested` and `bom::item_unit_metres`, not
//! by key. A part line's quantity default (1) is the meaning of an absent `qty`, not a preference,
//! and is not an entry; the guards (99 lines, 999 sub-items, 10 000 items…) are constants in
//! `bom.rs`.

use ecad_model::registry::data::Slot;
use ecad_model::registry::{Entry, Kind, Level, ProjectSource};

ecad_model::word_enum! {
    /// Whether part lines are listed flat or under their device.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BomLines {
        Flat,
        Nested,
    }
}

const GROUP: &str = "BOM";
/// Standard and project.
const SP: &[Level] = &[Level::Standard, Level::Project];

const fn entry(key: &'static str, label: &'static str, kind: Kind) -> Entry {
    Entry { key, label, group: GROUP, kind, slot: Slot::new(&crate::defaults::data::DATA), levels: SP, project: ProjectSource::Property }
}

/// `bom.column.article`: the Article number column of the BOM window, BOM page and CSV:
/// off until the user adds it.
pub static COLUMN_ARTICLE: Entry = entry("bom.column.article", "Article number column", Kind::Flag);

/// `bom.lines`: part lines merged with equal parts (`flat`) or listed under their device
/// (`nested`, By location only).
pub static LINES: Entry = entry("bom.lines", "Part lines", Kind::Choice(<BomLines as ecad_model::Word>::WORDS));

/// `bom.item.unit`: the unit of a new item when none is given.
pub static ITEM_UNIT: Entry =
    entry("bom.item.unit", "Unit of a new item", Kind::Choice(<ecad_model::objects::loose_part::ItemUnit as ecad_model::Word>::WORDS));

/// This lane's entries, in Standards order.
pub static ENTRIES: &[&Entry] = &[&COLUMN_ARTICLE, &LINES, &ITEM_UNIT];
