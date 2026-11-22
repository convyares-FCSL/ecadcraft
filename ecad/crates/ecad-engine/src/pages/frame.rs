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

use cadcraft_engine::doc::{Common, Entity, EntityKind, Handle, Lineweight, color::Color};
use ecad_model::document::book::Book;
use ecad_model::document::sheet::unit_scale;
use ecad_model::{Document, PageId};

use crate::styles::draw::EngineMetrics;
use ecad_electrical::pages::frames;
pub use ecad_electrical::pages::frames::{
    CUSTOMER, DERIVED, DRAWING_NUMBER, FRAME_ACI, FRAME_WEIGHT, LABEL_ACI, LAYER, STATUS, TABLE_ROWS, THIN_WEIGHT, TITLE, TextDraw, fit, touches,
};

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
    crate::cables::draw::colour_rgb(&ecad_electrical::styles::frame::frame_draw(doc, page).colour, crate::cables::draw::projection_sheet())
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

/// How a page's frame and title block are drawn from what they show (owner note of 2026-11-08:
/// users will define their own borders and title blocks). Brief 055: the frame style's block
/// ([`frames::FrameInput::block`]) is the user-defined part; [`BuiltIn`] draws the border and that block.
pub trait FrameLayout: Send + Sync {
    /// Every line and text of the frame `input` describes, in model coordinates (the paper's
    /// bottom-left corner at the origin).
    fn parts(&self, input: &frames::FrameInput) -> Vec<Part>;
}

/// The frame drawn from its style: border, zones, centre marks and the block (design 055 §3).
#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltIn;

impl FrameLayout for BuiltIn {
    fn parts(&self, input: &frames::FrameInput) -> Vec<Part> {
        drawn(input).0
    }
}

/// Every line and text of the frame `input` describes.
pub fn parts(input: &frames::FrameInput) -> Vec<Part> {
    BuiltIn.parts(input)
}

/// [`parts`], and what drawing the block found (items skipped, fields cut).
pub fn drawn(input: &frames::FrameInput) -> (Vec<Part>, crate::title_blocks::layout::Drawn) {
    use crate::title_blocks::layout;
    let mut pen = layout::Pen::new(unit_scale(input.unit));
    layout::border(&mut pen, input);
    let l = &input.block.layout;
    let origin = match input.draw.block {
        ecad_electrical::styles::frame::BlockAt::BottomFull => (input.draw.margins.left, input.draw.margins.bottom),
        ecad_electrical::styles::frame::BlockAt::BottomRight => layout::block_origin(input, l),
    };
    let d = layout::block(&mut pen, input, l, origin);
    (pen.out, d)
}

/// [`frames::FrameText`] with this machine's fonts ([`EngineMetrics`]); it reads as the rules'
/// type ([`std::ops::Deref`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameText(pub frames::FrameText);

impl std::ops::Deref for FrameText {
    type Target = frames::FrameText;

    fn deref(&self) -> &frames::FrameText {
        &self.0
    }
}

impl FrameText {
    /// The looks of `doc`'s frame.
    pub fn of(doc: &Document) -> FrameText {
        FrameText(frames::FrameText::with(doc, &EngineMetrics))
    }
}

/// [`frames::FrameInput`] with this machine's fonts ([`EngineMetrics`]); it reads and writes as the
/// rules' type ([`std::ops::Deref`], [`std::ops::DerefMut`]).
#[derive(Debug, Clone, PartialEq)]
pub struct FrameInput(pub frames::FrameInput);

impl std::ops::Deref for FrameInput {
    type Target = frames::FrameInput;

    fn deref(&self) -> &frames::FrameInput {
        &self.0
    }
}

impl std::ops::DerefMut for FrameInput {
    fn deref_mut(&mut self) -> &mut frames::FrameInput {
        &mut self.0
    }
}

impl FrameInput {
    /// What `page`'s frame shows in `doc` (`book`: its book order), with this machine's fonts.
    pub fn of(doc: &Document, page: PageId, book: &Book) -> FrameInput {
        FrameInput(frames::FrameInput::with(doc, page, book, frames::FrameText::with(doc, &EngineMetrics)))
    }

    /// [`frames::FrameInput::with_pending`].
    pub fn with_pending(self, doc: &Document, pending: bool) -> FrameInput {
        FrameInput(self.0.with_pending(doc, pending))
    }
}
