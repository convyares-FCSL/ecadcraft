//! The version and branch graph of a project's history as the History tab shows it (brief 020a,
//! design 020; moved out of the engine by brief 074): branch rows, graph rows and their lanes,
//! named points, session groups of the change list, restore plans, and the words and JSON rows
//! the history commands show.
//!
//! - The rows of all branches merge newest first, a page at a time ([`merge_by_time_paged`]); each
//!   branch's own order is kept and its rows stay above its fork row.
//! - [`layout`] gives each branch its lane (`main` in lane 0, at most [`MAX_LANES`]), the current
//!   line and the merge joins.
//! - Times are shown with an offset the caller passes (`None`: UTC, labelled).
//!
//! This module intentionally contains **no** file reading and **no** clock or time-zone lookup:
//! the engine reads the rows from the project file and passes the local offset.

use std::collections::BTreeMap;

use crate::schema::{BranchId, BranchStatus};
use ecad_model::{Op, Transaction, TxKind};
use serde_json::{Value, json};

/// Longest version name, in characters (design 020 §2.5.1).
pub const MAX_VERSION_NAME: usize = ecad_model::document::rev::MAX_VERSION_NAME;
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
    /// What it is: a user's version or an issued revision, keyed by kind and name.
    pub point: ecad_model::PointName,
    /// How it is shown: the version's text, or `Rev B` as the project's scheme writes it.
    pub name: String,
    /// The branch holding the step it names.
    pub branch: BranchId,
    pub seq: u64,
    pub description: Option<String>,
}

/// Who: `you` for the user, the agent's name otherwise.
pub fn author_text(a: &ecad_model::Author) -> String {
    match a {
        ecad_model::Author::User => "you".into(),
        ecad_model::Author::Agent(n) => n.clone(),
    }
}

/// `ms` moved by `offset` seconds (none: UTC).
fn shifted(ms: u64, offset: Option<i64>) -> u64 {
    let ms = i128::from(ms) + i128::from(offset.unwrap_or(0)) * 1000;
    u64::try_from(ms.max(0)).unwrap_or(0)
}

/// `MM-DD HH:MM` this year, `YYYY-MM-DD` otherwise (design 020 §2.2), with the zone's offset given (`None`: UTC, labelled).
pub fn short_time_in(ms: Option<u64>, now_ms: Option<u64>, offset: Option<i64>) -> String {
    let Some(ms) = ms else { return String::new() };
    let (y, mo, d, h, mi) = civil(shifted(ms, offset));
    let this_year = now_ms.map(|n| civil(shifted(n, offset)).0) == Some(y);
    let utc = if offset.is_none() { " UTC" } else { "" };
    if this_year { format!("{mo:02}-{d:02} {h:02}:{mi:02}{utc}") } else { format!("{y:04}-{mo:02}-{d:02}{utc}") }
}

/// `YYYY-MM-DD HH:MM`, with the zone's offset given (`None`: UTC, labelled).
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

/// One row of History's list of changes (brief 043, screen f): a **session group**, a run of
/// consecutive transactions by one agent, or one transaction (a person's, or an agent's alone).
///
/// # Semantics
/// Read only: grouping changes nothing in the ledger. The order is the order given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionGroup {
    pub author: ecad_model::Author,
    /// The transactions' sequence numbers and ids, in the order given.
    pub seqs: Vec<u64>,
    pub ids: Vec<ecad_model::TxId>,
    /// The earliest and latest wall-clock time of its transactions, when known.
    pub from_ms: Option<u64>,
    pub to_ms: Option<u64>,
}

impl SessionGroup {
    /// Whether it groups more than one agent transaction (it collapses to one row).
    pub fn is_session(&self) -> bool {
        matches!(self.author, ecad_model::Author::Agent(_)) && self.seqs.len() > 1
    }

    /// `claude · 14 changes · 20:31–20:44`, with the zone's offset given (`None`: UTC, labelled).
    pub fn text_in(&self, offset: Option<i64>) -> String {
        let n = self.seqs.len();
        let changes = if n == 1 { "1 change".to_string() } else { format!("{n} changes") };
        let clock = |ms: u64| {
            let (_, _, _, h, mi) = civil(shifted(ms, offset));
            format!("{h:02}:{mi:02}")
        };
        let span = match (self.from_ms, self.to_ms) {
            (Some(a), Some(b)) if clock(a) == clock(b) => clock(a),
            (Some(a), Some(b)) => format!("{}\u{2013}{}", clock(a), clock(b)),
            _ => String::new(),
        };
        let utc = if offset.is_none() && !span.is_empty() { " UTC" } else { "" };
        let mut parts = vec![author_text(&self.author), changes];
        if !span.is_empty() {
            parts.push(format!("{span}{utc}"));
        }
        parts.join(" \u{b7} ")
    }
}

/// `log` (any order; the Steps list's is newest first) as History's rows: consecutive
/// transactions by one agent form one [`SessionGroup`]; every other transaction is a group of its
/// own. Reads only `log`.
pub fn sessions(log: &[Transaction]) -> Vec<SessionGroup> {
    sessions_split(log, |_| false)
}

/// [`sessions`], also split where `marked` changes between two neighbours: the ledger records an
/// agent by name only, so two bridge sessions of one agent back to back would read as one group;
/// marking one session's steps (the live one's) keeps each session its own group.
pub fn sessions_split(log: &[Transaction], marked: impl Fn(&ecad_model::TxId) -> bool) -> Vec<SessionGroup> {
    let mut out: Vec<SessionGroup> = Vec::new();
    let mut last_mark = false;
    for t in log {
        let mark = marked(&t.id);
        let joins = matches!(t.author, ecad_model::Author::Agent(_)) && out.last().is_some_and(|g| g.author == t.author) && mark == last_mark;
        last_mark = mark;
        if !joins {
            out.push(SessionGroup { author: t.author.clone(), seqs: Vec::new(), ids: Vec::new(), from_ms: None, to_ms: None });
        }
        let Some(group) = out.last_mut() else { continue };
        group.seqs.push(t.seq);
        group.ids.push(t.id);
        if let Some(ms) = t.time_ms {
            group.from_ms = Some(group.from_ms.map_or(ms, |a| a.min(ms)));
            group.to_ms = Some(group.to_ms.map_or(ms, |b| b.max(ms)));
        }
    }
    out
}

/// Options of the graph (design 020 §2.3: both off by default, per session).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Show {
    pub autos: bool,
    pub archived: bool,
    /// Rows to read (0: one page, [`PAGE`]); scrolling to the end adds a page.
    pub limit: usize,
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
pub fn merge_by_time_paged(
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

impl Named {
    /// Whether it is an issued revision.
    pub fn is_revision(&self) -> bool {
        matches!(self.point, ecad_model::PointName::Revision(_))
    }

    /// The title of its [`Point`]: `Version "Before change"` or `Rev B`.
    pub fn title(&self) -> String {
        if self.is_revision() { self.name.clone() } else { format!("Version \"{}\"", self.name) }
    }
}

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

/// What a restore would do: its ops, how many objects change, and the pages they are on.
#[derive(Debug, Clone, Default)]
pub struct RestorePlan {
    pub ops: Vec<Op>,
    pub objects: usize,
    pub pages: Vec<String>,
    /// Issued revisions after the point on this line (`It lands after Rev C`).
    pub after_revisions: Vec<String>,
}

pub fn node_json(n: &Node) -> Value {
    json!({
        "branch": n.branch.hex(), "seq": n.seq, "kind": n.kind.word(), "name": n.name, "label": n.label,
        "time": n.time_ms, "author": n.author, "description": n.description,
        "forks": n.forks.iter().map(BranchId::hex).collect::<Vec<_>>(),
        "from": n.from.map(|(b, s)| json!({"branch": b.hex(), "seq": s})),
    })
}

pub fn branch_json(b: &BranchRow) -> Value {
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

pub fn kind_word(k: TxKind) -> &'static str {
    match k {
        TxKind::Edit => "edit",
        TxKind::Undo { .. } => "undo",
        TxKind::Redo { .. } => "redo",
        TxKind::Merge { .. } => "merge",
    }
}

/// The devices added, removed and changed between two snapshots, read from their differing
/// component chunks only: `-K05 added`, at most three named, then how many more.
pub fn device_changes(
    r: &crate::Reader, a: &crate::schema::VersionRecord, b: &crate::schema::VersionRecord, keys: &[&String],
) -> Option<Vec<String>> {
    use crate::chunks::{ChunkSet, devices_of};
    let load = |v: &crate::schema::VersionRecord| -> Option<BTreeMap<ecad_model::DeviceId, ecad_model::Device>> {
        let set = ChunkSet::from_record(&v.chunks).ok()?;
        let mut out = BTreeMap::new();
        for k in keys {
            let Some(h) = v.chunks.get(*k) else { continue };
            let hash = crate::chain::parse_hex(h)?;
            let body = r.chunk_body(&hash).ok()?;
            out.extend(devices_of(&set, k, &body).ok()?);
        }
        Some(out)
    };
    let (ca, cb) = (load(a)?, load(b)?);
    let tag = |c: &ecad_model::Device| c.tag().unwrap_or("a device").to_string();
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

/// How a transaction's author is shown in the log (agent tools and the debugger): `user` or
/// `agent <name>`.
pub fn logged_author_text(a: &ecad_model::Author) -> String {
    match a {
        ecad_model::Author::User => "user".into(),
        ecad_model::Author::Agent(n) => format!("agent {n}"),
    }
}

/// A transaction's kind in words.
pub fn tx_kind(tx: &Transaction, log: &[Transaction]) -> String {
    let seq_of = |id: ecad_model::TxId| log.iter().find(|t| t.id == id).map_or_else(|| id.short(), |t| format!("#{}", t.seq));
    match tx.kind {
        TxKind::Edit => "edit".into(),
        TxKind::Undo { of } => format!("undo of {}", seq_of(of)),
        TxKind::Redo { of } => format!("redo of {}", seq_of(of)),
        TxKind::Merge { source, source_head, .. } => {
            format!("merge of {}… at {}", source.iter().take(4).map(|b| format!("{b:02x}")).collect::<String>(), source_head.seq)
        }
    }
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
    fn times_read_at_the_offset_given_or_in_utc_labelled() {
        // 2026-11-10 09:12 UTC.
        let ms = 1_794_301_920_000;
        assert_eq!(full_time_in(Some(ms), Some(0)), "2026-11-10 09:12");
        assert_eq!(full_time_in(Some(ms), None), "2026-11-10 09:12 UTC");
        assert_eq!(full_time_in(Some(ms), Some(-10 * 3600)), "2026-11-09 23:12");
        assert_eq!(short_time_in(Some(ms), Some(ms), Some(3600)), "11-10 10:12");
        assert_eq!(short_time_in(Some(ms), Some(ms + 400 * 86_400_000), Some(0)), "2026-11-10");
        assert_eq!(short_time_in(None, Some(ms), Some(0)), "");
    }
}
