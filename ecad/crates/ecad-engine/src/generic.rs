//! The one runner of generic commands.
//!
//! A generic command is a declaration in ecad-electrical: a params struct that implements
//! [`Generic`]. Its [`Entry`] in the registry registers it and lists it with its schema, access
//! and file parameters; [`run`] does the whole path: read the parameters, read the files it declares, lock
//! the project, give the command its [`Cx`] and its declared state, commit its edit as one
//! transaction under its id ([`Project::apply_edit`]: history, undo, views), echo its message and
//! answer. The files it returns are written before its edit is committed, so a file that cannot be
//! written leaves the project as it was. Any refusal puts back the ids, tag numbers and designation
//! uses the command took, and once the edit is committed the answer no longer refuses. The agent gate reads the same declaration
//! ([`Typed::access`], [`Typed::files`]).
//!
//! Excludes: commands that need the session (their shims in [`crate::commands`], [`crate::commands::input`]).

use std::collections::BTreeMap;

use cadcraft_engine::cmd::Factory;
use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::commands::input::{parse, schema};
use ecad_electrical::commands::run::{Answer, Cx, Echo, Generic, Got, LastPlaced, Outcome, Pages as Runs, Read as Reads, Slot, State, Unread};
use ecad_electrical::symbols::proposals::Conflict;
use ecad_model::PageId;
use serde_json::Value;

use crate::access::Pages;
use crate::commands::input::Typed;
use crate::commands::{UNAVAILABLE, err, on_any_page, on_project};
use crate::project::Project;

/// A generic command as the registry lists it: its schema and declaration, and how it registers.
pub(crate) struct Entry {
    /// Its schema and what the agent gate reads.
    pub(crate) typed: Typed,
    spec: fn() -> CommandSpec,
    interactive: Option<Factory>,
    undoable: bool,
}

impl Entry {
    /// Generic command `P`.
    pub(crate) const fn of<P: Generic>() -> Entry {
        let typed = Typed { id: P::ID, run: run::<P>, schema: schema::<P>, access: Some(P::ACCESS), files: P::IO };
        Entry { typed, spec: spec::<P>, interactive: None, undoable: false }
    }

    /// With the click-based form `f`, started by a bare command name, a menu or a tool button.
    pub(crate) const fn interactive(self, f: Factory) -> Entry {
        Entry { interactive: Some(f), ..self }
    }

    /// Also grouped in the drawing's undo (`linkdevices`, `unlinkdevices`, `userfield`).
    pub(crate) const fn undoable(self) -> Entry {
        Entry { undoable: true, ..self }
    }

    /// Its registration.
    pub(crate) fn spec(&self) -> CommandSpec {
        let c = (self.spec)();
        let c = match self.interactive {
            Some(f) => c.interactive(f),
            None => c,
        };
        CommandSpec { undoable: c.undoable || self.undoable, ..c }
    }
}

/// The registration of generic command `P` (its parameters text is set by [`crate::commands::specs`]).
fn spec<P: Generic>() -> CommandSpec {
    CommandSpec::new(P::ID, P::TITLE, run::<P>).noundo()
}

/// What each read parameter of `P` that `params` gives read.
type Read = BTreeMap<&'static str, Result<Got, Unread>>;

/// The state a command declares, as given to it: kept once its edit is committed.
enum Held {
    None,
    LastCaption(Option<String>),
    Conflicts(Option<Vec<Conflict>>),
    LastPlaced(Option<LastPlaced>),
}

/// Run generic command `P` with `params`.
///
/// # Errors
/// Parameters that do not read, a project that cannot be taken, the command's refusal, or a file
/// it returned that could not be written.
pub(crate) fn run<P: Generic>(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let p = parse::<P>(params).map_err(|e| err(e.to_string()))?;
    p.check(&ecad_drawing_style::REGISTRY).map_err(|e| err(e.to_string()))?;
    let read: Read = P::IO.reads.iter().filter_map(|(k, what)| Some((*k, read_file(given(params, k)?, *what)))).collect();
    let (v, lines) = match P::PAGES {
        Runs::Available => on_project(s, |proj, pg, page| commit::<P>(proj, pg, Some(page), params, &read, p, None))?,
        Runs::Any => on_any_page(s, |proj, pg| commit::<P>(proj, pg, None, params, &read, p, None))?,
        Runs::ReadAnywhere => {
            let uid = s.state()?.uid;
            on_any_page(s, |proj, pg| {
                let page = proj.page_of(uid);
                let no_edit = match page {
                    None => Some("page view not found"),
                    Some(page) => proj.pages.get(&page).is_some_and(|v| v.unavailable).then_some(UNAVAILABLE),
                };
                commit::<P>(proj, pg, page, params, &read, p, no_edit)
            })?
        }
    };
    for line in lines {
        s.echo(line);
    }
    Ok(v)
}

/// The text parameter `key` gives, trimmed (also when blank).
fn given<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::as_str).map(str::trim)
}

/// What the file at `path` gives as `what` declares, or why it could not be read.
fn read_file(path: &str, what: Reads) -> Result<Got, Unread> {
    use std::io::Read as _;
    match what {
        Reads::Bytes(max) => {
            let mut buf = Vec::new();
            let f = std::fs::File::open(path).map_err(|e| Unread(e.to_string()))?;
            f.take(u64::try_from(max).unwrap_or(u64::MAX).saturating_add(1)).read_to_end(&mut buf).map_err(|e| Unread(e.to_string()))?;
            Ok(Got::Bytes(buf))
        }
        Reads::Standard => crate::file::read_standard(path).map(|s| Got::Standard(Box::new(s))).map_err(|e| Unread(e.to_string())),
    }
}

/// The indexes a command is given to change while it runs (new ids, the tag high-water marks with
/// the provisional numbers given out, the designation use index), as they were before it ran.
/// Put back when the command is refused, so a refused command leaves the session as replaying
/// its history would. The derived views need no copy: a command reaches them only to build a
/// view of the committed document on first show, which is what a fresh build gives, and the
/// commit's Before phase changes nothing.
struct Before {
    ids: ecad_model::IdAllocator,
    tag_high: ecad_electrical::devices::tags::TagHigh,
    designation_uses: ecad_electrical::locations::designations::Uses,
}

impl Before {
    fn of(proj: &Project) -> Before {
        Before { ids: proj.ids.clone(), tag_high: proj.indexes.tag_high.clone(), designation_uses: proj.indexes.designation_uses.clone() }
    }

    fn restore(self, proj: &mut Project) {
        proj.ids = self.ids;
        proj.indexes.tag_high = self.tag_high;
        proj.indexes.designation_uses = self.designation_uses;
    }
}

/// Run `p` on the project, write the files it returns to the paths `params` gives, commit its edit,
/// and answer with the lines to echo.
///
/// # Semantics
/// A refusal (the command's own, `no_edit` when given and the command edits, a file that cannot be
/// written, or a transaction the ledger refuses) commits nothing and leaves the project's indexes
/// as they were ([`Before`]). The declared state is given as a copy, kept only once the edit is
/// committed. Once the edit is committed the answer cannot fail: an answer read from the committed
/// document that cannot be made says so in its fields (`committed`, `problem`) instead.
fn commit<P: Generic>(
    proj: &mut Project, pg: &mut Pages<'_>, page: Option<PageId>, params: &Value, read: &Read, p: P, no_edit: Option<&'static str>,
) -> Result<(Value, Vec<String>), CmdError> {
    let before = Before::of(proj);
    let ran = run_and_write::<P>(proj, pg, page, params, read, p, no_edit);
    let (label, Outcome { answer, echo, .. }, held) = match ran {
        Ok(r) => r,
        Err(e) => {
            before.restore(proj);
            return Err(e);
        }
    };
    match held {
        Held::None => {}
        Held::LastCaption(c) => proj.defaults.last_caption = c,
        Held::Conflicts(None) => {}
        Held::Conflicts(Some(c)) => proj.conflicts = c,
        Held::LastPlaced(l) => proj.defaults.last_placed = l,
    }
    let v = match answer {
        Answer::Now(v) => v,
        Answer::Committed(f) => f(proj.ledger.document(), &mut proj.derived).unwrap_or_else(|e| committed_with_problem(label.as_deref(), &e)),
    };
    let echo = match echo {
        Echo::Nothing => Vec::new(),
        Echo::Lines(l) => l,
        Echo::Message => v.get("message").and_then(Value::as_str).map(str::to_string).into_iter().collect(),
    };
    Ok((v, echo))
}

/// The answer of a command whose edit is committed but whose answer, read from the committed
/// document, could not be made: it says the edit is in the history and why there is no more.
fn committed_with_problem(label: Option<&str>, e: &CmdError) -> Value {
    let what = label.unwrap_or("The edit");
    serde_json::json!({
        "committed": true,
        "problem": e.to_string(),
        "message": format!("{what} was done, but its answer could not be read: {e}"),
    })
}

/// What [`run_and_write`] gives once the edit (if any) is committed: the edit's label, the
/// command's outcome with its edit and files taken, and its declared state.
type Ran = (Option<String>, Outcome, Held);

/// Every step of [`commit`] that may refuse: run `p`, refuse an edit when `no_edit` is given, write
/// its files, commit its edit.
fn run_and_write<P: Generic>(
    proj: &mut Project, pg: &mut Pages<'_>, page: Option<PageId>, params: &Value, read: &Read, p: P, no_edit: Option<&'static str>,
) -> Result<Ran, CmdError> {
    let mut held = match P::STATE {
        State::None => Held::None,
        State::LastCaption => Held::LastCaption(proj.defaults.last_caption.clone()),
        State::Conflicts => Held::Conflicts(None),
        State::LastPlaced => Held::LastPlaced(proj.defaults.last_placed),
    };
    let change_count = proj.change_count();
    let plain = crate::commands::plain_style(proj);
    let off_release_line = proj.storage.file.project().filter(|v| !v.branch_record().release_line).map(|v| v.branch_record().name.clone());
    let mut out = {
        let state = match &mut held {
            Held::None => Slot::None,
            Held::LastCaption(c) => Slot::LastCaption(c),
            Held::Conflicts(set) => Slot::Conflicts { waiting: &proj.conflicts, set },
            Held::LastPlaced(l) => Slot::LastPlaced(l),
        };
        let mut cx = Cx {
            doc: proj.ledger.document(),
            ledger: &proj.ledger,
            ids: &mut proj.ids,
            page,
            plain,
            tolerance: proj.tolerance,
            change_count,
            tag_high: &mut proj.indexes.tag_high,
            designation_uses: &mut proj.indexes.designation_uses,
            derived: &mut proj.derived,
            off_release_line,
            metrics: &crate::styles::draw::EngineMetrics,
            look: &crate::styles::draw::LOOK,
            registry: &ecad_drawing_style::REGISTRY,
            read,
            state,
        };
        p.run(&mut cx)?
    };
    let edit = out.edit.take();
    if edit.is_some()
        && let Some(why) = no_edit
    {
        return Err(CmdError::refused(why));
    }
    for (key, bytes) in std::mem::take(&mut out.writes) {
        let target = P::IO.writes.contains(&key).then(|| given(params, key)).flatten();
        let target = target.ok_or_else(|| CmdError::refused(format!("`{key}` is not a file this command writes")))?;
        cadcraft_engine::atomic_write::write_atomic_making_folders(std::path::Path::new(target), &bytes)
            .map_err(|e| CmdError::refused(format!("Could not write {target}: {e}")))?;
    }
    let label = match edit {
        Some(e) => {
            proj.apply_edit(pg, P::ID, &e.label, e.ops)?;
            Some(e.label)
        }
        None => None,
    };
    Ok((label, out, held))
}

#[cfg(test)]
mod tests {
    use ecad_electrical::commands::run::Io;

    use super::*;

    /// A command that edits the project and writes a file.
    #[derive(serde::Deserialize, schemars::JsonSchema)]
    struct EditAndWrite {}

    impl Generic for EditAndWrite {
        const ID: &'static str = "editandwrite";
        const TITLE: &'static str = "Edit and write";
        const PAGES: Runs = Runs::Any;
        const IO: Io = Io { reads: &[], writes: &["path"] };

        fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
            let before = cx.doc.project().clone();
            let mut after = before.clone();
            after.info.title = Some(ecad_model::document::project::InfoText::new("Edited").map_err(|e| CmdError::refused(e.to_string()))?);
            let mut out = Outcome::edit("Edit and write", vec![ecad_model::document::op::Op::ModifyProject { before, after }], Value::Null);
            out.writes.push(("path", b"x".to_vec()));
            Ok(out)
        }
    }

    /// A file that cannot be written refuses the command, and its edit is not committed.
    #[test]
    fn a_file_that_cannot_be_written_commits_nothing() {
        crate::install();
        let mut s = Session::empty();
        let handle = Project::create(&mut s, "P", 1).unwrap();
        let head = handle.read(&mut s, |p, _| p.ledger.head()).unwrap();
        // Its folder is a file, so the folder cannot be made and the file cannot be written.
        let dir = std::env::temp_dir().join(format!("ecad-editandwrite-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let blocker = dir.join("a-file");
        std::fs::write(&blocker, b"").unwrap();
        let target = blocker.join("out.csv");
        let r = run::<EditAndWrite>(&mut s, &serde_json::json!({"path": target.to_string_lossy()}));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(r.is_err_and(|e| e.to_string().contains("Could not write")));
        assert_eq!(handle.read(&mut s, |p, _| p.ledger.head()).unwrap(), head);
    }

    /// A command that takes an id, tag numbers and the designation uses, builds the report models,
    /// then edits the project: through a file it writes (`write`), with a stale `before` the
    /// ledger refuses (`stale`), or with an answer that cannot be read (`answer`).
    #[derive(serde::Deserialize, schemars::JsonSchema)]
    struct TakeThenEdit {
        how: String,
    }

    impl Generic for TakeThenEdit {
        const ID: &'static str = "takethenedit";
        const TITLE: &'static str = "Take then edit";
        const PAGES: Runs = Runs::ReadAnywhere;
        const IO: Io = Io { reads: &[], writes: &["path"] };

        fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
            let _: ecad_model::DeviceId = cx.ids.fresh();
            cx.tag_high.next(cx.ledger, "K")?;
            let _ = cx.tag_high.next_provisional_in(cx.doc, "K");
            let _ = cx.uses();
            let _ = cx.derived.reports(cx.doc);
            let before = cx.doc.project().clone();
            let mut after = before.clone();
            after.info.title = Some(ecad_model::document::project::InfoText::new("Edited").map_err(|e| CmdError::refused(e.to_string()))?);
            let op = |before| vec![ecad_model::document::op::Op::ModifyProject { before, after: after.clone() }];
            Ok(match self.how.as_str() {
                "stale" => Outcome::edit("Take then edit", op(after.clone()), Value::Null),
                "answer" => Outcome::edit_then_try("Take then edit", op(before), |_| Err(CmdError::refused("the answer cannot be read"))),
                _ => {
                    let mut out = Outcome::edit("Take then edit", op(before), Value::Null);
                    out.writes.push(("path", b"x".to_vec()));
                    out
                }
            })
        }
    }

    /// What replay rebuilds of a session, as the session holds it: the ids, tag numbers and
    /// designation uses commands take, and the history's head.
    fn session_state(h: &crate::ProjectHandle, s: &mut Session) -> String {
        h.read(s, |p, _| format!("{:?} {:?} {:?} {:?}", p.ids, p.indexes.tag_high, p.indexes.designation_uses, p.ledger.head())).unwrap()
    }

    /// Whether every kept derived view equals a fresh build from the committed document (empty:
    /// they all do).
    fn views_differ(h: &crate::ProjectHandle, s: &mut Session) -> Vec<String> {
        h.configure(s, |p| {
            let doc = p.ledger.document().clone();
            p.derived.verify(&doc)
        })
        .unwrap()
    }

    /// A project with a page shown, and the state replay gives it before any command.
    fn fresh() -> (Session, crate::ProjectHandle, String) {
        crate::install();
        let mut s = Session::empty();
        let h = Project::create(&mut s, "P", 1).unwrap();
        let state = session_state(&h, &mut s);
        (s, h, state)
    }

    /// An edit refused because the page is unavailable gives back what the command took.
    #[test]
    fn a_refused_edit_on_an_unavailable_page_keeps_the_session_as_replay() {
        let (mut s, h, state) = fresh();
        h.configure(&mut s, |p| {
            let page = p.active_page().unwrap();
            p.pages.get_mut(&page).unwrap().unavailable = true;
        })
        .unwrap();
        let r = run::<TakeThenEdit>(&mut s, &serde_json::json!({"how": "write"}));
        assert!(r.is_err_and(|e| e.to_string().contains(UNAVAILABLE)));
        assert_eq!(session_state(&h, &mut s), state);
        assert_eq!(views_differ(&h, &mut s), Vec::<String>::new());
    }

    /// A file that cannot be written gives back what the command took.
    #[test]
    fn a_failed_write_keeps_the_session_as_replay() {
        let (mut s, h, state) = fresh();
        let dir = std::env::temp_dir().join(format!("ecad-takethenedit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let blocker = dir.join("a-file");
        std::fs::write(&blocker, b"").unwrap();
        let target = blocker.join("out.csv");
        let r = run::<TakeThenEdit>(&mut s, &serde_json::json!({"how": "write", "path": target.to_string_lossy()}));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(r.is_err_and(|e| e.to_string().contains("Could not write")));
        assert_eq!(session_state(&h, &mut s), state);
        assert_eq!(views_differ(&h, &mut s), Vec::<String>::new());
    }

    /// A transaction the ledger refuses gives back what the command took.
    #[test]
    fn a_refused_transaction_keeps_the_session_as_replay() {
        let (mut s, h, state) = fresh();
        let r = run::<TakeThenEdit>(&mut s, &serde_json::json!({"how": "stale"}));
        assert!(r.is_err_and(|e| e.to_string().contains("differ from the recorded value")));
        assert_eq!(session_state(&h, &mut s), state);
        assert_eq!(views_differ(&h, &mut s), Vec::<String>::new());
    }

    /// An answer that cannot be read after the commit answers "committed" with the problem: the
    /// reply never says refused while the history has the edit.
    #[test]
    fn an_answer_that_cannot_be_read_still_answers_committed() {
        let (mut s, h, _) = fresh();
        let head = h.read(&mut s, |p, _| p.ledger.head()).unwrap();
        let v = run::<TakeThenEdit>(&mut s, &serde_json::json!({"how": "answer"})).unwrap();
        assert_eq!(v["committed"], true);
        assert_eq!(v["problem"], "the answer cannot be read");
        assert_ne!(h.read(&mut s, |p, _| p.ledger.head()).unwrap(), head);
    }
}
