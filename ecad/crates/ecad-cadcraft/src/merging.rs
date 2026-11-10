//! Merge a branch into its parent, and update a branch from its parent (brief 020b, design 020
//! §4–§6), with the `merge` and `undomerge` commands.
//!
//! - **Sides.** A branch merges into, and updates from, its parent only (owner answer 14). The
//!   merge is appended to the **target**: Merge into main runs on main (switching there first,
//!   saving the branch: owner answer 6), Update from main runs on the branch.
//! - **Ancestor** (design §6.1): the fork, or the newest live merge between the two branches in
//!   either direction, read from the merge index (FQ2), never from the logs.
//! - **What is read** (§6.2): both branches' steps since the fork (only those stretches) for the
//!   objects either side touched, and three states, each on a copy: the ancestor and the source
//!   from their nearest snapshots (as Show and switching do), the target live when it is the
//!   branch shown. The comparison is [`ecad_model::merge`], over the touched objects only.
//! - **What is written** (§6.3): one transaction of kind merge (FQ1) on the target, ordinary
//!   ops; the merge index row and, with Close branch, the source's `merged` status are written by
//!   the save that writes it.
//! - A prepared merge (its three states and scope) is kept while nothing it read changes, so the
//!   dialog's choices re-plan without reading the file again.
//!
//! Excluded: merging between sibling branches; the preview's "Show ▾" window; the count of new
//! Checks entries (the Checks tab shows them after the merge).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use cadcraft_engine::{CommandSpec, Session};
use ecad_file::v2::schema::{BranchId, BranchStatus};
use ecad_model::merge::{self, Choice, ConflictKind, Inputs, Obj, Plan, Scope};
use ecad_model::{BranchPoint, Document, StepPoint, TxKind};
use serde_json::{Value, json};

use crate::commands::{err, with_any_page, with_project};
use crate::history::{self, NodeKind, Point};
use crate::project::Project;

/// The two branches of a merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sides {
    pub source: BranchId,
    pub target: BranchId,
    pub source_name: String,
    pub target_name: String,
    /// Update from the parent (the target is the child).
    pub update: bool,
    /// The child's fork point on the parent.
    pub fork: u64,
}

impl Sides {
    /// The parent (`main` in the design's words) and the child (`branch`).
    pub fn parent(&self) -> BranchId {
        if self.update { self.source } else { self.target }
    }

    pub fn child(&self) -> BranchId {
        if self.update { self.target } else { self.source }
    }

    /// The side a choice word names: `main` (or the parent's name) is the parent, `branch` the
    /// child.
    fn parent_is_target(&self) -> bool {
        !self.update
    }
}

/// The sides of `merge {from, into?}`: `into` defaults to `from`'s parent, or (`from` is the
/// parent of the branch shown) the branch shown.
///
/// # Errors
/// Unknown branches, or two branches that are not parent and child, in words.
pub fn sides(p: &Project, from: &str, into: Option<&str>) -> Result<Sides, String> {
    let (sid, srec) = history::find_branch(p, from)?;
    let cur = history::current(p);
    let (tid, trec) = match into {
        Some(t) => history::find_branch(p, t)?,
        None => match srec.parent.as_deref().and_then(BranchId::parse) {
            Some(parent) if cur != parent && history::find_branch(p, &cur.hex())?.1.parent.as_deref() == Some(sid.hex().as_str()) => {
                // `from` is the parent of the branch shown: update the branch shown from it.
                history::find_branch(p, &cur.hex())?
            }
            Some(parent) => history::find_branch(p, &parent.hex())?,
            None => history::find_branch(p, &cur.hex())?,
        },
    };
    if sid == tid {
        return Err(format!("\"{}\" cannot be merged into itself; pick its parent, or update it from its parent.", srec.name));
    }
    let child_of = |child: &ecad_file::v2::schema::BranchRecord, parent: BranchId| child.parent.as_deref() == Some(parent.hex().as_str());
    let (update, fork) = if child_of(&srec, tid) {
        (false, srec.fork_point.as_ref().map_or(0, |f| f.seq))
    } else if child_of(&trec, sid) {
        (true, trec.fork_point.as_ref().map_or(0, |f| f.seq))
    } else {
        return Err(format!(
            "A branch merges into, and updates from, its parent only: \"{}\" and \"{}\" are not parent and child.",
            srec.name, trec.name
        ));
    };
    for (rec, what) in [(&srec, "merged from"), (&trec, "merged into")] {
        if rec.status != BranchStatus::Open {
            return Err(format!(
                "Branch \"{}\" is {}; reopen it (History, Show merged and archived) before it is {what}.",
                rec.name,
                history::status_word(&rec.status)
            ));
        }
    }
    Ok(Sides { source: sid, target: tid, source_name: srec.name, target_name: trec.name, update, fork })
}

/// A merge with its states read: kept while nothing it read changes.
#[derive(Debug)]
pub struct Prepared {
    pub sides: Sides,
    /// What it was read at: the source's head and the target's (and whether the target was the
    /// branch shown).
    pub key: (u64, u64, bool),
    /// The common ancestor: a branch and its step.
    pub ancestor: (BranchId, u64),
    /// The ancestor as the header names it (`Rev A`, `"Before change"`, `main, step 640`).
    pub ancestor_text: String,
    /// The header (crossing revisions, design §4.2).
    pub header: String,
    /// Revisions the target issued after the ancestor.
    pub crossing: Vec<String>,
    ancestor_doc: Arc<Document>,
    source_doc: Arc<Document>,
    /// The target's state when it is not the branch shown.
    target_doc: Option<Arc<Document>>,
    scope: Scope,
    high: ecad_model::tag::HighWater,
    /// The target's last step touching each object (for "changed in Rev B").
    target_last: BTreeMap<Obj, u64>,
    /// The target's revisions on its own line: name and step.
    target_revisions: Vec<(String, u64)>,
}

/// The newest live merge between the two sides, as the ancestor (design §6.1): a merge into the
/// target from the source records the source's head (`A1`), a merge into the source from the
/// target records the target's head (`A2`); the newer of them, else the fork.
fn ancestor(p: &Project, s: &Sides) -> Result<(BranchId, u64), String> {
    let v = p.file.v2().ok_or("Branches live in the project file. Save it first.")?;
    let rows = |b: BranchId| if b == history::current(p) { v.merge_index(&p.ledger) } else { v.merge_rows(b) };
    // A1: into the target from the source, at target step m, of source head s1.
    let a1 = rows(s.target)?.into_iter().filter(|m| m.source == s.source).max_by_key(|m| m.seq);
    // A2: into the source from the target, at source step u, of target head t2.
    let a2 = rows(s.source)?.into_iter().filter(|m| m.source == s.target).max_by_key(|m| m.seq);
    Ok(match (a1, a2) {
        (None, None) => (s.parent(), s.fork),
        (Some(m), None) => (s.source, m.source_head.0),
        (None, Some(m)) => (s.target, m.source_head.0),
        // S@s1 includes the update at u2 when s1 >= u2; else T@t2 includes the merge at m1.
        (Some(m1), Some(m2)) => {
            if m1.source_head.0 >= m2.seq {
                (s.source, m1.source_head.0)
            } else {
                (s.target, m2.source_head.0)
            }
        }
    })
}

/// A branch's head: the ledger's for the branch shown.
fn head_of(p: &Project, b: BranchId) -> Result<u64, String> {
    if b == history::current(p) { Ok(p.ledger.head()) } else { Ok(history::find_branch(p, &b.hex())?.1.head.seq) }
}

/// A point's name for texts (design §4.2): the revision or version there, else `<branch>, step n`
/// (with the nearest revision or version before it, when there is one).
fn point_text(p: &Project, b: BranchId, seq: u64) -> String {
    let named = history::named_points(p, b).unwrap_or_default();
    let quoted = |n: &history::Named| if n.revision { n.name.clone() } else { format!("version \"{}\"", n.name) };
    // A version names the step before the one that named it; both are the same state.
    if let Some(n) = named.iter().filter(|n| n.branch == b && (n.seq == seq || n.seq + 1 == seq)).max_by_key(|n| n.seq) {
        return quoted(n);
    }
    let bname = history::find_branch(p, &b.hex()).map(|r| r.1.name).unwrap_or_else(|_| "main".into());
    match named.iter().filter(|n| n.branch == b && n.seq < seq).max_by_key(|n| n.seq) {
        Some(n) => format!("{bname}, step {seq} (after {})", quoted(n)),
        None => format!("{bname}, step {seq}"),
    }
}

/// Read what a merge of `sides` needs (design §6.2).
///
/// # Errors
/// The file could not be read.
pub fn prepare(p: &Project, sides: Sides) -> Result<Prepared, String> {
    let cur = history::current(p);
    let (anc_branch, anc_seq) = ancestor(p, &sides)?;
    let source_head = head_of(p, sides.source)?;
    let target_head = head_of(p, sides.target)?;
    let doc_of = |b: BranchId, seq: u64| -> Result<Arc<Document>, String> {
        let mut d = history::state_at(p, &Point { branch: b, seq, title: String::new(), kind: NodeKind::Fork })?;
        d.rebuild_index();
        Ok(Arc::new(d))
    };
    let ancestor_doc = doc_of(anc_branch, anc_seq)?;
    let source_doc = if sides.source == cur { Arc::new(p.ledger.document().clone()) } else { doc_of(sides.source, source_head)? };
    let target_doc = if sides.target == cur { None } else { Some(doc_of(sides.target, target_head)?) };
    // The touched set: both branches' steps since the fork (states on both lines are the fork's
    // state plus those steps, so every difference lies there).
    let (child, parent) = (sides.child(), sides.parent());
    let mut scope = Scope::default();
    let mut high = p.file.v2().map(|v| v.base_tag_high().clone()).unwrap_or_default();
    let mut target_last: BTreeMap<Obj, u64> = BTreeMap::new();
    for (b, from, to) in [(child, 1, head_of(p, child)?), (parent, sides.fork + 1, head_of(p, parent)?)] {
        let mut at = from;
        while at <= to {
            let hi = to.min(at.saturating_add(4_095));
            for tx in history::txs(p, b, at, hi)? {
                scope.add_ops(&tx.ops);
                ecad_model::tag::note_ops(&mut high, &tx.ops);
                if b == sides.target {
                    for o in objects_of(&tx.ops) {
                        target_last.insert(o, tx.seq);
                    }
                }
            }
            let Some(next) = hi.checked_add(1) else { break };
            at = next;
        }
    }
    // Revisions on the target's line after the ancestor (crossing revisions, §4.2).
    let target_revisions: Vec<(String, u64)> = history::named_points(p, sides.target)?
        .into_iter()
        .filter(|n| n.revision && n.branch == sides.target)
        .map(|n| (n.name, n.seq))
        .rev()
        .collect();
    // Where the ancestor lies on the target's line: revisions after it cross the merge.
    let after = if anc_branch == sides.target { anc_seq } else { last_merge_pos(p, &sides).unwrap_or(sides.fork) };
    let crossing: Vec<String> = target_revisions.iter().filter(|(_, s)| *s > after).map(|(n, _)| n.clone()).collect();
    let ancestor_text = point_text(p, anc_branch, anc_seq);
    let header = if sides.update {
        let steps = head_of(p, sides.source)?.saturating_sub(if anc_branch == sides.source { anc_seq } else { sides.fork });
        let mut what: Vec<String> = history::named_points(p, sides.source)?
            .into_iter()
            .filter(|n| n.revision && n.branch == sides.source && n.seq > if anc_branch == sides.source { anc_seq } else { sides.fork })
            .map(|n| n.name)
            .collect();
        what.reverse();
        what.push(format!("{steps} step{}", if steps == 1 { "" } else { "s" }));
        format!(
            "Brings {}'s changes since {ancestor_text} ({}) into this branch. Issued revisions are not changed.",
            sides.source_name,
            join_and(&what)
        )
    } else {
        let mut h = format!("Based on {ancestor_text}.");
        let last = target_revisions.last().map(|(n, _)| n.clone());
        if !crossing.is_empty() {
            let lands = crossing.last().cloned().unwrap_or_default();
            h.push_str(&format!(
                " {} has since issued {}; the merge lands after {lands} and first appears in the next revision.",
                sides.target_name,
                join_and(&crossing)
            ));
        } else if let Some(l) = last {
            h.push_str(&format!(" It lands after {l} and first appears in the next revision."));
        }
        h
    };
    Ok(Prepared {
        key: (source_head, target_head, sides.target == cur),
        sides,
        ancestor: (anc_branch, anc_seq),
        ancestor_text,
        header,
        crossing,
        ancestor_doc,
        source_doc,
        target_doc,
        scope,
        high,
        target_last,
        target_revisions,
    })
}

/// The target-side position of the newest merge into the target from the source (where its
/// ancestor lands on the target's line).
fn last_merge_pos(p: &Project, s: &Sides) -> Option<u64> {
    let v = p.file.v2()?;
    let rows = if s.target == history::current(p) { v.merge_index(&p.ledger).ok()? } else { v.merge_rows(s.target).ok()? };
    rows.into_iter().filter(|m| m.source == s.source).map(|m| m.seq).max()
}

fn join_and(v: &[String]) -> String {
    match v.len() {
        0 => String::new(),
        1 => v[0].clone(),
        n => format!("{} and {}", v[..n - 1].join(", "), v[n - 1]),
    }
}

/// The objects `ops` name.
fn objects_of(ops: &[ecad_model::Op]) -> BTreeSet<Obj> {
    let mut s = Scope::default();
    s.add_ops(ops);
    let t = &s.touched;
    let mut out = BTreeSet::new();
    out.extend(t.pages.iter().chain(&t.page_properties).map(|x| Obj::Page(*x)));
    out.extend(t.components.iter().map(|x| Obj::Component(*x)));
    out.extend(t.conductors.iter().map(|x| Obj::Conductor(*x)));
    out.extend(t.cables.iter().map(|x| Obj::Cable(*x)));
    out.extend(t.representations.iter().map(|x| Obj::Representation(*x)));
    out.extend(t.items.iter().map(|x| Obj::Item(*x)));
    out.extend(t.terminals.iter().map(|x| Obj::Terminal(*x)));
    out.extend(t.folders.iter().map(|x| Obj::Folder(*x)));
    if s.project {
        out.insert(Obj::Project);
    }
    out
}

/// Whether a merge into `target` writes this project property: revision fields are never
/// merged into the release line (owner answer 10); an update brings them unchanged.
fn keeps(update: bool) -> impl Fn(&str) -> bool {
    move |k: &str| update || !k.starts_with("rev.")
}

/// Plan the prepared merge with `choices` (nothing is written).
pub fn plan(p: &Project, prep: &Prepared, choices: &BTreeMap<String, Choice>) -> Plan {
    let target: &Document = match &prep.target_doc {
        Some(d) => d,
        None => p.ledger.document(),
    };
    let keep = keeps(prep.sides.update);
    let (tname, sname) = if prep.sides.update {
        ("this branch".to_string(), prep.sides.source_name.clone())
    } else {
        (prep.sides.target_name.clone(), "the branch".to_string())
    };
    let inp = Inputs {
        ancestor: &prep.ancestor_doc,
        target,
        source: &prep.source_doc,
        scope: &prep.scope,
        target_name: &tname,
        source_name: &sname,
        keep_project_key: &keep,
        high: &prep.high,
    };
    merge::plan(&inp, choices)
}

/// The prepared merge for `sides`, from the project's cache when nothing it read changed.
///
/// # Errors
/// As [`prepare`].
pub fn prepared(p: &mut Project, sides: Sides) -> Result<Arc<Prepared>, String> {
    let cur = history::current(p);
    let key = (head_of(p, sides.source)?, head_of(p, sides.target)?, sides.target == cur);
    if let Some(c) = &p.merge_cache
        && c.sides == sides
        && c.key.0 == key.0
        && c.key.1 == key.1
    {
        if c.key.2 == key.2 {
            return Ok(c.clone());
        }
        // The target is now the branch shown, at the same step it was read at (a switch for the
        // merge saved it): its live state replaces the copy; the rest is unchanged.
        if key.2 && !p.unsaved() {
            let mut next = Prepared {
                sides: c.sides.clone(),
                key,
                ancestor: c.ancestor,
                ancestor_text: c.ancestor_text.clone(),
                header: c.header.clone(),
                crossing: c.crossing.clone(),
                ancestor_doc: c.ancestor_doc.clone(),
                source_doc: c.source_doc.clone(),
                target_doc: None,
                scope: c.scope.clone(),
                high: c.high.clone(),
                target_last: c.target_last.clone(),
                target_revisions: c.target_revisions.clone(),
            };
            next.key = key;
            let next = Arc::new(next);
            p.merge_cache = Some(next.clone());
            return Ok(next);
        }
    }
    let prep = Arc::new(prepare(p, sides)?);
    p.merge_cache = Some(prep.clone());
    Ok(prep)
}

/// The choices in a command's `choices` object: `main` / `branch` name the parent's and the
/// child's side (design §10), plus `loose`, `remove`, `bringback`, `renumber_branch`,
/// `renumber_main` and `{"value": text}`.
///
/// # Errors
/// A choice that is not one of these, in words.
pub fn parse_choices(s: &Sides, v: Option<&Value>) -> Result<BTreeMap<String, Choice>, String> {
    let mut out = BTreeMap::new();
    let Some(obj) = v.and_then(Value::as_object) else { return Ok(out) };
    let (parent, child) = if s.parent_is_target() { (Choice::Target, Choice::Source) } else { (Choice::Source, Choice::Target) };
    let (renumber_parent, renumber_child) =
        if s.parent_is_target() { (Choice::RenumberTarget, Choice::RenumberSource) } else { (Choice::RenumberSource, Choice::RenumberTarget) };
    for (id, c) in obj {
        let choice = match c {
            Value::String(w) => match w.as_str() {
                "main" | "parent" => parent.clone(),
                "branch" | "child" => child.clone(),
                "target" => Choice::Target,
                "source" => Choice::Source,
                "loose" | "remove" | "drop" => Choice::Drop,
                "bringback" => Choice::BringBack,
                "renumber_branch" | "rename_branch" => renumber_child.clone(),
                "renumber_main" | "rename_main" => renumber_parent.clone(),
                other => {
                    return Err(format!(
                        "\"{other}\" is not a choice for {id}: use main, branch, loose, bringback, renumber_branch or renumber_main."
                    ));
                }
            },
            Value::Object(o) => match o.get("value").and_then(Value::as_str) {
                Some(t) => Choice::Value(t.to_string()),
                None => return Err(format!("A typed choice for {id} is {{\"value\": text}}.")),
            },
            _ => return Err(format!("The choice for {id} is a word or {{\"value\": text}}.")),
        };
        out.insert(id.clone(), choice);
    }
    Ok(out)
}

/// A choice as the command word for these sides.
pub fn choice_word(s: &Sides, c: &Choice) -> Value {
    let pt = s.parent_is_target();
    match c {
        Choice::Target => json!(if pt { "main" } else { "branch" }),
        Choice::Source => json!(if pt { "branch" } else { "main" }),
        Choice::Value(t) => json!({ "value": t }),
        Choice::BringBack => json!("bringback"),
        Choice::Drop => json!("loose"),
        Choice::RenumberSource => json!(if pt { "renumber_branch" } else { "renumber_main" }),
        Choice::RenumberTarget => json!(if pt { "renumber_main" } else { "renumber_branch" }),
    }
}

/// When the target last changed an object, against its revisions (`changed in Rev B`, `changed
/// after Rev C, not issued yet`).
fn framed(prep: &Prepared, obj: Obj) -> Option<String> {
    let seq = *prep.target_last.get(&obj)?;
    if prep.target_revisions.is_empty() {
        return None;
    }
    Some(match prep.target_revisions.iter().find(|(_, s)| *s >= seq) {
        Some((n, _)) => format!("changed in {n}"),
        None => format!("changed after {}, not issued yet", prep.target_revisions.last().map(|r| r.0.clone()).unwrap_or_default()),
    })
}

fn kind_word(k: ConflictKind) -> &'static str {
    match k {
        ConflictKind::Value => "value",
        ConflictKind::Removed => "removed",
        ConflictKind::Needed => "needed",
        ConflictKind::Tag => "tag",
        ConflictKind::PageNumber => "page_number",
        ConflictKind::FolderName => "folder_name",
        ConflictKind::Order => "order",
    }
}

/// The preview as JSON (design §10): the header, the counts line, the conflicts with their
/// choices (`main` / `branch` words), the changes and what Merge would do.
pub fn preview_json(prep: &Prepared, plan: &Plan) -> Value {
    let s = &prep.sides;
    let pt = s.parent_is_target();
    let target_doc: &Document = prep.target_doc.as_deref().unwrap_or(&prep.ancestor_doc);
    let docs = [target_doc, &prep.source_doc, &prep.ancestor_doc];
    let pages: Vec<String> =
        plan.pages.iter().map(|p| ecad_model::book::stored_number(&prep.source_doc, *p).map_or_else(|| p.short(), str::to_string)).collect();
    let n = plan.changes.len();
    let mut counts =
        format!("{n} change{} from {}", if n == 1 { "" } else { "s" }, if s.update { s.source_name.clone() } else { "the branch".into() });
    if !pages.is_empty() {
        counts.push_str(&format!(" · page{} {}", if pages.len() == 1 { "" } else { "s" }, pages.join(", ")));
    }
    let c = plan.conflicts.len();
    counts.push_str(&format!(" · {c} conflict{}", if c == 1 { "" } else { "s" }));
    let conflicts: Vec<Value> = plan
        .conflicts
        .iter()
        .map(|c| {
            let (main_text, branch_text) = if pt { (&c.target, &c.source) } else { (&c.source, &c.target) };
            let frame = if c.kind == ConflictKind::Value || c.kind == ConflictKind::Removed { framed(prep, c.object) } else { None };
            let main_text = match (&frame, pt) {
                (Some(f), true) => format!("{main_text} ({f})"),
                _ => main_text.clone(),
            };
            json!({
                "id": c.id, "kind": kind_word(c.kind), "text": c.what,
                "main": main_text, "branch": branch_text,
                "choices": c.choices.iter().map(|x| choice_word(s, x)).collect::<Vec<_>>(),
                "editable": c.choices.iter().any(|x| matches!(x, Choice::Value(_))),
                "chosen": c.chosen.as_ref().map(|x| choice_word(s, x)),
                "object": merge::name_of(&docs, c.object),
            })
        })
        .collect();
    json!({
        "from": s.source_name, "into": s.target_name, "update": s.update,
        "title": if s.update { format!("UPDATE \"{}\" FROM {}", s.target_name, s.source_name) } else { format!("MERGE \"{}\" INTO {}", s.source_name, s.target_name) },
        "ancestor": prep.ancestor_text, "ancestor_point": {"branch": prep.ancestor.0.hex(), "seq": prep.ancestor.1},
        "crossing": prep.crossing, "header": prep.header, "counts": counts,
        "changes": plan.changes.iter().map(|c| json!({
            "sign": c.sign.to_string(), "text": c.text,
            "page": c.page.map(|p| ecad_model::book::stored_number(&prep.source_doc, p).map_or_else(|| p.short(), str::to_string)),
        })).collect::<Vec<_>>(),
        "conflicts": conflicts, "unanswered": plan.unanswered, "ops": plan.ops.len(),
        "nothing": plan.changes.is_empty() && plan.conflicts.is_empty(),
        "after": if plan.unanswered.is_empty() { "The Checks tab lists what the merge leaves to check.".to_string() } else { "Resolve the conflicts above first.".to_string() },
    })
}

/// The label of the merge transaction (design §6.3).
fn label(prep: &Prepared) -> String {
    let s = &prep.sides;
    if s.update {
        let latest = prep_latest_revision(prep);
        format!("Update from {} ({latest})", s.source_name)
    } else {
        format!("Merge \"{}\" (based on {})", s.source_name, prep.ancestor_text)
    }
}

fn prep_latest_revision(prep: &Prepared) -> String {
    prep.crossing.last().cloned().unwrap_or_else(|| format!("step {}", prep.key.0))
}

/// The commands this module adds (design §10).
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("merge", "Merge", run_merge)
            .params(
                "{from, into?, preview?: bool, choices?: {<conflict id>: \"main\"|\"branch\"|\"loose\"|\"bringback\"|\"renumber_branch\"|\"renumber_main\"|{\"value\": text}}, close?: true} \
                 (merges a branch into its parent, or updates a branch from its parent: one step on the target; preview only reads)",
            )
            .noundo(),
        CommandSpec::new("undomerge", "Undo Merge", run_undomerge).params("{} (undoes the last step when it is a merge, as u asks)").noundo(),
    ]
}

/// A refusal of a merge with unanswered conflicts: what to give, never a dead end.
fn unanswered_refusal(prep: &Prepared, plan: &Plan) -> String {
    let n = plan.unanswered.len();
    let example = plan.unanswered.first().map(|id| format!("{{\"choices\": {{\"{id}\": \"branch\"}}}}")).unwrap_or_default();
    let _ = prep;
    format!(
        "Choose for {n} conflict{} first ({}), for example {example}; or give every one \"main\" or \"branch\".",
        if n == 1 { "" } else { "s" },
        plan.unanswered.join(", ")
    )
}

fn run_merge(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let from = params.get("from").and_then(Value::as_str).ok_or_else(|| err("`from` is required (the branch to merge)"))?.to_string();
    let into = params.get("into").and_then(Value::as_str).map(str::to_string);
    let preview = params.get("preview").and_then(Value::as_bool) == Some(true);
    if preview {
        return with_any_page(s, |p, _| {
            if p.file.v2().is_none() {
                return Err("Branches live in the project file. Save it first.".into());
            }
            let sides = sides(p, &from, into.as_deref())?;
            let choices = parse_choices(&sides, params.get("choices"))?;
            let prep = prepared(p, sides)?;
            let plan = plan(p, &prep, &choices);
            Ok(preview_json(&prep, &plan))
        });
    }
    with_any_page(s, |p, pg| {
        crate::history::branch_file(p)?;
        let sides = sides(p, &from, into.as_deref())?;
        let choices = parse_choices(&sides, params.get("choices"))?;
        let close = !sides.update && params.get("close").and_then(Value::as_bool).unwrap_or(true);
        let mut said = String::new();
        // The merge runs on the target (owner answer 6): switch there, saving first.
        if sides.target != history::current(p) {
            let left = history::current_name(p);
            let was_unsaved = p.unsaved();
            p.save_pending(pg)?;
            history::switch_to(p, pg, sides.target)?;
            if was_unsaved {
                said = format!("\"{left}\" was saved; now on {}. ", sides.target_name);
            } else {
                said = format!("Now on {}. ", sides.target_name);
            }
        }
        let prep = prepared(p, sides.clone())?;
        let plan = plan(p, &prep, &choices);
        if !plan.unanswered.is_empty() {
            return Err(format!("{said}{}", unanswered_refusal(&prep, &plan)));
        }
        if plan.ops.is_empty() {
            let mut line = format!("{said}\"{}\" has no changes since {}.", sides.source_name, prep.ancestor_text);
            if sides.update {
                line = format!("{said}This branch already has {}'s changes.", sides.source_name);
            } else if close && params.get("close").and_then(Value::as_bool) == Some(true) {
                let head = p.ledger.head();
                let v = p.file.v2_mut().ok_or("no file")?;
                v.mark_merged(sides.source, sides.target, head).map_err(|e| e.to_string())?;
                p.note_branch_change();
                line.push_str(&format!(" Branch \"{}\" closed.", sides.source_name));
            }
            pg.echo(line.clone());
            return Ok(json!({"merged": false, "message": line}));
        }
        let v = p.file.v2().ok_or("no file")?;
        let source_head = prep.key.0;
        let kind = TxKind::Merge {
            source: sides.source.0,
            source_head: StepPoint { seq: source_head, chain: v.chain_of(sides.source, source_head)? },
            ancestor: BranchPoint { branch: prep.ancestor.0.0, seq: prep.ancestor.1, chain: v.chain_of(prep.ancestor.0, prep.ancestor.1)? },
        };
        let changes = plan.changes.len();
        let resolved = plan.conflicts.len();
        let tx = p.apply_merge(pg, &label(&prep), kind, plan.ops.clone())?;
        if close && let Some(v) = p.file.v2_mut() {
            v.close_on_save(tx, sides.source);
        }
        p.merge_cache = None;
        p.note_branch_change();
        let resolved_text =
            if resolved > 0 { format!(", {resolved} conflict{} resolved", if resolved == 1 { "" } else { "s" }) } else { String::new() };
        let line = if sides.update {
            format!("{said}Updated \"{}\" from {} ({}). Save to keep it.", sides.target_name, sides.source_name, prep_latest_revision(&prep))
        } else {
            format!(
                "{said}Merged \"{}\" into {} ({changes} change{}{resolved_text}). Save to keep it.",
                sides.source_name,
                sides.target_name,
                if changes == 1 { "" } else { "s" }
            )
        };
        pg.echo(line.clone());
        Ok(json!({
            "merged": true, "transaction": p.ledger.head(), "from": sides.source_name, "into": sides.target_name,
            "changes": changes, "conflicts": resolved, "closes": close, "message": line,
        }))
    })
}

/// The question `u` asks before undoing a merge (design §4.3), or `None` when the last step is
/// not a merge.
pub fn undo_question(p: &Project) -> Option<String> {
    let top = p.ledger.undo_top()?;
    let is_merge = match top.kind {
        TxKind::Merge { .. } => true,
        TxKind::Redo { .. } => p.file.v2().and_then(|v| v.merge_index(&p.ledger).ok()).is_some_and(|m| m.iter().any(|x| x.seq == top.seq)),
        _ => false,
    };
    if !is_merge {
        return None;
    }
    // Counted as the preview counts: devices, wires, cables, pages, folders and the project (a
    // symbol and its drawing are shown through what they depict).
    let n = ecad_model::diff::objects(&top.ops).keys().filter(|(k, _)| !matches!(*k, "item" | "representation" | "terminal")).count().max(1);
    let changes = format!("{n} change{}", if n == 1 { "" } else { "s" });
    let label = top.label.clone();
    if let Some(rest) = label.strip_prefix("Update from ") {
        let from = rest.split(" (").next().unwrap_or(rest);
        return Some(format!("The last step updated this branch from {from} ({changes}).\nUndo the update?"));
    }
    let name = label.strip_prefix("Merge ").and_then(|r| r.split(" (based on").next()).unwrap_or(&label).to_string();
    Some(format!("The last step merged {name} into {} ({changes}).\nUndo the merge? {name} is reopened.", history::current_name(p)))
}

fn run_undomerge(s: &mut Session, _params: &Value) -> cadcraft_engine::Result<Value> {
    with_project(s, |p, pg, _| {
        if undo_question(p).is_none() {
            return Err("The last step is not a merge; use Undo.".into());
        }
        let label = p.ledger.undo_top().map(|t| t.label.clone()).unwrap_or_default();
        p.confirm_merge_undo = true;
        let r = p.undo_step(pg);
        p.confirm_merge_undo = false;
        r?;
        p.merge_cache = None;
        p.note_branch_change();
        let line = match label.strip_prefix("Merge ").and_then(|r| r.split(" (based on").next()) {
            Some(name) => format!("Merge of {name} undone; the branch is open again when saved."),
            None => format!("{label} undone."),
        };
        pg.echo(line.clone());
        Ok(json!({"undone": true, "message": line}))
    })
}
