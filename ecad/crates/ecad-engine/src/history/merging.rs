//! Merge a branch into its parent, and update a branch from its parent,
//! with the `merge` and `undomerge` commands.
//!
//! - **Sides.** A branch merges into, and updates from, its parent only. The
//!   merge is appended to the **target**: Merge into main runs on main (switching there first,
//!   saving the branch), Update from main runs on the branch.
//! - **Ancestor**: the fork, or the newest live merge between the two branches in
//!   either direction, read from the merge index, never from the logs.
//! - **What is read**: both branches' steps since the fork (only those stretches) for the
//!   objects either side touched, and three states, each on a copy: the ancestor and the source
//!   from their nearest snapshots (as Show and switching do), the target live when it is the
//!   branch shown. The comparison is [`ecad_model::merge`], over the touched objects only.
//! - **What is written**: one transaction of kind merge on the target, ordinary
//!   ops; the merge index row and, with Close branch, the source's `merged` status are written by
//!   the save that writes it.
//! - A prepared merge (its three states and scope) is kept while nothing it read changes, so the
//!   dialog's choices re-plan without reading the file again.
//!
//! Excluded: merging between sibling branches; the preview's "Show ▾" window; the count of new
//! Checks entries (the Checks tab shows them after the merge).

use std::collections::BTreeMap;
use std::sync::Arc;

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_file::schema::{BranchId, BranchStatus};
use ecad_model::merge::apply;
use ecad_model::merge::state::{self, Choice, ConflictKind, Inputs, Obj, Plan, Scope};
use ecad_model::{BranchPoint, Document, StepPoint, TxKind};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::access::Pages;
use crate::commands::input::{JsonCommand, Typed, run_json};
use crate::commands::{on_any_page, on_project};
use crate::history;
use crate::project::Project;
use ecad_file::history::{Named, NodeKind, Point, status_word};

/// The two branches of a merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sides {
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
    /// The parent (`main`) and the child (`branch`).
    pub(crate) fn parent(&self) -> BranchId {
        if self.update { self.source } else { self.target }
    }

    pub(crate) fn child(&self) -> BranchId {
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
pub(crate) fn sides(p: &Project, from: &str, into: Option<&str>) -> Result<Sides, String> {
    let (sid, srec) = history::find_branch(p, from)?;
    let cur = history::current(p);
    let (tid, trec) = match into {
        Some(t) => history::find_branch(p, t)?,
        None => match srec.parent {
            Some(parent) if cur != parent && history::find_branch(p, &cur.hex())?.1.parent == Some(sid) => {
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
    let child_of = |child: &ecad_file::schema::BranchRecord, parent: BranchId| child.parent == Some(parent);
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
                status_word(&rec.status)
            ));
        }
    }
    Ok(Sides { source: sid, target: tid, source_name: srec.name, target_name: trec.name, update, fork })
}

/// A merge with its states read: kept while nothing it read changes.
#[derive(Debug)]
pub(crate) struct Prepared {
    pub sides: Sides,
    /// What it was read at: the source's head and the target's (and whether the target was the
    /// branch shown).
    pub key: (u64, u64, bool),
    /// The common ancestor: a branch and its step.
    pub ancestor: (BranchId, u64),
    /// The ancestor as the header names it (`Rev A`, `"Before change"`, `main, step 640`).
    pub ancestor_text: String,
    /// The header (crossing revisions).
    pub header: String,
    /// Revisions the target issued after the ancestor.
    pub crossing: Vec<String>,
    ancestor_doc: Arc<Document>,
    source_doc: Arc<Document>,
    /// The target's state when it is not the branch shown.
    /// (Locked to try a plan's ops on it, which are always reverted.)
    target_doc: Option<std::sync::Mutex<Document>>,
    scope: Scope,
    high: ecad_model::naming::tag::HighWater,
    /// The target's last step touching each object (for "changed in Rev B").
    target_last: BTreeMap<Obj, u64>,
    /// The target's revisions on its own line: name and step.
    target_revisions: Vec<(String, u64)>,
}

/// The newest live merge between the two sides, as the ancestor: a merge into the
/// target from the source records the source's head (`A1`), a merge into the source from the
/// target records the target's head (`A2`); the newer of them, else the fork.
fn ancestor(p: &Project, s: &Sides) -> Result<(BranchId, u64), String> {
    let v = p.storage.file.project().ok_or("Branches live in the project file. Save it first.")?;
    let rows = |b: BranchId| if b == history::current(p) { v.merge_index(&p.ledger) } else { v.merge_rows(b) }.map_err(|e| e.to_string());
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

/// A point's name for texts: the revision or version there, else `<branch>, step n`
/// (with the nearest revision or version before it, when there is one).
fn point_text(p: &Project, b: BranchId, seq: u64) -> String {
    let named = history::named_points(p, b).unwrap_or_default();
    let quoted = |n: &Named| if n.is_revision() { n.name.clone() } else { format!("version \"{}\"", n.name) };
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

/// Read what a merge of `sides` needs.
///
/// # Errors
/// The file could not be read.
pub(crate) fn prepare(p: &Project, sides: Sides) -> Result<Prepared, String> {
    let cur = history::current(p);
    let (anc_branch, anc_seq) = ancestor(p, &sides)?;
    let source_head = head_of(p, sides.source)?;
    let target_head = head_of(p, sides.target)?;
    let doc_of = |b: BranchId, seq: u64| -> Result<Arc<Document>, String> {
        history::points::state_at(p, &Point { branch: b, seq, title: String::new(), kind: NodeKind::Fork }).map(Arc::new)
    };
    let ancestor_doc = doc_of(anc_branch, anc_seq)?;
    let source_doc = if sides.source == cur { Arc::new(p.ledger.document().clone()) } else { doc_of(sides.source, source_head)? };
    let target_doc = if sides.target == cur {
        None
    } else {
        Some(std::sync::Mutex::new(Arc::try_unwrap(doc_of(sides.target, target_head)?).unwrap_or_else(|d| (*d).clone())))
    };
    // The touched set: both branches' steps since the fork (states on both lines are the fork's
    // state plus those steps, so every difference lies there).
    let (child, parent) = (sides.child(), sides.parent());
    let mut scope = Scope::default();
    let mut high = p.storage.file.project().map(|v| v.base_tag_high().clone()).unwrap_or_default();
    let mut target_last: BTreeMap<Obj, u64> = BTreeMap::new();
    for (b, from, to) in [(child, 1, head_of(p, child)?), (parent, sides.fork + 1, head_of(p, parent)?)] {
        let mut at = from;
        while at <= to {
            let hi = to.min(at.saturating_add(4_095));
            for tx in history::txs(p, b, at, hi)? {
                scope.add_ops(&tx.ops);
                ecad_model::naming::tag::note_ops(&mut high, &tx.ops);
                if b == sides.target {
                    for o in state::objects_of(&tx.ops) {
                        target_last.insert(o, tx.seq);
                    }
                }
            }
            let Some(next) = hi.checked_add(1) else { break };
            at = next;
        }
    }
    // Revisions on the target's line after the ancestor (crossing revisions).
    let target_revisions: Vec<(String, u64)> = history::named_points(p, sides.target)?
        .into_iter()
        .filter(|n| n.is_revision() && n.branch == sides.target)
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
            .filter(|n| n.is_revision() && n.branch == sides.source && n.seq > if anc_branch == sides.source { anc_seq } else { sides.fork })
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
    let v = p.storage.file.project()?;
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

/// Whether a merge into `target` writes this project property: revision fields are never
/// merged into the release line; an update brings them unchanged. The tag
/// high-water resets (`numbering.high.<P>`) are internal and never merged: the
/// target's stays, so its mark is never lowered, and the merge's own tag ops raise it to every
/// tag brought in (no conflict row for them either).
fn keeps(update: bool) -> impl Fn(&str) -> bool {
    move |k: &str| ecad_model::naming::keys::NUMBERING_HIGH.parse(k).is_none() && (update || !k.starts_with("rev."))
}

/// Plan the prepared merge with `choices` (nothing is written). The plan's ops are then tried
/// with the commit's own check, on the live document (the branch shown) or the prepared copy,
/// and reverted: a preview that says yes never ends in a refused
/// Merge. The cost follows the ops.
pub(crate) fn plan(p: &mut Project, prep: &Prepared, choices: &BTreeMap<String, Choice>) -> Plan {
    let keep = keeps(prep.sides.update);
    let (tname, sname) = if prep.sides.update {
        ("this branch".to_string(), prep.sides.source_name.clone())
    } else {
        (prep.sides.target_name.clone(), "the branch".to_string())
    };
    let mut copy = prep.target_doc.as_ref().map(|m| m.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
    let mut plan = {
        let target: &Document = match &copy {
            Some(d) => d,
            None => p.ledger.document(),
        };
        let rule = |obj: Obj, key: &str, v: &str| ecad_electrical::merge::edit_rule(target, obj, key, v).map_err(state::RuleError::from);
        // Make them one: the device merge, with fresh IDs from the project.
        let ids = std::cell::RefCell::new(&mut p.ids);
        let join = |d: &Document, from: ecad_model::DeviceId, into: ecad_model::DeviceId| {
            let mut ids = ids.borrow_mut();
            ecad_electrical::devices::merge::plan_merge(d, &mut ids, from, into).map_err(state::RuleError::from)
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
            edit_rule: &rule,
            join: &join,
        };
        let mut plan = apply::plan(&inp, choices);
        // A built-in style's conflict is worded by its name, which
        // only the app knows (`Text style "Tag"`, not the ID `tag`).
        for c in &mut plan.conflicts {
            let Some(rest) = c.id.strip_prefix("project/style.") else { continue };
            let Some((_, rest)) = rest.split_once('.') else { continue };
            let Some((cat, id)) = rest.split_once('.') else { continue };
            if let Some(sid) = ecad_model::StyleRef::parse(id).filter(ecad_model::StyleRef::is_builtin)
                && let Some(cat) =
                    <ecad_model::StyleCatalogue as ecad_model::Word>::parse(cat).and_then(|c| ecad_drawing_style::REGISTRY.catalogue(c))
            {
                let name = ecad_model::registry::styles::name(target, cat, &sid);
                c.what = format!("{} \"{name}\" · changed on both", cat.one);
            }
        }
        plan
    };
    if !plan.ops.is_empty() {
        let tried = match copy.as_mut() {
            Some(d) => d.with_applied(&plan.ops, |_| ()),
            None => p.ledger.with_applied(&plan.ops, |_| ()),
        };
        if let Err(e) = tried {
            plan.problem =
                Some(format!("These answers would leave the project inconsistent ({e}); choose differently for the objects it names, first."));
            plan.ops.clear();
        }
    }
    plan
}

/// The prepared merge for `sides`, from the project's cache when nothing it read changed.
///
/// # Errors
/// As [`prepare`].
pub(crate) fn prepared(p: &mut Project, sides: Sides) -> Result<Arc<Prepared>, String> {
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

/// One conflict's choice as a command gives it: a word, or `{"value": text}`.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum ChoiceParam {
    /// `main`, `branch`, `loose`, `bringback`, `renumber_branch`, `renumber_main` or `one`.
    Word(String),
    /// A value typed for the conflict.
    Typed(TypedChoice),
}

/// A value typed for a conflict.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TypedChoice {
    /// The value.
    pub value: String,
}

/// The choices of a command's `choices`: `main` / `branch` name the parent's and the child's side,
/// plus `loose`, `remove`, `bringback`, `renumber_branch`, `renumber_main` and
/// `{"value": text}`.
///
/// # Errors
/// A word that is not one of these, in words.
pub(crate) fn parse_choices(s: &Sides, given: &BTreeMap<String, ChoiceParam>) -> Result<BTreeMap<String, Choice>, CmdError> {
    let mut out = BTreeMap::new();
    let (parent, child) = if s.parent_is_target() { (Choice::Target, Choice::Source) } else { (Choice::Source, Choice::Target) };
    let (renumber_parent, renumber_child) =
        if s.parent_is_target() { (Choice::RenumberTarget, Choice::RenumberSource) } else { (Choice::RenumberSource, Choice::RenumberTarget) };
    for (id, c) in given {
        let choice = match c {
            ChoiceParam::Word(w) => match w.as_str() {
                "main" | "parent" => parent.clone(),
                "branch" | "child" => child.clone(),
                "target" => Choice::Target,
                "source" => Choice::Source,
                "loose" | "remove" | "drop" => Choice::Drop,
                "bringback" => Choice::BringBack,
                "renumber_branch" | "rename_branch" => renumber_child.clone(),
                "renumber_main" | "rename_main" => renumber_parent.clone(),
                "one" => Choice::MakeOne,
                other => {
                    return Err(CmdError::refused(format!(
                        "\"{other}\" is not a choice for {id}: use main, branch, loose, bringback, renumber_branch, renumber_main or one."
                    )));
                }
            },
            ChoiceParam::Typed(t) => Choice::Value(t.value.clone()),
        };
        out.insert(id.clone(), choice);
    }
    Ok(out)
}

/// A choice as the command word for these sides.
pub(crate) fn choice_word(s: &Sides, c: &Choice) -> Value {
    let pt = s.parent_is_target();
    match c {
        Choice::Target => json!(if pt { "main" } else { "branch" }),
        Choice::Source => json!(if pt { "branch" } else { "main" }),
        Choice::Value(t) => json!({ "value": t }),
        Choice::BringBack => json!("bringback"),
        Choice::Drop => json!("loose"),
        Choice::RenumberSource => json!(if pt { "renumber_branch" } else { "renumber_main" }),
        Choice::RenumberTarget => json!(if pt { "renumber_main" } else { "renumber_branch" }),
        Choice::MakeOne => json!("one"),
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

/// The preview as JSON: the header, the counts line, the conflicts with their
/// choices (`main` / `branch` words), the changes and what Merge would do.
pub(crate) fn preview_json(prep: &Prepared, plan: &Plan) -> Value {
    let s = &prep.sides;
    let pt = s.parent_is_target();
    let docs = [&*prep.source_doc, &*prep.ancestor_doc];
    let pages: Vec<String> = plan
        .pages
        .iter()
        .map(|p| ecad_model::document::book::stored_number(&prep.source_doc, *p).map_or_else(|| p.short(), str::to_string))
        .collect();
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
                "id": c.id, "kind": apply::kind_word(c.kind), "text": c.what,
                "main": main_text, "branch": branch_text,
                "choices": c.choices.iter().map(|x| choice_word(s, x)).collect::<Vec<_>>(),
                "editable": c.choices.iter().any(|x| matches!(x, Choice::Value(_))),
                "chosen": c.chosen.as_ref().map(|x| choice_word(s, x)),
                "unavailable": plan.unavailable.get(&c.id).map(|why| json!({"choice": "one", "why": why})),
                "object": state::name_of(&docs, c.object),
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
            "page": c.page.map(|p| ecad_model::document::book::stored_number(&prep.source_doc, p).map_or_else(|| p.short(), str::to_string)),
        })).collect::<Vec<_>>(),
        "conflicts": conflicts, "unanswered": plan.unanswered, "ops": plan.ops.len(),
        "nothing": plan.changes.is_empty() && plan.conflicts.is_empty(),
        "problem": plan.problem,
        "ignored": plan.ignored,
        "after": match (&plan.problem, plan.unanswered.is_empty()) {
            (_, false) => "Resolve the conflicts above first.".to_string(),
            (Some(p), true) => p.clone(),
            (None, true) if !plan.ignored.is_empty() => {
                format!("Ignored: {}. The Checks tab lists what the merge leaves to check.", plan.ignored.join("; "))
            }
            (None, true) if plan.provisional_renumbered > 0 => format!(
                "{} provisional tag{} renumbered. The Checks tab lists what the merge leaves to check.",
                plan.provisional_renumbered,
                if plan.provisional_renumbered == 1 { "" } else { "s" }
            ),
            (None, true) => "The Checks tab lists what the merge leaves to check.".to_string(),
        },
        "provisional_renumbered": plan.provisional_renumbered,
    })
}

/// The label of the merge transaction.
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

/// The commands of this module with a typed input.
pub(crate) const TYPED: &[Typed] = &[Typed::json::<MergeParams>("merge"), Typed::json::<UndomergeParams>("undomerge")];

/// The commands this module adds.
pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("merge", "Merge", run_json::<MergeParams>).noundo(),
        CommandSpec::new("undomerge", "Undo Merge", run_json::<UndomergeParams>).noundo(),
    ]
}

impl Project {
    /// Drops the merge last prepared (the merge dialog was cancelled).
    pub fn forget_merge(&mut self) {
        self.merge_cache = None;
    }
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

/// Merges a branch into its parent, or updates a branch from its parent: one step on the target. `preview` only
/// reads.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct MergeParams {
    /// The branch to merge: its ID or name.
    from: String,
    /// The branch it goes into (default its parent).
    #[serde(default)]
    into: Option<String>,
    /// Only what the merge would do; nothing is recorded.
    #[serde(default)]
    preview: Option<bool>,
    /// Per conflict ID: "main", "branch", "loose", "bringback", "renumber_branch",
    /// "renumber_main", "one", or {"value": text}.
    #[serde(default)]
    choices: Option<BTreeMap<String, ChoiceParam>>,
    /// Close the merged branch (default true).
    #[serde(default)]
    close: Option<bool>,
}

impl JsonCommand for MergeParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let given = self.choices.unwrap_or_default();
        if self.preview == Some(true) {
            return on_any_page(s, |p, _| {
                if p.storage.file.project().is_none() {
                    return Err(CmdError::refused("Branches live in the project file. Save it first."));
                }
                let sides = sides(p, &self.from, self.into.as_deref()).map_err(CmdError::refused)?;
                let choices = parse_choices(&sides, &given)?;
                let prep = prepared(p, sides).map_err(CmdError::refused)?;
                let plan = plan(p, &prep, &choices);
                Ok(preview_json(&prep, &plan))
            });
        }
        on_any_page(s, |p, pg| merge(p, pg, &self.from, self.into.as_deref(), &given, self.close))
    }
}

/// `merge`'s work (not a preview).
fn merge(
    p: &mut Project, pg: &mut Pages<'_>, from: &str, into: Option<&str>, given: &BTreeMap<String, ChoiceParam>, close: Option<bool>,
) -> Result<Value, CmdError> {
    crate::history::branches::branch_file(p).map_err(CmdError::refused)?;
    let sides = sides(p, from, into).map_err(CmdError::refused)?;
    let choices = parse_choices(&sides, given)?;
    let asked_close = close;
    let close = !sides.update && close.unwrap_or(true);
    let mut said = String::new();
    // The merge runs on the target: switch there, saving first.
    if sides.target != history::current(p) {
        let left = history::current_name(p);
        let was_unsaved = p.unsaved();
        p.save_pending(pg).map_err(CmdError::refused)?;
        history::branches::switch_to(p, pg, sides.target).map_err(CmdError::refused)?;
        if was_unsaved {
            said = format!("\"{left}\" was saved; now on {}. ", sides.target_name);
        } else {
            said = format!("Now on {}. ", sides.target_name);
        }
    }
    let prep = prepared(p, sides.clone()).map_err(CmdError::refused)?;
    let plan = plan(p, &prep, &choices);
    if !plan.unanswered.is_empty() {
        return Err(CmdError::refused(format!("{said}{}", unanswered_refusal(&prep, &plan))));
    }
    if let Some(problem) = &plan.problem {
        return Err(CmdError::refused(format!("{said}{problem}")));
    }
    // Nothing to merge only when the branch changed nothing; keeping main's side of every
    // conflict is still a merge: it is recorded, so the next merge
    // starts from it, and it closes the branch like any other.
    if plan.ops.is_empty() && plan.changes.is_empty() && plan.conflicts.is_empty() {
        let mut line = format!("{said}\"{}\" has no changes since {}.", sides.source_name, prep.ancestor_text);
        if sides.update {
            line = format!("{said}This branch already has {}'s changes.", sides.source_name);
        } else if close && asked_close == Some(true) {
            let head = p.ledger.head();
            let v = p.storage.file.project_mut().ok_or(CmdError::refused(NO_FILE))?;
            v.mark_merged(sides.source, sides.target, head).map_err(|e| CmdError::refused(e.to_string()))?;
            p.note_branch_change();
            line.push_str(&format!(" Branch \"{}\" closed.", sides.source_name));
        }
        pg.echo(line.clone());
        return Ok(json!({"merged": false, "message": line}));
    }
    let v = p.storage.file.project().ok_or(CmdError::refused(NO_FILE))?;
    let source_head = prep.key.0;
    let chain = |b, seq| v.chain_of(b, seq).map_err(CmdError::from);
    let kind = TxKind::Merge {
        source: sides.source.0,
        source_head: StepPoint { seq: source_head, chain: chain(sides.source, source_head)? },
        ancestor: BranchPoint { branch: prep.ancestor.0.0, seq: prep.ancestor.1, chain: chain(prep.ancestor.0, prep.ancestor.1)? },
    };
    let changes = plan.changes.len();
    let resolved = plan.conflicts.len();
    let tx = p.apply_merge(pg, &label(&prep), kind, plan.ops.clone()).map_err(CmdError::refused)?;
    if close && let Some(v) = p.storage.file.project_mut() {
        v.close_on_save(tx, sides.source);
    }
    p.merge_cache = None;
    p.note_branch_change();
    let resolved_text = if resolved > 0 { format!(", {resolved} conflict{} resolved", if resolved == 1 { "" } else { "s" }) } else { String::new() };
    let ignored = if plan.ignored.is_empty() { String::new() } else { format!(" Ignored: {}.", plan.ignored.join("; ")) };
    let line = if sides.update {
        format!("{said}Updated \"{}\" from {} ({}). Save to keep it.{ignored}", sides.target_name, sides.source_name, prep_latest_revision(&prep))
    } else {
        format!(
            "{said}Merged \"{}\" into {} ({changes} change{}{resolved_text}). Save to keep it.{ignored}",
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
}

/// The file went away under a merge.
const NO_FILE: &str = "no file";

/// The question `u` asks before undoing a merge, or `None` when the last step is
/// not a merge.
/// Whether `label` is a merge's or an update's (their undos and redos carry the same label).
fn merge_label(label: &str) -> bool {
    label.starts_with("Merge \"") || label.starts_with("Update from ")
}

/// Whether the step on top of the undo stack is a merge (or the redo of one), cheaply: a redo is
/// looked up in the merge index only when its label is a merge's.
fn top_is_merge(p: &Project) -> bool {
    let Some(top) = p.ledger.undo_top() else { return false };
    match top.kind {
        TxKind::Merge { .. } => true,
        TxKind::Redo { .. } if merge_label(&top.label) => {
            p.storage.file.project().and_then(|v| v.merge_index(&p.ledger).ok()).is_some_and(|m| m.iter().any(|x| x.seq == top.seq))
        }
        _ => false,
    }
}

/// The branch the merge on top of the undo stack closed (or will close when saved), if it did:
/// only that branch is reopened by undoing it.
fn closed_by_top(p: &Project) -> Option<BranchId> {
    let top = p.ledger.undo_top()?;
    let v = p.storage.file.project()?;
    let cur = history::current(p).hex();
    let source = match top.kind {
        TxKind::Merge { source, .. } => BranchId(source),
        _ => v.merge_index(&p.ledger).ok()?.into_iter().find(|x| x.seq == top.seq)?.source,
    };
    if v.closes_on_save(top.id) == Some(source) {
        return Some(source);
    }
    let rec = v.store().reader().ok()?.branch(source).ok()??;
    (rec.status == BranchStatus::Merged && rec.merged.as_ref().is_some_and(|m| m.seq == top.seq && m.into == cur)).then_some(source)
}

/// The question `u` asks before undoing a merge, or `None` when the last step is
/// not a merge.
pub(crate) fn undo_question(p: &Project) -> Option<String> {
    if !top_is_merge(p) {
        return None;
    }
    let top = p.ledger.undo_top()?;
    // Counted as the preview counts: devices, wires, cables, pages, folders and the project (a
    // symbol and its drawing are shown through what they depict).
    let n =
        ecad_model::document::diff::objects(&top.ops).keys().filter(|(k, _)| !matches!(*k, "item" | "representation" | "connection_point")).count();
    let changes = format!("{n} change{}", if n == 1 { "" } else { "s" });
    let label = top.label.clone();
    if let Some(rest) = label.strip_prefix("Update from ") {
        let from = rest.split(" (").next().unwrap_or(rest);
        return Some(format!("The last step updated this branch from {from} ({changes}).\nUndo the update?"));
    }
    let name = label.strip_prefix("Merge ").and_then(|r| r.split(" (based on").next()).unwrap_or(&label).to_string();
    let reopened = if closed_by_top(p).is_some() { format!(" {name} is reopened.") } else { String::new() };
    Some(format!("The last step merged {name} into {} ({changes}).\nUndo the merge?{reopened}", history::current_name(p)))
}

/// Undoes the last step when it is a merge, as `u` asks.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("properties" = {}))]
pub(crate) struct UndomergeParams {}

impl JsonCommand for UndomergeParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_project(s, undo_merge)
    }
}

/// `undomerge`'s work.
fn undo_merge(p: &mut Project, pg: &mut Pages<'_>, _page: ecad_model::PageId) -> Result<Value, CmdError> {
    if undo_question(p).is_none() {
        return Err(CmdError::refused("The last step is not a merge; use Undo."));
    }
    let label = p.ledger.undo_top().map(|t| t.label.clone()).unwrap_or_default();
    let reopens = closed_by_top(p).is_some();
    p.asks.confirm_merge_undo = true;
    let r = p.undo_step(pg);
    p.asks.confirm_merge_undo = false;
    r.map_err(CmdError::refused)?;
    p.merge_cache = None;
    p.note_branch_change();
    // Worded for what changed: only a merge that closed its branch
    // reopens it.
    let line = match label.strip_prefix("Merge ").and_then(|r| r.split(" (based on").next()) {
        Some(name) if reopens => format!("Merge of {name} undone; the branch is open again (saved with the next Save)."),
        Some(name) => format!("Merge of {name} undone."),
        None => format!("{label} undone."),
    };
    pg.echo(line.clone());
    Ok(json!({"undone": true, "message": line}))
}
