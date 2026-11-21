//! The BOM commands (brief 047, design 047 §7): `partline`, `bomitem` and `article`. The rules
//! (part lines, items, article numbers) are [`crate::bom`].
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::bom`] decide, and the shim commits the ops, echoes the message and
//! answers in JSON.

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::bom::{Field, MAX_COMPONENTS};
use ecad_electrical::cmd::bom::{self as rules, ItemChange, ItemFields, LineFields, PartlineChange};
use ecad_model::DeviceId;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cmd::input::{Bounded, Command, OrNull, TextOrNumber, Typed, nullable};
use crate::cmd::{err, on_project};

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[Typed::of::<PartlineParams>("partline"), Typed::of::<BomitemParams>("bomitem"), Typed::of::<ArticleParams>("article")];

/// The commands of this module, registered through [`crate::lanes`].
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("partline", "Part Line", crate::cmd::input::typed::<PartlineParams>)
            .params(
                "{component: id | devices: [id…], add: {part_number?, maker?, description?, article_number?, qty?, unit?: \"pcs\"|\"m\", for?: \"1,2,4\"} \
| add: [{…}, …] (at most 20), kit?: name} | {component, line: n, set: {field: value|null}} | {component, line: n, remove: true} (one transaction)",
            )
            .noundo(),
        CommandSpec::new("bomitem", "BOM Item", crate::cmd::input::typed::<BomitemParams>)
            .params(
                "{description, qty?, unit?: \"pcs\"|\"m\", location?: \"+CP1\", part_number?, maker?, article_number?} (adds an item) \
| {item: id, set: {field: value|null}} | {item: id, delete: true} (one transaction)",
            )
            .interactive(crate::interact::bomitem)
            .noundo(),
        CommandSpec::new("article", "Article Number", crate::cmd::input::typed::<ArticleParams>)
            .params("{part_number, maker?, article_number: text|null} (sets it on every part with that part number; one transaction)")
            .noundo(),
    ]
}

/// A field's value: text or a number (written as typed), or null to clear it.
type FieldValue = Option<Option<TextOrNumber>>;

/// The fields given, each with its value (`None` clears).
fn given<K>(fields: impl IntoIterator<Item = (K, FieldValue)>) -> Vec<(K, Option<String>)> {
    fields.into_iter().filter_map(|(k, v)| v.map(|v| (k, v.map(|t| t.text())))).collect()
}

/// Echo a command's message, and answer.
fn echoed(s: &mut Session, v: Value) -> cadcraft_engine::Result<Value> {
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

// ---------------------------------------------------------------------------------------------
// partline

/// The fields of one part line (null clears a field).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct LineParams {
    /// The part number.
    #[serde(default, deserialize_with = "nullable")]
    part_number: FieldValue,
    /// The maker.
    #[serde(default, deserialize_with = "nullable")]
    maker: FieldValue,
    /// What the part is.
    #[serde(default, deserialize_with = "nullable")]
    description: FieldValue,
    /// The article number.
    #[serde(default, deserialize_with = "nullable")]
    article_number: FieldValue,
    /// How many (pieces, or metres with unit m).
    #[serde(default, deserialize_with = "nullable")]
    qty: FieldValue,
    /// pcs or m.
    #[serde(default, deserialize_with = "nullable")]
    unit: FieldValue,
    /// With `set` only: the kit the line belongs to (an add names it with the call's `kit`).
    #[serde(default, deserialize_with = "nullable")]
    kit: FieldValue,
    /// The sub-items the line is for ("1,2,4").
    #[serde(default, deserialize_with = "nullable")]
    r#for: FieldValue,
    /// On a connector: the half the line belongs to (s or p).
    #[serde(default, deserialize_with = "nullable")]
    half: FieldValue,
}

impl LineParams {
    fn fields(self) -> LineFields {
        given([
            (Field::PartNumber, self.part_number),
            (Field::Maker, self.maker),
            (Field::Description, self.description),
            (Field::ArticleNumber, self.article_number),
            (Field::Qty, self.qty),
            (Field::Unit, self.unit),
            (Field::Kit, self.kit),
            (Field::For, self.r#for),
            (Field::Half, self.half),
        ])
    }
}

/// Several lines to add, or one. (A list is tried first: a struct also reads from a list.)
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
enum AddParam {
    /// Several lines (at most 20).
    Many(Vec<LineParams>),
    /// One line.
    One(LineParams),
}

/// `partline {component | components, add, kit?}` adds part lines to each device;
/// `{component, line, set}` sets a line's fields; `{component, line, remove: true}` removes it.
/// One transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PartlineParams {
    /// The device or strip.
    #[serde(default)]
    component: Option<DeviceId>,
    /// Several devices or strips.
    #[serde(default)]
    devices: Option<Bounded<DeviceId, 0, MAX_COMPONENTS>>,
    /// The line to add, or a list of lines (at most 20).
    #[serde(default)]
    add: Option<AddParam>,
    /// With `add`: the kit the lines belong to.
    #[serde(default)]
    kit: Option<String>,
    /// With `set` or `remove`: the line's number (1 to 99).
    #[serde(default)]
    line: Option<u64>,
    /// The line's fields to set (null clears a field).
    #[serde(default)]
    set: Option<LineParams>,
    /// True removes the line.
    #[serde(default)]
    remove: Option<bool>,
}

impl Command for PartlineParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let change = match self.add {
            Some(add) => {
                let lines = match add {
                    AddParam::One(l) => vec![l.fields()],
                    AddParam::Many(ls) => ls.into_iter().map(LineParams::fields).collect(),
                };
                PartlineChange::Add { lines, kit: self.kit }
            }
            None => PartlineChange::Edit { line: self.line, remove: self.remove == Some(true), set: self.set.map(LineParams::fields) },
        };
        let (component, devices) = (self.component, self.devices.map(|c| c.0));
        let v = on_project(s, |p, pg, _| {
            let doc = p.ledger.document();
            let cs = rules::partline_targets(doc, component, devices)?;
            let plan = rules::plan_partline(doc, &cs, change)?;
            if plan.ops.is_empty() {
                return Ok(rules::partline_json(&plan.rows, None));
            }
            p.apply_edit(pg, "partline", &plan.label, plan.ops)?;
            Ok(rules::partline_json(&plan.rows, Some(&plan.message)))
        })?;
        echoed(s, v)
    }
}

// ---------------------------------------------------------------------------------------------
// bomitem

/// The fields of an item (null clears a field).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ItemParams {
    /// What the item is.
    #[serde(default, deserialize_with = "nullable")]
    description: FieldValue,
    /// pcs or m.
    #[serde(default, deserialize_with = "nullable")]
    unit: FieldValue,
    /// How many (pieces, or metres with unit m).
    #[serde(default, deserialize_with = "nullable")]
    qty: FieldValue,
    /// Where it goes: a location code such as +CP1.
    #[serde(default, deserialize_with = "nullable")]
    location: FieldValue,
    /// The part number.
    #[serde(default, deserialize_with = "nullable")]
    part_number: FieldValue,
    /// The maker.
    #[serde(default, deserialize_with = "nullable")]
    maker: FieldValue,
    /// The article number.
    #[serde(default, deserialize_with = "nullable")]
    article_number: FieldValue,
}

impl ItemParams {
    fn fields(self) -> ItemFields {
        given([
            ("description", self.description),
            ("unit", self.unit),
            ("qty", self.qty),
            ("location", self.location),
            ("part_number", self.part_number),
            ("maker", self.maker),
            ("article_number", self.article_number),
        ])
        .into_iter()
        .collect()
    }
}

/// `bomitem {description, qty?, unit?, location?, part_number?, maker?, article_number?}` adds an
/// item; `{item, set}` edits one; `{item, delete: true}` deletes one. One transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct BomitemParams {
    /// The item to edit or delete.
    #[serde(default)]
    item: Option<DeviceId>,
    /// With `item`: the fields to set (null clears a field).
    #[serde(default)]
    set: Option<ItemParams>,
    /// With `item`: true deletes it.
    #[serde(default)]
    delete: Option<bool>,
    /// A new item: what it is.
    #[serde(default, deserialize_with = "nullable")]
    description: FieldValue,
    /// A new item: pcs or m.
    #[serde(default, deserialize_with = "nullable")]
    unit: FieldValue,
    /// A new item: how many (pieces, or metres with unit m).
    #[serde(default, deserialize_with = "nullable")]
    qty: FieldValue,
    /// A new item: where it goes, a location code such as +CP1.
    #[serde(default, deserialize_with = "nullable")]
    location: FieldValue,
    /// A new item: the part number.
    #[serde(default, deserialize_with = "nullable")]
    part_number: FieldValue,
    /// A new item: the maker.
    #[serde(default, deserialize_with = "nullable")]
    maker: FieldValue,
    /// A new item: the article number.
    #[serde(default, deserialize_with = "nullable")]
    article_number: FieldValue,
}

impl Command for BomitemParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let change = match self.item {
            Some(item) => ItemChange::Edit { item, delete: self.delete == Some(true), set: self.set.map(ItemParams::fields) },
            None => {
                // An add takes item fields only.
                if let Some(k) = [("delete", self.delete.is_some()), ("set", self.set.is_some())].into_iter().find(|k| k.1) {
                    return Err(err(rules::not_an_item_field(k.0).to_string()));
                }
                let fields = ItemParams {
                    description: self.description,
                    unit: self.unit,
                    qty: self.qty,
                    location: self.location,
                    part_number: self.part_number,
                    maker: self.maker,
                    article_number: self.article_number,
                };
                ItemChange::Add(fields.fields())
            }
        };
        let v = on_project(s, |p, pg, _| {
            let plan = rules::plan_bomitem(p.ledger.document(), &mut p.ids, change)?;
            let item = plan.item.hex();
            let Some(message) = plan.message else { return Ok(json!({"changed": false, "item": item})) };
            p.apply_edit(pg, "bomitem", plan.label, plan.ops)?;
            Ok(json!({"changed": true, "item": item, "message": message}))
        })?;
        echoed(s, v)
    }
}

// ---------------------------------------------------------------------------------------------
// article

/// `article {part_number, maker?, article_number}`: sets the article number on every part with
/// that part number (one transaction).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArticleParams {
    /// The part number.
    part_number: String,
    /// Only parts of this maker.
    #[serde(default)]
    maker: Option<String>,
    /// The article number, or null to clear it.
    #[serde(deserialize_with = "nullable")]
    #[schemars(required, with = "OrNull<TextOrNumber>")]
    article_number: FieldValue,
}

impl Command for ArticleParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let value = self.article_number.flatten().map(|t| t.text());
        let v = on_project(s, |p, pg, _| {
            let plan = rules::plan_article(p.ledger.document(), &self.part_number, self.maker.as_deref(), value.as_deref())?;
            let changed = !plan.ops.is_empty();
            if changed {
                p.apply_edit(pg, "article", "Set article number", plan.ops)?;
            }
            Ok(json!({"changed": changed, "parts": plan.parts, "message": plan.message}))
        })?;
        echoed(s, v)
    }
}
