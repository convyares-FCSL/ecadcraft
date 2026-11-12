//! Lane W's defaults (032, wires and nets). Owned by lane W: append `pub static` entries and list them in
//! [`ENTRIES`]. Lane T created this file empty (design 031 §3.5) and does not edit it again.
//!
//! Read them through `wire_cmds::join_by_default`, `wires::label_rule`, `wires::label_rule_word` and
//! `wires::label_min_length`, not by key. The label rule's built-ins are `ecad-model`'s constants
//! (`wiring::DEFAULT_LABEL_RULE`, `DEFAULT_LABEL_MIN_LENGTH`), referenced here, never copied.

use super::{Entry, Kind, Level, ProjectSource, Value};

/// Standard and project.
const SP: &[Level] = &[Level::Standard, Level::Project];

/// `wire.join`: whether `wire` joins ends that meet other wires when `join` is not given, and the
/// Wire tool's starting state (design 032 §2.3, §2.4; owner Q3: on).
pub static WIRE_JOIN: Entry = Entry {
    key: "wire.join",
    label: "Join wire ends that meet",
    group: "Wires",
    brief: "032",
    kind: Kind::Choice(&["on", "off"]),
    builtin: Value::Choice("on"),
    levels: SP,
    project: ProjectSource::Property,
};

/// The label rule words, as registered.
const LABEL_RULES: &[&str] = &[ecad_model::wiring::LABEL_RULE_NET, ecad_model::wiring::LABEL_RULE_EACH];

/// The project's label rule as stored in `settings.wiring`, as a registered word; `None` while it
/// is the default (so it resolves as the built-in).
fn label_rule_setting(s: &ecad_model::ProjectSettings) -> Option<Value<'_>> {
    let r = s.wiring.label_rule.trim();
    if r.eq_ignore_ascii_case(ecad_model::wiring::DEFAULT_LABEL_RULE) {
        return None;
    }
    LABEL_RULES.iter().find(|w| w.eq_ignore_ascii_case(r)).map(|w| Value::Choice(w))
}

/// `wiring.label_rule`: which wires get a label, `net` (one per net per page) or `each` (design
/// 032 §4.1; owner Q6: `net` for every project).
pub static WIRING_LABEL_RULE: Entry = Entry {
    key: "wiring.label_rule",
    label: "Wire labels",
    group: "Wire labels",
    brief: "032",
    kind: Kind::Choice(LABEL_RULES),
    builtin: Value::Choice(ecad_model::wiring::DEFAULT_LABEL_RULE),
    levels: SP,
    project: ProjectSource::Settings(label_rule_setting),
};

/// The project's shortest labelled wire as stored, `None` while it is the default.
fn label_min_length_setting(s: &ecad_model::ProjectSettings) -> Option<Value<'_>> {
    let v = s.wiring.label_min_length;
    ((v - ecad_model::wiring::DEFAULT_LABEL_MIN_LENGTH).abs() > f64::EPSILON).then_some(Value::Length(v))
}

/// `wiring.label_min_length`: with `each`, the shortest labelled wire, millimetres (design 032
/// §4.1).
pub static WIRING_LABEL_MIN_LENGTH: Entry = Entry {
    key: "wiring.label_min_length",
    label: "Shortest labelled wire",
    group: "Wire labels",
    brief: "032",
    kind: Kind::Length { min: *ecad_model::wiring::LABEL_MIN_LENGTHS.start(), max: *ecad_model::wiring::LABEL_MIN_LENGTHS.end() },
    builtin: Value::Length(ecad_model::wiring::DEFAULT_LABEL_MIN_LENGTH),
    levels: SP,
    project: ProjectSource::Settings(label_min_length_setting),
};

/// Every entry of this file, in the order `defaults` lists them.
pub static ENTRIES: &[&Entry] = &[&WIRE_JOIN, &WIRING_LABEL_RULE, &WIRING_LABEL_MIN_LENGTH];
