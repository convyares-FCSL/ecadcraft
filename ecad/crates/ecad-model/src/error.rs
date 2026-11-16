//! The crate's error type.

use thiserror::Error;

/// Convenient result alias for ecad-model.
pub type Result<T> = std::result::Result<T, ModelError>;

/// Why an op, transaction or history step was refused.
///
/// # Semantics
/// Every variant leaves the document unchanged: transactions apply all or nothing.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ModelError {
    /// An op's recorded `before` value does not match the document (TX-04).
    #[error("precondition failed: {0}")]
    Precondition(String),
    /// The document would break an invariant (dangling reference, DOC-05, DOC-07, …).
    #[error("invalid document: {0}")]
    Invariant(String),
    /// A transaction with no ops.
    #[error("empty transaction")]
    Empty,
    /// Undo or redo has nothing to act on.
    #[error("{0}")]
    NothingTo(&'static str),
    /// The log itself is inconsistent (replay of a corrupt log).
    #[error("corrupt log: {0}")]
    CorruptLog(String),
}

/// What a refusal is about, in the words a person reads: "No such {noun}." (`docs/glossary.md`).
///
/// # Semantics
/// `Display` is the lower-case word as it stands inside a sentence ("connection point", never
/// "terminal" for a device's landing point). The variant names follow the code's own names.
///
/// # Example
/// ```
/// use ecad_model::Noun;
/// assert_eq!(format!("No such {}.", Noun::Terminal), "No such connection point.");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Noun {
    Cable,
    Channel,
    Definition,
    Device,
    Folder,
    Net,
    Object,
    Page,
    Point,
    Representation,
    Strip,
    Symbol,
    /// A device's landing point for a conductor (`Terminal` in code).
    Terminal,
    /// The DIN-rail terminal device.
    TerminalDevice,
    TerminalStrip,
    Wire,
}

impl Noun {
    /// The word in a sentence.
    pub const fn word(self) -> &'static str {
        match self {
            Noun::Cable => "cable",
            Noun::Channel => "channel",
            Noun::Definition => "definition",
            Noun::Device => "device",
            Noun::Folder => "folder",
            Noun::Net => "net",
            Noun::Object => "object",
            Noun::Page => "page",
            Noun::Point => "point",
            Noun::Representation => "representation",
            Noun::Strip => "strip",
            Noun::Symbol => "symbol",
            Noun::Terminal => "connection point",
            Noun::TerminalDevice => "terminal",
            Noun::TerminalStrip => "terminal strip",
            Noun::Wire => "wire",
        }
    }
}

impl std::fmt::Display for Noun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.word())
    }
}

/// Which of the model's public readers refused (see [`ParseError::Invalid`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Parsed {
    /// [`crate::links::parse`].
    Link,
    /// [`crate::numbering::plan`].
    Numbering,
    /// [`crate::SheetSpec::parse`] and [`crate::SheetSpec::check`].
    Sheet,
    /// [`crate::tag::normalise`].
    Tag,
    /// [`crate::wiring::valid_number`].
    WireNumber,
    /// [`crate::merge::join_cables`].
    CableJoin,
}

/// Why one of the model's public readers refused its input.
///
/// # Semantics
/// `Display` is the sentence a person reads, word for word as before this type existed (brief
/// 070); callers that only show it convert with `to_string()` (or `?` into a `String` error).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    /// The input names something the document does not have.
    #[error("No such {0}.")]
    NotFound(Noun),
    /// The input does not read; `why` says what to write instead.
    #[error("{why}")]
    Invalid { what: Parsed, why: std::borrow::Cow<'static, str> },
}

impl ParseError {
    /// An [`ParseError::Invalid`] of `what`.
    pub fn invalid(what: Parsed, why: impl Into<std::borrow::Cow<'static, str>>) -> Self {
        ParseError::Invalid { what, why: why.into() }
    }
}

/// The edge: a refusal shown as text (command results, MCP, the UI).
impl From<ParseError> for String {
    fn from(e: ParseError) -> String {
        e.to_string()
    }
}
