//! The JSON forms of brief 018's commands (design 018 §8): `place` (every form), `symbols` and
//! `devices` (read only), `deviceprops`, `devicedelete`, `ecaddelete` and `tagsettings`; and the
//! experiments' `convertcomponent` and `addconnectionpoint`.
//!
//! The interactive forms ([`crate::interact`]) commit through exactly these. `place` is a thin shim
//! (brief 070): [`PlaceParams`] reads the JSON, the rules in [`ecad_electrical::devices::commands::place`] say
//! which device a symbol joins, each form's planner builds the ops, and the shim commits and
//! answers in JSON.

use ecad_model::geometry::units::{Degrees, Radians};
use std::collections::BTreeMap;

use cadcraft_engine::doc::Handle;
use cadcraft_engine::{EngineError, Session};
use ecad_electrical::CmdError;
use ecad_electrical::devices::commands::devices::{self as device_rules, Doomed};
use ecad_electrical::devices::commands::place as rules;
use ecad_electrical::symbols::proposals::Mode;
use ecad_model::{
    CableId, Color, ConnectionPoint, ConnectionPointId, DefinitionId, Device, DeviceId, Form, Item, ItemId, ItemKind, Noun, Op, Placement,
    Representation, RepresentationId, Style, Subject, SymbolInstance, WireId,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cables::views;
use crate::commands::input::{Command, Typed};
use crate::commands::{MAX_HANDLES, on_any_page, on_project, plain_style};
use crate::project::{Mapped, Project};
use ecad_electrical::commands::input::{Bounded, Point, TextOrNumber, nullable};

fn err(m: impl Into<String>) -> EngineError {
    EngineError::Other(m.into())
}

fn style(p: &Project) -> Style {
    Style { layer: p.default_layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 }
}

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[
    Typed::of::<PlaceParams>("place"),
    Typed::of::<SymbolsParams>("symbols"),
    Typed::of::<DevicesParams>("devices"),
    Typed::of::<DevicepropsParams>("deviceprops"),
    Typed::of::<EcaddeleteParams>("ecaddelete"),
    Typed::of::<DevicedeleteParams>("devicedelete"),
    Typed::of::<TagsettingsParams>("tagsettings"),
    Typed::of::<ConvertcomponentParams>("convertcomponent"),
    Typed::of::<AddConnectionPointParams>("addconnectionpoint"),
];

/// `place {at, rotation?, mirror?, symbol?, definition?, device?, tag?, …}` (design 018 §8): a
/// starter symbol as a new device or another symbol of one; a junction or continuation (brief
/// 022), a terminal level (036), a connector half or pin (044), a PLC channel or card (045), a
/// project symbol (048); without `symbol`, `device` and `tag`, the experiment's coil or
/// `definition`. The function and location proposals (brief 052d) follow `function` and
/// `location`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlaceParams {
    /// Where the symbol goes, [x, y].
    pub(crate) at: Point,
    /// The rotation in degrees.
    #[serde(default)]
    pub(crate) rotation: Option<Degrees>,
    /// Mirrored.
    #[serde(default)]
    pub(crate) mirror: Option<bool>,
    /// A starter symbol's key (see `symbols`), or a project symbol's.
    #[serde(default)]
    pub(crate) symbol: Option<String>,
    /// Without `symbol`: the definition to place (default the experiment's coil).
    #[serde(default)]
    pub(crate) definition: Option<DefinitionId>,
    /// The device to add the symbol to, as a hex id (a connector: its id or tag).
    #[serde(default)]
    pub(crate) device: Option<String>,
    /// A new device's tag, or the device that has that tag.
    #[serde(default)]
    pub(crate) tag: Option<String>,
    /// A continuation's name: the potential or signal it carries.
    #[serde(default)]
    pub(crate) name: Option<String>,
    /// A continuation's caption.
    #[serde(default)]
    pub(crate) caption: Option<String>,
    /// A terminal level: the strip position it goes at.
    #[serde(default)]
    pub(crate) position: Option<String>,
    /// A connector pin: its name (default the next free pin).
    #[serde(default)]
    pub(crate) pin: Option<String>,
    /// A PLC channel: its number (default the next free channel).
    #[serde(default)]
    pub(crate) channel: Option<TextOrNumber>,
    /// A PLC card: its rack, an id or a tag (null: no rack; absent: the last card's, else the only
    /// one).
    #[serde(default, deserialize_with = "nullable")]
    pub(crate) rack: Option<Option<String>>,
    /// A PLC card: its slot (default the next free one).
    #[serde(default)]
    pub(crate) slot: Option<TextOrNumber>,
    /// The function proposal: propose (the default), move, keep, or a code.
    #[serde(default)]
    pub(crate) function: Option<String>,
    /// The location proposal: propose (the default), move, keep, half, or a code.
    #[serde(default)]
    pub(crate) location: Option<String>,
}

impl PlaceParams {
    /// The proposal modes `function` and `location` name (brief 052d).
    ///
    /// # Errors
    /// An empty word.
    pub(crate) fn modes(&self) -> Result<[Mode; 2], EngineError> {
        Mode::both(self.function.as_deref(), self.location.as_deref()).map_err(|e| err(e.to_string()))
    }

    /// `device` read as a hex ID.
    ///
    /// # Errors
    /// Text that is not one.
    pub(crate) fn device_id(&self) -> Result<Option<DeviceId>, EngineError> {
        rules::device_id(self.device.as_deref()).map_err(|e| err(e.to_string()))
    }
}

impl Command for PlaceParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        // Brief 018: a starter symbol, as a new device or another symbol of one.
        if self.symbol.is_some() || self.device.is_some() || self.tag.is_some() {
            return place_symbol(s, self);
        }
        place_definition(s, self)
    }
}

/// `place {at, rotation?, mirror?, symbol, device?, tag?}` (design 018 §8).
fn place_symbol(s: &mut Session, params: PlaceParams) -> cadcraft_engine::Result<Value> {
    if params.definition.is_some() && params.symbol.is_some() {
        return Err(err("`symbol` and `definition` are mutually exclusive"));
    }
    let key = params.symbol.clone().ok_or_else(|| err("`symbol` (a starter symbol key, see `symbols`) is required with `device` or `tag`"))?;
    // Brief 048: a project symbol's key places its newest version.
    let starter = match ecad_electrical::devices::starter(&key) {
        Ok(st) => st,
        Err(e) => return crate::symbols::commands::place_project_key(s, &params, &key)?.ok_or_else(|| err(e)),
    };
    let at = params.at.0;
    if !(at.is_finite() && at.x.abs() <= crate::interact::MAX_COORD && at.y.abs() <= crate::interact::MAX_COORD) {
        return Err(err(crate::interact::OUT_OF_RANGE));
    }
    let degrees = params.rotation.unwrap_or_default();
    let mirror = params.mirror.unwrap_or(false);
    // Brief 022: a junction or continuation is a tagless component of its own.
    if starter.kind == ecad_electrical::symbols::Kind::Tagless {
        return crate::wires::commands::place_tagless(s, &params, starter, at, degrees, mirror);
    }
    // Brief 036: a level, a whole terminal, a disconnect or fuse terminal joins a strip as a position.
    if ecad_electrical::terminal_strips::terminal_blocks::is_level_starter(starter.key) {
        return crate::terminal_strips::commands::place_level(s, &params, starter, at, degrees, mirror);
    }
    // Brief 045 (design 045 §4.4): a channel symbol joins a card as a channel.
    if ecad_electrical::plc::channels::cell_type(starter).is_some() {
        return crate::plc::commands::channels::place_channel(s, &params, starter, at, degrees, mirror);
    }
    // Brief 044: a connector half or pin makes or joins a connector (design 044 §3.3).
    if let ecad_electrical::symbols::Kind::ConnectorHalf { .. } = starter.kind {
        return crate::connectors::commands::place_half(s, &params, starter, at, degrees, mirror);
    }
    let device = params.device_id()?;
    rules::one_of(device.is_some(), params.tag.is_some(), "device").map_err(|e| err(e.to_string()))?;
    let modes = params.modes()?;
    let tag = params.tag.clone();
    let rack = params.rack.clone();
    let slot = params.slot.as_ref().map(TextOrNumber::text);
    on_project(s, |p, pg, page| {
        let doc = p.ledger.document();
        let target = rules::device_target(doc, device, tag.as_deref())?;
        // Brief 028 (design 028 §3.1): a terminal clicked on a wire lands on it, splitting it.
        let landing = (starter.kind == ecad_electrical::symbols::Kind::TerminalBlock)
            .then(|| crate::terminal_strips::commands::landing_point(doc, page, at))
            .flatten();
        let origin = landing.map_or(at, |on| on.at);
        let placement = Placement { origin, rotation: Radians::from(degrees), mirror, scale: 1.0 };
        let style = style(p);
        // Brief 025 (design 025 §4.3): a joined contact takes its part's next free offered labels.
        let offer_slot = match (&target, starter.kind) {
            (ecad_electrical::devices::Target::Join(c), ecad_electrical::symbols::Kind::Contact { make }) => {
                ecad_electrical::references::devices::offer_labels(doc, *c, make, &ecad_electrical::devices::labels_of(doc, *c)).is_some()
            }
            _ => false,
        };
        let mut plan = ecad_electrical::devices::plan_place(&p.ledger, &mut p.ids, &mut p.tag_high, page, style, starter, placement, target)?;
        let land = match (landing, plan.connection_points.first()) {
            (Some(on), Some((_, _, t))) => {
                let land = ecad_electrical::connectors::plan_land(p.ledger.document(), &mut p.ids, on, *t)?;
                plan.ops.extend(land.ops.clone());
                Some((*t, land))
            }
            _ => None,
        };
        // Brief 045 (design 045 §2.4): a new card goes in a rack's slot when the project has a rack.
        let card_slot = match ecad_electrical::plc::rack::CardType::of_symbol(starter) {
            Some(t) if !t.is_rack() && !plan.joined => {
                let last = p.last_placed.map(|l| l.device);
                let at = ecad_electrical::plc::rack::place_target(p.ledger.document(), rack.as_ref().map(Option::as_deref), slot.as_deref(), last)?;
                if let Some((r, s)) = at {
                    ecad_electrical::plc::rack::put_new_card(p.ledger.document(), &mut plan.ops, plan.component, r, s);
                }
                at.map(|x| (t, x))
            }
            _ => None,
        };
        // Brief 052d (design 052 §5.3): the context here proposes a function and a location; an
        // unset device takes it in this transaction, a device with another value is asked about.
        let proposed = crate::symbols::draw::plan_place(p, &plan.ops, modes, plan.component, page, origin, None).map_err(CmdError::refused)?;
        plan.ops.extend(proposed.ops.iter().cloned());
        let label = if land.is_some() { "Place terminal".to_string() } else { plan.label.clone() };
        p.apply_edit(pg, "place", &label, plan.ops.clone())?;
        p.last_placed = Some(ecad_electrical::commands::run::LastPlaced { starter: Some(starter.key), device: plan.component });
        let doc = p.ledger.document();
        let landed = land.map(|(t, l)| crate::terminal_strips::commands::land_result(doc, t, &l));
        if let (true, ecad_electrical::symbols::Kind::Contact { make }) = (plan.joined, starter.kind) {
            let labels: Vec<String> = plan.connection_points.iter().map(|(_, l, _)| l.clone()).collect();
            if let Some(line) = ecad_electrical::references::devices::over_line(doc, plan.component, &labels, make) {
                pg.echo(line);
            }
        }
        let mut v = json!({
            "component": plan.component.hex(),
            "representation": plan.representation.hex(),
            "item": plan.item.hex(),
            "definition": plan.definition.hex(),
            "symbol": starter.key,
            "tag": plan.tag,
            "joined": plan.joined,
            "offer_slot": offer_slot,
            "pages": ecad_electrical::devices::page_numbers(doc, plan.component),
            "connection_points": plan.connection_points.iter().map(|(k, l, t)| json!({"key": k, "label": l, "connection_point": t.hex()})).collect::<Vec<_>>(),
        });
        crate::symbols::draw::finish_place(p, pg, plan.component, plan.joined, &proposed, &mut v);
        let doc = p.ledger.document();
        if let Some((t, (r, slot))) = card_slot {
            let message = format!("Placed {} ({}) in {} slot {slot}.", plan.tag, t.noun(), ecad_electrical::devices::tag_words(doc, r));
            v["rack"] = json!(r.hex());
            v["slot"] = json!(slot);
            v["message"] = json!(message);
            pg.echo(message);
        }
        if let Some((fields, message)) = landed {
            if let (Some(o), Some(f)) = (v.as_object_mut(), fields.as_object()) {
                o.extend(f.clone());
            }
            v["message"] = json!(message);
            pg.echo(message);
        }
        Ok(v)
    })
}

/// `place {at, rotation?, mirror?, definition?}` without a starter symbol (the experiment): the
/// definition, or the built-in coil.
fn place_definition(s: &mut Session, params: PlaceParams) -> cadcraft_engine::Result<Value> {
    let at = params.at.0;
    let degrees = params.rotation.unwrap_or_default();
    let mirror = params.mirror.unwrap_or(false);
    let wanted = params.definition;
    on_project(s, |p, pg, page| {
        let doc = p.ledger.document();
        let coil = crate::commands::builtin_coil();
        let mut ops = Vec::new();
        let (def_id, def) = match wanted {
            Some(d) => (d, doc.definitions.get(&d).cloned().ok_or(CmdError::refused("no such definition"))?),
            None => match doc.definitions.iter().find(|(_, d)| d.name == coil.name) {
                Some((id, d)) => (*id, d.clone()),
                None => {
                    let id: DefinitionId = p.ids.fresh();
                    ops.push(Op::EmbedDefinition { id, definition: coil.clone() });
                    (id, coil)
                }
            },
        };
        let (c, item, r): (DeviceId, ItemId, RepresentationId) = (p.ids.fresh(), p.ids.fresh(), p.ids.fresh());
        ops.push(Op::CreateDevice { id: c, device: Device { definition: Some(def_id), ..Device::default() } });
        let mut connection_points = Vec::new();
        for td in &def.connection_points {
            let t: ConnectionPointId = p.ids.fresh();
            ops.push(Op::AddConnectionPoint {
                id: t,
                connection_point: ConnectionPoint { owner: c.into(), label: td.label.clone(), key: Some(td.key.clone()), accepts: td.accepts },
            });
            connection_points.push(json!({"key": td.key, "connection_point": t.hex()}));
        }
        let placement = Placement { origin: at, rotation: Radians::from(degrees), mirror, scale: 1.0 };
        ops.push(Op::AddItem {
            id: item,
            item: Item { page, style: plain_style(p), kind: ItemKind::Symbol(SymbolInstance { definition: def_id, placement }) },
        });
        ops.push(Op::LinkRepresentation {
            id: r,
            rep: Representation {
                subject: Subject::Device(c),
                page,
                form: Form::Symbol,
                role: ecad_model::Role::Part(def.part_role),
                items: vec![item],
                connection_points: BTreeMap::new(),
            },
        });
        p.apply_edit(pg, "place", "Place symbol", ops)?;
        Ok(json!({"component": c.hex(), "item": item.hex(), "definition": def_id.hex(), "connection_points": connection_points}))
    })
}

/// `symbols {query?}` (design 018 §8): the starter symbols and devices matching `query`; read only.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SymbolsParams {
    /// What to look for (absent: everything).
    #[serde(default)]
    query: Option<String>,
}

impl Command for SymbolsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_any_page(s, |p, _| Ok(ecad_electrical::devices::symbols_json(p.ledger.document(), self.query.as_deref())))
    }
}

/// `devices {tag?}` (design 018 §8): the devices whose tag starts with `tag`; read only.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DevicesParams {
    /// The start of the tags to list (absent: every device).
    #[serde(default)]
    tag: Option<String>,
}

impl Command for DevicesParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_any_page(s, |p, _| Ok(ecad_electrical::devices::devices_json(p.ledger.document(), self.tag.as_deref())))
    }
}

/// `deviceprops {component}`: the device's properties (read only); its interactive form changes
/// them.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DevicepropsParams {
    /// The device.
    component: DeviceId,
}

impl Command for DevicepropsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_any_page(s, |p, _| {
            let doc = p.ledger.document();
            if !doc.devices.contains_key(&self.component) {
                return Err(CmdError::NotFound(Noun::Device));
            }
            Ok(ecad_electrical::devices::device_json(doc, self.component))
        })
    }
}

/// `ecaddelete {conductor} | {component} | {cable}`: the object with its drawings, in one
/// transaction (a cable's wires stay).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct EcaddeleteParams {
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

impl Command for EcaddeleteParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let doomed = match (self.conductor, self.component, self.cable) {
            (Some(c), _, _) => Doomed::Wire(c),
            (None, Some(c), _) => Doomed::Device(c),
            (None, None, Some(k)) => Doomed::Cable(k),
            (None, None, None) => return Err(err("`conductor`, `component` or `cable` (hex id) is required")),
        };
        delete(s, doomed)
    }
}

/// Delete `doomed` in one transaction and answer as `ecaddelete` does.
fn delete(s: &mut Session, doomed: Doomed) -> cadcraft_engine::Result<Value> {
    on_project(s, |p, pg, _| {
        let done = device_rules::plan_delete(p.ledger.document(), doomed)?;
        p.apply_edit(pg, "ecaddelete", "Delete", done.ops)?;
        Ok(match (done.kept, done.message) {
            (Some(kept), Some(message)) => json!({"deleted": true, "kept": kept, "message": message}),
            (None, Some(message)) => {
                pg.echo(message.clone());
                json!({"deleted": true, "released": done.released, "message": message})
            }
            _ => json!({"deleted": true}),
        })
    })
}

/// `devicedelete {component}`: the device and every symbol of it, as `ecaddelete` (design 018
/// §6.3).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DevicedeleteParams {
    /// The device.
    component: DeviceId,
}

impl Command for DevicedeleteParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let tag = on_any_page(s, |p, _| Ok(ecad_electrical::devices::tag_words(p.ledger.document(), self.component)))?;
        let r = delete(s, Doomed::Device(self.component))?;
        Ok(json!({"deleted": r.get("deleted").cloned().unwrap_or(Value::Bool(true)), "message": format!("Deleted {tag}.")}))
    }
}

/// `tagsettings {padding?, prefixes?}` (brief 018, owner decision 2026-11-08): how many digits
/// automatic tag numbers have, and the project's own prefix per starter symbol (`{"coil": "KA"}`;
/// `null` goes back to the built-in one). One `SetProjectSettings` transaction; with nothing to
/// change, or no parameters, it records nothing and reports the settings.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TagsettingsParams {
    /// Digits of automatic tag numbers, 1 to 6.
    #[serde(default)]
    padding: Option<u64>,
    /// Per starter symbol, the project's prefix (null: the built-in one).
    #[serde(default)]
    prefixes: Option<BTreeMap<String, Option<String>>>,
}

impl Command for TagsettingsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let settings = device_rules::tag_settings(self.padding, self.prefixes.unwrap_or_default()).map_err(|e| err(e.to_string()))?;
        on_project(s, |p, pg, _| {
            let before = p.ledger.document().settings.clone();
            let mut after = before.clone();
            device_rules::apply_tag_settings(&mut after, settings);
            let changed = after != before;
            if changed {
                p.apply_edit(pg, "tagsettings", "Tag settings", vec![Op::SetProjectSettings { before, after: after.clone() }])?;
            }
            let doc = p.ledger.document();
            Ok(json!({"changed": changed, "padding": ecad_electrical::devices::padding(doc), "prefixes": device_rules::prefixes_json(doc)}))
        })
    }
}

// ---------------------------------------------------------------- drawn components (experiments)

/// `convertcomponent {handles?}` (experiment 004): plain geometry (default: the selection) becomes
/// one new component, drawn.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConvertcomponentParams {
    /// The geometry, as hex handles (default: the selection).
    #[serde(default)]
    handles: Option<Bounded<String, 0, MAX_HANDLES>>,
}

impl Command for ConvertcomponentParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        // Every entry must be a valid hex handle: a typo is an error, never a smaller set.
        let hs: Vec<Handle> = match self.handles {
            None => s.selection(),
            Some(b) => {
                b.0.iter()
                    .map(|h| Handle::parse_hex(h).ok_or_else(|| err(format!("`{}` is not a valid handle", json!(h)))))
                    .collect::<Result<_, _>>()?
            }
        };
        on_project(s, |p, pg, page| {
            let map = &p.pages.get(&page).ok_or(CmdError::refused(PAGE_VIEW_NOT_FOUND))?.map;
            let mut items = Vec::new();
            // Computed parts of a traditional-view cable are left out of a selection with other
            // objects (brief 009 review S2); on their own they are refused below.
            let mut drawn = None;
            for h in hs {
                match map.get(h) {
                    Some(Mapped::Item(i)) if p.ledger.document().representation_of_item(i).is_none() => items.push(i),
                    Some(Mapped::Fan(k)) if !map.is_fan(k) => drawn = Some(k),
                    Some(Mapped::Fan(k)) => return Err(CmdError::refused(views::refusal(map, k))),
                    Some(_) => return Err(CmdError::refused("only plain geometry can be converted; part of the selection already has a meaning")),
                    None => return Err(CmdError::refused("the selection holds an object this page does not know")),
                }
            }
            if items.is_empty() {
                return Err(match drawn {
                    Some(k) => CmdError::refused(views::refusal(map, k)),
                    None => CmdError::refused("select the geometry to convert"),
                });
            }
            let done = device_rules::plan_convert(&mut p.ids, page, items);
            p.apply_edit(pg, "convertcomponent", "Convert to component", done.ops)?;
            Ok(json!({"component": done.component.hex(), "representation": done.representation.hex()}))
        })
    }
}

/// `addconnectionpoint {handle, at, label?}` (experiment 008): a connection point on a drawn component.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AddConnectionPointParams {
    /// An item of the component, as a hex handle.
    handle: String,
    /// Where the connection point is, [x, y].
    at: Point,
    /// Its label.
    #[serde(default)]
    label: Option<String>,
}

impl Command for AddConnectionPointParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let h = Handle::parse_hex(&self.handle).ok_or_else(|| err(format!("`{}` is not a valid handle", json!(self.handle))))?;
        let (at, label) = (self.at.0, self.label);
        on_project(s, |p, pg, page| {
            let map = &p.pages.get(&page).ok_or(CmdError::refused(PAGE_VIEW_NOT_FOUND))?.map;
            let item = match map.get(h) {
                Some(Mapped::Item(item)) => item,
                Some(Mapped::Fan(k)) => return Err(CmdError::refused(views::refusal(map, k))),
                _ => return Err(CmdError::refused("pick an item of a component")),
            };
            let (t, ops) = device_rules::plan_addconnectionpoint(p.ledger.document(), &mut p.ids, item, at, label)?;
            p.apply_edit(pg, "addconnectionpoint", "Add connection point", ops)?;
            Ok(json!({"connection_point": t.hex()}))
        })
    }
}

/// The refusal of a page with no view (a project in a state it should not be in).
const PAGE_VIEW_NOT_FOUND: &str = "page view not found";
