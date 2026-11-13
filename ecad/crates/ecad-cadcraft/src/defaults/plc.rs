//! Brief 045's defaults (PLC racks and cards, design 045 §12): area `plc`, group "PLC". Owned by
//! lane D.
//!
//! # Semantics
//! - Rack entries (`plc.slots`, `plc.slot.first`) may be overridden on the rack device (object
//!   level), card entries (`plc.text.caption.show`) on the card device.
//! - `plc.<type>.channels`: how many channels a card of that type has unless the card says
//!   (`plc.channels`, a card property, [`crate::plc::rack::CHANNELS_KEY`]).
//! - `plc.card.caption`: the caption under a card body when the card is in a rack; `{rack}` is the
//!   rack's tag, `{slot}` the slot number, `{slot:02}` the slot padded to two digits. Nothing else
//!   in braces is accepted, so a stored pattern can never name something the code does not fill.
//! - `plc.<key>.prefix`: the tag prefix of this brief's new starters (034's pattern, project level
//!   in the project's `tag_prefixes`, keyed by starter key).
//!
//! Hostile-input bounds (999 slots, 256 channels) are constants beside the code they guard
//! ([`crate::plc::rack`]), not entries.

use ecad_model::ProjectSettings;

use super::{Entry, Kind, Level, ProjectSource, Value};

const GROUP: &str = "PLC";
const BRIEF: &str = "045";
/// A standard (reserved), the project and the object (rack or card) may override.
const SPO: &[Level] = &[Level::Standard, Level::Project, Level::Object];
/// A standard (reserved) and the project may override.
const SP: &[Level] = &[Level::Standard, Level::Project];

const ON_OFF: &[&str] = &["on", "off"];

pub static SLOTS: Entry = Entry {
    key: "plc.slots",
    label: "Slots in a rack",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Number { min: 1.0, max: 999.0 },
    builtin: Value::Number(32.0),
    levels: SPO,
    project: ProjectSource::Property,
};

pub static SLOT_FIRST: Entry = Entry {
    key: "plc.slot.first",
    label: "Number of a rack's first slot",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Choice(&["0", "1"]),
    builtin: Value::Choice("1"),
    levels: SPO,
    project: ProjectSource::Property,
};

macro_rules! channels {
    ($($name:ident = $code:literal, $builtin:literal, $label:literal;)*) => {
        $(
            pub static $name: Entry = Entry {
                key: concat!("plc.", $code, ".channels"),
                label: concat!("Channels of a ", $label, " card"),
                group: GROUP,
                brief: BRIEF,
                kind: Kind::Number { min: 0.0, max: 256.0 },
                builtin: Value::Number($builtin),
                levels: SP,
                project: ProjectSource::Property,
            };
        )*

        /// The channel-count entry of a card type, by its registry code (`di`, `ai`…).
        pub fn channels_entry(code: &str) -> Option<&'static Entry> {
            match code {
                $($code => Some(&$name),)*
                _ => None,
            }
        }
    };
}

channels! {
    CHANNELS_DI = "di", 16.0, "DI";
    CHANNELS_DQ = "dq", 16.0, "DQ";
    CHANNELS_AI = "ai", 4.0, "AI";
    CHANNELS_AO = "ao", 4.0, "AO";
    CHANNELS_RTD = "rtd", 4.0, "RTD";
    CHANNELS_IOL = "iol", 4.0, "IO-Link";
}

/// Whether `t` is a caption pattern: text whose only `{…}` parts are `{rack}`, `{slot}` and
/// `{slot:02}`, with no stray brace.
pub fn caption_ok(t: &str) -> bool {
    let mut rest = t;
    // At most one pass over the (length-capped) text.
    while let Some(i) = rest.find(['{', '}']) {
        let after = rest.get(i..).unwrap_or_default();
        if after.starts_with('}') {
            return false;
        }
        let Some(end) = after.find('}') else { return false };
        let name = after.get(1..end).unwrap_or_default();
        if !matches!(name, "rack" | "slot" | "slot:02") {
            return false;
        }
        rest = after.get(end + 1..).unwrap_or_default();
    }
    true
}

pub static CAPTION: Entry = Entry {
    key: "plc.card.caption",
    label: "Card caption",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Parsed { max_chars: 40, parse: caption_ok },
    builtin: Value::Text("{rack} slot {slot}"),
    levels: SP,
    project: ProjectSource::Property,
};

pub static CAPTION_SHOW: Entry = Entry {
    key: "plc.text.caption.show",
    label: "Show the card caption",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Choice(ON_OFF),
    builtin: Value::Choice("on"),
    levels: SPO,
    project: ProjectSource::Property,
};

/// A prefix as Tag Settings accepts it.
fn prefix_ok(t: &str) -> bool {
    crate::devices::valid_prefix(t).is_some()
}

macro_rules! prefixes {
    ($($name:ident = $key:literal, $prefix:literal, $symbol:literal;)*) => {
        $(
            pub static $name: Entry = Entry {
                key: concat!("plc.", $key, ".prefix"),
                label: concat!("Tag prefix: ", $symbol),
                group: "Tag prefixes",
                brief: BRIEF,
                kind: Kind::Parsed { max_chars: crate::devices::MAX_PREFIX_LETTERS + 1, parse: prefix_ok },
                builtin: Value::Text($prefix),
                levels: SP,
                project: ProjectSource::Settings({
                    fn read(s: &ProjectSettings) -> Option<Value<'_>> {
                        s.tag_prefixes.get($key).map(|p| Value::Text(p.as_str()))
                    }
                    read
                }),
            };
        )*

        /// The prefix entry of this brief's starter `key`.
        pub fn prefix_entry(key: &str) -> Option<&'static Entry> {
            match key {
                $($key => Some(&$name),)*
                _ => None,
            }
        }

        /// The prefix entries of this brief's starters.
        pub static PREFIXES: &[&Entry] = &[$(&$name),*];
    };
}

prefixes! {
    PLCIM = "plcim", "A", "PLC head module";
    PLCPM = "plcpm", "A", "PLC power module";
    PLCCM = "plccm", "A", "PLC communication module";
}

/// Every entry of this file, in the order `defaults` lists them.
pub static ENTRIES: &[&Entry] = &[
    &SLOTS,
    &SLOT_FIRST,
    &CHANNELS_DI,
    &CHANNELS_DQ,
    &CHANNELS_AI,
    &CHANNELS_AO,
    &CHANNELS_RTD,
    &CHANNELS_IOL,
    &CAPTION,
    &CAPTION_SHOW,
    &PLCIM,
    &PLCPM,
    &PLCCM,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caption_patterns_name_only_rack_and_slot() {
        for ok in ["{rack} slot {slot}", "{rack} Slot{slot:02}", "Card", "{slot}"] {
            assert!(caption_ok(ok), "{ok}");
        }
        for bad in ["{rack}{nope}", "{rack", "rack}", "{}", "{{rack}}", "{slot:03}"] {
            assert!(!caption_ok(bad), "{bad}");
        }
        assert!(CAPTION.parse(&"x".repeat(10_000)).is_none());
    }
}
