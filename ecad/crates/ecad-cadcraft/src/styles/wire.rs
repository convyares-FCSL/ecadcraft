//! Wire categories (brief 050b, design 050 §5): the `wire` catalogue.
//!
//! A category gives a wire its colour and cross-section (in the label, the reports and the BOM),
//! its drawn line (colour, type, weight), its label rule and its BOM part. A category names the
//! potential it is the default for (one direction stored, owner answer 3).
//!
//! # A conductor's category (design 050 §5.3)
//! Its own `wire.category`, else the category whose `potential` is its net's potential, else the
//! registry's `wire.category` (project, then standard), else none (today's look of a wire with no
//! potential). Its colour and cross-section are its own properties when it has them (022 FP3,
//! "overridden"), else the category's. A cable core takes no category colour: its core is its
//! colour.
//!
//! This module draws nothing; `wires`, `look` and `outputs` read [`WireLooks`].

use std::collections::BTreeMap;

use ecad_model::{ConductorId, Document, StyleRef};

use super::{Builtin, Catalogue, Field};
use crate::defaults::{Kind, Resolved, Scope, Source, Value};

/// The registry's built-in category choice: no category.
pub const NONE: &str = "none";

fn any_text(_: &str) -> bool {
    true
}

/// `wire` (the insulation colour, 040), `ink`, or one IEC 60757 colour code.
pub fn is_line_colour(s: &str) -> bool {
    let t = s.trim();
    t.eq_ignore_ascii_case("wire") || t.eq_ignore_ascii_case("ink") || crate::look::colour_code(t).is_some_and(|(_, two)| two.is_none())
}

const TEXT24: Kind = Kind::Parsed { max_chars: 24, parse: any_text };
const TEXT64: Kind = Kind::Parsed { max_chars: 64, parse: any_text };

static FIELDS: &[Field] = &[
    // Design 050 §5.3: potential and the BOM texts are empty by default (only colour and
    // cross-section default to `none`), so a potential named "none" is a name like any other.
    Field { key: "potential", label: "Potential", kind: TEXT24, builtin: Value::Text(""), over: None },
    Field { key: "colour", label: "Colour", kind: TEXT24, builtin: Value::Text(NONE), over: Some(ecad_model::wiring::COLOUR) },
    Field { key: "cross_section", label: "Cross-section", kind: TEXT24, builtin: Value::Text(NONE), over: Some(ecad_model::wiring::CROSS_SECTION) },
    Field {
        key: "line_colour",
        label: "Line colour",
        kind: Kind::Parsed { max_chars: 16, parse: is_line_colour },
        builtin: Value::Text("wire"),
        over: None,
    },
    Field {
        key: "line_type",
        label: "Line type",
        kind: Kind::Choice(&["continuous", "dashed", "dashdot"]),
        builtin: Value::Choice("continuous"),
        over: None,
    },
    Field { key: "line_weight", label: "Line weight", kind: Kind::Length { min: 0.13, max: 2.11 }, builtin: Value::Length(0.25), over: None },
    Field {
        key: "label_rule",
        label: "Label rule",
        kind: Kind::Choice(&["project", "net", "each", "none"]),
        builtin: Value::Choice("project"),
        over: None,
    },
    Field { key: "part_number", label: "Part number", kind: TEXT64, builtin: Value::Text(""), over: None },
    Field { key: "maker", label: "Maker", kind: TEXT64, builtin: Value::Text(""), over: None },
    Field { key: "description", label: "Description", kind: TEXT64, builtin: Value::Text(""), over: None },
    Field { key: "allowance", label: "Allowance (m per wire)", kind: Kind::Number { min: 0.0, max: 100.0 }, builtin: Value::Number(0.0), over: None },
];

const fn cat(id: &'static str, name: &'static str, values: &'static [(&'static str, Value<'static>)]) -> Builtin {
    Builtin { id, name, values }
}

const INK: Value<'static> = Value::Text("ink");
const WIRE: Value<'static> = Value::Text("wire");
const HEAVY: Value<'static> = Value::Length(0.6);
const THIN: Value<'static> = Value::Length(0.25);

/// Design 050 §5.2: from 022's default potentials and the UI spec's default category colours.
static BUILTINS: &[Builtin] = &[
    cat(
        "l",
        "Power L",
        &[
            ("potential", Value::Text("L")),
            ("colour", Value::Text("BN")),
            ("cross_section", Value::Text("2.5")),
            ("line_colour", INK),
            ("line_weight", HEAVY),
        ],
    ),
    cat(
        "l1",
        "Power L1",
        &[
            ("potential", Value::Text("L1")),
            ("colour", Value::Text("BN")),
            ("cross_section", Value::Text("2.5")),
            ("line_colour", INK),
            ("line_weight", HEAVY),
        ],
    ),
    cat(
        "l2",
        "Power L2",
        &[
            ("potential", Value::Text("L2")),
            ("colour", Value::Text("BK")),
            ("cross_section", Value::Text("2.5")),
            ("line_colour", INK),
            ("line_weight", HEAVY),
        ],
    ),
    cat(
        "l3",
        "Power L3",
        &[
            ("potential", Value::Text("L3")),
            ("colour", Value::Text("GY")),
            ("cross_section", Value::Text("2.5")),
            ("line_colour", INK),
            ("line_weight", HEAVY),
        ],
    ),
    cat(
        "n",
        "Neutral",
        &[
            ("potential", Value::Text("N")),
            ("colour", Value::Text("BU")),
            ("cross_section", Value::Text("2.5")),
            ("line_colour", INK),
            ("line_weight", HEAVY),
        ],
    ),
    cat(
        "pe",
        "Protective earth",
        &[
            ("potential", Value::Text("PE")),
            ("colour", Value::Text("GNYE")),
            ("cross_section", Value::Text("2.5")),
            ("line_colour", WIRE),
            ("line_weight", THIN),
        ],
    ),
    cat(
        "dc24",
        "24 V DC",
        &[
            ("potential", Value::Text("24V")),
            ("colour", Value::Text("BU")),
            ("cross_section", Value::Text("0.5")),
            ("line_colour", WIRE),
            ("line_weight", THIN),
        ],
    ),
    cat(
        "dc0",
        "0 V DC",
        &[
            ("potential", Value::Text("0V")),
            ("colour", Value::Text("BU")),
            ("cross_section", Value::Text("0.5")),
            ("line_colour", WIRE),
            ("line_weight", THIN),
        ],
    ),
    cat("signal", "Signal", &[("cross_section", Value::Text("0.5")), ("line_colour", INK), ("line_weight", THIN)]),
    cat("analogue", "Analogue", &[("cross_section", Value::Text("0.5")), ("line_colour", INK), ("line_weight", THIN)]),
];

fn choices() -> Vec<&'static crate::defaults::Entry> {
    vec![&crate::defaults::styles::WIRE_CATEGORY]
}

/// The wire category catalogue.
pub static CATALOGUE: Catalogue =
    Catalogue { word: "wire", plural: "Wire categories", one: "Wire category", fields: FIELDS, builtins: BUILTINS, choices, project_field: None };

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
pub struct WireLook {
    pub category: Option<StyleRef>,
    pub from: CategoryFrom,
    /// Colour code and cross-section, with where they come from (`Object` = the conductor's own).
    pub colour: Option<(String, Source)>,
    pub cross_section: Option<(String, Source)>,
    /// `wire`, `ink` or one colour code.
    pub line_colour: String,
    /// `continuous`, `dashed`, `dashdot`.
    pub line_type: &'static str,
    /// Millimetres.
    pub line_weight: f64,
    /// `net`, `each` or `none` (the project's rule resolved).
    pub label_rule: String,
}

impl WireLook {
    /// The code a route is drawn in: the insulation colour for `wire`, none for `ink`, else the
    /// line colour's code.
    pub fn route_code(&self) -> Option<String> {
        match self.line_colour.as_str() {
            "wire" => self.colour.as_ref().map(|c| c.0.clone()),
            "ink" => None,
            code => Some(code.to_string()),
        }
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
        for row in super::rows(doc, &CATALOGUE) {
            let p = super::field(doc, &CATALOGUE, &row.id, &FIELDS[0], None);
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
    pub fn category(&self, doc: &Document, c: ConductorId) -> (Option<StyleRef>, CategoryFrom) {
        let e = &crate::defaults::styles::WIRE_CATEGORY;
        let own = doc.conductors.get(&c).and_then(|x| x.properties.get(e.key)).and_then(|v| StyleRef::parse(v));
        if let Some(id) = own.filter(|id| super::exists(doc, "wire", id)) {
            return (Some(id), CategoryFrom::Own);
        }
        let conn = doc.connectivity();
        if let Some(net) = conn.net_of_conductor(c).and_then(|id| conn.net(id)) {
            let pot = ecad_model::wiring::net_potential(doc, net);
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
        match StyleRef::parse(&r.text()).filter(|id| super::exists(doc, "wire", id)) {
            Some(id) => (Some(id), from),
            None => (None, CategoryFrom::None),
        }
    }

    /// The look of conductor `c` (design 050 §5.3).
    pub fn look(&self, doc: &Document, c: ConductorId) -> WireLook {
        let (category, from) = self.category(doc, c);
        let props = doc.conductors.get(&c).map(|x| &x.properties);
        let core = !doc.cables_of(c).is_empty();
        let f = |key: &str| category.as_ref().and_then(|id| super::field_of(doc, "wire", id, key, None));
        let own = |key: &str| props.and_then(|p| p.get(key)).filter(|v| !v.trim().is_empty()).map(|v| (v.clone(), Source::Object));
        let from_cat = |key: &str| f(key).and_then(|r| text_of(&r).map(|t| (t, r.from)));
        let colour = own(ecad_model::wiring::COLOUR).or_else(|| if core { None } else { from_cat("colour") });
        let cross_section = own(ecad_model::wiring::CROSS_SECTION).or_else(|| if core { None } else { from_cat("cross_section") });
        let line_colour = f("line_colour").map_or_else(|| "wire".to_string(), |r| r.text().to_ascii_lowercase());
        let line_type = match f("line_type").map(|r| r.word()) {
            Some("dashed") => "dashed",
            Some("dashdot") => "dashdot",
            _ => "continuous",
        };
        let line_weight = f("line_weight").map_or(0.25, |r| r.number());
        let rule = f("label_rule").map_or("project", |r| r.word());
        let label_rule = match rule {
            "project" | "" => doc.settings.wiring.label_rule.clone(),
            other => other.to_string(),
        };
        WireLook { category, from, colour, cross_section, line_colour, line_type, line_weight, label_rule }
    }
}

/// [`WireLooks::look`] for one conductor (reads the categories once).
pub fn wire_look(doc: &Document, c: ConductorId) -> WireLook {
    WireLooks::new(doc).look(doc, c)
}

/// The category name of conductor `c` for reports (empty for none).
pub fn category_name(doc: &Document, look: &WireLook) -> String {
    look.category.as_ref().map(|id| super::name(doc, "wire", id)).unwrap_or_default()
}

/// The CADCraft linetype of a `line_type` word.
pub fn linetype(word: &str) -> &'static str {
    match word {
        "dashed" => crate::locations::LINETYPE,
        "dashdot" => "DASHDOT",
        _ => "CONTINUOUS",
    }
}
