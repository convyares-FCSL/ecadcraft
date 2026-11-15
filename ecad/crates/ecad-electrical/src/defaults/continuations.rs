//! Brief 049's defaults (continuation cross-references, design 049 §7): area `continuation`.
//!
//! Read them through [`crate::continuations`], not by key. The caption text's size, weight, font
//! and visibility are lane T's text kind `continuation_caption` (`defaults/text.rs`).

use super::{Entry, Kind, Level, ProjectSource, Value};

const GROUP: &str = "Continuations";
const BRIEF: &str = "049";
/// Standard and project.
const SP: &[Level] = &[Level::Standard, Level::Project];

const fn entry(key: &'static str, label: &'static str, kind: Kind, builtin: Value<'static>) -> Entry {
    Entry { key, label, group: GROUP, brief: BRIEF, kind, builtin, levels: SP, project: ProjectSource::Property }
}

/// How many other ends an arrow lists before `+n`.
pub static PLACES_MAX: Entry =
    entry("continuation.places.max", "Other ends shown before +n", Kind::Number { min: 1.0, max: 10.0 }, Value::Number(3.0));
/// The gap between an arrow's tip and its reference text.
pub static REFERENCE_GAP: Entry =
    entry("continuation.reference.gap", "Gap from arrow tip to reference", Kind::Length { min: 0.0, max: 10.0 }, Value::Length(1.0));
/// The most space between neighbouring arrows under one caption.
pub static CAPTION_RUN_GAP: Entry =
    entry("continuation.caption.run_gap", "Most space between arrows under one caption", Kind::Length { min: 1.0, max: 200.0 }, Value::Length(20.0));
/// The gap between the farthest reference text and the caption bracket.
pub static CAPTION_GAP: Entry =
    entry("continuation.caption.gap", "Gap from references to caption bracket", Kind::Length { min: 0.0, max: 20.0 }, Value::Length(1.5));
/// The length of the ticks at the bracket's ends.
pub static CAPTION_TICK: Entry =
    entry("continuation.caption.tick", "Caption bracket end tick", Kind::Length { min: 0.0, max: 10.0 }, Value::Length(1.5));

/// Every entry of this file, in the order `defaults` lists them.
pub static ENTRIES: &[&Entry] = &[&PLACES_MAX, &REFERENCE_GAP, &CAPTION_RUN_GAP, &CAPTION_GAP, &CAPTION_TICK];
