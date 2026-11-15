//! The commands of brief 052 (design 052 §5.7): `setlocation` and its alias `setfunction`, and the
//! route `setprop` takes for a `function`, `location` or `location.<type>` value.
//!
//! A typed code is stored as its entry's key in the project's list ([`crate::designations`]); an
//! unknown code adds its entry in the same transaction. Every write is one transaction through
//! [`crate::project::Project::apply_edit`]; refusals say what to do instead and write nothing.
//! Programmatic calls never prompt.

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::{ComponentId, Document, IdAllocator, ObjectRef, Op};
use serde_json::{Value, json};

use crate::commands::{MAX_HANDLES, err, id_list, opt_id, with_project};
use crate::designations::{self, Aspect};

/// The commands of brief 052: these and [`crate::designation_cmds::specs`] (one lane line).
pub fn specs() -> Vec<CommandSpec> {
    let mut v = own_specs();
    v.extend(crate::designation_cmds::specs());
    v
}

fn own_specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("conflicts", "Location Conflicts", run_conflicts)
            .params(
                "{answer?: \"move\" | \"leave\", components?: [id] (Choose: only these)} (the devices the last placement, move or box left with another function or location; no answer lists them; `move` sets them in one transaction; `leave` writes nothing)",
            )
            .noundo(),
        CommandSpec::new("setlocation", "Set Location", run_setlocation)
            .params(
                "{component: id | components: [id], value: code | null, aspect?: \"location\" (default) | \"function\", type?: \"socket\" | \"plug\" | \"terminal\" | a symbol type (an override, location only)} \
(one transaction; a code not in the project's list adds its entry; null clears it)",
            )
            .noundo(),
        CommandSpec::new("setfunction", "Set Function", run_setfunction)
            .params("{component: id | components: [id], value: code | null} (as setlocation with aspect \"function\")")
            .noundo(),
    ]
}

/// The property `key` names when it is an aspect property, with the retired connector half key
/// `half.<s|p>.location` read as the override `location.socket` / `location.plug` (FP4).
pub fn aspect_property(key: &str) -> Option<String> {
    let key = key.trim();
    match key {
        "half.s.location" => return Some("location.socket".into()),
        "half.p.location" => return Some("location.plug".into()),
        _ => {}
    }
    Aspect::of_property(key).map(|_| key.to_string())
}

/// What a planned set does.
#[derive(Debug, Default)]
pub struct Planned {
    pub ops: Vec<Op>,
    /// The objects changed.
    pub changed: usize,
    /// The stored key and its code (`None` when cleared).
    pub value: Option<(String, String)>,
    /// The echo for entries added to the list.
    pub added: Option<String>,
}

/// The ops setting property `prop` (an aspect property) of `objects` to the code `text` (`None`
/// clears it): devices, or boxes (`function` and `location` only).
///
/// # Errors
/// No such object; an override on a function, a box or a strip; an override type that is not one;
/// a box left with neither value; a code that is not valid.
pub fn plan(doc: &Document, ids: &mut IdAllocator, objects: &[ComponentId], prop: &str, text: Option<&str>) -> Result<Planned, String> {
    // The type's own refusals first: `location.head` and `location.<bogus>` are no aspect
    // property at all.
    if let Some(t) = prop.strip_prefix("location.") {
        if t == "head" {
            return Err("The head is the device; set the device's location.".into());
        }
        if !crate::locations::override_type_ok(t) {
            return Err(format!("`type` is socket, plug or a symbol type such as terminal, not \"{t}\"."));
        }
    }
    let a = Aspect::of_property(prop).ok_or("not a function or location property")?;
    let mut out = Planned::default();
    let mut pending = designations::Pending::default();
    let key = match text.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => {
            let k = pending.store(doc, ids, a, t, &mut out.ops)?;
            let code = designations::code_of(doc, a, &k).or_else(|| pending.new_codes.last().map(|(_, c)| c.clone())).unwrap_or_default();
            out.value = Some((k.clone(), code));
            Some(k)
        }
        None => None,
    };
    for c in objects {
        let comp = doc.components.get(c).ok_or("no such device")?;
        let is_box = crate::locations::is_box_component(comp);
        if prop != a.property() {
            if is_box {
                return Err("A box has a function and a location, not one per symbol type.".into());
            }
            if crate::strips::is_strip(doc, *c) {
                return Err("A strip's positions and levels are where the strip is; set the strip's location (a split strip is two strips).".into());
            }
        }
        if is_box && key.is_none() {
            let other = match a {
                Aspect::Function => Aspect::Location,
                Aspect::Location => Aspect::Function,
            };
            if !comp.properties.contains_key(other.property()) {
                return Err(crate::locations::NO_VALUE.into());
            }
        }
        let before = comp.properties.get(prop).cloned();
        if before != key {
            out.ops.push(Op::SetProperty { object: ObjectRef::Component(*c), key: prop.to_string(), before, after: key.clone() });
            out.changed += 1;
        }
    }
    out.added = pending.words();
    if out.changed == 0 {
        out.ops.clear();
    }
    Ok(out)
}

/// The transaction label: `Set location +CP1`, `Set function =F1 (3 devices)`, `Clear location`.
fn label(a: Aspect, value: Option<&(String, String)>, n: usize) -> String {
    let what = match value {
        Some((_, code)) => format!("Set {} {code}", a.word()),
        None => format!("Clear {}", a.word()),
    };
    if n > 1 { format!("{what} ({n} devices)") } else { what }
}

/// Run a planned set on the project and answer in JSON.
fn apply(s: &mut Session, objects: Vec<ComponentId>, prop: String, text: Option<String>, command: &str) -> cadcraft_engine::Result<Value> {
    let a = (if prop.starts_with("location.") { Some(Aspect::Location) } else { Aspect::of_property(&prop) })
        .ok_or_else(|| err("not a function or location property"))?;
    let v = with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let planned = plan(doc, &mut p.ids, &objects, &prop, text.as_deref())?;
        let mut v = json!({
            "changed": planned.changed > 0,
            "devices": planned.changed,
            a.word(): planned.value.as_ref().map(|(_, c)| c.clone()),
            format!("{}_key", a.word()): planned.value.as_ref().map(|(k, _)| k.clone()),
        });
        if planned.changed == 0 {
            return Ok(v);
        }
        // Brief 052d (design 052 §5.6): a box given another entry proposes it for the devices
        // inside: unset ones take it in this transaction; those at the box's old value are asked.
        let mut ops = planned.ops;
        let mut conflicts = Vec::new();
        if let ([b], Some((key, code))) = (objects.as_slice(), planned.value.as_ref())
            && prop == a.property()
            && let Some(lb) =
                doc.representations_of(ecad_model::Subject::Component(*b)).find_map(|(rid, r)| crate::locations::box_of_rep(doc, rid, r))
        {
            let spots = crate::proposals::spots_inside(doc, &lb);
            let proposed = crate::proposals::plan_change(doc, a, &spots, lb.key(a), key, code, &crate::proposals::Mode::Propose);
            ops.extend(proposed.ops);
            v["proposals"] = crate::proposals::json(
                doc,
                &crate::proposals::Planned { set: proposed.set, conflicts: proposed.conflicts.clone(), ..Default::default() },
            );
            conflicts = proposed.conflicts;
        }
        let words = crate::proposals::conflict_words(doc, &conflicts);
        p.apply_edit(pg, command, &label(a, planned.value.as_ref(), planned.changed), ops)?;
        p.conflicts = conflicts;
        let mut message: Vec<String> = planned.added.into_iter().collect();
        message.extend(words);
        if !message.is_empty() {
            v["message"] = json!(message.join("\n"));
        }
        Ok(v)
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

/// Most components one `setlocation` names (tester 052: a list over it is refused before any
/// work).
pub const MAX_DEVICES: usize = 10_000;

/// The components named, each once, in the order first named (tester 052, finding 4: a device
/// named twice is set once, not refused).
fn objects(params: &Value) -> Result<Vec<ComponentId>, String> {
    if params.get("components").is_some() {
        let mut seen = std::collections::BTreeSet::new();
        return Ok(id_list::<ComponentId>(params, "components", MAX_DEVICES)?.into_iter().filter(|c| seen.insert(*c)).collect());
    }
    opt_id::<ComponentId>(params, "component")?.map(|c| vec![c]).ok_or_else(|| "`component` or `components` (hex ids) is required".to_string())
}

fn text_value(params: &Value) -> Result<Option<String>, String> {
    match params.get("value") {
        Some(Value::Null) => Ok(None),
        Some(Value::String(v)) => Ok(Some(v.clone())),
        Some(_) => Err("`value` must be a code such as +CP1, or null to clear it".into()),
        None => Err("`value` is required (a code such as +CP1, or null to clear it)".into()),
    }
}

fn run_with(s: &mut Session, params: &Value, a: Aspect, command: &str) -> cadcraft_engine::Result<Value> {
    let objects = objects(params).map_err(err)?;
    let text = text_value(params).map_err(err)?;
    // A `type` that is not text is refused, never read as no type (tester 052, finding 3).
    let ty = match params.get("type") {
        None | Some(Value::Null) => None,
        Some(Value::String(t)) => Some(t.trim()).filter(|t| !t.is_empty()),
        Some(_) => return Err(err("`type` is text: socket, plug or a symbol type such as terminal")),
    };
    let prop = match ty {
        None => a.property().to_string(),
        Some(_) if a == Aspect::Function => return Err(err("A function has no overrides: a device's parts share its function.")),
        Some(t) => format!("location.{}", t.to_ascii_lowercase()),
    };
    apply(s, objects, prop, text, command)
}

/// `setlocation {component | components, value, aspect?, type?}`.
fn run_setlocation(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let a = match params.get("aspect").and_then(Value::as_str) {
        None => Aspect::Location,
        Some(w) => Aspect::of_word(w).ok_or_else(|| err("`aspect` is \"location\" or \"function\""))?,
    };
    run_with(s, params, a, "setlocation")
}

/// `setfunction {component | components, value}`.
fn run_setfunction(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    if params.get("type").is_some_and(|t| !t.is_null()) {
        return Err(err("A function has no overrides: a device's parts share its function."));
    }
    run_with(s, params, Aspect::Function, "setfunction")
}

/// `conflicts {answer?, components?}` (design 052 §5.5): the waiting conflicts; Move or Leave.
fn run_conflicts(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let answer = params.get("answer").and_then(Value::as_str).map(|a| a.trim().to_ascii_lowercase());
    let only: Option<Vec<ComponentId>> =
        if params.get("components").is_some() { Some(id_list(params, "components", MAX_HANDLES).map_err(err)?) } else { None };
    let v = match answer.as_deref() {
        None => crate::commands::with_any_page(s, |p, _| {
            let doc = p.ledger.document();
            Ok(json!({"conflicts": crate::proposals::json(doc, &crate::proposals::Planned { conflicts: p.conflicts.clone(), ..Default::default() })}))
        })?,
        Some("leave") => crate::commands::with_any_page(s, |p, _| {
            let doc = p.ledger.document();
            let words = crate::proposals::left_words(doc, &p.conflicts);
            p.conflicts.clear();
            Ok(json!({"changed": false, "message": words}))
        })?,
        Some("move") => with_project(s, |p, pg, _| {
            let chosen: Vec<crate::proposals::Conflict> =
                p.conflicts.iter().filter(|c| only.as_ref().is_none_or(|o| o.contains(&c.component))).cloned().collect();
            let doc = p.ledger.document();
            let ops = crate::proposals::move_ops(doc, &chosen);
            if ops.is_empty() {
                p.conflicts.clear();
                return Ok(json!({"changed": false}));
            }
            let label = crate::proposals::move_label(&chosen);
            p.apply_edit(pg, "conflicts", &label, ops)?;
            Ok(json!({"changed": true, "message": format!("{label}.")}))
        })?,
        Some(_) => return Err(err("`answer` is \"move\" or \"leave\"")),
    };
    if let Some(m) = v.get("message").and_then(Value::as_str).filter(|m| !m.is_empty()) {
        s.echo(m.to_string());
    }
    Ok(v)
}

/// `setprop` of an aspect property (`prop` from [`aspect_property`]) on one component or several.
pub(crate) fn run_setprop(s: &mut Session, params: &Value, prop: String) -> cadcraft_engine::Result<Value> {
    let objects = objects(params).map_err(err)?;
    let text = text_value(params).map_err(err)?;
    apply(s, objects, prop, text, "setprop")
}
