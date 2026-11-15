//! The BOM commands (brief 047, design 047 §7): `partline`, `bomitem` and `article`. The rules
//! (part lines, items, article numbers) are [`crate::bom`].

use std::collections::BTreeMap;

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::{Component, ComponentId, Document, ObjectRef, Op, Properties};
use serde_json::{Value, json};

use crate::bom::*;
use crate::cmd::{id_list, opt_id, with_project};

/// The commands of this module, registered through [`crate::lanes`].
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("partline", "Part Line", run_partline)
            .params(
                "{component: id | components: [id…], add: {part_number?, maker?, description?, article_number?, qty?, unit?: \"pcs\"|\"m\", for?: \"1,2,4\"} \
| add: [{…}, …] (at most 20), kit?: name} | {component, line: n, set: {field: value|null}} | {component, line: n, remove: true} (one transaction)",
            )
            .noundo(),
        CommandSpec::new("bomitem", "BOM Item", run_bomitem)
            .params(
                "{description, qty?, unit?: \"pcs\"|\"m\", location?: \"+CP1\", part_number?, maker?, article_number?} (adds an item) \
| {item: id, set: {field: value|null}} | {item: id, delete: true} (one transaction)",
            )
            .interactive(crate::interact::bomitem)
            .noundo(),
        CommandSpec::new("article", "Article Number", run_article)
            .params("{part_number, maker?, article_number: text|null} (sets it on every part with that part number; one transaction)")
            .noundo(),
    ]
}

/// A field value from JSON: a string, a number (written as typed), or `null` (clear).
fn json_text(v: &Value, what: &str) -> Result<Option<String>, String> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s.clone())),
        Value::Number(n) => Ok(Some(n.to_string())),
        _ => Err(format!("`{what}` must be text, a number, or null")),
    }
}

/// The fields of one `add` or `set` object, in field order.
fn fields_of(v: &Value, set: bool) -> Result<Vec<(Field, Option<String>)>, String> {
    let o = v.as_object().ok_or("a part line is an object of fields: {part_number, maker, description, qty…}")?;
    let mut out = Vec::new();
    for (k, x) in o {
        let f =
            Field::parse(k).filter(|f| set || *f != Field::Kit).ok_or_else(|| format!("`{k}` is not a part line field. Use {}.", field_words()))?;
        out.push((f, json_text(x, k)?));
    }
    // Unit before qty, so the qty is checked against the line's new unit.
    out.sort_by_key(|(f, _)| *f);
    Ok(out)
}

/// The components a `partline` call names, each one a device or strip.
fn partline_targets(doc: &Document, params: &Value) -> Result<Vec<ComponentId>, String> {
    let cs: Vec<ComponentId> = if params.get("components").is_some() {
        id_list(params, "components", MAX_COMPONENTS)?
    } else {
        vec![opt_id(params, "component")?.ok_or("`component` or `components` (hex id) is required")?]
    };
    if cs.is_empty() {
        return Err("`components` is empty".into());
    }
    for c in &cs {
        if !doc.components.contains_key(c) {
            return Err(format!("No component {}.", c.hex()));
        }
        if !takes_lines(doc, *c) {
            return Err(PARTLINE_FOR_DEVICES.into());
        }
    }
    Ok(cs)
}

fn run_partline(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let v = with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let cs = partline_targets(doc, params)?;
        let mut ops = Vec::new();
        let mut result = Vec::new();
        let (label, echo) = if let Some(add) = params.get("add") {
            let kit = match params.get("kit") {
                None | Some(Value::Null) => None,
                Some(Value::String(k)) => plain("kit", Some(k), MAX_KIT_CHARS)?,
                Some(_) => return Err("`kit` must be text".into()),
            };
            let adds: Vec<&Value> = match add {
                Value::Array(a) => a.iter().collect(),
                o => vec![o],
            };
            if adds.is_empty() {
                return Err("`add` names no line".into());
            }
            if adds.len() > MAX_ADD_LINES {
                return Err(format!("At most {MAX_ADD_LINES} lines per call."));
            }
            let lines: Vec<Vec<(Field, Option<String>)>> = adds.iter().map(|a| fields_of(a, false)).collect::<Result<_, _>>()?;
            for c in &cs {
                let props = doc.components.get(c).map(|x| &x.properties).ok_or("no such component")?;
                let mut used = used_lines(props);
                let mut ns = Vec::new();
                for fields in &lines {
                    let n = next_line(&used)
                        .ok_or_else(|| format!("{} has {MAX_LINES} part lines; remove one first.", crate::devices::tag_words(doc, *c)))?;
                    used.insert(n);
                    let mut fields = fields.clone();
                    if let Some(k) = &kit {
                        fields.push((Field::Kit, Some(k.clone())));
                    }
                    if fields.iter().all(|(_, v)| v.as_deref().is_none_or(|t| t.trim().is_empty())) {
                        return Err("A part line needs at least one field: part_number, maker or description.".into());
                    }
                    let line = line_ops(doc, *c, n, &fields)?;
                    ops.extend(line);
                    ns.push(n);
                }
                result.push(json!({"component": c.hex(), "lines": ns}));
            }
            let lines_words = if lines.len() == 1 { "a part line".to_string() } else { format!("{} part lines", lines.len()) };
            let to = if cs.len() == 1 { crate::devices::tag_words(doc, cs[0]) } else { format!("{} devices", cs.len()) };
            match (&kit, lines.len()) {
                (Some(k), _) => (format!("Add kit {k}"), format!("Added kit {k} ({lines_words}) to {to}.")),
                (None, 1) => ("Add part line".to_string(), format!("Added {lines_words} to {to}.")),
                (None, _) => ("Add part lines".to_string(), format!("Added {lines_words} to {to}.")),
            }
        } else {
            let n = params.get("line").and_then(Value::as_u64).and_then(|n| u8::try_from(n).ok()).filter(|n| (1..=MAX_LINES).contains(n));
            let n = n.ok_or_else(|| format!("`line` (1 to {MAX_LINES}) is required with `set` or `remove`, or give `add`"))?;
            for c in &cs {
                let props = doc.components.get(c).map(|x| &x.properties).ok_or("no such component")?;
                if !used_lines(props).contains(&n) {
                    return Err(format!("{} has no part line {n}.", crate::devices::tag_words(doc, *c)));
                }
                if params.get("remove").and_then(Value::as_bool) == Some(true) {
                    let prefix = format!("{PART_PREFIX}{n}.");
                    for (k, v) in props.range(prefix.clone()..).take_while(|(k, _)| k.starts_with(&prefix)) {
                        ops.push(Op::SetProperty { object: ObjectRef::Component(*c), key: k.clone(), before: Some(v.clone()), after: None });
                    }
                } else if let Some(set) = params.get("set") {
                    let fields = fields_of(set, true)?;
                    ops.extend(line_ops(doc, *c, n, &fields)?);
                } else {
                    return Err("Give `add`, `set` or `remove: true`.".into());
                }
                result.push(json!({"component": c.hex(), "lines": [n]}));
            }
            if params.get("remove").and_then(Value::as_bool) == Some(true) {
                ("Remove part line".to_string(), format!("Removed part line {n}."))
            } else {
                ("Edit part line".to_string(), format!("Edited part line {n}."))
            }
        };
        if ops.is_empty() {
            return Ok(json!({"changed": false, "components": result}));
        }
        p.apply_edit(pg, "partline", &label, ops)?;
        let mut v = match result.as_slice() {
            [one] => one.clone(),
            _ => json!({"components": result}),
        };
        v["changed"] = json!(true);
        v["message"] = json!(echo);
        Ok(v)
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

/// The item fields `bomitem` takes, in the order they are checked (unit before qty).
const ITEM_FIELDS: [&str; 7] = ["description", "unit", "qty", "location", "part_number", "maker", ARTICLE_KEY];

/// The item fields of a `bomitem` object.
fn item_fields(v: &Value) -> Result<BTreeMap<&'static str, Option<String>>, String> {
    let o = v.as_object().ok_or("an item is an object of fields")?;
    let mut out = BTreeMap::new();
    for (k, x) in o {
        let f =
            ITEM_FIELDS.iter().find(|f| **f == k.as_str()).ok_or_else(|| format!("`{k}` is not an item field. Use {}.", ITEM_FIELDS.join(", ")))?;
        out.insert(*f, json_text(x, k)?);
    }
    Ok(out)
}

fn run_bomitem(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let v = with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        if let Some(item) = opt_id::<ComponentId>(params, "item")? {
            let comp = doc.components.get(&item).filter(|c| is_item_component(c)).ok_or_else(|| format!("{} is not an item.", item.hex()))?;
            if params.get("delete").and_then(Value::as_bool) == Some(true) {
                let words = item_words(doc, &comp.properties);
                let ops = vec![Op::DeleteComponent { id: item, component: comp.clone() }];
                p.apply_edit(pg, "bomitem", "Delete item", ops)?;
                return Ok(json!({"changed": true, "item": item.hex(), "message": format!("Deleted item {words}.")}));
            }
            let set = params.get("set").ok_or("Give `set: {field: value}` or `delete: true` with `item`.")?;
            let fields = item_fields(set)?;
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
            let mut pending = crate::designations::Pending::default();
            for key in ITEM_FIELDS {
                let Some(v) = fields.get(key) else { continue };
                let after = match (key, v.as_deref().map(str::trim).filter(|t| !t.is_empty())) {
                    // Brief 052: an item's location is an entry key of the project's list.
                    ("location", Some(t)) => Some(pending.store(doc, &mut p.ids, crate::designations::Aspect::Location, t, &mut ops)?),
                    _ => item_value(doc, key, v.as_deref(), metres, qty.as_deref())?,
                };
                let before = props.get(key).cloned();
                if before != after {
                    ops.push(Op::SetProperty { object: ObjectRef::Component(item), key: key.to_string(), before, after });
                }
            }
            if ops.is_empty() {
                return Ok(json!({"changed": false, "item": item.hex()}));
            }
            p.apply_edit(pg, "bomitem", "Edit item", ops)?;
            return Ok(json!({"changed": true, "item": item.hex(), "message": "Edited the item."}));
        }
        let fields = item_fields(params)?;
        if !fields.contains_key("description") {
            return Err(ITEM_NEEDS_DESCRIPTION.into());
        }
        let items = doc.components.values().filter(|c| is_item_component(c)).count();
        if items >= MAX_ITEMS {
            return Err(format!("This project has {items} items, the most a project holds."));
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
        let mut pending = crate::designations::Pending::default();
        for key in ITEM_FIELDS {
            if key == UNIT_KEY {
                continue;
            }
            let Some(v) = fields.get(key) else { continue };
            let x = match (key, v.as_deref().map(str::trim).filter(|t| !t.is_empty())) {
                ("location", Some(t)) => Some(pending.store(doc, &mut p.ids, crate::designations::Aspect::Location, t, &mut ops)?),
                _ => item_value(doc, key, v.as_deref(), metres, qty.as_deref())?,
            };
            if let Some(x) = x {
                props.insert(key.to_string(), x);
            }
        }
        let words = item_words(p.ledger.document(), &props);
        let id: ComponentId = p.ids.fresh();
        ops.push(Op::CreateComponent { id, component: Component { properties: props, definition: None } });
        p.apply_edit(pg, "bomitem", "Add item", ops)?;
        Ok(json!({"changed": true, "item": id.hex(), "message": format!("Added item {words}.")}))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

fn run_article(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let v = with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let pn = params.get("part_number").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty());
        let pn = pn.ok_or("`part_number` is required")?;
        let maker = match params.get("maker") {
            None | Some(Value::Null) => None,
            Some(Value::String(m)) => Some(m.trim()).filter(|m| !m.is_empty()),
            Some(_) => return Err("`maker` must be text".into()),
        };
        let value = match params.get("article_number") {
            Some(v) => json_text(v, "article_number")?,
            None => return Err("`article_number` is required (text, or null to clear)".into()),
        };
        let after = plain(ARTICLE_KEY, value.as_deref(), MAX_ARTICLE_CHARS)?;
        let targets = article_targets(doc, pn, maker);
        if targets.is_empty() {
            return Err(format!("No part {pn} in this project."));
        }
        if targets.len() > MAX_COMPONENTS * 4 {
            return Err(format!("{pn} is used {} times; at most {} per call.", targets.len(), MAX_COMPONENTS * 4));
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
        let what = match &after {
            Some(a) => format!("Set {a} on {n} part{} ({}).", if n == 1 { "" } else { "s" }, parts.join(", ")),
            None => format!("Cleared the article number of {n} part{} ({}).", if n == 1 { "" } else { "s" }, parts.join(", ")),
        };
        if ops.is_empty() {
            return Ok(json!({"changed": false, "parts": n, "message": what}));
        }
        p.apply_edit(pg, "article", "Set article number", ops)?;
        Ok(json!({"changed": true, "parts": n, "message": what}))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}
