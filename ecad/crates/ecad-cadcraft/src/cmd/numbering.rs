//! Brief 026's commands (design 026 §3–§7, §13): `renumber`, `finalise` and `numbering`. The tag
//! rules of `setprop` (swap, make one, next free; typing a tag fixes it; `tag_fixed`) are
//! [`ecad_electrical::cmd::prop`]'s.
//!
//! The interactive forms (the Renumber dialog, the clash box, Project Settings) commit through
//! exactly these JSON forms. The plans come from [`ecad_model::numbering`]; this module turns them
//! into one transaction each and into words. The registered commands are thin shims (brief 070):
//! their params structs read the JSON and [`ecad_electrical::cmd::numbering`] reads what a renumber
//! covers.

use std::collections::BTreeSet;

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::cmd::numbering::{self as rules, RenumberInput, ScopeInput};
use ecad_model::numbering::{self, Kind, Mark, Plan, Request};
use ecad_model::tag::{self, PROVISIONAL_SETTING};
use ecad_model::{Document, FolderId, ObjectRef, Op, PageId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cmd::input::{Command, Typed, nullable, typed};
use crate::cmd::{on_any_page, on_project, opt_typed};
use crate::devices;
use crate::project::Project;

/// Most rows a JSON preview lists (`more` counts the rest).
pub const MAX_PREVIEW_ROWS: usize = 2_000;

/// Refusal of an Apply on a stale preview (design §3).
pub const STALE: &str = "The project changed since the preview; check the new list and Apply again.";

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] =
    &[Typed::of::<RenumberParams>("renumber"), Typed::of::<FinaliseParams>("finalise"), Typed::of::<NumberingParams>("numbering")];

/// The commands of this module.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("renumber", "Renumber", typed::<RenumberParams>)
            .params(
                "{kind?: \"devices\"|\"cables\"|\"wires\"|\"terminals\"|\"all\", strips?: [id] (terminals, with scope \"selection\"), scope?: \"all\"|\"selection\"|\"provisional\"|{prefix}|{folder: id}|{pages: [id]}, objects?: [id] (selection), start?: n, digits?: 1..6, digits_for_new?: bool, preview?: bool, based_on?: change count} (drawing order; fixed tags kept; one transaction)",
            )
            .noundo(),
        CommandSpec::new("finalise", "Finalise Numbering", typed::<FinaliseParams>)
            .params("{preview?: bool, based_on?: change count} (numbers the provisional tags and unnumbered nets in drawing order; one transaction)")
            .noundo(),
        CommandSpec::new("numbering", "Numbering Settings", typed::<NumberingParams>)
            .params("{provisional?: bool} (no parameters reads; provisional: new tags are -K?1 until Finalise)")
            .noundo(),
    ]
}

// ---------------------------------------------------------------- parameters

/// `scope` as an object: `{prefix}`, `{folder: id}` or `{pages: [id]}`.
#[derive(Deserialize, JsonSchema)]
struct ScopePick {
    /// The tag prefix (`-K`) or the potential to renumber.
    #[serde(default, deserialize_with = "nullable")]
    prefix: Option<Option<String>>,
    /// The folder whose pages to renumber.
    #[serde(default, deserialize_with = "nullable")]
    folder: Option<Option<FolderId>>,
    /// The pages to renumber.
    #[serde(default, deserialize_with = "nullable")]
    pages: Option<Option<Vec<PageId>>>,
}

/// `scope`: a word or an object.
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
enum ScopeParam {
    /// "all", "selection" or "provisional".
    Word(String),
    /// {prefix}, {folder: id} or {pages: [id]}.
    Pick(ScopePick),
}

/// `renumber {kind?, strips?, scope?, objects?, start?, digits?, digits_for_new?, preview?,
/// based_on?}` (design §3, §13): drawing order; fixed tags kept; one transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RenumberParams {
    /// What to renumber: devices, cables, wires, terminals or all (default devices).
    #[serde(default)]
    kind: Option<String>,
    /// Terminals with scope "selection": the strips (or terminals of them), as hex ids.
    #[serde(default, deserialize_with = "nullable")]
    strips: Option<Option<Vec<String>>>,
    /// "all", "selection", "provisional", {prefix}, {folder: id} or {pages: [id]}.
    #[serde(default)]
    scope: Option<ScopeParam>,
    /// With scope "selection": the devices, cables or wires, as hex ids.
    #[serde(default, deserialize_with = "nullable")]
    objects: Option<Option<Vec<String>>>,
    /// The first number.
    #[serde(default)]
    start: Option<u64>,
    /// Digits, 1 to 6.
    #[serde(default)]
    digits: Option<u64>,
    /// New tags and wire numbers take `digits` too.
    #[serde(default)]
    digits_for_new: Option<bool>,
    /// List the changes; write nothing.
    #[serde(default)]
    preview: Option<bool>,
    /// The preview's change count: refused if the project changed since.
    #[serde(default)]
    based_on: Option<u64>,
}

impl Command for RenumberParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let scope = self.scope.map(|sc| match sc {
            ScopeParam::Word(w) => ScopeInput::Word(w),
            ScopeParam::Pick(p) => ScopeInput::Pick { prefix: p.prefix, folder: p.folder, pages: p.pages },
        });
        let input = RenumberInput { kind: self.kind, scope, objects: self.objects, strips: self.strips, start: self.start, digits: self.digits };
        let how = How { preview: self.preview.unwrap_or(false), based_on: self.based_on, for_new: self.digits_for_new.unwrap_or(false) };
        renumber_with(s, Some(&input), how)
    }
}

/// `finalise {preview?, based_on?}` = `renumber {kind: "all", scope: "provisional"}` (design
/// §5.3): numbers the provisional tags and unnumbered nets in drawing order; one transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FinaliseParams {
    /// List the changes; write nothing.
    #[serde(default)]
    preview: Option<bool>,
    /// The preview's change count: refused if the project changed since.
    #[serde(default)]
    based_on: Option<u64>,
}

impl Command for FinaliseParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let how = How { preview: self.preview.unwrap_or(false), based_on: self.based_on, for_new: false };
        renumber_with(s, None, how)
    }
}

/// `numbering {provisional?}` (design §5.1, FP2): no parameters reads; `provisional` sets whether
/// new tags are provisional (`-K?1`) until Finalise.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NumberingParams {
    /// New tags are provisional until Finalise.
    #[serde(default)]
    provisional: Option<bool>,
}

impl Command for NumberingParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let read = |doc: &Document| json!({"provisional": numbering::provisional_on(doc), "padding": devices::padding(doc), "cable_prefix": format!("-{}", devices::cable_prefix(doc))});
        match self.provisional {
            None => on_any_page(s, |p, _| Ok(read(p.ledger.document()))),
            Some(on) => on_project(s, |p, pg, _| {
                let doc = p.ledger.document();
                let before = doc.properties.get(PROVISIONAL_SETTING).cloned();
                let after = on.then(|| ecad_model::words::flag_word(true).to_string());
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
}

// ---------------------------------------------------------------- words

fn kind_label(kinds: &BTreeSet<Kind>) -> &'static str {
    match kinds.iter().collect::<Vec<_>>().as_slice() {
        [Kind::Devices] => "Renumber devices",
        [Kind::Cables] => "Renumber cables",
        [Kind::Wires] => "Renumber wires",
        [Kind::Terminals] => "Renumber terminals",
        _ => "Renumber",
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The echo after Apply: `Renumbered 42 devices (3 fixed kept). U undoes it.`
pub fn applied_words(plan: &Plan, finalise: bool) -> String {
    let mut parts = Vec::new();
    for (k, one, many) in [
        (Kind::Devices, "device", "devices"),
        (Kind::Cables, "cable", "cables"),
        (Kind::Wires, "net", "nets"),
        (Kind::Terminals, "terminal", "terminals"),
    ] {
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

fn row_json(doc: &Document, r: &numbering::Row) -> Value {
    json!({
        "desc": desc(doc, r.object),
        "page": r.page.map(|p| p.hex()),
        "point": r.point.map(|a| [a.x, a.y]),
        "object": numbering::object_id(r.object).to_string(),
        "id": format!("{:032x}", numbering::object_id(r.object)),
        "kind": r.kind.word(),
        "terminal": r.terminal.map(|t| t.hex()),
        "old": r.old,
        "new": r.new,
        "at": r.at,
        "fixed": r.mark == Mark::Fixed,
        "change": r.mark.word(),
        "mark": r.mark.glyph(),
        "why": match &r.mark { Mark::Skipped(w) => Value::String((*w).to_string()), _ => Value::Null },
    })
}

/// A row's Desc: the device's description, `Cable, 3 wires`, or the net's potential.
fn desc(doc: &Document, o: ObjectRef) -> String {
    match o {
        ObjectRef::Component(c) => devices::description(doc, c),
        ObjectRef::Cable(k) => {
            let n = doc.cables.get(&k).map_or(0, |c| c.members.len());
            format!("Cable, {}", plural(n, "wire", "wires"))
        }
        ObjectRef::Conductor(w) => {
            let conn = doc.connectivity();
            conn.net_of_conductor(w).and_then(|id| conn.net(id)).and_then(|n| ecad_model::wiring::net_potential(doc, n).name).unwrap_or_default()
        }
        _ => String::new(),
    }
}

/// The preview as JSON (design §13).
pub fn preview_json(doc: &Document, plan: &Plan, change_count: u64) -> Value {
    let rows: Vec<Value> =
        plan.rows.iter().filter(|r| !matches!(r.mark, Mark::Skipped(_))).take(MAX_PREVIEW_ROWS).map(|r| row_json(doc, r)).collect();
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
pub fn plan_for(p: &mut Project, req: &Request) -> Result<Plan, CmdError> {
    p.tag_high.catch_up(&p.ledger);
    // Design 052 §4: a device is numbered at its head, else its first symbol in drawing order.
    let doc = p.ledger.document();
    numbering::plan_ranked(doc, req, p.tag_high.marks(), &|r| crate::reps::numbering_rank(doc, r)).map_err(CmdError::from)
}

/// How a renumber or finalise runs: a preview, the preview it applies, and whether new tags take
/// its digits.
#[derive(Clone, Copy)]
struct How {
    preview: bool,
    based_on: Option<u64>,
    for_new: bool,
}

/// Renumber (`input`) or finalise (`None`).
fn renumber_with(s: &mut Session, input: Option<&RenumberInput>, how: How) -> cadcraft_engine::Result<Value> {
    let finalise = input.is_none();
    let run = |p: &mut Project, pg: &mut crate::access::Pages<'_>| -> Result<Value, CmdError> {
        let doc = p.ledger.document();
        let req = match input {
            None => rules::finalise_request(),
            Some(i) => rules::request(doc, i, crate::cmd::MAX_HANDLES)?,
        };
        let plan = plan_for(p, &req)?;
        let count = p.change_count();
        // Brief 030 (FP4, FP7): accessories and links follow their relabelled terminals.
        let (mut acc_ops, mut acc_lines) = crate::accessories::renumber_ops(p.ledger.document(), &plan.ops);
        // Brief 047 (design 047 §2.3): a strip's part lines keep naming the positions they serve.
        let (for_ops, for_lines) = crate::bom::renumber_for_ops(p.ledger.document(), &plan.ops);
        acc_ops.extend(for_ops);
        acc_lines.extend(for_lines);
        if how.preview {
            let mut v = preview_json(p.ledger.document(), &plan, count);
            v["accessories"] = json!(acc_lines);
            return Ok(v);
        }
        if let Some(b) = how.based_on
            && b != count
        {
            return Err(CmdError::refused(STALE));
        }
        let mut ops = plan.ops.clone();
        ops.extend(acc_ops);
        if how.for_new
            && let Some(d) = req.digits
        {
            let doc = p.ledger.document();
            let before = doc.settings.clone();
            let mut after = before.clone();
            if req.kinds.contains(&Kind::Devices) || req.kinds.contains(&Kind::Cables) {
                after.tag_padding = Some(d);
            }
            if req.kinds.contains(&Kind::Wires) {
                after.wiring.digits = Some(d);
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
            "terminals": plan.changes_of(Kind::Terminals),
            "fixed_kept": plan.fixed(),
            "message": line,
        }))
    };
    if how.preview { on_any_page(s, run) } else { on_project(s, |p, pg, _| run(p, pg)) }
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

/// What the provisional step before an issue will do (design §5.5), worked out without writing.
#[derive(Debug, Clone)]
pub enum BeforeIssue {
    /// Go on to the issue; the note for the echo (`(2 provisional tags issued)`), if any.
    Go(Option<String>),
    /// Finalise numbering first, with this plan (its own transaction, `Finalise numbering`).
    Finalise(Plan),
}

/// Plan the provisional step before an issue (reads only; brief 027's release stages from it
/// before anything is recorded).
///
/// # Errors
/// Provisional items present and no `provisional` given (the ways forward), or a bad value.
pub fn plan_before_issue(p: &mut Project, params: &Value) -> Result<BeforeIssue, CmdError> {
    let way = opt_typed(params, "provisional", "\"finalise\" or \"issue\"", Value::as_str)?;
    plan_before(p, way)
}

/// [`plan_before_issue`] with the `provisional` word `way` read.
///
/// # Errors
/// Provisional items present and no `way` given (the ways forward), or a word that is neither.
pub fn plan_before(p: &mut Project, way: Option<&str>) -> Result<BeforeIssue, CmdError> {
    let way = way.map(|w| w.trim().to_ascii_lowercase());
    let items = provisional_items(p.ledger.document());
    if items.is_empty() {
        return Ok(BeforeIssue::Go(None));
    }
    match way.as_deref() {
        None => Err(CmdError::refused(format!(
            "{}. Give provisional: \"finalise\" (number them first, in one transaction before the issue) or \"issue\" (issue as they are).",
            items.words()
        ))),
        Some("issue") => Ok(BeforeIssue::Go(Some(if items.tags.is_empty() {
            format!("({} issued)", plural(items.nets, "provisional wire number", "provisional wire numbers"))
        } else {
            format!("({} issued)", plural(items.tags.len(), "provisional tag", "provisional tags"))
        }))),
        Some("finalise" | "finalize") => {
            let plan = plan_for(p, &rules::finalise_request())?;
            Ok(if plan.ops.is_empty() { BeforeIssue::Go(None) } else { BeforeIssue::Finalise(plan) })
        }
        Some(other) => Err(CmdError::input(format!("`provisional` must be \"finalise\" or \"issue\", not {other:?}"))),
    }
}

/// Commit a planned Finalise numbering (one transaction) and echo what it did.
///
/// # Errors
/// The commit was refused.
pub(crate) fn commit_finalise(p: &mut Project, pg: &mut crate::access::Pages<'_>, plan: &Plan) -> Result<(), CmdError> {
    p.apply_edit(pg, "finalise", "Finalise numbering", plan.ops.clone())?;
    pg.echo(applied_words(plan, true));
    Ok(())
}

/// `issuerevision`'s provisional step (design §5.5): `Ok(note)` to go on (the note for the echo),
/// after finalising first when asked.
///
/// # Errors
/// Provisional items present and no `provisional` given (the ways forward), or a bad value.
pub(crate) fn before_issue(p: &mut Project, pg: &mut crate::access::Pages<'_>, way: Option<&str>) -> Result<Option<String>, CmdError> {
    match plan_before(p, way)? {
        BeforeIssue::Go(note) => Ok(note),
        BeforeIssue::Finalise(plan) => commit_finalise(p, pg, &plan).map(|()| None),
    }
}
