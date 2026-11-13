//! Lane T's defaults (031, text styles, show and hide, placement): design 031 §3.2 and §3.5.
//!
//! Read them through [`crate::text_style`] (`text_style`, `text_shown`), not by key.
//!
//! Brief 050a (FP4): a kind's cap height, weight and font are no longer registry entries; they are
//! fields of its text style (`crate::styles::text`). 031's project properties
//! `text.<kind>.height`, `.weight` and `.font` are no longer read (Checks: `ignored-old-setting`).

use super::{Entry, Kind, Level, ProjectSource, Value};

const GROUP: &str = "Text";
const BRIEF: &str = "031";
/// Standard and project.
const SP: &[Level] = &[Level::Standard, Level::Project];
/// Standard, project, page.
const SPG: &[Level] = &[Level::Standard, Level::Project, Level::Page];
/// Standard, project, page, object (device, symbol, conductor).
const SPGO: &[Level] = &[Level::Standard, Level::Project, Level::Page, Level::Object];
/// The heights a project may set: cap heights (baseline to the top of a capital), millimetres
/// (brief 040, owner answer 9; CADCraft draws a text height as its cap height).
const HEIGHT: Kind = Kind::Length { min: 0.5, max: 10.0 };
const SHOW: Kind = Kind::Choice(&["on", "off"]);
const SHOW_RUN: Kind = Kind::Choice(&["on", "off", "first"]);

const fn entry(key: &'static str, label: &'static str, kind: Kind, builtin: Value<'static>, levels: &'static [Level]) -> Entry {
    Entry { key, label, group: GROUP, brief: BRIEF, kind, builtin, levels, project: ProjectSource::Property }
}

/// The wire label height as the project stores it (`settings.wiring.label_height`), or `None`
/// while it is the default.
pub(crate) fn wire_label_height(s: &ecad_model::ProjectSettings) -> Option<Value<'_>> {
    let h = s.wiring.label_height;
    ((h - ecad_model::wiring::DEFAULT_LABEL_HEIGHT).abs() > f64::EPSILON).then_some(Value::Length(h))
}

// --- Font ------------------------------------------------------------------------------------

/// The project font family (§2.1). Missing fonts fall back (§2.2); the stored value never changes.
pub static FONT: Entry = entry("text.font", "Font", Kind::Text { max_chars: 64 }, Value::Text(SHEET_FONT), SP);

/// The sheet font (brief 040, spec §4 "Type"): Barlow Semi Condensed, Regular and SemiBold, which
/// the app registers with CADCraft's font table at start (US3). A project with its own
/// `text.font` keeps it.
pub const SHEET_FONT: &str = ecad_tokens::sheet_font::FAMILY;
/// 031's built-in font, now the first fallback where the sheet font is not registered (a build
/// or a test without it): drawn with when found, else the stroke font, as 031 did.
pub const FALLBACK_FONT: &str = "BIZ UDPGothic";

// --- The wire label height (§3.4) ----------------------------------------------------------

/// Stored in `settings.wiring.label_height` (§3.4), never as a property. Since brief 050a it is
/// the project field of the `wire_label` text style's height, not a registry entry of its own.
pub static WIRE_LABEL_HEIGHT: Entry = Entry {
    key: "text.wire_label.height",
    label: "Wire label cap height",
    group: GROUP,
    brief: BRIEF,
    kind: HEIGHT,
    builtin: Value::Length(ecad_model::wiring::DEFAULT_LABEL_HEIGHT),
    levels: SP,
    project: ProjectSource::Settings(wire_label_height),
};

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
