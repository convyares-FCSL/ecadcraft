//! The rules of the commands of the project's list of functions and locations (design
//! Adding, renaming and describing, merging and removing entries. Each function reads
//! the document (and the use index, [`Uses`], caught up by the caller) and returns the ops to
//! commit and what to report. The commands are generic ([`crate::commands::run`]).

use ecad_model::{Document, IdAllocator, ListDescription, ObjectRef, Op};

use crate::locations::designations::{self, Aspect, Entry, Uses};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::input::nullable;
use crate::commands::run::{Access, Cx, Generic, Outcome, Pages};
use crate::error::CmdError;

/// The aspect a call must name where its other fields allow none (`designationremove`).
///
/// # Errors
/// None named.
pub(crate) fn required_aspect(aspect: Option<Aspect>) -> Result<Aspect, CmdError> {
    aspect.ok_or(CmdError::input("`aspect` is required: \"function\" or \"location\""))
}

/// The entry `text` (a key or a code) names; `field` is the parameter it came from.
fn named(doc: &Document, a: Aspect, text: Option<&str>, field: &str) -> Result<Entry, CmdError> {
    let t = text.ok_or_else(|| CmdError::input(format!("`{field}` (a key or a code such as {}) is required", designations::example(a))))?;
    designations::find(doc, a, t).ok_or_else(|| CmdError::refused(format!("{} is not in the project's list of {}.", t.trim(), a.plural())))
}

/// The entry `key`, else `code`, names.
///
/// # Errors
/// Neither given, or no entry has it.
pub(crate) fn named_key_or_code(doc: &Document, a: Aspect, key: Option<&str>, code: Option<&str>) -> Result<Entry, CmdError> {
    if key.is_some() { named(doc, a, key, "key") } else { named(doc, a, code, "code") }
}

/// A description as typed: trimmed; empty clears it.
fn description_value(doc: &Document, t: Option<&str>) -> Result<Option<ListDescription>, CmdError> {
    crate::devices::json::property_value(doc, "description", t)?.map(|d| designations::list_description(&d)).transpose()
}

/// The `ModifyProject` from `doc`'s project to `after`, if they differ.
fn modify(doc: &Document, after: ecad_model::Project) -> Option<Op> {
    (*doc.project() != after).then(|| Op::ModifyProject { before: doc.project().clone(), after })
}

/// A code refused because another entry has it.
fn taken(doc: &Document, a: Aspect, code: &str, except: Option<&str>) -> Option<Entry> {
    designations::entries(doc, a).into_iter().find(|e| e.code.eq_ignore_ascii_case(code) && Some(e.key.as_str()) != except)
}

/// What a list command records.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ListPlan {
    /// Empty when nothing changes.
    pub ops: Vec<Op>,
    /// The transaction's label.
    pub label: String,
    /// The entry's key.
    pub key: String,
    /// Its code after the change.
    pub code: String,
}

/// `designationadd`: a new entry with code `typed` and an optional description.
///
/// # Errors
/// A code that does not read or another entry has, or a description that does not read.
pub(crate) fn plan_add(doc: &Document, ids: &mut IdAllocator, a: Aspect, typed: &str, description: Option<&str>) -> Result<ListPlan, CmdError> {
    let code = designations::normalise(doc, a, typed)?;
    if taken(doc, a, &code, None).is_some() {
        return Err(CmdError::refused(format!("{code} is already in the list.")));
    }
    let typed = designations::list_code(&code)?;
    let id: u128 = ids.fresh();
    let key = format!("{id:032x}");
    let after = designations::with_new_entry(doc.project(), a, id, typed, description_value(doc, description)?);
    let ops = modify(doc, after).into_iter().collect();
    Ok(ListPlan { ops, label: format!("Add {} {code}", a.word()), key, code })
}

/// A `designationedit` call, as given.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct EditInput {
    /// The entry's key; with it, `code` is the new code.
    pub key: Option<String>,
    /// The new code (with `key`), or the entry (without).
    pub code: Option<String>,
    /// The new code (without `key`).
    pub to: Option<String>,
    /// The old name of `to`.
    pub new_code: Option<String>,
    /// The new description: `Some(None)` clears it.
    pub description: Option<Option<String>>,
}

/// `designationedit`: rename an entry (one `SetProperty`: everything storing its key shows the new
/// code) and describe it.
///
/// # Errors
/// No entry named, a code that does not read or another entry has (merge instead), or a
/// description that does not read.
pub(crate) fn plan_edit(doc: &Document, a: Aspect, edit: EditInput) -> Result<ListPlan, CmdError> {
    // With `key`, `code` is the new code; without it, `code` names the entry and `to` (or
    // `new_code`) is the new one.
    let (named_code, new_code) = if edit.key.is_some() { (None, edit.code) } else { (edit.code, edit.to.or(edit.new_code)) };
    let e = named_key_or_code(doc, a, edit.key.as_deref(), named_code.as_deref())?;
    let mut label = format!("Describe {} {}", a.word(), e.code);
    let mut code = e.code.clone();
    if let Some(t) = new_code.as_deref() {
        code = designations::normalise(doc, a, t)?;
        if let Some(other) = taken(doc, a, &code, Some(&e.key)) {
            return Err(CmdError::refused(format!("{} is already in the list. Merge {} into it instead?", other.code, e.code)));
        }
        if code != e.code {
            label = format!("Rename {} {} → {code}", a.word(), e.code);
        }
    }
    let description = match edit.description {
        Some(d) => description_value(doc, d.as_deref())?,
        None => e.description.as_deref().map(designations::list_description).transpose()?,
    };
    let after = designations::with_entry(doc.project(), a, &e.key, designations::list_code(&code)?, description);
    let ops = modify(doc, after).into_iter().collect();
    Ok(ListPlan { ops, label, key: e.key, code })
}

/// The value of property `prop` (an override, `location.<type>`) on device `o` (pages, the
/// project and a device's own value are typed ids).
fn property_of(doc: &Document, o: ObjectRef, prop: &str) -> Option<String> {
    match o {
        ObjectRef::Device(c) => doc.devices().get(&c).and_then(|x| x.properties.get(prop)).cloned(),
        _ => None,
    }
}

/// What `designationmerge` records and says.
#[derive(Debug, Clone, PartialEq)]
pub struct Merged {
    pub ops: Vec<Op>,
    pub label: String,
    /// The code merged away.
    pub from: String,
    /// The code kept, and its key.
    pub into: String,
    pub key: String,
    /// What moved, in words.
    pub moved: String,
}

/// `designationmerge`: every user of `from` takes `into`'s key, and `from` is removed, in
/// one transaction. `uses` is caught up with `doc`.
///
/// # Errors
/// Either entry not in the list, or both the same entry.
pub fn plan_merge(doc: &Document, uses: &Uses, a: Aspect, from: &str, into: &str) -> Result<Merged, CmdError> {
    let from = named(doc, a, Some(from), "from")?;
    let into = named(doc, a, Some(into), "into")?;
    if from.key == into.key {
        return Err(CmdError::refused(format!("{} and {} are the same entry.", from.code, into.code)));
    }
    let counts = uses.counts(a, &from.key);
    let mut ops = Vec::new();
    let mut project = doc.project().clone();
    for (o, prop) in uses.users(a, &from.key).map(|u| u.references.clone()).unwrap_or_default() {
        match o {
            ObjectRef::Page(pg) => {
                if let Some(page) = doc.pages().get(&pg).filter(|p| designations::page_key(p, a).as_deref() == Some(from.key.as_str())) {
                    let after = designations::with_page_key(page, a, Some(&into.key));
                    ops.push(Op::ModifyPage { id: pg, before: page.clone(), after });
                }
            }
            ObjectRef::Project => {
                if designations::default_key(&project, a).as_deref() == Some(from.key.as_str()) {
                    project = designations::with_default_key(&project, a, Some(&into.key));
                }
            }
            // A device's own value is a field; its `fixed` flag stays as it was.
            ObjectRef::Device(c) if prop == a.property() => {
                crate::devices::edit_planned(doc, &mut ops, c, |d| {
                    if designations::device_key(d, a).as_deref() == Some(from.key.as_str()) {
                        let fixed = designations::device_fixed(d, a);
                        *d = designations::with_device_key(d, a, Some(&into.key), fixed);
                    }
                });
            }
            _ => {
                let before = property_of(doc, o, &prop);
                if before.as_deref() == Some(from.key.as_str()) {
                    match o {
                        ObjectRef::Cable(k) => Op::set_cable_property(doc, &mut ops, k, &prop, Some(into.key.clone())),
                        _ => ops.push(Op::SetProperty { object: o, key: prop, before, after: Some(into.key.clone()) }),
                    }
                }
            }
        }
    }
    // A location box's values are its aid's fields.
    for b in uses.users(a, &from.key).map(|u| u.boxes.clone()).unwrap_or_default() {
        let Some(before @ ecad_model::Aid::Box(x)) = doc.aids().get(&b) else { continue };
        if designations::box_key(x, a).as_deref() == Some(from.key.as_str()) {
            let after = ecad_model::Aid::Box(designations::with_box_key(x, a, Some(&into.key)));
            ops.push(Op::ModifyAid { id: b, before: before.clone(), after });
        }
    }
    if into.description.is_none()
        && let Some(d) = &from.description
    {
        let code = designations::list_code(&into.code)?;
        project = designations::with_entry(&project, a, &into.key, code, Some(designations::list_description(d)?));
    }
    project = designations::without_entry(&project, a, &from.key);
    ops.extend(modify(doc, project));
    let label = format!("Merge {} {} into {} ({})", a.word(), from.code, into.code, counts.words());
    Ok(Merged { ops, label, from: from.code, into: into.code, key: into.key, moved: counts.words() })
}

/// `designationremove {aspect?, unused: true}` (**Remove unused**): the ops
/// removing every entry nothing uses (of `only`, else of both lists), and their codes.
pub(crate) fn plan_remove_unused(doc: &Document, uses: &Uses, only: Option<Aspect>) -> (Vec<Op>, Vec<String>) {
    let mut project = doc.project().clone();
    let mut codes = Vec::new();
    for a in Aspect::ALL.into_iter().filter(|a| only.is_none_or(|o| o == *a)) {
        for e in designations::entries(doc, a) {
            if uses.counts(a, &e.key).unused() {
                project = designations::without_entry(&project, a, &e.key);
                codes.push(e.code);
            }
        }
    }
    (modify(doc, project).into_iter().collect(), codes)
}

/// `designationremove {aspect, key | code}`: the ops removing an entry nothing uses, and its code.
///
/// # Errors
/// No entry named, or one something uses.
pub(crate) fn plan_remove(doc: &Document, uses: &Uses, a: Aspect, key: Option<&str>, code: Option<&str>) -> Result<(Vec<Op>, String), CmdError> {
    let e = named_key_or_code(doc, a, key, code)?;
    let counts = uses.counts(a, &e.key);
    if !counts.unused() {
        return Err(CmdError::refused(format!("{} is used by {}. Give them another {} first.", e.code, counts.words(), a.word())));
    }
    Ok((modify(doc, designations::without_entry(doc.project(), a, &e.key)).into_iter().collect(), e.code))
}

/// Whether the transactions after the first `seen` of `ledger`'s log could change the designation
/// rows (`ecad_engine::commands::designation::rows`): an op on the list, on a `function` /
/// `location` / `location.<type>` value, or a component or page created or deleted
/// the Settings card's update map).
pub fn rows_changed_since(ledger: &ecad_model::Ledger, seen: usize) -> bool {
    let log = ledger.log();
    if seen > log.len() {
        return true;
    }
    log.get(seen..).unwrap_or_default().iter().flat_map(|tx| tx.ops.iter()).any(|op| match op {
        Op::SetProperty { key, .. } => Aspect::of_property(key).is_some(),
        Op::ModifyDevice { before, after, .. } => before.location != after.location || before.function != after.function,
        Op::CreateDevice { .. } | Op::DeleteDevice { .. } | Op::AddPage { .. } | Op::RemovePage { .. } | Op::ModifyProject { .. } => true,
        Op::CreateAid { aid, .. } | Op::DeleteAid { aid, .. } => aid.as_box().is_some(),
        Op::ModifyAid { before, after, .. } => match (before.as_box(), after.as_box()) {
            (Some(b), Some(x)) => b.location != x.location || b.function != x.function,
            (b, x) => b.is_some() || x.is_some(),
        },
        Op::ModifyPage { before, after, .. } => {
            let ch = before.change(after);
            ch.location || ch.function
        }
        _ => false,
    })
}

// ---------------------------------------------------------------------------------------------
// The commands (generic: crate::commands::run)

/// The list with its counts, as `designations` answers (`only`: one aspect), for the command and
/// for Settings ▸ Project info. `uses` must be caught up.
pub fn rows(doc: &Document, uses: &Uses, only: Option<Aspect>) -> Value {
    let mut out = serde_json::Map::new();
    for a in Aspect::ALL.into_iter().filter(|a| only.is_none_or(|o| o == *a)) {
        let default = designations::default_key(doc.project(), a);
        let rows: Vec<Value> = designations::entries(doc, a)
            .into_iter()
            .map(|e| {
                let c = uses.counts(a, &e.key);
                json!({
                    "key": e.key, "code": e.code, "description": e.description,
                    "devices": c.devices, "overrides": c.overrides, "boxes": c.boxes, "pages": c.pages,
                    "default": default.as_ref() == Some(&e.key), "unused": c.unused(), "uses": c.words(),
                })
            })
            .collect();
        // Stored keys with no entry (only merges of branches make them).
        let missing: Vec<&str> = uses.keys(a).filter(|k| designations::code_of(doc, a, k).is_none()).collect();
        out.insert(a.plural().into(), json!({"entries": rows, "missing": missing}));
    }
    Value::Object(out)
}

/// Each function or location entry's key, code, description and counts. Read only.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DesignationsParams {
    /// Function or location (default: both).
    #[serde(default)]
    aspect: Option<Aspect>,
}

impl Generic for DesignationsParams {
    const ID: &'static str = "designations";
    const TITLE: &'static str = "Functions and Locations";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let only = self.aspect;
        let doc = cx.doc;
        Ok(Outcome::answer(rows(doc, cx.uses(), only)))
    }
}

/// Adds an entry to the project's function or location list. One transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddParams {
    /// Function or location.
    aspect: Aspect,
    /// The code: "=F1", "+CP1".
    code: String,
    /// What it is.
    #[serde(default)]
    description: Option<String>,
}

impl Generic for AddParams {
    const ID: &'static str = "designationadd";
    const TITLE: &'static str = "Add Function or Location";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let a = self.aspect;
        let plan = plan_add(cx.doc, cx.ids, a, &self.code, self.description.as_deref())?;
        Ok(Outcome::edit(plan.label, plan.ops, json!({"changed": true, "key": plan.key, "code": plan.code})))
    }
}

/// Renames or describes an entry: `{aspect, key, code?, description?}` (`code` is the new code), or `{aspect,
/// code, to, description?}` (`code` names the entry, `to` is the new code). Everything that stores the key shows
/// the new code. One transaction; a code another entry has is refused: merge instead.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditParams {
    /// Function or location.
    aspect: Aspect,
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

impl Generic for EditParams {
    const ID: &'static str = "designationedit";
    const TITLE: &'static str = "Rename Function or Location";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let a = self.aspect;
        let edit = EditInput { key: self.key, code: self.code, to: self.to, new_code: self.new_code, description: self.description };
        let plan = plan_edit(cx.doc, a, edit)?;
        if plan.ops.is_empty() {
            return Ok(Outcome::answer(json!({"changed": false, "key": plan.key, "code": plan.code})));
        }
        Ok(Outcome::edit(plan.label, plan.ops, json!({"changed": true, "key": plan.key, "code": plan.code})))
    }
}

/// Every user of `from` takes `into`, and `from` is removed. One transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MergeParams {
    /// Function or location.
    aspect: Aspect,
    /// The entry merged away: its key or code.
    from: String,
    /// The entry kept: its key or code.
    into: String,
}

impl Generic for MergeParams {
    const ID: &'static str = "designationmerge";
    const TITLE: &'static str = "Merge Functions or Locations";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let a = self.aspect;
        let doc = cx.doc;
        let m = plan_merge(doc, cx.uses(), a, &self.from, &self.into)?;
        Ok(Outcome::edit(m.label, m.ops, json!({"changed": true, "from": m.from, "into": m.into, "key": m.key, "moved": m.moved})))
    }
}

/// Removes an entry nothing uses (`{aspect, key | code}`), or every entry nothing uses (`{aspect?, unused:
/// true}`). One transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoveParams {
    /// Function or location (with `unused`, default both).
    #[serde(default)]
    aspect: Option<Aspect>,
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

impl Generic for RemoveParams {
    const ID: &'static str = "designationremove";
    const TITLE: &'static str = "Remove Function or Location";

    fn check(&self, _registry: &ecad_model::registry::data::Registry) -> Result<(), CmdError> {
        match self.unused == Some(true) {
            true => Ok(()),
            false => required_aspect(self.aspect).map(|_| ()),
        }
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let doc = cx.doc;
        if self.unused == Some(true) {
            let only = self.aspect;
            let (ops, codes) = plan_remove_unused(doc, cx.uses(), only);
            if codes.is_empty() {
                return Ok(Outcome::answer(json!({"changed": false, "removed": []})));
            }
            let n = codes.len();
            let label = format!("Remove {n} unused {}", if n == 1 { "entry" } else { "entries" });
            return Ok(Outcome::edit(label, ops, json!({"changed": true, "removed": codes})));
        }
        let a = required_aspect(self.aspect)?;
        let (ops, code) = plan_remove(doc, cx.uses(), a, self.key.as_deref(), self.code.as_deref())?;
        Ok(Outcome::edit(format!("Remove {} {code}", a.word()), ops, json!({"changed": true, "code": code})))
    }
}
