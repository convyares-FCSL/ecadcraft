//! The one runner of generic commands.
//!
//! A generic command is a declaration in ecad-electrical: a params struct that implements
//! [`Generic`]. Its [`Entry`] in the registry registers it and lists it with its schema, access
//! and file parameters; [`run`] does the whole path: read the parameters, read the files it declares, lock
//! the project, give the command its [`Cx`] and its declared state, commit its edit as one
//! transaction under its id ([`Project::apply_edit`]: history, undo, views), echo its message and
//! answer. The files it returns are written before its edit is committed, so a file that cannot be
//! written leaves the project as it was. The agent gate reads the same declaration
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

/// The registration of generic command `P`.
fn spec<P: Generic>() -> CommandSpec {
    CommandSpec::new(P::ID, P::TITLE, run::<P>).params(P::HELP).noundo()
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

/// Run `p` on the project, write the files it returns to the paths `params` gives, commit its edit,
/// and answer with the lines to echo. A file that cannot be written refuses the whole command:
/// nothing is committed. The declared state is given as a copy, kept only once the edit is
/// committed. An edit is refused with `no_edit` when given (the page is not one an edit runs on).
fn commit<P: Generic>(
    proj: &mut Project, pg: &mut Pages<'_>, page: Option<PageId>, params: &Value, read: &Read, p: P, no_edit: Option<&'static str>,
) -> Result<(Value, Vec<String>), CmdError> {
    let mut held = match P::STATE {
        State::None => Held::None,
        State::LastCaption => Held::LastCaption(proj.last_caption.clone()),
        State::Conflicts => Held::Conflicts(None),
        State::LastPlaced => Held::LastPlaced(proj.last_placed),
    };
    let change_count = proj.change_count();
    let plain = crate::commands::plain_style(proj);
    let off_release_line = proj.file.project().filter(|v| !v.branch_record().release_line).map(|v| v.branch_record().name.clone());
    let Outcome { edit, answer, writes, echo } = {
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
            tag_high: &mut proj.tag_high,
            designation_uses: &mut proj.designation_uses,
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
    if edit.is_some()
        && let Some(why) = no_edit
    {
        return Err(CmdError::refused(why));
    }
    for (key, bytes) in writes {
        let target = P::IO.writes.contains(&key).then(|| given(params, key)).flatten();
        let target = target.ok_or_else(|| CmdError::refused(format!("`{key}` is not a file this command writes")))?;
        cadcraft_engine::atomic_write::write_atomic_making_folders(std::path::Path::new(target), &bytes)
            .map_err(|e| CmdError::refused(format!("Could not write {target}: {e}")))?;
    }
    if let Some(e) = edit {
        proj.apply_edit(pg, P::ID, &e.label, e.ops)?;
    }
    match held {
        Held::None => {}
        Held::LastCaption(c) => proj.last_caption = c,
        Held::Conflicts(None) => {}
        Held::Conflicts(Some(c)) => proj.conflicts = c,
        Held::LastPlaced(l) => proj.last_placed = l,
    }
    let v = match answer {
        Answer::Now(v) => v,
        Answer::Committed(f) => f(proj.ledger.document(), &mut proj.derived)?,
    };
    let echo = match echo {
        Echo::Nothing => Vec::new(),
        Echo::Lines(l) => l,
        Echo::Message => v.get("message").and_then(Value::as_str).map(str::to_string).into_iter().collect(),
    };
    Ok((v, echo))
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
        const HELP: &'static str = "{path}";
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
}
