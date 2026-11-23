//! The references view (`ReferencesView`): the kept reference index
//! ([`crate::references::devices::index::RefIndex`]), and per commit the pages whose references changed (cable
//! references, contact texts and mirror tables, connector halves and pins, continuation arrows)
//! and the line about references left without another end.
//!
//! # Update map
//! Its scope is a [`crate::references::devices::watch::Watch`] (the table in [`crate::references::devices`]). Before
//! the commit, for the ops given to `begin`, each effect looked up on the document before them; after it, for the
//! transaction as recorded, on the document after:
//! - The cables (before only, [`crate::references::cables::cables_touched_by`]): a reference
//!   representation or item, a member's route, wire or connection point, a cable, a page's
//!   number, sheet, type or book position, a page type's sheet, a frame's grid. A device, strip
//!   or connector name, a layer, a definition or a link reaches none: a reference says only its
//!   own cable's other ends' page and column or row. Every cable only for the effects that carry
//!   a reason in [`crate::references::cables::Touch::All`] (a frame or standard, the sheet,
//!   length unit or reference style, the project record, a drawing aid).
//!   Their reference ends' pages when their texts changed, and the orphaned-reference line
//!   for the command line.
//! - The devices: items, representations, wires at their ends, connection points, a device made
//!   or removed or its definition switched, `contacts` and `mirror`, definitions; the devices with
//!   a member on a page whose number, sheet, type, book position, type's sheet or frame changed.
//!   The pages where their texts or tables differ before and after.
//! - The continuation names the effects name ([`crate::continuations::names_named_by`]) or that
//!   stand on such a page: the pages where their arrows' texts or caption runs differ.
//! - The table text, the reference style, the sheet, the length unit or the contact mirror
//!   default: every page with a device member (the wide case); the continuation text settings:
//!   every page with an arrow.
//! - The index: the groups the effects name are made again
//!   ([`crate::references::devices::index::RefIndex::groups_named_by`]); a page type's checks switched on or off makes it
//!   again whole (the scope's `is_all`, after the commit only).
//! - Every other effect changes nothing.
//!
//! What the last commit derived for its devices is kept and is the next commit's "before" (brief
//! every change of the document goes through the pipeline or a rebuild, so it is never
//! stale; verify mode checks each reuse against a fresh derivation, and the index against a
//! build ([`ReferencesView::check`]). Open, reload and branch switch build it whole
//! ([`ReferencesView::build`]).

use ecad_model::document::effect::{AreaSet, Effect};
use ecad_model::document::view::{DerivedView, NoteCx, Outcome, PageSet, Phase, ViewScope};
use ecad_model::{Document, Op};

use crate::references::devices::index::RefIndex;
use crate::references::devices::watch::{Shown, Watch};
use crate::views::Inputs;
use ecad_model::look::TextLook;

/// The references view: the kept index and what the last commit derived.
#[derive(Debug, Default)]
pub struct ReferencesView {
    index: RefIndex,
    /// The texts and tables the last commit derived after it, for the next commit's "before".
    shown: Shown,
}

/// The references view's scope: the [`Watch`] noted and taken before the commit.
#[derive(Debug, Default)]
pub struct ReferencesScope {
    watch: Watch,
}

impl ViewScope for ReferencesScope {
    fn is_empty(&self) -> bool {
        false
    }

    /// A page type's checks switched: the index is made again whole.
    fn is_all(&self) -> bool {
        self.watch.rebuilds()
    }
}

impl ReferencesView {
    /// The whole index of `doc`; nothing kept to reuse.
    pub fn build(doc: &Document) -> Self {
        ReferencesView { index: RefIndex::build(doc), shown: Shown::new() }
    }

    /// The kept reference index.
    pub fn index(&self) -> &RefIndex {
        &self.index
    }

    /// The end of the Before phase: derive what the noted devices, names and cables show on
    /// `before`. Changes nothing kept.
    pub fn take(&self, before: &Document, scope: &mut ReferencesScope, m: &dyn TextLook) {
        scope.watch.take(before, &self.index, &self.shown, m);
    }

    /// Verify mode: when the kept index differs from a fresh build of `doc`, rebuild it and say so.
    pub fn check(&mut self, doc: &Document) -> Option<String> {
        let e = self.index.verify(doc).err()?;
        self.index = RefIndex::build(doc);
        self.shown = Shown::new();
        Some(format!("the reference index was out of step ({e}); rebuilt"))
    }

    /// Before and after a transaction of `ops` on `before`, giving `after` (tests and benches
    /// that have no pipeline): the same notes and update as the pipeline's.
    pub fn commit(&mut self, before: &Document, after: &Document, ops: &[Op], inputs: &Inputs<'_>) -> Outcome {
        let effects = ecad_model::document::effect::Effects::of(ops);
        let mut scope = ReferencesScope::default();
        crate::views::note_view::<_, Inputs<'_>>(self, &NoteCx { doc: before, phase: Phase::Before, effects: &effects }, &mut scope);
        self.take(before, &mut scope, &*inputs.engine.look);
        crate::views::note_view::<_, Inputs<'_>>(self, &NoteCx { doc: after, phase: Phase::After, effects: &effects }, &mut scope);
        self.update(after, scope, inputs)
    }
}

impl<'i> DerivedView<Inputs<'i>> for ReferencesView {
    type Scope = ReferencesScope;
    const NAME: &'static str = "references";
    const AREAS: AreaSet = AreaSet::ALL;

    fn note(&self, cx: &NoteCx<'_>, effect: &Effect<'_>, scope: &mut ReferencesScope) {
        scope.watch.note(cx.doc, cx.phase == Phase::Before, effect);
    }

    fn update(&mut self, doc: &Document, scope: ReferencesScope, cx: &Inputs<'i>) -> Outcome {
        if scope.is_all() {
            self.index = RefIndex::build(doc);
        } else {
            scope.watch.refresh(doc, &mut self.index);
        }
        let out = scope.watch.finish(doc, &self.index, &*cx.engine.look);
        self.shown = out.shown;
        Outcome { pages: PageSet::from_option(out.pages), echo: out.orphaned.into_iter().collect(), problems: out.problems, ..Outcome::default() }
    }

    fn verify(&self, doc: &Document, _cx: &Inputs<'i>) -> Option<String> {
        self.index.verify(doc).err().map(|e| format!("the reference index differs from a rebuild ({e})"))
    }
}
