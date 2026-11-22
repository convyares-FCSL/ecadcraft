//! Lane P's defaults (033, print and project). Owned by lane P: append `pub static` entries and list them in
//! [`ENTRIES`]. Lane T created this file empty (design 031 §3.5) and does not edit it again.
//!
//! The tag style's defaults are `pub const` in `ecad_model::naming::tag` (overseer ruling 2), which
//! `ecad-model`'s numbering and merge read without the registry; these entries name them.

use crate::defaults::{Entry, Kind, Level, ProjectSource, Slot};
use crate::CmdError;

ecad_model::word_enum! {
    /// How continuation sheets are numbered.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ContinuationNumbers {
        Sequence,
    }
}
ecad_model::word_enum! {
    /// How a missing value is marked on a print.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MissingMark {
        Boxed,
    }
}
ecad_model::word_enum! {
    /// Where a contact table goes when the title block is in the way.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ContactTableFallback {
        RightThenLift,
    }
}
ecad_model::word_enum! {
    /// Whether prints are black or in the wires' colours.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PrintColour {
        Mono,
        Colour,
    }
}

const GROUP: &str = "Print";
const BRIEF: &str = "033";
/// Standard and project (no command writes these project keys in v0.15).
const SP: &[Level] = &[Level::Standard, Level::Project];

const fn entry(key: &'static str, label: &'static str, kind: Kind) -> Entry {
    Entry { key, label, group: GROUP, brief: BRIEF, kind, slot: Slot::new(), levels: SP, project: ProjectSource::Property }
}

/// How a generated page's continuation sheets are numbered (design 033 §5.1): `sequence` is the
/// trailing digits plus one (`400, 401`), else `N2`, `N3`.
pub static CONTINUATION_NUMBERS: Entry =
    entry("print.continuation_numbers", "Continuation sheet numbers", Kind::Choice(<ContinuationNumbers as ecad_model::Word>::WORDS));

/// How a missing value in a generated table is marked (design 033 §3.1): `boxed` is `?` in a box.
pub static MISSING_MARK: Entry = entry("print.missing_mark", "Missing value mark", Kind::Choice(<MissingMark as ecad_model::Word>::WORDS));

/// `ecad.missing-box`: how far the box round a missing value is inset into its cell.
pub static MISSING_BOX_INSET: Entry = entry("print.missing_box_inset", "Missing value box inset", Kind::Length { min: 0.0, max: 2.0 });

/// Where an automatic contact table goes when it would meet the title block (design 033 §4.1):
/// right of its head, else lifted.
pub static CONTACT_TABLE_FALLBACK: Entry =
    entry("print.contact_table_fallback", "Contact table clear of the title block", Kind::Choice(<ContactTableFallback as ecad_model::Word>::WORDS));

/// The room kept between an automatic contact table and the title block's top line.
pub static CONTACT_TABLE_CLEARANCE: Entry =
    entry("print.contact_table_clearance", "Contact table gap above the title block", Kind::Length { min: 0.0, max: 20.0 });

/// Tags are written with a leading `-` (design 033 §8.1, FP1).
pub static TAG_DASH: Entry = Entry {
    key: ecad_model::naming::tag::DASH_KEY,
    label: "Leading - on tags",
    group: "Tags",
    brief: BRIEF,
    kind: Kind::Choice(ecad_model::naming::tag::DASH_WORDS),
    slot: Slot::new(),
    levels: SP,
    project: ProjectSource::Property,
};

/// Tags are written in upper case, or as typed (design 033 §8.1, FP1).
pub static TAG_CASE: Entry = Entry {
    key: ecad_model::naming::tag::CASE_KEY,
    label: "Tag case",
    group: "Tags",
    brief: BRIEF,
    kind: Kind::Choice(ecad_model::naming::tag::CASE_WORDS),
    slot: Slot::new(),
    levels: SP,
    project: ProjectSource::Property,
};

/// The revision table's columns (design 033 §7.1, FP2): `field[:Header][@width], …`, widths in
/// millimetres on A3; Description takes the rest.
pub static REVISION_COLUMNS: Entry = Entry {
    key: crate::releases::revisions::COLUMNS_KEY,
    label: "Revision columns",
    group: "Revisions",
    brief: BRIEF,
    kind: Kind::Parsed { max_chars: crate::releases::revisions::MAX_COLUMNS_CHARS, parse: crate::releases::revisions::columns_ok },
    slot: Slot::new(),
    levels: SP,
    project: ProjectSource::Property,
};

/// The width of a custom revision column that does not give one (design 033 §7.1).
pub static REVISION_CUSTOM_WIDTH: Entry = Entry {
    key: "revisions.custom_width",
    label: "Custom revision column width",
    group: "Revisions",
    brief: BRIEF,
    kind: Kind::Length { min: crate::releases::revisions::MIN_WIDTH_MM, max: crate::releases::revisions::MAX_WIDTH_MM },
    slot: Slot::new(),
    levels: SP,
    project: ProjectSource::Property,
};

/// How the title block marks changes since the newest issue (design 033 §6.2, FP3).
pub static REVISION_PENDING: Entry = Entry {
    key: crate::releases::revisions::PENDING_KEY,
    label: "Pending changes marker",
    group: "Revisions",
    brief: BRIEF,
    kind: Kind::Choice(<crate::releases::revisions::PendingStyle as ecad_model::Word>::WORDS),
    slot: Slot::new(),
    levels: SP,
    project: ProjectSource::Property,
};

/// The project's tag style (design 033 §8): its padding, and the dash and case resolved through
/// this registry ([`TAG_DASH`], [`TAG_CASE`]). The same as `ecad_model::naming::tag::Style::of`, which
/// `ecad-model` reads without the registry (a test keeps them equal).
pub fn tag_style(doc: &ecad_model::Document) -> ecad_model::naming::tag::Style {
    use ecad_model::naming::tag::{Case, MAX_PADDING, Style};
    use ecad_model::naming::words::Word;
    let word = |e: &Entry| crate::defaults::resolve(doc, e, crate::defaults::Scope::project()).word();
    Style {
        padding: doc.settings.tag_padding().min(MAX_PADDING),
        dash: ecad_model::naming::words::flag(word(&TAG_DASH)).unwrap_or(true),
        case: Case::parse(word(&TAG_CASE)).unwrap_or_default(),
    }
}

/// Brief 040 (owner answer 1): whether prints are `mono` (black, with the colour code in the wire
/// label; the built-in default) or `colour` (wires in their colours, two-colour wires with the
/// second colour's dashes, everything else in ink). A print command's own `colour` wins.
pub static COLOUR: Entry = Entry {
    key: "print.colour",
    label: "Print colour",
    group: GROUP,
    brief: "040",
    kind: Kind::Choice(<PrintColour as ecad_model::Word>::WORDS),
    slot: Slot::new(),
    levels: SP,
    project: ProjectSource::Property,
};

/// Whether `doc` prints mono ([`COLOUR`] is `mono`, the default).
pub fn mono(doc: &ecad_model::Document) -> bool {
    crate::defaults::resolve(doc, &COLOUR, crate::defaults::Scope::project()).choice::<PrintColour>() != Some(PrintColour::Colour)
}

/// A print command's `colour` parameter (`mono`, `colour`, or `screen` as `colour`), or the
/// project's [`COLOUR`] when it gives none: whether to print mono.
///
/// # Errors
/// Any other value, in words.
pub fn mono_param(doc: &ecad_model::Document, params: &serde_json::Value) -> Result<bool, CmdError> {
    match params.get("colour") {
        None | Some(serde_json::Value::Null) => Ok(mono(doc)),
        Some(v) => match v.as_str() {
            Some(w) => mono_word(doc, Some(w)),
            None => Err(CmdError::input(COLOUR_WORDS)),
        },
    }
}

/// The refusal of a `colour` that is not a print colour.
pub const COLOUR_WORDS: &str = "`colour` is \"mono\" (black, the colour code in the wire label) or \"colour\" (wire colours)";

/// [`mono_param`] with the `colour` word read: `None` is the project's [`COLOUR`].
///
/// # Errors
/// A word that is not a print colour.
pub fn mono_word(doc: &ecad_model::Document, word: Option<&str>) -> Result<bool, CmdError> {
    match word.map(str::trim) {
        None => Ok(mono(doc)),
        Some("mono") => Ok(true),
        Some("colour" | "color" | "screen") => Ok(false),
        Some(_) => Err(CmdError::input(COLOUR_WORDS)),
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
    &COLOUR,
];
