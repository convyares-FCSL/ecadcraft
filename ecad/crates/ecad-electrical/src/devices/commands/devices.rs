//! The rules of the device commands (design 018 §6–§8; experiments 004 and 008): tag settings
//! (`tagsettings`), deleting a wire, device or cable (`ecaddelete`, `devicedelete`), and the ops of
//! `convertcomponent` and `addconnectionpoint`. Typed input in, ops and a plain result (or a
//! [`CmdError`]) out.
//!
//! Excludes: what a handle on the page is (the engine crate's page map), committing and the JSON
//! answer (the engine crate's shims).

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

use ecad_model::{
    CableId, ConnectionPoint, ConnectionPointId, Device, DeviceId, Document, Form, IdAllocator, ItemId, Noun, Op, PageId, ProjectSettings, Pt,
    Representation, RepresentationId, RouteEnd, RouteId, Subject, WireId,
};

use crate::commands::run::{Access, Cx, Generic, Outcome, Pages};
use crate::devices;
use crate::error::CmdError;

// ---------------------------------------------------------------- tagsettings

/// Tag settings to write: the padding, and per starter symbol key the prefix (`None`: back to the
/// built-in one).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagSettings {
    pub padding: Option<u8>,
    pub prefixes: Vec<(String, Option<String>)>,
}

/// The tag settings `tagsettings` is given (brief 018, owner decision 2026-11-08): `padding` digits,
/// and per starter symbol the project's own prefix (`None`: the built-in one).
///
/// # Errors
/// A padding out of range, an unknown symbol, a prefix that is not 1 to
/// [`devices::MAX_PREFIX_LETTERS`] letters.
pub fn tag_settings(padding: Option<u64>, prefixes: impl IntoIterator<Item = (String, Option<String>)>) -> Result<TagSettings, CmdError> {
    let max = ecad_model::naming::tag::MAX_PADDING;
    let padding = match padding {
        None => None,
        Some(n) => Some(
            Some(n)
                .filter(|n| (1..=u64::from(max)).contains(n))
                .and_then(|n| u8::try_from(n).ok())
                .ok_or_else(|| CmdError::refused(format!("`padding` must be a whole number from 1 to {max}")))?,
        ),
    };
    let mut changes = Vec::new();
    for (k, v) in prefixes {
        let symbol = devices::json::starter(&k)?;
        let prefix = match v {
            None => None,
            Some(t) => Some(
                devices::valid_prefix(&t)
                    .ok_or_else(|| CmdError::refused(format!("A prefix is 1 to {} letters, for example K or SR.", devices::MAX_PREFIX_LETTERS)))?,
            ),
        };
        changes.push((symbol.key.to_string(), prefix));
    }
    Ok(TagSettings { padding, prefixes: changes })
}

/// Apply tag settings read by [`tag_settings`] to `after`: a prefix equal to the built-in one is
/// stored as none.
pub fn apply_tag_settings(after: &mut ProjectSettings, settings: TagSettings) {
    if let Some(n) = settings.padding {
        after.tag_padding = Some(n);
    }
    for (k, v) in settings.prefixes {
        crate::devices::set_symbol_prefix(after, ecad_model::naming::prefix::PrefixOf::Starter(k.into()), v.as_deref());
    }
}

// ---------------------------------------------------------------- ecaddelete

/// What `ecaddelete` deletes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Doomed {
    /// A wire: the routes drawn straight between its two connection points.
    Wire(WireId),
    /// A drawn route.
    Route(RouteId),
    Device(DeviceId),
    Cable(CableId),
}

/// A planned delete.
#[derive(Debug, Clone, PartialEq)]
pub struct Deleted {
    pub ops: Vec<Op>,
    /// A rack's cards that left it (brief 045).
    pub released: usize,
    /// A cable's wires, which stay (design 004 §8).
    pub kept: Option<usize>,
    /// What to say, when there is something to say.
    pub message: Option<String>,
}

/// Ops removing every representation of `subject` with its items.
fn remove_representations(doc: &Document, subject: Subject) -> Vec<Op> {
    let mut ops = Vec::new();
    for (rid, r) in doc.representations_of(subject) {
        ops.push(Op::UnlinkRepresentation { id: rid, rep: r.clone() });
        for i in &r.items {
            if let Some(item) = doc.items.get(i) {
                ops.push(Op::RemoveItem { id: *i, item: item.clone() });
            }
        }
    }
    ops
}

fn kept_words(n: usize) -> String {
    match n {
        0 => "0 wires are".into(),
        1 => "1 wire is".into(),
        n => format!("{n} wires are"),
    }
}

/// The ops removing routes `routes` with their drawings; routes landing on them end free.
fn remove_routes(doc: &Document, routes: &[RouteId]) -> Vec<Op> {
    let mut edits = crate::wires::routes::RouteEdits::new(doc);
    for r in routes {
        edits.free_tees(*r);
    }
    for r in routes {
        edits.forget(*r);
    }
    let mut ops = edits.ops();
    for r in routes {
        ops.extend(crate::wires::routes::remove_route_ops(doc, *r));
    }
    ops
}

/// `ecaddelete {conductor} | {route} | {component} | {cable}`: the object with its drawings, in
/// one transaction. A wire goes with the routes drawn straight between its two connection points
/// (the commit's rewrite takes it away) and leaves its cables; a route goes with its drawing, and
/// routes landing on it end free; a device's connection points let go of their routes and a
/// rack's cards leave it (design 045 §2.2, FP1); a cable's wires stay (design 004 §8, owner
/// answer O3).
///
/// # Errors
/// No such conductor, route, component or cable; a wire with no route drawn straight between its
/// points.
pub fn plan_delete(doc: &Document, doomed: Doomed) -> Result<Deleted, CmdError> {
    let mut ops = Vec::new();
    match doomed {
        Doomed::Wire(c) => {
            let wire = doc.wires.get(&c).ok_or(CmdError::refused("no such conductor"))?;
            let [a, b] = wire.ends;
            let direct: Vec<RouteId> = doc
                .routes_at(a)
                .filter(|r| doc.routes.get(r).is_some_and(|x| x.ends.contains(&RouteEnd::Point(a)) && x.ends.contains(&RouteEnd::Point(b))))
                .collect();
            if direct.is_empty() {
                return Err(CmdError::refused("This wire is not drawn as one line between its two points; erase the lines it runs along instead."));
            }
            for cable in doc.cables_of(c) {
                if let Some(k) = doc.cables.get(&cable) {
                    let after = ecad_model::Cable { cores: k.cores.iter().filter(|m| m.wire() != Some(c)).cloned().collect(), ..k.clone() };
                    ops.extend(Op::modify_cable(cable, k, after));
                }
            }
            ops.extend(remove_routes(doc, &direct));
            Ok(Deleted { ops, released: 0, kept: None, message: None })
        }
        Doomed::Route(r) => {
            if !doc.routes.contains_key(&r) {
                return Err(CmdError::refused("no such wire"));
            }
            Ok(Deleted { ops: remove_routes(doc, &[r]), released: 0, kept: None, message: None })
        }
        Doomed::Device(c) => {
            let component = doc.devices.get(&c).ok_or(CmdError::refused("no such component"))?;
            ops.extend(crate::wires::routes::free_points_ops(doc, doc.connection_points_of(c).map(|(t, _)| t)));
            ops.extend(remove_representations(doc, Subject::Device(c)));
            for (t, term) in doc.connection_points_of(c) {
                ops.push(Op::RemoveConnectionPoint { id: t, connection_point: term.clone() });
            }
            // Brief 045 (design 045 §2.2, FP1): a rack's cards leave it in the same transaction.
            let (released_ops, released) =
                if crate::plc::edit::station_of(doc, c).is_some() { crate::plc::rack::release_cards(doc, c) } else { (Vec::new(), 0) };
            ops.extend(released_ops);
            ops.extend(crate::devices::commands::device_links::unlink_ops(doc, c));
            ops.push(Op::DeleteDevice { id: c, device: component.clone() });
            let mut message =
                (released > 0).then(|| format!("Deleted {}; {}", devices::tags::tag_words(doc, c), crate::plc::rack::released_words(released)));
            if let Some(cleared) = crate::circuits::trace::cleared_words(doc, c) {
                message = Some(format!("{}; {cleared}", message.unwrap_or_else(|| format!("Deleted {}", devices::tags::tag_words(doc, c)))));
            }
            Ok(Deleted { ops, released, kept: None, message })
        }
        Doomed::Cable(k) => {
            let cable = doc.cables.get(&k).ok_or(CmdError::refused("no such cable"))?;
            // Its named points go with it (design 072 §4.4, F4): the lines on them end free.
            ops.extend(crate::wires::routes::free_points_ops(doc, cable.cores.iter().flat_map(|c| c.through.iter().copied())));
            ops.extend(remove_representations(doc, Subject::Cable(k)));
            ops.push(Op::DeleteCable { id: k, cable: cable.clone() });
            let kept = cable.cores.iter().filter(|m| m.position != ecad_model::CorePosition::Unplaced).count();
            let message = format!("Cable {} deleted; its {} kept.", k.hex(), kept_words(kept));
            Ok(Deleted { ops, released: 0, kept: Some(kept), message: Some(message) })
        }
    }
}

// ---------------------------------------------------------------- convertcomponent, addconnectionpoint

/// A component made of drawn geometry ([`plan_convert`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Converted {
    pub component: DeviceId,
    pub representation: RepresentationId,
    pub ops: Vec<Op>,
}

/// `convertcomponent` (experiment 004): plain geometry `items` on `page` become one new component,
/// drawn.
pub fn plan_convert(ids: &mut IdAllocator, page: PageId, items: Vec<ItemId>) -> Converted {
    let (component, representation): (DeviceId, RepresentationId) = (ids.fresh(), ids.fresh());
    let ops = vec![
        Op::CreateDevice { id: component, device: Device::default() },
        Op::LinkRepresentation {
            id: representation,
            rep: Representation {
                subject: Subject::Device(component),
                page,
                form: Form::Drawn,
                role: ecad_model::Role::Drawn,
                items,
                connection_points: BTreeMap::new(),
            },
        },
    ];
    Converted { component, representation, ops }
}

/// The refusal of an item that belongs to no component.
pub const NOT_A_COMPONENTS_ITEM: &str = "that item is not part of a component";

/// `addconnectionpoint` (experiment 008): a connection point labelled `label` at `at` on the drawn
/// component that `item` is part of.
///
/// # Errors
/// An item of no component; a placed symbol (its connection points come from its definition).
pub fn plan_addconnectionpoint(
    doc: &Document, ids: &mut IdAllocator, item: ItemId, at: Pt, label: Option<String>,
) -> Result<(ConnectionPointId, Vec<Op>), CmdError> {
    let (rid, r) = doc.representation_of_item(item).ok_or(CmdError::refused(NOT_A_COMPONENTS_ITEM))?;
    let Subject::Device(c) = r.subject else { return Err(CmdError::refused(NOT_A_COMPONENTS_ITEM)) };
    if r.form != Form::Drawn {
        return Err(CmdError::refused("connection points of placed symbols come from their definition"));
    }
    let t: ConnectionPointId = ids.fresh();
    let mut after = r.clone();
    after.connection_points.insert(t, at);
    let ops = vec![
        Op::AddConnectionPoint { id: t, connection_point: ConnectionPoint { owner: c.into(), label, key: None, accepts: Default::default() } },
        Op::ModifyRepresentation { id: rid, before: r.clone(), after },
    ];
    Ok((t, ops))
}

/// Every starter symbol's prefix as this project uses it (`-K`).
pub fn prefixes_json(doc: &ecad_model::Document) -> serde_json::Map<String, Value> {
    crate::symbols::catalogue::STARTERS
        .iter()
        // Junctions and continuations have no tag (brief 022).
        .filter(|s| s.kind != crate::symbols::Kind::Tagless)
        .map(|s| (s.key.to_string(), Value::String(format!("-{}", devices::prefix_for(doc, s)))))
        .collect()
}

// ---------------------------------------------------------------- the commands (generic)

/// `symbols {query?}` (design 018 §8): the starter symbols and devices matching `query`; read only.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SymbolsParams {
    /// What to look for (absent: everything).
    #[serde(default)]
    query: Option<String>,
}

impl Generic for SymbolsParams {
    const ID: &'static str = "symbols";
    const TITLE: &'static str = "Find Symbol";
    const HELP: &'static str = "{query?} (starter symbols and devices matching it; read only)";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        Ok(Outcome::answer(devices::json::symbols_json(cx.doc, self.query.as_deref())))
    }
}

/// `devices {tag?}` (design 018 §8): the devices whose tag starts with `tag`; read only.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DevicesParams {
    /// The start of the tags to list (absent: every device).
    #[serde(default)]
    tag: Option<String>,
}

impl Generic for DevicesParams {
    const ID: &'static str = "devices";
    const TITLE: &'static str = "List Devices";
    const HELP: &'static str = "{tag?} (devices whose tag starts with it; read only)";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        Ok(Outcome::answer(devices::json::devices_json(cx.doc, self.tag.as_deref())))
    }
}

/// `deviceprops {component}`: the device's properties (read only); its interactive form changes
/// them.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DevicepropsParams {
    /// The device.
    component: DeviceId,
}

impl Generic for DevicepropsParams {
    const ID: &'static str = "deviceprops";
    const TITLE: &'static str = "Device Properties";
    const HELP: &'static str = "{component: id} (a device's properties; read only; interactive: change them)";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        if !cx.doc.devices.contains_key(&self.component) {
            return Err(CmdError::NotFound(Noun::Device));
        }
        Ok(Outcome::answer(devices::json::device_json(cx.doc, self.component)))
    }
}

/// `ecaddelete {conductor} | {component} | {cable}`: the object with its drawings, in one
/// transaction (a cable's wires stay).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EcaddeleteParams {
    /// A wire.
    #[serde(default)]
    conductor: Option<WireId>,
    /// A device (or any component).
    #[serde(default)]
    component: Option<DeviceId>,
    /// A cable.
    #[serde(default)]
    cable: Option<CableId>,
}

/// Delete `doomed` in one transaction and answer as `ecaddelete` does: the answer's message is
/// also echoed when the delete released something; `answer` may replace the JSON.
fn delete(doc: &Document, doomed: Doomed, answer: impl FnOnce(Value) -> Value) -> Result<Outcome, CmdError> {
    let done = plan_delete(doc, doomed)?;
    let (json, echo) = match (done.kept, done.message) {
        (Some(kept), Some(message)) => (json!({"deleted": true, "kept": kept, "message": message}), None),
        (None, Some(message)) => (json!({"deleted": true, "released": done.released, "message": message}), Some(message)),
        _ => (json!({"deleted": true}), None),
    };
    Ok(Outcome::edit("Delete", done.ops, answer(json)).echo(echo))
}

impl Generic for EcaddeleteParams {
    const ID: &'static str = "ecaddelete";
    const TITLE: &'static str = "Delete Electrical Object";
    const HELP: &'static str = "{conductor: id} | {component: id} | {cable: id}";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let doomed = match (self.conductor, self.component, self.cable) {
            (Some(c), _, _) => Doomed::Wire(c),
            (None, Some(c), _) => Doomed::Device(c),
            (None, None, Some(k)) => Doomed::Cable(k),
            (None, None, None) => return Err(CmdError::refused("`conductor`, `component` or `cable` (hex id) is required")),
        };
        delete(cx.doc, doomed, |v| v)
    }
}

/// `devicedelete {component}`: the device and every symbol of it, as `ecaddelete` (design 018
/// §6.3).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DevicedeleteParams {
    /// The device.
    component: DeviceId,
}

impl Generic for DevicedeleteParams {
    const ID: &'static str = "devicedelete";
    const TITLE: &'static str = "Delete Device";
    const HELP: &'static str = "{component: id} (deletes the device with every symbol, as `ecaddelete`)";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let tag = devices::tags::tag_words(cx.doc, self.component);
        delete(
            cx.doc,
            Doomed::Device(self.component),
            move |r| json!({"deleted": r.get("deleted").cloned().unwrap_or(Value::Bool(true)), "message": format!("Deleted {tag}.")}),
        )
    }
}

/// `tagsettings {padding?, prefixes?}` (brief 018, owner decision 2026-11-08): how many digits
/// automatic tag numbers have, and the project's own prefix per starter symbol (`{"coil": "KA"}`;
/// `null` goes back to the built-in one). One `SetProjectSettings` transaction; with nothing to
/// change, or no parameters, it records nothing and reports the settings.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TagsettingsParams {
    /// Digits of automatic tag numbers, 1 to 6.
    #[serde(default)]
    padding: Option<u64>,
    /// Per starter symbol, the project's prefix (null: the built-in one).
    #[serde(default)]
    prefixes: Option<BTreeMap<String, Option<String>>>,
}

impl Generic for TagsettingsParams {
    const ID: &'static str = "tagsettings";
    const TITLE: &'static str = "Tag Settings";
    const HELP: &'static str = "{padding?: 1..6, prefixes?: {symbol: prefix|null}} (automatic tags: digits, and this project's prefix per starter symbol; no parameters lists them)";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let settings = tag_settings(self.padding, self.prefixes.unwrap_or_default())?;
        let before = cx.doc.settings.clone();
        let mut after = before.clone();
        apply_tag_settings(&mut after, settings);
        let changed = after != before;
        let ops = if changed { vec![Op::SetProjectSettings { before, after }] } else { Vec::new() };
        Ok(Outcome::edit_then(
            "Tag settings",
            ops,
            move |doc| json!({"changed": changed, "padding": devices::padding(doc), "prefixes": prefixes_json(doc)}),
        ))
    }
}
