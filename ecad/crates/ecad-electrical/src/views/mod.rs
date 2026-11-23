//! One update pipeline for every derived view: [`Views`].
//!
//! The project owns one [`Views`], built once with the engine's inputs ([`EngineInputs`]) on open,
//! reload and branch switch, and after that updated only by the three commit paths (command,
//! drawing commit, undo and redo), each in the same three calls:
//!
//! ```text
//! let pending = views.begin(before_doc, &ops);       // nothing changes yet
//! <the path's own commit>                            // refused: drop `pending`
//! let changes = views.finish(after_doc, tx_ops, pending);
//! ```
//!
//! Every derived view has one way in: this set. Another document state (an
//! issued or finalised revision) is read through [`Views::detached`].
//!
//! This module excludes the engine: what the engine does with [`Changes`] (the change log, the
//! redraw) is ecad-engine's `Project::apply_changes`.
#![deny(clippy::wildcard_enum_match_arm)]

pub(crate) mod changes;

use std::sync::Arc;

use ecad_model::document::effect::{Effect, Effects};
use ecad_model::document::view::{DerivedView, NoteCx, Outcome, Phase};
use ecad_model::{Document, Op};

pub use crate::checks::view::{ChecksScope, ChecksView};
pub use crate::circuits::view::{RatingScope, RatingView};
pub use crate::connectors::view::{ConnectorsScope, ConnectorsView};
pub(crate) use crate::devices::view::{DevicesFeed, DevicesScope};
pub use crate::pages::frame_view::{FrameScope, FrameView};
pub use crate::pages::generated_view::{GeneratedScope, GeneratedView};
pub use crate::pages::tree_view::{TreeFeed, TreeScope};
pub use crate::pages::view::{PagesScope, PagesView, points_pages};
pub use crate::properties::view::{PropsFeed, PropsScope};
pub use crate::references::view::{ReferencesScope, ReferencesView};
pub use crate::reports::view::{ReportsScope, ReportsView};
pub use changes::Changes;

use ecad_model::look::Look;

/// What the engine supplies to the views: the text look, and any later
/// engine-made input. ecad-engine fills it in.
#[derive(Clone)]
pub struct EngineInputs {
    /// How a text is drawn on this machine, and how wide.
    pub look: Arc<dyn Look + Send + Sync>,
    /// Every crate's defaults and style catalogues.
    pub registry: &'static ecad_model::registry::data::Registry,
}

impl std::fmt::Debug for EngineInputs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineInputs").finish_non_exhaustive()
    }
}

/// What a view's update reads besides the document: the engine's inputs and the outcomes of
/// earlier stages.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    pub engine: &'a EngineInputs,
    /// The page whose frame is kept (`FrameView`), if any.
    pub frame_page: Option<ecad_model::PageId>,
    /// The rating results, already updated for the commit (for the Checks list), if kept.
    pub rating: Option<&'a RatingView>,
}

/// A view built on first show (the checks list, the report models): skipped by the pipeline
/// until then, at no cost.
#[derive(Debug)]
pub struct Lazy<V>(Option<V>);

impl<V> Default for Lazy<V> {
    fn default() -> Self {
        Lazy(None)
    }
}

impl<V> Lazy<V> {
    /// The view, once built.
    pub fn get(&self) -> Option<&V> {
        self.0.as_ref()
    }

    /// The view, once built, to update.
    pub fn get_mut(&mut self) -> Option<&mut V> {
        self.0.as_mut()
    }

    /// The view, built by `build` the first time.
    pub fn get_or_build(&mut self, build: impl FnOnce() -> V) -> &mut V {
        self.0.get_or_insert_with(build)
    }

    /// Keep `view` as the built view.
    pub fn set(&mut self, view: V) {
        self.0 = Some(view);
    }

    /// Forget the view: it is built again on its next show.
    pub fn reset(&mut self) {
        self.0 = None;
    }
}

/// Each kept view's scope, noted on the document before the ops ([`Views::begin`]).
#[derive(Debug, Default)]
pub struct Pending {
    references: ReferencesScope,
    connectors: ConnectorsScope,
    pages: PagesScope,
    frame: FrameScope,
    generated: GeneratedScope,
    reports: ReportsScope,
    rating: RatingScope,
    checks: ChecksScope,
    tree: TreeScope,
    devices: DevicesScope,
    props: PropsScope,
}

/// The views worked out for another document state ([`Views::detached`]): not kept, not
/// updated, dropped after use.
#[derive(Debug)]
pub struct Detached {
    inputs: EngineInputs,
    reports: ReportsView,
}

impl Detached {
    /// The report models of that state.
    pub fn reports(&self) -> &crate::reports::kept::ReportModels {
        self.reports.model()
    }

    /// Every check of that state `doc`, worked out from scratch with tolerance `tol`, the report
    /// models' rows included, in book order.
    pub fn checks(&self, doc: &Document, tol: f64) -> Vec<crate::checks::Check> {
        crate::checks::all_measured(doc, tol, self.reports(), &*self.inputs.look, self.inputs.registry)
    }
}

/// Every derived view the project keeps, in stage order.
#[derive(Debug)]
pub struct Views {
    inputs: EngineInputs,
    // Stage 1: indexes that depend only on the document.
    references: ReferencesView,
    connectors: ConnectorsView,
    // Stage 2: the report models, built on first show.
    reports: Lazy<ReportsView>,
    // Stage 3: pages.
    pages: PagesView,
    frame: FrameView,
    generated: GeneratedView,
    // Stage 3b: the rating results, built on first show (the Wires and Cables tabs, or the
    // Checks list), then the Checks list, built on first show (its rating rows follow the rating
    // results, its cover rows stage 3's cover).
    rating: Lazy<RatingView>,
    checks: Lazy<ChecksView>,
    // Stage 4: feeds for the change log.
    tree: TreeFeed,
    devices: DevicesFeed,
    props: PropsFeed,
    /// Counts every [`Views::rebuild`]: a view built off the thread for an earlier count is not
    /// installed ([`Views::install_reports`]).
    epoch: u64,
}

impl Views {
    /// No view built yet; [`Views::rebuild`] builds the kept ones.
    pub fn new(inputs: EngineInputs) -> Self {
        let registry = inputs.registry;
        Views {
            inputs,
            references: ReferencesView::default(),
            connectors: ConnectorsView::default(),
            reports: Lazy::default(),
            pages: PagesView::new(registry),
            frame: FrameView::default(),
            generated: GeneratedView::default(),
            rating: Lazy::default(),
            checks: Lazy::default(),
            tree: TreeFeed,
            devices: DevicesFeed::default(),
            props: PropsFeed::default(),
            epoch: 0,
        }
    }

    /// How many times the views were rebuilt whole (open, reload, branch switch).
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Put in the report models built elsewhere (off the UI thread) for the state `epoch` and the
    /// document the caller checked is the current one. Refused (`false`) when they are already kept
    /// or the views were rebuilt since.
    pub fn install_reports(&mut self, epoch: u64, view: ReportsView) -> bool {
        if epoch != self.epoch || self.reports.get().is_some() {
            return false;
        }
        self.reports.set(view);
        true
    }

    /// As [`Views::install_reports`], for the Checks list and the rating results it was built
    /// with (kept unless the rating results are already kept).
    pub fn install_checks(&mut self, epoch: u64, rating: RatingView, view: ChecksView) -> bool {
        if epoch != self.epoch || self.checks.get().is_some() {
            return false;
        }
        if self.rating.get().is_none() {
            self.rating.set(rating);
        }
        self.checks.set(view);
        true
    }

    /// The text look the views are built with, to build one off the thread
    /// ([`ReportsView::build`], [`ChecksView::build`]).
    pub fn look(&self) -> Arc<dyn Look + Send + Sync> {
        self.inputs.look.clone()
    }

    /// The engine's inputs the views are built with.
    pub fn inputs(&self) -> &EngineInputs {
        &self.inputs
    }

    /// The kept frame's view: what the active page's frame last showed.
    pub fn frame(&self) -> &FrameView {
        &self.frame
    }

    /// The kept frame's view, for the engine to record what it drew.
    pub fn frame_mut(&mut self) -> &mut FrameView {
        &mut self.frame
    }

    /// The kept reference index.
    pub fn references(&self) -> &crate::references::devices::index::RefIndex {
        self.references.index()
    }

    /// The references view, to put it out of step on purpose (tests of verify mode only).
    pub fn references_view_mut(&mut self) -> &mut ReferencesView {
        &mut self.references
    }

    /// The kept connector index.
    pub fn connectors(&self) -> &crate::connectors::Index {
        self.connectors.index()
    }

    /// The report models, built on their first show and kept after that.
    pub fn reports(&mut self, doc: &Document) -> &crate::reports::kept::ReportModels {
        let look = &*self.inputs.look;
        self.reports.get_or_build(|| ReportsView::build(doc, look)).model()
    }

    /// The report models, if shown once.
    pub fn kept_reports(&self) -> Option<&crate::reports::kept::ReportModels> {
        self.reports.get().map(ReportsView::model)
    }

    /// The report models' view, if shown once, to put it out of step on purpose (tests of verify
    /// mode only).
    pub fn reports_view_mut(&mut self) -> Option<&mut ReportsView> {
        self.reports.get_mut()
    }

    /// Forget the report models: they are built again on their next show.
    pub fn forget_reports(&mut self) {
        self.reports.reset();
    }

    /// The rating results, built on their first show and kept after that.
    pub fn rating(&mut self, doc: &Document) -> &crate::circuits::kept::Kept {
        self.rating.get_or_build(|| RatingView::build(doc)).kept()
    }

    /// The Checks list, built on its first show with tolerance `tol` (with the rating results, if
    /// not kept yet) and kept after that.
    pub fn checks(&mut self, doc: &Document, tol: f64) -> &crate::checks::kept::Checks {
        let (look, registry) = (&*self.inputs.look, self.inputs.registry);
        let rating = self.rating.get_or_build(|| RatingView::build(doc));
        self.checks.get_or_build(|| ChecksView::build(doc, tol, look, registry, rating.kept())).model()
    }

    /// The Checks list, if shown once.
    pub fn kept_checks(&self) -> Option<&crate::checks::kept::Checks> {
        self.checks.get().map(ChecksView::model)
    }

    /// Before phase: every kept view notes `ops` on the document before them. Changes nothing.
    pub fn begin(&self, before: &Document, ops: &[Op]) -> Pending {
        let effects = Effects::of(ops);
        let cx = NoteCx { doc: before, phase: Phase::Before, effects: &effects };
        let mut pending = Pending::default();
        note_view::<_, Inputs<'_>>(&self.references, &cx, &mut pending.references);
        // The end of the references' Before phase: what they show before the ops.
        self.references.take(before, &mut pending.references, &*self.inputs.look);
        note_view::<_, Inputs<'_>>(&self.connectors, &cx, &mut pending.connectors);
        if let Some(reports) = self.reports.get() {
            note_view::<_, Inputs<'_>>(reports, &cx, &mut pending.reports);
            // The end of the report models' Before phase.
            reports.close_before(before, &mut pending.reports);
        }
        note_view::<_, Inputs<'_>>(&self.pages, &cx, &mut pending.pages);
        note_view::<_, Inputs<'_>>(&self.frame, &cx, &mut pending.frame);
        note_view::<_, Inputs<'_>>(&self.generated, &cx, &mut pending.generated);
        if let Some(rating) = self.rating.get() {
            note_view::<_, Inputs<'_>>(rating, &cx, &mut pending.rating);
            rating.close_before(&mut pending.rating);
        }
        if let Some(checks) = self.checks.get() {
            note_view::<_, Inputs<'_>>(checks, &cx, &mut pending.checks);
            checks.close_before(&mut pending.checks);
        }
        note_view::<_, Inputs<'_>>(&self.tree, &cx, &mut pending.tree);
        note_view::<_, Inputs<'_>>(&self.devices, &cx, &mut pending.devices);
        note_view::<_, Inputs<'_>>(&self.props, &cx, &mut pending.props);
        pending
    }

    /// After phase on the committed transaction's ops, then update every kept view, stage by
    /// stage, and merge their outcomes.
    pub fn finish(&mut self, after: &Document, tx_ops: &[Op], pending: Pending) -> Changes {
        let Pending {
            mut references,
            mut connectors,
            mut reports,
            mut pages,
            mut frame,
            mut generated,
            mut rating,
            mut checks,
            mut tree,
            mut devices,
            mut props,
        } = pending;
        let effects = Effects::of(tx_ops);
        let cx = NoteCx { doc: after, phase: Phase::After, effects: &effects };
        note_view::<_, Inputs<'_>>(&self.references, &cx, &mut references);
        note_view::<_, Inputs<'_>>(&self.connectors, &cx, &mut connectors);
        if let Some(view) = self.reports.get() {
            note_view::<_, Inputs<'_>>(view, &cx, &mut reports);
        }
        note_view::<_, Inputs<'_>>(&self.pages, &cx, &mut pages);
        note_view::<_, Inputs<'_>>(&self.frame, &cx, &mut frame);
        note_view::<_, Inputs<'_>>(&self.generated, &cx, &mut generated);
        if let Some(view) = self.rating.get() {
            note_view::<_, Inputs<'_>>(view, &cx, &mut rating);
        }
        if let Some(view) = self.checks.get() {
            note_view::<_, Inputs<'_>>(view, &cx, &mut checks);
        }
        note_view::<_, Inputs<'_>>(&self.tree, &cx, &mut tree);
        note_view::<_, Inputs<'_>>(&self.devices, &cx, &mut devices);
        note_view::<_, Inputs<'_>>(&self.props, &cx, &mut props);
        let inputs = Inputs { engine: &self.inputs, frame_page: self.frame.shown().map(|(p, _)| p), rating: None };
        let mut changes = Changes::default();
        // Stage 1: the indexes.
        merge(&mut changes, self.references.update(after, references, &inputs));
        merge(&mut changes, self.connectors.update(after, connectors, &inputs));
        // Stage 2: the report models.
        if let Some(view) = self.reports.get_mut() {
            merge(&mut changes, view.update(after, reports, &inputs));
        }
        // Stage 3: pages.
        merge(&mut changes, self.pages.update(after, pages, &inputs));
        changes.named = self.pages.take_named();
        merge(&mut changes, self.frame.update(after, frame, &inputs));
        merge(&mut changes, self.generated.update(after, generated, &inputs));
        let cover = self.generated.take_cover();
        // The rating results, then the Checks list: its rating rows follow them, the cover's rows
        // the cover (a cover page added or removed too).
        if let Some(view) = self.rating.get_mut() {
            merge(&mut changes, view.update(after, rating, &inputs));
        }
        if let Some(view) = self.checks.get_mut() {
            let inputs = Inputs { rating: self.rating.get(), ..inputs };
            merge(&mut changes, view.update(after, checks, &inputs));
            if cover {
                view.set_cover(after, inputs.engine.look.as_ref());
            }
        }
        // Stage 4: the feeds, merged in field order.
        merge(&mut changes, self.tree.update(after, tree, &inputs));
        merge(&mut changes, self.devices.update(after, devices, &inputs));
        changes.devices = self.devices.take();
        merge(&mut changes, self.props.update(after, props, &inputs));
        changes.props = self.props.take();
        if ecad_model::document::ledger::verify_mode() {
            changes.problems.extend(self.verify(after));
        }
        changes
    }

    /// Verify mode: every kept view against a fresh build; one line per disagreement (the view
    /// is rebuilt).
    pub fn verify(&mut self, doc: &Document) -> Vec<String> {
        let inputs = Inputs { engine: &self.inputs, frame_page: None, rating: None };
        let generated = self.generated.verify(doc, &inputs);
        if generated.is_some() {
            self.generated.forget();
        }
        self.references.check(doc).into_iter().chain(generated).collect()
    }

    /// Open, reload, branch switch: rebuild every kept view from `doc`.
    pub fn rebuild(&mut self, doc: &Document) {
        self.epoch += 1;
        self.references = ReferencesView::build(doc);
        self.connectors = ConnectorsView::build(doc);
        self.reports.reset();
        self.rating.reset();
        self.checks.reset();
        self.frame.forget();
        self.generated.forget();
    }

    /// The views of another document state (an issued or finalised revision), built with the
    /// same engine inputs.
    pub fn detached(&self, doc: &Document) -> Detached {
        Detached { inputs: self.inputs.clone(), reports: ReportsView::build(doc, &*self.inputs.look) }
    }

    /// The report models of another state `doc`, worked out from scratch, when it has a page drawn
    /// from them ([`crate::pages::generated::has_bom_page`]); `None` otherwise. For drawing a state the
    /// project does not keep (debugger, verification, a revision, tests): the project passes its
    /// kept models instead.
    pub fn report_models(&self, doc: &Document) -> Option<crate::reports::kept::ReportModels> {
        crate::pages::generated::has_bom_page(doc).then(|| ReportsView::build(doc, &*self.inputs.look).into_model())
    }
}

/// Add one view's outcome to `changes`.
fn merge(changes: &mut Changes, outcome: Outcome) {
    let Outcome { pages, frame_stale, list, echo, problems } = outcome;
    changes.pages.union(pages);
    changes.frame_stale |= frame_stale;
    changes.list |= list;
    changes.echo.extend(echo);
    changes.problems.extend(problems);
}

/// Note every effect of `cx.effects` for `view`, dropping a property write outside the view's
/// areas. A view that is not kept notes nothing.
pub fn note_view<V, Cx>(view: &V, cx: &NoteCx<'_>, scope: &mut V::Scope)
where
    V: DerivedView<Cx>,
    Cx: ?Sized,
{
    if !view.kept() {
        return;
    }
    for effect in cx.effects {
        if let Effect::Prop { key, .. } = effect
            && !V::AREAS.contains(key.area)
        {
            continue;
        }
        view.note(cx, effect, scope);
    }
}
