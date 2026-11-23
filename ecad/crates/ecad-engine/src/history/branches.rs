//! Branch actions: create, rename, switch, archive, reopen.

use super::points::point_at;
use super::*;

/// `branches {}` (read only): the branch list.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("properties" = {}))]
pub(crate) struct BranchesParams {}

impl Command for BranchesParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_any_page(s, |p, _| {
            let cur = current(p);
            let rows = branch_rows(p).map_err(CmdError::refused)?;
            Ok(json!({"current": cur.hex(), "branches": rows.iter().map(|(id, b)| json!({
                "id": id.hex(), "name": b.name, "status": status_word(&b.status), "current": *id == cur,
                "parent": b.parent, "fork": b.fork_point.as_ref().map(|f| f.seq), "head": if *id == cur { p.ledger.head() } else { b.head.seq },
            })).collect::<Vec<_>>()}))
        })
    }
}

/// Branch actions with no version 2 file yet.
const SAVE_FIRST: &str = "Branches live in the project file. Save it first.";
/// The file went away under a branch action.
const NO_FILE: &str = "no file";

/// Why branch actions cannot run: no version 2 file yet, or read-only.
pub(crate) fn branch_file(p: &Project) -> Result<(), String> {
    match p.file.project() {
        None => Err("Branches live in the project file. Save it first.".into()),
        Some(v) => match v.read_only() {
            Some(r) => Err(format!("The project is read-only ({r}); branches cannot be changed.")),
            None => Ok(()),
        },
    }
}

/// Where a new branch starts: the point `from` names, or the head of the branch shown.
fn fork_of(p: &Project, from: Option<&PointQuery>) -> Result<Point, CmdError> {
    match from {
        None => Ok(Point { branch: current(p), seq: p.ledger.head(), title: "Now".into(), kind: NodeKind::Now }),
        Some(q) => {
            let mut pt = point_at(p, q)?;
            if q.version.is_none() && q.revision.is_none() && q.seq.is_none() {
                let head =
                    if pt.branch == current(p) { p.ledger.head() } else { find_branch(p, &pt.branch.hex()).map_err(CmdError::refused)?.1.head.seq };
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

/// Where a new branch starts: a point of the history.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FromParams {
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

/// `branch {name, from?, switch?}`: a new branch (saving first; written at once), shown unless
/// `switch` is false.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct BranchParams {
    /// The new branch's name.
    name: String,
    /// Where it starts (default: the head of the branch shown).
    #[serde(default)]
    from: Option<FromParams>,
    /// Show it (default true).
    #[serde(default)]
    switch: Option<bool>,
}

impl Command for BranchParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let switch = self.switch.unwrap_or(true);
        let from = self.from.map(|f| PointQuery { branch: f.branch, version: f.version, revision: f.revision, seq: f.seq, now: f.now });
        let name = self.name;
        on_any_page(s, |p, pg| {
            branch_file(p).map_err(CmdError::refused)?;
            let from = fork_of(p, from.as_ref())?;
            ecad_file::project::check_branch_name(&name).map_err(|e| CmdError::refused(e.to_string()))?;
            // Saved first: the fork point and the branch row are in the file.
            p.save_pending(pg).map_err(CmdError::refused)?;
            let by = Some(author_text(&p.author));
            let v = p.file.project_mut().ok_or(CmdError::refused(SAVE_FIRST))?;
            let (id, rec) =
                v.create_branch(&name, from.branch, from.seq, crate::project::now_ms(), by).map_err(|e| CmdError::refused(e.to_string()))?;
            p.note_branch_change();
            let mut line = format!("Branch \"{}\" created from {}.", rec.name, from_text(p, &from));
            if switch {
                switch_to(p, pg, id).map_err(CmdError::refused)?;
                line.push_str(&format!(" Now on branch \"{}\".", rec.name));
            }
            pg.echo(line.clone());
            Ok(
                json!({"branch": id.hex(), "name": rec.name, "from": {"branch": from.branch.hex(), "seq": from.seq}, "switched": switch, "message": line}),
            )
        })
    }
}

/// Show branch `id` (saved already), and say so.
pub(crate) fn switch_to(p: &mut Project, pg: &mut Pages<'_>, id: BranchId) -> Result<(), String> {
    let seed = crate::project::session_seed();
    let ledger = p.ledger.clone();
    let v = p.file.project_mut().ok_or("Branches live in the project file. Save it first.")?;
    let (l, report) = v.switch_to(&ledger, id, seed).map_err(|e| e.to_string())?;
    let high = v.base_tag_high().clone();
    p.adopt_branch(pg, l, high)?;
    if let Some(d) = report.damage {
        pg.echo(format!("This branch is damaged ({}); it is shown read-only.", d.detail));
    }
    Ok(())
}

/// `renamebranch {branch, name}`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RenamebranchParams {
    /// The branch: its ID or name.
    branch: String,
    /// Its new name.
    name: String,
}

impl Command for RenamebranchParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_any_page(s, |p, pg| {
            branch_file(p).map_err(CmdError::refused)?;
            let (id, old) = find_branch(p, &self.branch).map_err(CmdError::refused)?;
            let v = p.file.project_mut().ok_or(CmdError::refused(NO_FILE))?;
            let rec = v.rename_branch(id, &self.name).map_err(|e| CmdError::refused(e.to_string()))?;
            p.note_branch_change();
            let line = format!("Branch \"{}\" renamed \"{}\".", old.name, rec.name);
            pg.echo(line.clone());
            Ok(json!({"branch": id.hex(), "name": rec.name, "message": line}))
        })
    }
}

/// The refusal of a switch with unsaved changes on a file that cannot be written:
/// Save As or Cancel, never a silent loss.
pub(crate) fn read_only_refusal(p: &Project) -> String {
    let n = p.unsaved_transactions();
    format!(
        "{} has {n} unsaved change{} and the file is read-only. Use Save As to keep them; the branch was not switched.",
        current_name(p),
        if n == 1 { "" } else { "s" }
    )
}

/// The unsaved-steps refusal of `switchbranch` without `save`.
pub(crate) fn unsaved_refusal(p: &Project) -> String {
    let n = p.unsaved_transactions();
    format!("{} has {n} unsaved change{}; add \"save\": true to save them and switch.", current_name(p), if n == 1 { "" } else { "s" })
}

/// `switchbranch {branch, save?}`: show another branch; refused with unsaved changes unless
/// `save` is true.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SwitchbranchParams {
    /// The branch: its ID or name.
    branch: String,
    /// Save unsaved changes first.
    #[serde(default)]
    save: Option<bool>,
}

impl Command for SwitchbranchParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let save = self.save.unwrap_or(false);
        on_any_page(s, |p, pg| switch_branch(p, pg, &self.branch, save))
    }
}

/// `switchbranch`'s work.
fn switch_branch(p: &mut Project, pg: &mut Pages<'_>, which: &str, save: bool) -> Result<Value, CmdError> {
    if p.file.project().is_none() {
        return Err(CmdError::refused(SAVE_FIRST));
    }
    let (id, rec) = find_branch(p, which).map_err(CmdError::refused)?;
    if id == current(p) {
        return Ok(json!({"branch": id.hex(), "name": rec.name, "switched": false, "message": format!("Already on branch \"{}\".", rec.name)}));
    }
    let read_only = p.file.project().and_then(|v| v.read_only()).is_some();
    if p.unsaved() && read_only {
        // Never a silent loss: the edits can only be kept by Save As.
        return Err(CmdError::refused(read_only_refusal(p)));
    }
    if p.unsaved() {
        if !save {
            return Err(CmdError::refused(unsaved_refusal(p)));
        }
        p.save_pending(pg).map_err(CmdError::refused)?;
    }
    switch_to(p, pg, id).map_err(CmdError::refused)?;
    let from = match rec.fork_point.as_ref() {
        Some(f) => {
            let parent = rec.parent.unwrap_or_else(|| main_of(p));
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
}

/// `archivebranch {branch}`: archive a branch (the branch shown first gives way to its parent).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArchivebranchParams {
    /// The branch: its ID or name.
    branch: String,
}

impl Command for ArchivebranchParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_any_page(s, |p, pg| archive_branch(p, pg, &self.branch))
    }
}

/// `archivebranch`'s work.
fn archive_branch(p: &mut Project, pg: &mut Pages<'_>, which: &str) -> Result<Value, CmdError> {
    branch_file(p).map_err(CmdError::refused)?;
    let (id, rec) = find_branch(p, which).map_err(CmdError::refused)?;
    if id == main_of(p) {
        return Err(CmdError::refused("main is the release line and cannot be archived."));
    }
    let mut line = String::new();
    if id == current(p) {
        // Archiving the branch shown first switches to its parent, saving first; a parent that
        // is not open itself gives main instead, saying so (never a refusal after the save).
        let parent = rec.parent.unwrap_or_else(|| main_of(p));
        let rows = branch_rows(p).map_err(CmdError::refused)?;
        let parent_row = rows.iter().find(|(i, _)| *i == parent).map(|(_, b)| b.clone());
        let (to, why) = match parent_row {
            Some(b) if b.status == BranchStatus::Open => (parent, String::new()),
            Some(b) => (main_of(p), format!("Its parent \"{}\" is {}, so the project shows main. ", b.name, status_word(&b.status))),
            None => (main_of(p), String::new()),
        };
        p.save_pending(pg).map_err(CmdError::refused)?;
        switch_to(p, pg, to).map_err(CmdError::refused)?;
        line.push_str(&format!("{why}Now on branch \"{}\". ", current_name(p)));
    }
    let v = p.file.project_mut().ok_or(CmdError::refused(NO_FILE))?;
    v.set_branch_open(id, false).map_err(|e| CmdError::refused(e.to_string()))?;
    p.note_branch_change();
    line.push_str(&format!("Branch \"{}\" archived; reopen it from \"Show merged and archived\".", rec.name));
    pg.echo(line.clone());
    Ok(json!({"branch": id.hex(), "name": rec.name, "message": line}))
}

/// A name no open branch has: `base`, else `base (2)`, `base (3)`… (a reopen whose name was
/// taken).
pub(crate) fn free_branch_name(p: &Project, base: &str, except: Option<BranchId>) -> String {
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

/// `reopenbranch {branch, name?}`: open an archived or merged branch again; `name` reopens it under
/// that name, in one step, when another open branch has its name.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReopenbranchParams {
    /// The branch: its ID or name.
    branch: String,
    /// A new name for it.
    #[serde(default)]
    name: Option<String>,
}

impl Command for ReopenbranchParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_any_page(s, |p, pg| reopen_branch(p, pg, &self.branch, self.name.as_deref()))
    }
}

/// `reopenbranch`'s work.
fn reopen_branch(p: &mut Project, pg: &mut Pages<'_>, which: &str, name: Option<&str>) -> Result<Value, CmdError> {
    branch_file(p).map_err(CmdError::refused)?;
    let (id, rec) = find_branch(p, which).map_err(CmdError::refused)?;
    if rec.status == BranchStatus::Open {
        let line = format!("Branch \"{}\" is already open.", rec.name);
        return Ok(json!({"branch": id.hex(), "name": rec.name, "message": line}));
    }
    let free = free_branch_name(p, &rec.name, Some(id));
    let v = p.file.project_mut().ok_or(CmdError::refused(NO_FILE))?;
    let reopened = match name {
        Some(n) => v.reopen_as(id, n),
        None => v.set_branch_open(id, true),
    };
    let rec2 = match reopened {
        Ok(r) => r,
        Err(ecad_file::BranchError::NameTaken(other)) => {
            return Err(CmdError::refused(format!(
                "Another open branch is called \"{other}\"; reopen this one under a new name, for example {{\"branch\": \"{}\", \"name\": \"{free}\"}}.",
                id.hex()
            )));
        }
        Err(e) => return Err(CmdError::refused(e.to_string())),
    };
    p.note_branch_change();
    let line = if rec2.name == rec.name {
        format!("Branch \"{}\" is open again.", rec.name)
    } else {
        format!("Branch \"{}\" is open again as \"{}\".", rec.name, rec2.name)
    };
    pg.echo(line.clone());
    Ok(json!({"branch": id.hex(), "name": rec2.name, "message": line}))
}
