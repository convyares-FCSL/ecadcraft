//! The ops that turn one document into another (brief 020a, Restore): computed from the two
//! states, so their number follows what differs, not the history between them.
//!
//! # Semantics
//! - Ops are ordinary ops with `before` values from `from`, so [`crate::Ledger::commit`] checks
//!   them as any edit. Each op's local precondition holds where it is placed: removals come
//!   first, then additions, then changes in place; pages and folders are removed and inserted at
//!   indices valid at that point. Invariants hold at the end because `to` is a valid document.
//! - An object present in both with a different value is changed in place where an op can
//!   (`ModifyItem`, `ModifyRepresentation`, `ModifyFolder`, `SetProperty`, `AttachEnd`), else
//!   removed and added again.
//! - Pages and folders keep the longest run already in the target's order; only the others move.
//! - Named versions are never touched (they are history). `keep_project_key` says which project
//!   properties are compared (Restore leaves `rev.*` as it is now).
//!
//! Excluded: version names; the derived index and nets (rebuilt by applying the ops).

use std::collections::{BTreeMap, BTreeSet};

use crate::doc::{Document, End, Properties};
use crate::id::{FolderId, PageId};
use crate::invariant::Touched;
use crate::op::{ObjectRef, Op};

/// `SetProperty` ops turning `a` into `b` on `object`, for the keys `keep` accepts.
fn props(object: ObjectRef, a: &Properties, b: &Properties, keep: &dyn Fn(&str) -> bool, out: &mut Vec<Op>) {
    let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    for k in keys {
        if !keep(k) {
            continue;
        }
        let (before, after) = (a.get(k).cloned(), b.get(k).cloned());
        if before != after {
            out.push(Op::SetProperty { object, key: k.clone(), before, after });
        }
    }
}

/// The longest common subsequence of `a` and `b` (as a set of the elements kept).
fn kept<T: Copy + Ord>(a: &[T], b: &[T]) -> BTreeSet<T> {
    let (n, m) = (a.len(), b.len());
    let mut t = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            t[i][j] = if a[i] == b[j] { t[i + 1][j + 1] + 1 } else { t[i + 1][j].max(t[i][j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, BTreeSet::new());
    while i < n && j < m {
        if a[i] == b[j] {
            out.insert(a[i]);
            i += 1;
            j += 1;
        } else if t[i + 1][j] >= t[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// The ops that turn `from` into `to` (module docs). `keep_project_key` filters the project's
/// own properties.
pub fn ops_between(from: &Document, to: &Document, keep_project_key: &dyn Fn(&str) -> bool) -> Vec<Op> {
    ops_between_in(from, to, keep_project_key, None, true)
}

/// The IDs to compare: those of `scope` when given, else every key of either map.
fn ids<K: Ord + Copy, A, B>(a: &BTreeMap<K, A>, b: &BTreeMap<K, B>, scope: Option<&BTreeSet<K>>) -> Vec<K> {
    match scope {
        Some(s) => s.iter().copied().collect(),
        None => a.keys().chain(b.keys()).copied().collect::<BTreeSet<K>>().into_iter().collect(),
    }
}

/// [`ops_between`] over the objects `scope` names only (brief 020b, merge: its cost follows
/// the touched set, not the project). Objects outside it are assumed equal in both states. The
/// page and folder orders are compared whole when the scope says they changed; the project's
/// own properties and settings only with `project`.
pub fn ops_between_in(from: &Document, to: &Document, keep_project_key: &dyn Fn(&str) -> bool, scope: Option<&Touched>, project: bool) -> Vec<Op> {
    let all = |_: &str| true;
    let mut removes = Vec::new();
    let mut adds = Vec::new();
    let mut changes = Vec::new();

    // Cables: members are compared whole; a cable whose members differ is removed and added.
    for id in ids(&from.cables, &to.cables, scope.map(|s| &s.cables)) {
        match (from.cables.get(&id), to.cables.get(&id)) {
            (Some(a), Some(b)) if a.members == b.members => props(ObjectRef::Cable(id), &a.properties, &b.properties, &all, &mut changes),
            (Some(a), Some(b)) => {
                removes.push(Op::DeleteCable { id, cable: a.clone() });
                adds.push(Op::CreateCable { id, cable: b.clone() });
            }
            (Some(a), None) => removes.push(Op::DeleteCable { id, cable: a.clone() }),
            (None, Some(b)) => adds.push(Op::CreateCable { id, cable: b.clone() }),
            (None, None) => {}
        }
    }
    // Conductors: properties and ends change in place.
    for id in ids(&from.conductors, &to.conductors, scope.map(|s| &s.conductors)) {
        match (from.conductors.get(&id), to.conductors.get(&id)) {
            (Some(a), Some(b)) => {
                props(ObjectRef::Conductor(id), &a.properties, &b.properties, &all, &mut changes);
                for end in End::BOTH {
                    if a.end(end) != b.end(end) {
                        changes.push(Op::AttachEnd { conductor: id, end, before: a.end(end), after: b.end(end) });
                    }
                }
            }
            (Some(a), None) => removes.push(Op::DeleteConductor { id, conductor: a.clone() }),
            (None, Some(b)) => adds.push(Op::CreateConductor { id, conductor: b.clone() }),
            (None, None) => {}
        }
    }
    // Representations change in place.
    for id in ids(&from.representations, &to.representations, scope.map(|s| &s.representations)) {
        match (from.representations.get(&id), to.representations.get(&id)) {
            (Some(a), Some(b)) if a != b => changes.push(Op::ModifyRepresentation { id, before: a.clone(), after: b.clone() }),
            (Some(_), Some(_)) | (None, None) => {}
            (Some(a), None) => removes.push(Op::UnlinkRepresentation { id, rep: a.clone() }),
            (None, Some(b)) => adds.push(Op::LinkRepresentation { id, rep: b.clone() }),
        }
    }
    // Terminals: removed and added when they differ.
    for id in ids(&from.terminals, &to.terminals, scope.map(|s| &s.terminals)) {
        let (a, b) = (from.terminals.get(&id), to.terminals.get(&id));
        if a != b {
            if let Some(a) = a {
                removes.push(Op::RemoveTerminal { id, terminal: a.clone() });
            }
            if let Some(b) = b {
                adds.push(Op::AddTerminal { id, terminal: b.clone() });
            }
        }
    }
    // Components: properties in place; another definition is a remove and an add.
    for id in ids(&from.components, &to.components, scope.map(|s| &s.components)) {
        match (from.components.get(&id), to.components.get(&id)) {
            (Some(a), Some(b)) if a.definition == b.definition => props(ObjectRef::Component(id), &a.properties, &b.properties, &all, &mut changes),
            (Some(a), Some(b)) => {
                removes.push(Op::DeleteComponent { id, component: a.clone() });
                adds.push(Op::CreateComponent { id, component: b.clone() });
            }
            (Some(a), None) => removes.push(Op::DeleteComponent { id, component: a.clone() }),
            (None, Some(b)) => adds.push(Op::CreateComponent { id, component: b.clone() }),
            (None, None) => {}
        }
    }
    // Items change in place.
    for id in ids(&from.items, &to.items, scope.map(|s| &s.items)) {
        match (from.items.get(&id), to.items.get(&id)) {
            (Some(a), Some(b)) if a != b => changes.push(Op::ModifyItem { id, before: a.clone(), after: b.clone() }),
            (Some(_), Some(_)) | (None, None) => {}
            (Some(a), None) => removes.push(Op::RemoveItem { id, item: a.clone() }),
            (None, Some(b)) => adds.push(Op::AddItem { id, item: b.clone() }),
        }
    }
    // Definitions and layers: removed and added when they differ.
    for id in ids(&from.definitions, &to.definitions, scope.map(|s| &s.definitions)) {
        let (a, b) = (from.definitions.get(&id), to.definitions.get(&id));
        if a != b {
            if let Some(a) = a {
                removes.push(Op::RemoveDefinition { id, definition: a.clone() });
            }
            if let Some(b) = b {
                adds.push(Op::EmbedDefinition { id, definition: b.clone() });
            }
        }
    }
    for id in ids(&from.layers, &to.layers, scope.map(|s| &s.layers)) {
        let (a, b) = (from.layers.get(&id), to.layers.get(&id));
        if a != b {
            if let Some(a) = a {
                removes.push(Op::RemoveLayer { id, layer: a.clone() });
            }
            if let Some(b) = b {
                adds.push(Op::AddLayer { id, layer: b.clone() });
            }
        }
    }

    // Pages: those kept in place change only their properties; the others are removed (from the
    // highest index down) and inserted in the target's order.
    let scoped_pages: Option<BTreeSet<PageId>> = scope.map(|s| s.pages.iter().chain(&s.page_properties).copied().collect());
    let page_changed = |p: &PageId| scoped_pages.as_ref().is_none_or(|s| s.contains(p));
    let same_title: Vec<PageId> = from
        .page_order
        .iter()
        .copied()
        .filter(|p| !page_changed(p) || to.pages.get(p).zip(from.pages.get(p)).is_some_and(|(b, a)| a.title == b.title))
        .collect();
    let stay: BTreeSet<PageId> = if same_title == to.page_order { same_title.iter().copied().collect() } else { kept(&same_title, &to.page_order) };
    let mut order = from.page_order.clone();
    let mut page_removes = Vec::new();
    for i in (0..order.len()).rev() {
        let id = order[i];
        if !stay.contains(&id)
            && let Some(page) = from.pages.get(&id)
        {
            page_removes.push(Op::RemovePage { id, page: page.clone(), index: i });
            order.remove(i);
        }
    }
    let mut page_adds = Vec::new();
    for (i, id) in to.page_order.iter().enumerate() {
        if stay.contains(id) {
            if page_changed(id)
                && let (Some(a), Some(b)) = (from.pages.get(id), to.pages.get(id))
            {
                props(ObjectRef::Page(*id), &a.properties, &b.properties, &all, &mut changes);
            }
        } else if let Some(page) = to.pages.get(id) {
            page_adds.push(Op::AddPage { id: *id, page: page.clone(), index: i });
        }
    }

    // Folders: as pages; a kept folder's name or parent changes in place.
    let present: Vec<FolderId> = from.folder_order.iter().copied().filter(|f| to.folders.contains_key(f)).collect();
    let stay: BTreeSet<FolderId> = if present == to.folder_order { present.iter().copied().collect() } else { kept(&present, &to.folder_order) };
    let mut order = from.folder_order.clone();
    let mut folder_removes = Vec::new();
    for i in (0..order.len()).rev() {
        let id = order[i];
        if !stay.contains(&id)
            && let Some(folder) = from.folders.get(&id)
        {
            folder_removes.push(Op::RemoveFolder { id, folder: folder.clone(), index: i });
            order.remove(i);
        }
    }
    let mut folder_adds = Vec::new();
    for (i, id) in to.folder_order.iter().enumerate() {
        match (stay.contains(id), from.folders.get(id), to.folders.get(id)) {
            (true, Some(a), Some(b)) if a != b => changes.push(Op::ModifyFolder { id: *id, before: a.clone(), after: b.clone() }),
            (true, _, _) => {}
            (false, _, Some(b)) => folder_adds.push(Op::AddFolder { id: *id, folder: b.clone(), index: i }),
            (false, _, None) => {}
        }
    }

    // The project.
    if project {
        props(ObjectRef::Project, &from.properties, &to.properties, keep_project_key, &mut changes);
        if from.settings != to.settings {
            changes.push(Op::SetProjectSettings { before: from.settings.clone(), after: to.settings.clone() });
        }
    }

    let mut out = Vec::with_capacity(removes.len() + adds.len() + changes.len());
    out.extend(removes);
    out.extend(page_removes);
    out.extend(folder_removes);
    out.extend(page_adds);
    out.extend(folder_adds);
    out.extend(adds);
    out.extend(changes);
    out
}

/// The objects `ops` name, by kind and ID (for counting what a restore changes).
pub fn objects(ops: &[Op]) -> BTreeMap<(&'static str, u128), usize> {
    let mut out = BTreeMap::new();
    for op in ops {
        let key = match op {
            Op::AddPage { id, .. } | Op::RemovePage { id, .. } => ("page", id.0),
            Op::AddLayer { id, .. } | Op::RemoveLayer { id, .. } => ("layer", id.0),
            Op::AddItem { id, .. } | Op::ModifyItem { id, .. } | Op::RemoveItem { id, .. } => ("item", id.0),
            Op::CreateComponent { id, .. } | Op::DeleteComponent { id, .. } => ("component", id.0),
            Op::AddTerminal { id, .. } | Op::RemoveTerminal { id, .. } => ("terminal", id.0),
            Op::LinkRepresentation { id, .. } | Op::ModifyRepresentation { id, .. } | Op::UnlinkRepresentation { id, .. } => ("representation", id.0),
            Op::CreateConductor { id, .. } | Op::DeleteConductor { id, .. } | Op::AttachEnd { conductor: id, .. } => ("conductor", id.0),
            Op::CreateCable { id, .. }
            | Op::DeleteCable { id, .. }
            | Op::AddCableMember { cable: id, .. }
            | Op::RemoveCableMember { cable: id, .. } => ("cable", id.0),
            Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => ("definition", id.0),
            Op::AddFolder { id, .. } | Op::RemoveFolder { id, .. } | Op::ModifyFolder { id, .. } => ("folder", id.0),
            Op::SetProperty { object, .. } => match object {
                ObjectRef::Component(c) => ("component", c.0),
                ObjectRef::Conductor(c) => ("conductor", c.0),
                ObjectRef::Cable(c) => ("cable", c.0),
                ObjectRef::Page(c) => ("page", c.0),
                ObjectRef::Project => ("project", 0),
            },
            Op::SetProjectSettings { .. } => ("settings", 0),
            Op::NameVersion { .. } | Op::UnnameVersion { .. } => continue,
        };
        *out.entry(key).or_insert(0) += 1;
    }
    out
}
