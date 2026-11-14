//! Title blocks (brief 055, design 055): a frame style's block turned into frame parts, the
//! values its fields show, the built-in house block, and DXF import.
//!
//! # Semantics
//! - A page's frame is its resolved `print.frame.style` (`ecad_model::sheet::frame_style`). An own
//!   frame draws the block its record holds (FP1); the built-in `sheet` draws [`house::house`],
//!   computed for the page's frame width, so it fits any sheet.
//! - Fields show values read when the frame's input is worked out ([`crate::frame::FrameInput`]):
//!   project properties and the newest issued revision's fields only for the keys the block
//!   reads, so the frame's update map ([`crate::frame::touches`]) redraws it only when one of them
//!   changes ([`bind`]).
//! - Drawing never fails: items breaking a bound are skipped and counted ([`layout`]).
//!
//! This module records nothing; the commands are `crate::title_block_cmds`.

pub mod bind;
pub mod checks;
pub mod dxf;
pub mod house;
pub mod layout;

use std::sync::Arc;

use ecad_model::sheet::{self, Margins, SheetSpec};
use ecad_model::title_block::TitleBlockLayout;
use ecad_model::{Document, PageId, StyleRef};

use crate::frame::FrameText;

/// A page's block as drawn: the layout, whether it is the built-in's, and the frame style.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub frame: StyleRef,
    pub layout: Arc<TitleBlockLayout>,
    pub builtin: bool,
}

/// The width of the frame of `spec` with `margins`, in millimetres.
pub fn frame_width(spec: &SheetSpec, margins: &Margins) -> f64 {
    (f64::from(spec.width) - margins.left - margins.right).max(1.0)
}

/// The block of frame style `id` on `spec` with `margins`: its own when it has one, else the
/// house block computed for the frame's width (built-ins, and own frames without a body).
pub fn block_of(doc: &Document, id: &StyleRef, spec: &SheetSpec, margins: &Margins, text: &FrameText) -> Block {
    match sheet::frame_block(doc, id) {
        Some(l) => Block { frame: id.clone(), layout: Arc::new(l.clone()), builtin: false },
        None => Block { frame: id.clone(), layout: Arc::new(house::house(frame_width(spec, margins), text)), builtin: true },
    }
}

/// The block `page` shows.
pub fn page_block(doc: &Document, page: PageId) -> Block {
    let id = sheet::title_block_style(doc, page);
    block_of(doc, &id, &sheet::sheet_of(doc, page), &sheet::margins_of(doc, page), &FrameText::of(doc))
}

/// `A3`, or `594×420`: how a field bound to `print:sheet` names a sheet.
pub fn sheet_name(spec: &SheetSpec) -> String {
    spec.preset_name().map_or_else(|| format!("{}×{}", spec.width, spec.height), str::to_string)
}
