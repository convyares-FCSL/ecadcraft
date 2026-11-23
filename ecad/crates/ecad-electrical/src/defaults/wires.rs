//! The wire and net defaults: append `pub static` entries and list them in
//! [`ENTRIES`].
//!
//! Read them through `crate::wires::commands::wires::join_by_default`, `wires::label_rule::label_rule`, `wires::label_rule::label_rule_word` and
//! `wires::label_rule::label_min_length`, not by key. Their built-ins are data (`defaults/wires.toml`, brief;
//! a project's chosen value is its `settings.wiring` field, absent when it chose none.

use crate::defaults::{Entry, Kind, Level, ProjectSource, Slot, Value};

/// Standard and project.
const SP: &[Level] = &[Level::Standard, Level::Project];

/// `wire.join`: whether `wire` joins ends that meet other wires when `join` is not given, and the
/// Wire tool's starting state (on).
pub static WIRE_JOIN: Entry = Entry {
    key: "wire.join",
    label: "Join wire ends that meet",
    group: "Wires",
    brief: "032",
    kind: Kind::Flag,
    slot: Slot::new(),
    levels: SP,
    project: ProjectSource::Property,
};

/// The label rule words, as registered.
const LABEL_RULES: &[&str] = &[ecad_model::connectivity::wiring::LABEL_RULE_NET, ecad_model::connectivity::wiring::LABEL_RULE_EACH];

/// The label rule the project chose in `settings.wiring`, as a registered word; `None` when it
/// chose none, so it resolves as the built-in.
fn label_rule_setting(s: &ecad_model::ProjectSettings) -> Option<Value<'_>> {
    let r = s.wiring.label_rule.as_deref()?.trim();
    LABEL_RULES.iter().find(|w| w.eq_ignore_ascii_case(r)).map(|w| Value::Choice(w))
}

/// `wiring.label_rule`: which wires get a label, `net` (one per net per page) or `each` (design
/// `net` for every project).
pub static WIRING_LABEL_RULE: Entry = Entry {
    key: "wiring.label_rule",
    label: "Wire labels",
    group: "Wire labels",
    brief: "032",
    kind: Kind::Choice(LABEL_RULES),
    slot: Slot::new(),
    levels: SP,
    project: ProjectSource::Settings(label_rule_setting),
};

/// The shortest labelled wire the project chose, `None` when it chose none.
fn label_min_length_setting(s: &ecad_model::ProjectSettings) -> Option<Value<'_>> {
    s.wiring.label_min_length.map(Value::Length)
}

/// `wiring.label_min_length`: with `each`, the shortest labelled wire, millimetres
pub static WIRING_LABEL_MIN_LENGTH: Entry = Entry {
    key: "wiring.label_min_length",
    label: "Shortest labelled wire",
    group: "Wire labels",
    brief: "032",
    kind: Kind::Length {
        min: *ecad_model::connectivity::wiring::LABEL_MIN_LENGTHS.start(),
        max: *ecad_model::connectivity::wiring::LABEL_MIN_LENGTHS.end(),
    },
    slot: Slot::new(),
    levels: SP,
    project: ProjectSource::Settings(label_min_length_setting),
};

/// Every entry of this file, in the order `defaults` lists them.
pub static ENTRIES: &[&Entry] = &[&WIRE_JOIN, &WIRING_LABEL_RULE, &WIRING_LABEL_MIN_LENGTH];
