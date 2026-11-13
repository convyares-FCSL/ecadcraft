//! Inline edit (design §6.2): only cells of `Set` columns. F2, Enter or a double-click starts;
//! Enter commits one [`super::TableEvent::Commit`]; Esc reverts; Tab commits and goes to the next
//! set cell. The owner runs the command; a refusal it reports shows under the cell
//! ([`Refusal`]) and the value reverts, because the cell reads the source again.

use super::source::ColumnId;

/// A cell being edited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellEdit<K> {
    pub key: K,
    pub col: ColumnId,
    /// What is typed.
    pub text: String,
    /// The value when the edit started (an unchanged Enter commits nothing).
    pub original: String,
    /// Focus the editor on the next frame.
    pub(crate) focus: bool,
}

impl<K> CellEdit<K> {
    pub fn new(key: K, col: ColumnId, value: String) -> Self {
        Self { key, col, text: value.clone(), original: value, focus: true }
    }

    /// Whether the typed text differs from the value it started from.
    pub fn changed(&self) -> bool {
        self.text.trim() != self.original.trim()
    }
}

/// A refusal of the last commit, shown under its cell until the next edit or click.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal<K> {
    pub key: K,
    pub col: ColumnId,
    pub message: String,
}
