//! The text style catalogue (`text`).
//!
//! One built-in style per text kind, with the kind's key word as ID, carrying the kind's
//! built-in cap height and weight, so choosing a built-in style moves nothing on a page. Fields:
//! `font` (`project` = the registry's `text.font`), `height` (cap height), `weight` and `colour`
//! (`none` = ink; drawn on screen and in a colour print; a mono print is ink).
//!
//! The wire label's height stays where it always was, `settings.wiring.label_height`, read as the
//! project field of the `wire_label` style (`project_field`).

use ecad_model::geometry::units::Mm;
use ecad_model::objects::styles::text::{Font, StyleColour, TextStyle};
use ecad_model::{Document, StyleRef, Word};

use ecad_electrical::CmdError;
use ecad_model::registry::data::Slot;
use ecad_model::registry::styles::{Builtin, Catalogue, Field};
use ecad_model::registry::{Kind, Value};

/// A font field's value meaning "the project font" (`text.font`).
pub(crate) const PROJECT_FONT: &str = "project";

static FIELDS: [Field; 4] = [
    Field { key: "font", label: "Font", kind: Kind::Text { max_chars: 64 }, slot: Slot::new(&crate::defaults::data::DATA), over: None },
    Field {
        key: "height",
        label: "Cap height",
        kind: Kind::Length { min: 0.5, max: 10.0 },
        slot: Slot::new(&crate::defaults::data::DATA),
        over: None,
    },
    Field {
        key: "weight",
        label: "Weight",
        kind: Kind::Choice(<ecad_model::objects::styles::text::Weight as ecad_model::Word>::WORDS),
        slot: Slot::new(&crate::defaults::data::DATA),
        over: None,
    },
    Field {
        key: "colour",
        label: "Colour",
        kind: Kind::Parsed { max_chars: 16, parse: crate::catalogues::is_colour },
        slot: Slot::new(&crate::defaults::data::DATA),
        over: None,
    },
];

const fn style(id: &'static str, name: &'static str) -> Builtin {
    Builtin { id, name, slot: Slot::new(&crate::defaults::data::DATA) }
}

/// Each kind's cap height and weight.
static BUILTINS: [Builtin; 23] = [
    style("tag", "Tag"),
    style("point_label", "Connection point label"),
    style("description", "Description"),
    style("part_number", "Part number"),
    style("continuation_name", "Continuation name"),
    style("reference", "Reference"),
    style("continuation_caption", "Continuation caption"),
    style("wire_label", "Wire label"),
    style("cable_text", "Cable text"),
    style("location", "Location label"),
    style("title_value", "Title block value"),
    style("title_caption", "Title block caption"),
    style("title_main", "Title"),
    style("grid", "Grid label"),
    style("table", "Table text"),
    style("table_header", "Table header"),
    style("free", "Free text"),
    style("title_page", "Title block page number"),
    style("title_rev", "Title block revision"),
    style("table_heading", "Table heading"),
    style("cover", "Cover title"),
    // The pack writes channel functions at about the tag's size
    // and addresses between pin and tag size.
    style("channel_text", "PLC channel text"),
    style("address", "PLC address"),
];

/// The wire label style's height at project level: `settings.wiring.label_height`,
/// one value with one way to set it.
fn project_field<'a>(doc: &'a Document, id: &StyleRef, field: &str) -> Option<Value<'a>> {
    match (id, field) {
        (StyleRef::Builtin(w), "height") if w == "wire_label" => crate::defaults::text::wire_label_height(doc.settings()),
        _ => None,
    }
}

fn choices() -> Vec<&'static ecad_model::registry::Entry> {
    crate::defaults::styles::TEXT.to_vec()
}

/// The text style catalogue.
pub static CATALOGUE: Catalogue = Catalogue {
    catalogue: ecad_model::StyleCatalogue::Text,
    plural: "Text styles",
    one: "Text style",
    fields: &FIELDS,
    builtins: &BUILTINS,
    choices,
    project_field: Some(project_field),
    body,
    typed: None,
    adjust: Some(wire_label_height),
    set: set_body,
    implicit,
    reach,
};

/// The value field `key` of a text style record holds.
fn body<'a>(_: &'a ecad_model::Document, b: &'a ecad_model::objects::styles::StyleBody, key: &str) -> Option<Value<'a>> {
    match b {
        ecad_model::objects::styles::StyleBody::Text(t) => get(t, key),
        _ => None,
    }
}

/// The `wire_label` text style's height is the entry `text.wire_label.height` (one value, one
/// place).
fn wire_label_height(b: &ecad_model::registry::styles::Builtin, out: &mut Vec<(&'static str, Value<'static>)>) {
    if b.id == "wire_label" {
        out.retain(|(k, _)| *k != "height");
        out.push(("height", crate::defaults::text::WIRE_LABEL_HEIGHT.builtin()));
    }
}

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

/// The value of field `key` that `t` sets, if it sets one.
pub fn get<'a>(t: &'a TextStyle, key: &str) -> Option<Value<'a>> {
    match key {
        "font" => t.font.as_ref().map(|f| Value::Text(f.as_str())),
        "height" => t.height.map(|h| Value::Length(h.0)),
        "weight" => t.weight.map(|w| Value::Choice(w.word())),
        "colour" => t.colour.map(Value::Colour),
        _ => None,
    }
}

/// Set (`Some`) or clear (`None`) field `key` of `t` from its text, as the `style` command
/// checked it.
///
/// # Errors
/// An unknown field, or text the field does not take, in words.
pub fn set(t: &mut TextStyle, key: &str, value: Option<&str>) -> Result<(), CmdError> {
    let bad = |what: &str| CmdError::refused(format!("{key} takes {what}, not {:?}.", value.unwrap_or_default()));
    match (key, value) {
        ("font", None) => t.font = None,
        ("height", None) => t.height = None,
        ("weight", None) => t.weight = None,
        ("colour", None) => t.colour = None,
        ("font", Some(v)) => t.font = Some(Font::parse(v).ok_or_else(|| bad("a font name"))?),
        ("height", Some(v)) => t.height = Some(Mm(v.trim().parse::<f64>().ok().filter(|x| x.is_finite()).ok_or_else(|| bad("a length"))?)),
        ("weight", Some(v)) => t.weight = Some(Word::parse(v).ok_or_else(|| bad("regular or bold"))?),
        ("colour", Some(v)) => t.colour = Some(StyleColour::parse(v).ok_or_else(|| bad("none, a colour code or #rrggbb"))?),
        _ => return Err(CmdError::refused(format!("Text styles have no field {key:?}."))),
    }
    Ok(())
}

/// [`set`] on a record body of this catalogue.
fn set_body(b: &mut ecad_model::objects::styles::StyleBody, key: &str, value: Option<&str>) -> Result<(), ecad_model::registry::styles::Refused> {
    match b {
        ecad_model::objects::styles::StyleBody::Text(x) => set(x, key, value).map_err(|e| ecad_model::registry::styles::Refused(e.to_string())),
        other => Err(ecad_electrical::styles::wrong_body(other, CATALOGUE.one)),
    }
}

/// The objects of `entry`'s noun with no text style of their own when the project's choice is
/// `id`.
fn implicit(doc: &Document, entry: &ecad_model::registry::Entry, id: &StyleRef) -> usize {
    if ecad_model::registry::styles::chosen(doc, entry, ecad_model::registry::Scope::project()) != *id {
        return 0;
    }
    let own = |p: &ecad_model::Properties| ecad_model::registry::styles::chooses(doc, entry, p);
    match noun(entry.key) {
        Noun::Devices => doc.devices().values().filter(|x| !own(&x.properties)).count(),
        Noun::Wires => doc.wires().values().filter(|x| !own(&x.display)).count(),
        Noun::Pages => doc.pages().len(),
    }
}

/// A text style a location style labels with reaches every box; otherwise the boxes whose own
/// style labels with it.
fn reach(doc: &Document, _: &ecad_model::Op, id: &StyleRef) -> ecad_model::registry::styles::Reach {
    use crate::catalogues::location;
    if location::labels_with(doc, id) {
        return ecad_model::registry::styles::Reach::Every;
    }
    let mut out = std::collections::BTreeSet::new();
    for (a, x) in doc.aids() {
        if x.as_box().is_some() && location::own_style(doc, *a).is_some_and(|s| location::draw_of(doc, s).label_style == *id) {
            out.extend(doc.representations_of(ecad_model::Subject::Aid(*a)).map(|(_, r)| r.page));
        }
    }
    ecad_model::registry::styles::Reach::Also(out)
}
