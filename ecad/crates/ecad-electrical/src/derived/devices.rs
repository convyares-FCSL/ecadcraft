//! The Devices feed (design 064 §4.1, `DevicesFeed`): what a commit could change in the Devices
//! list and the symbol search ([`DeviceTouch`]), for the change log.
//!
//! # Update map
//! Before the ops (each looked up on the document before them):
//! - A device created, deleted or changed, its definition switched, or any property of it
//!   written (every area: the list shows its fields and properties): that device.
//! - A representation linked, unlinked or changed: the devices it shows, before and after.
//! - An item changed in place: the device its representation shows (a symbol moved).
//! - A project symbol's version embedded or removed: the devices showing its lineage ("update
//!   available").
//! - A frame effect (`title_block::frame_reach_of`): the devices on the pages it reaches, or every
//!   page's places (`pages`) when it reaches any page.
//! - A page changed in place: its number or sheet, the devices on it; its folder, see below.
//! - A page or a folder added, removed or moved, or a page refiled: the book order may change.
//!   The book positions before are kept, and after the commit the devices on every page whose
//!   position changed are added.
//! - The project settings: `settings` (the symbol search's prefixes and next tags); the sheet,
//!   the length unit or the reference style also `pages`.
//! - Every other effect (wires, ends, cables, members, connection points, layers, plain item adds
//!   and removes, styles other than frames, the project's own data, versions) changes nothing.
//!
//! After the commit it notes nothing: the numbering pass's ops renumber wires only.
//!
//! It keeps the change log's [`DeviceTouch`] of the last commit, which `Views::finish` takes.

use std::collections::BTreeMap;

use ecad_model::effect::{AreaSet, Effect};
use ecad_model::view::{DerivedView, NoteCx, Outcome, Phase, ViewScope};
use ecad_model::{DeviceId, Document, ObjectRef, PageId, Subject};

use super::Inputs;
use crate::device_list::DeviceTouch;
use crate::title_block::{Reach, frame_reach_of};

/// The Devices feed.
#[derive(Debug, Default)]
pub struct DevicesFeed {
    touch: DeviceTouch,
}

impl DevicesFeed {
    /// The last commit's touch, taken for the change log.
    pub fn take(&mut self) -> DeviceTouch {
        std::mem::take(&mut self.touch)
    }
}

/// What the Devices list must read again.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DevicesScope {
    pub touch: DeviceTouch,
    /// The book positions before the ops, kept when an effect may move pages.
    pub book_before: Option<BTreeMap<PageId, usize>>,
}

impl ViewScope for DevicesScope {
    fn is_empty(&self) -> bool {
        self.touch.leaves_search() && self.book_before.is_none()
    }

    fn is_all(&self) -> bool {
        false
    }
}

/// The devices shown on `page` of `doc`.
fn on_page(doc: &Document, page: PageId, out: &mut std::collections::BTreeSet<DeviceId>) {
    for (_, r) in doc.representations_on(page) {
        if let Subject::Device(c) = r.subject {
            out.insert(c);
        }
    }
}

impl<'i> DerivedView<Inputs<'i>> for DevicesFeed {
    type Scope = DevicesScope;
    const NAME: &'static str = "devices";
    const AREAS: AreaSet = AreaSet::ALL;

    fn note(&self, cx: &NoteCx<'_>, effect: &Effect<'_>, scope: &mut DevicesScope) {
        if cx.phase != Phase::Before {
            return;
        }
        let doc = cx.doc;
        let t = &mut scope.touch;
        // Brief 055 (FP4; review of #110, 1): a frame decides its pages' grids, so the places of
        // the devices on them.
        match frame_reach_of(doc, effect) {
            Reach::Pages(ps) => {
                for p in ps {
                    on_page(doc, p, &mut t.devices);
                }
            }
            Reach::Wide => t.pages = true,
            Reach::NotFrame | Reach::Unchanged => {}
        }
        let mut subject = |s: &Subject| {
            if let Subject::Device(c) = s {
                t.devices.insert(*c);
            }
        };
        let mut moves = false;
        match effect {
            // Design 072 §2.6: a device link is not shown in the list.
            Effect::DeviceLink { .. } => {}
            Effect::Device { id, .. } | Effect::DeviceDefinition { id, .. } => subject(&Subject::Device(*id)),
            Effect::Prop { object: ObjectRef::Device(c), .. } => subject(&Subject::Device(*c)),
            Effect::Rep { before, after, .. } => {
                for r in before.iter().chain(after.iter()) {
                    subject(&r.subject);
                }
            }
            Effect::Item { id, before: Some(_), after: Some(_) } => {
                if let Some((_, r)) = doc.representation_of_item(*id) {
                    subject(&r.subject);
                }
            }
            // Brief 048: a project symbol's new or removed version changes "update available" on
            // the devices showing its lineage.
            Effect::Definition { before, after, .. } => {
                let origin = after.or(*before).and_then(|d| d.origin.as_ref()).filter(|o| o.library == crate::symbols::PROJECT_SYMBOLS);
                if let Some(o) = origin {
                    for v in crate::symbols::versions_of(doc, o.definition) {
                        for item in doc.items_using(v.id) {
                            if let Some((_, r)) = doc.representation_of_item(item) {
                                subject(&r.subject);
                            }
                        }
                    }
                }
            }
            // Brief 019: a page's number or sheet changes the place of the symbols on it; the book
            // order (folders) may change every place.
            Effect::Page { id, before: Some(b), after: Some(a), index_before: None, index_after: None } => {
                let ch = b.change(a);
                if ch.number || ch.sheet {
                    on_page(doc, *id, &mut t.devices);
                }
                moves = ch.folder;
            }
            Effect::Page { .. } | Effect::Folder { .. } => moves = true,
            Effect::Settings { before, after } => {
                t.settings = true;
                if before.sheet != after.sheet || before.length_unit != after.length_unit || before.reference_style != after.reference_style {
                    t.pages = true;
                }
            }
            Effect::Item { .. }
            | Effect::Prop { .. }
            | Effect::Layer { .. }
            | Effect::Terminal { .. }
            | Effect::Wire { .. }
            | Effect::Route { .. }
            | Effect::Cable { .. }
            | Effect::Version { .. }
            | Effect::Style { .. }
            | Effect::Standard { .. }
            | Effect::Project { .. } => {}
        }
        if moves && scope.book_before.is_none() {
            scope.book_before = Some(ecad_model::book::Book::of(doc).positions);
        }
    }

    fn update(&mut self, doc: &Document, scope: DevicesScope, _cx: &Inputs<'i>) -> Outcome {
        let DevicesScope { mut touch, book_before } = scope;
        // Pages whose book position changed: only their symbols' places and order may differ; a
        // rename in place or an append moves none.
        if let Some(before) = book_before {
            let after = ecad_model::book::Book::of(doc).positions;
            for (p, i) in &before {
                if after.get(p) != Some(i) {
                    on_page(doc, *p, &mut touch.devices);
                }
            }
        }
        self.touch = touch;
        Outcome::default()
    }

    fn verify(&self, _doc: &Document, _cx: &Inputs<'i>) -> Option<String> {
        None
    }
}
