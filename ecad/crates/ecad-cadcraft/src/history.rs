//! The project's history as the user sees it (brief 020a, design 020): the History tab's graph,
//! versions, points shown read-only, Restore, and branches (create, rename, switch, archive,
//! reopen), with their commands.
//!
//! - The graph is built from the `branches` and `versions` rows and **one** transaction row per
//!   node (its time, author and label), plus, for the branch shown, the ledger's unsaved steps.
//!   It never reads the logs (design 020 §8.1).
//! - A point is shown on a copy, from its nearest snapshot (the branch's own or its ancestry's)
//!   and at most the snapshot interval of steps; nothing is recorded.
//! - Restore is one ordinary transaction labelled `Restore "…"` (FQ6) whose ops are the
//!   difference between the live state and the point's ([`ecad_model::diff`]), so they scale with
//!   what differs; versions and revision fields are never restored (they are history, not
//!   design).
//! - Branch actions are written to the file at once (owner answer 4, FQ8), after saving the
//!   branch shown (owner answer 3, "Save and switch").
//!
//! - Merges (brief 020b, [`crate::merging`]) are rows of their own, read from the merge index
//!   (FQ2), drawn as joins from the source's line.
//! - The graph reads one page of rows ([`PAGE`]) and the next on request (`limit`); each row
//!   reads its step's time and author only when it comes into the page.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use cadcraft_engine::{CommandSpec, Session};
use ecad_file::v2::schema::{BranchId, BranchRecord, BranchStatus, VersionKind};
use ecad_model::{Document, Op, Transaction, TxKind};
use serde_json::{Value, json};

use crate::access::Pages;
use crate::commands::{err, with_any_page, with_project};
use crate::project::Project;
use crate::revisions;
use crate::sheet_cmds::{RevisionView, next_view_id};

/// Longest version name, in characters (design 020 §2.5.1).
pub const MAX_VERSION_NAME: usize = 100;
/// The most lanes drawn; a branch beyond them is drawn in the last lane with `»`.
pub const MAX_LANES: usize = 6;
/// Nodes read per page of the graph (design 020 §2.2).
pub const PAGE: usize = 200;
/// Steps read per page of the Steps list (design 020 §2.4).
pub const STEPS_PAGE: u64 = 50;

/// What a graph row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NodeKind {
    /// A branch's head.
    Now,
    /// An issued revision.
    Revision,
    /// A version the user created.
    Version,
    /// A merge into this branch (brief 020b; none are written yet).
    Merge,
    /// Where a branch starts, when no other node is there.
    Fork,
    /// An automatic snapshot (shown only on request).
    Auto,
    /// The project's start.
    Root,
}

impl NodeKind {
    /// The word used in commands and element names.
    pub fn word(self) -> &'static str {
        match self {
            NodeKind::Now => "now",
            NodeKind::Revision => "revision",
            NodeKind::Version => "version",
            NodeKind::Merge => "merge",
            NodeKind::Fork => "fork",
            NodeKind::Auto => "auto",
            NodeKind::Root => "root",
        }
    }
}

/// One branch as the History tab lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRow {
    pub id: BranchId,
    pub name: String,
    pub status: BranchStatus,
    /// The parent's ID and the parent's step the branch starts after.
    pub fork: Option<(BranchId, u64)>,
    /// The last step (the ledger's head for the branch shown).
    pub head: u64,
    /// Unsaved steps (the branch shown only).
    pub unsaved: u64,
    pub current: bool,
    pub created_ms: Option<u64>,
}

/// One row of the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub branch: BranchId,
    pub seq: u64,
    pub kind: NodeKind,
    /// A version's or revision's name.
    pub name: Option<String>,
    /// What the row says (`Now · main · 2 unsaved`, `Rev B`, `Auto · 11-09 16:02`).
    pub label: String,
    pub time_ms: Option<u64>,
    pub author: String,
    pub description: Option<String>,
    /// Branches that start at this node (their lines bend into this row).
    pub forks: Vec<BranchId>,
    /// A merge row: the source branch and the step of it that was merged (its join is drawn
    /// from there, brief 020b).
    pub from: Option<(BranchId, u64)>,
}

/// The graph: branches and rows, newest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Graph {
    pub current: Option<BranchId>,
    pub branches: Vec<BranchRow>,
    pub nodes: Vec<Node>,
    /// More rows than were read (design 020 §2.2: the next page is read on scrolling to the end).
    pub more: bool,
}

/// A branch's line as drawn: its lane and the rows it spans (top row to its fork row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub branch: BranchId,
    pub lane: usize,
    /// The branch's topmost row.
    pub top: usize,
    /// Its bottom: the fork row on its parent's lane (`main`: the root row).
    pub bottom: usize,
    /// The parent's lane, where the line bends at `bottom` (`None` for `main`).
    pub into: Option<usize>,
    /// Drawn as the current line (`ecad.history-current`) from this row down; `None`: not at all.
    pub current_from: Option<usize>,
    /// More branches than lanes: drawn in the last lane with `»`.
    pub overflow: bool,
}

/// The lanes of a graph.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layout {
    /// Each node's lane.
    pub lanes: Vec<usize>,
    pub spans: Vec<Span>,
    /// Merge joins: from the source's row and lane to the merge row (brief 020b).
    pub joins: Vec<Join>,
}

/// A merge drawn as a join (brief scope 1): a line from the source branch's row at the step that
/// was merged up to the merge row on the target's lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Join {
    /// The merge row and its lane.
    pub row: usize,
    pub lane: usize,
    /// The source's row (below) and lane.
    pub from_row: usize,
    pub from_lane: usize,
}

/// A named point on a branch's line: its own versions and revisions, and those it inherits from
/// its ancestry (at or before each fork).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    pub name: String,
    pub revision: bool,
    /// The branch holding the step it names.
    pub branch: BranchId,
    pub seq: u64,
    pub description: Option<String>,
}

// ---------------------------------------------------------------- reading the project

/// `main`'s ID for this project.
pub fn main_of(p: &Project) -> BranchId {
    BranchId::main_of(p.id.0)
}

/// The branch shown.
pub fn current(p: &Project) -> BranchId {
    p.file.v2().map_or_else(|| main_of(p), ecad_file::v2::ProjectV2::branch)
}

/// The branch shown's name.
pub fn current_name(p: &Project) -> String {
    p.file.v2().map_or_else(|| "main".to_string(), |v| v.branch_record().name.clone())
}

/// Every branch row of the file (`main` first); a project without a version 2 file has only
/// `main`, in memory.
///
/// # Errors
/// The file could not be read.
pub fn branch_rows(p: &Project) -> Result<Vec<(BranchId, BranchRecord)>, String> {
    match p.file.v2() {
        Some(v) => v.branches().map_err(|e| e.to_string()),
        None => {
            let head = ecad_file::v2::schema::Point { seq: p.ledger.head(), chain: String::new() };
            Ok(vec![(main_of(p), BranchRecord::main(head))])
        }
    }
}

/// How many branches the project has (open or not); 1 without a file.
pub fn branch_count(p: &Project) -> usize {
    p.file.v2().and_then(|v| v.branches().ok()).map_or(1, |b| b.len())
}

/// The lineage of `branch`: itself, then its parents up to `main`, each with the parent's step it
/// forks after.
fn lineage_of(p: &Project, branch: BranchId) -> Result<Vec<(BranchId, Option<u64>)>, String> {
    let Some(v) = p.file.v2() else {
        return Ok(vec![(main_of(p), None)]);
    };
    let line = if branch == v.branch() {
        v.lineage().clone()
    } else {
        let r = v.store().reader().map_err(|e| e.to_string())?;
        ecad_file::v2::project::lineage(&r, p.id, branch)?
    };
    Ok(line.iter().map(|l| (l.branch, l.fork.map(|f| f.1))).collect())
}

/// The version rows (user and revision) of `branch` in the file.
fn rows(p: &Project, branch: BranchId, kinds: &[VersionKind]) -> Result<Vec<ecad_file::v2::schema::VersionRecord>, String> {
    let Some(v) = p.file.v2() else { return Ok(Vec::new()) };
    let r = v.store().reader().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for k in kinds {
        out.extend(r.versions(branch, *k).map_err(|e| e.to_string())?);
    }
    Ok(out)
}

/// The named points of `branch`'s line: its own, then the inherited ones.
///
/// # Errors
/// The file could not be read.
pub fn named_points(p: &Project, branch: BranchId) -> Result<Vec<Named>, String> {
    let line = lineage_of(p, branch)?;
    let mut inherited: Vec<Named> = Vec::new();
    for i in 1..line.len() {
        let (anc, _) = line[i];
        let bound = line.get(i - 1).and_then(|l| l.1).unwrap_or(0);
        for v in rows(p, anc, &[VersionKind::User, VersionKind::Revision])? {
            if v.point.seq <= bound {
                inherited.push(Named {
                    name: v.name.clone(),
                    revision: v.kind == VersionKind::Revision,
                    branch: anc,
                    seq: v.point.seq,
                    description: v.description.clone(),
                });
            }
        }
    }
    let mut own: Vec<Named> = Vec::new();
    if branch == current(p) {
        // The branch shown: from the ledger, so unsaved versions count and undone ones do not.
        let doc = p.ledger.document();
        let issued: BTreeSet<String> = revisions::issued(doc).into_iter().filter_map(|r| r.issued).collect();
        let stored: BTreeMap<String, Option<String>> =
            rows(p, branch, &[VersionKind::User, VersionKind::Revision])?.into_iter().map(|v| (v.name, v.description)).collect();
        let names: BTreeSet<&String> = inherited.iter().map(|n| &n.name).collect();
        for (name, at) in &doc.versions {
            if names.contains(name) {
                continue;
            }
            let description = stored.get(name).cloned().flatten().or_else(|| unsaved_description(p, name));
            own.push(Named { name: name.clone(), revision: issued.contains(name), branch, seq: *at, description });
        }
    } else {
        for v in rows(p, branch, &[VersionKind::User, VersionKind::Revision])? {
            own.push(Named { name: v.name, revision: v.kind == VersionKind::Revision, branch, seq: v.point.seq, description: v.description });
        }
    }
    own.sort_by_key(|n| std::cmp::Reverse(n.seq));
    own.extend(inherited);
    Ok(own)
}

/// The description of an unsaved version, from the step that named it.
fn unsaved_description(p: &Project, name: &str) -> Option<String> {
    p.ledger.log().iter().rev().flat_map(|t| t.ops.iter()).find_map(|op| match op {
        Op::NameVersion { name: n, description, .. } if n == name => description.clone(),
        _ => None,
    })
}

/// Transaction `seq` of `branch`: from the ledger for the branch shown (unsaved steps too), else
/// read from the file for display.
fn tx_at(p: &Project, branch: BranchId, seq: u64) -> Option<Transaction> {
    if seq == 0 {
        return None;
    }
    if branch == current(p)
        && let Some(t) = p.ledger.transaction(seq)
    {
        return Some(t.clone());
    }
    p.file.v2()?.tx_display(branch, seq, seq).ok()?.into_iter().next()
}

/// Steps `from..=to` of `branch`, oldest first.
pub(crate) fn txs(p: &Project, branch: BranchId, from: u64, to: u64) -> Result<Vec<Transaction>, String> {
    if from > to {
        return Ok(Vec::new());
    }
    if branch == current(p) {
        let base = p.ledger.base();
        let mut out = Vec::new();
        if from <= base {
            let hi = to.min(base);
            match (p.file.v2(), p.ledger.history()) {
                (Some(v), _) => out.extend(v.tx_display(branch, from, hi)?),
                (None, Some(h)) => out.extend(h.transactions(from, hi).map_err(|e| e.to_string())?),
                (None, None) => {}
            }
        }
        for s in from.max(base + 1)..=to {
            if let Some(t) = p.ledger.transaction(s) {
                out.push(t.clone());
            }
        }
        return Ok(out);
    }
    p.file.v2().ok_or("this branch is not in a file")?.tx_display(branch, from, to)
}

/// Who: `you` for the user, the agent's name otherwise.
pub fn author_text(a: &ecad_model::Author) -> String {
    match a {
        ecad_model::Author::User => "you".into(),
        ecad_model::Author::Agent(n) => n.clone(),
    }
}

/// The local zone's offset from UTC at `ms`, in seconds (overseer, 2026-11-10: the graph and the
/// details show local time; the file keeps UTC). `None` in a wasm build, where times are shown in
/// UTC, labelled.
pub fn local_offset(ms: u64) -> Option<i64> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use chrono::{Offset, TimeZone};
        let utc = chrono::DateTime::from_timestamp(i64::try_from(ms / 1000).ok()?, 0)?.naive_utc();
        Some(i64::from(chrono::Local.offset_from_utc_datetime(&utc).fix().local_minus_utc()))
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = ms;
        None
    }
}

/// `ms` moved by `offset` seconds (none: UTC).
fn shifted(ms: u64, offset: Option<i64>) -> u64 {
    let ms = i128::from(ms) + i128::from(offset.unwrap_or(0)) * 1000;
    u64::try_from(ms.max(0)).unwrap_or(0)
}

/// `MM-DD HH:MM` this year, `YYYY-MM-DD` otherwise, in local time (design 020 §2.2); ` UTC`
/// follows when the local zone cannot be read.
pub fn short_time(ms: Option<u64>, now_ms: Option<u64>) -> String {
    short_time_in(ms, now_ms, ms.and_then(local_offset))
}

/// [`short_time`] with the zone's offset given (`None`: UTC, labelled).
pub fn short_time_in(ms: Option<u64>, now_ms: Option<u64>, offset: Option<i64>) -> String {
    let Some(ms) = ms else { return String::new() };
    let (y, mo, d, h, mi) = civil(shifted(ms, offset));
    let this_year = now_ms.map(|n| civil(shifted(n, offset)).0) == Some(y);
    let utc = if offset.is_none() { " UTC" } else { "" };
    if this_year { format!("{mo:02}-{d:02} {h:02}:{mi:02}{utc}") } else { format!("{y:04}-{mo:02}-{d:02}{utc}") }
}

/// `YYYY-MM-DD HH:MM` in local time; ` UTC` follows when the local zone cannot be read.
pub fn full_time(ms: Option<u64>) -> String {
    full_time_in(ms, ms.and_then(local_offset))
}

/// [`full_time`] with the zone's offset given (`None`: UTC, labelled).
pub fn full_time_in(ms: Option<u64>, offset: Option<i64>) -> String {
    let Some(ms) = ms else { return String::new() };
    let (y, mo, d, h, mi) = civil(shifted(ms, offset));
    let utc = if offset.is_none() { " UTC" } else { "" };
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}{utc}")
}

/// Year, month, day, hour and minute (UTC) of milliseconds since 1970.
fn civil(ms: u64) -> (i64, u32, u32, u32, u32) {
    use chrono::{Datelike, Timelike};
    let t = chrono::DateTime::from_timestamp_millis(i64::try_from(ms).unwrap_or(i64::MAX)).unwrap_or_default();
    (i64::from(t.year()), t.month(), t.day(), t.hour(), t.minute())
}

/// Options of the graph (design 020 §2.3: both off by default, per session).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Show {
    pub autos: bool,
    pub archived: bool,
    /// Rows to read (0: one page, [`PAGE`]); scrolling to the end adds a page.
    pub limit: usize,
}

/// The graph of the project (design 020 §2.2): one row per node, newest first.
///
/// # Errors
/// The file could not be read.
pub fn graph(p: &Project, show: Show) -> Result<Graph, String> {
    let cur = current(p);
    let main = main_of(p);
    let all = branch_rows(p)?;
    let unsaved = p.unsaved_transactions();
    let mut g = Graph { current: Some(cur), ..Graph::default() };
    let shown: Vec<(BranchId, BranchRecord)> =
        all.into_iter().filter(|(id, b)| *id == cur || b.status != BranchStatus::Archived || show.archived).collect();
    let mut per: BTreeMap<BranchId, Vec<(Node, Option<u64>)>> = BTreeMap::new();
    let names: BTreeMap<BranchId, String> = shown.iter().map(|(id, b)| (*id, b.name.clone())).collect();
    for (id, b) in &shown {
        let is_cur = *id == cur;
        let head = if is_cur { p.ledger.head() } else { b.head.seq };
        let fork = b.parent.as_deref().and_then(BranchId::parse).zip(b.fork_point.as_ref().map(|f| f.seq));
        g.branches.push(BranchRow {
            id: *id,
            name: b.name.clone(),
            status: b.status.clone(),
            fork,
            head,
            unsaved: if is_cur { unsaved } else { 0 },
            current: is_cur,
            created_ms: b.created_ms,
        });
        let mut nodes = Vec::new();
        let head_tx = tx_at(p, *id, head);
        let now_label = if b.status == BranchStatus::Archived {
            format!("Archived \"{}\"", b.name)
        } else if is_cur && unsaved > 0 {
            format!("Now · {} · {unsaved} unsaved", b.name)
        } else {
            format!("Now · {}", b.name)
        };
        nodes.push(Node {
            branch: *id,
            seq: head,
            kind: NodeKind::Now,
            name: None,
            label: now_label,
            time_ms: head_tx.as_ref().and_then(|t| t.time_ms).or(b.created_ms),
            author: head_tx.as_ref().map(|t| author_text(&t.author)).or_else(|| b.created_by.clone()).unwrap_or_default(),
            description: None,
            forks: Vec::new(),
            from: None,
        });
        // Rows other than Now read their step's time and author only when they come into the
        // page (design 020 §8.1: one transaction row per node shown).
        let mut lazy: Vec<(Node, Option<u64>)> = Vec::new();
        let own = named_points(p, *id)?.into_iter().filter(|n| n.branch == *id);
        for n in own {
            lazy.push((
                Node {
                    branch: *id,
                    seq: n.seq,
                    kind: if n.revision { NodeKind::Revision } else { NodeKind::Version },
                    label: n.name.clone(),
                    name: Some(n.name),
                    time_ms: None,
                    author: String::new(),
                    description: n.description,
                    forks: Vec::new(),
                    from: None,
                },
                Some(n.seq + 1),
            ));
        }
        if show.autos {
            for v in rows(p, *id, &[VersionKind::Auto])? {
                lazy.push((
                    Node {
                        branch: *id,
                        seq: v.point.seq,
                        kind: NodeKind::Auto,
                        name: None,
                        label: "Auto".into(),
                        time_ms: None,
                        author: String::new(),
                        description: None,
                        forks: Vec::new(),
                        from: None,
                    },
                    Some(v.point.seq),
                ));
            }
        }
        // Merges into this branch (the merge index, FQ2), its unsaved ones included.
        if let Some(v) = p.file.v2() {
            let merges = if is_cur { v.merge_index(&p.ledger)? } else { v.merge_rows(*id)? };
            for m in merges {
                let src = names.get(&m.source).cloned().unwrap_or_else(|| "a branch".into());
                let update = b.parent.as_deref() == Some(m.source.hex().as_str());
                lazy.push((
                    Node {
                        branch: *id,
                        seq: m.seq,
                        kind: NodeKind::Merge,
                        name: None,
                        label: if update { format!("Updated from {src}") } else { format!("Merged \"{src}\"") },
                        time_ms: None,
                        author: String::new(),
                        description: None,
                        forks: Vec::new(),
                        from: Some((m.source, m.source_head.0)),
                    },
                    Some(m.seq),
                ));
            }
        }
        if *id == main {
            let label = match p.file.v2().and_then(|v| v.link_v1()) {
                Some(_) => "Converted from a version 1 file".to_string(),
                None => "Project created".to_string(),
            };
            let created = p.file.v2().and_then(ecad_file::v2::ProjectV2::created_ms);
            nodes.push(Node {
                branch: main,
                seq: 0,
                kind: NodeKind::Root,
                name: None,
                label,
                time_ms: created,
                author: String::new(),
                description: None,
                forks: Vec::new(),
                from: None,
            });
        }
        let mut nodes: Vec<(Node, Option<u64>)> = nodes.into_iter().map(|n| (n, None)).chain(lazy).collect();
        // Within a branch: newest step first; at one step, Now above a version above an auto.
        nodes.sort_by(|(a, _), (b, _)| b.seq.cmp(&a.seq).then(a.kind.cmp(&b.kind)));
        nodes.dedup_by(|(a, _), (b, _)| a.kind == NodeKind::Now && b.kind == NodeKind::Now);
        per.insert(*id, nodes);
    }
    // Fork rows: a branch starts at a node of its parent, or at a row of its own.
    for row in g.branches.clone() {
        let Some((parent, seq)) = row.fork else { continue };
        let Some(list) = per.get_mut(&parent) else { continue };
        match list.iter_mut().find(|(n, _)| n.seq == seq && n.kind != NodeKind::Now && n.kind != NodeKind::Merge) {
            Some((n, _)) => n.forks.push(row.id),
            None => {
                let at = list.iter().position(|(n, _)| n.seq < seq || (n.seq == seq && n.kind > NodeKind::Fork)).unwrap_or(list.len());
                list.insert(
                    at,
                    (
                        Node {
                            branch: parent,
                            seq,
                            kind: NodeKind::Fork,
                            name: None,
                            label: format!("Branch \"{}\" started", row.name),
                            time_ms: row.created_ms,
                            author: String::new(),
                            description: None,
                            forks: vec![row.id],
                            from: None,
                        },
                        None,
                    ),
                );
            }
        }
    }
    let limit = if show.limit == 0 { PAGE } else { show.limit };
    let now = crate::project::now_ms();
    let mut fill = |n: &mut Node, seq: u64| {
        if let Some(t) = tx_at(p, n.branch, seq) {
            n.time_ms = t.time_ms;
            n.author = author_text(&t.author);
        }
        if n.kind == NodeKind::Auto {
            n.label = format!("Auto · {}", short_time(n.time_ms, now));
        }
    };
    let (nodes, more) = merge_by_time_paged(per, &g.branches, limit, &mut fill);
    g.nodes = nodes;
    g.more = more;
    Ok(g)
}

/// Rows of all branches, newest first (all of them; see [`merge_by_time_paged`]).
#[cfg(test)]
fn merge_by_time(per: BTreeMap<BranchId, Vec<Node>>, branches: &[BranchRow]) -> Vec<Node> {
    let per = per.into_iter().map(|(k, v)| (k, v.into_iter().map(|n| (n, None)).collect())).collect();
    merge_by_time_paged(per, branches, usize::MAX, &mut |_, _| {}).0
}

/// Rows of all branches, newest first, at most `limit` of them (and whether more are left):
/// each branch's own order is kept, and between branches the newer row goes first (time is
/// display only: a wrong clock can mis-order rows between branches, never within one). A row's
/// time is read (`fill`, from its step) only when it reaches the front of its branch's list, so
/// a page reads about as many steps as it shows. Then every branch's rows are kept above its
/// fork row.
fn merge_by_time_paged(
    per: BTreeMap<BranchId, Vec<(Node, Option<u64>)>>, branches: &[BranchRow], limit: usize, fill: &mut dyn FnMut(&mut Node, u64),
) -> (Vec<Node>, bool) {
    let mut lists: Vec<std::collections::VecDeque<(Node, Option<u64>)>> = per.into_values().map(Into::into).collect();
    let mut out = Vec::new();
    while out.len() < limit {
        for l in &mut lists {
            if let Some((n, at)) = l.front_mut()
                && let Some(q) = at.take()
            {
                fill(n, q);
            }
        }
        let best = lists
            .iter()
            .enumerate()
            .filter_map(|(i, l)| l.front().map(|(n, _)| (i, n.time_ms.unwrap_or(0), n.kind == NodeKind::Root)))
            .max_by(|a, b| (!a.2, a.1).cmp(&(!b.2, b.1)).then(b.0.cmp(&a.0)));
        let Some((i, _, _)) = best else { break };
        if let Some((n, _)) = lists.get_mut(i).and_then(std::collections::VecDeque::pop_front) {
            out.push(n);
        }
    }
    let more = lists.iter().any(|l| !l.is_empty());
    // A fork row above any row of its branch moves down to just below the branch's last row.
    for b in branches {
        let Some(last) = out.iter().rposition(|n| n.branch == b.id) else { continue };
        if let Some(f) = out.iter().position(|n| n.forks.contains(&b.id))
            && f < last
        {
            let node = out.remove(f);
            out.insert(last, node);
        }
    }
    (out, more)
}

/// Lanes for `g` (design 020 §2.2): `main` in lane 0; other branches by their fork rows, bottom
/// up, each in the lowest lane free over its rows (lanes are reused); beyond [`MAX_LANES`], the
/// last lane with `»`. The branch shown and its ancestry below each fork are the current line.
pub fn layout(g: &Graph) -> Layout {
    let n = g.nodes.len();
    let mut spans: Vec<Span> = Vec::new();
    let mut lane_of: BTreeMap<BranchId, usize> = BTreeMap::new();
    // Each branch's rows: its top row and its fork row.
    let mut ranges: Vec<(BranchId, usize, usize, Option<BranchId>)> = Vec::new();
    for b in &g.branches {
        let Some(top) = g.nodes.iter().position(|x| x.branch == b.id) else { continue };
        let bottom = match b.fork {
            Some(_) => g.nodes.iter().position(|x| x.forks.contains(&b.id)).unwrap_or(n.saturating_sub(1)),
            None => g.nodes.iter().rposition(|x| x.branch == b.id).unwrap_or(top),
        };
        ranges.push((b.id, top, bottom, b.fork.map(|f| f.0)));
    }
    let main = g.branches.iter().find(|b| b.fork.is_none()).map(|b| b.id);
    // Bottom up: the branch forking lowest is placed first.
    ranges.sort_by_key(|a| (a.3.is_some(), std::cmp::Reverse(a.2)));
    let mut busy: Vec<Vec<(usize, usize)>> = vec![Vec::new(); MAX_LANES];
    for (id, top, bottom, parent) in &ranges {
        let (lane, overflow) = if Some(*id) == main {
            (0, false)
        } else {
            match (1..MAX_LANES).find(|l| busy[*l].iter().all(|(t, b)| bottom < t || top > b)) {
                Some(l) => (l, false),
                None => (MAX_LANES - 1, true),
            }
        };
        if let Some(slot) = busy.get_mut(lane) {
            slot.push((*top, *bottom));
        }
        lane_of.insert(*id, lane);
        spans.push(Span { branch: *id, lane, top: *top, bottom: *bottom, into: parent.map(|_| 0), current_from: None, overflow });
    }
    for s in &mut spans {
        let parent = g.branches.iter().find(|b| b.id == s.branch).and_then(|b| b.fork.map(|f| f.0));
        s.into = parent.and_then(|p| lane_of.get(&p).copied());
    }
    // The current line: the branch shown whole, then each ancestor from its child's fork row down.
    let mut at = g.current;
    let mut from = 0;
    while let Some(id) = at {
        let Some(s) = spans.iter_mut().find(|s| s.branch == id) else { break };
        s.current_from = Some(from.max(s.top));
        from = s.bottom;
        at = g.branches.iter().find(|b| b.id == id).and_then(|b| b.fork.map(|f| f.0));
    }
    let lanes: Vec<usize> = g.nodes.iter().map(|x| lane_of.get(&x.branch).copied().unwrap_or(0)).collect();
    // Joins: from the source's newest row at or before the merged step, below the merge row.
    let mut joins = Vec::new();
    for (i, nd) in g.nodes.iter().enumerate() {
        let Some((b, s)) = nd.from else { continue };
        let Some(j) = g.nodes.iter().enumerate().skip(i + 1).find(|(_, x)| x.branch == b && x.seq <= s).map(|(j, _)| j) else { continue };
        let (Some(from_lane), Some(lane)) = (lane_of.get(&b).copied(), lanes.get(i).copied()) else { continue };
        joins.push(Join { row: i, lane, from_row: j, from_lane });
    }
    Layout { lanes, spans, joins }
}

// ---------------------------------------------------------------- points

/// A point named in a command: `{version}`, `{revision}` or `{seq}` (with `branch`, default the
/// branch shown).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Point {
    pub branch: BranchId,
    pub seq: u64,
    /// What the point is called (`Version "Before change"`, `Rev B`, `Step 812`).
    pub title: String,
    pub kind: NodeKind,
}

/// Find a branch by ID (32 hex digits) or name (open ones first, any case).
///
/// # Errors
/// No such branch.
pub fn find_branch(p: &Project, text: &str) -> Result<(BranchId, BranchRecord), String> {
    let all = branch_rows(p)?;
    let t = text.trim();
    if let Some(id) = BranchId::parse(t)
        && let Some(found) = all.iter().find(|(i, _)| *i == id)
    {
        return Ok(found.clone());
    }
    let lower = t.to_lowercase();
    let mut by_name: Vec<&(BranchId, BranchRecord)> = all.iter().filter(|(_, b)| b.name.to_lowercase() == lower).collect();
    by_name.sort_by_key(|(_, b)| b.status != BranchStatus::Open);
    by_name.first().map(|f| (*f).clone()).ok_or_else(|| format!("There is no branch \"{t}\"."))
}

/// The point `params` names on its branch's line.
///
/// # Errors
/// Missing or unknown point, in words.
pub fn point(p: &Project, params: &Value) -> Result<Point, String> {
    let branch = match params.get("branch").and_then(Value::as_str) {
        Some(b) => find_branch(p, b)?.0,
        None => current(p),
    };
    let head = if branch == current(p) { p.ledger.head() } else { find_branch(p, &branch.hex())?.1.head.seq };
    if let Some(name) = params.get("version").and_then(Value::as_str) {
        let n = named_points(p, branch)?
            .into_iter()
            .find(|n| !n.revision && n.name.eq_ignore_ascii_case(name.trim()))
            .ok_or_else(|| format!("There is no version \"{}\" on this branch's line.", name.trim()))?;
        return Ok(Point { branch: n.branch, seq: n.seq, title: format!("Version \"{}\"", n.name), kind: NodeKind::Version });
    }
    if let Some(rev) = params.get("revision").and_then(Value::as_str) {
        let want = revisions::version_name(rev.trim().trim_start_matches("Rev ").trim());
        let n = named_points(p, branch)?
            .into_iter()
            .find(|n| n.revision && n.name.eq_ignore_ascii_case(&want))
            .ok_or_else(|| format!("{want} has not been issued on this branch's line."))?;
        return Ok(Point { branch: n.branch, seq: n.seq, title: n.name, kind: NodeKind::Revision });
    }
    if let Some(seq) = params.get("seq").and_then(Value::as_u64) {
        if seq > head {
            return Err(format!("This branch has no step {seq} (its last is {head})."));
        }
        let kind = if seq == head { NodeKind::Now } else { NodeKind::Fork };
        return Ok(Point { branch, seq, title: format!("Step {seq}"), kind });
    }
    if params.get("now").and_then(Value::as_bool) == Some(true) || params.get("branch").is_some() {
        return Ok(Point { branch, seq: head, title: "Now".into(), kind: NodeKind::Now });
    }
    Err("Name a point: {\"version\": name}, {\"revision\": \"B\"} or {\"seq\": n}.".into())
}

/// The document at `pt` (on a copy, design 020 §2.4): the ledger for the branch shown, else read
/// from the file.
///
/// # Errors
/// The file could not be read or replayed.
pub fn state_at(p: &Project, pt: &Point) -> Result<Document, String> {
    match reads_saved(p, pt) {
        true => p.file.v2().ok_or("this branch is not in a file")?.point_state(pt.branch, pt.seq),
        false => p.ledger.state_at(pt.seq).map_err(|e| e.to_string()),
    }
}

/// Whether `pt`'s state is read from the file (its nearest snapshot and at most the snapshot
/// interval of steps, reviewer finding 2) rather than from the ledger: every saved point, of any
/// branch. Only an unsaved point of the branch shown comes from the ledger, undoing at most the
/// unsaved steps, so the cost never grows with the session's length.
pub fn reads_saved(p: &Project, pt: &Point) -> bool {
    p.file.v2().is_some_and(|v| pt.branch != v.branch() || pt.seq <= v.head().0)
}

/// Whether `pt` lies on the branch shown's line: its own steps, or its ancestry's at or before
/// each fork (owner answer 11: Restore only there).
fn on_current_line(p: &Project, pt: &Point) -> Result<bool, String> {
    let line = lineage_of(p, current(p))?;
    for (i, (b, _)) in line.iter().enumerate() {
        if *b == pt.branch {
            let bound = if i == 0 { u64::MAX } else { line.get(i - 1).and_then(|l| l.1).unwrap_or(0) };
            return Ok(pt.seq <= bound);
        }
    }
    Ok(false)
}

// ---------------------------------------------------------------- restore

/// Whether Restore compares this project property: revision fields are history and stay as they
/// are now (design 020 §2.5.3, §6.4 rule 1).
fn restores_key(key: &str) -> bool {
    !key.starts_with("rev.")
}

/// The state Restore returns to: a saved point from the file (its nearest snapshot and at most
/// the snapshot interval of steps), an unsaved one from the ledger.
fn restore_target(p: &Project, pt: &Point) -> Result<Document, String> {
    state_at(p, pt)
}

/// What a restore would do: its ops, how many objects change, and the pages they are on.
#[derive(Debug, Clone, Default)]
pub struct RestorePlan {
    pub ops: Vec<Op>,
    pub objects: usize,
    pub pages: Vec<String>,
    /// Issued revisions after the point on this line (`It lands after Rev C`).
    pub after_revisions: Vec<String>,
}

/// Work out a restore of `pt` (design 020 §2.5.3): the ops are the difference between the live
/// state and the point's (overseer, 2026-11-10: they scale with what differs, not with how far
/// back the point is), without version names or revision fields.
///
/// # Errors
/// Not on the branch shown's line, or the history could not be read.
pub fn plan_restore(p: &Project, pt: &Point) -> Result<RestorePlan, String> {
    if !on_current_line(p, pt)? {
        return Err(format!("{} is on another branch: branch from here, or merge that branch.", pt.title));
    }
    let live = p.ledger.document();
    let then = restore_target(p, pt)?;
    let ops = ecad_model::diff::ops_between(live, &then, &restores_key);
    let objects = ecad_model::diff::objects(&ops);
    let mut pages = BTreeSet::new();
    for (kind, id) in objects.keys() {
        let page = match *kind {
            "item" => live.items.get(&ecad_model::ItemId(*id)).or_else(|| then.items.get(&ecad_model::ItemId(*id))).map(|i| i.page),
            "page" => Some(ecad_model::PageId(*id)),
            "representation" => live
                .representations
                .get(&ecad_model::RepresentationId(*id))
                .or_else(|| then.representations.get(&ecad_model::RepresentationId(*id)))
                .map(|r| r.page),
            _ => None,
        };
        if let Some(pg) = page {
            let doc = if live.pages.contains_key(&pg) { live } else { &then };
            pages.insert(crate::references::page_number(doc, pg));
        }
    }
    let after_revisions = named_points(p, current(p))?
        .into_iter()
        .filter(|n| {
            n.revision
                && (n.branch != pt.branch || n.seq >= pt.seq)
                && on_current_line(p, &Point { branch: n.branch, seq: n.seq, title: String::new(), kind: NodeKind::Revision }).unwrap_or(false)
        })
        .filter(|n| n.branch == current(p) || n.branch == pt.branch)
        .map(|n| n.name)
        .collect();
    Ok(RestorePlan { objects: objects.len(), ops, pages: pages.into_iter().collect(), after_revisions })
}

// ---------------------------------------------------------------- versions

/// Check a new version's name (design 020 §2.5.1).
///
/// # Errors
/// In words.
pub fn check_version_name(p: &Project, name: &str) -> Result<String, String> {
    let n = name.trim();
    let len = n.chars().count();
    if len == 0 || len > MAX_VERSION_NAME {
        return Err(format!("A version name is 1 to {MAX_VERSION_NAME} characters."));
    }
    if n.chars().any(char::is_control) {
        return Err("A version name cannot hold control characters.".into());
    }
    if let Some(rest) = n.strip_prefix("Rev ").or_else(|| n.strip_prefix("rev ")).or_else(|| n.strip_prefix("REV "))
        && (1..=8).contains(&rest.chars().count())
        && rest.chars().all(|c| c.is_ascii_alphanumeric())
    {
        return Err(format!("\"{n}\" names revisions; issue one from Revisions."));
    }
    let lower = n.to_lowercase();
    if p.ledger.document().versions.keys().any(|k| k.to_lowercase() == lower) {
        return Err(format!("There is already a version \"{n}\" on {}.", current_name(p)));
    }
    Ok(n.to_string())
}

/// The New version dialog's default name: `Version <n>`, n = this branch's version count + 1.
pub fn default_version_name(p: &Project) -> String {
    let own = named_points(p, current(p)).map(|v| v.iter().filter(|n| !n.revision && n.branch == current(p)).count()).unwrap_or(0);
    let mut n = own + 1;
    loop {
        let name = format!("Version {n}");
        if check_version_name(p, &name).is_ok() {
            return name;
        }
        n += 1;
    }
}

/// The New branch dialog's default name: `Branch <n>`.
pub fn default_branch_name(p: &Project) -> String {
    let taken: BTreeSet<String> =
        branch_rows(p).unwrap_or_default().into_iter().filter(|(_, b)| b.status == BranchStatus::Open).map(|(_, b)| b.name.to_lowercase()).collect();
    (1..).map(|n| format!("Branch {n}")).find(|n| !taken.contains(&n.to_lowercase())).unwrap_or_else(|| "Branch".into())
}

// ---------------------------------------------------------------- the details box

/// What the details box shows about a point (design 020 §2.4).
///
/// # Errors
/// The history could not be read.
pub fn point_info(p: &Project, pt: &Point) -> Result<Value, String> {
    let rows = branch_rows(p)?;
    let bname = rows.iter().find(|(id, _)| *id == pt.branch).map(|(_, b)| b.name.clone()).unwrap_or_else(|| "main".into());
    let first = match pt.kind {
        NodeKind::Revision => format!("{}, issued on {bname}", pt.title),
        NodeKind::Version => format!("Version on {bname}"),
        NodeKind::Auto => format!("Automatic snapshot on {bname}"),
        NodeKind::Now => format!("Now on {bname}"),
        NodeKind::Merge => format!("Merge into {bname}"),
        NodeKind::Root => "Project created".into(),
        NodeKind::Fork => format!("Step {} on {bname}", pt.seq),
    };
    let naming = match pt.kind {
        NodeKind::Version | NodeKind::Revision => tx_at(p, pt.branch, pt.seq + 1),
        _ => tx_at(p, pt.branch, pt.seq),
    };
    let when = naming.as_ref().map(|t| format!("{} · {}", full_time(t.time_ms), author_text(&t.author))).unwrap_or_default();
    // The previous node on this line, for the change summary.
    let named = named_points(p, current_or(p, pt.branch))?;
    let prev = named.iter().filter(|n| n.branch == pt.branch && n.seq < pt.seq).max_by_key(|n| n.seq).cloned();
    let (since, steps) = match &prev {
        Some(n) => (n.name.clone(), pt.seq - n.seq),
        None if pt.branch == main_of(p) => ("the start".to_string(), pt.seq),
        None => ("the branch's start".to_string(), pt.seq),
    };
    let mut summary = vec![format!("{steps} step{}", if steps == 1 { "" } else { "s" })];
    if let (Some(n), true) = (&prev, matches!(pt.kind, NodeKind::Version | NodeKind::Revision)) {
        summary.extend(chunk_summary(p, pt, n));
    }
    let restore = on_current_line(p, pt).unwrap_or(false);
    let description = named
        .iter()
        .find(|n| n.branch == pt.branch && n.seq == pt.seq && Some(&n.name) == title_name(pt).as_ref())
        .and_then(|n| n.description.clone());
    Ok(json!({
        "title": pt.title, "kind": pt.kind.word(), "branch": bname, "branch_id": pt.branch.hex(), "seq": pt.seq,
        "line1": first, "line2": when, "summary": format!("Since {since}: {}", summary.join(" · ")),
        "description": description, "restore": restore,
        "label": naming.as_ref().map(|t| t.label.clone()),
    }))
}

fn current_or(p: &Project, b: BranchId) -> BranchId {
    if lineage_of(p, current(p)).map(|l| l.iter().any(|x| x.0 == b)).unwrap_or(false) { current(p) } else { b }
}

fn title_name(pt: &Point) -> Option<String> {
    match pt.kind {
        NodeKind::Version => pt.title.strip_prefix("Version \"").and_then(|t| t.strip_suffix('"')).map(str::to_string),
        NodeKind::Revision => Some(pt.title.clone()),
        _ => None,
    }
}

/// Pages changed, added and removed between two saved snapshots, from their chunk hashes
/// (design 020 §2.4): no document is built.
fn chunk_summary(p: &Project, pt: &Point, prev: &Named) -> Vec<String> {
    let Some(v) = p.file.v2() else { return Vec::new() };
    let Ok(r) = v.store().reader() else { return Vec::new() };
    let find = |b: BranchId, name: &str| [VersionKind::User, VersionKind::Revision].iter().find_map(|k| r.version(b, *k, name).ok().flatten());
    let (Some(name), Some(a)) = (title_name(pt), find(prev.branch, &prev.name)) else { return Vec::new() };
    let Some(b) = find(pt.branch, &name) else { return Vec::new() };
    let pages = |v: &ecad_file::v2::schema::VersionRecord| -> BTreeMap<String, String> {
        v.chunks.iter().filter(|(k, _)| k.starts_with("page:")).map(|(k, h)| (k.clone(), h.clone())).collect()
    };
    let (pa, pb) = (pages(&a), pages(&b));
    let changed = pb.iter().filter(|(k, h)| pa.get(*k).is_some_and(|x| x != *h)).count();
    let added = pb.keys().filter(|k| !pa.contains_key(*k)).count();
    let removed = pa.keys().filter(|k| !pb.contains_key(*k)).count();
    let differs = |k: &String| a.chunks.get(k) != b.chunks.get(k);
    let model: BTreeSet<&String> = a
        .chunks
        .keys()
        .chain(b.chunks.keys())
        .filter(|k| k.starts_with("model:") && *k != "model:meta" && *k != "model:project")
        .filter(|k| differs(k))
        .collect();
    let mut out = Vec::new();
    let plural = |n: usize, w: &str| format!("{n} page{} {w}", if n == 1 { "" } else { "s" });
    if changed > 0 {
        out.push(plural(changed, "changed"));
    }
    if added > 0 {
        out.push(plural(added, "added"));
    }
    if removed > 0 {
        out.push(plural(removed, "removed"));
    }
    // Devices by tag, from the differing component chunks only (design 020 §8.3: at most three
    // named changes).
    let comp: Vec<&String> = model.iter().copied().filter(|k| k.starts_with("model:component:")).collect();
    let named = if comp.is_empty() || comp.len() > 8 { None } else { device_changes(&r, &a, &b, &comp) };
    match named {
        Some(lines) => out.extend(lines),
        None if !comp.is_empty() => out.push("devices changed".into()),
        None => {}
    }
    if model.iter().any(|k| !k.starts_with("model:component:")) {
        out.push("wiring changed".into());
    }
    out
}

/// The devices added, removed and changed between two snapshots, read from their differing
/// component chunks only: `-K05 added`, at most three named, then how many more.
fn device_changes(
    r: &ecad_file::v2::Reader, a: &ecad_file::v2::schema::VersionRecord, b: &ecad_file::v2::schema::VersionRecord, keys: &[&String],
) -> Option<Vec<String>> {
    use ecad_file::v2::chunks::{ChunkSet, components_of};
    let load = |v: &ecad_file::v2::schema::VersionRecord| -> Option<BTreeMap<ecad_model::ComponentId, ecad_model::Component>> {
        let set = ChunkSet::from_record(&v.chunks).ok()?;
        let mut out = BTreeMap::new();
        for k in keys {
            let Some(h) = v.chunks.get(*k) else { continue };
            let hash = ecad_file::chain::parse_hex(h)?;
            let body = r.chunk_body(&hash).ok()?;
            out.extend(components_of(&set, k, &body).ok()?);
        }
        Some(out)
    };
    let (ca, cb) = (load(a)?, load(b)?);
    let tag = |c: &ecad_model::Component| c.properties.get(ecad_model::tag::TAG_KEY).cloned().unwrap_or_else(|| "a device".into());
    let mut lines: Vec<String> = Vec::new();
    for (id, c) in &cb {
        match ca.get(id) {
            None => lines.push(format!("{} added", tag(c))),
            Some(old) if old != c => lines.push(format!("{} changed", tag(c))),
            Some(_) => {}
        }
    }
    for (id, c) in &ca {
        if !cb.contains_key(id) {
            lines.push(format!("{} removed", tag(c)));
        }
    }
    let more = lines.len().saturating_sub(3);
    lines.truncate(3);
    if more > 0 {
        lines.push(format!("{more} more device{} changed", if more == 1 { "" } else { "s" }));
    }
    Some(lines)
}

// ---------------------------------------------------------------- the commands

/// The commands this module adds (design 020 §10). Merge is brief 020b.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("history", "History", run_history)
            .params("{autos?: bool, archived?: bool, limit?: 200} (the History graph, newest first, `limit` rows; read only)")
            .noundo(),
        CommandSpec::new("steps", "History Steps", run_steps).params("{branch?, before?: seq, limit?: 50} (steps newest first, read only)").noundo(),
        CommandSpec::new("pointinfo", "Point Info", run_pointinfo)
            .params("{branch?, version?|revision?|seq?} (the details of a point, read only)")
            .noundo(),
        CommandSpec::new("version", "New Version", run_version)
            .params("{name, description?} (keeps the project as it is now under this name: one step)")
            .noundo(),
        CommandSpec::new("showpoint", "Show Point", run_showpoint)
            .params("{branch?, version?|revision?|seq?} (that state read only; records nothing)")
            .noundo(),
        CommandSpec::new("closepoint", "Back to Current", run_closepoint).params("{}").noundo(),
        CommandSpec::new("restore", "Restore", run_restore)
            .params("{version?|revision?|seq?, branch?, preview?: bool} (makes the project as it was there: one step, undoable; preview only asks)")
            .noundo(),
        CommandSpec::new("branches", "Branches", run_branches).params("{} (the branch list, read only)").noundo(),
        CommandSpec::new("branch", "New Branch", run_branch)
            .params("{name, from?: {branch?, version?|revision?|seq?}, switch?: true} (saves first; written at once)")
            .noundo(),
        CommandSpec::new("renamebranch", "Rename Branch", run_renamebranch).params("{branch, name}").noundo(),
        CommandSpec::new("switchbranch", "Switch Branch", run_switchbranch)
            .params("{branch, save?: bool} (refused with unsaved changes unless save is true)")
            .noundo(),
        CommandSpec::new("archivebranch", "Archive Branch", run_archivebranch).params("{branch}").noundo(),
        CommandSpec::new("reopenbranch", "Reopen Branch", run_reopenbranch)
            .params("{branch, name?} (name: reopen under that name, in one step, when another open branch has its name)")
            .noundo(),
    ]
}

fn node_json(n: &Node) -> Value {
    json!({
        "branch": n.branch.hex(), "seq": n.seq, "kind": n.kind.word(), "name": n.name, "label": n.label,
        "time": n.time_ms, "author": n.author, "description": n.description,
        "forks": n.forks.iter().map(BranchId::hex).collect::<Vec<_>>(),
        "from": n.from.map(|(b, s)| json!({"branch": b.hex(), "seq": s})),
    })
}

fn branch_json(b: &BranchRow) -> Value {
    json!({
        "id": b.id.hex(), "name": b.name, "status": status_word(&b.status), "current": b.current,
        "fork": b.fork.map(|(br, s)| json!({"branch": br.hex(), "seq": s})), "head": b.head, "unsaved": b.unsaved,
    })
}

/// `open`, `merged` or `archived`.
pub fn status_word(s: &BranchStatus) -> &'static str {
    match s {
        BranchStatus::Open => "open",
        BranchStatus::Merged => "merged",
        BranchStatus::Archived => "archived",
    }
}

fn run_history(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let show = Show {
        autos: params.get("autos").and_then(Value::as_bool).unwrap_or(false),
        archived: params.get("archived").and_then(Value::as_bool).unwrap_or(false),
        limit: params.get("limit").and_then(Value::as_u64).map_or(0, |l| usize::try_from(l).unwrap_or(usize::MAX).max(1)),
    };
    with_any_page(s, |p, _| {
        let g = graph(p, show)?;
        Ok(json!({
            "current": g.current.map(|c| c.hex()),
            "branches": g.branches.iter().map(branch_json).collect::<Vec<_>>(),
            "nodes": g.nodes.iter().map(node_json).collect::<Vec<_>>(),
            "more": g.more,
        }))
    })
}

/// Steps of `branch` newest first, `limit` of them, before `before` (exclusive; default after
/// the head).
///
/// # Errors
/// The history could not be read.
pub fn steps(p: &Project, branch: BranchId, before: Option<u64>, limit: u64) -> Result<Vec<Transaction>, String> {
    let head = if branch == current(p) { p.ledger.head() } else { find_branch(p, &branch.hex())?.1.head.seq };
    let to = before.map_or(head, |b| b.saturating_sub(1).min(head));
    if to == 0 {
        return Ok(Vec::new());
    }
    let from = to.saturating_sub(limit.max(1) - 1).max(1);
    let mut v = txs(p, branch, from, to)?;
    v.reverse();
    Ok(v)
}

fn kind_word(k: TxKind) -> &'static str {
    match k {
        TxKind::Edit => "edit",
        TxKind::Undo { .. } => "undo",
        TxKind::Redo { .. } => "redo",
        TxKind::Merge { .. } => "merge",
    }
}

fn run_steps(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let before = params.get("before").and_then(Value::as_u64);
    let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(STEPS_PAGE).clamp(1, 1_000);
    let branch = params.get("branch").and_then(Value::as_str).map(str::to_string);
    with_any_page(s, |p, _| {
        let b = match &branch {
            Some(t) => find_branch(p, t)?.0,
            None => current(p),
        };
        let v = steps(p, b, before, limit)?;
        Ok(json!({"steps": v.iter().map(|t| json!({
            "seq": t.seq, "label": t.label, "author": author_text(&t.author), "time": t.time_ms, "kind": kind_word(t.kind),
        })).collect::<Vec<_>>()}))
    })
}

fn run_pointinfo(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    with_any_page(s, |p, _| {
        let pt = point(p, params)?;
        point_info(p, &pt)
    })
}

fn run_version(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let name = params.get("name").and_then(Value::as_str).ok_or_else(|| err("`name` is required"))?.to_string();
    let description = params.get("description").and_then(Value::as_str).map(str::trim).filter(|d| !d.is_empty()).map(str::to_string);
    with_project(s, |p, pg, _| {
        let head = p.ledger.head();
        if head == 0 {
            return Err("Nothing to keep yet: draw or change something first.".into());
        }
        let name = check_version_name(p, &name)?;
        if description.as_ref().is_some_and(|d| d.chars().count() > 1_000 || d.chars().any(|c| c.is_control() && c != '\n')) {
            return Err("A description is at most 1 000 characters.".into());
        }
        let ops = vec![Op::NameVersion { name: name.clone(), at: head, description: description.clone() }];
        p.apply_edit(pg, "version", &format!("Create version \"{name}\""), ops)?;
        let line = format!("Version \"{name}\" created. Save to keep it.");
        pg.echo(line.clone());
        Ok(json!({"name": name, "at": head, "transaction": p.ledger.head(), "message": line}))
    })
}

/// The point bar's text (design 020 §2.4).
pub fn bar_text(p: &Project, pt: &Point) -> String {
    let bname = find_branch(p, &pt.branch.hex()).map(|b| b.1.name).unwrap_or_else(|_| "main".into());
    match pt.kind {
        NodeKind::Now => format!("Now · {bname} · read-only"),
        _ => format!("{} · {bname} · read-only", pt.title),
    }
}

fn run_showpoint(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    with_any_page(s, |p, _| {
        let pt = point(p, params)?;
        let then = state_at(p, &pt)?;
        let book = ecad_model::book::Book::of(&then);
        let pages: Vec<Value> = book
            .order
            .iter()
            .map(|pg| json!({"page": pg.hex(), "number": book.number(&then, *pg), "title": then.pages.get(pg).map(|x| x.title.clone())}))
            .collect();
        let bar = bar_text(p, &pt);
        p.revision_view = Some(RevisionView {
            rev: pt.title.clone(),
            date: String::new(),
            at: pt.seq,
            doc: Arc::new(then),
            bar: Some(bar.clone()),
            id: next_view_id(),
        });
        Ok(json!({"title": pt.title, "branch": pt.branch.hex(), "seq": pt.seq, "bar": bar, "pages": pages}))
    })
}

fn run_closepoint(s: &mut Session, _params: &Value) -> cadcraft_engine::Result<Value> {
    with_any_page(s, |p, _| {
        let was = p.revision_view.take().map(|v| v.rev);
        Ok(json!({"closed": was}))
    })
}

/// The confirmation text of a restore (design 020 §2.5.3).
pub fn restore_question(p: &Project, pt: &Point, plan: &RestorePlan) -> String {
    let on = match plan.pages.len() {
        0 => String::new(),
        1 => format!(", on page {}", plan.pages[0]),
        _ => format!(", on pages {}", plan.pages.join(", ")),
    };
    let mut q = format!(
        "Make the project on {} as it was at {}?\n{} object{} change{on}. Nothing is deleted from the history: this is one new step, and Undo takes it back.",
        current_name(p),
        pt.title,
        plan.objects,
        if plan.objects == 1 { "" } else { "s" }
    );
    if let Some(last) = plan.after_revisions.first() {
        q.push_str(&format!("\nIt lands after {last}, so it first appears in the next revision."));
    }
    q
}

fn run_restore(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    with_project(s, |p, pg, _| {
        let pt = point(p, params)?;
        let plan = plan_restore(p, &pt)?;
        let quoted = match pt.kind {
            NodeKind::Version => pt.title.trim_start_matches("Version ").to_string(),
            _ => format!("\"{}\"", pt.title),
        };
        if plan.objects == 0 || plan.ops.is_empty() {
            return Err(format!("The project is already as it was at {quoted}."));
        }
        if params.get("preview").and_then(Value::as_bool) == Some(true) {
            // The Restore dialog's question; nothing is recorded.
            return Ok(json!({"question": restore_question(p, &pt, &plan), "objects": plan.objects, "pages": plan.pages}));
        }
        p.apply_edit_with(pg, "restore", &format!("Restore {quoted}"), plan.ops.clone(), false)?;
        let line = format!("Restored {quoted} ({} changes). Undo takes it back.", plan.objects);
        pg.echo(line.clone());
        Ok(json!({"restored": pt.title, "objects": plan.objects, "pages": plan.pages, "transaction": p.ledger.head(), "message": line}))
    })
}

fn run_branches(s: &mut Session, _params: &Value) -> cadcraft_engine::Result<Value> {
    with_any_page(s, |p, _| {
        let cur = current(p);
        let rows = branch_rows(p)?;
        Ok(json!({"current": cur.hex(), "branches": rows.iter().map(|(id, b)| json!({
            "id": id.hex(), "name": b.name, "status": status_word(&b.status), "current": *id == cur,
            "parent": b.parent, "fork": b.fork_point.as_ref().map(|f| f.seq), "head": if *id == cur { p.ledger.head() } else { b.head.seq },
        })).collect::<Vec<_>>()}))
    })
}

/// Why branch actions cannot run: no version 2 file yet (design 020 §2.6), or read-only.
pub(crate) fn branch_file(p: &Project) -> Result<(), String> {
    match p.file.v2() {
        None => Err("Branches live in the project file. Save it first.".into()),
        Some(v) => match v.read_only() {
            Some(r) => Err(format!("The project is read-only ({r}); branches cannot be changed.")),
            None => Ok(()),
        },
    }
}

/// Where a new branch starts: `from` in `params`, or the head of the branch shown.
fn fork_of(p: &Project, params: &Value) -> Result<Point, String> {
    match params.get("from") {
        None | Some(Value::Null) => Ok(Point { branch: current(p), seq: p.ledger.head(), title: "Now".into(), kind: NodeKind::Now }),
        Some(v) => {
            let mut pt = point(p, v)?;
            if v.get("version").is_none() && v.get("revision").is_none() && v.get("seq").is_none() {
                let head = if pt.branch == current(p) { p.ledger.head() } else { find_branch(p, &pt.branch.hex())?.1.head.seq };
                pt.seq = head;
            }
            Ok(pt)
        }
    }
}

/// How a new branch's start reads (`Rev A`, `main, step 640`).
fn from_text(p: &Project, pt: &Point) -> String {
    let bname = find_branch(p, &pt.branch.hex()).map(|b| b.1.name).unwrap_or_else(|_| "main".into());
    match pt.kind {
        NodeKind::Version => pt.title.trim_start_matches("Version ").to_string(),
        NodeKind::Revision => pt.title.clone(),
        _ => format!("{bname}, step {}", pt.seq),
    }
}

fn run_branch(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let name = params.get("name").and_then(Value::as_str).ok_or_else(|| err("`name` is required"))?.to_string();
    let switch = params.get("switch").and_then(Value::as_bool).unwrap_or(true);
    with_any_page(s, |p, pg| {
        branch_file(p)?;
        let from = fork_of(p, params)?;
        ecad_file::v2::project::check_branch_name(&name).map_err(|e| e.to_string())?;
        // Saved first (owner answer 3): the fork point and the branch row are in the file.
        p.save_pending(pg)?;
        let by = Some(author_text(&p.author));
        let v = p.file.v2_mut().ok_or("Branches live in the project file. Save it first.")?;
        let (id, rec) = v.create_branch(&name, from.branch, from.seq, crate::project::now_ms(), by).map_err(|e| e.to_string())?;
        p.note_branch_change();
        let mut line = format!("Branch \"{}\" created from {}.", rec.name, from_text(p, &from));
        if switch {
            switch_to(p, pg, id)?;
            line.push_str(&format!(" Now on branch \"{}\".", rec.name));
        }
        pg.echo(line.clone());
        Ok(json!({"branch": id.hex(), "name": rec.name, "from": {"branch": from.branch.hex(), "seq": from.seq}, "switched": switch, "message": line}))
    })
}

/// Show branch `id` (saved already), and say so.
pub(crate) fn switch_to(p: &mut Project, pg: &mut Pages<'_>, id: BranchId) -> Result<(), String> {
    let seed = crate::project::session_seed();
    let ledger = p.ledger.clone();
    let v = p.file.v2_mut().ok_or("Branches live in the project file. Save it first.")?;
    let (l, report) = v.switch_to(&ledger, id, seed).map_err(|e| e.to_string())?;
    let high = v.base_tag_high().clone();
    p.adopt_branch(pg, l, high)?;
    if let Some(d) = report.damage {
        pg.echo(format!("This branch is damaged ({}); it is shown read-only.", d.detail));
    }
    Ok(())
}

fn run_renamebranch(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let which = params.get("branch").and_then(Value::as_str).ok_or_else(|| err("`branch` is required"))?.to_string();
    let name = params.get("name").and_then(Value::as_str).ok_or_else(|| err("`name` is required"))?.to_string();
    with_any_page(s, |p, pg| {
        branch_file(p)?;
        let (id, old) = find_branch(p, &which)?;
        let v = p.file.v2_mut().ok_or("no file")?;
        let rec = v.rename_branch(id, &name).map_err(|e| e.to_string())?;
        p.note_branch_change();
        let line = format!("Branch \"{}\" renamed \"{}\".", old.name, rec.name);
        pg.echo(line.clone());
        Ok(json!({"branch": id.hex(), "name": rec.name, "message": line}))
    })
}

/// The refusal of a switch with unsaved changes on a file that cannot be written (design 020
/// §3.3: Save As or Cancel, never a silent loss).
pub fn read_only_refusal(p: &Project) -> String {
    let n = p.unsaved_transactions();
    format!(
        "{} has {n} unsaved change{} and the file is read-only. Use Save As to keep them; the branch was not switched.",
        current_name(p),
        if n == 1 { "" } else { "s" }
    )
}

/// The unsaved-steps refusal of `switchbranch` without `save` (design 020 §10).
pub fn unsaved_refusal(p: &Project) -> String {
    let n = p.unsaved_transactions();
    format!("{} has {n} unsaved change{}; add \"save\": true to save them and switch.", current_name(p), if n == 1 { "" } else { "s" })
}

fn run_switchbranch(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let which = params.get("branch").and_then(Value::as_str).ok_or_else(|| err("`branch` is required"))?.to_string();
    let save = params.get("save").and_then(Value::as_bool).unwrap_or(false);
    with_any_page(s, |p, pg| {
        if p.file.v2().is_none() {
            return Err("Branches live in the project file. Save it first.".into());
        }
        let (id, rec) = find_branch(p, &which)?;
        if id == current(p) {
            return Ok(json!({"branch": id.hex(), "name": rec.name, "switched": false, "message": format!("Already on branch \"{}\".", rec.name)}));
        }
        let read_only = p.file.v2().and_then(|v| v.read_only()).is_some();
        if p.unsaved() && read_only {
            // Never a silent loss (design 020 §3.3): the edits can only be kept by Save As.
            return Err(read_only_refusal(p));
        }
        if p.unsaved() {
            if !save {
                return Err(unsaved_refusal(p));
            }
            p.save_pending(pg)?;
        }
        switch_to(p, pg, id)?;
        let from = match rec.fork_point.as_ref() {
            Some(f) => {
                let parent = rec.parent.as_deref().and_then(BranchId::parse).unwrap_or_else(|| main_of(p));
                let named = named_points(p, parent).unwrap_or_default();
                match named.iter().find(|n| n.branch == parent && n.seq == f.seq) {
                    Some(n) => format!(" (from {})", n.name),
                    None => String::new(),
                }
            }
            None => String::new(),
        };
        let line = format!("Now on branch \"{}\"{from}. Undo works on this branch's own steps.", rec.name);
        pg.echo(line.clone());
        Ok(json!({"branch": id.hex(), "name": rec.name, "switched": true, "message": line}))
    })
}

fn run_archivebranch(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let which = params.get("branch").and_then(Value::as_str).ok_or_else(|| err("`branch` is required"))?.to_string();
    with_any_page(s, |p, pg| {
        branch_file(p)?;
        let (id, rec) = find_branch(p, &which)?;
        if id == main_of(p) {
            return Err("main is the release line and cannot be archived.".into());
        }
        let mut line = String::new();
        if id == current(p) {
            // Archiving the branch shown first switches to its parent, saving first; a parent that
            // is not open itself gives main instead, saying so (never a refusal after the save).
            let parent = rec.parent.as_deref().and_then(BranchId::parse).unwrap_or_else(|| main_of(p));
            let rows = branch_rows(p)?;
            let parent_row = rows.iter().find(|(i, _)| *i == parent).map(|(_, b)| b.clone());
            let (to, why) = match parent_row {
                Some(b) if b.status == BranchStatus::Open => (parent, String::new()),
                Some(b) => (main_of(p), format!("Its parent \"{}\" is {}, so the project shows main. ", b.name, status_word(&b.status))),
                None => (main_of(p), String::new()),
            };
            p.save_pending(pg)?;
            switch_to(p, pg, to)?;
            line.push_str(&format!("{why}Now on branch \"{}\". ", current_name(p)));
        }
        let v = p.file.v2_mut().ok_or("no file")?;
        v.set_branch_open(id, false).map_err(|e| e.to_string())?;
        p.note_branch_change();
        line.push_str(&format!("Branch \"{}\" archived; reopen it from \"Show merged and archived\".", rec.name));
        pg.echo(line.clone());
        Ok(json!({"branch": id.hex(), "name": rec.name, "message": line}))
    })
}

/// A name no open branch has: `base`, else `base (2)`, `base (3)`… (a reopen whose name was
/// taken, brief 020a).
pub fn free_branch_name(p: &Project, base: &str, except: Option<BranchId>) -> String {
    let taken: BTreeSet<String> = branch_rows(p)
        .unwrap_or_default()
        .into_iter()
        .filter(|(id, b)| b.status == BranchStatus::Open && Some(*id) != except)
        .map(|(_, b)| b.name.to_lowercase())
        .collect();
    if !taken.contains(&base.to_lowercase()) {
        return base.to_string();
    }
    (2..).map(|n| format!("{base} ({n})")).find(|n| !taken.contains(&n.to_lowercase())).unwrap_or_else(|| base.to_string())
}

fn run_reopenbranch(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let which = params.get("branch").and_then(Value::as_str).ok_or_else(|| err("`branch` is required"))?.to_string();
    let name = params.get("name").and_then(Value::as_str).map(str::to_string);
    with_any_page(s, |p, pg| {
        branch_file(p)?;
        let (id, rec) = find_branch(p, &which)?;
        if rec.status == BranchStatus::Open {
            let line = format!("Branch \"{}\" is already open.", rec.name);
            return Ok(json!({"branch": id.hex(), "name": rec.name, "message": line}));
        }
        let free = free_branch_name(p, &rec.name, Some(id));
        let v = p.file.v2_mut().ok_or("no file")?;
        let reopened = match &name {
            Some(n) => v.reopen_as(id, n),
            None => v.set_branch_open(id, true),
        };
        let rec2 = match reopened {
            Ok(r) => r,
            Err(ecad_file::v2::BranchError::NameTaken(other)) => {
                return Err(format!(
                    "Another open branch is called \"{other}\"; reopen this one under a new name, for example {{\"branch\": \"{}\", \"name\": \"{free}\"}}.",
                    id.hex()
                ));
            }
            Err(e) => return Err(e.to_string()),
        };
        p.note_branch_change();
        let line = if rec2.name == rec.name {
            format!("Branch \"{}\" is open again.", rec.name)
        } else {
            format!("Branch \"{}\" is open again as \"{}\".", rec.name, rec2.name)
        };
        pg.echo(line.clone());
        Ok(json!({"branch": id.hex(), "name": rec2.name, "message": line}))
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn id(n: u8) -> BranchId {
        BranchId([n; 16])
    }

    fn node(b: u8, seq: u64, kind: NodeKind, time: u64) -> Node {
        Node {
            branch: id(b),
            seq,
            kind,
            name: None,
            label: String::new(),
            time_ms: Some(time),
            author: String::new(),
            description: None,
            forks: Vec::new(),
            from: None,
        }
    }

    fn row(b: u8, fork: Option<(u8, u64)>, current: bool) -> BranchRow {
        BranchRow {
            id: id(b),
            name: format!("b{b}"),
            status: BranchStatus::Open,
            fork: fork.map(|(p, s)| (id(p), s)),
            head: 0,
            unsaved: 0,
            current,
            created_ms: None,
        }
    }

    #[test]
    fn rows_merge_by_time_and_branches_stay_above_their_fork() {
        let mut per = BTreeMap::new();
        let mut fork = node(0, 3, NodeKind::Version, 30);
        fork.forks.push(id(1));
        per.insert(id(0), vec![node(0, 9, NodeKind::Now, 90), fork, node(0, 0, NodeKind::Root, 0)]);
        // A wrong clock: the branch's rows are older than its fork row.
        per.insert(id(1), vec![node(1, 2, NodeKind::Now, 20), node(1, 1, NodeKind::Version, 10)]);
        let branches = vec![row(0, None, true), row(1, Some((0, 3)), false)];
        let out = merge_by_time(per, &branches);
        let order: Vec<(u8, u64)> = out.iter().map(|n| (n.branch.0[0], n.seq)).collect();
        assert_eq!(order, vec![(0, 9), (1, 2), (1, 1), (0, 3), (0, 0)]);
        let g = Graph { current: Some(id(1)), branches, nodes: out, more: false };
        let l = layout(&g);
        assert_eq!(l.lanes, vec![0, 1, 1, 0, 0]);
        let s1 = l.spans.iter().find(|s| s.branch == id(1)).unwrap();
        assert_eq!((s1.top, s1.bottom, s1.into), (1, 3, Some(0)));
        let s0 = l.spans.iter().find(|s| s.branch == id(0)).unwrap();
        assert_eq!(s0.current_from, Some(3), "main is the current line from the fork down");
        assert_eq!(s1.current_from, Some(1));
    }

    #[test]
    fn lanes_are_reused_and_overflow_into_the_last_one() {
        let mut nodes = Vec::new();
        let mut branches = vec![row(0, None, true)];
        // Eight branches, all open at once over main.
        for b in 1..=8u8 {
            nodes.push(node(b, 1, NodeKind::Now, 100 - u64::from(b)));
            branches.push(row(b, Some((0, 1)), false));
        }
        let mut f = node(0, 1, NodeKind::Version, 1);
        f.forks = (1..=8).map(id).collect();
        nodes.push(node(0, 2, NodeKind::Now, 200));
        nodes.insert(0, nodes.last().cloned().unwrap());
        nodes.pop();
        nodes.push(f);
        let g = Graph { current: Some(id(0)), branches, nodes, more: false };
        let l = layout(&g);
        assert!(l.spans.iter().all(|s| s.lane < MAX_LANES));
        assert_eq!(l.spans.iter().filter(|s| s.overflow).count(), 3);
    }

    #[test]
    fn times_read_in_the_local_zone_or_in_utc_labelled() {
        // 2026-11-10 09:12 UTC.
        let ms = 1_794_301_920_000;
        assert_eq!(full_time_in(Some(ms), Some(0)), "2026-11-10 09:12");
        assert_eq!(full_time_in(Some(ms), None), "2026-11-10 09:12 UTC");
        assert_eq!(full_time_in(Some(ms), Some(-10 * 3600)), "2026-11-09 23:12");
        assert_eq!(short_time_in(Some(ms), Some(ms), Some(3600)), "11-10 10:12");
        assert_eq!(short_time_in(Some(ms), Some(ms + 400 * 86_400_000), Some(0)), "2026-11-10");
        assert_eq!(short_time_in(None, Some(ms), Some(0)), "");
        // This build reads the local zone: no label.
        assert!(local_offset(ms).is_some());
        assert!(!full_time(Some(ms)).ends_with("UTC"));
    }
}
