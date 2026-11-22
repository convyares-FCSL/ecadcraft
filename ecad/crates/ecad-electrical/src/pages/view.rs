//! The drawn pages' feed (design 064 §4.1, `PagesView`): the pages whose drawing a commit could
//! change, for the change log and the redraw, and the pages its ops name directly, one of which an
//! undo or redo brings to the front ([`PagesView::take_named`]). It keeps nothing between commits.
//!
//! # Update map
//! Before the commit, for the ops given to `begin`, each looked up on the document before them:
//! - An item, a representation or a route added, removed or changed: its pages, before and after
//!   (also the pages the ops name).
//! - A page changed in place: that page, and when its number changed the page before it in book
//!   order (its Next box); its folder changed: every page (the book order).
//! - A property of a page: that page.
//! - A device changed, its definition switched or a property of it written: the pages of its
//!   symbols; a rack's tag or a PLC module's place also its cards' pages
//!   (`crate::plc::pages_touched`, `crate::plc::pages_modified`, walking the station's modules
//!   through the document's index). A continuation made or removed: nothing (its own symbol
//!   ops and the reference index name its pages). A name or kind written on a device that is
//!   not a continuation: every page.
//! - A new device, connection point, wire or definition: nothing (it is drawn through the objects
//!   that show it). A connection point removed or relabelled: its device's symbols' pages.
//! - A wire removed: the pages of the routes at its ends; changed, or a property of it written:
//!   the pages its labels are on.
//! - A cable changed: its slashes' pages and its cores' pages; its cores changed: every page.
//! - A definition removed that nothing shows after the transaction: nothing.
//! - The project's data: the pages of the devices, boxes and pages storing a renamed list entry;
//!   every page when the info, the revisions or a default is changed.
//! - A project property: a tag-numbering high mark, a connector default (`ConnectorsView` names
//!   those pages) or a strip accessory default: nothing; any other: every page. A standard's value
//!   is a project property while the project holds no value of its own.
//! - A style record: its users' pages (`crate::styles::pages_touched`); the standard's header:
//!   nothing.
//! - Every other effect (a page, layer or folder added, removed or moved, a device deleted, a
//!   cable made or removed, a cable property, the settings, a version): every page.
//!
//! After the commit, the effects past those the Before phase saw (the transaction's extension: the
//! wire rewrite, design 072 §4.4, and the numbering pass, brief 022), each looked up on the
//! document after them: a wire made or removed, the pages of the routes at its ends; changed, or
//! a property of it written, the pages its labels are on. Nothing else.

use std::collections::BTreeSet;

use ecad_model::effect::{AreaSet, Effect, Effects};
use ecad_model::view::{DerivedView, NoteCx, Outcome, PageSet, Phase, ViewScope};
use ecad_model::{DefinitionId, Document, ItemKind, ObjectRef, Op, PageId, Subject};

use super::Inputs;

/// The drawn pages' feed.
#[derive(Debug, Default)]
pub struct PagesView {
    named: BTreeSet<PageId>,
}

impl PagesView {
    /// The pages the last commit's ops named directly, taken for the change log.
    pub fn take_named(&mut self) -> BTreeSet<PageId> {
        std::mem::take(&mut self.named)
    }
}

/// What the pages' feed found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagesScope {
    /// The pages whose drawing could change.
    pub pages: PageSet,
    /// The pages the ops name directly (items and representations).
    pub named: BTreeSet<PageId>,
    /// How many effects the Before phase saw; the After phase reads only those past them.
    seen: usize,
    /// How many effects the After phase has been given so far.
    after: usize,
}

impl Default for PagesScope {
    fn default() -> Self {
        PagesScope { pages: PageSet::Only(BTreeSet::new()), named: BTreeSet::new(), seen: 0, after: 0 }
    }
}

impl ViewScope for PagesScope {
    fn is_empty(&self) -> bool {
        self.pages.is_empty() && self.named.is_empty()
    }

    fn is_all(&self) -> bool {
        self.pages.as_option().is_none()
    }
}

/// Add `pages` to `set`.
fn extend(set: &mut PageSet, pages: impl IntoIterator<Item = PageId>) {
    for p in pages {
        set.insert(p);
    }
}

/// The pages of `s`'s representations in `doc`.
fn subject_pages(doc: &Document, s: Subject) -> impl Iterator<Item = PageId> + '_ {
    doc.representations_of(s).map(|(_, r)| r.page)
}

/// The pages where the routes landing on connection points `points` are drawn (a wire's labels),
/// through the index.
pub fn points_pages(doc: &Document, points: &[ecad_model::ConnectionPointId]) -> BTreeSet<PageId> {
    points.iter().flat_map(|t| doc.routes_at(*t)).filter_map(|r| doc.routes.get(&r).map(|x| x.page)).collect()
}

/// Brief 048 (FP8): whether definition `id` is shown by nothing after `effects` (its last copies
/// moved off it or removed in the same transaction); `doc` is the document before them.
fn unused_after(doc: &Document, id: DefinitionId, effects: &Effects<'_>) -> bool {
    let item_left = |i: &ecad_model::ItemId| {
        // Removed, or changed to show another definition.
        !effects.iter().any(|e| {
            if let Effect::Item { id: x, before, after } = e {
                x == i
                    && match after {
                        None => true,
                        Some(a) => before.is_some() && !matches!(&a.kind, ItemKind::Symbol(sym) if sym.definition == id),
                    }
            } else {
                false
            }
        })
    };
    let comp_left = |c: &ecad_model::DeviceId| {
        // Deleted, or switched to another definition.
        !effects.iter().any(|e| {
            if let Effect::Device { id: x, after: None, .. } = e {
                x == c
            } else if let Effect::DeviceDefinition { id: x, after, .. } = e {
                x == c && *after != Some(id)
            } else {
                false
            }
        })
    };
    !doc.items_using(id).iter().any(item_left) && !doc.components_using(id).iter().any(comp_left)
}

/// Whether a project property write of `key` is drawn on no page (`false`: on every page).
fn project_key_drawn_nowhere(key: &str) -> bool {
    key.starts_with(ecad_model::tag::HIGH_RESET_PREFIX)
        // Brief 044 (design 044 §11.1): a connector default's pages come from `ConnectorsView`.
        || key.starts_with("connector.")
        // Brief 030: strip accessory defaults (the plan page is a generated page, kept by its own rule).
        || crate::accessories::strip_key_of_project(key).is_some()
}

/// Brief 052 (design 052 §8.2a): what a change of the project's data redraws, read in `doc`
/// before it.
fn project_pages(doc: &Document, before: &ecad_model::project::Project, after: &ecad_model::project::Project, out: &mut PageSet) {
    use crate::designations::{Aspect, changed_codes, default_key, page_key};
    // Design 072 §5.3: the info and the revision rows are drawn in every page's frame.
    if before.info != after.info || before.revisions != after.revisions {
        *out = PageSet::All;
        return;
    }
    for a in Aspect::ALL {
        // A new default is drawn on every page that sets none.
        if default_key(before, a) != default_key(after, a) {
            *out = PageSet::All;
            return;
        }
        for entry in changed_codes(before, after, a) {
            if default_key(after, a).as_deref() == Some(entry.as_str()) {
                *out = PageSet::All;
                return;
            }
            for (c, x) in &doc.devices {
                if crate::designations::device_keys(x).iter().any(|(_, v)| *v == entry) {
                    extend(out, subject_pages(doc, Subject::Device(*c)));
                }
            }
            extend(out, doc.pages.iter().filter(|(_, x)| page_key(x, a).as_deref() == Some(entry.as_str())).map(|(p, _)| *p));
        }
    }
}

/// The Before phase: what `effect` could redraw, read in `doc` before the ops.
fn before(doc: &Document, effects: &Effects<'_>, effect: &Effect<'_>, scope: &mut PagesScope) {
    let out = &mut scope.pages;
    match effect {
        // Design 072 §2.6: a device link is a relationship, drawn on no page.
        Effect::DeviceLink { .. } => {}
        Effect::Item { before, after, .. } => {
            for p in before.iter().chain(after.iter()).map(|i| i.page) {
                out.insert(p);
                scope.named.insert(p);
            }
        }
        Effect::Rep { before, after, .. } => {
            for p in before.iter().chain(after.iter()).map(|r| r.page) {
                out.insert(p);
                scope.named.insert(p);
            }
        }
        Effect::Route { before, after, .. } => extend(out, before.iter().chain(after.iter()).map(|r| r.page)),
        // Brief 049 (design 049 §8.2): a continuation's partners list it on their pages (the
        // reference index's part), and its own pages come from its representation effects. A
        // location box's pages likewise (its label and the tag prefixes of the devices inside are
        // drawn on its own pages only).
        Effect::Aid { before: None, .. } | Effect::Aid { after: None, .. } => {}
        // A continuation renamed or captioned, a box's values changed: its own pages (design 072
        // §6.2).
        Effect::Aid { id, .. } => extend(out, subject_pages(doc, Subject::Aid(*id))),
        // A loose part is drawn on no page (design 072 §6.4).
        Effect::LoosePart { .. } => {}
        Effect::Device { id, before: None, after: Some(_) } if !doc.devices.contains_key(id) => {}
        Effect::Wire { id, before: None, after: Some(_) } if !doc.wires.contains_key(id) => {}
        Effect::Wire { before: Some(w), after: None, .. } => extend(out, points_pages(doc, &w.ends)),
        Effect::Wire { id, before: Some(_), after: Some(_) } => extend(out, crate::wires::wire_pages(doc, *id)),
        Effect::Prop { object: ObjectRef::Wire(c), .. } => extend(out, crate::wires::wire_pages(doc, *c)),
        // Brief 028: a connection point relabelled (removed and added) is drawn by its device's
        // symbols only. A new one is drawn through its device.
        Effect::Terminal { id, before: None, after: Some(_) } if !doc.terminals.contains_key(id) => {}
        Effect::Terminal { before, after, .. } => {
            for t in before.iter().chain(after.iter()) {
                extend(out, subject_pages(doc, t.owner.subject()));
            }
        }
        Effect::Definition { id, before: None, after: Some(_) } if !doc.definitions.contains_key(id) => {}
        // Brief 048 (FP8): a superseded version nothing shows after the transaction is drawn
        // nowhere; its last copies' own effects name their pages.
        Effect::Definition { id, before: Some(_), after: None } if unused_after(doc, *id, effects) => {}
        // A page's own properties show in its frame (brief 019). A folder move changes the book
        // order (Next boxes, unnumbered pages) anywhere.
        Effect::Page { id: p, before: Some(b), after: Some(a), index_before: None, index_after: None } => {
            let ch = b.change(a);
            if ch.folder {
                *out = PageSet::All;
                return;
            }
            out.insert(*p);
            // The page before it in book order shows its number in its Next box.
            if ch.number {
                let book = ecad_model::book::Book::of(doc);
                if let Some(prev) = book.position(*p).and_then(|i| i.checked_sub(2)).and_then(|i| book.order.get(i)) {
                    out.insert(*prev);
                }
            }
        }
        Effect::Prop { object: ObjectRef::Page(p), .. } => out.insert(*p),
        // Brief 018: a device's tag is drawn beside each of its symbols.
        Effect::Prop { object: ObjectRef::Device(c), key, after, .. } => {
            extend(out, subject_pages(doc, Subject::Device(*c)));
            // Brief 045 (design 045 §11.1): a rack's tag shows in its cards' captions, and a
            // card's slot or size moves the addresses of the cards after it.
            extend(out, crate::plc::pages_touched(doc, None, *c, key.raw, *after));
        }
        Effect::DeviceDefinition { id, .. } => extend(out, subject_pages(doc, Subject::Device(*id))),
        // Design 072: a device's typed fields show beside its symbols; a PLC station's or module's
        // fields (§3) on its station's cards' pages.
        Effect::Device { id: c, before: Some(b), after: Some(a) } => {
            extend(out, subject_pages(doc, Subject::Device(*c)));
            if b.tag != a.tag {
                extend(out, crate::plc::pages_touched(doc, None, *c, ecad_model::tag::TAG_KEY, a.tag()));
            }
            if crate::plc::is_plc(b) || crate::plc::is_plc(a) {
                extend(out, crate::plc::pages_modified(doc, None, *c, b, a));
            }
        }
        // Design 072 §4.5: a cable's fields are drawn by its slashes and its cores' lines; a
        // change of its cores redraws every page.
        Effect::Cable { id, before: Some(b), after: Some(a) } => {
            if b.cores != a.cores {
                *out = PageSet::All;
                return;
            }
            extend(out, subject_pages(doc, Subject::Cable(*id)));
            for m in &a.cores {
                extend(out, crate::cables::member_pages(doc, m));
            }
        }
        Effect::Project { before, after } => project_pages(doc, before, after, out),
        Effect::Prop { object: ObjectRef::Project, key, .. } => {
            if !project_key_drawn_nowhere(key.raw) {
                *out = PageSet::All;
            }
        }
        // A standard's value acts like a project-level write of its key, and changes nothing on
        // a page while the project holds its own value.
        Effect::Prop { object: ObjectRef::Standard, key, .. } => {
            let hidden = crate::defaults::find(key.raw)
                .is_some_and(|e| crate::defaults::resolve(doc, e, crate::defaults::Scope::project()).from == crate::defaults::Source::Project);
            if !hidden && !project_key_drawn_nowhere(key.raw) {
                *out = PageSet::All;
            }
        }
        // Brief 050a (design 050 §10): a style record marks its users' pages (every page when the
        // project's choice resolves to it); the standard's header none.
        Effect::Style { layer, catalogue, id, before, after } => {
            let op = Op::SetStyle { layer: *layer, catalogue: *catalogue, id: (*id).clone(), before: before.cloned(), after: after.cloned() };
            match crate::styles::pages_touched(doc, &op) {
                Some(ps) => extend(out, ps),
                None => *out = PageSet::All,
            }
        }
        Effect::Standard { .. } => {}
        Effect::Page { .. }
        | Effect::Layer { .. }
        | Effect::Device { .. }
        | Effect::Wire { .. }
        | Effect::Definition { .. }
        | Effect::Cable { .. }
        | Effect::Prop { .. }
        | Effect::Settings { .. }
        | Effect::Version { .. }
        | Effect::Folder { .. } => *out = PageSet::All,
    }
}

/// The After phase, for an effect of the transaction's extension: the wire labels it moves, read
/// in `doc` after the commit.
fn extension(doc: &Document, effect: &Effect<'_>, out: &mut PageSet) {
    match effect {
        Effect::DeviceLink { .. } => {}
        Effect::Prop { object: ObjectRef::Wire(c), .. } | Effect::Wire { id: c, before: Some(_), after: Some(_) } => {
            extend(out, crate::wires::wire_pages(doc, *c));
        }
        // The rewrite's wires (design 072 §4.4): labelled on the routes at their ends.
        Effect::Wire { before: Some(w), after: None, .. } | Effect::Wire { before: None, after: Some(w), .. } => {
            extend(out, points_pages(doc, &w.ends));
        }
        Effect::Wire { .. }
        | Effect::Prop { .. }
        | Effect::Page { .. }
        | Effect::Layer { .. }
        | Effect::Item { .. }
        | Effect::Device { .. }
        | Effect::DeviceDefinition { .. }
        | Effect::Terminal { .. }
        | Effect::Rep { .. }
        | Effect::Route { .. }
        | Effect::Cable { .. }
        | Effect::Definition { .. }
        | Effect::Settings { .. }
        | Effect::Version { .. }
        | Effect::Folder { .. }
        | Effect::Style { .. }
        | Effect::Standard { .. }
        | Effect::Project { .. }
        | Effect::Aid { .. }
        | Effect::LoosePart { .. } => {}
    }
}

impl<'i> DerivedView<Inputs<'i>> for PagesView {
    type Scope = PagesScope;
    const NAME: &'static str = "pages";
    const AREAS: AreaSet = AreaSet::ALL;

    fn note(&self, cx: &NoteCx<'_>, effect: &Effect<'_>, scope: &mut PagesScope) {
        match cx.phase {
            Phase::Before => {
                scope.seen += 1;
                if !scope.is_all() {
                    before(cx.doc, cx.effects, effect, scope);
                } else if let Effect::Item { before, after, .. } = effect {
                    scope.named.extend(before.iter().chain(after.iter()).map(|i| i.page));
                } else if let Effect::Rep { before, after, .. } = effect {
                    scope.named.extend(before.iter().chain(after.iter()).map(|r| r.page));
                }
            }
            Phase::After => {
                scope.after += 1;
                if scope.after > scope.seen && !scope.is_all() {
                    extension(cx.doc, effect, &mut scope.pages);
                }
            }
        }
    }

    fn update(&mut self, _doc: &Document, scope: PagesScope, _cx: &Inputs<'i>) -> Outcome {
        self.named = scope.named;
        Outcome { pages: scope.pages, ..Outcome::default() }
    }

    fn verify(&self, _doc: &Document, _cx: &Inputs<'i>) -> Option<String> {
        None
    }
}
