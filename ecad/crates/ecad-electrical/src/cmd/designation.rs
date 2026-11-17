//! The rules of the commands of the project's list of functions and locations (brief 052, design
//! 052 §5.9): adding, renaming and describing, merging and removing entries. Each function reads
//! the document (and the use index, [`Uses`], caught up by the caller) and returns the ops to
//! commit and what to report.

use ecad_model::{DeviceId, Document, IdAllocator, ObjectRef, Op};

use crate::designations::{self, Aspect, Entry, Uses};
use crate::error::CmdError;

/// The aspect `word` names: "function" or "location" (also `f`, `l`, `=`, `+`).
///
/// # Errors
/// Any other word.
pub fn aspect(word: &str) -> Result<Aspect, CmdError> {
    Aspect::of_word(word).ok_or(CmdError::refused("`aspect` is \"function\" or \"location\""))
}

/// The aspect a call that may name none names (`None`: both).
///
/// # Errors
/// A word that is no aspect.
pub fn opt_aspect(word: Option<&str>) -> Result<Option<Aspect>, CmdError> {
    word.map(aspect).transpose()
}

/// The aspect a call must name where its other fields allow none (`designationremove`).
///
/// # Errors
/// None named, or a word that is no aspect.
pub fn required_aspect(word: Option<&str>) -> Result<Aspect, CmdError> {
    aspect(word.ok_or(CmdError::input("`aspect` is required: \"function\" or \"location\""))?)
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
pub fn named_key_or_code(doc: &Document, a: Aspect, key: Option<&str>, code: Option<&str>) -> Result<Entry, CmdError> {
    if key.is_some() { named(doc, a, key, "key") } else { named(doc, a, code, "code") }
}

/// A description as typed: trimmed; empty clears it.
fn description_value(doc: &Document, t: Option<&str>) -> Result<Option<String>, CmdError> {
    crate::devices::property_value(doc, "description", t)
}

fn set(key: String, before: Option<String>, after: Option<String>) -> Option<Op> {
    (before != after).then_some(Op::SetProperty { object: ObjectRef::Project, key, before, after })
}

/// A code refused because another entry has it.
fn taken(doc: &Document, a: Aspect, code: &str, except: Option<&str>) -> Option<Entry> {
    designations::entries(doc, a).into_iter().find(|e| e.code.eq_ignore_ascii_case(code) && Some(e.key.as_str()) != except)
}

/// What a list command records.
#[derive(Debug, Clone, PartialEq)]
pub struct ListPlan {
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
pub fn plan_add(doc: &Document, ids: &mut IdAllocator, a: Aspect, typed: &str, description: Option<&str>) -> Result<ListPlan, CmdError> {
    let code = designations::normalise(doc, a, typed)?;
    if taken(doc, a, &code, None).is_some() {
        return Err(CmdError::refused(format!("{code} is already in the list.")));
    }
    let id: DeviceId = ids.fresh();
    let key = id.hex();
    let mut ops =
        vec![Op::SetProperty { object: ObjectRef::Project, key: designations::code_property(a, &key), before: None, after: Some(code.clone()) }];
    if let Some(d) = description_value(doc, description)? {
        ops.push(Op::SetProperty { object: ObjectRef::Project, key: designations::description_property(a, &key), before: None, after: Some(d) });
    }
    Ok(ListPlan { ops, label: format!("Add {} {code}", a.word()), key, code })
}

/// A `designationedit` call, as given.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EditInput {
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
pub fn plan_edit(doc: &Document, a: Aspect, edit: EditInput) -> Result<ListPlan, CmdError> {
    // With `key`, `code` is the new code; without it, `code` names the entry and `to` (or
    // `new_code`) is the new one.
    let (named_code, new_code) = if edit.key.is_some() { (None, edit.code) } else { (edit.code, edit.to.or(edit.new_code)) };
    let e = named_key_or_code(doc, a, edit.key.as_deref(), named_code.as_deref())?;
    let mut ops = Vec::new();
    let mut label = format!("Describe {} {}", a.word(), e.code);
    let mut code = e.code.clone();
    if let Some(t) = new_code.as_deref() {
        code = designations::normalise(doc, a, t)?;
        if let Some(other) = taken(doc, a, &code, Some(&e.key)) {
            return Err(CmdError::refused(format!("{} is already in the list. Merge {} into it instead?", other.code, e.code)));
        }
        if code != e.code {
            label = format!("Rename {} {} → {code}", a.word(), e.code);
            ops.extend(set(designations::code_property(a, &e.key), Some(e.code.clone()), Some(code.clone())));
        }
    }
    if let Some(d) = edit.description {
        let after = description_value(doc, d.as_deref())?;
        let prop = designations::description_property(a, &e.key);
        ops.extend(set(prop.clone(), doc.properties.get(&prop).cloned(), after));
    }
    Ok(ListPlan { ops, label, key: e.key, code })
}

/// The value of property `prop` on object `o`.
fn property_of(doc: &Document, o: ObjectRef, prop: &str) -> Option<String> {
    match o {
        ObjectRef::Device(c) => doc.devices.get(&c).and_then(|x| x.properties.get(prop)).cloned(),
        ObjectRef::Page(pg) => doc.pages.get(&pg).and_then(|x| x.display.get(prop)).cloned(),
        ObjectRef::Project => doc.properties.get(prop).cloned(),
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

/// `designationmerge` (Q11): every user of `from` takes `into`'s key, and `from` is removed, in
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
    let counts = uses.counts(doc, a, &from.key);
    let mut ops = Vec::new();
    for (o, prop) in uses.users(a, &from.key).map(|u| u.refs.clone()).unwrap_or_default() {
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
    ops.extend(remove_ops(a, &from));
    let label = format!("Merge {} {} into {} ({})", a.word(), from.code, into.code, counts.words());
    Ok(Merged { ops, label, from: from.code, into: into.code, key: into.key, moved: counts.words() })
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

/// `designationremove {aspect?, unused: true}` (design 052 §9.4, **Remove unused**): the ops
/// removing every entry nothing uses (of `only`, else of both lists), and their codes.
pub fn plan_remove_unused(doc: &Document, uses: &Uses, only: Option<Aspect>) -> (Vec<Op>, Vec<String>) {
    let mut ops = Vec::new();
    let mut codes = Vec::new();
    for a in Aspect::ALL.into_iter().filter(|a| only.is_none_or(|o| o == *a)) {
        for e in designations::entries(doc, a) {
            if uses.counts(doc, a, &e.key).unused() {
                ops.extend(remove_ops(a, &e));
                codes.push(e.code);
            }
        }
    }
    (ops, codes)
}

/// `designationremove {aspect, key | code}`: the ops removing an entry nothing uses, and its code.
///
/// # Errors
/// No entry named, or one something uses.
pub fn plan_remove(doc: &Document, uses: &Uses, a: Aspect, key: Option<&str>, code: Option<&str>) -> Result<(Vec<Op>, String), CmdError> {
    let e = named_key_or_code(doc, a, key, code)?;
    let counts = uses.counts(doc, a, &e.key);
    if !counts.unused() {
        return Err(CmdError::refused(format!("{} is used by {}. Give them another {} first.", e.code, counts.words(), a.word())));
    }
    Ok((remove_ops(a, &e), e.code))
}
