//! The protocol every derived view follows: [`DerivedView`].
//!
//! A derived view keeps state worked out from the document (a reference index, the checks list,
//! the report models) or names what the change log records (pages, devices, properties). Its
//! update map is its [`DerivedView::note`]: called once per [`Effect`] on the document before the
//! ops and once on the document after them, it adds to the view's scope what the effect could
//! change. [`DerivedView::update`] then brings the kept state in line for that scope only.
//!
//! This module excludes the views themselves and the pipeline that runs them (`Views`, in
//! ecad-electrical's `derived` module).

use std::collections::BTreeSet;

use crate::Document;
use crate::document::effect::{AreaSet, Effect, Effects};
use crate::naming::id::PageId;

/// Which side of the commit a [`NoteCx`] reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The document before the ops.
    Before,
    /// The document after the committed transaction.
    After,
}

/// What [`DerivedView::note`] may read besides the effect.
#[derive(Debug, Clone, Copy)]
pub struct NoteCx<'a> {
    /// Before: the state before the ops; After: the state after.
    pub doc: &'a Document,
    pub phase: Phase,
    /// The whole transaction (for pair-wide questions).
    pub effects: &'a Effects<'a>,
}

/// A view's scope: what it must redo. `Default` is "nothing".
pub trait ViewScope: Default + Send {
    /// Whether there is nothing to redo.
    fn is_empty(&self) -> bool;
    /// Whether the scope asks for a whole rebuild (allowed only for the ops on the update-map
    /// allowlist).
    fn is_all(&self) -> bool;
}

/// A set of pages, or every page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageSet {
    /// These pages only.
    Only(BTreeSet<PageId>),
    /// Any page.
    All,
}

impl Default for PageSet {
    fn default() -> Self {
        PageSet::Only(BTreeSet::new())
    }
}

impl PageSet {
    /// The set as the change log keeps it: `None` for every page.
    pub fn into_option(self) -> Option<BTreeSet<PageId>> {
        match self {
            PageSet::Only(s) => Some(s),
            PageSet::All => None,
        }
    }

    /// The set from the change log's form: `None` is every page.
    pub fn from_option(pages: Option<BTreeSet<PageId>>) -> Self {
        match pages {
            Some(s) => PageSet::Only(s),
            None => PageSet::All,
        }
    }

    /// The set as a reference: `None` for every page.
    pub fn as_option(&self) -> Option<&BTreeSet<PageId>> {
        match self {
            PageSet::Only(s) => Some(s),
            PageSet::All => None,
        }
    }

    /// Add one page.
    pub fn insert(&mut self, page: PageId) {
        if let PageSet::Only(s) = self {
            s.insert(page);
        }
    }

    /// Add `other`; every page when either is.
    pub fn union(&mut self, other: PageSet) {
        match (&mut *self, other) {
            (PageSet::Only(a), PageSet::Only(b)) => a.extend(b),
            (PageSet::All, PageSet::Only(_) | PageSet::All) => {}
            (PageSet::Only(_), PageSet::All) => *self = PageSet::All,
        }
    }

    /// Whether no page is named.
    pub fn is_empty(&self) -> bool {
        match self {
            PageSet::Only(s) => s.is_empty(),
            PageSet::All => false,
        }
    }
}

/// What a view's update reports to the pipeline.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    /// Pages whose drawing changed.
    pub pages: PageSet,
    /// The kept frame must be drawn again.
    pub frame_stale: bool,
    /// The page list or tree must be read again.
    pub list: bool,
    /// Lines for the command line (an orphaned reference, for example).
    pub echo: Vec<String>,
    /// Verify-mode disagreements (recorded as `IndexRebuilt`).
    pub problems: Vec<String>,
}

/// A view derived from the document and kept up to date by the commit pipeline.
///
/// `Cx` is what [`DerivedView::update`] reads from earlier stages and the engine (ecad-electrical's
/// `Inputs`).
pub trait DerivedView<Cx: ?Sized>: Send {
    type Scope: ViewScope;
    /// Its name in the update-map matrix, counters and the debugger.
    const NAME: &'static str;
    /// The property areas it reads: a property effect outside them never
    /// reaches [`DerivedView::note`].
    const AREAS: AreaSet;
    /// Whether it is kept now. A view built on first show returns false until then, and the
    /// pipeline then skips it entirely.
    fn kept(&self) -> bool {
        true
    }
    /// Add to `scope` what `effect` could change, read in `cx.doc`. Called once per effect in the
    /// Before phase and once in the After phase. Reads `&self` only.
    fn note(&self, cx: &NoteCx<'_>, effect: &Effect<'_>, scope: &mut Self::Scope);
    /// Bring the kept state in line with `doc` for `scope` only.
    fn update(&mut self, doc: &Document, scope: Self::Scope, cx: &Cx) -> Outcome;
    /// Verify mode: compare with a fresh build; `Some(why)` if they differ.
    fn verify(&self, doc: &Document, cx: &Cx) -> Option<String>;
}

// The commit pipeline may run views' notes and updates in parallel.
const _: fn() = || {
    fn is_sync<T: Sync>() {}
    is_sync::<Document>();
};
