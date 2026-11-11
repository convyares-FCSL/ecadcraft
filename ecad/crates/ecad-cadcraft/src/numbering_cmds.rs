//! Brief 026's commands (design 026 §3–§7, §13): `renumber`, `finalise`, `numbering`, and the
//! tag rules of `setprop` (swap, make one, next free; typing a tag fixes it; `tag_fixed`) for
//! devices and cables, which share one tag namespace.
//!
//! The interactive forms (the Renumber dialog, the clash box, Project Settings) commit through
//! exactly these JSON forms. The plans come from [`ecad_model::numbering`]; this module turns them
//! into one transaction each and into words.

use std::collections::BTreeSet;

use cadcraft_engine::{CommandSpec, EngineError, Session};
use ecad_model::numbering::{self, Kind, Mark, Plan, Request, Scope};
use ecad_model::tag::{self, FIXED_KEY, PROVISIONAL_SETTING, TAG_KEY};
use ecad_model::{CableId, ComponentId, ConductorId, Document, FolderId, ObjectRef, Op, PageId, Subject};
use serde_json::{Value, json};

use crate::commands::{opt_typed, with_any_page, with_project};
use crate::devices::{self, Holder};
use crate::project::Project;

fn err(m: impl Into<String>) -> EngineError {
    EngineError::Other(m.into())
}

/// Most rows a JSON preview lists (`more` counts the rest).
pub const MAX_PREVIEW_ROWS: usize = 2_000;

/// Refusal of an Apply on a stale preview (design §3).
pub const STALE: &str = "The project changed since the preview; check the new list and Apply again.";

/// Refusal of fixing a provisional tag (design §4).
pub const PROVISIONAL_FIXED: &str = "A provisional tag can't be fixed; give it a number first.";

/// The commands of this module.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("renumber", "Renumber", run_renumber)
            .params(
                "{kind?: \"devices\"|\"cables\"|\"wires\"|\"all\", scope?: \"all\"|\"selection\"|\"provisional\"|{prefix}|{folder: id}|{pages: [id]}, objects?: [id] (selection), start?: n, digits?: 1..6, digits_for_new?: bool, preview?: bool, based_on?: change count} (drawing order; fixed tags kept; one transaction)",
            )
            .noundo(),
        CommandSpec::new("finalise", "Finalise Numbering", run_finalise)
            .params("{preview?: bool, based_on?: change count} (numbers the provisional tags and unnumbered nets in drawing order; one transaction)")
            .noundo(),
        CommandSpec::new("numbering", "Numbering Settings", run_numbering)
            .params("{provisional?: bool} (no parameters reads; provisional: new tags are -K?1 until Finalise)")
            .noundo(),
    ]
}

// ---------------------------------------------------------------- parameters

fn kinds_param(params: &Value) -> Result<BTreeSet<Kind>, String> {
    let k = opt_typed(params, "kind", "a string", Value::as_str)?.unwrap_or("devices").trim().to_ascii_lowercase();
    Ok(match k.as_str() {
        "devices" | "device" | "dev" => [Kind::Devices].into(),
        "cables" | "cable" | "cbl" => [Kind::Cables].into(),
        "wires" | "wire" => [Kind::Wires].into(),
        "all" => [Kind::Devices, Kind::Cables, Kind::Wires].into(),
        other => return Err(format!("`kind` must be \"devices\", \"cables\", \"wires\" or \"all\", not {other:?}")),
    })
}

fn hex(v: &Value) -> Option<u128> {
    v.as_str().and_then(|t| u128::from_str_radix(t.trim(), 16).ok())
}

fn scope_param(doc: &Document, params: &Value) -> Result<Scope, String> {
    let scope = params.get("scope").filter(|v| !v.is_null());
    match scope {
        None => Ok(Scope::All),
        Some(Value::String(w)) => match w.trim().to_ascii_lowercase().as_str() {
            "all" => Ok(Scope::All),
            "provisional" | "unnumbered" => Ok(Scope::Provisional),
            "selection" => {
                let list =
                    params.get("objects").and_then(Value::as_array).ok_or("`objects` (array of hex ids) is required with scope \"selection\"")?;
                if list.len() > crate::commands::MAX_HANDLES {
                    return Err(format!("at most {} entries in `objects` per call", crate::commands::MAX_HANDLES));
                }
                let (mut components, mut cables, mut conductors) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
                for v in list {
                    let n = hex(v).ok_or_else(|| format!("`{v}` in `objects` is not a valid hex id"))?;
                    if doc.components.contains_key(&ComponentId(n)) {
                        components.insert(ComponentId(n));
                    } else if doc.cables.contains_key(&CableId(n)) {
                        cables.insert(CableId(n));
                    } else if doc.conductors.contains_key(&ConductorId(n)) {
                        conductors.insert(ConductorId(n));
                    } else {
                        return Err(format!("{} in `objects` is not a device, cable or wire.", v.as_str().unwrap_or_default()));
                    }
                }
                Ok(Scope::Selection { components, cables, conductors })
            }
            other => Err(format!("`scope` must be \"all\", \"selection\", \"provisional\", {{prefix}}, {{folder}} or {{pages}}, not {other:?}")),
        },
        Some(Value::Object(o)) => {
            if let Some(p) = o.get("prefix") {
                let p = p.as_str().map(str::trim).filter(|p| !p.is_empty()).ok_or("`prefix` must be letters, such as \"-K\", or a potential")?;
                return Ok(Scope::Prefix(p.to_string()));
            }
            if let Some(f) = o.get("folder") {
                let f = hex(f).map(FolderId).ok_or("`folder` (hex id) is required")?;
                if !doc.folders.contains_key(&f) {
                    return Err("No such folder.".into());
                }
                return Ok(Scope::Folder(f));
            }
            if let Some(ps) = o.get("pages") {
                let a = ps.as_array().ok_or("`pages` must be an array of hex ids")?;
                let mut pages = BTreeSet::new();
                for v in a.iter().take(100_000) {
                    let p = hex(v).map(PageId).ok_or_else(|| format!("`{v}` in `pages` is not a valid hex id"))?;
                    if !doc.pages.contains_key(&p) {
                        return Err(format!("No page {}.", p.hex()));
                    }
                    pages.insert(p);
                }
                return Ok(Scope::Pages(pages));
            }
            Err("`scope` as an object is {prefix}, {folder: id} or {pages: [id]}".into())
        }
        Some(_) => Err("`scope` must be a word or an object".into()),
    }
}

fn request(doc: &Document, params: &Value) -> Result<Request, String> {
    let kinds = kinds_param(params)?;
    let scope = scope_param(doc, params)?;
    let start = match params.get("start") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_u64().ok_or(numbering::BAD_START)?),
    };
    let digits = match params.get("digits") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_u64().and_then(|d| u8::try_from(d).ok()).ok_or(numbering::BAD_DIGITS)?),
    };
    Ok(Request { kinds, scope, start, digits })
}

fn bool_param(params: &Value, key: &str) -> Result<bool, String> {
    Ok(opt_typed(params, key, "true or false", Value::as_bool)?.unwrap_or(false))
}

// ---------------------------------------------------------------- words

fn kind_label(kinds: &BTreeSet<Kind>) -> &'static str {
    match kinds.iter().collect::<Vec<_>>().as_slice() {
        [Kind::Devices] => "Renumber devices",
        [Kind::Cables] => "Renumber cables",
        [Kind::Wires] => "Renumber wires",
        _ => "Renumber",
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The echo after Apply: `Renumbered 42 devices (3 fixed kept). U undoes it.`
pub fn applied_words(plan: &Plan, finalise: bool) -> String {
    let mut parts = Vec::new();
    for (k, one, many) in [(Kind::Devices, "device", "devices"), (Kind::Cables, "cable", "cables"), (Kind::Wires, "net", "nets")] {
        let n = plan.changes_of(k);
        if n > 0 {
            parts.push(plural(n, one, many));
        }
    }
    let what = if parts.is_empty() { "nothing".to_string() } else { parts.join(", ") };
    let verb = if finalise { "Finalised" } else { "Renumbered" };
    let fixed = plan.fixed();
    let kept = if fixed > 0 { format!(" ({fixed} fixed kept)") } else { String::new() };
    format!("{verb} {what}{kept}. U undoes it.")
}

fn row_json(r: &numbering::Row) -> Value {
    json!({
        "page": r.page.map(|p| p.hex()),
        "point": r.point.map(|a| [a.x, a.y]),
        "object": numbering::object_id(r.object).to_string(),
        "id": format!("{:032x}", numbering::object_id(r.object)),
        "kind": r.kind.word(),
        "old": r.old,
        "new": r.new,
        "at": r.at,
        "fixed": r.mark == Mark::Fixed,
        "change": r.mark.word(),
        "mark": r.mark.glyph(),
        "why": match &r.mark { Mark::Skipped(w) => Value::String((*w).to_string()), _ => Value::Null },
    })
}

/// The preview as JSON (design §13).
pub fn preview_json(plan: &Plan, change_count: u64) -> Value {
    let rows: Vec<Value> = plan.rows.iter().filter(|r| !matches!(r.mark, Mark::Skipped(_))).take(MAX_PREVIEW_ROWS).map(row_json).collect();
    let skipped: Vec<Value> = plan
        .rows
        .iter()
        .filter_map(|r| match &r.mark {
            Mark::Skipped(why) => {
                Some(json!({"id": format!("{:032x}", numbering::object_id(r.object)), "kind": r.kind.word(), "tag": r.old, "why": why}))
            }
            _ => None,
        })
        .take(MAX_PREVIEW_ROWS)
        .collect();
    let listed = plan.rows.len() - plan.skipped();
    json!({
        "rows": rows,
        "skipped": skipped,
        "changes": plan.changes(),
        "fixed": plan.fixed(),
        "same": plan.same(),
        "skipped_count": plan.skipped(),
        "more": listed.saturating_sub(MAX_PREVIEW_ROWS),
        "change_count": change_count,
        "summary": summary_line(plan),
    })
}

/// `42 to change · 3 fixed · 7 same · 1 skipped`.
pub fn summary_line(plan: &Plan) -> String {
    format!("{} to change · {} fixed · {} same · {} skipped", plan.changes(), plan.fixed(), plan.same(), plan.skipped())
}

// ---------------------------------------------------------------- renumber and finalise

/// Plan `req` on the project (reads only).
///
/// # Errors
/// As [`numbering::plan`].
pub fn plan_for(p: &mut Project, req: &Request) -> Result<Plan, String> {
    p.tag_high.catch_up(&p.ledger);
    numbering::plan(p.ledger.document(), req, p.tag_high.marks())
}

fn renumber_with(s: &mut Session, params: &Value, finalise: bool) -> cadcraft_engine::Result<Value> {
    let preview = bool_param(params, "preview").map_err(err)?;
    let based_on = opt_typed(params, "based_on", "a change count", Value::as_u64).map_err(err)?;
    let for_new = bool_param(params, "digits_for_new").map_err(err)?;
    let run = |p: &mut Project, pg: &mut crate::access::Pages<'_>| -> Result<Value, String> {
        let doc = p.ledger.document();
        let req = if finalise {
            Request { kinds: [Kind::Devices, Kind::Cables, Kind::Wires].into(), scope: Scope::Provisional, start: None, digits: None }
        } else {
            request(doc, params)?
        };
        let plan = plan_for(p, &req)?;
        let count = p.change_count();
        if preview {
            return Ok(preview_json(&plan, count));
        }
        if let Some(b) = based_on
            && b != count
        {
            return Err(STALE.into());
        }
        let mut ops = plan.ops.clone();
        if for_new && let Some(d) = req.digits {
            let doc = p.ledger.document();
            let before = doc.settings.clone();
            let mut after = before.clone();
            if req.kinds.contains(&Kind::Devices) || req.kinds.contains(&Kind::Cables) {
                after.tag_padding = d;
            }
            if req.kinds.contains(&Kind::Wires) {
                after.wiring.digits = d;
            }
            if after != before {
                ops.push(Op::SetProjectSettings { before, after });
            }
        }
        if ops.is_empty() {
            return Ok(json!({"renumbered": 0, "fixed_kept": plan.fixed(), "message": "Nothing to change."}));
        }
        let label = if finalise { "Finalise numbering" } else { kind_label(&req.kinds) };
        p.apply_edit(pg, if finalise { "finalise" } else { "renumber" }, label, ops)?;
        let line = applied_words(&plan, finalise);
        pg.echo(line.clone());
        Ok(json!({
            "renumbered": plan.changes(),
            "devices": plan.changes_of(Kind::Devices),
            "cables": plan.changes_of(Kind::Cables),
            "wires": plan.changes_of(Kind::Wires),
            "fixed_kept": plan.fixed(),
            "message": line,
        }))
    };
    if preview { with_any_page(s, run) } else { with_project(s, |p, pg, _| run(p, pg)) }
}

/// `renumber {kind?, scope?, objects?, start?, digits?, digits_for_new?, preview?, based_on?}`
/// (design §3, §13).
pub(crate) fn run_renumber(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    renumber_with(s, params, false)
}

/// `finalise {preview?, based_on?}` = `renumber {kind: "all", scope: "provisional"}` (design §5.3).
pub(crate) fn run_finalise(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    renumber_with(s, params, true)
}

/// `numbering {provisional?}` (design §5.1, FP2).
pub(crate) fn run_numbering(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let prov = opt_typed(params, "provisional", "true or false", Value::as_bool).map_err(err)?;
    let read = |doc: &Document| json!({"provisional": numbering::provisional_on(doc), "padding": devices::padding(doc), "cable_prefix": format!("-{}", devices::cable_prefix(doc))});
    match prov {
        None => with_any_page(s, |p, _| Ok(read(p.ledger.document()))),
        Some(on) => with_project(s, |p, pg, _| {
            let doc = p.ledger.document();
            let before = doc.properties.get(PROVISIONAL_SETTING).cloned();
            let after = on.then(|| "yes".to_string());
            if before != after {
                p.apply_edit(
                    pg,
                    "numbering",
                    "Set numbering",
                    vec![Op::SetProperty { object: ObjectRef::Project, key: PROVISIONAL_SETTING.into(), before, after }],
                )?;
            }
            Ok(read(p.ledger.document()))
        }),
    }
}

/// What the Renumber dialog offers (design §3): the tag prefixes in use (`-K`, `-Q`…, provisional
/// ones included), the potentials of the wire table, the folders as paths (`Control / Motors`),
/// and how many provisional items there are. Read when the dialog opens.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Choices {
    pub prefixes: Vec<String>,
    pub potentials: Vec<String>,
    pub folders: Vec<(FolderId, String)>,
    pub provisional: usize,
}

/// The dialog's choices for `doc`.
pub fn choices(doc: &Document) -> Choices {
    let mut prefixes: BTreeSet<String> = BTreeSet::new();
    for k in doc.tagged().map(|(k, _)| k).chain(doc.tagged_cables().map(|(k, _)| k)) {
        if let Some(p) = tag::prefix_of(k) {
            prefixes.insert(p);
        }
    }
    let potentials = doc.settings.wiring.potentials.iter().filter(|p| p.numbered).map(|p| p.name.clone()).collect();
    let mut folders: Vec<(FolderId, String)> = doc
        .folder_order
        .iter()
        .filter_map(|f| {
            let mut names = Vec::new();
            let mut at = Some(*f);
            for _ in 0..ecad_model::book::MAX_FOLDER_DEPTH {
                let Some(id) = at else { break };
                let folder = doc.folders.get(&id)?;
                names.push(folder.name.clone());
                at = folder.parent;
            }
            names.reverse();
            Some((*f, names.join(" / ")))
        })
        .collect();
    folders.sort_by(|a, b| a.1.cmp(&b.1));
    let items = provisional_items(doc);
    Choices { prefixes: prefixes.into_iter().map(|p| format!("-{p}")).collect(), potentials, folders, provisional: items.tags.len() + items.nets }
}

// ---------------------------------------------------------------- provisional summary (§5.5)

/// The provisional items of a project: the provisional tags (devices and cables, in tag order) and,
/// with provisional tags on, the unnumbered nets per potential.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Provisional {
    pub tags: Vec<String>,
    pub nets: usize,
    pub by_potential: Vec<(String, usize)>,
}

impl Provisional {
    /// Whether nothing is provisional.
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty() && self.nets == 0
    }

    /// `3 provisional tags and 12 wire numbers are not final`.
    pub fn words(&self) -> String {
        let t = plural(self.tags.len(), "provisional tag", "provisional tags");
        let w = plural(self.nets, "wire number", "wire numbers");
        let are = if self.tags.len() + self.nets == 1 { "is" } else { "are" };
        match (self.tags.is_empty(), self.nets == 0) {
            (false, true) => format!("{t} {are} not final"),
            (true, false) => format!("{w} {are} not final"),
            _ => format!("{t} and {w} are not final"),
        }
    }

    /// `-K?1  -K?2  -W?1   · 12 nets (24V ×8, 0V ×4)`.
    pub fn list_line(&self) -> String {
        let mut s = self.tags.iter().take(30).cloned().collect::<Vec<_>>().join("  ");
        if self.tags.len() > 30 {
            s.push_str(&format!("  +{}", self.tags.len() - 30));
        }
        if self.nets > 0 {
            let pots = self.by_potential.iter().map(|(p, n)| format!("{p} ×{n}")).collect::<Vec<_>>().join(", ");
            if !s.is_empty() {
                s.push_str("   · ");
            }
            s.push_str(&format!("{} ({pots})", plural(self.nets, "net", "nets")));
        }
        s
    }
}

/// The provisional items of `doc` (read when a revision is issued: a walk over the tag index and
/// the nets).
pub fn provisional_items(doc: &Document) -> Provisional {
    let mut tags: Vec<String> = doc
        .tagged()
        .filter_map(|(_, c)| devices::tag_of(doc, c).filter(|t| tag::is_provisional(t)).map(str::to_string))
        .chain(doc.tagged_cables().filter_map(|(_, k)| devices::cable_tag_of(doc, k).filter(|t| tag::is_provisional(t)).map(str::to_string)))
        .collect();
    tags.sort_by_key(|t| tag::provisional(t));
    let mut by: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut nets = 0;
    // Unnumbered nets are provisional only with provisional tags on; otherwise they are 022's
    // nets with no number yet (an Info row), never a reason to stop an issue.
    let on = numbering::provisional_on(doc);
    for (_, n) in doc.connectivity().nets().filter(|_| on) {
        if n.conductors.is_empty() || !n.numbers.is_empty() {
            continue;
        }
        let pot = ecad_model::wiring::net_potential(doc, n);
        if !pot.numbered {
            continue;
        }
        nets += 1;
        *by.entry(pot.name.unwrap_or_else(|| "?".into())).or_default() += 1;
    }
    let mut by_potential: Vec<(String, usize)> = by.into_iter().collect();
    by_potential.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Provisional { tags, nets, by_potential }
}

/// `issuerevision`'s provisional step (design §5.5): `Ok(note)` to go on (the note for the echo),
/// after finalising first when asked.
///
/// # Errors
/// Provisional items present and no `provisional` given (the ways forward), or a bad value.
pub(crate) fn before_issue(p: &mut Project, pg: &mut crate::access::Pages<'_>, params: &Value) -> Result<Option<String>, String> {
    let way = opt_typed(params, "provisional", "\"finalise\" or \"issue\"", Value::as_str)?.map(|w| w.trim().to_ascii_lowercase());
    let items = provisional_items(p.ledger.document());
    if items.is_empty() {
        return Ok(None);
    }
    match way.as_deref() {
        None => Err(format!(
            "{}. Give provisional: \"finalise\" (number them first, in one transaction before the issue) or \"issue\" (issue as they are).",
            items.words()
        )),
        Some("issue") => Ok(Some(if items.tags.is_empty() {
            format!("({} issued)", plural(items.nets, "provisional wire number", "provisional wire numbers"))
        } else {
            format!("({} issued)", plural(items.tags.len(), "provisional tag", "provisional tags"))
        })),
        Some("finalise" | "finalize") => {
            let req = Request { kinds: [Kind::Devices, Kind::Cables, Kind::Wires].into(), scope: Scope::Provisional, start: None, digits: None };
            let plan = plan_for(p, &req)?;
            if !plan.ops.is_empty() {
                p.apply_edit(pg, "finalise", "Finalise numbering", plan.ops.clone())?;
                pg.echo(applied_words(&plan, true));
            }
            Ok(None)
        }
        Some(other) => Err(format!("`provisional` must be \"finalise\" or \"issue\", not {other:?}")),
    }
}

// ---------------------------------------------------------------- setprop: tags (§4, §7)

/// How a taken tag is resolved (design §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conflict {
    Swap,
    Merge,
    Next,
}

fn conflict_param(params: &Value) -> Result<Option<Conflict>, String> {
    let merge = opt_typed(params, "merge", "true or false", Value::as_bool)?.unwrap_or(false);
    let c = match opt_typed(params, "conflict", "\"swap\", \"merge\" or \"next\"", Value::as_str)?.map(|w| w.trim().to_ascii_lowercase()) {
        None => None,
        Some(w) => Some(match w.as_str() {
            "swap" => Conflict::Swap,
            "merge" | "same" | "one" => Conflict::Merge,
            "next" | "another" => Conflict::Next,
            other => return Err(format!("`conflict` must be \"swap\", \"merge\" or \"next\", not {other:?}")),
        }),
    };
    Ok(c.or(merge.then_some(Conflict::Merge)))
}

fn props_of(doc: &Document, o: ObjectRef) -> Option<&ecad_model::Properties> {
    match o {
        ObjectRef::Component(c) => doc.components.get(&c).map(|x| &x.properties),
        ObjectRef::Cable(k) => doc.cables.get(&k).map(|x| &x.properties),
        _ => None,
    }
}

fn set(o: ObjectRef, key: &str, before: Option<&String>, after: Option<String>) -> Option<Op> {
    (before != after.as_ref()).then(|| Op::SetProperty { object: o, key: key.into(), before: before.cloned(), after })
}

/// The ops writing `tag` on `o` and its fixed flag (`fixed`; a provisional tag is never fixed).
fn tag_ops(doc: &Document, o: ObjectRef, tag_value: Option<String>, fixed: bool) -> Vec<Op> {
    let Some(props) = props_of(doc, o) else { return Vec::new() };
    let fixed = fixed && tag_value.as_deref().is_some_and(|t| !tag::is_provisional(t));
    let mut ops = Vec::new();
    ops.extend(set(o, TAG_KEY, props.get(TAG_KEY), tag_value));
    ops.extend(set(o, FIXED_KEY, props.get(FIXED_KEY), fixed.then(|| "yes".to_string())));
    ops
}

fn noun(o: ObjectRef) -> &'static str {
    match o {
        ObjectRef::Cable(_) => "cable",
        _ => "device",
    }
}

/// The refusal of a taken tag with all three ways (design §7.1, 022's wording pattern).
fn taken_words(doc: &Document, p: &Project, o: ObjectRef, t: &str, holder: Holder) -> String {
    let next = tag::prefix_of(t).and_then(|px| p.tag_high.peek(doc, &px).ok());
    let next = next.map_or_else(|| "another tag".to_string(), |n| n.to_string());
    let same_kind = matches!((o, holder), (ObjectRef::Component(_), Holder::Device(_)) | (ObjectRef::Cable(_), Holder::Cable(_)));
    let ways = if same_kind {
        format!("Give conflict: \"swap\" (exchange the two tags), \"merge\" (make this the same {}) or \"next\" (use {next}).", noun(o))
    } else {
        format!("It is a {}, so only conflict: \"next\" (use {next}) works.", noun(holder.object()))
    };
    format!("{t} is used by {}. {ways}", devices::holder_words_of(doc, holder))
}

/// `setprop {component|cable, key: "tag", value, conflict?, merge?}` (design §4, §7): a typed tag
/// is fixed with it (Q4); `-K?` takes the next provisional number; a tag another device or cable
/// holds needs `conflict` (`merge: true` = `"merge"`).
pub(crate) fn run_set_tag(s: &mut Session, params: &Value, o: ObjectRef) -> cadcraft_engine::Result<Value> {
    let value = match params.get("value") {
        Some(Value::Null) => None,
        Some(Value::String(v)) => Some(v.clone()),
        Some(_) => return Err(err("`value` must be a string, or null to clear the property")),
        None => return Err(err("`value` is required (a string, or null to clear the property)")),
    };
    let conflict = conflict_param(params).map_err(err)?;
    // Merging two devices keeps 018's path (labels, contacts, connection points).
    if conflict == Some(Conflict::Merge)
        && let ObjectRef::Component(c) = o
    {
        let mut q = params.clone();
        if let Some(m) = q.as_object_mut() {
            m.insert("merge".into(), Value::Bool(true));
        }
        return crate::device_cmds::run_setprop(s, &q, c, TAG_KEY.into());
    }
    with_project(s, |p, pg, _| {
        p.tag_high.catch_up(&p.ledger);
        let doc = p.ledger.document();
        let props = props_of(doc, o).ok_or("no such object")?;
        let before = props.get(TAG_KEY).cloned();
        let value = match value.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            None => None,
            Some(v) => {
                let t = tag::normalise(v, devices::padding(doc))?;
                match tag::provisional(&t) {
                    Some((px, 0)) => Some(p.tag_high.next_provisional_in(doc, &px)),
                    _ => Some(t),
                }
            }
        };
        let doc = p.ledger.document();
        let Some(t) = value else {
            let ops = tag_ops(doc, o, None, false);
            if ops.is_empty() {
                return Ok(json!({"changed": false, "value": Value::Null}));
            }
            p.apply_edit(pg, "setprop", "Set tag", ops)?;
            return Ok(json!({"changed": true, "value": Value::Null}));
        };
        let typed_fixed = !tag::is_provisional(&t);
        let Some(holder) = devices::tag_holder(doc, &t, Some(o)) else {
            let ops = tag_ops(doc, o, Some(t.clone()), typed_fixed);
            if ops.is_empty() {
                return Ok(json!({"changed": false, "value": t}));
            }
            p.apply_edit(pg, "setprop", "Set tag", ops)?;
            return Ok(json!({"changed": true, "value": t, "fixed": typed_fixed}));
        };
        let same_kind = matches!((o, holder), (ObjectRef::Component(_), Holder::Device(_)) | (ObjectRef::Cable(_), Holder::Cable(_)));
        match conflict {
            None => Err(taken_words(doc, p, o, &t, holder)),
            Some(Conflict::Swap) => {
                if !same_kind {
                    return Err(format!("{t} is a {}; swap needs two of a kind. Give conflict: \"next\".", noun(holder.object())));
                }
                let Some(mine) = before.clone() else {
                    return Err(format!("This {} has no tag to give the other; give conflict: \"next\" or \"merge\".", noun(o)));
                };
                let h = holder.object();
                let h_fixed = props_of(doc, h).is_some_and(numbering::is_fixed);
                let mut ops = tag_ops(doc, o, Some(t.clone()), typed_fixed);
                ops.extend(tag_ops(doc, h, Some(mine.clone()), h_fixed));
                p.apply_edit(pg, "setprop", "Swap tags", ops)?;
                let other = if tag::is_provisional(&mine) { format!("provisional {mine}") } else { mine.clone() };
                let line = format!("Swapped: this is {t}, the other is now {other}.");
                pg.echo(line.clone());
                Ok(json!({"changed": true, "swapped": true, "value": t, "other": h_object(h), "other_value": mine, "message": line}))
            }
            Some(Conflict::Merge) => {
                // Two cables: the members join the holder (design §7.1); the slash follows.
                let (ObjectRef::Cable(from), Holder::Cable(into)) = (o, holder) else {
                    return Err(format!("{t} is a {}; only two of a kind can be one. Give conflict: \"next\".", noun(holder.object())));
                };
                let ops = join_cable_ops(doc, from, into)?;
                p.apply_edit(pg, "setprop", &format!("Merge into {t}"), ops)?;
                let doc = p.ledger.document();
                let n = doc.cables.get(&into).map_or(0, |c| c.members.len());
                let line = format!("{} is now part of {t} ({}).", before.unwrap_or_else(|| "The cable".into()), plural(n, "wire", "wires"));
                pg.echo(line.clone());
                Ok(json!({"changed": true, "merged": true, "cable": into.hex(), "tag": t, "message": line}))
            }
            Some(Conflict::Next) => {
                let px = tag::prefix_of(&t).ok_or_else(|| format!("{t} has no number to follow; type another tag."))?;
                let next = if tag::is_provisional(&t) { p.tag_high.next_provisional_in(doc, &px) } else { p.tag_high.next_in(doc, &px)? };
                let doc = p.ledger.document();
                let fixed = !tag::is_provisional(&next);
                let ops = tag_ops(doc, o, Some(next.clone()), fixed);
                p.apply_edit(pg, "setprop", "Set tag", ops)?;
                let line = format!("{} is now {next}.", before.unwrap_or_else(|| format!("The {}", noun(o))));
                pg.echo(line.clone());
                Ok(json!({"changed": true, "value": next, "fixed": fixed, "message": line}))
            }
        }
    })
}

fn h_object(o: ObjectRef) -> Value {
    match o {
        ObjectRef::Component(c) => json!({"component": c.hex()}),
        ObjectRef::Cable(k) => json!({"cable": k.hex()}),
        _ => Value::Null,
    }
}

/// The ops making cable `from` part of cable `into`: its members join (after `into`'s own; a wire
/// in both stays once), its drawings show `into`, and it is deleted.
///
/// # Errors
/// A missing cable.
pub fn join_cable_ops(doc: &Document, from: CableId, into: CableId) -> Result<Vec<Op>, String> {
    if from == into {
        return Err("A cable cannot be merged with itself.".into());
    }
    let a = doc.cables.get(&from).ok_or("No such cable.")?;
    let b = doc.cables.get(&into).ok_or("No such cable.")?;
    let mut ops = Vec::new();
    for (i, m) in a.members.iter().enumerate().rev() {
        ops.push(Op::RemoveCableMember { cable: from, index: i, member: m.clone() });
    }
    let mut n = b.members.len();
    for m in &a.members {
        if b.members.iter().any(|x| x.conductor == m.conductor) {
            continue;
        }
        ops.push(Op::AddCableMember { cable: into, index: n, member: m.clone() });
        n += 1;
    }
    for (rid, r) in doc.representations_of(Subject::Cable(from)) {
        ops.push(Op::UnlinkRepresentation { id: rid, rep: r.clone() });
        ops.push(Op::LinkRepresentation { id: rid, rep: ecad_model::Representation { subject: Subject::Cable(into), ..r.clone() } });
    }
    let mut empty = a.clone();
    empty.members.clear();
    ops.push(Op::DeleteCable { id: from, cable: empty });
    Ok(ops)
}

/// `setprop {component|cable, key: "tag_fixed", value: "yes"|null}` (design §4, FP1).
pub(crate) fn run_set_fixed(s: &mut Session, params: &Value, o: ObjectRef) -> cadcraft_engine::Result<Value> {
    let on = match params.get("value") {
        Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::String(v)) => match v.trim().to_ascii_lowercase().as_str() {
            "yes" | "true" => true,
            "" | "no" | "false" => false,
            other => return Err(err(format!("`tag_fixed` is \"yes\" or null, not {other:?}"))),
        },
        _ => return Err(err("`value` is required (\"yes\", or null to release)")),
    };
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let props = props_of(doc, o).ok_or("no such object")?;
        if on {
            match props.get(TAG_KEY) {
                None => return Err(format!("This {} has no tag to fix.", noun(o))),
                Some(t) if tag::is_provisional(t) => return Err(PROVISIONAL_FIXED.into()),
                Some(_) => {}
            }
        }
        let Some(op) = set(o, FIXED_KEY, props.get(FIXED_KEY), on.then(|| "yes".to_string())) else {
            return Ok(json!({"changed": false, "fixed": on}));
        };
        p.apply_edit(pg, "setprop", if on { "Fix tag" } else { "Release tag" }, vec![op])?;
        Ok(json!({"changed": true, "fixed": on}))
    })
}
