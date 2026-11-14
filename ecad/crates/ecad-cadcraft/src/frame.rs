//! The sheet frame and title block of a page (brief 019, design 019 §3, §4): derived graphics on
//! the reserved, locked layer [`LAYER`], drawn by the projection and never stored.
//!
//! # Semantics
//! - [`FrameInput::of`] reads everything a page's frame shows: its sheet, the project and page
//!   properties, the newest issued revisions, the page's number, the next page's number and the
//!   page count. [`parts`] turns it into lines and texts.
//! - **Update map** (design 019 §9, `ecad/CLAUDE.md` "Derived views"): [`touches`] says which ops
//!   can change the frame of the page shown; the project keeps the last parts and recomputes them
//!   only after such an op, on open and when another page becomes active. Every other op leaves
//!   them as they are.
//!
//! This module has no UI and records nothing.

use std::collections::{BTreeMap, BTreeSet};

use cadcraft_engine::doc::{Common, Entity, EntityKind, Handle, Lineweight, color::Color};
use ecad_model::book::{self, Book, FUNCTION_KEY, LOCATION_KEY};
use ecad_model::sheet::{SheetSpec, unit_scale};
use ecad_model::{Document, LengthUnit, ObjectRef, Op, PageId};

use crate::text_style::{Look, TextKind, Weight};

/// The reserved layer of the frame (`ecad.frame`), locked so CADCraft's own commands leave it.
pub const LAYER: &str = "ECAD-FRAME";
/// `ecad.frame`: ACI 7.
pub const FRAME_ACI: u8 = 7;
/// `ecad.frame-label` and `ecad.sheet-edge`: ACI 8.
pub const LABEL_ACI: u8 = 8;
/// `ecad.frame-weight` (hundredths of a millimetre).
pub const FRAME_WEIGHT: u16 = 50;
/// `ecad.wire-weight`.
pub const THIN_WEIGHT: u16 = 25;
/// Refusal of an edit of the frame (design 019 §3.2).
pub const DERIVED: &str = "The sheet frame is derived; change it in Project settings or the page's Sheet.";
/// Most issued revisions the title block lists (owner answer Q12).
pub const TABLE_ROWS: usize = 7;
/// Project property keys the title block shows.
pub const TITLE: &str = "title";
pub const CUSTOMER: &str = "customer";
pub const DRAWING_NUMBER: &str = "drawing_number";
pub const STATUS: &str = "status";

/// The layer as the projection adds it.
pub fn layer() -> cadcraft_engine::doc::Layer {
    let mut l = cadcraft_engine::doc::Layer::new(LAYER);
    l.color = Color::Index(FRAME_ACI);
    l.locked = true;
    l
}

/// Take the sheet frame's entities out of the active document's selection (design 019 §3.2:
/// selecting the frame is refused; CADCraft selects objects on locked layers). Returns `true`
/// when the selection held nothing else, so the caller can say why it is empty. Cheap: reads the
/// selection only.
pub fn drop_from_selection(s: &mut cadcraft_engine::Session) -> bool {
    let sel = s.selection();
    if sel.is_empty() {
        return false;
    }
    let Ok(d) = s.doc() else { return false };
    let keep: Vec<Handle> = sel.iter().copied().filter(|h| d.entity(*h).is_none_or(|e| e.common.layer != LAYER)).collect();
    if keep.len() == sel.len() {
        return false;
    }
    let only_frame = keep.is_empty();
    s.set_selection(keep);
    only_frame
}

/// Everything one page's frame shows.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameInput {
    pub sheet: SheetSpec,
    pub unit: LengthUnit,
    pub project_title: String,
    pub customer: String,
    pub drawing_number: String,
    pub status: String,
    pub page_title: String,
    pub function: String,
    pub location: String,
    pub page: String,
    pub rev: String,
    /// The next page in book order, and its number.
    pub next_page: Option<PageId>,
    pub next: String,
    pub total: String,
    /// Newest issued first, at most [`TABLE_ROWS`]: each row's values in [`Self::rev_columns`]
    /// order (brief 033).
    pub revisions: Vec<Vec<String>>,
    /// The project's revision columns (design 033 §7).
    pub rev_columns: Vec<crate::revisions::Column>,
    /// Whether the drawing changed since the newest issue (design 033 §6): set by the project,
    /// which keeps the flag; `of` leaves it off.
    pub pending: bool,
    /// The frame's text looks, from lane T's getter (design 033 §1.2).
    pub text: FrameText,
    /// Brief 050a: the page's frame style (line weights, colour, grid labels).
    pub look: crate::styles::frame::FrameLook,
    /// Brief 055: the block the frame draws (its own, or the house block for this sheet).
    pub block: crate::title_block::Block,
    /// Brief 055: the project properties the block's fields read, by key (empty when unset): the
    /// block's bound key set (design 055 §10.1).
    pub project: BTreeMap<String, String>,
    /// Brief 055: the newest issued revision's fields the block's fields read (besides `rev`).
    pub newest: BTreeMap<String, String>,
}

/// A text kind's look where ECAD draws in paper millimetres (frame, generated pages): its height
/// in millimetres, its style name, and the lineweight of bold drawn in the stroke font.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextLook {
    pub mm: f64,
    pub style: &'static str,
    pub weight: Option<u16>,
}

impl TextLook {
    /// `kind`'s look in a project of length unit `unit`, from its resolved `look`.
    pub fn of(look: &Look, kind: TextKind, unit: LengthUnit) -> TextLook {
        let s = look.style(kind);
        let k = unit_scale(unit);
        let mm = if k.is_finite() && k > 0.0 { s.height / k } else { s.height };
        // Lane T's per-kind fonts decide whether bold is drawn as a heavier stroke (design 031
        // §2.2), as `Look::apply` does.
        let weight = match s.weight {
            Weight::Bold => look.fonts_for(kind).bold_lineweight,
            Weight::Regular => None,
        };
        TextLook { mm, style: s.style_name, weight }
    }
}

/// The frame's text looks (design 033 §1.2): grid labels, box captions, values and revision rows,
/// Project and Page title, the Page number box, the Rev box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameText {
    pub grid: TextLook,
    pub caption: TextLook,
    pub value: TextLook,
    pub main: TextLook,
    pub page: TextLook,
    pub rev: TextLook,
    /// Brief 055: the lineweight bold is drawn with in the title fonts, for a block's bold texts
    /// when the main style is not bold itself.
    pub bold_weight: Option<u16>,
}

impl FrameText {
    /// The looks of `doc`'s frame.
    pub fn of(doc: &Document) -> FrameText {
        let look = Look::new(doc);
        let u = doc.settings.length_unit;
        let t = |k| TextLook::of(&look, k, u);
        FrameText {
            grid: t(TextKind::Grid),
            caption: t(TextKind::TitleCaption),
            value: t(TextKind::TitleValue),
            main: t(TextKind::TitleMain),
            page: t(TextKind::TitlePage),
            rev: t(TextKind::TitleRev),
            bold_weight: look.fonts_for(TextKind::TitleMain).bold_lineweight,
        }
    }
}

impl FrameInput {
    /// What `page`'s frame shows in `doc` (`book`: its book order).
    pub fn of(doc: &Document, page: PageId, book: &Book) -> FrameInput {
        let prop = |k: &str| doc.properties.get(k).cloned().unwrap_or_default();
        let pprops = doc.pages.get(&page).map(|p| &p.properties);
        let inherited = |k: &str| pprops.and_then(|p| p.get(k)).cloned().unwrap_or_else(|| prop(k));
        let issued = crate::revisions::issued(doc);
        let rev_columns = crate::revisions::columns(doc);
        let next_page = book.next(page);
        // Checked there, so the frame and the references always use the same sheet (brief 055:
        // the frame's own sheet first).
        let sheet = ecad_model::sheet::sheet_of(doc, page);
        let text = FrameText::of(doc);
        let look = crate::styles::frame::frame_look(doc, page);
        let id = ecad_model::sheet::frame_style(doc, page);
        let block = crate::title_block::block_of(doc, &id, &sheet, &look.margins, &text);
        let (project, newest) = crate::title_block::bind::read(doc, &block.layout);
        // The rows the block's tables can show.
        let rows = block
            .layout
            .items
            .iter()
            .filter_map(|i| match i {
                ecad_model::title_block::BlockItem::RevTable { rows, .. } => Some(usize::from(*rows)),
                _ => None,
            })
            .max()
            .unwrap_or(0)
            .min(usize::from(ecad_model::title_block::MAX_TABLE_ROWS));
        FrameInput {
            sheet,
            unit: doc.settings.length_unit,
            project_title: prop(TITLE),
            customer: prop(CUSTOMER),
            drawing_number: prop(DRAWING_NUMBER),
            status: prop(STATUS),
            page_title: doc.pages.get(&page).map(|p| p.title.clone()).unwrap_or_default(),
            function: inherited(FUNCTION_KEY),
            location: inherited(LOCATION_KEY),
            page: book.number(doc, page),
            rev: crate::revisions::rev_box(doc, false),
            next_page,
            next: next_page.map_or_else(|| "-".to_string(), |n| book.number(doc, n)),
            total: book.order.len().to_string(),
            revisions: issued.iter().take(rows).map(|r| rev_columns.iter().map(|c| r.get(&c.field).to_string()).collect()).collect(),
            rev_columns,
            pending: false,
            text,
            look,
            block,
            project,
            newest,
        }
    }
}

impl FrameInput {
    /// This input with the project's pending flag (design 033 §6): the Rev box marked as the
    /// project's `revisions.pending` says.
    pub fn with_pending(mut self, doc: &Document, pending: bool) -> FrameInput {
        self.pending = pending;
        self.rev = crate::revisions::rev_box(doc, pending);
        self
    }

    /// Brief 055 (FP4): the keep-out strip at the bottom of the frame, mm: the block's height
    /// (the built-in's 40 mm), never more than the frame's height. As
    /// `ecad_model::sheet::keep_out`.
    pub fn keep_out(&self) -> f64 {
        let h = self.block.layout.height().unwrap_or(ecad_model::sheet::TITLE_BLOCK_MM);
        let m = self.look.margins;
        h.min((f64::from(self.sheet.height) - m.bottom - m.top).max(0.0))
    }
}

/// Whether `ops` can change the frame of `page`, whose frame
/// last showed `shown` (design 019 §9, the frame and title block rows). Every op not listed
/// changes nothing there.
pub fn touches(ops: &[Op], page: PageId, shown: &FrameInput) -> bool {
    // A rename or move is a RemovePage + AddPage pair of one page.
    let removed: BTreeSet<PageId> = ops.iter().filter_map(|o| if let Op::RemovePage { id, .. } = o { Some(*id) } else { None }).collect();
    let added: BTreeSet<PageId> = ops.iter().filter_map(|o| if let Op::AddPage { id, .. } = o { Some(*id) } else { None }).collect();
    let pair = |id: &PageId| removed.contains(id) && added.contains(id);
    ops.iter().any(|op| match op {
        Op::SetProjectSettings { before, after } => before.sheet != after.sheet || before.length_unit != after.length_unit,
        // Only what the title block shows: its fields, the defaults pages inherit, and issuing.
        // Brief 033 (design 033 §9.1): a text size or the font (lane T's `text.` keys).
        Op::SetProperty { object: ObjectRef::Project | ObjectRef::Standard, key, .. } if crate::text_style::is_reserved_key(key) => true,
        // Brief 050a: the frame style choice, and any frame or text style record (the frame's
        // look and its texts' styles).
        Op::SetProperty { object: ObjectRef::Project | ObjectRef::Standard, key, .. } if key == crate::defaults::styles::FRAME.key => true,
        Op::SetStyle { catalogue, .. } => catalogue == "frame" || catalogue == "text",
        Op::SetStandard { before, after } => before.is_none() != after.is_none(),
        // Brief 055 (design 055 §10.1): a project key the block's fields read (its bound key
        // set), the defaults pages inherit, and the revision keys; any other key redraws nothing.
        Op::SetProperty { object: ObjectRef::Project, key, .. } => {
            shown.project.contains_key(key.as_str())
                || [FUNCTION_KEY, LOCATION_KEY].contains(&key.as_str())
                // Brief 033 (design 033 §9.1).
                || key == crate::revisions::COLUMNS_KEY
                || key == crate::revisions::PENDING_KEY
                || (key.starts_with(crate::revisions::PREFIX) && key.ends_with(".issued"))
        }
        Op::SetProperty { object: ObjectRef::Page(p), key, .. } => {
            *p == page || key == book::FOLDER_KEY || (key == book::NUMBER_KEY && shown.next_page == Some(*p))
        }
        // A page renamed in place changes only its own title box; a page moved changes the book
        // order (Next, and the numbers of unnumbered pages).
        Op::AddPage { id, index, .. } | Op::RemovePage { id, index, .. } if pair(id) => {
            let moved = ops.iter().any(|o| match o {
                Op::AddPage { id: a, index: i, .. } | Op::RemovePage { id: a, index: i, .. } => a == id && i != index,
                _ => false,
            });
            *id == page || moved
        }
        Op::AddPage { .. } | Op::RemovePage { .. } => true,
        Op::AddFolder { .. } | Op::RemoveFolder { .. } | Op::ModifyFolder { .. } => true,
        Op::NameVersion { .. } | Op::UnnameVersion { .. } => true,
        _ => false,
    })
}

/// One part of a frame drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub kind: EntityKind,
    /// Hundredths of a millimetre, or `None` for ByLayer.
    pub weight: Option<u16>,
    /// `None`: ByLayer (ACI 7).
    pub aci: Option<u8>,
    /// Its layer: [`LAYER`], or a generated page's content layer (brief 024).
    pub layer: &'static str,
}

/// The entity of `part` with `handle`.
pub fn entity(part: &Part, handle: Handle) -> Entity {
    Entity {
        handle,
        common: Common {
            layer: part.layer.into(),
            lineweight: part.weight.map_or(Lineweight::ByLayer, Lineweight::Mm100),
            color: part.aci.map_or(Color::ByLayer, Color::Index),
            ..Common::default()
        },
        kind: part.kind.clone(),
    }
}

/// Brief 050a: the frame style's line colour of `page` as drawn, or `None` for ink.
pub fn line_colour(doc: &Document, page: PageId) -> Option<cadcraft_engine::doc::color::Rgb> {
    crate::styles::colour_rgb(&crate::styles::frame::frame_look(doc, page).colour, crate::look::projection_sheet())
}

/// [`entity`] in the frame style's line colour `rgb` (brief 050a): the frame's own lines take it;
/// texts and parts with a colour of their own keep theirs.
pub fn styled_entity(part: &Part, handle: Handle, rgb: Option<cadcraft_engine::doc::color::Rgb>) -> Entity {
    let mut e = entity(part, handle);
    if let Some(rgb) = rgb
        && part.layer == LAYER
        && part.aci.is_none()
        && !matches!(part.kind, EntityKind::Text(_))
    {
        e.common.color = Color::True(rgb);
    }
    e
}

/// Roughly how wide a character of the standard font is, per unit height.
const CHAR_WIDTH: f64 = 0.8;
/// The narrowest width factor a title-block text is squeezed to before it is cut.
const MIN_WIDTH_FACTOR: f64 = 0.6;

/// `value` fitted in `room` mm at `height` (design 019 §4): narrowed down to
/// [`MIN_WIDTH_FACTOR`], then cut with `…`.
pub fn fit(value: &str, height: f64, room: Option<f64>) -> (String, f64) {
    let Some(room) = room.filter(|r| *r > 0.0) else { return (value.to_string(), 1.0) };
    let n = value.chars().count() as f64;
    let wide = n * height * CHAR_WIDTH;
    if wide <= room {
        return (value.to_string(), 1.0);
    }
    let wf = room / wide;
    if wf >= MIN_WIDTH_FACTOR {
        return (value.to_string(), wf);
    }
    let keep = ((room / (height * CHAR_WIDTH * MIN_WIDTH_FACTOR)).floor() as usize).saturating_sub(1).max(1);
    let mut s: String = value.chars().take(keep).collect();
    s.push('…');
    (s, MIN_WIDTH_FACTOR)
}

/// How a page's frame and title block are drawn from what they show (owner note of 2026-11-08:
/// users will define their own borders and title blocks). Brief 055: the frame style's block
/// ([`FrameInput::block`]) is the user-defined part; [`BuiltIn`] draws the border and that block.
pub trait FrameLayout: Send + Sync {
    /// Every line and text of the frame `input` describes, in model coordinates (the paper's
    /// bottom-left corner at the origin).
    fn parts(&self, input: &FrameInput) -> Vec<Part>;
}

/// The frame drawn from its style: border, zones, centre marks and the block (design 055 §3).
#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltIn;

impl FrameLayout for BuiltIn {
    fn parts(&self, input: &FrameInput) -> Vec<Part> {
        drawn(input).0
    }
}

/// Every line and text of the frame `input` describes.
pub fn parts(input: &FrameInput) -> Vec<Part> {
    BuiltIn.parts(input)
}

/// [`parts`], and what drawing the block found (items skipped, fields cut).
pub fn drawn(input: &FrameInput) -> (Vec<Part>, crate::title_block::layout::Drawn) {
    use crate::title_block::layout;
    let mut pen = layout::Pen::new(unit_scale(input.unit));
    layout::border(&mut pen, input);
    let l = &input.block.layout;
    let origin = match input.look.block {
        crate::styles::frame::BlockAt::BottomFull => (input.look.margins.left, input.look.margins.bottom),
        crate::styles::frame::BlockAt::BottomRight => layout::block_origin(input, l),
    };
    let d = layout::block(&mut pen, input, l, origin);
    (pen.out, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_is_narrowed_then_cut() {
        assert_eq!(fit("abc", 2.5, Some(100.0)), ("abc".to_string(), 1.0));
        let (s, wf) = fit("abcdefghij", 2.5, Some(15.0));
        assert_eq!(s, "abcdefghij");
        assert!((wf - 0.75).abs() < 1e-9);
        let (s, wf) = fit(&"x".repeat(100), 2.5, Some(20.0));
        assert!(s.ends_with('…') && s.chars().count() < 20);
        assert!((wf - MIN_WIDTH_FACTOR).abs() < 1e-9);
    }
}
