//! The style commands' rules (brief 050a, design 050 §6): the project's standard (`standard`), a
//! style's fields, name and copies (`setstyle`), deleting a style and moving its users
//! (`deletestyle`), and moving a style or a value to the standard (`promote`). Each function reads
//! the document and returns the ops to commit and what to report; reading another project's file
//! for a copy is the engine crate's.

use std::collections::BTreeMap;

use ecad_model::style::{BASED_ON_IEC, MAX_STYLES_PER_CATALOGUE, is_name};
use ecad_model::{Document, IdAllocator, ObjectRef, Op, Standard, StandardHeader, StandardId, StyleLayer, StyleRecord, StyleRef};

use crate::defaults::{self, Entry, Kind};
use crate::error::CmdError;
use crate::styles::{self, Catalogue};

/// Refusals, in words.
pub const STANDARD_FIRST: &str = "This project uses IEC (built in). Make a standard first (`standard {new}`).";
pub const NEEDS_NAME: &str = "A standard needs a name (1 to 64 characters, no control characters).";
pub const STYLE_NEEDS_NAME: &str = "A style needs a name (1 to 64 characters, no control characters).";
pub const BUILTIN_KEPT: &str = "Built-in styles cannot be deleted; reset its fields instead.";
pub const NOTHING_TO_COPY: &str = "That project uses IEC (built in); nothing to copy.";
/// The words of IEC (built in), the standard of a project without one.
pub const IEC: &str = "IEC (built in)";
/// Most fields one `setstyle` call sets (a guard on hostile input: the record's own bound).
pub const MAX_FIELDS_PER_CALL: usize = ecad_model::style::MAX_FIELDS_PER_STYLE;

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
pub fn required_catalogue(word: Option<&str>) -> Result<&'static Catalogue, CmdError> {
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
        Some("standard") if doc.standard.is_some() => Ok(StyleLayer::Standard),
        Some("standard") => Err(CmdError::refused(STANDARD_FIRST)),
        Some(other) => Err(CmdError::refused(format!("Unknown layer {other:?}: project or standard."))),
    }
}

/// What field `f` takes, in the registry's words (design 050 §6.2).
pub fn field_rule(f: &styles::Field) -> String {
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
pub enum FieldInput {
    /// `null`: back to the inherited value.
    Clear,
    /// Text, or a number written as text.
    Value(String),
    /// A value of another kind (a list, an object, true or false).
    Wrong,
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
        FieldInput::Wrong => return Err(CmdError::refused(format!("{}.", field_rule(f)))),
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
        defaults::Value::Flag(b) => ecad_model::words::flag_word(b).to_string(),
        defaults::Value::Text(t) => t.to_string(),
        defaults::Value::OwnStyle(id) => ecad_model::StyleRef::Own(id).text(),
    }))
}

/// The `fields` of a call, each checked against the catalogue, in the order given.
///
/// # Errors
/// More than [`MAX_FIELDS_PER_CALL`], a field the catalogue does not have, or a value the field
/// does not take.
pub fn fields(doc: &Document, cat: &Catalogue, given: Vec<(String, FieldInput)>) -> Result<Vec<(String, Option<String>)>, CmdError> {
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

/// `4 devices`, `1 wire`, `2 pages and 1 device`: explicit users in words.
pub fn users_words(users: &[(&'static str, ObjectRef)]) -> String {
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
    let parts: Vec<String> = [one(d, "device", "devices"), one(w, "wire", "wires"), one(p, "page", "pages")].into_iter().flatten().collect();
    parts.join(" and ")
}

fn props_of(doc: &Document, o: ObjectRef) -> Option<&ecad_model::Properties> {
    match o {
        ObjectRef::Device(c) => doc.devices.get(&c).map(|x| &x.properties),
        ObjectRef::Wire(c) => doc.wires.get(&c).map(|x| &x.display),
        ObjectRef::Cable(c) => doc.cables.get(&c).map(|x| &x.properties),
        ObjectRef::Page(p) => doc.pages.get(&p).map(|x| &x.display),
        ObjectRef::Project => Some(&doc.properties),
        ObjectRef::Standard => doc.standard.as_ref().map(|s| &s.values),
    }
}

/// Ops moving every choice of `id` (objects', the project's and the standard's) to `to`.
pub fn rewrite_users(doc: &Document, cat: &Catalogue, id: &StyleRef, to: &StyleRef) -> Vec<Op> {
    let mut out = Vec::new();
    let mut put = |o: ObjectRef, key: &str| {
        let before = props_of(doc, o).and_then(|p| p.get(key)).cloned();
        if before.as_deref().and_then(StyleRef::parse).as_ref() == Some(id) {
            out.push(Op::SetProperty { object: o, key: key.to_string(), before, after: Some(to.text()) });
        }
    };
    for (key, o) in styles::explicit_users(doc, cat.catalogue, id) {
        put(o, key);
    }
    for e in (cat.choices)() {
        put(ObjectRef::Project, e.key);
        if doc.standard.is_some() {
            put(ObjectRef::Standard, e.key);
        }
    }
    out
}

/// Whether the project's or the standard's own choice names `id`.
fn chosen_by_levels(doc: &Document, cat: &Catalogue, id: &StyleRef) -> bool {
    (cat.choices)().iter().any(|e| {
        [Some(&doc.properties), doc.standard.as_ref().map(|s| &s.values)]
            .into_iter()
            .flatten()
            .any(|p| p.get(e.key).and_then(|v| StyleRef::parse(v)).as_ref() == Some(id))
    })
}

/// "4 devices use X" or "The project uses X": who uses a style, for a refusal.
fn who_uses(users: &[(&'static str, ObjectRef)]) -> (String, &'static str) {
    let who = if users.is_empty() { "The project".to_string() } else { users_words(users) };
    (who, if users.len() <= 1 { "uses" } else { "use" })
}

// ---------------------------------------------------------------------------------------------
// standard

/// The header line's counts: the standard's values and styles.
pub fn standard_counts(doc: &Document) -> (usize, usize) {
    doc.standard.as_ref().map_or((0, 0), |s| (s.values.len(), s.styles.values().map(|m| m.len()).sum()))
}

/// What a `standard` call does.
#[derive(Debug, Clone, PartialEq)]
pub enum StandardChange {
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
pub struct StandardPlan {
    pub ops: Vec<Op>,
    pub label: String,
    pub message: String,
}

/// The ops removing the standard: its values, its records, then its header (so undo restores
/// them, design 050 §3.1).
fn clear_standard(doc: &Document) -> Vec<Op> {
    let Some(s) = &doc.standard else { return Vec::new() };
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
fn standard_name(name: &str) -> Result<&str, CmdError> {
    Some(name.trim()).filter(|n| is_name(n)).ok_or(CmdError::refused(NEEDS_NAME))
}

/// A `standard` change (design 050 §6.1): `None` when a rename keeps the name.
///
/// # Errors
/// A name that does not read, a second standard, no standard to rename or remove, or a user of
/// a standard-only style with no (or no valid) replacement.
pub fn plan_standard(doc: &Document, ids: &mut IdAllocator, change: StandardChange) -> Result<Option<StandardPlan>, CmdError> {
    let plan = |ops, label, message| Ok(Some(StandardPlan { ops, label, message }));
    match change {
        StandardChange::New(name) => {
            let name = standard_name(&name)?;
            if let Some(s) = &doc.standard {
                return Err(CmdError::refused(format!(
                    "This project already has a standard ({}). Rename it, or go back to IEC first.",
                    s.header.name
                )));
            }
            let id: StandardId = ids.fresh();
            let header = StandardHeader { id, name: name.to_string(), based_on: BASED_ON_IEC.into(), origin: None };
            plan(
                vec![Op::SetStandard { before: None, after: Some(header) }],
                format!("Make standard {name}"),
                format!("Made standard {name}, based on IEC."),
            )
        }
        StandardChange::Rename(name) => {
            let name = standard_name(&name)?;
            let cur = doc.standard.as_ref().ok_or(CmdError::refused(STANDARD_FIRST))?;
            if cur.header.name == name {
                return Ok(None);
            }
            let after = StandardHeader { name: name.to_string(), ..cur.header.clone() };
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
            let cur = doc.standard.as_ref().ok_or(CmdError::refused(STANDARD_FIRST))?;
            let mut ops = Vec::new();
            // Explicit users of a standard-only own style need a replacement (design 050 §6.1).
            for cat in styles::CATALOGUES {
                for (id, _) in styles::records(doc, StyleLayer::Standard, cat.catalogue) {
                    if cat.builtin(id).is_some() || styles::record(doc, StyleLayer::Project, cat.catalogue, id).is_some() {
                        continue;
                    }
                    let users = styles::explicit_users(doc, cat.catalogue, id);
                    if users.is_empty() && !chosen_by_levels(doc, cat, id) {
                        continue;
                    }
                    let Some(w) = replace_with.get(ecad_model::Word::word(cat.catalogue)) else {
                        let (who, verb) = who_uses(&users);
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
pub struct StyleEdit {
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
pub struct StylePlan {
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
pub fn plan_style(doc: &Document, ids: &mut IdAllocator, cat: &Catalogue, edit: StyleEdit) -> Result<StylePlan, CmdError> {
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
        let mut rec = StyleRecord { name: name.clone(), fields: Default::default(), body: None };
        for f in cat.fields {
            let Some(from) = &from else { break };
            // 050b: a copy is no potential's default (one category per potential).
            if cat.catalogue == ecad_model::StyleCatalogue::Wire && f.key == "potential" {
                continue;
            }
            let v = styles::field(doc, cat, from, f, None).text();
            if !v.is_empty() {
                rec.fields.insert(f.key.to_string(), v);
            }
        }
        let rec = styles::with_fields(Some(&rec), None, &fields);
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
    let name_layer = if styles::record(doc, StyleLayer::Standard, cat.catalogue, &id).is_some_and(|r| !r.name.is_empty()) {
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
        let rec = styles::with_fields(before.as_ref(), n, &f);
        // A built-in's override with nothing left is no record; an own style keeps its named
        // record.
        let keep = !rec.fields.is_empty() || !rec.name.is_empty();
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
pub struct DeletePlan {
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
pub fn plan_delete(doc: &Document, cat: &Catalogue, id: StyleRef, replace_with: Option<&str>) -> Result<DeletePlan, CmdError> {
    if id.is_builtin() {
        return Err(CmdError::refused(BUILTIN_KEPT));
    }
    let name = styles::name(doc, cat.catalogue, &id);
    let users = styles::explicit_users(doc, cat.catalogue, &id);
    let mut ops = Vec::new();
    if !users.is_empty() || chosen_by_levels(doc, cat, &id) {
        let Some(w) = replace_with.map(str::trim) else {
            let (who, verb) = who_uses(&users);
            return Err(CmdError::refused(format!("{who} {verb} {name}; give replace_with.")));
        };
        let to = styles::find(doc, cat.catalogue, w)?;
        if to == id {
            return Err(CmdError::refused("replace_with must be another style."));
        }
        ops.extend(rewrite_users(doc, cat, &id, &to));
    }
    for l in [StyleLayer::Project, StyleLayer::Standard] {
        if let Some(r) = ecad_model::style::record(doc, l, cat.catalogue, &id) {
            ops.push(Op::SetStyle { layer: l, catalogue: cat.catalogue, id: id.clone(), before: Some(r.clone()), after: None });
        }
    }
    Ok(DeletePlan { ops, id, name, moved: users.len() })
}

// ---------------------------------------------------------------------------------------------
// promote

/// The registry entry `promote {key}` names.
///
/// # Errors
/// No default has that key.
pub fn promote_entry(key: &str) -> Result<&'static Entry, CmdError> {
    defaults::find(key).ok_or_else(|| CmdError::refused(format!("No default \"{key}\".")))
}

/// The project's standard, which a promotion needs.
///
/// # Errors
/// The project uses IEC (built in).
pub fn needs_standard(doc: &Document) -> Result<&Standard, CmdError> {
    doc.standard.as_ref().ok_or(CmdError::refused(STANDARD_FIRST))
}

/// The ops moving the project's own value of `entry` to the standard, before the project's value
/// is reset, and that value as text (design 050 §6.4). The caller checks first that the entry may
/// be set at standard level.
///
/// # Errors
/// No standard, no project value of its own, or a standard saying something else.
pub fn promote_value(doc: &Document, entry: &Entry) -> Result<(Vec<Op>, String), CmdError> {
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
pub fn plan_promote_style(doc: &Document, cat: &Catalogue, id: &StyleRef) -> Result<(Vec<Op>, String), CmdError> {
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
