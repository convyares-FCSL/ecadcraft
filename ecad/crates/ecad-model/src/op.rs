//! Ops: the smallest recorded changes (proposal §6.1).
//!
//! Every op records resolved intent and carries the values it replaces (TX-01, TX-02), so its
//! inverse is computed from the op alone and replay checks its preconditions (TX-04).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::doc::{
    Cable, CableMember, Component, Conductor, Definition, Document, End, Item, Layer, Page, ProjectSettings,
    Representation, Terminal,
};
use crate::error::{ModelError, Result};
use crate::id::{
    CableId, ComponentId, ConductorId, DefinitionId, ItemId, LayerId, PageId, RepresentationId, TerminalId,
};

/// An object that carries properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ObjectRef {
    Component(ComponentId),
    Conductor(ConductorId),
    Cable(CableId),
}

/// One recorded change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Op {
    AddPage {
        id: PageId,
        page: Page,
        index: usize,
    },
    RemovePage {
        id: PageId,
        page: Page,
        index: usize,
    },
    AddLayer {
        id: LayerId,
        layer: Layer,
    },
    RemoveLayer {
        id: LayerId,
        layer: Layer,
    },
    AddItem {
        id: ItemId,
        item: Item,
    },
    ModifyItem {
        id: ItemId,
        before: Item,
        after: Item,
    },
    RemoveItem {
        id: ItemId,
        item: Item,
    },
    CreateComponent {
        id: ComponentId,
        component: Component,
    },
    DeleteComponent {
        id: ComponentId,
        component: Component,
    },
    AddTerminal {
        id: TerminalId,
        terminal: Terminal,
    },
    RemoveTerminal {
        id: TerminalId,
        terminal: Terminal,
    },
    LinkRepresentation {
        id: RepresentationId,
        rep: Representation,
    },
    ModifyRepresentation {
        id: RepresentationId,
        before: Representation,
        after: Representation,
    },
    UnlinkRepresentation {
        id: RepresentationId,
        rep: Representation,
    },
    CreateConductor {
        id: ConductorId,
        conductor: Conductor,
    },
    DeleteConductor {
        id: ConductorId,
        conductor: Conductor,
    },
    AttachEnd {
        conductor: ConductorId,
        end: End,
        before: Option<TerminalId>,
        after: Option<TerminalId>,
    },
    CreateCable {
        id: CableId,
        cable: Cable,
    },
    DeleteCable {
        id: CableId,
        cable: Cable,
    },
    AddCableMember {
        cable: CableId,
        index: usize,
        member: CableMember,
    },
    RemoveCableMember {
        cable: CableId,
        index: usize,
        member: CableMember,
    },
    SetProperty {
        object: ObjectRef,
        key: String,
        before: Option<String>,
        after: Option<String>,
    },
    EmbedDefinition {
        id: DefinitionId,
        definition: Definition,
    },
    RemoveDefinition {
        id: DefinitionId,
        definition: Definition,
    },
    SetProjectSettings {
        before: ProjectSettings,
        after: ProjectSettings,
    },
    /// Bookmark transaction `at` under `name` (§6.1).
    NameVersion {
        name: String,
        at: u64,
    },
    UnnameVersion {
        name: String,
        at: u64,
    },
}

fn pre(msg: String) -> ModelError {
    ModelError::Precondition(msg)
}

fn insert_new<K: Ord + Copy + std::fmt::Display, V>(map: &mut BTreeMap<K, V>, id: K, v: V, what: &str) -> Result<()> {
    if map.contains_key(&id) {
        return Err(pre(format!("{what} {id} already exists")));
    }
    map.insert(id, v);
    Ok(())
}

fn remove_exact<K: Ord + Copy + std::fmt::Display, V: PartialEq>(
    map: &mut BTreeMap<K, V>,
    id: K,
    v: &V,
    what: &str,
) -> Result<()> {
    match map.get(&id) {
        Some(cur) if cur == v => {
            map.remove(&id);
            Ok(())
        }
        Some(_) => Err(pre(format!("{what} {id} differs from the recorded value"))),
        None => Err(pre(format!("{what} {id} does not exist"))),
    }
}

fn replace_exact<K: Ord + Copy + std::fmt::Display, V: PartialEq + Clone>(
    map: &mut BTreeMap<K, V>,
    id: K,
    before: &V,
    after: &V,
    what: &str,
) -> Result<()> {
    match map.get_mut(&id) {
        Some(cur) if cur == before => {
            *cur = after.clone();
            Ok(())
        }
        Some(_) => Err(pre(format!("{what} {id} differs from the recorded value"))),
        None => Err(pre(format!("{what} {id} does not exist"))),
    }
}

impl Op {
    /// The op that undoes this one.
    pub fn inverse(&self) -> Op {
        match self.clone() {
            Op::AddPage { id, page, index } => Op::RemovePage { id, page, index },
            Op::RemovePage { id, page, index } => Op::AddPage { id, page, index },
            Op::AddLayer { id, layer } => Op::RemoveLayer { id, layer },
            Op::RemoveLayer { id, layer } => Op::AddLayer { id, layer },
            Op::AddItem { id, item } => Op::RemoveItem { id, item },
            Op::RemoveItem { id, item } => Op::AddItem { id, item },
            Op::ModifyItem { id, before, after } => Op::ModifyItem { id, before: after, after: before },
            Op::CreateComponent { id, component } => Op::DeleteComponent { id, component },
            Op::DeleteComponent { id, component } => Op::CreateComponent { id, component },
            Op::AddTerminal { id, terminal } => Op::RemoveTerminal { id, terminal },
            Op::RemoveTerminal { id, terminal } => Op::AddTerminal { id, terminal },
            Op::LinkRepresentation { id, rep } => Op::UnlinkRepresentation { id, rep },
            Op::UnlinkRepresentation { id, rep } => Op::LinkRepresentation { id, rep },
            Op::ModifyRepresentation { id, before, after } => {
                Op::ModifyRepresentation { id, before: after, after: before }
            }
            Op::CreateConductor { id, conductor } => Op::DeleteConductor { id, conductor },
            Op::DeleteConductor { id, conductor } => Op::CreateConductor { id, conductor },
            Op::AttachEnd { conductor, end, before, after } => {
                Op::AttachEnd { conductor, end, before: after, after: before }
            }
            Op::CreateCable { id, cable } => Op::DeleteCable { id, cable },
            Op::DeleteCable { id, cable } => Op::CreateCable { id, cable },
            Op::AddCableMember { cable, index, member } => Op::RemoveCableMember { cable, index, member },
            Op::RemoveCableMember { cable, index, member } => Op::AddCableMember { cable, index, member },
            Op::SetProperty { object, key, before, after } => {
                Op::SetProperty { object, key, before: after, after: before }
            }
            Op::EmbedDefinition { id, definition } => Op::RemoveDefinition { id, definition },
            Op::RemoveDefinition { id, definition } => Op::EmbedDefinition { id, definition },
            Op::SetProjectSettings { before, after } => Op::SetProjectSettings { before: after, after: before },
            Op::NameVersion { name, at } => Op::UnnameVersion { name, at },
            Op::UnnameVersion { name, at } => Op::NameVersion { name, at },
        }
    }

    /// Apply to `doc`, checking the recorded preconditions.
    ///
    /// # Semantics
    /// Applies one op only; invariants that span ops are checked per transaction by the ledger.
    /// On error `doc` may be partly changed, which is why the ledger applies to a copy.
    ///
    /// # Errors
    /// [`ModelError::Precondition`] when the document does not hold the recorded `before` state.
    pub fn apply(&self, doc: &mut Document) -> Result<()> {
        match self {
            Op::AddPage { id, page, index } => {
                insert_new(&mut doc.pages, *id, page.clone(), "page")?;
                if *index > doc.page_order.len() {
                    return Err(pre(format!("page index {index} is past the end")));
                }
                doc.page_order.insert(*index, *id);
                Ok(())
            }
            Op::RemovePage { id, page, index } => {
                if doc.page_order.get(*index) != Some(id) {
                    return Err(pre(format!("page {id} is not at index {index}")));
                }
                remove_exact(&mut doc.pages, *id, page, "page")?;
                doc.page_order.remove(*index);
                Ok(())
            }
            Op::AddLayer { id, layer } => insert_new(&mut doc.layers, *id, layer.clone(), "layer"),
            Op::RemoveLayer { id, layer } => remove_exact(&mut doc.layers, *id, layer, "layer"),
            Op::AddItem { id, item } => insert_new(&mut doc.items, *id, item.clone(), "item"),
            Op::ModifyItem { id, before, after } => replace_exact(&mut doc.items, *id, before, after, "item"),
            Op::RemoveItem { id, item } => remove_exact(&mut doc.items, *id, item, "item"),
            Op::CreateComponent { id, component } => {
                insert_new(&mut doc.components, *id, component.clone(), "component")
            }
            Op::DeleteComponent { id, component } => remove_exact(&mut doc.components, *id, component, "component"),
            Op::AddTerminal { id, terminal } => insert_new(&mut doc.terminals, *id, terminal.clone(), "terminal"),
            Op::RemoveTerminal { id, terminal } => remove_exact(&mut doc.terminals, *id, terminal, "terminal"),
            Op::LinkRepresentation { id, rep } => {
                insert_new(&mut doc.representations, *id, rep.clone(), "representation")
            }
            Op::ModifyRepresentation { id, before, after } => {
                replace_exact(&mut doc.representations, *id, before, after, "representation")
            }
            Op::UnlinkRepresentation { id, rep } => remove_exact(&mut doc.representations, *id, rep, "representation"),
            Op::CreateConductor { id, conductor } => {
                insert_new(&mut doc.conductors, *id, conductor.clone(), "conductor")
            }
            Op::DeleteConductor { id, conductor } => remove_exact(&mut doc.conductors, *id, conductor, "conductor"),
            Op::AttachEnd { conductor, end, before, after } => {
                let c = doc
                    .conductors
                    .get_mut(conductor)
                    .ok_or_else(|| pre(format!("conductor {conductor} does not exist")))?;
                if c.end(*end) != *before {
                    return Err(pre(format!("conductor {conductor} end {end:?} is not attached as recorded")));
                }
                c.set_end(*end, *after);
                Ok(())
            }
            Op::CreateCable { id, cable } => insert_new(&mut doc.cables, *id, cable.clone(), "cable"),
            Op::DeleteCable { id, cable } => remove_exact(&mut doc.cables, *id, cable, "cable"),
            Op::AddCableMember { cable, index, member } => {
                let c = doc.cables.get_mut(cable).ok_or_else(|| pre(format!("cable {cable} does not exist")))?;
                if *index > c.members.len() {
                    return Err(pre(format!("cable {cable} member index {index} is past the end")));
                }
                c.members.insert(*index, member.clone());
                Ok(())
            }
            Op::RemoveCableMember { cable, index, member } => {
                let c = doc.cables.get_mut(cable).ok_or_else(|| pre(format!("cable {cable} does not exist")))?;
                if c.members.get(*index) != Some(member) {
                    return Err(pre(format!("cable {cable} member {index} differs from the recorded value")));
                }
                c.members.remove(*index);
                Ok(())
            }
            Op::SetProperty { object, key, before, after } => {
                let props = match object {
                    ObjectRef::Component(id) => doc.components.get_mut(id).map(|c| &mut c.properties),
                    ObjectRef::Conductor(id) => doc.conductors.get_mut(id).map(|c| &mut c.properties),
                    ObjectRef::Cable(id) => doc.cables.get_mut(id).map(|c| &mut c.properties),
                }
                .ok_or_else(|| pre(format!("{object:?} does not exist")))?;
                if props.get(key) != before.as_ref() {
                    return Err(pre(format!("property {key} of {object:?} differs from the recorded value")));
                }
                match after {
                    Some(v) => props.insert(key.clone(), v.clone()),
                    None => props.remove(key),
                };
                Ok(())
            }
            Op::EmbedDefinition { id, definition } => {
                insert_new(&mut doc.definitions, *id, definition.clone(), "definition")
            }
            Op::RemoveDefinition { id, definition } => {
                remove_exact(&mut doc.definitions, *id, definition, "definition")
            }
            Op::SetProjectSettings { before, after } => {
                if &doc.settings != before {
                    return Err(pre("project settings differ from the recorded value".into()));
                }
                doc.settings = after.clone();
                Ok(())
            }
            Op::NameVersion { name, at } => {
                if doc.versions.contains_key(name) {
                    return Err(pre(format!("version name {name:?} is already used")));
                }
                doc.versions.insert(name.clone(), *at);
                Ok(())
            }
            Op::UnnameVersion { name, at } => match doc.versions.get(name) {
                Some(cur) if cur == at => {
                    doc.versions.remove(name);
                    Ok(())
                }
                _ => Err(pre(format!("version name {name:?} does not name transaction {at}"))),
            },
        }
    }

    /// A short name for listings.
    pub fn name(&self) -> &'static str {
        match self {
            Op::AddPage { .. } => "AddPage",
            Op::RemovePage { .. } => "RemovePage",
            Op::AddLayer { .. } => "AddLayer",
            Op::RemoveLayer { .. } => "RemoveLayer",
            Op::AddItem { .. } => "AddItem",
            Op::ModifyItem { .. } => "ModifyItem",
            Op::RemoveItem { .. } => "RemoveItem",
            Op::CreateComponent { .. } => "CreateComponent",
            Op::DeleteComponent { .. } => "DeleteComponent",
            Op::AddTerminal { .. } => "AddTerminal",
            Op::RemoveTerminal { .. } => "RemoveTerminal",
            Op::LinkRepresentation { .. } => "LinkRepresentation",
            Op::ModifyRepresentation { .. } => "ModifyRepresentation",
            Op::UnlinkRepresentation { .. } => "UnlinkRepresentation",
            Op::CreateConductor { .. } => "CreateConductor",
            Op::DeleteConductor { .. } => "DeleteConductor",
            Op::AttachEnd { .. } => "AttachEnd",
            Op::CreateCable { .. } => "CreateCable",
            Op::DeleteCable { .. } => "DeleteCable",
            Op::AddCableMember { .. } => "AddCableMember",
            Op::RemoveCableMember { .. } => "RemoveCableMember",
            Op::SetProperty { .. } => "SetProperty",
            Op::EmbedDefinition { .. } => "EmbedDefinition",
            Op::RemoveDefinition { .. } => "RemoveDefinition",
            Op::SetProjectSettings { .. } => "SetProjectSettings",
            Op::NameVersion { .. } => "NameVersion",
            Op::UnnameVersion { .. } => "UnnameVersion",
        }
    }
}
