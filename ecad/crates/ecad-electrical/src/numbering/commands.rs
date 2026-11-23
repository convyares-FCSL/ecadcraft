//! The rules of `renumber`: what a
//! renumber covers, read from its typed input into an [`ecad_model::naming::numbering::Request`]. The plan
//! itself is [`ecad_model::naming::numbering`]'s; the engine crate's shim plans it against the project's
//! tag marks, commits and answers in JSON.

use std::collections::BTreeSet;

use ecad_model::naming::numbering::{self, Kind, Mark, Plan, Request, Scope};
use ecad_model::naming::tag;
use ecad_model::naming::tag::PROVISIONAL_SETTING;
use ecad_model::{CableId, ConnectionPointId, DeviceId, Document, FolderId, Ledger, Noun, ObjectRef, Op, PageId, WireId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::input::nullable;
use crate::commands::run::{Cx, Generic, Outcome, Pages};
use crate::devices::tags::TagHigh;
use crate::devices::{self};
use crate::error::CmdError;

/// The kinds `kind` names (absent: devices); `all` is devices, cables and wires.
///
/// # Errors
/// Any other word.
pub fn kinds(word: Option<&str>) -> Result<BTreeSet<Kind>, CmdError> {
    let k = word.unwrap_or("devices").trim().to_ascii_lowercase();
    Ok(match k.as_str() {
        "devices" | "device" | "dev" => [Kind::Devices].into(),
        "cables" | "cable" | "cbl" => [Kind::Cables].into(),
        "wires" | "wire" => [Kind::Wires].into(),
        // Terminal block labels per strip ("all" keeps its usual meaning: tags and wires).
        "terminal_blocks" | "terminal_block" | "trm" => [Kind::TerminalBlocks].into(),
        "all" => [Kind::Devices, Kind::Cables, Kind::Wires].into(),
        other => {
            return Err(CmdError::refused(format!(
                "`kind` must be \"devices\", \"cables\", \"wires\", \"terminal_blocks\" or \"all\", not {other:?}"
            )));
        }
    })
}

/// A renumber's `scope` as given: a word, or an object. Each of the object's entries: absent
/// (`None`), null (`Some(None)`) or given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScopeInput {
    /// `all`, `selection`, `provisional` (or `unnumbered`).
    Word(String),
    /// `{prefix}`, `{folder: id}` or `{pages: [id]}`, read in that order.
    Pick { prefix: Option<Option<String>>, folder: Option<Option<FolderId>>, pages: Option<Option<Vec<PageId>>> },
}

/// What `renumber` is given. `objects` and `strips` are hex IDs as written (a device, cable or
/// wire; a strip or one of its terminal blocks); `Some(None)`: given as null.
#[derive(Debug, Clone, Default)]
pub(crate) struct RenumberInput {
    pub kind: Option<String>,
    pub scope: Option<ScopeInput>,
    pub objects: Option<Option<Vec<String>>>,
    pub strips: Option<Option<Vec<String>>>,
    pub start: Option<u64>,
    pub digits: Option<u64>,
}

/// Most pages a `{pages}` scope reads.
const MAX_PAGES: usize = 100_000;

/// The ID hex `t` writes, else the refusal naming it in `list`.
fn hex(t: &str, list: &str) -> Result<u128, CmdError> {
    ecad_model::hex_id(t).ok_or_else(|| CmdError::input(format!("`{t:?}` in `{list}` is not an ID (32 hex digits)")))
}

/// The scope of a renumber.
///
/// # Errors
/// An unknown word, a selection without `objects`, an object that is none of the three, or an
/// ID the project does not have.
pub(crate) fn scope(doc: &Document, input: &RenumberInput, max_objects: usize) -> Result<Scope, CmdError> {
    match &input.scope {
        None => Ok(Scope::All),
        Some(ScopeInput::Word(w)) => match w.trim().to_ascii_lowercase().as_str() {
            "all" => Ok(Scope::All),
            "provisional" | "unnumbered" => Ok(Scope::Provisional),
            // A terminal block renumber may name its strips in `strips` alone.
            "selection" if input.objects.is_none() && input.strips.is_some() => {
                Ok(Scope::Selection { devices: BTreeSet::new(), cables: BTreeSet::new(), conductors: BTreeSet::new() })
            }
            "selection" => {
                let list = input
                    .objects
                    .as_ref()
                    .and_then(Option::as_ref)
                    .ok_or(CmdError::input("`objects` (array of hex ids) is required with scope \"selection\""))?;
                if list.len() > max_objects {
                    return Err(CmdError::input(format!("at most {max_objects} entries in `objects` per call")));
                }
                let (mut devices, mut cables, mut conductors) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
                for t in list {
                    let n = hex(t, "objects")?;
                    if doc.devices().contains_key(&DeviceId(n)) {
                        devices.insert(DeviceId(n));
                    } else if doc.cables().contains_key(&CableId(n)) {
                        cables.insert(CableId(n));
                    } else if doc.wires().contains_key(&WireId(n)) {
                        conductors.insert(WireId(n));
                    } else {
                        return Err(CmdError::refused(format!("{t} in `objects` is not a device, cable or wire.")));
                    }
                }
                Ok(Scope::Selection { devices, cables, conductors })
            }
            other => Err(CmdError::refused(format!(
                "`scope` must be \"all\", \"selection\", \"provisional\", {{prefix}}, {{folder}} or {{pages}}, not {other:?}"
            ))),
        },
        Some(ScopeInput::Pick { prefix, folder, pages }) => {
            if let Some(p) = prefix {
                let p = p
                    .as_deref()
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .ok_or(CmdError::refused("`prefix` must be letters, such as \"-K\", or a potential"))?;
                return Ok(Scope::Prefix(p.to_string()));
            }
            if let Some(f) = folder {
                let f = f.ok_or(CmdError::input("`folder` (hex id) is required"))?;
                if !doc.folders().contains_key(&f) {
                    return Err(CmdError::NotFound(Noun::Folder));
                }
                return Ok(Scope::Folder(f));
            }
            if let Some(ps) = pages {
                let a = ps.as_ref().ok_or(CmdError::input("`pages` must be an array of hex ids"))?;
                let mut set = BTreeSet::new();
                for p in a.iter().take(MAX_PAGES) {
                    if !doc.pages().contains_key(p) {
                        return Err(CmdError::refused(format!("No page {}.", p.hex())));
                    }
                    set.insert(*p);
                }
                return Ok(Scope::Pages(set));
            }
            Err(CmdError::input("`scope` as an object is {prefix}, {folder: id} or {pages: [id]}"))
        }
    }
}

/// The strips a terminal block renumber covers, from the scope: every
/// strip (all, folder, pages; the planner keeps those on the pages), the strips with that tag
/// prefix, or the strips named in `strips` or `objects` (selection; a terminal block ID names its strip).
///
/// # Errors
/// A named strip that is not a terminal strip or one of its terminal blocks.
pub fn strips(doc: &Document, named: &[String], scope: &Scope) -> Result<BTreeSet<DeviceId>, CmdError> {
    let all = || doc.devices().keys().copied().filter(|c| crate::terminal_strips::strip_of(doc, *c).is_some());
    Ok(match scope {
        Scope::All | Scope::Folder(_) | Scope::Pages(_) => all().collect(),
        Scope::Prefix(p) => {
            let p = p.trim_start_matches('-').to_ascii_uppercase();
            all().filter(|c| crate::devices::tags::tag_of(doc, *c).and_then(ecad_model::naming::tag::parse).is_some_and(|(q, _)| q == p)).collect()
        }
        Scope::Selection { devices, .. } => {
            let mut out: BTreeSet<DeviceId> = devices.iter().copied().filter(|c| crate::terminal_strips::strip_of(doc, *c).is_some()).collect();
            for t in named {
                let n = hex(t, "strips")?;
                let c = match doc.connection_points().get(&ConnectionPointId(n)) {
                    Some(t) => t.device(),
                    None => Some(DeviceId(n)),
                };
                let Some(c) = c.filter(|c| crate::terminal_strips::strip_of(doc, *c).is_some()) else {
                    return Err(CmdError::refused(format!("{t} in `strips` is not a terminal strip or one of its terminal blocks.")));
                };
                out.insert(c);
            }
            out
        }
        Scope::Provisional => BTreeSet::new(),
    })
}

/// The request a `renumber` input makes of `doc` (`max_named`: most entries of `objects`, and
/// most strips read from `strips`).
///
/// # Errors
/// As [`kinds`], [`scope`] and [`strips`]; digits that are not a small whole number.
pub(crate) fn request(doc: &Document, input: &RenumberInput, max_named: usize) -> Result<Request, CmdError> {
    let kinds = kinds(input.kind.as_deref())?;
    let scope = scope(doc, input, max_named)?;
    let digits = match input.digits {
        None => None,
        Some(d) => Some(u8::try_from(d).map_err(|_| CmdError::refused(numbering::BAD_DIGITS))?),
    };
    let strips = if kinds.contains(&Kind::TerminalBlocks) {
        let named: Vec<String> = input.strips.iter().flatten().flatten().take(max_named).cloned().collect();
        strips(doc, &named, &scope)?
    } else {
        BTreeSet::new()
    };
    // Each strip's positions, numbered as groups.
    let positions = strips.iter().map(|c| (*c, crate::terminal_strips::terminal_blocks::position_groups(doc, *c))).collect();
    Ok(Request { kinds, scope, start: input.start, digits, strips, positions })
}

/// `finalise`'s request: the provisional tags and unnumbered nets.
pub fn finalise_request() -> Request {
    Request {
        kinds: [Kind::Devices, Kind::Cables, Kind::Wires].into(),
        scope: Scope::Provisional,
        start: None,
        digits: None,
        strips: BTreeSet::new(),
        positions: Default::default(),
    }
}

// ---------------------------------------------------------------- words, choices and provisional items

/// Most rows a JSON preview lists (`more` counts the rest).
pub(crate) const MAX_PREVIEW_ROWS: usize = 2_000;

pub(crate) fn kind_label(kinds: &BTreeSet<Kind>) -> &'static str {
    match kinds.iter().collect::<Vec<_>>().as_slice() {
        [Kind::Devices] => "Renumber devices",
        [Kind::Cables] => "Renumber cables",
        [Kind::Wires] => "Renumber wires",
        [Kind::TerminalBlocks] => "Renumber terminal blocks",
        _ => "Renumber",
    }
}

pub fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The echo after Apply: `Renumbered 42 devices (3 fixed kept). U undoes it.`
pub fn applied_words(plan: &Plan, finalise: bool) -> String {
    let mut parts = Vec::new();
    for (k, one, many) in [
        (Kind::Devices, "device", "devices"),
        (Kind::Cables, "cable", "cables"),
        (Kind::Wires, "net", "nets"),
        (Kind::TerminalBlocks, "terminal_block", "terminal_blocks"),
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

pub(crate) fn row_json(doc: &Document, r: &numbering::Row) -> Value {
    json!({
        "desc": desc(doc, r.object),
        "page": r.page.map(|p| p.hex()),
        "point": r.point.map(|a| [a.x, a.y]),
        "object": numbering::object_id(r.object).to_string(),
        "id": format!("{:032x}", numbering::object_id(r.object)),
        "kind": r.kind.word(),
        "connection_point": r.connection_point.map(|t| t.hex()),
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
pub fn desc(doc: &Document, o: ObjectRef) -> String {
    match o {
        ObjectRef::Device(c) => crate::devices::tags::description(doc, c),
        ObjectRef::Cable(k) => {
            let n = doc.cables().get(&k).map_or(0, |c| c.cores.len());
            format!("Cable, {}", plural(n, "wire", "wires"))
        }
        ObjectRef::Wire(w) => {
            let conn = doc.connectivity();
            conn.net_of_conductor(w)
                .and_then(|id| conn.net(id))
                .and_then(|n| ecad_model::connectivity::wiring::net_potential(doc, n).name)
                .unwrap_or_default()
        }
        _ => String::new(),
    }
}

/// The preview as JSON.
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

/// A kind's JSON word, read back as the kind.
#[derive(Deserialize)]
#[serde(remote = "Kind", rename_all = "snake_case")]
enum KindWord {
    Devices,
    Cables,
    Wires,
    TerminalBlocks,
}

/// What a preview row does, read from its `change` word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowChange {
    Change,
    Fixed,
    Same,
    Skipped,
}

impl RowChange {
    /// The mark as shown (`→`, `F`, `=`, `–`).
    #[must_use]
    pub fn glyph(self) -> &'static str {
        match self {
            RowChange::Change => "→",
            RowChange::Fixed => "F",
            RowChange::Same => "=",
            RowChange::Skipped => "–",
        }
    }
}

/// One listed row of a preview: an item that is numbered.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PreviewRow {
    #[serde(with = "KindWord")]
    pub kind: Kind,
    pub old: String,
    pub new: String,
    pub at: String,
    pub desc: String,
    pub change: RowChange,
    /// Where the anchor is, for going to it.
    pub page: Option<PageId>,
    pub point: Option<[f64; 2]>,
}

/// One item the run leaves alone, and why.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PreviewSkipped {
    #[serde(with = "KindWord")]
    pub kind: Kind,
    pub tag: String,
    pub why: String,
}

/// A `renumber` or `finalise` preview reply, read as the types it holds (the JSON is for the agent
/// and the command line; the app reads this).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Preview {
    pub rows: Vec<PreviewRow>,
    pub skipped: Vec<PreviewSkipped>,
    /// How many rows change.
    pub changes: u64,
    /// The project's change count the plan was made at (an Apply passes it back as `based_on`).
    pub change_count: u64,
    /// How many more rows than are listed.
    pub more: u64,
    pub summary: String,
}

impl Preview {
    /// The preview a reply holds; `None` when the reply is not one.
    #[must_use]
    pub fn from_reply(reply: &Value) -> Option<Self> {
        serde_json::from_value(reply.clone()).ok()
    }
}

/// `42 to change · 3 fixed · 7 same · 1 skipped`.
pub(crate) fn summary_line(plan: &Plan) -> String {
    format!("{} to change · {} fixed · {} same · {} skipped", plan.changes(), plan.fixed(), plan.same(), plan.skipped())
}

/// What the Renumber dialog offers: the tag prefixes in use (`-K`, `-Q`…, provisional
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
        if let Some(p) = tag::prefix_of(&k.text) {
            prefixes.insert(p);
        }
    }
    let potentials = doc.settings().wiring.potentials.iter().filter(|p| p.numbered).map(|p| p.name.clone()).collect();
    let mut folders: Vec<(FolderId, String)> = doc
        .folder_order()
        .iter()
        .filter_map(|f| {
            let mut names = Vec::new();
            let mut at = Some(*f);
            for _ in 0..ecad_model::document::book::MAX_FOLDER_DEPTH {
                let Some(id) = at else { break };
                let folder = doc.folders().get(&id)?;
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
    // The provisional keys are their own part of the index: no scan of the real tags.
    let mut held: Vec<(Option<(String, u64)>, String)> = doc
        .tagged()
        .filter(|(k, _)| k.provisional)
        .filter_map(|(_, c)| doc.devices().get(&c))
        .map(|d| (d.tag().and_then(tag::parse), d.tag_shown().unwrap_or_default()))
        .chain(
            doc.tagged_cables()
                .filter(|(k, _)| k.provisional)
                .filter_map(|(_, k)| doc.cables().get(&k))
                .map(|c| (c.tag().and_then(tag::parse), c.tag_shown().unwrap_or_default())),
        )
        .collect();
    held.sort();
    let tags: Vec<String> = held.into_iter().map(|(_, shown)| shown).collect();
    let mut by: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut nets = 0;
    // Unnumbered nets are provisional only with provisional tags on; otherwise they are the usual
    // nets with no number yet (an Info row), never a reason to stop an issue.
    let on = numbering::provisional_on(doc);
    for (_, n) in doc.connectivity().nets().filter(|_| on) {
        if n.wires.is_empty() || !n.numbers.is_empty() {
            continue;
        }
        let pot = ecad_model::connectivity::wiring::net_potential(doc, n);
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

/// What the provisional step before an issue will do, worked out without writing.
#[derive(Debug, Clone)]
pub enum BeforeIssue {
    /// Go on to the issue; the note for the echo (`(2 provisional tags issued)`), if any.
    Go(Option<String>),
    /// Finalise numbering first, with this plan (its own transaction, `Finalise numbering`).
    Finalise(Plan),
}

/// What the provisional step before an issue does for `doc` with the `provisional` word `way`:
/// go on (with the echo's note), or finalise numbering first, which the caller
/// plans against the project's tag marks.
///
/// # Errors
/// Provisional items present and no `way` given (the ways forward), or a word that is neither.
pub fn way_before_issue(doc: &Document, way: Option<&str>) -> Result<Option<Option<String>>, CmdError> {
    let way = way.map(|w| w.trim().to_ascii_lowercase());
    let items = provisional_items(doc);
    if items.is_empty() {
        return Ok(Some(None));
    }
    match way.as_deref() {
        None => Err(CmdError::refused(format!(
            "{}. Give provisional: \"finalise\" (number them first, in one transaction before the issue) or \"issue\" (issue as they are).",
            items.words()
        ))),
        Some("issue") => Ok(Some(Some(if items.tags.is_empty() {
            format!("({} issued)", plural(items.nets, "provisional wire number", "provisional wire numbers"))
        } else {
            format!("({} issued)", plural(items.tags.len(), "provisional tag", "provisional tags"))
        }))),
        Some("finalise" | "finalize") => Ok(None),
        Some(other) => Err(CmdError::input(format!("`provisional` must be \"finalise\" or \"issue\", not {other:?}"))),
    }
}

// ---------------------------------------------------------------------------------------------
// The commands (generic: crate::commands::run)

/// The refusal of an Apply whose preview the project has moved past.
pub(crate) const STALE: &str = "The project changed since the preview; check the new list and Apply again.";

/// `scope` as an object: `{prefix}`, `{folder: id}` or `{pages: [id]}`.
#[derive(Deserialize, JsonSchema)]
pub struct ScopePick {
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
pub enum ScopeParam {
    /// "all", "selection" or "provisional".
    Word(String),
    /// {prefix}, {folder: id} or {pages: [id]}.
    Pick(ScopePick),
}

/// `renumber {kind?, strips?, scope?, objects?, start?, digits?, digits_for_new?, preview?,
/// based_on?}`: drawing order; fixed tags kept; one transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenumberParams {
    /// What to renumber: devices, cables, wires, terminal blocks or all (default devices).
    #[serde(default)]
    kind: Option<String>,
    /// Terminal blocks with scope "selection": the strips (or terminal blocks of them), as hex ids.
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

/// `finalise {preview?, based_on?}` = `renumber {kind: "all", scope: "provisional"}` (design
/// numbers the provisional tags and unnumbered nets in drawing order; one transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FinaliseParams {
    /// List the changes; write nothing.
    #[serde(default)]
    preview: Option<bool>,
    /// The preview's change count: refused if the project changed since.
    #[serde(default)]
    based_on: Option<u64>,
}

/// `numbering {provisional?}`: no parameters reads; `provisional` sets whether
/// new tags are provisional (`-K?1`) until Finalise.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NumberingParams {
    /// New tags are provisional until Finalise.
    #[serde(default)]
    provisional: Option<bool>,
}

/// The plan of `req` in drawing order (a device is numbered at its head, else its
/// first symbol), with the tag high-water mark caught up.
///
/// # Errors
/// The plan's refusal.
pub fn plan_for(ledger: &Ledger, tag_high: &mut TagHigh, req: &Request) -> Result<Plan, CmdError> {
    tag_high.catch_up(ledger);
    let doc = ledger.document();
    numbering::plan_ranked(doc, req, tag_high.marks(), &|r| crate::symbols::reps::numbering_rank(doc, r)).map_err(CmdError::from)
}

/// What a numbering is asked to do besides planning.
#[derive(Clone, Copy)]
struct How {
    preview: bool,
    based_on: Option<u64>,
    for_new: bool,
}

/// Renumber with `input` (`None`: finalise): the preview, or the edit and its words.
fn renumber_with(cx: &mut Cx<'_>, input: Option<&RenumberInput>, how: How) -> Result<Outcome, CmdError> {
    let finalise = input.is_none();
    let req = match input {
        None => finalise_request(),
        Some(i) => request(cx.doc, i, crate::cables::words::MAX_HANDLES)?,
    };
    let plan = plan_for(cx.ledger, cx.tag_high, &req)?;
    let doc = cx.doc;
    let count = cx.change_count;
    // Accessories and jumper bars follow their relabelled terminal blocks.
    let (mut acc_ops, mut acc_lines) = crate::terminal_strips::accessories::renumber_ops(doc, &plan.ops);
    // A strip's part lines keep naming the positions they serve.
    let (for_ops, for_lines) = crate::reports::bom::renumber_for_ops(doc, &plan.ops);
    acc_ops.extend(for_ops);
    acc_lines.extend(for_lines);
    if how.preview {
        let mut v = preview_json(doc, &plan, count);
        v["accessories"] = json!(acc_lines);
        return Ok(Outcome::answer(v));
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
        let before = doc.settings().clone();
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
        return Ok(Outcome::answer(json!({"renumbered": 0, "fixed_kept": plan.fixed(), "message": "Nothing to change."})));
    }
    let label = if finalise { "Finalise numbering" } else { kind_label(&req.kinds) };
    let line = applied_words(&plan, finalise);
    let answer = json!({
        "renumbered": plan.changes(),
        "devices": plan.changes_of(Kind::Devices),
        "cables": plan.changes_of(Kind::Cables),
        "wires": plan.changes_of(Kind::Wires),
        "connection_points": plan.changes_of(Kind::TerminalBlocks),
        "fixed_kept": plan.fixed(),
        "message": line,
    });
    Ok(Outcome::edit(label, ops, answer).echo(Some(line)))
}

impl Generic for RenumberParams {
    const ID: &'static str = "renumber";
    const TITLE: &'static str = "Renumber";
    const HELP: &'static str = "{kind?: \"devices\"|\"cables\"|\"wires\"|\"terminal_blocks\"|\"all\", strips?: [id] (terminal blocks, with scope \"selection\"), scope?: \"all\"|\"selection\"|\"provisional\"|{prefix}|{folder: id}|{pages: [id]}, objects?: [id] (selection), start?: n, digits?: 1..6, digits_for_new?: bool, preview?: bool, based_on?: change count} (drawing order; fixed tags kept; one transaction)";
    const PAGES: Pages = Pages::ReadAnywhere;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let scope = self.scope.map(|sc| match sc {
            ScopeParam::Word(w) => ScopeInput::Word(w),
            ScopeParam::Pick(p) => ScopeInput::Pick { prefix: p.prefix, folder: p.folder, pages: p.pages },
        });
        let input = RenumberInput { kind: self.kind, scope, objects: self.objects, strips: self.strips, start: self.start, digits: self.digits };
        let how = How { preview: self.preview.unwrap_or(false), based_on: self.based_on, for_new: self.digits_for_new.unwrap_or(false) };
        renumber_with(cx, Some(&input), how)
    }
}

impl Generic for FinaliseParams {
    const ID: &'static str = "finalise";
    const TITLE: &'static str = "Finalise Numbering";
    const HELP: &'static str =
        "{preview?: bool, based_on?: change count} (numbers the provisional tags and unnumbered nets in drawing order; one transaction)";
    const PAGES: Pages = Pages::ReadAnywhere;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let how = How { preview: self.preview.unwrap_or(false), based_on: self.based_on, for_new: false };
        renumber_with(cx, None, how)
    }
}

impl Generic for NumberingParams {
    const ID: &'static str = "numbering";
    const TITLE: &'static str = "Numbering Settings";
    const HELP: &'static str = "{provisional?: bool} (no parameters reads; provisional: new tags are -K?1 until Finalise)";
    const PAGES: Pages = Pages::ReadAnywhere;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let read = |doc: &Document| json!({"provisional": numbering::provisional_on(doc), "padding": devices::padding(doc), "cable_prefix": format!("-{}", devices::tags::cable_prefix(doc))});
        let Some(on) = self.provisional else { return Ok(Outcome::answer(read(cx.doc))) };
        let before = cx.doc.display().get(PROVISIONAL_SETTING).cloned();
        let after = on.then(|| ecad_model::naming::words::flag_word(true).to_string());
        let ops = match before == after {
            true => Vec::new(),
            false => vec![Op::SetProperty { object: ObjectRef::Project, key: PROVISIONAL_SETTING.into(), before, after }],
        };
        Ok(Outcome::edit_then("Set numbering", ops, read))
    }
}
