//! One update pipeline for every derived view (design 064 §5): [`Views`].
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
//! Every derived view has one way in: this set (design 064 §5.2). Another document state (an
//! issued or finalised revision) is read through [`Views::detached`].
//!
//! This module excludes the engine: what the engine does with [`Changes`] (the change log, the
//! redraw) is ecad-engine's `Project::apply_changes`.
#![deny(clippy::wildcard_enum_match_arm)]

mod changes;
mod checks;
mod connectors;
mod devices;
mod frame;
mod generated;
mod outputs;
mod pages;
mod props;
mod refs;
mod tree;

use std::sync::Arc;

use ecad_model::effect::{Effect, Effects};
use ecad_model::view::{DerivedView, NoteCx, Outcome, Phase};
use ecad_model::{Document, Op};

pub use changes::Changes;
pub use checks::{ChecksScope, ChecksView};
pub use connectors::{ConnectorsScope, ConnectorsView};
pub use devices::{DevicesFeed, DevicesScope};
pub use frame::{FrameScope, FrameView};
pub use generated::{GeneratedScope, GeneratedView};
pub use outputs::{OutputsScope, OutputsView};
pub use pages::{PagesScope, PagesView, points_pages};
pub use props::{PropsFeed, PropsScope};
pub use refs::{RefsScope, RefsView};
pub use tree::{TreeFeed, TreeScope};

use crate::text_style::fonts::TextMetrics;

/// What the engine supplies to the views (design 064 §5.2): the text metrics, and any later
/// engine-made input. ecad-engine fills it in.
#[derive(Clone)]
pub struct EngineInputs {
    /// How wide a text is drawn on this machine.
    pub metrics: Arc<dyn TextMetrics + Send + Sync>,
}

impl std::fmt::Debug for EngineInputs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineInputs").finish_non_exhaustive()
    }
}

/// What a view's update reads besides the document: the engine's inputs and the outcomes of
/// earlier stages (design 064 §4).
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    pub engine: &'a EngineInputs,
    /// The page whose frame is kept (`FrameView`), if any.
    pub frame_page: Option<ecad_model::PageId>,
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

    /// Forget the view: it is built again on its next show.
    pub fn reset(&mut self) {
        self.0 = None;
    }
}

/// Each kept view's scope, noted on the document before the ops ([`Views::begin`]).
#[derive(Debug, Default)]
pub struct Pending {
    refs: RefsScope,
    connectors: ConnectorsScope,
    pages: PagesScope,
    frame: FrameScope,
    generated: GeneratedScope,
    outputs: OutputsScope,
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
    outputs: OutputsView,
}

impl Detached {
    /// The report models of that state.
    pub fn outputs(&self) -> &crate::outputs::kept::Outputs {
        self.outputs.model()
    }

    /// Every check of that state `doc`, worked out from scratch with tolerance `tol`, the report
    /// models' rows included, in book order.
    pub fn checks(&self, doc: &Document, tol: f64) -> Vec<crate::checks::Check> {
        crate::checks::all_measured(doc, tol, self.outputs(), &*self.inputs.metrics)
    }
}

/// Every derived view the project keeps, in stage order (design 064 §5).
#[derive(Debug)]
pub struct Views {
    inputs: EngineInputs,
    // Stage 1: indexes that depend only on the document.
    refs: RefsView,
    connectors: ConnectorsView,
    // Stage 2: the report models, built on first show.
    outputs: Lazy<OutputsView>,
    // Stage 3: pages.
    pages: PagesView,
    frame: FrameView,
    generated: GeneratedView,
    // Stage 3b: the Checks list, built on first show (its cover rows follow stage 3's cover).
    checks: Lazy<ChecksView>,
    // Stage 4: feeds for the change log.
    tree: TreeFeed,
    devices: DevicesFeed,
    props: PropsFeed,
}

impl Views {
    /// No view built yet; [`Views::rebuild`] builds the kept ones.
    pub fn new(inputs: EngineInputs) -> Self {
        Views {
            inputs,
            refs: RefsView::default(),
            connectors: ConnectorsView::default(),
            outputs: Lazy::default(),
            pages: PagesView::default(),
            frame: FrameView::default(),
            generated: GeneratedView::default(),
            checks: Lazy::default(),
            tree: TreeFeed,
            devices: DevicesFeed::default(),
            props: PropsFeed::default(),
        }
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

    /// The kept reference index (design 025 §3.3).
    pub fn refs(&self) -> &crate::xref::RefIndex {
        self.refs.index()
    }

    /// The references view, to put it out of step on purpose (tests of verify mode only).
    pub fn refs_view_mut(&mut self) -> &mut RefsView {
        &mut self.refs
    }

    /// The kept connector index (design 044 §11.1).
    pub fn connectors(&self) -> &crate::connectors::Index {
        self.connectors.index()
    }

    /// The report models, built on their first show (design 024 §8.1) and kept after that.
    pub fn outputs(&mut self, doc: &Document) -> &crate::outputs::kept::Outputs {
        let metrics = &*self.inputs.metrics;
        self.outputs.get_or_build(|| OutputsView::build(doc, metrics)).model()
    }

    /// The report models, if shown once.
    pub fn kept_outputs(&self) -> Option<&crate::outputs::kept::Outputs> {
        self.outputs.get().map(OutputsView::model)
    }

    /// The report models' view, if shown once, to put it out of step on purpose (tests of verify
    /// mode only).
    pub fn outputs_view_mut(&mut self) -> Option<&mut OutputsView> {
        self.outputs.get_mut()
    }

    /// Forget the report models: they are built again on their next show.
    pub fn forget_outputs(&mut self) {
        self.outputs.reset();
    }

    /// The Checks list, built on its first show with tolerance `tol` (design 022 §8) and kept
    /// after that.
    pub fn checks(&mut self, doc: &Document, tol: f64) -> &crate::checks::kept::Checks {
        let metrics = &*self.inputs.metrics;
        self.checks.get_or_build(|| ChecksView::build(doc, tol, metrics)).model()
    }

    /// The Checks list, if shown once.
    pub fn kept_checks(&self) -> Option<&crate::checks::kept::Checks> {
        self.checks.get().map(ChecksView::model)
    }

    /// The Checks list's view, if shown once (the bench times its update on its own).
    pub fn checks_view(&self) -> Option<&ChecksView> {
        self.checks.get()
    }

    /// The Checks list's view, if shown once, to update it on its own (the bench).
    pub fn checks_view_mut(&mut self) -> Option<&mut ChecksView> {
        self.checks.get_mut()
    }

    /// Before phase: every kept view notes `ops` on the document before them. Changes nothing.
    pub fn begin(&self, before: &Document, ops: &[Op]) -> Pending {
        let effects = Effects::of(ops);
        let cx = NoteCx { doc: before, phase: Phase::Before, effects: &effects };
        let mut pending = Pending::default();
        note_view::<_, Inputs<'_>>(&self.refs, &cx, &mut pending.refs);
        // The end of the references' Before phase: what they show before the ops.
        self.refs.take(before, &mut pending.refs, &*self.inputs.metrics);
        note_view::<_, Inputs<'_>>(&self.connectors, &cx, &mut pending.connectors);
        if let Some(outputs) = self.outputs.get() {
            note_view::<_, Inputs<'_>>(outputs, &cx, &mut pending.outputs);
            // The end of the report models' Before phase.
            outputs.close_before(before, &mut pending.outputs);
        }
        note_view::<_, Inputs<'_>>(&self.pages, &cx, &mut pending.pages);
        note_view::<_, Inputs<'_>>(&self.frame, &cx, &mut pending.frame);
        note_view::<_, Inputs<'_>>(&self.generated, &cx, &mut pending.generated);
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
        let Pending { mut refs, mut connectors, mut outputs, mut pages, mut frame, mut generated, mut checks, mut tree, mut devices, mut props } =
            pending;
        let effects = Effects::of(tx_ops);
        let cx = NoteCx { doc: after, phase: Phase::After, effects: &effects };
        note_view::<_, Inputs<'_>>(&self.refs, &cx, &mut refs);
        note_view::<_, Inputs<'_>>(&self.connectors, &cx, &mut connectors);
        if let Some(view) = self.outputs.get() {
            note_view::<_, Inputs<'_>>(view, &cx, &mut outputs);
        }
        note_view::<_, Inputs<'_>>(&self.pages, &cx, &mut pages);
        note_view::<_, Inputs<'_>>(&self.frame, &cx, &mut frame);
        note_view::<_, Inputs<'_>>(&self.generated, &cx, &mut generated);
        if let Some(view) = self.checks.get() {
            note_view::<_, Inputs<'_>>(view, &cx, &mut checks);
        }
        note_view::<_, Inputs<'_>>(&self.tree, &cx, &mut tree);
        note_view::<_, Inputs<'_>>(&self.devices, &cx, &mut devices);
        note_view::<_, Inputs<'_>>(&self.props, &cx, &mut props);
        let inputs = Inputs { engine: &self.inputs, frame_page: self.frame.shown().map(|(p, _)| p) };
        let mut changes = Changes::default();
        // Stage 1: the indexes.
        merge(&mut changes, self.refs.update(after, refs, &inputs));
        merge(&mut changes, self.connectors.update(after, connectors, &inputs));
        // Stage 2: the report models.
        if let Some(view) = self.outputs.get_mut() {
            merge(&mut changes, view.update(after, outputs, &inputs));
        }
        // Stage 3: pages.
        merge(&mut changes, self.pages.update(after, pages, &inputs));
        changes.named = self.pages.take_named();
        merge(&mut changes, self.frame.update(after, frame, &inputs));
        merge(&mut changes, self.generated.update(after, generated, &inputs));
        let cover = self.generated.take_cover();
        // Stage 3b: the Checks list; brief 054b: the cover's rows follow the cover (a cover page
        // added or removed too).
        if let Some(view) = self.checks.get_mut() {
            merge(&mut changes, view.update(after, checks, &inputs));
            if cover {
                view.set_cover(after);
            }
        }
        // Stage 4: the feeds, merged in field order.
        merge(&mut changes, self.tree.update(after, tree, &inputs));
        merge(&mut changes, self.devices.update(after, devices, &inputs));
        changes.devices = self.devices.take();
        merge(&mut changes, self.props.update(after, props, &inputs));
        changes.props = self.props.take();
        if ecad_model::ledger::verify_mode() {
            changes.problems.extend(self.verify(after));
        }
        changes
    }

    /// Verify mode: every kept view against a fresh build; one line per disagreement (the view
    /// is rebuilt).
    pub fn verify(&mut self, doc: &Document) -> Vec<String> {
        let inputs = Inputs { engine: &self.inputs, frame_page: None };
        let generated = self.generated.verify(doc, &inputs);
        if generated.is_some() {
            self.generated.forget();
        }
        self.refs.check(doc).into_iter().chain(self.connectors.check(doc)).chain(generated).collect()
    }

    /// Open, reload, branch switch: rebuild every kept view from `doc`.
    pub fn rebuild(&mut self, doc: &Document) {
        self.refs = RefsView::build(doc);
        self.connectors = ConnectorsView::build(doc);
        self.outputs.reset();
        self.checks.reset();
        self.frame.forget();
        self.generated.forget();
    }

    /// The views of another document state (an issued or finalised revision), built with the
    /// same engine inputs.
    pub fn detached(&self, doc: &Document) -> Detached {
        Detached { inputs: self.inputs.clone(), outputs: OutputsView::build(doc, &*self.inputs.metrics) }
    }

    /// The report models of another state `doc`, worked out from scratch, when it has a page drawn
    /// from them ([`crate::generated::has_bom_page`]); `None` otherwise. For drawing a state the
    /// project does not keep (debugger, verification, a revision, tests): the project passes its
    /// kept models instead.
    pub fn report_models(&self, doc: &Document) -> Option<crate::outputs::kept::Outputs> {
        crate::generated::has_bom_page(doc).then(|| OutputsView::build(doc, &*self.inputs.metrics).into_model())
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
/// areas (design 064 §3.3). A view that is not kept notes nothing.
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
