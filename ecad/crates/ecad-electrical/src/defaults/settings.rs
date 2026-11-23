//! Entries: project settings with a built-in.
//!
//! The settings' built-ins live only in ecad-model's data (`ecad-model/defaults/settings.toml`;
//! the registry reads them through `ecad_model::naming::builtins::get`, never a copy. Their project level is the
//! settings field, absent when the project chose none. They have no standard level: ecad-model
//! reads the project's choice or the built-in, never the standard. The project sheet's built-in
//! (`page.sheet`) is ecad-model's alone: a sheet is structured, not one value.

use ecad_model::ProjectSettings;
use ecad_model::naming::builtins as mb;

use crate::defaults::{Entry, Kind, Level, ProjectSource, Slot, Value};

const P: &[Level] = &[Level::Project];

fn tag_padding(s: &ProjectSettings) -> Option<Value<'_>> {
    s.tag_padding.map(|n| Value::Number(f64::from(n)))
}

fn wire_digits(s: &ProjectSettings) -> Option<Value<'_>> {
    s.wiring.digits.map(|n| Value::Number(f64::from(n)))
}

fn colour_style(s: &ProjectSettings) -> Option<Value<'_>> {
    s.wiring.colour_style.map(|c| Value::Choice(ecad_model::naming::words::Word::word(c)))
}

fn page_number_step(s: &ProjectSettings) -> Option<Value<'_>> {
    s.page_number_step.map(|n| Value::Number(f64::from(n)))
}

fn reference_style(s: &ProjectSettings) -> Option<Value<'_>> {
    s.reference_style.map(|r| Value::Choice(r.word()))
}

/// Digits an automatic tag number is padded to.
pub static TAG_PADDING: Entry = Entry {
    key: mb::TAG_PADDING,
    label: "Tag number digits",
    group: "Tags",

    kind: Kind::Number { min: 1.0, max: ecad_model::naming::tag::MAX_PADDING as f64 },
    slot: Slot::new(),
    levels: P,
    project: ProjectSource::Settings(tag_padding),
};

/// Digits a wire number is padded to.
pub static WIRE_DIGITS: Entry = Entry {
    key: mb::WIRE_DIGITS,
    label: "Wire number digits",
    group: "Wire numbers",

    kind: Kind::Number { min: 1.0, max: ecad_model::connectivity::wiring::MAX_DIGITS as f64 },
    slot: Slot::new(),
    levels: P,
    project: ProjectSource::Settings(wire_digits),
};

/// How label colours are written: IEC 60757 codes or three letters.
pub static COLOUR_STYLE: Entry = Entry {
    key: mb::COLOUR_STYLE,
    label: "Wire colour codes",
    group: "Wire labels",

    kind: Kind::Choice(ecad_model::connectivity::wiring::COLOUR_STYLE_WORDS),
    slot: Slot::new(),
    levels: P,
    project: ProjectSource::Settings(colour_style),
};

/// What a new page's number adds to the one before it.
pub static PAGE_NUMBER_STEP: Entry = Entry {
    key: mb::PAGE_NUMBER_STEP,
    label: "Page number step",
    group: "Pages",

    kind: Kind::Number { min: 1.0, max: mb::MAX_PAGE_NUMBER_STEP as f64 },
    slot: Slot::new(),
    levels: P,
    project: ProjectSource::Settings(page_number_step),
};

/// How cross-references name a place.
pub static REFERENCE_STYLE: Entry = Entry {
    key: mb::REFERENCE_STYLE,
    label: "Reference style",
    group: "Pages",

    kind: Kind::Choice(ecad_model::ReferenceStyle::WORDS),
    slot: Slot::new(),
    levels: P,
    project: ProjectSource::Settings(reference_style),
};

/// Every entry of this file.
pub static ENTRIES: &[&Entry] = &[&TAG_PADDING, &WIRE_DIGITS, &COLOUR_STYLE, &PAGE_NUMBER_STEP, &REFERENCE_STYLE];
