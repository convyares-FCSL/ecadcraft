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
//! The calculations are split by side (design 072 §2.4): [`circuit`] works out a subject's Iz,
//! factors and volt drop (lane D's `Circuit`), [`protection`] the device that protects it (lane
//! B's `Protects`); this module keeps the shared words, the update map and the Checks rows.
//!
//! This module reads the document; it changes nothing.

pub mod circuit;
pub mod dataset;
pub mod load;
pub mod parse;
pub mod protection;
pub mod trace;

pub use circuit::*;
pub use protection::*;

use ecad_model::keys::PropsExt;
use std::collections::{BTreeMap, BTreeSet};

use ecad_model::{CableId, ConductorId, DeviceId, Document, NetId, ObjectRef, Op, StyleRef};

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
    pub const MATERIAL: Field =
        Field { key: "material", label: "Material", kind: Kind::Choice(<super::Material as ecad_model::Word>::WORDS), slot: Slot::new(), over: None };
    pub const CONDUCTOR_CLASS: Field = Field {
        key: "conductor_class",
        label: "Conductor class",
        kind: Kind::Choice(<super::ConductorClass as ecad_model::Word>::WORDS),
        slot: Slot::new(),
        over: None,
    };
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
    DeviceUnread(DeviceId, String),
    NamedDevice(String),
    NoData(String),
    NoTable(String),
    Assumed,
    NoCurrent,
    BadValue(&'static str, String),
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
    pub devices: BTreeSet<DeviceId>,
    pub styles: BTreeSet<(String, StyleRef)>,
    /// Locations whose temperature changed: `at[location]` (D1).
    pub locations: BTreeSet<String>,
    /// Location boxes to read again.
    pub boxes: BTreeSet<DeviceId>,
    /// Devices whose location may have changed: their wires' results.
    pub moved: BTreeSet<DeviceId>,
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
                Op::SetProperty { object: ObjectRef::Device(c), key, before, after } => {
                    let is_box = doc.devices.get(c).is_some_and(crate::locations::is_box_component);
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
                Op::CreateDevice { id, device: component } | Op::DeleteDevice { id, device: component }
                    if crate::locations::is_box_component(component) =>
                {
                    self.boxes.insert(*id);
                    if let Some(l) = component.properties.get(ecad_model::book::LOCATION_KEY) {
                        self.locations.insert(l.trim().to_string());
                    }
                    self.box_pages(doc, *id);
                }
                // Lane 072 B: a device's description, part or type text is read by no rating row.
                Op::ModifyDevice { before, after, .. } if before.only_text_differs(after) => {}
                Op::CreateDevice { id, .. } | Op::DeleteDevice { id, .. } | Op::ModifyDevice { id, .. } | Op::SetDeviceDefinition { id, .. } => {
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
                    if let ecad_model::Subject::Device(c) = after.subject {
                        if doc.devices.get(&c).is_some_and(crate::locations::is_box_component) {
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
    protects: BTreeMap<DeviceId, BTreeSet<RatedSubject>>,
    /// net → subjects whose trace crossed it.
    path: BTreeMap<NetId, BTreeSet<RatedSubject>>,
    tally: Tally,
    /// Devices whose rating does not read → how many results name them.
    unread: BTreeMap<DeviceId, (usize, String)>,
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
    boxes: BTreeMap<DeviceId, BoxTemp>,
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
        let mut moved: BTreeSet<DeviceId> = touch.moved.clone();
        for page in &touch.pages {
            moved.extend(doc.representations_on(*page).filter_map(|(_, r)| match r.subject {
                ecad_model::Subject::Device(c) => Some(c),
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
    pub fn bad_boxes(&self) -> impl Iterator<Item = (DeviceId, &str, &str)> {
        self.boxes.iter().filter_map(|(c, b)| match &b.temp {
            Temp::Unread(v) => Some((*c, b.location.as_str(), v.as_str())),
            Temp::Read(_) => None,
        })
    }

    /// Devices whose rating does not read, with their rating text.
    pub fn unread(&self) -> impl Iterator<Item = (DeviceId, &str)> {
        self.unread.iter().map(|(c, (_, t))| (*c, t.as_str()))
    }

    pub fn table_rows(&self) -> &[Issue] {
        &self.table_rows
    }

    /// The subjects a device protects.
    pub fn protected_by(&self, c: DeviceId) -> usize {
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
    fn box_locations(&mut self, doc: &Document, c: DeviceId) {
        if let Some(l) = doc.devices.get(&c).and_then(|x| x.properties.get(ecad_model::book::LOCATION_KEY)) {
            self.locations.insert(l.trim().to_string());
        }
    }

    fn box_pages(&mut self, doc: &Document, c: DeviceId) {
        self.pages.extend(doc.representations_of(ecad_model::Subject::Device(c)).map(|(_, r)| r.page));
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

ecad_model::word_enum! {
    /// A conductor material.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Material {
        Cu,
    }
}
ecad_model::word_enum! {
    /// A conductor class (IEC 60228).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ConductorClass {
        #[strum(serialize = "1")]
        One,
        #[strum(serialize = "2")]
        Two,
        #[strum(serialize = "5")]
        Five,
        #[strum(serialize = "6")]
        Six,
    }
}

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
        let (page, at) = ecad_model::numbering::rep_anchor(doc, ecad_model::Subject::Device(c)).map_or((None, None), |(p, a)| (Some(p), Some(a)));
        out.push(Check {
            kind: CheckKind::RatingDeviceUnread,
            page,
            at,
            object: Object::Device(c),
            which: format!("{tag}'s rating '{text}' has no current; write it as 16 A"),
            fix: None,
        });
    }
    for (c, loc, v) in k.bad_boxes() {
        // Brief 052: the box stores an entry key; the row names the entry's code.
        let loc = if loc.is_empty() { String::new() } else { location_code(doc, loc) };
        let (page, at) = ecad_model::numbering::rep_anchor(doc, ecad_model::Subject::Device(c)).map_or((None, None), |(p, a)| (Some(p), Some(a)));
        out.push(Check {
            kind: CheckKind::RatingBadValue,
            page,
            at,
            object: Object::Device(c),
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
