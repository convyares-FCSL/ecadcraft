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
use ecad_electrical::cmd::run::{Answer, Cx, Echo, Generic, Outcome, Pages as Runs, Slot, State, Unread};
use ecad_model::PageId;
use serde_json::Value;

use crate::access::Pages;
use crate::cmd::input::Typed;
use crate::cmd::{err, on_any_page, on_project};
use crate::project::Project;

/// The registration of generic command `P`.
pub(crate) fn spec<P: Generic>() -> CommandSpec {
    CommandSpec::new(P::ID, P::TITLE, run::<P>).params(P::HELP).noundo()
}

/// Generic command `P` with its schema and what the agent gate reads.
pub(crate) const fn typed<P: Generic>() -> Typed {
    Typed { id: P::ID, run: run::<P>, schema: schema::<P>, access: Some(P::ACCESS), files: P::IO }
}

/// The bytes read for each read parameter of `P` that `params` gives.
type Read = BTreeMap<&'static str, Result<Vec<u8>, Unread>>;

/// The files a command returns to write: a write parameter and the bytes.
type Writes = Vec<(&'static str, Vec<u8>)>;

/// Run generic command `P` with `params`.
///
/// # Errors
/// Parameters that do not read, a project that cannot be taken, the command's refusal, or a file
/// it returned that could not be written.
pub(crate) fn run<P: Generic>(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let p = parse::<P>(params).map_err(|e| err(e.to_string()))?;
    p.check().map_err(|e| err(e.to_string()))?;
    let read: Read = P::IO.reads.iter().filter_map(|(k, max)| Some((*k, read_file(path(params, k)?, *max)))).collect();
    let (v, writes) = match P::PAGES {
        Runs::Available => on_project(s, |proj, pg, page| commit::<P>(proj, pg, Some(page), &read, p))?,
        Runs::Any => on_any_page(s, |proj, pg| commit::<P>(proj, pg, None, &read, p))?,
    };
    for (key, bytes) in writes {
        let target = P::IO.writes.contains(&key).then(|| path(params, key)).flatten();
        let target = target.ok_or_else(|| err(format!("`{key}` is not a file this command writes")))?;
        std::fs::write(target, bytes).map_err(|e| err(format!("{target} could not be written: {e}.")))?;
    }
    if P::ECHO == Echo::Message
        && let Some(m) = v.get("message").and_then(Value::as_str)
    {
        s.echo(m.to_string());
    }
    Ok(v)
}

/// The path parameter `key` gives, when it gives one.
fn path<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty())
}

/// The bytes of the file at `path`, at most `max` (one more says it is too large), or why not.
fn read_file(path: &str, max: usize) -> Result<Vec<u8>, Unread> {
    use std::io::Read as _;
    let mut buf = Vec::new();
    let f = std::fs::File::open(path).map_err(|e| Unread(e.to_string()))?;
    f.take(u64::try_from(max).unwrap_or(u64::MAX).saturating_add(1)).read_to_end(&mut buf).map_err(|e| Unread(e.to_string()))?;
    Ok(buf)
}

/// Run `p` on the project, commit its edit, and answer with the files to write. The declared
/// state changes only with a committed edit (it is put back otherwise).
fn commit<P: Generic>(proj: &mut Project, pg: &mut Pages<'_>, page: Option<PageId>, read: &Read, p: P) -> Result<(Value, Writes), CmdError> {
    let caption_before = proj.last_caption.clone();
    let ran = {
        let state = match P::STATE {
            State::None => Slot::None,
            State::LastCaption => Slot::LastCaption(&mut proj.last_caption),
        };
        let mut cx = Cx {
            doc: proj.ledger.document(),
            ledger: &proj.ledger,
            ids: &mut proj.ids,
            page,
            tag_high: &mut proj.tag_high,
            designation_uses: &mut proj.designation_uses,
            derived: &mut proj.derived,
            read,
            state,
        };
        p.run(&mut cx)
    };
    let committed = ran.and_then(|Outcome { edit, answer, writes }| {
        if let Some(e) = edit {
            proj.apply_edit(pg, P::ID, &e.label, e.ops)?;
        }
        Ok((answer, writes))
    });
    let (answer, writes) = committed.inspect_err(|_| proj.last_caption = caption_before)?;
    let v = match answer {
        Answer::Now(v) => v,
        Answer::Committed(f) => f(proj.ledger.document()),
    };
    Ok((v, writes))
}
