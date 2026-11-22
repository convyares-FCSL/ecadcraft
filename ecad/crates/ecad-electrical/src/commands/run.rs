//! A generic command (owner decision 5, design `commands.md`): its params struct, what it may read
//! and change ([`Cx`]), the files it declares ([`Io`]) and its result ([`Outcome`]).
//!
//! A generic command is a declaration: a `#[derive(Deserialize, JsonSchema)]` params struct that
//! implements [`Generic`], with what it touches as data: its pages, its access, its files, its own
//! state. The engine's one runner reads the parameters ([`crate::commands::input`]),
//! takes the project, gives the command a [`Cx`], commits its [`Edit`] as one transaction under its
//! id, echoes its message and answers with its JSON. Its MCP schema is the params struct's.
//!
//! Excludes: the session (drawing, cursor, selection, CADCraft entities), locking and committing,
//! which are the engine's. A command that needs the session is not generic.

use std::collections::BTreeMap;

use ecad_model::{DeviceId, Document, IdAllocator, Ledger, Op, PageId, Standard, Style};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::CmdError;
use crate::devices::tags::TagHigh;
use crate::locations::designations::Uses;
use crate::styles::text_style::fonts::TextMetrics;
use crate::symbols::StarterKey;
use crate::symbols::proposals::Conflict;
use crate::views::Views;

/// What every generic command may read and change: the document, new ids and the state several
/// commands share. The document changes only through the [`Edit`] it returns. State a few commands
/// keep beyond the document is declared by each of them ([`Generic::STATE`]) and given in
/// [`Cx::state`].
pub struct Cx<'a> {
    /// The project's document, as committed.
    pub doc: &'a Document,
    /// The committed ledger (the document's history), for [`Cx::uses`].
    pub ledger: &'a Ledger,
    /// New ids.
    pub ids: &'a mut IdAllocator,
    /// The active page (none for a command that runs on any page, [`Pages::Any`]).
    pub page: Option<PageId>,
    /// The style of new drawn items: the drawing's default layer, by layer.
    pub plain: Style,
    /// The drawing's snap tolerance.
    pub tolerance: f64,
    /// The project's change count since it opened (a preview's `based_on`).
    pub change_count: u64,
    /// The highest tag numbers in use.
    pub tag_high: &'a mut TagHigh,
    /// Where each designation is used, caught up by [`Cx::uses`].
    pub designation_uses: &'a mut Uses,
    /// The derived views (report models are built on demand).
    pub derived: &'a mut Views,
    /// The name of the branch shown when it is not the release line (none: on the release line, or
    /// the project is not in a file). Revisions are edited and issued on the release line only.
    pub off_release_line: Option<String>,
    /// This machine's fonts, to measure text.
    pub metrics: &'static dyn TextMetrics,
    /// The files the command declares ([`Generic::IO`]), read by the engine before it runs: what
    /// each read parameter's file gave, or why it could not be read. A parameter not given is
    /// absent.
    pub read: &'a BTreeMap<&'static str, Result<Got, Unread>>,
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

/// Project state a command keeps beyond the document, as it declares it ([`Generic::STATE`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    /// None.
    None,
    /// The last continuation caption typed (offered next time; `contcaption`).
    LastCaption,
    /// The location and function conflicts waiting for an answer (design 052 §5.5; `setlocation`,
    /// `setfunction`, `setprop`, `conflicts`). Any committed edit leaves them.
    Conflicts,
    /// What the last placement in this session placed (the click tools' defaults; `terminalblocks`).
    LastPlaced,
}

/// The declared state, given to the command. What it changes is kept only when its edit is
/// committed (or it has none).
pub enum Slot<'a> {
    /// None declared.
    None,
    /// [`State::LastCaption`]: the caption, which the command may change.
    LastCaption(&'a mut Option<String>),
    /// [`State::Conflicts`]: those waiting, and the ones to wait once the edit is committed (none
    /// set: a committed edit leaves them, no edit keeps them).
    Conflicts { waiting: &'a [Conflict], set: &'a mut Option<Vec<Conflict>> },
    /// [`State::LastPlaced`]: the last placement, which the command may change.
    LastPlaced(&'a mut Option<LastPlaced>),
}

/// What the last placement in this session placed, for the prompts' defaults (design 018 §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastPlaced {
    /// The starter symbol placed (none for a connector, which has no starter of its own).
    pub starter: Option<StarterKey>,
    /// The device it was placed on or as.
    pub device: DeviceId,
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
    /// What to echo on the command line, once the edit is committed.
    pub echo: Echo,
}

/// What a command echoes on the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Echo {
    /// Nothing.
    Nothing,
    /// These lines, one after another.
    Lines(Vec<String>),
    /// The answer's `message`, read once the edit is committed.
    Message,
}

impl Outcome {
    fn new(edit: Option<Edit>, answer: Answer) -> Outcome {
        Outcome { edit, answer, writes: Vec::new(), echo: Echo::Nothing }
    }

    /// This outcome, echoing `line` (none: nothing).
    #[must_use]
    pub fn echo(mut self, line: Option<impl Into<String>>) -> Outcome {
        self.echo = line.map_or(Echo::Nothing, |l| Echo::Lines(vec![l.into()]));
        self
    }

    /// This outcome, echoing `lines`, one after another.
    #[must_use]
    pub fn echo_lines(mut self, lines: Vec<String>) -> Outcome {
        self.echo = Echo::Lines(lines);
        self
    }

    /// This outcome, echoing the answer's `message` (read once the edit is committed).
    #[must_use]
    pub fn echo_message(mut self) -> Outcome {
        self.echo = Echo::Message;
        self
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
        Outcome::edit_then_try(label, ops, move |doc| Ok(answer(doc)))
    }

    /// As [`Outcome::edit_then`], with an answer that may refuse (the edit stays committed).
    pub fn edit_then_try(label: impl Into<String>, ops: Vec<Op>, answer: impl FnOnce(&Document) -> Result<Value, CmdError> + 'static) -> Outcome {
        let edit = (!ops.is_empty()).then(|| Edit { label: label.into(), ops });
        Outcome::new(edit, Answer::Committed(Box::new(move |doc, _| answer(doc))))
    }

    /// As [`Outcome::edit_then`], answering from the document and its derived views.
    pub fn edit_then_derived(label: impl Into<String>, ops: Vec<Op>, answer: impl FnOnce(&Document, &mut Views) -> Value + 'static) -> Outcome {
        let edit = (!ops.is_empty()).then(|| Edit { label: label.into(), ops });
        Outcome::new(edit, Answer::Committed(Box::new(move |doc, views| Ok(answer(doc, views)))))
    }
}

/// A command's JSON answer.
pub enum Answer {
    /// Known before the commit.
    Now(Value),
    /// Read from the document once the edit is committed (a refusal leaves the edit committed).
    Committed(Box<dyn FnOnce(&Document, &mut Views) -> Result<Value, CmdError>>),
}

/// Which pages a command runs on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pages {
    /// A page whose drawing could be rebuilt (an unavailable page is refused).
    Available,
    /// Any page of the project, also an unavailable one.
    Any,
    /// Any page to read; an edit only on a page whose drawing could be rebuilt.
    ReadAnywhere,
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
    /// Parameters naming a file read before the command runs, and what is read from it.
    pub reads: &'static [(&'static str, Read)],
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

/// What the engine reads from a declared file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Read {
    /// Its bytes, at most this many (one more says it is too large).
    Bytes(usize),
    /// The standard of the `.ecad` project it is, opened read-only (the file is never changed).
    Standard,
}

/// What a declared file gave ([`Read`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Got {
    /// [`Read::Bytes`].
    Bytes(Vec<u8>),
    /// [`Read::Standard`]: the project's standard (one with none is not read).
    Standard(Box<Standard>),
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
    /// The state it keeps beyond the document.
    const STATE: State = State::None;

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
