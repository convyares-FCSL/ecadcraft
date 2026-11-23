//! The look's built-in values as data: the TOML files under `ecad-drawing-style/defaults/`,
//! embedded at build time ([`FILES`]), read through [`DATA`] (the loader is
//! [`ecad_model::registry::data`]).

use ecad_model::registry::Entry;
use ecad_model::registry::data::DataSet;

use crate::defaults::{styles, text};

/// The built-in files: one line per area.
#[rustfmt::skip]
pub static FILES: &[(&str, &str)] = &[
    ("text.toml", include_str!("../../defaults/text.toml")),
    ("styles.toml", include_str!("../../defaults/styles.toml")),
];

/// Every area's entries: one line per area.
#[rustfmt::skip]
pub static LISTS: &[&[&Entry]] = &[
    text::ENTRIES,
    styles::ENTRIES,
];

/// The look's data set: [`FILES`] for the entries of [`LISTS`], the wire label height (the
/// project field of the `wire_label` text style, not listed in the registry) and the look's style
/// catalogues. It has no layout values.
pub static DATA: DataSet = DataSet::new(FILES, LISTS, &[&text::WIRE_LABEL_HEIGHT], crate::catalogues::CATALOGUES, &[], "text.toml", "styles.toml");
