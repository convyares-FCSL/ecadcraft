//! Lane T's defaults (031, text styles, show and hide, placement): design 031 §3.2 and §3.5.
//!
//! Read them through [`crate::text_style`] (`text_style`, `text_shown`), not by key.

use super::{Entry, Kind, Level, ProjectSource, Value};

const GROUP: &str = "Text";
const BRIEF: &str = "031";
/// Standard and project.
const SP: &[Level] = &[Level::Standard, Level::Project];
/// Standard, project, page.
const SPG: &[Level] = &[Level::Standard, Level::Project, Level::Page];
/// Standard, project, page, object (device, symbol, conductor).
const SPGO: &[Level] = &[Level::Standard, Level::Project, Level::Page, Level::Object];
/// The heights a project may set, millimetres.
const HEIGHT: Kind = Kind::Length { min: 0.5, max: 10.0 };
const WEIGHT: Kind = Kind::Choice(&["regular", "bold"]);
const SHOW: Kind = Kind::Choice(&["on", "off"]);
const SHOW_RUN: Kind = Kind::Choice(&["on", "off", "first"]);

const fn entry(key: &'static str, label: &'static str, kind: Kind, builtin: Value<'static>, levels: &'static [Level]) -> Entry {
    Entry { key, label, group: GROUP, brief: BRIEF, kind, builtin, levels, project: ProjectSource::Property }
}
const fn height(key: &'static str, label: &'static str, mm: f64) -> Entry {
    entry(key, label, HEIGHT, Value::Length(mm), SP)
}
const fn regular(key: &'static str, label: &'static str) -> Entry {
    entry(key, label, WEIGHT, Value::Choice("regular"), SP)
}
/// A kind's own font (owner, 2026-11-12): its built-in is the project font's, and a kind with no
/// value of its own uses the project font (`text.font`).
const fn font(key: &'static str, label: &'static str) -> Entry {
    entry(key, label, Kind::Text { max_chars: 64 }, Value::Text("BIZ UDPGothic"), SP)
}
const fn bold(key: &'static str, label: &'static str) -> Entry {
    entry(key, label, WEIGHT, Value::Choice("bold"), SP)
}

/// The wire label height as the project stores it (`settings.wiring.label_height`), or `None`
/// while it is the default.
fn wire_label_height(s: &ecad_model::ProjectSettings) -> Option<Value<'_>> {
    let h = s.wiring.label_height;
    ((h - ecad_model::wiring::DEFAULT_LABEL_HEIGHT).abs() > f64::EPSILON).then_some(Value::Length(h))
}

// --- Font ------------------------------------------------------------------------------------

/// The project font family (§2.1). Missing fonts fall back (§2.2); the stored value never changes.
pub static FONT: Entry = entry("text.font", "Font", Kind::Text { max_chars: 64 }, Value::Text("BIZ UDPGothic"), SP);

// --- Heights (§3.2) --------------------------------------------------------------------------

pub static TAG_HEIGHT: Entry = height("text.tag.height", "Tag height", 2.5);
pub static POINT_LABEL_HEIGHT: Entry = height("text.point_label.height", "Connection point label height", 1.5);
pub static DESCRIPTION_HEIGHT: Entry = height("text.description.height", "Description height", 1.6);
pub static PART_NUMBER_HEIGHT: Entry = height("text.part_number.height", "Part number height", 1.6);
pub static CONTINUATION_NAME_HEIGHT: Entry = height("text.continuation_name.height", "Continuation name height", 2.0);
pub static REFERENCE_HEIGHT: Entry = height("text.reference.height", "Reference height", 2.0);
/// Stored in `settings.wiring.label_height` (§3.4), never as a property.
pub static WIRE_LABEL_HEIGHT: Entry = Entry {
    key: "text.wire_label.height",
    label: "Wire label height",
    group: GROUP,
    brief: BRIEF,
    kind: HEIGHT,
    builtin: Value::Length(ecad_model::wiring::DEFAULT_LABEL_HEIGHT),
    levels: SP,
    project: ProjectSource::Settings(wire_label_height),
};
pub static CABLE_TEXT_HEIGHT: Entry = height("text.cable_text.height", "Cable text height", 1.6);
pub static LOCATION_HEIGHT: Entry = height("text.location.height", "Location label height", 1.6);
pub static TITLE_VALUE_HEIGHT: Entry = height("text.title_value.height", "Title block value height", 2.5);
pub static TITLE_CAPTION_HEIGHT: Entry = height("text.title_caption.height", "Title block caption height", 1.8);
pub static TITLE_MAIN_HEIGHT: Entry = height("text.title_main.height", "Title height", 3.5);
pub static TITLE_PAGE_HEIGHT: Entry = height("text.title_page.height", "Title block page number height", 7.0);
pub static TITLE_REV_HEIGHT: Entry = height("text.title_rev.height", "Title block revision height", 5.0);
pub static GRID_HEIGHT: Entry = height("text.grid.height", "Grid label height", 3.0);
pub static TABLE_HEIGHT: Entry = height("text.table.height", "Table text height", 2.5);
pub static TABLE_HEADER_HEIGHT: Entry = height("text.table_header.height", "Table header height", 2.5);
pub static TABLE_HEADING_HEIGHT: Entry = height("text.table_heading.height", "Table heading height", 3.5);
pub static COVER_HEIGHT: Entry = height("text.cover.height", "Cover title height", 10.0);
/// The height a free text gets when its command gives none (a typed height is always the user's).
pub static FREE_HEIGHT: Entry = height("text.free.height", "Free text height", 2.5);

// --- Weights (§3.2) --------------------------------------------------------------------------

pub static TAG_WEIGHT: Entry = bold("text.tag.weight", "Tag weight");
pub static POINT_LABEL_WEIGHT: Entry = regular("text.point_label.weight", "Connection point label weight");
pub static DESCRIPTION_WEIGHT: Entry = regular("text.description.weight", "Description weight");
pub static PART_NUMBER_WEIGHT: Entry = regular("text.part_number.weight", "Part number weight");
pub static CONTINUATION_NAME_WEIGHT: Entry = regular("text.continuation_name.weight", "Continuation name weight");
pub static REFERENCE_WEIGHT: Entry = regular("text.reference.weight", "Reference weight");
pub static WIRE_LABEL_WEIGHT: Entry = regular("text.wire_label.weight", "Wire label weight");
pub static CABLE_TEXT_WEIGHT: Entry = regular("text.cable_text.weight", "Cable text weight");
pub static LOCATION_WEIGHT: Entry = regular("text.location.weight", "Location label weight");
pub static TITLE_VALUE_WEIGHT: Entry = regular("text.title_value.weight", "Title block value weight");
pub static TITLE_CAPTION_WEIGHT: Entry = regular("text.title_caption.weight", "Title block caption weight");
pub static TITLE_MAIN_WEIGHT: Entry = bold("text.title_main.weight", "Title weight");
pub static TITLE_PAGE_WEIGHT: Entry = regular("text.title_page.weight", "Title block page number weight");
pub static TITLE_REV_WEIGHT: Entry = regular("text.title_rev.weight", "Title block revision weight");
pub static GRID_WEIGHT: Entry = regular("text.grid.weight", "Grid label weight");
pub static TABLE_WEIGHT: Entry = regular("text.table.weight", "Table text weight");
pub static TABLE_HEADER_WEIGHT: Entry = bold("text.table_header.weight", "Table header weight");
pub static TABLE_HEADING_WEIGHT: Entry = regular("text.table_heading.weight", "Table heading weight");
pub static COVER_WEIGHT: Entry = regular("text.cover.weight", "Cover title weight");
pub static FREE_WEIGHT: Entry = regular("text.free.weight", "Free text weight");

// --- Fonts per kind (owner, 2026-11-12) -----------------------------------------------------

pub static TAG_FONT: Entry = font("text.tag.font", "Tag font");
pub static POINT_LABEL_FONT: Entry = font("text.point_label.font", "Connection point label font");
pub static DESCRIPTION_FONT: Entry = font("text.description.font", "Description font");
pub static PART_NUMBER_FONT: Entry = font("text.part_number.font", "Part number font");
pub static CONTINUATION_NAME_FONT: Entry = font("text.continuation_name.font", "Continuation name font");
pub static REFERENCE_FONT: Entry = font("text.reference.font", "Reference font");
pub static WIRE_LABEL_FONT: Entry = font("text.wire_label.font", "Wire label font");
pub static CABLE_TEXT_FONT: Entry = font("text.cable_text.font", "Cable text font");
pub static LOCATION_FONT: Entry = font("text.location.font", "Location label font");
pub static TITLE_VALUE_FONT: Entry = font("text.title_value.font", "Title block value font");
pub static TITLE_CAPTION_FONT: Entry = font("text.title_caption.font", "Title block caption font");
pub static TITLE_MAIN_FONT: Entry = font("text.title_main.font", "Title font");
pub static GRID_FONT: Entry = font("text.grid.font", "Grid label font");
pub static TABLE_FONT: Entry = font("text.table.font", "Table text font");
pub static TABLE_HEADER_FONT: Entry = font("text.table_header.font", "Table header font");
pub static FREE_FONT: Entry = font("text.free.font", "Free text font");
pub static TITLE_PAGE_FONT: Entry = font("text.title_page.font", "Title block page number font");
pub static TITLE_REV_FONT: Entry = font("text.title_rev.font", "Title block revision font");
pub static TABLE_HEADING_FONT: Entry = font("text.table_heading.font", "Table heading font");
pub static COVER_FONT: Entry = font("text.cover.font", "Cover title font");

// --- Shown (§5.1, §5.3) ----------------------------------------------------------------------

pub static TAG_SHOW: Entry = entry("text.tag.show", "Tag shown", SHOW_RUN, Value::Choice("on"), SPGO);
/// The run rule for strip terminals (§5.3), used where `text.tag.show` is not set.
pub static TAG_SHOW_STRIP: Entry = entry("text.tag.show.strip", "Strip terminal tag shown", SHOW_RUN, Value::Choice("first"), SP);
/// The run rule for PLC channels (§5.3), used where `text.tag.show` is not set.
pub static TAG_SHOW_PLC: Entry = entry("text.tag.show.plc", "PLC channel tag shown", SHOW_RUN, Value::Choice("first"), SP);
pub static POINT_LABEL_SHOW: Entry = entry("text.point_label.show", "Connection point labels shown", SHOW, Value::Choice("on"), SPGO);
pub static DESCRIPTION_SHOW: Entry = entry("text.description.show", "Description shown", SHOW, Value::Choice("off"), SPGO);
pub static PART_NUMBER_SHOW: Entry = entry("text.part_number.show", "Part number shown", SHOW, Value::Choice("off"), SPGO);
pub static CONTINUATION_NAME_SHOW: Entry = entry("text.continuation_name.show", "Continuation name shown", SHOW, Value::Choice("off"), SPGO);
pub static REFERENCE_SHOW: Entry = entry("text.reference.show", "References shown", SHOW, Value::Choice("on"), SPG);
pub static LOCATION_SHOW: Entry = entry("text.location.show", "Location labels shown", SHOW, Value::Choice("on"), SPG);
/// Read by lane W (conductor, page, project).
pub static WIRE_LABEL_SHOW: Entry = entry("text.wire_label.show", "Wire labels shown", SHOW, Value::Choice("on"), SPGO);

// --- Runs and placement (§5.3, §6) -----------------------------------------------------------

/// Across-line tolerance of a run: centres within this of each other.
pub static RUN_ALIGN: Entry = entry("text.run.align", "Run alignment tolerance", Kind::Length { min: 0.0, max: 10.0 }, Value::Length(1.0), SP);
/// The largest gap between neighbouring boxes of a run.
pub static RUN_GAP: Entry = entry("text.run.gap", "Run gap", Kind::Length { min: 0.0, max: 100.0 }, Value::Length(15.0), SP);
/// Gap between a symbol's box and its tag block, × the tag height.
pub static TAG_GAP: Entry = entry("text.tag.gap", "Tag gap (× tag height)", Kind::Number { min: 0.0, max: 5.0 }, Value::Number(0.6), SP);
/// Line pitch of a tag block, × each line's height.
pub static LINE_PITCH: Entry = entry("text.line_pitch", "Line pitch (× height)", Kind::Number { min: 1.0, max: 3.0 }, Value::Number(1.25), SP);
/// How far a wire keeps texts away (each route segment is inflated by this).
pub static WIRE_CLEARANCE: Entry =
    entry("text.wire_clearance", "Text clearance from wires", Kind::Length { min: 0.0, max: 5.0 }, Value::Length(0.3), SP);
/// The lineweight of bold text drawn in the stroke-font fallback (§2.2).
pub static BOLD_STROKE_WEIGHT: Entry =
    entry("text.bold_stroke_weight", "Bold lineweight in the stroke font", Kind::Length { min: 0.13, max: 2.11 }, Value::Length(0.35), SP);

/// Every entry of this file, in the order `defaults` lists them.
pub static ENTRIES: &[&Entry] = &[
    &FONT,
    &TAG_HEIGHT,
    &POINT_LABEL_HEIGHT,
    &DESCRIPTION_HEIGHT,
    &PART_NUMBER_HEIGHT,
    &CONTINUATION_NAME_HEIGHT,
    &REFERENCE_HEIGHT,
    &WIRE_LABEL_HEIGHT,
    &CABLE_TEXT_HEIGHT,
    &LOCATION_HEIGHT,
    &TITLE_VALUE_HEIGHT,
    &TITLE_CAPTION_HEIGHT,
    &TITLE_MAIN_HEIGHT,
    &TITLE_PAGE_HEIGHT,
    &TITLE_REV_HEIGHT,
    &GRID_HEIGHT,
    &TABLE_HEIGHT,
    &TABLE_HEADER_HEIGHT,
    &TABLE_HEADING_HEIGHT,
    &COVER_HEIGHT,
    &FREE_HEIGHT,
    &TAG_WEIGHT,
    &POINT_LABEL_WEIGHT,
    &DESCRIPTION_WEIGHT,
    &PART_NUMBER_WEIGHT,
    &CONTINUATION_NAME_WEIGHT,
    &REFERENCE_WEIGHT,
    &WIRE_LABEL_WEIGHT,
    &CABLE_TEXT_WEIGHT,
    &LOCATION_WEIGHT,
    &TITLE_VALUE_WEIGHT,
    &TITLE_CAPTION_WEIGHT,
    &TITLE_MAIN_WEIGHT,
    &TITLE_PAGE_WEIGHT,
    &TITLE_REV_WEIGHT,
    &GRID_WEIGHT,
    &TABLE_WEIGHT,
    &TABLE_HEADER_WEIGHT,
    &TABLE_HEADING_WEIGHT,
    &COVER_WEIGHT,
    &FREE_WEIGHT,
    &TAG_FONT,
    &POINT_LABEL_FONT,
    &DESCRIPTION_FONT,
    &PART_NUMBER_FONT,
    &CONTINUATION_NAME_FONT,
    &REFERENCE_FONT,
    &WIRE_LABEL_FONT,
    &CABLE_TEXT_FONT,
    &LOCATION_FONT,
    &TITLE_VALUE_FONT,
    &TITLE_CAPTION_FONT,
    &TITLE_MAIN_FONT,
    &GRID_FONT,
    &TABLE_FONT,
    &TABLE_HEADER_FONT,
    &FREE_FONT,
    &TITLE_PAGE_FONT,
    &TITLE_REV_FONT,
    &TABLE_HEADING_FONT,
    &COVER_FONT,
    &TAG_SHOW,
    &TAG_SHOW_STRIP,
    &TAG_SHOW_PLC,
    &POINT_LABEL_SHOW,
    &DESCRIPTION_SHOW,
    &PART_NUMBER_SHOW,
    &CONTINUATION_NAME_SHOW,
    &REFERENCE_SHOW,
    &LOCATION_SHOW,
    &WIRE_LABEL_SHOW,
    &RUN_ALIGN,
    &RUN_GAP,
    &TAG_GAP,
    &LINE_PITCH,
    &WIRE_CLEARANCE,
    &BOLD_STROKE_WEIGHT,
];
