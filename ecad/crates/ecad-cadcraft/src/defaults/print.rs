//! Lane P's defaults (033, print and project). Owned by lane P: append `pub static` entries and list them in
//! [`ENTRIES`]. Lane T created this file empty (design 031 §3.5) and does not edit it again.
//!
//! The tag style (`tags.dash`, `tags.case`) and the revision settings (`revisions.columns`,
//! `revisions.pending`) join this file when the registry takes their key areas (overseer, 2026-11-12).

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
    entry("print.continuation_numbers", "Continuation sheet numbers", Kind::Word(&["sequence"]), Value::Word("sequence"));

/// How a missing value in a generated table is marked (design 033 §3.1): `boxed` is `?` in a box.
pub static MISSING_MARK: Entry = entry("print.missing_mark", "Missing value mark", Kind::Word(&["boxed"]), Value::Word("boxed"));

/// `ecad.missing-box`: how far the box round a missing value is inset into its cell.
pub static MISSING_BOX_INSET: Entry =
    entry("print.missing_box_inset", "Missing value box inset", Kind::Length { min: 0.0, max: 2.0 }, Value::Length(0.5));

/// Where an automatic contact table goes when it would meet the title block (design 033 §4.1):
/// right of its head, else lifted.
pub static CONTACT_TABLE_FALLBACK: Entry =
    entry("print.contact_table_fallback", "Contact table clear of the title block", Kind::Word(&["right_then_lift"]), Value::Word("right_then_lift"));

/// The room kept between an automatic contact table and the title block's top line.
pub static CONTACT_TABLE_CLEARANCE: Entry =
    entry("print.contact_table_clearance", "Contact table gap above the title block", Kind::Length { min: 0.0, max: 20.0 }, Value::Length(2.0));

/// Every entry of this file, in the order `defaults` lists them.
pub static ENTRIES: &[&Entry] = &[&CONTINUATION_NUMBERS, &MISSING_MARK, &MISSING_BOX_INSET, &CONTACT_TABLE_FALLBACK, &CONTACT_TABLE_CLEARANCE];
