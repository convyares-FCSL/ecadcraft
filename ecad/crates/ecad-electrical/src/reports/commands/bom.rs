//! The rules of the BOM commands: which devices `partline` writes to and
//! the lines it adds, sets or removes; the item `bomitem` adds, edits or deletes; the article
//! numbers `article` writes. The part line, item and article rules themselves are [`crate::reports::bom`];
//! the engine crate's shims commit the ops and answer in JSON.

use std::collections::BTreeMap;

use ecad_model::objects::loose_part::{ItemUnit, LooseDescription, LoosePart, MAX_DESCRIPTION, Qty};
use ecad_model::{DeviceId, Document, IdAllocator, LocationId, LoosePartId, ObjectRef, Op, TextField};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::input::{Bounded, OrNull, TextOrNumber, nullable};
use crate::commands::run::{Cx, Generic, Outcome};
use crate::error::CmdError;
use crate::locations::designations::{Aspect, Pending};
use crate::reports::bom::*;
use ecad_model::naming::text::TextError;

// ---------------------------------------------------------------------------------------------
// partline

/// The fields of one part line given (`None` clears a field).
pub(crate) type LineFields = Vec<(Field, Option<String>)>;

/// The refusal for a field a part line does not take there.
pub(crate) fn not_a_line_field(word: &str) -> CmdError {
    CmdError::refused(format!("`{word}` is not a part line field. Use {}.", field_words()))
}

/// What `partline` does to each device named.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PartlineChange {
    /// Add these lines (as a kit, when named).
    Add { lines: Vec<LineFields>, kit: Option<String> },
    /// Set fields of line `line`, or remove it.
    Edit { line: Option<u64>, remove: bool, set: Option<LineFields> },
}

/// What `partline` records and says.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PartlinePlan {
    /// Empty when nothing changes.
    pub ops: Vec<Op>,
    /// Each device and the lines written.
    pub rows: Vec<(DeviceId, Vec<u8>)>,
    pub label: String,
    pub message: String,
}

/// The JSON `partline` answers for each device and the lines written (`rows`): with no `message`
/// nothing changed (`{changed: false, devices}`); else one device's row, or `{devices}` for
/// several, with `changed` and the message.
pub(crate) fn partline_json(rows: &[(DeviceId, Vec<u8>)], message: Option<&str>) -> Value {
    let rows: Vec<Value> = rows.iter().map(|(c, ns)| json!({"component": c.hex(), "lines": ns})).collect();
    let Some(message) = message else { return json!({"changed": false, "devices": rows}) };
    let mut v = match rows.as_slice() {
        [one] => one.clone(),
        _ => json!({"devices": rows}),
    };
    v["changed"] = json!(true);
    v["message"] = json!(message);
    v
}

/// The devices a `partline` call names: `devices` when given, else `component`; each one a
/// device or strip.
///
/// # Errors
/// None named, an empty list, no such component, or one that takes no part lines.
pub(crate) fn partline_targets(doc: &Document, component: Option<DeviceId>, devices: Option<Vec<DeviceId>>) -> Result<Vec<DeviceId>, CmdError> {
    let cs = match (devices, component) {
        (Some(cs), _) => cs,
        (None, Some(c)) => vec![c],
        (None, None) => return Err(CmdError::input("`component` or `devices` (hex id) is required")),
    };
    if cs.is_empty() {
        return Err(CmdError::input("`devices` is empty"));
    }
    for c in &cs {
        if !doc.devices().contains_key(c) {
            return Err(CmdError::refused(format!("No component {}.", c.hex())));
        }
        if !takes_lines(doc, *c) {
            return Err(CmdError::refused(PARTLINE_FOR_DEVICES));
        }
    }
    Ok(cs)
}

/// `partline {component | components, add | line + set | line + remove}`: the ops on every
/// device named, in one transaction.
///
/// # Errors
/// [`partline_targets`]'s refusals, a line that does not read, a device with no room or without
/// that line.
pub(crate) fn plan_partline(doc: &Document, cs: &[DeviceId], change: PartlineChange) -> Result<PartlinePlan, CmdError> {
    let mut ops = Vec::new();
    let mut rows = Vec::new();
    let no_component = || CmdError::refused("no such component");
    let (label, message) = match change {
        PartlineChange::Add { lines, kit } => {
            let kit = match kit {
                None => None,
                Some(k) => plain("kit", Some(&k), MAX_KIT_CHARS)?,
            };
            if lines.is_empty() {
                return Err(CmdError::input("`add` names no line"));
            }
            if lines.len() > MAX_ADD_LINES {
                return Err(CmdError::refused(format!("At most {MAX_ADD_LINES} lines per call.")));
            }
            if let Some((f, _)) = lines.iter().flatten().find(|(f, _)| *f == Field::Kit) {
                return Err(not_a_line_field(f.word()));
            }
            let lines: Vec<LineFields> = lines.into_iter().map(sorted).collect();
            for c in cs {
                let props = doc.devices().get(c).map(|x| &x.properties).ok_or_else(no_component)?;
                let mut used = used_lines(props);
                let mut ns = Vec::new();
                for fields in &lines {
                    let n = next_line(&used).ok_or_else(|| {
                        CmdError::refused(format!("{} has {MAX_LINES} part lines; remove one first.", crate::devices::tags::tag_words(doc, *c)))
                    })?;
                    used.insert(n);
                    let mut fields = fields.clone();
                    if let Some(k) = &kit {
                        fields.push((Field::Kit, Some(k.clone())));
                    }
                    if fields.iter().all(|(_, v)| v.as_deref().is_none_or(|t| t.trim().is_empty())) {
                        return Err(CmdError::refused("A part line needs at least one field: part_number, maker or description."));
                    }
                    ops.extend(line_ops(doc, *c, n, &fields)?);
                    ns.push(n);
                }
                rows.push((*c, ns));
            }
            let lines_words = if lines.len() == 1 { "a part line".to_string() } else { format!("{} part lines", lines.len()) };
            let to = match cs {
                [one] => crate::devices::tags::tag_words(doc, *one),
                _ => format!("{} devices", cs.len()),
            };
            match (&kit, lines.len()) {
                (Some(k), _) => (format!("Add kit {k}"), format!("Added kit {k} ({lines_words}) to {to}.")),
                (None, 1) => ("Add part line".to_string(), format!("Added {lines_words} to {to}.")),
                (None, _) => ("Add part lines".to_string(), format!("Added {lines_words} to {to}.")),
            }
        }
        PartlineChange::Edit { line, remove, set } => {
            let n = line.and_then(|n| u8::try_from(n).ok()).filter(|n| (1..=MAX_LINES).contains(n));
            let n = n.ok_or_else(|| CmdError::input(format!("`line` (1 to {MAX_LINES}) is required with `set` or `remove`, or give `add`")))?;
            let set = set.map(sorted);
            for c in cs {
                let props = doc.devices().get(c).map(|x| &x.properties).ok_or_else(no_component)?;
                if !used_lines(props).contains(&n) {
                    return Err(CmdError::refused(format!("{} has no part line {n}.", crate::devices::tags::tag_words(doc, *c))));
                }
                if remove {
                    let prefix = format!("{PART_PREFIX}{n}.");
                    for (k, v) in props.range(prefix.clone()..).take_while(|(k, _)| k.starts_with(&prefix)) {
                        ops.push(Op::SetProperty { object: ObjectRef::Device(*c), key: k.clone(), before: Some(v.clone()), after: None });
                    }
                } else if let Some(fields) = &set {
                    ops.extend(line_ops(doc, *c, n, fields)?);
                } else {
                    return Err(CmdError::input("Give `add`, `set` or `remove: true`."));
                }
                rows.push((*c, vec![n]));
            }
            if remove {
                ("Remove part line".to_string(), format!("Removed part line {n}."))
            } else {
                ("Edit part line".to_string(), format!("Edited part line {n}."))
            }
        }
    };
    Ok(PartlinePlan { ops, rows, label, message })
}

/// A line's fields in field order: unit before qty, so the qty is checked against the line's new
/// unit.
fn sorted(mut fields: LineFields) -> LineFields {
    fields.sort_by_key(|(f, _)| *f);
    fields
}

// ---------------------------------------------------------------------------------------------
// bomitem

/// The item fields `bomitem` takes, in the order they are checked (unit before qty).
pub(crate) const ITEM_FIELDS: [&str; 7] = ["description", "unit", "qty", "location", "part_number", "maker", ARTICLE_KEY];

/// The item fields given (`None` clears a field).
pub(crate) type ItemFields = BTreeMap<&'static str, Option<String>>;

/// The refusal for a parameter that is not an item field (an add without `item`).
pub(crate) fn not_an_item_field(word: &str) -> CmdError {
    CmdError::refused(format!("`{word}` is not an item field. Use {}.", ITEM_FIELDS.join(", ")))
}

/// What `bomitem` does.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ItemChange {
    /// Add an item with these fields.
    Add(ItemFields),
    /// Delete item `item`, or set its fields.
    Edit { item: LoosePartId, delete: bool, set: Option<ItemFields> },
}

/// What `bomitem` records and says.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ItemPlan {
    /// Empty when nothing changes.
    pub ops: Vec<Op>,
    pub item: LoosePartId,
    pub label: &'static str,
    /// What happened, in words (none when nothing changes).
    pub message: Option<String>,
}

/// The refusal for a text value of an item field (`key`, at most `max` characters).
fn text_refusal(key: &str, max: usize, e: TextError) -> CmdError {
    CmdError::refused(match e {
        TextError::TooLong { .. } => format!("`{key}` can be at most {max} characters."),
        TextError::Hidden { .. } | TextError::Lines => format!("`{key}` is one line with no control or hidden characters."),
        TextError::Empty => format!("`{key}` is empty."),
    })
}

/// A part text (part number, maker, article number): `None` clears it.
fn part_text(key: &str, t: Option<&str>) -> Result<Option<ecad_model::objects::devices::PartText>, CmdError> {
    t.map(|t| ecad_model::objects::devices::PartText::new(t).map_err(|e| text_refusal(key, MAX_ARTICLE_CHARS, e))).transpose()
}

/// Item `p` with `fields` written, each checked, in [`ITEM_FIELDS`] order (unit before qty). A
/// typed location adds its entry to the project's list (`ops`).
fn with_fields(doc: &Document, ids: &mut IdAllocator, mut p: LoosePart, fields: &ItemFields, ops: &mut Vec<Op>) -> Result<LoosePart, CmdError> {
    let mut pending = Pending::default();
    for key in ITEM_FIELDS {
        let Some(v) = fields.get(key) else { continue };
        let t = v.as_deref().map(str::trim).filter(|t| !t.is_empty());
        match key {
            "description" => {
                let t = t.ok_or(CmdError::refused(ITEM_NEEDS_DESCRIPTION))?;
                p.description = LooseDescription::new(t).map_err(|e| text_refusal(key, MAX_DESCRIPTION, e))?;
            }
            UNIT_KEY => p.unit = if parse_unit(t.unwrap_or(""))? { ItemUnit::Metres } else { ItemUnit::Pcs },
            QTY_KEY => p.qty = t.map_or(Ok(Qty::ONE), |t| parse_qty(t, p.unit == ItemUnit::Metres).map(|q| Qty(q.0)))?,
            // An item's location is an entry of the project's list.
            "location" => {
                p.location = match t {
                    Some(t) => {
                        let k = pending.store(doc, ids, Aspect::Location, t, ops)?;
                        Some(crate::locations::designations::raw(&k).map(LocationId).ok_or(CmdError::refused("That location is not in the list."))?)
                    }
                    None => None,
                }
            }
            "part_number" => p.part_number = part_text(key, t)?,
            "maker" => p.manufacturer = part_text(key, t)?,
            _ => p.article_number = part_text(key, t)?,
        }
    }
    if p.unit == ItemUnit::Pcs && !p.qty.whole() {
        return Err(CmdError::refused(format!("The quantity is {}: a count of pieces is whole. Set a whole quantity first.", p.qty)));
    }
    Ok(p)
}

/// `bomitem {description, …}` / `{item, set}` / `{item, delete}`: an item (a loose part, design
/// added, edited or deleted (one transaction). A typed location adds its entry to the
/// project's list.
///
/// # Errors
/// Not an item, no description, a value that does not read, or a project full of items.
pub(crate) fn plan_bomitem(doc: &Document, ids: &mut IdAllocator, change: ItemChange) -> Result<ItemPlan, CmdError> {
    match change {
        ItemChange::Edit { item, delete, set } => {
            let part = doc.loose_parts().get(&item).ok_or_else(|| CmdError::refused(format!("{} is not an item.", item.hex())))?;
            if delete {
                let words = item_words(doc, part);
                let ops = vec![Op::DeleteLoosePart { id: item, part: part.clone() }];
                return Ok(ItemPlan { ops, item, label: "Delete item", message: Some(format!("Deleted item {words}.")) });
            }
            let fields = set.ok_or(CmdError::input("Give `set: {field: value}` or `delete: true` with `item`."))?;
            let mut ops = Vec::new();
            let after = with_fields(doc, ids, part.clone(), &fields, &mut ops)?;
            if after != *part {
                ops.push(Op::ModifyLoosePart { id: item, before: part.clone(), after });
            }
            let message = (!ops.is_empty()).then(|| "Edited the item.".to_string());
            Ok(ItemPlan { ops, item, label: "Edit item", message })
        }
        ItemChange::Add(fields) => {
            let description = match fields.get("description") {
                Some(Some(d)) if !d.trim().is_empty() => {
                    LooseDescription::new(d.trim()).map_err(|e| text_refusal("description", MAX_DESCRIPTION, e))?
                }
                _ => return Err(CmdError::refused(ITEM_NEEDS_DESCRIPTION)),
            };
            let items = doc.loose_parts().len();
            if items >= MAX_ITEMS {
                return Err(CmdError::refused(format!("This project has {items} items, the most a project holds.")));
            }
            let mut part = LoosePart::new(description, Qty::ONE);
            if item_unit_metres(doc) {
                part.unit = ItemUnit::Metres;
            }
            let mut ops = Vec::new();
            let part = with_fields(doc, ids, part, &fields, &mut ops)?;
            let words = item_words(doc, &part);
            let item: LoosePartId = ids.fresh();
            ops.push(Op::CreateLoosePart { id: item, part });
            Ok(ItemPlan { ops, item, label: "Add item", message: Some(format!("Added item {words}.")) })
        }
    }
}

// ---------------------------------------------------------------------------------------------
// article

/// What `article` records and says.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ArticlePlan {
    /// Empty when every part already has that number.
    pub ops: Vec<Op>,
    /// The parts with that part number.
    pub parts: usize,
    pub message: String,
}

/// `article {part_number, maker?, article_number}`: the article number on every part with that
/// part number (and maker), in one transaction (`None` clears it).
///
/// # Errors
/// No part number, an article number that does not read, no such part, or too many.
pub(crate) fn plan_article(doc: &Document, part_number: &str, maker: Option<&str>, article_number: Option<&str>) -> Result<ArticlePlan, CmdError> {
    let pn = Some(part_number.trim()).filter(|t| !t.is_empty()).ok_or(CmdError::input("`part_number` is required"))?;
    let maker = maker.map(str::trim).filter(|m| !m.is_empty());
    let after = plain(ARTICLE_KEY, article_number, MAX_ARTICLE_CHARS)?;
    let targets = article_targets(doc, pn, maker);
    if targets.is_empty() {
        return Err(CmdError::refused(format!("No part {pn} in this project.")));
    }
    if targets.len() > MAX_COMPONENTS * 4 {
        return Err(CmdError::refused(format!("{pn} is used {} times; at most {} per call.", targets.len(), MAX_COMPONENTS * 4)));
    }
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut ops = Vec::new();
    for t in &targets {
        *counts.entry(t.what()).or_default() += 1;
        let (object, key) = match t {
            ArticleTarget::Item(id) => {
                let Some(p) = doc.loose_parts().get(id) else { continue };
                let typed = LoosePart { article_number: part_text(ARTICLE_KEY, after.as_deref())?, ..p.clone() };
                if typed != *p {
                    ops.push(Op::ModifyLoosePart { id: *id, before: p.clone(), after: typed });
                }
                continue;
            }
            ArticleTarget::Key(object, key, _) => (object, key),
        };
        // A device's main article number is its field: one `ModifyDevice`, first.
        if let ObjectRef::Device(c) = object
            && let Some(f) = TextField::of_key(key)
            && let Some(d) = doc.devices().get(c)
        {
            let typed = crate::properties::commands::devices::with_text(d, f, after.as_deref())?;
            if typed != *d {
                ops.insert(0, Op::ModifyDevice { id: *c, before: d.clone(), after: typed });
            }
            continue;
        }
        let before = match object {
            ObjectRef::Device(c) => doc.devices().get(c).and_then(|x| x.properties.get(key)).cloned(),
            _ => doc.display().get(key).cloned(),
        };
        if before != after {
            ops.push(Op::SetProperty { object: *object, key: key.clone(), before, after: after.clone() });
        }
    }
    let parts: Vec<String> =
        [("line", "line", "lines"), ("device", "device", "devices"), ("item", "item", "items"), ("strip", "strip part", "strip parts")]
            .iter()
            .filter_map(|(k, one, many)| counts.get(k).map(|n| format!("{n} {}", if *n == 1 { one } else { many })))
            .collect();
    let n = targets.len();
    let message = match &after {
        Some(a) => format!("Set {a} on {n} part{} ({}).", if n == 1 { "" } else { "s" }, parts.join(", ")),
        None => format!("Cleared the article number of {n} part{} ({}).", if n == 1 { "" } else { "s" }, parts.join(", ")),
    };
    Ok(ArticlePlan { ops, parts: n, message })
}

// ---------------------------------------------------------------------------------------------
// The commands (generic: crate::commands::run)

/// A field as typed: absent (unchanged), null (cleared) or a value.
type FieldValue = Option<Option<TextOrNumber>>;

/// The fields given, each with its value as text (`None`: cleared).
fn given<K>(fields: impl IntoIterator<Item = (K, FieldValue)>) -> Vec<(K, Option<String>)> {
    fields.into_iter().filter_map(|(k, v)| v.map(|v| (k, v.map(|t| t.text())))).collect()
}

/// An answer echoing its `message`.
fn echoed(out: Outcome) -> Outcome {
    out.echo_message()
}

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

/// Several lines to add, or one. (A list is tried first: a struct also reads from a list.)
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum AddParam {
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
pub struct PartlineParams {
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

/// `bomitem {description, qty?, unit?, location?, part_number?, maker?, article_number?}` adds an
/// item; `{item, set}` edits one; `{item, delete: true}` deletes one. One transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BomitemParams {
    /// The item (a loose part's id) to edit or delete.
    #[serde(default)]
    item: Option<LoosePartId>,
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

/// `article {part_number, maker?, article_number}`: sets the article number on every part with
/// that part number (one transaction).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArticleParams {
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

impl Generic for PartlineParams {
    const ID: &'static str = "partline";
    const TITLE: &'static str = "Part Line";
    const HELP: &'static str = "{component: id | devices: [id…], add: {part_number?, maker?, description?, article_number?, qty?, unit?: \"pcs\"|\"m\", for?: \"1,2,4\"} \
| add: [{…}, …] (at most 20), kit?: name} | {component, line: n, set: {field: value|null}} | {component, line: n, remove: true} (one transaction)";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
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
        let doc = cx.doc;
        let cs = partline_targets(doc, self.component, self.devices.map(|c| c.0))?;
        let plan = plan_partline(doc, &cs, change)?;
        if plan.ops.is_empty() {
            return Ok(echoed(Outcome::answer(partline_json(&plan.rows, None))));
        }
        let answer = partline_json(&plan.rows, Some(&plan.message));
        Ok(echoed(Outcome::edit(plan.label, plan.ops, answer)))
    }
}

impl Generic for BomitemParams {
    const ID: &'static str = "bomitem";
    const TITLE: &'static str = "BOM Item";
    const HELP: &'static str = "{description, qty?, unit?: \"pcs\"|\"m\", location?: \"+CP1\", part_number?, maker?, article_number?} (adds an item) \
| {item: id, set: {field: value|null}} | {item: id, delete: true} (one transaction)";

    fn check(&self) -> Result<(), CmdError> {
        // An add takes item fields only.
        match self.item {
            Some(_) => Ok(()),
            None => match [("delete", self.delete.is_some()), ("set", self.set.is_some())].into_iter().find(|k| k.1) {
                Some(k) => Err(not_an_item_field(k.0)),
                None => Ok(()),
            },
        }
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        self.check()?;
        let change = match self.item {
            Some(item) => ItemChange::Edit { item, delete: self.delete == Some(true), set: self.set.map(ItemParams::fields) },
            None => {
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
        let plan = plan_bomitem(cx.doc, cx.ids, change)?;
        let item = plan.item.hex();
        let Some(message) = plan.message else { return Ok(Outcome::answer(json!({"changed": false, "item": item}))) };
        Ok(echoed(Outcome::edit(plan.label, plan.ops, json!({"changed": true, "item": item, "message": message}))))
    }
}

impl Generic for ArticleParams {
    const ID: &'static str = "article";
    const TITLE: &'static str = "Article Number";
    const HELP: &'static str = "{part_number, maker?, article_number: text|null} (sets it on every part with that part number; one transaction)";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let value = self.article_number.flatten().map(|t| t.text());
        let plan = plan_article(cx.doc, &self.part_number, self.maker.as_deref(), value.as_deref())?;
        let changed = !plan.ops.is_empty();
        let answer = json!({"changed": changed, "parts": plan.parts, "message": plan.message});
        Ok(echoed(Outcome::edit_if_any("Set article number", plan.ops, answer)))
    }
}
