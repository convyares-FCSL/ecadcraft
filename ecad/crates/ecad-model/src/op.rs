//! Ops: the smallest recorded changes (proposal §6.1).
//!
//! Every op records resolved intent and carries the values it replaces (TX-01, TX-02), so its
//! inverse is computed from the op alone and replay checks its preconditions (TX-04).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::doc::{
    Cable, CableMember, Component, Conductor, Definition, Document, End, Folder, Item, Layer, Page, ProjectSettings, Representation, Terminal,
};
use crate::error::{ModelError, Result};
use crate::id::{CableId, ComponentId, ConductorId, DefinitionId, FolderId, ItemId, LayerId, PageId, RepresentationId, TerminalId};
use crate::index::Delta;
use crate::style::{Standard, StandardHeader, StyleLayer, StyleRecord, StyleRef};

/// An object that carries properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ObjectRef {
    Component(ComponentId),
    Conductor(ConductorId),
    Cable(CableId),
    /// A page's properties (brief 019, decision FP1).
    Page(PageId),
    /// The project's own properties (brief 019, decision FP1).
    Project,
    /// The standard's registry values (brief 050a, FP3): a value at the Standard level. Refused
    /// while the project has no standard.
    Standard,
}

/// Settle the document's nets after ops applied with [`Op::apply_unsettled`] (brief 022).
pub(crate) fn settle(doc: &mut Document) {
    let mut conn = std::mem::take(&mut doc.conn);
    conn.settle(doc);
    doc.conn = conn;
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
    /// Change which embedded definition a component's symbols use (brief 048, FP5), keeping
    /// its properties, terminals and representations.
    SetComponentDefinition {
        id: ComponentId,
        before: Option<DefinitionId>,
        after: Option<DefinitionId>,
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
        /// A version's description (brief 020, FQ5); left out of the record when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    UnnameVersion {
        name: String,
        at: u64,
        /// A version's description (brief 020, FQ5); left out of the record when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// A folder of pages at `index` in the folder order (brief 019, decision FP3).
    AddFolder {
        id: FolderId,
        folder: Folder,
        index: usize,
    },
    RemoveFolder {
        id: FolderId,
        folder: Folder,
        index: usize,
    },
    /// Rename or re-parent a folder.
    ModifyFolder {
        id: FolderId,
        before: Folder,
        after: Folder,
    },
    /// Write one whole style record (brief 050a, FP3): `None` is no record. A `Standard` layer
    /// write is refused while the project has no standard.
    SetStyle {
        layer: StyleLayer,
        catalogue: String,
        id: StyleRef,
        before: Option<StyleRecord>,
        after: Option<StyleRecord>,
    },
    /// Set, change or remove the standard's header (brief 050a, FP3). `None → Some` makes an
    /// empty standard; `Some → None` removes one, which must hold no values or styles by then
    /// (the command clears them with ops before it, so undo restores them).
    SetStandard {
        before: Option<StandardHeader>,
        after: Option<StandardHeader>,
    },
}

fn pre(msg: String) -> ModelError {
    ModelError::Precondition(msg)
}

/// Precondition: `id` is not in `map`.
fn absent<K: Ord + Copy + std::fmt::Display, V>(map: &BTreeMap<K, V>, id: K, what: &str) -> Result<()> {
    if map.contains_key(&id) {
        return Err(pre(format!("{what} {id} already exists")));
    }
    Ok(())
}

/// Precondition: `map` holds exactly `v` at `id` (only that it holds something at `id` when not
/// `checked`).
fn holds<K: Ord + Copy + std::fmt::Display, V: PartialEq>(map: &BTreeMap<K, V>, id: K, v: &V, what: &str, checked: bool) -> Result<()> {
    match map.get(&id) {
        Some(cur) if !checked || cur == v => Ok(()),
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
            Op::SetComponentDefinition { id, before, after } => Op::SetComponentDefinition { id, before: after, after: before },
            Op::AddTerminal { id, terminal } => Op::RemoveTerminal { id, terminal },
            Op::RemoveTerminal { id, terminal } => Op::AddTerminal { id, terminal },
            Op::LinkRepresentation { id, rep } => Op::UnlinkRepresentation { id, rep },
            Op::UnlinkRepresentation { id, rep } => Op::LinkRepresentation { id, rep },
            Op::ModifyRepresentation { id, before, after } => Op::ModifyRepresentation { id, before: after, after: before },
            Op::CreateConductor { id, conductor } => Op::DeleteConductor { id, conductor },
            Op::DeleteConductor { id, conductor } => Op::CreateConductor { id, conductor },
            Op::AttachEnd { conductor, end, before, after } => Op::AttachEnd { conductor, end, before: after, after: before },
            Op::CreateCable { id, cable } => Op::DeleteCable { id, cable },
            Op::DeleteCable { id, cable } => Op::CreateCable { id, cable },
            Op::AddCableMember { cable, index, member } => Op::RemoveCableMember { cable, index, member },
            Op::RemoveCableMember { cable, index, member } => Op::AddCableMember { cable, index, member },
            Op::SetProperty { object, key, before, after } => Op::SetProperty { object, key, before: after, after: before },
            Op::EmbedDefinition { id, definition } => Op::RemoveDefinition { id, definition },
            Op::RemoveDefinition { id, definition } => Op::EmbedDefinition { id, definition },
            Op::SetProjectSettings { before, after } => Op::SetProjectSettings { before: after, after: before },
            Op::NameVersion { name, at, description } => Op::UnnameVersion { name, at, description },
            Op::UnnameVersion { name, at, description } => Op::NameVersion { name, at, description },
            Op::AddFolder { id, folder, index } => Op::RemoveFolder { id, folder, index },
            Op::RemoveFolder { id, folder, index } => Op::AddFolder { id, folder, index },
            Op::ModifyFolder { id, before, after } => Op::ModifyFolder { id, before: after, after: before },
            Op::SetStyle { layer, catalogue, id, before, after } => Op::SetStyle { layer, catalogue, id, before: after, after: before },
            Op::SetStandard { before, after } => Op::SetStandard { before: after, after: before },
        }
    }

    /// Apply to `doc`, checking the recorded preconditions.
    ///
    /// # Semantics
    /// - **Atomic:** every precondition is checked before anything changes, so on error `doc` is
    ///   exactly as it was.
    /// - Applies one op only; invariants that span ops are checked once per transaction by the
    ///   ledger, so a state between two ops of a transaction may break them.
    /// - Keeps the document's reverse indexes up to date.
    /// - After `op.apply(doc)` succeeds, `op.inverse().apply(doc)` succeeds and restores `doc`
    ///   exactly, unless a recorded value holds NaN (which never equals itself). The ledger's rollback
    ///   uses a variant that does not compare recorded values, so it always succeeds.
    ///
    /// # Errors
    /// [`ModelError::Precondition`] when the document does not hold the recorded `before` state.
    pub fn apply(&self, doc: &mut Document) -> Result<()> {
        let r = self.apply_unsettled(doc, true);
        settle(doc);
        r
    }

    /// [`Op::apply`] without settling the nets (brief 022): the ledger settles them once after a
    /// whole transaction ([`settle`]).
    pub(crate) fn apply_unsettled(&self, doc: &mut Document, checked: bool) -> Result<()> {
        let mut conn = std::mem::take(&mut doc.conn);
        conn.before_op(doc, self);
        let r = self.apply_with(doc, checked);
        match &r {
            Ok(()) => conn.after_op(doc, self),
            Err(_) => conn.repair(doc, self),
        }
        doc.conn = conn;
        r
    }

    /// Undo this op, which was the last change applied to `doc`, by applying its inverse without
    /// comparing recorded values (only that the objects it names exist and positions are in
    /// range).
    ///
    /// # Semantics
    /// Used by the ledger to roll back a refused transaction. Comparing recorded values could
    /// refuse a rollback that must succeed: a value holding NaN (refused by the end-of-transaction
    /// check) never equals itself. The index is updated from the values actually displaced.
    ///
    /// It writes the recorded value back, which need not be bit-identical to what the document
    /// held when that value compared equal: `-0.0` where the document held `0.0`, for example.
    /// So it is only for undoing the op just applied, whose recorded value is what it wrote.
    ///
    /// # Errors
    /// [`ModelError::Precondition`] only when `doc` is not the state this op produced.
    pub(crate) fn revert(&self, doc: &mut Document) -> Result<()> {
        self.inverse().apply_unsettled(doc, false)
    }

    fn apply_with(&self, doc: &mut Document, checked: bool) -> Result<()> {
        match self {
            Op::AddPage { id, page, index } => {
                absent(&doc.pages, *id, "page")?;
                if *index > doc.page_order.len() {
                    return Err(pre(format!("page index {index} is past the end")));
                }
                doc.index.page(*id, page, Delta::Add);
                doc.pages.insert(*id, page.clone());
                doc.page_order.insert(*index, *id);
            }
            Op::RemovePage { id, page, index } => {
                if doc.page_order.get(*index) != Some(id) {
                    return Err(pre(format!("page {id} is not at index {index}")));
                }
                holds(&doc.pages, *id, page, "page", checked)?;
                if let Some(old) = doc.pages.remove(id) {
                    doc.index.page(*id, &old, Delta::Remove);
                }
                doc.page_order.remove(*index);
            }
            Op::AddLayer { id, layer } => {
                absent(&doc.layers, *id, "layer")?;
                doc.layers.insert(*id, layer.clone());
            }
            Op::RemoveLayer { id, layer } => {
                holds(&doc.layers, *id, layer, "layer", checked)?;
                doc.layers.remove(id);
            }
            Op::AddItem { id, item } => {
                absent(&doc.items, *id, "item")?;
                doc.index.item(*id, item, Delta::Add);
                doc.items.insert(*id, item.clone());
            }
            Op::ModifyItem { id, before, after } => {
                holds(&doc.items, *id, before, "item", checked)?;
                if let Some(old) = doc.items.insert(*id, after.clone()) {
                    doc.index.item(*id, &old, Delta::Remove);
                }
                doc.index.item(*id, after, Delta::Add);
            }
            Op::RemoveItem { id, item } => {
                holds(&doc.items, *id, item, "item", checked)?;
                if let Some(old) = doc.items.remove(id) {
                    doc.index.item(*id, &old, Delta::Remove);
                }
            }
            Op::CreateComponent { id, component } => {
                absent(&doc.components, *id, "component")?;
                doc.index.component(*id, component, Delta::Add);
                doc.components.insert(*id, component.clone());
            }
            Op::DeleteComponent { id, component } => {
                holds(&doc.components, *id, component, "component", checked)?;
                if let Some(old) = doc.components.remove(id) {
                    doc.index.component(*id, &old, Delta::Remove);
                }
            }
            Op::SetComponentDefinition { id, before, after } => {
                let c = doc.components.get(id).ok_or_else(|| pre(format!("component {id} does not exist")))?;
                if checked && c.definition != *before {
                    return Err(pre(format!("component {id}'s definition differs from the recorded value")));
                }
                if let Some(old) = doc.components.remove(id) {
                    doc.index.component(*id, &old, Delta::Remove);
                    let new = Component { definition: *after, ..old };
                    doc.index.component(*id, &new, Delta::Add);
                    doc.components.insert(*id, new);
                }
            }
            Op::AddTerminal { id, terminal } => {
                absent(&doc.terminals, *id, "terminal")?;
                doc.index.terminal(*id, terminal, Delta::Add);
                doc.terminals.insert(*id, terminal.clone());
            }
            Op::RemoveTerminal { id, terminal } => {
                holds(&doc.terminals, *id, terminal, "terminal", checked)?;
                if let Some(old) = doc.terminals.remove(id) {
                    doc.index.terminal(*id, &old, Delta::Remove);
                }
            }
            Op::LinkRepresentation { id, rep } => {
                absent(&doc.representations, *id, "representation")?;
                doc.index.representation(*id, rep, Delta::Add);
                doc.representations.insert(*id, rep.clone());
            }
            Op::ModifyRepresentation { id, before, after } => {
                holds(&doc.representations, *id, before, "representation", checked)?;
                if let Some(old) = doc.representations.insert(*id, after.clone()) {
                    doc.index.representation(*id, &old, Delta::Remove);
                }
                doc.index.representation(*id, after, Delta::Add);
            }
            Op::UnlinkRepresentation { id, rep } => {
                holds(&doc.representations, *id, rep, "representation", checked)?;
                if let Some(old) = doc.representations.remove(id) {
                    doc.index.representation(*id, &old, Delta::Remove);
                }
            }
            Op::CreateConductor { id, conductor } => {
                absent(&doc.conductors, *id, "conductor")?;
                doc.index.conductor(*id, conductor, Delta::Add);
                doc.conductors.insert(*id, conductor.clone());
            }
            Op::DeleteConductor { id, conductor } => {
                holds(&doc.conductors, *id, conductor, "conductor", checked)?;
                if let Some(old) = doc.conductors.remove(id) {
                    doc.index.conductor(*id, &old, Delta::Remove);
                }
            }
            Op::AttachEnd { conductor, end, before, after } => {
                let c = doc.conductors.get_mut(conductor).ok_or_else(|| pre(format!("conductor {conductor} does not exist")))?;
                let current = c.end(*end);
                if checked && current != *before {
                    return Err(pre(format!("conductor {conductor} end {end:?} is not attached as recorded")));
                }
                c.set_end(*end, *after);
                doc.index.conductor_end(*conductor, *end, current, Delta::Remove);
                doc.index.conductor_end(*conductor, *end, *after, Delta::Add);
            }
            Op::CreateCable { id, cable } => {
                absent(&doc.cables, *id, "cable")?;
                doc.index.cable(*id, cable, Delta::Add);
                doc.cables.insert(*id, cable.clone());
            }
            Op::DeleteCable { id, cable } => {
                holds(&doc.cables, *id, cable, "cable", checked)?;
                if let Some(old) = doc.cables.remove(id) {
                    doc.index.cable(*id, &old, Delta::Remove);
                }
            }
            Op::AddCableMember { cable, index, member } => {
                let c = doc.cables.get_mut(cable).ok_or_else(|| pre(format!("cable {cable} does not exist")))?;
                if *index > c.members.len() {
                    return Err(pre(format!("cable {cable} member index {index} is past the end")));
                }
                c.members.insert(*index, member.clone());
                doc.index.cable_member(*cable, member, Delta::Add);
            }
            Op::RemoveCableMember { cable, index, member } => {
                let c = doc.cables.get_mut(cable).ok_or_else(|| pre(format!("cable {cable} does not exist")))?;
                match c.members.get(*index) {
                    Some(cur) if !checked || cur == member => {}
                    _ => {
                        return Err(pre(format!("cable {cable} member {index} differs from the recorded value")));
                    }
                }
                let old = c.members.remove(*index);
                doc.index.cable_member(*cable, &old, Delta::Remove);
            }
            Op::SetProperty { object, key, before, after } => {
                let props = match object {
                    ObjectRef::Component(id) => doc.components.get_mut(id).map(|c| &mut c.properties),
                    ObjectRef::Conductor(id) => doc.conductors.get_mut(id).map(|c| &mut c.properties),
                    ObjectRef::Cable(id) => doc.cables.get_mut(id).map(|c| &mut c.properties),
                    ObjectRef::Page(id) => doc.pages.get_mut(id).map(|p| &mut p.properties),
                    ObjectRef::Project => Some(&mut doc.properties),
                    ObjectRef::Standard => doc.standard.as_mut().map(|s| &mut s.values),
                }
                .ok_or_else(|| pre(format!("{object:?} does not exist")))?;
                if checked && props.get(key) != before.as_ref() {
                    return Err(pre(format!("property {key} of {object:?} differs from the recorded value")));
                }
                let old = match after {
                    Some(v) => props.insert(key.clone(), v.clone()),
                    None => props.remove(key),
                };
                // A continuation's name is indexed (brief 022).
                if let ObjectRef::Component(id) = object
                    && (key == crate::connectivity::KIND || key == crate::connectivity::NAME)
                {
                    let mut before_props = props.clone();
                    match &old {
                        Some(v) => before_props.insert(key.clone(), v.clone()),
                        None => before_props.remove(key),
                    };
                    let was = crate::connectivity::continuation_key(&before_props);
                    let now = crate::connectivity::continuation_key(props);
                    if was != now {
                        if let Some(k) = was {
                            doc.index.continuation(*id, k, Delta::Remove);
                        }
                        if let Some(k) = now {
                            doc.index.continuation(*id, k, Delta::Add);
                        }
                    }
                }
                // A component's tag is indexed (brief 018).
                if let ObjectRef::Component(id) = object
                    && key == crate::tag::TAG_KEY
                {
                    if let Some(old) = &old {
                        doc.index.tag(*id, old, Delta::Remove);
                    }
                    if let Some(new) = after {
                        doc.index.tag(*id, new, Delta::Add);
                    }
                }
                if let ObjectRef::Cable(id) = object
                    && key == crate::tag::TAG_KEY
                {
                    if let Some(old) = &old {
                        doc.index.cable_tag(*id, old, Delta::Remove);
                    }
                    if let Some(new) = after {
                        doc.index.cable_tag(*id, new, Delta::Add);
                    }
                }
                // A style choice is indexed (brief 050a): the style's explicit users.
                if crate::style::is_choice_key(key) {
                    let obj = *object;
                    if !matches!(obj, ObjectRef::Project | ObjectRef::Standard) {
                        if let Some(old) = &old {
                            doc.index.style_choice(obj, key, old, Delta::Remove);
                        }
                        if let Some(new) = after {
                            doc.index.style_choice(obj, key, new, Delta::Add);
                        }
                    }
                }
                // A page's folder is indexed (brief 019).
                if let ObjectRef::Page(id) = object
                    && key == crate::book::FOLDER_KEY
                {
                    if let Some(old) = &old {
                        doc.index.page_folder(*id, old, Delta::Remove);
                    }
                    if let Some(new) = after {
                        doc.index.page_folder(*id, new, Delta::Add);
                    }
                }
            }
            Op::EmbedDefinition { id, definition } => {
                absent(&doc.definitions, *id, "definition")?;
                doc.definitions.insert(*id, definition.clone());
            }
            Op::RemoveDefinition { id, definition } => {
                holds(&doc.definitions, *id, definition, "definition", checked)?;
                doc.definitions.remove(id);
            }
            Op::SetProjectSettings { before, after } => {
                if checked && &doc.settings != before {
                    return Err(pre("project settings differ from the recorded value".into()));
                }
                doc.settings = after.clone();
            }
            Op::NameVersion { name, at, .. } => {
                if doc.versions.contains_key(name) {
                    return Err(pre(format!("version name {name:?} is already used")));
                }
                doc.versions.insert(name.clone(), *at);
            }
            Op::UnnameVersion { name, at, .. } => {
                if doc.versions.get(name) != Some(at) {
                    return Err(pre(format!("version name {name:?} does not name transaction {at}")));
                }
                doc.versions.remove(name);
            }
            Op::AddFolder { id, folder, index } => {
                absent(&doc.folders, *id, "folder")?;
                if *index > doc.folder_order.len() {
                    return Err(pre(format!("folder index {index} is past the end")));
                }
                doc.index.folder(*id, folder, Delta::Add);
                doc.folders.insert(*id, folder.clone());
                doc.folder_order.insert(*index, *id);
            }
            Op::RemoveFolder { id, folder, index } => {
                if doc.folder_order.get(*index) != Some(id) {
                    return Err(pre(format!("folder {id} is not at index {index}")));
                }
                holds(&doc.folders, *id, folder, "folder", checked)?;
                if let Some(old) = doc.folders.remove(id) {
                    doc.index.folder(*id, &old, Delta::Remove);
                }
                doc.folder_order.remove(*index);
            }
            Op::ModifyFolder { id, before, after } => {
                holds(&doc.folders, *id, before, "folder", checked)?;
                if let Some(old) = doc.folders.insert(*id, after.clone()) {
                    doc.index.folder(*id, &old, Delta::Remove);
                }
                doc.index.folder(*id, after, Delta::Add);
            }
            Op::SetStyle { layer, catalogue, id, before, after } => {
                let cats = match layer {
                    StyleLayer::Project => &mut doc.styles,
                    StyleLayer::Standard => {
                        &mut doc.standard.as_mut().ok_or_else(|| pre(format!("style {catalogue}/{id}: the project has no standard")))?.styles
                    }
                };
                let current = cats.get(catalogue).and_then(|c| c.get(id));
                if checked && current != before.as_ref() {
                    return Err(pre(format!("style {catalogue}/{id} ({}) differs from the recorded value", layer.word())));
                }
                crate::style::put(cats, catalogue, id, after.as_ref());
            }
            Op::SetStandard { before, after } => {
                let current = doc.standard.as_ref().map(|s| &s.header);
                if checked && current != before.as_ref() {
                    return Err(pre("the standard's header differs from the recorded value".into()));
                }
                match (after, doc.standard.as_mut()) {
                    (Some(h), Some(s)) => s.header = h.clone(),
                    (Some(h), None) => doc.standard = Some(Standard::new(h.clone())),
                    (None, Some(s)) => {
                        // Removing a standard that still holds values or styles could not be
                        // undone from the op alone: the command clears them first.
                        if !s.values.is_empty() || !s.styles.is_empty() {
                            return Err(pre("the standard still holds values or styles".into()));
                        }
                        doc.standard = None;
                    }
                    (None, None) => {}
                }
            }
        }
        Ok(())
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
            Op::SetComponentDefinition { .. } => "SetComponentDefinition",
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
            Op::AddFolder { .. } => "AddFolder",
            Op::RemoveFolder { .. } => "RemoveFolder",
            Op::ModifyFolder { .. } => "ModifyFolder",
            Op::SetStyle { .. } => "SetStyle",
            Op::SetStandard { .. } => "SetStandard",
        }
    }
}
