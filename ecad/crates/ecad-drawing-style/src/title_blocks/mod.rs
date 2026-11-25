//! The title block a frame draws: its own block, or the built-in house block ([`house`])
//! computed for the frame's width so it fits any sheet; and the title block commands
//! ([`commands`]). What fills a block's fields, and the checks, are
//! `ecad_electrical::title_blocks`.

pub mod commands;
pub mod house;

use std::sync::Arc;

use ecad_model::document::sheet::{self, Margins, SheetSpec};
use ecad_model::look::{FrameBlock, FrameText};
use ecad_model::{Document, StyleRef};

/// The block of frame style `id` on `spec` with `margins`: its own when it has one, else the
/// house block computed for the frame's width (built-ins, and own frames without a body).
pub(crate) fn block_of(doc: &Document, id: &StyleRef, spec: &SheetSpec, margins: &Margins, text: &FrameText) -> FrameBlock {
    match sheet::frame_block(doc, id) {
        Some(l) => FrameBlock { frame: id.clone(), layout: Arc::new(l.clone()), builtin: false },
        None => FrameBlock { frame: id.clone(), layout: Arc::new(house::house(sheet::frame_width(spec, margins), text)), builtin: true },
    }
}
