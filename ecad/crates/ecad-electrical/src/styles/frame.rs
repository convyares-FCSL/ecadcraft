//! The title block and frame style catalogue (`frame`, design 050 §4.3; brief 055, design 055
//! §1.4).
//!
//! The built-in `sheet` ("Schematic sheet") is the house block (brief 055, FP3): the frame at
//! 0.5 mm, inner lines and grid at 0.25 mm, in ink, with grid labels on all four sides and centre
//! marks. Its title block is computed for the page's sheet (`crate::title_block::house`). An own
//! frame carries its block as the record's body (FP1) and may set the sheet it is drawn for, and
//! its margins (FP2): `ecad_model::sheet` reads those, so every page, reference and zone agrees.
//! The title block's texts keep their own text-style choices.

use ecad_model::sheet::{Margins, parse_frame_sheet, parse_margins};
use ecad_model::{Document, PageId, StyleRef};

use super::{Builtin, Catalogue, Field};
use crate::defaults::{Kind, Value};

/// The `sheet` field's word for "the page's own sheet".
pub const PAGE_SHEET: &str = "page";

fn sheet_ok(s: &str) -> bool {
    s == PAGE_SHEET || parse_frame_sheet(s).is_some()
}

fn margins_ok(s: &str) -> bool {
    parse_margins(s).is_some()
}

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
    // Brief 055 (FP2): the sheet the frame is drawn for (`page`: the page's own), its margins,
    // its centre marks and where its block sits. 050a's `title_block` choice is retired (FP3): a
    // stored value is listed as unknown.
    Field { key: "sheet", label: "Sheet", kind: Kind::Parsed { max_chars: 48, parse: sheet_ok }, builtin: Value::Text(PAGE_SHEET), over: None },
    Field {
        key: "margins",
        label: "Margins",
        kind: Kind::Parsed { max_chars: 48, parse: margins_ok },
        builtin: Value::Text("10,10,10,7"),
        over: None,
    },
    Field { key: "centre_marks", label: "Centre marks", kind: Kind::Choice(&["on", "off"]), builtin: Value::Choice("on"), over: None },
    Field {
        key: "block",
        label: "Block position",
        kind: Kind::Choice(&["bottom_full", "bottom_right"]),
        builtin: Value::Choice("bottom_full"),
        over: None,
    },
];

static BUILTINS: &[Builtin] = &[Builtin { id: "sheet", name: "Schematic sheet", values: &[] }];

fn choices() -> Vec<&'static crate::defaults::Entry> {
    vec![&crate::defaults::styles::FRAME]
}

/// The frame style catalogue.
pub static CATALOGUE: Catalogue =
    Catalogue { word: "frame", plural: "Frames", one: "Frame style", fields: FIELDS, builtins: BUILTINS, choices, project_field: None };

/// Where a frame's block sits (design 055 §1.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlockAt {
    /// The block's left part stretches to the frame's left edge (built-ins).
    #[default]
    BottomFull,
    /// The block keeps its width against the frame's bottom-right corner.
    BottomRight,
}

/// A page frame's resolved look.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameLook {
    /// Hundredths of a millimetre.
    pub border: u16,
    pub inner: u16,
    /// `none` (ink), an IEC code or `#rrggbb`.
    pub colour: String,
    pub grid_labels: bool,
    /// Brief 055: a short mark at the middle of each side.
    pub centre_marks: bool,
    /// Brief 055: where the block sits.
    pub block: BlockAt,
    /// Brief 055: the frame's distance from the paper edge (as `ecad_model::sheet::margins_of`).
    pub margins: Margins,
    /// Brief 055 (overseer ruling): zone ticks and labels drawn; off when the page type turns
    /// zones off, whatever the frame says.
    pub zones: bool,
}

impl Default for FrameLook {
    fn default() -> Self {
        FrameLook {
            border: 50,
            inner: 25,
            colour: "none".into(),
            grid_labels: true,
            centre_marks: true,
            block: BlockAt::BottomFull,
            margins: Margins::default(),
            zones: true,
        }
    }
}

fn weight(mm: f64) -> u16 {
    (mm * 100.0).round().clamp(0.0, 211.0) as u16
}

/// The frame look of `page`: its frame (`ecad_model::sheet::frame_style`, the one resolver: page,
/// page type, project, standard, built-in), with zones off when its page type turns them off.
pub fn frame_look(doc: &Document, page: PageId) -> FrameLook {
    let id = ecad_model::sheet::frame_style(doc, page);
    let mut look = look_of(doc, &id);
    look.zones = ecad_model::sheet::page_type_zones(doc, page).unwrap_or(true);
    look
}

/// The look of frame style `id`.
pub fn look_of(doc: &Document, id: &StyleRef) -> FrameLook {
    let f = |k: &str| super::field_of(doc, "frame", id, k, None);
    FrameLook {
        border: weight(f("border_weight").map_or(0.5, |r| r.number())),
        inner: weight(f("inner_weight").map_or(0.25, |r| r.number())),
        colour: f("line_colour").map_or_else(|| "none".to_string(), |r| r.text()),
        grid_labels: f("grid_labels").is_none_or(|r| r.word() != "off"),
        centre_marks: f("centre_marks").is_none_or(|r| r.word() != "off"),
        // Own frames are drawn for one sheet, as made (design 055 §1.4).
        block: if id.is_builtin() && f("block").is_none_or(|r| r.word() != "bottom_right") { BlockAt::BottomFull } else { BlockAt::BottomRight },
        margins: f("margins").and_then(|r| parse_margins(&r.text())).unwrap_or_default(),
        zones: true,
    }
}
