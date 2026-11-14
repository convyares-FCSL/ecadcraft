//! Brief 054's registry entries (design 054 §2.3, §6.4, FP5): a page's type (page level only)
//! and the cover's block toggles (standard and project).

use super::{Entry, Kind, Level, ProjectSource, Value};

const GROUP: &str = "Pages";
const BRIEF: &str = "054";
const G: &[Level] = &[Level::Page];
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
pub static ENTRIES: &[&Entry] = &[&PAGE_TYPE];
