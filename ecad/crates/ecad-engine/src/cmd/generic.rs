//! The one runner of generic commands (owner decision 5, design `commands.md`).
//!
//! A generic command is a declaration in ecad-electrical: a params struct that implements
//! [`Generic`]. [`spec`] registers it and [`typed`] lists it with its schema, access and file
//! parameters; [`run`] does the whole path: read the parameters, read the files it declares, lock
//! the project, give the command its [`Cx`] and its declared state, commit its edit as one
//! transaction under its id ([`Project::apply_edit`]: history, undo, views), write the files it
//! returns, echo its message and answer. The agent gate reads the same declaration
//! ([`Typed::access`], [`Typed::files`]).
//!
//! Excludes: commands that need the session (their shims in [`crate::cmd`], [`super::input`]).

use std::collections::BTreeMap;

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::cmd::input::{parse, schema};
use ecad_electrical::cmd::run::{Answer, Cx, Echo, Generic, Got, Outcome, Pages as Runs, Read as Reads, Slot, State, Unread};
use ecad_electrical::proposals::Conflict;
use ecad_model::PageId;
use serde_json::Value;

use crate::access::Pages;
use crate::cmd::input::Typed;
use crate::cmd::{UNAVAILABLE, err, on_any_page, on_project};
use crate::project::Project;

/// The registration of generic command `P`.
pub(crate) fn spec<P: Generic>() -> CommandSpec {
    CommandSpec::new(P::ID, P::TITLE, run::<P>).params(P::HELP).noundo()
}

/// Generic command `P` with its schema and what the agent gate reads.
pub(crate) const fn typed<P: Generic>() -> Typed {
    Typed { id: P::ID, run: run::<P>, schema: schema::<P>, access: Some(P::ACCESS), files: P::IO }
}

/// What each read parameter of `P` that `params` gives read.
type Read = BTreeMap<&'static str, Result<Got, Unread>>;

/// The files a command returns to write: a write parameter and the bytes.
type Writes = Vec<(&'static str, Vec<u8>)>;

/// The state a command declares, as given to it: kept once its edit is committed.
enum Held {
    None,
    LastCaption(Option<String>),
    Conflicts(Option<Vec<Conflict>>),
}

/// Run generic command `P` with `params`.
///
/// # Errors
/// Parameters that do not read, a project that cannot be taken, the command's refusal, or a file
/// it returned that could not be written.
pub(crate) fn run<P: Generic>(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let p = parse::<P>(params).map_err(|e| err(e.to_string()))?;
    p.check().map_err(|e| err(e.to_string()))?;
    let read: Read = P::IO.reads.iter().filter_map(|(k, what)| Some((*k, read_file(given(params, k)?, *what)))).collect();
    let (v, writes, echo) = match P::PAGES {
        Runs::Available => on_project(s, |proj, pg, page| commit::<P>(proj, pg, Some(page), &read, p, None))?,
        Runs::Any => on_any_page(s, |proj, pg| commit::<P>(proj, pg, None, &read, p, None))?,
        Runs::ReadAnywhere => {
            let uid = s.state()?.uid;
            on_any_page(s, |proj, pg| {
                let page = proj.page_of(uid);
                let no_edit = match page {
                    None => Some("page view not found"),
                    Some(page) => proj.pages.get(&page).is_some_and(|v| v.unavailable).then_some(UNAVAILABLE),
                };
                commit::<P>(proj, pg, page, &read, p, no_edit)
            })?
        }
    };
    for (key, bytes) in writes {
        let target = P::IO.writes.contains(&key).then(|| given(params, key)).flatten();
        let target = target.ok_or_else(|| err(format!("`{key}` is not a file this command writes")))?;
        std::fs::write(target, bytes).map_err(|e| err(format!("Could not write {target}: {e}")))?;
    }
    if let Some(line) = echo {
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

/// Run `p` on the project, commit its edit, and answer with the files to write and the line to
/// echo. The declared state is given as a copy, kept only once the edit is committed. An edit is
/// refused with `no_edit` when given (the page is not one an edit runs on).
fn commit<P: Generic>(
    proj: &mut Project, pg: &mut Pages<'_>, page: Option<PageId>, read: &Read, p: P, no_edit: Option<&'static str>,
) -> Result<(Value, Writes, Option<String>), CmdError> {
    let mut held = match P::STATE {
        State::None => Held::None,
        State::LastCaption => Held::LastCaption(proj.last_caption.clone()),
        State::Conflicts => Held::Conflicts(None),
    };
    let change_count = proj.change_count();
    let Outcome { edit, answer, writes, echo } = {
        let state = match &mut held {
            Held::None => Slot::None,
            Held::LastCaption(c) => Slot::LastCaption(c),
            Held::Conflicts(set) => Slot::Conflicts { waiting: &proj.conflicts, set },
        };
        let mut cx = Cx {
            doc: proj.ledger.document(),
            ledger: &proj.ledger,
            ids: &mut proj.ids,
            page,
            change_count,
            tag_high: &mut proj.tag_high,
            designation_uses: &mut proj.designation_uses,
            derived: &mut proj.derived,
            metrics: &crate::text_style::EngineMetrics,
            read,
            state,
        };
        p.run(&mut cx)?
    };
    if let Some(e) = edit {
        if let Some(why) = no_edit {
            return Err(CmdError::refused(why));
        }
        proj.apply_edit(pg, P::ID, &e.label, e.ops)?;
    }
    match held {
        Held::None => {}
        Held::LastCaption(c) => proj.last_caption = c,
        Held::Conflicts(None) => {}
        Held::Conflicts(Some(c)) => proj.conflicts = c,
    }
    let v = match answer {
        Answer::Now(v) => v,
        Answer::Committed(f) => f(proj.ledger.document(), &mut proj.derived)?,
    };
    let echo = match echo {
        Echo::Nothing => None,
        Echo::Line(l) => Some(l),
        Echo::Message => v.get("message").and_then(Value::as_str).map(str::to_string),
    };
    Ok((v, writes, echo))
}
