//! A generic command (owner decision 5, design `commands.md`): its params struct, what it may read
//! and change ([`Cx`]), the files it declares ([`Io`]) and its result ([`Outcome`]).
//!
//! A generic command is a declaration: a `#[derive(Deserialize, JsonSchema)]` params struct that
//! implements [`Generic`], with what it touches as data: its pages, its access, its files, its own
//! state. The engine's one runner reads the parameters ([`crate::cmd::input`]),
//! takes the project, gives the command a [`Cx`], commits its [`Edit`] as one transaction under its
//! id, echoes its message and answers with its JSON. Its MCP schema is the params struct's.
//!
//! Excludes: the session (drawing, cursor, selection, CADCraft entities), locking and committing,
//! which are the engine's. A command that needs the session is not generic.

use std::collections::BTreeMap;

use ecad_model::{Document, IdAllocator, Ledger, Op, PageId};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::CmdError;
use crate::derived::Views;
use crate::designations::Uses;
use crate::devices::TagHigh;

/// What every generic command may read and change: the document, new ids and the state several
/// commands share. The document changes only through the [`Edit`] it returns. State only one
/// command uses is declared by that command ([`Generic::STATE`]) and given in [`Cx::state`].
pub struct Cx<'a> {
    /// The project's document, as committed.
    pub doc: &'a Document,
    /// The committed ledger (the document's history), for [`Cx::uses`].
    pub ledger: &'a Ledger,
    /// New ids.
    pub ids: &'a mut IdAllocator,
    /// The active page (none for a command that runs on any page, [`Pages::Any`]).
    pub page: Option<PageId>,
    /// The highest tag numbers in use.
    pub tag_high: &'a mut TagHigh,
    /// Where each designation is used, caught up by [`Cx::uses`].
    pub designation_uses: &'a mut Uses,
    /// The derived views (report models are built on demand).
    pub derived: &'a mut Views,
    /// The files the command declares ([`Generic::IO`]), read by the engine before it runs: each
    /// read parameter's bytes, or why it could not be read. A parameter not given is absent.
    pub read: &'a BTreeMap<&'static str, Result<Vec<u8>, Unread>>,
    /// The state the command declares ([`Generic::STATE`]).
    pub state: Slot<'a>,
}

impl Cx<'_> {
    /// Where each designation is used, caught up with the ledger.
    pub fn uses(&mut self) -> &Uses {
        self.designation_uses.catch_up(self.ledger);
        self.designation_uses
    }
}

/// Why a declared file could not be read, in the system's words.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Unread(pub String);

/// Project state only one command uses, as it declares it ([`Generic::STATE`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    /// None.
    None,
    /// The last continuation caption typed (offered next time; `contcaption`).
    LastCaption,
}

/// The declared state, given to the command: it may change it; the engine puts it back if the
/// edit is not committed.
pub enum Slot<'a> {
    /// None declared.
    None,
    /// [`State::LastCaption`].
    LastCaption(&'a mut Option<String>),
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
    /// Files to write once the edit is committed: a write parameter of [`Generic::IO`] (its value
    /// is the path) and the bytes. The engine writes them.
    pub writes: Vec<(&'static str, Vec<u8>)>,
}

impl Outcome {
    fn new(edit: Option<Edit>, answer: Answer) -> Outcome {
        Outcome { edit, answer, writes: Vec::new() }
    }

    /// An answer that writes nothing.
    pub fn answer(answer: Value) -> Outcome {
        Outcome::new(None, Answer::Now(answer))
    }

    /// `ops` as one transaction labelled `label`, and the answer.
    pub fn edit(label: impl Into<String>, ops: Vec<Op>, answer: Value) -> Outcome {
        Outcome::new(Some(Edit { label: label.into(), ops }), Answer::Now(answer))
    }

    /// As [`Outcome::edit`], with no transaction when `ops` is empty.
    pub fn edit_if_any(label: impl Into<String>, ops: Vec<Op>, answer: Value) -> Outcome {
        let edit = (!ops.is_empty()).then(|| Edit { label: label.into(), ops });
        Outcome::new(edit, Answer::Now(answer))
    }

    /// As [`Outcome::edit_if_any`], answering from the document once the edit is committed.
    pub fn edit_then(label: impl Into<String>, ops: Vec<Op>, answer: impl FnOnce(&Document) -> Value + 'static) -> Outcome {
        let edit = (!ops.is_empty()).then(|| Edit { label: label.into(), ops });
        Outcome::new(edit, Answer::Committed(Box::new(answer)))
    }
}

/// A command's JSON answer.
pub enum Answer {
    /// Known before the commit.
    Now(Value),
    /// Read from the document once the edit is committed.
    Committed(Box<dyn FnOnce(&Document) -> Value>),
}

/// Which pages a command runs on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pages {
    /// A page whose drawing could be rebuilt (an unavailable page is refused).
    Available,
    /// Any page of the project, also an unavailable one.
    Any,
}

/// Whether a command changes the project (the agent gate lets a read run with writes off).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Access {
    /// It never records a transaction.
    Reads,
    /// It may record one.
    Edits,
}

/// The files a command reads and writes, by the parameters that name them. The engine reads and
/// writes them (Core touches no disk); the agent gate refuses these parameters from an agent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Io {
    /// Parameters naming a file read before the command runs, with the most bytes read.
    pub reads: &'static [(&'static str, usize)],
    /// Parameters naming a file written after the commit ([`Outcome::writes`]).
    pub writes: &'static [&'static str],
}

impl Io {
    /// No files.
    pub const NONE: Io = Io { reads: &[], writes: &[] };

    /// Every parameter that names a file.
    pub fn keys(&self) -> impl Iterator<Item = &'static str> {
        self.reads.iter().map(|(k, _)| *k).chain(self.writes.iter().copied())
    }
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
pub trait Generic: DeserializeOwned + schemars::JsonSchema {
    /// The command's id (`execute`'s name, the command line's word).
    const ID: &'static str;
    /// Its title in menus and the history.
    const TITLE: &'static str;
    /// Its parameters in words (`help`, the command list).
    const HELP: &'static str;
    /// The pages it runs on.
    const PAGES: Pages = Pages::Available;
    /// Whether it changes the project.
    const ACCESS: Access = Access::Edits;
    /// The files it declares.
    const IO: Io = Io::NONE;
    /// The state only it uses.
    const STATE: State = State::None;
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
