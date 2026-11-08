//! Reverse indexes over the document: who refers to an object.
//!
//! The document stores references one way (a conductor names its terminals, a cable names its
//! conductors). Live editing and the scoped invariant check ([`crate::invariant`]) need the other
//! direction without scanning every object. [`Index`] holds it.
//!
//! # Semantics
//! - The index is derived data: it is never serialised, and [`Index::build`] recomputes it from
//!   the stored objects. Two documents with the same objects have equal indexes.
//! - [`crate::op::Op::apply`] keeps it up to date, so it holds for every state an op sequence
//!   reaches, including states inside a transaction that break invariants (a reference to a
//!   missing object is still indexed under that object's ID).
//! - Entries count references, so a cable that lists a conductor twice (refused at the end of a
//!   transaction, but reachable in between) stays indexed until both entries are gone. Empty
//!   entries are removed, so an incrementally kept index equals a rebuilt one.
//! - Code that changes a document's maps directly, not through ops, must rebuild the index
//!   ([`crate::Document::rebuild_index`]); the full check in verify mode reports a stale one.
//!
//! This module holds no invariant logic and no I/O.

use std::collections::BTreeMap;

use crate::doc::{
    Cable, CableMember, Component, Conductor, Document, End, Item, ItemKind, Representation, Subject, Terminal,
};
use crate::id::{
    CableId, ComponentId, ConductorId, DefinitionId, ItemId, LayerId, PageId, RepresentationId, TerminalId,
};

/// Whether a reference is being added or removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Delta {
    Add,
    Remove,
}

/// `key → referrer → number of references`.
type Multi<K, V> = BTreeMap<K, BTreeMap<V, u32>>;

fn bump<K: Ord, V: Ord>(map: &mut Multi<K, V>, key: K, value: V, delta: Delta) {
    match delta {
        Delta::Add => {
            let n = map.entry(key).or_default().entry(value).or_insert(0);
            *n = n.saturating_add(1);
        }
        Delta::Remove => {
            let Some(inner) = map.get_mut(&key) else { return };
            if let Some(n) = inner.get_mut(&value) {
                *n = n.saturating_sub(1);
                if *n == 0 {
                    inner.remove(&value);
                }
            }
            if inner.is_empty() {
                map.remove(&key);
            }
        }
    }
}

/// The referrers of `key`, in ID order.
pub(crate) fn referrers<'a, K: Ord, V: Copy + 'a>(
    map: &'a Multi<K, V>,
    key: &K,
) -> impl Iterator<Item = V> + use<'a, K, V> {
    map.get(key).into_iter().flat_map(|inner| inner.keys().copied())
}

/// The reverse indexes.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Index {
    /// Terminal → conductor ends attached to it.
    pub(crate) terminal_ends: Multi<TerminalId, (ConductorId, End)>,
    /// Conductor → cables listing it.
    pub(crate) conductor_cables: Multi<ConductorId, CableId>,
    /// Component → its terminals.
    pub(crate) component_terminals: Multi<ComponentId, TerminalId>,
    /// Subject → representations depicting it.
    pub(crate) subject_reps: Multi<Subject, RepresentationId>,
    /// Page → representations on it.
    pub(crate) page_reps: Multi<PageId, RepresentationId>,
    /// Item → representations listing it.
    pub(crate) item_reps: Multi<ItemId, RepresentationId>,
    /// Terminal → representations giving it a connection point.
    pub(crate) terminal_reps: Multi<TerminalId, RepresentationId>,
    /// Page → items on it.
    pub(crate) page_items: Multi<PageId, ItemId>,
    /// Layer → items using it.
    pub(crate) layer_items: Multi<LayerId, ItemId>,
    /// Definition → symbol-instance items placing it.
    pub(crate) definition_items: Multi<DefinitionId, ItemId>,
    /// Definition → components using it.
    pub(crate) definition_components: Multi<DefinitionId, ComponentId>,
    /// Tag key ([`crate::tag::key`]) → components whose `tag` property has it (brief 018).
    pub(crate) tags: Multi<String, ComponentId>,
}

impl Index {
    /// The index of `doc`, computed from its stored objects.
    pub(crate) fn build(doc: &Document) -> Self {
        let mut ix = Index::default();
        for (id, item) in &doc.items {
            ix.item(*id, item, Delta::Add);
        }
        for (id, c) in &doc.components {
            ix.component(*id, c, Delta::Add);
        }
        for (id, t) in &doc.terminals {
            ix.terminal(*id, t, Delta::Add);
        }
        for (id, r) in &doc.representations {
            ix.representation(*id, r, Delta::Add);
        }
        for (id, c) in &doc.conductors {
            ix.conductor(*id, c, Delta::Add);
        }
        for (id, c) in &doc.cables {
            ix.cable(*id, c, Delta::Add);
        }
        ix
    }

    pub(crate) fn item(&mut self, id: ItemId, item: &Item, delta: Delta) {
        bump(&mut self.page_items, item.page, id, delta);
        bump(&mut self.layer_items, item.style.layer, id, delta);
        if let ItemKind::Symbol(s) = &item.kind {
            bump(&mut self.definition_items, s.definition, id, delta);
        }
    }

    pub(crate) fn component(&mut self, id: ComponentId, c: &Component, delta: Delta) {
        if let Some(d) = c.definition {
            bump(&mut self.definition_components, d, id, delta);
        }
        if let Some(t) = c.properties.get(crate::tag::TAG_KEY) {
            self.tag(id, t, delta);
        }
    }

    /// A component's tag `tag` (its stored value) is added or removed.
    pub(crate) fn tag(&mut self, id: ComponentId, tag: &str, delta: Delta) {
        bump(&mut self.tags, crate::tag::key(tag), id, delta);
    }

    pub(crate) fn terminal(&mut self, id: TerminalId, t: &Terminal, delta: Delta) {
        bump(&mut self.component_terminals, t.component, id, delta);
    }

    pub(crate) fn representation(&mut self, id: RepresentationId, r: &Representation, delta: Delta) {
        bump(&mut self.subject_reps, r.subject, id, delta);
        bump(&mut self.page_reps, r.page, id, delta);
        for item in &r.items {
            bump(&mut self.item_reps, *item, id, delta);
        }
        for t in r.connection_points.keys() {
            bump(&mut self.terminal_reps, *t, id, delta);
        }
    }

    pub(crate) fn conductor(&mut self, id: ConductorId, c: &Conductor, delta: Delta) {
        for end in End::BOTH {
            self.conductor_end(id, end, c.end(end), delta);
        }
    }

    pub(crate) fn conductor_end(&mut self, id: ConductorId, end: End, terminal: Option<TerminalId>, delta: Delta) {
        if let Some(t) = terminal {
            bump(&mut self.terminal_ends, t, (id, end), delta);
        }
    }

    pub(crate) fn cable(&mut self, id: CableId, c: &Cable, delta: Delta) {
        for m in &c.members {
            self.cable_member(id, m, delta);
        }
    }

    pub(crate) fn cable_member(&mut self, id: CableId, m: &CableMember, delta: Delta) {
        bump(&mut self.conductor_cables, m.conductor, id, delta);
    }

    /// How many times representations list `item`, in total (DOC-05 allows one).
    pub(crate) fn item_listings(&self, item: ItemId) -> u32 {
        self.item_reps.get(&item).map_or(0, |inner| inner.values().fold(0u32, |a, n| a.saturating_add(*n)))
    }
}
