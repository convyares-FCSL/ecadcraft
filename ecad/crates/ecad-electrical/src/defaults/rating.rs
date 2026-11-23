//! The defaults: the `rating` area. Standard and project levels;
//! a cable's own `rating.ambient`, `rating.method`, `rating.voltage`, `rating.system` and
//! `rating.vd_limit` (and a conductor's `rating.bundling`) are read by [`crate::circuits`] from the
//! object itself, since the registry has no cable level.

use ecad_model::registry::data::Slot;
use ecad_model::registry::{Entry, Kind, Level, ProjectSource};

ecad_model::word_enum! {
    /// The supply system a rating is checked for.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SupplySystem {
        #[strum(serialize = "ac1")]
        Ac1,
        #[strum(serialize = "ac3")]
        Ac3,
        Dc,
    }
}

const GROUP: &str = "Rating";
const SP: &[Level] = &[Level::Standard, Level::Project];

const fn entry(key: &'static str, label: &'static str, kind: Kind) -> Entry {
    Entry { key, label, group: GROUP, kind, slot: Slot::new(&crate::defaults::data::DATA), levels: SP, project: ProjectSource::Property }
}

/// The bases a subject may use.
pub const BASES: &[&str] = &["datasheet", "table", "manual"];

fn bundling_or_none(s: &str) -> bool {
    s.trim().eq_ignore_ascii_case("none") || crate::devices::ratings::parse::bundling(s).is_some()
}

pub static WIRE_BASIS: Entry = entry("rating.wire_basis", "Panel wire basis", Kind::Choice(BASES));
pub static CABLE_BASIS: Entry = entry("rating.cable_basis", "Field cable basis", Kind::Choice(BASES));
pub static PANEL_TEMPERATURE: Entry = entry("rating.panel_temperature", "Panel temperature (\u{b0}C)", Kind::Number { min: -40.0, max: 90.0 });
pub static AMBIENT: Entry = entry("rating.ambient", "Field ambient (\u{b0}C)", Kind::Number { min: -40.0, max: 90.0 });
/// The project's rating table: none until the user enters or imports one.
pub static TABLE: Entry = entry("rating.table", "Rating table", Kind::Style(&crate::styles::ratingtable::CATALOGUE));
/// Not set: × 1.00 is used, with a warning.
pub static BUNDLING: Entry = entry("rating.bundling", "Bundling", Kind::Parsed { max_chars: 32, parse: bundling_or_none });
pub static METHOD: Entry = entry("rating.method", "Installation method", Kind::Choice(&crate::devices::ratings::dataset::METHODS));
pub static VOLTAGE: Entry = entry("rating.voltage", "Supply voltage (V)", Kind::Number { min: 1.0, max: 1000.0 });
pub static SYSTEM: Entry = entry("rating.system", "Supply system", Kind::Choice(<SupplySystem as ecad_model::Word>::WORDS));
pub static VD_LIMIT: Entry = entry("rating.vd_limit", "Volt-drop limit (%)", Kind::Number { min: 0.5, max: 20.0 });

/// Every entry of this file, in the order Standards ▸ Defaults lists them.
pub static ENTRIES: &[&Entry] = &[&WIRE_BASIS, &CABLE_BASIS, &PANEL_TEMPERATURE, &AMBIENT, &TABLE, &BUNDLING, &METHOD, &VOLTAGE, &SYSTEM, &VD_LIMIT];
