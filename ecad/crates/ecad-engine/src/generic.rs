//! The one runner of generic commands.
//!
//! A generic command is a declaration in ecad-electrical: a type that implements [`Generic`] (or a
//! params struct that implements the older [`JsonGeneric`]). Its [`Entry`] in the registry registers
//! it and lists it with its schemas, access and file parameters; [`run`] does the whole path: read
//! the parameters, read the files it declares, lock the project, give the command its [`Cx`] and its
//! declared state, commit its edit as one transaction under its id ([`Project::apply_edit`]:
//! history, undo, views), echo its message and answer with its reply, made JSON here. The files it returns are written after its edit is committed, so a refusal leaves no file behind; a
//! file that cannot be written then is said in the answer (`not_written`), and refuses the command only when
//! nothing else happened (no edit, no other file). Any refusal puts back the ids, tag numbers and designation
//! uses the command took, and once the edit is committed the answer no longer refuses. The agent gate reads the same declaration
//! ([`Typed::access`], [`Typed::files`]).
//!
//! Excludes: commands that need the session (their shims in [`crate::commands`], [`crate::commands::input`]).

use std::collections::BTreeMap;

use cadcraft_engine::cmd::Factory;
use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::commands::input::{parse, schema};
use ecad_electrical::commands::run::{
    Access, Answer, Cx, Echo, Generic, Got, Io, JsonGeneric, LastPlaced, Outcome, Pages as Runs, Read as Reads, Slot, State, Unread,
};
use ecad_electrical::symbols::proposals::Conflict;
use ecad_model::PageId;
use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;
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

/// What the runner reads of a generic command, whichever form it is written in: [`Def`] for a
/// [`Generic`], [`Json`] for a [`JsonGeneric`].
pub(crate) trait Declared {
    /// What it takes.
    type Params: DeserializeOwned + JsonSchema;
    /// What it answers, made JSON by the runner.
    type Answer: Serialize + 'static;
    const ID: &'static str;
    const TITLE: &'static str;
    const PAGES: Runs;
    const ACCESS: Access;
    const IO: Io;
    const STATE: State;
    /// Its reply's schema (none: a JSON value).
    const REPLY: Option<fn() -> Value>;
    fn check(p: &Self::Params) -> Result<(), CmdError>;
    fn run(p: Self::Params, cx: &mut Cx<'_>) -> Result<Outcome<Self::Answer>, CmdError>;
}

/// A [`Generic`] command, as the runner reads it.
pub(crate) struct Def<G>(std::marker::PhantomData<G>);

impl<G: Generic> Declared for Def<G> {
    type Params = G::Params;
    type Answer = G::Reply;
    const ID: &'static str = G::ID;
    const TITLE: &'static str = G::TITLE;
    const PAGES: Runs = G::PAGES;
    const ACCESS: Access = G::ACCESS;
    const IO: Io = G::IO;
    const STATE: State = G::STATE;
    const REPLY: Option<fn() -> Value> = Some(schema::<G::Reply>);
    fn check(p: &G::Params) -> Result<(), CmdError> {
        G::check(p, &ecad_drawing_style::REGISTRY)
    }
    fn run(p: G::Params, cx: &mut Cx<'_>) -> Result<Outcome<G::Reply>, CmdError> {
        G::run(p, cx)
    }
}

/// A [`JsonGeneric`] command, as the runner reads it.
pub(crate) struct Json<P>(std::marker::PhantomData<P>);

impl<P: JsonGeneric> Declared for Json<P> {
    type Params = P;
    type Answer = Value;
    const ID: &'static str = P::ID;
    const TITLE: &'static str = P::TITLE;
    const PAGES: Runs = P::PAGES;
    const ACCESS: Access = P::ACCESS;
    const IO: Io = P::IO;
    const STATE: State = P::STATE;
    const REPLY: Option<fn() -> Value> = None;
    fn check(p: &P) -> Result<(), CmdError> {
        p.check(&ecad_drawing_style::REGISTRY)
    }
    fn run(p: P, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        p.run(cx)
    }
}

impl Entry {
    /// Generic command `G`.
    pub(crate) const fn of<G: Generic>() -> Entry {
        Entry::declared::<Def<G>>()
    }

    /// The older generic command `P`, answering with a JSON value.
    pub(crate) const fn json<P: JsonGeneric>() -> Entry {
        Entry::declared::<Json<P>>()
    }

    const fn declared<D: Declared>() -> Entry {
        let typed = Typed { id: D::ID, run: run::<D>, schema: schema::<D::Params>, reply: D::REPLY, access: Some(D::ACCESS), files: D::IO };
        Entry { typed, spec: spec::<D>, interactive: None, undoable: false }
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
fn spec<P: Declared>() -> CommandSpec {
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
pub(crate) fn run<P: Declared>(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let p = parse::<P::Params>(params).map_err(|e| err(e.to_string()))?;
    P::check(&p).map_err(|e| err(e.to_string()))?;
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
/// its history would. The ids and the high-water marks are small and copied; the designation use
/// index grows with the project, so it journals its changes from a mark instead and undoes them
/// ([`ecad_electrical::locations::designations::Uses::rollback`]). The derived views need no copy: a
/// command reaches them only to build a view of the committed document on first show, which is what a
/// fresh build gives, and the commit's Before phase changes nothing.
struct Before {
    ids: ecad_model::IdAllocator,
    tag_high: ecad_electrical::devices::tags::TagHigh,
}

impl Before {
    /// Copy the small indexes and set the designation use index's mark.
    fn of(proj: &mut Project) -> Before {
        proj.indexes.designation_uses.mark();
        Before { ids: proj.ids.clone(), tag_high: proj.indexes.tag_high.clone() }
    }

    /// The command was refused: put every index back.
    fn restore(self, proj: &mut Project) {
        proj.ids = self.ids;
        proj.indexes.tag_high = self.tag_high;
        proj.indexes.designation_uses.rollback();
    }

    /// The command ran: keep what it changed.
    fn keep(self, proj: &mut Project) {
        proj.indexes.designation_uses.release();
    }
}

/// Run `p` on the project, commit its edit, write the files it returns to the paths `params` gives,
/// and answer with the lines to echo.
///
/// # Semantics
/// A refusal (the command's own, `no_edit` when given and the command edits, a file parameter the
/// command does not declare, a transaction the ledger refuses, or its only file that cannot be written when
/// it has no edit) commits nothing, writes no file and leaves the project's indexes as they were
/// ([`Before`]). The declared state is given as a copy, kept only once the edit is committed. Once the
/// edit is committed (or a file written) the answer cannot fail: an answer read from the committed
/// document that cannot be made says so in its fields (`committed`, `problem`), and so does a file that
/// could not be written (`committed`, `not_written`: each path with its problem, and the message).
fn commit<P: Declared>(
    proj: &mut Project, pg: &mut Pages<'_>, page: Option<PageId>, params: &Value, read: &Read, p: P::Params, no_edit: Option<&'static str>,
) -> Result<(Value, Vec<String>), CmdError> {
    let before = Before::of(proj);
    let ran = run_and_write::<P>(proj, pg, page, params, read, p, no_edit);
    let (label, Outcome { answer, echo, .. }, held, unwritten) = match ran {
        Ok(r) => {
            before.keep(proj);
            r
        }
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
    let answer = match answer {
        Answer::Now(v) => Ok(v),
        Answer::Committed(f) => f(proj.ledger.document(), &mut proj.derived),
    };
    // The edge: the reply becomes JSON here.
    let v = answer
        .and_then(|a| serde_json::to_value(a).map_err(|e| CmdError::refused(format!("the reply could not be written: {e}"))))
        .unwrap_or_else(|e| committed_with_problem(label.as_deref(), &e));
    let (v, also) = if unwritten.is_empty() { (v, None) } else { with_unwritten(v, label.as_deref(), &unwritten) };
    let mut echo = match echo {
        Echo::Nothing => Vec::new(),
        Echo::Lines(l) => l,
        Echo::Message => v.get("message").and_then(Value::as_str).map(str::to_string).into_iter().collect(),
    };
    if let Some(line) = also
        && !echo.contains(&line)
    {
        echo.push(line);
    }
    Ok((v, echo))
}

/// A file the command returned that could not be written after its edit was committed: the path and why.
type Unwritten = (String, String);

/// The answer `v` of a command whose edit is committed (or some file written) but which could not write every
/// file: it says which were not written and why (`not_written`), whether the edit is in the history
/// (`committed`), and a message saying both; with that message, for the echo.
fn with_unwritten(v: Value, label: Option<&str>, unwritten: &[Unwritten]) -> (Value, Option<String>) {
    let mut v = if v.is_object() { v } else { serde_json::json!({ "answer": v }) };
    let files = unwritten.iter().map(|(path, problem)| format!("{path} ({problem})")).collect::<Vec<_>>().join(", ");
    let message = match label {
        Some(l) => format!("{l} was done, but a file could not be written: {files}"),
        None => format!("Not every file could be written: {files}"),
    };
    if let Some(o) = v.as_object_mut() {
        o.insert("committed".into(), Value::Bool(label.is_some()));
        o.insert(
            "not_written".into(),
            Value::Array(unwritten.iter().map(|(path, problem)| serde_json::json!({ "path": path, "problem": problem })).collect()),
        );
        o.insert("message".into(), Value::String(message.clone()));
    }
    (v, Some(message))
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

/// What [`run_and_write`] gives once the edit (if any) is committed and the files written: the edit's label,
/// the command's outcome with its edit and files taken, its declared state, and the files that could not be
/// written.
type Ran<A> = (Option<String>, Outcome<A>, Held, Vec<Unwritten>);

/// Every step of [`commit`] that may refuse: run `p`, refuse an edit when `no_edit` is given, check its file
/// parameters, commit its edit; then write its files, which refuses only when nothing else happened.
fn run_and_write<P: Declared>(
    proj: &mut Project, pg: &mut Pages<'_>, page: Option<PageId>, params: &Value, read: &Read, p: P::Params, no_edit: Option<&'static str>,
) -> Result<Ran<P::Answer>, CmdError> {
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
        P::run(p, &mut cx)?
    };
    let edit = out.edit.take();
    if edit.is_some()
        && let Some(why) = no_edit
    {
        return Err(CmdError::refused(why));
    }
    // Every file parameter is checked before anything happens; the files are written only after the commit.
    let mut files = Vec::new();
    for (key, bytes) in std::mem::take(&mut out.writes) {
        let target = P::IO.writes.contains(&key).then(|| given(params, key)).flatten();
        let target = target.ok_or_else(|| CmdError::refused(format!("`{key}` is not a file this command writes")))?;
        files.push((target.to_string(), bytes));
    }
    let label = match edit {
        Some(e) => {
            proj.apply_edit(pg, P::ID, &e.label, e.ops)?;
            Some(e.label)
        }
        None => None,
    };
    let mut written = false;
    let mut unwritten = Vec::new();
    for (target, bytes) in files {
        match cadcraft_engine::atomic_write::write_atomic_making_folders(std::path::Path::new(&target), &bytes) {
            Ok(()) => written = true,
            Err(e) => unwritten.push((target, e.to_string())),
        }
    }
    // Nothing happened (no edit, no file written): the command is refused.
    if label.is_none()
        && !written
        && let Some((target, e)) = unwritten.first()
    {
        return Err(CmdError::refused(format!("Could not write {target}: {e}")));
    }
    Ok((label, out, held, unwritten))
}

#[cfg(test)]
mod tests {
    use ecad_electrical::commands::run::Io;

    use super::*;

    /// A command that edits the project and writes a file.
    #[derive(serde::Deserialize, schemars::JsonSchema)]
    struct EditAndWrite {}

    impl JsonGeneric for EditAndWrite {
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

    /// A folder for one test's files, with a path whose folder is a file (so it cannot be written); removed when
    /// dropped.
    struct Files(std::path::PathBuf);

    impl Files {
        fn new(name: &str) -> Files {
            let dir = std::env::temp_dir().join(format!("ecad-generic-{name}-{}", ecad_store::disk::unique()));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("a-file"), b"").unwrap();
            Files(dir)
        }
        fn path(&self, name: &str) -> std::path::PathBuf {
            self.0.join(name)
        }
        /// A path that cannot be written: its folder is a file.
        fn blocked(&self) -> std::path::PathBuf {
            self.0.join("a-file").join("out.csv")
        }
    }

    impl Drop for Files {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The files are written after the commit: a file that cannot be written then is said in the answer
    /// (`committed`, `not_written`), and the edit stays in the history.
    #[test]
    fn a_file_that_cannot_be_written_after_the_commit_is_said_in_the_answer() {
        crate::install();
        let mut s = Session::empty();
        let handle = Project::create(&mut s, "P", 1).unwrap();
        let head = handle.read(&mut s, |p, _| p.ledger.head()).unwrap();
        let files = Files::new("editandwrite");
        let target = files.blocked();
        let v = run::<Json<EditAndWrite>>(&mut s, &serde_json::json!({"path": target.to_string_lossy()})).unwrap();
        assert_eq!(v["committed"], true, "{v}");
        assert_eq!(v["not_written"][0]["path"], target.to_string_lossy().as_ref(), "{v}");
        assert!(v["message"].as_str().is_some_and(|m| m.contains("was done, but a file could not be written")), "{v}");
        assert_eq!(handle.read(&mut s, |p, _| p.ledger.head()).unwrap(), head + 1);
    }

    /// A command that writes two files (`first`, `second`) and edits the project unless `edit` is false.
    #[derive(serde::Deserialize, schemars::JsonSchema)]
    struct TwoFiles {
        edit: bool,
    }

    impl JsonGeneric for TwoFiles {
        const ID: &'static str = "twofiles";
        const TITLE: &'static str = "Two files";
        const PAGES: Runs = Runs::Any;
        const IO: Io = Io { reads: &[], writes: &["first", "second"] };

        fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
            let mut out = if self.edit {
                let before = cx.doc.project().clone();
                let mut after = before.clone();
                after.info.title = Some(ecad_model::document::project::InfoText::new("Edited").map_err(|e| CmdError::refused(e.to_string()))?);
                Outcome::edit("Two files", vec![ecad_model::document::op::Op::ModifyProject { before, after }], serde_json::json!({}))
            } else {
                Outcome::answer(serde_json::json!({}))
            };
            out.writes.push(("first", b"1".to_vec()));
            out.writes.push(("second", b"2".to_vec()));
            Ok(out)
        }
    }

    /// The second of two files fails after the commit: the answer says the edit is in the history, the first
    /// file was written, and the second was not, with why.
    #[test]
    fn a_second_file_that_cannot_be_written_is_reported_truthfully() {
        let (mut s, h, _) = fresh();
        let head = h.read(&mut s, |p, _| p.ledger.head()).unwrap();
        let files = Files::new("twofiles");
        let (first, second) = (files.path("first.csv"), files.blocked());
        let params = serde_json::json!({"edit": true, "first": first.to_string_lossy(), "second": second.to_string_lossy()});
        let v = run::<Json<TwoFiles>>(&mut s, &params).unwrap();
        assert_eq!(v["committed"], true, "{v}");
        let not_written = v["not_written"].as_array().unwrap();
        assert_eq!(not_written.len(), 1, "{v}");
        assert_eq!(not_written[0]["path"], second.to_string_lossy().as_ref(), "{v}");
        assert!(!not_written[0]["problem"].as_str().unwrap_or_default().is_empty(), "{v}");
        assert_eq!(std::fs::read(&first).unwrap(), b"1", "the first file is written");
        assert_eq!(h.read(&mut s, |p, _| p.ledger.head()).unwrap(), head + 1, "the edit is in the history");

        // Without an edit, the same: the first file was written, so the answer says what was not.
        std::fs::remove_file(&first).unwrap();
        let params = serde_json::json!({"edit": false, "first": first.to_string_lossy(), "second": second.to_string_lossy()});
        let v = run::<Json<TwoFiles>>(&mut s, &params).unwrap();
        assert_eq!(
            (v["committed"].clone(), v["not_written"][0]["path"].clone()),
            (Value::Bool(false), Value::from(second.to_string_lossy().as_ref()))
        );
        assert!(first.exists());
    }

    /// A command with no edit whose only file cannot be written is refused: nothing happened.
    #[test]
    fn a_command_whose_files_all_fail_and_that_has_no_edit_is_refused() {
        let (mut s, h, state) = fresh();
        let files = Files::new("nofile");
        let params = serde_json::json!({"edit": false, "first": files.blocked().to_string_lossy(), "second": files.blocked().to_string_lossy()});
        let r = run::<Json<TwoFiles>>(&mut s, &params);
        assert!(r.is_err_and(|e| e.to_string().contains("Could not write")));
        assert_eq!(session_state(&h, &mut s), state);
    }

    /// A command that takes an id, tag numbers and the designation uses, builds the report models,
    /// then edits the project: with a file it writes (`write`), with a stale `before` the ledger refuses and a
    /// file (`stale`), or with an answer that cannot be read (`answer`).
    #[derive(serde::Deserialize, schemars::JsonSchema)]
    struct TakeThenEdit {
        how: String,
    }

    impl JsonGeneric for TakeThenEdit {
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
                "stale" => {
                    let mut out = Outcome::edit("Take then edit", op(after.clone()), Value::Null);
                    out.writes.push(("path", b"x".to_vec()));
                    out
                }
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
        let r = run::<Json<TakeThenEdit>>(&mut s, &serde_json::json!({"how": "write"}));
        assert!(r.is_err_and(|e| e.to_string().contains(UNAVAILABLE)));
        assert_eq!(session_state(&h, &mut s), state);
        assert_eq!(views_differ(&h, &mut s), Vec::<String>::new());
    }

    /// A refused command that brought the designation uses up to date (a device moved from one location to
    /// another: a use taken and a use added) leaves them exactly as they were, and the next command reads the
    /// same uses as a fresh build.
    #[test]
    fn a_refused_command_leaves_the_designation_uses_as_they_were() {
        use ecad_electrical::locations::designations::{Aspect, Uses};
        let (mut s, h, _) = fresh();
        let placed = s.execute("place", &serde_json::json!({"symbol": "coil", "at": [40.0, 40.0]})).unwrap();
        let device = placed["component"].as_str().unwrap().to_string();
        for code in ["+CP1", "+CP2"] {
            s.execute("setlocation", &serde_json::json!({"component": device, "aspect": "location", "value": code})).unwrap();
        }
        let state = session_state(&h, &mut s);
        let r = run::<Json<TakeThenEdit>>(&mut s, &serde_json::json!({"how": "stale"}));
        assert!(r.is_err());
        assert_eq!(session_state(&h, &mut s), state);
        let (kept, fresh_build) = h
            .configure(&mut s, |p| {
                p.indexes.designation_uses.catch_up(&p.ledger);
                let keys =
                    |u: &Uses| u.keys(Aspect::Location).map(|k| (k.to_string(), format!("{:?}", u.users(Aspect::Location, k)))).collect::<Vec<_>>();
                (keys(&p.indexes.designation_uses), keys(&Uses::of(p.ledger.document())))
            })
            .unwrap();
        assert_eq!(kept.len(), 1, "{kept:?}");
        assert_eq!(kept, fresh_build);
    }

    /// A transaction the ledger refuses gives back what the command took, and writes no file.
    #[test]
    fn a_refused_transaction_keeps_the_session_as_replay_and_writes_no_file() {
        let (mut s, h, state) = fresh();
        let files = Files::new("stale");
        let target = files.path("out.csv");
        let r = run::<Json<TakeThenEdit>>(&mut s, &serde_json::json!({"how": "stale", "path": target.to_string_lossy()}));
        assert!(r.is_err_and(|e| e.to_string().contains("differ from the recorded value")));
        assert!(!target.exists(), "a refusal leaves no file behind");
        assert_eq!(session_state(&h, &mut s), state);
        assert_eq!(views_differ(&h, &mut s), Vec::<String>::new());
    }

    /// An answer that cannot be read after the commit answers "committed" with the problem: the
    /// reply never says refused while the history has the edit.
    #[test]
    fn an_answer_that_cannot_be_read_still_answers_committed() {
        let (mut s, h, _) = fresh();
        let head = h.read(&mut s, |p, _| p.ledger.head()).unwrap();
        let v = run::<Json<TakeThenEdit>>(&mut s, &serde_json::json!({"how": "answer"})).unwrap();
        assert_eq!(v["committed"], true);
        assert_eq!(v["problem"], "the answer cannot be read");
        assert_ne!(h.read(&mut s, |p, _| p.ledger.head()).unwrap(), head);
    }
}
