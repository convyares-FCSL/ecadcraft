//! The circuit side of the rating checks (design 072 lane D; brief 057): a subject's Iz, its
//! factors and volt drop, worked out from the wire or cable, its table and its load. The
//! protective device it is checked against is [`super::protection`]'s.

use super::*;
/// Round half up to `dp` decimals.
pub fn half_up(x: f64, dp: i32) -> f64 {
    let m = 10f64.powi(dp);
    ((x * m) + 0.5 + 1e-9).floor() / m
}

/// A factor rounded to 2 dp (design 057 §3.6).
pub fn round2(x: f64) -> f64 {
    half_up(x, 2)
}

/// The temperature factor of the IEC derivation, `round2(√((θmax − θ)/(θmax − θref)))`, or `None`
/// when θ ≥ θmax (too hot) or the reference is not below θmax. `cap` limits it to 1.00 (Q6).
pub fn k_temp(theta_max: f64, theta_ref: f64, theta: f64, cap: bool) -> Option<f64> {
    if theta >= theta_max || theta_ref >= theta_max {
        return None;
    }
    let k = round2(((theta_max - theta) / (theta_max - theta_ref)).sqrt());
    k.is_finite().then_some(if cap { k.min(1.0) } else { k })
}

/// Volt drop of a copper conductor (design 057 §3.5): `(volts, percent)`.
pub fn volt_drop(r20: f64, theta_max: f64, three_phase: bool, amps: f64, metres: f64, volts: f64) -> Option<(f64, f64)> {
    let r = r20 * (1.0 + 0.00393 * (theta_max - 20.0));
    let mv = if three_phase { 3f64.sqrt() * r } else { 2.0 * r };
    let du = mv * amps * metres / 1000.0;
    let pc = 100.0 * du / volts;
    (du.is_finite() && pc.is_finite()).then_some((du, pc))
}

/// A current as shown: 0.1 A, half up, trailing `.0` dropped (`18.8`, `27`); with the extra digit
/// when within 0.05 A of `against` (§3.6).
pub fn fmt_amps(x: f64, against: Option<f64>) -> String {
    let near = against.is_some_and(|a| (x - a).abs() < 0.05 && (x - a).abs() > 1e-9);
    if near {
        // §3.6: a pass never reads the same as a fail (`21.60 A` against `21.6 A`).
        return format!("{:.2}", half_up(x, 2));
    }
    dataset::fmt_num(half_up(x, 1))
}

/// A volt drop as shown: 0.1 %, half up; 0.01 % below 0.1 %.
pub fn fmt_percent(x: f64) -> String {
    if x < 0.1 { format!("{:.2}", half_up(x, 2)) } else { format!("{:.1}", half_up(x, 1)) }
}

/// A factor as shown: `0.87`.
pub fn fmt_factor(x: f64) -> String {
    format!("{x:.2}")
}

/// One factor of a result, with its input and where it came from (§8.2).
#[derive(Debug, Clone, PartialEq)]
pub struct Factor {
    /// `panel`, `bundling`, `ambient`, `grouping`.
    pub name: &'static str,
    pub factor: f64,
    /// `40 °C`, `3 on tray`, `0.80`.
    pub input: String,
    /// `location +CAB1`, `project default`, `set here`, `computed`, `table`.
    pub from: String,
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
    /// A wire's ends' locations as entry keys (brief 052; the `at` index, design 057 §10); the first is the one whose
    /// temperature was used, when one was.
    pub locations: Vec<String>,
    pub issues: Vec<Issue>,
}

impl Rating {
    /// The folded summary (§2.1, §2.2): `18.8 A ≥ 4 A`, `3.5 A < 4 A`, `no data`; a cable adds its
    /// volt drop.
    pub fn summary(&self) -> String {
        let mut s = match (self.iz, self.device.as_ref().and_then(|d| d.amps)) {
            (Some(iz), Some(i)) => format!("{} A {} {} A", fmt_amps(iz, Some(i)), if iz >= i { "\u{2265}" } else { "<" }, dataset::fmt_num(i)),
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

    /// The Status words of the wires table (§2.1): `underrated`, `load > -Q01`, else `None`.
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
    pub looks: wire::WireLooks,
    /// Location entry key → its panel temperature (location boxes with `rating.temperature`;
    /// brief 052: a stored location is an entry key, shown as its code).
    temps: BTreeMap<String, f64>,
    boxes: crate::locations::BoxCache,
    /// The chosen rating table, parsed once and shared (design 057 FP8: the rows are user data
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
    pub(super) fn with_boxes(doc: &Document, boxes: &BTreeMap<DeviceId, BoxTemp>) -> Self {
        let mut temps: BTreeMap<String, f64> = BTreeMap::new();
        for b in boxes.values() {
            if let Temp::Read(t) = b.temp {
                let e = temps.entry(b.location.clone()).or_insert(t);
                if t > *e {
                    *e = t;
                }
            }
        }
        Context { looks: wire::WireLooks::new(doc), temps, boxes: crate::locations::BoxCache::default(), table: None, loads: TypedLoads }
    }
}

/// A location box's panel temperature (FP6): its location, and the value or the text that did not
/// read (`rating-bad-value`).
#[derive(Debug, Clone, PartialEq)]
pub struct BoxTemp {
    /// The box's stored location: an entry key of the project's list (brief 052).
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

/// The temperature of location box `c`, if it is one and sets one.
pub fn box_temp(doc: &Document, c: DeviceId) -> Option<BoxTemp> {
    let x = doc.devices.get(&c).filter(|x| crate::locations::is_box_component(x))?;
    let v = x.properties.get(TEMPERATURE)?;
    let location = crate::designations::device_key(x, crate::designations::Aspect::Location).unwrap_or_default();
    Some(BoxTemp { location, temp: parse::ambient(v).map_or_else(|| Temp::Unread(v.chars().take(32).collect()), Temp::Read) })
}

/// The code of location entry `key` (brief 052), or the key itself when the list has no entry.
pub fn location_code(doc: &Document, key: &str) -> String {
    crate::designations::code_of(doc, crate::designations::Aspect::Location, key).unwrap_or_else(|| key.to_string())
}

/// Every location box that sets a temperature (open, reload, first show).
pub fn box_temps(doc: &Document) -> BTreeMap<DeviceId, BoxTemp> {
    doc.devices.keys().filter_map(|c| box_temp(doc, *c).map(|b| (*c, b))).collect()
}

pub(super) fn project_number(doc: &Document, e: &defaults::Entry) -> f64 {
    defaults::resolve(doc, e, Scope::project()).number()
}

pub(super) fn project_word(doc: &Document, e: &defaults::Entry) -> &'static str {
    defaults::resolve(doc, e, Scope::project()).word()
}

/// A project default's source in words.
pub(super) fn default_from(doc: &Document, e: &defaults::Entry) -> &'static str {
    match defaults::resolve(doc, e, Scope::project()).from {
        defaults::Source::Standard => "standard",
        defaults::Source::Project => "project",
        _ => "project default",
    }
}

/// Whether the subject's properties name any rating key.
pub(super) fn has_own(props: &ecad_model::Properties) -> bool {
    props.contains_key(LOAD) || props.contains_key(PROTECTED_BY) || props.keys().any(|k| k.starts_with("rating."))
}

/// Whether conductor `c` is a panel wire (not a cable core).
pub fn is_panel_wire(doc: &Document, c: WireId) -> bool {
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
        rated: f("rated_current").and_then(|r| parse::current(&r.text())),
        rated_at: f("rated_at").map_or(30.0, |r| r.number()),
        max: f("max_temperature").map_or(70.0, |r| r.number()),
        class: f("conductor_class").and_then(|r| r.word().parse().ok()).unwrap_or(5),
    }
}

/// Whether conductor `c` is a rated subject (design 057 §7).
pub fn wire_is_rated(doc: &Document, ctx: &Context, c: WireId) -> bool {
    if !is_panel_wire(doc, c) {
        return false;
    }
    let Some(x) = doc.wires.get(&c) else { return false };
    if !x.rating.is_empty() || x.load.is_some() || x.display.contains_key(PROTECTED_BY) {
        return true;
    }
    let (cat, _) = ctx.looks.category(doc, c);
    sheet(doc, ecad_model::StyleCatalogue::Wire, cat.as_ref()).rated.is_some()
}

/// Whether cable `k` is a rated subject (design 057 §7).
pub fn cable_is_rated(doc: &Document, ctx: &Context, k: CableId) -> bool {
    let Some(x) = doc.cables.get(&k) else { return false };
    if has_own(&x.properties) || cable::of(doc, k).is_some() {
        return true;
    }
    ctx.table.is_some() && x.properties.get("type").and_then(|t| crate::strips::notation_size(t)).is_some()
}

/// The panel temperature of a wire (design 057 §4.4): the hottest of its ends' locations, else
/// the project's; with where it came from.
pub fn panel_temperature(doc: &Document, ctx: &mut Context, c: WireId) -> (f64, String) {
    let (t, from, _) = panel_temperature_at(doc, ctx, c);
    (t, from)
}

/// [`panel_temperature`], and the locations of the wire's ends (the one used first).
pub(super) fn panel_temperature_at(doc: &Document, ctx: &mut Context, c: WireId) -> (f64, String, Vec<String>) {
    let mut best: Option<(f64, String)> = None;
    let mut locations: Vec<String> = Vec::new();
    if let Some(x) = doc.wires.get(&c) {
        for t in x.ends.iter().flatten() {
            let Some(comp) = doc.terminals.get(t).map(|t| t.component) else { continue };
            // Brief 052: the device's stored location is an entry key; boxes match by key, and the
            // row shows the entry's code.
            let Some(loc) = crate::locations::resolve_with(doc, comp, &mut ctx.boxes).key.map(|l| l.trim().to_string()) else { continue };
            if let Some(temp) = ctx.temps.get(&loc).copied()
                && best.as_ref().is_none_or(|(b, _)| temp > *b)
            {
                best = Some((temp, format!("location {}", location_code(doc, &loc))));
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
        None => (
            project_number(doc, &defaults::rating::PANEL_TEMPERATURE),
            default_from(doc, &defaults::rating::PANEL_TEMPERATURE).to_string(),
            locations,
        ),
    }
}

/// Read a subject's own value of `key` with `parse`: `Ok(None)` when absent, `Err` (a bad-value
/// issue) when it does not read.
pub(super) fn own<T>(
    props: Option<&ecad_model::Properties>, key: &'static str, parse: impl Fn(&str) -> Option<T>, issues: &mut Vec<Issue>,
) -> Option<T> {
    let v = props?.get(key)?;
    match parse(v) {
        Some(x) => Some(x),
        None => {
            issues.push(Issue::BadValue(key, v.chars().take(32).collect()));
            None
        }
    }
}

/// Evaluate subject `s` with its trace result `found`.
pub fn evaluate(doc: &Document, ctx: &mut Context, s: RatedSubject, found: &Found) -> Rating {
    let props = s.props(doc);
    let mut issues = Vec::new();
    let mut factors = Vec::new();
    let is_cable = matches!(s, RatedSubject::Cable(_));
    // Basis.
    let (inp, _) = s.inputs(doc, &mut issues);
    let own_basis = inp.basis;
    let default_basis = Basis::parse(project_word(doc, if is_cable { &defaults::rating::CABLE_BASIS } else { &defaults::rating::WIRE_BASIS }))
        .unwrap_or(if is_cable { Basis::Table } else { Basis::Datasheet });
    let basis = own_basis.unwrap_or(default_basis);
    // Datasheet values, size.
    let (sh, size, assumed) = match s {
        RatedSubject::Wire(c) => {
            let look = ctx.looks.look(doc, c);
            let sh = sheet(doc, ecad_model::StyleCatalogue::Wire, look.category.as_ref());
            (sh, look.cross_section.as_ref().and_then(|(t, _)| parse::cross_section(t)), false)
        }
        RatedSubject::Cable(k) => {
            let ty = cable::of(doc, k);
            let sh = sheet(doc, ecad_model::StyleCatalogue::Cable, ty.as_ref());
            let from_type = ty
                .as_ref()
                .and_then(|id| styles::field_of(doc, ecad_model::StyleCatalogue::Cable, id, "size", None))
                .and_then(|r| parse::cross_section(&r.text()));
            let size = from_type.or_else(|| props.and_then(|p| p.get("type")).and_then(|t| crate::strips::notation_size(t)));
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
    if load.is_none() && props.is_some_and(|p| p.contains_key(LOAD)) {
        issues.push(Issue::BadValue(LOAD, props.and_then(|p| p.get(LOAD)).map(|v| v.chars().take(32).collect()).unwrap_or_default()));
    }
    // The temperature the conductor sits in.
    let mut locations = Vec::new();
    let (theta, theta_from, place) = match s {
        RatedSubject::Wire(c) => {
            let (t, from, at) = panel_temperature_at(doc, ctx, c);
            locations = at;
            let place = from.strip_prefix("location ").map_or_else(|| "The panel".to_string(), str::to_string);
            (t, from, place)
        }
        RatedSubject::Cable(_) => match inp.ambient.map(|c| c.0) {
            Some(t) => (t, "set here".to_string(), s.words(doc)),
            None => (project_number(doc, &defaults::rating::AMBIENT), default_from(doc, &defaults::rating::AMBIENT).to_string(), s.words(doc)),
        },
    };
    let temp_name = if is_cable { "ambient" } else { "panel" };
    let mut table_used = None;
    let mut table_value = None;
    let mut rated = None;
    // Grouping input: a wire's bundling, a cable's grouped count.
    let bundling = if is_cable {
        inp.grouped.map(|n| (parse::Bundling::Circuits(n), "set here".to_string()))
    } else {
        match inp.bundling {
            Some(b) => Some((b, "set here".to_string())),
            None => {
                let r = defaults::resolve(doc, &defaults::rating::BUNDLING, Scope::project());
                parse::bundling(&r.text()).map(|b| (b, default_from(doc, &defaults::rating::BUNDLING).to_string()))
            }
        }
    };
    let method: &'static str =
        if is_cable { inp.method.map(|m| m.word()).unwrap_or_else(|| project_word(doc, &defaults::rating::METHOD)) } else { "B1" };
    let system = inp.system.map(|s| s.word()).unwrap_or_else(|| project_word(doc, &defaults::rating::SYSTEM));
    let loaded: u8 = if system == "ac3" { 3 } else { 2 };
    let grouping_factor = |data: Option<&Dataset>, issues: &mut Vec<Issue>, factors: &mut Vec<Factor>| -> Option<f64> {
        match &bundling {
            Some((parse::Bundling::Factor(f), from)) => {
                let k = round2(*f);
                factors.push(Factor { name: if is_cable { "grouping" } else { "bundling" }, factor: k, input: fmt_factor(k), from: from.clone() });
                Some(k)
            }
            Some((parse::Bundling::Circuits(n), from)) => {
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
                factors.push(Factor { name: "bundling", factor: 1.0, input: "1.00".into(), from: "not set: \u{d7} 1.00 assumed".into() });
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
                    RatedSubject::Wire(c) => match ctx.looks.category(doc, c).0 {
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
                            input: format!("{} \u{b0}C", dataset::fmt_num(theta)),
                            from: theta_from.clone(),
                        });
                        Some(0.0)
                    }
                    Some(k) => {
                        factors.push(Factor {
                            name: temp_name,
                            factor: k,
                            input: format!("{} \u{b0}C", dataset::fmt_num(theta)),
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
                issues.push(Issue::NoTable("No rating table chosen".into()));
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
                        issues.push(Issue::NoTable(format!("No rating table column for a {} \u{b0}C conductor", dataset::fmt_num(sh.max))));
                        let _ = sz;
                        None
                    }
                    (Some(sz), Some(ins)) => match t.data.capacity(method, ins, loaded, sz) {
                        None => {
                            issues.push(Issue::NoTable(format!(
                                "{named} has no capacity for {} mm\u{b2}, {method}, {}, {loaded} loaded",
                                dataset::fmt_num(sz),
                                ins.word()
                            )));
                            None
                        }
                        Some(cap) => {
                            table_value = Some((cap, sz, method));
                            let k_amb = if t.data.has_ambient(ins) {
                                match t.data.ambient(ins, theta) {
                                    Some((f, _)) => Some((round2(f), "table".to_string())),
                                    None => {
                                        issues.push(Issue::NoTable(format!(
                                            "{named} has no ambient factor at {} \u{b0}C, {}",
                                            dataset::fmt_num(theta),
                                            ins.word()
                                        )));
                                        None
                                    }
                                }
                            } else {
                                match k_temp(sh.max, 30.0, theta, false) {
                                    Some(k) => Some((k, "computed".to_string())),
                                    None => {
                                        issues.push(Issue::TooHot { place: place.clone(), temp: theta, max: sh.max });
                                        Some((0.0, "computed".to_string()))
                                    }
                                }
                            };
                            match k_amb {
                                None => None,
                                Some((k, how)) => {
                                    let from = if how == "computed" && theta_from != "set here" {
                                        format!("{theta_from}, computed")
                                    } else if how == "computed" {
                                        "computed".into()
                                    } else {
                                        theta_from.clone()
                                    };
                                    factors.push(Factor { name: temp_name, factor: k, input: format!("{} \u{b0}C", dataset::fmt_num(theta)), from });
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
    // The overload checks (§3.4), on unrounded values.
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
    // Volt drop (cables only, §3.5).
    let voltdrop =
        if is_cable { volt_drop_of(doc, ctx, s, props, &inp, &sh, size, system, load.as_ref(), device.as_ref(), &mut issues) } else { None };
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
    doc: &Document, ctx: &Context, s: RatedSubject, props: Option<&ecad_model::Properties>, inp: &CircuitInputs, sh: &Sheet, size: Option<f64>,
    system: &str, load: Option<&Load>, device: Option<&Device>, issues: &mut Vec<Issue>,
) -> Option<VoltDrop> {
    let metres = props.and_then(|p| p.get("length")).and_then(|v| parse::length_m(v))?;
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
            dataset::fmt_num(size),
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
    let _ = s;
    Some(VoltDrop { volts: du, percent: pc, amps, why, limit, limit_from, larger })
}

/// Every rated subject of `doc`.
pub fn subjects(doc: &Document, ctx: &Context) -> Vec<RatedSubject> {
    let mut v: Vec<RatedSubject> = doc.wires.keys().filter(|c| wire_is_rated(doc, ctx, **c)).map(|c| RatedSubject::Wire(*c)).collect();
    v.extend(doc.cables.keys().filter(|k| cable_is_rated(doc, ctx, **k)).map(|k| RatedSubject::Cable(*k)));
    v
}

/// One subject's rating, traced now (the `ratings` command, the panels).
pub fn rating_of(doc: &Document, s: RatedSubject) -> Rating {
    let mut ctx = Context::new(doc);
    let found = trace::trace_subject(doc, s);
    evaluate(doc, &mut ctx, s, &found)
}
