//! The commands of brief 052 (design 052 §5.7): `setlocation` and its alias `setfunction`, and the
//! route `setprop` takes for a `function`, `location` or `location.<type>` value.
//!
//! A typed code is stored as its entry's key in the project's list ([`crate::designations`]); an
//! unknown code adds its entry in the same transaction. Every write is one transaction through
//! [`crate::project::Project::apply_edit`]; refusals say what to do instead and write nothing.
//! Programmatic calls never prompt.
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::location`] decide, and the shim commits the ops, keeps the waiting
//! conflicts and answers in JSON.

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::cmd::location::{self as rules, Answer, MAX_DEVICES, Pick};
use ecad_model::ComponentId;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cmd::input::{Bounded, Command, OrNull, Typed, nullable};
use crate::cmd::{MAX_HANDLES, err, on_any_page, on_project};
use crate::designations::Aspect;

pub use rules::aspect_property;

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] =
    &[Typed::of::<ConflictsParams>("conflicts"), Typed::of::<SetlocationParams>("setlocation"), Typed::of::<SetfunctionParams>("setfunction")];

/// The commands of brief 052: these and [`crate::cmd::designation::specs`] (one lane line).
pub fn specs() -> Vec<CommandSpec> {
    let mut v = own_specs();
    v.extend(crate::cmd::designation::specs());
    v
}

fn own_specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("conflicts", "Location Conflicts", crate::cmd::input::typed::<ConflictsParams>)
            .params(
                "{answer?: \"move\" | \"leave\", components?: [id] (Choose: only these)} (the devices the last placement, move or box left with another function or location; no answer lists them; `move` sets them in one transaction; `leave` writes nothing)",
            )
            .noundo(),
        CommandSpec::new("setlocation", "Set Location", crate::cmd::input::typed::<SetlocationParams>)
            .params(
                "{component: id | components: [id], value: code | null, aspect?: \"location\" (default) | \"function\", type?: \"socket\" | \"plug\" | \"terminal\" | a symbol type (an override, location only)} \
(one transaction; a code not in the project's list adds its entry; null clears it)",
            )
            .noundo(),
        CommandSpec::new("setfunction", "Set Function", crate::cmd::input::typed::<SetfunctionParams>)
            .params("{component: id | components: [id], value: code | null} (as setlocation with aspect \"function\")")
            .noundo(),
    ]
}

/// Run a set on the project and answer in JSON.
pub(crate) fn apply(s: &mut Session, objects: Vec<ComponentId>, prop: String, text: Option<String>, command: &str) -> cadcraft_engine::Result<Value> {
    let a = rules::aspect_of(&prop).map_err(|e| err(e.to_string()))?;
    let v = on_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let plan = rules::plan_set(doc, &mut p.ids, &objects, &prop, text.as_deref())?;
        let planned = &plan.planned;
        let mut v = json!({
            "changed": planned.changed > 0,
            "devices": planned.changed,
            a.word(): planned.value.as_ref().map(|(_, c)| c.clone()),
            format!("{}_key", a.word()): planned.value.as_ref().map(|(k, _)| k.clone()),
        });
        if planned.changed == 0 {
            return Ok(v);
        }
        if let Some(proposed) = &plan.proposals {
            v["proposals"] = crate::proposals::json(doc, proposed);
        }
        let words = crate::proposals::conflict_words(doc, &plan.conflicts);
        let added = planned.added.clone();
        p.apply_edit(pg, command, &plan.label, plan.ops)?;
        p.conflicts = plan.conflicts;
        let mut message: Vec<String> = added.into_iter().collect();
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

/// `setlocation {component | components, value, aspect?, type?}`: a device's (or box's) location
/// or function, as a code of the project's list (one transaction; null clears it).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SetlocationParams {
    /// One device or box.
    #[serde(default)]
    component: Option<ComponentId>,
    /// Several (at most 10000; one named twice is set once).
    #[serde(default)]
    components: Option<Bounded<ComponentId, 0, MAX_DEVICES>>,
    /// The code, such as +CP1, or null to clear it.
    #[serde(deserialize_with = "nullable")]
    #[schemars(required, with = "OrNull<String>")]
    value: Option<Option<String>>,
    /// location (the default) or function.
    #[serde(default)]
    aspect: Option<String>,
    /// Location only: an override for one symbol type (socket, plug, terminal or another type).
    #[serde(default)]
    r#type: Option<String>,
}

impl Command for SetlocationParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let a = rules::aspect(self.aspect.as_deref()).map_err(|e| err(e.to_string()))?;
        let objects = rules::objects(self.component, self.components.map(|c| c.0)).map_err(|e| err(e.to_string()))?;
        let prop = rules::property_for(a, self.r#type.as_deref()).map_err(|e| err(e.to_string()))?;
        let command = "setlocation";
        apply(s, objects, prop, self.value.flatten(), command)
    }
}

/// `setfunction {component | components, value}`: as `setlocation` with aspect function.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SetfunctionParams {
    /// One device or box.
    #[serde(default)]
    component: Option<ComponentId>,
    /// Several (at most 10000; one named twice is set once).
    #[serde(default)]
    components: Option<Bounded<ComponentId, 0, MAX_DEVICES>>,
    /// The code, such as =F1, or null to clear it.
    #[serde(deserialize_with = "nullable")]
    #[schemars(required, with = "OrNull<String>")]
    value: Option<Option<String>>,
    /// Not taken: a function has no overrides (a device's parts share its function).
    #[serde(default)]
    r#type: Option<String>,
}

impl Command for SetfunctionParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        if self.r#type.is_some() {
            return Err(err(rules::NO_FUNCTION_OVERRIDES));
        }
        let objects = rules::objects(self.component, self.components.map(|c| c.0)).map_err(|e| err(e.to_string()))?;
        apply(s, objects, Aspect::Function.property().to_string(), self.value.flatten(), "setfunction")
    }
}

/// `conflicts {answer?, components?, aspect?}` (design 052 §5.5): the waiting conflicts; Move,
/// Half or Leave. `components` and `aspect` choose rows (Choose: one row is one device and one
/// aspect). Move and Half act only on the rows that still hold (an undo may have taken them away).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConflictsParams {
    /// move, half or leave; none lists the waiting conflicts.
    #[serde(default)]
    answer: Option<String>,
    /// Choose: only these devices.
    #[serde(default)]
    components: Option<Bounded<ComponentId, 0, MAX_HANDLES>>,
    /// Choose: only this aspect, location or function.
    #[serde(default)]
    aspect: Option<String>,
}

impl Command for ConflictsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let answer = rules::answer(self.answer.as_deref()).map_err(|e| err(e.to_string()))?;
        let aspect = match self.aspect.as_deref() {
            None => None,
            Some(w) => Some(Aspect::of_word(w).ok_or_else(|| err(rules::ASPECT_WORDS))?),
        };
        let pick = Pick { only: self.components.map(|c| c.0), aspect };
        let v = match answer {
            Answer::List => on_any_page(s, |p, _| {
                let doc = p.ledger.document();
                Ok(
                    json!({"conflicts": crate::proposals::json(doc, &crate::proposals::Planned { conflicts: p.conflicts.clone(), ..Default::default() })}),
                )
            })?,
            Answer::Leave => on_any_page(s, |p, _| {
                let words = crate::proposals::left_words(p.ledger.document(), &p.conflicts);
                p.conflicts.clear();
                Ok(json!({"changed": false, "message": words}))
            })?,
            Answer::Half => on_project(s, |p, pg, _| {
                let (ops, label) = rules::plan_half(p.ledger.document(), &p.conflicts, &pick)?;
                p.apply_edit(pg, "conflicts", &label, ops)?;
                Ok(json!({"changed": true, "message": format!("{label}.")}))
            })?,
            Answer::Move => on_project(s, |p, pg, _| {
                let plan = rules::plan_move(p.ledger.document(), &p.conflicts, &pick);
                if plan.ops.is_empty() {
                    p.conflicts = plan.still_valid;
                    return Ok(json!({"changed": false, "message": "Nothing to move: those devices no longer differ, or the entry is gone."}));
                }
                p.apply_edit(pg, "conflicts", &plan.label, plan.ops)?;
                Ok(json!({"changed": true, "message": format!("{}.", plan.label)}))
            })?,
        };
        if let Some(m) = v.get("message").and_then(Value::as_str).filter(|m| !m.is_empty()) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}
