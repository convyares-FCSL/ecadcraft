//! Title blocks: a frame style's block turned into frame parts, the
//! values its fields show, the built-in house block, and DXF import.
//!
//! # Semantics
//! - A page's frame is its resolved `print.frame.style` (`ecad_model::document::sheet::frame_style`). An own
//!   frame draws the block its record holds; the built-in `sheet` draws [`house::house`],
//!   computed for the page's frame width, so it fits any sheet.
//! - Fields show values read when the frame's input is worked out ([`crate::pages::frames::FrameInput`]):
//!   project properties and the newest issued revision's fields only for the keys the block
//!   reads, so the frame's update map ([`crate::pages::frames::touches`]) redraws it only when one of them
//!   changes ([`bind`]).
//! - Drawing never fails: items breaking a bound are skipped and counted (`layout`).
//!
//! This module records nothing; the commands are `ecad_engine::title_blocks::commands`.

pub mod bind;
pub mod checks;
pub mod commands;
pub mod house;

use std::sync::Arc;

use ecad_model::document::sheet::{self, Margins, SheetSpec};
use ecad_model::objects::title_block::TitleBlockLayout;
use ecad_model::{Document, PageId, StyleRef};

use ecad_model::look::{FrameBlock, FrameText};

/// Inset of a caption and of a value from its box's edge, mm.
pub const INSET_MM: f64 = 1.0;

/// The block of frame style `id` on `spec` with `margins`: its own when it has one, else the
/// house block computed for the frame's width (built-ins, and own frames without a body).
pub(crate) fn block_of(doc: &Document, id: &StyleRef, spec: &SheetSpec, margins: &Margins, text: &FrameText) -> FrameBlock {
    match sheet::frame_block(doc, id) {
        Some(l) => FrameBlock { frame: id.clone(), layout: Arc::new(l.clone()), builtin: false },
        None => FrameBlock { frame: id.clone(), layout: Arc::new(house::house(sheet::frame_width(spec, margins), text)), builtin: true },
    }
}

/// What an op can change of the pages' grids and keep-out strips through their frames:
/// what the kept references and place indexes must follow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Reach {
    /// Not a frame op: the index's own rules apply.
    NotFrame,
    /// A frame op that changes no page's sheet, margins or block height (a weight, a colour, a
    /// field moved): nothing to follow.
    Unchanged,
    /// The pages whose grid or keep-out may change.
    Pages(std::collections::BTreeSet<PageId>),
    /// Any page (the project's or the standard's frame choice, a standard switched).
    Wide,
}

/// What a frame record sets that the grid and the keep-out read: its sheet, its margins and its
/// block's height.
type Geometry = (Option<ecad_model::objects::styles::frame::FrameSheet>, Option<ecad_model::objects::styles::frame::FrameMargins>, Option<u64>);

fn geometry(r: Option<&ecad_model::StyleRecord>) -> Option<Geometry> {
    let f = r?.body.frame()?;
    let h = f.title_block.as_ref().and_then(TitleBlockLayout::height).map(f64::to_bits);
    Some((f.sheet, f.margins, h))
}

/// What `effect` reaches through the frames (see [`Reach`]). `doc` is the state the effect applies
/// to or came from; callers that see both states call it with each. Bounded: a frame record's
/// change walks the pages once.
pub(crate) fn frame_reach_of(doc: &Document, effect: &ecad_model::document::effect::Effect<'_>) -> Reach {
    use ecad_model::ObjectRef;
    use ecad_model::document::effect::Effect;
    match effect {
        Effect::Prop { object: ObjectRef::Page(p), key, .. } if key.raw == sheet::FRAME_STYLE_KEY => Reach::Pages([*p].into()),
        Effect::Page { id, before: Some(before), after: Some(after), index_before: None, index_after: None }
            if before.title_block != after.title_block || before.page_type != after.page_type =>
        {
            Reach::Pages([*id].into())
        }
        Effect::Prop { object: ObjectRef::Project | ObjectRef::Standard, key, .. } if key.raw == sheet::FRAME_STYLE_KEY => Reach::Wide,
        Effect::Standard { before, after } if before != after => Reach::Wide,
        Effect::Style { catalogue, id, before, after, .. } if *catalogue == sheet::FRAME_CATALOGUE => {
            match (geometry(*before), geometry(*after)) {
                (Some(a), Some(b)) if a == b => Reach::Unchanged,
                // A frame made or deleted: a choice naming it starts or stops resolving.
                (None, _) | (_, None) => Reach::Wide,
                _ => Reach::Pages(
                    doc.page_order()
                        .iter()
                        .copied()
                        .filter(|p| sheet::frame_style(doc, *p) == **id || sheet::title_block_style(doc, *p) == **id)
                        .collect(),
                ),
            }
        }
        Effect::Page { .. }
        | Effect::Layer { .. }
        | Effect::Item { .. }
        | Effect::Device { .. }
        | Effect::DeviceDefinition { .. }
        | Effect::ConnectionPoint { .. }
        | Effect::Rep { .. }
        | Effect::Wire { .. }
        | Effect::Route { .. }
        | Effect::Cable { .. }
        | Effect::Prop { .. }
        | Effect::Definition { .. }
        | Effect::Settings { .. }
        | Effect::Version { .. }
        | Effect::Folder { .. }
        | Effect::Style { .. }
        | Effect::Standard { .. }
        | Effect::Project { .. }
        | Effect::Aid { .. }
        | Effect::LoosePart { .. }
        | Effect::DeviceLink { .. } => Reach::NotFrame,
    }
}

/// `A3`, or `594×420`: how a field bound to `print:sheet` names a sheet.
pub fn sheet_name(spec: &SheetSpec) -> String {
    spec.preset_name().map_or_else(|| format!("{}×{}", spec.width, spec.height), str::to_string)
}

/// The block area of a frame on `spec`: the frame's width and half its height (mm).
pub fn block_area(spec: &SheetSpec, margins: &ecad_model::document::sheet::Margins) -> (f64, f64) {
    let w = sheet::frame_width(spec, margins);
    let h = ((f64::from(spec.height) - margins.top - margins.bottom) / 2.0).min(ecad_model::objects::title_block::MAX_BLOCK_HEIGHT_MM);
    (w, h)
}
