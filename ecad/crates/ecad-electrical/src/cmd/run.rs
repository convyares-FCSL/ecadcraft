//! A generic command (owner decision 5, design `commands.md`): its params struct, what it may read
//! and change ([`Cx`]), the files it declares ([`Io`]) and its result ([`Outcome`]).
//!
//! A generic command is a declaration: a `#[derive(Deserialize, JsonSchema)]` params struct that
//! implements [`Generic`]. The engine's one runner reads the parameters ([`crate::cmd::input`]),
//! takes the project, gives the command a [`Cx`], commits its [`Edit`] as one transaction under its
//! id, echoes its message and answers with its JSON. Its MCP schema is the params struct's.
//!
//! Excludes: the session (drawing, cursor, selection, CADCraft entities), locking and committing,
//! which are the engine's. A command that needs the session is not generic.

use ecad_model::{Document, IdAllocator, Ledger, Op, PageId};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::CmdError;
use crate::derived::Views;
use crate::designations::Uses;
use crate::devices::TagHigh;
use crate::proposals::Conflict;

/// What a generic command may read and change: the document, new ids and the project's working
/// state. The document changes only through the [`Edit`] it returns.
pub struct Cx<'a> {
    /// The project's document, as committed.
    pub doc: &'a Document,
    /// New ids.
    pub ids: &'a mut IdAllocator,
    /// The active page (none for a command that runs on any page, [`Pages::Any`]).
    pub page: Option<PageId>,
    /// The highest tag numbers in use.
    pub tag_high: &'a mut TagHigh,
    /// The committed ledger (the document's history), for [`Cx::uses`].
    pub ledger: &'a Ledger,
    /// Where each designation is used, caught up by [`Cx::uses`].
    pub designation_uses: &'a mut Uses,
    /// The derived views (report models are built on demand).
    pub derived: &'a mut Views,
    /// The location and function conflicts waiting for an answer.
    pub conflicts: &'a [Conflict],
    /// The disk, when the command declares files ([`Generic::IO`]).
    pub files: &'a mut dyn Files,
}

impl Cx<'_> {
    /// Where each designation is used, caught up with the ledger.
    pub fn uses(&mut self) -> &Uses {
        self.designation_uses.catch_up(self.ledger);
        self.designation_uses
    }
}

/// The disk as a generic command sees it. The engine gives the real one only to a command that
/// declares [`Io::Files`]; any other command's reads and writes are refused.
pub trait Files {
    /// The bytes of the file at `path`, at most `max` (one more says it is too large).
    ///
    /// # Errors
    /// The file cannot be read.
    fn read(&mut self, path: &str, max: usize) -> std::io::Result<Vec<u8>>;

    /// Write `bytes` to the file at `path`, replacing it.
    ///
    /// # Errors
    /// The file cannot be written.
    fn write(&mut self, path: &str, bytes: &[u8]) -> std::io::Result<()>;
}

/// No disk: every read and write is refused.
pub struct NoFiles;

impl Files for NoFiles {
    fn read(&mut self, _: &str, _: usize) -> std::io::Result<Vec<u8>> {
        Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "this command declares no files"))
    }

    fn write(&mut self, _: &str, _: &[u8]) -> std::io::Result<()> {
        Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "this command declares no files"))
    }
}

/// One transaction: its label and its ops.
pub struct Edit {
    /// The label in the history.
    pub label: String,
    /// The ops.
    pub ops: Vec<Op>,
}

/// A generic command's result.
pub struct Outcome {
    /// The transaction to commit (none: the command writes nothing).
    pub edit: Option<Edit>,
    /// The JSON answer.
    pub answer: Answer,
    /// Project state to keep once the edit is committed (a commit clears the waiting conflicts).
    pub keep: Keep,
}

impl Outcome {
    /// An answer that writes nothing.
    pub fn answer(answer: Value) -> Outcome {
        Outcome { edit: None, answer: Answer::Now(answer), keep: Keep::Nothing }
    }

    /// `ops` as one transaction labelled `label`, and the answer.
    pub fn edit(label: impl Into<String>, ops: Vec<Op>, answer: Value) -> Outcome {
        Outcome { edit: Some(Edit { label: label.into(), ops }), answer: Answer::Now(answer), keep: Keep::Nothing }
    }

    /// As [`Outcome::edit`], with no transaction when `ops` is empty.
    pub fn edit_if_any(label: impl Into<String>, ops: Vec<Op>, answer: Value) -> Outcome {
        let edit = (!ops.is_empty()).then(|| Edit { label: label.into(), ops });
        Outcome { edit, answer: Answer::Now(answer), keep: Keep::Nothing }
    }

    /// As [`Outcome::edit_if_any`], answering from the document once the edit is committed.
    pub fn edit_then(label: impl Into<String>, ops: Vec<Op>, answer: impl FnOnce(&Document) -> Value + 'static) -> Outcome {
        let edit = (!ops.is_empty()).then(|| Edit { label: label.into(), ops });
        Outcome { edit, answer: Answer::Committed(Box::new(answer)), keep: Keep::Nothing }
    }
}

/// A command's JSON answer.
pub enum Answer {
    /// Known before the commit.
    Now(Value),
    /// Read from the document once the edit is committed.
    Committed(Box<dyn FnOnce(&Document) -> Value>),
}

/// Project state a command keeps after its commit.
pub enum Keep {
    /// Nothing.
    Nothing,
    /// These conflicts wait for an answer (`conflicts`).
    Conflicts(Vec<Conflict>),
    /// The last continuation caption typed (offered next time).
    Caption(String),
}

/// Which pages a command runs on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pages {
    /// A page whose drawing could be rebuilt (an unavailable page is refused).
    Available,
    /// Any page of the project, also an unavailable one.
    Any,
}

/// The files a command declares.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Io {
    /// None.
    None,
    /// It reads or writes the files its parameters name.
    Files,
}

/// What the engine echoes on the command line.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Echo {
    /// Nothing.
    Nothing,
    /// The answer's `message`, when it has one.
    Message,
}

/// A generic command: its params struct, read from the JSON, and its run.
pub trait Generic: DeserializeOwned + JsonSchema {
    /// The command's id (`execute`'s name, the command line's word).
    const ID: &'static str;
    /// Its title in menus and the history.
    const TITLE: &'static str;
    /// Its parameters in words (`help`, the command list).
    const HELP: &'static str;
    /// The pages it runs on.
    const PAGES: Pages = Pages::Available;
    /// The files it declares.
    const IO: Io = Io::None;
    /// What it echoes.
    const ECHO: Echo = Echo::Nothing;

    /// Refuse parameters that read but do not fit together, before the project is taken.
    ///
    /// # Errors
    /// The refusal, in words.
    fn check(&self) -> Result<(), CmdError> {
        Ok(())
    }

    /// Decide: the edit to commit and the answer.
    ///
    /// # Errors
    /// The command's refusal, in words; nothing is written.
    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError>;
}
