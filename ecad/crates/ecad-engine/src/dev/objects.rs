//! Objects the debugger follows: ops as before/after views, what each op touches, labels and JSON.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::{ConnectionPointId, Document, ItemId, ItemKind, Op, PageId, RepresentationId, Subject, Transaction};
use serde_json::{Value, json};

use super::debug::*;
use crate::project::Mapped;

/// One op's target, before and after values (JSON of the model's own serialisation).
#[derive(Debug, Clone, PartialEq)]
pub struct OpView {
    pub name: &'static str,
    /// Every object the op names: its target first, then the objects it refers to.
    pub targets: Vec<ObjectId>,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

fn val<T: serde::Serialize>(v: &T) -> Option<Value> {
    Some(serde_json::to_value(v).unwrap_or_else(|e| Value::String(format!("(cannot show: {e})"))))
}

/// An op's view.
pub fn op_view(op: &Op) -> OpView {
    let (before, after) = match op {
        Op::AddPage { page, index, .. } => (None, val(&json!({"page": page, "index": index}))),
        Op::RemovePage { page, index, .. } => (val(&json!({"page": page, "index": index})), None),
        Op::ModifyPage { before, after, .. } => (val(before), val(after)),
        Op::AddLayer { layer, .. } => (None, val(layer)),
        Op::RemoveLayer { layer, .. } => (val(layer), None),
        Op::AddItem { item, .. } => (None, val(item)),
        Op::ModifyItem { before, after, .. } => (val(before), val(after)),
        Op::RemoveItem { item, .. } => (val(item), None),
        Op::CreateDevice { device, .. } => (None, val(device)),
        Op::DeleteDevice { device, .. } => (val(device), None),
        Op::ModifyDevice { before, after, .. } => (val(before), val(after)),
        Op::SetDeviceDefinition { before, after, .. } => (val(before), val(after)),
        Op::AddConnectionPoint { connection_point, .. } => (None, val(connection_point)),
        Op::RemoveConnectionPoint { connection_point, .. } => (val(connection_point), None),
        Op::LinkRepresentation { rep, .. } => (None, val(rep)),
        Op::ModifyRepresentation { before, after, .. } => (val(before), val(after)),
        Op::UnlinkRepresentation { rep, .. } => (val(rep), None),
        Op::CreateWire { wire, .. } => (None, val(wire)),
        Op::DeleteWire { wire, .. } => (val(wire), None),
        Op::ModifyWire { before, after, .. } => (val(before), val(after)),
        Op::AddRoute { route, .. } => (None, val(route)),
        Op::RemoveRoute { route, .. } => (val(route), None),
        Op::ModifyRoute { before, after, .. } => (val(before), val(after)),
        Op::CreateAid { aid, .. } => (None, val(aid)),
        Op::DeleteAid { aid, .. } => (val(aid), None),
        Op::ModifyAid { before, after, .. } => (val(before), val(after)),
        Op::CreateLoosePart { part, .. } => (None, val(part)),
        Op::DeleteLoosePart { part, .. } => (val(part), None),
        Op::ModifyLoosePart { before, after, .. } => (val(before), val(after)),
        Op::CreateCable { cable, .. } => (None, val(cable)),
        Op::DeleteCable { cable, .. } => (val(cable), None),
        Op::ModifyCable { before, after, .. } => (val(before), val(after)),
        Op::SetProperty { key, before, after, .. } => (val(&json!({ key.as_str(): before })), val(&json!({ key.as_str(): after }))),
        Op::EmbedDefinition { definition, .. } => (None, val(definition)),
        Op::RemoveDefinition { definition, .. } => (val(definition), None),
        Op::SetProjectSettings { before, after } => (val(before), val(after)),
        Op::NameVersion { name, at, .. } => (None, val(&json!({"name": name, "at": at}))),
        Op::UnnameVersion { name, at, .. } => (val(&json!({"name": name, "at": at})), None),
        Op::AddFolder { folder, index, .. } => (None, val(&json!({"folder": folder, "index": index}))),
        Op::RemoveFolder { folder, index, .. } => (val(&json!({"folder": folder, "index": index})), None),
        Op::ModifyFolder { before, after, .. } => (val(before), val(after)),
        Op::SetStyle { layer, catalogue, id, before, after } => (
            val(&json!({"layer": layer, "catalogue": catalogue, "id": id, "record": before})),
            val(&json!({"layer": layer, "catalogue": catalogue, "id": id, "record": after})),
        ),
        Op::SetStandard { before, after } => (val(before), val(after)),
        Op::ModifyProject { before, after } => (val(before), val(after)),
        Op::AddDeviceLink { link, .. } => (None, val(link)),
        Op::RemoveDeviceLink { link, .. } => (val(link), None),
    };
    OpView { name: op.name(), targets: op_targets(op), before, after }
}

fn item_references(item: &ecad_model::Item, out: &mut Vec<ObjectId>) {
    out.push(ObjectId::Page(item.page));
    out.push(ObjectId::Layer(item.style.layer));
    if let ItemKind::Symbol(s) = &item.kind {
        out.push(ObjectId::Definition(s.definition));
    }
}

fn rep_references(rep: &ecad_model::Representation, out: &mut Vec<ObjectId>) {
    out.push(rep.subject.into());
    out.push(ObjectId::Page(rep.page));
    out.extend(rep.items.iter().map(|i| ObjectId::Item(*i)));
    out.extend(rep.connection_points.keys().map(|t| ObjectId::ConnectionPoint(*t)));
}

/// Every object an op names: its target first.
pub(crate) fn op_targets(op: &Op) -> Vec<ObjectId> {
    let mut out = Vec::new();
    match op {
        Op::AddPage { id, page, .. } | Op::RemovePage { id, page, .. } => {
            out.push(ObjectId::Page(*id));
            out.extend(page.kind.strip().map(ObjectId::Device));
        }
        Op::ModifyPage { id, before, .. } => {
            out.push(ObjectId::Page(*id));
            out.extend(before.kind.strip().map(ObjectId::Device));
        }
        Op::AddLayer { id, .. } | Op::RemoveLayer { id, .. } => out.push(ObjectId::Layer(*id)),
        Op::AddItem { id, item } | Op::RemoveItem { id, item } => {
            out.push(ObjectId::Item(*id));
            item_references(item, &mut out);
        }
        Op::ModifyItem { id, before, after } => {
            out.push(ObjectId::Item(*id));
            item_references(before, &mut out);
            item_references(after, &mut out);
        }
        Op::CreateDevice { id, device } | Op::DeleteDevice { id, device } => {
            out.push(ObjectId::Device(*id));
            out.extend(device.definition.map(ObjectId::Definition));
        }
        Op::ModifyDevice { id, before, after } => {
            out.push(ObjectId::Device(*id));
            out.extend(before.definition.iter().chain(&after.definition).copied().map(ObjectId::Definition));
        }
        Op::SetDeviceDefinition { id, before, after } => {
            out.push(ObjectId::Device(*id));
            out.extend(before.iter().chain(after).copied().map(ObjectId::Definition));
        }
        Op::AddConnectionPoint { id, connection_point } | Op::RemoveConnectionPoint { id, connection_point } => {
            out.push(ObjectId::ConnectionPoint(*id));
            out.push(match connection_point.owner {
                ecad_model::ConnectionPointOwner::Device(c) => ObjectId::Device(c),
                ecad_model::ConnectionPointOwner::Aid(a) => ObjectId::Aid(a),
            });
        }
        Op::LinkRepresentation { id, rep } | Op::UnlinkRepresentation { id, rep } => {
            out.push(ObjectId::Representation(*id));
            rep_references(rep, &mut out);
        }
        Op::ModifyRepresentation { id, before, after } => {
            out.push(ObjectId::Representation(*id));
            rep_references(before, &mut out);
            rep_references(after, &mut out);
        }
        Op::CreateWire { id, wire } | Op::DeleteWire { id, wire } => {
            out.push(ObjectId::Wire(*id));
            out.extend(wire.ends.iter().map(|t| ObjectId::ConnectionPoint(*t)));
        }
        Op::ModifyWire { id, .. } => out.push(ObjectId::Wire(*id)),
        Op::AddRoute { id, route } | Op::RemoveRoute { id, route } => {
            out.push(ObjectId::Route(*id));
            out.push(ObjectId::Page(route.page));
            out.extend(route.points_landed().map(ObjectId::ConnectionPoint));
        }
        Op::ModifyRoute { id, before, after } => {
            out.push(ObjectId::Route(*id));
            out.extend(before.points_landed().chain(after.points_landed()).map(ObjectId::ConnectionPoint));
        }
        Op::CreateAid { id, .. } | Op::DeleteAid { id, .. } | Op::ModifyAid { id, .. } => out.push(ObjectId::Aid(*id)),
        Op::CreateLoosePart { id, .. } | Op::DeleteLoosePart { id, .. } | Op::ModifyLoosePart { id, .. } => out.push(ObjectId::LoosePart(*id)),
        Op::CreateCable { id, cable } | Op::DeleteCable { id, cable } => {
            out.push(ObjectId::Cable(*id));
            out.extend(cable.cores.iter().filter_map(member_object));
        }
        Op::ModifyCable { id, before, after } => {
            out.push(ObjectId::Cable(*id));
            out.extend(before.cores.iter().chain(&after.cores).filter_map(member_object));
        }
        Op::SetProperty { object, .. } => out.push(match object {
            ecad_model::ObjectRef::Device(c) => ObjectId::Device(*c),
            ecad_model::ObjectRef::Wire(c) => ObjectId::Wire(*c),
            ecad_model::ObjectRef::Cable(c) => ObjectId::Cable(*c),
            ecad_model::ObjectRef::Page(p) => ObjectId::Page(*p),
            ecad_model::ObjectRef::Project | ecad_model::ObjectRef::Standard => ObjectId::Settings,
        }),
        Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => out.push(ObjectId::Definition(*id)),
        Op::SetProjectSettings { .. } | Op::ModifyProject { .. } => out.push(ObjectId::Settings),
        // A device link (design 072 §2.6) shows in the history of both devices it joins.
        Op::AddDeviceLink { link, .. } | Op::RemoveDeviceLink { link, .. } => {
            out.push(ObjectId::Device(link.a));
            out.push(ObjectId::Device(link.b));
        }
        Op::NameVersion { .. } | Op::UnnameVersion { .. } => {}
        // Folders (brief 019) are not among the debugger's objects yet.
        Op::AddFolder { .. } | Op::RemoveFolder { .. } | Op::ModifyFolder { .. } => {}
        // The standard and style records (brief 050a) are project-wide, like the settings.
        Op::SetStyle { .. } | Op::SetStandard { .. } => out.push(ObjectId::Settings),
    }
    let mut seen = BTreeSet::new();
    out.retain(|i| seen.insert(*i));
    out
}

/// The object a cable member stands on: its wire, or its loose line while it has no wire.
fn member_object(m: &ecad_model::Core) -> Option<ObjectId> {
    m.wire().map(ObjectId::Wire).or_else(|| m.route().map(ObjectId::Route))
}

/// Whether `tx` names any of `ids`.
pub fn touches(tx: &Transaction, ids: &BTreeSet<ObjectId>) -> bool {
    tx.ops.iter().any(|op| op_targets(op).iter().any(|i| ids.contains(i)))
}

/// What a selected object stands for in the model, and the IDs the trail follows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Resolved {
    pub item: Option<ItemId>,
    pub representation: Option<RepresentationId>,
    pub subject: Option<Subject>,
    /// A terminal marker's terminal.
    pub connection_point: Option<ConnectionPointId>,
    /// The subject's terminals (a component) or attached terminals (a conductor).
    pub connection_points: Vec<ConnectionPointId>,
    /// The objects whose history the trail follows: item, representation, subject, terminals.
    pub ids: Vec<ObjectId>,
}

/// Follow a mapped handle into the model.
pub fn resolve(doc: &Document, mapped: Mapped) -> Resolved {
    let mut r = Resolved::default();
    match mapped {
        Mapped::Item(i) => {
            r.item = Some(i);
            r.representation = doc.representations.iter().find(|(_, rep)| rep.items.contains(&i)).map(|(id, _)| *id);
        }
        Mapped::Marker(rep, t) => {
            r.representation = Some(rep);
            r.connection_point = Some(t);
        }
        // A fan view line (brief 005) stands for its cable; it is no item.
        Mapped::Fan(_) => {}
        // A reference's derived text (brief 010) stands for its reference representation.
        Mapped::RefText(rep) => r.representation = Some(rep),
        // A device's tag text or terminal label (brief 018) stands for its symbol representation.
        Mapped::Tag(rep) => r.representation = Some(rep),
        // A conductor label (brief 022) stands for its conductor.
        Mapped::Label(_) => {}
        // A line or text of the sheet frame (brief 019) stands for nothing stored.
        Mapped::Frame(_) => {}
        // A device cross-reference (brief 025) stands for its contact or head representation.
        Mapped::DeviceReference(rep) => r.representation = Some(rep),
        // A link line (brief 030) stands for its strip.
        Mapped::JumperBar(..) => {}
        // A T's dot (lane 072 D) stands for the route that ends there.
        Mapped::Tee(_) => {}
    }
    r.subject = match mapped {
        Mapped::Fan(k) => Some(Subject::Cable(k)),
        Mapped::Label(c) => Some(Subject::Wire(c)),
        Mapped::JumperBar(c, _) => Some(Subject::Device(c)),
        Mapped::Tee(r) => Some(Subject::Route(r)),
        _ => r.representation.and_then(|id| doc.representations.get(&id)).map(|rep| rep.subject),
    };
    r.connection_points = match r.subject {
        Some(Subject::Device(c)) => doc.connection_points.iter().filter(|(_, t)| t.device() == Some(c)).map(|(id, _)| *id).collect(),
        Some(Subject::Wire(c)) => doc.wires.get(&c).map(|x| x.ends.to_vec()).unwrap_or_default(),
        Some(Subject::Route(x)) => doc.routes.get(&x).map(|r| r.points_landed().collect()).unwrap_or_default(),
        _ => Vec::new(),
    };
    r.ids.extend(r.item.map(ObjectId::Item));
    r.ids.extend(r.representation.map(ObjectId::Representation));
    r.ids.extend(r.subject.map(ObjectId::from));
    // A route's line: the wire it carries follows it (design 072 §4: routes draw, wires connect).
    if let Some(Subject::Route(x)) = r.subject {
        r.ids.extend(ecad_electrical::wires::routes::wire_of_route(doc, x).map(ObjectId::Wire));
    }
    r.ids.extend(r.connection_point.map(ObjectId::ConnectionPoint));
    for t in &r.connection_points {
        if r.connection_point != Some(*t) {
            r.ids.push(ObjectId::ConnectionPoint(*t));
        }
    }
    r
}

/// An object as the model holds it (`None` when it does not exist in `doc`).
pub fn object_json(doc: &Document, id: ObjectId) -> Option<Value> {
    match id {
        ObjectId::Page(i) => doc.pages.get(&i).and_then(val),
        ObjectId::Layer(i) => doc.layers.get(&i).and_then(val),
        ObjectId::Item(i) => doc.items.get(&i).and_then(val),
        ObjectId::Device(i) => doc.devices.get(&i).and_then(val),
        ObjectId::ConnectionPoint(i) => doc.connection_points.get(&i).and_then(val),
        ObjectId::Representation(i) => doc.representations.get(&i).and_then(val),
        ObjectId::Wire(i) => doc.wires.get(&i).and_then(val),
        ObjectId::Cable(i) => doc.cables.get(&i).and_then(val),
        ObjectId::Route(i) => doc.routes.get(&i).and_then(val),
        ObjectId::Aid(i) => doc.aids.get(&i).and_then(val),
        ObjectId::LoosePart(i) => doc.loose_parts.get(&i).and_then(val),
        ObjectId::Definition(i) => doc.definitions.get(&i).and_then(val),
        ObjectId::Settings => val(&doc.settings),
    }
}

/// The objects that refer to `id`: a definition's components and symbol instances, a component's
/// terminals and representations, a terminal's attached conductors, a conductor's cables and
/// representations, a cable's members (in member order) and then its representations (design 004
/// §9: slash → cable → conductors). Representations of a conductor or a cable are listed by page,
/// in page order (design 010 §8: a cable drawn on several pages, a conductor's route on each), and
/// a cable reference's are its cable and its partner reference.
pub fn used_by(doc: &Document, id: ObjectId) -> Vec<ObjectId> {
    let page_index: BTreeMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    let reps_of = |s: Subject| {
        let mut v: Vec<(usize, RepresentationId)> = doc
            .representations
            .iter()
            .filter(|(_, r)| r.subject == s)
            .map(|(i, r)| (page_index.get(&r.page).copied().unwrap_or(usize::MAX), *i))
            .collect();
        v.sort();
        v.into_iter().map(|(_, i)| ObjectId::Representation(i))
    };
    match id {
        ObjectId::Definition(d) => doc
            .devices
            .iter()
            .filter(|(_, c)| c.definition == Some(d))
            .map(|(i, _)| ObjectId::Device(*i))
            .chain(doc.items.iter().filter(|(_, it)| matches!(&it.kind, ItemKind::Symbol(s) if s.definition == d)).map(|(i, _)| ObjectId::Item(*i)))
            .collect(),
        ObjectId::Device(c) => doc
            .connection_points
            .iter()
            .filter(|(_, t)| t.device() == Some(c))
            .map(|(i, _)| ObjectId::ConnectionPoint(*i))
            .chain(reps_of(Subject::Device(c)))
            .collect(),
        ObjectId::ConnectionPoint(t) => doc
            .wires
            .iter()
            .filter(|(_, c)| c.ends.contains(&t))
            .map(|(i, _)| ObjectId::Wire(*i))
            .chain(doc.routes.iter().filter(|(_, r)| r.points_landed().any(|p| p == t)).map(|(i, _)| ObjectId::Route(*i)))
            .collect(),
        ObjectId::Wire(c) => doc
            .cables
            .iter()
            .filter(|(_, k)| k.cores.iter().any(|m| m.wire() == Some(c)))
            .map(|(i, _)| ObjectId::Cable(*i))
            .chain(reps_of(Subject::Wire(c)))
            .chain(ecad_electrical::wires::routes::routes_of_wire(doc, c).into_iter().map(ObjectId::Route))
            .collect(),
        ObjectId::Cable(c) => {
            doc.cables.get(&c).into_iter().flat_map(|k| k.cores.iter().filter_map(member_object)).chain(reps_of(Subject::Cable(c))).collect()
        }
        ObjectId::Aid(a) => reps_of(Subject::Aid(a)).collect(),
        ObjectId::Route(r) => doc
            .routes
            .iter()
            .filter(|(_, x)| x.ends.iter().any(|e| e.route() == Some(r)))
            .map(|(i, _)| ObjectId::Route(*i))
            .chain(reps_of(Subject::Route(r)))
            .chain(ecad_electrical::wires::routes::wire_of_route(doc, r).map(ObjectId::Wire))
            .collect(),
        ObjectId::Page(p) => doc
            .representations
            .iter()
            .filter(|(_, r)| r.page == p)
            .map(|(i, _)| ObjectId::Representation(*i))
            .chain(doc.routes.iter().filter(|(_, r)| r.page == p).map(|(i, _)| ObjectId::Route(*i)))
            .collect(),
        ObjectId::Representation(r) => match doc.representations.get(&r) {
            Some(rep) if ecad_electrical::references::cables::is_reference(rep) => {
                let mut out: Vec<ObjectId> = vec![ObjectId::from(rep.subject)];
                if let Some(x) = ecad_electrical::references::cables::all(doc).into_iter().find(|x| x.reference == r) {
                    out.extend(x.partners.into_iter().map(ObjectId::Representation));
                }
                out
            }
            _ => Vec::new(),
        },
        ObjectId::Layer(_) | ObjectId::Item(_) | ObjectId::LoosePart(_) | ObjectId::Settings => Vec::new(),
    }
}

fn subject_label(doc: &Document, s: Subject) -> String {
    let t = match s {
        Subject::Device(c) => doc.devices.get(&c).and_then(|x| x.tag_shown()),
        Subject::Wire(c) => doc.wires.get(&c).and_then(|x| x.number_text()).map(str::to_string),
        Subject::Cable(c) => doc.cables.get(&c).and_then(|x| x.tag_shown()),
        Subject::Route(r) => doc.routes.get(&r).map(|x| format!("line on {}", x.page.short())),
        Subject::Aid(a) => doc.aids.get(&a).map(|x| match x {
            ecad_model::Aid::Continuation { name: Some(n), .. } => format!("continuation {n}"),
            ecad_model::Aid::Continuation { name: None, .. } => "unnamed continuation".into(),
            ecad_model::Aid::Box(_) => "location box".into(),
        }),
    };
    t.unwrap_or_else(|| format!("untagged {}", ObjectId::from(s).kind()))
}

/// A readable label for an object.
pub fn object_label(doc: &Document, id: ObjectId) -> String {
    match id {
        ObjectId::Page(i) => doc.pages.get(&i).map_or_else(String::new, |p| format!("page {}", p.title)),
        ObjectId::Layer(i) => doc.layers.get(&i).map_or_else(String::new, |l| format!("layer {}", l.name)),
        ObjectId::Item(i) => doc.items.get(&i).map_or_else(String::new, |it| match &it.kind {
            ItemKind::Primitive(p) => format!("{p:?}").split([' ', '{', '(']).next().unwrap_or("item").to_lowercase(),
            ItemKind::Symbol(_) => "symbol".into(),
        }),
        ObjectId::Device(c) => subject_label(doc, Subject::Device(c)),
        // A member of a cable shows its core (design 009 §8): `W3 · core 1`.
        ObjectId::Wire(c) => {
            let core = doc.cables_of(c).first().and_then(|k| doc.cables.get(k)).and_then(|k| {
                k.cores.iter().find(|m| m.wire() == Some(c)).and_then(|m| {
                    m.label_str()
                        .filter(|x| !x.trim().is_empty())
                        .map(|x| ecad_electrical::cables::words::shorten(x, ecad_electrical::cables::words::MAX_CORE_CHARS))
                })
            });
            match core {
                Some(core) => format!("{} · core {core}", subject_label(doc, Subject::Wire(c))),
                None => subject_label(doc, Subject::Wire(c)),
            }
        }
        ObjectId::Cable(c) => subject_label(doc, Subject::Cable(c)),
        ObjectId::Route(r) => subject_label(doc, Subject::Route(r)),
        ObjectId::Aid(a) => subject_label(doc, Subject::Aid(a)),
        ObjectId::LoosePart(i) => doc.loose_parts.get(&i).map_or_else(String::new, |p| format!("loose part {}", p.description)),
        ObjectId::ConnectionPoint(i) => doc.connection_points.get(&i).map_or_else(String::new, |t| {
            let name = t.label.clone().or_else(|| t.key.as_ref().map(ToString::to_string)).unwrap_or_else(|| i.short());
            format!("{}:{name}", subject_label(doc, t.owner.subject()))
        }),
        ObjectId::Representation(i) => doc.representations.get(&i).map_or_else(String::new, |r| {
            // A cable's reference end shows the text it draws (design 009 §8).
            if r.role == ecad_model::Role::Reference && matches!(r.subject, Subject::Cable(_)) {
                format!("{} of {} \"{}\"", r.role, subject_label(doc, r.subject), ecad_electrical::cables::layout::reference_text(doc, i))
            } else {
                format!("{} of {}", r.role, subject_label(doc, r.subject))
            }
        }),
        ObjectId::Definition(i) => doc.definitions.get(&i).map_or_else(String::new, |d| d.name.clone()),
        ObjectId::Settings => format!("length unit {:?}", doc.settings.length_unit),
    }
}

/// Every object of `doc`, in kind order.
pub fn objects(doc: &Document) -> Vec<ObjectId> {
    let mut out: Vec<ObjectId> = Vec::new();
    out.extend(doc.page_order.iter().map(|i| ObjectId::Page(*i)));
    out.extend(doc.layers.keys().map(|i| ObjectId::Layer(*i)));
    out.extend(doc.devices.keys().map(|i| ObjectId::Device(*i)));
    out.extend(doc.connection_points.keys().map(|i| ObjectId::ConnectionPoint(*i)));
    out.extend(doc.wires.keys().map(|i| ObjectId::Wire(*i)));
    out.extend(doc.cables.keys().map(|i| ObjectId::Cable(*i)));
    out.extend(doc.routes.keys().map(|i| ObjectId::Route(*i)));
    out.extend(doc.aids.keys().map(|i| ObjectId::Aid(*i)));
    out.extend(doc.loose_parts.keys().map(|i| ObjectId::LoosePart(*i)));
    out.extend(doc.representations.keys().map(|i| ObjectId::Representation(*i)));
    out.extend(doc.items.keys().map(|i| ObjectId::Item(*i)));
    out.extend(doc.definitions.keys().map(|i| ObjectId::Definition(*i)));
    out.push(ObjectId::Settings);
    out
}

/// Objects of `doc` per kind, for a summary line.
pub fn counts(doc: &Document) -> BTreeMap<&'static str, usize> {
    let mut m = BTreeMap::new();
    for id in objects(doc) {
        *m.entry(id.kind()).or_insert(0) += 1;
    }
    m
}

/// Bytes for display: text as it is (records are UTF-8 JSON), with control characters
/// (the line break included) and invalid UTF-8 escaped.
pub fn printable(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.utf8_chunks() {
        for c in chunk.valid().chars() {
            if c.is_control() {
                out.extend(c.escape_default());
            } else {
                out.push(c);
            }
        }
        out.extend(chunk.invalid().iter().map(|b| format!("\\x{b:02x}")));
    }
    out
}
