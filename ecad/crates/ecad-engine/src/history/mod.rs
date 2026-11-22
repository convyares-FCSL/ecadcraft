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
//!   difference between the live state and the point's ([`ecad_model::document::diff`]), so they scale with
//!   what differs; versions and revision fields are never restored (they are history, not
//!   design).
//! - Branch actions are written to the file at once (owner answer 4, FQ8), after saving the
//!   branch shown (owner answer 3, "Save and switch").
//!
//! - Merges (brief 020b, [`crate::history::merging`]) are rows of their own, read from the merge index
//!   (FQ2), drawn as joins from the source's line.
//! - The graph reads one page of rows ([`PAGE`]) and the next on request (`limit`); each row
//!   reads its step's time and author only when it comes into the page.
//! - The graph's model (rows, lanes, session groups, time text, JSON rows) is
//!   [`ecad_file::history`]; this module reads the project and the local zone for it.

pub mod branches;
pub mod event_log;
pub mod merging;
pub mod points;
pub mod versions;
use branches::{ArchivebranchParams, BranchParams, BranchesParams, RenamebranchParams, ReopenbranchParams, SwitchbranchParams};
use points::{ClosepointParams, PointQuery, PointinfoParams, RestoreParams, ShowpointParams};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use versions::VersionParams;

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_file::chunks::{ChunkKey, ModelKind};
use ecad_file::schema::{BranchId, BranchRecord, BranchStatus, RowKind};
use ecad_model::{Document, Op, Transaction};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::access::Pages;
use crate::commands::input::{Command, Typed, typed};
use crate::commands::{on_any_page, on_project};
use crate::pages::commands::sheets::{RevisionView, next_view_id};
use crate::project::Project;
use ecad_electrical::releases::revisions;
use ecad_model::{PointName, Rev, VersionName};

use ecad_file::history::{
    BranchRow, Graph, MAX_VERSION_NAME, Named, Node, NodeKind, PAGE, Point, RestorePlan, STEPS_PAGE, SessionGroup, Show, author_text, branch_json,
    full_time_in, kind_word, merge_by_time_paged, node_json, short_time_in, status_word,
};

// ---------------------------------------------------------------- reading the project

/// `main`'s ID for this project.
pub fn main_of(p: &Project) -> BranchId {
    BranchId::main_of(p.id.0)
}

/// The branch shown.
pub fn current(p: &Project) -> BranchId {
    p.file.project().map_or_else(|| main_of(p), ecad_file::Project::branch)
}

/// The branch shown's name.
pub fn current_name(p: &Project) -> String {
    p.file.project().map_or_else(|| "main".to_string(), |v| v.branch_record().name.clone())
}

/// Every branch row of the file (`main` first); a project without a version 2 file has only
/// `main`, in memory.
///
/// # Errors
/// The file could not be read.
pub fn branch_rows(p: &Project) -> Result<Vec<(BranchId, BranchRecord)>, String> {
    match p.file.project() {
        Some(v) => v.branches().map_err(|e| e.to_string()),
        None => {
            let head = ecad_file::schema::Point { seq: p.ledger.head(), chain: String::new() };
            Ok(vec![(main_of(p), BranchRecord::main(head))])
        }
    }
}

/// The lineage of `branch`: itself, then its parents up to `main`, each with the parent's step it
/// forks after.
fn lineage_of(p: &Project, branch: BranchId) -> Result<Vec<(BranchId, Option<u64>)>, String> {
    let Some(v) = p.file.project() else {
        return Ok(vec![(main_of(p), None)]);
    };
    let line = if branch == v.branch() {
        v.lineage().clone()
    } else {
        let r = v.store().reader().map_err(|e| e.to_string())?;
        ecad_file::project::lineage(&r, p.id, branch).map_err(|e| e.to_string())?
    };
    Ok(line.iter().map(|l| (l.branch, l.fork.map(|f| f.1))).collect())
}

/// The version rows (user and revision) of `branch` in the file.
fn rows(p: &Project, branch: BranchId, kinds: &[RowKind]) -> Result<Vec<ecad_file::schema::VersionRecord>, String> {
    let Some(v) = p.file.project() else { return Ok(Vec::new()) };
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
    let scheme = &p.ledger.document().project().rev_scheme;
    let mut inherited: Vec<Named> = Vec::new();
    for i in 1..line.len() {
        let (anc, _) = line[i];
        let bound = line.get(i - 1).and_then(|l| l.1).unwrap_or(0);
        for v in rows(p, anc, &[RowKind::VERSION, RowKind::REVISION])? {
            if v.point.seq <= bound
                && let Some(point) = v.named_point()
            {
                inherited.push(Named { name: point.label(scheme), point, branch: anc, seq: v.point.seq, description: v.description.clone() });
            }
        }
    }
    let mut own: Vec<Named> = Vec::new();
    if branch == current(p) {
        // The branch shown: from the ledger, so unsaved versions count and undone ones do not.
        let doc = p.ledger.document();
        let stored: BTreeMap<PointName, Option<String>> = rows(p, branch, &[RowKind::VERSION, RowKind::REVISION])?
            .into_iter()
            .filter_map(|v| v.named_point().map(|k| (k, v.description)))
            .collect();
        let names: BTreeSet<&PointName> = inherited.iter().map(|n| &n.point).collect();
        for (point, at) in doc.versions() {
            if names.contains(point) {
                continue;
            }
            let description = stored.get(point).cloned().flatten().or_else(|| unsaved_description(p, point));
            own.push(Named { name: point.label(scheme), point: point.clone(), branch, seq: *at, description });
        }
    } else {
        for v in rows(p, branch, &[RowKind::VERSION, RowKind::REVISION])? {
            if let Some(point) = v.named_point() {
                own.push(Named { name: point.label(scheme), point, branch, seq: v.point.seq, description: v.description });
            }
        }
    }
    own.sort_by_key(|n| std::cmp::Reverse(n.seq));
    own.extend(inherited);
    Ok(own)
}

/// The description of an unsaved version, from the step that named it.
fn unsaved_description(p: &Project, name: &PointName) -> Option<String> {
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
    p.file.project()?.tx_display(branch, seq, seq).ok()?.into_iter().next()
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
            match (p.file.project(), p.ledger.history()) {
                (Some(v), _) => out.extend(v.tx_display(branch, from, hi).map_err(|e| e.to_string())?),
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
    p.file.project().ok_or("this branch is not in a file")?.tx_display(branch, from, to).map_err(|e| e.to_string())
}

/// The local zone's offset from UTC at `ms`, in seconds (overseer, 2026-11-10: the graph and the
/// details show local time; the file keeps UTC). `None` when the time is out of range.
pub(crate) fn local_offset(ms: u64) -> Option<i64> {
    use chrono::{Offset, TimeZone};
    let utc = chrono::DateTime::from_timestamp(i64::try_from(ms / 1000).ok()?, 0)?.naive_utc();
    Some(i64::from(chrono::Local.offset_from_utc_datetime(&utc).fix().local_minus_utc()))
}

/// `MM-DD HH:MM` this year, `YYYY-MM-DD` otherwise, in local time (design 020 §2.2); ` UTC`
/// follows when the local zone cannot be read.
pub fn short_time(ms: Option<u64>, now_ms: Option<u64>) -> String {
    short_time_in(ms, now_ms, ms.and_then(local_offset))
}

/// `YYYY-MM-DD HH:MM` in local time; ` UTC` follows when the local zone cannot be read.
pub fn full_time(ms: Option<u64>) -> String {
    full_time_in(ms, ms.and_then(local_offset))
}

/// `g`'s text, `claude · 14 changes · 20:31–20:44`, in local time (` UTC` when the zone cannot be
/// read).
pub fn group_text(g: &SessionGroup) -> String {
    g.text_in(g.from_ms.and_then(local_offset))
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
                    kind: if n.is_revision() { NodeKind::Revision } else { NodeKind::Version },
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
            for v in rows(p, *id, &[RowKind::Auto])? {
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
        if let Some(v) = p.file.project() {
            let merges = if is_cur { v.merge_index(&p.ledger) } else { v.merge_rows(*id) }.map_err(|e| e.to_string())?;
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
            let label = "Project created".to_string();
            let created = p.file.project().and_then(ecad_file::Project::created_ms);
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

// ---------------------------------------------------------------- points

/// Find a branch by ID (32 hex digits) or name (open ones first, any case).
///
/// # Errors
/// No such branch.
pub(crate) fn find_branch(p: &Project, text: &str) -> Result<(BranchId, BranchRecord), String> {
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
// ---------------------------------------------------------------- the commands

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[
    Typed::of::<HistoryParams>("history"),
    Typed::of::<StepsParams>("steps"),
    Typed::of::<PointinfoParams>("pointinfo"),
    Typed::of::<VersionParams>("version"),
    Typed::of::<ShowpointParams>("showpoint"),
    Typed::of::<ClosepointParams>("closepoint"),
    Typed::of::<RestoreParams>("restore"),
    Typed::of::<BranchesParams>("branches"),
    Typed::of::<BranchParams>("branch"),
    Typed::of::<RenamebranchParams>("renamebranch"),
    Typed::of::<SwitchbranchParams>("switchbranch"),
    Typed::of::<ArchivebranchParams>("archivebranch"),
    Typed::of::<ReopenbranchParams>("reopenbranch"),
];

/// The commands this module adds (design 020 §10). Merge is brief 020b.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("history", "History", typed::<HistoryParams>)
            .params("{autos?: bool, archived?: bool, limit?: 200} (the History graph, newest first, `limit` rows; read only)")
            .noundo(),
        CommandSpec::new("steps", "History Steps", typed::<StepsParams>)
            .params("{branch?, before?: seq, limit?: 50} (steps newest first, read only)")
            .noundo(),
        CommandSpec::new("pointinfo", "Point Info", typed::<PointinfoParams>)
            .params("{branch?, version?|revision?|seq?} (the details of a point, read only)")
            .noundo(),
        CommandSpec::new("version", "New Version", typed::<VersionParams>)
            .params("{name, description?} (keeps the project as it is now under this name: one step)")
            .noundo(),
        CommandSpec::new("showpoint", "Show Point", typed::<ShowpointParams>)
            .params("{branch?, version?|revision?|seq?} (that state read only; records nothing)")
            .noundo(),
        CommandSpec::new("closepoint", "Back to Current", typed::<ClosepointParams>).params("{}").noundo(),
        CommandSpec::new("restore", "Restore", typed::<RestoreParams>)
            .params("{version?|revision?|seq?, branch?, preview?: bool} (makes the project as it was there: one step, undoable; preview only asks)")
            .noundo(),
        CommandSpec::new("branches", "Branches", typed::<BranchesParams>).params("{} (the branch list, read only)").noundo(),
        CommandSpec::new("branch", "New Branch", typed::<BranchParams>)
            .params("{name, from?: {branch?, version?|revision?|seq?}, switch?: true} (saves first; written at once)")
            .noundo(),
        CommandSpec::new("renamebranch", "Rename Branch", typed::<RenamebranchParams>).params("{branch, name}").noundo(),
        CommandSpec::new("switchbranch", "Switch Branch", typed::<SwitchbranchParams>)
            .params("{branch, save?: bool} (refused with unsaved changes unless save is true)")
            .noundo(),
        CommandSpec::new("archivebranch", "Archive Branch", typed::<ArchivebranchParams>).params("{branch}").noundo(),
        CommandSpec::new("reopenbranch", "Reopen Branch", typed::<ReopenbranchParams>)
            .params("{branch, name?} (name: reopen under that name, in one step, when another open branch has its name)")
            .noundo(),
    ]
}

/// `history {autos?, archived?, limit?}` (read only): the History graph, newest first, `limit`
/// rows.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryParams {
    /// Also the automatic steps.
    #[serde(default)]
    autos: Option<bool>,
    /// Also merged and archived branches.
    #[serde(default)]
    archived: Option<bool>,
    /// Rows to read (default one page).
    #[serde(default)]
    limit: Option<u64>,
}

impl Command for HistoryParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let show = Show {
            autos: self.autos.unwrap_or(false),
            archived: self.archived.unwrap_or(false),
            limit: self.limit.map_or(0, |l| usize::try_from(l).unwrap_or(usize::MAX).max(1)),
        };
        on_any_page(s, |p, _| {
            let g = graph(p, show).map_err(CmdError::refused)?;
            Ok(json!({
                "current": g.current.map(|c| c.hex()),
                "branches": g.branches.iter().map(branch_json).collect::<Vec<_>>(),
                "nodes": g.nodes.iter().map(node_json).collect::<Vec<_>>(),
                "more": g.more,
            }))
        })
    }
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

/// `steps {branch?, before?, limit?}` (read only): the steps of a branch newest first.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct StepsParams {
    /// The branch: its ID or name (default: the branch shown).
    #[serde(default)]
    branch: Option<String>,
    /// Only steps before this one.
    #[serde(default)]
    before: Option<u64>,
    /// At most this many (default 50, at most 1000).
    #[serde(default)]
    limit: Option<u64>,
}

impl Command for StepsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let limit = self.limit.unwrap_or(STEPS_PAGE).clamp(1, 1_000);
        on_any_page(s, |p, _| {
            let b = match &self.branch {
                Some(t) => find_branch(p, t).map_err(CmdError::refused)?.0,
                None => current(p),
            };
            let v = steps(p, b, self.before, limit).map_err(CmdError::refused)?;
            Ok(json!({"steps": v.iter().map(|t| json!({
                "seq": t.seq, "label": t.label, "author": author_text(&t.author), "time": t.time_ms, "kind": kind_word(t.kind),
            })).collect::<Vec<_>>()}))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_in_the_local_zone() {
        // 2026-11-10 09:12 UTC. This build reads the local zone: no label.
        let ms = 1_794_301_920_000;
        assert!(local_offset(ms).is_some());
        assert!(!full_time(Some(ms)).ends_with("UTC"));
        assert_eq!(full_time(Some(ms)), full_time_in(Some(ms), local_offset(ms)));
    }
}
