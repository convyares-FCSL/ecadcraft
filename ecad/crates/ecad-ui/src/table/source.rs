//! What a table shows (design §6.1): a [`TableSource`] gives the keys, the [`Column`]s and each
//! [`Cell`]; the table never holds its own copy of a value, it reads cells again for the keys the
//! source reports changed.

use std::borrow::Cow;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};

use ecad_tokens::Color;

use crate::prim::Tone;

/// A column's identity within its source (stable across sorts and resizes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColumnId(pub u16);

/// How wide a column is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Width {
    /// Exactly this wide, not resizable.
    Fixed(f32),
    /// This wide at first; the user can resize it.
    Initial(f32),
    /// The room left.
    Remainder,
}

/// What a cell's editor holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Editor {
    #[default]
    Text,
    Number,
}

/// Whether a column's values are set here or read from the drawing (spec §5, "What you set and
/// what is read").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnKind {
    /// Set here: its [`Cell::Input`] cells look like fields and edit in place.
    Set(Editor),
    /// Read: plain grey text; a cell with a target goes to it.
    Read,
}

/// Where a column's text sits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Right,
    Center,
}

/// One column.
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub id: ColumnId,
    pub title: &'static str,
    pub width: Width,
    pub kind: ColumnKind,
    pub align: Align,
    /// A click on its header sorts by it.
    pub sortable: bool,
    /// It can be grouped by (the header's Group menu).
    pub groupable: bool,
}

impl Column {
    /// A read, sortable, left-aligned column.
    pub fn read(id: u16, title: &'static str, width: Width) -> Self {
        Self { id: ColumnId(id), title, width, kind: ColumnKind::Read, align: Align::Left, sortable: true, groupable: false }
    }
    /// A set (editable) text column.
    pub fn set(id: u16, title: &'static str, width: Width) -> Self {
        Self { kind: ColumnKind::Set(Editor::Text), ..Self::read(id, title, width) }
    }
    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }
    pub fn groupable(mut self) -> Self {
        self.groupable = true;
        self
    }
    pub fn unsorted(mut self) -> Self {
        self.sortable = false;
        self
    }
    /// Whether its values are set here.
    pub fn is_set(&self) -> bool {
        matches!(self.kind, ColumnKind::Set(_))
    }
}

/// One mark in a [`LaneCell`]: a link lane passing a row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Lane {
    /// The row is one the link joins (a dot).
    pub on: bool,
    /// The lane continues to the row above.
    pub up: bool,
    /// The lane continues to the row below.
    pub down: bool,
}

/// Link lanes beside a row (terminal links, design 036 §9): lane `i` is drawn `i` steps right.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct LaneCell {
    pub lanes: Vec<Option<Lane>>,
    /// Shown next to the lanes (028's bridge group `● 1`).
    pub text: String,
}

/// One cell's value and how it looks.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    /// Plain text (`text` colour).
    Text(String),
    /// A number with a unit, right-aligned; sorts by value.
    Number(f64, &'static str),
    /// A value set here: drawn as a field, edited in place (only in a [`ColumnKind::Set`] column).
    Input(String),
    /// A value read from the drawing: grey text; `go` makes it a go-to.
    Read {
        text: String,
        go: bool,
    },
    /// A provisional value: the provisional colour with a dotted underline.
    Provisional(String),
    /// Colour swatches (a wire's colours).
    Swatch(Vec<Color>),
    /// A small label in a tone.
    Badge(String, Tone),
    /// Link lanes.
    Lanes(LaneCell),
    /// Action buttons (a check's fixes): a click reports [`super::TableEvent::Button`] with the
    /// button's index.
    Buttons(Vec<String>),
    Empty,
}

impl Cell {
    /// A read cell with no target.
    pub fn read(text: impl Into<String>) -> Self {
        Cell::Read { text: text.into(), go: false }
    }
    /// A read cell that goes to where the value is set.
    pub fn go(text: impl Into<String>) -> Self {
        Cell::Read { text: text.into(), go: true }
    }

    /// Its text: what filters match, sorts compare (except numbers) and CSV writes.
    pub fn text(&self) -> Cow<'_, str> {
        match self {
            Cell::Text(t) | Cell::Input(t) | Cell::Provisional(t) | Cell::Badge(t, _) => Cow::Borrowed(t),
            Cell::Read { text, .. } => Cow::Borrowed(text),
            Cell::Number(v, unit) => Cow::Owned(number_text(*v, unit)),
            Cell::Lanes(l) => Cow::Borrowed(&l.text),
            Cell::Swatch(_) | Cell::Buttons(_) | Cell::Empty => Cow::Borrowed(""),
        }
    }

    /// A fingerprint of what it shows, for the change tint: two cells that look the same have
    /// the same fingerprint. Not a copy of the value.
    pub fn fingerprint(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        std::mem::discriminant(self).hash(&mut h);
        match self {
            Cell::Number(v, unit) => {
                v.to_bits().hash(&mut h);
                unit.hash(&mut h);
            }
            Cell::Read { text, go } => {
                text.hash(&mut h);
                go.hash(&mut h);
            }
            Cell::Swatch(c) => {
                for c in c {
                    (c.r, c.g, c.b, c.a).hash(&mut h);
                }
            }
            Cell::Badge(t, tone) => {
                t.hash(&mut h);
                tone.name().hash(&mut h);
            }
            Cell::Lanes(l) => l.hash(&mut h),
            Cell::Buttons(b) => b.hash(&mut h),
            other => other.text().hash(&mut h),
        }
        h.finish()
    }
}

/// `12.5 mm`, `3`: a number with no trailing zeros, and its unit.
pub fn number_text(v: f64, unit: &str) -> String {
    let n = if v.is_finite() && v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    if unit.is_empty() { n } else { format!("{n} {unit}") }
}

/// What a source says changed since the table last read it (the update map's answer).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Changed<K> {
    /// Nothing the table shows.
    Nothing,
    /// These keys' rows: patch them (added, changed or removed).
    Keys(Vec<K>),
    /// Everything: rebuild (first show, reload, a feed that rolled over).
    All,
}

/// The rows a table shows. Implemented by each column set (`ecad-trial`'s `lists`).
pub trait TableSource {
    /// The object's ID: the row ↔ object link.
    type Key: Copy + Ord + Hash + Debug;

    fn columns(&self) -> &[Column];

    /// Every row, in the source's own order (the order shown when no column sorts).
    fn keys(&self) -> &[Self::Key];

    /// Whether `key` is a row now. The default looks through [`TableSource::keys`].
    fn contains(&self, key: Self::Key) -> bool {
        self.keys().contains(&key)
    }

    /// Row `key`'s cell in column `col` ([`Cell::Empty`] for a key or column it does not know).
    fn cell(&self, key: Self::Key, col: ColumnId) -> Cell;

    /// The row's accessible label (tests and the control channel find it by this). The default is
    /// its cells' texts joined.
    fn row_label(&self, key: Self::Key) -> String {
        self.columns().iter().map(|c| self.cell(key, c.id).text().into_owned()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(", ")
    }

    /// The filter chips this source offers ("Provisional", "Not on a page yet").
    fn chips(&self) -> &[&'static str] {
        &[]
    }

    /// Whether row `key` passes chip `chip` (chips compose with AND).
    fn chip(&self, _key: Self::Key, _chip: usize) -> bool {
        true
    }

    /// How the search text matches row `key`, when the source has its own rule (`k1` finds
    /// `-K01`). `None`: the table's rule, any shown text containing the query.
    fn matches(&self, _key: Self::Key, _query: &str) -> Option<bool> {
        None
    }
}
