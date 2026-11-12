//! Lane W's defaults (032, wires and nets). Owned by lane W: append `pub static` entries and list them in
//! [`ENTRIES`]. Lane T created this file empty (design 031 §3.5) and does not edit it again.
//!
//! Read them through `wire_cmds::join_by_default` and `wires::label_rule`, not by key.
//!
//! `wiring.label_rule` and `wiring.label_min_length` (design 032 §4.5) are stored in
//! `settings.wiring` with their defaults in `ecad-model` (`wiring::DEFAULT_LABEL_RULE`,
//! `DEFAULT_LABEL_MIN_LENGTH`). They are registered here once the registry reads word-valued
//! settings and the `wiring.` area (031, overseer ruling 2026-11-12).

use super::{Entry, Kind, Level, ProjectSource, Value};

/// `wire.join`: whether `wire` joins ends that meet other wires when `join` is not given, and the
/// Wire tool's starting state (design 032 §2.3, §2.4; owner Q3: on).
pub static WIRE_JOIN: Entry = Entry {
    key: "wire.join",
    label: "Join wire ends that meet",
    group: "Wires",
    brief: "032",
    kind: Kind::Word(&["on", "off"]),
    builtin: Value::Word("on"),
    levels: &[Level::Standard, Level::Project],
    project: ProjectSource::Property,
};

/// Every entry of this file, in the order `defaults` lists them.
pub static ENTRIES: &[&Entry] = &[&WIRE_JOIN];
