//! Brief 042's command: `setdefault`, the one writer for the Standards screen and the inspector's
//! "change at level" and Reset (design `ui-architecture.md` §11, U4c).
//!
//! # Semantics
//! It writes a registry entry's value at one level through the same stored values the registry
//! reads ([`crate::defaults::resolve`]): the project property of the entry's key, or the project
//! setting behind it ([`ProjectSource::Settings`], through [`Op::SetProjectSettings`]); a page's
//! property; a device's, conductor's or symbol's property (`text.<rep>.<rest>` on its component).
//! No new op and no new stored field. One call is one transaction, undoable like any other; a call
//! that changes nothing records nothing. `reset: true` removes the level's value, so the entry
//! resolves one level further out.
//!
//! Brief 050a: `level: "standard"` writes the project's standard's value
//! (`SetProperty` on `ObjectRef::Standard`); it is refused while the project uses IEC (built in).
//! A style choice ([`defaults::Kind::Style`]) takes a style's ID or name and stores the ID.
//!
//! Excluded: any entry's levels, which only the registry sets.

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::{ComponentId, ConductorId, Document, ObjectRef, Op, PageId, ProjectSettings, RepresentationId};
use serde_json::{Value, json};

use crate::commands::{err, opt_id, with_project};
use crate::defaults::{self, Entry, Level, ProjectSource, Scope};

/// Refusals, in words.
pub const STANDARD_READ_ONLY: &str = "This project uses IEC (built in). Make a standard first (`standard {new}`).";
pub const NEEDS_VALUE: &str = "Give a `value`, or `reset: true` to remove this level's value.";
pub const NEEDS_OBJECT: &str = "The object level needs a `component`, `conductor` or `representation`.";

/// The commands of this module, registered through [`crate::lanes`].
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("setdefault", "Set Default", run_setdefault)
            .params(
                "{key, level: \"standard\"|\"project\"|\"page\"|\"object\", page?: id, component?: id, conductor?: id, representation?: id, \
value: string|number | reset: true} (writes a default at that level; page: the page shown if not given; one transaction)",
            )
            .noundo(),
    ]
}

/// The words of a level in `defaults` and here.
pub fn level_word(l: Level) -> &'static str {
    match l {
        Level::Standard => "standard",
        Level::Project => "project",
        Level::Page => "page",
        Level::Object => "object",
    }
}

fn level_of(word: &str) -> Option<Level> {
    match word.trim().to_ascii_lowercase().as_str() {
        "standard" => Some(Level::Standard),
        "project" => Some(Level::Project),
        "page" => Some(Level::Page),
        "object" | "symbol" | "device" => Some(Level::Object),
        _ => None,
    }
}

/// `"`text.tag.height` cannot be set at page level. It can be set at: project, object."`
pub fn level_refused(entry: &Entry, level: Level) -> String {
    let allowed: Vec<&str> = entry.levels.iter().filter(|l| **l != Level::Standard).map(|l| level_word(*l)).collect();
    if allowed.is_empty() {
        return format!("`{}` cannot be set at {} level; it has only its built-in value.", entry.key, level_word(level));
    }
    format!("`{}` cannot be set at {} level. It can be set at: {}.", entry.key, level_word(level), allowed.join(", "))
}

/// The text a value is stored as: a number as short as it reads, a word as registered, a prefix
/// as Tag Settings writes it (letters, upper case, no `-`), other text trimmed.
///
/// # Errors
/// The value is not valid for the entry, in words.
pub fn stored_text(entry: &Entry, value: &Value) -> Result<String, String> {
    let typed = match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return Err(format!("`value` for `{}` is a string or a number.", entry.key)),
    };
    let parsed = entry.parse(&typed).ok_or_else(|| bad_value(entry))?;
    if is_prefix(entry) {
        return crate::devices::valid_prefix(&typed).ok_or_else(|| bad_value(entry));
    }
    Ok(match parsed {
        defaults::Value::Length(x) | defaults::Value::Number(x) => format!("{x}"),
        defaults::Value::Choice(w) => w.to_string(),
        defaults::Value::Text(t) => t.trim().to_string(),
    })
}

/// What a value of `entry` must be, in words.
fn bad_value(entry: &Entry) -> String {
    match entry.kind {
        defaults::Kind::Length { min, max } => format!("`{}` is a length from {min} to {max} mm.", entry.key),
        defaults::Kind::Number { min, max } => format!("`{}` is a number from {min} to {max}.", entry.key),
        defaults::Kind::Choice(words) => format!("`{}` is one of: {}.", entry.key, words.join(", ")),
        _ if is_prefix(entry) => format!("A prefix is 1 to {} letters, for example K or SR.", crate::devices::MAX_PREFIX_LETTERS),
        defaults::Kind::Text { max_chars } | defaults::Kind::Parsed { max_chars, .. } => {
            format!("`{}` is 1 to {max_chars} characters with no control characters, in its own form.", entry.key)
        }
        defaults::Kind::Style(cat) => format!("`{}` is a style of the {cat} catalogue: its ID or name (see `styles`).", entry.key),
    }
}

/// A tag prefix entry (`symbol.<key>.prefix`, `terminal.<key>.prefix`).
fn is_prefix(entry: &Entry) -> bool {
    starter_key(entry).is_some()
}

/// The starter symbol key of a prefix entry.
fn starter_key(entry: &Entry) -> Option<&'static str> {
    // Brief 045: its new starters' prefixes are in the `plc` area.
    let rest = entry.key.strip_prefix("symbol.").or_else(|| entry.key.strip_prefix("terminal.")).or_else(|| entry.key.strip_prefix("plc."))?;
    let key = rest.strip_suffix(".prefix")?;
    defaults::symbols::prefix_entry(key).filter(|e| e.key == entry.key).map(|_| key)
}

/// Write `value` (`None`: remove) into the project setting behind a [`ProjectSource::Settings`]
/// entry, as the setting's own command writes it.
///
/// # Errors
/// The entry has no setting this command knows (a registry entry added without a writer).
pub fn write_setting(entry: &Entry, s: &mut ProjectSettings, value: Option<&str>) -> Result<(), String> {
    let number = |v: Option<&str>, default: f64| v.and_then(|t| t.parse::<f64>().ok()).unwrap_or(default);
    match entry.key {
        k if k == defaults::text::WIRE_LABEL_HEIGHT.key => s.wiring.label_height = number(value, ecad_model::wiring::DEFAULT_LABEL_HEIGHT),
        k if k == defaults::wires::WIRING_LABEL_RULE.key => {
            s.wiring.label_rule = value.unwrap_or(ecad_model::wiring::DEFAULT_LABEL_RULE).to_string();
        }
        k if k == defaults::wires::WIRING_LABEL_MIN_LENGTH.key => {
            s.wiring.label_min_length = number(value, ecad_model::wiring::DEFAULT_LABEL_MIN_LENGTH);
        }
        _ => match starter_key(entry) {
            Some(key) => crate::device_cmds::apply_tag_settings(s, None, vec![(key.to_string(), value.map(str::to_string))]),
            None => return Err(format!("`{}` has no project setting this command can write.", entry.key)),
        },
    }
    Ok(())
}

/// The ops writing `value` (`None`: reset) for `entry` at `level` on `target`; empty when it is
/// already so.
///
/// # Errors
/// A level the entry does not allow, the standard level, or a missing target, in words.
pub fn ops(doc: &Document, entry: &Entry, level: Level, target: Target, value: Option<String>) -> Result<Vec<Op>, String> {
    if level == Level::Standard && doc.standard.is_none() {
        return Err(STANDARD_READ_ONLY.into());
    }
    if !entry.allows(level) {
        return Err(level_refused(entry, level));
    }
    let property = |object: ObjectRef, props: Option<&ecad_model::Properties>, key: String| -> Result<Vec<Op>, String> {
        let props = props.ok_or("No such object.")?;
        let before = props.get(&key).cloned();
        Ok(if before == value { Vec::new() } else { vec![Op::SetProperty { object, key, before, after: value.clone() }] })
    };
    match (level, target) {
        (Level::Project, _) => match entry.project {
            ProjectSource::Property => property(ObjectRef::Project, Some(&doc.properties), entry.key.to_string()),
            ProjectSource::Settings(_) => {
                let before = doc.settings.clone();
                let mut after = before.clone();
                write_setting(entry, &mut after, value.as_deref())?;
                Ok(if after == before { Vec::new() } else { vec![Op::SetProjectSettings { before, after }] })
            }
        },
        (Level::Page, Target::Page(pg)) => property(ObjectRef::Page(pg), doc.pages.get(&pg).map(|p| &p.properties), entry.key.to_string()),
        (Level::Object, Target::Symbol(rep)) => {
            let c = match doc.representations.get(&rep).map(|r| r.subject) {
                Some(ecad_model::Subject::Component(c)) => c,
                _ => return Err(format!("No symbol {}.", rep.hex())),
            };
            property(ObjectRef::Component(c), doc.components.get(&c).map(|c| &c.properties), defaults::symbol_key(entry.key, rep))
        }
        (Level::Object, Target::Component(c)) => {
            property(ObjectRef::Component(c), doc.components.get(&c).map(|c| &c.properties), entry.key.to_string())
        }
        (Level::Object, Target::Conductor(c)) => {
            property(ObjectRef::Conductor(c), doc.conductors.get(&c).map(|c| &c.properties), entry.key.to_string())
        }
        (Level::Object, _) => Err(NEEDS_OBJECT.into()),
        (Level::Page, _) => Err("The page level needs a `page`.".into()),
        (Level::Standard, _) => property(ObjectRef::Standard, doc.standard.as_ref().map(|s| &s.values), entry.key.to_string()),
    }
}

/// What a `setdefault` call writes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Project,
    Page(PageId),
    Component(ComponentId),
    Conductor(ConductorId),
    Symbol(RepresentationId),
}

/// The JSON line the Standards screen and the inspector run for a change (`None`: reset).
pub fn line(key: &str, level: Level, target: Target, value: Option<&str>) -> String {
    let mut p = json!({"key": key, "level": level_word(level)});
    match target {
        Target::Project => {}
        Target::Page(pg) => p["page"] = json!(pg.hex()),
        Target::Component(c) => p["component"] = json!(c.hex()),
        Target::Conductor(c) => p["conductor"] = json!(c.hex()),
        Target::Symbol(r) => p["representation"] = json!(r.hex()),
    }
    match value {
        Some(v) => p["value"] = json!(v),
        None => p["reset"] = json!(true),
    }
    format!("setdefault {p}")
}

/// `setdefault {key, level, page?, component?, conductor?, representation?, value | reset: true}`.
fn run_setdefault(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let key = params.get("key").and_then(Value::as_str).map(str::trim).ok_or_else(|| err("`key` (a default's key, see `defaults`) is required."))?;
    let Some(entry) = defaults::find(key) else {
        // Brief 048 (FP6): a symbol's family entry, `symbol.<key>.<field>`.
        return crate::symbol_cmds::run_set_family(s, params, key)?.ok_or_else(|| err(format!("No default \"{key}\".")));
    };
    let level = match params.get("level").and_then(Value::as_str) {
        Some(w) => level_of(w).ok_or_else(|| err(format!("Unknown level \"{w}\". Levels: standard, project, page, object.")))?,
        None => Level::Project,
    };
    let reset = params.get("reset").and_then(Value::as_bool).unwrap_or(false);
    // A style choice names a style by ID or name, resolved in the project below (brief 050a).
    let style_name = match (entry.kind, params.get("value")) {
        (defaults::Kind::Style(cat), Some(Value::String(v))) if !reset => Some((cat, v.clone())),
        _ => None,
    };
    let value = match (params.get("value"), reset) {
        (Some(_), false) if style_name.is_some() => None,
        (Some(v), false) if !v.is_null() => Some(stored_text(entry, v).map_err(err)?),
        (None | Some(Value::Null), true) => None,
        (Some(_), true) => return Err(err("Give either `value` or `reset: true`, not both.")),
        _ => return Err(err(NEEDS_VALUE)),
    };
    let rep: Option<RepresentationId> = opt_id(params, "representation").map_err(err)?;
    let component: Option<ComponentId> = opt_id(params, "component").map_err(err)?;
    let conductor: Option<ConductorId> = opt_id(params, "conductor").map_err(err)?;
    let page: Option<PageId> = opt_id(params, "page").map_err(err)?;
    with_project(s, |p, pg, shown| {
        let doc = p.ledger.document();
        let value = match &style_name {
            Some((cat, v)) => Some(crate::styles::find(doc, cat, v)?.text()),
            None => value,
        };
        let target = match level {
            Level::Project | Level::Standard => Target::Project,
            Level::Page => Target::Page(page.unwrap_or(shown)),
            Level::Object => match (rep, component, conductor) {
                (Some(r), _, _) => Target::Symbol(r),
                (None, Some(c), _) => Target::Component(c),
                (None, None, Some(c)) => Target::Conductor(c),
                _ => return Err(NEEDS_OBJECT.into()),
            },
        };
        let ops = ops(doc, entry, level, target, value)?;
        let changed = !ops.is_empty();
        if changed {
            let label = format!("Default {}", entry.label);
            p.apply_edit(pg, "setdefault", &label, ops)?;
        }
        let doc = p.ledger.document();
        let at = match target {
            Target::Project => Scope::project(),
            Target::Page(pg) => Scope::page(pg),
            Target::Component(c) => Scope::component(c),
            Target::Conductor(c) => Scope { conductor: Some(c), ..Scope::project() },
            Target::Symbol(r) => Scope::symbol(doc, r),
        };
        let mut v = crate::text_cmds::entry_json(doc, entry, at);
        v["changed"] = json!(changed);
        Ok(v)
    })
}
