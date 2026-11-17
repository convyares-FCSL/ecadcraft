//! The commands of the project's list of functions and locations (brief 052, design 052 §5.9):
//! `designations` (read), `designationadd`, `designationedit` (rename and describe),
//! `designationmerge` and `designationremove`.
//!
//! The list is project properties ([`crate::designations`], FP7). A rename is one `SetProperty`:
//! everything storing the entry's key shows the new code. A merge sets every user of one entry to
//! the other's key and removes the first, in one transaction. Every write is one transaction
//! through [`crate::project::Project::apply_edit`]; refusals say what to do instead and write
//! nothing.
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::designation`] decide, and the shim commits the ops and answers in JSON.

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::cmd::designation::{self as rules, EditInput};
use ecad_model::Op;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cmd::err;
use crate::cmd::input::{Command, Typed, nullable};
use crate::cmd::{on_any_page, on_project};
use crate::designations::{self, Aspect};

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[
    Typed::of::<DesignationsParams>("designations"),
    Typed::of::<AddParams>("designationadd"),
    Typed::of::<EditParams>("designationedit"),
    Typed::of::<MergeParams>("designationmerge"),
    Typed::of::<RemoveParams>("designationremove"),
];

/// The commands this module adds.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("designations", "Functions and Locations", crate::cmd::input::typed::<DesignationsParams>)
            .params("{aspect?: \"function\" | \"location\"} (read only: each entry's key, code, description and counts)")
            .noundo(),
        CommandSpec::new("designationadd", "Add Function or Location", crate::cmd::input::typed::<AddParams>)
            .params("{aspect: \"function\" | \"location\", code, description?} (one transaction)")
            .noundo(),
        CommandSpec::new("designationedit", "Rename Function or Location", crate::cmd::input::typed::<EditParams>)
            .params("{aspect, key, code?: the new code (a rename), description?: text | null} | {aspect, code: the entry, to: the new code, description?} (one transaction; a code another entry has is refused: merge instead)")
            .noundo(),
        CommandSpec::new("designationmerge", "Merge Functions or Locations", crate::cmd::input::typed::<MergeParams>)
            .params("{aspect, from: key | code, into: key | code} (every user of `from` takes `into`; `from` is removed; one transaction)")
            .noundo(),
        CommandSpec::new("designationremove", "Remove Function or Location", crate::cmd::input::typed::<RemoveParams>)
            .params("{aspect, key | code} (only an entry nothing uses) | {aspect?, unused: true} (every entry nothing uses) (one transaction)")
            .noundo(),
    ]
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
        Op::ModifyPage { before, after, .. } => before.change(after).display.iter().any(|k| Aspect::of_property(k).is_some()),
        _ => false,
    })
}

/// `designations {aspect?}` (read only): each entry's key, code, description and counts.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DesignationsParams {
    /// "function" or "location" (default: both).
    #[serde(default)]
    aspect: Option<String>,
}

impl Command for DesignationsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let only = rules::opt_aspect(self.aspect.as_deref()).map_err(|e| err(e.to_string()))?;
        on_any_page(s, |p, _| Ok(rows(p, only)))
    }
}

/// `designationadd {aspect, code, description?}`: a new entry of the list.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AddParams {
    /// "function" or "location".
    aspect: String,
    /// The code: "=F1", "+CP1".
    code: String,
    /// What it is.
    #[serde(default)]
    description: Option<String>,
}

impl Command for AddParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let a = rules::aspect(&self.aspect).map_err(|e| err(e.to_string()))?;
        on_project(s, |p, pg, _| {
            let plan = rules::plan_add(p.ledger.document(), &mut p.ids, a, &self.code, self.description.as_deref())?;
            p.apply_edit(pg, "designationadd", &plan.label, plan.ops)?;
            Ok(json!({"changed": true, "key": plan.key, "code": plan.code}))
        })
    }
}

/// `designationedit {aspect, key | code, code?, to?, description?}`: an entry renamed (everything
/// storing its key shows the new code) or described.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditParams {
    /// "function" or "location".
    aspect: String,
    /// The entry's key; with it, `code` is the new code.
    #[serde(default)]
    key: Option<String>,
    /// The new code (with `key`), or the entry's code (without).
    #[serde(default)]
    code: Option<String>,
    /// The new code (without `key`).
    #[serde(default)]
    to: Option<String>,
    /// The old name of `to`.
    #[serde(default)]
    new_code: Option<String>,
    /// The description, or null to clear it.
    #[serde(default, deserialize_with = "nullable")]
    description: Option<Option<String>>,
}

impl Command for EditParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let a = rules::aspect(&self.aspect).map_err(|e| err(e.to_string()))?;
        let edit = EditInput { key: self.key, code: self.code, to: self.to, new_code: self.new_code, description: self.description };
        on_project(s, |p, pg, _| {
            let plan = rules::plan_edit(p.ledger.document(), a, edit)?;
            if plan.ops.is_empty() {
                return Ok(json!({"changed": false, "key": plan.key, "code": plan.code}));
            }
            p.apply_edit(pg, "designationedit", &plan.label, plan.ops)?;
            Ok(json!({"changed": true, "key": plan.key, "code": plan.code}))
        })
    }
}

/// `designationmerge {aspect, from, into}` (Q11): every user of `from` takes `into`; `from` is
/// removed.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct MergeParams {
    /// "function" or "location".
    aspect: String,
    /// The entry merged away: its key or code.
    from: String,
    /// The entry kept: its key or code.
    into: String,
}

impl Command for MergeParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let a = rules::aspect(&self.aspect).map_err(|e| err(e.to_string()))?;
        on_project(s, |p, pg, _| {
            p.designation_uses.catch_up(&p.ledger);
            let m = rules::plan_merge(p.ledger.document(), &p.designation_uses, a, &self.from, &self.into)?;
            p.apply_edit(pg, "designationmerge", &m.label, m.ops)?;
            Ok(json!({"changed": true, "from": m.from, "into": m.into, "key": m.key, "moved": m.moved}))
        })
    }
}

/// `designationremove {aspect, key | code}`, or `{aspect?, unused: true}`: every entry nothing
/// uses, in one transaction (design 052 §9.4, **Remove unused**).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemoveParams {
    /// "function" or "location" (with `unused`, default both).
    #[serde(default)]
    aspect: Option<String>,
    /// The entry's key.
    #[serde(default)]
    key: Option<String>,
    /// The entry's code.
    #[serde(default)]
    code: Option<String>,
    /// Remove every entry nothing uses.
    #[serde(default)]
    unused: Option<bool>,
}

impl Command for RemoveParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        if self.unused == Some(true) {
            let only = rules::opt_aspect(self.aspect.as_deref()).map_err(|e| err(e.to_string()))?;
            return on_project(s, |p, pg, _| {
                p.designation_uses.catch_up(&p.ledger);
                let (ops, codes) = rules::plan_remove_unused(p.ledger.document(), &p.designation_uses, only);
                if codes.is_empty() {
                    return Ok(json!({"changed": false, "removed": []}));
                }
                let n = codes.len();
                p.apply_edit(pg, "designationremove", &format!("Remove {n} unused {}", if n == 1 { "entry" } else { "entries" }), ops)?;
                Ok(json!({"changed": true, "removed": codes}))
            });
        }
        let a = rules::required_aspect(self.aspect.as_deref()).map_err(|e| err(e.to_string()))?;
        on_project(s, |p, pg, _| {
            p.designation_uses.catch_up(&p.ledger);
            let (ops, code) = rules::plan_remove(p.ledger.document(), &p.designation_uses, a, self.key.as_deref(), self.code.as_deref())?;
            p.apply_edit(pg, "designationremove", &format!("Remove {} {code}", a.word()), ops)?;
            Ok(json!({"changed": true, "code": code}))
        })
    }
}
