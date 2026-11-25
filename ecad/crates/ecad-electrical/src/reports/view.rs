//! The report models' view (`ReportsView`): the kept [`ReportModels`] (rows, strips,
//! PLC model, strip pages), built on first show and then kept from each commit's effects.
//!
//! # Update map
//! Before the commit, on the state before: the rows each effect names (`reports::Scope::note`),
//! then their net mates and the users of renamed entries (`reports::Scope::close`).
//! After the commit, on the state after, reading the models as they were: the rows each effect
//! names again, the wires only the state after names (`reports::Scope::note_after`), the PLC
//! model's touch (`plc::kept::Plc::note`), the strip pages' names (`strips::pages::Named`), a page
//! number or sheet count that can move, the pages added, removed or moved, the book positions,
//! and what can move a place on a rack sheet (`plc::sheets::RackPlaces`).
//! The update recomputes what was noted ([`ReportModels::update`]). Its outcome:
//! - `pages`: the generated pages drawn from the models whose content changed, by kind: a BOM
//!   page when the models' revision moved, a terminal plan when a strip it shows changed
//!   (`generated::connection_points_rewritten`), a cable schedule when a cable row changed, an I/O list
//!   when a card's rows changed, a rack sheet when a card or rack changed or a place on it moved.
//! - `frame_stale`: the generated pages' sheet count changed, the kept frame's page is one of
//!   those rewritten, or a page number moved.
//! - Every other effect changes nothing. Until first show the view is not kept and notes nothing.
//!
//! Verify mode: the kept strips and PLC model against a rebuild, after each update.

use std::collections::BTreeSet;

use ecad_model::document::effect::{AreaSet, Effect};
use ecad_model::document::view::{DerivedView, NoteCx, Outcome, PageSet, Phase, ViewScope};
use ecad_model::{Document, PageId};

use crate::pages::generated;
use crate::plc::sheets::RackPlaces;
use crate::reports::kept::{Noted, ReportModels};
use crate::views::Inputs;
use ecad_model::GenKind;
use ecad_model::look::Look;

/// The kept report models.
#[derive(Debug)]
pub struct ReportsView {
    model: ReportModels,
}

impl ReportsView {
    /// Every model, from scratch (first show), with text measured in `m`.
    pub fn build(doc: &Document, m: &dyn Look) -> Self {
        ReportsView { model: ReportModels::build_with(doc, m) }
    }

    /// The kept models.
    pub fn model(&self) -> &ReportModels {
        &self.model
    }

    /// The models, the view dropped.
    pub fn into_model(self) -> ReportModels {
        self.model
    }

    /// The kept models, to put them out of step on purpose (tests of verify mode only).
    pub fn model_mut(&mut self) -> &mut ReportModels {
        &mut self.model
    }

    /// The sheet count of the generated pages drawn from the models.
    fn sheets(&self, doc: &Document) -> usize {
        let o = &self.model;
        o.generated_pages().iter().filter(|p| doc.pages().contains_key(p)).map(|p| generated::sheets_of(doc, *p, Some(o))).sum()
    }
}

/// What a commit could change in the models: noted on both states.
#[derive(Debug, Default)]
pub struct ReportsScope {
    noted: Noted,
    places: RackPlaces,
    /// The Before phase was closed ([`ReportsView::close_before`]): the view was kept then.
    begun: bool,
}

impl ViewScope for ReportsScope {
    fn is_empty(&self) -> bool {
        !self.begun
    }

    fn is_all(&self) -> bool {
        false
    }
}

impl ReportsView {
    /// The end of the Before phase on `before`: the noted rows' net mates and renamed entries' users.
    pub fn close_before(&self, before: &Document, scope: &mut ReportsScope) {
        self.model.close_before(before, &mut scope.noted);
        scope.begun = true;
    }
}

impl<'i> DerivedView<Inputs<'i>> for ReportsView {
    type Scope = ReportsScope;
    const NAME: &'static str = "reports";
    const AREAS: AreaSet = AreaSet::ALL;

    fn note(&self, cx: &NoteCx<'_>, effect: &Effect<'_>, scope: &mut ReportsScope) {
        match cx.phase {
            Phase::Before => self.model.note_before(cx.doc, effect, &mut scope.noted),
            Phase::After => {
                self.model.note_after(cx.doc, effect, &mut scope.noted);
                scope.places.note(effect);
            }
        }
    }

    fn update(&mut self, doc: &Document, scope: ReportsScope, cx: &Inputs<'i>) -> Outcome {
        let ReportsScope { noted, places, begun } = scope;
        if !begun {
            return Outcome::default();
        }
        let look = &*cx.engine.look;
        // The terminal plan and cable schedule pages count their sheets from the strip
        // and cable rows; only the pages drawn from the models can change their sheet count here.
        let (before, revision) = (self.sheets(doc), self.model.revision);
        self.model.update(doc, noted, look);
        // The rack sheet prints each card's place.
        let places_moved = places.touched(doc, &self.model.plc);
        let mut problems = Vec::new();
        if ecad_model::document::ledger::verify_mode()
            && let Some(line) = self.model.verify_strips_with(doc)
        {
            problems.push(line);
        }
        let o = &self.model;
        // Each generated page says whether it was rewritten, by its kind's
        // kept change set.
        let rewritten = |p: PageId| match generated::kind_of(doc, p) {
            Some(GenKind::BomLocation) => o.revision != revision,
            // Either style, by its own kept change set.
            Some(GenKind::TerminalBlocks) => generated::connection_points_rewritten(doc, p, o),
            Some(GenKind::Cables) => o.last_cables_changed,
            // The I/O list when a card's rows changed.
            Some(GenKind::IoList) => !o.plc.rows_changed.is_empty(),
            // The rack sheet when a card or rack changed.
            Some(GenKind::PlcRack) => !o.plc.last_changed.is_empty() || !o.plc.rows_changed.is_empty() || places_moved,
            Some(GenKind::Cover | GenKind::Contents | GenKind::Revisions) | None => false,
        };
        let pages: BTreeSet<PageId> = o.generated_pages().iter().copied().filter(|p| doc.pages().contains_key(p) && rewritten(*p)).collect();
        let shown = cx.frame_page.is_some_and(rewritten);
        let frame_stale = self.sheets(doc) != before || shown || self.model.numbers_moved;
        Outcome { pages: PageSet::Only(pages), frame_stale, problems, ..Outcome::default() }
    }

    fn verify(&self, _doc: &Document, _cx: &Inputs<'i>) -> Option<String> {
        // Verify mode checks the strips and PLC model after each update (they are put right there).
        None
    }
}
