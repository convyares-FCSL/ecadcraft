//! Lane P's defaults (033, print and project). Owned by lane P: append `pub static` entries and list them in
//! [`ENTRIES`]. Lane T created this file empty (design 031 §3.5) and does not edit it again.
//!
//! The tag style's defaults are `pub const` in `ecad_model::tag` (overseer ruling 2), which
//! `ecad-model`'s numbering and merge read without the registry; these entries name them.

use super::{Entry, Kind, Level, ProjectSource, Value};

const GROUP: &str = "Print";
const BRIEF: &str = "033";
/// Standard and project (no command writes these project keys in v0.15).
const SP: &[Level] = &[Level::Standard, Level::Project];

const fn entry(key: &'static str, label: &'static str, kind: Kind, builtin: Value<'static>) -> Entry {
    Entry { key, label, group: GROUP, brief: BRIEF, kind, builtin, levels: SP, project: ProjectSource::Property }
}

/// How a generated page's continuation sheets are numbered (design 033 §5.1): `sequence` is the
/// trailing digits plus one (`400, 401`), else `N2`, `N3`.
pub static CONTINUATION_NUMBERS: Entry =
    entry("print.continuation_numbers", "Continuation sheet numbers", Kind::Choice(&["sequence"]), Value::Choice("sequence"));

/// How a missing value in a generated table is marked (design 033 §3.1): `boxed` is `?` in a box.
pub static MISSING_MARK: Entry = entry("print.missing_mark", "Missing value mark", Kind::Choice(&["boxed"]), Value::Choice("boxed"));

/// `ecad.missing-box`: how far the box round a missing value is inset into its cell.
pub static MISSING_BOX_INSET: Entry =
    entry("print.missing_box_inset", "Missing value box inset", Kind::Length { min: 0.0, max: 2.0 }, Value::Length(0.5));

/// Where an automatic contact table goes when it would meet the title block (design 033 §4.1):
/// right of its head, else lifted.
pub static CONTACT_TABLE_FALLBACK: Entry = entry(
    "print.contact_table_fallback",
    "Contact table clear of the title block",
    Kind::Choice(&["right_then_lift"]),
    Value::Choice("right_then_lift"),
);

/// The room kept between an automatic contact table and the title block's top line.
pub static CONTACT_TABLE_CLEARANCE: Entry =
    entry("print.contact_table_clearance", "Contact table gap above the title block", Kind::Length { min: 0.0, max: 20.0 }, Value::Length(2.0));

/// Tags are written with a leading `-` (design 033 §8.1, FP1).
pub static TAG_DASH: Entry = Entry {
    key: ecad_model::tag::DASH_KEY,
    label: "Leading - on tags",
    group: "Tags",
    brief: BRIEF,
    kind: Kind::Choice(ecad_model::tag::DASH_WORDS),
    builtin: Value::Choice(ecad_model::tag::DEFAULT_DASH),
    levels: SP,
    project: ProjectSource::Property,
};

/// Tags are written in upper case, or as typed (design 033 §8.1, FP1).
pub static TAG_CASE: Entry = Entry {
    key: ecad_model::tag::CASE_KEY,
    label: "Tag case",
    group: "Tags",
    brief: BRIEF,
    kind: Kind::Choice(ecad_model::tag::CASE_WORDS),
    builtin: Value::Choice(ecad_model::tag::DEFAULT_CASE),
    levels: SP,
    project: ProjectSource::Property,
};

/// The revision table's columns (design 033 §7.1, FP2): `field[:Header][@width], …`, widths in
/// millimetres on A3; Description takes the rest.
pub static REVISION_COLUMNS: Entry = Entry {
    key: crate::revisions::COLUMNS_KEY,
    label: "Revision columns",
    group: "Revisions",
    brief: BRIEF,
    kind: Kind::Parsed { max_chars: crate::revisions::MAX_COLUMNS_CHARS, parse: crate::revisions::columns_ok },
    builtin: Value::Text("rev@12, date@24, description, drawn@22, checked@22, approved@22"),
    levels: SP,
    project: ProjectSource::Property,
};

/// The width of a custom revision column that does not give one (design 033 §7.1).
pub static REVISION_CUSTOM_WIDTH: Entry = Entry {
    key: "revisions.custom_width",
    label: "Custom revision column width",
    group: "Revisions",
    brief: BRIEF,
    kind: Kind::Length { min: crate::revisions::MIN_WIDTH_MM, max: crate::revisions::MAX_WIDTH_MM },
    builtin: Value::Length(20.0),
    levels: SP,
    project: ProjectSource::Property,
};

/// How the title block marks changes since the newest issue (design 033 §6.2, FP3).
pub static REVISION_PENDING: Entry = Entry {
    key: crate::revisions::PENDING_KEY,
    label: "Pending changes marker",
    group: "Revisions",
    brief: BRIEF,
    kind: Kind::Choice(&["star", "draft", "off"]),
    builtin: Value::Choice("star"),
    levels: SP,
    project: ProjectSource::Property,
};

/// The project's tag style (design 033 §8): its padding, and the dash and case resolved through
/// this registry ([`TAG_DASH`], [`TAG_CASE`]). The same as `ecad_model::tag::Style::of`, which
/// `ecad-model` reads without the registry (a test keeps them equal).
pub fn tag_style(doc: &ecad_model::Document) -> ecad_model::tag::Style {
    use ecad_model::tag::{Case, MAX_PADDING, Style};
    let word = |e: &Entry| super::resolve(doc, e, super::Scope::project()).word();
    Style {
        padding: doc.settings.tag_padding.min(MAX_PADDING),
        dash: word(&TAG_DASH) == "yes",
        case: if word(&TAG_CASE) == "typed" { Case::Typed } else { Case::Upper },
    }
}

/// Every entry of this file, in the order `defaults` lists them.
pub static ENTRIES: &[&Entry] = &[
    &TAG_DASH,
    &TAG_CASE,
    &REVISION_COLUMNS,
    &REVISION_CUSTOM_WIDTH,
    &REVISION_PENDING,
    &CONTINUATION_NUMBERS,
    &MISSING_MARK,
    &MISSING_BOX_INSET,
    &CONTACT_TABLE_FALLBACK,
    &CONTACT_TABLE_CLEARANCE,
];
