//! The kept frame's view (design 064 §4.1, `FrameView`): what the active page's frame last showed
//! (its page and [`FrameInput`]), and whether a commit could change it, so the engine draws it
//! again (`frame_stale`). The drawn parts stay with the engine, which records here what each
//! drawing showed ([`FrameView::show`]).
//!
//! # Update map
//! After the commit, while a frame is shown on page `p`:
//! - Every effect of `crate::pages::frames::touched_by` for `p` and what it showed: the sheet or unit
//!   settings, the text and frame style choices, a frame, text or page style record, the standard,
//!   the project's info, issue or a code it shows, a bound project key or a revision setting, a
//!   property of `p`, `p` changed or moved, a page or folder added, removed or moved, a version.
//! - When `p` is a generated page, every effect of `crate::pages::generated::touched_by` for its kind
//!   (the Contents and Revisions pages' own maps).
//! - Every other effect, and every effect while no frame is shown, changes nothing.
//!
//! Before the commit it notes nothing, so a refused commit leaves the flag as it was. Open,
//! reload and branch switch forget what was shown.

use ecad_model::document::effect::{AreaSet, Effect};
use ecad_model::document::view::{DerivedView, NoteCx, Outcome, Phase, ViewScope};
use ecad_model::{Document, PageId};

use crate::views::Inputs;
use crate::pages::frames::FrameInput;

/// The kept frame's view.
#[derive(Debug, Default)]
pub struct FrameView {
    shown: Option<(PageId, FrameInput)>,
}

impl FrameView {
    /// The page whose frame was last drawn, and what it showed.
    pub fn shown(&self) -> Option<(PageId, &FrameInput)> {
        self.shown.as_ref().map(|(p, i)| (*p, i))
    }

    /// The engine drew `page`'s frame showing `input`.
    pub fn show(&mut self, page: PageId, input: FrameInput) {
        self.shown = Some((page, input));
    }

    /// No frame is kept.
    pub fn forget(&mut self) {
        self.shown = None;
    }
}

/// Whether the kept frame must be drawn again.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FrameScope(pub bool);

impl ViewScope for FrameScope {
    fn is_empty(&self) -> bool {
        !self.0
    }

    fn is_all(&self) -> bool {
        false
    }
}

impl<'i> DerivedView<Inputs<'i>> for FrameView {
    type Scope = FrameScope;
    const NAME: &'static str = "frame";
    const AREAS: AreaSet = AreaSet::ALL;

    fn kept(&self) -> bool {
        self.shown.is_some()
    }

    fn note(&self, cx: &NoteCx<'_>, effect: &Effect<'_>, scope: &mut FrameScope) {
        if cx.phase != Phase::After || scope.0 {
            return;
        }
        let Some((page, input)) = &self.shown else { return };
        let generated = || crate::pages::generated::kind_of(cx.doc, *page).is_some_and(|k| crate::pages::generated::touched_by(effect, k));
        scope.0 = crate::pages::frames::touched_by(effect, *page, input) || generated();
    }

    fn update(&mut self, _doc: &Document, scope: FrameScope, _cx: &Inputs<'i>) -> Outcome {
        Outcome { frame_stale: scope.0, ..Outcome::default() }
    }

    fn verify(&self, _doc: &Document, _cx: &Inputs<'i>) -> Option<String> {
        None
    }
}
