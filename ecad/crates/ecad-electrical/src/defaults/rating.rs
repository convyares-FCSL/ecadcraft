//! The defaults: the `rating` area. Standard and project levels;
//! a cable's own `rating.ambient`, `rating.method`, `rating.voltage`, `rating.system` and
//! `rating.vd_limit` (and a conductor's `rating.bundling`) are read by [`crate::circuits`] from the
//! object itself, since the registry has no cable level.

use ecad_model::registry::data::Slot;
use ecad_model::registry::{Entry, Kind, Level, ProjectSource};

use ecad_model::objects::devices::DeviceGroup;

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
pub(crate) const BASES: &[&str] = &["datasheet", "table", "manual"];

fn bundling_or_none(s: &str) -> bool {
    s.trim().eq_ignore_ascii_case("none") || crate::devices::ratings::parse::bundling(s).is_some()
}

pub(crate) static WIRE_BASIS: Entry = entry("rating.wire_basis", "Panel wire basis", Kind::Choice(BASES));
pub(crate) static CABLE_BASIS: Entry = entry("rating.cable_basis", "Field cable basis", Kind::Choice(BASES));
pub(crate) static PANEL_TEMPERATURE: Entry = entry("rating.panel_temperature", "Panel temperature (\u{b0}C)", Kind::Number { min: -40.0, max: 90.0 });
pub(crate) static AMBIENT: Entry = entry("rating.ambient", "Field ambient (\u{b0}C)", Kind::Number { min: -40.0, max: 90.0 });
/// The project's rating table: none until the user enters or imports one.
pub(crate) static TABLE: Entry = entry("rating.table", "Rating table", Kind::Style(&crate::styles::ratingtable::CATALOGUE));
/// Not set: × 1.00 is used, with a warning.
pub(crate) static BUNDLING: Entry = entry("rating.bundling", "Bundling", Kind::Parsed { max_chars: 32, parse: bundling_or_none });
pub(crate) static METHOD: Entry = entry("rating.method", "Installation method", Kind::Choice(&crate::devices::ratings::dataset::METHODS));
pub(crate) static VOLTAGE: Entry = entry("rating.voltage", "Supply voltage (V)", Kind::Number { min: 1.0, max: 1000.0 });
pub(crate) static SYSTEM: Entry = entry("rating.system", "Supply system", Kind::Choice(<SupplySystem as ecad_model::Word>::WORDS));
pub(crate) static VD_LIMIT: Entry = entry("rating.vd_limit", "Volt-drop limit (%)", Kind::Number { min: 0.5, max: 20.0 });
/// How far a motor's rated voltage may be from its supply's before Checks flags it.
pub(crate) static SUPPLY_TOLERANCE: Entry = entry("rating.supply_tolerance", "Motor voltage tolerance (%)", Kind::Number { min: 0.0, max: 50.0 });

/// The field words of a rating-line template for `group`: words of its fields, separated by
/// commas, each once, in the order shown; `None` when one is not a field of the group.
pub(crate) fn template_words(group: DeviceGroup, text: &str) -> Option<Vec<&'static str>> {
    let fields: &[ecad_model::objects::devices::RatingField] = group.rating_fields();
    let mut out: Vec<&'static str> = Vec::new();
    for w in text.split(',').map(str::trim) {
        let f = fields.iter().find(|f| f.word.eq_ignore_ascii_case(w))?;
        if out.contains(&f.word) {
            return None;
        }
        out.push(f.word);
    }
    (!out.is_empty()).then_some(out)
}

fn template_power_supply(s: &str) -> bool {
    template_words(DeviceGroup::PowerSupply, s).is_some()
}
fn template_protection(s: &str) -> bool {
    template_words(DeviceGroup::Protection, s).is_some()
}
fn template_switching(s: &str) -> bool {
    template_words(DeviceGroup::Switching, s).is_some()
}
fn template_sensor(s: &str) -> bool {
    template_words(DeviceGroup::Sensor, s).is_some()
}
fn template_motor(s: &str) -> bool {
    template_words(DeviceGroup::Motor, s).is_some()
}
fn template_load(s: &str) -> bool {
    template_words(DeviceGroup::Load, s).is_some()
}
fn template_indicator(s: &str) -> bool {
    template_words(DeviceGroup::Indicator, s).is_some()
}

// The rating line on the drawing: which of a group's fields it shows, in order. Built in: the
// plate and design fields; a datasheet field shows when a template adds it.
pub(crate) static TEMPLATE_POWER_SUPPLY: Entry =
    entry("rating.template.power_supply", "Rating line: power supply", Kind::Parsed { max_chars: 200, parse: template_power_supply });
pub(crate) static TEMPLATE_PROTECTION: Entry =
    entry("rating.template.protection", "Rating line: protection", Kind::Parsed { max_chars: 200, parse: template_protection });
pub(crate) static TEMPLATE_SWITCHING: Entry =
    entry("rating.template.switching", "Rating line: switching device", Kind::Parsed { max_chars: 200, parse: template_switching });
pub(crate) static TEMPLATE_SENSOR: Entry =
    entry("rating.template.sensor", "Rating line: sensor", Kind::Parsed { max_chars: 200, parse: template_sensor });
pub(crate) static TEMPLATE_MOTOR: Entry =
    entry("rating.template.motor", "Rating line: motor", Kind::Parsed { max_chars: 200, parse: template_motor });
pub(crate) static TEMPLATE_LOAD: Entry = entry("rating.template.load", "Rating line: load", Kind::Parsed { max_chars: 200, parse: template_load });
pub(crate) static TEMPLATE_INDICATOR: Entry =
    entry("rating.template.indicator", "Rating line: indicator", Kind::Parsed { max_chars: 200, parse: template_indicator });

/// The template entry of `group`, when its devices have a rating line.
pub(crate) fn template_entry(group: DeviceGroup) -> Option<&'static Entry> {
    match group {
        DeviceGroup::PowerSupply => Some(&TEMPLATE_POWER_SUPPLY),
        DeviceGroup::Protection => Some(&TEMPLATE_PROTECTION),
        DeviceGroup::Switching => Some(&TEMPLATE_SWITCHING),
        DeviceGroup::Sensor => Some(&TEMPLATE_SENSOR),
        DeviceGroup::Motor => Some(&TEMPLATE_MOTOR),
        DeviceGroup::Load => Some(&TEMPLATE_LOAD),
        DeviceGroup::Indicator => Some(&TEMPLATE_INDICATOR),
        DeviceGroup::TerminalStrip | DeviceGroup::Connector | DeviceGroup::PlcRack | DeviceGroup::PlcModule | DeviceGroup::General => None,
    }
}

/// Every entry of this file, in the order Standards ▸ Defaults lists them.
pub(crate) static ENTRIES: &[&Entry] = &[
    &WIRE_BASIS,
    &CABLE_BASIS,
    &PANEL_TEMPERATURE,
    &AMBIENT,
    &TABLE,
    &BUNDLING,
    &METHOD,
    &VOLTAGE,
    &SYSTEM,
    &VD_LIMIT,
    &SUPPLY_TOLERANCE,
    &TEMPLATE_POWER_SUPPLY,
    &TEMPLATE_PROTECTION,
    &TEMPLATE_SWITCHING,
    &TEMPLATE_SENSOR,
    &TEMPLATE_MOTOR,
    &TEMPLATE_LOAD,
    &TEMPLATE_INDICATOR,
];
