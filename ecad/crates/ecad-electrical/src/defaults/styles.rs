//! Lane D's style choices (brief 050a, design 050 §4, FP5): registry entries whose value is a
//! style ID. Which style an object uses resolves through the levels like any entry; what the style
//! says is [`crate::styles::field`].
//!
//! - `text.<kind>.style`: S P O (object: the component or conductor owning the text). No page
//!   level (owner answer 4: a page picks no text style).
//! - `location.style`: S P O (object: the location box).
//! - `print.frame.style`: S P G (a frame belongs to the sheet).

use crate::defaults::{Entry, Kind, Level, ProjectSource, Slot};

const GROUP: &str = "Styles";
const BRIEF: &str = "050";
const SPO: &[Level] = &[Level::Standard, Level::Project, Level::Object];
const SPG: &[Level] = &[Level::Standard, Level::Project, Level::Page];

const fn text(key: &'static str, label: &'static str) -> Entry {
    Entry {
        key,
        label,
        group: GROUP,
        brief: BRIEF,
        kind: Kind::Style(ecad_model::StyleCatalogue::Text),
        slot: Slot::new(),
        levels: SPO,
        project: ProjectSource::Property,
    }
}

pub static TAG: Entry = text("text.tag.style", "Tag style");
pub static POINT_LABEL: Entry = text("text.point_label.style", "Connection point label style");
pub static DESCRIPTION: Entry = text("text.description.style", "Description style");
pub static PART_NUMBER: Entry = text("text.part_number.style", "Part number style");
pub static CONTINUATION_NAME: Entry = text("text.continuation_name.style", "Continuation name style");
pub static REFERENCE: Entry = text("text.reference.style", "Reference style");
/// Brief 049's continuation caption (design 049 §7), a built-in text style since 050a.
pub static CONTINUATION_CAPTION: Entry = text("text.continuation_caption.style", "Continuation caption style");
pub static WIRE_LABEL: Entry = text("text.wire_label.style", "Wire label style");
pub static CABLE_TEXT: Entry = text("text.cable_text.style", "Cable text style");
pub static LOCATION_LABEL: Entry = text("text.location.style", "Location label style");
pub static TITLE_VALUE: Entry = text("text.title_value.style", "Title block value style");
pub static TITLE_CAPTION: Entry = text("text.title_caption.style", "Title block caption style");
pub static TITLE_MAIN: Entry = text("text.title_main.style", "Title style");
pub static GRID: Entry = text("text.grid.style", "Grid label style");
pub static TABLE: Entry = text("text.table.style", "Table text style");
pub static TABLE_HEADER: Entry = text("text.table_header.style", "Table header style");
pub static FREE: Entry = text("text.free.style", "Free text style");
pub static TITLE_PAGE: Entry = text("text.title_page.style", "Title block page number style");
pub static TITLE_REV: Entry = text("text.title_rev.style", "Title block revision style");
pub static TABLE_HEADING: Entry = text("text.table_heading.style", "Table heading style");
pub static COVER: Entry = text("text.cover.style", "Cover title style");
// Brief 051 (design 051 §5, FP1).
pub static CHANNEL_TEXT: Entry = text("text.channel_text.style", "PLC channel text style");
pub static ADDRESS: Entry = text("text.address.style", "PLC address style");

/// Every text style choice, in the order of `TextKind::ALL`.
pub static TEXT: &[&Entry] = &[
    &TAG,
    &POINT_LABEL,
    &DESCRIPTION,
    &PART_NUMBER,
    &CONTINUATION_NAME,
    &REFERENCE,
    &CONTINUATION_CAPTION,
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
    &CHANNEL_TEXT,
    &ADDRESS,
];

/// A location box's style.
pub static LOCATION: Entry = Entry {
    key: "location.style",
    label: "Location box style",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Style(ecad_model::StyleCatalogue::Location),
    slot: Slot::new(),
    levels: SPO,
    project: ProjectSource::Property,
};

/// A page's frame and title block style.
pub static FRAME: Entry = Entry {
    key: "print.frame.style",
    label: "Frame style",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Style(ecad_model::StyleCatalogue::Frame),
    slot: Slot::new(),
    levels: SPG,
    project: ProjectSource::Property,
};

/// A conductor's wire category (brief 050b, FP5): S P O; the built-in `none` is no category.
pub static WIRE_CATEGORY: Entry = Entry {
    key: "wire.category",
    label: "Wire category",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Style(ecad_model::StyleCatalogue::Wire),
    slot: Slot::new(),
    levels: SPO,
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
    &CONTINUATION_CAPTION,
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
    &CHANNEL_TEXT,
    &ADDRESS,
    &LOCATION,
    &FRAME,
    &WIRE_CATEGORY,
];
