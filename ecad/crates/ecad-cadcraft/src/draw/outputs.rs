//! The engine side of the report models (design 065 §3): the kept [`kept::Outputs`] with the PLC
//! model's text measured in CADCraft's fonts (cut 10, option F2).

use ecad_model::{Document, Op};

use crate::checks::Check;
use crate::draw::text_style::EngineMetrics;
use crate::outputs::{Scope, kept};

/// The kept report models ([`kept::Outputs`]) with text measured in CADCraft's fonts.
#[derive(Debug, Clone, Default)]
pub struct Outputs(pub kept::Outputs);

impl std::ops::Deref for Outputs {
    type Target = kept::Outputs;

    fn deref(&self) -> &kept::Outputs {
        &self.0
    }
}

impl std::ops::DerefMut for Outputs {
    fn deref_mut(&mut self) -> &mut kept::Outputs {
        &mut self.0
    }
}

impl Outputs {
    /// Every model, from scratch (open, reload, first show): [`kept::Outputs::build_with`].
    pub fn build(doc: &Document) -> Self {
        Outputs(kept::Outputs::build_with(doc, &EngineMetrics))
    }

    /// Recompute the rows in `scope` on `doc`, the state after `ops`: [`kept::Outputs::update_with`].
    pub fn update(&mut self, doc: &Document, ops: &[Op], scope: Scope) {
        self.0.update_with(doc, ops, scope, &EngineMetrics);
    }

    /// Verify mode: the kept strips and PLC model against a rebuild on `doc`
    /// ([`kept::Outputs::verify_strips_with`]).
    pub fn verify_strips(&mut self, doc: &Document) -> Option<String> {
        self.0.verify_strips_with(doc, &EngineMetrics)
    }

    /// [`kept::Outputs::checks`] (also as a path, `Outputs::checks`).
    pub fn checks(&self) -> Vec<Check> {
        self.0.checks()
    }

    /// [`kept::Outputs::check_counts`] (also as a path, `Outputs::check_counts`).
    pub fn check_counts(&self) -> (usize, usize, usize) {
        self.0.check_counts()
    }

    /// [`kept::Outputs::device_count`] (also as a path, `Outputs::device_count`).
    pub fn device_count(&self) -> usize {
        self.0.device_count()
    }

    /// [`kept::Outputs::bom_sheets`] (also as a path, `Outputs::bom_sheets`).
    pub fn bom_sheets(&self) -> usize {
        self.0.bom_sheets()
    }
}

/// The kept models inside `o` (for the rules, which take [`kept::Outputs`]).
pub fn kept_of(o: Option<&Outputs>) -> Option<&kept::Outputs> {
    o.map(|o| &o.0)
}
