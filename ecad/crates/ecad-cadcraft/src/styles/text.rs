//! The text style catalogue (`text`, design 050 §4.1).
//!
//! One built-in style per 031 text kind, with the kind's key word as ID, carrying exactly 031's
//! and 040's built-in cap height and weight, so nothing on a page moves when 050a lands. Fields:
//! `font` (`project` = the registry's `text.font`), `height` (cap height), `weight` and `colour`
//! (046 Q8: `none` = ink; drawn on screen and in a colour print; a mono print is ink).
//!
//! The wire label's height stays where 031 put it, `settings.wiring.label_height`, read as the
//! project field of the `wire_label` style ([`project_field`]).

use ecad_model::{Document, StyleRef};

use super::{Builtin, Catalogue, Field};
use crate::defaults::{Kind, Value};

/// A font field's value meaning "the project font" (`text.font`).
pub const PROJECT_FONT: &str = "project";

static FIELDS: &[Field] = &[
    Field { key: "font", label: "Font", kind: Kind::Text { max_chars: 64 }, builtin: Value::Text(PROJECT_FONT), over: None },
    Field { key: "height", label: "Cap height", kind: Kind::Length { min: 0.5, max: 10.0 }, builtin: Value::Length(2.5), over: None },
    Field { key: "weight", label: "Weight", kind: Kind::Choice(&["regular", "bold"]), builtin: Value::Choice("regular"), over: None },
    Field { key: "colour", label: "Colour", kind: Kind::Parsed { max_chars: 16, parse: super::is_colour }, builtin: Value::Text("none"), over: None },
];

const fn style(id: &'static str, name: &'static str, values: &'static [(&'static str, Value<'static>)]) -> Builtin {
    Builtin { id, name, values }
}

const R: Value<'static> = Value::Choice("regular");
const B: Value<'static> = Value::Choice("bold");

/// 031 §3.2 and 040: each kind's cap height and weight.
static BUILTINS: &[Builtin] = &[
    style("tag", "Tag", &[("height", Value::Length(2.5)), ("weight", B)]),
    style("point_label", "Connection point label", &[("height", Value::Length(1.5)), ("weight", R)]),
    style("description", "Description", &[("height", Value::Length(1.6)), ("weight", R)]),
    style("part_number", "Part number", &[("height", Value::Length(1.6)), ("weight", R)]),
    style("continuation_name", "Continuation name", &[("height", Value::Length(2.0)), ("weight", R)]),
    style("reference", "Reference", &[("height", Value::Length(2.0)), ("weight", R)]),
    // Brief 049 (design 049 §7).
    style("continuation_caption", "Continuation caption", &[("height", Value::Length(2.5)), ("weight", R)]),
    style("wire_label", "Wire label", &[("height", Value::Length(ecad_model::wiring::DEFAULT_LABEL_HEIGHT)), ("weight", R)]),
    style("cable_text", "Cable text", &[("height", Value::Length(1.6)), ("weight", R)]),
    style("location", "Location label", &[("height", Value::Length(1.6)), ("weight", R)]),
    style("title_value", "Title block value", &[("height", Value::Length(2.5)), ("weight", R)]),
    style("title_caption", "Title block caption", &[("height", Value::Length(1.8)), ("weight", R)]),
    style("title_main", "Title", &[("height", Value::Length(3.5)), ("weight", B)]),
    style("grid", "Grid label", &[("height", Value::Length(3.0)), ("weight", R)]),
    style("table", "Table text", &[("height", Value::Length(2.5)), ("weight", R)]),
    style("table_header", "Table header", &[("height", Value::Length(2.5)), ("weight", B)]),
    style("free", "Free text", &[("height", Value::Length(2.5)), ("weight", R)]),
    style("title_page", "Title block page number", &[("height", Value::Length(7.0)), ("weight", R)]),
    style("title_rev", "Title block revision", &[("height", Value::Length(5.0)), ("weight", R)]),
    style("table_heading", "Table heading", &[("height", Value::Length(3.5)), ("weight", R)]),
    style("cover", "Cover title", &[("height", Value::Length(10.0)), ("weight", R)]),
    // Brief 051 (design 051 §5, FP1): the pack writes channel functions at about the tag's size
    // and addresses between pin and tag size.
    style("channel_text", "PLC channel text", &[("height", Value::Length(2.5)), ("weight", R)]),
    style("address", "PLC address", &[("height", Value::Length(2.0)), ("weight", R)]),
];

/// The wire label style's height at project level: `settings.wiring.label_height` (031 §3.4),
/// one value with one way to set it.
fn project_field<'a>(doc: &'a Document, id: &StyleRef, field: &str) -> Option<Value<'a>> {
    match (id, field) {
        (StyleRef::Builtin(w), "height") if w == "wire_label" => crate::defaults::text::wire_label_height(&doc.settings),
        _ => None,
    }
}

fn choices() -> Vec<&'static crate::defaults::Entry> {
    crate::defaults::styles::TEXT.to_vec()
}

/// The text style catalogue.
pub static CATALOGUE: Catalogue = Catalogue {
    word: "text",
    plural: "Text styles",
    one: "Text style",
    fields: FIELDS,
    builtins: BUILTINS,
    choices,
    project_field: Some(project_field),
};

/// What a text kind's choice styles, for counting users.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Noun {
    Devices,
    Wires,
    Pages,
}

/// What objects choice `key` (`text.<kind>.style`) styles.
pub fn noun(key: &str) -> Noun {
    match key.strip_prefix("text.").and_then(|k| k.strip_suffix(".style")) {
        Some("tag" | "point_label" | "description" | "part_number" | "continuation_name" | "channel_text" | "address") => Noun::Devices,
        Some("wire_label") => Noun::Wires,
        _ => Noun::Pages,
    }
}
