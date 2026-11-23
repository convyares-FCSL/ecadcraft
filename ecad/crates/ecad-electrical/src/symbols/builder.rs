//! The symbol builder's commands: `symbolnew`, `symboledit`,
//! `symbolimpact` (read only) and `symbolupdate`: params in, ops and a JSON result out, with the
//! content fields the builder's `makesymbol` shares. Generic ([`crate::commands::run`]); the rules
//! are in [`super::commands`].
//!
//! Excludes: `makesymbol` (it reads the selection on the page: the engine's session command).

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::geometry::units::{Degrees, Radians};
use ecad_model::{DefinitionId, PageId, RepresentationId, TextHAlign, TextVAlign};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::commands::{self as rules, Content, ContentInput, MAX_COPIES, MAX_POINTS, MAX_SHAPES, MAX_SLOTS, PointInput, SlotInput, Update};
use crate::commands::input::{Bounded, Point};
use crate::commands::run::{Access, Cx, Generic, Outcome};
use crate::error::CmdError;

// ---------------------------------------------------------------- content

/// The shapes of a symbol: file records (`{kind: line|polyline|arc|circle|text|point, …}`).
pub type Shapes = Bounded<Map<String, Value>, 0, MAX_SHAPES>;
/// The connection points of a symbol.
pub type Points = Bounded<PointParam, 0, MAX_POINTS>;
/// The text slots of a symbol.
pub type Slots = Bounded<SlotParam, 0, MAX_SLOTS>;
/// The bridges of a symbol: lists of point keys.
pub type Bridges = Bounded<Bounded<String, 0, MAX_POINTS>, 0, MAX_POINTS>;
/// Representation IDs of placed copies.
pub(crate) type Copies = Bounded<RepresentationId, 0, MAX_COPIES>;

/// A point's key: text, or a number read as its text.
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum PointKey {
    Text(String),
    Number(serde_json::Number),
}

/// A connection point of a symbol.
#[derive(Deserialize, JsonSchema)]
pub struct PointParam {
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
pub(crate) enum HParam {
    Left,
    Center,
    Right,
}

/// A slot's vertical alignment.
#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub(crate) enum VParam {
    Baseline,
    Bottom,
    Middle,
    Top,
}

/// A text slot of a symbol.
#[derive(Deserialize, JsonSchema)]
pub struct SlotParam {
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
pub struct ContentFields {
    pub name: Option<String>,
    pub key: Option<String>,
    pub graphics: Option<Shapes>,
    pub points: Option<Points>,
    pub slots: Option<Slots>,
    pub defaults: Option<BTreeMap<String, String>>,
    pub bridges: Option<Bridges>,
}

impl ContentFields {
    /// The content they give.
    ///
    /// # Errors
    /// A shape that is not a file record, or the rules of [`rules::content`].
    pub fn content(self) -> Result<Content, CmdError> {
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

/// `symbolnew {name, key, graphics, points, slots?, defaults?, bridges?, from?}`:
/// version 1 of a project symbol.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewParams {
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
    defaults: Option<BTreeMap<String, String>>,
    /// Groups of point keys bridged inside the device.
    #[serde(default)]
    bridges: Option<Bridges>,
    /// The definition it is copied from (a built-in).
    #[serde(default)]
    from: Option<DefinitionId>,
}

/// `symbolimpact {definition, …content as symbolnew…, builtins?}` (read only):
/// every placed copy with what an update changes there and whether it clashes.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImpactParams {
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
    defaults: Option<BTreeMap<String, String>>,
    /// Groups of point keys bridged inside the device.
    #[serde(default)]
    bridges: Option<Bridges>,
    /// Also the copies of an edited built-in.
    #[serde(default)]
    builtins: Option<bool>,
}

/// `symboledit {definition, …content as symbolnew…, update, copies?, builtins?}`:
/// a new version, and the chosen placed copies updated, in one transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditParams {
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
    defaults: Option<BTreeMap<String, String>>,
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

/// `symbolupdate {copies | lineage, to?}`: placed copies moved to a version of their symbol
/// (default: the newest); refused when a copy would clash.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateParams {
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

impl Generic for NewParams {
    const ID: &'static str = "symbolnew";
    const TITLE: &'static str = "New Symbol";
    const HELP: &'static str = "{name, key, graphics: [{kind: line|polyline|arc|circle|text|point, …} as in the file record], points: [{key, label?, at: [x,y]}], \
slots?: [{text: tag|description|rating|rating.<label>|part_number|maker|article_number|function|location, at: [x,y], h?: left|center|right, \
v?: baseline|bottom|middle|top, rotation?: degrees}], defaults?: {prefix, description?, rating?, join?: device|part, aliases?, family?}, \
bridges?: [[key…]], from?: definition id} (design 048 §9.1: embeds version 1 of a project symbol; one transaction)";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let fields = ContentFields {
            name: self.name,
            key: self.key,
            graphics: self.graphics,
            points: self.points,
            slots: self.slots,
            defaults: self.defaults,
            bridges: self.bridges,
        };
        let content = fields.content()?;
        let plan = rules::plan_new(cx.doc, cx.ids, &content, self.from)?;
        let id = plan.id;
        Ok(Outcome::edit_then(plan.label, plan.ops, move |doc| rules::result(doc, id)))
    }
}

impl Generic for ImpactParams {
    const ID: &'static str = "symbolimpact";
    const TITLE: &'static str = "Symbol Impact";
    const HELP: &'static str = "{definition: id, …content as symbolnew…, builtins?: bool} (read only: every placed copy with what an update changes there and \
whether it clashes, design 048 §6.2)";
    const ACCESS: Access = Access::Reads;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let fields = ContentFields {
            name: self.name,
            key: self.key,
            graphics: self.graphics,
            points: self.points,
            slots: self.slots,
            defaults: self.defaults,
            bridges: self.bridges,
        };
        let content = fields.content()?;
        let builtins = self.builtins.unwrap_or(false);
        let doc = cx.doc;
        let i = rules::plan_impact(doc, cx.ids, self.definition, &content, builtins)?;
        let pages: BTreeSet<PageId> = i.rows.iter().map(|r| r.page).collect();
        let book = ecad_model::document::book::Book::of(doc);
        let mut places = crate::references::devices::places::Places::new(doc);
        let rows_json: Vec<Value> = i.rows.iter().map(|r| rules::row_json(r, doc, &book, &mut places)).collect();
        Ok(Outcome::answer(json!({
            "name": content.name, "version": i.version, "builtin": i.builtin,
            "copies": i.rows.len(), "pages": pages.len(), "clashes": i.rows.iter().filter(|r| r.clash.is_some()).count(),
            "rows": rows_json,
            "problems": i.problems,
        })))
    }
}

impl Generic for EditParams {
    const ID: &'static str = "symboledit";
    const TITLE: &'static str = "Edit Symbol";
    const HELP: &'static str = "{definition: id of the version edited, …content as symbolnew…, update: all|selected|none|new, copies?: [representation id], \
builtins?: bool} (design 048 §9.2: a new version, and the chosen placed copies updated, in one transaction)";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let fields = ContentFields {
            name: self.name,
            key: self.key,
            graphics: self.graphics,
            points: self.points,
            slots: self.slots,
            defaults: self.defaults,
            bridges: self.bridges,
        };
        let content = fields.content()?;
        let update = Update::of_word(self.update.as_deref())?;
        let copies = self.copies.map(|c| c.0);
        let builtins = self.builtins.unwrap_or(false);
        let plan = rules::plan_edit(cx.doc, cx.ids, self.definition, &content, update, copies, builtins)?;
        let (id, updated, left) = (plan.id, plan.updated, plan.left);
        Ok(Outcome::edit_then(plan.label, plan.ops, move |doc| {
            let mut v = rules::result(doc, id);
            v["updated"] = json!(updated);
            v["left"] = json!(left);
            v
        }))
    }
}

impl Generic for UpdateParams {
    const ID: &'static str = "symbolupdate";
    const TITLE: &'static str = "Update Symbol";
    const HELP: &'static str =
        "{copies: [representation id] | lineage: id, to?: definition id (default: the newest)} (refused when a copy would clash)";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let copies = self.copies.map(|c| c.0);
        let plan = rules::plan_update(cx.doc, cx.ids, copies, self.lineage, self.to)?;
        let (id, updated) = (plan.id, plan.updated);
        Ok(Outcome::edit_then(plan.label, plan.ops, move |doc| {
            let mut v = rules::result(doc, id);
            v["updated"] = json!(updated);
            v
        }))
    }
}
