//! The commands of the project's list of functions and locations (brief 052, design 052 §5.9):
//! `designations` (read), `designationadd`, `designationedit` (rename and describe),
//! `designationmerge` and `designationremove`.
//!
//! The list is project properties ([`crate::designations`], FP7). A rename is one `SetProperty`:
//! everything storing the entry's key shows the new code. A merge sets every user of one entry to
//! the other's key and removes the first, in one transaction. Every write is one transaction
//! through [`crate::project::Project::apply_edit`]; refusals say what to do instead and write
//! nothing.

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::{ComponentId, Document, ObjectRef, Op};
use serde_json::{Value, json};

use crate::cmd::{err, with_any_page, with_project};
use crate::designations::{self, Aspect, Entry};

/// The commands this module adds.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("designations", "Functions and Locations", run_designations)
            .params("{aspect?: \"function\" | \"location\"} (read only: each entry's key, code, description and counts)")
            .noundo(),
        CommandSpec::new("designationadd", "Add Function or Location", run_add)
            .params("{aspect: \"function\" | \"location\", code, description?} (one transaction)")
            .noundo(),
        CommandSpec::new("designationedit", "Rename Function or Location", run_edit)
            .params("{aspect, key, code?: the new code (a rename), description?: text | null} | {aspect, code: the entry, to: the new code, description?} (one transaction; a code another entry has is refused: merge instead)")
            .noundo(),
        CommandSpec::new("designationmerge", "Merge Functions or Locations", run_merge)
            .params("{aspect, from: key | code, into: key | code} (every user of `from` takes `into`; `from` is removed; one transaction)")
            .noundo(),
        CommandSpec::new("designationremove", "Remove Function or Location", run_remove)
            .params("{aspect, key | code} (only an entry nothing uses) | {aspect?, unused: true} (every entry nothing uses) (one transaction)")
            .noundo(),
    ]
}

fn aspect(params: &Value, required: bool) -> Result<Option<Aspect>, String> {
    match params.get("aspect") {
        None | Some(Value::Null) if !required => Ok(None),
        Some(Value::String(w)) => Aspect::of_word(w).map(Some).ok_or_else(|| "`aspect` is \"function\" or \"location\"".to_string()),
        _ => Err("`aspect` is required: \"function\" or \"location\"".into()),
    }
}

fn text(params: &Value, key: &str) -> Result<Option<String>, String> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(t)) => Ok(Some(t.clone())),
        Some(_) => Err(format!("`{key}` must be text")),
    }
}

/// The entry named by `params[field]` (a key or a code).
fn named(doc: &Document, a: Aspect, params: &Value, field: &str) -> Result<Entry, String> {
    let t = text(params, field)?.ok_or_else(|| format!("`{field}` (a key or a code such as {}) is required", designations::example(a)))?;
    designations::find(doc, a, &t).ok_or_else(|| format!("{} is not in the project's list of {}.", t.trim(), a.plural()))
}

/// The entry `key` or `code` names.
fn named_key_or_code(doc: &Document, a: Aspect, params: &Value) -> Result<Entry, String> {
    if params.get("key").is_some_and(|v| !v.is_null()) { named(doc, a, params, "key") } else { named(doc, a, params, "code") }
}

/// A description as typed: trimmed; empty clears it.
fn description_value(doc: &Document, t: Option<&str>) -> Result<Option<String>, String> {
    crate::devices::property_value(doc, "description", t)
}

fn set(key: String, before: Option<String>, after: Option<String>) -> Option<Op> {
    (before != after).then_some(Op::SetProperty { object: ObjectRef::Project, key, before, after })
}

/// A code refused because another entry has it.
fn taken(doc: &Document, a: Aspect, code: &str, except: Option<&str>) -> Option<Entry> {
    designations::entries(doc, a).into_iter().find(|e| e.code.eq_ignore_ascii_case(code) && Some(e.key.as_str()) != except)
}

/// The list with its counts, as `designations` answers (`only`: one aspect), for the command and
/// for Settings ▸ Project info (design 052 §9.4). Reads the use index, caught up first.
pub fn rows(p: &mut crate::project::Project, only: Option<Aspect>) -> Value {
    p.designation_uses.catch_up(&p.ledger);
    let doc = p.ledger.document();
    let mut out = serde_json::Map::new();
    for a in Aspect::ALL.into_iter().filter(|a| only.is_none_or(|o| o == *a)) {
        let default = doc.properties.get(a.property());
        let rows: Vec<Value> = designations::entries(doc, a)
            .into_iter()
            .map(|e| {
                let c = p.designation_uses.counts(doc, a, &e.key);
                json!({
                    "key": e.key, "code": e.code, "description": e.description,
                    "devices": c.devices, "overrides": c.overrides, "boxes": c.boxes, "pages": c.pages,
                    "default": default == Some(&e.key), "unused": c.unused(), "uses": c.words(),
                })
            })
            .collect();
        // Stored keys with no entry (design 052 §5.9: only merges of branches make them).
        let missing: Vec<&str> = p.designation_uses.keys(a).filter(|k| designations::code_of(doc, a, k).is_none()).collect();
        out.insert(a.plural().into(), json!({"entries": rows, "missing": missing}));
    }
    Value::Object(out)
}

/// Whether the transactions after the first `seen` of `ledger`'s log could change [`rows`]: an
/// op on the list, on a `function` / `location` / `location.<type>` value, or a component or page
/// created or deleted (design 052 §8.2a, the Settings card's update map).
pub fn rows_changed_since(ledger: &ecad_model::Ledger, seen: usize) -> bool {
    let log = ledger.log();
    if seen > log.len() {
        return true;
    }
    log.get(seen..).unwrap_or_default().iter().flat_map(|tx| tx.ops.iter()).any(|op| match op {
        Op::SetProperty { key, .. } => designations::is_list_property(key) || Aspect::of_property(key).is_some(),
        Op::CreateComponent { .. } | Op::DeleteComponent { .. } | Op::AddPage { .. } | Op::RemovePage { .. } => true,
        _ => false,
    })
}

/// `designations {aspect?}`.
fn run_designations(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let only = aspect(params, false).map_err(err)?;
    with_any_page(s, |p, _| Ok(rows(p, only)))
}

/// `designationadd {aspect, code, description?}`.
fn run_add(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let a = aspect(params, true).map_err(err)?.ok_or_else(|| err("`aspect` is required"))?;
    let typed = text(params, "code").map_err(err)?.ok_or_else(|| err("`code` is required, such as +CP1"))?;
    let description = text(params, "description").map_err(err)?;
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let code = designations::normalise(doc, a, &typed)?;
        if taken(doc, a, &code, None).is_some() {
            return Err(format!("{code} is already in the list."));
        }
        let id: ComponentId = p.ids.fresh();
        let key = id.hex();
        let mut ops =
            vec![Op::SetProperty { object: ObjectRef::Project, key: designations::code_property(a, &key), before: None, after: Some(code.clone()) }];
        if let Some(d) = description_value(doc, description.as_deref())? {
            ops.push(Op::SetProperty { object: ObjectRef::Project, key: designations::description_property(a, &key), before: None, after: Some(d) });
        }
        p.apply_edit(pg, "designationadd", &format!("Add {} {code}", a.word()), ops)?;
        Ok(json!({"changed": true, "key": key, "code": code}))
    })
}

/// `designationedit {aspect, key | code, code?, description?}`.
fn run_edit(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let a = aspect(params, true).map_err(err)?.ok_or_else(|| err("`aspect` is required"))?;
    // With `key`, `code` is the new code; without it, `code` names the entry and `to` (or
    // `new_code`) is the new one.
    let has_key = params.get("key").is_some_and(|v| !v.is_null());
    let new_code =
        if has_key { text(params, "code") } else { text(params, "to").and_then(|t| if t.is_some() { Ok(t) } else { text(params, "new_code") }) }
            .map_err(err)?;
    let description = params.get("description").map(|v| v.as_str().map(str::to_string));
    if matches!(description, Some(None)) && params.get("description").is_some_and(|v| !v.is_null()) {
        return Err(err("`description` must be text, or null to clear it"));
    }
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let e = named_key_or_code(doc, a, params)?;
        let mut ops = Vec::new();
        let mut label = format!("Describe {} {}", a.word(), e.code);
        let mut code = e.code.clone();
        if let Some(t) = new_code.as_deref() {
            code = designations::normalise(doc, a, t)?;
            if let Some(other) = taken(doc, a, &code, Some(&e.key)) {
                return Err(format!("{} is already in the list. Merge {} into it instead?", other.code, e.code));
            }
            if code != e.code {
                label = format!("Rename {} {} → {code}", a.word(), e.code);
                ops.extend(set(designations::code_property(a, &e.key), Some(e.code.clone()), Some(code.clone())));
            }
        }
        if let Some(d) = description {
            let after = description_value(doc, d.as_deref())?;
            ops.extend(set(
                designations::description_property(a, &e.key),
                doc.properties.get(&designations::description_property(a, &e.key)).cloned(),
                after,
            ));
        }
        if ops.is_empty() {
            return Ok(json!({"changed": false, "key": e.key, "code": code}));
        }
        p.apply_edit(pg, "designationedit", &label, ops)?;
        Ok(json!({"changed": true, "key": e.key, "code": code}))
    })
}

/// The value of property `prop` on object `o`.
fn property_of(doc: &Document, o: ObjectRef, prop: &str) -> Option<String> {
    match o {
        ObjectRef::Component(c) => doc.components.get(&c).and_then(|x| x.properties.get(prop)).cloned(),
        ObjectRef::Page(pg) => doc.pages.get(&pg).and_then(|x| x.properties.get(prop)).cloned(),
        ObjectRef::Project => doc.properties.get(prop).cloned(),
        _ => None,
    }
}

/// `designationmerge {aspect, from, into}` (Q11).
fn run_merge(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let a = aspect(params, true).map_err(err)?.ok_or_else(|| err("`aspect` is required"))?;
    with_project(s, |p, pg, _| {
        p.designation_uses.catch_up(&p.ledger);
        let doc = p.ledger.document();
        let from = named(doc, a, params, "from")?;
        let into = named(doc, a, params, "into")?;
        if from.key == into.key {
            return Err(format!("{} and {} are the same entry.", from.code, into.code));
        }
        let counts = p.designation_uses.counts(doc, a, &from.key);
        let mut ops = Vec::new();
        for (o, prop) in p.designation_uses.users(a, &from.key).map(|u| u.refs.clone()).unwrap_or_default() {
            let before = property_of(doc, o, &prop);
            if before.as_deref() == Some(from.key.as_str()) {
                ops.push(Op::SetProperty { object: o, key: prop, before, after: Some(into.key.clone()) });
            }
        }
        if into.description.is_none()
            && let Some(d) = &from.description
        {
            ops.push(Op::SetProperty {
                object: ObjectRef::Project,
                key: designations::description_property(a, &into.key),
                before: None,
                after: Some(d.clone()),
            });
        }
        ops.push(Op::SetProperty {
            object: ObjectRef::Project,
            key: designations::code_property(a, &from.key),
            before: Some(from.code.clone()),
            after: None,
        });
        if let Some(d) = &from.description {
            ops.push(Op::SetProperty {
                object: ObjectRef::Project,
                key: designations::description_property(a, &from.key),
                before: Some(d.clone()),
                after: None,
            });
        }
        let label = format!("Merge {} {} into {} ({})", a.word(), from.code, into.code, counts.words());
        p.apply_edit(pg, "designationmerge", &label, ops)?;
        Ok(json!({"changed": true, "from": from.code, "into": into.code, "key": into.key, "moved": counts.words()}))
    })
}

/// The ops removing entry `e`.
fn remove_ops(a: Aspect, e: &Entry) -> Vec<Op> {
    let mut ops =
        vec![Op::SetProperty { object: ObjectRef::Project, key: designations::code_property(a, &e.key), before: Some(e.code.clone()), after: None }];
    if let Some(d) = &e.description {
        ops.push(Op::SetProperty {
            object: ObjectRef::Project,
            key: designations::description_property(a, &e.key),
            before: Some(d.clone()),
            after: None,
        });
    }
    ops
}

/// `designationremove {aspect, key | code}`, or `{aspect?, unused: true}`: every entry nothing
/// uses, in one transaction (design 052 §9.4, **Remove unused**).
fn run_remove(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    if params.get("unused").and_then(Value::as_bool) == Some(true) {
        let only = aspect(params, false).map_err(err)?;
        return with_project(s, |p, pg, _| {
            p.designation_uses.catch_up(&p.ledger);
            let doc = p.ledger.document();
            let mut ops = Vec::new();
            let mut codes = Vec::new();
            for a in Aspect::ALL.into_iter().filter(|a| only.is_none_or(|o| o == *a)) {
                for e in designations::entries(doc, a) {
                    if p.designation_uses.counts(doc, a, &e.key).unused() {
                        ops.extend(remove_ops(a, &e));
                        codes.push(e.code);
                    }
                }
            }
            if codes.is_empty() {
                return Ok(json!({"changed": false, "removed": []}));
            }
            let n = codes.len();
            p.apply_edit(pg, "designationremove", &format!("Remove {n} unused {}", if n == 1 { "entry" } else { "entries" }), ops)?;
            Ok(json!({"changed": true, "removed": codes}))
        });
    }
    let a = aspect(params, true).map_err(err)?.ok_or_else(|| err("`aspect` is required"))?;
    with_project(s, |p, pg, _| {
        p.designation_uses.catch_up(&p.ledger);
        let doc = p.ledger.document();
        let e = named_key_or_code(doc, a, params)?;
        let counts = p.designation_uses.counts(doc, a, &e.key);
        if !counts.unused() {
            return Err(format!("{} is used by {}. Give them another {} first.", e.code, counts.words(), a.word()));
        }
        let ops = remove_ops(a, &e);
        p.apply_edit(pg, "designationremove", &format!("Remove {} {}", a.word(), e.code), ops)?;
        Ok(json!({"changed": true, "code": e.code}))
    })
}
