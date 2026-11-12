//! The Checks list (brief 022, design 022 §8): open ends, loose wires, false-looking connections,
//! nets with one connection point, unconnected device points, number and potential conflicts.
//!
//! # Semantics
//! - Every check is derived from the document; nothing is stored.
//! - Checks belong to a **subject**: a net, a conductor, a connection point, a component, or the
//!   project (the unnumbered-nets row). [`Checks`] keeps the checks by subject and is built whole
//!   only on open, reload or first show ([`Checks::build`]); after that [`Checks::update`] re-checks
//!   only the subjects a transaction's ops could change (the update map below). The geometric
//!   checks use a per-page grid of route segments and loose ends ([`GRID_MM`] cells), built with
//!   the list and kept by the same updates.
//!
//! # Update map (design 022 §11)
//! | Op | Re-checked |
//! | --- | --- |
//! | `CreateConductor` / `DeleteConductor` / `AttachEnd` | that conductor; the connection points at its ends before and after; their nets (before and after) and the nets sharing their numbers |
//! | `AddTerminal` / `RemoveTerminal` | that point and its net |
//! | `CreateComponent` / `DeleteComponent` / `SetProperty` component | that component (its contact count against its part's `contacts`, brief 025); its points' nets; continuations with its name (partners) |
//! | `SetProperty` conductor | that conductor's net, and the nets sharing its old and new number |
//! | `AddItem` / `ModifyItem` / `RemoveItem` / representation ops of a route | that conductor; loose ends near its old and new route |
//! | the same of a component symbol | conductors attached to its points ("drawn away"); continuations with its name |
//! | `EmbedDefinition` / `RemoveDefinition` | components using it and their points |
//! | `SetProjectSettings` changing the numbering or potentials (`wiring` scheme, digits, prefix, table) | every net (rare and explicit) |
//! | `SetProjectSettings` changing anything else (label looks, tags, sheet, reference style) | nothing (sheet and reference style: rows are copied again) |
//! | pages, layers, cables, versions | nothing (page texts are worked out when shown) |
//!
//! This module reads the document; it changes nothing.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use ecad_model::connectivity::{CONTINUATION, JUNCTION, NAME, name_key};
use ecad_model::wiring;
use ecad_model::{ComponentId, ConductorId, Document, End, NetData, NetId, ObjectRef, Op, PageId, Pt, Subject, TerminalId};
use serde_json::{Value, json};

use crate::wires::{self, ON_WIRE, kind_of};

/// The grid cell size of the geometric checks, in millimetres.
pub const GRID_MM: f64 = 10.0;
/// Most rows `checks` returns per call by default, and at most.
pub const DEFAULT_LIMIT: usize = 200;
pub const MAX_LIMIT: usize = 2000;

/// How serious a check is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }

    /// The glyph shown before a row.
    pub fn glyph(self) -> &'static str {
        match self {
            Severity::Error => "\u{25cf}",
            Severity::Warning => "\u{25b2}",
            Severity::Info => "\u{b7}",
        }
    }
}

/// What a check found (design 022 §8's table, in its order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    JoinsPotentials,
    SharedNumber,
    TwoFixed,
    OpenEnd,
    LooseWire,
    LooksConnected,
    DrawnAway,
    SinglePoint,
    ThinJunction,
    Unconnected,
    LonelyContinuation,
    MixedPotentials,
    Unnumbered,
    UnknownName,
    /// Brief 024 (design 024 §8.3): a device's own location differs from its box's.
    OwnLocation,
    /// A location box outline crosses itself.
    BoxCrosses,
    /// Two location boxes overlap without nesting.
    BoxesOverlap,
    /// Devices without a part number (one count row).
    NoPartNumber,
    /// Devices without a location (one count row).
    NoLocation,
    /// A device with more contacts of a kind drawn than its part offers (brief 025).
    TooManyContacts,
    /// Brief 028 (design 028 §8): a strip with more terminals drawn than it offers.
    TooManyTerminals,
    /// Two terminals of one strip with one label.
    TerminalTwice,
    /// A cable with more members than cores offered.
    TooManyCores,
    /// Two members of one cable with one core label.
    CoreTwice,
    /// Cables with no core count (one count row).
    NoCoreCount,
}

impl Kind {
    pub const ALL: [Kind; 25] = [
        Kind::JoinsPotentials,
        Kind::SharedNumber,
        Kind::TwoFixed,
        Kind::OpenEnd,
        Kind::LooseWire,
        Kind::LooksConnected,
        Kind::DrawnAway,
        Kind::SinglePoint,
        Kind::ThinJunction,
        Kind::Unconnected,
        Kind::LonelyContinuation,
        Kind::MixedPotentials,
        Kind::Unnumbered,
        Kind::UnknownName,
        Kind::OwnLocation,
        Kind::BoxCrosses,
        Kind::BoxesOverlap,
        Kind::NoPartNumber,
        Kind::NoLocation,
        Kind::TooManyContacts,
        Kind::TooManyTerminals,
        Kind::TerminalTwice,
        Kind::TooManyCores,
        Kind::CoreTwice,
        Kind::NoCoreCount,
    ];

    /// The JSON name.
    pub fn name(self) -> &'static str {
        match self {
            Kind::JoinsPotentials => "joins-potentials",
            Kind::SharedNumber => "shared-number",
            Kind::TwoFixed => "two-fixed",
            Kind::OpenEnd => "open-end",
            Kind::LooseWire => "loose-wire",
            Kind::LooksConnected => "looks-connected",
            Kind::DrawnAway => "drawn-away",
            Kind::SinglePoint => "single-point",
            Kind::ThinJunction => "thin-junction",
            Kind::Unconnected => "unconnected",
            Kind::LonelyContinuation => "lonely-continuation",
            Kind::MixedPotentials => "mixed-potentials",
            Kind::Unnumbered => "unnumbered",
            Kind::UnknownName => "unknown-name",
            Kind::OwnLocation => "own-location",
            Kind::BoxCrosses => "box-crosses",
            Kind::BoxesOverlap => "boxes-overlap",
            Kind::NoPartNumber => "no-part-number",
            Kind::NoLocation => "no-location",
            Kind::TooManyContacts => "too-many-contacts",
            Kind::TooManyTerminals => "too-many-terminals",
            Kind::TerminalTwice => "terminal-twice",
            Kind::TooManyCores => "too-many-cores",
            Kind::CoreTwice => "core-twice",
            Kind::NoCoreCount => "no-core-count",
        }
    }

    /// The kind a JSON name gives.
    pub fn parse(text: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == text.trim())
    }

    /// The words of the Show list.
    pub fn words(self) -> &'static str {
        match self {
            Kind::JoinsPotentials => "Net joins two potentials",
            Kind::SharedNumber => "Two nets share a number",
            Kind::TwoFixed => "One net, two fixed numbers",
            Kind::OpenEnd => "Open end",
            Kind::LooseWire => "Loose wire",
            Kind::LooksConnected => "Looks connected, no junction",
            Kind::DrawnAway => "End drawn away from its connection point",
            Kind::SinglePoint => "Net with a single connection point",
            Kind::ThinJunction => "Junction with fewer than three wires",
            Kind::Unconnected => "Connection point not connected",
            Kind::LonelyContinuation => "Continuation without partner",
            Kind::MixedPotentials => "Mixed potentials in one net",
            Kind::Unnumbered => "Unnumbered nets",
            Kind::UnknownName => "Continuation name not a potential",
            Kind::OwnLocation => "Own location differs from its box",
            Kind::BoxCrosses => "Location box outline crosses itself",
            Kind::BoxesOverlap => "Location boxes overlap without nesting",
            Kind::NoPartNumber => "Devices without a part number",
            Kind::NoLocation => "Devices without a location",
            Kind::TooManyContacts => "More contacts than the part offers",
            Kind::TooManyTerminals => "More terminals than the strip offers",
            Kind::TerminalTwice => "Terminal drawn twice",
            Kind::TooManyCores => "More cores used than the cable offers",
            Kind::CoreTwice => "Core used by two wires",
            Kind::NoCoreCount => "Cables without a core count",
        }
    }

    pub fn severity(self) -> Severity {
        match self {
            Kind::JoinsPotentials | Kind::SharedNumber | Kind::TwoFixed | Kind::TerminalTwice => Severity::Error,
            Kind::OpenEnd
            | Kind::LooseWire
            | Kind::LooksConnected
            | Kind::DrawnAway
            | Kind::SinglePoint
            | Kind::UnknownName
            | Kind::TooManyContacts
            | Kind::TooManyTerminals
            | Kind::TooManyCores
            | Kind::CoreTwice
            | Kind::OwnLocation
            | Kind::BoxCrosses => Severity::Warning,
            _ => Severity::Info,
        }
    }
}

/// What a row points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Object {
    Conductor(ConductorId),
    Terminal(TerminalId),
    Component(ComponentId),
    None,
}

/// What a fix button on a row does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fix {
    /// `junction {at}` on the row's page.
    AddJunction(Pt),
    /// `renumberwires {scope: "unnumbered"}`.
    NumberThem,
    /// Brief 024: clear the device's own location (`setprop … value: null`): Use the box's location.
    UseBoxLocation(ComponentId),
    /// Brief 024: open Reports on the BOM tab with Only ? on.
    ShowReports,
    /// Brief 028: offer the drawn count (`setprop terminals`, PE kept): Offer 14.
    OfferDrawn(ComponentId),
    /// Brief 028: renumber the strip's terminals (`renumber {kind: "terminals"}`).
    RenumberStrip(ComponentId),
    /// Brief 028: focus Properties ▸ Cores on the cable.
    SetCores(ecad_model::CableId),
    /// Brief 028: give the member the next free core (`cablecore`).
    NextFreeCore(ConductorId),
    /// Brief 028: open Reports on the Cables tab with Only ? on.
    ShowCables,
}

/// One row of the Checks list.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub kind: Kind,
    pub page: Option<PageId>,
    pub at: Option<Pt>,
    pub object: Object,
    /// Which net, wire or point, in words (`24V-01`, `-W01 3`, `-M01:PE`).
    pub which: String,
    pub fix: Option<Fix>,
}

impl Check {
    pub fn severity(&self) -> Severity {
        self.kind.severity()
    }

    /// The row's text: the kind's words, or for the unnumbered row its count.
    pub fn what(&self) -> String {
        self.kind.words().to_string()
    }

    /// Where the row is, as references name a place (`3-D-5`, or `/3.D` in that style); the page
    /// number alone when it has no point; empty when it has no page.
    pub fn locator(&self, doc: &Document) -> String {
        let Some(page) = self.page else { return String::new() };
        match self.at.and_then(|a| ecad_model::wiring::place(doc, page, a)) {
            Some(p) => p.text(doc.settings.reference_style),
            None => ecad_model::book::page_number(doc, page),
        }
    }

    /// Book order: severity, then page, then position (top to bottom, left to right).
    pub fn order_key(&self, order: &HashMap<PageId, usize>) -> (Severity, usize, i64, i64, Kind, Object) {
        let page = self.page.and_then(|p| order.get(&p).copied()).unwrap_or(usize::MAX);
        let (y, x) = self.at.map_or((0, 0), |a| ((-a.y * 1000.0) as i64, (a.x * 1000.0) as i64));
        (self.severity(), page, y, x, self.kind, self.object)
    }

    pub fn to_json(&self, doc: &Document) -> Value {
        let object = match self.object {
            Object::Conductor(c) => json!({"conductor": c.hex()}),
            Object::Terminal(t) => json!({"terminal": t.hex()}),
            Object::Component(c) => json!({"component": c.hex()}),
            Object::None => Value::Null,
        };
        let place = self.page.zip(self.at).and_then(|(p, a)| ecad_model::wiring::place(doc, p, a));
        json!({
            "kind": self.kind.name(),
            "severity": self.severity().name(),
            "page": self.page.map(|p| ecad_model::book::page_number(doc, p)),
            "column": place.as_ref().map(|p| p.column_label.clone()),
            "row": place.as_ref().map(|p| p.row_label.clone()),
            "at": self.at.map(|a| [a.x, a.y]),
            "object": object,
            "text": self.what(),
            "which": self.which,
            "where": self.locator(doc),
        })
    }
}

/// What a check belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    Net(NetId),
    Conductor(ConductorId),
    Terminal(TerminalId),
    Component(ComponentId),
    Project,
}

type Cell = (PageId, i64, i64);

fn cell_of(page: PageId, p: Pt) -> Option<Cell> {
    if !p.is_finite() {
        return None;
    }
    Some((page, (p.x / GRID_MM).floor() as i64, (p.y / GRID_MM).floor() as i64))
}

/// The cells a segment's box (grown by [`ON_WIRE`]) covers; capped, so a hostile huge segment
/// cannot cost more than a page.
fn cells_of_segment(page: PageId, a: Pt, b: Pt) -> Vec<Cell> {
    let (Some(lo), Some(hi)) = (
        cell_of(page, Pt::new(a.x.min(b.x) - ON_WIRE, a.y.min(b.y) - ON_WIRE)),
        cell_of(page, Pt::new(a.x.max(b.x) + ON_WIRE, a.y.max(b.y) + ON_WIRE)),
    ) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for x in lo.1..=hi.1 {
        for y in lo.2..=hi.2 {
            out.push((page, x, y));
            if out.len() > 10_000 {
                return out;
            }
        }
    }
    out
}

/// The per-page grid of route segments and loose ends.
#[derive(Debug, Clone, Default)]
pub struct Grid {
    segments: HashMap<Cell, BTreeSet<(ConductorId, usize)>>,
    ends: HashMap<Cell, BTreeSet<(ConductorId, End)>>,
    /// What each conductor put in the grid, to take it out again.
    placed: HashMap<ConductorId, Placed>,
}

#[derive(Debug, Clone, Default)]
struct Placed {
    /// (page, segment index, a, b)
    segments: Vec<(PageId, usize, Pt, Pt)>,
    /// (page, end, point) of loose ends
    ends: Vec<(PageId, End, Pt)>,
    cells: BTreeSet<Cell>,
}

/// The route vertices of `c` on each page it is drawn on.
fn routes_of(doc: &Document, c: ConductorId) -> Vec<(PageId, Vec<Pt>)> {
    let mut out = Vec::new();
    for (_, r) in doc.representations_of(Subject::Conductor(c)) {
        if let Some((_, _, v)) = wires::route_vertices(doc, c, r.page)
            && !out.iter().any(|(p, _)| *p == r.page)
        {
            out.push((r.page, v));
        }
    }
    out
}

impl Grid {
    fn remove(&mut self, c: ConductorId) {
        let Some(p) = self.placed.remove(&c) else { return };
        for (page, i, a, b) in &p.segments {
            for cell in cells_of_segment(*page, *a, *b) {
                if let Some(s) = self.segments.get_mut(&cell) {
                    s.remove(&(c, *i));
                    if s.is_empty() {
                        self.segments.remove(&cell);
                    }
                }
            }
        }
        for (page, end, at) in &p.ends {
            if let Some(cell) = cell_of(*page, *at)
                && let Some(s) = self.ends.get_mut(&cell)
            {
                s.remove(&(c, *end));
                if s.is_empty() {
                    self.ends.remove(&cell);
                }
            }
        }
    }

    fn insert(&mut self, doc: &Document, c: ConductorId) {
        self.remove(c);
        let Some(cond) = doc.conductors.get(&c) else { return };
        let mut placed = Placed::default();
        for (page, v) in routes_of(doc, c) {
            for (i, w) in v.windows(2).enumerate() {
                let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
                for cell in cells_of_segment(page, *a, *b) {
                    self.segments.entry(cell).or_default().insert((c, i));
                    placed.cells.insert(cell);
                }
                placed.segments.push((page, i, *a, *b));
            }
            for (end, at) in [(End::A, v.first().copied()), (End::B, v.last().copied())] {
                let Some(at) = at else { continue };
                if cond.end(end).is_none()
                    && let Some(cell) = cell_of(page, at)
                {
                    self.ends.entry(cell).or_default().insert((c, end));
                    placed.cells.insert(cell);
                    placed.ends.push((page, end, at));
                }
            }
        }
        self.placed.insert(c, placed);
    }

    /// The cells `c` occupies now.
    fn cells(&self, c: ConductorId) -> BTreeSet<Cell> {
        self.placed.get(&c).map(|p| p.cells.clone()).unwrap_or_default()
    }

    /// Conductors with a loose end in any of `cells` (and the 8 cells round each).
    fn loose_near(&self, cells: &BTreeSet<Cell>) -> BTreeSet<ConductorId> {
        let mut out = BTreeSet::new();
        for (page, x, y) in cells {
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if let Some(s) = self.ends.get(&(*page, x + dx, y + dy)) {
                        out.extend(s.iter().map(|(c, _)| *c));
                    }
                }
            }
        }
        out
    }

    /// Whether `at` on `page` lies on the interior of another conductor's route, or on another
    /// loose end: the conductor it lies on.
    fn under(&self, page: PageId, at: Pt, me: ConductorId) -> Option<(ConductorId, bool)> {
        let cell = cell_of(page, at)?;
        for (c, i) in self.segments.get(&cell).into_iter().flatten() {
            if *c == me {
                continue;
            }
            let Some(p) = self.placed.get(c) else { continue };
            let Some((_, _, a, b)) = p.segments.iter().find(|(pg, j, _, _)| *pg == page && j == i) else { continue };
            if near_segment(at, *a, *b) && !at.near(*a, ON_WIRE) && !at.near(*b, ON_WIRE) {
                return Some((*c, false));
            }
        }
        for (c, end) in self.ends.get(&cell).into_iter().flatten() {
            if *c == me {
                continue;
            }
            let Some(p) = self.placed.get(c) else { continue };
            if p.ends.iter().any(|(pg, e, x)| *pg == page && e == end && x.near(at, ON_WIRE)) {
                return Some((*c, true));
            }
        }
        None
    }
}

fn near_segment(p: Pt, a: Pt, b: Pt) -> bool {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let q = Pt::new(a.x + t * dx, a.y + t * dy);
    q.is_finite() && q.near(p, ON_WIRE)
}

/// Words for a net: its number, or its first device point.
fn net_words(doc: &Document, net: &NetData) -> String {
    wiring::net_number(doc, net).unwrap_or_else(|| wires::net_points(doc, net).first().cloned().unwrap_or_default())
}

/// Where a conductor is: its first page in book order and the midpoint of its longest segment
/// there, or a given end.
fn conductor_place(doc: &Document, c: ConductorId, end: Option<End>) -> (Option<PageId>, Option<Pt>) {
    let order: HashMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    let routes = routes_of(doc, c);
    let Some((page, v)) = routes.iter().min_by_key(|(p, _)| order.get(p).copied().unwrap_or(usize::MAX)) else {
        return (None, None);
    };
    let at = match end {
        Some(End::A) => v.first().copied(),
        Some(End::B) => v.last().copied(),
        None => wires::longest_segment(v).map(|(a, b)| Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)),
    };
    (Some(*page), at)
}

/// Where a connection point is shown first (book order).
fn terminal_place(doc: &Document, t: TerminalId) -> (Option<PageId>, Option<Pt>) {
    let Some(c) = doc.terminals.get(&t).map(|x| x.component) else { return (None, None) };
    for page in &doc.page_order {
        let shown = doc.representations_of(Subject::Component(c)).any(|(_, r)| r.page == *page);
        if shown && let Some(at) = ecad_model::derive::terminal_position(doc, *page, t) {
            return (Some(*page), Some(at));
        }
    }
    (None, None)
}

/// A conductor's words: its cable and core, or its net's number.
fn conductor_words(doc: &Document, c: ConductorId) -> String {
    if let Some(k) = doc.cables_of(c).first() {
        let core = doc.cables.get(k).and_then(|x| x.members.iter().find(|m| m.conductor == c)).and_then(|m| m.core.clone()).unwrap_or_default();
        return format!("{} {core}", crate::references::cable_words(doc, *k)).trim().to_string();
    }
    let conn = doc.connectivity();
    conn.net_of_conductor(c).and_then(|id| conn.net(id)).map(|n| net_words(doc, n)).unwrap_or_default()
}

/// The net checks of net `id`.
pub fn net_checks(doc: &Document, id: NetId, net: &NetData) -> Vec<Check> {
    let mut out = Vec::new();
    let first = net.first_conductor();
    let (page, at) = first.map_or((None, None), |c| conductor_place(doc, c, None));
    let object = first.map_or(Object::None, Object::Conductor);
    let joined = wiring::joined_potentials(doc, net);
    if !joined.is_empty() {
        // Point at a continuation in it, when it has one.
        let cont = net.terminals.iter().find(|t| doc.terminals.get(t).is_some_and(|x| kind_of(doc, x.component) == Some(CONTINUATION)));
        let (page, at, object) = match cont {
            Some(t) => {
                let (p, a) = terminal_place(doc, *t);
                (p, a, doc.terminals.get(t).map_or(Object::None, |x| Object::Component(x.component)))
            }
            None => (page, at, object),
        };
        out.push(Check { kind: Kind::JoinsPotentials, page, at, object, which: joined.join(" and "), fix: None });
    }
    if net.fixed.len() > 1 {
        out.push(Check { kind: Kind::TwoFixed, page, at, object, which: net.fixed.keys().cloned().collect::<Vec<_>>().join(", "), fix: None });
    }
    if let Some(number) = wiring::net_number(doc, net)
        && wiring::sharing(doc, &number).contains(&id)
    {
        out.push(Check { kind: Kind::SharedNumber, page, at, object, which: number, fix: None });
    }
    // A net with conductors, one connection point and no continuation.
    if !net.conductors.is_empty() && net.terminals.len() == 1 && net.names.is_empty() {
        let t = net.terminals.first().copied();
        let jc = t.and_then(|t| doc.terminals.get(&t)).map(|x| kind_of(doc, x.component));
        if !matches!(jc, Some(Some(JUNCTION | CONTINUATION)))
            && let Some(t) = t
        {
            let (p, a) = terminal_place(doc, t);
            out.push(Check { kind: Kind::SinglePoint, page: p, at: a, object: Object::Terminal(t), which: wires::point_words(doc, t), fix: None });
        }
    }
    if net.potentials.len() > 1 {
        let main = wiring::net_potential(doc, net).name;
        let odd = net
            .conductors
            .iter()
            .find(|c| doc.conductors.get(c).and_then(|x| x.properties.get(wiring::POTENTIAL_KEY)).is_some_and(|p| Some(p) != main.as_ref()));
        let (p, a) = odd.map_or((page, at), |c| conductor_place(doc, *c, None));
        out.push(Check {
            kind: Kind::MixedPotentials,
            page: p,
            at: a,
            object: odd.map_or(object, |c| Object::Conductor(*c)),
            which: net.potentials.keys().cloned().collect::<Vec<_>>().join(", "),
            fix: None,
        });
    }
    // A continuation name that is neither in the potentials table nor the net's own number (a
    // pair placed by Merge the nets): a warning, never a refusal (owner, 2026-11-09). Only names
    // in the table give a net its potential (design 022 §6 rule 2).
    for key in &net.names {
        let numbered = net.numbers.keys().any(|n| name_key(n) == *key);
        if doc.settings.wiring.potential(key).is_some() || numbered {
            continue;
        }
        let cont = net.terminals.iter().find_map(|t| {
            let x = doc.terminals.get(t)?;
            let c = doc.components.get(&x.component)?;
            (ecad_model::connectivity::continuation_key(&c.properties).as_ref() == Some(key)).then_some((*t, x.component))
        });
        let (p, a, o, which) = match cont {
            Some((t, c)) => {
                let (p, a) = terminal_place(doc, t);
                let name = doc.components.get(&c).and_then(|x| x.properties.get(NAME)).cloned();
                (p, a, Object::Component(c), name.unwrap_or_else(|| key.clone()))
            }
            None => (page, at, object, key.clone()),
        };
        out.push(Check { kind: Kind::UnknownName, page: p, at: a, object: o, which, fix: None });
    }
    out
}

/// The checks of conductor `c`: open end, loose wire, looks connected, drawn away.
pub fn conductor_checks(doc: &Document, c: ConductorId, grid: &Grid, tol: f64) -> Vec<Check> {
    let mut out = Vec::new();
    let Some(cond) = doc.conductors.get(&c) else { return out };
    let which = conductor_words(doc, c);
    let loose: Vec<End> = End::BOTH.into_iter().filter(|e| cond.end(*e).is_none()).collect();
    match loose.len() {
        2 => {
            let (page, at) = conductor_place(doc, c, None);
            out.push(Check { kind: Kind::LooseWire, page, at, object: Object::Conductor(c), which: which.clone(), fix: None });
        }
        1 => {
            let (page, at) = conductor_place(doc, c, loose.first().copied());
            out.push(Check { kind: Kind::OpenEnd, page, at, object: Object::Conductor(c), which: which.clone(), fix: None });
        }
        _ => {}
    }
    // Looks connected: a loose end on another wire's interior, or on another loose end (reported
    // by the lower conductor ID only, so the pair is one row).
    for (page, v) in routes_of(doc, c) {
        for end in &loose {
            let at = if *end == End::A { v.first() } else { v.last() };
            let Some(at) = at.copied() else { continue };
            if let Some((other, is_end)) = grid.under(page, at, c) {
                if is_end && other < c {
                    continue;
                }
                out.push(Check {
                    kind: Kind::LooksConnected,
                    page: Some(page),
                    at: Some(at),
                    object: Object::Conductor(c),
                    which: which.clone(),
                    fix: Some(Fix::AddJunction(at)),
                });
            }
        }
        // Drawn away: an attached end whose route end is not at its point on this page.
        for (end, at) in [(End::A, v.first()), (End::B, v.last())] {
            let (Some(t), Some(at)) = (cond.end(end), at) else { continue };
            if let Some(tp) = ecad_model::derive::terminal_position(doc, page, t)
                && !tp.near(*at, tol.max(1e-6))
            {
                out.push(Check {
                    kind: Kind::DrawnAway,
                    page: Some(page),
                    at: Some(*at),
                    object: Object::Conductor(c),
                    which: which.clone(),
                    fix: None,
                });
            }
        }
    }
    out
}

/// The checks of connection point `t`: a device point with nothing attached.
pub fn terminal_checks(doc: &Document, t: TerminalId) -> Vec<Check> {
    let Some(term) = doc.terminals.get(&t) else { return Vec::new() };
    if matches!(kind_of(doc, term.component), Some(JUNCTION | CONTINUATION)) || doc.has_ends(t) {
        return Vec::new();
    }
    // Bridged inside its device to an attached point: connected.
    let conn = doc.connectivity();
    if conn.net_of_terminal(t).and_then(|id| conn.net(id)).is_some_and(|n| !n.conductors.is_empty()) {
        return Vec::new();
    }
    let (page, at) = terminal_place(doc, t);
    vec![Check { kind: Kind::Unconnected, page, at, object: Object::Terminal(t), which: wires::point_words(doc, t), fix: None }]
}

/// The checks of component `c`: a junction with fewer than three wires, a continuation with no
/// partner.
pub fn component_checks(doc: &Document, c: ComponentId) -> Vec<Check> {
    let mut out = Vec::new();
    let place = || doc.terminals_of(c).next().map_or((None, None), |(t, _)| terminal_place(doc, t));
    match kind_of(doc, c) {
        Some(JUNCTION) => {
            let wires_here: usize = doc.terminals_of(c).map(|(t, _)| doc.ends_at(t).len()).sum();
            if wires_here < 3 {
                let (page, at) = place();
                out.push(Check { kind: Kind::ThinJunction, page, at, object: Object::Component(c), which: format!("{wires_here} wires"), fix: None });
            }
        }
        Some(CONTINUATION) => {
            let name = doc.components.get(&c).and_then(|x| x.properties.get(NAME)).cloned().unwrap_or_default();
            let partners = doc.continuations_named(&name_key(&name)).len();
            if partners < 2 {
                let (page, at) = place();
                out.push(Check { kind: Kind::LonelyContinuation, page, at, object: Object::Component(c), which: name, fix: None });
            }
        }
        _ => {}
    }
    // Brief 025 (design 025 §4.3): drawn beyond the part's offer, ringed at the first such pole.
    for (which, page, at) in crate::xref::count_checks(doc, c) {
        out.push(Check { kind: Kind::TooManyContacts, page, at, object: Object::Component(c), which, fix: None });
    }
    out
}

/// The kept Checks list (see the module docs).
#[derive(Debug, Clone, Default)]
pub struct Checks {
    by_key: BTreeMap<Key, Vec<Check>>,
    grid: Grid,
    /// Nets with conductors and no number (for the one Unnumbered row).
    unnumbered: BTreeSet<NetId>,
    /// Bumped on every change, for views that copy the rows.
    pub revision: u64,
}

impl Checks {
    /// The whole list, computed from scratch (open, reload, first show).
    pub fn build(doc: &Document, tol: f64) -> Self {
        let mut ch = Checks::default();
        for c in doc.conductors.keys() {
            ch.grid.insert(doc, *c);
        }
        let conn = doc.connectivity();
        for (id, net) in conn.nets() {
            ch.set(Key::Net(id), net_checks(doc, id, net));
            if !net.conductors.is_empty() && net.numbers.is_empty() {
                ch.unnumbered.insert(id);
            }
        }
        for c in doc.conductors.keys() {
            let v = conductor_checks(doc, *c, &ch.grid, tol);
            ch.set(Key::Conductor(*c), v);
        }
        for t in doc.terminals.keys() {
            ch.set(Key::Terminal(*t), terminal_checks(doc, *t));
        }
        for c in doc.components.keys() {
            ch.set(Key::Component(*c), component_checks(doc, *c));
        }
        ch.set_unnumbered();
        ch
    }

    fn set(&mut self, key: Key, v: Vec<Check>) {
        if v.is_empty() {
            self.by_key.remove(&key);
        } else {
            self.by_key.insert(key, v);
        }
    }

    fn set_unnumbered(&mut self) {
        let n = self.unnumbered.len();
        let row = (n > 0).then(|| Check {
            kind: Kind::Unnumbered,
            page: None,
            at: None,
            object: Object::None,
            which: if n == 1 { "1 net has no number".into() } else { format!("{n} nets have no number") },
            fix: Some(Fix::NumberThem),
        });
        self.set(Key::Project, row.into_iter().collect());
    }

    /// Every row, in no particular order.
    pub fn rows(&self) -> impl Iterator<Item = &Check> {
        self.by_key.values().flatten()
    }

    /// How many rows.
    pub fn len(&self) -> usize {
        self.by_key.values().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }

    /// The rows sorted in book order (design 022 §8).
    pub fn sorted(&self, doc: &Document) -> Vec<Check> {
        sort(doc, self.rows().cloned().collect())
    }

    /// What `ops` (about to be committed on `before`) could change, worked out on the document
    /// before them: the subjects to re-check after them. Pair with [`Checks::update`].
    pub fn scope(&self, before: &Document, ops: &[Op]) -> Scope {
        let mut s = Scope::default();
        s.collect(before, ops, &self.grid);
        s
    }

    /// Re-check the subjects in `scope` (from [`Checks::scope`]) on `doc`, the state after the ops,
    /// adding what the ops themselves name in this state.
    pub fn update(&mut self, doc: &Document, ops: &[Op], mut scope: Scope, tol: f64) {
        scope.collect(doc, ops, &self.grid);
        if scope.all {
            *self = Checks { revision: self.revision + 1, ..Checks::build(doc, tol) };
            return;
        }
        // Geometry first: the grid must hold the new routes before any conductor is re-checked.
        for c in &scope.routes {
            let old = self.grid.cells(*c);
            self.grid.insert(doc, *c);
            let new = self.grid.cells(*c);
            let near: BTreeSet<Cell> = old.union(&new).copied().collect();
            scope.conductors.extend(self.grid.loose_near(&near));
        }
        let conn = doc.connectivity();
        // Nets: those named, and those sharing their numbers.
        let mut nets: BTreeSet<NetId> = BTreeSet::new();
        let mut numbers = std::mem::take(&mut scope.numbers);
        for n in &scope.nodes {
            if let Some(id) = conn.net_of(n) {
                nets.insert(id);
            }
        }
        for id in &nets {
            if let Some(net) = conn.net(*id) {
                numbers.extend(net.numbers.keys().cloned());
            }
        }
        // An unnumbered potential's shared number (`PE`) is never a shared-number error, so the
        // other nets carrying it have nothing to re-check (otherwise every PE edit would re-check
        // every PE net in the project).
        let exempt = |n: &str| doc.settings.wiring.potentials.iter().any(|p| !p.numbered && p.prefix == n);
        for n in numbers.iter().filter(|n| !exempt(n)) {
            nets.extend(conn.nets_numbered(n).map(|(id, _)| id));
        }
        // Nets that no longer exist lose their rows; those that do are checked again.
        for id in scope.old_nets.iter().chain(nets.iter()) {
            match conn.net(*id) {
                Some(net) => {
                    self.set(Key::Net(*id), net_checks(doc, *id, net));
                    if !net.conductors.is_empty() && net.numbers.is_empty() {
                        self.unnumbered.insert(*id);
                    } else {
                        self.unnumbered.remove(id);
                    }
                }
                None => {
                    self.by_key.remove(&Key::Net(*id));
                    self.unnumbered.remove(id);
                }
            }
        }
        for c in &scope.conductors {
            let v = conductor_checks(doc, *c, &self.grid, tol);
            self.set(Key::Conductor(*c), v);
        }
        for t in &scope.terminals {
            self.set(Key::Terminal(*t), terminal_checks(doc, *t));
        }
        for c in &scope.components {
            self.set(Key::Component(*c), component_checks(doc, *c));
        }
        self.set_unnumbered();
        self.revision += 1;
    }
}

/// The subjects a transaction could change (see the module's update map).
#[derive(Debug, Clone, Default)]
pub struct Scope {
    all: bool,
    /// Only where rows are (page numbers, folders) changed: the revision moves, so copies of the
    /// rows are taken again.
    relocate: bool,
    nodes: BTreeSet<ecad_model::Node>,
    old_nets: BTreeSet<NetId>,
    numbers: BTreeSet<String>,
    conductors: BTreeSet<ConductorId>,
    routes: BTreeSet<ConductorId>,
    terminals: BTreeSet<TerminalId>,
    components: BTreeSet<ComponentId>,
}

impl Scope {
    /// Add what `ops` name, looked up in `doc` (called on the state before and after).
    fn collect(&mut self, doc: &Document, ops: &[Op], _grid: &Grid) {
        use ecad_model::Node;
        let conn = doc.connectivity();
        let node = |s: &mut Scope, n: Node| {
            if let Some(id) = conn.net_of(&n) {
                s.old_nets.insert(id);
                if let Some(net) = conn.net(id) {
                    s.numbers.extend(net.numbers.keys().cloned());
                }
            }
            s.nodes.insert(n);
        };
        for op in ops {
            match op {
                Op::CreateConductor { id, conductor } | Op::DeleteConductor { id, conductor } => {
                    self.conductors.insert(*id);
                    self.routes.insert(*id);
                    node(self, Node::Conductor(*id));
                    for t in conductor.ends.iter().flatten() {
                        self.terminals.insert(*t);
                        node(self, Node::Terminal(*t));
                        self.component_of(doc, *t);
                    }
                }
                Op::AttachEnd { conductor, before, after, .. } => {
                    self.conductors.insert(*conductor);
                    self.routes.insert(*conductor);
                    node(self, Node::Conductor(*conductor));
                    for t in [before, after].into_iter().flatten() {
                        self.terminals.insert(*t);
                        node(self, Node::Terminal(*t));
                        self.component_of(doc, *t);
                    }
                }
                Op::AddTerminal { id, terminal } | Op::RemoveTerminal { id, terminal } => {
                    self.terminals.insert(*id);
                    self.components.insert(terminal.component);
                    node(self, Node::Terminal(*id));
                }
                Op::CreateComponent { id, component } | Op::DeleteComponent { id, component } => {
                    self.component(doc, *id, component.properties.get(NAME).map(String::as_str));
                    for (t, _) in doc.terminals_of(*id) {
                        node(self, Node::Terminal(t));
                    }
                }
                Op::SetProperty { object: ObjectRef::Component(c), before, after, .. } => {
                    for n in [before, after].into_iter().flatten() {
                        self.component(doc, *c, Some(n));
                        node(self, Node::Name(name_key(n)));
                    }
                    self.component(doc, *c, None);
                    for (t, _) in doc.terminals_of(*c) {
                        node(self, Node::Terminal(t));
                    }
                }
                Op::SetProperty { object: ObjectRef::Conductor(c), before, after, .. } => {
                    node(self, Node::Conductor(*c));
                    self.conductors.insert(*c);
                    self.numbers.extend(before.iter().chain(after.iter()).cloned());
                }
                Op::SetProperty { object: ObjectRef::Cable(k), .. }
                | Op::CreateCable { id: k, .. }
                | Op::DeleteCable { id: k, .. }
                | Op::AddCableMember { cable: k, .. }
                | Op::RemoveCableMember { cable: k, .. } => {
                    // A cable's tag and cores are part of its members' words.
                    if let Some(cable) = doc.cables.get(k) {
                        self.conductors.extend(cable.members.iter().map(|m| m.conductor));
                    }
                    if let Op::AddCableMember { member, .. } | Op::RemoveCableMember { member, .. } = op {
                        self.conductors.insert(member.conductor);
                    }
                }
                Op::AddItem { id, .. } | Op::RemoveItem { id, .. } => self.item(doc, *id),
                Op::ModifyItem { id, .. } => self.item(doc, *id),
                Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => self.rep(doc, rep),
                Op::ModifyRepresentation { before, after, .. } => {
                    self.rep(doc, before);
                    self.rep(doc, after);
                }
                Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => {
                    for c in doc.components_using(*id) {
                        self.components.insert(c);
                        for (t, _) in doc.terminals_of(c) {
                            self.terminals.insert(t);
                            node(self, Node::Terminal(t));
                        }
                    }
                }
                // Only the numbering and potential settings change what checks find; sheet, page
                // numbering and reference style change where rows say they are; label looks
                // (colour style, height), tag settings and units change nothing here.
                Op::SetProjectSettings { before, after } => {
                    let (b, a) = (&before.wiring, &after.wiring);
                    if b.scheme != a.scheme || b.digits != a.digits || b.unclassified_prefix != a.unclassified_prefix || b.potentials != a.potentials
                    {
                        self.all = true;
                    } else if before.sheet != after.sheet
                        || before.page_number_step != after.page_number_step
                        || before.reference_style != after.reference_style
                    {
                        self.relocate = true;
                    }
                }
                // Page numbers, folders and the page order change where rows say they are (brief
                // 019), not what they find: the rows are copied again.
                Op::SetProperty { object: ObjectRef::Page(_) | ObjectRef::Project, .. }
                | Op::AddFolder { .. }
                | Op::RemoveFolder { .. }
                | Op::ModifyFolder { .. }
                | Op::AddPage { .. }
                | Op::RemovePage { .. } => self.relocate = true,
                Op::AddLayer { .. } | Op::RemoveLayer { .. } | Op::NameVersion { .. } | Op::UnnameVersion { .. } => {}
            }
        }
    }

    fn component_of(&mut self, doc: &Document, t: TerminalId) {
        if let Some(c) = doc.terminals.get(&t).map(|x| x.component) {
            self.components.insert(c);
        }
    }

    /// A component and, for a continuation name, every continuation with that name.
    fn component(&mut self, doc: &Document, c: ComponentId, name: Option<&str>) {
        self.components.insert(c);
        let name = name.map(str::to_string).or_else(|| doc.components.get(&c).and_then(|x| x.properties.get(NAME).cloned()));
        if let Some(n) = name {
            self.components.extend(doc.continuations_named(&name_key(&n)));
        }
        for (t, _) in doc.terminals_of(c) {
            self.terminals.insert(t);
        }
    }

    /// An item's representation (a new item's representation op names it instead).
    fn item(&mut self, doc: &Document, id: ecad_model::ItemId) {
        if let Some((_, r)) = doc.representation_of_item(id) {
            let r = r.clone();
            self.rep(doc, &r);
        }
    }

    fn rep(&mut self, doc: &Document, r: &ecad_model::Representation) {
        match r.subject {
            Subject::Conductor(c) => {
                self.conductors.insert(c);
                self.routes.insert(c);
            }
            Subject::Component(c) => {
                // Its points moved: the wires attached to them may now be drawn away; a
                // continuation's place moves its partners' rows.
                self.component(doc, c, None);
                for (t, _) in doc.terminals_of(c) {
                    self.conductors.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
                }
            }
            Subject::Cable(_) => {}
        }
    }
}

/// The drawn handles of `object` on `page` (for a go-to's selection): a conductor's route, a
/// component's symbols, a connection point's symbol.
pub fn handles_of(p: &crate::Project, page: PageId, object: Object) -> Vec<cadcraft_engine::doc::Handle> {
    let Some(map) = p.pages.get(&page).map(|v| &v.map) else { return Vec::new() };
    let doc = p.ledger.document();
    let subject = match object {
        Object::Conductor(c) => Subject::Conductor(c),
        Object::Component(c) => Subject::Component(c),
        Object::Terminal(t) => match doc.terminals.get(&t) {
            Some(x) => Subject::Component(x.component),
            None => return Vec::new(),
        },
        Object::None => return Vec::new(),
    };
    doc.representations_of(subject).filter(|(_, r)| r.page == page).flat_map(|(_, r)| r.items.iter().filter_map(|i| map.item(*i))).collect()
}

/// `rows` in book order.
pub fn sort(doc: &Document, mut rows: Vec<Check>) -> Vec<Check> {
    let order: HashMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    rows.sort_by_key(|c| c.order_key(&order));
    rows
}

/// Every check of the project, worked out from scratch (the `checks` command reads on request).
pub fn all(doc: &Document, tol: f64) -> Vec<Check> {
    all_with(doc, tol, &crate::outputs::Outputs::build(doc))
}

/// [`all`] with the report models already kept (brief 024: the project's, reused by `checks`).
pub fn all_with(doc: &Document, tol: f64, outputs: &crate::outputs::Outputs) -> Vec<Check> {
    let mut rows: Vec<Check> = Checks::build(doc, tol).rows().cloned().collect();
    rows.extend(outputs.checks());
    sort(doc, rows)
}
