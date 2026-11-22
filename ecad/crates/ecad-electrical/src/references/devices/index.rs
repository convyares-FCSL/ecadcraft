//! The general reference index (design 025 §3.3): each member's group and class, each group's members and edges.

use crate::CmdError;
use ecad_model::document::effect::{Effect, Effects};
use ecad_model::{DeviceId, Document, Op, PageId, Representation, RepresentationId, Subject};
use std::collections::{BTreeMap, BTreeSet};

use super::classify::{Class, Group, raw_class};

// ---------------------------------------------------------------- the index (design 025 §3.3)

/// The general reference index: each member's group and class, each group's members, and its
/// edges both ways. Derived, never saved.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefIndex {
    member: BTreeMap<RepresentationId, (Group, Class)>,
    members: BTreeMap<Group, BTreeSet<RepresentationId>>,
    targets: BTreeMap<RepresentationId, BTreeSet<RepresentationId>>,
    sources: BTreeMap<RepresentationId, BTreeSet<RepresentationId>>,
}

impl RefIndex {
    /// The whole index of `doc` (open, reload, branch switch): O(representations).
    pub fn build(doc: &Document) -> Self {
        // One pass in ID order: each member classified once (the first head of a device is its
        // head, as `make` decides), then each group's edges.
        let mut groups: BTreeMap<Group, Vec<(RepresentationId, Class)>> = BTreeMap::new();
        for (rid, r) in doc.representations() {
            let (g, c) = match r.subject {
                Subject::Cable(k) if crate::references::cables::is_reference(r) => (Group::Cable(k), Class::End),
                Subject::Device(c) => match raw_class(doc, r) {
                    Class::Other => continue,
                    class => (Group::Device(c), class),
                },
                _ => continue,
            };
            let list = groups.entry(g).or_default();
            let c = if c == Class::Head && list.iter().any(|(_, x)| *x == Class::Head) { Class::Other } else { c };
            list.push((*rid, c));
        }
        let mut idx = RefIndex::default();
        for (g, list) in groups {
            idx.insert_group(g, list);
        }
        // Brief 044: connectors' members and edges (design 044 §7).
        for c in doc.devices().keys() {
            if crate::connectors::connector_of(doc, *c).is_some_and(|x| x.mating) {
                idx.insert_connector(doc, *c);
            }
        }
        // Brief 049 (FP4): one group per continuation name, one pass over the name index.
        let names: BTreeSet<&str> = doc.continuation_names().map(|(k, _)| k).collect();
        for k in names {
            if let Some((g, list)) = crate::continuations::group_named(doc, k) {
                idx.insert_group(g, list.into_iter().map(|r| (r, Class::Arrow)).collect());
            }
        }
        idx
    }

    /// Add connector `c`'s group (design 044 §7): its halves and pins, and their edges.
    fn insert_connector(&mut self, doc: &Document, c: DeviceId) {
        let list = crate::connectors::ref_classes(doc, c);
        if list.is_empty() {
            return;
        }
        let edges = crate::connectors::ref_edges(doc, c, &list);
        let g = Group::Device(c);
        for (r, k) in &list {
            self.member.insert(*r, (g, *k));
        }
        self.members.insert(g, list.iter().map(|(r, _)| *r).collect());
        for (s, t) in edges {
            self.targets.entry(s).or_default().insert(t);
            self.sources.entry(t).or_default().insert(s);
        }
    }

    /// Take group `g` out: its members, classes and edges. O(group).
    fn remove(&mut self, g: Group) {
        if let Some(old) = self.members.remove(&g) {
            for r in old {
                self.member.remove(&r);
                for t in self.targets.remove(&r).unwrap_or_default() {
                    if let Some(s) = self.sources.get_mut(&t) {
                        s.remove(&r);
                        if s.is_empty() {
                            self.sources.remove(&t);
                        }
                    }
                }
                for s in self.sources.remove(&r).unwrap_or_default() {
                    if let Some(t) = self.targets.get_mut(&s) {
                        t.remove(&r);
                        if t.is_empty() {
                            self.targets.remove(&s);
                        }
                    }
                }
            }
        }
    }

    /// Put group `g` back from `doc` (after [`RefIndex::remove`]): its members, classes and edges.
    /// O(group).
    fn make(&mut self, doc: &Document, g: Group) {
        // Brief 049: a continuation group is made from its name; its key may have changed (the
        // lowest arrow deleted or renamed), so it goes in under the key `doc` gives it.
        if let Group::Continuation(c) = g {
            let Some(name) = crate::continuations::name_of(doc, c) else { return };
            if let Some((key, list)) = crate::continuations::group_named(doc, &name) {
                self.remove(key);
                self.insert_group(key, list.into_iter().map(|r| (r, Class::Arrow)).collect());
            }
            return;
        }
        if let Group::Device(c) = g
            && crate::connectors::connector_of(doc, c).is_some_and(|x| x.mating)
        {
            self.insert_connector(doc, c);
            return;
        }
        let list: Vec<(RepresentationId, Class)> = match g {
            Group::Cable(k) => doc
                .representations_of(Subject::Cable(k))
                .filter(|(_, r)| crate::references::cables::is_reference(r))
                .map(|(id, _)| (id, Class::End))
                .collect(),
            Group::Device(c) => {
                let mut head_seen = false;
                doc.representations_of(Subject::Device(c))
                    // Brief 054 (design 054 §3.4): no references to or from a Plain page.
                    .filter(|(_, r)| ecad_model::document::sheet::page_checks(doc, r.page))
                    .filter_map(|(id, r)| match raw_class(doc, r) {
                        Class::Head if !head_seen => {
                            head_seen = true;
                            Some((id, Class::Head))
                        }
                        Class::Head => Some((id, Class::Other)),
                        Class::Child => Some((id, Class::Child)),
                        _ => None,
                    })
                    .collect()
            }
            Group::Continuation(_) => Vec::new(),
        };
        self.insert_group(g, list);
    }

    /// Add group `g` with its members `list` (classes decided) and its edges.
    fn insert_group(&mut self, g: Group, list: Vec<(RepresentationId, Class)>) {
        if list.is_empty() {
            return;
        }
        let mut edges: Vec<(RepresentationId, RepresentationId)> = Vec::new();
        match g {
            Group::Cable(_) => {
                for (a, _) in &list {
                    for (b, _) in &list {
                        if a != b {
                            edges.push((*a, *b));
                        }
                    }
                }
            }
            Group::Device(_) => {
                if let Some((h, _)) = list.iter().find(|(_, c)| *c == Class::Head) {
                    for (ch, _) in list.iter().filter(|(_, c)| *c == Class::Child) {
                        edges.push((*ch, *h));
                        edges.push((*h, *ch));
                    }
                }
            }
            // Brief 049 (FP4): implicit, never stored ([`RefIndex::targets`]).
            Group::Continuation(_) => {}
        }
        for (r, c) in &list {
            self.member.insert(*r, (g, *c));
        }
        self.members.insert(g, list.iter().map(|(r, _)| *r).collect());
        for (s, t) in edges {
            self.targets.entry(s).or_default().insert(t);
            self.sources.entry(t).or_default().insert(s);
        }
    }

    /// Make the groups `groups` again from `doc` (after a commit): all of them out first, then
    /// back, so a continuation group whose key changed is never removed after it was remade.
    pub fn refresh(&mut self, doc: &Document, groups: &BTreeSet<Group>) {
        for g in groups {
            self.remove(*g);
        }
        for g in groups {
            self.make(doc, *g);
        }
    }

    /// The groups whose members or classes `ops` could change, looked up in `doc` (called on the
    /// state before and after the ops).
    pub fn groups_named(doc: &Document, ops: &[Op]) -> BTreeSet<Group> {
        let mut out = BTreeSet::new();
        for e in &Effects::of(ops) {
            RefIndex::groups_named_by(doc, e, &mut out);
        }
        out
    }

    /// One effect of [`RefIndex::groups_named`]: adds to `out` the groups whose members or
    /// classes `effect` could change, looked up in `doc`.
    pub(crate) fn groups_named_by(doc: &Document, effect: &Effect<'_>, out: &mut BTreeSet<Group>) {
        let of = |r: &Representation, out: &mut BTreeSet<Group>| match r.subject {
            Subject::Cable(k) => {
                out.insert(Group::Cable(k));
            }
            Subject::Device(c) => {
                out.insert(Group::Device(c));
            }
            // Design 072 §6.2: a continuation's name group.
            Subject::Aid(a) => {
                let key = doc.aids().get(&a).and_then(ecad_model::Aid::continuation_name).map(|n| ecad_model::connectivity::name_key(n.as_str()));
                out.extend(key.filter(|k| !k.is_empty()).and_then(|k| crate::continuations::group_key(doc, &k)));
            }
            Subject::Wire(_) | Subject::Route(_) => {}
        };
        match effect {
            Effect::Rep { before, after, .. } => {
                for r in before.iter().chain(after.iter()) {
                    of(r, out);
                }
            }
            // A symbol item's definition says what the symbol is.
            Effect::Item { id, .. } => {
                if let Some((_, r)) = doc.representation_of_item(*id) {
                    of(r, out);
                }
            }
            Effect::Device { id, before: None, .. } | Effect::Device { id, after: None, .. } | Effect::DeviceDefinition { id, .. } => {
                out.insert(Group::Device(*id));
            }
            Effect::Cable { id, .. } => {
                out.insert(Group::Cable(*id));
            }
            Effect::Definition { id, .. } => out.extend(doc.components_using(*id).into_iter().map(Group::Device)),
            Effect::Page { .. }
            | Effect::Layer { .. }
            | Effect::Device { .. }
            | Effect::DeviceLink { .. }
            | Effect::ConnectionPoint { .. }
            | Effect::Wire { .. }
            | Effect::Route { .. }
            | Effect::Prop { .. }
            | Effect::Settings { .. }
            | Effect::Version { .. }
            | Effect::Folder { .. }
            | Effect::Style { .. }
            | Effect::Standard { .. }
            | Effect::Project { .. }
            | Effect::Aid { .. }
            | Effect::LoosePart { .. } => {}
        }
        // Brief 049 (design 049 §8.1): the name groups the effect names, as `doc` keys them.
        let mut names = BTreeSet::new();
        crate::continuations::names_named_by(doc, effect, &mut names);
        for n in names {
            out.extend(crate::continuations::group_key(doc, &n));
        }
    }

    /// The group and class of member `r`.
    pub fn member(&self, r: RepresentationId) -> Option<(Group, Class)> {
        self.member.get(&r).copied()
    }

    /// The members of group `g`.
    pub fn members(&self, g: Group) -> impl Iterator<Item = RepresentationId> + '_ {
        self.members.get(&g).into_iter().flatten().copied()
    }

    /// The other members of `r`'s continuation group (brief 049: its implicit edges, both ways).
    fn implicit(&self, r: RepresentationId) -> impl Iterator<Item = RepresentationId> + '_ {
        let group = match self.member.get(&r) {
            Some((g @ Group::Continuation(_), _)) => self.members.get(g),
            _ => None,
        };
        group.into_iter().flatten().copied().filter(move |x| *x != r)
    }

    /// What source `s` shows.
    pub fn targets(&self, s: RepresentationId) -> impl Iterator<Item = RepresentationId> + '_ {
        self.targets.get(&s).into_iter().flatten().copied().chain(self.implicit(s))
    }

    /// Who shows target `t`.
    pub fn sources(&self, t: RepresentationId) -> impl Iterator<Item = RepresentationId> + '_ {
        self.sources.get(&t).into_iter().flatten().copied().chain(self.implicit(t))
    }

    /// How many members group `g` has (O(1)).
    pub(crate) fn group_len(&self, g: Group) -> usize {
        self.members.get(&g).map_or(0, BTreeSet::len)
    }

    /// Every group.
    pub fn groups(&self) -> impl Iterator<Item = Group> + '_ {
        self.members.keys().copied()
    }

    /// How many members, groups and edges (a continuation group's m(m−1) implicit edges counted,
    /// not stored).
    pub fn sizes(&self) -> (usize, usize, usize) {
        let implicit: usize = self
            .members
            .iter()
            .filter(|(g, _)| matches!(g, Group::Continuation(_)))
            .map(|(_, m)| m.len().saturating_mul(m.len().saturating_sub(1)))
            .fold(0, usize::saturating_add);
        (self.member.len(), self.members.len(), self.targets.values().map(BTreeSet::len).sum::<usize>().saturating_add(implicit))
    }

    /// The pages that hold a continuation arrow (brief 049: the wide case's pages).
    pub(crate) fn arrow_pages(&self, doc: &Document) -> BTreeSet<PageId> {
        self.members
            .iter()
            .filter(|(g, _)| matches!(g, Group::Continuation(_)))
            .flat_map(|(_, m)| m.iter())
            .filter_map(|r| doc.representations().get(r).map(|x| x.page))
            .collect()
    }

    /// The pages that hold a member of a device group (the wide case's pages).
    pub(crate) fn device_pages(&self, doc: &Document) -> BTreeSet<PageId> {
        self.member
            .iter()
            .filter(|(_, (g, _))| matches!(g, Group::Device(_)))
            .filter_map(|(r, _)| doc.representations().get(r).map(|x| x.page))
            .collect()
    }

    /// The devices with a member on `page` (through the page index).
    pub(crate) fn devices_on(&self, doc: &Document, page: PageId) -> BTreeSet<DeviceId> {
        doc.representations_on(page)
            .filter_map(|(r, _)| match self.member.get(&r) {
                Some((Group::Device(c), _)) => Some(*c),
                _ => None,
            })
            .collect()
    }

    /// Compare with a rebuild from `doc` (verify mode, the debugger's Check index): `Err` names
    /// the first difference.
    ///
    /// # Errors
    /// The kept index differs from a rebuilt one.
    pub fn verify(&self, doc: &Document) -> Result<(), CmdError> {
        let fresh = RefIndex::build(doc);
        if *self == fresh {
            return Ok(());
        }
        let first = self
            .member
            .iter()
            .find(|(r, v)| fresh.member.get(r) != Some(v))
            .map(|(r, v)| format!("member {} kept as {v:?}, rebuilt as {:?}", r.hex(), fresh.member.get(r)))
            .or_else(|| {
                fresh.member.iter().find(|(r, _)| !self.member.contains_key(r)).map(|(r, v)| format!("member {} missing (rebuilt as {v:?})", r.hex()))
            })
            .unwrap_or_else(|| "edges differ".into());
        Err(CmdError::refused(format!("The reference index differs from a rebuild: {first}.")))
    }
}
