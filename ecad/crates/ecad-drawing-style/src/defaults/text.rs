//! The text defaults: the project font, which texts are shown, the wire label height and bold
//! in the stroke font.
//!
//! Read them through [`crate::text`] (`text_style`, `text_shown`), not by key. A kind's cap
//! height, weight and font are fields of its text style ([`crate::catalogues::text`]).

use ecad_model::registry::data::Slot;
use ecad_model::registry::{Entry, Kind, Level, ProjectSource, Value};

const GROUP: &str = "Text";
/// Standard and project.
const SP: &[Level] = &[Level::Standard, Level::Project];
/// Standard, project, page.
const SPG: &[Level] = &[Level::Standard, Level::Project, Level::Page];
/// Standard, project, page, object (device, symbol, conductor).
const SPGO: &[Level] = &[Level::Standard, Level::Project, Level::Page, Level::Object];
/// The heights a project may set: cap heights (baseline to the top of a capital), millimetres
/// (a text height is drawn as its cap height).
const HEIGHT: Kind = Kind::Length { min: 0.5, max: 10.0 };
const SHOW: Kind = Kind::Flag;
const SHOW_RUN: Kind = Kind::Choice(<ecad_model::look::Shown as ecad_model::Word>::WORDS);

const fn entry(key: &'static str, label: &'static str, kind: Kind, levels: &'static [Level]) -> Entry {
    Entry { key, label, group: GROUP, kind, slot: Slot::new(&crate::defaults::data::DATA), levels, project: ProjectSource::Property }
}

/// The wire label height the project chose (`settings.wiring.label_height`), or `None` when it
/// chose none (F2: a chosen value equal to the built-in is still chosen).
pub(crate) fn wire_label_height(s: &ecad_model::ProjectSettings) -> Option<Value<'_>> {
    s.wiring.label_height.map(Value::Length)
}

// --- Font ------------------------------------------------------------------------------------

/// The project font family. Missing fonts fall back; the stored value never changes.
pub static FONT: Entry = entry("text.font", "Font", Kind::Text { max_chars: 64 }, SP);

/// The sheet font: Barlow Semi Condensed, Regular and SemiBold, which
/// the app registers with CADCraft's font table at start (US3). A project with its own
/// `text.font` keeps it.
pub const SHEET_FONT: &str = ecad_model::palette::sheet_font::FAMILY;
/// The built-in font, now the first fallback where the sheet font is not registered (a build
/// or a test without it): drawn with when found, else the stroke font.
pub(crate) const FALLBACK_FONT: &str = "BIZ UDPGothic";

// --- The wire label height ----------------------------------------------------------

/// Stored in `settings.wiring.label_height`, never as a property. It is
/// the project field of the `wire_label` text style's height, not a registry entry of its own.
pub static WIRE_LABEL_HEIGHT: Entry = Entry {
    key: ecad_model::look::WIRE_LABEL_HEIGHT_KEY,
    label: "Wire label cap height",
    group: GROUP,

    kind: HEIGHT,
    slot: Slot::new(&crate::defaults::data::DATA),
    levels: SP,
    project: ProjectSource::Settings(wire_label_height),
};

// --- Shown ----------------------------------------------------------------------

pub static TAG_SHOW: Entry = entry("text.tag.show", "Tag shown", SHOW_RUN, SPGO);
/// The run rule for strip terminals, used where `text.tag.show` is not set.
pub(crate) static TAG_SHOW_STRIP: Entry = entry("text.tag.show.strip", "Strip terminal tag shown", SHOW_RUN, SP);
/// The run rule for PLC channels, used where `text.tag.show` is not set.
pub(crate) static TAG_SHOW_PLC: Entry = entry("text.tag.show.plc", "PLC channel tag shown", SHOW_RUN, SP);
pub(crate) static POINT_LABEL_SHOW: Entry = entry("text.point_label.show", "Connection point labels shown", SHOW, SPGO);
pub(crate) static DESCRIPTION_SHOW: Entry = entry("text.description.show", "Description shown", SHOW, SPGO);
pub(crate) static PART_NUMBER_SHOW: Entry = entry("text.part_number.show", "Part number shown", SHOW, SPGO);
pub(crate) static CONTINUATION_NAME_SHOW: Entry = entry("text.continuation_name.show", "Continuation name shown", SHOW, SPGO);
pub(crate) static REFERENCE_SHOW: Entry = entry("text.reference.show", "References shown", SHOW, SPG);
pub(crate) static CONTINUATION_CAPTION_SHOW: Entry = entry("text.continuation_caption.show", "Continuation captions shown", SHOW, SPG);
pub(crate) static LOCATION_SHOW: Entry = entry("text.location.show", "Location labels shown", SHOW, SPG);
/// Read for (conductor, page, project).
pub(crate) static WIRE_LABEL_SHOW: Entry = entry("text.wire_label.show", "Wire labels shown", SHOW, SPGO);

/// The lineweight of bold text drawn in the stroke-font fallback.
pub(crate) static BOLD_STROKE_WEIGHT: Entry =
    entry("text.bold_stroke_weight", "Bold lineweight in the stroke font", Kind::Length { min: 0.13, max: 2.11 }, SP);

/// Every entry of this file, in the order `defaults` lists them.
pub static ENTRIES: &[&Entry] = &[
    &FONT,
    &TAG_SHOW,
    &TAG_SHOW_STRIP,
    &TAG_SHOW_PLC,
    &POINT_LABEL_SHOW,
    &DESCRIPTION_SHOW,
    &PART_NUMBER_SHOW,
    &CONTINUATION_NAME_SHOW,
    &REFERENCE_SHOW,
    &CONTINUATION_CAPTION_SHOW,
    &LOCATION_SHOW,
    &WIRE_LABEL_SHOW,
    &BOLD_STROKE_WEIGHT,
];
