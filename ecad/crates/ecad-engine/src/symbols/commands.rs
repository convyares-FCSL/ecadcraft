//! The commands of brief 048, the symbol builder (design 048 §9): `symbolnew`, `symboledit`,
//! `symbolimpact` (read only), `symbolupdate` and `makesymbol`, and `place` of a project symbol.
//!
//! Every write is one transaction, committed through [`Project::apply_edit`]; nothing here opens a
//! dialog. The builder (048b) saves through these JSON forms.
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::symbols::commands`] (content, impact, updates, promotion) decide, and the shim
//! finds the selection on the page, commits the ops and answers in JSON.
//!
//! Not here: the builder's drawing, drafts and the review drawer (048b); "update available" in
//! Properties and Devices (048c).

use ecad_model::geometry::units::{Degrees, Radians};
use std::collections::BTreeSet;

use cadcraft_engine::doc::Handle;
use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::symbols::commands::{self as rules, ContentInput, MakeAt, PointInput, SlotInput, Update};
pub use ecad_electrical::symbols::commands::{
    Content, MAX_COPIES, MAX_EXTENT, MAX_NAME, MAX_POINTS, MAX_SHAPES, MAX_SLOTS, Rules, grid_origin, not_plain, problems, shifted,
};
use ecad_model::{DefinitionId, DeviceId, PageId, Placement, RepresentationId, TextHAlign, TextVAlign};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::commands::input::{Command, Typed};
use crate::commands::{MAX_HANDLES, err, on_project, plain_style};
use crate::project::Mapped;
use ecad_electrical::commands::input::{Bounded, Point};
use ecad_electrical::symbols;
pub use ecad_electrical::symbols::POINT_GRID_MM;
pub use ecad_electrical::symbols::update_available;

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[
    Typed::of::<NewParams>("symbolnew"),
    Typed::of::<EditParams>("symboledit"),
    Typed::of::<ImpactParams>("symbolimpact"),
    Typed::of::<UpdateParams>("symbolupdate"),
    Typed::of::<MakeParams>("makesymbol"),
];

/// The commands this module adds (registered in `lanes.rs`).
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("symbolnew", "New Symbol", crate::commands::input::typed::<NewParams>)
            .params(
                "{name, key, graphics: [{kind: line|polyline|arc|circle|text|point, …} as in the file record], points: [{key, label?, at: [x,y]}], \
slots?: [{text: tag|description|rating|rating.<label>|part_number|maker|article_number|function|location, at: [x,y], h?: left|center|right, \
v?: baseline|bottom|middle|top, rotation?: degrees}], defaults?: {prefix, description?, rating?, join?: device|part, aliases?, family?}, \
bridges?: [[key…]], from?: definition id} (design 048 §9.1: embeds version 1 of a project symbol; one transaction)",
            )
            .noundo(),
        CommandSpec::new("symboledit", "Edit Symbol", crate::commands::input::typed::<EditParams>)
            .params(
                "{definition: id of the version edited, …content as symbolnew…, update: all|selected|none|new, copies?: [representation id], \
builtins?: bool} (design 048 §9.2: a new version, and the chosen placed copies updated, in one transaction)",
            )
            .noundo(),
        CommandSpec::new("symbolimpact", "Symbol Impact", crate::commands::input::typed::<ImpactParams>).params(
            "{definition: id, …content as symbolnew…, builtins?: bool} (read only: every placed copy with what an update changes there and \
whether it clashes, design 048 §6.2)",
        ),
        CommandSpec::new("symbolupdate", "Update Symbol", crate::commands::input::typed::<UpdateParams>)
            .params("{copies: [representation id] | lineage: id, to?: definition id (default: the newest)} (refused when a copy would clash)")
            .noundo(),
        CommandSpec::new("makesymbol", "Make Symbol", crate::commands::input::typed::<MakeParams>)
            .params(
                "{handles?: [hex] (default: the selection) | component: id, name, key, points?: [{key, label?, at: [x,y] on the page}], \
slots?: [… at on the page], defaults?, replace?: bool (default true), graphics?: the drawing as edited in the builder, in the symbol's frame (points and slots then in that frame too), at?: [x,y] the page point of its origin} (design 048 §9.5: a symbol from plain geometry, or a drawn \
component promoted, keeping its IDs)",
            )
            .noundo(),
    ]
}

// ---------------------------------------------------------------- content (§3.5)

/// The shapes of a symbol: file records (`{kind: line|polyline|arc|circle|text|point, …}`).
type Shapes = Bounded<Map<String, Value>, 0, MAX_SHAPES>;
/// The connection points of a symbol.
type Points = Bounded<PointParam, 0, MAX_POINTS>;
/// The text slots of a symbol.
type Slots = Bounded<SlotParam, 0, MAX_SLOTS>;
/// The bridges of a symbol: lists of point keys.
type Bridges = Bounded<Bounded<String, 0, MAX_POINTS>, 0, MAX_POINTS>;
/// Representation IDs of placed copies.
type Copies = Bounded<RepresentationId, 0, MAX_COPIES>;

/// A point's key: text, or a number read as its text.
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
enum PointKey {
    Text(String),
    Number(serde_json::Number),
}

/// A connection point of a symbol.
#[derive(Deserialize, JsonSchema)]
struct PointParam {
    /// Its key, unique in the symbol.
    #[serde(default)]
    key: Option<PointKey>,
    /// The label printed beside it.
    #[serde(default)]
    label: Option<String>,
    /// Where it is.
    #[serde(default)]
    at: Option<Point>,
}

/// A slot's horizontal alignment.
#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum HParam {
    Left,
    Center,
    Right,
}

/// A slot's vertical alignment.
#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum VParam {
    Baseline,
    Bottom,
    Middle,
    Top,
}

/// A text slot of a symbol.
#[derive(Deserialize, JsonSchema)]
struct SlotParam {
    /// The device text it places: tag, description, rating, rating.<label>, part_number, maker,
    /// article_number, function or location.
    #[serde(default)]
    text: Option<String>,
    /// Where it is.
    #[serde(default)]
    at: Option<Point>,
    /// Horizontal alignment (default left).
    #[serde(default)]
    h: Option<HParam>,
    /// Vertical alignment (default baseline).
    #[serde(default)]
    v: Option<VParam>,
    /// Degrees, counter-clockwise.
    #[serde(default)]
    rotation: Option<Degrees>,
}

/// The content fields every builder command takes, as read.
struct ContentFields {
    name: Option<String>,
    key: Option<String>,
    graphics: Option<Shapes>,
    points: Option<Points>,
    slots: Option<Slots>,
    defaults: Option<std::collections::BTreeMap<String, String>>,
    bridges: Option<Bridges>,
}

impl ContentFields {
    /// The content they give.
    ///
    /// # Errors
    /// A shape that is not a file record, or the rules of [`rules::content`].
    fn content(self) -> Result<Content, CmdError> {
        let mut graphics = Vec::new();
        for m in self.graphics.map(|b| b.0).unwrap_or_default() {
            let rec: ecad_file::records::Primitive =
                serde_json::from_value(Value::Object(m)).map_err(|e| CmdError::input(format!("A shape could not be read: {e}")))?;
            graphics.push(ecad_model::Primitive::from(rec));
        }
        let points = self.points.map(|b| b.0).unwrap_or_default().into_iter().map(|p| PointInput {
            key: p.key.map(|k| match k {
                PointKey::Text(t) => t,
                PointKey::Number(n) => n.to_string(),
            }),
            label: p.label,
            at: p.at.map(|a| a.0),
        });
        let slots = self.slots.map(|b| b.0).unwrap_or_default().into_iter().map(|s| SlotInput {
            text: s.text,
            at: s.at.map(|a| a.0),
            h: match s.h {
                None | Some(HParam::Left) => TextHAlign::Left,
                Some(HParam::Center) => TextHAlign::Center,
                Some(HParam::Right) => TextHAlign::Right,
            },
            v: match s.v {
                None | Some(VParam::Baseline) => TextVAlign::Baseline,
                Some(VParam::Bottom) => TextVAlign::Bottom,
                Some(VParam::Middle) => TextVAlign::Middle,
                Some(VParam::Top) => TextVAlign::Top,
            },
            rotation: s.rotation.map(Radians::from).unwrap_or_default(),
        });
        rules::content(ContentInput {
            name: self.name,
            key: self.key,
            graphics,
            points: points.collect(),
            slots: slots.collect(),
            defaults: self.defaults.unwrap_or_default(),
            bridges: self.bridges.map(|b| b.0.into_iter().map(|g| g.0).collect()).unwrap_or_default(),
        })
    }
}

// ---------------------------------------------------------------- commands

/// `symbolnew {name, key, graphics, points, slots?, defaults?, bridges?, from?}` (design 048
/// §9.1): version 1 of a project symbol.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NewParams {
    /// The symbol's name.
    #[serde(default)]
    name: Option<String>,
    /// Its key: up to 32 letters, digits, - and _.
    #[serde(default)]
    key: Option<String>,
    /// Its shapes, as in the file record.
    #[serde(default)]
    graphics: Option<Shapes>,
    /// Its connection points.
    #[serde(default)]
    points: Option<Points>,
    /// Where its device texts go.
    #[serde(default)]
    slots: Option<Slots>,
    /// Its defaults: prefix, description, rating, join (device or part), aliases, family.
    #[serde(default)]
    defaults: Option<std::collections::BTreeMap<String, String>>,
    /// Groups of point keys bridged inside the device.
    #[serde(default)]
    bridges: Option<Bridges>,
    /// The definition it is copied from (a built-in).
    #[serde(default)]
    from: Option<DefinitionId>,
}

impl Command for NewParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let fields = ContentFields {
            name: self.name,
            key: self.key,
            graphics: self.graphics,
            points: self.points,
            slots: self.slots,
            defaults: self.defaults,
            bridges: self.bridges,
        };
        let content = fields.content().map_err(|e| err(e.to_string()))?;
        on_project(s, |p, pg, _| {
            let plan = rules::plan_new(p.ledger.document(), &mut p.ids, &content, self.from)?;
            p.apply_edit(pg, "symbolnew", &plan.label, plan.ops)?;
            Ok(rules::result(p.ledger.document(), plan.id))
        })
    }
}

/// `symbolimpact {definition, …content as symbolnew…, builtins?}` (read only, design 048 §6.2):
/// every placed copy with what an update changes there and whether it clashes.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ImpactParams {
    /// The version edited.
    definition: DefinitionId,
    /// The symbol's name.
    #[serde(default)]
    name: Option<String>,
    /// Its key.
    #[serde(default)]
    key: Option<String>,
    /// Its shapes, as in the file record.
    #[serde(default)]
    graphics: Option<Shapes>,
    /// Its connection points.
    #[serde(default)]
    points: Option<Points>,
    /// Where its device texts go.
    #[serde(default)]
    slots: Option<Slots>,
    /// Its defaults.
    #[serde(default)]
    defaults: Option<std::collections::BTreeMap<String, String>>,
    /// Groups of point keys bridged inside the device.
    #[serde(default)]
    bridges: Option<Bridges>,
    /// Also the copies of an edited built-in.
    #[serde(default)]
    builtins: Option<bool>,
}

impl Command for ImpactParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let fields = ContentFields {
            name: self.name,
            key: self.key,
            graphics: self.graphics,
            points: self.points,
            slots: self.slots,
            defaults: self.defaults,
            bridges: self.bridges,
        };
        let content = fields.content().map_err(|e| err(e.to_string()))?;
        let builtins = self.builtins.unwrap_or(false);
        on_project(s, |p, _, _| {
            let doc = p.ledger.document();
            let i = rules::plan_impact(doc, &p.ids, self.definition, &content, builtins)?;
            let pages: BTreeSet<PageId> = i.rows.iter().map(|r| r.page).collect();
            let book = ecad_model::document::book::Book::of(doc);
            let mut places = ecad_electrical::references::devices::places::Places::new(doc);
            let rows_json: Vec<Value> = i.rows.iter().map(|r| rules::row_json(r, doc, &book, &mut places)).collect();
            Ok(json!({
                "name": content.name, "version": i.version, "builtin": i.builtin,
                "copies": i.rows.len(), "pages": pages.len(), "clashes": i.rows.iter().filter(|r| r.clash.is_some()).count(),
                "rows": rows_json,
                "problems": i.problems,
            }))
        })
    }
}

/// `symboledit {definition, …content as symbolnew…, update, copies?, builtins?}` (design 048
/// §9.2): a new version, and the chosen placed copies updated, in one transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditParams {
    /// The version edited.
    definition: DefinitionId,
    /// The symbol's name.
    #[serde(default)]
    name: Option<String>,
    /// Its key.
    #[serde(default)]
    key: Option<String>,
    /// Its shapes, as in the file record.
    #[serde(default)]
    graphics: Option<Shapes>,
    /// Its connection points.
    #[serde(default)]
    points: Option<Points>,
    /// Where its device texts go.
    #[serde(default)]
    slots: Option<Slots>,
    /// Its defaults.
    #[serde(default)]
    defaults: Option<std::collections::BTreeMap<String, String>>,
    /// Groups of point keys bridged inside the device.
    #[serde(default)]
    bridges: Option<Bridges>,
    /// Which placed copies to update: all (default), selected, none, or new (a new symbol).
    #[serde(default)]
    update: Option<String>,
    /// With `update: selected`: the copies (representation IDs).
    #[serde(default)]
    copies: Option<Copies>,
    /// With `update: all`: also the copies of an edited built-in.
    #[serde(default)]
    builtins: Option<bool>,
}

impl Command for EditParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let fields = ContentFields {
            name: self.name,
            key: self.key,
            graphics: self.graphics,
            points: self.points,
            slots: self.slots,
            defaults: self.defaults,
            bridges: self.bridges,
        };
        let content = fields.content().map_err(|e| err(e.to_string()))?;
        let update = Update::of_word(self.update.as_deref()).map_err(|e| err(e.to_string()))?;
        let copies = self.copies.map(|c| c.0);
        let builtins = self.builtins.unwrap_or(false);
        on_project(s, |p, pg, _| {
            let plan = rules::plan_edit(p.ledger.document(), &mut p.ids, self.definition, &content, update, copies, builtins)?;
            p.apply_edit(pg, "symboledit", &plan.label, plan.ops)?;
            let mut v = rules::result(p.ledger.document(), plan.id);
            v["updated"] = json!(plan.updated);
            v["left"] = json!(plan.left);
            Ok(v)
        })
    }
}

/// `symbolupdate {copies | lineage, to?}`: placed copies moved to a version of their symbol
/// (default: the newest); refused when a copy would clash.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateParams {
    /// The copies to update (representation IDs).
    #[serde(default)]
    copies: Option<Copies>,
    /// Without `copies`: every copy of this symbol (its lineage ID).
    #[serde(default)]
    lineage: Option<DefinitionId>,
    /// The version to move to (default: the newest).
    #[serde(default)]
    to: Option<DefinitionId>,
}

impl Command for UpdateParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let copies = self.copies.map(|c| c.0);
        on_project(s, |p, pg, _| {
            let plan = rules::plan_update(p.ledger.document(), &mut p.ids, copies, self.lineage, self.to)?;
            p.apply_edit(pg, "symbolupdate", &plan.label, plan.ops)?;
            let mut v = rules::result(p.ledger.document(), plan.id);
            v["updated"] = json!(plan.updated);
            Ok(v)
        })
    }
}

// ---------------------------------------------------------------- make symbol (§4)

/// `makesymbol {handles? | component, name, key, points?, slots?, defaults?, replace?, graphics?,
/// at?}` (design 048 §9.5): a symbol from plain geometry, or a drawn component promoted, keeping
/// its IDs.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct MakeParams {
    /// The shapes to make it of (hex handles; default: the selection).
    #[serde(default)]
    handles: Option<Bounded<String, 0, { MAX_HANDLES }>>,
    /// A drawn component to promote instead.
    #[serde(default)]
    component: Option<DeviceId>,
    /// The symbol's name.
    #[serde(default)]
    name: Option<String>,
    /// Its key.
    #[serde(default)]
    key: Option<String>,
    /// The drawing as edited in the builder, in the symbol's frame (points and slots then in that
    /// frame too; default: the page geometry).
    #[serde(default)]
    graphics: Option<Shapes>,
    /// Its connection points (page points, unless `graphics` is given).
    #[serde(default)]
    points: Option<Points>,
    /// Where its device texts go (page points, unless `graphics` is given).
    #[serde(default)]
    slots: Option<Slots>,
    /// Its defaults.
    #[serde(default)]
    defaults: Option<std::collections::BTreeMap<String, String>>,
    /// Groups of point keys bridged inside the device.
    #[serde(default)]
    bridges: Option<Bridges>,
    /// Replace the drawing with the new symbol (default true).
    #[serde(default)]
    replace: Option<bool>,
    /// The page point of the symbol's origin (default: the drawing's centre on the point grid).
    #[serde(default)]
    at: Option<Point>,
}

impl Command for MakeParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        // A promoted component's drawing is its own; the selection is not read.
        let hs: Vec<Handle> = match (&self.component, self.handles) {
            (Some(_), _) => Vec::new(),
            (None, None) => s.selection(),
            (None, Some(b)) => {
                b.0.iter()
                    .map(|h| Handle::parse_hex(h).ok_or_else(|| err(format!("`{}` is not a valid handle", json!(h)))))
                    .collect::<Result<_, _>>()?
            }
        };
        let fields = ContentFields {
            name: self.name,
            key: self.key,
            graphics: self.graphics,
            points: self.points,
            slots: self.slots,
            defaults: self.defaults,
            bridges: self.bridges,
        };
        let content = fields.content().map_err(|e| err(e.to_string()))?;
        let replace = self.replace.unwrap_or(true);
        let at = self.at.map(|a| a.0);
        on_project(s, |p, pg, page| {
            if let Some(c) = self.component {
                let style = plain_style(p);
                let made = rules::plan_promote(p.ledger.document(), &mut p.ids, c, content, style)?;
                p.apply_edit(pg, "makesymbol", &made.plan.label, made.plan.ops)?;
                let mut v = rules::result(p.ledger.document(), made.plan.id);
                v["component"] = json!(c.hex());
                v["representation"] = json!(made.representation.hex());
                return Ok(v);
            }
            let map = &p.pages.get(&page).ok_or(CmdError::refused("page view not found"))?.map;
            let doc = p.ledger.document();
            let mut items = Vec::new();
            for h in hs {
                match map.get(h) {
                    Some(Mapped::Item(i)) if doc.representation_of_item(i).is_none() => items.push(i),
                    Some(Mapped::Item(i)) => return Err(CmdError::refused(not_plain(doc, i))),
                    Some(_) => return Err(CmdError::refused("Make symbol takes lines, arcs and text; part of the selection already has a meaning.")),
                    None => return Err(CmdError::refused("The selection holds an object this page does not know.")),
                }
            }
            let style = plain_style(p);
            let made = rules::plan_make(&p.ledger, &mut p.ids, &mut p.tag_high, &items, content, MakeAt { page, style, at, replace })?;
            p.apply_edit(pg, "makesymbol", &made.plan.label, made.plan.ops)?;
            let mut v = rules::result(p.ledger.document(), made.plan.id);
            if let Some((c, r, t)) = made.placed {
                v["component"] = json!(c.hex());
                v["representation"] = json!(r.hex());
                v["tag"] = json!(t);
            }
            Ok(v)
        })
    }
}

// ---------------------------------------------------------------- placing (§5.1)

/// `place {symbol: <project key>, at, rotation?, mirror?, device?, tag?}` (design 048 §5.1): the
/// newest version of that project symbol. `None` when `key` names no project symbol.
pub(crate) fn place_project_key(
    s: &mut Session, params: &crate::devices::commands::PlaceParams, key: &str,
) -> cadcraft_engine::Result<Option<Value>> {
    let at = params.at.0;
    if !(at.x.abs() <= crate::interact::MAX_COORD && at.y.abs() <= crate::interact::MAX_COORD) {
        return Err(err(crate::interact::OUT_OF_RANGE));
    }
    let degrees = params.rotation.unwrap_or_default();
    let mirror = params.mirror.unwrap_or(false);
    let device = params.device_id()?;
    // Pack re-run, bug 2: a typed tag names the new device, as for a built-in symbol.
    let tag = params.tag.clone();
    ecad_electrical::devices::commands::place::one_of(device.is_some(), tag.is_some(), "device").map_err(|e| err(e.to_string()))?;
    let modes = params.modes()?;
    let key = key.to_string();
    on_project(s, |p, pg, page| {
        let doc = p.ledger.document();
        let Some(v) = symbols::project_by_key(doc, &key) else { return Ok(None) };
        let (id, def) = (v.id, v.def.clone());
        // A typed tag a device already holds is that device (a tag is an identity), as `place`
        // does for built-in symbols; devices and cables share one namespace (brief 026).
        let (device, tag) = ecad_electrical::devices::commands::place::project_target(doc, device, tag.as_deref())?;
        let pl = Placement { origin: at, rotation: Radians::from(degrees), mirror, scale: 1.0 };
        let style = plain_style(p);
        let mut plan = rules::plan_place(&p.ledger, &mut p.ids, &mut p.tag_high, page, style, id, &def, pl, device, tag)?;
        let label = if device.is_some() { format!("Add symbol to {}", plan.tag) } else { "Place symbol".to_string() };
        // Brief 052d (design 052 §5.3): the place proposes.
        let proposed = crate::symbols::draw::plan_place(p, &plan.ops, modes, plan.component, page, at, None).map_err(CmdError::refused)?;
        plan.ops.extend(proposed.ops.iter().cloned());
        p.apply_edit(pg, "place", &label, plan.ops)?;
        let doc = p.ledger.document();
        let connection_points: Vec<Value> =
            doc.connection_points_of(plan.component).map(|(t, x)| json!({"id": t.hex(), "key": x.key, "label": x.label})).collect();
        let mut v = json!({
            "component": plan.component.hex(), "representation": plan.representation.hex(), "definition": id.hex(),
            "tag": plan.tag, "joined": device.is_some(), "connection_points": connection_points,
        });
        crate::symbols::draw::finish_place(p, pg, plan.component, device.is_some(), &proposed, &mut v);
        Ok(Some(v))
    })
}
