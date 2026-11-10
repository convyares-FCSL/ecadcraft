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
    let all = |_: &str| true;
    let mut removes = Vec::new();
    let mut adds = Vec::new();
    let mut changes = Vec::new();

    // Cables: members are compared whole; a cable whose members differ is removed and added.
    for (id, a) in &from.cables {
        match to.cables.get(id) {
            Some(b) if a.members == b.members => props(ObjectRef::Cable(*id), &a.properties, &b.properties, &all, &mut changes),
            _ => removes.push(Op::DeleteCable { id: *id, cable: a.clone() }),
        }
    }
    for (id, b) in &to.cables {
        if from.cables.get(id).is_none_or(|a| a.members != b.members) {
            adds.push(Op::CreateCable { id: *id, cable: b.clone() });
        }
    }
    // Conductors: properties and ends change in place.
    for (id, a) in &from.conductors {
        match to.conductors.get(id) {
            Some(b) => {
                props(ObjectRef::Conductor(*id), &a.properties, &b.properties, &all, &mut changes);
                for end in End::BOTH {
                    if a.end(end) != b.end(end) {
                        changes.push(Op::AttachEnd { conductor: *id, end, before: a.end(end), after: b.end(end) });
                    }
                }
            }
            None => removes.push(Op::DeleteConductor { id: *id, conductor: a.clone() }),
        }
    }
    for (id, b) in &to.conductors {
        if !from.conductors.contains_key(id) {
            adds.push(Op::CreateConductor { id: *id, conductor: b.clone() });
        }
    }
    // Representations change in place.
    for (id, a) in &from.representations {
        match to.representations.get(id) {
            Some(b) if a != b => changes.push(Op::ModifyRepresentation { id: *id, before: a.clone(), after: b.clone() }),
            Some(_) => {}
            None => removes.push(Op::UnlinkRepresentation { id: *id, rep: a.clone() }),
        }
    }
    for (id, b) in &to.representations {
        if !from.representations.contains_key(id) {
            adds.push(Op::LinkRepresentation { id: *id, rep: b.clone() });
        }
    }
    // Terminals: removed and added when they differ.
    for (id, a) in &from.terminals {
        if to.terminals.get(id) != Some(a) {
            removes.push(Op::RemoveTerminal { id: *id, terminal: a.clone() });
        }
    }
    for (id, b) in &to.terminals {
        if from.terminals.get(id) != Some(b) {
            adds.push(Op::AddTerminal { id: *id, terminal: b.clone() });
        }
    }
    // Components: properties in place; another definition is a remove and an add.
    for (id, a) in &from.components {
        match to.components.get(id) {
            Some(b) if a.definition == b.definition => props(ObjectRef::Component(*id), &a.properties, &b.properties, &all, &mut changes),
            _ => removes.push(Op::DeleteComponent { id: *id, component: a.clone() }),
        }
    }
    for (id, b) in &to.components {
        if from.components.get(id).is_none_or(|a| a.definition != b.definition) {
            adds.push(Op::CreateComponent { id: *id, component: b.clone() });
        }
    }
    // Items change in place.
    for (id, a) in &from.items {
        match to.items.get(id) {
            Some(b) if a != b => changes.push(Op::ModifyItem { id: *id, before: a.clone(), after: b.clone() }),
            Some(_) => {}
            None => removes.push(Op::RemoveItem { id: *id, item: a.clone() }),
        }
    }
    for (id, b) in &to.items {
        if !from.items.contains_key(id) {
            adds.push(Op::AddItem { id: *id, item: b.clone() });
        }
    }
    // Definitions and layers: removed and added when they differ.
    for (id, a) in &from.definitions {
        if to.definitions.get(id) != Some(a) {
            removes.push(Op::RemoveDefinition { id: *id, definition: a.clone() });
        }
    }
    for (id, b) in &to.definitions {
        if from.definitions.get(id) != Some(b) {
            adds.push(Op::EmbedDefinition { id: *id, definition: b.clone() });
        }
    }
    for (id, a) in &from.layers {
        if to.layers.get(id) != Some(a) {
            removes.push(Op::RemoveLayer { id: *id, layer: a.clone() });
        }
    }
    for (id, b) in &to.layers {
        if from.layers.get(id) != Some(b) {
            adds.push(Op::AddLayer { id: *id, layer: b.clone() });
        }
    }

    // Pages: those kept in place change only their properties; the others are removed (from the
    // highest index down) and inserted in the target's order.
    let same_title: Vec<_> =
        from.page_order.iter().copied().filter(|p| to.pages.get(p).zip(from.pages.get(p)).is_some_and(|(b, a)| a.title == b.title)).collect();
    let stay = kept(&same_title, &to.page_order);
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
            if let (Some(a), Some(b)) = (from.pages.get(id), to.pages.get(id)) {
                props(ObjectRef::Page(*id), &a.properties, &b.properties, &all, &mut changes);
            }
        } else if let Some(page) = to.pages.get(id) {
            page_adds.push(Op::AddPage { id: *id, page: page.clone(), index: i });
        }
    }

    // Folders: as pages; a kept folder's name or parent changes in place.
    let stay = kept(&from.folder_order.iter().copied().filter(|f| to.folders.contains_key(f)).collect::<Vec<_>>(), &to.folder_order);
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
    props(ObjectRef::Project, &from.properties, &to.properties, keep_project_key, &mut changes);
    if from.settings != to.settings {
        changes.push(Op::SetProjectSettings { before: from.settings.clone(), after: to.settings.clone() });
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
