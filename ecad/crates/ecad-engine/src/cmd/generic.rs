//! The one runner of generic commands (owner decision 5, design `commands.md`).
//!
//! A generic command is a declaration in ecad-electrical: a params struct that implements
//! [`Generic`]. [`spec`] registers it and [`typed`] lists it with its schema; [`run`] does the whole
//! path: read the parameters, lock the project, give the command its [`Cx`], commit its edit as
//! one transaction under its id ([`Project::apply_edit`]: history, undo, views), keep what it
//! keeps, echo its message and answer. The agent gate is outside the command and sees the same id.
//!
//! Excludes: commands that need the session (their shims in [`crate::cmd`], [`super::input`]).

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::cmd::input::{parse, schema};
use ecad_electrical::cmd::run::{Answer, Cx, Echo, Files, Generic, Io, Keep, NoFiles, Outcome, Pages as Runs};
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

/// Generic command `P` with its schema.
pub(crate) const fn typed<P: Generic>() -> Typed {
    Typed { id: P::ID, run: run::<P>, schema: schema::<P> }
}

/// Run generic command `P` with `params`.
///
/// # Errors
/// Parameters that do not read, a project that cannot be taken, or the command's refusal.
pub(crate) fn run<P: Generic>(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let p = parse::<P>(params).map_err(|e| err(e.to_string()))?;
    p.check().map_err(|e| err(e.to_string()))?;
    let v = match P::PAGES {
        Runs::Available => on_project(s, |proj, pg, page| commit::<P>(proj, pg, Some(page), p))?,
        Runs::Any => on_any_page(s, |proj, pg| commit::<P>(proj, pg, None, p))?,
    };
    if P::ECHO == Echo::Message
        && let Some(m) = v.get("message").and_then(Value::as_str)
    {
        s.echo(m.to_string());
    }
    Ok(v)
}

/// Run `p` on the project and commit its edit.
fn commit<P: Generic>(proj: &mut Project, pg: &mut Pages<'_>, page: Option<PageId>, p: P) -> Result<Value, CmdError> {
    let mut disk = Disk;
    let mut none = NoFiles;
    let files: &mut dyn Files = match P::IO {
        Io::Files => &mut disk,
        Io::None => &mut none,
    };
    let Outcome { edit, answer, keep } = {
        let mut cx = Cx {
            doc: proj.ledger.document(),
            ids: &mut proj.ids,
            page,
            tag_high: &mut proj.tag_high,
            ledger: &proj.ledger,
            designation_uses: &mut proj.designation_uses,
            derived: &mut proj.derived,
            conflicts: &proj.conflicts,
            files,
        };
        p.run(&mut cx)?
    };
    if let Some(e) = edit {
        proj.apply_edit(pg, P::ID, &e.label, e.ops)?;
    }
    match keep {
        Keep::Nothing => {}
        Keep::Conflicts(c) => proj.conflicts = c,
        Keep::Caption(t) => proj.last_caption = Some(t),
    }
    Ok(match answer {
        Answer::Now(v) => v,
        Answer::Committed(f) => f(proj.ledger.document()),
    })
}

/// The real disk, for commands that declare files.
struct Disk;

impl Files for Disk {
    fn read(&mut self, path: &str, max: usize) -> std::io::Result<Vec<u8>> {
        use std::io::Read as _;
        let f = std::fs::File::open(path)?;
        let mut buf = Vec::new();
        f.take(u64::try_from(max).unwrap_or(u64::MAX).saturating_add(1)).read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn write(&mut self, path: &str, bytes: &[u8]) -> std::io::Result<()> {
        std::fs::write(path, bytes)
    }
}
