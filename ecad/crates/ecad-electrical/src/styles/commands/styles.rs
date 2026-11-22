//! The style commands' rules (brief 050a, design 050 §6): the project's standard (`standard`), a
//! style's fields, name and copies (`setstyle`), deleting a style and moving its users
//! (`deletestyle`), and moving a style or a value to the standard (`promote`). Each function reads
//! the document and returns the ops to commit and what to report; reading another project's file
//! for a copy is the engine crate's.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Number, Value, json};
use std::collections::BTreeMap;

use ecad_model::objects::styles::{BasedOn, MAX_STYLES_PER_CATALOGUE, StyleName, is_name};
use ecad_model::{Document, IdAllocator, ObjectRef, Op, Standard, StandardHeader, StandardId, StyleLayer, StyleRef};

use crate::commands::run::{Access, Cx, Generic, Got, Io, Outcome, Pages, Read};
use crate::defaults::{self, Entry, Kind};
use crate::error::CmdError;
use crate::styles::{self, Catalogue, Layer};

/// Refusals, in words.
pub const STANDARD_FIRST: &str = "This project uses IEC (built in). Make a standard first (`standard {new}`).";
pub const NEEDS_NAME: &str = "A standard needs a name (1 to 64 characters, no control characters).";
pub const STYLE_NEEDS_NAME: &str = "A style needs a name (1 to 64 characters, no control characters).";
pub const BUILTIN_KEPT: &str = "Built-in styles cannot be deleted; reset its fields instead.";
pub const NOTHING_TO_COPY: &str = "That project uses IEC (built in); nothing to copy.";
/// The words of IEC (built in), the standard of a project without one.
pub const IEC: &str = "IEC (built in)";
/// Most fields one `setstyle` call sets (a guard on hostile input: the record's own bound).
pub(crate) const MAX_FIELDS_PER_CALL: usize = ecad_model::objects::styles::MAX_FIELDS_PER_STYLE;

// ---------------------------------------------------------------------------------------------
// Parameters

/// `No catalogue "x"; the catalogues are text, location, …`
fn no_catalogue(w: &str) -> CmdError {
    CmdError::refused(format!("No catalogue {w:?}; the catalogues are {}.", styles::words()))
}

/// The catalogue `word` names (spaces around ignored).
///
/// # Errors
/// No catalogue has that word.
pub fn catalogue(word: &str) -> Result<&'static Catalogue, CmdError> {
    let w = word.trim();
    <ecad_model::StyleCatalogue as ecad_model::Word>::parse(w).map(styles::catalogue).ok_or_else(|| no_catalogue(w))
}

/// The catalogue a call that may name none names (`promote {key}` names none).
///
/// # Errors
/// None named, or no catalogue has that word.
pub(crate) fn required_catalogue(word: Option<&str>) -> Result<&'static Catalogue, CmdError> {
    let w = word.ok_or_else(|| CmdError::input(format!("`catalogue` is required: {}.", styles::words())))?;
    catalogue(w)
}

/// The style a call names: `id`, else `name` (each trimmed).
///
/// # Errors
/// Neither given, or no style of the catalogue has it (or two share the name).
pub fn target(doc: &Document, cat: &Catalogue, id: Option<&str>, name: Option<&str>) -> Result<StyleRef, CmdError> {
    let t = id.or(name).map(str::trim).ok_or(CmdError::input("Name the style: `id` or `name`."))?;
    styles::find(doc, cat.catalogue, t)
}

/// The layer a `setstyle` call writes: "project" (the default) or "standard".
///
/// # Errors
/// The standard layer of a project without one, or another word.
pub fn layer(doc: &Document, word: Option<&str>) -> Result<StyleLayer, CmdError> {
    match word.map(str::trim) {
        None | Some("project") => Ok(StyleLayer::Project),
        Some("standard") if doc.standard().is_some() => Ok(StyleLayer::Standard),
        Some("standard") => Err(CmdError::refused(STANDARD_FIRST)),
        Some(other) => Err(CmdError::refused(format!("Unknown layer {other:?}: project or standard."))),
    }
}

/// What field `f` takes, in the registry's words (design 050 §6.2).
pub(crate) fn field_rule(f: &styles::Field) -> String {
    match f.kind {
        Kind::Length { min, max } if f.key == "height" => format!("{} is {min} to {max} mm (cap height)", f.key),
        Kind::Length { min, max } => format!("{} is {min} to {max} mm", f.key),
        Kind::Number { min, max } => format!("{} is a number from {min} to {max}", f.key),
        Kind::Choice(words) => format!("{} is one of {}", f.key, words.join(", ")),
        Kind::Flag => format!("{} is yes or no", f.key),
        Kind::Parsed { .. } if matches!(f.key, "colour" | "line_colour" | "fill") => {
            format!("{} is none, one IEC 60757 colour code (RD, BU…) or #rrggbb", f.key)
        }
        Kind::Text { max_chars } | Kind::Parsed { max_chars, .. } => format!("{} is 1 to {max_chars} characters", f.key),
        Kind::Style(c) => format!("{} is a style of the {c} catalogue (its ID or name)", f.key),
    }
}

/// One entry of a call's `fields`, as given.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FieldInput {
    /// `null`: back to the inherited value.
    Clear,
    /// Text, or a number written as text.
    Value(String),
}

/// The field `key` of `cat`.
fn field_of(cat: &Catalogue, key: &str) -> Result<&'static styles::Field, CmdError> {
    cat.field(key).ok_or_else(|| {
        let keys: Vec<&str> = cat.fields.iter().map(|f| f.key).collect();
        CmdError::refused(format!("No field {key:?}; {} have {}.", cat.plural, keys.join(", ")))
    })
}

/// The stored text of `v` for field `f`, checked (`None`: clear it).
fn field_value(doc: &Document, f: &styles::Field, v: FieldInput) -> Result<Option<String>, CmdError> {
    let typed = match v {
        FieldInput::Clear => return Ok(None),
        FieldInput::Value(s) => s.trim().to_string(),
    };
    if let Kind::Style(c) = f.kind {
        return styles::find(doc, c, &typed).map(|id| Some(id.text())).map_err(|_| CmdError::refused(format!("{}.", field_rule(f))));
    }
    let parsed = match f.kind {
        Kind::Flag => defaults::parse_input(f.kind, &typed),
        _ => defaults::parse_value_in(doc, f.kind, &typed),
    }
    .ok_or_else(|| CmdError::refused(format!("{}.", field_rule(f))))?;
    Ok(Some(match parsed {
        defaults::Value::Length(x) | defaults::Value::Number(x) => format!("{x}"),
        defaults::Value::Choice(w) => w.to_string(),
        defaults::Value::Flag(b) => ecad_model::naming::words::flag_word(b).to_string(),
        defaults::Value::Text(t) => t.to_string(),
        defaults::Value::OwnStyle(id) => ecad_model::StyleRef::Own(id).text(),
        defaults::Value::Size(s) => s.to_string(),
        defaults::Value::Colours(c) => c.to_string(),
        defaults::Value::Colour(c) => c.to_string(),
        defaults::Value::Sheet(s) => s.text(),
        defaults::Value::Margins(m) => ecad_model::document::sheet::margins_text(&m),
        defaults::Value::CoreColours(c) => c.to_string(),
    }))
}

/// The `fields` of a call, each checked against the catalogue, in the order given.
///
/// # Errors
/// More than [`MAX_FIELDS_PER_CALL`], a field the catalogue does not have, or a value the field
/// does not take.
pub(crate) fn fields(doc: &Document, cat: &Catalogue, given: Vec<(String, FieldInput)>) -> Result<Vec<(String, Option<String>)>, CmdError> {
    if given.len() > MAX_FIELDS_PER_CALL {
        return Err(CmdError::refused(format!("At most {MAX_FIELDS_PER_CALL} fields per call.")));
    }
    let mut out = Vec::new();
    for (k, v) in given {
        let f = field_of(cat, &k)?;
        out.push((f.key.to_string(), field_value(doc, f, v)?));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Users and their rewrites

/// `4 devices`, `1 wire`, `2 pages and 1 location box`: explicit users in words, with `boxes`
/// location boxes of their own style ([`styles::box_users`]).
pub fn users_words(users: &[(&'static str, ObjectRef)], boxes: usize) -> String {
    let (mut d, mut w, mut p) = (0, 0, 0);
    for (_, o) in users {
        match o {
            ObjectRef::Device(_) => d += 1,
            ObjectRef::Wire(_) | ObjectRef::Cable(_) => w += 1,
            ObjectRef::Page(_) => p += 1,
            ObjectRef::Project | ObjectRef::Standard => {}
        }
    }
    let one = |n: usize, s: &str, pl: &str| (n > 0).then(|| format!("{n} {}", if n == 1 { s } else { pl }));
    let parts: Vec<String> =
        [one(d, "device", "devices"), one(w, "wire", "wires"), one(p, "page", "pages"), one(boxes, "location box", "location boxes")]
            .into_iter()
            .flatten()
            .collect();
    parts.join(" and ")
}

fn props_of(doc: &Document, o: ObjectRef) -> Option<&ecad_model::Properties> {
    match o {
        ObjectRef::Device(c) => doc.devices().get(&c).map(|x| &x.properties),
        ObjectRef::Wire(c) => doc.wires().get(&c).map(|x| &x.display),
        ObjectRef::Cable(c) => doc.cables().get(&c).map(|x| &x.properties),
        ObjectRef::Page(p) => doc.pages().get(&p).map(|x| &x.display),
        ObjectRef::Project => Some(doc.display()),
        ObjectRef::Standard => doc.standard().map(|s| &s.values),
    }
}

/// Ops moving every choice of `id` (objects', the project's and the standard's) to `to`.
pub(crate) fn rewrite_users(doc: &Document, cat: &Catalogue, id: &StyleRef, to: &StyleRef) -> Vec<Op> {
    let mut out = Vec::new();
    let mut put = |o: ObjectRef, key: &str| {
        // Design 072 §4.6: a wire's own type is its field.
        if let ObjectRef::Wire(w) = o
            && key == ecad_model::connectivity::wiring::WIRE_CATEGORY
        {
            if let Some(before) = doc.wires().get(&w).filter(|x| x.wire_type.as_ref() == Some(id)) {
                let mut after = before.clone();
                after.wire_type = Some(to.clone());
                out.push(Op::ModifyWire { id: w, before: before.clone(), after });
            }
            return;
        }
        // Design 072 §4.5: a cable's type is its field.
        if let ObjectRef::Cable(k) = o
            && key == styles::cable::CABLE_TYPE
        {
            if let Some(before) = doc.cables().get(&k).filter(|x| x.cable_type.as_ref() == Some(id)) {
                let mut after = before.clone();
                after.cable_type = Some(to.clone());
                styles::cable::sync_offered_cores(doc, before, &mut after);
                out.extend(Op::modify_cable(k, before, after));
            }
            return;
        }
        let before = props_of(doc, o).and_then(|p| p.get(key)).cloned();
        if before.as_deref().and_then(StyleRef::parse).as_ref() == Some(id) {
            match o {
                ObjectRef::Cable(k) => Op::set_cable_property(doc, &mut out, k, key, Some(to.text())),
                _ => out.push(Op::SetProperty { object: o, key: key.to_string(), before, after: Some(to.text()) }),
            }
        }
    };
    for (key, o) in styles::explicit_users(doc, cat.catalogue, id) {
        put(o, key);
    }
    for e in (cat.choices)() {
        put(ObjectRef::Project, e.key);
        if doc.standard().is_some() {
            put(ObjectRef::Standard, e.key);
        }
    }
    // Brief 050a: a location box's own style is its field (design 072 §6.2).
    for a in styles::box_users(doc, cat.catalogue, id) {
        out.extend(crate::continuations::commands::box_op(doc, a, |b| b.style = Some(to.clone())));
    }
    out
}

/// Whether the project's or the standard's own choice names `id`.
fn chosen_by_levels(doc: &Document, cat: &Catalogue, id: &StyleRef) -> bool {
    (cat.choices)().iter().any(|e| {
        [Some(doc.display()), doc.standard().map(|s| &s.values)]
            .into_iter()
            .flatten()
            .any(|p| p.get(e.key).and_then(|v| StyleRef::parse(v)).as_ref() == Some(id))
    })
}

/// "-W01 and -W02 use X; give replace_with.": the cables using cable type `name` (the first
/// [`NAMED_CABLES`] by tag, then how many more), for a refusal.
fn cables_using(doc: &Document, users: &[(&'static str, ObjectRef)], name: &str) -> String {
    let mut tags: Vec<String> = users
        .iter()
        .filter_map(|(_, o)| match o {
            ObjectRef::Cable(k) => Some(crate::cables::cable_title(doc, *k)),
            _ => None,
        })
        .collect();
    tags.sort();
    let more = tags.len().saturating_sub(NAMED_CABLES);
    let verb = if tags.len() == 1 { "uses" } else { "use" };
    tags.truncate(NAMED_CABLES);
    let mut list = match tags.split_last() {
        Some((last, rest)) if !rest.is_empty() && more == 0 => format!("{} and {last}", rest.join(", ")),
        _ => tags.join(", "),
    };
    if more > 0 {
        list.push_str(&format!(" and {more} more"));
    }
    format!(
        "{list} {verb} {}; give replace_with, or choose another type for {} first.",
        crate::cables::clip(name),
        if verb == "uses" { "it" } else { "them" }
    )
}

/// Most cables a refusal names by tag.
const NAMED_CABLES: usize = 8;

/// "4 devices use X" or "The project uses X": who uses a style, for a refusal.
fn who_uses(users: &[(&'static str, ObjectRef)], boxes: usize) -> (String, &'static str) {
    let n = users.len() + boxes;
    let who = if n == 0 { "The project".to_string() } else { users_words(users, boxes) };
    (who, if n <= 1 { "uses" } else { "use" })
}

// ---------------------------------------------------------------------------------------------
// standard

/// The header line's counts: the standard's values and styles.
pub fn standard_counts(doc: &Document) -> (usize, usize) {
    doc.standard().map_or((0, 0), |s| (s.values.len(), s.styles.values().map(|m| m.len()).sum()))
}

/// What a `standard` call does.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StandardChange {
    /// Make a standard with this name, based on IEC.
    New(String),
    /// Rename the standard.
    Rename(String),
    /// Take another project's standard (read by the caller) in place of this one's.
    Copy(Box<Standard>),
    /// Back to IEC; the users of a standard-only style move to the style named per catalogue.
    Remove(BTreeMap<String, String>),
}

/// What a `standard` call records and says.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StandardPlan {
    pub ops: Vec<Op>,
    pub label: String,
    pub message: String,
}

/// The ops removing the standard: its values, its records, then its header (so undo restores
/// them, design 050 §3.1).
fn clear_standard(doc: &Document) -> Vec<Op> {
    let Some(s) = &doc.standard() else { return Vec::new() };
    let mut ops = Vec::new();
    for (k, v) in &s.values {
        ops.push(Op::SetProperty { object: ObjectRef::Standard, key: k.clone(), before: Some(v.clone()), after: None });
    }
    for (cat, m) in &s.styles {
        for (id, r) in m {
            ops.push(Op::SetStyle { layer: StyleLayer::Standard, catalogue: *cat, id: id.clone(), before: Some(r.clone()), after: None });
        }
    }
    ops.push(Op::SetStandard { before: Some(s.header.clone()), after: None });
    ops
}

/// A standard name as given, checked.
fn standard_text(name: &str) -> Result<StyleName, CmdError> {
    StyleName::new(name).map_err(|e| CmdError::refused(format!("A standard's name: {e}.")))
}

fn standard_name(name: &str) -> Result<&str, CmdError> {
    Some(name.trim()).filter(|n| is_name(n)).ok_or(CmdError::refused(NEEDS_NAME))
}

/// A `standard` change (design 050 §6.1): `None` when a rename keeps the name.
///
/// # Errors
/// A name that does not read, a second standard, no standard to rename or remove, or a user of
/// a standard-only style with no (or no valid) replacement.
pub(crate) fn plan_standard(doc: &Document, ids: &mut IdAllocator, change: StandardChange) -> Result<Option<StandardPlan>, CmdError> {
    let plan = |ops, label, message| Ok(Some(StandardPlan { ops, label, message }));
    match change {
        StandardChange::New(name) => {
            let name = standard_name(&name)?;
            if let Some(s) = &doc.standard() {
                return Err(CmdError::refused(format!(
                    "This project already has a standard ({}). Rename it, or go back to IEC first.",
                    s.header.name
                )));
            }
            let id: StandardId = ids.fresh();
            let header = StandardHeader { id, name: standard_text(name)?, based_on: BasedOn::Iec, origin: None };
            plan(
                vec![Op::SetStandard { before: None, after: Some(header) }],
                format!("Make standard {name}"),
                format!("Made standard {name}, based on IEC."),
            )
        }
        StandardChange::Rename(name) => {
            let name = standard_name(&name)?;
            let cur = doc.standard().ok_or(CmdError::refused(STANDARD_FIRST))?;
            if cur.header.name.as_str() == name {
                return Ok(None);
            }
            let after = StandardHeader { name: standard_text(name)?, ..cur.header.clone() };
            plan(
                vec![Op::SetStandard { before: Some(cur.header.clone()), after: Some(after) }],
                format!("Rename standard {name}"),
                format!("Renamed the standard {name}."),
            )
        }
        StandardChange::Copy(src) => {
            let mut ops = clear_standard(doc);
            let (v, n) = (src.values.len(), src.styles.values().map(|m| m.len().min(MAX_STYLES_PER_CATALOGUE)).sum::<usize>());
            ops.push(Op::SetStandard { before: None, after: Some(src.header.clone()) });
            for (k, v) in &src.values {
                ops.push(Op::SetProperty { object: ObjectRef::Standard, key: k.clone(), before: None, after: Some(v.clone()) });
            }
            for (cat, m) in &src.styles {
                for (id, r) in m.iter().take(MAX_STYLES_PER_CATALOGUE) {
                    ops.push(Op::SetStyle { layer: StyleLayer::Standard, catalogue: *cat, id: id.clone(), before: None, after: Some(r.clone()) });
                }
            }
            let label = format!("Copy standard {}", src.header.name);
            plan(ops, label, format!("Copied standard {} ({v} values, {n} styles). Your project values stay.", src.header.name))
        }
        StandardChange::Remove(replace_with) => {
            let cur = doc.standard().ok_or(CmdError::refused(STANDARD_FIRST))?;
            let mut ops = Vec::new();
            // Explicit users of a standard-only own style need a replacement (design 050 §6.1).
            for cat in styles::CATALOGUES {
                for (id, _) in styles::records(doc, StyleLayer::Standard, cat.catalogue) {
                    if cat.builtin(id).is_some() || styles::record(doc, StyleLayer::Project, cat.catalogue, id).is_some() {
                        continue;
                    }
                    let users = styles::explicit_users(doc, cat.catalogue, id);
                    let boxes = styles::box_users(doc, cat.catalogue, id).len();
                    if users.is_empty() && boxes == 0 && !chosen_by_levels(doc, cat, id) {
                        continue;
                    }
                    let Some(w) = replace_with.get(ecad_model::Word::word(cat.catalogue)) else {
                        let (who, verb) = who_uses(&users, boxes);
                        return Err(CmdError::refused(format!(
                            "{who} {verb} {}, which only the standard has; give replace_with {{\"{}\": style}}.",
                            styles::name(doc, cat.catalogue, id),
                            cat.catalogue
                        )));
                    };
                    let to = styles::find(doc, cat.catalogue, w)?;
                    let standard_only = cat.builtin(&to).is_none() && styles::record(doc, StyleLayer::Project, cat.catalogue, &to).is_none();
                    if standard_only {
                        return Err(CmdError::refused(format!(
                            "{} is the standard's too; replace it with a built-in or a project style.",
                            styles::name(doc, cat.catalogue, &to)
                        )));
                    }
                    ops.extend(rewrite_users(doc, cat, id, &to));
                }
            }
            let (v, n) = standard_counts(doc);
            // The rewrites above may change the standard's own values: clear from the state after them.
            let mut after = doc.clone();
            for op in &ops {
                op.apply(&mut after)?;
            }
            ops.extend(clear_standard(&after));
            let label = format!("Back to IEC from {}", cur.header.name);
            plan(ops, label, format!("{v} values and {n} styles went back to IEC; project values stay."))
        }
    }
}

// ---------------------------------------------------------------------------------------------
// setstyle

/// A `setstyle` call, as given (names trimmed by the rule).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct StyleEdit {
    /// Make a new style called `name`.
    pub new: bool,
    /// The style's ID (a built-in word or `#<32 hex>`).
    pub id: Option<String>,
    /// The style's name (the new style's, with `new`).
    pub name: Option<String>,
    /// The style a new one copies (default: the catalogue's first built-in).
    pub copy_of: Option<String>,
    /// A new name.
    pub rename: Option<String>,
    /// The fields to set.
    pub fields: Vec<(String, FieldInput)>,
    /// "project" (default) or "standard".
    pub layer: Option<String>,
}

/// What a `setstyle` call records.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StylePlan {
    /// Empty when everything is already so.
    pub ops: Vec<Op>,
    /// The style written.
    pub id: StyleRef,
    /// "Text style Tag: colour none → RD".
    pub label: String,
}

/// A `setstyle` change (design 050 §6.2): a new style, or a style's fields and name.
///
/// # Errors
/// A layer, field or name that does not read, a name another style has, a built-in renamed,
/// no style named, nothing to change, or a record the catalogue refuses.
pub(crate) fn plan_style(doc: &Document, ids: &mut IdAllocator, cat: &Catalogue, edit: StyleEdit) -> Result<StylePlan, CmdError> {
    let trim = |t: &Option<String>| t.as_deref().map(str::trim).map(str::to_string);
    let (name, rename, copy_of) = (trim(&edit.name), trim(&edit.rename), trim(&edit.copy_of));
    let layer = layer(doc, edit.layer.as_deref())?;
    let fields = fields(doc, cat, edit.fields)?;
    if edit.new {
        let name = name.filter(|n| is_name(n)).ok_or(CmdError::refused(STYLE_NEEDS_NAME))?;
        if styles::rows(doc, cat).iter().any(|r| r.name.eq_ignore_ascii_case(&name)) {
            return Err(CmdError::refused(format!("A {} is already called {name:?}; choose another name.", cat.one.to_lowercase())));
        }
        // Brief 057: a catalogue with no built-in styles (cable types, rating tables) starts an
        // empty style.
        let from = match copy_of {
            Some(t) => Some(styles::find(doc, cat.catalogue, &t)?),
            None => cat.builtins.first().map(|b| StyleRef::Builtin(b.id.to_string())),
        };
        let id = StyleRef::Own(ids.fresh::<u128>());
        let mut copied: Vec<(String, Option<String>)> = Vec::new();
        for f in cat.fields {
            let Some(from) = &from else { break };
            // 050b: a copy is no potential's default (one type per potential).
            if cat.catalogue == ecad_model::StyleCatalogue::Wire && f.key == "potential" {
                continue;
            }
            let v = styles::field(doc, cat, from, f, None).text();
            if !v.is_empty() {
                copied.push((f.key.to_string(), Some(v)));
            }
        }
        copied.extend(fields.iter().cloned());
        let rec = styles::with_fields(cat.catalogue, None, Some(&name), &copied)?;
        styles::check_record(doc, cat, &id, layer, &rec)?;
        let op = Op::SetStyle { layer, catalogue: cat.catalogue, id: id.clone(), before: None, after: Some(rec) };
        return Ok(StylePlan { ops: vec![op], id, label: format!("New {} {name}", cat.one.to_lowercase()) });
    }
    let id = target(doc, cat, edit.id.as_deref(), name.as_deref())?;
    let rename = rename.as_deref();
    if let Some(n) = rename {
        if !is_name(n) {
            return Err(CmdError::refused(STYLE_NEEDS_NAME));
        }
        if id.is_builtin() {
            return Err(CmdError::refused("A built-in style keeps its name; make a new style from it (`new`, `copy_of`) instead."));
        }
        if styles::rows(doc, cat).iter().any(|r| r.id != id && r.name.eq_ignore_ascii_case(n)) {
            return Err(CmdError::refused(format!("A {} is already called {n:?}; choose another name.", cat.one.to_lowercase())));
        }
    }
    if rename.is_none() && fields.is_empty() {
        return Err(CmdError::input("Give `fields` to set, `rename`, or `new: true`."));
    }
    // A rename goes to the layer holding the style's name.
    let name_layer = if styles::record(doc, StyleLayer::Standard, cat.catalogue, &id).is_some_and(|r| r.name.is_some()) {
        StyleLayer::Standard
    } else {
        StyleLayer::Project
    };
    let mut ops = Vec::new();
    for l in [StyleLayer::Standard, StyleLayer::Project] {
        let f: Vec<(String, Option<String>)> = if l == layer { fields.clone() } else { Vec::new() };
        let n = rename.filter(|_| l == name_layer);
        if f.is_empty() && n.is_none() {
            continue;
        }
        let before = styles::record(doc, l, cat.catalogue, &id).cloned();
        let rec = styles::with_fields(cat.catalogue, before.as_ref(), n, &f)?;
        // A built-in's override with nothing left is no record; an own style keeps its named
        // record.
        let keep = !rec.body.is_empty() || rec.name.is_some();
        let after = keep.then_some(rec);
        if let Some(r) = &after {
            styles::check_record(doc, cat, &id, l, r)?;
        }
        if after != before {
            ops.push(Op::SetStyle { layer: l, catalogue: cat.catalogue, id: id.clone(), before, after });
        }
    }
    // Design 050 §3.5: history names the style and the fields that changed
    // (`Text style Tag: colour none → RD`).
    let changes: Vec<String> = fields
        .iter()
        .map(|(k, v)| {
            let was = styles::field_of(doc, cat.catalogue, &id, k, None).map(|r| r.text()).unwrap_or_default();
            format!("{k} {was} → {}", v.as_deref().unwrap_or("(reset)"))
        })
        .collect();
    let name = rename.map_or_else(|| styles::name(doc, cat.catalogue, &id), str::to_string);
    let label = if changes.is_empty() { format!("{} {name}", cat.one) } else { format!("{} {name}: {}", cat.one, changes.join(", ")) };
    Ok(StylePlan { ops, id, label })
}

// ---------------------------------------------------------------------------------------------
// deletestyle

/// What a `deletestyle` call records and says.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DeletePlan {
    pub ops: Vec<Op>,
    /// The style deleted.
    pub id: StyleRef,
    /// Its name.
    pub name: String,
    /// How many explicit users moved.
    pub moved: usize,
}

/// Delete own style `id` (design 050 §6.3); its users move to `replace_with`.
///
/// # Errors
/// A built-in style, or a used style with no (or the same) replacement.
pub(crate) fn plan_delete(doc: &Document, cat: &Catalogue, id: StyleRef, replace_with: Option<&str>) -> Result<DeletePlan, CmdError> {
    if id.is_builtin() {
        return Err(CmdError::refused(BUILTIN_KEPT));
    }
    let name = styles::name(doc, cat.catalogue, &id);
    let users = styles::explicit_users(doc, cat.catalogue, &id);
    let boxes = styles::box_users(doc, cat.catalogue, &id).len();
    let mut ops = Vec::new();
    if !users.is_empty() || boxes > 0 || chosen_by_levels(doc, cat, &id) {
        let Some(w) = replace_with.map(str::trim) else {
            // Design 072 §4.5: a cable type in use names its cables.
            if cat.catalogue == ecad_model::StyleCatalogue::Cable {
                return Err(CmdError::refused(cables_using(doc, &users, &name)));
            }
            let (who, verb) = who_uses(&users, boxes);
            return Err(CmdError::refused(format!("{who} {verb} {name}; give replace_with.")));
        };
        let to = styles::find(doc, cat.catalogue, w)?;
        if to == id {
            return Err(CmdError::refused("replace_with must be another style."));
        }
        ops.extend(rewrite_users(doc, cat, &id, &to));
    }
    for l in [StyleLayer::Project, StyleLayer::Standard] {
        if let Some(r) = ecad_model::objects::styles::record(doc, l, cat.catalogue, &id) {
            ops.push(Op::SetStyle { layer: l, catalogue: cat.catalogue, id: id.clone(), before: Some(r.clone()), after: None });
        }
    }
    Ok(DeletePlan { ops, id, name, moved: users.len() + boxes })
}

// ---------------------------------------------------------------------------------------------
// promote

/// The registry entry `promote {key}` names.
///
/// # Errors
/// No default has that key.
pub(crate) fn promote_entry(key: &str) -> Result<&'static Entry, CmdError> {
    defaults::find(key).ok_or_else(|| CmdError::refused(format!("No default \"{key}\".")))
}

/// The project's standard, which a promotion needs.
///
/// # Errors
/// The project uses IEC (built in).
pub(crate) fn needs_standard(doc: &Document) -> Result<&Standard, CmdError> {
    doc.standard().ok_or(CmdError::refused(STANDARD_FIRST))
}

/// The ops moving the project's own value of `entry` to the standard, before the project's value
/// is reset, and that value as text (design 050 §6.4). The caller checks first that the entry may
/// be set at standard level.
///
/// # Errors
/// No standard, no project value of its own, or a standard saying something else.
pub(crate) fn promote_value(doc: &Document, entry: &Entry) -> Result<(Vec<Op>, String), CmdError> {
    let std = needs_standard(doc)?;
    let key = entry.key;
    let r = defaults::resolve(doc, entry, defaults::Scope::project());
    if r.from != defaults::Source::Project {
        return Err(CmdError::refused(format!("The project has no value of its own for `{key}` to promote.")));
    }
    let text = r.text();
    if let Some(v) = std.values.get(key).filter(|v| entry.parse_in(doc, v).is_some_and(|x| x != r.value)) {
        return Err(CmdError::refused(format!("The standard says {v}; use setdefault level standard to change it.")));
    }
    let mut ops = Vec::new();
    let before = std.values.get(key).cloned();
    if before.as_deref() != Some(text.as_str()) {
        ops.push(Op::SetProperty { object: ObjectRef::Standard, key: key.to_string(), before, after: Some(text.clone()) });
    }
    Ok((ops, text))
}

/// The ops moving project-only style `id` to the standard, and its name (design 050 §6.4).
///
/// # Errors
/// No standard, a built-in style, a style already in the standard, or not a project style.
pub(crate) fn plan_promote_style(doc: &Document, cat: &Catalogue, id: &StyleRef) -> Result<(Vec<Op>, String), CmdError> {
    if id.is_builtin() {
        return Err(CmdError::refused(
            "A built-in style is in every standard; set its fields at standard level instead (setstyle … layer: \"standard\").",
        ));
    }
    if styles::record(doc, StyleLayer::Standard, cat.catalogue, id).is_some() {
        return Err(CmdError::refused(format!("{} is already in the standard.", styles::name(doc, cat.catalogue, id))));
    }
    let rec =
        styles::record(doc, StyleLayer::Project, cat.catalogue, id).cloned().ok_or(CmdError::refused("Only a project style can be promoted."))?;
    let name = styles::name(doc, cat.catalogue, id);
    let ops = vec![
        Op::SetStyle { layer: StyleLayer::Project, catalogue: cat.catalogue, id: id.clone(), before: Some(rec.clone()), after: None },
        Op::SetStyle { layer: StyleLayer::Standard, catalogue: cat.catalogue, id: id.clone(), before: None, after: Some(rec) },
    ];
    Ok((ops, name))
}

/// The standard as `standard {}` returns it (design 050 §6.1).
pub(crate) fn standard_json(doc: &Document) -> Value {
    match &doc.standard() {
        None => json!({"name": IEC, "id": null}),
        Some(s) => {
            let styles: Map<String, Value> = s.styles.iter().map(|(c, m)| (c.to_string(), json!(m.len()))).collect();
            json!({
                "name": s.header.name,
                "id": s.header.id.hex(),
                "based_on": s.header.based_on,
                "origin": null,
                "values": s.values.len(),
                "styles": styles,
            })
        }
    }
}

/// One style as `styles` returns it.
pub(crate) fn style_json(doc: &Document, cat: &Catalogue, row: &styles::Row, users: bool) -> Value {
    let fields: Map<String, Value> = cat
        .fields
        .iter()
        .map(|f| {
            let r = styles::field(doc, cat, &row.id, f, None);
            (f.key.to_string(), json!({"value": value_json(&r), "from": r.from.as_str()}))
        })
        .collect();
    let mut v = json!({
        "catalogue": cat.catalogue,
        "id": row.id.text(),
        "name": row.name,
        "layer": row.layer.word(),
        "fields": fields,
    });
    if row.overridden {
        v["overridden"] = json!(true);
    }
    // Design 072 §8: a record holds only its catalogue's typed fields; none is unknown.
    if users {
        let c = styles::users(doc, cat.catalogue, &row.id);
        v["users"] = json!({"explicit": c.explicit, "implicit": c.implicit});
    }
    v
}

pub(crate) fn value_json(r: &defaults::Resolved<'_>) -> Value {
    match r.value {
        defaults::Value::Length(x) | defaults::Value::Number(x) => json!(x),
        defaults::Value::Choice(w) => json!(w),
        defaults::Value::Flag(b) => json!(ecad_model::naming::words::flag_word(b)),
        defaults::Value::Text(t) => json!(t),
        defaults::Value::OwnStyle(id) => json!(ecad_model::StyleRef::Own(id).text()),
        defaults::Value::Size(s) => json!(s.to_string()),
        defaults::Value::Colours(c) => json!(c.to_string()),
        defaults::Value::Colour(c) => json!(c.to_string()),
        defaults::Value::Sheet(s) => json!(s.text()),
        defaults::Value::Margins(m) => json!(ecad_model::document::sheet::margins_text(&m)),
        defaults::Value::CoreColours(c) => json!(c.to_string()),
    }
}

// ---------------------------------------------------------------------------------------------
// The commands (generic: crate::commands::run; design 050 §6)

/// `styles {catalogue?, users?}` (read only): every style, catalogue by catalogue; records of
/// catalogues this build does not know are listed apart (a newer file opens in an older build).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StylesParams {
    /// Only this catalogue: text, location, frame, wire, …
    #[serde(default)]
    catalogue: Option<String>,
    /// Also count each style's users.
    #[serde(default)]
    users: Option<bool>,
}

/// `standard {}` / `{new}` / `{name}` / `{from}` / `{remove, replace_with?}` (design 050 §6.1): no
/// field reads the standard; the first given of `new`, `name`, `from` and `remove` is done.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StandardParams {
    /// Make a standard with this name, based on IEC.
    #[serde(default)]
    new: Option<String>,
    /// Rename the standard.
    #[serde(default)]
    name: Option<String>,
    /// Copy the standard of this .ecad file (opened read-only).
    #[serde(default)]
    from: Option<String>,
    /// Back to IEC.
    #[serde(default)]
    remove: Option<bool>,
    /// Back to IEC: the style each catalogue's users of a standard-only style move to,
    /// {catalogue: style}.
    #[serde(default)]
    replace_with: Option<BTreeMap<String, String>>,
}

/// `setstyle {catalogue, id|name, new?, copy_of?, rename?, fields?, layer?}` (design 050 §6.2): a
/// new style, or a style's fields and name, as one transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetstyleParams {
    /// The catalogue: text, location, frame, wire, …
    catalogue: String,
    /// The style's ID (a built-in word or `#<32 hex>`).
    #[serde(default)]
    id: Option<String>,
    /// The style's name (with `new`: the new style's).
    #[serde(default)]
    name: Option<String>,
    /// Make a new style called `name`.
    #[serde(default)]
    new: Option<bool>,
    /// The style a new one copies (default: the catalogue's first built-in).
    #[serde(default)]
    copy_of: Option<String>,
    /// A new name.
    #[serde(default)]
    rename: Option<String>,
    /// The fields to set: {key: value | null} (null: back to the inherited value).
    #[serde(default)]
    fields: Option<BTreeMap<String, Option<FieldParam>>>,
    /// "project" (default) or "standard".
    #[serde(default)]
    layer: Option<String>,
}

/// `deletestyle {catalogue, id|name, replace_with?}` (design 050 §6.3): an own style deleted; its
/// users move to `replace_with`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeletestyleParams {
    /// The catalogue: text, location, frame, wire, …
    catalogue: String,
    /// The style's ID.
    #[serde(default)]
    id: Option<String>,
    /// The style's name.
    #[serde(default)]
    name: Option<String>,
    /// The style its users move to (its ID or name).
    #[serde(default)]
    replace_with: Option<String>,
}

/// `promote {catalogue, id|name}` or `promote {key}` (design 050 §6.4): a project-only style, or
/// the project's own value of a default, moves to the standard.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PromoteParams {
    /// A default's key: its project value moves to the standard.
    #[serde(default)]
    key: Option<String>,
    /// The style's catalogue: text, location, frame, wire, …
    #[serde(default)]
    catalogue: Option<String>,
    /// The style's ID.
    #[serde(default)]
    id: Option<String>,
    /// The style's name.
    #[serde(default)]
    name: Option<String>,
}

/// A style field's value as typed: text or a number.
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum FieldParam {
    Text(String),
    Number(Number),
}

/// The field inputs of `fields` (null clears a field).
pub(crate) fn field_inputs(fields: Option<BTreeMap<String, Option<FieldParam>>>) -> Vec<(String, FieldInput)> {
    let input = |v: Option<FieldParam>| match v {
        None => FieldInput::Clear,
        Some(FieldParam::Text(t)) => FieldInput::Value(t),
        Some(FieldParam::Number(n)) => FieldInput::Value(n.to_string()),
    };
    fields.into_iter().flatten().map(|(k, v)| (k, input(v))).collect()
}

impl Generic for StylesParams {
    const ID: &'static str = "styles";
    const TITLE: &'static str = "Styles";
    const HELP: &'static str = "{catalogue?, users?: bool} (read only)";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;

    fn check(&self) -> Result<(), CmdError> {
        self.catalogue.as_deref().map(catalogue).transpose().map(|_| ())
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let only = self.catalogue.as_deref().map(catalogue).transpose()?;
        let users = self.users.unwrap_or(false);
        let doc = cx.doc;
        let mut out = Vec::new();
        for cat in styles::CATALOGUES.iter().filter(|c| only.is_none_or(|o| o.catalogue == c.catalogue)) {
            for row in styles::rows(doc, cat) {
                out.push(style_json(doc, cat, &row, users));
            }
        }
        Ok(Outcome::answer(json!({"styles": out})))
    }
}

impl Generic for StandardParams {
    const ID: &'static str = "standard";
    const TITLE: &'static str = "Standard";
    const HELP: &'static str = "{} (read) | {new: name} | {name: name} (rename) | {from: path to a .ecad} (copy its standard) | \
{remove: true, replace_with?: {catalogue: style}} (back to IEC); one transaction";
    const PAGES: Pages = Pages::ReadAnywhere;
    const IO: Io = Io { reads: &[("from", Read::Standard)], writes: &[] };

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        if self.new.is_none() && self.name.is_none() && self.from.is_none() && self.remove.is_none() {
            return Ok(Outcome::answer(standard_json(cx.doc)));
        }
        // The engine read the other project before this one was taken; that file is never changed.
        let from = match self.from {
            None => None,
            Some(_) => match cx.read.get("from") {
                Some(Ok(Got::Standard(s))) => Some(s.clone()),
                Some(Err(e)) => return Err(CmdError::refused(e.0.clone())),
                Some(Ok(Got::Bytes(_))) | None => return Err(CmdError::refused("`from` could not be read")),
            },
        };
        let change = match (self.new, self.name, from) {
            (Some(n), _, _) => StandardChange::New(n),
            (None, Some(n), _) => StandardChange::Rename(n),
            (None, None, Some(src)) => StandardChange::Copy(src),
            (None, None, None) => StandardChange::Remove(self.replace_with.unwrap_or_default()),
        };
        let Some(plan) = plan_standard(cx.doc, cx.ids, change)? else {
            return Ok(Outcome::answer(json!({"changed": false, "standard": standard_json(cx.doc)})));
        };
        let message = plan.message;
        Ok(Outcome::edit_then(plan.label, plan.ops, move |doc| json!({"changed": true, "message": message, "standard": standard_json(doc)})))
    }
}

impl Generic for SetstyleParams {
    const ID: &'static str = "setstyle";
    const TITLE: &'static str = "Set Style";
    const HELP: &'static str = "{catalogue, id? | name?, new?: true, copy_of?: style, rename?: name, fields?: {key: value|null}, \
layer?: \"project\"|\"standard\"} (one transaction)";

    fn check(&self) -> Result<(), CmdError> {
        catalogue(&self.catalogue).map(|_| ())
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let cat = catalogue(&self.catalogue)?;
        let edit = StyleEdit {
            new: self.new == Some(true),
            id: self.id,
            name: self.name,
            copy_of: self.copy_of,
            rename: self.rename,
            fields: field_inputs(self.fields),
            layer: self.layer,
        };
        let plan = plan_style(cx.doc, cx.ids, cat, edit)?;
        let changed = !plan.ops.is_empty();
        let id = plan.id;
        Ok(Outcome::edit_then(plan.label, plan.ops, move |doc| {
            let row = styles::rows(doc, cat).into_iter().find(|r| r.id == id).unwrap_or_else(|| styles::Row {
                id: id.clone(),
                name: styles::name(doc, cat.catalogue, &id),
                layer: Layer::Project,
                overridden: false,
            });
            let mut v = style_json(doc, cat, &row, false);
            v["changed"] = json!(changed);
            v
        }))
    }
}

impl Generic for DeletestyleParams {
    const ID: &'static str = "deletestyle";
    const TITLE: &'static str = "Delete Style";
    const HELP: &'static str = "{catalogue, id | name, replace_with?: style} (own styles only; its users move to replace_with)";

    fn check(&self) -> Result<(), CmdError> {
        catalogue(&self.catalogue).map(|_| ())
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let cat = catalogue(&self.catalogue)?;
        let doc = cx.doc;
        let id = target(doc, cat, self.id.as_deref(), self.name.as_deref())?;
        let plan = plan_delete(doc, cat, id, self.replace_with.as_deref())?;
        let label = format!("Delete {} {}", cat.one.to_lowercase(), plan.name);
        let answer = json!({"changed": true, "deleted": plan.id.text(), "moved": plan.moved, "message": format!("Deleted {}.", plan.name)});
        Ok(Outcome::edit(label, plan.ops, answer))
    }
}

impl Generic for PromoteParams {
    const ID: &'static str = "promote";
    const TITLE: &'static str = "Promote to Standard";
    const HELP: &'static str = "{catalogue, id | name} (a project-only style moves to the standard) | {key} (a project value moves to the standard)";

    fn check(&self) -> Result<(), CmdError> {
        match self.key.as_deref().map(str::trim) {
            Some(key) => promote_entry(key).map(|_| ()),
            None => required_catalogue(self.catalogue.as_deref()).map(|_| ()),
        }
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let doc = cx.doc;
        if let Some(key) = self.key.as_deref().map(str::trim) {
            let entry = promote_entry(key)?;
            needs_standard(doc)?;
            if !entry.allows(defaults::Level::Standard) {
                return Err(CmdError::refused(crate::styles::commands::defaults::level_refused(entry, defaults::Level::Standard)));
            }
            let (mut ops, text) = promote_value(doc, entry)?;
            ops.extend(crate::styles::commands::defaults::ops(
                doc,
                entry,
                defaults::Level::Project,
                crate::styles::commands::defaults::Target::Project,
                None,
            )?);
            return Ok(Outcome::edit(format!("Promote {}", entry.label), ops, json!({"changed": true, "key": entry.key, "value": text})));
        }
        let cat = required_catalogue(self.catalogue.as_deref())?;
        needs_standard(doc)?;
        let id = target(doc, cat, self.id.as_deref(), self.name.as_deref())?;
        let (ops, name) = plan_promote_style(doc, cat, &id)?;
        let answer = json!({"changed": true, "id": id.text(), "message": format!("{name} is in the standard now.")});
        Ok(Outcome::edit(format!("Promote {name}"), ops, answer))
    }
}
