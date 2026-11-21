//! The Properties feed (design 064 §4.1, `PropsFeed`): the objects whose shown properties a
//! commit could change, so the change log tells the Properties palette (ecad-cadcraft's
//! `props::PropsView`, which names the objects its rows read) whether to read them again.
//!
//! # Update map
//! Before the ops (each looked up on the document before them):
//! - A page changed in place (`ModifyPage`): that page's own properties.
//! - A property written (every area): its object; on a location box (brief 024) also the pages
//!   its symbols are on, whose devices inherit its location.
//! - A device created, deleted or changed, its definition switched, or a connection point of it
//!   added or removed: that device.
//! - A wire made, removed or changed: that wire; made or removed, also the devices at its ends,
//!   which show its net (brief 022).
//! - A cable made, removed or changed: that cable and the wires of its cores, before and after.
//! - A route drawn, removed or changed: the devices it lands on, before and after (a route has no
//!   properties, design 072 F31).
//! - An item changed in place or removed: the subject its representation shows (a location box's
//!   also its page). An item added: nothing (it is shown through its representation).
//! - A representation linked, unlinked or changed: its subjects; linked or unlinked on a location
//!   box (or with the location-box role), also its page.
//! - A version named or unnamed, a style record or the standard: the project.
//! - A project symbol's version embedded or removed (brief 048): the subjects of every item using
//!   one of its versions ("update available").
//! - A layer: nothing.
//! - A page added, removed or moved, any folder effect, the settings, the project's own data
//!   (design 072 §5.3: a renamed code shows on every object storing it) or a device link: any
//!   object (`None`).
//!
//! After the commit, the effects past those the Before phase saw (the transaction's extension: the
//! wire rewrite, design 072 §4.4, and the numbering pass, brief 022), each looked up on the
//! document after them: a property written, its object; a wire made, removed or changed, that
//! wire and the devices at its ends (after the change). Nothing else.
//!
//! It keeps the change log's objects of the last commit, which `Views::finish` takes.

use std::collections::BTreeSet;

use ecad_model::effect::{AreaSet, Effect, Effects};
use ecad_model::view::{DerivedView, NoteCx, Outcome, Phase, ViewScope};
use ecad_model::{Document, ObjectRef, Op, Subject, TerminalId};

use super::Inputs;

/// The Properties feed.
#[derive(Debug)]
pub struct PropsFeed {
    objects: Option<BTreeSet<ObjectRef>>,
}

impl Default for PropsFeed {
    fn default() -> Self {
        PropsFeed { objects: Some(BTreeSet::new()) }
    }
}

impl PropsFeed {
    /// The last commit's objects (`None`: any), taken for the change log.
    pub fn take(&mut self) -> Option<BTreeSet<ObjectRef>> {
        self.objects.replace(BTreeSet::new())
    }

    /// The objects whose shown properties `ops` could change, by the Before phase alone, each
    /// looked up in `doc` (the agent's outline of a past transaction reads them on the document
    /// as it is now). `None`: any object.
    pub fn touched_by(doc: &Document, ops: &[Op]) -> Option<BTreeSet<ObjectRef>> {
        let mut out = Some(BTreeSet::new());
        for effect in &Effects::of(ops) {
            before(doc, effect, &mut out);
        }
        out
    }
}

/// The objects found so far (`None`: any).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropsScope {
    pub objects: Option<BTreeSet<ObjectRef>>,
    /// How many effects the Before phase saw; the After phase reads only those past them.
    seen: usize,
    /// How many effects the After phase has been given so far.
    after: usize,
}

impl Default for PropsScope {
    fn default() -> Self {
        PropsScope { objects: Some(BTreeSet::new()), seen: 0, after: 0 }
    }
}

impl ViewScope for PropsScope {
    fn is_empty(&self) -> bool {
        self.objects.as_ref().is_some_and(BTreeSet::is_empty)
    }

    fn is_all(&self) -> bool {
        self.objects.is_none()
    }
}

/// The devices of the connection points `ends`, in `doc`.
fn devices_at<'a>(doc: &'a Document, ends: impl Iterator<Item = TerminalId> + 'a) -> impl Iterator<Item = ObjectRef> + 'a {
    ends.filter_map(|t| doc.terminals.get(&t)).map(|t| ObjectRef::Device(t.component))
}

/// The object a subject's properties are kept on (a route has none, design 072 F31).
fn subject_ref(s: Subject) -> Option<ObjectRef> {
    match s {
        Subject::Device(c) => Some(ObjectRef::Device(c)),
        Subject::Wire(c) => Some(ObjectRef::Wire(c)),
        Subject::Cable(c) => Some(ObjectRef::Cable(c)),
        Subject::Route(_) => None,
    }
}

/// The Before phase for one effect, looked up in `doc` before the ops.
fn before(doc: &Document, effect: &Effect<'_>, objects: &mut Option<BTreeSet<ObjectRef>>) {
    let Some(out) = objects.as_mut() else { return };
    let is_box = |s: Subject| matches!(s, Subject::Device(c) if crate::locations::is_box(doc, c));
    match effect {
        Effect::Page { id, before: Some(_), after: Some(_), index_before: None, index_after: None } => {
            out.insert(ObjectRef::Page(*id));
        }
        Effect::Prop { object, .. } => {
            out.insert(*object);
            // A location box's own location is what its page's devices inherit (brief 024).
            if let ObjectRef::Device(c) = object
                && crate::locations::is_box(doc, *c)
            {
                out.extend(doc.representations_of(Subject::Device(*c)).map(|(_, r)| ObjectRef::Page(r.page)));
            }
        }
        Effect::Device { id, .. } | Effect::DeviceDefinition { id, .. } => {
            out.insert(ObjectRef::Device(*id));
        }
        Effect::Terminal { before, after, .. } => {
            out.extend(before.iter().chain(after.iter()).map(|t| ObjectRef::Device(t.component)));
        }
        Effect::Wire { id, before: Some(_), after: Some(_) } => {
            out.insert(ObjectRef::Wire(*id));
        }
        Effect::Wire { id, before, after } => {
            out.insert(ObjectRef::Wire(*id));
            // The devices at its ends: a wire's net (brief 022) is shown through them.
            let ends = before.iter().chain(after.iter()).flat_map(|w| w.ends.iter().copied());
            out.extend(devices_at(doc, ends));
        }
        Effect::Cable { id, before, after } => {
            out.insert(ObjectRef::Cable(*id));
            out.extend(before.iter().chain(after.iter()).flat_map(|c| c.cores.iter()).filter_map(|m| m.wire()).map(ObjectRef::Wire));
        }
        // A route has no properties (design 072 F31); the devices it lands on show its net.
        Effect::Route { before, after, .. } => {
            let landed = before.iter().chain(after.iter()).flat_map(|r| r.points_landed());
            out.extend(devices_at(doc, landed));
        }
        // A new item is shown through its representation (linked in the same transaction).
        Effect::Item { before: None, .. } => {}
        // A moved or removed one changes its subject's place.
        Effect::Item { id, .. } => {
            if let Some((_, r)) = doc.representation_of_item(*id) {
                out.extend(subject_ref(r.subject));
                if is_box(r.subject) {
                    out.insert(ObjectRef::Page(r.page));
                }
            }
        }
        Effect::Rep { before: Some(b), after: Some(a), .. } => {
            out.extend(subject_ref(b.subject));
            out.extend(subject_ref(a.subject));
        }
        Effect::Rep { before, after, .. } => {
            for rep in before.iter().chain(after.iter()) {
                out.extend(subject_ref(rep.subject));
                // A location box (brief 024) changes the location its page's devices inherit.
                if is_box(rep.subject) || (matches!(rep.subject, Subject::Device(_)) && rep.role == ecad_model::Role::LocationBox) {
                    out.insert(ObjectRef::Page(rep.page));
                }
            }
        }
        // Brief 050a: styles and the standard show in the property views of the project.
        Effect::Version { .. } | Effect::Style { .. } | Effect::Standard { .. } => {
            out.insert(ObjectRef::Project);
        }
        // Brief 048 (design §8.4): a project symbol's new or removed version changes whether the
        // devices showing its lineage read "update available".
        Effect::Definition { before, after, .. } => {
            for d in before.iter().chain(after.iter()) {
                if let Some(o) = d.origin.as_ref().filter(|o| o.library == crate::symbols::PROJECT_SYMBOLS) {
                    for v in crate::symbols::versions_of(doc, o.definition) {
                        for item in doc.items_using(v.id) {
                            if let Some((_, r)) = doc.representation_of_item(item) {
                                out.extend(subject_ref(r.subject));
                            }
                        }
                    }
                }
            }
        }
        Effect::Layer { .. } => {}
        Effect::Page { .. }
        | Effect::Folder { .. }
        | Effect::Settings { .. }
        // Design 072 §5.3: a code renamed in the project's list shows on every object storing it.
        | Effect::Project { .. }
        | Effect::DeviceLink { .. } => *objects = None,
    }
}

/// The After phase, for an effect of the transaction's extension, looked up in `doc` after it.
fn extension(doc: &Document, effect: &Effect<'_>, out: &mut BTreeSet<ObjectRef>) {
    match effect {
        Effect::Prop { object, .. } => {
            out.insert(*object);
        }
        Effect::Wire { id, before, after } => {
            out.insert(ObjectRef::Wire(*id));
            // The devices at its ends show its net (brief 022).
            if let Some(w) = after.or(*before) {
                out.extend(devices_at(doc, w.ends.iter().copied()));
            }
        }
        Effect::Page { .. }
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
        | Effect::DeviceLink { .. } => {}
    }
}

impl<'i> DerivedView<Inputs<'i>> for PropsFeed {
    type Scope = PropsScope;
    const NAME: &'static str = "props";
    const AREAS: AreaSet = AreaSet::ALL;

    fn note(&self, cx: &NoteCx<'_>, effect: &Effect<'_>, scope: &mut PropsScope) {
        match cx.phase {
            Phase::Before => {
                scope.seen += 1;
                before(cx.doc, effect, &mut scope.objects);
            }
            Phase::After => {
                scope.after += 1;
                if scope.after > scope.seen
                    && let Some(out) = scope.objects.as_mut()
                {
                    extension(cx.doc, effect, out);
                }
            }
        }
    }

    fn update(&mut self, _doc: &Document, scope: PropsScope, _cx: &Inputs<'i>) -> Outcome {
        self.objects = scope.objects;
        Outcome::default()
    }

    fn verify(&self, _doc: &Document, _cx: &Inputs<'i>) -> Option<String> {
        None
    }
}
