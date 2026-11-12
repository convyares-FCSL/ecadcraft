//! Brief 031's commands (design 031 §10): `textstyle` (Q1) and `defaults` (§10.8, the defaults
//! registry's read-only list). Writes commit through [`crate::project::Project::apply_edit`] as
//! one transaction; reads change nothing and run with writes off.

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::{ComponentId, Document, ObjectRef, Op, PageId, RepresentationId};
use serde_json::{Map, Value, json};

use crate::commands::{err, opt_id, with_any_page, with_project};
use crate::defaults::{self, Entry, Level, Resolved, Scope, Source};
use crate::text_style::{Fonts, TextKind};

/// Refusals (design 031 §10.1).
pub const BAD_HEIGHT: &str = "A text height is 0.5 to 10 mm.";
pub const BAD_WEIGHT: &str = "Weight is regular or bold.";
pub const BAD_SHOW: &str = "Shown is on, off or first (first: tag only).";
pub const BAD_FONT: &str = "A font name is 1 to 64 characters with no control characters.";
pub const NEEDS_KIND: &str = "height, weight and show need a kind.";

/// The commands of this module, appended to the registry by [`crate::commands::specs`].
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("textstyle", "Text Styles", run_textstyle)
            .params(
                "{font?: string|null, kind?: word, height?: number|null (mm, 0.5-10), weight?: \"regular\"|\"bold\"|null, \
show?: \"on\"|\"off\"|\"first\"|null} (no fields: read only; all given fields are one transaction; null: back to the default)",
            )
            .noundo(),
        CommandSpec::new("defaults", "Defaults", run_defaults)
            .params("{group?: string, key?: string, page?: id, component?: id, representation?: id} (read only)")
            .noundo(),
    ]
}

/// `Unknown text kind "x". Kinds: tag, point_label, …`
fn unknown_kind(word: &str) -> String {
    let all: Vec<&str> = TextKind::ALL.iter().map(|k| k.word()).collect();
    format!("Unknown text kind \"{word}\". Kinds: {}.", all.join(", "))
}

/// The kind a call names.
pub(crate) fn kind_param(params: &Value) -> Result<Option<TextKind>, String> {
    match params.get("kind") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(w)) => {
            let w = w.trim().to_ascii_lowercase();
            TextKind::from_word(&w).map(Some).ok_or_else(|| unknown_kind(&w))
        }
        Some(_) => Err(unknown_kind("?")),
    }
}

/// A field that may be absent (not given), `null` (back to the default) or a value.
enum Field<T> {
    Absent,
    Clear,
    Set(T),
}

fn field<T>(params: &Value, key: &str, parse: impl FnOnce(&Value) -> Option<T>, bad: &str) -> Result<Field<T>, String> {
    match params.get(key) {
        None => Ok(Field::Absent),
        Some(Value::Null) => Ok(Field::Clear),
        Some(v) => parse(v).map(Field::Set).ok_or_else(|| bad.to_string()),
    }
}

/// One `text.*` project property change, or nothing when it is already so.
fn set_project(doc: &Document, key: &str, after: Option<String>, ops: &mut Vec<Op>) {
    let before = doc.properties.get(key).cloned();
    if before != after {
        ops.push(Op::SetProperty { object: ObjectRef::Project, key: key.to_string(), before, after });
    }
}

/// The stored text of a number: as short as it reads (`3`, `2.5`).
fn number_text(x: f64) -> String {
    format!("{x}")
}

/// `textstyle {font?, kind?, height?, weight?, show?}` (design 031 §10.1).
fn run_textstyle(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let given = ["font", "kind", "height", "weight", "show"].iter().any(|k| params.get(*k).is_some());
    if !given {
        return with_any_page(s, |p, _| Ok(textstyle_json(p.ledger.document(), false)));
    }
    let kind = kind_param(params).map_err(err)?;
    let font = field(params, "font", |v| v.as_str().map(str::trim).map(str::to_string), BAD_FONT).map_err(err)?;
    if let Field::Set(f) = &font
        && defaults::text::FONT.parse(f).is_none()
    {
        return Err(err(BAD_FONT));
    }
    let height = field(params, "height", |v| v.as_f64().filter(|h| h.is_finite() && (0.5..=10.0).contains(h)), BAD_HEIGHT).map_err(err)?;
    let weight = field(params, "weight", |v| v.as_str().map(|w| w.trim().to_ascii_lowercase()).filter(|w| w == "regular" || w == "bold"), BAD_WEIGHT)
        .map_err(err)?;
    let show = field(
        params,
        "show",
        |v| v.as_str().map(|w| w.trim().to_ascii_lowercase()).filter(|w| ["on", "off", "first"].contains(&w.as_str())),
        BAD_SHOW,
    )
    .map_err(err)?;
    let needs_kind = !matches!(height, Field::Absent) || !matches!(weight, Field::Absent) || !matches!(show, Field::Absent);
    let kind = match (kind, needs_kind) {
        (Some(k), _) => Some(k),
        (None, true) => return Err(err(NEEDS_KIND)),
        (None, false) => None,
    };
    if let (Some(k), Field::Set(w)) = (kind, &show) {
        let entry = k.show_entry().ok_or_else(|| err(format!("{} has no Shown setting.", k.word())))?;
        if entry.parse(w).is_none() {
            return Err(err(BAD_SHOW));
        }
    }
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let mut ops = Vec::new();
        match &font {
            Field::Absent => {}
            Field::Clear => set_project(doc, defaults::text::FONT.key, None, &mut ops),
            Field::Set(f) => set_project(doc, defaults::text::FONT.key, Some(f.clone()), &mut ops),
        }
        if let Some(k) = kind {
            let h = match height {
                Field::Absent => None,
                Field::Clear => Some(None),
                Field::Set(h) => Some(Some(h)),
            };
            if let Some(h) = h {
                if k == TextKind::WireLabel {
                    // One stored value (§3.4): the wiring setting, never a property.
                    let before = doc.settings.clone();
                    let mut after = before.clone();
                    after.wiring.label_height = h.unwrap_or(ecad_model::wiring::DEFAULT_LABEL_HEIGHT);
                    if after != before {
                        ops.push(Op::SetProjectSettings { before, after });
                    }
                } else {
                    set_project(doc, k.height_entry().key, h.map(number_text), &mut ops);
                }
            }
            match &weight {
                Field::Absent => {}
                Field::Clear => set_project(doc, k.weight_entry().key, None, &mut ops),
                Field::Set(w) => set_project(doc, k.weight_entry().key, Some(w.clone()), &mut ops),
            }
            if let Some(entry) = k.show_entry() {
                match &show {
                    Field::Absent => {}
                    Field::Clear => set_project(doc, entry.key, None, &mut ops),
                    Field::Set(w) => set_project(doc, entry.key, Some(w.clone()), &mut ops),
                }
            }
        }
        let changed = !ops.is_empty();
        if changed {
            p.apply_edit(pg, "textstyle", "Text styles", ops)?;
        }
        Ok(textstyle_json(p.ledger.document(), changed))
    })
}

/// The style table as `textstyle` returns it: `{font, drawn_with, status, kinds: [...], changed}`.
pub fn textstyle_json(doc: &Document, changed: bool) -> Value {
    let fonts = Fonts::resolve(doc);
    let kinds: Vec<Value> = TextKind::ALL
        .iter()
        .map(|k| {
            let h = defaults::resolve(doc, k.height_entry(), Scope::project());
            let w = defaults::resolve(doc, k.weight_entry(), Scope::project());
            let sh = k.show_entry().map(|e| defaults::resolve(doc, e, Scope::project()));
            let default = h.from == Source::Builtin && w.from == Source::Builtin && sh.is_none_or(|r| r.from == Source::Builtin);
            json!({
                "kind": k.word(),
                "height": h.number(),
                "weight": w.word(),
                "show": sh.map(|r| r.word()),
                "default": default,
            })
        })
        .collect();
    json!({
        "font": fonts.wanted,
        "drawn_with": fonts.drawn_with,
        "status": fonts.status(),
        "kinds": kinds,
        "changed": changed,
    })
}

/// The JSON form of a registry value.
fn value_json(r: &Resolved<'_>) -> Value {
    match r.value {
        defaults::Value::Length(x) | defaults::Value::Number(x) => json!(x),
        defaults::Value::Word(w) => json!(w),
        defaults::Value::Text(t) => json!(t),
    }
}

fn level_word(l: Level) -> &'static str {
    match l {
        Level::Standard => "standard",
        Level::Project => "project",
        Level::Page => "page",
        Level::Object => "object",
    }
}

/// One registry entry as `defaults` lists it, resolved at `at`.
pub fn entry_json(doc: &Document, e: &Entry, at: Scope) -> Value {
    let r = defaults::resolve(doc, e, at);
    let builtin = Resolved { value: e.builtin, from: Source::Builtin };
    let mut m = Map::new();
    m.insert("key".into(), json!(e.key));
    m.insert("label".into(), json!(e.label));
    m.insert("group".into(), json!(e.group));
    m.insert("brief".into(), json!(e.brief));
    m.insert("kind".into(), json!(e.kind_word()));
    m.insert("builtin".into(), value_json(&builtin));
    m.insert("levels".into(), json!(e.levels.iter().map(|l| level_word(*l)).collect::<Vec<_>>()));
    m.insert("value".into(), value_json(&r));
    m.insert("from".into(), json!(r.from.as_str()));
    Value::Object(m)
}

/// The scope a `defaults` (or `showtext`) call names, checked against `doc`.
pub(crate) fn scope_param(doc: &Document, params: &Value) -> Result<Scope, String> {
    let rep: Option<RepresentationId> = opt_id(params, "representation")?;
    let component: Option<ComponentId> = opt_id(params, "component")?;
    let page: Option<PageId> = opt_id(params, "page")?;
    if let Some(r) = rep {
        if !doc.representations.contains_key(&r) {
            return Err(format!("No representation {}.", r.hex()));
        }
        return Ok(Scope::symbol(doc, r));
    }
    let mut at = Scope::project();
    if let Some(c) = component {
        if !doc.components.contains_key(&c) {
            return Err(format!("No component {}.", c.hex()));
        }
        at.component = Some(c);
    }
    if let Some(pg) = page {
        if !doc.pages.contains_key(&pg) {
            return Err(format!("No page {}.", pg.hex()));
        }
        at.page = Some(pg);
    }
    Ok(at)
}

/// `defaults {group?, key?, page?, component?, representation?}` (design 031 §10.8).
fn run_defaults(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let group = params.get("group").and_then(Value::as_str).map(str::trim).map(str::to_string);
    let key = params.get("key").and_then(Value::as_str).map(str::trim).map(str::to_string);
    with_any_page(s, |p, _| {
        let doc = p.ledger.document();
        let at = scope_param(doc, params)?;
        if let Some(k) = &key
            && defaults::find(k).is_none()
        {
            return Err(format!("No default \"{k}\"."));
        }
        let list: Vec<Value> = defaults::all()
            .filter(|e| group.as_deref().is_none_or(|g| e.group.eq_ignore_ascii_case(g)))
            .filter(|e| key.as_deref().is_none_or(|k| e.key == k))
            .map(|e| entry_json(doc, e, at))
            .collect();
        Ok(json!({ "defaults": list }))
    })
}
