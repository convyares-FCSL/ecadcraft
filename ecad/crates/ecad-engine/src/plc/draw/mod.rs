//! The engine side of the PLC model: the card frames' text measured in CADCraft's
//! fonts, and the PLC sheets drawn.

pub mod card;
pub(crate) mod sheets;

use ecad_model::{Document, Op};

use crate::styles::draw::LOOK;
use ecad_electrical::plc::kept;

/// The kept PLC model ([`kept::Plc`]) with text measured in CADCraft's fonts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plc(pub kept::Plc);

impl std::ops::Deref for Plc {
    type Target = kept::Plc;

    fn deref(&self) -> &kept::Plc {
        &self.0
    }
}

impl std::ops::DerefMut for Plc {
    fn deref_mut(&mut self) -> &mut kept::Plc {
        &mut self.0
    }
}

impl Plc {
    /// The whole model, from scratch (open, reload, first show): [`kept::Plc::build_with`].
    pub fn build(doc: &Document) -> Self {
        Plc(kept::Plc::build_with(doc, &LOOK))
    }

    /// Bring the model in line with `ops`: [`kept::Plc::update_with`]. Returns whether anything
    /// changed.
    pub fn update(&mut self, doc: &Document, ops: &[Op]) -> bool {
        self.0.update_with(doc, &ecad_model::document::effect::Effects::of(ops), &LOOK)
    }
}
