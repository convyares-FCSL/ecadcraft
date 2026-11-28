//! A generic command: its definition ([`Request`]: id, params and reply), what it may read
//! and change ([`Cx`]), the files it declares ([`Io`]) and its result ([`Outcome`]).
//!
//! A generic command is a declaration: a type that implements [`Generic`], with its params and
//! reply types and what it touches as data: its pages, its access, its files, its own state. The
//! engine's one runner reads the parameters ([`crate::commands::input`]), takes the project, gives
//! the command a [`Cx`], commits its [`Edit`] as one transaction under its id, echoes its message
//! and answers with its reply, made JSON only there. Its MCP schemas are its params' and its
//! reply's, and its parameters text is made from the params ([`crate::commands::input::help`]):
//! the struct's doc comment is the command's summary and notes (its alternative forms, units,
//! refusals), each field's doc comment that field's help, and a closed set of words is an enum, so
//! its words are listed.
//!
//! [`JsonGeneric`] is the older form, a params struct answering with a JSON value; commands still
//! written that way are run by the same runner.
//!
//! Excludes: the session (drawing, cursor, selection, CADCraft entities), locking and committing,
//! which are the engine's. A command that needs the session is not generic.

use std::collections::BTreeMap;

use ecad_model::{DeviceId, Document, IdAllocator, Ledger, Op, PageId, Standard, Style};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::CmdError;
pub use crate::commands::request::{Reply, Request};
use crate::devices::tags::TagHigh;
use crate::locations::designations::Uses;
use crate::symbols::StarterKey;
use crate::symbols::proposals::Conflict;
use crate::views::Views;
use ecad_model::look::Look;
use ecad_model::look::TextMetrics;

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
    /// This machine's fonts, for the commands that resolve fonts themselves (text styles, title
    /// blocks).
    pub metrics: &'static dyn TextMetrics,
    /// How the rules' texts look on this machine, to lay out and measure them.
    pub look: &'static dyn Look,
    /// Every crate's defaults and style catalogues.
    pub registry: &'static ecad_model::registry::data::Registry,
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
    /// The location and function conflicts waiting for an answer (`setlocation`,
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

/// What the last placement in this session placed, for the prompts' defaults.
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

/// A generic command's result, answering with `R` (its [`Request::Reply`]; a JSON value for a
/// [`JsonGeneric`]).
pub struct Outcome<R = Value> {
    /// The transaction to commit (none: the command writes nothing).
    pub edit: Option<Edit>,
    /// The answer.
    pub answer: Answer<R>,
    /// Files to write once the edit (if any) is committed: a write parameter of [`Generic::IO`] (its value
    /// is the path) and the bytes. The engine writes them; one that cannot be written commits nothing.
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

impl<R: 'static> Outcome<R> {
    fn new(edit: Option<Edit>, answer: Answer<R>) -> Outcome<R> {
        Outcome { edit, answer, writes: Vec::new(), echo: Echo::Nothing }
    }

    /// This outcome, echoing `line` (none: nothing).
    #[must_use]
    pub fn echo(mut self, line: Option<impl Into<String>>) -> Outcome<R> {
        self.echo = line.map_or(Echo::Nothing, |l| Echo::Lines(vec![l.into()]));
        self
    }

    /// This outcome, echoing `lines`, one after another.
    #[must_use]
    pub fn echo_lines(mut self, lines: Vec<String>) -> Outcome<R> {
        self.echo = Echo::Lines(lines);
        self
    }

    /// This outcome, echoing the answer's `message` (read once the edit is committed).
    #[must_use]
    pub fn echo_message(mut self) -> Outcome<R> {
        self.echo = Echo::Message;
        self
    }

    /// An answer that writes nothing.
    pub fn answer(answer: R) -> Outcome<R> {
        Outcome::new(None, Answer::Now(answer))
    }

    /// `ops` as one transaction labelled `label`, and the answer.
    pub fn edit(label: impl Into<String>, ops: Vec<Op>, answer: R) -> Outcome<R> {
        Outcome::new(Some(Edit { label: label.into(), ops }), Answer::Now(answer))
    }

    /// As [`Outcome::edit`], with no transaction when `ops` is empty.
    pub fn edit_if_any(label: impl Into<String>, ops: Vec<Op>, answer: R) -> Outcome<R> {
        let edit = (!ops.is_empty()).then(|| Edit { label: label.into(), ops });
        Outcome::new(edit, Answer::Now(answer))
    }

    /// As [`Outcome::edit_if_any`], answering from the document once the edit is committed.
    pub fn edit_then(label: impl Into<String>, ops: Vec<Op>, answer: impl FnOnce(&Document) -> R + 'static) -> Outcome<R> {
        Outcome::edit_then_try(label, ops, move |doc| Ok(answer(doc)))
    }

    /// As [`Outcome::edit_then`], with an answer that may refuse (the edit stays committed).
    pub fn edit_then_try(label: impl Into<String>, ops: Vec<Op>, answer: impl FnOnce(&Document) -> Result<R, CmdError> + 'static) -> Outcome<R> {
        let edit = (!ops.is_empty()).then(|| Edit { label: label.into(), ops });
        Outcome::new(edit, Answer::Committed(Box::new(move |doc, _| answer(doc))))
    }

    /// As [`Outcome::edit_then`], answering from the document and its derived views.
    pub fn edit_then_derived(label: impl Into<String>, ops: Vec<Op>, answer: impl FnOnce(&Document, &mut Views) -> R + 'static) -> Outcome<R> {
        let edit = (!ops.is_empty()).then(|| Edit { label: label.into(), ops });
        Outcome::new(edit, Answer::Committed(Box::new(move |doc, views| Ok(answer(doc, views)))))
    }
}

/// A command's answer, `R` (made JSON by the engine's runner).
pub enum Answer<R = Value> {
    /// Known before the commit.
    Now(R),
    /// Read from the document once the edit is committed (a refusal leaves the edit committed).
    Committed(Box<dyn FnOnce(&Document, &mut Views) -> Result<R, CmdError>>),
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

/// A generic command: its definition ([`Request`]: its id, params and reply) and its run. Implemented
/// on a type of its own (`pub enum Net {}`), which names the command everywhere: the registry's
/// entry, the app's `call` and the tests.
///
/// # Example
/// ```
/// use ecad_electrical::CmdError;
/// use ecad_electrical::commands::run::{Cx, Generic, Outcome, Reply, Request};
///
/// /// Says hello. Reads only.
/// #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema, Default)]
/// #[serde(deny_unknown_fields)]
/// pub struct HelloParams {}
///
/// /// What `hello` answers.
/// #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
/// pub struct HelloReply {
///     /// Always "hello".
///     pub said: String,
/// }
/// impl Reply for HelloReply {}
///
/// pub enum Hello {}
/// impl Request for Hello {
///     const ID: &'static str = "hello";
///     type Params = HelloParams;
///     type Reply = HelloReply;
/// }
/// impl Generic for Hello {
///     const TITLE: &'static str = "Hello";
///     fn run(_: HelloParams, _: &mut Cx<'_>) -> Result<Outcome<HelloReply>, CmdError> {
///         Ok(Outcome::answer(HelloReply { said: "hello".into() }))
///     }
/// }
/// ```
pub trait Generic: Request {
    /// Its title in menus and the history.
    const TITLE: &'static str;
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
    fn check(_params: &Self::Params, _registry: &ecad_model::registry::data::Registry) -> Result<(), CmdError> {
        Ok(())
    }

    /// Decide: the edit to commit and the reply.
    ///
    /// # Errors
    /// The command's refusal, in words; nothing is written.
    fn run(params: Self::Params, cx: &mut Cx<'_>) -> Result<Outcome<Self::Reply>, CmdError>;
}

/// The older form of a generic command: its params struct, read from the JSON, answering with a
/// JSON value. Run by the same runner as [`Generic`]; a command moves to [`Generic`] when its
/// feature's replies are typed.
pub trait JsonGeneric: DeserializeOwned + schemars::JsonSchema {
    /// The command's id (`execute`'s name, the command line's word).
    const ID: &'static str;
    /// Its title in menus and the history.
    const TITLE: &'static str;
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
    fn check(&self, _registry: &ecad_model::registry::data::Registry) -> Result<(), CmdError> {
        Ok(())
    }

    /// Decide: the edit to commit and the answer.
    ///
    /// # Errors
    /// The command's refusal, in words; nothing is written.
    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError>;
}
