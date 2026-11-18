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
//! redraw) is ecad-cadcraft's `Project::apply_changes`.
#![deny(clippy::wildcard_enum_match_arm)]

mod changes;
mod connectors;
mod devices;
mod frame;
mod tree;

use std::sync::Arc;

use ecad_model::effect::{Effect, Effects};
use ecad_model::view::{DerivedView, NoteCx, Outcome, Phase};
use ecad_model::{Document, Op};

pub use changes::Changes;
pub use connectors::{ConnectorsScope, ConnectorsView};
pub use devices::{DevicesFeed, DevicesScope};
pub use frame::{FrameScope, FrameView};
pub use tree::{TreeFeed, TreeScope};

use crate::text_style::fonts::TextMetrics;

/// What the engine supplies to the views (design 064 §5.2): the text metrics, and any later
/// engine-made input. ecad-cadcraft fills it in.
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
    connectors: ConnectorsScope,
    frame: FrameScope,
    tree: TreeScope,
    devices: DevicesScope,
}

/// The views worked out for another document state ([`Views::detached`]): not kept, not
/// updated, dropped after use.
#[derive(Debug, Default)]
pub struct Detached {}

/// Every derived view the project keeps, in stage order (design 064 §5).
#[derive(Debug)]
pub struct Views {
    inputs: EngineInputs,
    // Stage 1: indexes that depend only on the document.
    connectors: ConnectorsView,
    // Stage 3: pages.
    frame: FrameView,
    // Stage 4: feeds for the change log.
    tree: TreeFeed,
    devices: DevicesFeed,
}

impl Views {
    /// No view built yet; [`Views::rebuild`] builds the kept ones.
    pub fn new(inputs: EngineInputs) -> Self {
        Views { inputs, connectors: ConnectorsView::default(), frame: FrameView::default(), tree: TreeFeed, devices: DevicesFeed::default() }
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

    /// The kept connector index (design 044 §11.1).
    pub fn connectors(&self) -> &crate::connectors::Index {
        self.connectors.index()
    }

    /// Before phase: every kept view notes `ops` on the document before them. Changes nothing.
    pub fn begin(&self, before: &Document, ops: &[Op]) -> Pending {
        let effects = Effects::of(ops);
        let cx = NoteCx { doc: before, phase: Phase::Before, effects: &effects };
        let mut pending = Pending::default();
        note_view::<_, Inputs<'_>>(&self.connectors, &cx, &mut pending.connectors);
        note_view::<_, Inputs<'_>>(&self.frame, &cx, &mut pending.frame);
        note_view::<_, Inputs<'_>>(&self.tree, &cx, &mut pending.tree);
        note_view::<_, Inputs<'_>>(&self.devices, &cx, &mut pending.devices);
        pending
    }

    /// After phase on the committed transaction's ops, then update every kept view, stage by
    /// stage, and merge their outcomes.
    pub fn finish(&mut self, after: &Document, tx_ops: &[Op], pending: Pending) -> Changes {
        let Pending { mut connectors, mut frame, mut tree, mut devices } = pending;
        let effects = Effects::of(tx_ops);
        let cx = NoteCx { doc: after, phase: Phase::After, effects: &effects };
        note_view::<_, Inputs<'_>>(&self.connectors, &cx, &mut connectors);
        note_view::<_, Inputs<'_>>(&self.frame, &cx, &mut frame);
        note_view::<_, Inputs<'_>>(&self.tree, &cx, &mut tree);
        note_view::<_, Inputs<'_>>(&self.devices, &cx, &mut devices);
        let inputs = Inputs { engine: &self.inputs };
        let mut changes = Changes::default();
        // Stage 1: the indexes.
        merge(&mut changes, self.connectors.update(after, connectors, &inputs));
        // Stage 3: pages.
        merge(&mut changes, self.frame.update(after, frame, &inputs));
        // Stage 4: the feeds, merged in field order.
        merge(&mut changes, self.tree.update(after, tree, &inputs));
        merge(&mut changes, self.devices.update(after, devices, &inputs));
        changes.devices = self.devices.take();
        if ecad_model::ledger::verify_mode() {
            changes.problems.extend(self.verify(after));
        }
        changes
    }

    /// Verify mode: every kept view against a fresh build; one line per disagreement (the view
    /// is rebuilt).
    pub fn verify(&mut self, doc: &Document) -> Vec<String> {
        self.connectors.check(doc).into_iter().collect()
    }

    /// Open, reload, branch switch: rebuild every kept view from `doc`.
    pub fn rebuild(&mut self, doc: &Document) {
        self.connectors = ConnectorsView::build(doc);
        self.frame.forget();
    }

    /// The views of another document state (an issued or finalised revision), built with the
    /// same engine inputs.
    pub fn detached(&self, _doc: &Document) -> Detached {
        Detached {}
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
