//! Rating tables: the `ratingtable` catalogue.
//!
//! A rating table is a user's data set of ratings and factors, entered by hand or imported from a
//! CSV they are licensed to use. It lives in the project or in a standard, like any style. There
//! are **no built-in entries, and ECAD ships no values**. The project chooses one
//! with the registry key `rating.table` ([`crate::defaults::rating::TABLE`]).
//!
//! Fields: `source` (what the user says the values come from) and [`ROWS`] (the CSV, read by
//! [`crate::devices::ratings::dataset`]). The rows field may hold up to
//! [`crate::devices::ratings::dataset::MAX_BYTES`] ([`crate::styles::field_cap`]); it is read here, raw, not through
//! [`crate::styles::field`] (it holds line breaks, which a text field refuses).

use ecad_model::objects::styles::StyleLayer;
use ecad_model::{Document, StyleRef};

use ecad_model::objects::styles::{RatingTable, StyleBody};

use crate::CmdError;
use crate::defaults::{Kind, Scope, Slot, Value};
use crate::devices::ratings::dataset::{self, Dataset};
use crate::styles::{Catalogue, Field};

/// The field holding the CSV rows.
pub const ROWS: &str = "rows";
/// The field holding the source.
pub const SOURCE: &str = "source";
/// The longest source text.
pub(crate) const MAX_SOURCE_CHARS: usize = 200;

fn any_text(_: &str) -> bool {
    true
}

static FIELDS: [Field; 2] = [
    Field { key: SOURCE, label: "Source", kind: Kind::Parsed { max_chars: MAX_SOURCE_CHARS, parse: any_text }, slot: Slot::new(), over: None },
    // Read raw by `rows_text`; listed so `style` knows the key.
    Field { key: ROWS, label: "Rows", kind: Kind::Parsed { max_chars: 1, parse: any_text }, slot: Slot::new(), over: None },
];

fn choices() -> Vec<&'static crate::defaults::Entry> {
    vec![&crate::defaults::rating::TABLE]
}

/// The rating table catalogue: no built-in entries.
pub static CATALOGUE: Catalogue = Catalogue {
    catalogue: ecad_model::StyleCatalogue::RatingTable,
    plural: "Rating tables",
    one: "Rating table",
    fields: &FIELDS,
    builtins: &[],
    choices,
    project_field: None,
};

/// The project's chosen rating table, when it names one that exists.
pub fn chosen(doc: &Document) -> Option<StyleRef> {
    let r = crate::defaults::resolve(doc, &crate::defaults::rating::TABLE, Scope::project());
    StyleRef::parse(&r.text()).filter(|id| crate::styles::exists(doc, ecad_model::StyleCatalogue::RatingTable, id))
}

/// The stored rows of table `id`: the project's record, else the standard's; within the cap.
pub fn rows_text<'a>(doc: &'a Document, id: &StyleRef) -> &'a str {
    for layer in [StyleLayer::Project, StyleLayer::Standard] {
        if let Some(v) = crate::styles::record(doc, layer, ecad_model::StyleCatalogue::RatingTable, id).and_then(|r| match &r.body {
            StyleBody::RatingTable(t) => t.rows.as_ref(),
            _ => None,
        }) && v.as_str().len() <= dataset::MAX_BYTES
        {
            return v.as_str();
        }
    }
    ""
}

/// The source text of table `id` (empty when none was given).
pub fn source(doc: &Document, id: &StyleRef) -> String {
    crate::styles::field_of(doc, ecad_model::StyleCatalogue::RatingTable, id, SOURCE, None).map(|r| r.text()).unwrap_or_default()
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
    Table {
        id: id.clone(),
        name: crate::styles::name(doc, ecad_model::StyleCatalogue::RatingTable, id),
        source: source(doc, id),
        data: dataset::parse(rows_text(doc, id)),
    }
}

/// The project's chosen table, read.
pub(crate) fn chosen_table(doc: &Document) -> Option<Table> {
    chosen(doc).map(|id| read(doc, &id))
}

/// The value of field `key` that `t` sets, if it sets one.
pub fn get<'a>(t: &'a RatingTable, key: &str) -> Option<Value<'a>> {
    match key {
        SOURCE => t.source.as_ref().map(|v| Value::Text(v.as_str())),
        // The rows are read by [`rows_text`], never as a field value (a table, not one value).
        _ => None,
    }
}

/// Set (`Some`) or clear (`None`) field `key` of `t` from its text. Rows written with CR LF line
/// ends are stored with LF.
///
/// # Errors
/// An unknown field, or text the field does not take, in words.
pub fn set(t: &mut RatingTable, key: &str, value: Option<&str>) -> Result<(), CmdError> {
    let bad = |e: ecad_model::naming::text::TextError| CmdError::refused(format!("{key}: {e}"));
    match (key, value) {
        (SOURCE, None) => t.source = None,
        (ROWS, None) => t.rows = None,
        (SOURCE, Some(v)) => t.source = Some(ecad_model::naming::text::Text::new(v).map_err(bad)?),
        (ROWS, Some(v)) => t.rows = Some(ecad_model::naming::text::LongText::new(&v.replace("\r\n", "\n")).map_err(bad)?),
        _ => return Err(CmdError::refused(format!("Rating tables have no field {key:?}."))),
    }
    Ok(())
}
