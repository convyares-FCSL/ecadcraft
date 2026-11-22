//! The title block and frame style catalogue (`frame`, design 050 §4.3; brief 055, design 055
//! §1.4).
//!
//! The built-in `sheet` ("Schematic sheet") is the house block (brief 055, FP3): the frame at
//! 0.5 mm, inner lines and grid at 0.25 mm, in ink, with grid labels on all four sides and centre
//! marks. Its title block is computed for the page's sheet (`crate::title_blocks::house`). An own
//! frame carries its block as the record's body (FP1) and may set the sheet it is drawn for, and
//! its margins (FP2): `ecad_model::document::sheet` reads those, so every page, reference and zone agrees.
//! The title block's texts keep their own text-style choices.

use ecad_model::document::sheet::{Margins, parse_frame_sheet, parse_margins};
use ecad_model::objects::styles::FrameStyle;
use ecad_model::objects::styles::frame::{FrameMargins, FrameSheet};
use ecad_model::objects::styles::text::StyleColour;
use ecad_model::geometry::units::Mm;
use ecad_model::{Document, PageId, StyleRef, Word};

use crate::styles::{Builtin, Catalogue, Field};
use crate::CmdError;
use crate::defaults::{Kind, Slot, Value};

/// The `sheet` field's word for "the page's own sheet".
pub const PAGE_SHEET: &str = "page";

fn sheet_ok(s: &str) -> bool {
    s == PAGE_SHEET || parse_frame_sheet(s).is_some()
}

fn margins_ok(s: &str) -> bool {
    parse_margins(s).is_some()
}

static FIELDS: [Field; 8] = [
    Field { key: "border_weight", label: "Border weight", kind: Kind::Length { min: 0.13, max: 2.11 }, slot: Slot::new(), over: None },
    Field { key: "inner_weight", label: "Inner line weight", kind: Kind::Length { min: 0.13, max: 2.11 }, slot: Slot::new(), over: None },
    Field { key: "line_colour", label: "Line colour", kind: Kind::Parsed { max_chars: 16, parse: crate::styles::is_colour }, slot: Slot::new(), over: None },
    Field { key: "grid_labels", label: "Grid labels", kind: Kind::Flag, slot: Slot::new(), over: None },
    // Brief 055 (FP2): the sheet the frame is drawn for (`page`: the page's own), its margins,
    // its centre marks and where its block sits. 050a's `title_block` choice is retired (FP3): a
    // stored value is listed as unknown.
    Field { key: "sheet", label: "Sheet", kind: Kind::Parsed { max_chars: 48, parse: sheet_ok }, slot: Slot::new(), over: None },
    Field { key: "margins", label: "Margins", kind: Kind::Parsed { max_chars: 48, parse: margins_ok }, slot: Slot::new(), over: None },
    Field { key: "centre_marks", label: "Centre marks", kind: Kind::Flag, slot: Slot::new(), over: None },
    Field { key: "block", label: "Block position", kind: Kind::Choice(<BlockAt as ecad_model::Word>::WORDS), slot: Slot::new(), over: None },
];

static BUILTINS: [Builtin; 1] = [Builtin { id: "sheet", name: "Schematic sheet", slot: Slot::new() }];

fn choices() -> Vec<&'static crate::defaults::Entry> {
    vec![&crate::defaults::styles::FRAME]
}

/// The frame style catalogue.
pub static CATALOGUE: Catalogue = Catalogue {
    catalogue: ecad_model::StyleCatalogue::Frame,
    plural: "Frames",
    one: "Frame style",
    fields: &FIELDS,
    builtins: &BUILTINS,
    choices,
    project_field: None,
};

pub use ecad_model::objects::styles::frame::BlockAt;

/// A page frame's resolved look.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameDraw {
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
    /// Brief 055: the frame's distance from the paper edge (as `ecad_model::document::sheet::margins_of`).
    pub margins: Margins,
    /// Brief 055 (overseer ruling): zone ticks and labels drawn; off when the page type turns
    /// zones off, whatever the frame says.
    pub zones: bool,
}

impl Default for FrameDraw {
    fn default() -> Self {
        FrameDraw {
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

/// The frame look of `page`: its frame (`ecad_model::document::sheet::frame_style`, the one resolver: page,
/// page type, project, standard, built-in), with zones off when its page type turns them off.
pub fn frame_draw(doc: &Document, page: PageId) -> FrameDraw {
    let id = ecad_model::document::sheet::frame_style(doc, page);
    let mut look = draw_of(doc, &id);
    look.zones = ecad_model::document::sheet::page_type_zones(doc, page).unwrap_or(true);
    look
}

/// The look of frame style `id`.
pub fn draw_of(doc: &Document, id: &StyleRef) -> FrameDraw {
    let f = |k: &str| crate::styles::field_of(doc, ecad_model::StyleCatalogue::Frame, id, k, None);
    FrameDraw {
        border: weight(f("border_weight").map_or(0.5, |r| r.number())),
        inner: weight(f("inner_weight").map_or(0.25, |r| r.number())),
        colour: f("line_colour").map_or_else(|| "none".to_string(), |r| r.text()),
        grid_labels: f("grid_labels").is_none_or(|r| ecad_model::naming::words::flag(r.word()) != Some(false)),
        centre_marks: f("centre_marks").is_none_or(|r| ecad_model::naming::words::flag(r.word()) != Some(false)),
        // Own frames are drawn for one sheet, as made (design 055 §1.4).
        block: if id.is_builtin() && f("block").and_then(|r| <BlockAt as ecad_model::Word>::parse(r.word())) != Some(BlockAt::BottomRight) {
            BlockAt::BottomFull
        } else {
            BlockAt::BottomRight
        },
        margins: f("margins").and_then(|r| parse_margins(&r.text())).unwrap_or_default(),
        zones: true,
    }
}

/// The value of field `key` that `f` sets, if it sets one.
pub fn get<'a>(f: &'a FrameStyle, key: &str) -> Option<Value<'a>> {
    match key {
        "border_weight" => f.border_weight.map(|v| Value::Length(v.0)),
        "inner_weight" => f.inner_weight.map(|v| Value::Length(v.0)),
        "line_colour" => f.line_colour.map(Value::Colour),
        "grid_labels" => f.grid_labels.map(Value::Flag),
        "sheet" => f.sheet.map(|v| match v {
            FrameSheet::Page => Value::Text(PAGE_SHEET),
            FrameSheet::Sheet(s) => Value::Sheet(s),
        }),
        "margins" => f.margins.map(|m| Value::Margins(m.0)),
        "centre_marks" => f.centre_marks.map(Value::Flag),
        "block" => f.block.map(|v| Value::Choice(v.word())),
        _ => None,
    }
}

/// Set (`Some`) or clear (`None`) field `key` of `f` from its text, as the `style` command
/// checked it.
///
/// # Errors
/// An unknown field, or text the field does not take, in words.
pub fn set(f: &mut FrameStyle, key: &str, value: Option<&str>) -> Result<(), CmdError> {
    let bad = |what: &str| CmdError::refused(format!("{key} takes {what}, not {:?}.", value.unwrap_or_default()));
    let mm = |v: &str| v.trim().parse::<f64>().ok().filter(|x| x.is_finite()).map(Mm).ok_or_else(|| bad("a length"));
    let flag = |v: &str| ecad_model::naming::words::flag(v).ok_or_else(|| bad("yes or no"));
    match (key, value) {
        ("border_weight", None) => f.border_weight = None,
        ("inner_weight", None) => f.inner_weight = None,
        ("line_colour", None) => f.line_colour = None,
        ("grid_labels", None) => f.grid_labels = None,
        ("sheet", None) => f.sheet = None,
        ("margins", None) => f.margins = None,
        ("centre_marks", None) => f.centre_marks = None,
        ("block", None) => f.block = None,
        ("border_weight", Some(v)) => f.border_weight = Some(mm(v)?),
        ("inner_weight", Some(v)) => f.inner_weight = Some(mm(v)?),
        ("line_colour", Some(v)) => f.line_colour = Some(StyleColour::parse(v).ok_or_else(|| bad("none, a colour code or #rrggbb"))?),
        ("grid_labels", Some(v)) => f.grid_labels = Some(flag(v)?),
        ("sheet", Some(v)) => f.sheet = Some(FrameSheet::parse(v).ok_or_else(|| bad("page or a sheet"))?),
        ("margins", Some(v)) => f.margins = Some(FrameMargins(parse_margins(v).ok_or_else(|| bad("four margins l,r,b,t"))?)),
        ("centre_marks", Some(v)) => f.centre_marks = Some(flag(v)?),
        ("block", Some(v)) => f.block = Some(Word::parse(v).ok_or_else(|| bad("bottom_full or bottom_right"))?),
        _ => return Err(CmdError::refused(format!("Frames have no field {key:?}."))),
    }
    Ok(())
}
