//! The electrical rules' built-in values as data: the TOML files under `ecad-electrical/defaults/`,
//! one per area, embedded at build time ([`FILES`]), read through [`DATA`] (the loader is
//! [`ecad_model::registry::data`]).

use ecad_model::registry::data::DataSet;
use ecad_model::registry::{Entry, Kind};

use crate::defaults::{
    bom, connectors, continuations, pages, plc, print, rating, settings, strip_pages, styles, symbols, terminal_blocks, text, wires,
};

/// The built-in files: one line per area.
#[rustfmt::skip]
pub static FILES: &[(&str, &str)] = &[
    ("text.toml", include_str!("../../defaults/text.toml")),
    ("wires.toml", include_str!("../../defaults/wires.toml")),
    ("print.toml", include_str!("../../defaults/print.toml")),
    ("symbols.toml", include_str!("../../defaults/symbols.toml")),
    ("terminal_blocks.toml", include_str!("../../defaults/terminal_blocks.toml")),
    ("connectors.toml", include_str!("../../defaults/connectors.toml")),
    ("strip_pages.toml", include_str!("../../defaults/strip_pages.toml")),
    ("bom.toml", include_str!("../../defaults/bom.toml")),
    ("styles.toml", include_str!("../../defaults/styles.toml")),
    ("continuations.toml", include_str!("../../defaults/continuations.toml")),
    ("plc.toml", include_str!("../../defaults/plc.toml")),
    ("pages.toml", include_str!("../../defaults/pages.toml")),
    ("rating.toml", include_str!("../../defaults/rating.toml")),
];

/// Every area's entries: one line per area, so an area added adds one line.
#[rustfmt::skip]
pub static LISTS: &[&[&Entry]] = &[
    text::ENTRIES,
    wires::ENTRIES,
    print::ENTRIES,
    symbols::ENTRIES,
    terminal_blocks::ENTRIES,
    connectors::ENTRIES,
    strip_pages::ENTRIES,
    bom::ENTRIES,
    styles::ENTRIES,
    symbols::BUILDER_ENTRIES,
    continuations::ENTRIES,
    plc::ENTRIES,
    pages::ENTRIES,
    rating::ENTRIES,
    settings::ENTRIES,
];

/// Built-in layout values no level overrides: `[layout]` keys of the files, with the
/// check each value must pass. Not registry entries (every entry has a project level).
#[rustfmt::skip]
pub static LAYOUT: &[(&str, Kind)] = &[
    ("page.report.row_height", Kind::Length { min: 3.0, max: 20.0 }),
    ("page.report.header_height", Kind::Length { min: 3.0, max: 30.0 }),
    // The cover's lines as fractions of the `Cover` style height.
    ("front.cover.block_title", Kind::Number { min: 0.05, max: 1.0 }),
    ("front.cover.body", Kind::Number { min: 0.05, max: 1.0 }),
    ("front.cover.column_head", Kind::Number { min: 0.05, max: 1.0 }),
    ("front.cover.customer", Kind::Number { min: 0.05, max: 1.0 }),
];

/// The electrical rules' data set: [`FILES`] for the entries of [`LISTS`], the wire label height
/// (the project field of the `wire_label` text style, not listed in the registry), the style
/// catalogues and [`LAYOUT`].
pub static DATA: DataSet = DataSet::new(FILES, LISTS, &[&text::WIRE_LABEL_HEIGHT], crate::styles::CATALOGUES, LAYOUT, "pages.toml", "styles.toml");
