//! The report models of brief 024 (design 024 §6, §8, §9): the BOM, the wire report and the cable
//! report, the device locations they show, and the location rows of the Checks list.
//!
//! # Semantics
//! - [`Outputs`] keeps one row per device (a component with a tag and no `kind`), per conductor
//!   and per cable, and the BOM groups. It is built whole only on open, reload or first show
//!   ([`Outputs::build`]); after that [`Outputs::update`] recomputes only the rows a transaction's
//!   ops could change. Nothing here is stored or recorded.
//! - Wires never appear on the BOM; cable cores are conductors and appear on the wire report with
//!   their cable (owner answer Q10). Cables are not on the BOM (Q6). A terminal device counts once
//!   (Q11).
//! - A missing value is `None` and shown as `?`; it is flagged and counted, never refused.
//!
//! # Update map (design 024 §9)
//! | Op (kind, target) | Recomputed |
//! | --- | --- |
//! | `CreateComponent` / `DeleteComponent` / `SetProperty` / terminal and representation ops of a device, its items moved | that device; the conductors at its connection points; their cables |
//! | the same of a location box on page P | the devices whose main symbol is on P (before and after); their conductors and cables; P's box checks |
//! | `SetProperty(Page P, location)`, `AddPage` / `RemovePage` of P | the devices whose main symbol is on P; conductors routed on P (their Page cell) |
//! | `SetProperty(Page P, number)` | conductors routed on P |
//! | `SetProperty(Project, location)` | devices whose location comes from the project or that have none (the one project-wide case) |
//! | `CreateConductor` / `DeleteConductor` / `AttachEnd` / `SetProperty` conductor / route ops | that conductor and its net's conductors (before and after); their cables |
//! | `CreateCable` / `DeleteCable` / members / `SetProperty` cable | that cable; its cores |
//! | `SetProjectSettings` changing the potentials table | conductors on the potentials whose row changed (potential index); with a row added or removed, also those with no potential |
//! | `SetProjectSettings` changing the sheet | conductors routed on pages that use the project's sheet (their Page cell and ends' places) |
//! | `SetProjectSettings` changing colour style or reference style | no row: rows keep codes and places, tables format them |
//! | anything else | nothing |
//!
//! This module reads the document; it changes nothing.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::book::{LOCATION_KEY, NUMBER_KEY};
use ecad_model::{CableId, ComponentId, ConductorId, Document, End, ObjectRef, Op, PageId, Pt, RepresentationId, Subject, TerminalId};
use serde_json::{Value, json};

use crate::checks::{Check, Fix, Kind, Object};
use crate::locations::{self, BoxCache, Located, Source};

/// Rows per A3 sheet of a generated table (design 024 §5.2).
pub const ROWS_PER_SHEET: usize = 44;
/// Default and largest `limit` of `bom`, `wirelist`, `cablelist`.
pub const DEFAULT_LIMIT: usize = 500;
pub const MAX_LIMIT: usize = 5000;

fn clean(v: Option<&String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Each page's position in the book and shown number, worked out once per build or update
/// (O(pages)), so a wire's places never read the book order again.
pub struct PageCx {
    pages: std::collections::HashMap<PageId, (usize, String)>,
}

impl PageCx {
    pub fn of(doc: &Document) -> PageCx {
        let b = ecad_model::book::Book::of(doc);
        PageCx { pages: b.order.iter().enumerate().map(|(i, p)| (*p, (i, b.number(doc, *p)))).collect() }
    }
    fn pos(&self, p: PageId) -> usize {
        self.pages.get(&p).map_or(usize::MAX, |x| x.0)
    }
    fn place(&self, doc: &Document, p: PageId, at: Pt) -> Option<ecad_model::derive::Place> {
        let number = self.pages.get(&p)?.1.as_str();
        ecad_model::derive::place_on(&ecad_model::sheet::Grid::of(doc, p), number, at)
    }

    /// The number page `p` is shown with (brief 028: a terminal's Place, with its grid kept by
    /// the caller).
    pub(crate) fn number(&self, p: PageId) -> Option<&str> {
        self.pages.get(&p).map(|x| x.1.as_str())
    }
}

/// Where conductor `w`'s two ends are drawn: end A at its first route's first vertex, end B at
/// its last route's last vertex, routes in book order.
pub fn end_points(doc: &Document, w: ConductorId, cx: &PageCx) -> [Option<(PageId, Pt)>; 2] {
    let mut routes: Vec<(usize, Vec<Pt>, PageId)> = doc
        .representations_of(Subject::Conductor(w))
        .filter_map(|(_, r)| {
            let pts = r.items.iter().filter_map(|i| doc.items.get(i)).find_map(|i| match &i.kind {
                ecad_model::ItemKind::Primitive(ecad_model::Primitive::Polyline { vertices, .. }) => Some(vertices.clone()),
                ecad_model::ItemKind::Primitive(ecad_model::Primitive::Line { a, b }) => Some(vec![*a, *b]),
                _ => None,
            })?;
            Some((cx.pos(r.page), pts, r.page))
        })
        .collect();
    routes.sort_by_key(|(pos, _, _)| *pos);
    [routes.first().and_then(|(_, v, p)| v.first().map(|x| (*p, *x))), routes.last().and_then(|(_, v, p)| v.last().map(|x| (*p, *x)))]
}

/// One device as the reports see it.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceRow {
    pub component: ComponentId,
    pub tag: String,
    pub description: Option<String>,
    pub maker: Option<String>,
    pub part_number: Option<String>,
    pub location: Located,
    /// The page of its main symbol and where that symbol is.
    pub main: Option<(RepresentationId, PageId, Option<Pt>)>,
    /// Its own location differs from the box its main symbol is in: that box and its location.
    pub own_box: Option<(ComponentId, String)>,
}

/// Sort key of a tag: prefix, number, text.
fn tag_key(t: &str) -> (String, u64, String) {
    let (p, n) = ecad_model::tag::parse(t).unwrap_or_else(|| (ecad_model::tag::key(t), 0));
    (p, n, t.to_string())
}

/// The key a device is grouped by on the BOM (part): maker, part number, description, compared
/// case-insensitively; a device with no part number is a group of its own.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PartKey(String, String, String, Option<ComponentId>);

impl DeviceRow {
    fn part_key(&self) -> PartKey {
        let low = |v: &Option<String>| v.as_deref().unwrap_or("").to_lowercase();
        match &self.part_number {
            Some(_) => PartKey(low(&self.maker), low(&self.part_number), low(&self.description), None),
            None => PartKey(String::new(), String::new(), low(&self.description), Some(self.component)),
        }
    }
    fn loc_key(&self) -> LocKey {
        loc_key(self.location.value.as_deref())
    }
}

/// A location's sort key: natural order, no location (`?`) last.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocKey(bool, Vec<(String, u64)>, String);

fn loc_key(v: Option<&str>) -> LocKey {
    match v {
        Some(t) => LocKey(false, locations::natural_key(t), t.to_string()),
        None => LocKey(true, Vec::new(), String::new()),
    }
}

/// One end of a wire as the report shows it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EndCell {
    /// `-K01:A1`, `24V >`; `None` for a loose end or a junction.
    pub name: Option<String>,
    /// A junction end: where it is (shown as `junction 105-C-4` in the reference style).
    pub junction: Option<Option<ecad_model::derive::Place>>,
    /// The device's location; `None` with `device` set means the device has none.
    pub location: Option<String>,
    pub device: Option<ComponentId>,
    /// (page position, column, row) of the end, for the From/To order.
    pub order: (usize, u32, u32),
}

impl EndCell {
    /// The end as shown, in reference style `style`.
    pub fn text(&self, style: ecad_model::ReferenceStyle) -> Option<String> {
        match &self.junction {
            Some(p) => Some(format!("junction {}", p.as_ref().map_or_else(|| "?".to_string(), |x| x.text(style)))),
            None => self.name.clone(),
        }
    }
}

/// One conductor as the wire report shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct WireRow {
    pub conductor: ConductorId,
    pub wire_number: Option<String>,
    pub from: EndCell,
    pub to: EndCell,
    pub cross_section: Option<String>,
    /// The colour as stored (IEC code or free text); the table writes it in the colour style.
    pub colour: Option<String>,
    pub potential: Option<String>,
    pub cable: String,
    /// Where its first route's midpoint is; the table writes it in the reference style.
    pub page: Option<ecad_model::derive::Place>,
    /// Where to go: the midpoint of its first route.
    pub go: Option<(PageId, Pt)>,
}

/// One cable as the cable report shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct CableRow {
    pub cable: CableId,
    pub tag: Option<String>,
    pub kind: Option<String>,
    pub cores: usize,
    pub from_location: Option<String>,
    pub to_location: Option<String>,
    /// Why From/To is `?`, if it is.
    pub location_reason: Option<String>,
    pub maker: Option<String>,
    pub length: Option<String>,
    pub designation: Option<String>,
    pub go: Option<(PageId, Pt)>,
    /// Brief 028: how its members meet its core offer (design 028 §6.2).
    pub count: crate::strips::CoreCount,
    /// The members (in member order), for the over and twice go-tos.
    pub members: Vec<ConductorId>,
    /// Their stored core labels, in member order.
    pub member_cores: Vec<Option<String>>,
    /// The devices and strips at its cores' ends in its From and To locations (design 028 §6.3).
    pub from_devices: Vec<String>,
    pub to_devices: Vec<String>,
}

impl CableRow {
    /// The Cores cell: the offered count, else the member count.
    pub fn core_total(&self) -> usize {
        self.count.offer.as_ref().map_or(self.cores, Vec::len)
    }
}

/// BOM grouping (design 024 §6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Group {
    #[default]
    Location,
    Part,
}

impl Group {
    pub fn parse(t: &str) -> Option<Group> {
        match t.trim() {
            "location" => Some(Group::Location),
            "part" => Some(Group::Part),
            _ => None,
        }
    }
}

/// One BOM row.
#[derive(Debug, Clone, PartialEq)]
pub struct BomRow {
    /// By location: the location (`None`: `?`). By part: `+CP1 ×2, +FIELD ×1`.
    pub location: Option<String>,
    pub qty: usize,
    pub tags: Vec<String>,
    pub devices: Vec<ComponentId>,
    pub description: Option<String>,
    pub maker: Option<String>,
    pub part_number: Option<String>,
    /// Brief 030: the accessory a strip's row counts (`end_stop`, `jumper`…); `None` for devices
    /// and terminals.
    pub accessory: Option<&'static str>,
}

/// The rows of a report, as text cells for tables, CSV and generated pages.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    pub headers: Vec<&'static str>,
    pub rows: Vec<Vec<Cell>>,
}

/// One cell: its text and, when flagged (`?`), why.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cell {
    pub text: String,
    pub flag: Option<String>,
}

impl Cell {
    fn ok(t: impl Into<String>) -> Cell {
        Cell { text: t.into(), flag: None }
    }
    fn opt(v: &Option<String>, why: impl FnOnce() -> String) -> Cell {
        match v {
            Some(t) => Cell::ok(t.clone()),
            None => Cell { text: "?".into(), flag: Some(why()) },
        }
    }
}

impl Table {
    /// How many flagged cells.
    pub fn flagged(&self) -> usize {
        self.rows.iter().flatten().filter(|c| c.flag.is_some()).count()
    }
}

/// What a transaction could change in the models (see the module's update map).
#[derive(Debug, Clone, Default)]
pub struct Scope {
    devices: BTreeSet<ComponentId>,
    pages: BTreeSet<PageId>,
    route_pages: BTreeSet<PageId>,
    conductors: BTreeSet<ConductorId>,
    cables: BTreeSet<CableId>,
    project_location: bool,
    /// Potentials (lower case; `""`: none) whose table row changed.
    potentials: BTreeSet<String>,
    /// The project's sheet changed: pages without their own sheet.
    sheet: bool,
    /// Colour or reference style changed (no row work).
    styles: bool,
    /// Brief 030: project `strip_*` keys that changed (the strips inheriting them).
    strip_keys: BTreeSet<String>,
}

impl Scope {
    /// Whether nothing is in scope.
    pub fn is_empty(&self) -> bool {
        !self.project_location
            && self.devices.is_empty()
            && self.pages.is_empty()
            && self.route_pages.is_empty()
            && self.conductors.is_empty()
            && self.cables.is_empty()
            && self.potentials.is_empty()
            && !self.sheet
            && !self.styles
            && self.strip_keys.is_empty()
    }

    fn subject(&mut self, doc: &Document, s: Subject, page: PageId) {
        match s {
            Subject::Component(c) if locations::is_box(doc, c) => {
                self.pages.insert(page);
            }
            Subject::Component(c) => {
                self.devices.insert(c);
            }
            Subject::Conductor(w) => {
                self.conductors.insert(w);
            }
            Subject::Cable(k) => {
                self.cables.insert(k);
            }
        }
    }

    /// Collect what `ops` name on `doc` (call once on the state before, once after).
    pub fn collect(&mut self, doc: &Document, ops: &[Op]) {
        for op in ops {
            match op {
                Op::CreateComponent { id, component } | Op::DeleteComponent { id, component } => {
                    if locations::is_box_component(component) {
                        self.pages.extend(doc.representations_of(Subject::Component(*id)).map(|(_, r)| r.page));
                    } else {
                        self.devices.insert(*id);
                    }
                }
                Op::SetProperty { object: ObjectRef::Component(c), .. } => {
                    if locations::is_box(doc, *c) {
                        self.pages.extend(doc.representations_of(Subject::Component(*c)).map(|(_, r)| r.page));
                    } else {
                        self.devices.insert(*c);
                    }
                }
                Op::SetProperty { object: ObjectRef::Conductor(w), .. } => {
                    self.conductors.insert(*w);
                }
                Op::SetProperty { object: ObjectRef::Cable(k), .. } => {
                    self.cables.insert(*k);
                }
                Op::SetProperty { object: ObjectRef::Page(p), key, .. } => {
                    if key == LOCATION_KEY {
                        self.pages.insert(*p);
                    }
                    if key == NUMBER_KEY {
                        self.route_pages.insert(*p);
                    }
                }
                Op::SetProperty { object: ObjectRef::Project, key, .. } => {
                    if key == LOCATION_KEY {
                        self.project_location = true;
                    }
                    // Brief 036: the registry's `terminal.*` defaults reach strips the same way.
                    if key.starts_with("strip_") || key.starts_with("terminal.") {
                        self.strip_keys.insert(key.clone());
                    }
                }
                Op::AddTerminal { id, terminal } | Op::RemoveTerminal { id, terminal } => {
                    self.devices.insert(terminal.component);
                    // Brief 028: a relabelled terminal (Remove + Add of one ID) renames its wires' ends.
                    self.conductors.extend(doc.ends_at(*id).into_iter().map(|(w, _)| w));
                }
                Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => self.subject(doc, rep.subject, rep.page),
                Op::ModifyRepresentation { before, after, .. } => {
                    self.subject(doc, before.subject, before.page);
                    self.subject(doc, after.subject, after.page);
                }
                Op::AddItem { id, .. } | Op::ModifyItem { id, .. } | Op::RemoveItem { id, .. } => {
                    if let Some((_, r)) = doc.representation_of_item(*id) {
                        self.subject(doc, r.subject, r.page);
                    }
                }
                Op::CreateConductor { id, .. } | Op::DeleteConductor { id, .. } | Op::AttachEnd { conductor: id, .. } => {
                    self.conductors.insert(*id);
                }
                Op::CreateCable { id, .. } | Op::DeleteCable { id, .. } => {
                    self.cables.insert(*id);
                }
                Op::AddCableMember { cable, member, .. } | Op::RemoveCableMember { cable, member, .. } => {
                    self.cables.insert(*cable);
                    self.conductors.insert(member.conductor);
                }
                Op::SetProjectSettings { before, after } => {
                    let rows = |w: &ecad_model::wiring::WiringSettings| -> BTreeMap<String, ecad_model::wiring::PotentialDef> {
                        w.potentials.iter().map(|p| (p.name.trim().to_lowercase(), p.clone())).collect()
                    };
                    let (a, b) = (rows(&before.wiring), rows(&after.wiring));
                    for k in a.keys().chain(b.keys()) {
                        if a.get(k) != b.get(k) {
                            self.potentials.insert(k.clone());
                            if !(a.contains_key(k) && b.contains_key(k)) {
                                self.potentials.insert(String::new());
                            }
                        }
                    }
                    self.sheet |= before.sheet != after.sheet;
                    self.styles |= before.reference_style != after.reference_style || before.wiring.colour_style != after.wiring.colour_style;
                }
                Op::AddPage { id, .. } | Op::RemovePage { id, .. } => {
                    self.pages.insert(*id);
                    self.route_pages.insert(*id);
                }
                Op::EmbedDefinition { .. }
                | Op::RemoveDefinition { .. }
                | Op::AddLayer { .. }
                | Op::RemoveLayer { .. }
                | Op::NameVersion { .. }
                | Op::UnnameVersion { .. }
                | Op::AddFolder { .. }
                | Op::RemoveFolder { .. }
                | Op::ModifyFolder { .. } => {}
            }
        }
        // Net mates: a wire's number and potential are its net's (brief 022).
        let conn = doc.connectivity();
        let mut mates = BTreeSet::new();
        for w in &self.conductors {
            if let Some(net) = conn.net_of_conductor(*w).and_then(|id| conn.net(id)) {
                mates.extend(net.conductors.iter().copied());
            }
        }
        self.conductors.extend(mates);
        // Devices whose main symbol sits on a page in scope.
        for p in &self.pages {
            if doc.pages.contains_key(p) {
                for (_, r) in doc.representations_on(*p) {
                    if let Subject::Component(c) = r.subject
                        && locations::is_device(doc, c)
                    {
                        self.devices.insert(c);
                    }
                }
            }
        }
    }
}

/// The kept report models (see the module docs).
#[derive(Debug, Clone, Default)]
pub struct Outputs {
    devices: BTreeMap<ComponentId, DeviceRow>,
    by_page: BTreeMap<PageId, BTreeSet<ComponentId>>,
    from_project: BTreeSet<ComponentId>,
    /// Devices with no location at all: a project location set reaches them (tester 024, bug 1).
    from_none: BTreeSet<ComponentId>,
    by_location: BTreeMap<(LocKey, PartKey), BTreeSet<((String, u64, String), ComponentId)>>,
    by_part: BTreeMap<PartKey, BTreeSet<((String, u64, String), ComponentId)>>,
    wires: BTreeMap<ConductorId, WireRow>,
    /// Conductors by their net's potential (lower case; `""`: none), for wiring-table changes.
    by_potential: BTreeMap<String, BTreeSet<ConductorId>>,
    cables: BTreeMap<CableId, CableRow>,
    /// Rows of the BOM-by-location table per location (a heading each), for its sheet count.
    loc_rows: BTreeMap<LocKey, usize>,
    /// The styles the tables write codes and places in.
    style: ecad_model::ReferenceStyle,
    colour_style: ecad_model::wiring::ColourStyle,
    /// Location box checks by page.
    box_checks: BTreeMap<PageId, Vec<Check>>,
    no_part_number: usize,
    no_location: usize,
    /// Brief 028: the terminal strips (design 028 §11.1).
    pub strips: crate::strips::Strips,
    /// Brief 028: whether the last update changed a cable row (what a shown cable schedule page
    /// depends on; tester 028, defect 2).
    pub last_cables_changed: bool,
    /// The pages drawn from these models (BOM, terminal plan, cable schedule), kept from the page
    /// ops (review of PR #55, finding 1: no page scan per commit).
    generated: BTreeSet<PageId>,
    /// Which build these models are (unique in the run): a copy keyed on kept revisions also
    /// keys on this, so a rebuild (reload, branch switch) is never mistaken for the old models.
    pub build_id: u64,
    /// How many sorted row lists were made (tests: an unrelated edit makes none).
    pub sorts: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// Devices whose own location differs from their box's (one warning row each), and the
    /// location box rows by severity (warnings, info): kept with the rows (brief 027's badge).
    own_box: usize,
    box_counts: [usize; 2],
    /// Bumped only when the check rows may read differently (brief 027's Release dialog).
    pub checks_changed: u64,
    /// Brief 028's cable Checks rows, kept: `too-many-cores` warnings, `core-twice` warnings, and
    /// cables with no core count (one info row when any).
    cable_checks: [usize; 3],
    /// Brief 030 (review of PR #57, 4): devices that are not strips but carry `link.<n>` keys.
    links_off_strip: BTreeSet<ComponentId>,
    link_keys: BTreeMap<ComponentId, Vec<String>>,
    /// Bumped on every change, for views that copy the rows.
    pub revision: u64,
    /// How many rows the last update recomputed (tests and bench: never the whole project).
    pub last_work: usize,
}

impl Outputs {
    /// Every model, from scratch (open, reload, first show).
    pub fn build(doc: &Document) -> Outputs {
        let mut o = Outputs { style: doc.settings.reference_style, colour_style: doc.settings.wiring.colour_style, ..Outputs::default() };
        let mut cache = BoxCache::default();
        for (_, c) in doc.tagged() {
            o.set_device(doc, c, &mut cache);
        }
        let cx = PageCx::of(doc);
        for w in doc.conductors.keys() {
            o.set_wire(doc, *w, &cx);
        }
        for k in doc.cables.keys() {
            o.set_cable(doc, *k);
        }
        for p in &doc.page_order {
            o.set_box_checks(doc, *p);
        }
        o.strips = crate::strips::Strips::build(doc, &o, &cx);
        o.generated = doc.page_order.iter().copied().filter(|p| needs_outputs(doc, *p)).collect();
        static BUILDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        o.build_id = BUILDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        o
    }

    /// The pages drawn from these models.
    pub fn generated_pages(&self) -> &BTreeSet<PageId> {
        &self.generated
    }

    /// How many BOM rows `group` lists, from kept counts (no rows made).
    pub fn bom_count(&self, group: Group) -> usize {
        let strips: usize = self.strips.bom_rows.values().sum();
        match group {
            Group::Location => self.by_location.len() + strips,
            Group::Part => self.by_part.len() + strips,
        }
    }

    /// How many wire rows.
    pub fn wire_count(&self) -> usize {
        self.wires.len()
    }

    /// One cable's row.
    pub fn cable_row(&self, k: CableId) -> Option<&CableRow> {
        self.cables.get(&k)
    }

    /// How many cables (no sort).
    pub fn cable_count(&self) -> usize {
        self.cables.len()
    }

    /// The strips sorted by tag (counted in [`Outputs::sorts`]).
    pub fn sorted_strips(&self) -> Vec<&crate::strips::StripData> {
        self.sorts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.strips.sorted()
    }

    /// What `ops`, about to be committed on `before`, could change. Pair with [`Outputs::update`].
    pub fn scope(&self, before: &Document, ops: &[Op]) -> Scope {
        let mut s = Scope::default();
        s.collect(before, ops);
        // The devices whose main symbol was on a page in scope.
        for p in &s.pages {
            s.devices.extend(self.by_page.get(p).into_iter().flatten().copied());
        }
        s
    }

    /// Recompute the rows in `scope` on `doc` (the state after), adding what `ops` (the whole
    /// committed transaction) name in this state.
    pub fn update(&mut self, doc: &Document, ops: &[Op], mut scope: Scope) {
        // What this update changes, for the generated pages that depend on it (brief 028).
        self.last_cables_changed = false;
        self.strips.last_changed.clear();
        for op in ops {
            let p = match op {
                Op::AddPage { id, .. } | Op::RemovePage { id, .. } => *id,
                Op::SetProperty { object: ObjectRef::Page(p), key, .. } if key == crate::generated::KEY => *p,
                _ => continue,
            };
            if needs_outputs(doc, p) {
                self.generated.insert(p);
            } else {
                self.generated.remove(&p);
            }
        }
        scope.collect(doc, ops);
        if scope.is_empty() {
            return;
        }
        let mut work = 0;
        self.style = doc.settings.reference_style;
        self.colour_style = doc.settings.wiring.colour_style;
        for k in &scope.potentials {
            scope.conductors.extend(self.by_potential.get(k).into_iter().flatten().copied());
        }
        if scope.sheet {
            let own = |p: &PageId| doc.pages.get(p).is_some_and(|x| x.properties.contains_key(ecad_model::sheet::SHEET_KEY));
            scope.route_pages.extend(doc.page_order.iter().filter(|p| !own(p)).copied());
        }
        if scope.project_location {
            scope.devices.extend(self.from_project.iter().copied());
            scope.devices.extend(self.from_none.iter().copied());
        }
        let mut cache = BoxCache::default();
        for c in &scope.devices {
            let before = self.devices.get(c).map(|d| (d.tag.clone(), d.location.value.clone()));
            self.set_device(doc, *c, &mut cache);
            work += 1;
            let after = self.devices.get(c).map(|d| (d.tag.clone(), d.location.value.clone()));
            if before != after {
                for (t, _) in doc.terminals_of(*c) {
                    scope.conductors.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
                }
            }
        }
        for p in &scope.route_pages {
            if doc.pages.contains_key(p) {
                for (_, r) in doc.representations_on(*p) {
                    if let Subject::Conductor(w) = r.subject {
                        scope.conductors.insert(w);
                    }
                }
            }
        }
        // Page positions and numbers only when a wire is in scope (review of PR #51, finding 3).
        let mut cx = if scope.conductors.is_empty() { None } else { Some(PageCx::of(doc)) };
        for (w, cx) in scope.conductors.iter().zip(std::iter::repeat_with(|| cx.as_ref()).flatten()) {
            self.set_wire(doc, *w, cx);
            work += 1;
            scope.cables.extend(doc.cables_of(*w));
        }
        for k in &scope.cables {
            self.set_cable(doc, *k);
            work += 1;
        }
        for p in &scope.pages {
            self.set_box_checks(doc, *p);
        }
        // Brief 028: the strips these rows touch (design 028 §11.1).
        let mut pages = scope.route_pages.clone();
        pages.extend(ecad_model::book::positions_changed(doc, ops));
        let mut strips = std::mem::take(&mut self.strips);
        let which = strips.touched(
            doc,
            &crate::strips::StripScope {
                devices: &scope.devices,
                conductors: &scope.conductors,
                cables: &scope.cables,
                pages: &pages,
                project_keys: &scope.strip_keys,
            },
        );
        let acc_only = strips.touched_acc(&scope.strip_keys, &which);
        if which.is_empty() {
            strips.last_work = 0;
            strips.last_changed.clear();
        } else {
            // The page positions worked out for the wires, if they were (O(pages) once). Counted
            // in the strips' own `last_work`, not in the rows'.
            let cx = cx.get_or_insert_with(|| PageCx::of(doc));
            strips.recompute(doc, &which, self, cx);
        }
        // Brief 030: a project default reaches only its inheriting strips' accessories.
        if !acc_only.is_empty() {
            strips.recompute_acc(doc, &acc_only);
        }
        // Brief 028's strip Checks rows read from the strips just recomputed (brief 027's key).
        if !strips.last_changed.is_empty() {
            self.checks_changed += 1;
        }
        self.strips = strips;
        self.last_work = work;
        self.revision += 1;
    }

    /// Verify mode (brief 028, design 028 §11.4): the kept strips against a rebuild on `doc`; a
    /// difference is put right and described (a bug, for the event log).
    pub fn verify_strips(&mut self, doc: &Document) -> Option<String> {
        let built = crate::strips::Strips::build(doc, self, &PageCx::of(doc));
        if self.strips.same_as(&built) {
            return None;
        }
        let work = self.strips.last_work;
        self.strips = built;
        self.strips.last_work = work;
        Some("the terminal strip model was out of step; rebuilt".into())
    }

    fn remove_device(&mut self, c: ComponentId) {
        let Some(old) = self.devices.remove(&c) else { return };
        if let Some((_, p, _)) = old.main
            && let Some(set) = self.by_page.get_mut(&p)
        {
            set.remove(&c);
            if set.is_empty() {
                self.by_page.remove(&p);
            }
        }
        self.from_project.remove(&c);
        self.from_none.remove(&c);
        let tk = (tag_key(&old.tag), c);
        let lk = (old.loc_key(), old.part_key());
        if let Some(set) = self.by_location.get_mut(&lk) {
            set.remove(&tk);
            if set.is_empty() {
                self.by_location.remove(&lk);
                if let Some(n) = self.loc_rows.get_mut(&lk.0) {
                    *n -= 1;
                    if *n == 0 {
                        self.loc_rows.remove(&lk.0);
                    }
                }
            }
        }
        let pk = old.part_key();
        if let Some(set) = self.by_part.get_mut(&pk) {
            set.remove(&tk);
            if set.is_empty() {
                self.by_part.remove(&pk);
            }
        }
        self.no_part_number -= usize::from(old.part_number.is_none());
        self.no_location -= usize::from(old.location.value.is_none());
        self.own_box -= usize::from(old.own_box.is_some());
        if old.own_box.is_some() {
            self.checks_changed += 1;
        }
    }

    fn set_device(&mut self, doc: &Document, c: ComponentId, cache: &mut BoxCache) {
        self.remove_device(c);
        // Review of PR #57, 4: a link key on a device that is not a strip.
        let misplaced = !crate::strips::is_strip(doc, c)
            && doc.components.get(&c).is_some_and(|x| x.properties.keys().any(|k| ecad_model::links::link_number(k).is_some()));
        if misplaced {
            let keys = doc
                .components
                .get(&c)
                .map(|x| x.properties.keys().filter(|k| ecad_model::links::link_number(k).is_some()).cloned().collect())
                .unwrap_or_default();
            self.link_keys.insert(c, keys);
            self.links_off_strip.insert(c);
        } else if self.links_off_strip.remove(&c) {
            self.link_keys.remove(&c);
            self.checks_changed += 1;
        }
        if misplaced {
            self.checks_changed += 1;
        }
        if !locations::is_device(doc, c) {
            return;
        }
        let Some(comp) = doc.components.get(&c) else { return };
        let props = &comp.properties;
        let location = locations::resolve_with(doc, c, cache);
        let main = locations::main_rep(doc, c).map(|(rid, r)| (rid, r.page, locations::anchor(doc, r)));
        let own_box = locations::own_differs(doc, c, cache).map(|b| (b.component, b.location.unwrap_or_default()));
        let row = DeviceRow {
            component: c,
            tag: props.get(ecad_model::tag::TAG_KEY).cloned().unwrap_or_default(),
            description: clean(props.get("description")),
            maker: clean(props.get("maker")),
            part_number: clean(props.get("part_number")),
            location,
            main,
            own_box,
        };
        if let Some((_, p, _)) = row.main {
            self.by_page.entry(p).or_default().insert(c);
        }
        match row.location.source {
            Source::Project => {
                self.from_project.insert(c);
            }
            Source::None => {
                self.from_none.insert(c);
            }
            _ => {}
        }
        let tk = (tag_key(&row.tag), c);
        // Brief 028 (Q13): a strip has one BOM row per terminal type, from the strip model.
        if !crate::strips::is_strip(doc, c) {
            let group = self.by_location.entry((row.loc_key(), row.part_key())).or_default();
            if group.is_empty() {
                *self.loc_rows.entry(row.loc_key()).or_default() += 1;
            }
            group.insert(tk.clone());
            self.by_part.entry(row.part_key()).or_default().insert(tk);
        }
        self.no_part_number += usize::from(row.part_number.is_none());
        self.no_location += usize::from(row.location.value.is_none());
        self.own_box += usize::from(row.own_box.is_some());
        if row.own_box.is_some() {
            self.checks_changed += 1;
        }
        self.devices.insert(c, row);
    }

    fn end_cell(&self, doc: &Document, t: Option<TerminalId>, at: Option<(PageId, Pt)>, cx: &PageCx) -> EndCell {
        let order = at.map_or((usize::MAX, 0, 0), |(p, a)| {
            let pos = cx.pos(p);
            let place = cx.place(doc, p, a);
            (pos, place.as_ref().map_or(0, |x| x.column), place.as_ref().map_or(0, |x| x.row))
        });
        let Some(t) = t else { return EndCell { order, ..EndCell::default() } };
        let Some(term) = doc.terminals.get(&t) else { return EndCell { order, ..EndCell::default() } };
        let c = term.component;
        match crate::wires::kind_of(doc, c) {
            Some(ecad_model::connectivity::JUNCTION) => {
                EndCell { junction: Some(at.and_then(|(p, a)| cx.place(doc, p, a))), order, ..EndCell::default() }
            }
            Some(ecad_model::connectivity::CONTINUATION) => {
                let name = doc.components.get(&c).and_then(|x| x.properties.get(ecad_model::connectivity::NAME)).cloned().unwrap_or_default();
                EndCell { name: Some(format!("{name} >")), order, ..EndCell::default() }
            }
            _ => {
                let label = crate::interact::terminal_name(doc, t);
                let device = self.devices.get(&c);
                let tag = device.map(|d| d.tag.clone()).or_else(|| crate::devices::tag_of(doc, c).map(str::to_string)).unwrap_or_else(|| "?".into());
                let location = match device {
                    Some(d) => d.location.value.clone(),
                    None => locations::resolve(doc, c).value,
                };
                EndCell { name: Some(format!("{tag}:{label}")), location, device: Some(c), order, junction: None }
            }
        }
    }

    /// The cell of conductor end `t` drawn at `at` (brief 028: a terminal's far end).
    pub(crate) fn end_cell_of(&self, doc: &Document, t: Option<TerminalId>, at: Option<(PageId, Pt)>, cx: &PageCx) -> EndCell {
        self.end_cell(doc, t, at, cx)
    }

    fn set_wire(&mut self, doc: &Document, w: ConductorId, cx: &PageCx) {
        if let Some(old) = self.wires.remove(&w) {
            let k = old.potential.as_deref().unwrap_or("").trim().to_lowercase();
            if let Some(set) = self.by_potential.get_mut(&k) {
                set.remove(&w);
                if set.is_empty() {
                    self.by_potential.remove(&k);
                }
            }
        }
        let Some(cond) = doc.conductors.get(&w) else { return };
        // Routes in book order.
        let mut routes: Vec<(usize, Vec<Pt>, PageId)> = doc
            .representations_of(Subject::Conductor(w))
            .filter_map(|(_, r)| {
                let pts = r.items.iter().filter_map(|i| doc.items.get(i)).find_map(|i| match &i.kind {
                    ecad_model::ItemKind::Primitive(ecad_model::Primitive::Polyline { vertices, .. }) => Some(vertices.clone()),
                    ecad_model::ItemKind::Primitive(ecad_model::Primitive::Line { a, b }) => Some(vec![*a, *b]),
                    _ => None,
                })?;
                let pos = cx.pos(r.page);
                Some((pos, pts, r.page))
            })
            .collect();
        routes.sort_by_key(|(pos, _, _)| *pos);
        let a_at = routes.first().and_then(|(_, v, p)| v.first().map(|x| (*p, *x)));
        let b_at = routes.last().and_then(|(_, v, p)| v.last().map(|x| (*p, *x)));
        let mid = routes.first().and_then(|(_, v, p)| {
            let (Some(a), Some(b)) = (v.first(), v.get(1).or(v.first())) else { return None };
            Some((*p, Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)))
        });
        let mut ends = [self.end_cell(doc, cond.end(End::A), a_at, cx), self.end_cell(doc, cond.end(End::B), b_at, cx)];
        if ends[1].order < ends[0].order {
            ends.swap(0, 1);
        }
        let [from, to] = ends;
        let conn = doc.connectivity();
        let net = conn.net_of_conductor(w).and_then(|id| conn.net(id));
        let pot = net.map(|n| ecad_model::wiring::net_potential(doc, n));
        let stored = |k: &str| clean(cond.properties.get(k));
        let cross_section = stored(ecad_model::wiring::CROSS_SECTION).or_else(|| pot.as_ref().and_then(|p| p.cross_section.clone()));
        let colour = stored(ecad_model::wiring::COLOUR).or_else(|| pot.as_ref().and_then(|p| p.colour.clone()));
        let cable = doc
            .cables_of(w)
            .into_iter()
            .filter_map(|k| {
                let cab = doc.cables.get(&k)?;
                let tag = cab.properties.get("tag").cloned().unwrap_or_else(|| "?".into());
                let i = cab.members.iter().position(|m| m.conductor == w)?;
                let core = cab.members.get(i).and_then(|m| m.core.clone()).unwrap_or_else(|| (i + 1).to_string());
                Some(format!("{tag} core {core}"))
            })
            .collect::<Vec<_>>()
            .join(", ");
        // Brief 028 (design 028 §9.2): a wire between two terminals of one strip is a bridge.
        let cable = match crate::strips::bridge_of(doc, w) {
            Some(strip) if cable.is_empty() => format!("Bridge {}", crate::devices::tag_of(doc, strip).unwrap_or("?")),
            _ => cable,
        };
        let page = mid.and_then(|(p, a)| cx.place(doc, p, a));
        let potential = pot.and_then(|p| p.name);
        self.by_potential.entry(potential.as_deref().unwrap_or("").trim().to_lowercase()).or_default().insert(w);
        self.wires.insert(
            w,
            WireRow {
                conductor: w,
                wire_number: net.and_then(|n| ecad_model::wiring::net_number(doc, n)),
                from,
                to,
                cross_section,
                colour,
                potential,
                cable,
                page,
                go: mid,
            },
        );
    }

    fn set_cable(&mut self, doc: &Document, k: CableId) {
        let before = self.cables.get(&k).cloned();
        self.set_cable_row(doc, k);
        if self.cables.get(&k) != before.as_ref() {
            self.last_cables_changed = true;
            let rows = |r: Option<&CableRow>| -> [usize; 3] {
                r.map_or([0, 0, 0], |r| {
                    let known = r.count.offer.is_some();
                    [usize::from(known && !r.count.over.is_empty()), r.count.twice.len(), usize::from(!known)]
                })
            };
            let (old, new) = (rows(before.as_ref()), rows(self.cables.get(&k)));
            for ((n, o), a) in self.cable_checks.iter_mut().zip(old).zip(new) {
                *n = n.saturating_sub(o) + a;
            }
            if old != new {
                self.checks_changed += 1;
            }
        }
    }

    fn set_cable_row(&mut self, doc: &Document, k: CableId) {
        let Some(cab) = doc.cables.get(&k) else {
            self.cables.remove(&k);
            return;
        };
        let p = |key: &str| clean(cab.properties.get(key));
        let mut locs: BTreeSet<(LocKey, String)> = BTreeSet::new();
        let mut missing = false;
        // Brief 028: the devices at the cores' ends, per location.
        let mut at_loc: BTreeMap<String, BTreeSet<((String, u64, String), ComponentId)>> = BTreeMap::new();
        for m in &cab.members {
            let Some(cond) = doc.conductors.get(&m.conductor) else { continue };
            for t in cond.ends.iter().flatten() {
                let Some(c) = doc.terminals.get(t).map(|x| x.component) else { continue };
                if crate::wires::kind_of(doc, c).is_some() {
                    continue;
                }
                let (loc, tag) = match self.devices.get(&c) {
                    Some(d) => (d.location.value.clone(), d.tag.clone()),
                    None => (locations::resolve(doc, c).value, crate::devices::tag_of(doc, c).unwrap_or("?").to_string()),
                };
                match loc {
                    Some(l) => {
                        at_loc.entry(l.clone()).or_default().insert((tag_key(&tag), c));
                        locs.insert((loc_key(Some(&l)), l));
                    }
                    None => missing = true,
                }
            }
        }
        let tags_at = |l: &Option<String>| -> Vec<String> {
            let mut seen = BTreeSet::new();
            l.iter()
                .flat_map(|l| l.split(", "))
                .flat_map(|l| at_loc.get(l).into_iter().flatten())
                .filter(|(_, c)| seen.insert(*c))
                .map(|((_, _, t), _)| t.clone())
                .collect()
        };
        let list: Vec<String> = locs.into_iter().map(|(_, l)| l).collect();
        let (from, to, reason) = match (missing, list.as_slice()) {
            (true, _) => (None, None, Some("A device at one of its cores' ends has no location.".to_string())),
            (false, []) => (None, None, Some("No core ends at a device.".to_string())),
            (false, [one]) => (Some(one.clone()), Some(one.clone()), None),
            (false, [a, rest @ ..]) => (Some(a.clone()), Some(rest.join(", ")), None),
        };
        let (from_devices, to_devices) = (tags_at(&from), tags_at(&to));
        let go = doc.representations_of(Subject::Cable(k)).find_map(|(_, r)| {
            let at = r.items.first().and_then(|i| doc.items.get(i)).and_then(|i| match &i.kind {
                ecad_model::ItemKind::Primitive(ecad_model::Primitive::Line { a, .. }) => Some(*a),
                ecad_model::ItemKind::Primitive(ecad_model::Primitive::Polyline { vertices, .. }) => vertices.first().copied(),
                _ => None,
            })?;
            Some((r.page, at))
        });
        self.cables.insert(
            k,
            CableRow {
                cable: k,
                tag: p("tag"),
                kind: p("type"),
                cores: cab.members.len(),
                from_location: from,
                to_location: to,
                location_reason: reason,
                maker: p("maker"),
                length: p("length"),
                designation: p("designation"),
                go,
                count: crate::strips::count_cores(crate::strips::cable_offer(&cab.properties), &cab.members),
                members: cab.members.iter().map(|m| m.conductor).collect(),
                member_cores: cab.members.iter().map(|m| m.core.clone()).collect(),
                from_devices,
                to_devices,
            },
        );
    }

    fn set_box_checks(&mut self, doc: &Document, page: PageId) {
        if !doc.pages.contains_key(&page) {
            self.box_checks.remove(&page);
            return;
        }
        let boxes = locations::boxes_on(doc, page);
        let mut out = Vec::new();
        for b in &boxes {
            if locations::crosses_itself(&b.vertices) {
                out.push(Check {
                    kind: Kind::BoxCrosses,
                    page: Some(page),
                    at: b.vertices.first().copied(),
                    object: Object::Component(b.component),
                    which: b.location.clone().unwrap_or_else(|| "+?".into()),
                    fix: None,
                });
            }
        }
        for (i, a) in boxes.iter().enumerate() {
            for b in boxes.iter().skip(i + 1) {
                if let Some(at) = crossing(&a.vertices, &b.vertices) {
                    out.push(Check {
                        kind: Kind::BoxesOverlap,
                        page: Some(page),
                        at: Some(at),
                        object: Object::Component(a.component),
                        which: format!("{} and {}", a.location.as_deref().unwrap_or("+?"), b.location.as_deref().unwrap_or("+?")),
                        fix: None,
                    });
                }
            }
        }
        let old = self.box_checks.get(&page);
        if old.map_or(out.is_empty(), |o| *o == out) {
            return;
        }
        let tally = |v: &[Check]| {
            let w = v.iter().filter(|c| c.severity() == crate::checks::Severity::Warning).count();
            [w, v.len() - w]
        };
        let before = old.map_or([0, 0], |o| tally(o));
        let now = tally(&out);
        for i in 0..2 {
            if let (Some(n), Some(b), Some(a)) = (self.box_counts.get_mut(i), before.get(i), now.get(i)) {
                *n = n.saturating_sub(*b) + a;
            }
        }
        self.checks_changed += 1;
        if out.is_empty() {
            self.box_checks.remove(&page);
        } else {
            self.box_checks.insert(page, out);
        }
    }

    /// The location rows of the Checks list (design 024 §8.3).
    pub fn checks(&self) -> Vec<Check> {
        let mut out: Vec<Check> = self.box_checks.values().flatten().cloned().collect();
        for d in self.devices.values() {
            if let Some((_, loc)) = &d.own_box {
                out.push(Check {
                    kind: Kind::OwnLocation,
                    page: d.main.map(|(_, p, _)| p),
                    at: d.main.and_then(|(_, _, a)| a),
                    object: Object::Component(d.component),
                    which: format!("{} is {} but sits in box {loc}", d.tag, d.location.text()),
                    fix: Some(Fix::UseBoxLocation(d.component)),
                });
            }
        }
        let n = self.no_part_number;
        if n > 0 {
            let which = if n == 1 { "1 device has no part number".to_string() } else { format!("{n} devices have no part number") };
            out.push(Check { kind: Kind::NoPartNumber, page: None, at: None, object: Object::None, which, fix: Some(Fix::ShowReports) });
        }
        let n = self.no_location;
        if n > 0 && n < self.devices.len() {
            let which = if n == 1 { "1 device has no location".to_string() } else { format!("{n} devices have no location") };
            out.push(Check { kind: Kind::NoLocation, page: None, at: None, object: Object::None, which, fix: Some(Fix::ShowReports) });
        }
        out.extend(self.strip_checks());
        out
    }

    /// The Checks rows of brief 028 (design 028 §8), from the kept strip and cable rows.
    pub fn strip_checks(&self) -> Vec<Check> {
        let mut out = Vec::new();
        for s in self.strips.strips.values() {
            if let Some(off) = s.offer
                && s.drawn > off.positions as usize
            {
                let hi = s.highest();
                out.push(Check {
                    kind: Kind::TooManyTerminals,
                    page: hi.and_then(|r| r.go).map(|(p, _)| p),
                    at: hi.and_then(|r| r.go).map(|(_, a)| a),
                    object: hi.and_then(|r| r.terminal).map_or(Object::Component(s.component), Object::Terminal),
                    which: format!("{}: {} terminals drawn, strip offers {}", s.tag, s.drawn, off.positions),
                    fix: Some(Fix::OfferDrawn(s.component)),
                });
            }
            for (label, t) in &s.twice {
                let go = s.row_of(*t).and_then(|r| r.go);
                out.push(Check {
                    kind: Kind::TerminalTwice,
                    page: go.map(|(p, _)| p),
                    at: go.map(|(_, a)| a),
                    object: Object::Terminal(*t),
                    which: format!("{}:{label} is drawn twice", s.tag),
                    fix: Some(Fix::RenumberStrip(s.component)),
                });
            }
            out.extend(accessory_checks(s));
        }
        for c in &self.links_off_strip {
            let tag = self.devices.get(c).map_or_else(|| "?".to_string(), |d| d.tag.clone());
            out.push(Check {
                kind: Kind::LinkOnNonStrip,
                page: None,
                at: None,
                object: Object::Component(*c),
                which: format!("{tag} has link keys but is not a terminal strip"),
                fix: Some(Fix::LinkOnNonStrip(*c)),
            });
        }
        if self.strips.acc_no_part > 0 {
            let n = self.strips.acc_no_part;
            let which =
                if n == 1 { "1 strip accessory has no part number".to_string() } else { format!("{n} strip accessories have no part number") };
            out.push(Check { kind: Kind::NoAccessoryPart, page: None, at: None, object: Object::None, which, fix: Some(Fix::ShowReports) });
        }
        let mut unknown = 0;
        for r in self.cables.values() {
            let tag = r.tag.clone().unwrap_or_else(|| "?".into());
            let Some(offer) = &r.count.offer else {
                unknown += 1;
                for i in &r.count.twice {
                    out.push(self.core_twice(r, &tag, *i));
                }
                continue;
            };
            if let Some(first) = r.count.over.first().and_then(|i| r.members.get(*i)) {
                let go = self.wire(*first).and_then(|w| w.go);
                let named = match (&r.kind, &r.count.offer_text) {
                    (Some(k), _) => k.clone(),
                    (None, Some(t)) => t.clone(),
                    (None, None) => String::new(),
                };
                out.push(Check {
                    kind: Kind::TooManyCores,
                    page: go.map(|(p, _)| p),
                    at: go.map(|(_, a)| a),
                    object: Object::Conductor(*first),
                    which: format!("{tag}: {} cores used, cable offers {} ({named})", r.members.len(), offer.len()),
                    fix: Some(Fix::SetCores(r.cable)),
                });
            }
            for i in &r.count.twice {
                out.push(self.core_twice(r, &tag, *i));
            }
        }
        if unknown > 0 {
            let which = if unknown == 1 { "1 cable has no core count".to_string() } else { format!("{unknown} cables have no core count") };
            out.push(Check { kind: Kind::NoCoreCount, page: None, at: None, object: Object::None, which, fix: Some(Fix::ShowCables) });
        }
        out
    }

    fn core_twice(&self, r: &CableRow, tag: &str, i: usize) -> Check {
        let w = r.members.get(i).copied();
        let go = w.and_then(|w| self.wire(w)).and_then(|x| x.go);
        let label = w.and_then(|w| self.wire(w)).map(|x| x.cable.clone()).unwrap_or_default();
        let core = label.rsplit(" core ").next().unwrap_or("?").to_string();
        Check {
            kind: Kind::CoreTwice,
            page: go.map(|(p, _)| p),
            at: go.map(|(_, a)| a),
            object: w.map_or(Object::None, Object::Conductor),
            which: format!("{tag}: core {core} is used by two wires"),
            fix: w.map(Fix::NextFreeCore),
        }
    }

    /// The check rows by severity: (errors, warnings, info), as [`Outputs::checks`] would give
    /// them. O(1): kept by every change (brief 027's badge).
    pub fn check_counts(&self) -> (usize, usize, usize) {
        let info = self.box_counts[1]
            + usize::from(self.strips.acc_no_part > 0)
            + usize::from(self.no_part_number > 0)
            + usize::from(self.no_location > 0 && self.no_location < self.devices.len())
            + usize::from(self.cable_checks[2] > 0);
        // Brief 028: the strips' and cables' rows, kept with them.
        let warnings =
            self.own_box + self.box_counts[0] + self.strips.check_warnings + self.cable_checks[0] + self.cable_checks[1] + self.links_off_strip.len();
        (self.strips.check_errors, warnings, info)
    }

    /// What the check rows depend on besides [`Outputs::checks_changed`]: the two count rows.
    pub fn checks_key(&self) -> (u64, usize, usize, usize) {
        (self.checks_changed, self.no_part_number, self.no_location, self.devices.len())
    }

    /// How many sheets the BOM-by-location page needs (design 024 §8.2): from the rows per
    /// location only (O(locations)), never the BOM rows themselves.
    pub fn bom_sheets(&self) -> usize {
        let per = crate::generated::ROWS_PER_SHEET;
        let (mut sheets, mut used) = (1, 0);
        // Brief 028: the strips' rows per location, kept with them (O(locations), never the rows).
        let mut rows = self.loc_rows.clone();
        for (loc, n) in &self.strips.bom_rows {
            *rows.entry(loc_key(loc.as_deref())).or_default() += n;
        }
        for n in rows.values() {
            if used == per {
                sheets += 1;
                used = 0;
            }
            used += 1;
            let mut left = *n;
            while left > 0 {
                if used == per {
                    sheets += 1;
                    used = 1;
                }
                let take = left.min(per - used);
                used += take;
                left -= take;
            }
        }
        sheets
    }

    /// The `link.<n>` keys a device that is not a strip carries (kept per device, read on demand).
    pub fn link_keys_off_strip(&self, c: ComponentId) -> Vec<String> {
        if !self.links_off_strip.contains(&c) {
            return Vec::new();
        }
        self.link_keys.get(&c).cloned().unwrap_or_default()
    }

    /// One device's row.
    pub fn device(&self, c: ComponentId) -> Option<&DeviceRow> {
        self.devices.get(&c)
    }

    /// How many devices.
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// A strip's BOM rows (design 028 §9.1, Q13; brief 036 §7.2): one row per terminal type, Qty
    /// in positions (a three-level terminal is one part), in catalogue order.
    fn strip_bom(&self, s: &crate::strips::StripData, location: Option<String>) -> Vec<BomRow> {
        let mut out = Vec::new();
        for t in &s.types {
            if t.qty() == 0 {
                continue;
            }
            out.push(BomRow {
                location: location.clone(),
                qty: t.qty(),
                tags: vec![s.tag.clone()],
                devices: vec![s.component],
                description: Some(t.description.clone()),
                maker: t.maker.clone(),
                part_number: t.part_number.clone(),
                accessory: None,
            });
        }
        // Brief 030 (design 030 §7.4): one row per accessory part, only for what is placed.
        for a in &s.acc.bom {
            out.push(BomRow {
                location: location.clone(),
                qty: a.qty,
                tags: vec![s.tag.clone()],
                devices: vec![s.component],
                description: Some(a.description.clone()),
                maker: a.maker.clone(),
                part_number: a.part_number.clone(),
                accessory: Some(a.accessory),
            });
        }
        out
    }

    /// The BOM rows (design 024 §6.1): devices, and each strip per terminal type (brief 028).
    pub fn bom(&self, group: Group) -> Vec<BomRow> {
        let row = |set: &BTreeSet<((String, u64, String), ComponentId)>, location: Option<String>| {
            let devices: Vec<ComponentId> = set.iter().map(|(_, c)| *c).collect();
            let first = devices.first().and_then(|c| self.devices.get(c));
            BomRow {
                location,
                qty: devices.len(),
                tags: set.iter().map(|((_, _, t), _)| t.clone()).collect(),
                description: first.and_then(|d| d.description.clone()),
                maker: first.and_then(|d| d.maker.clone()),
                part_number: first.and_then(|d| d.part_number.clone()),
                devices,
                accessory: None,
            }
        };
        match group {
            Group::Location => {
                let mut v: Vec<(LocKey, (String, u64, String), BomRow)> = self
                    .by_location
                    .iter()
                    .filter_map(|((lk, _), set)| {
                        let first = set.iter().next()?.0.clone();
                        let loc = (!lk.0).then(|| lk.2.clone());
                        Some((lk.clone(), first, row(set, loc)))
                    })
                    .collect();
                for s in self.strips.strips.values() {
                    for r in self.strip_bom(s, s.location.clone()) {
                        v.push((loc_key(s.location.as_deref()), tag_key(&s.tag), r));
                    }
                }
                v.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
                v.into_iter().map(|(_, _, r)| r).collect()
            }
            Group::Part => {
                let mut v: Vec<((String, u64, String), BomRow)> = self
                    .by_part
                    .values()
                    .filter_map(|set| {
                        let first = set.iter().next()?.0.clone();
                        let mut counts: BTreeMap<LocKey, usize> = BTreeMap::new();
                        for (_, c) in set {
                            if let Some(d) = self.devices.get(c) {
                                *counts.entry(d.loc_key()).or_default() += 1;
                            }
                        }
                        let locs: Vec<String> =
                            counts.iter().map(|(k, n)| format!("{} \u{d7}{n}", if k.0 { "?".to_string() } else { k.2.clone() })).collect();
                        Some((first, row(set, Some(locs.join(", ")))))
                    })
                    .collect();
                for s in self.strips.strips.values() {
                    let loc = s.location.clone().unwrap_or_else(|| "?".into());
                    for r in self.strip_bom(s, Some(format!("{loc} \u{d7}1"))) {
                        v.push((tag_key(&s.tag), r));
                    }
                }
                v.sort_by(|a, b| a.0.cmp(&b.0));
                v.into_iter().map(|(_, r)| r).collect()
            }
        }
    }

    /// The wire rows, sorted by wire number naturally, then conductor ID.
    pub fn wires(&self) -> Vec<&WireRow> {
        let mut v: Vec<&WireRow> = self.wires.values().collect();
        v.sort_by(|a, b| {
            let ka = loc_key(a.wire_number.as_deref());
            let kb = loc_key(b.wire_number.as_deref());
            ka.cmp(&kb).then(a.conductor.cmp(&b.conductor))
        });
        v
    }

    /// The cable rows, sorted by tag naturally.
    pub fn cables(&self) -> Vec<&CableRow> {
        self.sorts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut v: Vec<&CableRow> = self.cables.values().collect();
        v.sort_by(|a, b| loc_key(a.tag.as_deref()).cmp(&loc_key(b.tag.as_deref())).then(a.cable.cmp(&b.cable)));
        v
    }

    /// One wire's row.
    pub fn wire(&self, w: ConductorId) -> Option<&WireRow> {
        self.wires.get(&w)
    }

    /// The BOM as a table (`Location` or `Locations` first).
    pub fn bom_table(&self, group: Group) -> Table {
        let first = if group == Group::Location { "Location" } else { "Locations" };
        let mut t = Table { headers: vec![first, "Qty", "Tags", "Description", "Maker", "Part number"], rows: Vec::new() };
        for r in self.bom(group) {
            let who = r.tags.first().cloned().unwrap_or_default();
            t.rows.push(vec![
                Cell::opt(&r.location, || format!("{who} has no location.")),
                Cell::ok(r.qty.to_string()),
                Cell::ok(r.tags.join(", ")),
                Cell::opt(&r.description, || format!("{who} has no description.")),
                Cell::opt(&r.maker, || format!("{who} has no maker.")),
                Cell::opt(&r.part_number, || format!("{who} has no part number.")),
            ]);
        }
        t
    }

    /// The wire report as a table.
    pub fn wire_table(&self) -> Table {
        let mut t = Table {
            headers: vec!["Wire number", "From", "From location", "To", "To location", "Cross-section", "Colour", "Potential", "Cable", "Page"],
            rows: Vec::new(),
        };
        for r in self.wires() {
            let style = self.style;
            let end_loc = |e: &EndCell| match (&e.device, &e.location) {
                (Some(_), None) => Cell { text: "?".into(), flag: Some(format!("{} has no location.", e.text(style).unwrap_or_default())) },
                (_, Some(l)) => Cell::ok(l.clone()),
                (None, None) => Cell::default(),
            };
            t.rows.push(vec![
                Cell::opt(&r.wire_number, || "The wire has no number.".into()),
                Cell::opt(&r.from.text(style), || "Loose end.".into()),
                end_loc(&r.from),
                Cell::opt(&r.to.text(style), || "Loose end.".into()),
                end_loc(&r.to),
                Cell::opt(&r.cross_section, || "No cross-section, and its potential gives none.".into()),
                Cell::opt(&r.colour.as_deref().map(|c| ecad_model::wiring::colour_text(self.colour_style, c)), || {
                    "No colour, and its potential gives none.".into()
                }),
                Cell::ok(r.potential.clone().unwrap_or_default()),
                Cell::ok(r.cable.clone()),
                Cell::opt(&r.page.as_ref().map(|p| p.text(style)), || "The wire has no route.".into()),
            ]);
        }
        t
    }

    /// The cable report as a table; `Maker`, `Length` and `Designation` follow when any cable
    /// has them.
    pub fn cable_table(&self) -> Table {
        let rows = self.cables();
        let any = |f: fn(&CableRow) -> &Option<String>| rows.iter().any(|r| f(r).is_some());
        let extra: Vec<(&'static str, fn(&CableRow) -> &Option<String>)> = [
            ("Maker", (|r: &CableRow| &r.maker) as fn(&CableRow) -> &Option<String>),
            ("Length", |r| &r.length),
            ("Designation", |r| &r.designation),
        ]
        .into_iter()
        .filter(|(_, f)| any(*f))
        .collect();
        let mut t = Table { headers: vec!["Tag", "Type", "Cores", "From location", "To location", "Used", "Spare", "From", "To"], rows: Vec::new() };
        t.headers.extend(extra.iter().map(|(h, _)| *h));
        for r in rows {
            let why = r.location_reason.clone();
            let loc = |v: &Option<String>| match v {
                Some(l) => Cell::ok(l.clone()),
                None => Cell { text: "?".into(), flag: Some(why.clone().unwrap_or_default()) },
            };
            let mut row = vec![
                Cell::opt(&r.tag, || "The cable has no tag.".into()),
                Cell::opt(&r.kind, || "The cable has no type.".into()),
                Cell::ok(r.core_total().to_string()),
                loc(&r.from_location),
                loc(&r.to_location),
                Cell::ok(r.count.used().to_string()),
                Cell::opt(&r.count.spare_text(), || "Set the cable's cores or type (7G1.5) to count spares.".into()),
                Cell::ok(ends_text(&r.from_location, &r.from_devices)),
                Cell::ok(ends_text(&r.to_location, &r.to_devices)),
            ];
            for (_, f) in &extra {
                row.push(Cell::ok(f(r).clone().unwrap_or_default()));
            }
            t.rows.push(row);
        }
        t
    }
}

impl Outputs {
    /// The terminal plan as a table (Reports ▸ Terminals, CSV): every strip, or `strip` only;
    /// one line per position and conductor (design 028 §5.1), with where each line goes.
    pub fn terminal_table(&self, strip: Option<ComponentId>) -> (Table, Vec<Option<(PageId, Pt)>>) {
        self.terminal_table_with(strip, true)
    }

    /// [`Outputs::terminal_table`], with or without the accessory gap rows (brief 030: the
    /// Terminals tab's Accessories check box; view only).
    pub fn terminal_table_with(&self, strip: Option<ComponentId>, accessories: bool) -> (Table, Vec<Option<(PageId, Pt)>>) {
        let style = self.style;
        let mut t = Table {
            headers: vec!["Strip", "Inside target", "Inside wire", "Terminal", "Bridge", "Outside wire", "Cable", "Core", "Outside target", "Place"],
            rows: Vec::new(),
        };
        let mut go = Vec::new();
        let strips: Vec<&crate::strips::StripData> = match strip {
            Some(c) => self.strips.get(c).into_iter().collect(),
            None => self.strips.sorted(),
        };
        for s in strips {
            for item in s.plan(accessories) {
                let (ri, r) = match item {
                    crate::strips::PlanItem::Term(i, r) => (i, r),
                    crate::strips::PlanItem::Gap { gap, acc } => {
                        let pe_before = gap_pe(s, gap);
                        let words =
                            if acc == crate::accessories::Acc::StripLabel { format!("strip label {}", s.tag) } else { acc.words().to_string() };
                        let mut row = vec![Cell::ok(s.tag.clone()), Cell::ok(words), Cell::default(), Cell::ok(acc.mark())];
                        row.extend(std::iter::repeat_n(Cell::default(), 5));
                        row.push(Cell::opt(&s.acc.gap_part(acc, pe_before).cloned(), || "No part number for this accessory.".into()));
                        t.rows.push(row);
                        go.push(None);
                        continue;
                    }
                };
                let marker = if s.acc.markers.contains(&r.label) { " \u{25ad}" } else { "" };
                if r.spare() {
                    let mut row = vec![Cell::ok(s.tag.clone()), Cell::ok("spare"), Cell::default()];
                    row.push(Cell::ok(if r.pe { format!("{} PE{marker}", r.label) } else { format!("{}{marker}", r.label) }));
                    row.push(Cell::ok(s.bridge_cell(ri)));
                    row.extend(std::iter::repeat_n(Cell::default(), 5));
                    t.rows.push(row);
                    go.push(None);
                    continue;
                }
                let lines = r.inside.len().max(r.outside.len()).max(1);
                for i in 0..lines {
                    let (inside, outside) = (r.inside.get(i), r.outside.get(i));
                    let side = |c: Option<&crate::strips::SideCell>, f: fn(&crate::strips::SideCell) -> Option<String>, why: &str| match c {
                        None => Cell::default(),
                        Some(c) => Cell::opt(&f(c), || why.to_string()),
                    };
                    let label = if i == 0 { format!("{}{marker}", r.label) } else { format!("{}\u{2032}", r.label) };
                    let cable = outside.and_then(|c| c.cable.as_ref());
                    t.rows.push(vec![
                        Cell::ok(s.tag.clone()),
                        side(inside, |c| c.target.clone(), "A loose far end."),
                        side(inside, |c| c.wire.clone(), "The wire has no number."),
                        Cell::ok(if r.pe { format!("{label} PE") } else { label }),
                        Cell::ok(if i == 0 { s.bridge_cell(ri) } else { String::new() }),
                        side(outside, |c| c.wire.clone(), "The wire has no number."),
                        Cell::ok(cable.map_or_else(String::new, |(_, t)| t.clone().unwrap_or_else(|| "?".into()))),
                        match (cable, outside) {
                            (Some(_), Some(c)) => Cell::opt(&c.core, || "The core has no label.".into()),
                            _ => Cell::default(),
                        },
                        side(outside, |c| c.target.clone(), "A loose far end."),
                        Cell::ok(if i == 0 { r.place.as_ref().map(|p| p.text(style)).unwrap_or_default() } else { String::new() }),
                    ]);
                    go.push(r.go);
                }
            }
        }
        (t, go)
    }

    /// The terminal plan as the release writes it (027's `terminal-plan` slot): one line per
    /// position and conductor of every strip in tag order, columns named by `stripplan`'s keys
    /// (design 028 §10); an absent value is an empty cell.
    pub fn terminal_plan_csv(&self) -> Table {
        let style = self.style;
        let mut t = Table {
            headers: vec![
                "tag",
                "location",
                "offered",
                "label",
                "type",
                "state",
                "inside_target",
                "inside_wire",
                "bridge_group",
                "outside_wire",
                "cable",
                "core",
                "outside_target",
                "place",
                // Brief 030 (design 030 §9.3).
                "kind",
                "accessory",
                "gap",
                "marker",
                "links",
            ],
            rows: Vec::new(),
        };
        let opt = |v: Option<&String>| Cell::ok(v.cloned().unwrap_or_default());
        for s in self.strips.sorted() {
            for item in s.plan(true) {
                let r = match item {
                    crate::strips::PlanItem::Term(_, r) => r,
                    crate::strips::PlanItem::Gap { gap, acc } => {
                        let mut row =
                            vec![Cell::ok(s.tag.clone()), opt(s.location.as_ref()), Cell::ok(s.offer.map(|o| o.text()).unwrap_or_default())];
                        row.extend([Cell::default(), Cell::default(), Cell::ok("accessory")]);
                        row.extend(std::iter::repeat_n(Cell::default(), 7));
                        row.push(Cell::ok(s.acc.gap_part(acc, gap_pe(s, gap)).cloned().unwrap_or_else(|| "?".into())));
                        row.extend([Cell::ok("accessory"), Cell::ok(acc.key()), Cell::ok(gap), Cell::default(), Cell::default()]);
                        t.rows.push(row);
                        continue;
                    }
                };
                let marker = if s.acc.markers.contains(&r.label) { "yes" } else { "" };
                let links: Vec<&str> = s.acc.links.iter().filter(|l| l.labels.contains(&r.label)).map(|l| l.text.as_str()).collect();
                let lines = r.inside.len().max(r.outside.len()).max(1);
                for i in 0..lines {
                    let (inside, outside) = (r.inside.get(i), r.outside.get(i));
                    let label = if i == 0 { r.label.clone() } else { format!("{}\u{2032}", r.label) };
                    t.rows.push(vec![
                        Cell::ok(s.tag.clone()),
                        opt(s.location.as_ref()),
                        Cell::ok(s.offer.map(|o| o.text()).unwrap_or_default()),
                        Cell::ok(label),
                        Cell::ok(if r.pe { "pe" } else { "feed-through" }),
                        Cell::ok(if r.spare() { "spare" } else { "drawn" }),
                        opt(inside.and_then(|c| c.target.as_ref())),
                        opt(inside.and_then(|c| c.wire.as_ref())),
                        Cell::ok(r.bridge_group.filter(|_| i == 0).map(|g| g.to_string()).unwrap_or_default()),
                        opt(outside.and_then(|c| c.wire.as_ref())),
                        Cell::ok(outside.and_then(|c| c.cable.as_ref()).map(|(_, t)| t.clone().unwrap_or_else(|| "?".into())).unwrap_or_default()),
                        opt(outside.and_then(|c| c.core.as_ref())),
                        opt(outside.and_then(|c| c.target.as_ref())),
                        Cell::ok(if i == 0 { r.place.as_ref().map(|p| p.text(style)).unwrap_or_default() } else { String::new() }),
                        Cell::ok("terminal"),
                        Cell::default(),
                        Cell::default(),
                        Cell::ok(marker),
                        Cell::ok(links.join("; ")),
                    ]);
                }
            }
        }
        t
    }

    /// The cable schedule as the release writes it (027's `cable-schedule` slot): every cable in
    /// tag order with its cores (`cablecores`' keys: `label`, `state`, `wire`, `from`, `to`), an
    /// over core after the offered ones; an absent value is an empty cell.
    pub fn cable_schedule_csv(&self) -> Table {
        let mut t = Table { headers: vec!["cable", "type", "offer", "label", "state", "wire", "from", "to"], rows: Vec::new() };
        for r in self.cables() {
            let head = [
                Cell::ok(r.tag.clone().unwrap_or_default()),
                Cell::ok(r.kind.clone().unwrap_or_default()),
                Cell::ok(r.count.offer_text.clone().unwrap_or_default()),
            ];
            for row in self.core_table(r.cable).rows {
                let mut line = head.to_vec();
                // core_table: Core, Wire number, From, To, State; a spare core has no From.
                let get = |i: usize| row.get(i).map(|c| c.text.clone()).unwrap_or_default();
                let state = get(4);
                let from = if state == "spare" { String::new() } else { get(2) };
                line.extend([Cell::ok(get(0)), Cell::ok(state), Cell::ok(get(1)), Cell::ok(from), Cell::ok(get(3))]);
                t.rows.push(line);
            }
        }
        t
    }

    /// One cable's core list (design 028 §6.3): core, wire number, from, to, state; spare cores
    /// show `spare` in From.
    pub fn core_table(&self, k: CableId) -> Table {
        let mut t = Table { headers: vec!["Core", "Wire number", "From", "To", "State"], rows: Vec::new() };
        let Some(r) = self.cables.get(&k) else { return t };
        let style = self.style;
        let member = |i: usize, label: Option<String>, state: &str| {
            let w = r.members.get(i).and_then(|w| self.wires.get(w));
            vec![
                Cell::opt(&label, || "The core has no label.".into()),
                Cell::ok(w.and_then(|w| w.wire_number.clone()).unwrap_or_default()),
                Cell::ok(w.and_then(|w| w.from.text(style)).unwrap_or_default()),
                Cell::ok(w.and_then(|w| w.to.text(style)).unwrap_or_default()),
                Cell::ok(state),
            ]
        };
        match &r.count.offer {
            Some(offer) => {
                for label in offer {
                    match r.count.matched.iter().position(|m| m.as_deref() == Some(label.as_str())) {
                        Some(i) => t.rows.push(member(i, Some(label.clone()), "used")),
                        None => t.rows.push(vec![Cell::ok(label.clone()), Cell::default(), Cell::ok("spare"), Cell::default(), Cell::ok("spare")]),
                    }
                }
                for i in &r.count.over {
                    t.rows.push(member(*i, r.member_cores.get(*i).cloned().flatten(), "over"));
                }
            }
            None => {
                for i in 0..r.members.len() {
                    t.rows.push(member(i, r.member_cores.get(i).cloned().flatten(), "used"));
                }
            }
        }
        t
    }
}

/// Whether `page` exists and is drawn from the report models.
fn needs_outputs(doc: &Document, page: PageId) -> bool {
    doc.page_order.contains(&page) && crate::generated::kind_of(doc, page).is_some_and(crate::generated::GenKind::needs_outputs)
}

/// A cable end as the schedule shows it: `+CP1 -X01` (design 028 §6.3).
fn ends_text(location: &Option<String>, devices: &[String]) -> String {
    let mut parts: Vec<String> = location.iter().cloned().collect();
    parts.extend(devices.iter().cloned());
    parts.join(" ")
}

/// A point where two outlines' segments cross, if they do.
fn crossing(a: &[Pt], b: &[Pt]) -> Option<Pt> {
    let segs = |v: &[Pt]| -> Vec<(Pt, Pt)> { (0..v.len()).filter_map(|i| Some((*v.get(i)?, *v.get((i + 1) % v.len())?))).collect() };
    for (p, q) in segs(a) {
        for (r, s) in segs(b) {
            let d = (q.x - p.x) * (s.y - r.y) - (q.y - p.y) * (s.x - r.x);
            if d.abs() < 1e-12 {
                continue;
            }
            let t = ((r.x - p.x) * (s.y - r.y) - (r.y - p.y) * (s.x - r.x)) / d;
            let u = ((r.x - p.x) * (q.y - p.y) - (r.y - p.y) * (q.x - p.x)) / d;
            if t > 1e-9 && t < 1.0 - 1e-9 && u > 1e-9 && u < 1.0 - 1e-9 {
                return Some(Pt::new(p.x + t * (q.x - p.x), p.y + t * (q.y - p.y)));
            }
        }
    }
    None
}

// ---------------------------------------------------------------- CSV (design 024 §6.3)

/// A table as CSV: UTF-8 with a byte-order mark, comma separator, CRLF, RFC 4180 quoting. With
/// `excel_safe`, a cell starting with `=`, `+`, `-`, `@`, tab or CR (or the full-width `＝ ＋ － ＠`,
/// which Excel also reads as formulas) gets a leading `'`.
pub fn csv(t: &Table, excel_safe: bool) -> String {
    let cell = |s: &str| {
        let s = if excel_safe && s.starts_with(['=', '+', '-', '@', '\t', '\r', '\u{ff1d}', '\u{ff0b}', '\u{ff0d}', '\u{ff20}']) {
            format!("'{s}")
        } else {
            s.to_string()
        };
        if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s }
    };
    let mut out = String::from("\u{feff}");
    out.push_str(&t.headers.iter().map(|h| cell(h)).collect::<Vec<_>>().join(","));
    out.push_str("\r\n");
    for r in &t.rows {
        out.push_str(&r.iter().map(|c| cell(&c.text)).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    }
    out
}

/// A table's rows as JSON objects keyed by `keys`, with the flagged keys in `flags`.
pub fn rows_json(t: &Table, keys: &[&str], ids: &[Value]) -> Vec<Value> {
    t.rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let mut o = serde_json::Map::new();
            if let Some(m) = ids.get(i).and_then(Value::as_object) {
                for (k, v) in m {
                    o.insert(k.clone(), v.clone());
                }
            }
            let mut flags = Vec::new();
            for (k, c) in keys.iter().zip(r) {
                let v = match (c.flag.is_some(), *k) {
                    (true, _) => Value::Null,
                    (false, "qty" | "cores") => c.text.parse::<u64>().map_or_else(|_| json!(c.text), |n| json!(n)),
                    (false, _) => json!(c.text),
                };
                o.insert((*k).to_string(), v);
                if c.flag.is_some() {
                    flags.push(json!(k));
                }
            }
            o.insert("flags".into(), Value::Array(flags));
            Value::Object(o)
        })
        .collect()
}

/// Whether the position before gap `gap` of strip `s` is PE (the end gap: the last position).
pub fn gap_pe(s: &crate::strips::StripData, gap: &str) -> bool {
    match gap {
        "start" => false,
        "end" => s.rows.last().is_some_and(|r| r.pe),
        l => s.rows.iter().rev().find(|r| r.label == l).is_some_and(|r| r.pe),
    }
}

/// Brief 030's Checks rows of one strip (design 030 §8): orphan places, links across partitions,
/// links naming missing positions. The go-to is the strip's last terminal, or the link's first
/// drawn terminal.
pub fn accessory_checks(s: &crate::strips::StripData) -> Vec<Check> {
    let mut out = Vec::new();
    let last = s.highest().and_then(|r| r.go);
    let obj = |r: Option<&crate::strips::TermRow>| r.and_then(|r| r.terminal).map_or(Object::Component(s.component), Object::Terminal);
    for (i, (key, items)) in s.acc.orphans.iter().enumerate() {
        let spot = crate::accessories::spot_of(key);
        let label = spot.as_ref().and_then(|x| x.label()).unwrap_or("?");
        let words: Vec<&str> = items.iter().map(|a| a.words()).collect();
        let how = if spot.as_ref().is_some_and(|x| x.is_on()) { "on" } else { "after" };
        out.push(Check {
            kind: Kind::AccessoryOrphan,
            page: last.map(|(p, _)| p),
            at: last.map(|(_, a)| a),
            object: obj(s.highest()),
            which: format!("{}: {} {how} {label}, but {} has no position {label}", s.tag, words.join(", "), s.tag),
            fix: Some(Fix::AccessoryOrphan(s.component, u16::try_from(i).unwrap_or(u16::MAX))),
        });
    }
    for l in &s.acc.links {
        let first = l.labels.iter().find_map(|x| s.rows.iter().find(|r| r.label == *x && !r.spare()));
        let go = first.and_then(|r| r.go).or(last);
        let n = u16::try_from(l.n).unwrap_or(u16::MAX);
        for at in &l.crosses {
            let i = s.rows.iter().position(|r| r.label == *at).unwrap_or(0);
            out.push(Check {
                kind: Kind::LinkAcrossPartition,
                page: go.map(|(p, _)| p),
                at: go.map(|(_, a)| a),
                object: obj(first),
                which: format!("{}: link {} crosses the partition after {at}", s.tag, l.text),
                fix: Some(Fix::LinkAcrossPartition(s.component, n, u16::try_from(i).unwrap_or(u16::MAX))),
            });
        }
        if !l.missing.is_empty() {
            out.push(Check {
                kind: Kind::LinkOrphan,
                page: go.map(|(p, _)| p),
                at: go.map(|(_, a)| a),
                object: obj(first),
                which: format!("{}: link {} names {}, which {} does not have", s.tag, l.text, l.missing.join(", "), s.tag),
                fix: Some(Fix::LinkOrphan(s.component, n)),
            });
        }
    }
    out
}

/// The JSON forms that carry out a brief 030 fix, one per button: (button words, command,
/// params), each one transaction (**Move to end**, **Remove**; **Split link at 4**, **Remove
/// partition**; **Drop missing positions**, **Delete link**). Empty when the strip or key is gone.
pub fn accessory_fix_commands(o: &Outputs, fix: Fix) -> Vec<(String, &'static str, serde_json::Value)> {
    use serde_json::json;
    match fix {
        Fix::AccessoryOrphan(c, i) => {
            let Some((key, items)) = o.strips.get(c).and_then(|s| s.acc.orphans.get(usize::from(i))) else { return Vec::new() };
            let mut out = Vec::new();
            if !key.starts_with(crate::accessories::ON_PREFIX) {
                let words: Vec<&str> = items.iter().map(|a| a.key()).collect();
                out.push(("Move to end".to_string(), "accessory", json!({"strip": c.hex(), "at": "end", "add": words, "clear": [key]})));
            }
            out.push(("Remove".to_string(), "setprop", json!({"component": c.hex(), "key": key, "value": null})));
            out
        }
        Fix::LinkAcrossPartition(c, n, i) => {
            let Some(label) = o.strips.get(c).and_then(|s| s.rows.get(usize::from(i))).map(|r| r.label.clone()) else { return Vec::new() };
            vec![
                (format!("Split link at {label}"), "jumper", json!({"link": {"strip": c.hex(), "n": n}, "split_at": label})),
                ("Remove partition".to_string(), "accessory", json!({"strip": c.hex(), "at": label, "remove": ["partition"]})),
            ]
        }
        Fix::LinkOnNonStrip(c) => o
            .link_keys_off_strip(c)
            .into_iter()
            .take(1)
            .map(|k| (format!("Remove {k}"), "setprop", json!({"component": c.hex(), "key": k, "value": null})))
            .collect(),
        Fix::LinkOrphan(c, n) => vec![
            ("Drop missing positions".to_string(), "jumper", json!({"link": {"strip": c.hex(), "n": n}, "drop_missing": true})),
            ("Delete link".to_string(), "jumper", json!({"link": {"strip": c.hex(), "n": n}, "delete": true})),
        ],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_is_excel_safe_and_quoted() {
        let t = Table {
            headers: vec!["Tags", "Description"],
            rows: vec![vec![Cell::ok("-K01, -K02"), Cell::ok("Say \"hi\"")], vec![Cell::ok("+CP1"), Cell::ok("=SUM(A1)")]],
        };
        let s = csv(&t, true);
        assert!(s.starts_with('\u{feff}'));
        assert!(s.contains("\"'-K01, -K02\",\"Say \"\"hi\"\"\"\r\n"));
        assert!(s.contains("'+CP1,'=SUM(A1)\r\n"));
        let plain = csv(&t, false);
        assert!(plain.contains("+CP1,=SUM(A1)\r\n"));
    }
}
