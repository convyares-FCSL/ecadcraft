//! The crate's error type, [`CmdError`], and the refusals said in more than one place ([`msg`]).
//!
//! The words a person reads are each variant's `Display` text, word for word as before (brief
//! 070). They become a `String` only at the edge: a command's result, MCP, the UI.

use std::borrow::Cow;

use ecad_model::{ModelError, Noun, ParseError};

/// Why an electrical rule or command refused.
///
/// # Semantics
/// Nothing was changed: a refused command records no transaction. Callers that only show the
/// refusal convert with `to_string()`; callers that act on it match the kind (an agent can tell
/// a missing object from a read-only project from a rule's refusal).
///
/// # Example
/// ```
/// use ecad_electrical::CmdError;
/// use ecad_model::Noun;
/// assert_eq!(CmdError::NotFound(Noun::Wire).to_string(), "No such wire.");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CmdError {
    /// The input names something the project does not have.
    #[error("No such {0}.")]
    NotFound(Noun),
    /// No project is open.
    #[error("No project is open.")]
    NoProject,
    /// The project is open read-only.
    #[error("The project is read-only.")]
    ReadOnly,
    /// The command's input does not read: a missing, mistyped or unknown parameter.
    #[error("{0}")]
    Input(Cow<'static, str>),
    /// A rule's own explanation, with the way forward where there is one.
    #[error("{0}")]
    Refused(Cow<'static, str>),
    /// One of the model's readers refused.
    #[error(transparent)]
    Parse(#[from] ParseError),
    /// The model refused the ops.
    #[error(transparent)]
    Model(#[from] ModelError),
    /// The project file's data did not read, or could not be written.
    #[error(transparent)]
    File(#[from] ecad_file::FileError),
    /// A project file could not be opened or saved.
    #[error(transparent)]
    Binding(#[from] ecad_file::binding::BindingError),
    /// A path typed for a project file cannot be used.
    #[error(transparent)]
    Path(#[from] ecad_file::binding::PathError),
}

impl CmdError {
    /// A rule's refusal in words.
    pub fn refused(why: impl Into<Cow<'static, str>>) -> Self {
        CmdError::Refused(why.into())
    }

    /// An input that does not read, in words.
    pub fn input(why: impl Into<Cow<'static, str>>) -> Self {
        CmdError::Input(why.into())
    }
}

/// The edge: a refusal shown as text.
impl From<CmdError> for String {
    fn from(e: CmdError) -> String {
        e.to_string()
    }
}

/// Refusals said in more than one place, so their words live once.
pub mod msg {
    /// `setprop` of a wire number on one conductor.
    pub const USE_WIRENUMBER: &str = "use wirenumber: a wire number belongs to the whole net";
    /// `setprop` of a potential on one conductor.
    pub const USE_POTENTIAL: &str = "use potential: a potential belongs to the whole net";
    /// A potential's colour or cross-section (brief 050b, design 050 §6.9).
    pub const CATEGORY_COLOURS: &str = "Colour and cross-section are set on wire categories now (setstyle {catalogue: \"wire\"}).";
    /// `joinwires` where nothing meets.
    pub const NO_ENDS_TO_JOIN: &str = "There are no wire ends to join here.";
    /// A wire-number command on a net without a wire.
    pub const NO_NUMBER_TO_FIX: &str = "That net has no number to fix yet.";
    /// A folder the input names that the project does not have (pages, folders and revisions).
    pub const NO_SUCH_FOLDER: &str = "no such folder";
    /// A page the input names that the project does not have (pages, folders and revisions).
    pub const NO_SUCH_PAGE: &str = "no such page";
    /// A component the input names that the project does not have (continuations, cross-references).
    pub const NO_SUCH_COMPONENT: &str = "no such component";
    /// A cable the input names that the project does not have (cable references, cross-references).
    pub const NO_SUCH_CABLE: &str = "no such cable";
    /// A folder name that does not read.
    pub const FOLDER_NAME: &str = "A folder name is 1 to 100 characters, with no control characters.";
    /// A page title that does not read.
    pub const PAGE_TITLE: &str = "`title` must be a non-empty string of at most 200 characters";
    /// A `proposal` word that does not read (brief 052d).
    pub const PROPOSAL_WORDS: &str = "`proposal` is \"propose\", \"move\" or \"keep\"";
}
