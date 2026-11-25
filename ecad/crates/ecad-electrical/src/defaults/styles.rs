//! The wire category choice: a registry entry whose value is a wire style's ID (S P O; the
//! built-in `none` is no category). What the style says is [`ecad_model::registry::styles::field`].

use ecad_model::registry::data::Slot;
use ecad_model::registry::{Entry, Kind, Level, ProjectSource};

const GROUP: &str = "Styles";
const SPO: &[Level] = &[Level::Standard, Level::Project, Level::Object];

/// A conductor's wire category: S P O; the built-in `none` is no category.
pub(crate) static WIRE_CATEGORY: Entry = Entry {
    key: "wire.category",
    label: "Wire category",
    group: GROUP,

    kind: Kind::Style(&crate::styles::wire::CATALOGUE),
    slot: Slot::new(&crate::defaults::data::DATA),
    levels: SPO,
    project: ProjectSource::Property,
};

/// Every entry of this file, in the order `defaults` lists them.
pub(crate) static ENTRIES: &[&Entry] = &[&WIRE_CATEGORY];
