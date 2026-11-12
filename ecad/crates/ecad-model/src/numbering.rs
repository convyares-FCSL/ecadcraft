//! Numbering (brief 026, design 026 §2): one planner for device tags, cable tags and wire numbers.
//!
//! # Semantics
//! - [`plan`] reads a document and a [`Request`] and returns a [`Plan`]: one row per item in scope
//!   (old → new, or why it is kept), and the ops that write it. Nothing is written here; Renumber,
//!   Finalise, Swap's next free number and the merge's renumber choices commit the ops as one
//!   transaction.
//! - **Drawing order** (§2.2): page in book order, then the column and row of that page's frame
//!   grid, then top to bottom and left to right, then ID. Each item has one anchor: a device's
//!   `main` representation (else its first in drawing order), a cable's first representation, a
//!   net's first conductor's route start. An item with no anchor sorts last.
//! - **Numbers** (§2.3): per tag prefix (devices and cables share one namespace, design §6) or per
//!   wire-number scope, `start`, `start + 1`, … in drawing order, skipping every number taken by an
//!   item not being renumbered (fixed, out of scope, the kind not chosen). A plan never produces a
//!   duplicate: the duplicate rule holds on the transaction's end state (FP6), so exchanges such as
//!   `-K01 ↔ -K02` are plain `SetProperty` ops.
//! - **High-water reset** (FP4): last, one `SetProperty(Project, "numbering.high.<P>")` per prefix
//!   whose tags changed, set to the highest number of `P` in use after the plan.
//! - **Terminals** (brief 028, design 028 §9.5): the caller names the strips in scope
//!   ([`Request::strips`]; this crate does not know the starter catalogue). Per strip, its drawn
//!   terminals are labelled `start`, `start + 1`, … in drawing order, feed-through and PE in one
//!   sequence; a strip's fixed tag does not stop it. A changed terminal is relabelled by
//!   `RemoveTerminal` + `AddTerminal` of the same ID with the same key; its wires stay attached
//!   (the transaction's end state holds every invariant, decision FP5 of 2026-11-12).
//! - The plan is built on request only (a preview, an Apply); it reads each item once, sorts, and
//!   writes one op per changed item (§11).
//!
//! This module holds no UI and commits nothing; words for people stay short (design §10).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::book::Book;
use crate::connectivity::{self, NetData, NetId, split_number};
use crate::doc::{Document, ItemKind, Primitive, Representation, Subject};
use crate::geom::Pt;
use crate::id::{CableId, ComponentId, ConductorId, FolderId, PageId, TerminalId};
use crate::op::{ObjectRef, Op};
use crate::sheet::Grid;
use crate::tag::{self, FIXED_KEY, HIGH_RESET_PREFIX, HighWater, TAG_KEY};
use crate::wiring;

/// The kind of item a row numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Devices,
    Cables,
    Wires,
    /// Brief 028: terminal labels per strip.
    Terminals,
}

impl Kind {
    /// The short word (design §10): `Dev`, `Cbl`, `Wire`.
    pub fn short(self) -> &'static str {
        match self {
            Kind::Devices => "Dev",
            Kind::Cables => "Cbl",
            Kind::Wires => "Wire",
            Kind::Terminals => "Trm",
        }
    }

    /// The JSON word: `devices`, `cables`, `wires`.
    pub fn word(self) -> &'static str {
        match self {
            Kind::Devices => "devices",
            Kind::Cables => "cables",
            Kind::Wires => "wires",
            Kind::Terminals => "terminals",
        }
    }
}

/// Which items a run numbers (design §2.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// Every tagged item in the usual form (provisional included); every net with an automatic
    /// number or none.
    All,
    /// These devices, cables and the nets of these conductors.
    Selection { components: BTreeSet<ComponentId>, cables: BTreeSet<CableId>, conductors: BTreeSet<ConductorId> },
    /// Tags with these letters (`K`); nets of this potential (`24V`).
    Prefix(String),
    /// Items anchored on a page of this folder or its subfolders.
    Folder(FolderId),
    /// Items anchored on these pages.
    Pages(BTreeSet<PageId>),
    /// Provisional tags; nets with no number.
    Provisional,
}

/// What to number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub kinds: BTreeSet<Kind>,
    pub scope: Scope,
    /// The first number (default 1; for [`Scope::Provisional`] the next free per prefix).
    pub start: Option<u64>,
    /// Digits for this run (default: the project's tag padding, or the wire digits).
    pub digits: Option<u8>,
    /// Brief 028: the strips whose terminals [`Kind::Terminals`] numbers, chosen by the caller.
    pub strips: BTreeSet<ComponentId>,
}

/// What a row does (design §3, row marks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mark {
    /// `→`: the tag or number changes.
    Change,
    /// `F`: fixed, kept.
    Fixed,
    /// `=`: unchanged.
    Same,
    /// `–`: left alone, with the reason.
    Skipped(&'static str),
}

impl Mark {
    /// The mark as shown (`→`, `F`, `=`, `–`).
    pub fn glyph(&self) -> &'static str {
        match self {
            Mark::Change => "→",
            Mark::Fixed => "F",
            Mark::Same => "=",
            Mark::Skipped(_) => "–",
        }
    }

    /// The JSON word.
    pub fn word(&self) -> &'static str {
        match self {
            Mark::Change => "change",
            Mark::Fixed => "fixed",
            Mark::Same => "same",
            Mark::Skipped(_) => "skipped",
        }
    }

    fn group(&self) -> u8 {
        match self {
            Mark::Change => 0,
            Mark::Fixed => 1,
            Mark::Same => 2,
            Mark::Skipped(_) => 3,
        }
    }
}

/// Why an item is left alone.
pub const NOT_USUAL: &str = "not a letter code and a number";
/// Why a device is left alone.
pub const NO_TAG: &str = "no tag";

/// One item of a plan.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The device, the cable, or the first conductor of the net.
    pub object: ObjectRef,
    pub kind: Kind,
    /// The tag or wire number before (`?` for a net with none, `(no tag)`).
    pub old: String,
    /// The tag or wire number after (equal to `old` unless [`Mark::Change`]).
    pub new: String,
    /// Where the anchor is, in the project's reference style (`101-A-2`), or empty.
    pub at: String,
    /// The anchor's page, for going to it.
    pub page: Option<PageId>,
    /// The anchor point, for going to it.
    pub point: Option<Pt>,
    pub mark: Mark,
    /// Brief 028: the terminal a [`Kind::Terminals`] row relabels.
    pub terminal: Option<TerminalId>,
    order: Order,
}

/// A plan: rows in the preview's order (changes, then fixed, then same, then skipped; each in
/// drawing order; kinds grouped Dev, Cbl, Wire), and the ops writing it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub rows: Vec<Row>,
    pub ops: Vec<Op>,
}

impl Plan {
    /// Rows that change.
    pub fn changes(&self) -> usize {
        self.rows.iter().filter(|r| r.mark == Mark::Change).count()
    }

    /// Rows kept because fixed.
    pub fn fixed(&self) -> usize {
        self.rows.iter().filter(|r| r.mark == Mark::Fixed).count()
    }

    /// Rows unchanged.
    pub fn same(&self) -> usize {
        self.rows.iter().filter(|r| r.mark == Mark::Same).count()
    }

    /// Rows skipped.
    pub fn skipped(&self) -> usize {
        self.rows.iter().filter(|r| matches!(r.mark, Mark::Skipped(_))).count()
    }

    /// Changed rows of `kind`.
    pub fn changes_of(&self, kind: Kind) -> usize {
        self.rows.iter().filter(|r| r.kind == kind && r.mark == Mark::Change).count()
    }
}

/// The drawing-order key: (has no anchor, book position, column, row, −y, x).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Order {
    none: bool,
    page: usize,
    column: u32,
    row: u32,
    ny: f64,
    x: f64,
}

impl Order {
    fn cmp(&self, o: &Order) -> std::cmp::Ordering {
        (self.none, self.page, self.column, self.row)
            .cmp(&(o.none, o.page, o.column, o.row))
            .then(self.ny.total_cmp(&o.ny))
            .then(self.x.total_cmp(&o.x))
    }
}

/// Most numbers a run hands out per prefix before it gives up (bounded skipping of taken numbers).
const MAX_SKIPS: u64 = 1_000_000;

/// Refusal of a start number out of range.
pub const BAD_START: &str = "Start is a whole number from 1.";
/// Refusal of digits out of range.
pub const BAD_DIGITS: &str = "Digits is a whole number from 1 to 6.";

/// What reads the drawing order: the book order and each page's grid, built once per plan.
pub struct Places<'a> {
    doc: &'a Document,
    book: Book,
    grids: HashMap<PageId, Grid>,
}

impl<'a> Places<'a> {
    /// The places of `doc`.
    pub fn new(doc: &'a Document) -> Self {
        Places { doc, book: Book::of(doc), grids: HashMap::new() }
    }

    fn grid(&mut self, page: PageId) -> &Grid {
        let doc = self.doc;
        self.grids.entry(page).or_insert_with(|| Grid::of(doc, page))
    }

    fn order(&mut self, page: PageId, at: Pt) -> Order {
        let pos = self.book.position(page).unwrap_or(usize::MAX);
        let g = self.grid(page);
        let (column, row) = (g.column_of(at.x).unwrap_or(u32::MAX), g.row_of(at.y).unwrap_or(u32::MAX));
        Order { none: false, page: pos, column, row, ny: -at.y, x: at.x }
    }

    /// `101-A-2` for `at` on `page`, in the project's reference style.
    pub fn text(&mut self, page: PageId, at: Pt) -> String {
        let label = self.book.number(self.doc, page);
        let style = self.doc.settings.reference_style;
        let g = self.grid(page);
        crate::derive::place_on(g, &label, at).map_or(label, |p| p.text(style))
    }

    /// The anchor of a representation: its symbol's origin, its path's midpoint, its point.
    fn rep_point(&self, r: &Representation) -> Option<Pt> {
        let item = self.doc.items.get(r.items.first()?)?;
        let p = match &item.kind {
            ItemKind::Symbol(s) => s.placement.origin,
            ItemKind::Primitive(Primitive::Point { at }) => *at,
            ItemKind::Primitive(Primitive::Text { at, .. }) => *at,
            ItemKind::Primitive(p) => {
                let (a, b) = p.path_ends()?;
                Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
            }
        };
        p.is_finite().then_some(p)
    }

    /// The anchor of `subject`: a representation with role `main` when `prefer_main`, else the
    /// first in drawing order.
    fn anchor(&mut self, subject: Subject, prefer_main: bool) -> Option<(PageId, Pt, Order)> {
        let doc = self.doc;
        let mut best: Option<(PageId, Pt, Order)> = None;
        let mut main: Option<(PageId, Pt, Order)> = None;
        for (_, r) in doc.representations_of(subject) {
            let Some(at) = self.rep_point(r) else { continue };
            let o = self.order(r.page, at);
            if prefer_main && r.role == "main" && main.as_ref().is_none_or(|m| o.cmp(&m.2).is_lt()) {
                main = Some((r.page, at, o));
            }
            if best.as_ref().is_none_or(|b| o.cmp(&b.2).is_lt()) {
                best = Some((r.page, at, o));
            }
        }
        main.or(best)
    }

    /// The anchor of a net: its first conductor's route start, on its first page in drawing order.
    fn net_anchor(&mut self, net: &NetData) -> Option<(PageId, Pt, Order)> {
        let doc = self.doc;
        let first = net.first_conductor()?;
        let mut best: Option<(PageId, Pt, Order)> = None;
        for (_, r) in doc.representations_of(Subject::Conductor(first)) {
            let Some((a, _)) = crate::derive::route_ends_on(doc, first, r.page) else { continue };
            if !a.is_finite() {
                continue;
            }
            let o = self.order(r.page, a);
            if best.as_ref().is_none_or(|b| o.cmp(&b.2).is_lt()) {
                best = Some((r.page, a, o));
            }
        }
        best
    }

    /// Whether `page` lies in `folder` or one of its subfolders (bounded depth).
    fn pages_of_folder(&self, folder: FolderId) -> BTreeSet<PageId> {
        let mut out = BTreeSet::new();
        let mut todo = vec![(folder, 1usize)];
        let mut seen = BTreeSet::new();
        while let Some((f, depth)) = todo.pop() {
            if depth > crate::book::MAX_FOLDER_DEPTH || !seen.insert(f) {
                continue;
            }
            out.extend(self.doc.pages_in_folder(f));
            todo.extend(self.doc.subfolders(f).into_iter().map(|c| (c, depth + 1)));
        }
        out
    }
}

/// Where `subject` is shown, cheaply (for Checks rows): its `main` representation, else its
/// lowest-ID one, and that representation's anchor point.
pub fn rep_anchor(doc: &Document, subject: Subject) -> Option<(PageId, Pt)> {
    let places = Places { doc, book: Book::default(), grids: HashMap::new() };
    let mut first = None;
    for (_, r) in doc.representations_of(subject) {
        let Some(at) = places.rep_point(r) else { continue };
        if r.role == "main" {
            return Some((r.page, at));
        }
        first.get_or_insert((r.page, at));
    }
    first
}

/// Whether a project has provisional tags on (FP2).
pub fn provisional_on(doc: &Document) -> bool {
    doc.properties.get(tag::PROVISIONAL_SETTING).is_some_and(|v| v == "yes")
}

/// Whether a device or cable with these properties is fixed (FP1). A provisional tag never
/// counts as fixed (overseer ruling, tester 026 bug 2): Finalise numbers it whatever the flag says.
pub fn is_fixed(props: &crate::doc::Properties) -> bool {
    props.get(FIXED_KEY).is_some_and(|v| v == "yes") && !props.get(TAG_KEY).is_some_and(|t| tag::is_provisional(t))
}

/// The next provisional tag of `prefix` (`-K?3`): the highest provisional number of that prefix
/// in use + 1, through a range of the tag indexes (devices and cables).
pub fn next_provisional(doc: &Document, prefix: &str) -> String {
    let p = prefix.trim_start_matches('-').to_ascii_uppercase();
    let lo = format!("{p}?");
    let mut top = 0;
    for k in doc.tag_keys_starting(&lo) {
        if let Some(rest) = k.strip_prefix(&lo)
            && let Ok(n) = rest.parse::<u64>()
        {
            top = top.max(n);
        }
    }
    tag::format_provisional(&p, top.saturating_add(1))
}

/// The highest number of `prefix` in use by any device or cable tag in the usual form.
pub fn highest_in_use(doc: &Document, prefix: &str) -> u64 {
    let p = prefix.trim_start_matches('-').to_ascii_uppercase();
    doc.tag_keys_starting(&p).filter_map(tag::parse).filter(|(q, _)| *q == p).map(|(_, n)| n).max().unwrap_or(0)
}

/// The fixed flag stored on a provisional tag, which does not count (see [`is_fixed`]).
fn stale_flag(props: &crate::doc::Properties) -> Option<String> {
    props.get(TAG_KEY).filter(|t| tag::is_provisional(t)).and(props.get(FIXED_KEY)).cloned()
}

/// One tagged item (device or cable).
struct Tagged {
    object: ObjectRef,
    kind: Kind,
    tag: Option<String>,
    fixed: bool,
    /// A flag left on a provisional tag (it does not count): dropped when the tag is numbered.
    stale_flag: Option<String>,
    anchor: Option<(PageId, Pt, Order)>,
    id: u128,
}

/// Work out `req` on `doc` (module docs). `high` is the project's tag high-water mark (the
/// provisional scope continues after it).
///
/// # Errors
/// [`BAD_START`] or [`BAD_DIGITS`] for values out of range; an unknown folder.
pub fn plan(doc: &Document, req: &Request, high: &HighWater) -> Result<Plan, String> {
    if let Some(s) = req.start
        && !(1..=tag::MAX_TAG_NUMBER).contains(&s)
    {
        return Err(BAD_START.into());
    }
    if let Some(d) = req.digits
        && !(1..=tag::MAX_PADDING).contains(&d)
    {
        return Err(BAD_DIGITS.into());
    }
    let mut places = Places::new(doc);
    let pages: Option<BTreeSet<PageId>> = match &req.scope {
        Scope::Folder(f) => {
            if !doc.folders.contains_key(f) {
                return Err("No such folder.".into());
            }
            Some(places.pages_of_folder(*f))
        }
        Scope::Pages(p) => Some(p.clone()),
        _ => None,
    };
    let mut rows = Vec::new();
    let mut ops = Vec::new();
    if req.kinds.contains(&Kind::Devices) || req.kinds.contains(&Kind::Cables) {
        plan_tags(doc, req, high, &mut places, pages.as_ref(), &mut rows, &mut ops);
    }
    if req.kinds.contains(&Kind::Wires) {
        plan_wires(doc, req, &mut places, pages.as_ref(), &mut rows, &mut ops);
    }
    if req.kinds.contains(&Kind::Terminals) {
        plan_terminals(doc, req, &mut places, pages.as_ref(), &mut rows, &mut ops);
    }
    rows.sort_by(|a, b| a.mark.group().cmp(&b.mark.group()).then(a.kind.cmp(&b.kind)).then(a.order.cmp(&b.order)));
    Ok(Plan { rows, ops })
}

fn object_hex(o: ObjectRef) -> u128 {
    match o {
        ObjectRef::Component(c) => c.0,
        ObjectRef::Cable(c) => c.0,
        ObjectRef::Conductor(c) => c.0,
        ObjectRef::Page(p) => p.0,
        ObjectRef::Project => 0,
    }
}

#[allow(clippy::too_many_arguments)]
fn plan_tags(
    doc: &Document, req: &Request, high: &HighWater, places: &mut Places<'_>, pages: Option<&BTreeSet<PageId>>, rows: &mut Vec<Row>,
    ops: &mut Vec<Op>,
) {
    // Every device (tagless junctions and continuations aside) and every tagged cable.
    let mut items: Vec<Tagged> = Vec::new();
    for (id, c) in &doc.components {
        if c.properties.contains_key(connectivity::KIND) {
            continue;
        }
        let tag = c.properties.get(TAG_KEY).cloned();
        let has_reps = doc.representations_of(Subject::Component(*id)).next().is_some();
        if tag.is_none() && !has_reps {
            continue;
        }
        items.push(Tagged {
            object: ObjectRef::Component(*id),
            kind: Kind::Devices,
            tag,
            fixed: is_fixed(&c.properties),
            stale_flag: stale_flag(&c.properties),
            anchor: None,
            id: id.0,
        });
    }
    for (_, k) in doc.tagged_cables() {
        let Some(c) = doc.cables.get(&k) else { continue };
        let tag = c.properties.get(TAG_KEY).cloned();
        items.push(Tagged {
            object: ObjectRef::Cable(k),
            kind: Kind::Cables,
            tag,
            fixed: is_fixed(&c.properties),
            stale_flag: stale_flag(&c.properties),
            anchor: None,
            id: k.0,
        });
    }
    let digits = req.digits.unwrap_or_else(|| doc.settings.tag_padding.min(tag::MAX_PADDING));
    // Which items are numbered: in a chosen kind, in scope, not fixed, in a numberable form.
    let mut chosen: Vec<(usize, String)> = Vec::new(); // (item index, prefix)
    // Numbers taken per prefix by everything not renumbered.
    let mut taken: BTreeMap<String, BTreeSet<u64>> = BTreeMap::new();
    for (i, it) in items.iter_mut().enumerate() {
        let wanted_kind = req.kinds.contains(&it.kind);
        let parsed = it.tag.as_deref().and_then(tag::parse);
        let prov = it.tag.as_deref().and_then(tag::provisional).filter(|(_, n)| *n > 0);
        let prefix = parsed.as_ref().or(prov.as_ref()).map(|(p, _)| p.clone());
        // Scope needs the anchor only for folder and page scopes; the row needs it to sort.
        let in_scope = wanted_kind && {
            match &req.scope {
                Scope::All => true,
                Scope::Selection { components, cables, .. } => match it.object {
                    ObjectRef::Component(c) => components.contains(&c),
                    ObjectRef::Cable(c) => cables.contains(&c),
                    _ => false,
                },
                Scope::Prefix(p) => prefix.as_deref() == Some(p.trim_start_matches('-').to_ascii_uppercase().as_str()),
                Scope::Folder(_) | Scope::Pages(_) => {
                    let subject = match it.object {
                        ObjectRef::Component(c) => Subject::Component(c),
                        ObjectRef::Cable(c) => Subject::Cable(c),
                        _ => continue,
                    };
                    it.anchor = places.anchor(subject, it.kind == Kind::Devices);
                    it.anchor.as_ref().is_some_and(|(pg, _, _)| pages.is_some_and(|s| s.contains(pg)))
                }
                Scope::Provisional => prov.is_some(),
            }
        };
        let numbered = in_scope && !it.fixed && (parsed.is_some() || prov.is_some());
        if numbered {
            if let Some(p) = &prefix {
                chosen.push((i, p.clone()));
            }
        } else if let Some((p, n)) = &parsed {
            taken.entry(p.clone()).or_default().insert(*n);
        }
        if in_scope && it.anchor.is_none() {
            let subject = match it.object {
                ObjectRef::Component(c) => Subject::Component(c),
                ObjectRef::Cable(c) => Subject::Cable(c),
                _ => continue,
            };
            it.anchor = places.anchor(subject, it.kind == Kind::Devices);
        }
        if in_scope && !numbered {
            let old = it.tag.clone().unwrap_or_else(|| "(no tag)".into());
            let mark = if it.tag.is_none() {
                Mark::Skipped(NO_TAG)
            } else if it.fixed {
                Mark::Fixed
            } else {
                Mark::Skipped(NOT_USUAL)
            };
            rows.push(row_of(places, it, old.clone(), old, mark));
        }
    }
    // Drawing order per prefix.
    let mut by_prefix: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, p) in &chosen {
        by_prefix.entry(p.clone()).or_default().push(*i);
    }
    let mut changed_prefixes: BTreeSet<String> = BTreeSet::new();
    let mut in_use_after: BTreeMap<String, u64> = BTreeMap::new();
    for (p, list) in &mut by_prefix {
        list.sort_by(|a, b| {
            let (x, y) = (&items[*a], &items[*b]);
            let ox = x.anchor.map_or(Order { none: true, ..Order::default() }, |a| a.2);
            let oy = y.anchor.map_or(Order { none: true, ..Order::default() }, |a| a.2);
            // No anchor: by old number, then ID.
            let nx = x.tag.as_deref().and_then(tag::parse).map_or(u64::MAX, |t| t.1);
            let ny = y.tag.as_deref().and_then(tag::parse).map_or(u64::MAX, |t| t.1);
            ox.cmp(&oy).then(nx.cmp(&ny)).then(x.id.cmp(&y.id))
        });
        let start = match (&req.scope, req.start) {
            (_, Some(s)) => s,
            (Scope::Provisional, None) => highest_in_use(doc, p).max(high.get(p).copied().unwrap_or(0)).saturating_add(1),
            _ => 1,
        };
        let skip = taken.get(p);
        let mut next = start;
        let mut top = skip.and_then(|s| s.last().copied()).unwrap_or(0);
        for i in list.iter() {
            let mut tries = 0;
            while skip.is_some_and(|s| s.contains(&next)) && tries < MAX_SKIPS {
                next = next.saturating_add(1);
                tries += 1;
            }
            let it = &items[*i];
            let old = it.tag.clone().unwrap_or_default();
            if next > tag::MAX_TAG_NUMBER {
                rows.push(row_of(places, it, old.clone(), old, Mark::Skipped("no number left")));
                continue;
            }
            let new = tag::format(p, next, digits);
            top = top.max(next);
            next = next.saturating_add(1);
            if new == old {
                rows.push(row_of(places, it, old.clone(), new, Mark::Same));
            } else {
                changed_prefixes.insert(p.clone());
                let before = it.tag.clone();
                ops.push(Op::SetProperty { object: it.object, key: TAG_KEY.into(), before, after: Some(new.clone()) });
                if let Some(f) = &it.stale_flag {
                    ops.push(Op::SetProperty { object: it.object, key: FIXED_KEY.into(), before: Some(f.clone()), after: None });
                }
                rows.push(row_of(places, it, old, new, Mark::Change));
            }
        }
        in_use_after.insert(p.clone(), top);
    }
    // FP4: the mark of each prefix whose tags changed is the highest number now in use.
    for p in changed_prefixes {
        let n = in_use_after.get(&p).copied().unwrap_or(0);
        let key = format!("{HIGH_RESET_PREFIX}{p}");
        let before = doc.properties.get(&key).cloned();
        // Always written, also when the number is the stored one (tester 026, 6): the reset
        // carries the mark it replaces and a newer generation (`tag::reset_value`).
        let generation = before.as_deref().and_then(tag::parse_reset).map_or(0, |r| r.2).saturating_add(1);
        let was = high.get(&p).copied().unwrap_or(0);
        let after = Some(tag::reset_value(n, was, generation));
        ops.push(Op::SetProperty { object: ObjectRef::Project, key, before, after });
    }
}

/// Brief 028: number each strip's terminals in drawing order (module docs).
fn plan_terminals(doc: &Document, req: &Request, places: &mut Places<'_>, pages: Option<&BTreeSet<PageId>>, rows: &mut Vec<Row>, ops: &mut Vec<Op>) {
    let start = req.start.unwrap_or(1);
    for strip in &req.strips {
        let Some(comp) = doc.components.get(strip) else { continue };
        let tag = comp.properties.get(TAG_KEY).cloned().unwrap_or_else(|| "?".into());
        // Each terminal once, anchored at the symbol that shows it.
        let mut seen: BTreeSet<TerminalId> = BTreeSet::new();
        let mut list: Vec<(TerminalId, PageId, Pt, Order)> = Vec::new();
        for (_, r) in doc.representations_of(Subject::Component(*strip)) {
            let Some(at) = places.rep_point(r) else { continue };
            let shown: Vec<TerminalId> = if r.connection_points.is_empty() {
                let keys: BTreeSet<&str> = match doc.items.get(r.items.first().unwrap_or(&crate::id::ItemId(0))).map(|i| &i.kind) {
                    Some(ItemKind::Symbol(s)) => {
                        doc.definitions.get(&s.definition).map(|d| d.terminals.iter().map(|t| t.key.as_str()).collect()).unwrap_or_default()
                    }
                    _ => BTreeSet::new(),
                };
                doc.terminals_of(*strip).filter(|(_, t)| t.key.as_deref().is_some_and(|k| keys.contains(k))).map(|(t, _)| t).collect()
            } else {
                r.connection_points.keys().copied().collect()
            };
            let o = places.order(r.page, at);
            for t in shown {
                if seen.insert(t) {
                    list.push((t, r.page, at, o));
                }
            }
        }
        // Page and folder scopes: a strip with a terminal on those pages, numbered whole.
        if let Some(ps) = pages
            && !list.iter().any(|(_, p, _, _)| ps.contains(p))
        {
            continue;
        }
        list.sort_by(|a, b| a.3.cmp(&b.3).then(a.0.cmp(&b.0)));
        for (n, (t, page, at, order)) in (start..).zip(list) {
            let Some(term) = doc.terminals.get(&t) else { continue };
            let old_label = term.label.clone().unwrap_or_default();
            let new_label = n.to_string();
            let mark = if old_label == new_label { Mark::Same } else { Mark::Change };
            if mark == Mark::Change {
                let mut after = term.clone();
                after.label = Some(new_label.clone());
                ops.push(Op::RemoveTerminal { id: t, terminal: term.clone() });
                ops.push(Op::AddTerminal { id: t, terminal: after });
            }
            rows.push(Row {
                object: ObjectRef::Component(*strip),
                kind: Kind::Terminals,
                old: format!("{tag}:{old_label}"),
                new: format!("{tag}:{new_label}"),
                at: places.text(page, at),
                page: Some(page),
                point: Some(at),
                mark,
                terminal: Some(t),
                order,
            });
        }
    }
}

fn row_of(places: &mut Places<'_>, it: &Tagged, old: String, new: String, mark: Mark) -> Row {
    let (page, point, order) = match it.anchor {
        Some((pg, at, o)) => (Some(pg), Some(at), o),
        None => (None, None, Order { none: true, ..Order::default() }),
    };
    let at = match (page, point) {
        (Some(pg), Some(p)) => places.text(pg, p),
        _ => String::new(),
    };
    Row { object: it.object, kind: it.kind, old, new, at, page, point, mark, terminal: None, order }
}

fn plan_wires(doc: &Document, req: &Request, places: &mut Places<'_>, pages: Option<&BTreeSet<PageId>>, rows: &mut Vec<Row>, ops: &mut Vec<Op>) {
    let conn = doc.connectivity();
    let fresh = !matches!(req.scope, Scope::Provisional);
    let mut chosen: Vec<(NetId, &NetData, Option<(PageId, Pt, Order)>)> = Vec::new();
    let mut fixed_rows: Vec<(NetId, &NetData, Option<(PageId, Pt, Order)>)> = Vec::new();
    let mut kept: BTreeSet<NetId> = BTreeSet::new();
    let selected: Option<BTreeSet<NetId>> = match &req.scope {
        Scope::Selection { conductors, .. } => Some(conductors.iter().filter_map(|c| conn.net_of_conductor(*c)).collect()),
        _ => None,
    };
    for (id, net) in conn.nets() {
        if net.conductors.is_empty() {
            continue;
        }
        let unnumbered = net.numbers.is_empty() || net.conductors.iter().any(|c| wiring::unnumbered(doc, *c));
        let mut anchor = None;
        let in_scope = match &req.scope {
            Scope::All => true,
            Scope::Selection { .. } => selected.as_ref().is_some_and(|s| s.contains(&id)),
            Scope::Prefix(p) => {
                let pot = wiring::net_potential(doc, net);
                pot.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(p.trim())) || pot.prefix.eq_ignore_ascii_case(p.trim())
            }
            Scope::Folder(_) | Scope::Pages(_) => {
                anchor = places.net_anchor(net);
                anchor.as_ref().is_some_and(|(pg, _, _)| pages.is_some_and(|s| s.contains(pg)))
            }
            Scope::Provisional => unnumbered,
        };
        if !in_scope {
            kept.insert(id);
            continue;
        }
        if anchor.is_none() {
            anchor = places.net_anchor(net);
        }
        if net.fixed.is_empty() {
            chosen.push((id, net, anchor));
        } else {
            kept.insert(id);
            fixed_rows.push((id, net, anchor));
        }
    }
    let key = |a: &Option<(PageId, Pt, Order)>| a.map_or(Order { none: true, ..Order::default() }, |x| x.2);
    chosen.sort_by(|a, b| key(&a.2).cmp(&key(&b.2)).then(a.1.first_conductor().cmp(&b.1.first_conductor())));
    let row = |places: &mut Places<'_>, net: &NetData, anchor: Option<(PageId, Pt, Order)>, old: String, new: String, mark: Mark| {
        let (page, point, order) = match anchor {
            Some((pg, at, o)) => (Some(pg), Some(at), o),
            None => (None, None, Order { none: true, ..Order::default() }),
        };
        let at = match (page, point) {
            (Some(pg), Some(p)) => places.text(pg, p),
            _ => String::new(),
        };
        let object = ObjectRef::Conductor(net.first_conductor().unwrap_or(ConductorId(0)));
        Row { object, kind: Kind::Wires, old, new, at, page, point, mark, terminal: None, order }
    };
    for (_, net, anchor) in fixed_rows {
        let n = wiring::net_number(doc, net).unwrap_or_else(|| "?".into());
        rows.push(row(places, net, anchor, n.clone(), n, Mark::Fixed));
    }
    let digits = req.digits.map(usize::from);
    let mut out_rows = Vec::new();
    if fresh {
        // Taken: every number of a net not renumbered, per sequence scope.
        let mut taken: BTreeMap<String, BTreeSet<u64>> = BTreeMap::new();
        for id in &kept {
            if let Some(net) = conn.net(*id) {
                for n in net.numbers.keys().chain(net.fixed.keys()) {
                    if let Some((p, s)) = split_number(n) {
                        taken.entry(p.to_string()).or_default().insert(s);
                    }
                }
            }
        }
        let start = req.start.unwrap_or(1);
        let mut next: BTreeMap<String, u64> = BTreeMap::new();
        for (_, net, anchor) in chosen {
            let pot = wiring::net_potential(doc, net);
            let number = if pot.numbered {
                let sc = wiring::scope_of(doc, net, &pot);
                let slot = next.entry(sc.clone()).or_insert(start);
                let mut tries = 0;
                while taken.get(&sc).is_some_and(|s| s.contains(slot)) && tries < MAX_SKIPS {
                    *slot = slot.saturating_add(1);
                    tries += 1;
                }
                let n = *slot;
                *slot = slot.saturating_add(1);
                wiring::format_digits(doc, &sc, n, digits)
            } else {
                pot.prefix.clone()
            };
            out_rows.push((net, anchor, number));
        }
    } else {
        let mut alloc = wiring::Allocator::default();
        for (id, net, anchor) in chosen {
            let pot = wiring::net_potential(doc, net);
            let number = if !pot.numbered {
                pot.prefix.clone()
            } else if let Some(w) = wiring::kept_number(doc, id, net) {
                w
            } else {
                alloc.next(doc, net, &pot)
            };
            out_rows.push((net, anchor, number));
        }
    }
    for (net, anchor, number) in out_rows {
        let old = wiring::net_number(doc, net);
        let w = wiring::write_ops(doc, net, &number, false);
        let shown_old = old.clone().unwrap_or_else(|| "?".into());
        if w.is_empty() {
            rows.push(row(places, net, anchor, shown_old.clone(), shown_old, Mark::Same));
        } else {
            ops.extend(w);
            rows.push(row(places, net, anchor, shown_old, number, Mark::Change));
        }
    }
}

/// The page rows of `plan` lie on, for the pages that redraw (callers' update maps).
pub fn pages_of(plan: &Plan) -> BTreeSet<PageId> {
    plan.rows.iter().filter(|r| r.mark == Mark::Change).filter_map(|r| r.page).collect()
}

/// `object`'s ID as a number, for stable ordering in callers.
pub fn object_id(o: ObjectRef) -> u128 {
    object_hex(o)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::doc::{Component, Form, Item, Page, Style, SymbolInstance};
    use crate::geom::Placement;
    use crate::id::{DefinitionId, ItemId, LayerId, RepresentationId};

    /// Three pages; each device a symbol at (x, y) on its page.
    fn fixture(devs: &[(u128, &str, usize, f64, f64, bool)]) -> Document {
        let mut d = Document::default();
        for i in 0..3u128 {
            let p = PageId(1000 + i);
            d.pages.insert(p, Page::new(format!("P{i}")));
            d.page_order.push(p);
        }
        let style = Style { layer: LayerId(1), color: crate::doc::Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
        for (id, t, page, x, y, fixed) in devs {
            let mut c = Component::default();
            c.properties.insert(TAG_KEY.into(), (*t).into());
            if *fixed {
                c.properties.insert(FIXED_KEY.into(), "yes".into());
            }
            d.components.insert(ComponentId(*id), c);
            let item = ItemId(5000 + id);
            let pg = d.page_order[*page];
            d.items.insert(
                item,
                Item {
                    page: pg,
                    style: style.clone(),
                    kind: ItemKind::Symbol(SymbolInstance {
                        definition: DefinitionId(9),
                        placement: Placement { origin: Pt::new(*x, *y), rotation: 0.0, mirror: false, scale: 1.0 },
                    }),
                },
            );
            d.representations.insert(
                RepresentationId(7000 + id),
                Representation {
                    subject: Subject::Component(ComponentId(*id)),
                    page: pg,
                    form: Form::Symbol,
                    role: "main".into(),
                    items: vec![item],
                    connection_points: BTreeMap::new(),
                },
            );
        }
        d.rebuild_index();
        d
    }

    fn req(scope: Scope) -> Request {
        Request { kinds: [Kind::Devices].into_iter().collect(), scope, start: None, digits: None, strips: BTreeSet::new() }
    }

    fn after(d: &Document, p: &Plan) -> BTreeMap<u128, String> {
        let mut x = d.clone();
        for op in &p.ops {
            op.apply(&mut x).unwrap();
        }
        x.components.iter().filter_map(|(id, c)| c.properties.get(TAG_KEY).map(|t| (id.0, t.clone()))).collect()
    }

    #[test]
    fn devices_all_in_drawing_order_around_fixed() {
        // Page 0: -K07 at column A (x 50), -K01 at column C (x 150); page 1: -K03 fixed, -K02.
        let d = fixture(&[
            (1, "-K07", 0, 50.0, 200.0, false),
            (2, "-K01", 0, 150.0, 200.0, false),
            (3, "-K03", 1, 50.0, 200.0, true),
            (4, "-K02", 1, 300.0, 100.0, false),
            (5, "-Q01", 1, 60.0, 100.0, false),
        ]);
        let p = plan(&d, &req(Scope::All), &HighWater::new()).unwrap();
        let t = after(&d, &p);
        assert_eq!(t[&1], "-K01");
        assert_eq!(t[&2], "-K02");
        assert_eq!(t[&3], "-K03");
        assert_eq!(t[&4], "-K04");
        assert_eq!(t[&5], "-Q01");
        assert_eq!(p.fixed(), 1);
        assert_eq!(p.changes(), 3);
        // The high-water reset for K, last.
        assert!(
            matches!(p.ops.last(), Some(Op::SetProperty { object: ObjectRef::Project, key, after: Some(v), .. }) if key == "numbering.high.K" && v.starts_with("4;"))
        );
        let mut high = HighWater::new();
        tag::note_ops(&mut high, &[Op::SetProperty { object: ObjectRef::Project, key: TAG_KEY.into(), before: None, after: Some("-K07".into()) }]);
        tag::note_ops(&mut high, &p.ops);
        assert_eq!(high.get("K"), Some(&4));
    }

    #[test]
    fn scopes_change_only_their_items() {
        let d = fixture(&[(1, "-K07", 0, 50.0, 200.0, false), (2, "-K01", 2, 150.0, 200.0, false), (3, "-Q05", 1, 50.0, 200.0, false)]);
        let p = plan(&d, &req(Scope::Prefix("-Q".into())), &HighWater::new()).unwrap();
        let t = after(&d, &p);
        assert_eq!((t[&1].as_str(), t[&2].as_str(), t[&3].as_str()), ("-K07", "-K01", "-Q01"));
        let p = plan(&d, &req(Scope::Pages([d.page_order[0]].into_iter().collect())), &HighWater::new()).unwrap();
        let t = after(&d, &p);
        // -K01 (page 2) is out of scope: its number is taken, so -K07 becomes -K02.
        assert_eq!((t[&1].as_str(), t[&2].as_str()), ("-K02", "-K01"));
        let sel = Scope::Selection { components: [ComponentId(2)].into_iter().collect(), cables: BTreeSet::new(), conductors: BTreeSet::new() };
        let p = plan(&d, &Request { start: Some(5), ..req(sel) }, &HighWater::new()).unwrap();
        let t = after(&d, &p);
        assert_eq!((t[&1].as_str(), t[&2].as_str()), ("-K07", "-K05"));
    }

    #[test]
    fn provisional_continue_after_highest() {
        let d = fixture(&[
            (1, "-K?2", 0, 150.0, 200.0, false),
            (2, "-K?1", 0, 50.0, 200.0, false),
            (3, "-K04", 1, 50.0, 200.0, false),
            (4, "-K1A", 1, 70.0, 200.0, false),
        ]);
        let mut high = HighWater::new();
        high.insert("K".into(), 6);
        let p = plan(&d, &req(Scope::Provisional), &high).unwrap();
        let t = after(&d, &p);
        assert_eq!((t[&2].as_str(), t[&1].as_str(), t[&3].as_str(), t[&4].as_str()), ("-K07", "-K08", "-K04", "-K1A"));
        let p = plan(&d, &req(Scope::All), &HighWater::new()).unwrap();
        assert!(p.rows.iter().any(|r| r.mark == Mark::Skipped(NOT_USUAL) && r.old == "-K1A"));
        assert_eq!(next_provisional(&d, "K"), "-K?3");
        assert_eq!(next_provisional(&d, "W"), "-W?1");
    }

    #[test]
    fn hostile_requests_are_refused() {
        let d = fixture(&[]);
        for (start, digits) in [(Some(0), None), (Some(1_000_000_000_000), None), (None, Some(9)), (None, Some(0))] {
            assert!(plan(&d, &Request { start, digits, ..req(Scope::All) }, &HighWater::new()).is_err());
        }
        assert!(plan(&d, &req(Scope::Folder(FolderId(77))), &HighWater::new()).is_err());
    }

    #[test]
    fn swaps_are_plain_writes() {
        // -K02 drawn before -K01: an exchange, no placeholder.
        let d = fixture(&[(1, "-K02", 0, 50.0, 200.0, false), (2, "-K01", 0, 150.0, 200.0, false)]);
        let p = plan(&d, &req(Scope::All), &HighWater::new()).unwrap();
        let t = after(&d, &p);
        assert_eq!((t[&1].as_str(), t[&2].as_str()), ("-K01", "-K02"));
        assert_eq!(p.ops.iter().filter(|o| matches!(o, Op::SetProperty { key, .. } if key == TAG_KEY)).count(), 2);
    }
}
