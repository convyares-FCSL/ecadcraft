//! The location and box style catalogue (`location`).
//!
//! The built-in `box` ("Location box") is a dashed outline at the wire weight
//! in ink, labelled in the `location` text style. `fill` is a reserved field: it is stored and
//! listed, and drawn as `none` (CADCraft draws no fill behind other entities on a layer yet; the
//! quick start says so).

use ecad_model::geometry::units::Mm;
use ecad_model::objects::styles::LocationStyle;
use ecad_model::objects::styles::text::StyleColour;
use ecad_model::{AidId, Document, StyleRef, Word};

use ecad_electrical::CmdError;
use ecad_model::registry::data::Slot;
use ecad_model::registry::styles::{Builtin, Catalogue, Field};
use ecad_model::registry::{Kind, Scope, Value};

/// The line types a box may take, as the field stores them.
pub(crate) const LINE_TYPES: &[&str] = &["dashed", "continuous", "dashdot"];

static FIELDS: [Field; 5] = [
    Field { key: "line_type", label: "Line type", kind: Kind::Choice(LINE_TYPES), slot: Slot::new(&crate::defaults::data::DATA), over: None },
    Field {
        key: "line_weight",
        label: "Line weight",
        kind: Kind::Length { min: 0.13, max: 2.11 },
        slot: Slot::new(&crate::defaults::data::DATA),
        over: None,
    },
    Field {
        key: "line_colour",
        label: "Line colour",
        kind: Kind::Parsed { max_chars: 16, parse: crate::catalogues::is_colour },
        slot: Slot::new(&crate::defaults::data::DATA),
        over: None,
    },
    Field {
        key: "label_style",
        label: "Label style",
        kind: Kind::Style(&crate::catalogues::text::CATALOGUE),
        slot: Slot::new(&crate::defaults::data::DATA),
        over: None,
    },
    Field {
        key: "fill",
        label: "Fill (reserved)",
        kind: Kind::Parsed { max_chars: 16, parse: crate::catalogues::is_colour },
        slot: Slot::new(&crate::defaults::data::DATA),
        over: None,
    },
];

static BUILTINS: [Builtin; 1] = [Builtin { id: "box", name: "Location box", slot: Slot::new(&crate::defaults::data::DATA) }];

fn choices() -> Vec<&'static ecad_model::registry::Entry> {
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
    body,
    typed: None,
    adjust: None,
    set: set_body,
    implicit,
    reach,
};

/// The value field `key` of a location style record holds; a label style naming no text style
/// counts as absent.
fn body<'a>(doc: &'a ecad_model::Document, b: &'a ecad_model::objects::styles::StyleBody, key: &str) -> Option<Value<'a>> {
    match b {
        ecad_model::objects::styles::StyleBody::Location(l) => {
            get(l, key).filter(|v| v.style().is_none_or(|id| ecad_model::registry::styles::exists(doc, &crate::catalogues::text::CATALOGUE, &id)))
        }
        _ => None,
    }
}

/// A location box's resolved look.
#[derive(Debug, Clone, PartialEq)]
pub struct BoxDraw {
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
        _ => ecad_electrical::locations::LINETYPE,
    }
}

/// The look of location box `a`: its own style when it names one that exists, else
/// the project's location style, the standard's, the built-in `box`.
pub fn box_draw(doc: &Document, a: AidId) -> BoxDraw {
    draw_of(doc, &box_style(doc, a))
}

/// The location style box `a` is drawn in: its own when it names one that exists, else the
/// project's (the standard's, the built-in `box`).
pub(crate) fn box_style(doc: &Document, a: AidId) -> StyleRef {
    own_style(doc, a).cloned().unwrap_or_else(|| ecad_model::registry::styles::chosen(doc, &crate::defaults::styles::LOCATION, Scope::project()))
}

/// Box `a`'s own location style, when it names one that exists (a dangling one falls through).
pub(crate) fn own_style(doc: &Document, a: AidId) -> Option<&StyleRef> {
    ecad_electrical::locations::box_values(doc, a)
        .and_then(|b| b.style.as_ref())
        .filter(|s| ecad_model::registry::styles::exists(doc, &crate::catalogues::location::CATALOGUE, s))
}

/// The look of location style `id`.
pub(crate) fn draw_of(doc: &Document, id: &StyleRef) -> BoxDraw {
    let f = |k: &str| ecad_model::registry::styles::field_of(doc, &crate::catalogues::location::CATALOGUE, id, k, None);
    let lt = f("line_type").map(|r| r.word()).unwrap_or("dashed");
    let mm = f("line_weight").map_or(0.25, |r| r.number());
    let lineweight = (mm * 100.0).round().clamp(0.0, 211.0) as i16;
    let colour = f("line_colour").map_or_else(|| "none".to_string(), |r| r.text());
    let label_style = f("label_style").and_then(|r| StyleRef::parse(&r.text())).unwrap_or_else(|| StyleRef::Builtin("location".into()));
    BoxDraw { linetype: linetype(lt), lineweight, colour, label_style }
}

/// Whether the project's location style labels with text style `id` (a change of that text style
/// then reaches every box).
pub(crate) fn labels_with(doc: &Document, id: &StyleRef) -> bool {
    let project = ecad_model::registry::styles::chosen(doc, &crate::defaults::styles::LOCATION, Scope::project());
    draw_of(doc, &project).label_style == *id
}

/// The value of field `key` that `l` sets, if it sets one.
pub fn get<'a>(l: &'a LocationStyle, key: &str) -> Option<Value<'a>> {
    match key {
        "line_type" => l.line_type.map(|v| Value::Choice(v.word())),
        "line_weight" => l.line_weight.map(|v| Value::Length(v.0)),
        "line_colour" => l.line_colour.map(Value::Colour),
        "label_style" => l.label_style.as_ref().map(Value::of_style),
        "fill" => l.fill.map(Value::Colour),
        _ => None,
    }
}

/// Set (`Some`) or clear (`None`) field `key` of `l` from its text, as the `style` command
/// checked it.
///
/// # Errors
/// An unknown field, or text the field does not take, in words.
pub fn set(l: &mut LocationStyle, key: &str, value: Option<&str>) -> Result<(), CmdError> {
    let bad = |what: &str| CmdError::refused(format!("{key} takes {what}, not {:?}.", value.unwrap_or_default()));
    let colour = |v: &str| StyleColour::parse(v).ok_or_else(|| bad("none, a colour code or #rrggbb"));
    match (key, value) {
        ("line_type", None) => l.line_type = None,
        ("line_weight", None) => l.line_weight = None,
        ("line_colour", None) => l.line_colour = None,
        ("label_style", None) => l.label_style = None,
        ("fill", None) => l.fill = None,
        ("line_type", Some(v)) => l.line_type = Some(Word::parse(v).ok_or_else(|| bad(&LINE_TYPES.join(", ")))?),
        ("line_weight", Some(v)) => {
            l.line_weight = Some(Mm(v.trim().parse::<f64>().ok().filter(|x| x.is_finite()).ok_or_else(|| bad("a length"))?))
        }
        ("line_colour", Some(v)) => l.line_colour = Some(colour(v)?),
        ("label_style", Some(v)) => l.label_style = Some(StyleRef::parse(v).ok_or_else(|| bad("a text style"))?),
        ("fill", Some(v)) => l.fill = Some(colour(v)?),
        _ => return Err(CmdError::refused(format!("Location styles have no field {key:?}."))),
    }
    Ok(())
}

/// [`set`] on a record body of this catalogue.
fn set_body(b: &mut ecad_model::objects::styles::StyleBody, key: &str, value: Option<&str>) -> Result<(), ecad_model::registry::styles::Refused> {
    match b {
        ecad_model::objects::styles::StyleBody::Location(x) => set(x, key, value).map_err(|e| ecad_model::registry::styles::Refused(e.to_string())),
        other => Err(ecad_electrical::styles::wrong_body(other, CATALOGUE.one)),
    }
}

/// Boxes with no style of their own when the project's choice is `id`.
fn implicit(doc: &Document, entry: &ecad_model::registry::Entry, id: &StyleRef) -> usize {
    if ecad_model::registry::styles::chosen(doc, entry, ecad_model::registry::Scope::project()) != *id {
        return 0;
    }
    doc.aids().iter().filter(|(a, x)| x.as_box().is_some() && own_style(doc, **a).is_none()).count()
}

/// The pages of the boxes drawn in style `id` of their own.
fn reach(doc: &Document, _: &ecad_model::Op, id: &StyleRef) -> ecad_model::registry::styles::Reach {
    let mut out = std::collections::BTreeSet::new();
    for a in ecad_electrical::styles::box_users(doc, ecad_model::StyleCatalogue::Location, id) {
        out.extend(doc.representations_of(ecad_model::Subject::Aid(a)).map(|(_, r)| r.page));
    }
    ecad_model::registry::styles::Reach::Also(out)
}

/// The font family a box's label style draws in when it is not the project font, for
/// the page to install (`install_family`).
pub fn label_family(doc: &Document, rep: ecad_model::RepresentationId) -> Option<String> {
    let c = doc.representations().get(&rep).and_then(|r| match r.subject {
        ecad_model::Subject::Aid(a) => Some(a),
        _ => None,
    })?;
    let label = box_draw(doc, c).label_style;
    crate::text::own_style_of(doc, &label).family
}
