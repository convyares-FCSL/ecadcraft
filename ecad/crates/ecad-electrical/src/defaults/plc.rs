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

use super::{Entry, Kind, Level, ProjectSource, Slot, Value};

ecad_model::word_enum! {
    /// The number of a card's first channel.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ChannelFirst {
        #[strum(serialize = "0")]
        Zero,
        #[strum(serialize = "1")]
        One,
    }
}
ecad_model::word_enum! {
    /// How a card's connection points are named.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PlcPins {
        Role,
        Channel,
    }
}
ecad_model::word_enum! {
    /// How PLC addresses are written.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum AddressStyle {
        #[strum(serialize = "byte.bit")]
        ByteBit,
        Iec,
        Percent,
        Slot,
        None,
    }
}
ecad_model::word_enum! {
    /// How a card is drawn on its page.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CardRotation {
        #[strum(serialize = "90")]
        Quarter,
        #[strum(serialize = "0")]
        Upright,
    }
}
ecad_model::word_enum! {
    /// A digital output's safe state.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SafeState {
        Off,
        On,
        Hold,
    }
}

const GROUP: &str = "PLC";
const BRIEF: &str = "045";
/// A standard (reserved), the project and the object (rack or card) may override.
const SPO: &[Level] = &[Level::Standard, Level::Project, Level::Object];
/// A standard (reserved) and the project may override.
const SP: &[Level] = &[Level::Standard, Level::Project];

pub static SLOTS: Entry = Entry {
    key: "plc.slots",
    label: "Slots in a rack",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Number { min: 1.0, max: 999.0 },
    slot: Slot::new(),
    levels: SPO,
    project: ProjectSource::Property,
};

pub static SLOT_FIRST: Entry = Entry {
    key: "plc.slot.first",
    label: "Number of a rack's first slot",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Choice(<ChannelFirst as ecad_model::Word>::WORDS),
    slot: Slot::new(),
    levels: SPO,
    project: ProjectSource::Property,
};

macro_rules! channels {
    ($($name:ident = $code:literal, $label:literal;)*) => {
        $(
            pub static $name: Entry = Entry {
                key: concat!("plc.", $code, ".channels"),
                label: concat!("Channels of a ", $label, " card"),
                group: GROUP,
                brief: BRIEF,
                kind: Kind::Number { min: 0.0, max: 256.0 },
                slot: Slot::new(),
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
    CHANNELS_DI = "di", "DI";
    CHANNELS_DQ = "dq", "DQ";
    CHANNELS_AI = "ai", "AI";
    CHANNELS_AO = "ao", "AO";
    CHANNELS_RTD = "rtd", "RTD";
    CHANNELS_IOL = "iol", "IO-Link";
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
    slot: Slot::new(),
    levels: SP,
    project: ProjectSource::Property,
};

pub static CAPTION_SHOW: Entry = Entry {
    key: "plc.text.caption.show",
    label: "Show the card caption",
    group: GROUP,
    brief: BRIEF,
    kind: Kind::Flag,
    slot: Slot::new(),
    levels: SPO,
    project: ProjectSource::Property,
};

/// A prefix as Tag Settings accepts it.
fn prefix_ok(t: &str) -> bool {
    crate::devices::valid_prefix(t).is_some()
}

macro_rules! prefixes {
    ($($name:ident = $var:ident $key:literal, $symbol:literal;)*) => {
        $(
            pub static $name: Entry = Entry {
                key: concat!("plc.", $key, ".prefix"),
                label: concat!("Tag prefix: ", $symbol),
                group: "Tag prefixes",
                brief: BRIEF,
                kind: Kind::Parsed { max_chars: crate::devices::MAX_PREFIX_LETTERS + 1, parse: prefix_ok },
                slot: Slot::new(),
                levels: SP,
                project: ProjectSource::Settings({
                    fn read(s: &ProjectSettings) -> Option<Value<'_>> {
                        s.starter_prefix($key).map(|p| Value::Text(p.as_str()))
                    }
                    read
                }),
            };
        )*

        /// The prefix entry of this brief's starter `key`.
        pub fn prefix_entry(key: crate::symbols::StarterKey) -> Option<&'static Entry> {
            match key {
                $(crate::symbols::StarterKey::$var => Some(&$name),)*
                _ => None,
            }
        }

        /// The prefix entries of this brief's starters.
        pub static PREFIXES: &[&Entry] = &[$(&$name),*];
    };
}

prefixes! {
    PLCIM = Plcim "plcim", "PLC head module";
    PLCPM = Plcpm "plcpm", "PLC power module";
    PLCCM = Plccm "plccm", "PLC communication module";
    CHDI = Chdi "chdi", "PLC digital input channel";
    CHDQ = Chdq "chdq", "PLC digital output channel";
    CHAI = Chai "chai", "PLC analogue input channel";
    CHAO = Chao "chao", "PLC analogue output channel";
    CHRTD = Chrtd "chrtd", "PLC RTD channel";
    CHIOL = Chiol "chiol", "PLC IO-Link port";
}

pub static CHANNEL_FIRST: Entry = Entry {
    key: "plc.channel.first",
    label: "Number of a card's first channel",
    group: GROUP,
    brief: "045",
    kind: Kind::Choice(<ChannelFirst as ecad_model::Word>::WORDS),
    slot: Slot::new(),
    levels: SPO,
    project: ProjectSource::Property,
};

pub static PINS: Entry = Entry {
    key: "plc.pins",
    label: "Pin layout of a card",
    group: GROUP,
    brief: "045",
    kind: Kind::Choice(<PlcPins as ecad_model::Word>::WORDS),
    slot: Slot::new(),
    levels: SPO,
    project: ProjectSource::Property,
};

pub static ADDRESS_STYLE: Entry = Entry {
    key: "plc.address.style",
    label: "Address style",
    group: GROUP,
    brief: "045",
    kind: Kind::Choice(<AddressStyle as ecad_model::Word>::WORDS),
    slot: Slot::new(),
    levels: SPO,
    project: ProjectSource::Property,
};

pub static START_INPUT: Entry = Entry {
    key: "plc.address.start.input",
    label: "First input byte of a rack",
    group: GROUP,
    brief: "045",
    kind: Kind::Number { min: 0.0, max: 65535.0 },
    slot: Slot::new(),
    levels: SPO,
    project: ProjectSource::Property,
};

pub static START_OUTPUT: Entry = Entry {
    key: "plc.address.start.output",
    label: "First output byte of a rack",
    group: GROUP,
    brief: "045",
    kind: Kind::Number { min: 0.0, max: 65535.0 },
    slot: Slot::new(),
    levels: SPO,
    project: ProjectSource::Property,
};

pub static ANALOG_BYTES: Entry = Entry {
    key: "plc.address.analog.bytes",
    label: "Bytes per analogue channel",
    group: GROUP,
    brief: "045",
    kind: Kind::Number { min: 1.0, max: 16.0 },
    slot: Slot::new(),
    levels: SPO,
    project: ProjectSource::Property,
};

pub static IOL_BYTES: Entry = Entry {
    key: "plc.address.iol.bytes",
    label: "Bytes per IO-Link port",
    group: GROUP,
    brief: "045",
    kind: Kind::Number { min: 1.0, max: 32.0 },
    slot: Slot::new(),
    levels: SPO,
    project: ProjectSource::Property,
};

pub static SPARE_TEXT: Entry = Entry {
    key: "plc.spare.text",
    label: "Text under a spare channel",
    group: GROUP,
    brief: "045",
    kind: Kind::Text { max_chars: 16 },
    slot: Slot::new(),
    levels: SP,
    project: ProjectSource::Property,
};

pub static CHANNEL_SHOW: Entry = Entry {
    key: "plc.text.channel.show",
    label: "Show channel numbers",
    group: GROUP,
    brief: "045",
    kind: Kind::Flag,
    slot: Slot::new(),
    levels: SPO,
    project: ProjectSource::Property,
};

pub static ADDRESS_SHOW: Entry = Entry {
    key: "plc.text.address.show",
    label: "Show addresses on channels",
    group: GROUP,
    brief: "045",
    kind: Kind::Flag,
    slot: Slot::new(),
    levels: SPO,
    project: ProjectSource::Property,
};

pub static ROTATION: Entry = Entry {
    key: "plc.text.channel.rotation",
    label: "Channel description rotation",
    group: GROUP,
    brief: "045",
    kind: Kind::Choice(<CardRotation as ecad_model::Word>::WORDS),
    slot: Slot::new(),
    levels: SPO,
    project: ProjectSource::Property,
};

pub static XREF_TABLE: Entry = Entry {
    key: "plc.xref.table",
    label: "Channel table under a card body",
    group: GROUP,
    brief: "045",
    kind: Kind::Flag,
    slot: Slot::new(),
    levels: SPO,
    project: ProjectSource::Property,
};

// ---------------------------------------------------------------- Brief 051 (design 051 §6)

/// Whether `t` is a page-title pattern: text whose only `{…}` parts are `{tag}`, `{type}`,
/// `{channels}`, `{rack}`, `{slot}` and `{slot:02}`, with no stray brace.
pub fn page_title_ok(t: &str) -> bool {
    let mut rest = t;
    while let Some(i) = rest.find(['{', '}']) {
        let after = rest.get(i..).unwrap_or_default();
        if after.starts_with('}') {
            return false;
        }
        let Some(end) = after.find('}') else { return false };
        let name = after.get(1..end).unwrap_or_default();
        if !matches!(name, "tag" | "type" | "channels" | "rack" | "slot" | "slot:02") {
            return false;
        }
        rest = after.get(end + 1..).unwrap_or_default();
    }
    true
}

pub static FRAME_SHOW: Entry = Entry {
    key: "plc.frame.show",
    label: "Draw a frame around a card's cells",
    group: GROUP,
    brief: "051",
    kind: Kind::Flag,
    slot: Slot::new(),
    levels: SPO,
    project: ProjectSource::Property,
};

pub static PAGE_TITLE: Entry = Entry {
    key: "plc.page.title",
    label: "Title of a card's page (Make its page)",
    group: GROUP,
    brief: "051",
    kind: Kind::Parsed { max_chars: 40, parse: page_title_ok },
    slot: Slot::new(),
    levels: SP,
    project: ProjectSource::Property,
};

/// Brief 056 (design 056 §4.4, §11): what a digital output does when the PLC stops.
pub static SAFE_DQ: Entry = Entry {
    key: "plc.safe.dq",
    label: "Safe state of a digital output",
    group: GROUP,
    brief: "056",
    kind: Kind::Choice(<SafeState as ecad_model::Word>::WORDS),
    slot: Slot::new(),
    levels: SPO,
    project: ProjectSource::Property,
};

/// Brief 056: what an analogue output does when the PLC stops: `hold`, `min`, `max` or a value.
pub static SAFE_AO: Entry = Entry {
    key: "plc.safe.ao",
    label: "Safe state of an analogue output",
    group: GROUP,
    brief: "056",
    kind: Kind::Text { max_chars: 16 },
    slot: Slot::new(),
    levels: SPO,
    project: ProjectSource::Property,
};

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
    // Brief 045b.
    &CHANNEL_FIRST,
    &PINS,
    &ADDRESS_STYLE,
    &START_INPUT,
    &START_OUTPUT,
    &ANALOG_BYTES,
    &IOL_BYTES,
    &SPARE_TEXT,
    &CHANNEL_SHOW,
    &ADDRESS_SHOW,
    &ROTATION,
    &XREF_TABLE,
    &CHDI,
    &CHDQ,
    &CHAI,
    &CHAO,
    &CHRTD,
    &CHIOL,
    // Brief 051.
    &FRAME_SHOW,
    &PAGE_TITLE,
    // Brief 056.
    &SAFE_DQ,
    &SAFE_AO,
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
