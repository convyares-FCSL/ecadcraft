//! Ops: the smallest recorded changes (proposal §6.1).
//!
//! Every op records resolved intent and carries the values it replaces (TX-01, TX-02), so its
//! inverse is computed from the op alone and replay checks its preconditions (TX-04).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::document::doc::{Cable, ConnectionPoint, Definition, Device, Document, Folder, Item, Layer, Page, ProjectSettings, Representation, Wire};
use crate::document::index::Delta;
use crate::error::{ModelError, Result};
use crate::naming::id::{CableId, ConnectionPointId, DefinitionId, DeviceId, FolderId, ItemId, LayerId, PageId, RepresentationId, RouteId, WireId};
use crate::objects::route::Route;
use crate::objects::styles::{Standard, StandardHeader, StyleCatalogue, StyleLayer, StyleRecord, StyleRef};

/// An object that carries properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ObjectRef {
    Device(DeviceId),
    Wire(WireId),
    Cable(CableId),
    /// A page's properties (brief 019, decision FP1).
    Page(PageId),
    /// The project's own properties (brief 019, decision FP1).
    Project,
    /// The standard's registry values (brief 050a, FP3): a value at the Standard level. Refused
    /// while the project has no standard.
    Standard,
}

impl ObjectRef {
    /// The kind of holder it is; `None` for the standard's registry values.
    pub fn holder(self) -> Option<crate::naming::keys::Holder> {
        use crate::naming::keys::Holder;
        match self {
            ObjectRef::Device(_) => Some(Holder::Device),
            ObjectRef::Wire(_) => Some(Holder::Wire),
            ObjectRef::Cable(_) => Some(Holder::Cable),
            ObjectRef::Page(_) => Some(Holder::Page),
            ObjectRef::Project => Some(Holder::Project),
            ObjectRef::Standard => None,
        }
    }
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
    /// Change a page in place (design 072 §5.1): its title, number, folder, look or display
    /// choices. Its place in the page order and its kind stay.
    ModifyPage {
        id: PageId,
        before: Page,
        after: Page,
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
    CreateDevice {
        id: DeviceId,
        device: Device,
    },
    DeleteDevice {
        id: DeviceId,
        device: Device,
    },
    /// Change a device's stored fields, its type among them (design 072 F13): the whole device
    /// before and after, so the inverse swaps them.
    ModifyDevice {
        id: DeviceId,
        before: Device,
        after: Device,
    },
    /// Change which embedded definition a component's symbols use (brief 048, FP5), keeping
    /// its properties, terminals and representations.
    SetDeviceDefinition {
        id: DeviceId,
        before: Option<DefinitionId>,
        after: Option<DefinitionId>,
    },
    AddConnectionPoint {
        id: ConnectionPointId,
        connection_point: ConnectionPoint,
    },
    RemoveConnectionPoint {
        id: ConnectionPointId,
        connection_point: ConnectionPoint,
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
    CreateWire {
        id: WireId,
        wire: Wire,
    },
    DeleteWire {
        id: WireId,
        wire: Wire,
    },
    /// A wire's values, net or place in its net's order change, the whole struct recorded (design
    /// 072 F20). Its ends and conductor id never change: a wire between other points is another
    /// wire.
    ModifyWire {
        id: WireId,
        before: Wire,
        after: Wire,
    },
    /// A route drawn (design 072 F31).
    AddRoute {
        id: RouteId,
        route: Route,
    },
    RemoveRoute {
        id: RouteId,
        route: Route,
    },
    /// A route redrawn or its ends moved, the whole struct recorded.
    ModifyRoute {
        id: RouteId,
        before: Route,
        after: Route,
    },
    /// A drawing aid made (design 072 §6.2).
    CreateAid {
        id: crate::naming::id::AidId,
        aid: crate::objects::aid::Aid,
    },
    DeleteAid {
        id: crate::naming::id::AidId,
        aid: crate::objects::aid::Aid,
    },
    /// A drawing aid's values changed, the whole aid recorded.
    ModifyAid {
        id: crate::naming::id::AidId,
        before: crate::objects::aid::Aid,
        after: crate::objects::aid::Aid,
    },
    /// A loose part added to the BOM (design 072 §6.4).
    CreateLoosePart {
        id: crate::naming::id::LoosePartId,
        part: crate::objects::loose_part::LoosePart,
    },
    DeleteLoosePart {
        id: crate::naming::id::LoosePartId,
        part: crate::objects::loose_part::LoosePart,
    },
    /// A loose part's values changed, the whole part recorded.
    ModifyLoosePart {
        id: crate::naming::id::LoosePartId,
        before: crate::objects::loose_part::LoosePart,
        after: crate::objects::loose_part::LoosePart,
    },
    CreateCable {
        id: CableId,
        cable: Cable,
    },
    DeleteCable {
        id: CableId,
        cable: Cable,
    },
    /// A cable's fields or cores changed, the whole struct recorded (design 072 §4.5).
    ModifyCable {
        id: CableId,
        before: Cable,
        after: Cable,
    },
    /// Link two devices (design 072 §2.6, F-ops `AddDeviceLink`/`RemoveDeviceLink`).
    AddDeviceLink {
        id: crate::naming::id::LinkId,
        link: crate::objects::user::DeviceLink,
    },
    RemoveDeviceLink {
        id: crate::naming::id::LinkId,
        link: crate::objects::user::DeviceLink,
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
    /// Change the project's own data (design 072 §5.3): its location and function lists and
    /// defaults, as one whole value.
    ModifyProject {
        before: crate::document::project::Project,
        after: crate::document::project::Project,
    },
    /// Write one whole style record (brief 050a, FP3): `None` is no record. A `Standard` layer
    /// write is refused while the project has no standard.
    SetStyle {
        layer: StyleLayer,
        catalogue: StyleCatalogue,
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

/// A cable's offer key (`4G`, `BN,BK,GY`): written only with the cores it offers, by ModifyCable.
pub const CABLE_CORES_KEY: &str = "cores";

impl Op {
    /// The `ModifyCable` that makes cable `id` from `before` into `after`; `None` when they are
    /// the same.
    pub fn modify_cable(id: CableId, before: &Cable, after: Cable) -> Option<Op> {
        (before != &after).then(|| Op::ModifyCable { id, before: before.clone(), after })
    }

    /// Edit cable `id` within `ops` (design 072 §4.5: a cable is written whole): `edit` changes the
    /// `after` of the `ModifyCable` that `ops` already holds for it, or of a new one from `doc`'s
    /// cable. A `ModifyCable` left unchanged is dropped; nothing when the cable is not there.
    pub fn edit_cable(doc: &Document, ops: &mut Vec<Op>, id: CableId, edit: impl FnOnce(&mut Cable)) {
        let at = ops.iter().position(|o| matches!(o, Op::ModifyCable { id: k, .. } if *k == id));
        match at {
            Some(i) => {
                if let Some(Op::ModifyCable { before, after, .. }) = ops.get_mut(i) {
                    edit(after);
                    if before == after {
                        ops.remove(i);
                    }
                }
            }
            None => {
                let Some(cab) = doc.cables.get(&id) else { return };
                let mut after = cab.clone();
                edit(&mut after);
                ops.extend(Op::modify_cable(id, cab, after));
            }
        }
    }

    /// Set property `key` of cable `id` to `value` within `ops` ([`Op::edit_cable`]).
    pub fn set_cable_property(doc: &Document, ops: &mut Vec<Op>, id: CableId, key: &str, value: Option<String>) {
        // A tag or description that does not read (too long, a hidden character) is left as it was:
        // the commands rule it out with the text types first.
        Op::edit_cable(doc, ops, id, |c| {
            let _ = c.set_value(key, value.as_deref());
        });
    }

    /// The op that undoes this one.
    pub fn inverse(&self) -> Op {
        match self.clone() {
            Op::AddPage { id, page, index } => Op::RemovePage { id, page, index },
            Op::RemovePage { id, page, index } => Op::AddPage { id, page, index },
            Op::ModifyPage { id, before, after } => Op::ModifyPage { id, before: after, after: before },
            Op::AddLayer { id, layer } => Op::RemoveLayer { id, layer },
            Op::RemoveLayer { id, layer } => Op::AddLayer { id, layer },
            Op::AddItem { id, item } => Op::RemoveItem { id, item },
            Op::RemoveItem { id, item } => Op::AddItem { id, item },
            Op::ModifyItem { id, before, after } => Op::ModifyItem { id, before: after, after: before },
            Op::CreateDevice { id, device } => Op::DeleteDevice { id, device },
            Op::DeleteDevice { id, device } => Op::CreateDevice { id, device },
            Op::ModifyDevice { id, before, after } => Op::ModifyDevice { id, before: after, after: before },
            Op::SetDeviceDefinition { id, before, after } => Op::SetDeviceDefinition { id, before: after, after: before },
            Op::AddConnectionPoint { id, connection_point } => Op::RemoveConnectionPoint { id, connection_point },
            Op::RemoveConnectionPoint { id, connection_point } => Op::AddConnectionPoint { id, connection_point },
            Op::LinkRepresentation { id, rep } => Op::UnlinkRepresentation { id, rep },
            Op::UnlinkRepresentation { id, rep } => Op::LinkRepresentation { id, rep },
            Op::ModifyRepresentation { id, before, after } => Op::ModifyRepresentation { id, before: after, after: before },
            Op::CreateWire { id, wire } => Op::DeleteWire { id, wire },
            Op::DeleteWire { id, wire } => Op::CreateWire { id, wire },
            Op::ModifyWire { id, before, after } => Op::ModifyWire { id, before: after, after: before },
            Op::AddRoute { id, route } => Op::RemoveRoute { id, route },
            Op::RemoveRoute { id, route } => Op::AddRoute { id, route },
            Op::ModifyRoute { id, before, after } => Op::ModifyRoute { id, before: after, after: before },
            Op::CreateAid { id, aid } => Op::DeleteAid { id, aid },
            Op::DeleteAid { id, aid } => Op::CreateAid { id, aid },
            Op::ModifyAid { id, before, after } => Op::ModifyAid { id, before: after, after: before },
            Op::CreateLoosePart { id, part } => Op::DeleteLoosePart { id, part },
            Op::DeleteLoosePart { id, part } => Op::CreateLoosePart { id, part },
            Op::ModifyLoosePart { id, before, after } => Op::ModifyLoosePart { id, before: after, after: before },
            Op::CreateCable { id, cable } => Op::DeleteCable { id, cable },
            Op::DeleteCable { id, cable } => Op::CreateCable { id, cable },
            Op::ModifyCable { id, before, after } => Op::ModifyCable { id, before: after, after: before },
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
            Op::ModifyProject { before, after } => Op::ModifyProject { before: after, after: before },
            Op::AddDeviceLink { id, link } => Op::RemoveDeviceLink { id, link },
            Op::RemoveDeviceLink { id, link } => Op::AddDeviceLink { id, link },
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
            Op::ModifyPage { id, before, after } => {
                holds(&doc.pages, *id, before, "page", checked)?;
                if checked && before.kind != after.kind {
                    return Err(pre(format!("page {id}: a page's kind is set when it is made")));
                }
                if let Some(old) = doc.pages.insert(*id, after.clone()) {
                    doc.index.page(*id, &old, Delta::Remove);
                }
                doc.index.page(*id, after, Delta::Add);
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
            Op::CreateDevice { id, device } => {
                absent(&doc.devices, *id, "device")?;
                doc.index.device(*id, device, Delta::Add);
                doc.devices.insert(*id, device.clone());
            }
            Op::DeleteDevice { id, device } => {
                holds(&doc.devices, *id, device, "device", checked)?;
                if doc.device_links.values().any(|l| l.names(*id)) {
                    return Err(pre(format!("device {id} is linked to another device; remove its links first")));
                }
                if let Some(old) = doc.devices.remove(id) {
                    doc.index.device(*id, &old, Delta::Remove);
                }
            }
            Op::ModifyDevice { id, before, after } => {
                holds(&doc.devices, *id, before, "device", checked)?;
                if let Some(old) = doc.devices.insert(*id, after.clone()) {
                    doc.index.device(*id, &old, Delta::Remove);
                }
                doc.index.device(*id, after, Delta::Add);
            }
            Op::SetDeviceDefinition { id, before, after } => {
                let c = doc.devices.get(id).ok_or_else(|| pre(format!("device {id} does not exist")))?;
                if checked && c.definition != *before {
                    return Err(pre(format!("device {id}'s definition differs from the recorded value")));
                }
                if let Some(old) = doc.devices.remove(id) {
                    doc.index.device(*id, &old, Delta::Remove);
                    let new = Device { definition: *after, ..old };
                    doc.index.device(*id, &new, Delta::Add);
                    doc.devices.insert(*id, new);
                }
            }
            Op::AddConnectionPoint { id, connection_point } => {
                absent(&doc.connection_points, *id, "connection_point")?;
                doc.index.connection_point(*id, connection_point, Delta::Add);
                doc.connection_points.insert(*id, connection_point.clone());
            }
            Op::RemoveConnectionPoint { id, connection_point } => {
                holds(&doc.connection_points, *id, connection_point, "connection_point", checked)?;
                if let Some(old) = doc.connection_points.remove(id) {
                    doc.index.connection_point(*id, &old, Delta::Remove);
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
            Op::CreateWire { id, wire } => {
                absent(&doc.wires, *id, "wire")?;
                if wire.ends[0] == wire.ends[1] {
                    return Err(pre(format!("wire {id} would end twice at one connection point")));
                }
                doc.index.conductor(*id, wire, Delta::Add);
                doc.wires.insert(*id, wire.clone());
            }
            Op::DeleteWire { id, wire } => {
                holds(&doc.wires, *id, wire, "wire", checked)?;
                if let Some(old) = doc.wires.remove(id) {
                    doc.index.conductor(*id, &old, Delta::Remove);
                }
            }
            Op::ModifyWire { id, before, after } => {
                if !before.same_wire(after) {
                    return Err(pre(format!("wire {id}: a wire's ends and conductor never change")));
                }
                let w = doc.wires.get_mut(id).ok_or_else(|| pre(format!("wire {id} does not exist")))?;
                if checked && *before != *w {
                    return Err(pre(format!("wire {id} differs from the recorded value")));
                }
                let old = std::mem::replace(w, after.clone());
                doc.index.conductor(*id, &old, Delta::Remove);
                doc.index.conductor(*id, after, Delta::Add);
            }
            Op::AddRoute { id, route } => {
                absent(&doc.routes, *id, "route")?;
                route.check(*id).map_err(|e| pre(format!("route {id}: {e}")))?;
                doc.index.route(*id, route, Delta::Add);
                doc.routes.insert(*id, route.clone());
            }
            Op::RemoveRoute { id, route } => {
                holds(&doc.routes, *id, route, "route", checked)?;
                if let Some(old) = doc.routes.remove(id) {
                    doc.index.route(*id, &old, Delta::Remove);
                }
            }
            Op::ModifyRoute { id, before, after } => {
                holds(&doc.routes, *id, before, "route", checked)?;
                after.check(*id).map_err(|e| pre(format!("route {id}: {e}")))?;
                if let Some(old) = doc.routes.insert(*id, after.clone()) {
                    doc.index.route(*id, &old, Delta::Remove);
                }
                doc.index.route(*id, after, Delta::Add);
            }
            Op::CreateAid { id, aid } => {
                absent(&doc.aids, *id, "aid")?;
                if let Some(e) = aid.problem() {
                    return Err(pre(format!("aid {id}: {e}")));
                }
                doc.index.aid(*id, aid, Delta::Add);
                doc.aids.insert(*id, aid.clone());
            }
            Op::DeleteAid { id, aid } => {
                holds(&doc.aids, *id, aid, "aid", checked)?;
                if let Some(old) = doc.aids.remove(id) {
                    doc.index.aid(*id, &old, Delta::Remove);
                }
            }
            Op::ModifyAid { id, before, after } => {
                holds(&doc.aids, *id, before, "aid", checked)?;
                if let Some(e) = after.problem() {
                    return Err(pre(format!("aid {id}: {e}")));
                }
                if let Some(old) = doc.aids.insert(*id, after.clone()) {
                    doc.index.aid(*id, &old, Delta::Remove);
                }
                doc.index.aid(*id, after, Delta::Add);
            }
            Op::CreateLoosePart { id, part } => {
                absent(&doc.loose_parts, *id, "loose part")?;
                if let Some(e) = part.problem() {
                    return Err(pre(format!("loose part {id}: {e}")));
                }
                doc.loose_parts.insert(*id, part.clone());
            }
            Op::DeleteLoosePart { id, part } => {
                holds(&doc.loose_parts, *id, part, "loose part", checked)?;
                doc.loose_parts.remove(id);
            }
            Op::ModifyLoosePart { id, before, after } => {
                holds(&doc.loose_parts, *id, before, "loose part", checked)?;
                if let Some(e) = after.problem() {
                    return Err(pre(format!("loose part {id}: {e}")));
                }
                doc.loose_parts.insert(*id, after.clone());
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
            Op::ModifyCable { id, before, after } => {
                holds(&doc.cables, *id, before, "cable", checked)?;
                if let Some(old) = doc.cables.insert(*id, after.clone()) {
                    doc.index.cable(*id, &old, Delta::Remove);
                }
                doc.index.cable(*id, after, Delta::Add);
            }
            Op::SetProperty { object, key, before, after } => {
                let reserved = match object.holder() {
                    Some(h) => crate::naming::keys::is_reserved_on(h, key),
                    None => crate::naming::keys::is_reserved(key),
                };
                if reserved {
                    return Err(pre(format!("property {key} is a field now (design 072); its object's Modify op changes it")));
                }
                // Design 072 §4.5: a cable is written whole; only ModifyCable changes it.
                if matches!(object, ObjectRef::Cable(_)) {
                    return Err(pre(format!("property {key} of a cable is written with the cable; ModifyCable changes it")));
                }
                let props = match object {
                    ObjectRef::Device(id) => doc.devices.get_mut(id).map(|c| &mut c.properties),
                    ObjectRef::Wire(id) => doc.wires.get_mut(id).map(|c| &mut c.display),
                    ObjectRef::Cable(_) => None,
                    ObjectRef::Page(id) => doc.pages.get_mut(id).map(|p| &mut p.display),
                    ObjectRef::Project => Some(&mut doc.display),
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
                // A style choice is indexed (brief 050a): the style's explicit users.
                if crate::objects::styles::is_choice_key(key) {
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
            }
            Op::EmbedDefinition { id, definition } => {
                absent(&doc.definitions, *id, "definition")?;
                doc.definitions.insert(*id, definition.clone());
            }
            Op::RemoveDefinition { id, definition } => {
                holds(&doc.definitions, *id, definition, "definition", checked)?;
                doc.definitions.remove(id);
            }
            Op::AddDeviceLink { id, link } => {
                absent(&doc.device_links, *id, "device link")?;
                if link.a == link.b || !doc.devices.contains_key(&link.a) || !doc.devices.contains_key(&link.b) {
                    return Err(pre(format!("device link {id} must join two different devices that exist")));
                }
                if doc.device_links.values().any(|l| l.same_as(link)) {
                    return Err(pre(format!("device link {id} repeats a link the devices already have")));
                }
                doc.device_links.insert(*id, *link);
            }
            Op::RemoveDeviceLink { id, link } => {
                holds(&doc.device_links, *id, link, "device link", checked)?;
                doc.device_links.remove(id);
            }
            Op::ModifyProject { before, after } => {
                if checked && &doc.project != before {
                    return Err(pre("the project's lists differ from the recorded value".into()));
                }
                doc.project = after.clone();
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
                if let Some(r) = after.as_ref().filter(|r| r.catalogue() != *catalogue) {
                    return Err(pre(format!("style {catalogue}/{id}: a {} record filed under {catalogue}", r.catalogue())));
                }
                let current = cats.get(catalogue).and_then(|c| c.get(id));
                if checked && current != before.as_ref() {
                    return Err(pre(format!("style {catalogue}/{id} ({}) differs from the recorded value", layer.word())));
                }
                crate::objects::styles::put(cats, *catalogue, id, after.as_ref());
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
            Op::ModifyPage { .. } => "ModifyPage",
            Op::RemovePage { .. } => "RemovePage",
            Op::AddLayer { .. } => "AddLayer",
            Op::RemoveLayer { .. } => "RemoveLayer",
            Op::AddItem { .. } => "AddItem",
            Op::ModifyItem { .. } => "ModifyItem",
            Op::RemoveItem { .. } => "RemoveItem",
            Op::CreateDevice { .. } => "CreateDevice",
            Op::DeleteDevice { .. } => "DeleteDevice",
            Op::ModifyDevice { .. } => "ModifyDevice",
            Op::SetDeviceDefinition { .. } => "SetDeviceDefinition",
            Op::AddConnectionPoint { .. } => "AddConnectionPoint",
            Op::RemoveConnectionPoint { .. } => "RemoveConnectionPoint",
            Op::LinkRepresentation { .. } => "LinkRepresentation",
            Op::ModifyRepresentation { .. } => "ModifyRepresentation",
            Op::UnlinkRepresentation { .. } => "UnlinkRepresentation",
            Op::CreateWire { .. } => "CreateWire",
            Op::DeleteWire { .. } => "DeleteWire",
            Op::ModifyWire { .. } => "ModifyWire",
            Op::AddRoute { .. } => "AddRoute",
            Op::RemoveRoute { .. } => "RemoveRoute",
            Op::ModifyRoute { .. } => "ModifyRoute",
            Op::CreateAid { .. } => "CreateAid",
            Op::DeleteAid { .. } => "DeleteAid",
            Op::ModifyAid { .. } => "ModifyAid",
            Op::CreateLoosePart { .. } => "CreateLoosePart",
            Op::DeleteLoosePart { .. } => "DeleteLoosePart",
            Op::ModifyLoosePart { .. } => "ModifyLoosePart",
            Op::CreateCable { .. } => "CreateCable",
            Op::DeleteCable { .. } => "DeleteCable",
            Op::ModifyCable { .. } => "ModifyCable",
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
            Op::ModifyProject { .. } => "ModifyProject",
            Op::AddDeviceLink { .. } => "AddDeviceLink",
            Op::RemoveDeviceLink { .. } => "RemoveDeviceLink",
        }
    }
}
