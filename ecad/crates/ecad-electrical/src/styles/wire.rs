//! Wire types (brief 050b, design 050 §5; design 072 §4.6, §8, F36): the `wire` catalogue.
//!
//! A wire type gives a wire its colours and size (in the label, the reports and the BOM), its
//! insulation, its drawn line (colour, type, weight), its label rule and its BOM part. A type names
//! the potential it is the default for (one direction stored, owner answer 3).
//!
//! Its records are typed ([`WireType`]); the built-in types and the defaults are our own data,
//! `defaults/wire_types.toml`, read as the same type. [`FIELDS`] is the editor's description of
//! the fields (labels, input rules), not their storage.
//!
//! # A wire's type (design 050 §5.3)
//! Its own `wire_type`, else the type whose `potential` is its net's potential, else the
//! registry's `wire.category` (project, then standard), else none (today's look of a wire with no
//! potential). Its colours and size are its own when it has them (022 FP3, "overridden"), else the
//! type's. A cable core takes no type colour: its core is its colour.
//!
//! This module draws nothing; `wires`, `look` and `outputs` read [`WireLooks`].

use std::collections::BTreeMap;
use std::sync::OnceLock;

use ecad_model::objects::styles::wire::{ConductorRating, Insulation, LineColour, WireType};
pub use ecad_model::objects::styles::wire::{LineType, RouteLabels};
use ecad_model::geometry::units::{Amps, Celsius, Metres, Mm};
use ecad_model::objects::wire::{Colours, WireSize};
use ecad_model::{Document, StyleRef, WireId, Word};
use serde::Deserialize;

use crate::styles::{Builtin, Catalogue, Field};
use crate::CmdError;
use crate::defaults::{Kind, Resolved, Scope, Slot, Source, Value};

/// The registry's built-in type choice: no type.
pub const NONE: &str = "none";

fn any_text(_: &str) -> bool {
    true
}

/// `wire` (the insulation colour, 040), `ink`, or one colour in any of its forms.
pub fn is_line_colour(s: &str) -> bool {
    LineColour::parse(s).is_some()
}

fn is_size(s: &str) -> bool {
    WireSize::parse(s).is_some()
}

fn is_colours(s: &str) -> bool {
    Colours::parse(s).is_some()
}

const TEXT24: Kind = Kind::Parsed { max_chars: 24, parse: any_text };
const TEXT64: Kind = Kind::Parsed { max_chars: 64, parse: any_text };

/// The fields of a wire type, for the editor and the `style` command.
pub static FIELDS: [Field; 17] = [
    Field { key: "potential", label: "Potential", kind: TEXT24, slot: Slot::new(), over: None },
    Field { key: "size", label: "Size", kind: Kind::Parsed { max_chars: 24, parse: is_size }, slot: Slot::new(), over: None },
    Field { key: "colours", label: "Colours", kind: Kind::Parsed { max_chars: 24, parse: is_colours }, slot: Slot::new(), over: None },
    Field { key: "insulation", label: "Insulation", kind: Kind::Choice(<Insulation as Word>::WORDS), slot: Slot::new(), over: None },
    Field { key: "line_colour", label: "Line colour", kind: Kind::Parsed { max_chars: 16, parse: is_line_colour }, slot: Slot::new(), over: None },
    Field { key: "line_type", label: "Line type", kind: Kind::Choice(<LineType as Word>::WORDS), slot: Slot::new(), over: None },
    Field { key: "line_weight", label: "Line weight", kind: Kind::Length { min: 0.13, max: 2.11 }, slot: Slot::new(), over: None },
    Field { key: "label_rule", label: "Label rule", kind: Kind::Choice(<RouteLabels as Word>::WORDS), slot: Slot::new(), over: None },
    Field { key: "part_number", label: "Part number", kind: TEXT64, slot: Slot::new(), over: None },
    Field { key: "maker", label: "Maker", kind: TEXT64, slot: Slot::new(), over: None },
    Field { key: "description", label: "Description", kind: TEXT64, slot: Slot::new(), over: None },
    Field { key: "allowance", label: "Allowance (m per wire)", kind: Kind::Number { min: 0.0, max: 100.0 }, slot: Slot::new(), over: None },
    // Brief 057 (design 057 §4.4, FP3): the datasheet rating, shared by the type's wires.
    crate::circuits::fields::RATED_CURRENT,
    crate::circuits::fields::RATED_AT,
    crate::circuits::fields::MAX_TEMPERATURE,
    crate::circuits::fields::MATERIAL,
    crate::circuits::fields::CONDUCTOR_CLASS,
];

const fn wire_type(id: &'static str, name: &'static str) -> Builtin {
    Builtin { id, name, slot: Slot::new() }
}

/// Design 050 §5.2 and design 072 F37: the built-in types, in table order; their values are in
/// `defaults/wire_types.toml`.
static BUILTINS: [Builtin; 12] = [
    wire_type("l", "Power L"),
    wire_type("l1", "Power L1"),
    wire_type("l2", "Power L2"),
    wire_type("l3", "Power L3"),
    wire_type("n", "Neutral"),
    wire_type("pe", "Protective earth"),
    wire_type("ac230_l", "230 V AC L"),
    wire_type("ac230_n", "230 V AC N"),
    wire_type("dc24", "24 V DC"),
    wire_type("dc0", "0 V DC"),
    wire_type("signal", "Signal"),
    wire_type("analogue", "Analogue"),
];

fn choices() -> Vec<&'static crate::defaults::Entry> {
    vec![&crate::defaults::styles::WIRE_CATEGORY]
}

/// The wire type catalogue.
pub static CATALOGUE: Catalogue = Catalogue {
    catalogue: ecad_model::StyleCatalogue::Wire,
    plural: "Wire types",
    one: "Wire type",
    fields: &FIELDS,
    builtins: &BUILTINS,
    choices,
    project_field: None,
};

/// The built-in data's text.
pub const DATA: &str = include_str!("../../defaults/wire_types.toml");
/// Its name, for messages.
pub const DATA_FILE: &str = "wire_types.toml";

/// The built-in data, read as typed records.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Data {
    pub defaults: WireType,
    #[serde(default)]
    pub types: BTreeMap<String, WireType>,
}

/// Read the built-in data.
///
/// # Errors
/// The data's first problem ([`DATA_FILE`]'s field, value or syntax).
pub fn read_data(text: &str) -> Result<Data, toml::de::Error> {
    toml::from_str(text)
}

fn data() -> &'static Data {
    static DATA_READ: OnceLock<Data> = OnceLock::new();
    // A damaged built-in file gives no built-in values (each field's safe value); the test
    // `wire_types_data_reads` keeps it from shipping.
    DATA_READ.get_or_init(|| read_data(DATA).unwrap_or_default())
}

/// The catalogue's defaults: what a field takes when nothing else sets it.
pub fn defaults() -> &'static WireType {
    &data().defaults
}

/// Built-in type `id`'s values.
pub fn builtin_type(id: &str) -> Option<&'static WireType> {
    data().types.get(id)
}

/// The value of field `key` that `w` sets, if it sets one.
pub fn get<'a>(w: &'a WireType, key: &str) -> Option<Value<'a>> {
    let r = &w.rating;
    let text = |t: Option<&'a str>| t.map(Value::Text);
    match key {
        "potential" => text(w.potential.as_ref().map(|t| t.as_str())),
        "size" => w.size.map(Value::Size),
        "colours" => w.colours.as_ref().map(Value::Colours),
        "insulation" => w.insulation.map(|v| Value::Choice(v.word())),
        "line_colour" => w.line_colour.map(|v| Value::Text(v.word())),
        "line_type" => w.line_type.map(|v| Value::Choice(v.word())),
        "line_weight" => w.line_weight.map(|v| Value::Length(v.0)),
        "label_rule" => w.label_rule.map(|v| Value::Choice(v.word())),
        "part_number" => text(w.part_number.as_ref().map(|t| t.as_str())),
        "maker" => text(w.maker.as_ref().map(|t| t.as_str())),
        "description" => text(w.description.as_ref().map(|t| t.as_str())),
        "allowance" => w.allowance.map(|v| Value::Number(v.0)),
        "rated_current" => r.rated_current.map(|v| Value::Number(v.0)),
        "rated_at" => r.rated_at.map(|v| Value::Number(v.0)),
        "max_temperature" => r.max_temperature.map(|v| Value::Number(v.0)),
        "material" => r.material.map(|v| Value::Choice(v.word())),
        "conductor_class" => r.conductor_class.map(|v| Value::Choice(v.word())),
        _ => None,
    }
}

fn refused(why: String) -> CmdError {
    CmdError::refused(why)
}

fn number(key: &str, t: &str) -> Result<f64, CmdError> {
    t.trim().parse::<f64>().ok().filter(|x| x.is_finite()).ok_or_else(|| refused(format!("{key} takes a number, not {t:?}")))
}

fn word<W: Word>(key: &str, t: &str) -> Result<W, CmdError> {
    W::parse(t).ok_or_else(|| refused(format!("{key} takes {}, not {t:?}", W::WORDS.join(", "))))
}

fn text<const N: usize>(t: &str) -> Result<ecad_model::naming::text::Text<N>, CmdError> {
    ecad_model::naming::text::Text::new(t).map_err(|e| refused(e.to_string()))
}

fn no_field(key: &str) -> CmdError {
    refused(format!("Wire types have no field {key:?}."))
}

/// Set (`Some`) or clear (`None`) field `key` of `w` from its text, as the `style` command
/// checked it.
///
/// # Errors
/// An unknown field, or text the field does not take, in words.
pub fn set(w: &mut WireType, key: &str, value: Option<&str>) -> Result<(), CmdError> {
    let Some(t) = value else {
        let r: &mut ConductorRating = &mut w.rating;
        match key {
            "potential" => w.potential = None,
            "size" => w.size = None,
            "colours" => w.colours = None,
            "insulation" => w.insulation = None,
            "line_colour" => w.line_colour = None,
            "line_type" => w.line_type = None,
            "line_weight" => w.line_weight = None,
            "label_rule" => w.label_rule = None,
            "part_number" => w.part_number = None,
            "maker" => w.maker = None,
            "description" => w.description = None,
            "allowance" => w.allowance = None,
            "rated_current" => r.rated_current = None,
            "rated_at" => r.rated_at = None,
            "max_temperature" => r.max_temperature = None,
            "material" => r.material = None,
            "conductor_class" => r.conductor_class = None,
            _ => return Err(no_field(key)),
        }
        return Ok(());
    };
    match key {
        "potential" => w.potential = Some(text(t)?),
        "size" => w.size = Some(WireSize::parse(t).ok_or_else(|| refused(format!("{t:?} is not a wire size")))?),
        "colours" => w.colours = Some(Colours::parse(t).ok_or_else(|| refused(format!("{t:?} is not a list of wire colours")))?),
        "insulation" => w.insulation = Some(word(key, t)?),
        "line_colour" => w.line_colour = Some(LineColour::parse(t).ok_or_else(|| refused(format!("{t:?} is not a line colour")))?),
        "line_type" => w.line_type = Some(word(key, t)?),
        "line_weight" => w.line_weight = Some(Mm(number(key, t)?)),
        "label_rule" => w.label_rule = Some(word(key, t)?),
        "part_number" => w.part_number = Some(text(t)?),
        "maker" => w.maker = Some(text(t)?),
        "description" => w.description = Some(text(t)?),
        "allowance" => w.allowance = Some(Metres(number(key, t)?)),
        "rated_current" => {
            let a = crate::devices::ratings::parse::current(t).ok_or_else(|| refused(format!("{t:?} is not a current")))?;
            w.rating.rated_current = Some(Amps(a));
        }
        "rated_at" => w.rating.rated_at = Some(Celsius(number(key, t)?)),
        "max_temperature" => w.rating.max_temperature = Some(Celsius(number(key, t)?)),
        "material" => w.rating.material = Some(word(key, t)?),
        "conductor_class" => w.rating.conductor_class = Some(word(key, t)?),
        _ => return Err(no_field(key)),
    }
    Ok(())
}

/// Where a conductor's category came from (the inspector's badge, design 050 §7.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CategoryFrom {
    /// The conductor's own `wire.category`.
    Own,
    /// The net's potential (`From potential 24V`).
    Potential(String),
    /// The registry's choice at project or standard level.
    Project,
    Standard,
    /// No category.
    None,
}

/// A conductor's resolved look (design 050 §5.3).
#[derive(Debug, Clone, PartialEq)]
pub struct WireDraw {
    pub category: Option<StyleRef>,
    pub from: CategoryFrom,
    /// Its colours and cross-section, with where they come from (`Object` = the conductor's own).
    pub colour: Option<(Colours, Source)>,
    pub cross_section: Option<(String, Source)>,
    /// What its route is drawn in: the insulation colour, ink, or one colour.
    pub line_colour: LineColour,
    /// `continuous`, `dashed`, `dashdot`.
    pub line_type: &'static str,
    /// Millimetres.
    pub line_weight: f64,
    /// `net`, `each` or `none` (the project's rule resolved).
    pub label_rule: String,
}

impl WireDraw {
    /// The colours a route is drawn in: the insulation colours for `wire`, none for `ink`, else the
    /// line colour.
    pub fn route_code(&self) -> Option<Colours> {
        match self.line_colour {
            LineColour::Wire => self.colour.as_ref().map(|c| c.0.clone()),
            LineColour::Ink => None,
            LineColour::Colour(c) => Colours::new(vec![c]),
        }
    }

    /// Its colours in their stored words (`GN/YE`), with where they come from.
    pub fn colour_text(&self) -> Option<(String, Source)> {
        self.colour.as_ref().map(|(c, s)| (c.to_string(), *s))
    }
}

/// A field's text value, `None` for the `none` sentinel.
fn text_of(r: &Resolved<'_>) -> Option<String> {
    let t = r.text();
    (!t.trim().is_empty() && !t.eq_ignore_ascii_case(NONE)).then_some(t)
}

/// A text field with no sentinel (potential, part number, maker, description): `None` when empty.
pub fn filled(r: &Resolved<'_>) -> Option<String> {
    let t = r.text();
    (!t.trim().is_empty()).then_some(t)
}

/// The categories of a project read once, for many conductors (a page, a report).
#[derive(Debug, Clone, Default)]
pub struct WireLooks {
    /// Potential (lower case) → the category that is its default, the first by ID.
    by_potential: BTreeMap<String, StyleRef>,
    /// Potentials named by two or more categories, with all of them (Checks).
    pub twice: BTreeMap<String, Vec<StyleRef>>,
}

impl WireLooks {
    /// Read the categories of `doc`.
    pub fn new(doc: &Document) -> Self {
        let mut all: BTreeMap<String, Vec<StyleRef>> = BTreeMap::new();
        for row in crate::styles::rows(doc, &CATALOGUE) {
            let p = crate::styles::field(doc, &CATALOGUE, &row.id, &FIELDS[0], None);
            if let Some(name) = filled(&p) {
                all.entry(name.trim().to_lowercase()).or_default().push(row.id);
            }
        }
        let mut by_potential = BTreeMap::new();
        let mut twice = BTreeMap::new();
        for (p, mut ids) in all {
            ids.sort();
            if let Some(first) = ids.first() {
                by_potential.insert(p.clone(), first.clone());
            }
            if ids.len() > 1 {
                twice.insert(p, ids);
            }
        }
        WireLooks { by_potential, twice }
    }

    /// Every category that is a potential's default.
    pub fn by_potential_ids(&self) -> impl Iterator<Item = &StyleRef> {
        self.by_potential.values()
    }

    /// The category that is the default for potential `name`, if any.
    pub fn for_potential(&self, name: &str) -> Option<&StyleRef> {
        self.by_potential.get(&name.trim().to_lowercase())
    }

    /// The category of conductor `c` and where it came from (design 050 §5.3).
    pub fn category(&self, doc: &Document, c: WireId) -> (Option<StyleRef>, CategoryFrom) {
        let e = &crate::defaults::styles::WIRE_CATEGORY;
        let own = doc.wires.get(&c).and_then(|x| x.wire_type.clone());
        if let Some(id) = own.filter(|id| crate::styles::exists(doc, ecad_model::StyleCatalogue::Wire, id)) {
            return (Some(id), CategoryFrom::Own);
        }
        let conn = doc.connectivity();
        if let Some(net) = conn.net_of_conductor(c).and_then(|id| conn.net(id)) {
            let pot = ecad_model::connectivity::wiring::net_potential(doc, net);
            if let Some(p) = pot.name
                && let Some(id) = self.for_potential(&p)
            {
                return (Some(id.clone()), CategoryFrom::Potential(p));
            }
        }
        let r = crate::defaults::resolve(doc, e, Scope::project());
        let from = match r.from {
            Source::Project => CategoryFrom::Project,
            Source::Standard => CategoryFrom::Standard,
            _ => CategoryFrom::None,
        };
        match StyleRef::parse(&r.text()).filter(|id| crate::styles::exists(doc, ecad_model::StyleCatalogue::Wire, id)) {
            Some(id) => (Some(id), from),
            None => (None, CategoryFrom::None),
        }
    }

    /// The look of conductor `c` (design 050 §5.3).
    pub fn look(&self, doc: &Document, c: WireId) -> WireDraw {
        let (category, from) = self.category(doc, c);
        let wire = doc.wires.get(&c);
        let core = !doc.cables_of(c).is_empty();
        let f = |key: &str| category.as_ref().and_then(|id| crate::styles::field_of(doc, ecad_model::StyleCatalogue::Wire, id, key, None));
        let from_cat = |key: &str| f(key).and_then(|r| text_of(&r).map(|t| (t, r.from)));
        let own_colours = wire.and_then(|w| w.colours.clone()).map(|c| (c, Source::Object));
        let own_size = wire.and_then(|w| w.size).map(|s| (s.to_string(), Source::Object));
        let type_colours = || f("colours").and_then(|r| text_of(&r).and_then(|t| Colours::parse(&t)).map(|c| (c, r.from)));
        let colour = own_colours.or_else(|| if core { None } else { type_colours() });
        let cross_section = own_size.or_else(|| if core { None } else { from_cat("size") });
        let line_colour = f("line_colour").and_then(|r| LineColour::parse(&r.text())).unwrap_or(LineColour::Wire);
        let line_type = match f("line_type").map(|r| r.word()) {
            Some("dashed") => "dashed",
            Some("dashdot") => "dashdot",
            _ => "continuous",
        };
        let line_weight = f("line_weight").map_or(0.25, |r| r.number());
        let rule = f("label_rule").map_or("project", |r| r.word());
        let label_rule = match rule {
            "project" | "" => crate::wires::label_rule_word(doc).to_string(),
            other => other.to_string(),
        };
        WireDraw { category, from, colour, cross_section, line_colour, line_type, line_weight, label_rule }
    }
}

/// [`WireLooks::look`] for one conductor (reads the categories once).
pub fn wire_draw(doc: &Document, c: WireId) -> WireDraw {
    WireLooks::new(doc).look(doc, c)
}

/// The category name of conductor `c` for reports (empty for none).
pub fn category_name(doc: &Document, look: &WireDraw) -> String {
    look.category.as_ref().map(|id| crate::styles::name(doc, ecad_model::StyleCatalogue::Wire, id)).unwrap_or_default()
}

/// The CADCraft linetype of a `line_type` word.
pub fn linetype(word: &str) -> &'static str {
    match word {
        "dashed" => crate::locations::LINETYPE,
        "dashdot" => "DASHDOT",
        _ => "CONTINUOUS",
    }
}

#[cfg(test)]
mod tests {
    use crate::styles::*;

    /// Design 072 F37: the built-in data reads as typed records, every built-in type has its values
    /// and every type in the data is listed, and the 230 V L/N types are there.
    #[test]
    fn wire_types_data_reads() {
        let data = read_data(DATA).unwrap();
        for b in &BUILTINS {
            assert!(data.types.contains_key(b.id), "{} has no values in {DATA_FILE}", b.id);
        }
        for id in data.types.keys() {
            assert!(BUILTINS.iter().any(|b| b.id == id), "{id} in {DATA_FILE} is not listed");
        }
        for (id, potential) in [("ac230_l", None), ("ac230_n", None), ("pe", Some("PE"))] {
            let t = &data.types[id];
            assert_eq!(t.potential.as_ref().map(|p| p.as_str()), potential, "{id}");
            assert!(t.size.is_some() && t.colours.is_some(), "{id}");
        }
        assert!(data.defaults.line_weight.is_some() && data.defaults.rating.max_temperature.is_some());
        assert!(read_data("[defaults]\ncolour = \"BU\"\n").is_err(), "an unknown field is refused");
    }

    /// Every field's text sets and reads back the same value, and clears.
    #[test]
    fn every_field_sets_reads_and_clears() {
        let samples = [
            ("potential", "L1"),
            ("size", "2.5"),
            ("colours", "GN/YE"),
            ("insulation", "pvc"),
            ("line_colour", "BU"),
            ("line_type", "dashed"),
            ("line_weight", "0.35"),
            ("label_rule", "each"),
            ("part_number", "H07V-K 2.5"),
            ("maker", "Maker"),
            ("description", "Flexible wire"),
            ("allowance", "0.2"),
            ("rated_current", "24"),
            ("rated_at", "30"),
            ("max_temperature", "70"),
            ("material", "cu"),
            ("conductor_class", "5"),
        ];
        assert_eq!(samples.len(), FIELDS.len());
        let mut w = WireType::default();
        for (key, text) in samples {
            assert!(FIELDS.iter().any(|f| f.key == key), "{key}");
            set(&mut w, key, Some(text)).unwrap();
            let shown = get(&w, key).map(|value| Resolved { value, from: Source::Project }.text());
            assert_eq!(shown.as_deref(), Some(text), "{key}");
        }
        for (key, _) in samples {
            set(&mut w, key, None).unwrap();
        }
        assert_eq!(w, WireType::default());
        assert!(set(&mut w, "colour", Some("BU")).is_err());
        assert!(set(&mut w, "insulation", Some("paper")).is_err());
    }
}
