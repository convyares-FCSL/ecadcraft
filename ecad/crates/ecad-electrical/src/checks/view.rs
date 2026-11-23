//! The Checks list's view (`ChecksView`): the kept [`Checks`], built on first
//! show and then kept from each commit's effects.
//!
//! # Update map
//! Before the commit, on the state before, and after it, on the state after: the subjects each
//! effect names ([`crate::checks::Scope::note`]: nets, wires, routes, connection points, devices,
//! cables, the rating results, the style, title block and project rows, and whether the pages
//! with checks off may change). The update re-checks what was noted ([`Checks::update`]); a
//! numbering or potentials setting, or a change to the pages with checks off, rebuilds the list.
//! The cover's rows follow the cover when the generated pages' view reports it changed
//! ([`ChecksView::set_cover`]). The outcome is empty: the Checks panel reads the list's revision.
//! Every other effect changes nothing. Until first show the view is not kept and notes nothing.

use ecad_model::Document;
use ecad_model::document::effect::{AreaSet, Effect};
use ecad_model::document::view::{DerivedView, NoteCx, Outcome, ViewScope};

use crate::checks::Scope;
use crate::checks::kept::Checks;
use crate::styles::text_style::fonts::TextMetrics;
use crate::views::Inputs;

/// The kept Checks list, with the tolerance it was built with.
#[derive(Debug)]
pub struct ChecksView {
    model: Checks,
    tol: f64,
}

impl ChecksView {
    /// The whole list, from scratch (first show), with tolerance `tol` and text measured in `m`.
    pub fn build(doc: &Document, tol: f64, m: &dyn TextMetrics) -> Self {
        ChecksView { model: Checks::build_with(doc, tol, m), tol }
    }

    /// The kept list.
    pub fn model(&self) -> &Checks {
        &self.model
    }

    /// The cover's rows again: the cover changed, or a cover page came or went.
    pub fn set_cover(&mut self, doc: &Document) {
        self.model.set_cover(doc);
    }

    /// The end of the Before phase: the view was kept then.
    pub fn close_before(&self, scope: &mut ChecksScope) {
        scope.begun = true;
    }
}

/// What a commit could change in the list: noted on both states.
#[derive(Debug, Default)]
pub struct ChecksScope {
    scope: Scope,
    /// The Before phase was closed ([`ChecksView::close_before`]): the view was kept then.
    begun: bool,
}

impl ViewScope for ChecksScope {
    fn is_empty(&self) -> bool {
        !self.begun
    }

    /// A numbering or potentials setting, or the pages with checks off: the list is built again.
    fn is_all(&self) -> bool {
        self.scope.is_all()
    }
}

impl<'i> DerivedView<Inputs<'i>> for ChecksView {
    type Scope = ChecksScope;
    const NAME: &'static str = "checks";
    const AREAS: AreaSet = AreaSet::ALL;

    fn note(&self, cx: &NoteCx<'_>, effect: &Effect<'_>, scope: &mut ChecksScope) {
        scope.scope.note(cx.doc, effect);
    }

    fn update(&mut self, doc: &Document, scope: ChecksScope, cx: &Inputs<'i>) -> Outcome {
        let ChecksScope { scope, begun } = scope;
        if begun {
            self.model.update(doc, scope, self.tol, &*cx.engine.metrics);
        }
        Outcome::default()
    }

    fn verify(&self, _doc: &Document, _cx: &Inputs<'i>) -> Option<String> {
        None
    }
}
