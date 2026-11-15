//! Brief 054's registry entries (design 054 §2.3, §6.4, FP5): a page's type (page level only)
//! and the cover's block toggles (standard and project).

use super::{Entry, Kind, Level, ProjectSource, Value};

const GROUP: &str = "Pages";
const BRIEF: &str = "054";
const G: &[Level] = &[Level::Page];
const SP: &[Level] = &[Level::Standard, Level::Project];
const ON_OFF: Kind = Kind::Choice(&["on", "off"]);

const fn toggle(key: &'static str, label: &'static str, on: bool) -> Entry {
    Entry {
        key,
        label,
        group: "Cover",
        brief: BRIEF,
        kind: ON_OFF,
        builtin: Value::Choice(if on { "on" } else { "off" }),
        levels: SP,
        project: ProjectSource::Property,
    }
}

/// The cover's block toggles (design 054 §6.4), standard and project only (Q8).
pub static NOTES: Entry = toggle("front.cover.notes", "Notes", true);
pub static WIRE_COLOURS: Entry = toggle("front.cover.wire_colours", "Wire colours", true);
pub static SYMBOL_LEGEND: Entry = toggle("front.cover.symbol_legend", "Symbol legend", true);
pub static REVISIONS: Entry = toggle("front.cover.revisions", "Revisions", true);
pub static STRUCTURE_CODES: Entry = toggle("front.cover.structure_codes", "Structure codes", false);
pub static CUSTOMER_SITE: Entry = toggle("front.cover.customer_site", "Customer and site", true);
/// What an empty cover heading is stored as: the cover shows the title alone.
pub const NO_HEADING: &str = "none";

/// The words after the project's title in the cover heading (Q12); empty (stored as
/// [`NO_HEADING`]) leaves the title alone.
pub static HEADING: Entry = Entry {
    key: "front.cover.heading",
    label: "Cover heading",
    group: "Cover",
    brief: BRIEF,
    kind: Kind::Text { max_chars: 64 },
    builtin: Value::Text("Electrical documentation"),
    levels: SP,
    project: ProjectSource::Property,
};

/// A page's type: a style of catalogue `page`, at page level only (a type is a property of the
/// page, never a project default). The built-in is the Schematic type; a page's resolved type
/// also reads its generated kind ([`crate::styles::page::type_of`]).
pub static PAGE_TYPE: Entry = Entry {
    key: "page.type",
    label: "Page type",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Style("page"),
    builtin: Value::Text("schematic"),
    levels: G,
    project: ProjectSource::Property,
};

/// Every entry of this file.
pub static ENTRIES: &[&Entry] = &[&PAGE_TYPE, &NOTES, &WIRE_COLOURS, &SYMBOL_LEGEND, &REVISIONS, &STRUCTURE_CODES, &CUSTOMER_SITE, &HEADING];
