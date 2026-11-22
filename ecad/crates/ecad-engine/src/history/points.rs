//! Points of the history shown read-only, Restore and the details box.

use super::*;

/// A point of the history as a command names it: a version, an issued revision or a step, on
/// `branch` (default the branch shown); `branch` or `now` alone is that branch's head.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema)]
pub(crate) struct PointQuery {
    /// The branch: its ID or name (default: the branch shown).
    #[serde(default)]
    pub branch: Option<String>,
    /// A version's name.
    #[serde(default)]
    pub version: Option<String>,
    /// An issued revision, such as "B".
    #[serde(default)]
    pub revision: Option<String>,
    /// A step.
    #[serde(default)]
    pub seq: Option<u64>,
    /// The head of the branch.
    #[serde(default)]
    pub now: Option<bool>,
}

/// The point `params` names on its branch's line, read as a [`PointQuery`] (other keys are left
/// alone: the agent gate reads `restore`'s whole parameters).
///
/// # Errors
/// Parameters that do not read; a missing or unknown point, in words.
pub fn point(p: &Project, params: &Value) -> Result<Point, String> {
    let q = PointQuery::deserialize(params).map_err(|e| e.to_string())?;
    Ok(point_at(p, &q)?)
}

/// The point `q` names on its branch's line.
///
/// # Errors
/// A missing or unknown point, in words.
pub(crate) fn point_at(p: &Project, q: &PointQuery) -> Result<Point, CmdError> {
    let branch = match &q.branch {
        Some(b) => find_branch(p, b).map_err(CmdError::refused)?.0,
        None => current(p),
    };
    let head = if branch == current(p) { p.ledger.head() } else { find_branch(p, &branch.hex()).map_err(CmdError::refused)?.1.head.seq };
    if let Some(name) = &q.version {
        let n = named_points(p, branch)
            .map_err(CmdError::refused)?
            .into_iter()
            .find(|n| !n.is_revision() && n.name.eq_ignore_ascii_case(name.trim()))
            .ok_or_else(|| CmdError::refused(format!("There is no version \"{}\" on this branch's line.", name.trim())))?;
        return Ok(Point { branch: n.branch, seq: n.seq, title: n.title(), kind: NodeKind::Version });
    }
    if let Some(rev) = &q.revision {
        let want = read_rev(p, rev).map_err(|e| CmdError::refused(e.to_string()))?;
        let n = named_points(p, branch).map_err(CmdError::refused)?.into_iter().find(|n| n.point == PointName::Revision(want)).ok_or_else(|| {
            CmdError::refused(format!("Rev {} has not been issued on this branch's line.", p.ledger.document().project.rev_scheme.show(want)))
        })?;
        return Ok(Point { branch: n.branch, seq: n.seq, title: n.name, kind: NodeKind::Revision });
    }
    if let Some(seq) = q.seq {
        if seq > head {
            return Err(CmdError::refused(format!("This branch has no step {seq} (its last is {head}).")));
        }
        let kind = if seq == head { NodeKind::Now } else { NodeKind::Fork };
        return Ok(Point { branch, seq, title: format!("Step {seq}"), kind });
    }
    if q.now == Some(true) || q.branch.is_some() {
        return Ok(Point { branch, seq: head, title: "Now".into(), kind: NodeKind::Now });
    }
    Err(CmdError::refused("Name a point: {\"version\": name}, {\"revision\": \"B\"} or {\"seq\": n}."))
}

/// The document at `pt` (on a copy, design 020 §2.4): the ledger for the branch shown, else read
/// from the file.
///
/// # Errors
/// The file could not be read or replayed.
pub fn state_at(p: &Project, pt: &Point) -> Result<Document, String> {
    match reads_saved(p, pt) {
        true => p.file.project().ok_or("this branch is not in a file")?.point_state(pt.branch, pt.seq).map_err(|e| e.to_string()),
        false => p.ledger.state_at(pt.seq).map_err(|e| e.to_string()),
    }
}

/// The rev a revision point is (its named point's key), if it is one.
pub(crate) fn point_rev(p: &Project, pt: &Point) -> Option<Rev> {
    named_points(p, current_or(p, pt.branch))
        .ok()?
        .into_iter()
        .find(|n| n.branch == pt.branch && n.seq == pt.seq && n.title() == pt.title)?
        .point
        .rev()
}

/// A rev typed or sent as text (`B`, `Rev B`, `P03`): read once, here at the input edge, in the
/// project's scheme. The `Rev ` is how it is shown, not part of it.
///
/// # Errors
/// What is wrong with it, in words.
pub(crate) fn read_rev(p: &Project, text: &str) -> Result<Rev, ecad_model::RevError> {
    let t = text.trim();
    let t = t.strip_prefix("Rev ").unwrap_or(t);
    p.ledger.document().project.rev_scheme.parse(t)
}

/// The step that issued revision `rev`, whose `NameVersion` names step `at` of `branch`: the
/// issue transaction itself (`at + 1`, the step after the point it names, design 027 §0), when
/// that step does issue it.
pub(crate) fn issue_seq(p: &Project, branch: BranchId, at: u64, rev: Rev) -> Option<u64> {
    let seq = at.checked_add(1)?;
    let t = tx_at(p, branch, seq)?;
    (revisions::issue_in(&t.ops)?.0 == rev).then_some(seq)
}

/// The project as revision `rev` was issued (design 027 §5.2, §8; brief 019 follow-up): the state
/// **after** its issue transaction, so its own title block and revision list read it. Found on the
/// branch shown's line (020's `named_points`); returns the named point and that state. A hostile
/// history whose step after the point does not issue it gets the point's state with the issued
/// rows copied in (issued rows never change, so this is exact for what they say).
///
/// # Errors
/// Not issued on this line, or the history could not be read.
pub fn issued_state(p: &Project, rev: Rev) -> Result<(Named, u64, Document), String> {
    let n = named_points(p, current(p))?
        .into_iter()
        .find(|n| n.point == PointName::Revision(rev))
        .ok_or_else(|| format!("Rev {} has not been issued on this branch's line.", p.ledger.document().project.rev_scheme.show(rev)))?;
    if let Some(seq) = issue_seq(p, n.branch, n.seq, rev) {
        let pt = Point { branch: n.branch, seq, title: n.name.clone(), kind: NodeKind::Revision };
        let doc = state_at(p, &pt)?;
        return Ok((n, seq, doc));
    }
    let pt = Point { branch: n.branch, seq: n.seq, title: n.name.clone(), kind: NodeKind::Revision };
    let mut then = state_at(p, &pt)?;
    let live = p.ledger.document();
    let upto = revisions::list(live).into_iter().find(|r| r.issued && r.rev_value == Some(rev)).map_or(0, |r| r.n);
    // Design 072 §5.3: the issued rows up to it, as they are now.
    let upto = usize::try_from(upto).unwrap_or(usize::MAX);
    then.project.revisions = live.project.revisions.iter().take(upto).filter(|r| r.issued).cloned().collect();
    Ok((n.clone(), n.seq, then))
}

/// Whether `pt`'s state is read from the file (its nearest snapshot and at most the snapshot
/// interval of steps, reviewer finding 2) rather than from the ledger: every saved point, of any
/// branch. Only an unsaved point of the branch shown comes from the ledger, undoing at most the
/// unsaved steps, so the cost never grows with the session's length.
pub(crate) fn reads_saved(p: &Project, pt: &Point) -> bool {
    p.file.project().is_some_and(|v| pt.branch != v.branch() || pt.seq <= v.head().0)
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

/// Work out a restore of `pt` (design 020 §2.5.3): the ops are the difference between the live
/// state and the point's (overseer, 2026-11-10: they scale with what differs, not with how far
/// back the point is), without version names or revision fields.
///
/// # Errors
/// Not on the branch shown's line, or the history could not be read.
pub(crate) fn plan_restore(p: &Project, pt: &Point) -> Result<RestorePlan, String> {
    if !on_current_line(p, pt)? {
        return Err(format!("{} is on another branch: branch from here, or merge that branch.", pt.title));
    }
    let live = p.ledger.document();
    let then = restore_target(p, pt)?;
    let ops = ecad_model::document::diff::ops_between(live, &then, &restores_key);
    let objects = ecad_model::document::diff::objects(&ops);
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
            pages.insert(ecad_electrical::references::cables::page_number(doc, pg));
        }
    }
    let after_revisions = named_points(p, current(p))?
        .into_iter()
        .filter(|n| {
            n.is_revision()
                && (n.branch != pt.branch || n.seq >= pt.seq)
                && on_current_line(p, &Point { branch: n.branch, seq: n.seq, title: String::new(), kind: NodeKind::Revision }).unwrap_or(false)
        })
        .filter(|n| n.branch == current(p) || n.branch == pt.branch)
        .map(|n| n.name)
        .collect();
    Ok(RestorePlan { objects: objects.len(), ops, pages: pages.into_iter().collect(), after_revisions })
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
    let this = named.iter().find(|n| n.branch == pt.branch && n.seq == pt.seq && n.title() == pt.title);
    let (since, steps) = match &prev {
        Some(n) => (n.name.clone(), pt.seq - n.seq),
        None if pt.branch == main_of(p) => ("the start".to_string(), pt.seq),
        None => ("the branch's start".to_string(), pt.seq),
    };
    let mut summary = vec![format!("{steps} step{}", if steps == 1 { "" } else { "s" })];
    if let (Some(n), Some(this), true) = (&prev, this, matches!(pt.kind, NodeKind::Version | NodeKind::Revision)) {
        summary.extend(chunk_summary(p, this, n));
    }
    let restore = on_current_line(p, pt).unwrap_or(false);
    let description = this.and_then(|n| n.description.clone());
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

/// Pages changed, added and removed between two saved snapshots, from their chunk hashes
/// (design 020 §2.4): no document is built.
fn chunk_summary(p: &Project, this: &Named, prev: &Named) -> Vec<String> {
    let Some(v) = p.file.project() else { return Vec::new() };
    let Ok(r) = v.store().reader() else { return Vec::new() };
    let find = |n: &Named| r.version(n.branch, RowKind::of(&n.point), &n.point.key()).ok().flatten();
    let (Some(a), Some(b)) = (find(prev), find(this)) else { return Vec::new() };
    let pages = |v: &ecad_file::schema::VersionRecord| -> BTreeMap<String, String> {
        v.chunks.iter().filter(|(k, _)| matches!(ChunkKey::parse(k), Some(ChunkKey::Page(_)))).map(|(k, h)| (k.clone(), h.clone())).collect()
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
        .filter(|k| matches!(ChunkKey::parse(k), Some(ChunkKey::Model { .. })))
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
    let is_device = |k: &String| matches!(ChunkKey::parse(k), Some(ChunkKey::Model { kind: ModelKind::Device, .. }));
    let comp: Vec<&String> = model.iter().copied().filter(|k| is_device(k)).collect();
    let named = if comp.is_empty() || comp.len() > 8 { None } else { ecad_file::history::device_changes(&r, &a, &b, &comp) };
    match named {
        Some(lines) => out.extend(lines),
        None if !comp.is_empty() => out.push("devices changed".into()),
        None => {}
    }
    if model.iter().any(|k| !is_device(k)) {
        out.push("wiring changed".into());
    }
    out
}

/// `pointinfo {branch?, version? | revision? | seq?, now?}` (read only): the details of a point.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PointinfoParams {
    /// The branch: its ID or name (default: the branch shown); alone, its head.
    #[serde(default)]
    branch: Option<String>,
    /// A version's name.
    #[serde(default)]
    version: Option<String>,
    /// An issued revision, such as "B".
    #[serde(default)]
    revision: Option<String>,
    /// A step.
    #[serde(default)]
    seq: Option<u64>,
    /// The head of the branch.
    #[serde(default)]
    now: Option<bool>,
}

impl PointinfoParams {
    fn query(self) -> PointQuery {
        PointQuery { branch: self.branch, version: self.version, revision: self.revision, seq: self.seq, now: self.now }
    }
}

impl Command for PointinfoParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let q = self.query();
        on_any_page(s, |p, _| {
            let pt = point_at(p, &q)?;
            point_info(p, &pt).map_err(CmdError::refused)
        })
    }
}

/// The point bar's text (design 020 §2.4).
pub(crate) fn bar_text(p: &Project, pt: &Point) -> String {
    let bname = find_branch(p, &pt.branch.hex()).map(|b| b.1.name).unwrap_or_else(|_| "main".into());
    match pt.kind {
        NodeKind::Now => format!("Now · {bname} · read-only"),
        _ => format!("{} · {bname} · read-only", pt.title),
    }
}

/// `showpoint {branch?, version? | revision? | seq?, now?}`: that state, read only; records
/// nothing.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ShowpointParams {
    /// The branch: its ID or name (default: the branch shown); alone, its head.
    #[serde(default)]
    branch: Option<String>,
    /// A version's name.
    #[serde(default)]
    version: Option<String>,
    /// An issued revision, such as "B".
    #[serde(default)]
    revision: Option<String>,
    /// A step.
    #[serde(default)]
    seq: Option<u64>,
    /// The head of the branch.
    #[serde(default)]
    now: Option<bool>,
}

impl Command for ShowpointParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let q = PointQuery { branch: self.branch, version: self.version, revision: self.revision, seq: self.seq, now: self.now };
        on_any_page(s, |p, _| show_point(p, &q))
    }
}

/// `showpoint`'s work: the point's state as a read-only view.
fn show_point(p: &mut Project, q: &PointQuery) -> Result<Value, CmdError> {
    let pt = point_at(p, q)?;
    // An issued revision is shown as issued (brief 027 §16): the step after the point it
    // names, so its title block reads it.
    let shown = match pt.kind {
        NodeKind::Revision => {
            point_rev(p, &pt).and_then(|rev| issue_seq(p, pt.branch, pt.seq, rev)).map_or_else(|| pt.clone(), |seq| Point { seq, ..pt.clone() })
        }
        _ => pt.clone(),
    };
    let then = state_at(p, &shown).map_err(CmdError::refused)?;
    let book = ecad_model::document::book::Book::of(&then);
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
}

/// `closepoint {}`: back to the current state.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("properties" = {}))]
pub(crate) struct ClosepointParams {}

impl Command for ClosepointParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_any_page(s, |p, _| {
            let was = p.revision_view.take().map(|v| v.rev);
            Ok(json!({"closed": was}))
        })
    }
}

/// The confirmation text of a restore (design 020 §2.5.3).
pub(crate) fn restore_question(p: &Project, pt: &Point, plan: &RestorePlan) -> String {
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

/// `restore {version? | revision? | seq?, branch?, now?, preview?}`: make the project as it was
/// there; one step, undoable. `preview` only asks.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RestoreParams {
    /// The branch: its ID or name (default: the branch shown); alone, its head.
    #[serde(default)]
    branch: Option<String>,
    /// A version's name.
    #[serde(default)]
    version: Option<String>,
    /// An issued revision, such as "B".
    #[serde(default)]
    revision: Option<String>,
    /// A step.
    #[serde(default)]
    seq: Option<u64>,
    /// The head of the branch.
    #[serde(default)]
    now: Option<bool>,
    /// Only the question the Restore dialog asks; nothing is recorded.
    #[serde(default)]
    preview: Option<bool>,
}

impl Command for RestoreParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let preview = self.preview == Some(true);
        let q = PointQuery { branch: self.branch, version: self.version, revision: self.revision, seq: self.seq, now: self.now };
        on_project(s, |p, pg, _| restore(p, pg, &q, preview))
    }
}

/// `restore`'s work.
fn restore(p: &mut Project, pg: &mut Pages<'_>, q: &PointQuery, preview: bool) -> Result<Value, CmdError> {
    let pt = point_at(p, q)?;
    let plan = plan_restore(p, &pt).map_err(CmdError::refused)?;
    let quoted = match pt.kind {
        NodeKind::Version => pt.title.trim_start_matches("Version ").to_string(),
        _ => format!("\"{}\"", pt.title),
    };
    if plan.objects == 0 || plan.ops.is_empty() {
        return Err(CmdError::refused(format!("The project is already as it was at {quoted}.")));
    }
    if preview {
        // The Restore dialog's question; nothing is recorded.
        return Ok(json!({"question": restore_question(p, &pt, &plan), "objects": plan.objects, "pages": plan.pages}));
    }
    p.apply_edit_with(pg, "restore", &format!("Restore {quoted}"), plan.ops.clone(), false)?;
    let line = format!("Restored {quoted} ({} changes). Undo takes it back.", plan.objects);
    pg.echo(line.clone());
    Ok(json!({"restored": pt.title, "objects": plan.objects, "pages": plan.pages, "transaction": p.ledger.head(), "message": line}))
}
