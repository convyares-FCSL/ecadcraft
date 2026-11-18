//! The location and box style catalogue (`location`, design 050 §4.2).
//!
//! The built-in `box` ("Location box") is exactly 024's look: a dashed outline at the wire weight
//! in ink, labelled in the `location` text style. `fill` is a reserved field: it is stored and
//! listed, and drawn as `none` (CADCraft draws no fill behind other entities on a layer yet; the
//! quick start says so).

use ecad_model::{DeviceId, Document, StyleRef};

use super::{Builtin, Catalogue, Field};
use crate::defaults::{Kind, Scope, Slot};

/// The line types a box may take, as the field stores them.
pub const LINE_TYPES: &[&str] = &["dashed", "continuous", "dashdot"];

static FIELDS: [Field; 5] = [
    Field { key: "line_type", label: "Line type", kind: Kind::Choice(LINE_TYPES), slot: Slot::new(), over: None },
    Field { key: "line_weight", label: "Line weight", kind: Kind::Length { min: 0.13, max: 2.11 }, slot: Slot::new(), over: None },
    Field { key: "line_colour", label: "Line colour", kind: Kind::Parsed { max_chars: 16, parse: super::is_colour }, slot: Slot::new(), over: None },
    Field { key: "label_style", label: "Label style", kind: Kind::Style(ecad_model::StyleCatalogue::Text), slot: Slot::new(), over: None },
    Field { key: "fill", label: "Fill (reserved)", kind: Kind::Parsed { max_chars: 16, parse: super::is_colour }, slot: Slot::new(), over: None },
];

static BUILTINS: [Builtin; 1] = [Builtin { id: "box", name: "Location box", slot: Slot::new() }];

fn choices() -> Vec<&'static crate::defaults::Entry> {
    vec![&crate::defaults::styles::LOCATION]
}

/// The location and box style catalogue.
pub static CATALOGUE: Catalogue = Catalogue {
    catalogue: ecad_model::StyleCatalogue::Location,
    plural: "Location boxes",
    one: "Location box style",
    fields: &FIELDS,
    builtins: &BUILTINS,
    choices,
    project_field: None,
};

/// A location box's resolved look.
#[derive(Debug, Clone, PartialEq)]
pub struct BoxLook {
    /// The CADCraft linetype name.
    pub linetype: &'static str,
    /// Hundredths of a millimetre.
    pub lineweight: i16,
    /// `none` (ink), an IEC code or `#rrggbb`.
    pub colour: String,
    /// The text style of its label.
    pub label_style: StyleRef,
}

/// The CADCraft linetype of a `line_type` word.
pub fn linetype(word: &str) -> &'static str {
    match word {
        "continuous" => "CONTINUOUS",
        "dashdot" => "DASHDOT",
        _ => crate::locations::LINETYPE,
    }
}

/// The look of location box `c` (its own `location.style`, else the project's, the standard's,
/// the built-in `box`).
pub fn box_look(doc: &Document, c: DeviceId) -> BoxLook {
    let id = super::chosen(doc, &crate::defaults::styles::LOCATION, Scope::component(c));
    look_of(doc, &id)
}

/// The look of location style `id`.
pub fn look_of(doc: &Document, id: &StyleRef) -> BoxLook {
    let f = |k: &str| super::field_of(doc, ecad_model::StyleCatalogue::Location, id, k, None);
    let lt = f("line_type").map(|r| r.word()).unwrap_or("dashed");
    let mm = f("line_weight").map_or(0.25, |r| r.number());
    let lineweight = (mm * 100.0).round().clamp(0.0, 211.0) as i16;
    let colour = f("line_colour").map_or_else(|| "none".to_string(), |r| r.text());
    let label_style = f("label_style").and_then(|r| StyleRef::parse(&r.text())).unwrap_or_else(|| StyleRef::Builtin("location".into()));
    BoxLook { linetype: linetype(lt), lineweight, colour, label_style }
}

/// Whether the project's location style labels with text style `id` (a change of that text style
/// then reaches every box).
pub fn labels_with(doc: &Document, id: &StyleRef) -> bool {
    let project = super::chosen(doc, &crate::defaults::styles::LOCATION, Scope::project());
    look_of(doc, &project).label_style == *id
}
