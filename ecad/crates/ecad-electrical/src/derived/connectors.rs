//! The connector index view (design 064 §4.1, `ConnectorsView`): the kept
//! [`crate::connectors::Index`] (which connectors each page shows, and which inherit each naming
//! key from the project), and the pages a project naming or description default redraws.
//!
//! # Update map
//! After the commit (each looked up on the document after the ops):
//! - A connection point added or removed (or both, a relabel): its device.
//! - A representation linked, unlinked or changed: the devices it shows, before and after.
//! - An item added: the device its representation shows (a symbol placed).
//! - A device created, deleted or changed, its definition switched, or any property of it
//!   written (every area: its naming keys): that device.
//! - A definition embedded or removed: the devices using it.
//! - A project `connector.` naming or description key: the pages of the connectors that inherit
//!   it (design 044 §11.1), worked out after the index is brought up to date.
//! - Every other effect changes nothing.
//!
//! Before the commit it notes nothing. Open, reload and branch switch build it whole
//! ([`ConnectorsView::build`]); verify mode compares it with a fresh build.

use std::collections::BTreeSet;

use ecad_model::effect::{AreaSet, Effect};
use ecad_model::view::{DerivedView, NoteCx, Outcome, PageSet, Phase, ViewScope};
use ecad_model::{DeviceId, Document, ObjectRef, Representation, Subject};

use super::Inputs;
use crate::connectors::Index;

/// The connector index view.
#[derive(Debug, Default)]
pub struct ConnectorsView {
    index: Index,
}

/// The connectors to recompute, and the project keys whose inheritors' pages redraw.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ConnectorsScope {
    pub devices: BTreeSet<DeviceId>,
    pub named: BTreeSet<String>,
}

impl ViewScope for ConnectorsScope {
    fn is_empty(&self) -> bool {
        self.devices.is_empty() && self.named.is_empty()
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

    /// Verify mode: when the kept index differs from a fresh build of `doc`, rebuild it and say so.
    pub fn check(&mut self, doc: &Document) -> Option<String> {
        let built = Index::build(doc);
        (self.index != built).then(|| {
            self.index = built;
            "the kept connector index differed from a rebuild; rebuilt".to_string()
        })
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
        let doc = cx.doc;
        let out = &mut scope.devices;
        let subject = |r: &Representation, out: &mut BTreeSet<DeviceId>| {
            if let Subject::Device(c) = r.subject {
                out.insert(c);
            }
        };
        match effect {
            // Design 072 §2.6: a device link is a relationship, not drawn.
            Effect::DeviceLink { .. } => {}
            Effect::Terminal { before, after, .. } => {
                out.extend(before.iter().chain(after.iter()).map(|t| t.component));
            }
            Effect::Rep { before, after, .. } => {
                for r in before.iter().chain(after.iter()) {
                    subject(r, out);
                }
            }
            Effect::Item { id, before: None, after: Some(_) } => {
                if let Some((_, r)) = doc.representation_of_item(*id) {
                    subject(r, out);
                }
            }
            Effect::Device { id, .. } | Effect::DeviceDefinition { id, .. } => {
                out.insert(*id);
            }
            Effect::Prop { object: ObjectRef::Device(c), .. } => {
                out.insert(*c);
            }
            Effect::Definition { id, .. } => out.extend(doc.components_using(*id)),
            Effect::Prop { object: ObjectRef::Project, key, .. } if redraws_inheritors(key.raw) => {
                scope.named.insert(key.raw.to_string());
            }
            Effect::Item { .. }
            | Effect::Prop { .. }
            | Effect::Page { .. }
            | Effect::Layer { .. }
            | Effect::Wire { .. }
            | Effect::Route { .. }
            | Effect::Cable { .. }
            | Effect::Settings { .. }
            | Effect::Version { .. }
            | Effect::Folder { .. }
            | Effect::Style { .. }
            | Effect::Standard { .. }
            | Effect::Project { .. } => {}
        }
    }

    fn update(&mut self, doc: &Document, scope: ConnectorsScope, _cx: &Inputs<'i>) -> Outcome {
        self.index.update(doc, &scope.devices);
        let mut pages = BTreeSet::new();
        for key in &scope.named {
            pages.extend(self.index.pages_inheriting(key));
        }
        Outcome { pages: PageSet::Only(pages), ..Outcome::default() }
    }

    fn verify(&self, doc: &Document, _cx: &Inputs<'i>) -> Option<String> {
        (self.index != Index::build(doc)).then(|| "the kept connector index differed from a rebuild".to_string())
    }
}
