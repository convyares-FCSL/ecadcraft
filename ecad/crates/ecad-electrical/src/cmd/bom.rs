//! The rules of the BOM commands (brief 047, design 047 §7): which devices `partline` writes to and
//! the lines it adds, sets or removes; the item `bomitem` adds, edits or deletes; the article
//! numbers `article` writes. The part line, item and article rules themselves are [`crate::bom`];
//! the engine crate's shims commit the ops and answer in JSON.

use std::collections::BTreeMap;

use ecad_model::{Component, ComponentId, Document, IdAllocator, ObjectRef, Op, Properties};

use crate::bom::*;
use crate::designations::{Aspect, Pending};
use crate::error::CmdError;

// ---------------------------------------------------------------------------------------------
// partline

/// The fields of one part line given (`None` clears a field).
pub type LineFields = Vec<(Field, Option<String>)>;

/// The refusal for a field a part line does not take there.
pub fn not_a_line_field(word: &str) -> CmdError {
    CmdError::refused(format!("`{word}` is not a part line field. Use {}.", field_words()))
}

/// What `partline` does to each device named.
#[derive(Debug, Clone, PartialEq)]
pub enum PartlineChange {
    /// Add these lines (as a kit, when named).
    Add { lines: Vec<LineFields>, kit: Option<String> },
    /// Set fields of line `line`, or remove it.
    Edit { line: Option<u64>, remove: bool, set: Option<LineFields> },
}

/// What `partline` records and says.
#[derive(Debug, Clone, PartialEq)]
pub struct PartlinePlan {
    /// Empty when nothing changes.
    pub ops: Vec<Op>,
    /// Each device and the lines written.
    pub rows: Vec<(ComponentId, Vec<u8>)>,
    pub label: String,
    pub message: String,
}

/// The devices a `partline` call names: `components` when given, else `component`; each one a
/// device or strip.
///
/// # Errors
/// None named, an empty list, no such component, or one that takes no part lines.
pub fn partline_targets(doc: &Document, component: Option<ComponentId>, components: Option<Vec<ComponentId>>) -> Result<Vec<ComponentId>, CmdError> {
    let cs = match (components, component) {
        (Some(cs), _) => cs,
        (None, Some(c)) => vec![c],
        (None, None) => return Err(CmdError::input("`component` or `components` (hex id) is required")),
    };
    if cs.is_empty() {
        return Err(CmdError::input("`components` is empty"));
    }
    for c in &cs {
        if !doc.components.contains_key(c) {
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
pub fn plan_partline(doc: &Document, cs: &[ComponentId], change: PartlineChange) -> Result<PartlinePlan, CmdError> {
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
                let props = doc.components.get(c).map(|x| &x.properties).ok_or_else(no_component)?;
                let mut used = used_lines(props);
                let mut ns = Vec::new();
                for fields in &lines {
                    let n = next_line(&used).ok_or_else(|| {
                        CmdError::refused(format!("{} has {MAX_LINES} part lines; remove one first.", crate::devices::tag_words(doc, *c)))
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
                [one] => crate::devices::tag_words(doc, *one),
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
                let props = doc.components.get(c).map(|x| &x.properties).ok_or_else(no_component)?;
                if !used_lines(props).contains(&n) {
                    return Err(CmdError::refused(format!("{} has no part line {n}.", crate::devices::tag_words(doc, *c))));
                }
                if remove {
                    let prefix = format!("{PART_PREFIX}{n}.");
                    for (k, v) in props.range(prefix.clone()..).take_while(|(k, _)| k.starts_with(&prefix)) {
                        ops.push(Op::SetProperty { object: ObjectRef::Component(*c), key: k.clone(), before: Some(v.clone()), after: None });
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
pub const ITEM_FIELDS: [&str; 7] = ["description", "unit", "qty", "location", "part_number", "maker", ARTICLE_KEY];

/// The item fields given (`None` clears a field).
pub type ItemFields = BTreeMap<&'static str, Option<String>>;

/// The refusal for a parameter that is not an item field (an add without `item`).
pub fn not_an_item_field(word: &str) -> CmdError {
    CmdError::refused(format!("`{word}` is not an item field. Use {}.", ITEM_FIELDS.join(", ")))
}

/// What `bomitem` does.
#[derive(Debug, Clone, PartialEq)]
pub enum ItemChange {
    /// Add an item with these fields.
    Add(ItemFields),
    /// Delete item `item`, or set its fields.
    Edit { item: ComponentId, delete: bool, set: Option<ItemFields> },
}

/// What `bomitem` records and says.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemPlan {
    /// Empty when nothing changes.
    pub ops: Vec<Op>,
    pub item: ComponentId,
    pub label: &'static str,
    /// What happened, in words (none when nothing changes).
    pub message: Option<String>,
}

/// `bomitem {description, …}` / `{item, set}` / `{item, delete}`: an item added, edited or
/// deleted (one transaction). A typed location adds its entry to the project's list.
///
/// # Errors
/// Not an item, no description, a value that does not read, or a project full of items.
pub fn plan_bomitem(doc: &Document, ids: &mut IdAllocator, change: ItemChange) -> Result<ItemPlan, CmdError> {
    match change {
        ItemChange::Edit { item, delete, set } => {
            let comp = doc
                .components
                .get(&item)
                .filter(|c| is_item_component(c))
                .ok_or_else(|| CmdError::refused(format!("{} is not an item.", item.hex())))?;
            if delete {
                let words = item_words(doc, &comp.properties);
                let ops = vec![Op::DeleteComponent { id: item, component: comp.clone() }];
                return Ok(ItemPlan { ops, item, label: "Delete item", message: Some(format!("Deleted item {words}.")) });
            }
            let fields = set.ok_or(CmdError::input("Give `set: {field: value}` or `delete: true` with `item`."))?;
            let props = &comp.properties;
            let metres = match fields.get(UNIT_KEY) {
                Some(u) => parse_unit(u.as_deref().unwrap_or(""))?,
                None => props.get(UNIT_KEY).is_some_and(|u| parse_unit(u).unwrap_or(false)),
            };
            let qty = match fields.get(QTY_KEY) {
                Some(q) => q.clone(),
                None => props.get(QTY_KEY).cloned(),
            };
            let mut ops = Vec::new();
            let mut pending = Pending::default();
            for key in ITEM_FIELDS {
                let Some(v) = fields.get(key) else { continue };
                let after = match (key, v.as_deref().map(str::trim).filter(|t| !t.is_empty())) {
                    // Brief 052: an item's location is an entry key of the project's list.
                    ("location", Some(t)) => Some(pending.store(doc, ids, Aspect::Location, t, &mut ops)?),
                    _ => item_value(doc, key, v.as_deref(), metres, qty.as_deref())?,
                };
                let before = props.get(key).cloned();
                if before != after {
                    ops.push(Op::SetProperty { object: ObjectRef::Component(item), key: key.to_string(), before, after });
                }
            }
            let message = (!ops.is_empty()).then(|| "Edited the item.".to_string());
            Ok(ItemPlan { ops, item, label: "Edit item", message })
        }
        ItemChange::Add(fields) => {
            if !fields.contains_key("description") {
                return Err(CmdError::refused(ITEM_NEEDS_DESCRIPTION));
            }
            let items = doc.components.values().filter(|c| is_item_component(c)).count();
            if items >= MAX_ITEMS {
                return Err(CmdError::refused(format!("This project has {items} items, the most a project holds.")));
            }
            let metres = match fields.get(UNIT_KEY) {
                Some(u) => parse_unit(u.as_deref().unwrap_or(""))?,
                None => item_unit_metres(doc),
            };
            let qty = fields.get(QTY_KEY).cloned().flatten();
            let mut props = Properties::new();
            props.insert(ecad_model::connectivity::KIND.into(), ITEM_KIND.into());
            if metres {
                props.insert(UNIT_KEY.into(), METRES.into());
            }
            let mut ops = Vec::new();
            let mut pending = Pending::default();
            for key in ITEM_FIELDS {
                if key == UNIT_KEY {
                    continue;
                }
                let Some(v) = fields.get(key) else { continue };
                let x = match (key, v.as_deref().map(str::trim).filter(|t| !t.is_empty())) {
                    ("location", Some(t)) => Some(pending.store(doc, ids, Aspect::Location, t, &mut ops)?),
                    _ => item_value(doc, key, v.as_deref(), metres, qty.as_deref())?,
                };
                if let Some(x) = x {
                    props.insert(key.to_string(), x);
                }
            }
            let words = item_words(doc, &props);
            let item: ComponentId = ids.fresh();
            ops.push(Op::CreateComponent { id: item, component: Component { properties: props, definition: None } });
            Ok(ItemPlan { ops, item, label: "Add item", message: Some(format!("Added item {words}.")) })
        }
    }
}

// ---------------------------------------------------------------------------------------------
// article

/// What `article` records and says.
#[derive(Debug, Clone, PartialEq)]
pub struct ArticlePlan {
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
pub fn plan_article(doc: &Document, part_number: &str, maker: Option<&str>, article_number: Option<&str>) -> Result<ArticlePlan, CmdError> {
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
    for (object, key, what) in &targets {
        *counts.entry(what).or_default() += 1;
        let before = match object {
            ObjectRef::Component(c) => doc.components.get(c).and_then(|x| x.properties.get(key)).cloned(),
            _ => doc.properties.get(key).cloned(),
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
