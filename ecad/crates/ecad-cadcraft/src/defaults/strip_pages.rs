//! Brief 046's defaults (graphical terminal-strip pages, design 046 §7.2). Owned by lane U: append
//! `pub static` entries and list them in [`ENTRIES`].
//!
//! Read them through [`crate::strip_page::Geometry::of`] and [`crate::strip_page::style_of`], not
//! by key. Guards (most cable lanes, leads per side, far devices) are constants in
//! [`crate::strip_page`], not entries.

use super::{Entry, Kind, Level, ProjectSource, Value};

/// Project only.
const P: &[Level] = &[Level::Project];
/// Project and page.
const PP: &[Level] = &[Level::Project, Level::Page];

const GROUP: &str = "Terminal strip pages";

/// The style words, as registered.
pub const TABLE: &str = "table";
pub const GRAPHIC: &str = "graphic";

/// `terminal.plan.style`: how a `terminals` page draws, the table (028) or the strip graphic
/// (design 046 §2). Page, then project, then `table`.
pub static STYLE: Entry = Entry {
    key: "terminal.plan.style",
    label: "Terminal plan style",
    group: GROUP,
    brief: "046",
    kind: Kind::Choice(&[TABLE, GRAPHIC]),
    builtin: Value::Choice(TABLE),
    levels: PP,
    project: ProjectSource::Property,
};

/// One project-level length entry of the strip layout.
macro_rules! length {
    ($name:ident, $key:literal, $label:literal, $min:literal, $max:literal, $builtin:literal) => {
        #[doc = concat!("`", $key, "`: ", $label, ", millimetres (design 046 §3).")]
        pub static $name: Entry = Entry {
            key: $key,
            label: $label,
            group: GROUP,
            brief: "046",
            kind: Kind::Length { min: $min, max: $max },
            builtin: Value::Length($builtin),
            levels: P,
            project: ProjectSource::Property,
        };
    };
}

length!(PITCH, "strippage.pitch", "Terminal column width", 5.0, 20.0, 9.0);
length!(BODY, "strippage.body", "Terminal body height", 10.0, 40.0, 22.0);
length!(LEAD, "strippage.lead", "Lead length", 10.0, 60.0, 25.0);
length!(BAND_Y, "strippage.band_y", "Strip band from the top", 60.0, 200.0, 110.0);
length!(TAGBOX, "strippage.tagbox", "Strip tag box width", 10.0, 40.0, 18.0);
length!(BAR, "strippage.bar", "Cable bar below the targets", 2.0, 20.0, 6.0);
length!(MARGIN, "strippage.margin", "Right margin of a strip sheet", 0.0, 30.0, 10.0);

/// `strippage.tag_scale`: the strip tag's height as a multiple of the table heading's.
pub static TAG_SCALE: Entry = Entry {
    key: "strippage.tag_scale",
    label: "Strip tag size (× heading)",
    group: GROUP,
    brief: "046",
    kind: Kind::Number { min: 1.0, max: 4.0 },
    builtin: Value::Number(2.0),
    levels: P,
    project: ProjectSource::Property,
};

/// Every entry of this file, in the order `defaults` lists them.
pub static ENTRIES: &[&Entry] = &[&STYLE, &PITCH, &BODY, &LEAD, &BAND_Y, &TAGBOX, &BAR, &MARGIN, &TAG_SCALE];

/// Whether project key `key` changes the strip layout (any `strippage.*` entry).
pub fn is_layout_key(key: &str) -> bool {
    ENTRIES.iter().any(|e| e.key == key && e.key != STYLE.key)
}
