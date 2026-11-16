//! Brief 050a's commands (design 050 §6): `standard`, `style`, `deletestyle`, `promote` and the
//! read `styles`.
//!
//! # Semantics
//! Every writing call is one transaction, undoable like any other, and refused with writes off
//! (the MCP policy). A style is named by its ID (a built-in word or `#<32 hex>`) or by its name;
//! a name shared by two styles asks for the ID. Refusals say what is allowed, in the registry's
//! words. Nothing here draws: the projection reads the styles ([`crate::styles`]).
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::style`] decide, and the shim commits the ops and answers in JSON.
//!
//! Excluded: exporting a standard as a file of its own, and library origins (2.1).

use std::collections::BTreeMap;

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::cmd::style::{self as rules, FieldInput, StandardChange, StyleEdit};
use ecad_model::Document;
use ecad_model::style::{MAX_STYLES_PER_CATALOGUE, StyleLayer};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Number, Value, json};

use crate::cmd::err;
use crate::cmd::input::{Command, Typed, lenient};
use crate::cmd::{on_any_page, on_project};
use crate::defaults;
use crate::styles::{self, Catalogue, Layer};
pub use ecad_electrical::cmd::style::{
    BUILTIN_KEPT, IEC, NEEDS_NAME, NOTHING_TO_COPY, STANDARD_FIRST, STYLE_NEEDS_NAME, field_rule, rewrite_users, standard_counts, users_words,
};

/// The commands of this module with a typed input (`category`, registered here, is
/// [`crate::cmd::wire`]'s).
pub const TYPED: &[Typed] = &[
    Typed::of::<StandardParams>("standard"),
    Typed::of::<SetstyleParams>("setstyle"),
    Typed::of::<DeletestyleParams>("deletestyle"),
    Typed::of::<PromoteParams>("promote"),
    Typed::of::<StylesParams>("styles"),
];

/// The commands of this module, registered through [`crate::lanes`].
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("standard", "Standard", crate::cmd::input::typed::<StandardParams>)
            .params(
                "{} (read) | {new: name} | {name: name} (rename) | {from: path to a .ecad} (copy its standard) | \
{remove: true, replace_with?: {catalogue: style}} (back to IEC); one transaction",
            )
            .noundo(),
        CommandSpec::new("setstyle", "Set Style", crate::cmd::input::typed::<SetstyleParams>)
            .params(
                "{catalogue, id? | name?, new?: true, copy_of?: style, rename?: name, fields?: {key: value|null}, \
layer?: \"project\"|\"standard\"} (one transaction)",
            )
            .noundo(),
        CommandSpec::new("deletestyle", "Delete Style", crate::cmd::input::typed::<DeletestyleParams>)
            .params("{catalogue, id | name, replace_with?: style} (own styles only; its users move to replace_with)")
            .noundo(),
        CommandSpec::new("promote", "Promote to Standard", crate::cmd::input::typed::<PromoteParams>)
            .params("{catalogue, id | name} (a project-only style moves to the standard) | {key} (a project value moves to the standard)")
            .noundo(),
        // Brief 050b (design 050 §6.9).
        CommandSpec::new("category", "Wire Category", crate::cmd::input::typed::<crate::cmd::wire::CategoryParams>)
            .params("{conductor: id | net: id (a conductor of the net) | conductors: [id], value: category id or name | null} (one transaction)")
            .noundo(),
        CommandSpec::new("styles", "Styles", crate::cmd::input::typed::<StylesParams>).params("{catalogue?, users?: bool} (read only)").noundo(),
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

/// `styles {catalogue?, users?}` (read only): every style, catalogue by catalogue; records of
/// catalogues this build does not know are listed apart (a newer file opens in an older build).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct StylesParams {
    /// Only this catalogue: text, location, frame, wire, …
    #[serde(default)]
    catalogue: Option<String>,
    /// Also count each style's users.
    #[serde(default, deserialize_with = "lenient")]
    #[schemars(with = "Option<bool>")]
    users: Option<bool>,
}

impl Command for StylesParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let only = self.catalogue.as_deref().map(rules::catalogue).transpose().map_err(|e| err(e.to_string()))?;
        let users = self.users.unwrap_or(false);
        on_any_page(s, |p, _| {
            let doc = p.ledger.document();
            let mut out = Vec::new();
            for cat in styles::CATALOGUES.iter().filter(|c| only.is_none_or(|o| o.word == c.word)) {
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
}

// ---------------------------------------------------------------------------------------------
// Fields

/// A field's value in `fields`: text, a number, or null (back to the inherited value).
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
enum FieldParam {
    /// Text.
    Text(String),
    /// A number.
    Number(Number),
}

/// The `fields` of a typed call, in the rules' form.
fn field_inputs(fields: Option<BTreeMap<String, Option<FieldParam>>>) -> Vec<(String, FieldInput)> {
    let input = |v: Option<FieldParam>| match v {
        None => FieldInput::Clear,
        Some(FieldParam::Text(t)) => FieldInput::Value(t),
        Some(FieldParam::Number(n)) => FieldInput::Value(n.to_string()),
    };
    fields.into_iter().flatten().map(|(k, v)| (k, input(v))).collect()
}

/// The `fields` of a JSON call, each checked against the catalogue (the title block commands,
/// until they are typed).
pub(crate) fn fields_param(doc: &Document, cat: &Catalogue, params: &Value) -> Result<Vec<(String, Option<String>)>, String> {
    let Some(v) = params.get("fields").filter(|v| !v.is_null()) else { return Ok(Vec::new()) };
    let o = v.as_object().ok_or("`fields` is an object of field: value | null.")?;
    let given = o
        .iter()
        .map(|(k, v)| {
            let input = match v {
                Value::Null => FieldInput::Clear,
                Value::String(s) => FieldInput::Value(s.clone()),
                Value::Number(n) => FieldInput::Value(n.to_string()),
                _ => FieldInput::Wrong,
            };
            (k.clone(), input)
        })
        .collect();
    Ok(rules::fields(doc, cat, given)?)
}

// ---------------------------------------------------------------------------------------------
// standard

/// `standard {}` / `{new}` / `{name}` / `{from}` / `{remove, replace_with?}` (design 050 §6.1): no
/// field reads the standard; the first given of `new`, `name`, `from` and `remove` is done.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct StandardParams {
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

impl Command for StandardParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        if self.new.is_none() && self.name.is_none() && self.from.is_none() && self.remove.is_none() {
            return on_any_page(s, |p, _| Ok(standard_json(p.ledger.document())));
        }
        // Copying reads the other project before this one is locked; that file is never changed.
        let from = match self.from.as_deref().map(str::trim) {
            Some(path) => Some(read_standard(path).map_err(err)?),
            None => None,
        };
        let change = match (self.new, self.name, from) {
            (Some(n), _, _) => StandardChange::New(n),
            (None, Some(n), _) => StandardChange::Rename(n),
            (None, None, Some(src)) => StandardChange::Copy(Box::new(src)),
            (None, None, None) => StandardChange::Remove(self.replace_with.unwrap_or_default()),
        };
        on_project(s, |p, pg, _| {
            let Some(plan) = rules::plan_standard(p.ledger.document(), &mut p.ids, change)? else {
                return Ok(json!({"changed": false, "standard": standard_json(p.ledger.document())}));
            };
            p.apply_edit(pg, "standard", &plan.label, plan.ops)?;
            let doc = p.ledger.document();
            Ok(json!({"changed": true, "message": plan.message, "standard": standard_json(doc)}))
        })
    }
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
// setstyle

/// `setstyle {catalogue, id|name, new?, copy_of?, rename?, fields?, layer?}` (design 050 §6.2): a
/// new style, or a style's fields and name, as one transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SetstyleParams {
    /// The catalogue: text, location, frame, wire, …
    catalogue: String,
    /// The style's ID (a built-in word or `#<32 hex>`).
    #[serde(default)]
    id: Option<String>,
    /// The style's name (with `new`: the new style's).
    #[serde(default)]
    name: Option<String>,
    /// Make a new style called `name`.
    #[serde(default, deserialize_with = "lenient")]
    #[schemars(with = "Option<bool>")]
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

impl Command for SetstyleParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let cat = rules::catalogue(&self.catalogue).map_err(|e| err(e.to_string()))?;
        let edit = StyleEdit {
            new: self.new == Some(true),
            id: self.id,
            name: self.name,
            copy_of: self.copy_of,
            rename: self.rename,
            fields: field_inputs(self.fields),
            layer: self.layer,
        };
        on_project(s, |p, pg, _| {
            let plan = rules::plan_style(p.ledger.document(), &mut p.ids, cat, edit)?;
            let changed = !plan.ops.is_empty();
            if changed {
                p.apply_edit(pg, "setstyle", &plan.label, plan.ops)?;
            }
            let doc = p.ledger.document();
            let id = plan.id;
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
}

// ---------------------------------------------------------------------------------------------
// deletestyle

/// `deletestyle {catalogue, id|name, replace_with?}` (design 050 §6.3): an own style deleted; its
/// users move to `replace_with`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeletestyleParams {
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

impl Command for DeletestyleParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let cat = rules::catalogue(&self.catalogue).map_err(|e| err(e.to_string()))?;
        on_project(s, |p, pg, _| {
            let doc = p.ledger.document();
            let id = rules::target(doc, cat, self.id.as_deref(), self.name.as_deref())?;
            let plan = rules::plan_delete(doc, cat, id, self.replace_with.as_deref())?;
            p.apply_edit(pg, "deletestyle", &format!("Delete {} {}", cat.one.to_lowercase(), plan.name), plan.ops)?;
            Ok(json!({"changed": true, "deleted": plan.id.text(), "moved": plan.moved, "message": format!("Deleted {}.", plan.name)}))
        })
    }
}

// ---------------------------------------------------------------------------------------------
// promote

/// `promote {catalogue, id|name}` or `promote {key}` (design 050 §6.4): a project-only style, or
/// the project's own value of a default, moves to the standard.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PromoteParams {
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

impl Command for PromoteParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        if let Some(key) = self.key.as_deref().map(str::trim) {
            let entry = rules::promote_entry(key).map_err(|e| err(e.to_string()))?;
            return on_project(s, |p, pg, _| {
                let doc = p.ledger.document();
                rules::needs_standard(doc)?;
                if !entry.allows(defaults::Level::Standard) {
                    return Err(CmdError::refused(crate::cmd::default::level_refused(entry, defaults::Level::Standard)));
                }
                let (mut ops, text) = rules::promote_value(doc, entry)?;
                let reset = crate::cmd::default::ops(doc, entry, defaults::Level::Project, crate::cmd::default::Target::Project, None);
                ops.extend(reset.map_err(CmdError::refused)?);
                p.apply_edit(pg, "promote", &format!("Promote {}", entry.label), ops)?;
                Ok(json!({"changed": true, "key": entry.key, "value": text}))
            });
        }
        let cat = rules::required_catalogue(self.catalogue.as_deref()).map_err(|e| err(e.to_string()))?;
        on_project(s, |p, pg, _| {
            let doc = p.ledger.document();
            rules::needs_standard(doc)?;
            let id = rules::target(doc, cat, self.id.as_deref(), self.name.as_deref())?;
            let (ops, name) = rules::plan_promote_style(doc, cat, &id)?;
            p.apply_edit(pg, "promote", &format!("Promote {name}"), ops)?;
            Ok(json!({"changed": true, "id": id.text(), "message": format!("{name} is in the standard now.")}))
        })
    }
}
