//! The JSON forms of the commands `place` (every form), `symbols` and
//! `devices` (read only), `deviceprops`, `devicedelete`, `ecaddelete` and `tagsettings`; and
//! `convertcomponent` and `addconnectionpoint`, which make a device from plain drawing.
//!
//! The interactive forms ([`crate::devices::interact`]) commit through exactly these. `place` is a thin shim:
//! [`PlaceParams`] reads the JSON, the rules in [`ecad_electrical::devices::commands::place`] say
//! which device a symbol joins, each form's planner builds the ops, and the shim commits and
//! answers in JSON.

use ecad_model::geometry::units::{Degrees, Radians};
use std::collections::BTreeMap;

use cadcraft_engine::doc::Handle;
use cadcraft_engine::{EngineError, Session};
use ecad_electrical::CmdError;
use ecad_electrical::devices::commands::devices as device_rules;
use ecad_electrical::devices::commands::place as rules;
use ecad_electrical::symbols::proposals::Mode;
use ecad_model::{
    Color, ConnectionPoint, ConnectionPointId, DefinitionId, Device, DeviceId, Form, Item, ItemId, ItemKind, Op, Placement, Representation,
    RepresentationId, Style, Subject, SymbolInstance,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cables::views;
use crate::commands::input::{Command, Typed};
use crate::commands::{MAX_HANDLES, on_project, plain_style};
use crate::project::{Mapped, Project};
use ecad_electrical::commands::input::{Bounded, Point, TextOrNumber, nullable};

fn err(m: impl Into<String>) -> EngineError {
    EngineError::Other(m.into())
}

fn style(p: &Project) -> Style {
    Style { layer: p.default_layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 }
}

/// The commands of this module with a typed input.
pub(crate) const TYPED: &[Typed] = &[
    Typed::of::<PlaceParams>("place"),
    Typed::of::<ConvertcomponentParams>("convertcomponent"),
    Typed::of::<AddConnectionPointParams>("addconnectionpoint"),
];

/// Places a starter symbol (`symbol`, see `symbols`) as a new device or as another symbol of one (`device`; `tag`
/// names a new device or adds to the device that has it); also a continuation, a terminal level, a
/// connector half or pin, a PLC channel or card, or a project symbol. Without `symbol`, `device` and `tag`, it
/// places `definition`. `function` and `location` say what happens to the proposals.
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
    /// Without `symbol`: the definition to place (default: the built-in two-point coil, A1 and A2).
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
    /// The proposal modes `function` and `location` name.
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
        // A starter symbol, as a new device or another symbol of one.
        if self.symbol.is_some() || self.device.is_some() || self.tag.is_some() {
            return place_symbol(s, self);
        }
        place_definition(s, self)
    }
}

/// `place {at, rotation?, mirror?, symbol, device?, tag?}`.
fn place_symbol(s: &mut Session, params: PlaceParams) -> cadcraft_engine::Result<Value> {
    if params.definition.is_some() && params.symbol.is_some() {
        return Err(err("`symbol` and `definition` are mutually exclusive"));
    }
    let key = params.symbol.clone().ok_or_else(|| err("`symbol` (a starter symbol key, see `symbols`) is required with `device` or `tag`"))?;
    // A project symbol's key places its newest version.
    let starter = match ecad_electrical::devices::json::starter(&key) {
        Ok(st) => st,
        Err(e) => return crate::symbols::commands::place_project_key(s, &params, &key)?.ok_or_else(|| err(e)),
    };
    let at = params.at.0;
    if !(at.is_finite() && at.x.abs() <= ecad_electrical::commands::input::MAX_COORD && at.y.abs() <= ecad_electrical::commands::input::MAX_COORD) {
        return Err(err(ecad_electrical::commands::input::OUT_OF_RANGE));
    }
    let degrees = params.rotation.unwrap_or_default();
    let mirror = params.mirror.unwrap_or(false);
    // A continuation is a tagless component of its own.
    if starter.kind == ecad_electrical::symbols::Kind::Tagless {
        return crate::wires::commands::place_tagless(s, &params, starter, at, degrees, mirror);
    }
    // A level, a whole terminal, a disconnect or fuse terminal joins a strip as a position.
    if ecad_electrical::terminal_strips::terminal_blocks::is_level_starter(starter.key) {
        return crate::terminal_strips::commands::place_level(s, &params, starter, at, degrees, mirror);
    }
    // A channel symbol joins a card as a channel.
    if ecad_electrical::plc::channels::cell_type(starter).is_some() {
        return crate::plc::commands::channels::place_channel(s, &params, starter, at, degrees, mirror);
    }
    // A connector half or pin makes or joins a connector.
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
        // A terminal clicked on a wire lands on it, splitting it.
        let landing = (starter.kind == ecad_electrical::symbols::Kind::TerminalBlock)
            .then(|| ecad_electrical::wires::placing::landing_point(doc, page, at))
            .flatten();
        let origin = landing.map_or(at, |on| on.at);
        let placement = Placement { origin, rotation: Radians::from(degrees), mirror, scale: 1.0 };
        let style = style(p);
        // A joined contact takes its part's next free offered labels.
        let offer_slot = match (&target, starter.kind) {
            (ecad_electrical::devices::placing::Target::Join(c), ecad_electrical::symbols::Kind::Contact { make }) => {
                ecad_electrical::references::devices::table::offer_labels(doc, *c, make, &ecad_electrical::devices::placing::labels_of(doc, *c))
                    .is_some()
            }
            _ => false,
        };
        let mut plan =
            ecad_electrical::devices::placing::plan_place(&p.ledger, &mut p.ids, &mut p.indexes.tag_high, page, style, starter, placement, target)?;
        let land = match (landing, plan.connection_points.first()) {
            (Some(on), Some((_, _, t))) => {
                let land = ecad_electrical::connectors::plan_land(p.ledger.document(), &mut p.ids, on, *t)?;
                plan.ops.extend(land.ops.clone());
                Some((*t, land))
            }
            _ => None,
        };
        // A new card goes in a rack's slot when the project has a rack.
        let card_slot = match ecad_electrical::plc::rack::CardType::of_symbol(starter) {
            Some(t) if !t.is_rack() && !plan.joined => {
                let last = p.defaults.last_placed.map(|l| l.device);
                let at = ecad_electrical::plc::rack::place_target(p.ledger.document(), rack.as_ref().map(Option::as_deref), slot.as_deref(), last)?;
                if let Some((r, s)) = at {
                    ecad_electrical::plc::rack::put_new_card(p.ledger.document(), &mut plan.ops, plan.component, r, s);
                }
                at.map(|x| (t, x))
            }
            _ => None,
        };
        // The context here proposes a function and a location; an
        // unset device takes it in this transaction, a device with another value is asked about.
        let proposed = crate::symbols::draw::plan_place(p, &plan.ops, modes, plan.component, page, origin, None).map_err(CmdError::refused)?;
        plan.ops.extend(proposed.ops.iter().cloned());
        let label = if land.is_some() { "Place terminal block".to_string() } else { plan.label.clone() };
        p.apply_edit(pg, "place", &label, plan.ops.clone())?;
        p.defaults.last_placed = Some(ecad_electrical::commands::run::LastPlaced { starter: Some(starter.key), device: plan.component });
        let doc = p.ledger.document();
        let landed = land.map(|(t, l)| ecad_electrical::connectors::land_result(doc, t, &l));
        if let (true, ecad_electrical::symbols::Kind::Contact { make }) = (plan.joined, starter.kind) {
            let labels: Vec<String> = plan.connection_points.iter().map(|(_, l, _)| l.clone()).collect();
            if let Some(line) = ecad_electrical::references::devices::table::over_line(doc, plan.component, &labels, make) {
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
            "pages": ecad_electrical::devices::tags::page_numbers(doc, plan.component),
            "connection_points": plan.connection_points.iter().map(|(k, l, t)| json!({"key": k, "label": l, "connection_point": t.hex()})).collect::<Vec<_>>(),
        });
        crate::symbols::draw::finish_place(p, pg, plan.component, plan.joined, &proposed, &mut v);
        let doc = p.ledger.document();
        if let Some((t, (r, slot))) = card_slot {
            let message = format!("Placed {} ({}) in {} slot {slot}.", plan.tag, t.noun(), ecad_electrical::devices::tags::tag_words(doc, r));
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

/// `place {at, rotation?, mirror?, definition?}` without a starter symbol: the definition, or the
/// built-in coil.
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
            Some(d) => (d, doc.definitions().get(&d).cloned().ok_or(CmdError::refused("no such definition"))?),
            None => match doc.definitions().iter().find(|(_, d)| d.name == coil.name) {
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
                connection_point: ConnectionPoint {
                    owner: c.into(),
                    label: td.label.clone(),
                    key: Some(td.key.clone()),
                    accepts: td.accepts,
                    protective_earth: ecad_electrical::symbols::seeds_pe(&def, &td.key),
                },
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

// ---------------------------------------------------------------- devices made from plain drawing

/// Plain geometry (default: the selection) becomes one new component, drawn.
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
            // objects; on their own they are refused below.
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

/// A connection point on a drawn component.
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
