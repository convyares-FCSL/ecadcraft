//! Brief 042's command: `setdefault`, the one writer for the Standards screen and the inspector's
//! "change at level" and Reset (design `ui-architecture.md` §11, U4c).
//!
//! # Semantics
//! It writes a registry entry's value at one level through the same stored values the registry
//! reads ([`crate::defaults::resolve`]): the project property of the entry's key, or the project
//! setting behind it ([`ecad_electrical::defaults::ProjectSource::Settings`], through
//! [`Op::SetProjectSettings`]); a page's property; a device's, conductor's or symbol's property
//! (`text.<rep>.<rest>` on its component). No new op and no new stored field. One call is one
//! transaction, undoable like any other; a call that changes nothing records nothing. `reset: true`
//! removes the level's value, so the entry resolves one level further out.
//!
//! Brief 050a: `level: "standard"` writes the project's standard's value
//! (`SetProperty` on `ObjectRef::Standard`); it is refused while the project uses IEC (built in).
//! A style choice ([`defaults::Kind::Style`]) takes a style's ID or name and stores the ID.
//!
//! A thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::default`] decide, and the shim commits the ops and answers in JSON.
//!
//! Excluded: any entry's levels, which only the registry sets.

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::cmd::default::{self as rules, Given, Objects};
use ecad_model::{DeviceId, Document, Op, PageId, RepresentationId, WireId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

pub use ecad_electrical::cmd::default::{NEEDS_OBJECT, NEEDS_VALUE, STANDARD_READ_ONLY, Target, level_refused, level_word, write_setting};

use crate::cmd::input::{Command, TextOrNumber, Typed};
use crate::cmd::{err, on_project};
use crate::defaults::{self, Entry, Level, Scope};

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[Typed::of::<SetdefaultParams>("setdefault")];

/// The commands of this module, registered through [`crate::lanes`].
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("setdefault", "Set Default", crate::cmd::input::typed::<SetdefaultParams>)
            .params(
                "{key, level: \"standard\"|\"project\"|\"page\"|\"object\", page?: id, component?: id, conductor?: id, representation?: id, \
value: string|number | reset: true} (writes a default at that level; page: the page shown if not given; one transaction)",
            )
            .noundo(),
    ]
}

/// [`rules::ops`], its refusal in words (the Standards screen's tests and `promote`).
///
/// # Errors
/// As [`rules::ops`].
pub fn ops(doc: &Document, entry: &Entry, level: Level, target: Target, value: Option<String>) -> Result<Vec<Op>, CmdError> {
    rules::ops(doc, entry, level, target, value)
}

/// The JSON line the Standards screen and the inspector run for a change (`None`: reset).
pub fn line(key: &str, level: Level, target: Target, value: Option<&str>) -> String {
    let mut p = json!({"key": key, "level": level_word(level)});
    match target {
        Target::Project => {}
        Target::Page(pg) => p["page"] = json!(pg.hex()),
        Target::Device(c) => p["component"] = json!(c.hex()),
        Target::Wire(c) => p["conductor"] = json!(c.hex()),
        Target::Symbol(r) => p["representation"] = json!(r.hex()),
    }
    match value {
        Some(v) => p["value"] = json!(v),
        None => p["reset"] = json!(true),
    }
    format!("setdefault {p}")
}

/// `setdefault {key, level, page?, component?, conductor?, representation?, value | reset: true}`:
/// writes a default at that level (page: the page shown if not given); one transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SetdefaultParams {
    /// A default's key (see `defaults`).
    key: String,
    /// standard, project (the default), page or object.
    #[serde(default)]
    level: Option<String>,
    /// The page level: the page (default the page shown).
    #[serde(default)]
    page: Option<PageId>,
    /// The object level: a device.
    #[serde(default)]
    component: Option<DeviceId>,
    /// The object level: a wire.
    #[serde(default)]
    conductor: Option<WireId>,
    /// The object level: a symbol.
    #[serde(default)]
    representation: Option<RepresentationId>,
    /// The value: text or a number.
    #[serde(default)]
    value: Option<TextOrNumber>,
    /// true: remove this level's value.
    #[serde(default)]
    reset: Option<bool>,
}

impl Command for SetdefaultParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let key = self.key.trim();
        let reset = self.reset.unwrap_or(false);
        let given = self.value.map(|v| match v {
            TextOrNumber::Text(t) => Given::Text(t),
            TextOrNumber::Number(n) => Given::Number(n.to_string()),
        });
        let Some(entry) = defaults::find(key) else {
            // Brief 048 (FP6): a symbol's family entry, `symbol.<key>.<field>`.
            let level = self.level;
            let key = key.to_string();
            let v = on_project(s, |p, pg, _| {
                let Some(done) = rules::plan_family(p.ledger.document(), &key, level.as_deref(), given, reset)? else { return Ok(None) };
                let changed = !done.ops.is_empty();
                if changed {
                    p.apply_edit(pg, "setdefault", &format!("Default {}: {}", done.field.label(), done.symbol), done.ops)?;
                }
                let mut v = crate::defaults::symbols::family_json(p.ledger.document(), &done.symbol, done.field);
                v["changed"] = json!(changed);
                Ok(Some(v))
            })?;
            return v.ok_or_else(|| err(format!("No default \"{key}\".")));
        };
        let level = match self.level.as_deref() {
            Some(w) => rules::level_of(w).map_err(|e| err(e.to_string()))?,
            None => Level::Project,
        };
        let chosen = rules::chosen(entry, given, reset).map_err(|e| err(e.to_string()))?;
        let objects = Objects { representation: self.representation, component: self.component, conductor: self.conductor };
        let page = self.page;
        on_project(s, |p, pg, shown| {
            let doc = p.ledger.document();
            let target = rules::target(level, page, shown, objects)?;
            let ops = rules::ops_for(doc, entry, level, target, chosen)?;
            let changed = !ops.is_empty();
            if changed {
                let label = format!("Default {}", entry.label);
                p.apply_edit(pg, "setdefault", &label, ops)?;
            }
            let doc = p.ledger.document();
            let at = match target {
                Target::Project => Scope::project(),
                Target::Page(pg) => Scope::page(pg),
                Target::Device(c) => Scope::component(c),
                Target::Wire(c) => Scope { conductor: Some(c), ..Scope::project() },
                Target::Symbol(r) => Scope::symbol(doc, r),
            };
            let mut v = ecad_electrical::cmd::text::entry_json(doc, entry, at);
            v["changed"] = json!(changed);
            Ok(v)
        })
    }
}
