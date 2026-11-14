//! Brief 050a's commands (design 050 §6): `standard`, `style`, `deletestyle`, `promote` and the
//! read `styles`.
//!
//! # Semantics
//! Every writing call is one transaction, undoable like any other, and refused with writes off
//! (the MCP policy). A style is named by its ID (a built-in word or `#<32 hex>`) or by its name;
//! a name shared by two styles asks for the ID. Refusals say what is allowed, in the registry's
//! words. Nothing here draws: the projection reads the styles ([`crate::styles`]).
//!
//! Excluded: exporting a standard as a file of its own, and library origins (2.1).

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::style::{BASED_ON_IEC, MAX_STYLES_PER_CATALOGUE, StandardHeader, StyleLayer, StyleRecord, StyleRef, is_name};
use ecad_model::{Document, ObjectRef, Op, StandardId};
use serde_json::{Map, Value, json};

use crate::commands::{err, with_any_page, with_project};
use crate::defaults::{self, Kind};
use crate::styles::{self, Catalogue, Layer};

/// Refusals, in words.
pub const STANDARD_FIRST: &str = "This project uses IEC (built in). Make a standard first (`standard {new}`).";
pub const NEEDS_NAME: &str = "A standard needs a name (1 to 64 characters, no control characters).";
pub const STYLE_NEEDS_NAME: &str = "A style needs a name (1 to 64 characters, no control characters).";
pub const BUILTIN_KEPT: &str = "Built-in styles cannot be deleted; reset its fields instead.";
pub const NOTHING_TO_COPY: &str = "That project uses IEC (built in); nothing to copy.";
/// The words of IEC (built in), the standard of a project without one.
pub const IEC: &str = "IEC (built in)";
/// Most fields one `style` call sets (a guard on hostile input: the record's own bound).
const MAX_FIELDS_PER_CALL: usize = ecad_model::style::MAX_FIELDS_PER_STYLE;

/// The commands of this module, registered through [`crate::lanes`].
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("standard", "Standard", run_standard)
            .params(
                "{} (read) | {new: name} | {name: name} (rename) | {from: path to a .ecad} (copy its standard) | \
{remove: true, replace_with?: {catalogue: style}} (back to IEC); one transaction",
            )
            .noundo(),
        CommandSpec::new("setstyle", "Set Style", run_style)
            .params(
                "{catalogue, id? | name?, new?: true, copy_of?: style, rename?: name, fields?: {key: value|null}, \
layer?: \"project\"|\"standard\"} (one transaction)",
            )
            .noundo(),
        CommandSpec::new("deletestyle", "Delete Style", run_deletestyle)
            .params("{catalogue, id | name, replace_with?: style} (own styles only; its users move to replace_with)")
            .noundo(),
        CommandSpec::new("promote", "Promote to Standard", run_promote)
            .params("{catalogue, id | name} (a project-only style moves to the standard) | {key} (a project value moves to the standard)")
            .noundo(),
        // Brief 050b (design 050 §6.9).
        CommandSpec::new("category", "Wire Category", crate::wire_cmds::run_category)
            .params("{conductor: id | net: id (a conductor of the net) | conductors: [id], value: category id or name | null} (one transaction)")
            .noundo(),
        CommandSpec::new("styles", "Styles", run_styles).params("{catalogue?, users?: bool} (read only)").noundo(),
    ]
}

// ---------------------------------------------------------------------------------------------
// Reading

/// The standard as `standard {}` returns it (design 050 §6.1).
pub fn standard_json(doc: &Document) -> Value {
    match &doc.standard {
        None => json!({"name": IEC, "id": null}),
        Some(s) => {
            let styles: Map<String, Value> = s.styles.iter().map(|(c, m)| (c.clone(), json!(m.len()))).collect();
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

/// The header line's counts: the standard's values and styles.
pub fn standard_counts(doc: &Document) -> (usize, usize) {
    doc.standard.as_ref().map_or((0, 0), |s| (s.values.len(), s.styles.values().map(|m| m.len()).sum()))
}

/// One style as `styles` returns it.
pub fn style_json(doc: &Document, cat: &Catalogue, row: &styles::Row, users: bool) -> Value {
    let fields: Map<String, Value> = cat
        .fields
        .iter()
        .map(|f| {
            let r = styles::field(doc, cat, &row.id, f, None);
            (f.key.to_string(), json!({"value": value_json(&r), "from": r.from.as_str()}))
        })
        .collect();
    let mut v = json!({
        "catalogue": cat.word,
        "id": row.id.text(),
        "name": row.name,
        "layer": row.layer.word(),
        "fields": fields,
    });
    if row.overridden {
        v["overridden"] = json!(true);
    }
    let unknown: std::collections::BTreeSet<String> = [StyleLayer::Standard, StyleLayer::Project]
        .into_iter()
        .filter_map(|l| styles::record(doc, l, cat.word, &row.id))
        .flat_map(|r| r.fields.keys())
        .filter(|k| cat.field(k).is_none())
        .cloned()
        .collect();
    if !unknown.is_empty() {
        v["unknown_fields"] = json!(unknown);
    }
    if users {
        let c = styles::users(doc, cat.word, &row.id);
        v["users"] = json!({"explicit": c.explicit, "implicit": c.implicit});
    }
    v
}

fn value_json(r: &defaults::Resolved<'_>) -> Value {
    match r.value {
        defaults::Value::Length(x) | defaults::Value::Number(x) => json!(x),
        defaults::Value::Choice(w) => json!(w),
        defaults::Value::Text(t) => json!(t),
    }
}

/// `styles {catalogue?, users?}`: every style, catalogue by catalogue; records of catalogues this
/// build does not know are listed apart (a newer file opens in an older build).
fn run_styles(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let only = text_param(params, "catalogue").map(str::to_string);
    if let Some(w) = &only
        && styles::catalogue(w).is_none()
    {
        return Err(err(no_catalogue(w)));
    }
    let users = params.get("users").and_then(Value::as_bool).unwrap_or(false);
    with_any_page(s, |p, _| {
        let doc = p.ledger.document();
        let mut out = Vec::new();
        for cat in styles::CATALOGUES.iter().filter(|c| only.as_deref().is_none_or(|w| w == c.word)) {
            for row in styles::rows(doc, cat) {
                out.push(style_json(doc, cat, &row, users));
            }
        }
        let mut unknown = Vec::new();
        if only.is_none() {
            for layer in [StyleLayer::Standard, StyleLayer::Project] {
                for (word, m) in ecad_model::style::layer_of(doc, layer).into_iter().flatten() {
                    if styles::catalogue(word).is_none() {
                        unknown.push(json!({"catalogue": word, "layer": layer.word(), "styles": m.len().min(MAX_STYLES_PER_CATALOGUE)}));
                    }
                }
            }
        }
        Ok(json!({"styles": out, "unknown_catalogues": unknown}))
    })
}

fn no_catalogue(w: &str) -> String {
    format!("No catalogue {w:?}; the catalogues are {}.", styles::words())
}

// ---------------------------------------------------------------------------------------------
// Parameters

fn text_param<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::as_str).map(str::trim)
}

fn catalogue_param(params: &Value) -> Result<&'static Catalogue, String> {
    let w = text_param(params, "catalogue").ok_or_else(|| format!("`catalogue` is required: {}.", styles::words()))?;
    styles::catalogue(w).ok_or_else(|| no_catalogue(w))
}

/// The style a call names (`id` or `name`).
fn target(doc: &Document, cat: &Catalogue, params: &Value) -> Result<StyleRef, String> {
    let t = text_param(params, "id").or_else(|| text_param(params, "name")).ok_or("Name the style: `id` or `name`.")?;
    styles::find(doc, cat.word, t)
}

fn layer_param(doc: &Document, params: &Value) -> Result<StyleLayer, String> {
    match text_param(params, "layer") {
        None | Some("project") => Ok(StyleLayer::Project),
        Some("standard") if doc.standard.is_some() => Ok(StyleLayer::Standard),
        Some("standard") => Err(STANDARD_FIRST.into()),
        Some(other) => Err(format!("Unknown layer {other:?}: project or standard.")),
    }
}

/// What field `f` takes, in the registry's words (design 050 §6.2).
pub fn field_rule(f: &styles::Field) -> String {
    match f.kind {
        Kind::Length { min, max } if f.key == "height" => format!("{} is {min} to {max} mm (cap height)", f.key),
        Kind::Length { min, max } => format!("{} is {min} to {max} mm", f.key),
        Kind::Number { min, max } => format!("{} is a number from {min} to {max}", f.key),
        Kind::Choice(words) => format!("{} is one of {}", f.key, words.join(", ")),
        Kind::Parsed { .. } if matches!(f.key, "colour" | "line_colour" | "fill") => {
            format!("{} is none, one IEC 60757 colour code (RD, BU…) or #rrggbb", f.key)
        }
        Kind::Text { max_chars } | Kind::Parsed { max_chars, .. } => format!("{} is 1 to {max_chars} characters", f.key),
        Kind::Style(c) => format!("{} is a style of the {c} catalogue (its ID or name)", f.key),
    }
}

/// The stored text of `v` for field `f`, checked (`None`: clear it).
fn field_value(doc: &Document, f: &styles::Field, v: &Value) -> Result<Option<String>, String> {
    let typed = match v {
        Value::Null => return Ok(None),
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        _ => return Err(format!("{}.", field_rule(f))),
    };
    if let Kind::Style(c) = f.kind {
        return styles::find(doc, c, &typed).map(|id| Some(id.text())).map_err(|_| format!("{}.", field_rule(f)));
    }
    let parsed = defaults::parse_value_in(doc, f.kind, &typed).ok_or_else(|| format!("{}.", field_rule(f)))?;
    Ok(Some(match parsed {
        defaults::Value::Length(x) | defaults::Value::Number(x) => format!("{x}"),
        defaults::Value::Choice(w) => w.to_string(),
        defaults::Value::Text(t) => t.to_string(),
    }))
}

/// The `fields` of a call, each checked against the catalogue.
fn fields_param(doc: &Document, cat: &Catalogue, params: &Value) -> Result<Vec<(String, Option<String>)>, String> {
    let Some(v) = params.get("fields").filter(|v| !v.is_null()) else { return Ok(Vec::new()) };
    let o = v.as_object().ok_or("`fields` is an object of field: value | null.")?;
    if o.len() > MAX_FIELDS_PER_CALL {
        return Err(format!("At most {MAX_FIELDS_PER_CALL} fields per call."));
    }
    let mut out = Vec::new();
    for (k, v) in o {
        let f = cat.field(k).ok_or_else(|| {
            let keys: Vec<&str> = cat.fields.iter().map(|f| f.key).collect();
            format!("No field {k:?}; {} have {}.", cat.plural, keys.join(", "))
        })?;
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
            ObjectRef::Component(_) => d += 1,
            ObjectRef::Conductor(_) | ObjectRef::Cable(_) => w += 1,
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
        ObjectRef::Component(c) => doc.components.get(&c).map(|x| &x.properties),
        ObjectRef::Conductor(c) => doc.conductors.get(&c).map(|x| &x.properties),
        ObjectRef::Cable(c) => doc.cables.get(&c).map(|x| &x.properties),
        ObjectRef::Page(p) => doc.pages.get(&p).map(|x| &x.properties),
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
    for (key, o) in styles::explicit_users(doc, cat.word, id) {
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

// ---------------------------------------------------------------------------------------------
// standard

/// `standard {}` / `{new}` / `{name}` / `{from}` / `{remove, replace_with?}` (design 050 §6.1).
fn run_standard(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let given = ["new", "name", "from", "remove"].iter().any(|k| params.get(*k).is_some_and(|v| !v.is_null()));
    if !given {
        return with_any_page(s, |p, _| Ok(standard_json(p.ledger.document())));
    }
    // Copying reads the other project before this one is locked; that file is never changed.
    let from = match text_param(params, "from") {
        Some(path) => Some(read_standard(path).map_err(err)?),
        None => None,
    };
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let (ops, label, message) = if let Some(name) = params.get("new").filter(|v| !v.is_null()) {
            let name = name.as_str().map(str::trim).filter(|n| is_name(n)).ok_or(NEEDS_NAME)?;
            if let Some(s) = &doc.standard {
                return Err(format!("This project already has a standard ({}). Rename it, or go back to IEC first.", s.header.name));
            }
            let id: StandardId = p.ids.fresh();
            let header = StandardHeader { id, name: name.to_string(), based_on: BASED_ON_IEC.into(), origin: None };
            let ops = vec![Op::SetStandard { before: None, after: Some(header) }];
            (ops, format!("Make standard {name}"), format!("Made standard {name}, based on IEC."))
        } else if let Some(name) = params.get("name").filter(|v| !v.is_null()) {
            let name = name.as_str().map(str::trim).filter(|n| is_name(n)).ok_or(NEEDS_NAME)?;
            let cur = doc.standard.as_ref().ok_or(STANDARD_FIRST)?;
            if cur.header.name == name {
                return Ok(json!({"changed": false, "standard": standard_json(doc)}));
            }
            let after = StandardHeader { name: name.to_string(), ..cur.header.clone() };
            let ops = vec![Op::SetStandard { before: Some(cur.header.clone()), after: Some(after) }];
            (ops, format!("Rename standard {name}"), format!("Renamed the standard {name}."))
        } else if let Some(src) = &from {
            let mut ops = clear_standard(doc);
            let (v, n) = (src.values.len(), src.styles.values().map(|m| m.len().min(MAX_STYLES_PER_CATALOGUE)).sum::<usize>());
            ops.push(Op::SetStandard { before: None, after: Some(src.header.clone()) });
            for (k, v) in &src.values {
                ops.push(Op::SetProperty { object: ObjectRef::Standard, key: k.clone(), before: None, after: Some(v.clone()) });
            }
            for (cat, m) in &src.styles {
                for (id, r) in m.iter().take(MAX_STYLES_PER_CATALOGUE) {
                    ops.push(Op::SetStyle {
                        layer: StyleLayer::Standard,
                        catalogue: cat.clone(),
                        id: id.clone(),
                        before: None,
                        after: Some(r.clone()),
                    });
                }
            }
            let label = format!("Copy standard {}", src.header.name);
            (ops, label, format!("Copied standard {} ({v} values, {n} styles). Your project values stay.", src.header.name))
        } else {
            let cur = doc.standard.as_ref().ok_or(STANDARD_FIRST)?;
            let mut ops = Vec::new();
            // Explicit users of a standard-only own style need a replacement (design 050 §6.1).
            for cat in styles::CATALOGUES {
                for (id, _) in styles::records(doc, StyleLayer::Standard, cat.word) {
                    if cat.builtin(id).is_some() || styles::record(doc, StyleLayer::Project, cat.word, id).is_some() {
                        continue;
                    }
                    let users = styles::explicit_users(doc, cat.word, id);
                    if users.is_empty() && !chosen_by_levels(doc, cat, id) {
                        continue;
                    }
                    let Some(w) = params.get("replace_with").and_then(|r| r.get(cat.word)).and_then(Value::as_str) else {
                        let who = if users.is_empty() { "The project".to_string() } else { users_words(&users) };
                        let verb = if users.len() <= 1 { "uses" } else { "use" };
                        return Err(format!(
                            "{who} {verb} {}, which only the standard has; give replace_with {{\"{}\": style}}.",
                            styles::name(doc, cat.word, id),
                            cat.word
                        ));
                    };
                    let to = styles::find(doc, cat.word, w)?;
                    let standard_only = cat.builtin(&to).is_none() && styles::record(doc, StyleLayer::Project, cat.word, &to).is_none();
                    if standard_only {
                        return Err(format!(
                            "{} is the standard's too; replace it with a built-in or a project style.",
                            styles::name(doc, cat.word, &to)
                        ));
                    }
                    ops.extend(rewrite_users(doc, cat, id, &to));
                }
            }
            let (v, n) = standard_counts(doc);
            // The rewrites above may change the standard's own values: clear from the state after them.
            let mut after = doc.clone();
            for op in &ops {
                op.apply(&mut after).map_err(|e| e.to_string())?;
            }
            ops.extend(clear_standard(&after));
            let label = format!("Back to IEC from {}", cur.header.name);
            (ops, label, format!("{v} values and {n} styles went back to IEC; project values stay."))
        };
        p.apply_edit(pg, "standard", &label, ops)?;
        let doc = p.ledger.document();
        Ok(json!({"changed": true, "message": message, "standard": standard_json(doc)}))
    })
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
            ops.push(Op::SetStyle { layer: StyleLayer::Standard, catalogue: cat.clone(), id: id.clone(), before: Some(r.clone()), after: None });
        }
    }
    ops.push(Op::SetStandard { before: Some(s.header.clone()), after: None });
    ops
}

/// The standard of the project file at `path`, opened read-only and closed again (question 6).
///
/// # Errors
/// The file cannot be opened, or uses IEC (built in), in words.
pub fn read_standard(path: &str) -> Result<ecad_model::Standard, String> {
    let path = crate::file::normalize(path)?;
    let opened = crate::file::Binding::open_read_only(&path, 0x050a, "ecad standard copy")?;
    let doc = opened.ledger.document();
    doc.standard.clone().ok_or_else(|| NOTHING_TO_COPY.to_string())
}

// ---------------------------------------------------------------------------------------------
// style

/// `style {catalogue, id|name, new?, copy_of?, rename?, fields?, layer?}` (design 050 §6.2).
fn run_style(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let cat = catalogue_param(params).map_err(err)?;
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let layer = layer_param(doc, params)?;
        let fields = fields_param(doc, cat, params)?;
        let (ops, id, label) = if params.get("new").and_then(Value::as_bool) == Some(true) {
            let name = text_param(params, "name").filter(|n| is_name(n)).ok_or(STYLE_NEEDS_NAME)?;
            if styles::rows(doc, cat).iter().any(|r| r.name.eq_ignore_ascii_case(name)) {
                return Err(format!("A {} is already called {name:?}; choose another name.", cat.one.to_lowercase()));
            }
            let from = match text_param(params, "copy_of") {
                Some(t) => styles::find(doc, cat.word, t)?,
                None => cat.builtins.first().map(|b| StyleRef::Builtin(b.id.to_string())).ok_or("This catalogue has no built-in style to copy.")?,
            };
            let id = StyleRef::Own(p.ids.fresh::<u128>());
            let mut rec = StyleRecord { name: name.to_string(), fields: Default::default() };
            for f in cat.fields {
                // 050b: a copy is no potential's default (one category per potential).
                if cat.word == "wire" && f.key == "potential" {
                    continue;
                }
                let v = styles::field(doc, cat, &from, f, None).text();
                if !v.is_empty() {
                    rec.fields.insert(f.key.to_string(), v);
                }
            }
            let rec = styles::with_fields(Some(&rec), None, &fields);
            styles::check_record(doc, cat, &id, layer, &rec)?;
            let op = Op::SetStyle { layer, catalogue: cat.word.into(), id: id.clone(), before: None, after: Some(rec) };
            (vec![op], id, format!("New {} {name}", cat.one.to_lowercase()))
        } else {
            let id = target(doc, cat, params)?;
            let rename = text_param(params, "rename");
            if let Some(n) = rename {
                if !is_name(n) {
                    return Err(STYLE_NEEDS_NAME.into());
                }
                if id.is_builtin() {
                    return Err("A built-in style keeps its name; make a new style from it (`new`, `copy_of`) instead.".into());
                }
                if styles::rows(doc, cat).iter().any(|r| r.id != id && r.name.eq_ignore_ascii_case(n)) {
                    return Err(format!("A {} is already called {n:?}; choose another name.", cat.one.to_lowercase()));
                }
            }
            if rename.is_none() && fields.is_empty() {
                return Err("Give `fields` to set, `rename`, or `new: true`.".into());
            }
            // A rename goes to the layer holding the style's name.
            let name_layer = if styles::record(doc, StyleLayer::Standard, cat.word, &id).is_some_and(|r| !r.name.is_empty()) {
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
                let before = styles::record(doc, l, cat.word, &id).cloned();
                let rec = styles::with_fields(before.as_ref(), n, &f);
                // A built-in's override with nothing left is no record; an own style keeps its
                // named record.
                let keep = !rec.fields.is_empty() || !rec.name.is_empty();
                let after = keep.then_some(rec);
                if let Some(r) = &after {
                    styles::check_record(doc, cat, &id, l, r)?;
                }
                if after != before {
                    ops.push(Op::SetStyle { layer: l, catalogue: cat.word.into(), id: id.clone(), before, after });
                }
            }
            // Design 050 §3.5: history names the style and the fields that changed
            // (`Text style Tag: colour none → RD`).
            let changes: Vec<String> = fields
                .iter()
                .map(|(k, v)| {
                    let was = styles::field_of(doc, cat.word, &id, k, None).map(|r| r.text()).unwrap_or_default();
                    format!("{k} {was} → {}", v.as_deref().unwrap_or("(reset)"))
                })
                .collect();
            let name = rename.map_or_else(|| styles::name(doc, cat.word, &id), str::to_string);
            let label = if changes.is_empty() { format!("{} {name}", cat.one) } else { format!("{} {name}: {}", cat.one, changes.join(", ")) };
            (ops, id, label)
        };
        let changed = !ops.is_empty();
        if changed {
            p.apply_edit(pg, "setstyle", &label, ops)?;
        }
        let doc = p.ledger.document();
        let row = styles::rows(doc, cat).into_iter().find(|r| r.id == id).unwrap_or_else(|| styles::Row {
            id: id.clone(),
            name: styles::name(doc, cat.word, &id),
            layer: Layer::Project,
            overridden: false,
        });
        let mut v = style_json(doc, cat, &row, false);
        v["changed"] = json!(changed);
        Ok(v)
    })
}

// ---------------------------------------------------------------------------------------------
// deletestyle

/// `deletestyle {catalogue, id|name, replace_with?}` (design 050 §6.3).
fn run_deletestyle(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let cat = catalogue_param(params).map_err(err)?;
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let id = target(doc, cat, params)?;
        if id.is_builtin() {
            return Err(BUILTIN_KEPT.into());
        }
        let name = styles::name(doc, cat.word, &id);
        let users = styles::explicit_users(doc, cat.word, &id);
        let mut ops = Vec::new();
        if !users.is_empty() || chosen_by_levels(doc, cat, &id) {
            let Some(w) = text_param(params, "replace_with") else {
                let who = if users.is_empty() { "The project".to_string() } else { users_words(&users) };
                let verb = if users.len() <= 1 { "uses" } else { "use" };
                return Err(format!("{who} {verb} {name}; give replace_with."));
            };
            let to = styles::find(doc, cat.word, w)?;
            if to == id {
                return Err("replace_with must be another style.".into());
            }
            ops.extend(rewrite_users(doc, cat, &id, &to));
        }
        for l in [StyleLayer::Project, StyleLayer::Standard] {
            if let Some(r) = ecad_model::style::record(doc, l, cat.word, &id) {
                ops.push(Op::SetStyle { layer: l, catalogue: cat.word.into(), id: id.clone(), before: Some(r.clone()), after: None });
            }
        }
        let moved = users.len();
        p.apply_edit(pg, "deletestyle", &format!("Delete {} {name}", cat.one.to_lowercase()), ops)?;
        Ok(json!({"changed": true, "deleted": id.text(), "moved": moved, "message": format!("Deleted {name}.")}))
    })
}

// ---------------------------------------------------------------------------------------------
// promote

/// `promote {catalogue, id|name}` or `promote {key}` (design 050 §6.4).
fn run_promote(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    if let Some(key) = text_param(params, "key") {
        let entry = defaults::find(key).ok_or_else(|| err(format!("No default \"{key}\".")))?;
        return with_project(s, |p, pg, _| {
            let doc = p.ledger.document();
            let std = doc.standard.as_ref().ok_or(STANDARD_FIRST)?;
            if !entry.allows(defaults::Level::Standard) {
                return Err(crate::default_cmds::level_refused(entry, defaults::Level::Standard));
            }
            let r = defaults::resolve(doc, entry, defaults::Scope::project());
            if r.from != defaults::Source::Project {
                return Err(format!("The project has no value of its own for `{key}` to promote."));
            }
            let text = r.text();
            if let Some(v) = std.values.get(key).filter(|v| entry.parse_in(doc, v).is_some_and(|x| x != r.value)) {
                return Err(format!("The standard says {v}; use setdefault level standard to change it."));
            }
            let mut ops = Vec::new();
            let before = std.values.get(key).cloned();
            if before.as_deref() != Some(text.as_str()) {
                ops.push(Op::SetProperty { object: ObjectRef::Standard, key: key.to_string(), before, after: Some(text.clone()) });
            }
            ops.extend(crate::default_cmds::ops(doc, entry, defaults::Level::Project, crate::default_cmds::Target::Project, None)?);
            p.apply_edit(pg, "promote", &format!("Promote {}", entry.label), ops)?;
            Ok(json!({"changed": true, "key": key, "value": text}))
        });
    }
    let cat = catalogue_param(params).map_err(err)?;
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        if doc.standard.is_none() {
            return Err(STANDARD_FIRST.into());
        }
        let id = target(doc, cat, params)?;
        if id.is_builtin() {
            return Err("A built-in style is in every standard; set its fields at standard level instead (setstyle … layer: \"standard\").".into());
        }
        if styles::record(doc, StyleLayer::Standard, cat.word, &id).is_some() {
            return Err(format!("{} is already in the standard.", styles::name(doc, cat.word, &id)));
        }
        let rec = styles::record(doc, StyleLayer::Project, cat.word, &id).cloned().ok_or("Only a project style can be promoted.")?;
        let name = styles::name(doc, cat.word, &id);
        let ops = vec![
            Op::SetStyle { layer: StyleLayer::Project, catalogue: cat.word.into(), id: id.clone(), before: Some(rec.clone()), after: None },
            Op::SetStyle { layer: StyleLayer::Standard, catalogue: cat.word.into(), id: id.clone(), before: None, after: Some(rec) },
        ];
        p.apply_edit(pg, "promote", &format!("Promote {name}"), ops)?;
        Ok(json!({"changed": true, "id": id.text(), "message": format!("{name} is in the standard now.")}))
    })
}
