//! The report models: the BOM, the wire report and the cable
//! report, the device locations they show, and the location rows of the Checks list.
//!
//! # Semantics
//! - `ReportModels` keeps one row per device (a component with a tag and no `kind`), per conductor
//!   and per cable, and the BOM groups. It is built whole only on open, reload or first show
//!   (`kept::ReportModels::build_with`, by `derived::ReportsView`); after that `kept::ReportModels::update`
//!   recomputes only the rows a transaction's effects could change. Nothing here is stored or recorded.
//! - Wires never appear on the BOM; cable cores are conductors and appear on the wire report with
//!   their cable. Cables are not on the BOM. A terminal device counts once.
//! - A missing value is `None` and shown as `?`; it is flagged and counted, never refused.
//!
//! # Update map
//! | Op (kind, target) | Recomputed |
//! | --- | --- |
//! | `CreateDevice` / `DeleteDevice` / `SetProperty` / terminal and representation ops of a device, its items moved | that device; the conductors at its connection points; their cables |
//! | the same of a location box on page P | P's box checks only: a device's location is stored, never derived from a box |
//! | `SetProperty(Page P, location)`, `AddPage` / `RemovePage` of P | conductors routed on P (their Page cell); no device row |
//! | `SetProperty(Page P, number)` | conductors routed on P |
//! | `SetProperty(Project, designation.<a>.<key>.code)` (a rename in the list) | the devices, connectors and items storing that key (the one wide case, by intent) |
//! | `CreateWire` / `DeleteWire` / `SetProperty` conductor (the commit's wire rewrite included) / route ops | that conductor and its net's conductors (before and after); their cables |
//! | `CreateCable` / `DeleteCable` / `ModifyCable` | that cable; its cores |
//! | `SetProperty` of a continuation's `name` | the cables with a member attached to its old or new name group: their core chains |
//! | `SetProjectSettings` changing the potentials table | conductors on the potentials whose row changed (potential index); with a row added or removed, also those with no potential |
//! | `SetProjectSettings` changing the sheet | conductors routed on pages that use the project's sheet (their Page cell and ends' places) |
//! | `SetProjectSettings` changing colour style or reference style | no row: rows keep codes and places, tables format them |
//! | `SetProperty(Device c, part.<n>.* / article_number)`, c a device or strip | c's entries (its main part and part lines) move between rows; the article index of their part keys; c's uncovered fuses |
//! | `CreateDevice` / `DeleteDevice` / `SetProperty` of an item | that item's entry |
//! | `SetProperty(Project, location)` | also the items whose location is the project's |
//! | `SetProperty(Project, bom.*)` | no row: the tables and page re-read the view (`BomView`); the sheet count switches to the other kept count |
//! | anything else | nothing |
//!
//! This module reads the document; it changes nothing.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::document::effect::{Effect, Effects};
use ecad_model::{CableId, DeviceId, Document, LoosePartId, ObjectRef, Op, PageId, Pt, RepresentationId, Subject, WireId};
use serde_json::{Value, json};

use crate::checks::{Check, Fix, Kind, Object};
use crate::locations::{self, BoxCache, Located};
use crate::reports::bom::Qty;

pub mod bom;
pub mod commands;
pub mod kept;
pub(crate) mod view;

/// Default and largest `limit` of `bom`, `wirelist`, `cablelist`.
pub const DEFAULT_LIMIT: usize = 500;
pub const MAX_LIMIT: usize = 5000;

fn clean(v: Option<impl AsRef<str>>) -> Option<String> {
    v.map(|s| s.as_ref().trim().to_string()).filter(|s| !s.is_empty())
}

/// Each page's position in the book and shown number, worked out once per build or update
/// (O(pages)), so a wire's places never read the book order again.
pub struct PageCx {
    pages: std::collections::HashMap<PageId, (usize, String)>,
}

impl PageCx {
    pub fn of(doc: &Document) -> PageCx {
        let b = ecad_model::document::book::Book::of(doc);
        PageCx { pages: b.order.iter().enumerate().map(|(i, p)| (*p, (i, b.number(doc, *p)))).collect() }
    }
    fn pos(&self, p: PageId) -> usize {
        self.pages.get(&p).map_or(usize::MAX, |x| x.0)
    }
    fn place(&self, doc: &Document, p: PageId, at: Pt) -> Option<ecad_model::connectivity::derive::Place> {
        let number = self.pages.get(&p)?.1.as_str();
        ecad_model::connectivity::derive::place_on(&ecad_model::document::sheet::Grid::of(doc, p), number, at)
    }

    /// The number page `p` is shown with (a connection point's Place, with its grid kept by
    /// the caller).
    pub(crate) fn number(&self, p: PageId) -> Option<&str> {
        self.pages.get(&p).map(|x| x.1.as_str())
    }
}

/// Where conductor `w`'s two ends are drawn: each at the first (in book order) route end landing
/// on its connection point.
pub fn end_points(doc: &Document, w: WireId, cx: &PageCx) -> [Option<(PageId, Pt)>; 2] {
    let Some(wire) = doc.wires().get(&w) else { return [None, None] };
    wire.ends.map(|t| crate::wires::routes::drawn_ends_at(doc, t).into_iter().min_by_key(|(p, _)| cx.pos(*p)))
}

/// One device as the reports see it.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceRow {
    pub component: DeviceId,
    pub tag: String,
    pub description: Option<String>,
    pub maker: Option<String>,
    pub part_number: Option<String>,
    /// Its main part's article number.
    pub article_number: Option<String>,
    /// Its stored location (never derived from boxes or pages).
    pub location: Located,
    /// Its stored function.
    pub function: Located,
    /// Where go-to lands ([`crate::symbols::reps::goto`]: its head, else its first symbol in drawing
    /// order): the symbol, its page and its anchor. Display only: it is in no scope.
    pub goto: Option<(RepresentationId, PageId, Option<Pt>)>,
}

/// Sort key of a tag: prefix, number, text.
fn tag_key(t: &str) -> (String, u64, String) {
    let (p, n) = ecad_model::naming::tag::parse(t).unwrap_or_else(|| (ecad_model::naming::tag::key(t), 0));
    (p, n, t.to_string())
}

/// What a BOM entry belongs to: a device (or strip), or a loose part (an
/// item). Devices sort first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Owner {
    /// A device's main part (line 0) or one of its part lines.
    Device(DeviceId),
    /// An item: a loose part, always line 0.
    Loose(LoosePartId),
}

impl Owner {
    /// The device, when it is one.
    pub fn device(self) -> Option<DeviceId> {
        match self {
            Owner::Device(c) => Some(c),
            Owner::Loose(_) => None,
        }
    }

    /// The loose part, when it is one.
    pub fn loose(self) -> Option<LoosePartId> {
        match self {
            Owner::Loose(l) => Some(l),
            Owner::Device(_) => None,
        }
    }
}

/// The key an entry is grouped by on the BOM (part): maker, part number, description, compared
/// case-insensitively, and the unit (pieces and metres never add up); an entry with no
/// part number is a group of its own.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PartKey(String, String, String, Option<(Owner, u8)>, bool);

impl PartKey {
    /// The same part, but a row of its own for entry `own` (the nested layout's device rows).
    fn alone(&self, own: (Owner, u8)) -> PartKey {
        PartKey(self.0.clone(), self.1.clone(), self.2.clone(), Some(own), self.4)
    }
}

/// One BOM entry, a line of a part set or an item.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The device or the loose part it lists.
    pub owner: Owner,
    /// 0 for a device's main part or an item, else the part line.
    pub line: u8,
    /// The device's tag (empty for an item).
    pub tag: String,
    pub location: Option<String>,
    pub description: Option<String>,
    pub maker: Option<String>,
    pub part_number: Option<String>,
    pub article_number: Option<String>,
    /// A device's main part: its set rating fields as shown, by field word.
    pub ratings: Vec<(&'static str, String)>,
    /// Per device, or per sub-item when `for` names some.
    pub qty: Qty,
    pub metres: bool,
    pub kit: Option<String>,
    /// The sub-items `for` names that the device has, in `for` order.
    pub subs: Vec<String>,
    /// The sub-items `for` names that the device does not have (counted 0).
    pub missing: Vec<String>,
}

/// An entry's place in its row: tagged entries by tag, then items by description and part number.
type EntrySort = (bool, (String, u64, String), Owner, u8);

impl Entry {
    /// Whether it is an item (a loose part).
    pub fn is_item(&self) -> bool {
        matches!(self.owner, Owner::Loose(_))
    }
    fn part_key(&self) -> PartKey {
        let low = |v: &Option<String>| v.as_deref().unwrap_or("").to_lowercase();
        match &self.part_number {
            Some(_) => PartKey(low(&self.maker), low(&self.part_number), low(&self.description), None, self.metres),
            None => PartKey(String::new(), String::new(), low(&self.description), Some((self.owner, self.line)), self.metres),
        }
    }
    fn loc_key(&self) -> LocKey {
        loc_key(self.location.as_deref())
    }
    fn sort(&self) -> EntrySort {
        if self.is_item() {
            let low = |v: &Option<String>| v.as_deref().unwrap_or("").to_lowercase();
            (true, (low(&self.description), 0, low(&self.part_number)), self.owner, self.line)
        } else {
            (false, tag_key(&self.tag), self.owner, self.line)
        }
    }
    /// Whether its `for` names sub-items (its quantity is per sub-item).
    fn per_sub(&self) -> bool {
        !self.subs.is_empty() || !self.missing.is_empty()
    }
    /// The quantity it adds to its row: per sub-item named (only those the device has), or once;
    /// `None` on overflow.
    pub fn total(&self) -> Option<Qty> {
        if self.per_sub() { self.qty.checked_mul(self.subs.len()) } else { Some(self.qty) }
    }
    /// The tags it shows: `-K03`, or `-F01:1, -F01:2` for a `for` line; none for an item.
    pub fn tags(&self) -> Vec<String> {
        if self.is_item() {
            return Vec::new();
        }
        if !self.per_sub() {
            return vec![self.tag.clone()];
        }
        self.subs.iter().map(|s| format!("{}:{s}", self.tag)).collect()
    }
}

/// A location's sort key: natural order, no location (`?`) last.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct LocKey(bool, Vec<(String, u64)>, String);

fn loc_key(v: Option<&str>) -> LocKey {
    match v {
        Some(t) => LocKey(false, locations::natural_key(t), t.to_string()),
        None => LocKey(true, Vec::new(), String::new()),
    }
}

/// One end of a wire as the report shows it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EndCell {
    /// `-K01:A1`, `24V >`; `None` for a loose end.
    pub name: Option<String>,
    /// The device's location; `None` with `device` set means the device has none.
    pub location: Option<String>,
    pub device: Option<DeviceId>,
    /// (page position, column, row) of the end, for the From/To order.
    pub order: (usize, u32, u32),
    /// The end is a continuation (a core chain's inner end).
    pub continuation: bool,
}

impl EndCell {
    /// The end as shown, in reference style `style`.
    pub fn text(&self, _style: ecad_model::ReferenceStyle) -> Option<String> {
        self.name.clone()
    }
}

/// One conductor as the wire report shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct WireRow {
    pub conductor: WireId,
    pub wire_number: Option<String>,
    pub from: EndCell,
    pub to: EndCell,
    pub cross_section: Option<String>,
    /// The colour as stored (IEC code or free text); the table writes it in the colour style.
    pub colour: Option<String>,
    pub potential: Option<String>,
    /// Its wire category (ID and name), and whether its colour and cross-section are
    /// its own (overriding the category's).
    pub category: Option<(ecad_model::StyleRef, String)>,
    pub colour_own: bool,
    pub cross_section_own: bool,
    /// The category's BOM part (part number, maker, description,
    /// allowance in metres per wire), when the category has a part number.
    pub part: Option<(String, Option<String>, Option<String>, f64)>,
    pub cable: String,
    /// Where its first route's midpoint is; the table writes it in the reference style.
    pub page: Option<ecad_model::connectivity::derive::Place>,
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
    /// How its members meet its core offer.
    pub count: crate::terminal_strips::cores::CoreCount,
    /// The first member of each core chain (one member per chain without
    /// continuations), in member order, for the over and twice go-tos.
    pub members: Vec<WireId>,
    /// Their stored core labels, in member order.
    pub member_cores: Vec<Option<String>>,
    /// The last member of each core chain (its To), aligned with `members`.
    pub tails: Vec<WireId>,
    /// The devices and strips at its cores' ends in its From and To locations.
    pub from_devices: Vec<String>,
    pub to_devices: Vec<String>,
    /// Whether it is drawn (it has members or a representation); a cable built in
    /// Project ▸ Cables is not drawn until assigned.
    pub drawn: bool,
    /// Each end's planned function and location as shown (`=PUMP +CAB1`), from, to.
    pub planned: [Option<String>; 2],
    /// Each end's gland, its own or its cable type's, from, to.
    pub glands: [Option<String>; 2],
    /// Its shield word, its own or its cable type's (`none` shown as absent).
    pub shield: Option<String>,
}

impl CableRow {
    /// The Cores cell: the offered count, else the member count.
    pub fn core_total(&self) -> usize {
        self.count.offer.as_ref().map_or(self.cores, Vec::len)
    }
}

ecad_model::word_enum! {
    /// BOM grouping.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum Group {
        #[default]
        Location,
        Part,
    }
}

/// One BOM row.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BomRow {
    /// By location: the location (`None`: `?`). By part: `+CP1 ×2, +FIELD ×1`.
    pub location: Option<String>,
    /// A count, or metres when `metres`.
    pub qty: Qty,
    pub metres: bool,
    /// The sum overflowed (never a panic): `qty` is its last good value, flagged.
    pub overflow: bool,
    pub tags: Vec<String>,
    pub devices: Vec<DeviceId>,
    pub description: Option<String>,
    pub maker: Option<String>,
    pub part_number: Option<String>,
    /// The article number its entries share (the most common one when they differ,
    /// with `article_flag` saying which differ).
    pub article_number: Option<String>,
    pub article_flag: Option<String>,
    /// Per rating field word its entries have: the most common value (ties: the first in tag
    /// order), and a flag when two differ.
    pub ratings: Vec<(&'static str, String, Option<String>)>,
    /// The entries counted, `(component, line)`; empty for strip terminal and
    /// accessory rows.
    pub entries: Vec<(Owner, u8)>,
    /// The row lists items only.
    pub item: bool,
    /// Nested layout: the part line this row lists under its device, and its kit.
    pub line: Option<u8>,
    pub kit: Option<String>,
    /// The accessory a strip's row counts (`end_stop`, `jumper`…); `None` for devices
    /// and terminals.
    pub accessory: Option<&'static str>,
}

impl BomRow {
    /// The Qty cell as the window and the BOM page show it: `4`, `5.6 m`.
    pub fn qty_text(&self) -> String {
        if self.no_qty() {
            String::new()
        } else if self.metres {
            format!("{} m", self.qty)
        } else {
            self.qty.to_string()
        }
    }

    /// A wire row whose category has no allowance has no Qty (an
    /// empty cell, never "0 m"); its note gives the count of wires.
    pub fn no_qty(&self) -> bool {
        self.metres && self.qty.0 == 0 && self.tags.is_empty() && self.devices.is_empty()
    }

    /// The Description as shown: a nested line indented with `\u{21b3}` and its kit.
    pub fn description_text(&self) -> Option<String> {
        let d = self.description.clone()?;
        Some(match (self.line, &self.kit) {
            (Some(_), Some(k)) => format!("  \u{21b3} {d} (kit: {k})"),
            (Some(_), None) => format!("  \u{21b3} {d}"),
            _ => d,
        })
    }
}

/// The BOM's report settings (`bom.column.article`, `bom.column.rating`, `bom.lines`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BomView {
    pub article: bool,
    /// The rating columns: field words, in order.
    pub ratings: Vec<&'static str>,
    pub nested: bool,
}

impl BomView {
    /// The project's settings.
    pub fn of(doc: &Document) -> BomView {
        BomView { article: bom::article_column(doc), ratings: bom::rating_columns(doc), nested: bom::lines_nested(doc) }
    }
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scope {
    devices: BTreeSet<DeviceId>,
    /// Loose parts made, changed or removed (their BOM entries).
    loose: BTreeSet<LoosePartId>,
    pages: BTreeSet<PageId>,
    route_pages: BTreeSet<PageId>,
    wires: BTreeSet<WireId>,
    cables: BTreeSet<CableId>,
    project_location: bool,
    /// Entry keys of the project's list whose code changed.
    renamed: BTreeSet<String>,
    /// Potentials (lower case; `""`: none) whose table row changed.
    potentials: BTreeSet<String>,
    /// The project's sheet changed: pages without their own sheet.
    sheet: bool,
    /// Colour or reference style changed (no row work).
    styles: bool,
    /// Project `strip_*` keys that changed (the strips inheriting them).
    strip_keys: BTreeSet<String>,
    /// Project `connector.*` keys that changed (the connectors inheriting them).
    connector_keys: BTreeSet<String>,
    /// A `bom.*` report setting changed (no row work: the view is re-read).
    bom_view: bool,
    /// The project's or standard's wire category choice, or a category it resolves
    /// to, changed: every wire row.
    all_wires: bool,
}

impl Scope {
    /// Whether nothing is in scope.
    pub fn is_empty(&self) -> bool {
        !self.project_location
            && self.renamed.is_empty()
            && self.devices.is_empty()
            && self.loose.is_empty()
            && self.pages.is_empty()
            && self.route_pages.is_empty()
            && self.wires.is_empty()
            && self.cables.is_empty()
            && self.potentials.is_empty()
            && !self.sheet
            && !self.styles
            && self.strip_keys.is_empty()
            && self.connector_keys.is_empty()
            && !self.bom_view
            && !self.all_wires
    }

    /// A drawing aid made, removed or changed: a box's pages.
    fn aid<'a>(&mut self, doc: &Document, id: ecad_model::AidId, aids: impl IntoIterator<Item = &'a ecad_model::Aid>) {
        if aids.into_iter().any(|a| a.as_box().is_some()) {
            self.pages.extend(doc.representations_of(Subject::Aid(id)).map(|(_, r)| r.page));
        }
    }

    fn subject(&mut self, doc: &Document, s: Subject, page: PageId) {
        match s {
            Subject::Aid(a) => {
                if doc.aids().get(&a).is_some_and(|x| x.as_box().is_some()) {
                    self.pages.insert(page);
                }
            }
            Subject::Device(c) => {
                self.devices.insert(c);
            }
            Subject::Wire(w) => {
                self.wires.insert(w);
            }
            Subject::Cable(k) => {
                self.cables.insert(k);
            }
            // A route's drawing: the wires at the points it lands on (where their ends are drawn).
            Subject::Route(r) => {
                if let Some(route) = doc.routes().get(&r) {
                    for t in route.points_landed() {
                        self.wires.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
                    }
                }
            }
        }
    }

    /// Collect what `ops` name on `doc` (call once on the state before, once after): each effect
    /// ([`Scope::note`]), then their net mates and the users of renamed entries
    /// ([`Scope::close`]).
    pub fn collect(&mut self, doc: &Document, ops: &[Op]) {
        for effect in &Effects::of(ops) {
            self.note(doc, effect);
        }
        self.close(doc);
    }

    /// Note what one effect names, looked up in `doc` (the state before or after it).
    pub fn note(&mut self, doc: &Document, effect: &Effect<'_>) {
        match effect {
            Effect::Device { id, .. } | Effect::DeviceDefinition { id, .. } => {
                self.devices.insert(*id);
            }
            Effect::Prop { object: ObjectRef::Device(c), .. } => {
                self.devices.insert(*c);
            }
            Effect::Prop { object: ObjectRef::Wire(w), .. } | Effect::Wire { id: w, .. } => {
                self.wires.insert(*w);
            }
            // A cable is written whole (`ModifyCable`); `SetProperty` on one is refused.
            Effect::Prop { object: ObjectRef::Page(_) | ObjectRef::Cable(_), .. } => {}
            // No check or report reads a device link yet.
            Effect::DeviceLink { .. } => {}
            // A box's values are its pages' context; a continuation renamed
            // joins or breaks its name groups' cable core chains.
            Effect::Aid { id, before, after } => {
                self.aid(doc, *id, before.iter().chain(after.iter()).copied());
                if let (Some(b), Some(a)) = (before, after)
                    && ecad_model::connectivity::continuation_key(b) != ecad_model::connectivity::continuation_key(a)
                {
                    self.cables.extend(continuation_group_cables(doc, *id));
                }
            }
            // A loose part is an item on the BOM (its entry only).
            Effect::LoosePart { id, .. } => {
                self.loose.insert(*id);
            }
            Effect::Project { before, after } => {
                use crate::locations::designations::{Aspect, changed_codes, default_key};
                // An item without its own location shows the project's.
                if default_key(before, Aspect::Location) != default_key(after, Aspect::Location) {
                    self.project_location = true;
                }
                // A rename in the project's list re-keys its users' rows.
                for a in Aspect::ALL {
                    for k in changed_codes(before, after, a) {
                        // The cables planning an end there show its new code.
                        self.cables.extend(crate::cables::planning(doc, &k));
                        self.renamed.insert(k);
                    }
                }
            }
            // A standard's value of a registry key reaches the same views.
            Effect::Prop { object: ObjectRef::Project | ObjectRef::Standard, key, .. } => {
                let area = key.area;
                let key = key.raw;
                // The project's or standard's wire category reaches every wire row.
                if key == ecad_model::connectivity::wiring::WIRE_CATEGORY {
                    self.all_wires = true;
                }
                // The registry's `terminal_block.*` defaults reach strips the same way.
                if area == ecad_model::naming::keys::Area::TerminalBlock {
                    self.strip_keys.insert(key.to_string());
                }
                // The registry's `connector.*` defaults reach connectors.
                if area == ecad_model::naming::keys::Area::Connector {
                    self.connector_keys.insert(key.to_string());
                }
                // The BOM's report settings.
                if area == ecad_model::naming::keys::Area::Bom {
                    self.bom_view = true;
                }
            }
            // A wire category reaches the rows of the conductors
            // resolving to it: its explicit users, the wires of the potentials it names (before
            // and after), and every wire when it is the project's or standard's choice.
            Effect::Style { catalogue: ecad_model::StyleCatalogue::Wire, id, before, after, .. } => {
                for (_, o) in crate::styles::explicit_users(doc, &crate::styles::wire::CATALOGUE, id) {
                    if let ObjectRef::Wire(c) = o {
                        self.wires.insert(c);
                    }
                }
                for r in before.iter().chain(after.iter()) {
                    if let Some(pot) = r.body.wire().and_then(|w| w.potential.as_ref()) {
                        self.potentials.insert(pot.as_str().to_lowercase());
                    }
                }
                if let Some(r) = ecad_model::registry::styles::field_of(doc, &crate::styles::wire::CATALOGUE, id, "potential", None) {
                    self.potentials.insert(r.text().trim().to_lowercase());
                }
                if ecad_model::registry::styles::chosen(doc, &crate::defaults::styles::WIRE_CATEGORY, ecad_model::registry::Scope::project()) == **id
                {
                    self.all_wires = true;
                }
            }
            // A cable type's cores, shield and gland reach its users' rows.
            Effect::Style { catalogue: ecad_model::StyleCatalogue::Cable, id, .. } => {
                self.cables.extend(crate::styles::cable::users(doc, id));
            }
            // Other style records and the standard header change no report row.
            Effect::Style { .. } | Effect::Standard { .. } => {}
            Effect::ConnectionPoint { id, before, after } => {
                self.devices.extend(before.iter().chain(after.iter()).filter_map(|t| t.device()));
                // A relabelled terminal (Remove + Add of one ID) renames its wires' ends.
                self.wires.extend(doc.ends_at(*id).into_iter().map(|(w, _)| w));
            }
            Effect::Rep { before, after, .. } => {
                for r in before.iter().chain(after.iter()) {
                    self.subject(doc, r.subject, r.page);
                }
            }
            Effect::Item { id, .. } => {
                if let Some((_, r)) = doc.representation_of_item(*id) {
                    self.subject(doc, r.subject, r.page);
                }
            }
            // A cable is made or deleted with its cores: their wires' rows.
            Effect::Cable { id, before, after } => {
                self.cables.insert(*id);
                self.wires.extend(before.iter().chain(after.iter()).flat_map(|c| c.cores.iter()).filter_map(|m| m.wire()));
            }
            // A route moved, added or removed moves where the wires at the
            // points it lands on are drawn.
            Effect::Route { before, after, .. } => {
                for t in before.iter().chain(after.iter()).flat_map(|r| r.points_landed()) {
                    self.wires.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
                }
            }
            Effect::Settings { before, after } => {
                let rows =
                    |w: &ecad_model::connectivity::wiring::WiringSettings| -> BTreeMap<String, ecad_model::connectivity::wiring::PotentialDef> {
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
                self.styles |= before.reference_style != after.reference_style || before.wiring.colour_style() != after.wiring.colour_style();
            }
            // A page changed in place; its number and title are where rows say
            // they are.
            Effect::Page { id, before: Some(before), after: Some(after), index_before: None, index_after: None } => {
                if before.title != after.title || before.folder != after.folder {
                    self.pages.insert(*id);
                    self.route_pages.insert(*id);
                }
                if before.number != after.number {
                    self.route_pages.insert(*id);
                }
            }
            // Added, removed, or both (a move). A move also changes the go-to of the devices
            // drawn on the page (a device's first symbol in page order); the other pages keep
            // their order against each other.
            Effect::Page { id, index_before, index_after, .. } => {
                self.pages.insert(*id);
                self.route_pages.insert(*id);
                if index_before.is_some() && index_after.is_some() {
                    self.devices.extend(doc.representations_on(*id).filter_map(|(_, r)| match r.subject {
                        Subject::Device(c) => Some(c),
                        _ => None,
                    }));
                }
            }
            Effect::Definition { .. } | Effect::Layer { .. } | Effect::Version { .. } | Effect::Folder { .. } => {}
        }
    }

    /// Note what one effect names that only the state after reads (`doc`; call it there, after
    /// [`Scope::note`]). A level's label, or a separator or level
    /// names a strip reads, set on a strip changes the wire rows ending on its terminals (only the
    /// position named by a `block_label.<p>.<n>` key). A connector naming key set on a
    /// mating connector changes its pins' wire rows.
    pub fn note_after(&mut self, doc: &Document, effect: &Effect<'_>) {
        let Effect::Prop { object: ObjectRef::Device(c), key, .. } = effect else { return };
        let key = key.raw;
        let position = key.strip_prefix(crate::terminal_strips::terminal_blocks::LABEL_PREFIX).and_then(|r| r.rsplit_once('.')).map(|(p, _)| p);
        if (position.is_some() || crate::defaults::terminal_blocks::is_label_key(key)) && crate::terminal_strips::strip_of(doc, *c).is_some() {
            for (t, x) in doc.connection_points_of(*c) {
                if position.is_none_or(|p| x.label.as_deref() == Some(p)) {
                    self.wires.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
                }
            }
        }
        if crate::defaults::connectors::is_naming_key(key) && crate::connectors::connector_of(doc, *c).is_some_and(|x| x.mating) {
            for (t, _) in doc.connection_points_of(*c) {
                self.wires.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
            }
        }
    }

    /// After the effects: their wires' net mates, and the users of renamed entries.
    pub fn close(&mut self, doc: &Document) {
        // Net mates: a wire's number and potential are its net's.
        let conn = doc.connectivity();
        let mut mates = BTreeSet::new();
        for w in &self.wires {
            if let Some(net) = conn.net_of_conductor(*w).and_then(|id| conn.net(id)) {
                mates.extend(net.wires.iter().copied());
            }
        }
        self.wires.extend(mates);
        // The users of a renamed entry, the one wide case.
        if !self.renamed.is_empty() {
            for (c, x) in doc.devices() {
                if crate::locations::designations::device_keys(x).iter().any(|(_, v)| self.renamed.contains(v)) {
                    self.devices.insert(*c);
                }
            }
            for (id, p) in doc.loose_parts() {
                if p.location.is_some_and(|l| self.renamed.contains(&l.hex())) {
                    self.loose.insert(*id);
                }
            }
        }
    }
}

/// The cables with a member attached to a continuation of `c`'s name group,
/// `c` included, read on `doc` (called on the state before and after a rename, so both the old
/// and the new group): their core chains may join or break.
fn continuation_group_cables(doc: &Document, c: ecad_model::AidId) -> BTreeSet<CableId> {
    let Some(key) = crate::continuations::name_of(doc, c) else {
        return BTreeSet::new();
    };
    let mut group = doc.continuations_named(&key);
    if !group.contains(&c) {
        group.push(c);
    }
    let mut out = BTreeSet::new();
    for k in group {
        for (t, _) in doc.connection_points_of(k) {
            for (w, _) in doc.ends_at(t) {
                out.extend(doc.cables_of(w));
            }
        }
    }
    out
}

/// The members standing for `cable`'s cores, in member order, each with its stored label, and
/// each core's last member: the same wire, since a core through continuations or cable references
/// is one wire.
fn chain_heads(_doc: &Document, _k: CableId, cab: &ecad_model::Cable) -> (Vec<ecad_model::Core>, Vec<WireId>) {
    let heads: Vec<ecad_model::Core> = cab.cores.iter().filter_map(|m| Some(ecad_model::Core::on_wire(m.wire()?, m.label.clone()))).collect();
    let tails = heads.iter().filter_map(|m| m.wire()).collect();
    (heads, tails)
}

/// Take `v` out of `map[k]`; drop the set when empty. Whether a set was dropped (a row went).
fn remove_from<K: Ord, V: Ord>(map: &mut BTreeMap<K, BTreeSet<V>>, k: &K, v: &V) -> bool {
    let Some(set) = map.get_mut(k) else { return false };
    set.remove(v);
    if set.is_empty() {
        map.remove(k);
        return true;
    }
    false
}

/// One less at `k`; drop it at 0.
fn count_down<K: Ord>(map: &mut BTreeMap<K, usize>, k: &K) {
    if let Some(n) = map.get_mut(k) {
        *n = n.saturating_sub(1);
        if *n == 0 {
            map.remove(k);
        }
    }
}

/// Whether `s` is one of `subs`' names.
fn subs_has(subs: &bom::Subs, s: &str) -> bool {
    match subs {
        bom::Subs::None => false,
        bom::Subs::Fuses(v) => v.iter().any(|x| x == s),
        bom::Subs::Positions { all, .. } => all.iter().any(|x| x == s),
    }
}

/// Whether `page` exists and is drawn from the report models.
/// A half row's group (as for devices): maker, part number, description, compared
/// without case; a half with no part number is a group of its own (its connector and tag).
type HalfKey = (String, String, String, Option<(DeviceId, String)>);

fn half_key(r: &BomRow) -> HalfKey {
    let low = |v: &Option<String>| v.as_deref().unwrap_or("").to_lowercase();
    let own = if r.part_number.is_some() { None } else { r.devices.first().copied().zip(r.tags.first().cloned()) };
    (low(&r.maker), low(&r.part_number), low(&r.description), own)
}

/// Connector `c`'s BOM rows, one per drawn half, each at its half's stored
/// location.
fn connector_rows(doc: &Document, c: DeviceId, cache: &mut BoxCache) -> Vec<BomRow> {
    let halves = crate::connectors::half_rows(doc, c, cache);
    halves
        .into_iter()
        .map(|h| BomRow {
            location: h.location.value,
            qty: Qty::ONE,
            tags: vec![h.tag],
            devices: vec![c],
            description: h.description,
            maker: h.maker,
            part_number: h.part_number,
            ..BomRow::default()
        })
        .collect()
}

/// Half rows grouped as device rows are: by location and part, or by part with
/// their locations counted; a half with no part number is a row of its own.
fn group_halves<'a>(rows: impl Iterator<Item = &'a BomRow>, group: Group) -> Vec<BomRow> {
    let low = |v: &Option<String>| v.as_deref().unwrap_or("").to_lowercase();
    type Key = (String, String, String, String, usize);
    let mut groups: BTreeMap<Key, (BomRow, BTreeMap<LocKey, usize>)> = BTreeMap::new();
    for (i, r) in rows.enumerate() {
        let loc = if group == Group::Location { r.location.clone().unwrap_or_default() } else { String::new() };
        let own = if r.part_number.is_some() { 0 } else { i + 1 };
        let key = (loc, low(&r.maker), low(&r.part_number), low(&r.description), own);
        let e = groups.entry(key).or_insert_with(|| (BomRow { qty: Qty(0), tags: Vec::new(), devices: Vec::new(), ..r.clone() }, BTreeMap::new()));
        e.0.qty = Qty(e.0.qty.0.saturating_add(Qty::ONE.0));
        e.0.tags.extend(r.tags.iter().cloned());
        e.0.devices.extend(r.devices.iter().copied());
        *e.1.entry(loc_key(r.location.as_deref())).or_default() += 1;
    }
    groups
        .into_values()
        .map(|(mut r, locs)| {
            if group == Group::Part {
                let words: Vec<String> = locs.iter().map(|(k, n)| format!("{} \u{d7}{n}", if k.0 { "?".to_string() } else { k.2.clone() })).collect();
                r.location = Some(words.join(", "));
            }
            r
        })
        .collect()
}

fn needs_reports(doc: &Document, page: PageId) -> bool {
    doc.page_order().contains(&page) && crate::pages::generated::kind_of(doc, page).is_some_and(ecad_model::GenKind::needs_reports)
}

/// A cable end as the schedule shows it: `+CP1 -X01`.
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

// ---------------------------------------------------------------- CSV

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
pub(crate) fn rows_json(t: &Table, keys: &[&str], ids: &[Value]) -> Vec<Value> {
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
                    (false, "qty" | "cores") => match c.text.parse::<u64>() {
                        Ok(n) => json!(n),
                        // Metres (`5.6`).
                        Err(_) => c.text.parse::<f64>().ok().filter(|x| x.is_finite()).map_or_else(|| json!(c.text), |x| json!(x)),
                    },
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
pub(crate) fn gap_pe(s: &crate::terminal_strips::strip_model::StripData, gap: &str) -> bool {
    match gap {
        "start" => false,
        "end" => s.rows.last().is_some_and(|r| r.pe),
        l => s.rows.iter().rev().find(|r| r.label == l).is_some_and(|r| r.pe),
    }
}

/// Checks rows of one strip: orphan places, jumper bars across partitions, jumper bars naming
/// missing positions. The go-to is the strip's last terminal block, or the jumper bar's first drawn
/// terminal block.
pub(crate) fn accessory_checks(s: &crate::terminal_strips::strip_model::StripData) -> Vec<Check> {
    let mut out = Vec::new();
    let last = s.highest().and_then(|r| r.go);
    let obj = |r: Option<&crate::terminal_strips::strip_model::BlockRow>| {
        r.and_then(|r| r.connection_point).map_or(Object::Device(s.component), Object::ConnectionPoint)
    };
    for (i, (key, items)) in s.acc.orphans.iter().enumerate() {
        let spot = crate::terminal_strips::accessories::spot_of(key);
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
    for l in &s.acc.jumper_bars {
        let first = l.labels.iter().find_map(|x| s.rows.iter().find(|r| r.label == *x && !r.spare()));
        let go = first.and_then(|r| r.go).or(last);
        let n = u16::try_from(l.n).unwrap_or(u16::MAX);
        for at in &l.crosses {
            let i = s.rows.iter().position(|r| r.label == *at).unwrap_or(0);
            out.push(Check {
                kind: Kind::JumperBarAcrossPartition,
                page: go.map(|(p, _)| p),
                at: go.map(|(_, a)| a),
                object: obj(first),
                which: format!("{}: jumper bar {} crosses the partition after {at}", s.tag, l.text),
                fix: Some(Fix::JumperBarAcrossPartition(s.component, n, u16::try_from(i).unwrap_or(u16::MAX))),
            });
        }
        if !l.missing.is_empty() {
            out.push(Check {
                kind: Kind::JumperBarOrphan,
                page: go.map(|(p, _)| p),
                at: go.map(|(_, a)| a),
                object: obj(first),
                which: format!("{}: jumper bar {} names {}, which {} does not have", s.tag, l.text, l.missing.join(", "), s.tag),
                fix: Some(Fix::JumperBarOrphan(s.component, n)),
            });
        }
    }
    out
}

/// The JSON forms that carry out a fix, one per button: (button words, command,
/// params), each one transaction (**Move to end**, **Remove**; ***Split jumper bar at 4**,
/// **Remove partition**; **Drop missing positions**, **Delete jumper bar**). Empty when the strip or key is gone.
pub fn accessory_fix_commands(o: &kept::ReportModels, fix: Fix) -> Vec<(String, &'static str, serde_json::Value)> {
    use serde_json::json;
    match fix {
        Fix::AccessoryOrphan(c, i) => {
            let Some((key, items)) = o.strips.get(c).and_then(|s| s.acc.orphans.get(usize::from(i))) else { return Vec::new() };
            let mut out = Vec::new();
            if !key.starts_with(crate::terminal_strips::accessories::ON_PREFIX) {
                let words: Vec<&str> = items.iter().map(|a| a.key()).collect();
                out.push(("Move to end".to_string(), "accessory", json!({"strip": c.hex(), "at": "end", "add": words, "clear": [key]})));
            }
            out.push(("Remove".to_string(), "setprop", json!({"component": c.hex(), "key": key, "value": null})));
            out
        }
        Fix::JumperBarAcrossPartition(c, n, i) => {
            let Some(label) = o.strips.get(c).and_then(|s| s.rows.get(usize::from(i))).map(|r| r.label.clone()) else { return Vec::new() };
            vec![
                (format!("Split jumper bar at {label}"), "jumper", json!({"jumper_bar": {"strip": c.hex(), "n": n}, "split_at": label})),
                ("Remove partition".to_string(), "accessory", json!({"strip": c.hex(), "at": label, "remove": ["partition"]})),
            ]
        }
        Fix::JumperBarOnNonStrip(c) => o
            .jumper_bar_keys_off_strip(c)
            .into_iter()
            .take(1)
            .map(|k| (format!("Remove {k}"), "setprop", json!({"component": c.hex(), "key": k, "value": null})))
            .collect(),
        Fix::JumperBarOrphan(c, n) => vec![
            ("Drop missing positions".to_string(), "jumper", json!({"jumper_bar": {"strip": c.hex(), "n": n}, "drop_missing": true})),
            ("Delete jumper bar".to_string(), "jumper", json!({"jumper_bar": {"strip": c.hex(), "n": n}, "delete": true})),
        ],
        Fix::MoveToNextFree(t) => {
            vec![("Move to next free".to_string(), "terminalblocklabel", json!({"connection_point": t.hex(), "move_to": "next"}))]
        }
        Fix::MoveToNewPosition(t) => {
            vec![("Move to new position".to_string(), "terminalblocklabel", json!({"connection_point": t.hex(), "move_to": "new"}))]
        }
        Fix::LevelMissing(c, n) => {
            let Some(s) = o.strips.get(c) else { return Vec::new() };
            let key = ecad_model::connectivity::jumper_bars::key(u32::from(n));
            let kept = s.level_missing.iter().find(|(m, _)| *m == u32::from(n)).map(|(_, items)| items.clone()).unwrap_or_default();
            let after =
                s.acc.jumper_bars.iter().find(|l| l.n == u32::from(n)).map(|l| crate::terminal_strips::accessories::drop_items(&l.text, &kept));
            let mut out = Vec::new();
            if let Some(v) = after {
                let v = if v.is_empty() { serde_json::Value::Null } else { json!(v) };
                out.push(("Drop missing levels".to_string(), "setprop", json!({"component": c.hex(), "key": key, "value": v})));
            }
            out.push(("Delete jumper bar".to_string(), "jumper", json!({"jumper_bar": {"strip": c.hex(), "n": n}, "delete": true})));
            out
        }
        Fix::Plc(c, f) => o.plc.fix_commands(c, f),
        Fix::UseArticle(c, n) => {
            let Some(e) = o.entry(c, n) else { return Vec::new() };
            let Some(best) = o.article_values(c, n).into_iter().next() else { return Vec::new() };
            let mut params = json!({"part_number": e.part_number, "article_number": best.0});
            if let Some(m) = &e.maker {
                params["maker"] = json!(m);
            }
            vec![(format!("Use {} for all", best.0), "article", params)]
        }
        Fix::DropMissingFor(c, n) => {
            let Some(e) = o.entry(Owner::Device(c), n) else { return Vec::new() };
            let kept = if e.subs.is_empty() { serde_json::Value::Null } else { json!(e.subs.join(", ")) };
            vec![(format!("Remove {}", e.missing.join(", ")), "partline", json!({"component": c.hex(), "line": n, "set": {"for": kept}}))]
        }
        Fix::FuseDevices => o
            .strips
            .fuse_devices
            .iter()
            .map(|d| {
                let tag = o.device(*d).map_or_else(|| "?".to_string(), |x| x.tag.clone());
                (format!("Make {tag} a strip position"), "toposition", json!({"device": d.hex()}))
            })
            .collect(),
        Fix::ConnectorNextPin(t) => vec![("Move to next free".to_string(), "pinlabel", json!({"connection_point": t.hex(), "to": "next"}))],
        Fix::SetPins(c, n) => {
            vec![(format!("Set pins to {n}"), "setprop", json!({"component": c.hex(), "key": crate::connectors::PINS_KEY, "value": n.to_string()}))]
        }
        Fix::IgnoreConnector(c, k) => {
            vec![(
                "Ignore".to_string(),
                "setprop",
                json!({"component": c.hex(), "key": format!("{}{}", crate::connectors::IGNORE_PREFIX, k.name()), "value": "yes"}),
            )]
        }
        _ => Vec::new(),
    }
}
