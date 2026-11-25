//! The placement defaults of a symbol's texts: tag runs, the gap between a symbol and its tag
//! block, the line pitch, and how far texts keep from wires.
//!
//! Read them through [`crate::devices::text_place`], not by key.

use ecad_model::registry::data::Slot;
use ecad_model::registry::{Entry, Kind, Level, ProjectSource};

const GROUP: &str = "Text";
/// Standard and project.
const SP: &[Level] = &[Level::Standard, Level::Project];

const fn entry(key: &'static str, label: &'static str, kind: Kind, levels: &'static [Level]) -> Entry {
    Entry { key, label, group: GROUP, kind, slot: Slot::new(&crate::defaults::data::DATA), levels, project: ProjectSource::Property }
}

/// Across-line tolerance of a run: centres within this of each other.
pub static RUN_ALIGN: Entry = entry("text.run.align", "Run alignment tolerance", Kind::Length { min: 0.0, max: 10.0 }, SP);
/// The largest gap between neighbouring boxes of a run.
pub static RUN_GAP: Entry = entry("text.run.gap", "Run gap", Kind::Length { min: 0.0, max: 100.0 }, SP);
/// Gap between a symbol's box and its tag block, × the tag height.
pub static TAG_GAP: Entry = entry("text.tag.gap", "Tag gap (× tag height)", Kind::Number { min: 0.0, max: 5.0 }, SP);
/// Line pitch of a tag block, × each line's height.
pub static LINE_PITCH: Entry = entry("text.line_pitch", "Line pitch (× height)", Kind::Number { min: 1.0, max: 3.0 }, SP);
/// How far a wire keeps texts away (each route segment is inflated by this).
pub static WIRE_CLEARANCE: Entry = entry("text.wire_clearance", "Text clearance from wires", Kind::Length { min: 0.0, max: 5.0 }, SP);
/// Every entry of this file, in the order `defaults` lists them.
pub static ENTRIES: &[&Entry] = &[&RUN_ALIGN, &RUN_GAP, &TAG_GAP, &LINE_PITCH, &WIRE_CLEARANCE];
