//! The connector index view (design 064 §4.1, `ConnectorsView`): the kept
//! [`crate::connectors::Index`] (which connectors each page shows, and which inherit each naming
//! key from the project), and the pages a project naming or description default redraws.
//!
//! # No update map
//! A full build takes 0.4 ms on the large project (end-gate review, 2026-11-21), under the ~5 ms
//! bar, so the index is built again from scratch after every commit. Its note only collects the
//! project `connector.` naming and description keys written, whose inheritors' pages redraw
//! (design 044 §11.1), worked out on the rebuilt index.

use std::collections::BTreeSet;

use ecad_model::document::effect::{AreaSet, Effect};
use ecad_model::document::view::{DerivedView, NoteCx, Outcome, PageSet, Phase, ViewScope};
use ecad_model::{Document, ObjectRef};

use crate::connectors::Index;
use crate::views::Inputs;

/// The connector index view.
#[derive(Debug, Default)]
pub struct ConnectorsView {
    index: Index,
}

/// The project keys whose inheritors' pages redraw.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ConnectorsScope {
    pub named: BTreeSet<String>,
}

impl ViewScope for ConnectorsScope {
    fn is_empty(&self) -> bool {
        self.named.is_empty()
    }

    fn is_all(&self) -> bool {
        false
    }
}

impl ConnectorsView {
    /// The whole index of `doc`.
    pub fn build(doc: &Document) -> Self {
        ConnectorsView { index: Index::build(doc) }
    }

    /// The kept index.
    pub fn index(&self) -> &Index {
        &self.index
    }
}

/// Whether `key` is a project key whose inheritors' pages redraw (design 044 §11.1).
fn redraws_inheritors(key: &str) -> bool {
    key.starts_with("connector.") && (crate::defaults::connectors::is_naming_key(key) || key.ends_with(".description"))
}

impl<'i> DerivedView<Inputs<'i>> for ConnectorsView {
    type Scope = ConnectorsScope;
    const NAME: &'static str = "connectors";
    const AREAS: AreaSet = AreaSet::ALL;

    fn note(&self, cx: &NoteCx<'_>, effect: &Effect<'_>, scope: &mut ConnectorsScope) {
        if cx.phase != Phase::After {
            return;
        }
        if let Effect::Prop { object: ObjectRef::Project, key, .. } = effect
            && redraws_inheritors(key.raw)
        {
            scope.named.insert(key.raw.to_string());
        }
    }

    fn update(&mut self, doc: &Document, scope: ConnectorsScope, _cx: &Inputs<'i>) -> Outcome {
        self.index = Index::build(doc);
        let mut pages = BTreeSet::new();
        for key in &scope.named {
            pages.extend(self.index.pages_inheriting(key));
        }
        Outcome { pages: PageSet::Only(pages), ..Outcome::default() }
    }

    fn verify(&self, _doc: &Document, _cx: &Inputs<'i>) -> Option<String> {
        None
    }
}
