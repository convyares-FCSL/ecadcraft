//! The rules of brief 026's `renumber` (design 026 §3, §13; brief 028 §9.5 for terminals): what a
//! renumber covers, read from its typed input into an [`ecad_model::numbering::Request`]. The plan
//! itself is [`ecad_model::numbering`]'s; the engine crate's shim plans it against the project's
//! tag marks, commits and answers in JSON.

use std::collections::BTreeSet;

use ecad_model::numbering::{self, Kind, Mark, Plan, Request, Scope};
use ecad_model::tag;
use ecad_model::{CableId, DeviceId, Document, FolderId, Noun, ObjectRef, PageId, TerminalId, WireId};
use serde_json::{Value, json};

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
        // Brief 028: terminal labels per strip ("all" keeps its 026 meaning: tags and wires).
        "terminals" | "terminal" | "trm" => [Kind::Terminals].into(),
        "all" => [Kind::Devices, Kind::Cables, Kind::Wires].into(),
        other => {
            return Err(CmdError::refused(format!("`kind` must be \"devices\", \"cables\", \"wires\", \"terminals\" or \"all\", not {other:?}")));
        }
    })
}

/// A renumber's `scope` as given: a word, or an object. Each of the object's entries: absent
/// (`None`), null (`Some(None)`) or given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeInput {
    /// `all`, `selection`, `provisional` (or `unnumbered`).
    Word(String),
    /// `{prefix}`, `{folder: id}` or `{pages: [id]}`, read in that order.
    Pick { prefix: Option<Option<String>>, folder: Option<Option<FolderId>>, pages: Option<Option<Vec<PageId>>> },
}

/// What `renumber` is given. `objects` and `strips` are hex IDs as written (a device, cable or
/// wire; a strip or one of its terminals); `Some(None)`: given as null.
#[derive(Debug, Clone, Default)]
pub struct RenumberInput {
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
    u128::from_str_radix(t.trim(), 16).map_err(|_| CmdError::input(format!("`{t:?}` in `{list}` is not a valid hex id")))
}

/// The scope of a renumber.
///
/// # Errors
/// An unknown word, a selection without `objects`, an object that is none of the three, or an
/// ID the project does not have.
pub fn scope(doc: &Document, input: &RenumberInput, max_objects: usize) -> Result<Scope, CmdError> {
    match &input.scope {
        None => Ok(Scope::All),
        Some(ScopeInput::Word(w)) => match w.trim().to_ascii_lowercase().as_str() {
            "all" => Ok(Scope::All),
            "provisional" | "unnumbered" => Ok(Scope::Provisional),
            // Brief 028: a terminal renumber may name its strips in `strips` alone.
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
                    if doc.devices.contains_key(&DeviceId(n)) {
                        devices.insert(DeviceId(n));
                    } else if doc.cables.contains_key(&CableId(n)) {
                        cables.insert(CableId(n));
                    } else if doc.wires.contains_key(&WireId(n)) {
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
                if !doc.folders.contains_key(&f) {
                    return Err(CmdError::NotFound(Noun::Folder));
                }
                return Ok(Scope::Folder(f));
            }
            if let Some(ps) = pages {
                let a = ps.as_ref().ok_or(CmdError::input("`pages` must be an array of hex ids"))?;
                let mut set = BTreeSet::new();
                for p in a.iter().take(MAX_PAGES) {
                    if !doc.pages.contains_key(p) {
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

/// Brief 028 (design 028 §9.5): the strips a terminal renumber covers, from the scope: every
/// strip (all, folder, pages; the planner keeps those on the pages), the strips with that tag
/// prefix, or the strips named in `strips` or `objects` (selection; a terminal ID names its strip).
///
/// # Errors
/// A named strip that is not a terminal strip or one of its terminals.
pub fn strips(doc: &Document, named: &[String], scope: &Scope) -> Result<BTreeSet<DeviceId>, CmdError> {
    let all = || doc.devices.keys().copied().filter(|c| crate::strips::strip_of(doc, *c).is_some());
    Ok(match scope {
        Scope::All | Scope::Folder(_) | Scope::Pages(_) => all().collect(),
        Scope::Prefix(p) => {
            let p = p.trim_start_matches('-').to_ascii_uppercase();
            all().filter(|c| crate::devices::tag_of(doc, *c).and_then(ecad_model::tag::parse).is_some_and(|(q, _)| q == p)).collect()
        }
        Scope::Selection { devices, .. } => {
            let mut out: BTreeSet<DeviceId> = devices.iter().copied().filter(|c| crate::strips::strip_of(doc, *c).is_some()).collect();
            for t in named {
                let n = hex(t, "strips")?;
                let c = match doc.terminals.get(&TerminalId(n)) {
                    Some(t) => t.device(),
                    None => Some(DeviceId(n)),
                };
                let Some(c) = c.filter(|c| crate::strips::strip_of(doc, *c).is_some()) else {
                    return Err(CmdError::refused(format!("{t} in `strips` is not a terminal strip or one of its terminals.")));
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
pub fn request(doc: &Document, input: &RenumberInput, max_named: usize) -> Result<Request, CmdError> {
    let kinds = kinds(input.kind.as_deref())?;
    let scope = scope(doc, input, max_named)?;
    let digits = match input.digits {
        None => None,
        Some(d) => Some(u8::try_from(d).map_err(|_| CmdError::refused(numbering::BAD_DIGITS))?),
    };
    let strips = if kinds.contains(&Kind::Terminals) {
        let named: Vec<String> = input.strips.iter().flatten().flatten().take(max_named).cloned().collect();
        strips(doc, &named, &scope)?
    } else {
        BTreeSet::new()
    };
    // Brief 036 (FP8c): each strip's positions, numbered as groups.
    let positions = strips.iter().map(|c| (*c, crate::terminal_types::position_groups(doc, *c))).collect();
    Ok(Request { kinds, scope, start: input.start, digits, strips, positions })
}

/// `finalise`'s request: the provisional tags and unnumbered nets (design 026 §5.3).
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

// ---------------------------------------------------------------- words, choices and provisional items (brief 074: from the engine)

/// Most rows a JSON preview lists (`more` counts the rest).
pub const MAX_PREVIEW_ROWS: usize = 2_000;

pub fn kind_label(kinds: &BTreeSet<Kind>) -> &'static str {
    match kinds.iter().collect::<Vec<_>>().as_slice() {
        [Kind::Devices] => "Renumber devices",
        [Kind::Cables] => "Renumber cables",
        [Kind::Wires] => "Renumber wires",
        [Kind::Terminals] => "Renumber terminals",
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

pub fn row_json(doc: &Document, r: &numbering::Row) -> Value {
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
pub fn desc(doc: &Document, o: ObjectRef) -> String {
    match o {
        ObjectRef::Device(c) => crate::devices::description(doc, c),
        ObjectRef::Cable(k) => {
            let n = doc.cables.get(&k).map_or(0, |c| c.cores.len());
            format!("Cable, {}", plural(n, "wire", "wires"))
        }
        ObjectRef::Wire(w) => {
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
        .filter_map(|(_, c)| crate::devices::tag_of(doc, c).filter(|t| tag::is_provisional(t)).map(str::to_string))
        .chain(doc.tagged_cables().filter_map(|(_, k)| crate::devices::cable_tag_of(doc, k).filter(|t| tag::is_provisional(t)).map(str::to_string)))
        .collect();
    tags.sort_by_key(|t| tag::provisional(t));
    let mut by: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut nets = 0;
    // Unnumbered nets are provisional only with provisional tags on; otherwise they are 022's
    // nets with no number yet (an Info row), never a reason to stop an issue.
    let on = numbering::provisional_on(doc);
    for (_, n) in doc.connectivity().nets().filter(|_| on) {
        if n.wires.is_empty() || !n.numbers.is_empty() {
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

/// What the provisional step before an issue does for `doc` with the `provisional` word `way`
/// (design §5.5): go on (with the echo's note), or finalise numbering first, which the caller
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
