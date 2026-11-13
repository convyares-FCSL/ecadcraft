//! Lane D's style choices (brief 050a, design 050 §4, FP5): registry entries whose value is a
//! style ID. Which style an object uses resolves through the levels like any entry; what the style
//! says is [`crate::styles::field`].
//!
//! - `text.<kind>.style`: S P O (object: the component or conductor owning the text). No page
//!   level (owner answer 4: a page picks no text style).
//! - `location.style`: S P O (object: the location box).
//! - `print.frame.style`: S P G (a frame belongs to the sheet).

use super::{Entry, Kind, Level, ProjectSource, Value};

const GROUP: &str = "Styles";
const BRIEF: &str = "050";
const SPO: &[Level] = &[Level::Standard, Level::Project, Level::Object];
const SPG: &[Level] = &[Level::Standard, Level::Project, Level::Page];

const fn text(key: &'static str, label: &'static str, builtin: &'static str) -> Entry {
    Entry {
        key,
        label,
        group: GROUP,
        brief: BRIEF,
        kind: Kind::Style("text"),
        builtin: Value::Text(builtin),
        levels: SPO,
        project: ProjectSource::Property,
    }
}

pub static TAG: Entry = text("text.tag.style", "Tag style", "tag");
pub static POINT_LABEL: Entry = text("text.point_label.style", "Connection point label style", "point_label");
pub static DESCRIPTION: Entry = text("text.description.style", "Description style", "description");
pub static PART_NUMBER: Entry = text("text.part_number.style", "Part number style", "part_number");
pub static CONTINUATION_NAME: Entry = text("text.continuation_name.style", "Continuation name style", "continuation_name");
pub static REFERENCE: Entry = text("text.reference.style", "Reference style", "reference");
pub static WIRE_LABEL: Entry = text("text.wire_label.style", "Wire label style", "wire_label");
pub static CABLE_TEXT: Entry = text("text.cable_text.style", "Cable text style", "cable_text");
pub static LOCATION_LABEL: Entry = text("text.location.style", "Location label style", "location");
pub static TITLE_VALUE: Entry = text("text.title_value.style", "Title block value style", "title_value");
pub static TITLE_CAPTION: Entry = text("text.title_caption.style", "Title block caption style", "title_caption");
pub static TITLE_MAIN: Entry = text("text.title_main.style", "Title style", "title_main");
pub static GRID: Entry = text("text.grid.style", "Grid label style", "grid");
pub static TABLE: Entry = text("text.table.style", "Table text style", "table");
pub static TABLE_HEADER: Entry = text("text.table_header.style", "Table header style", "table_header");
pub static FREE: Entry = text("text.free.style", "Free text style", "free");
pub static TITLE_PAGE: Entry = text("text.title_page.style", "Title block page number style", "title_page");
pub static TITLE_REV: Entry = text("text.title_rev.style", "Title block revision style", "title_rev");
pub static TABLE_HEADING: Entry = text("text.table_heading.style", "Table heading style", "table_heading");
pub static COVER: Entry = text("text.cover.style", "Cover title style", "cover");

/// Every text style choice, in the order of `TextKind::ALL`.
pub static TEXT: &[&Entry] = &[
    &TAG,
    &POINT_LABEL,
    &DESCRIPTION,
    &PART_NUMBER,
    &CONTINUATION_NAME,
    &REFERENCE,
    &WIRE_LABEL,
    &CABLE_TEXT,
    &LOCATION_LABEL,
    &TITLE_VALUE,
    &TITLE_CAPTION,
    &TITLE_MAIN,
    &GRID,
    &TABLE,
    &TABLE_HEADER,
    &FREE,
    &TITLE_PAGE,
    &TITLE_REV,
    &TABLE_HEADING,
    &COVER,
];

/// A location box's style.
pub static LOCATION: Entry = Entry {
    key: "location.style",
    label: "Location box style",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Style("location"),
    builtin: Value::Text("box"),
    levels: SPO,
    project: ProjectSource::Property,
};

/// A page's frame and title block style.
pub static FRAME: Entry = Entry {
    key: "print.frame.style",
    label: "Frame style",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Style("frame"),
    builtin: Value::Text("sheet"),
    levels: SPG,
    project: ProjectSource::Property,
};

/// Every entry of this file, in the order `defaults` lists them.
pub static ENTRIES: &[&Entry] = &[
    &TAG,
    &POINT_LABEL,
    &DESCRIPTION,
    &PART_NUMBER,
    &CONTINUATION_NAME,
    &REFERENCE,
    &WIRE_LABEL,
    &CABLE_TEXT,
    &LOCATION_LABEL,
    &TITLE_VALUE,
    &TITLE_CAPTION,
    &TITLE_MAIN,
    &GRID,
    &TABLE,
    &TABLE_HEADER,
    &FREE,
    &TITLE_PAGE,
    &TITLE_REV,
    &TABLE_HEADING,
    &COVER,
    &LOCATION,
    &FRAME,
];
