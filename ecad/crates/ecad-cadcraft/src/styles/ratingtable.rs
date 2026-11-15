//! Rating tables (brief 057, design 057 §5, FP8): the `ratingtable` catalogue.
//!
//! A rating table is a user's data set of ratings and factors, entered by hand or imported from a
//! CSV they are licensed to use. It lives in the project or in a standard, like any style. There
//! are **no built-in entries, and ECAD ships no values** (owner, Q11). The project chooses one
//! with the registry key `rating.table` ([`crate::defaults::rating::TABLE`]).
//!
//! Fields: `source` (what the user says the values come from) and [`ROWS`] (the CSV, read by
//! [`crate::rating::dataset`]). The rows field may hold up to
//! [`crate::rating::dataset::MAX_BYTES`] ([`super::field_cap`]); it is read here, raw, not through
//! [`super::field`] (it holds line breaks, which a text field refuses).

use ecad_model::style::StyleLayer;
use ecad_model::{Document, StyleRef};

use super::{Catalogue, Field};
use crate::defaults::{Kind, Scope, Value};
use crate::rating::dataset::{self, Dataset};

/// The catalogue word.
pub const WORD: &str = "ratingtable";
/// The field holding the CSV rows.
pub const ROWS: &str = "rows";
/// The field holding the source.
pub const SOURCE: &str = "source";
/// The longest source text.
pub const MAX_SOURCE_CHARS: usize = 200;

fn any_text(_: &str) -> bool {
    true
}

static FIELDS: &[Field] = &[
    Field { key: SOURCE, label: "Source", kind: Kind::Parsed { max_chars: MAX_SOURCE_CHARS, parse: any_text }, builtin: Value::Text(""), over: None },
    // Read raw by `rows_text`; listed so `style` knows the key.
    Field { key: ROWS, label: "Rows", kind: Kind::Parsed { max_chars: 1, parse: any_text }, builtin: Value::Text(""), over: None },
];

fn choices() -> Vec<&'static crate::defaults::Entry> {
    vec![&crate::defaults::rating::TABLE]
}

/// The rating table catalogue: no built-in entries.
pub static CATALOGUE: Catalogue =
    Catalogue { word: WORD, plural: "Rating tables", one: "Rating table", fields: FIELDS, builtins: &[], choices, project_field: None };

/// The project's chosen rating table, when it names one that exists.
pub fn chosen(doc: &Document) -> Option<StyleRef> {
    let r = crate::defaults::resolve(doc, &crate::defaults::rating::TABLE, Scope::project());
    StyleRef::parse(&r.text()).filter(|id| super::exists(doc, WORD, id))
}

/// The stored rows of table `id`: the project's record, else the standard's; within the cap.
pub fn rows_text<'a>(doc: &'a Document, id: &StyleRef) -> &'a str {
    for layer in [StyleLayer::Project, StyleLayer::Standard] {
        if let Some(v) = super::record(doc, layer, WORD, id).and_then(|r| r.fields.get(ROWS))
            && v.len() <= dataset::MAX_BYTES
        {
            return v.as_str();
        }
    }
    ""
}

/// The source text of table `id` (empty when none was given).
pub fn source(doc: &Document, id: &StyleRef) -> String {
    super::field_of(doc, WORD, id, SOURCE, None).map(|r| r.text()).unwrap_or_default()
}

/// A rating table read: its ID, name, source and rows.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub id: StyleRef,
    pub name: String,
    pub source: String,
    pub data: Dataset,
}

/// Read table `id`.
pub fn read(doc: &Document, id: &StyleRef) -> Table {
    Table { id: id.clone(), name: super::name(doc, WORD, id), source: source(doc, id), data: dataset::parse(rows_text(doc, id)) }
}

/// The project's chosen table, read.
pub fn chosen_table(doc: &Document) -> Option<Table> {
    chosen(doc).map(|id| read(doc, &id))
}
