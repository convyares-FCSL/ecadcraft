//! The rating results' view (`RatingView`): the kept [`Kept`], built on first show (the Wires and
//! Cables tabs, or the Checks list, whose rating rows read it) and then kept from each commit's
//! effects.
//!
//! # Update map
//! Before the commit, on the state before, and after it, on the state after: what each effect
//! reaches ([`Touch::note`]). The update re-evaluates that ([`Kept::update`]); [`Kept::last`] and
//! [`RatingView::reached`] then tell the Checks list, the next stage, which rating rows to work out
//! again. The outcome is empty. Until first show the view is not kept and notes nothing.

use ecad_model::Document;
use ecad_model::document::effect::{AreaSet, Effect};
use ecad_model::document::view::{DerivedView, NoteCx, Outcome, ViewScope};

use super::kept::{Kept, Touch};
use crate::views::Inputs;

/// The kept rating results, and whether the last commit reached them.
#[derive(Debug, Default)]
pub struct RatingView {
    kept: Kept,
    reached: bool,
}

impl RatingView {
    /// Every result, from scratch (first show).
    pub fn build(doc: &Document) -> Self {
        RatingView { kept: Kept::build(doc), reached: false }
    }

    /// The kept results.
    pub fn kept(&self) -> &Kept {
        &self.kept
    }

    /// Whether the last update's ops reached anything here (the Checks list then works out its
    /// count rows again, even when no result changed).
    pub fn reached(&self) -> bool {
        self.reached
    }

    /// The end of the Before phase: the view was kept then.
    pub fn close_before(&self, scope: &mut RatingScope) {
        scope.begun = true;
    }
}

/// What a commit reaches of the results: noted on both states.
#[derive(Debug, Default)]
pub struct RatingScope {
    touch: Touch,
    /// The Before phase was closed ([`RatingView::close_before`]): the view was kept then.
    begun: bool,
}

impl ViewScope for RatingScope {
    fn is_empty(&self) -> bool {
        !self.begun
    }

    fn is_all(&self) -> bool {
        false
    }
}

impl<'i> DerivedView<Inputs<'i>> for RatingView {
    type Scope = RatingScope;
    const NAME: &'static str = "rating";
    const AREAS: AreaSet = AreaSet::ALL;

    fn note(&self, cx: &NoteCx<'_>, effect: &Effect<'_>, scope: &mut RatingScope) {
        scope.touch.note(cx.doc, effect);
    }

    fn update(&mut self, doc: &Document, scope: RatingScope, _cx: &Inputs<'i>) -> Outcome {
        let RatingScope { touch, begun } = scope;
        self.reached = begun && touch.any();
        if begun {
            self.kept.update(doc, &touch);
        }
        Outcome::default()
    }

    fn verify(&self, _doc: &Document, _cx: &Inputs<'i>) -> Option<String> {
        None
    }
}
