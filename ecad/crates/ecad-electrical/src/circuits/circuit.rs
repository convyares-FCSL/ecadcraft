//! The circuit side of the rating checks: a subject's Iz, its
//! factors and volt drop, worked out from the wire or cable, its table and its load. The
//! protective device it is checked against is [`crate::circuits::protection`]'s.

use crate::circuits::*;
/// Round half up to `dp` decimals.
pub(crate) fn half_up(x: f64, dp: i32) -> f64 {
    let m = 10f64.powi(dp);
    ((x * m) + 0.5 + 1e-9).floor() / m
}

/// A factor rounded to 2 dp.
pub(crate) fn round2(x: f64) -> f64 {
    half_up(x, 2)
}

/// The temperature factor of the IEC derivation, `round2(√((θmax − θ)/(θmax − θref)))`, or `None`
/// when θ ≥ θmax (too hot) or the reference is not below θmax. `cap` limits it to 1.00.
pub fn k_temp(theta_max: f64, theta_ref: f64, theta: f64, cap: bool) -> Option<f64> {
    if theta >= theta_max || theta_ref >= theta_max {
        return None;
    }
    let k = round2(((theta_max - theta) / (theta_max - theta_ref)).sqrt());
    k.is_finite().then_some(if cap { k.min(1.0) } else { k })
}

/// Volt drop of a copper conductor: `(volts, percent)`.
pub fn volt_drop(r20: f64, theta_max: f64, three_phase: bool, amps: f64, metres: f64, volts: f64) -> Option<(f64, f64)> {
    let r = r20 * (1.0 + 0.00393 * (theta_max - 20.0));
    let mv = if three_phase { 3f64.sqrt() * r } else { 2.0 * r };
    let du = mv * amps * metres / 1000.0;
    let pc = 100.0 * du / volts;
    (du.is_finite() && pc.is_finite()).then_some((du, pc))
}

/// A current as shown: 0.1 A, half up, trailing `.0` dropped (`18.8`, `27`); with the extra digit
/// when within 0.05 A of `against`.
pub fn fmt_amps(x: f64, against: Option<f64>) -> String {
    let near = against.is_some_and(|a| (x - a).abs() < 0.05 && (x - a).abs() > 1e-9);
    if near {
        // A pass never reads the same as a fail (`21.60 A` against `21.6 A`).
        return format!("{:.2}", half_up(x, 2));
    }
    crate::devices::ratings::dataset::fmt_num(half_up(x, 1))
}

/// A volt drop as shown: 0.1 %, half up; 0.01 % below 0.1 %.
pub fn fmt_percent(x: f64) -> String {
    if x < 0.1 { format!("{:.2}", half_up(x, 2)) } else { format!("{:.1}", half_up(x, 1)) }
}

/// A factor as shown: `0.87`.
pub fn fmt_factor(x: f64) -> String {
    format!("{x:.2}")
}

/// One factor of a result, with its input and where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Factor {
    /// `panel`, `bundling`, `ambient`, `grouping`.
    pub name: &'static str,
    pub factor: f64,
    /// `40 °C`, `3 on tray`, `0.80`.
    pub input: String,
    pub from: FactorSource,
}

impl Factor {
    /// Whether the input is a plain factor (`0.80`), not a count (`3 bunched`).
    pub fn is_plain_number(&self) -> bool {
        self.input.trim().parse::<f64>().is_ok()
    }
}

/// Where a factor's input came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactorSource {
    /// Set on the wire or cable itself.
    SetHere,
    /// The standard's value.
    Standard,
    /// The project's own value.
    Project,
    /// The project default, whichever level gave it.
    ProjectDefault,
    /// A location box (its code).
    Location(String),
    /// A rating table.
    Table,
    /// Computed from the input, which came from the source inside when it is not set here.
    Computed(Option<Box<FactorSource>>),
    /// Not set: the factor is assumed to be 1.
    NotSet,
}

impl FactorSource {
    /// The words the rating's tooltips and notes show: `location +CAB1`, `project default`,
    /// `set here`, `computed`, `table`.
    pub fn words(&self) -> String {
        match self {
            FactorSource::SetHere => "set here".into(),
            FactorSource::Standard => "standard".into(),
            FactorSource::Project => "project".into(),
            FactorSource::ProjectDefault => "project default".into(),
            FactorSource::Location(l) => format!("location {l}"),
            FactorSource::Table => "table".into(),
            FactorSource::Computed(None) => "computed".into(),
            FactorSource::Computed(Some(b)) => format!("{}, computed", b.words()),
            FactorSource::NotSet => "not set: \u{d7} 1.00 assumed".into(),
        }
    }

    /// Whether the factor was computed rather than read from a table.
    pub fn is_computed(&self) -> bool {
        matches!(self, FactorSource::Computed(_))
    }
}

/// A cable's volt drop.
#[derive(Debug, Clone, PartialEq)]
pub struct VoltDrop {
    pub volts: f64,
    pub percent: f64,
    pub amps: f64,
    /// Why this current: `typed`, `from Power`, `-Q01 rating`.
    pub why: String,
    pub limit: f64,
    pub limit_from: &'static str,
    /// The next size of the class that would be within the limit, with its drop.
    pub larger: Option<(f64, f64)>,
}

/// What a subject's rating is.
#[derive(Debug, Clone, PartialEq)]
pub struct Rating {
    pub subject: RatedSubject,
    pub basis: Basis,
    /// The basis is the subject's own (else the project default).
    pub basis_own: bool,
    /// Datasheet: the rated current and its reference temperature.
    pub rated: Option<(f64, f64)>,
    /// Table: the capacity, the size, the method.
    pub table_value: Option<(f64, f64, &'static str)>,
    /// The rating table used: name and source.
    pub table: Option<(String, String)>,
    pub factors: Vec<Factor>,
    /// Iz, unrounded.
    pub iz: Option<f64>,
    pub device: Option<Device>,
    pub load: Option<Load>,
    pub voltdrop: Option<VoltDrop>,
    /// The cross-section used.
    pub size: Option<f64>,
    /// A wire's ends' locations as entry keys (the `at` index); the first is the one whose
    /// temperature was used, when one was.
    pub locations: Vec<String>,
    pub issues: Vec<Issue>,
}

impl Rating {
    /// The folded summary: `18.8 A ≥ 4 A`, `3.5 A < 4 A`, `no data`; a cable adds its
    /// volt drop.
    pub fn summary(&self) -> String {
        let mut s = match (self.iz, self.device.as_ref().and_then(|d| d.amps)) {
            (Some(iz), Some(i)) => {
                format!("{} A {} {} A", fmt_amps(iz, Some(i)), if iz >= i { "\u{2265}" } else { "<" }, crate::devices::ratings::dataset::fmt_num(i))
            }
            (Some(iz), None) => format!("{} A", fmt_amps(iz, None)),
            (None, _) => "no data".into(),
        };
        if let Some(v) = &self.voltdrop {
            s.push_str(&format!(" \u{b7} {} %", fmt_percent(v.percent)));
        }
        s
    }

    /// Whether Iz is below In.
    pub fn fails(&self) -> bool {
        self.issues.iter().any(|i| matches!(i, Issue::Under { .. } | Issue::TooHot { .. }))
    }

    /// The Status words of the wires table: `underrated`, `load > -Q01`, else `None`.
    pub fn status(&self) -> Option<String> {
        if self.fails() {
            return Some("underrated".into());
        }
        if self.issues.contains(&Issue::LoadOver) {
            return Some(format!("load > {}", self.device.as_ref().map(|d| d.tag.as_str()).unwrap_or("device")));
        }
        if self.issues.contains(&Issue::VoltDropOver) {
            return Some("volt drop".into());
        }
        None
    }
}

/// What every evaluation shares, read once per build or update.
pub struct Context {
    pub draws: wire::WireDraws,
    /// Location entry key → its panel temperature (location boxes with a panel temperature;
    /// A stored location is an entry key, shown as its code).
    temps: BTreeMap<String, f64>,
    boxes: crate::locations::BoxCache,
    /// The chosen rating table, parsed once and shared (the rows are user data
    /// of up to 256 KiB, parsed lazily and kept off per-frame paths).
    pub table: Option<std::sync::Arc<ratingtable::Table>>,
    loads: TypedLoads,
}

impl Context {
    /// A context reusing an already parsed rating table (`None`: read it now).
    pub fn with_table(doc: &Document, table: Option<Option<std::sync::Arc<ratingtable::Table>>>) -> Self {
        let mut c = Context::new_without_table(doc);
        c.table = table.unwrap_or_else(|| ratingtable::chosen_table(doc).map(std::sync::Arc::new));
        c
    }

    pub(super) fn new_without_table(doc: &Document) -> Self {
        let mut c = Context::new_inner(doc);
        c.table = None;
        c
    }

    pub fn new(doc: &Document) -> Self {
        let mut c = Context::new_inner(doc);
        c.table = ratingtable::chosen_table(doc).map(std::sync::Arc::new);
        c
    }

    pub(super) fn new_inner(doc: &Document) -> Self {
        Context::with_boxes(doc, &box_temps(doc))
    }

    /// A context from kept location boxes (no scan of the components).
    pub(super) fn with_boxes(doc: &Document, boxes: &BTreeMap<ecad_model::AidId, BoxTemp>) -> Self {
        let mut temps: BTreeMap<String, f64> = BTreeMap::new();
        for b in boxes.values() {
            if let Temp::Read(t) = b.temp {
                let e = temps.entry(b.location.clone()).or_insert(t);
                if t > *e {
                    *e = t;
                }
            }
        }
        Context { draws: wire::WireDraws::new(doc), temps, boxes: crate::locations::BoxCache::default(), table: None, loads: TypedLoads }
    }
}

/// A location box's panel temperature: its location, and the value or the text that did not
/// read (`rating-bad-value`).
#[derive(Debug, Clone, PartialEq)]
pub struct BoxTemp {
    /// The box's stored location: an entry key of the project's list.
    pub location: String,
    pub temp: Temp,
}

/// A box's stored panel temperature: the value in degrees Celsius, or the text that did not read.
#[derive(Debug, Clone, PartialEq)]
pub enum Temp {
    /// The temperature read, in degrees Celsius.
    Read(f64),
    /// The stored text (at most 32 characters of it) that is not a temperature.
    Unread(String),
}

/// The temperature of location box `a`, if it is one and sets one. A value outside −40 to 90 °C
/// (a file made elsewhere) does not read.
pub fn box_temp(doc: &Document, a: ecad_model::AidId) -> Option<BoxTemp> {
    let x = crate::locations::box_values(doc, a)?;
    let t = x.panel_temperature?.0;
    let location = crate::locations::designations::box_key(x, crate::locations::designations::Aspect::Location).unwrap_or_default();
    let temp = if t.is_finite() && (-40.0..=90.0).contains(&t) { Temp::Read(t) } else { Temp::Unread(format!("{t}").chars().take(32).collect()) };
    Some(BoxTemp { location, temp })
}

/// The code of location entry `key`, or the key itself when the list has no entry.
pub fn location_code(doc: &Document, key: &str) -> String {
    crate::locations::designations::code_of(doc, crate::locations::designations::Aspect::Location, key).unwrap_or_else(|| key.to_string())
}

/// Every location box that sets a temperature (open, reload, first show).
pub(crate) fn box_temps(doc: &Document) -> BTreeMap<ecad_model::AidId, BoxTemp> {
    doc.aids().keys().filter_map(|a| box_temp(doc, *a).map(|b| (*a, b))).collect()
}

pub(super) fn project_number(doc: &Document, e: &defaults::Entry) -> f64 {
    defaults::resolve(doc, e, Scope::project()).number()
}

pub(super) fn project_word(doc: &Document, e: &defaults::Entry) -> &'static str {
    defaults::resolve(doc, e, Scope::project()).word()
}

/// A project default's source.
pub(super) fn default_source(doc: &Document, e: &defaults::Entry) -> FactorSource {
    match defaults::resolve(doc, e, Scope::project()).from {
        defaults::Source::Standard => FactorSource::Standard,
        defaults::Source::Project => FactorSource::Project,
        _ => FactorSource::ProjectDefault,
    }
}

/// A project default's source in words.
pub(super) fn default_from(doc: &Document, e: &defaults::Entry) -> &'static str {
    match defaults::resolve(doc, e, Scope::project()).from {
        defaults::Source::Standard => "standard",
        defaults::Source::Project => "project",
        _ => "project default",
    }
}

/// Whether conductor `c` is a panel wire (not a cable core).
pub(crate) fn is_panel_wire(doc: &Document, c: WireId) -> bool {
    doc.cables_of(c).is_empty()
}

/// A wire category's or cable type's datasheet values: rated current, θref, θmax, class.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sheet {
    pub rated: Option<f64>,
    pub rated_at: f64,
    pub max: f64,
    pub class: u8,
}

pub(super) fn sheet(doc: &Document, catalogue: ecad_model::StyleCatalogue, id: Option<&StyleRef>) -> Sheet {
    let f = |k: &str| id.and_then(|id| styles::field_of(doc, catalogue, id, k, None));
    Sheet {
        rated: f("rated_current").and_then(|r| crate::devices::ratings::parse::current(&r.text())),
        rated_at: f("rated_at").map_or(30.0, |r| r.number()),
        max: f("max_temperature").map_or(70.0, |r| r.number()),
        class: f("conductor_class").and_then(|r| r.word().parse().ok()).unwrap_or(5),
    }
}

/// Whether conductor `c` is a rated subject.
pub(crate) fn wire_is_rated(doc: &Document, ctx: &Context, c: WireId) -> bool {
    if !is_panel_wire(doc, c) {
        return false;
    }
    let Some(x) = doc.wires().get(&c) else { return false };
    if !x.rating.is_empty() || x.load.is_some() || x.protected_by().is_some() {
        return true;
    }
    let (cat, _) = ctx.draws.category(doc, c);
    sheet(doc, ecad_model::StyleCatalogue::Wire, cat.as_ref()).rated.is_some()
}

/// Whether cable `k` is a rated subject.
pub(crate) fn cable_is_rated(doc: &Document, ctx: &Context, k: CableId) -> bool {
    let Some(x) = doc.cables().get(&k) else { return false };
    if !x.rating.is_empty() || x.load.is_some() || x.protected_by().is_some() || cable::of(doc, k).is_some() {
        return true;
    }
    ctx.table.is_some() && x.type_text().and_then(crate::terminal_strips::cores::notation_size).is_some()
}

/// The panel temperature of a wire: the hottest of its ends' locations, else
/// the project's; with where it came from.
pub fn panel_temperature(doc: &Document, ctx: &mut Context, c: WireId) -> (f64, FactorSource) {
    let (t, from, _) = panel_temperature_at(doc, ctx, c);
    (t, from)
}

/// [`panel_temperature`], and the locations of the wire's ends (the one used first).
pub(super) fn panel_temperature_at(doc: &Document, ctx: &mut Context, c: WireId) -> (f64, FactorSource, Vec<String>) {
    let mut best: Option<(f64, FactorSource)> = None;
    let mut locations: Vec<String> = Vec::new();
    if let Some(x) = doc.wires().get(&c) {
        for t in &x.ends {
            let Some(comp) = doc.connection_points().get(t).and_then(|t| t.device()) else { continue };
            // The device's stored location is an entry key; boxes match by key, and the
            // row shows the entry's code.
            let Some(loc) = crate::locations::resolve_with(doc, comp, &mut ctx.boxes).key.map(|l| l.trim().to_string()) else { continue };
            if let Some(temp) = ctx.temps.get(&loc).copied()
                && best.as_ref().is_none_or(|(b, _)| temp > *b)
            {
                best = Some((temp, FactorSource::Location(location_code(doc, &loc))));
                locations.retain(|l| *l != loc);
                locations.insert(0, loc.clone());
            }
            if !locations.contains(&loc) {
                locations.push(loc);
            }
        }
    }
    match best {
        Some((t, from)) => (t, from, locations),
        None => (project_number(doc, &defaults::rating::PANEL_TEMPERATURE), default_source(doc, &defaults::rating::PANEL_TEMPERATURE), locations),
    }
}

/// Evaluate subject `s` with its trace result `found`.
pub(crate) fn evaluate(doc: &Document, ctx: &mut Context, s: RatedSubject, found: &Found) -> Rating {
    let circuit = s.circuit(doc);
    let mut issues = Vec::new();
    let mut factors = Vec::new();
    let is_cable = matches!(s, RatedSubject::Cable(_));
    // Basis.
    let inp = circuit.map(|c| *c.inputs()).unwrap_or_default();
    let own_basis = inp.basis;
    let default_basis = Basis::parse(project_word(doc, if is_cable { &defaults::rating::CABLE_BASIS } else { &defaults::rating::WIRE_BASIS }))
        .unwrap_or(if is_cable { Basis::Table } else { Basis::Datasheet });
    let basis = own_basis.unwrap_or(default_basis);
    // Datasheet values, size.
    let (sh, size, assumed) = match s {
        RatedSubject::Wire(c) => {
            let draw = ctx.draws.draw(doc, c);
            let sh = sheet(doc, ecad_model::StyleCatalogue::Wire, draw.category.as_ref());
            (sh, draw.cross_section.as_ref().and_then(|(t, _)| crate::devices::ratings::parse::cross_section(t)), false)
        }
        RatedSubject::Cable(k) => {
            let ty = cable::of(doc, k);
            let sh = sheet(doc, ecad_model::StyleCatalogue::Cable, ty.as_ref());
            let from_type = ty
                .as_ref()
                .and_then(|id| styles::field_of(doc, ecad_model::StyleCatalogue::Cable, id, "size", None))
                .and_then(|r| crate::devices::ratings::parse::cross_section(&r.text()));
            let size = from_type.or_else(|| circuit.and_then(|c| c.type_text()).and_then(crate::terminal_strips::cores::notation_size));
            (sh, size, ty.is_none())
        }
    };
    if assumed {
        issues.push(Issue::Assumed);
    }
    // Device and In.
    let device = device_of(doc, s, found, &mut issues);
    let i_n = device.as_ref().and_then(|d| d.amps);
    // Load.
    let load = ctx.loads.load(doc, s);
    // The temperature the conductor sits in.
    let mut locations = Vec::new();
    let (theta, theta_from, place) = match s {
        RatedSubject::Wire(c) => {
            let (t, from, at) = panel_temperature_at(doc, ctx, c);
            locations = at;
            let place = match &from {
                FactorSource::Location(l) => l.clone(),
                _ => "The panel".to_string(),
            };
            (t, from, place)
        }
        RatedSubject::Cable(_) => match inp.ambient.map(|c| c.0) {
            Some(t) => (t, FactorSource::SetHere, s.words(doc)),
            None => (project_number(doc, &defaults::rating::AMBIENT), default_source(doc, &defaults::rating::AMBIENT), s.words(doc)),
        },
    };
    let temp_name = if is_cable { "ambient" } else { "panel" };
    let mut table_used = None;
    let mut table_value = None;
    let mut rated = None;
    // Grouping input: a wire's bundling, a cable's grouped count.
    let bundling = if is_cable {
        inp.grouped.map(|n| (ecad_model::objects::wire::Bundling::Circuits(n), FactorSource::SetHere))
    } else {
        match inp.bundling {
            Some(b) => Some((b, FactorSource::SetHere)),
            None => {
                let r = defaults::resolve(doc, &defaults::rating::BUNDLING, Scope::project());
                crate::devices::ratings::parse::bundling(&r.text()).map(|b| (b, default_source(doc, &defaults::rating::BUNDLING)))
            }
        }
    };
    let method: &'static str =
        if is_cable { inp.method.map(|m| m.word()).unwrap_or_else(|| project_word(doc, &defaults::rating::METHOD)) } else { "B1" };
    let system = inp.system.map(|s| s.word()).unwrap_or_else(|| project_word(doc, &defaults::rating::SYSTEM));
    let loaded: u8 = if system == "ac3" { 3 } else { 2 };
    let grouping_factor = |data: Option<&Dataset>, issues: &mut Vec<Issue>, factors: &mut Vec<Factor>| -> Option<f64> {
        match &bundling {
            Some((ecad_model::objects::wire::Bundling::Factor(f), from)) => {
                let k = round2(*f);
                factors.push(Factor { name: if is_cable { "grouping" } else { "bundling" }, factor: k, input: fmt_factor(k), from: from.clone() });
                Some(k)
            }
            Some((ecad_model::objects::wire::Bundling::Circuits(n), from)) => {
                let Some(d) = data else {
                    issues.push(Issue::NoTable(format!("{n} circuits need a rating table's grouping factor; no rating table chosen")));
                    return None;
                };
                match d.grouping(method, u32::from(*n)) {
                    Some((f, _)) => {
                        let k = round2(f);
                        let input =
                            if is_cable { format!("{n} {}", if method == "E" { "on tray" } else { "grouped" }) } else { format!("{n} bunched") };
                        factors.push(Factor { name: if is_cable { "grouping" } else { "bundling" }, factor: k, input, from: from.clone() });
                        Some(k)
                    }
                    None => {
                        issues.push(Issue::NoTable(format!("no grouping factor for {n} circuits, {method}")));
                        None
                    }
                }
            }
            None if !is_cable => {
                issues.push(Issue::BundlingUnset);
                factors.push(Factor { name: "bundling", factor: 1.0, input: "1.00".into(), from: FactorSource::NotSet });
                Some(1.0)
            }
            None => Some(1.0),
        }
    };
    let iz = match basis {
        Basis::Manual => match inp.manual.map(|a| a.0) {
            Some(x) => Some(x),
            None => {
                issues.push(Issue::NoData("no Manual value set".into()));
                None
            }
        },
        Basis::Datasheet => match sh.rated {
            None => {
                let why = match s {
                    RatedSubject::Wire(c) => match ctx.draws.category(doc, c).0 {
                        Some(id) => format!("category {} has no rated current", styles::name(doc, ecad_model::StyleCatalogue::Wire, &id)),
                        None => "no wire category".into(),
                    },
                    RatedSubject::Cable(k) => match cable::of(doc, k) {
                        Some(id) => format!("cable type {} has no rated current", styles::name(doc, ecad_model::StyleCatalogue::Cable, &id)),
                        None => "no cable type".into(),
                    },
                };
                issues.push(Issue::NoData(why));
                None
            }
            Some(r) => {
                rated = Some((r, sh.rated_at));
                match k_temp(sh.max, sh.rated_at, theta, true) {
                    None => {
                        issues.push(Issue::TooHot { place: place.clone(), temp: theta, max: sh.max });
                        factors.push(Factor {
                            name: temp_name,
                            factor: 0.0,
                            input: format!("{} \u{b0}C", crate::devices::ratings::dataset::fmt_num(theta)),
                            from: theta_from.clone(),
                        });
                        Some(0.0)
                    }
                    Some(k) => {
                        factors.push(Factor {
                            name: temp_name,
                            factor: k,
                            input: format!("{} \u{b0}C", crate::devices::ratings::dataset::fmt_num(theta)),
                            from: theta_from.clone(),
                        });
                        let data = ctx.table.as_ref().map(|t| &t.data);
                        grouping_factor(data, &mut issues, &mut factors).map(|g| r * k * g)
                    }
                }
            }
        },
        Basis::Table => match ctx.table.as_ref() {
            None => {
                issues.push(Issue::NoTableChosen);
                None
            }
            Some(t) => {
                table_used = Some((t.name.clone(), t.source.clone()));
                let named = format!("Rating table '{}'", t.name);
                match (size, Insulation::of_max(sh.max)) {
                    (None, _) => {
                        issues.push(Issue::NoTable(format!("{named}: no size to look up")));
                        None
                    }
                    (Some(sz), None) => {
                        issues.push(Issue::NoTable(format!(
                            "No rating table column for a {} \u{b0}C conductor",
                            crate::devices::ratings::dataset::fmt_num(sh.max)
                        )));
                        let _ = sz;
                        None
                    }
                    (Some(sz), Some(ins)) => match t.data.capacity(method, ins, loaded, sz) {
                        None => {
                            issues.push(Issue::NoTable(format!(
                                "{named} has no capacity for {} mm\u{b2}, {method}, {}, {loaded} loaded",
                                crate::devices::ratings::dataset::fmt_num(sz),
                                ins.word()
                            )));
                            None
                        }
                        Some(cap) => {
                            table_value = Some((cap, sz, method));
                            let k_amb = if t.data.has_ambient(ins) {
                                match t.data.ambient(ins, theta) {
                                    Some((f, _)) => Some((round2(f), false)),
                                    None => {
                                        issues.push(Issue::NoTable(format!(
                                            "{named} has no ambient factor at {} \u{b0}C, {}",
                                            crate::devices::ratings::dataset::fmt_num(theta),
                                            ins.word()
                                        )));
                                        None
                                    }
                                }
                            } else {
                                match k_temp(sh.max, 30.0, theta, false) {
                                    Some(k) => Some((k, true)),
                                    None => {
                                        issues.push(Issue::TooHot { place: place.clone(), temp: theta, max: sh.max });
                                        Some((0.0, true))
                                    }
                                }
                            };
                            match k_amb {
                                None => None,
                                Some((k, computed)) => {
                                    let from = match (computed, &theta_from) {
                                        (true, FactorSource::SetHere) => FactorSource::Computed(None),
                                        (true, base) => FactorSource::Computed(Some(Box::new(base.clone()))),
                                        (false, base) => base.clone(),
                                    };
                                    factors.push(Factor {
                                        name: temp_name,
                                        factor: k,
                                        input: format!("{} \u{b0}C", crate::devices::ratings::dataset::fmt_num(theta)),
                                        from,
                                    });
                                    grouping_factor(Some(&t.data), &mut issues, &mut factors).map(|g| cap * k * g)
                                }
                            }
                        }
                    },
                }
            }
        },
    };
    let iz = iz.filter(|x| x.is_finite());
    // The overload checks, on unrounded values.
    if let (Some(iz), Some(i)) = (iz, i_n)
        && iz < i
        && !issues.iter().any(|x| matches!(x, Issue::TooHot { .. }))
    {
        let larger = (basis == Basis::Table).then(|| larger_size(ctx, size, method, sh.max, loaded, &factors, i)).flatten();
        issues.push(Issue::Under { larger });
    }
    if let (Some(l), Some(i)) = (load.as_ref(), i_n)
        && l.amps > i
    {
        issues.push(Issue::LoadOver);
    }
    // Volt drop (cables only).
    let voltdrop =
        if is_cable { volt_drop_of(doc, ctx, circuit, &inp, &sh, size, system, load.as_ref(), device.as_ref(), &mut issues) } else { None };
    Rating {
        subject: s,
        basis,
        basis_own: own_basis.is_some(),
        rated,
        table_value,
        table: table_used,
        factors,
        iz,
        device,
        load,
        voltdrop,
        size,
        locations,
        issues,
    }
}

/// The smallest larger size whose capacity × the same factors reaches `i_n` (the **Use 2.5 mm²**
/// fix).
pub(super) fn larger_size(ctx: &Context, size: Option<f64>, method: &str, max: f64, loaded: u8, factors: &[Factor], i_n: f64) -> Option<f64> {
    let t = ctx.table.as_ref()?;
    let size = size?;
    let ins = Insulation::of_max(max)?;
    let k: f64 = factors.iter().map(|f| f.factor).product();
    t.data.capacity_sizes(method, ins, loaded).into_iter().find(|(s, cap)| *s > size + 1e-9 && cap * k >= i_n).map(|(s, _)| s)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn volt_drop_of(
    doc: &Document, ctx: &Context, circuit: Option<&dyn Circuit>, inp: &CircuitInputs, sh: &Sheet, size: Option<f64>, system: &str,
    load: Option<&Load>, device: Option<&Device>, issues: &mut Vec<Issue>,
) -> Option<VoltDrop> {
    let metres = circuit.and_then(|c| c.length()).and_then(crate::devices::ratings::parse::length_m)?;
    let (amps, why) = match (load, device.and_then(|d| d.amps.map(|a| (a, d.tag.clone())))) {
        (Some(l), _) => (l.amps, if l.from == LoadFrom::Typed { "typed".to_string() } else { "from Power".to_string() }),
        (None, Some((a, tag))) => (a, format!("{tag} rating")),
        (None, None) => {
            issues.push(Issue::NoCurrent);
            return None;
        }
    };
    let size = size?;
    let t = ctx.table.as_ref()?;
    if size > 16.0 {
        issues.push(Issue::NoTable("volt drop needs reactance above 16 mm\u{b2}; not computed".into()));
        return None;
    }
    let Some(r20) = t.data.resistance(sh.class, size) else {
        issues.push(Issue::NoTable(format!(
            "Rating table '{}' has no resistance for {} mm\u{b2}, class {}",
            t.name,
            crate::devices::ratings::dataset::fmt_num(size),
            sh.class
        )));
        return None;
    };
    let (volts, vfrom) = match inp.voltage.map(|v| v.0) {
        Some(v) => (v, "set here"),
        None => (project_number(doc, &defaults::rating::VOLTAGE), default_from(doc, &defaults::rating::VOLTAGE)),
    };
    let (limit, limit_from) = match inp.vd_limit.map(|p| p.0) {
        Some(v) => (v, "set here"),
        None => (project_number(doc, &defaults::rating::VD_LIMIT), default_from(doc, &defaults::rating::VD_LIMIT)),
    };
    let _ = vfrom;
    let three = system == "ac3";
    let (du, pc) = volt_drop(r20, sh.max, three, amps, metres, volts)?;
    let mut larger = None;
    if pc > limit {
        issues.push(Issue::VoltDropOver);
        larger = t
            .data
            .resistance_sizes(sh.class)
            .into_iter()
            .filter(|x| *x > size + 1e-9)
            .filter_map(|x| t.data.resistance(sh.class, x).and_then(|r| volt_drop(r, sh.max, three, amps, metres, volts)).map(|(_, p)| (x, p)))
            .find(|(_, p)| *p <= limit);
    }
    Some(VoltDrop { volts: du, percent: pc, amps, why, limit, limit_from, larger })
}

/// Every rated subject of `doc`.
pub fn subjects(doc: &Document, ctx: &Context) -> Vec<RatedSubject> {
    let mut v: Vec<RatedSubject> = doc.wires().keys().filter(|c| wire_is_rated(doc, ctx, **c)).map(|c| RatedSubject::Wire(*c)).collect();
    v.extend(doc.cables().keys().filter(|k| cable_is_rated(doc, ctx, **k)).map(|k| RatedSubject::Cable(*k)));
    v
}

/// One subject's rating, traced now (the `ratings` command, the panels).
pub fn rating_of(doc: &Document, s: RatedSubject) -> Rating {
    let mut ctx = Context::new(doc);
    let found = trace::trace_subject(doc, s);
    evaluate(doc, &mut ctx, s, &found)
}
