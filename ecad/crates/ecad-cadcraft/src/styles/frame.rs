//! The title block and frame style catalogue (`frame`, design 050 §4.3).
//!
//! The built-in `sheet` ("Schematic sheet") is 019's look: the frame at 0.5 mm, inner lines and
//! grid at 0.25 mm, in ink, with grid labels. The sheet size and grid counts are the paper, not
//! the style, and stay in `settings.sheet` and a page's `sheet`. The title block's texts keep
//! their own text-style choices.

use ecad_model::{Document, PageId, StyleRef};

use super::{Builtin, Catalogue, Field};
use crate::defaults::{Kind, Scope, Value};

static FIELDS: &[Field] = &[
    Field { key: "border_weight", label: "Border weight", kind: Kind::Length { min: 0.13, max: 2.11 }, builtin: Value::Length(0.5), over: None },
    Field { key: "inner_weight", label: "Inner line weight", kind: Kind::Length { min: 0.13, max: 2.11 }, builtin: Value::Length(0.25), over: None },
    Field {
        key: "line_colour",
        label: "Line colour",
        kind: Kind::Parsed { max_chars: 16, parse: super::is_colour },
        builtin: Value::Text("none"),
        over: None,
    },
    Field { key: "grid_labels", label: "Grid labels", kind: Kind::Choice(&["on", "off"]), builtin: Value::Choice("on"), over: None },
    Field { key: "title_block", label: "Title block", kind: Kind::Choice(&["standard"]), builtin: Value::Choice("standard"), over: None },
];

static BUILTINS: &[Builtin] = &[Builtin { id: "sheet", name: "Schematic sheet", values: &[] }];

fn choices() -> Vec<&'static crate::defaults::Entry> {
    vec![&crate::defaults::styles::FRAME]
}

/// The frame style catalogue.
pub static CATALOGUE: Catalogue =
    Catalogue { word: "frame", plural: "Frames", one: "Frame style", fields: FIELDS, builtins: BUILTINS, choices, project_field: None };

/// A page frame's resolved look.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameLook {
    /// Hundredths of a millimetre.
    pub border: u16,
    pub inner: u16,
    /// `none` (ink), an IEC code or `#rrggbb`.
    pub colour: String,
    pub grid_labels: bool,
}

impl Default for FrameLook {
    fn default() -> Self {
        FrameLook { border: 50, inner: 25, colour: "none".into(), grid_labels: true }
    }
}

fn weight(mm: f64) -> u16 {
    (mm * 100.0).round().clamp(0.0, 211.0) as u16
}

/// The frame look of `page` (its own `print.frame.style`, else the project's, the standard's, the
/// built-in `sheet`).
pub fn frame_look(doc: &Document, page: PageId) -> FrameLook {
    let id = super::chosen(doc, &crate::defaults::styles::FRAME, Scope::page(page));
    look_of(doc, &id)
}

/// The look of frame style `id`.
pub fn look_of(doc: &Document, id: &StyleRef) -> FrameLook {
    let f = |k: &str| super::field_of(doc, "frame", id, k, None);
    FrameLook {
        border: weight(f("border_weight").map_or(0.5, |r| r.number())),
        inner: weight(f("inner_weight").map_or(0.25, |r| r.number())),
        colour: f("line_colour").map_or_else(|| "none".to_string(), |r| r.text()),
        grid_labels: f("grid_labels").is_none_or(|r| r.word() != "off"),
    }
}
