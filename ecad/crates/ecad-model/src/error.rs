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
