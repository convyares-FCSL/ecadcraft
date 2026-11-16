//! Wire and cable rating checks (brief 057, design 057): is each panel wire and field cable big
//! enough for the device that protects it, **Ib ≤ In ≤ Iz**, and is a cable's volt drop within its
//! limit. Calculations are checks, not tools: every result is a row in the Checks list.
//!
//! # Semantics
//! - A **rated subject** ([`RatedSubject`]) is a panel wire (a conductor that is not a cable core)
//!   or a field cable that has rating data (design 057 §7): a wire whose category has a rated
//!   current, or with `rating.*`, `load` or `protected_by` of its own; a cable with a cable type,
//!   `rating.*`, `load` or `protected_by`, or a readable size when the project has chosen a rating
//!   table. Anything else makes no rows, so a project that does not rate wires is not nagged.
//! - Iz comes from one of three bases ([`Basis`], §3): the datasheet (category or cable type), the
//!   project's **rating table** (user-supplied, [`dataset`]; ECAD ships no values) or a typed value.
//!   Factors are rounded to 2 dp before multiplying; comparisons use the unrounded product.
//! - In comes from the protective device ([`trace`]), Ib from a [`load::LoadSource`].
//!
//! # Update map (design 057 §10)
//! [`Kept`] holds every subject's result and three indexes: `protects` (device → subjects),
//! `path` (net → subjects, from the trace) and `users` (wire category / cable type → subjects).
//! [`Kept::update`] re-evaluates only what an op reaches ([`Touch`]):
//!
//! | Op | Re-checked |
//! | --- | --- |
//! | a conductor's or cable's own property | that subject (re-traced) |
//! | a device's `rating` / `rating.<n>` | `protects[device]` (result only) |
//! | a device's `tag` | subjects it protects or that name it (result only) |
//! | a location box's `rating.temperature` or `location`, a device's `location` | every wire's result |
//! | connectivity (conductors, ends, points, components, definitions, links, cable members) | subjects whose path holds a touched net, and those on a touched net (re-traced) |
//! | `SetStyle` of a wire category or cable type | its users (result only) |
//! | `SetStyle` of a rating table, `SetStandard`, a project or standard `rating.*` / `wire.*` key, the potentials | every result (no trace) |
//! | anything else | nothing |
//!
//! This module reads the document; it changes nothing.

pub mod dataset;
pub mod load;
pub mod parse;
pub mod trace;

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::{CableId, ComponentId, ConductorId, Document, NetId, ObjectRef, Op, StyleRef};

use crate::defaults::{self, Scope};
use crate::styles::{self, cable, ratingtable, wire};
use dataset::{Dataset, Insulation};
use load::{Load, LoadFrom, LoadSource, TypedLoads};
use trace::{Found, How};

/// The style fields lane 057 adds (design 057 §4.4, FP3 and FP4): wire categories take the five
/// rating fields, cable types all of them.
pub mod fields {
    // Each catalogue that lists one of these holds its own copy in its static `FIELDS`, whose
    // slot is filled once there; the constants themselves are never read directly (brief 066).
    #![allow(clippy::declare_interior_mutable_const, reason = "copied into each catalogue's static field list")]

    use crate::defaults::{Kind, Slot};
    use crate::styles::Field;

    fn is_current(s: &str) -> bool {
        super::parse::current(s).is_some()
    }

    fn is_size(s: &str) -> bool {
        super::parse::cross_section(s).is_some()
    }

    pub const RATED_CURRENT: Field = Field {
        key: "rated_current",
        label: "Rated current (A)",
        kind: Kind::Parsed { max_chars: 32, parse: is_current },
        slot: Slot::new(),
        over: None,
    };
    pub const RATED_AT: Field =
        Field { key: "rated_at", label: "Rated at (\u{b0}C)", kind: Kind::Number { min: -40.0, max: 90.0 }, slot: Slot::new(), over: None };
    pub const MAX_TEMPERATURE: Field = Field {
        key: "max_temperature",
        label: "Max conductor temperature (\u{b0}C)",
        kind: Kind::Number { min: 50.0, max: 250.0 },
        slot: Slot::new(),
        over: None,
    };
    pub const MATERIAL: Field = Field { key: "material", label: "Material", kind: Kind::Choice(&["cu"]), slot: Slot::new(), over: None };
    pub const CONDUCTOR_CLASS: Field =
        Field { key: "conductor_class", label: "Conductor class", kind: Kind::Choice(&["1", "2", "5", "6"]), slot: Slot::new(), over: None };
    pub const CROSS_SECTION: Field = Field {
        key: "cross_section",
        label: "Cross-section (mm\u{b2})",
        kind: Kind::Parsed { max_chars: 32, parse: is_size },
        slot: Slot::new(),
        over: None,
    };
}

/// A refusal of a wire category or cable type record whose reference temperature is not below its
/// maximum (design 057 §3.1); `None` for every other record.
pub fn check_style(catalogue: &str, r: &ecad_model::style::StyleRecord) -> Option<String> {
    if catalogue != wire::CATALOGUE.word && catalogue != cable::WORD {
        return None;
    }
    let num = |k: &str| r.fields.get(k).and_then(|v| v.trim().parse::<f64>().ok()).filter(|x| x.is_finite());
    let at = num("rated_at").unwrap_or(30.0);
    let max = num("max_temperature").unwrap_or(70.0);
    (at >= max).then(|| format!("Rated at {} \u{b0}C is not below the conductor's {} \u{b0}C.", dataset::fmt_num(at), dataset::fmt_num(max)))
}

/// A panel wire or a field cable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RatedSubject {
    Conductor(ConductorId),
    Cable(CableId),
}

impl RatedSubject {
    pub fn object(self) -> ObjectRef {
        match self {
            RatedSubject::Conductor(c) => ObjectRef::Conductor(c),
            RatedSubject::Cable(k) => ObjectRef::Cable(k),
        }
    }

    fn props(self, doc: &Document) -> Option<&ecad_model::Properties> {
        match self {
            RatedSubject::Conductor(c) => doc.conductors.get(&c).map(|x| &x.properties),
            RatedSubject::Cable(k) => doc.cables.get(&k).map(|x| &x.properties),
        }
    }

    /// Its words in a row: the wire number (or `wire`), the cable tag.
    pub fn words(self, doc: &Document) -> String {
        match self {
            RatedSubject::Conductor(c) => doc
                .conductors
                .get(&c)
                .and_then(|x| x.properties.get(ecad_model::connectivity::WIRE_NUMBER))
                .filter(|n| !n.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| {
                    let conn = doc.connectivity();
                    conn.net_of_conductor(c)
                        .and_then(|n| conn.net(n))
                        .and_then(|n| ecad_model::wiring::net_number(doc, n))
                        .unwrap_or_else(|| "wire".into())
                }),
            RatedSubject::Cable(k) => crate::cables::cable_title(doc, k),
        }
    }
}

/// How Iz is found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basis {
    Datasheet,
    Table,
    Manual,
}

impl Basis {
    pub fn word(self) -> &'static str {
        match self {
            Basis::Datasheet => "datasheet",
            Basis::Table => "table",
            Basis::Manual => "manual",
        }
    }

    pub fn parse(t: &str) -> Option<Basis> {
        match t.trim().to_ascii_lowercase().as_str() {
            "datasheet" => Some(Basis::Datasheet),
            "table" | "iec table" => Some(Basis::Table),
            "manual" => Some(Basis::Manual),
            _ => None,
        }
    }
}

/// Property keys (FP1, FP2, FP6).
pub const BASIS: &str = "rating.basis";
pub const MANUAL: &str = "rating.manual";
pub const BUNDLING: &str = "rating.bundling";
pub const LOAD: &str = "load";
pub const PROTECTED_BY: &str = "protected_by";
pub const METHOD: &str = "rating.method";
pub const AMBIENT: &str = "rating.ambient";
pub const GROUPED: &str = "rating.grouped";
pub const VOLTAGE: &str = "rating.voltage";
pub const SYSTEM: &str = "rating.system";
pub const VD_LIMIT: &str = "rating.vd_limit";
/// A location box's panel temperature (FP6).
pub const TEMPERATURE: &str = "rating.temperature";

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

/// The device that protects a subject.
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub component: ComponentId,
    /// The fuse of a bank, or a strip's fuse position.
    pub fuse: Option<String>,
    /// `-Q01`, `-F01:3`.
    pub tag: String,
    /// In, when its rating reads.
    pub amps: Option<f64>,
    pub how: How,
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

/// A problem a result raises (one Checks row each; the count kinds are summed project-wide).
#[derive(Debug, Clone, PartialEq)]
pub enum Issue {
    /// Iz < In.
    Under {
        larger: Option<f64>,
    },
    /// The panel or ambient is at or above θmax.
    TooHot {
        place: String,
        temp: f64,
        max: f64,
    },
    LoadOver,
    VoltDropOver,
    BundlingUnset,
    /// `protected_by` names nothing.
    DeviceMissing(String),
    /// No device found (`true`: the trace stopped at its bound).
    NoDevice(bool),
    TwoDevices(String, String),
    /// A device whose rating does not read.
    DeviceUnread(ComponentId, String),
    NamedDevice(String),
    NoData(String),
    NoTable(String),
    Assumed,
    NoCurrent,
    BadValue(&'static str, String),
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

    fn new_without_table(doc: &Document) -> Self {
        let mut c = Context::new_inner(doc);
        c.table = None;
        c
    }

    pub fn new(doc: &Document) -> Self {
        let mut c = Context::new_inner(doc);
        c.table = ratingtable::chosen_table(doc).map(std::sync::Arc::new);
        c
    }

    fn new_inner(doc: &Document) -> Self {
        Context::with_boxes(doc, &box_temps(doc))
    }

    /// A context from kept location boxes (no scan of the components).
    fn with_boxes(doc: &Document, boxes: &BTreeMap<ComponentId, BoxTemp>) -> Self {
        let mut temps: BTreeMap<String, f64> = BTreeMap::new();
        for b in boxes.values() {
            if let Ok(t) = b.temp {
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
    pub temp: Result<f64, String>,
}

/// The temperature of location box `c`, if it is one and sets one.
pub fn box_temp(doc: &Document, c: ComponentId) -> Option<BoxTemp> {
    let x = doc.components.get(&c).filter(|x| crate::locations::is_box_component(x))?;
    let v = x.properties.get(TEMPERATURE)?;
    let location = x.properties.get(ecad_model::book::LOCATION_KEY).map(|l| l.trim().to_string()).unwrap_or_default();
    Some(BoxTemp { location, temp: parse::ambient(v).ok_or_else(|| v.chars().take(32).collect()) })
}

/// The code of location entry `key` (brief 052), or the key itself when the list has no entry.
pub fn location_code(doc: &Document, key: &str) -> String {
    crate::designations::code_of(doc, crate::designations::Aspect::Location, key).unwrap_or_else(|| key.to_string())
}

/// Every location box that sets a temperature (open, reload, first show).
pub fn box_temps(doc: &Document) -> BTreeMap<ComponentId, BoxTemp> {
    doc.components.keys().filter_map(|c| box_temp(doc, *c).map(|b| (*c, b))).collect()
}

fn project_number(doc: &Document, e: &defaults::Entry) -> f64 {
    defaults::resolve(doc, e, Scope::project()).number()
}

fn project_word(doc: &Document, e: &defaults::Entry) -> &'static str {
    defaults::resolve(doc, e, Scope::project()).word()
}

/// A project default's source in words.
fn default_from(doc: &Document, e: &defaults::Entry) -> &'static str {
    match defaults::resolve(doc, e, Scope::project()).from {
        defaults::Source::Standard => "standard",
        defaults::Source::Project => "project",
        _ => "project default",
    }
}

/// Whether the subject's properties name any rating key.
fn has_own(props: &ecad_model::Properties) -> bool {
    props.contains_key(LOAD) || props.contains_key(PROTECTED_BY) || props.keys().any(|k| k.starts_with("rating."))
}

/// Whether conductor `c` is a panel wire (not a cable core).
pub fn is_panel_wire(doc: &Document, c: ConductorId) -> bool {
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

fn sheet(doc: &Document, catalogue: &str, id: Option<&StyleRef>) -> Sheet {
    let f = |k: &str| id.and_then(|id| styles::field_of(doc, catalogue, id, k, None));
    Sheet {
        rated: f("rated_current").and_then(|r| parse::current(&r.text())),
        rated_at: f("rated_at").map_or(30.0, |r| r.number()),
        max: f("max_temperature").map_or(70.0, |r| r.number()),
        class: f("conductor_class").and_then(|r| r.word().parse().ok()).unwrap_or(5),
    }
}

/// Whether conductor `c` is a rated subject (design 057 §7).
pub fn wire_is_rated(doc: &Document, ctx: &Context, c: ConductorId) -> bool {
    if !is_panel_wire(doc, c) {
        return false;
    }
    let Some(x) = doc.conductors.get(&c) else { return false };
    if has_own(&x.properties) {
        return true;
    }
    let (cat, _) = ctx.looks.category(doc, c);
    sheet(doc, "wire", cat.as_ref()).rated.is_some()
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
pub fn panel_temperature(doc: &Document, ctx: &mut Context, c: ConductorId) -> (f64, String) {
    let (t, from, _) = panel_temperature_at(doc, ctx, c);
    (t, from)
}

/// [`panel_temperature`], and the locations of the wire's ends (the one used first).
fn panel_temperature_at(doc: &Document, ctx: &mut Context, c: ConductorId) -> (f64, String, Vec<String>) {
    let mut best: Option<(f64, String)> = None;
    let mut locations: Vec<String> = Vec::new();
    if let Some(x) = doc.conductors.get(&c) {
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
fn own<T>(props: Option<&ecad_model::Properties>, key: &'static str, parse: impl Fn(&str) -> Option<T>, issues: &mut Vec<Issue>) -> Option<T> {
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
    let own_basis = own(props, BASIS, Basis::parse, &mut issues);
    let default_basis = Basis::parse(project_word(doc, if is_cable { &defaults::rating::CABLE_BASIS } else { &defaults::rating::WIRE_BASIS }))
        .unwrap_or(if is_cable { Basis::Table } else { Basis::Datasheet });
    let basis = own_basis.unwrap_or(default_basis);
    // Datasheet values, size.
    let (sh, size, assumed) = match s {
        RatedSubject::Conductor(c) => {
            let look = ctx.looks.look(doc, c);
            let sh = sheet(doc, "wire", look.category.as_ref());
            (sh, look.cross_section.as_ref().and_then(|(t, _)| parse::cross_section(t)), false)
        }
        RatedSubject::Cable(k) => {
            let ty = cable::of(doc, k);
            let sh = sheet(doc, cable::WORD, ty.as_ref());
            let from_type = ty
                .as_ref()
                .and_then(|id| styles::field_of(doc, cable::WORD, id, "cross_section", None))
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
        RatedSubject::Conductor(c) => {
            let (t, from, at) = panel_temperature_at(doc, ctx, c);
            locations = at;
            let place = from.strip_prefix("location ").map_or_else(|| "The panel".to_string(), str::to_string);
            (t, from, place)
        }
        RatedSubject::Cable(_) => match own(props, AMBIENT, parse::ambient, &mut issues) {
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
        own(props, GROUPED, parse::count, &mut issues).map(|n| (parse::Bundling::Circuits(n), "set here".to_string()))
    } else {
        match own(props, BUNDLING, parse::bundling, &mut issues) {
            Some(b) => Some((b, "set here".to_string())),
            None => {
                let r = defaults::resolve(doc, &defaults::rating::BUNDLING, Scope::project());
                parse::bundling(&r.text()).map(|b| (b, default_from(doc, &defaults::rating::BUNDLING).to_string()))
            }
        }
    };
    let method: &'static str = if is_cable {
        own(props, METHOD, dataset::method, &mut issues).unwrap_or_else(|| project_word(doc, &defaults::rating::METHOD))
    } else {
        "B1"
    };
    let system = own(props, SYSTEM, |v| ["ac1", "ac3", "dc"].into_iter().find(|w| w.eq_ignore_ascii_case(v.trim())), &mut issues)
        .unwrap_or_else(|| project_word(doc, &defaults::rating::SYSTEM));
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
                match d.grouping(method, *n) {
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
        Basis::Manual => match own(props, MANUAL, parse::current, &mut issues) {
            Some(x) => Some(x),
            None => {
                issues.push(Issue::NoData("no Manual value set".into()));
                None
            }
        },
        Basis::Datasheet => match sh.rated {
            None => {
                let why = match s {
                    RatedSubject::Conductor(c) => match ctx.looks.category(doc, c).0 {
                        Some(id) => format!("category {} has no rated current", styles::name(doc, "wire", &id)),
                        None => "no wire category".into(),
                    },
                    RatedSubject::Cable(k) => match cable::of(doc, k) {
                        Some(id) => format!("cable type {} has no rated current", styles::name(doc, cable::WORD, &id)),
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
    let voltdrop = if is_cable { volt_drop_of(doc, ctx, s, props, &sh, size, system, load.as_ref(), device.as_ref(), &mut issues) } else { None };
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
fn larger_size(ctx: &Context, size: Option<f64>, method: &str, max: f64, loaded: u8, factors: &[Factor], i_n: f64) -> Option<f64> {
    let t = ctx.table.as_ref()?;
    let size = size?;
    let ins = Insulation::of_max(max)?;
    let k: f64 = factors.iter().map(|f| f.factor).product();
    t.data.capacity_sizes(method, ins, loaded).into_iter().find(|(s, cap)| *s > size + 1e-9 && cap * k >= i_n).map(|(s, _)| s)
}

#[allow(clippy::too_many_arguments)]
fn volt_drop_of(
    doc: &Document, ctx: &Context, s: RatedSubject, props: Option<&ecad_model::Properties>, sh: &Sheet, size: Option<f64>, system: &str,
    load: Option<&Load>, device: Option<&Device>, issues: &mut Vec<Issue>,
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
    let mut scratch = Vec::new();
    let (volts, vfrom) = match own(props, VOLTAGE, parse::voltage, &mut scratch) {
        Some(v) => (v, "set here"),
        None => (project_number(doc, &defaults::rating::VOLTAGE), default_from(doc, &defaults::rating::VOLTAGE)),
    };
    let (limit, limit_from) = match own(props, VD_LIMIT, parse::percent, &mut scratch) {
        Some(v) => (v, "set here"),
        None => (project_number(doc, &defaults::rating::VD_LIMIT), default_from(doc, &defaults::rating::VD_LIMIT)),
    };
    issues.extend(scratch);
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

/// The device of subject `s`: its `protected_by`, else the trace's.
fn device_of(doc: &Document, s: RatedSubject, found: &Found, issues: &mut Vec<Issue>) -> Option<Device> {
    if let Some(named) = s.props(doc).and_then(|p| p.get(PROTECTED_BY)).filter(|v| !v.trim().is_empty()) {
        return match trace::named(doc, named) {
            Some((c, fuse)) => {
                let amps = trace::amps_of(doc, c, fuse.as_deref());
                let tag = trace::device_words(doc, c, fuse.as_deref());
                if amps.is_none() {
                    issues.push(Issue::DeviceUnread(c, trace::rating_text(doc, c, fuse.as_deref())));
                }
                if !trace::is_protective_component(doc, c) && fuse.is_none() {
                    issues.push(Issue::NamedDevice(tag.clone()));
                }
                Some(Device { component: c, fuse, tag, amps, how: How::Named })
            }
            None => {
                issues.push(Issue::DeviceMissing(named.chars().take(32).collect()));
                None
            }
        };
    }
    let mut devs: Vec<Device> = found
        .devices
        .iter()
        .map(|(c, fuse)| Device {
            component: *c,
            fuse: fuse.clone(),
            tag: trace::device_words(doc, *c, fuse.as_deref()),
            amps: trace::amps_of(doc, *c, fuse.as_deref()),
            how: How::Traced,
        })
        .collect();
    for d in &devs {
        if d.amps.is_none() {
            issues.push(Issue::DeviceUnread(d.component, trace::rating_text(doc, d.component, d.fuse.as_deref())));
        }
    }
    devs.sort_by(|a, b| b.amps.unwrap_or(0.0).total_cmp(&a.amps.unwrap_or(0.0)).then(a.tag.cmp(&b.tag)));
    if devs.is_empty() {
        issues.push(Issue::NoDevice(found.bounded));
        return None;
    }
    if let [first, second, ..] = devs.as_slice() {
        issues.push(Issue::TwoDevices(first.tag.clone(), second.tag.clone()));
    }
    devs.into_iter().next()
}

/// Every rated subject of `doc`.
pub fn subjects(doc: &Document, ctx: &Context) -> Vec<RatedSubject> {
    let mut v: Vec<RatedSubject> = doc.conductors.keys().filter(|c| wire_is_rated(doc, ctx, **c)).map(|c| RatedSubject::Conductor(*c)).collect();
    v.extend(doc.cables.keys().filter(|k| cable_is_rated(doc, ctx, **k)).map(|k| RatedSubject::Cable(*k)));
    v
}

/// One subject's rating, traced now (the `ratings` command, the panels).
pub fn rating_of(doc: &Document, s: RatedSubject) -> Rating {
    let mut ctx = Context::new(doc);
    let found = trace::trace_subject(doc, s);
    evaluate(doc, &mut ctx, s, &found)
}

/// The count kinds' tallies, kept by [`Kept`] without a rescan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tally {
    pub bundling_unset: usize,
    pub no_device: usize,
    pub no_data: usize,
    pub assumed: usize,
    pub no_current: usize,
}

/// What an op reaches (collected on the state before and after, as `checks::Scope` does).
#[derive(Debug, Clone, Default)]
pub struct Touch {
    pub subjects: BTreeSet<RatedSubject>,
    /// Re-trace these too.
    pub retrace: BTreeSet<RatedSubject>,
    pub nets: BTreeSet<NetId>,
    pub devices: BTreeSet<ComponentId>,
    pub styles: BTreeSet<(String, StyleRef)>,
    /// Locations whose temperature changed: `at[location]` (D1).
    pub locations: BTreeSet<String>,
    /// Location boxes to read again.
    pub boxes: BTreeSet<ComponentId>,
    /// Devices whose location may have changed: their wires' results.
    pub moved: BTreeSet<ComponentId>,
    /// Pages a location box moved on: the wires of the devices drawn there.
    pub pages: BTreeSet<ecad_model::PageId>,
    /// Every result again (no re-trace).
    pub all_results: bool,
    /// The rating table's own rows (bad rows, odd pairs) again.
    pub table: bool,
}

impl Touch {
    /// Add what `ops` reach in `doc`.
    pub fn collect(&mut self, doc: &Document, ops: &[Op]) {
        let conn = doc.connectivity();
        let net_of_terminal = |t: ecad_model::TerminalId, s: &mut Touch| {
            if let Some(n) = conn.net_of(&ecad_model::Node::Terminal(t)) {
                s.nets.insert(n);
            }
        };
        for op in ops {
            match op {
                Op::SetProperty { object: ObjectRef::Conductor(c), key, .. } => {
                    self.retrace.insert(RatedSubject::Conductor(*c));
                    if (key == ecad_model::connectivity::POTENTIAL || key == ecad_model::connectivity::WIRE_NUMBER)
                        && let Some(n) = conn.net_of_conductor(*c)
                    {
                        self.nets.insert(n);
                    }
                    for k in doc.cables_of(*c) {
                        self.retrace.insert(RatedSubject::Cable(k));
                    }
                }
                Op::SetProperty { object: ObjectRef::Cable(k), .. } => {
                    self.retrace.insert(RatedSubject::Cable(*k));
                }
                Op::SetProperty { object: ObjectRef::Component(c), key, before, after } => {
                    let is_box = doc.components.get(c).is_some_and(crate::locations::is_box_component);
                    if is_box {
                        // A box's temperature or location: the wires at its locations (§10).
                        if key == TEMPERATURE || key == ecad_model::book::LOCATION_KEY {
                            self.boxes.insert(*c);
                            self.box_locations(doc, *c);
                            if key == ecad_model::book::LOCATION_KEY {
                                self.locations.extend([before, after].into_iter().flatten().map(|l| l.trim().to_string()));
                                self.box_pages(doc, *c);
                            }
                        }
                    } else if crate::symbols::is_rating_key(key) || key == ecad_model::tag::TAG_KEY {
                        self.devices.insert(*c);
                    } else if key == ecad_model::book::LOCATION_KEY {
                        // A device's own location: its wires only.
                        self.moved.insert(*c);
                    } else if key.starts_with("link.") {
                        for (t, _) in doc.terminals_of(*c) {
                            net_of_terminal(t, self);
                        }
                    }
                }
                Op::SetProperty { object: ObjectRef::Project | ObjectRef::Standard, key, .. } => {
                    // Brief 052: a location entry's code changed (or the entry came or went): the
                    // rows naming it are worded again.
                    if let Some((crate::designations::Aspect::Location, k, true)) = crate::designations::parse_list_property(key) {
                        self.locations.insert(k.to_string());
                    }
                    if key.starts_with("rating.") || key.starts_with("wire.") {
                        self.all_results = true;
                        self.table |= key == defaults::rating::TABLE.key;
                    }
                }
                Op::SetStyle { catalogue, id, .. } => {
                    if catalogue == ratingtable::WORD {
                        self.all_results = true;
                        self.table = true;
                    } else if catalogue == wire::CATALOGUE.word || catalogue == cable::WORD {
                        self.styles.insert((catalogue.clone(), id.clone()));
                        // A category naming a potential reaches every wire of it.
                        if catalogue == wire::CATALOGUE.word {
                            self.all_results = true;
                        }
                    }
                }
                Op::SetStandard { .. } => {
                    self.all_results = true;
                    self.table = true;
                }
                Op::SetProjectSettings { before, after } => {
                    if before.wiring.potentials != after.wiring.potentials {
                        self.all_results = true;
                    }
                }
                Op::CreateConductor { id, conductor } | Op::DeleteConductor { id, conductor } => {
                    self.retrace.insert(RatedSubject::Conductor(*id));
                    if let Some(n) = conn.net_of_conductor(*id) {
                        self.nets.insert(n);
                    }
                    for t in conductor.ends.iter().flatten() {
                        net_of_terminal(*t, self);
                    }
                }
                Op::AttachEnd { conductor, before, after, .. } => {
                    self.retrace.insert(RatedSubject::Conductor(*conductor));
                    if let Some(n) = conn.net_of_conductor(*conductor) {
                        self.nets.insert(n);
                    }
                    for t in [before, after].into_iter().flatten() {
                        net_of_terminal(*t, self);
                    }
                }
                Op::AddTerminal { id, .. } | Op::RemoveTerminal { id, .. } => net_of_terminal(*id, self),
                Op::CreateComponent { id, component } | Op::DeleteComponent { id, component } if crate::locations::is_box_component(component) => {
                    self.boxes.insert(*id);
                    if let Some(l) = component.properties.get(ecad_model::book::LOCATION_KEY) {
                        self.locations.insert(l.trim().to_string());
                    }
                    self.box_pages(doc, *id);
                }
                Op::CreateComponent { id, .. } | Op::DeleteComponent { id, .. } | Op::SetComponentDefinition { id, .. } => {
                    self.devices.insert(*id);
                    for (t, _) in doc.terminals_of(*id) {
                        net_of_terminal(t, self);
                    }
                }
                Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => {
                    for c in doc.components_using(*id) {
                        for (t, _) in doc.terminals_of(c) {
                            net_of_terminal(t, self);
                        }
                    }
                }
                Op::CreateCable { id, cable } | Op::DeleteCable { id, cable } => {
                    self.retrace.insert(RatedSubject::Cable(*id));
                    for m in &cable.members {
                        self.retrace.insert(RatedSubject::Conductor(m.conductor));
                    }
                }
                Op::AddCableMember { cable, member, .. } | Op::RemoveCableMember { cable, member, .. } => {
                    self.retrace.insert(RatedSubject::Cable(*cable));
                    self.retrace.insert(RatedSubject::Conductor(member.conductor));
                }
                // A device moved may enter another location box: its wires; a box moved: the wires at
                // its location and those of the devices on its page.
                Op::ModifyRepresentation { after, .. } | Op::LinkRepresentation { rep: after, .. } | Op::UnlinkRepresentation { rep: after, .. } => {
                    if let ecad_model::Subject::Component(c) = after.subject {
                        if doc.components.get(&c).is_some_and(crate::locations::is_box_component) {
                            self.boxes.insert(c);
                            self.box_locations(doc, c);
                            self.pages.insert(after.page);
                        } else {
                            self.moved.insert(c);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// The kept results and indexes (design 057 §10).
#[derive(Debug, Clone, Default)]
pub struct Kept {
    pub results: BTreeMap<RatedSubject, Rating>,
    traces: BTreeMap<RatedSubject, Found>,
    /// device → subjects it protects (by the trace or by name).
    protects: BTreeMap<ComponentId, BTreeSet<RatedSubject>>,
    /// net → subjects whose trace crossed it.
    path: BTreeMap<NetId, BTreeSet<RatedSubject>>,
    tally: Tally,
    /// Devices whose rating does not read → how many results name them.
    unread: BTreeMap<ComponentId, (usize, String)>,
    /// The rating table's own rows.
    table_rows: Vec<Issue>,
    /// How many subjects the last build or update evaluated, and re-traced (the update map's
    /// counters, design 057 §11 check 16).
    pub evaluated: usize,
    pub retraced: usize,
    /// Bumped by every build and update; with [`Kept::last`], a view that copied the results
    /// at epoch `n` patches only `last` at `n + 1` (design 057 §10: tables patch the re-checked
    /// rows only).
    pub epoch: u64,
    /// The subjects the last update re-checked.
    pub last: BTreeSet<RatedSubject>,
    /// The methods the chosen rating table has capacity rows for (the Method drop-down).
    pub methods: Vec<&'static str>,
    /// The chosen rating table as last parsed; read again only when an op reaches it.
    table: Option<std::sync::Arc<ratingtable::Table>>,
    /// Location boxes that set a temperature (read again only when an op reaches one).
    boxes: BTreeMap<ComponentId, BoxTemp>,
    /// location → the wires whose ends are there (design 057 §10, `at`).
    at: BTreeMap<String, BTreeSet<RatedSubject>>,
    /// Panel wires a device protects that have no rating data (`rating-no-data`, §7 and check 13),
    /// kept while the project rates anything.
    unrated: BTreeSet<ConductorId>,
}

impl Kept {
    /// Every subject, from scratch (open, reload, first show).
    pub fn build(doc: &Document) -> Self {
        let mut k = Kept { boxes: box_temps(doc), ..Kept::default() };
        let mut ctx = Context::with_boxes(doc, &k.boxes);
        ctx.table = ratingtable::chosen_table(doc).map(std::sync::Arc::new);
        k.table = ctx.table.clone();
        for s in subjects(doc, &ctx) {
            k.put(doc, &mut ctx, s, true);
        }
        k.unrated_pass(doc, &mut ctx);
        k.table_rows = table_issues(&ctx);
        k.methods = ctx.table.as_ref().map(|t| t.data.methods()).unwrap_or_default();
        k.epoch = 1;
        k
    }

    fn untally(&mut self, r: &Rating) {
        let t = &mut self.tally;
        for i in &r.issues {
            match i {
                Issue::BundlingUnset => t.bundling_unset = t.bundling_unset.saturating_sub(1),
                Issue::NoDevice(false) => t.no_device = t.no_device.saturating_sub(1),
                Issue::NoData(_) => t.no_data = t.no_data.saturating_sub(1),
                Issue::Assumed => t.assumed = t.assumed.saturating_sub(1),
                Issue::NoCurrent => t.no_current = t.no_current.saturating_sub(1),
                Issue::DeviceUnread(c, _) => {
                    if let Some(e) = self.unread.get_mut(c) {
                        e.0 = e.0.saturating_sub(1);
                        if e.0 == 0 {
                            self.unread.remove(c);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn tally(&mut self, r: &Rating) {
        let t = &mut self.tally;
        for i in &r.issues {
            match i {
                Issue::BundlingUnset => t.bundling_unset += 1,
                Issue::NoDevice(false) => t.no_device += 1,
                Issue::NoData(_) => t.no_data += 1,
                Issue::Assumed => t.assumed += 1,
                Issue::NoCurrent => t.no_current += 1,
                Issue::DeviceUnread(c, text) => self.unread.entry(*c).or_insert((0, text.clone())).0 += 1,
                _ => {}
            }
        }
    }

    fn forget(&mut self, s: RatedSubject) {
        if let RatedSubject::Conductor(c) = s {
            self.unrated.remove(&c);
        }
        if let Some(old) = self.results.remove(&s) {
            self.untally(&old);
            for l in &old.locations {
                if let Some(set) = self.at.get_mut(l) {
                    set.remove(&s);
                    if set.is_empty() {
                        self.at.remove(l);
                    }
                }
            }
            if let Some(d) = &old.device
                && let Some(set) = self.protects.get_mut(&d.component)
            {
                set.remove(&s);
            }
        }
        if let Some(f) = self.traces.remove(&s) {
            for n in &f.path {
                if let Some(set) = self.path.get_mut(n) {
                    set.remove(&s);
                    if set.is_empty() {
                        self.path.remove(n);
                    }
                }
            }
            for (c, _) in &f.devices {
                if let Some(set) = self.protects.get_mut(c) {
                    set.remove(&s);
                }
            }
        }
    }

    /// Evaluate `s` again (re-tracing it when `retrace`), or drop it when it is no longer rated.
    fn put(&mut self, doc: &Document, ctx: &mut Context, s: RatedSubject, retrace: bool) {
        let rated = match s {
            RatedSubject::Conductor(c) => wire_is_rated(doc, ctx, c),
            RatedSubject::Cable(k) => cable_is_rated(doc, ctx, k),
        };
        let found = if retrace || !self.traces.contains_key(&s) { None } else { self.traces.get(&s).cloned() };
        self.forget(s);
        if !rated {
            // A protected panel wire with no rating data, counted while the project rates
            // anything (§7, check 13).
            if let RatedSubject::Conductor(c) = s
                && !self.results.is_empty()
                && is_panel_wire(doc, c)
                && doc.conductors.contains_key(&c)
            {
                let f = found.unwrap_or_else(|| {
                    self.retraced += 1;
                    trace::trace_subject(doc, s)
                });
                if !f.devices.is_empty() {
                    self.unrated.insert(c);
                    // On the path index (a connectivity change re-traces it), not on `protects`: a
                    // device's rating does not change whether it has rating data.
                    for n in &f.path {
                        self.path.entry(*n).or_default().insert(s);
                    }
                    self.traces.insert(s, f);
                }
            }
            return;
        }
        self.evaluated += 1;
        let found = match found {
            Some(f) => f,
            None => {
                self.retraced += 1;
                trace::trace_subject(doc, s)
            }
        };
        let r = evaluate(doc, ctx, s, &found);
        for n in &found.path {
            self.path.entry(*n).or_default().insert(s);
        }
        for (c, _) in &found.devices {
            self.protects.entry(*c).or_default().insert(s);
        }
        if let Some(d) = &r.device {
            self.protects.entry(d.component).or_default().insert(s);
        }
        self.tally(&r);
        for l in &r.locations {
            self.at.entry(l.clone()).or_default().insert(s);
        }
        self.traces.insert(s, found);
        self.results.insert(s, r);
    }

    /// The protected panel wires with no rating data, from scratch: while the project rates
    /// anything, one pass over the panel wires (open, and the rare ops that change what is rated);
    /// none otherwise.
    fn unrated_pass(&mut self, doc: &Document, ctx: &mut Context) {
        if self.results.is_empty() {
            for c in std::mem::take(&mut self.unrated) {
                self.forget(RatedSubject::Conductor(c));
            }
            return;
        }
        let wires: Vec<ConductorId> = doc.conductors.keys().copied().filter(|c| !self.results.contains_key(&RatedSubject::Conductor(*c))).collect();
        for c in wires {
            self.put(doc, ctx, RatedSubject::Conductor(c), false);
        }
    }

    /// How many protected wires have no rating data.
    pub fn unrated(&self) -> usize {
        self.unrated.len()
    }

    /// Re-check what `touch` reaches on `doc` (the state after the ops). Returns the subjects
    /// whose rows may have changed.
    pub fn update(&mut self, doc: &Document, touch: &Touch) -> BTreeSet<RatedSubject> {
        self.evaluated = 0;
        self.retraced = 0;
        // The rating table is parsed again only when an op reaches it (FP8).
        let fresh = touch.table || touch.all_results;
        // Location boxes reached are read again (D1); the rest are kept.
        for c in &touch.boxes {
            match box_temp(doc, *c) {
                Some(b) => self.boxes.insert(*c, b),
                None => self.boxes.remove(c),
            };
        }
        let mut ctx = Context::with_boxes(doc, &self.boxes);
        ctx.table = if fresh { ratingtable::chosen_table(doc).map(std::sync::Arc::new) } else { self.table.clone() };
        if fresh {
            self.table = ctx.table.clone();
        }
        let mut retrace: BTreeSet<RatedSubject> = touch.retrace.clone();
        for n in &touch.nets {
            if let Some(set) = self.path.get(n) {
                retrace.extend(set.iter().copied());
            }
        }
        // New subjects on touched nets: the conductors there and their cables.
        let conn = doc.connectivity();
        for n in &touch.nets {
            if let Some(net) = conn.net(*n) {
                for c in &net.conductors {
                    retrace.insert(RatedSubject::Conductor(*c));
                    for k in doc.cables_of(*c) {
                        retrace.insert(RatedSubject::Cable(k));
                    }
                }
            }
        }
        let mut results: BTreeSet<RatedSubject> = BTreeSet::new();
        for c in &touch.devices {
            if let Some(set) = self.protects.get(c) {
                results.extend(set.iter().copied());
            }
        }
        // Subjects that name a device by hand.
        if !touch.devices.is_empty() {
            results.extend(self.results.iter().filter(|(_, r)| r.device.as_ref().is_some_and(|d| d.how == How::Named)).map(|(s, _)| *s));
        }
        for (cat, id) in &touch.styles {
            if cat == cable::WORD {
                results.extend(cable::users(doc, id).into_iter().map(RatedSubject::Cable));
            }
        }
        // D1: a location's temperature reaches the wires at it; a device moved, its wires; a box
        // moved, the wires of the devices on its page.
        for l in &touch.locations {
            if let Some(set) = self.at.get(l) {
                results.extend(set.iter().copied());
            }
        }
        let mut moved: BTreeSet<ComponentId> = touch.moved.clone();
        for page in &touch.pages {
            moved.extend(doc.representations_on(*page).filter_map(|(_, r)| match r.subject {
                ecad_model::Subject::Component(c) => Some(c),
                _ => None,
            }));
        }
        for c in &moved {
            for (t, _) in doc.terminals_of(*c) {
                results.extend(doc.ends_at(t).into_iter().map(|(w, _)| RatedSubject::Conductor(w)));
            }
        }
        if touch.all_results || touch.table {
            results.extend(self.results.keys().copied());
            // Subjects that become rated by the change (a category gaining a rated current, a
            // table chosen): one pass over the wires and cables, on these rare ops only.
            results.extend(subjects(doc, &ctx));
        }
        let mut changed = BTreeSet::new();
        for s in &retrace {
            self.put(doc, &mut ctx, *s, true);
            changed.insert(*s);
        }
        for s in results.difference(&retrace) {
            self.put(doc, &mut ctx, *s, false);
            changed.insert(*s);
        }
        if touch.all_results || touch.table {
            self.unrated_pass(doc, &mut ctx);
        }
        if touch.table || touch.all_results {
            self.table_rows = table_issues(&ctx);
            self.methods = ctx.table.as_ref().map(|t| t.data.methods()).unwrap_or_default();
        }
        self.epoch += 1;
        self.last = changed.clone();
        changed
    }

    pub fn tally_now(&self) -> &Tally {
        &self.tally
    }

    /// Location boxes whose temperature does not read: (box, location, text).
    pub fn bad_boxes(&self) -> impl Iterator<Item = (ComponentId, &str, &str)> {
        self.boxes.iter().filter_map(|(c, b)| b.temp.as_ref().err().map(|v| (*c, b.location.as_str(), v.as_str())))
    }

    /// Devices whose rating does not read, with their rating text.
    pub fn unread(&self) -> impl Iterator<Item = (ComponentId, &str)> {
        self.unread.iter().map(|(c, (_, t))| (*c, t.as_str()))
    }

    pub fn table_rows(&self) -> &[Issue] {
        &self.table_rows
    }

    /// The subjects a device protects.
    pub fn protected_by(&self, c: ComponentId) -> usize {
        self.protects.get(&c).map_or(0, BTreeSet::len)
    }

    /// Whether any subject uses the Table basis.
    pub fn any_table(&self) -> bool {
        self.results.values().any(|r| r.basis == Basis::Table)
    }
}

/// The rating table's own rows: bad rows, odd pairs (§5.2, §5.4).
fn table_issues(ctx: &Context) -> Vec<Issue> {
    let Some(t) = &ctx.table else { return Vec::new() };
    let mut v = Vec::new();
    if let Some(why) = &t.data.too_big {
        v.push(Issue::NoTable(format!("Rating table '{}': {why}", t.name)));
    }
    if t.data.bad_count > 0 {
        let first = t.data.bad.first().map(|(n, why)| format!("line {n}, {why}")).unwrap_or_default();
        v.push(Issue::BadValue(
            "rows",
            format!("Rating table '{}': {} row{} not read (first: {first})", t.name, t.data.bad_count, if t.data.bad_count == 1 { "" } else { "s" }),
        ));
    }
    if let Some(odd) = t.data.odd() {
        v.push(Issue::NoData(format!("Rating table '{}': {odd}", t.name)));
    }
    v
}

impl Touch {
    fn box_locations(&mut self, doc: &Document, c: ComponentId) {
        if let Some(l) = doc.components.get(&c).and_then(|x| x.properties.get(ecad_model::book::LOCATION_KEY)) {
            self.locations.insert(l.trim().to_string());
        }
    }

    fn box_pages(&mut self, doc: &Document, c: ComponentId) {
        self.pages.extend(doc.representations_of(ecad_model::Subject::Component(c)).map(|(_, r)| r.page));
    }

    /// Whether the ops reach anything here.
    pub fn any(&self) -> bool {
        !(self.subjects.is_empty()
            && self.retrace.is_empty()
            && self.nets.is_empty()
            && self.devices.is_empty()
            && self.styles.is_empty()
            && self.locations.is_empty()
            && self.boxes.is_empty()
            && self.moved.is_empty()
            && self.pages.is_empty())
            || self.all_results
            || self.table
    }
}

use crate::checks::{Check, Fix, Kind as CheckKind, Object};

/// Whether `k` is one of lane 057's kinds.
pub fn is_rating_kind(k: CheckKind) -> bool {
    k.name().starts_with("rating-") || k == CheckKind::VoltdropOver
}

/// Where a subject's rows point.
fn place(doc: &Document, s: RatedSubject) -> (Option<ecad_model::PageId>, Option<ecad_model::Pt>, Object) {
    match s {
        RatedSubject::Conductor(c) => {
            let (p, a) = crate::checks::conductor_place(doc, c, None);
            (p, a, Object::Conductor(c))
        }
        RatedSubject::Cable(k) => {
            let (p, a) = ecad_model::numbering::rep_anchor(doc, ecad_model::Subject::Cable(k)).map_or((None, None), |(p, a)| (Some(p), Some(a)));
            (p, a, Object::Cable(k))
        }
    }
}

/// A subject's own rows (design 057 §7); the count kinds are left to [`summary_rows`].
pub fn rows(doc: &Document, r: &Rating) -> Vec<Check> {
    let s = r.subject;
    let (page, at, object) = place(doc, s);
    let who = s.words(doc);
    let show = Some(Fix::ShowRating(s));
    let dev = r.device.as_ref();
    let i_n = dev.and_then(|d| d.amps);
    let mut out = Vec::new();
    let mut row = |kind: CheckKind, which: String, fix: Option<Fix>| out.push(Check { kind, page, at, object, which, fix });
    for i in &r.issues {
        match i {
            Issue::Under { larger } => {
                let (Some(iz), Some(d), Some(a)) = (r.iz, dev, i_n) else { continue };
                let fix = match (larger, s) {
                    (Some(sz), RatedSubject::Conductor(c)) => Some(Fix::UseSize(c, *sz)),
                    (Some(sz), RatedSubject::Cable(k)) => Some(Fix::PickCableType(k, *sz)),
                    _ => show,
                };
                row(CheckKind::RatingUnder, format!("{who} carries {} A; {} is {} A", fmt_amps(iz, Some(a)), d.tag, dataset::fmt_num(a)), fix);
            }
            Issue::TooHot { place, temp, max } => row(
                CheckKind::RatingTooHot,
                format!("{place} at {} \u{b0}C is not below {who}'s {} \u{b0}C", dataset::fmt_num(*temp), dataset::fmt_num(*max)),
                show,
            ),
            Issue::LoadOver => {
                let (Some(l), Some(d), Some(a)) = (r.load.as_ref(), dev, i_n) else { continue };
                row(
                    CheckKind::RatingLoadOver,
                    format!("{who} load {} A is above {}'s {} A", dataset::fmt_num(l.amps), d.tag, dataset::fmt_num(a)),
                    show,
                );
            }
            Issue::VoltDropOver => {
                let Some(v) = &r.voltdrop else { continue };
                let larger =
                    v.larger.map(|(sz, p)| format!(" ({} mm\u{b2} would drop {} %)", dataset::fmt_num(sz), fmt_percent(p))).unwrap_or_default();
                row(
                    CheckKind::VoltdropOver,
                    format!(
                        "{who} drops {} % at {} A; limit {} %{larger}",
                        fmt_percent(v.percent),
                        dataset::fmt_num(v.amps),
                        dataset::fmt_num(v.limit)
                    ),
                    show,
                );
            }
            Issue::DeviceMissing(t) => {
                row(CheckKind::RatingDeviceMissing, format!("{who} names {t} as its protection; there is no {t}"), Some(Fix::ClearProtectedBy(s)))
            }
            Issue::TwoDevices(a, b) => {
                let used = i_n.map(|x| format!("; {} A ({a}) used", dataset::fmt_num(x))).unwrap_or_default();
                row(CheckKind::RatingTwoDevices, format!("{who} is fed by {a} and {b}{used}"), None);
            }
            // F6 (§4.2 step 3): the bound, said in words.
            Issue::NoDevice(true) => {
                row(CheckKind::RatingNoDevice, format!("{who}: trace stopped after {} nets; set Protected by", trace::MAX_TRACE_NETS), show)
            }
            Issue::NamedDevice(t) => {
                row(CheckKind::RatingNamedDevice, format!("{who} is protected by {t} by hand; {t} is not a protective device"), None)
            }
            Issue::NoTable(why) => {
                let fix = if why.starts_with("No rating table chosen") {
                    Some(Fix::ChooseRatingTable)
                } else if r.basis == Basis::Table && has_datasheet(doc, s) {
                    Some(Fix::SetBasis(s, Basis::Datasheet))
                } else {
                    None
                };
                row(CheckKind::RatingNoTable, format!("{who}: {why}"), fix);
            }
            Issue::BadValue(key, v) => row(CheckKind::RatingBadValue, format!("{who} has {key} '{v}', which ECAD does not read"), show),
            _ => {}
        }
    }
    out
}

/// Whether a subject's category or cable type has a rated current (the **Use Datasheet** fix).
fn has_datasheet(doc: &Document, s: RatedSubject) -> bool {
    match s {
        RatedSubject::Conductor(c) => sheet(doc, "wire", wire::WireLooks::new(doc).category(doc, c).0.as_ref()).rated.is_some(),
        RatedSubject::Cable(k) => sheet(doc, cable::WORD, cable::of(doc, k).as_ref()).rated.is_some(),
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {many}") }
}

/// The project-wide rows (design 057 §7): the count kinds, each unread device once, and the
/// rating table's own rows. Built from [`Kept`]'s tallies; nothing is rescanned.
pub fn summary_rows(doc: &Document, k: &Kept) -> Vec<Check> {
    let t = k.tally_now();
    let mut out = Vec::new();
    let mut row = |kind: CheckKind, which: String, fix: Option<Fix>| out.push(Check { kind, page: None, at: None, object: Object::None, which, fix });
    if t.bundling_unset > 0 {
        let verb = if t.bundling_unset == 1 { "has" } else { "have" };
        row(
            CheckKind::RatingBundlingUnset,
            format!("{} {verb} no bundling factor (\u{d7} 1.00 used)", plural(t.bundling_unset, "panel wire", "panel wires")),
            Some(Fix::ChooseRatingTable),
        );
    }
    if t.no_device > 0 {
        row(
            CheckKind::RatingNoDevice,
            format!("{} no protective device upstream", plural(t.no_device, "rated wire or cable has", "rated wires and cables have")),
            Some(Fix::ShowRatedWires),
        );
    }
    // §7, check 13: rated subjects lacking a value, and protected wires with no rating data.
    let no_data = t.no_data + k.unrated();
    if no_data > 0 {
        row(
            CheckKind::RatingNoData,
            format!("{} no rating data", plural(no_data, "protected wire or cable has", "protected wires and cables have")),
            Some(Fix::OpenStandards("wire")),
        );
    }
    if t.assumed > 0 {
        row(
            CheckKind::RatingAssumed,
            format!("{} no cable type; Cu, 70 \u{b0}C, class 5 assumed", plural(t.assumed, "cable has", "cables have")),
            Some(Fix::OpenStandards(cable::WORD)),
        );
    }
    if t.no_current > 0 {
        row(CheckKind::RatingNoCurrent, format!("{} no load and no device; no volt drop", plural(t.no_current, "cable has", "cables have")), None);
    }
    for (c, text) in k.unread() {
        let tag = crate::devices::tag_words(doc, c);
        let (page, at) = ecad_model::numbering::rep_anchor(doc, ecad_model::Subject::Component(c)).map_or((None, None), |(p, a)| (Some(p), Some(a)));
        out.push(Check {
            kind: CheckKind::RatingDeviceUnread,
            page,
            at,
            object: Object::Component(c),
            which: format!("{tag}'s rating '{text}' has no current; write it as 16 A"),
            fix: None,
        });
    }
    for (c, loc, v) in k.bad_boxes() {
        // Brief 052: the box stores an entry key; the row names the entry's code.
        let loc = if loc.is_empty() { String::new() } else { location_code(doc, loc) };
        let (page, at) = ecad_model::numbering::rep_anchor(doc, ecad_model::Subject::Component(c)).map_or((None, None), |(p, a)| (Some(p), Some(a)));
        out.push(Check {
            kind: CheckKind::RatingBadValue,
            page,
            at,
            object: Object::Component(c),
            which: format!(
                "{} has {TEMPERATURE} '{v}', which ECAD does not read (\u{2212}40 to 90 \u{b0}C)",
                if loc.is_empty() { "A location box" } else { loc.as_str() }
            ),
            fix: None,
        });
    }
    for i in k.table_rows() {
        let (kind, which) = match i {
            Issue::BadValue(_, w) | Issue::NoTable(w) => (CheckKind::RatingTableBadRow, w.clone()),
            Issue::NoData(w) => (CheckKind::RatingTableOdd, w.clone()),
            _ => continue,
        };
        out.push(Check { kind, page: None, at: None, object: Object::None, which, fix: Some(Fix::OpenStandards(ratingtable::WORD)) });
    }
    out
}
