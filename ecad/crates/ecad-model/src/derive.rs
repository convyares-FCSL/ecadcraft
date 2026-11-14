//! Derived results: recomputed from the document, never stored (principle P5).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::book::Book;
use crate::doc::{Document, End, Form, ItemKind, Primitive, ReferenceStyle, Representation, Subject};
use crate::geom::Pt;
use crate::id::{CableId, ConductorId, ItemId, PageId, RepresentationId, TerminalId};
use crate::sheet::Grid;

/// Where each terminal sits on one page, from drawn connection points and symbol placements.
///
/// # Semantics
/// Symbol terminal positions are computed from the stored placement and the embedded
/// definition's local terminal position (proposal §4.3), or the representation's connection
/// points in the definition's local frame when it has them (brief 018). A terminal shown by several
/// representations on the same page appears once per representation.
pub fn terminal_positions(doc: &Document, page: PageId) -> Vec<(TerminalId, RepresentationId, Pt)> {
    let mut out = Vec::new();
    for (rid, r) in doc.representations.iter().filter(|(_, r)| r.page == page) {
        out.extend(shown_by(doc, r).into_iter().map(|(t, p)| (t, *rid, p)));
    }
    out
}

/// Where `terminal` is shown on `page`: the first entry for it in [`terminal_positions`], found
/// through its component's representations only, so the cost does not grow with the page or
/// the project.
///
/// # Semantics
/// The same answer as `terminal_positions(doc, page)`'s first entry for `terminal` on any valid
/// document, where a representation shows only its own component's terminals (a drawn
/// representation's connection points are its component's terminals). Both read a
/// representation through the same helper, so they cannot drift apart.
pub fn terminal_position(doc: &Document, page: PageId, terminal: TerminalId) -> Option<Pt> {
    let component = doc.terminals.get(&terminal)?.component;
    doc.representations_of(Subject::Component(component))
        .filter(|(_, r)| r.page == page)
        .find_map(|(_, r)| shown_by(doc, r).into_iter().find(|(t, _)| *t == terminal).map(|(_, p)| p))
}

/// The terminals a component representation shows, with their positions, in the order
/// [`terminal_positions`] lists them.
fn shown_by(doc: &Document, r: &Representation) -> Vec<(TerminalId, Pt)> {
    let Subject::Component(component) = r.subject else { return Vec::new() };
    match r.form {
        Form::Drawn => r.connection_points.iter().map(|(t, p)| (*t, *p)).collect(),
        Form::Symbol => {
            let Some(ItemKind::Symbol(inst)) = r.items.first().and_then(|i| doc.items.get(i)).map(|i| &i.kind) else {
                return Vec::new();
            };
            // Connection points of a symbol representation are in the definition's local frame
            // (brief 018): the terminals follow the instance.
            if !r.connection_points.is_empty() {
                return r.connection_points.iter().map(|(t, p)| (*t, inst.placement.apply(*p))).collect();
            }
            let Some(def) = doc.definitions.get(&inst.definition) else { return Vec::new() };
            doc.terminals_of(component)
                .filter_map(|(tid, t)| {
                    let td = t.key.as_ref().and_then(|k| def.terminals.iter().find(|d| &d.key == k))?;
                    Some((tid, inst.placement.apply(td.at)))
                })
                .collect()
        }
    }
}

/// The route of a conductor on a page: its first route item's path.
pub fn route_on_page(doc: &Document, conductor: ConductorId, page: PageId) -> Option<(crate::id::ItemId, Primitive)> {
    doc.representations_of(Subject::Conductor(conductor)).filter(|(_, r)| r.page == page).find_map(|(_, r)| {
        let id = *r.items.first()?;
        match &doc.items.get(&id)?.kind {
            ItemKind::Primitive(p) if p.path_ends().is_some() => Some((id, p.clone())),
            _ => None,
        }
    })
}

/// An attached conductor end whose drawn route end is not at its terminal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mismatch {
    pub conductor: ConductorId,
    pub end: End,
    pub terminal: TerminalId,
    pub page: PageId,
    pub route_end: Pt,
    pub terminal_at: Pt,
}

/// Attached ends whose geometry disagrees with their attachment (DOC-03 check).
///
/// # Semantics
/// Only pages where both the route and the terminal are shown are compared; an attachment to a
/// terminal not shown on the route's page is not a mismatch. Positions compare within `tol`.
pub fn mismatches(doc: &Document, tol: f64) -> Vec<Mismatch> {
    let mut out = Vec::new();
    let mut positions: BTreeMap<PageId, Vec<(TerminalId, RepresentationId, Pt)>> = BTreeMap::new();
    for page in &doc.page_order {
        positions.insert(*page, terminal_positions(doc, *page));
    }
    for (cid, c) in &doc.conductors {
        for (_, r) in doc.representations_of(Subject::Conductor(*cid)) {
            let Some((a, b)) = r.items.first().and_then(|i| doc.items.get(i)).and_then(|i| match &i.kind {
                ItemKind::Primitive(p) => p.path_ends(),
                ItemKind::Symbol(_) => None,
            }) else {
                continue;
            };
            for (end, at) in [(End::A, a), (End::B, b)] {
                let Some(t) = c.end(end) else { continue };
                let shown: Vec<Pt> = positions.get(&r.page).into_iter().flatten().filter(|(pt, _, _)| *pt == t).map(|(_, _, p)| *p).collect();
                if !shown.is_empty() && !shown.iter().any(|p| p.near(at, tol)) {
                    let terminal_at = shown.first().copied().unwrap_or_default();
                    out.push(Mismatch { conductor: *cid, end, terminal: t, page: r.page, route_end: at, terminal_at });
                }
            }
        }
    }
    out
}

/// A connected set of terminals and conductors (derived net; identity not stable, §15).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Net {
    pub terminals: Vec<TerminalId>,
    pub conductors: Vec<ConductorId>,
}

/// Nets from stored attachments only (geometry is never consulted, DOC-03).
pub fn nets(doc: &Document) -> Vec<Net> {
    // Union-find over terminals and conductors.
    let mut nodes: Vec<Node> = Vec::new();
    let mut index: BTreeMap<Node, usize> = BTreeMap::new();
    let mut parent: Vec<usize> = Vec::new();
    let mut id_of = |n: Node, nodes: &mut Vec<Node>, parent: &mut Vec<usize>| -> usize {
        *index.entry(n).or_insert_with(|| {
            nodes.push(n);
            parent.push(parent.len());
            parent.len() - 1
        })
    };
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        let mut guard = 0usize;
        while let Some(&p) = parent.get(x) {
            if p == x || guard > parent.len() {
                break;
            }
            x = p;
            guard += 1;
        }
        x
    }
    for (cid, c) in &doc.conductors {
        let ci = id_of(Node::Conductor(*cid), &mut nodes, &mut parent);
        for t in c.ends.iter().flatten() {
            let ti = id_of(Node::Terminal(*t), &mut nodes, &mut parent);
            let (a, b) = (find(&mut parent, ci), find(&mut parent, ti));
            if let Some(slot) = parent.get_mut(a) {
                *slot = b;
            }
        }
    }
    let mut groups: BTreeMap<usize, Net> = BTreeMap::new();
    for (i, n) in nodes.iter().enumerate() {
        let root = find(&mut parent, i);
        let net = groups.entry(root).or_insert_with(|| Net { terminals: Vec::new(), conductors: Vec::new() });
        match n {
            Node::Terminal(t) => net.terminals.push(*t),
            Node::Conductor(c) => net.conductors.push(*c),
        }
    }
    let mut out: Vec<Net> = groups.into_values().collect();
    for n in &mut out {
        n.terminals.sort();
        n.conductors.sort();
    }
    out.sort_by(|a, b| a.conductors.cmp(&b.conductors));
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Node {
    Terminal(TerminalId),
    Conductor(ConductorId),
}

// ---------------------------------------------------------------- cable references (brief 010)

/// The representation role of a cable reference end (design 010 §0): a drawn representation of a
/// cable holding one Point item, where the drawing of the cable stops on its page.
pub const REFERENCE_ROLE: &str = "reference";

/// Most other ends one reference text lists before `+N` (design 010 §6.2); also the most partner
/// IDs a [`CableReference`] keeps, so a hostile file with many references of one cable costs
/// linear, not quadratic, memory.
pub const MAX_LISTED_PARTNERS: usize = 3;

/// Most references one cable may have (design 010, owner question Q2): one continuation. More
/// can only come from a file; they are reported, never made by a command.
pub const MAX_REFERENCES_PER_CABLE: usize = 2;

/// Most member routes on a page whose direction is averaged to place one reference text: enough
/// for any real cable, and a bound on the work per reference.
pub const MAX_DIRECTION_SAMPLES: usize = 1_000;

/// The text that names a place on a sheet in `style` (owner answer Q1, 2026-11-08; UI round 2
/// and rev 3): `105/B3`, `105-B-3` or `/105.3`; `?` (`/?`) when the place cannot be located.
///
/// # Semantics
/// The zone's **letter comes first**: on the default grid (rows lettered, columns numbered) the
/// row, then the column; on a grid lettered across the top (a project saved before UI rev 3, or a
/// sheet spec saying `letters numbers`) the column, then the row, as before (`105-C-4`). The
/// page-and-column style names the column whatever its label (`/105.3` on the default grid): the
/// current path it exists to name (review of PR #96, finding 2).
pub fn place_text(style: ReferenceStyle, page: &str, column: Option<&str>, row: Option<&str>) -> String {
    let lettered = |l: &str| l.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
    // Rows lettered and columns not: the row is the zone's letter.
    let rows_first = matches!((column, row), (Some(c), Some(r)) if lettered(r) && !lettered(c));
    match (style, column, row) {
        (ReferenceStyle::PageZone, Some(c), Some(r)) if rows_first => format!("{page}/{r}{c}"),
        (ReferenceStyle::PageZone, Some(c), Some(r)) => format!("{page}/{c}{r}"),
        (ReferenceStyle::PageColumnRow, Some(c), Some(r)) if rows_first => format!("{page}-{r}-{c}"),
        (ReferenceStyle::PageColumnRow, Some(c), Some(r)) => format!("{page}-{c}-{r}"),
        (ReferenceStyle::PageColumn, Some(c), _) => format!("/{page}.{c}"),
        _ => unlocated(style).to_string(),
    }
}

/// A zone as `B3`: its letter first, whichever axis is lettered ([`place_text`]).
pub fn zone_text(column: &str, row: &str) -> String {
    let lettered = |l: &str| l.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
    if lettered(row) && !lettered(column) { format!("{row}{column}") } else { format!("{column}{row}") }
}

/// What a reference shows for a place that cannot be located, or for no other end.
pub fn unlocated(style: ReferenceStyle) -> &'static str {
    match style {
        ReferenceStyle::PageColumnRow | ReferenceStyle::PageZone => "?",
        ReferenceStyle::PageColumn => "/?",
    }
}

/// Where a point lies on its page's sheet (brief 019): the page's shown number and the frame's
/// column and row, as the frame letters and numbers them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub page: String,
    pub column: u32,
    pub row: u32,
    pub column_label: String,
    pub row_label: String,
}

impl Place {
    /// `105-C-4` (or `/105.C`) in the project's reference style.
    pub fn text(&self, style: ReferenceStyle) -> String {
        place_text(style, &self.page, Some(&self.column_label), Some(&self.row_label))
    }
}

/// The place of `at` on a page with `grid`, whose shown number is `page_label`. `None` when `at`
/// is not finite.
pub fn place_on(grid: &Grid, page_label: &str, at: Pt) -> Option<Place> {
    let (column, row) = (grid.column_of(at.x)?, grid.row_of(at.y)?);
    Some(Place { page: page_label.to_string(), column, row, column_label: grid.column_label(column), row_label: grid.row_label(row) })
}

/// One cable reference end, with everything it says derived from the document (design 010 §6,
/// design 019 §3.3).
///
/// # Semantics
/// Nothing here is stored: the page number is the page's `number` property or its book position
/// ([`crate::book`]), the column and row come from the page's sheet ([`crate::sheet`]), the
/// partners are the cable's other references, and the text names the partners in the project's
/// reference style. All of it changes when pages are renumbered, reordered or moved between
/// folders, or a reference point moves, with no transaction of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct CableReference {
    /// The reference representation.
    pub reference: RepresentationId,
    pub cable: CableId,
    pub page: PageId,
    /// The 1-based book position of `page` (`None` for a page in no order: hostile).
    pub position: Option<usize>,
    /// The number `page` is shown with (`?` for a page in no order).
    pub page_number: String,
    /// The Point item, when the representation holds one.
    pub item: Option<ItemId>,
    /// Where the reference is, when it can be located (a finite Point item).
    pub at: Option<Pt>,
    /// The frame column and row of `at` (1-based), and their labels.
    pub column: Option<u32>,
    pub row: Option<u32>,
    pub column_label: Option<String>,
    pub row_label: Option<String>,
    /// The project's reference style when derived.
    pub style: ReferenceStyle,
    /// The cable's other references, in page order, then column, then ID: at most
    /// [`MAX_LISTED_PARTNERS`] of them (the count is [`CableReference::partner_count`]).
    pub partners: Vec<RepresentationId>,
    /// How many other references the cable has.
    pub partner_count: usize,
    /// The place of each partner ([`CableReference::locator`], at most [`MAX_LISTED_PARTNERS`],
    /// then `+N`), separated by a space; [`unlocated`] for an orphan, and for a partner that
    /// cannot be located.
    pub text: String,
    /// The cable has no other reference.
    pub orphan: bool,
    /// The cable's members whose conductor has a route on `page`.
    pub wires_here: usize,
    /// The unit direction from `at` toward its text: away from the wires (the opposite of the mean
    /// direction in which the member routes on the page run away from `at`); `+x` when no member
    /// route is on the page or the directions cancel out.
    pub text_dir: Pt,
}

impl CableReference {
    /// This end's place (`105-C-4`, or `?` when it cannot be located), as its partners show it.
    pub fn locator(&self) -> String {
        if self.position.is_none() {
            return unlocated(self.style).to_string();
        }
        place_text(self.style, &self.page_number, self.column_label.as_deref(), self.row_label.as_deref())
    }

    /// The partner shown first (the cable's other end, in page order), if any.
    pub fn partner(&self) -> Option<RepresentationId> {
        self.partners.first().copied()
    }
}

/// The end points of each conductor's route per page (its first route item, as
/// [`route_on_page`]), built in one pass so that many lookups stay cheap.
pub fn route_ends_by_page(doc: &Document) -> BTreeMap<(ConductorId, PageId), (Pt, Pt)> {
    let mut out = BTreeMap::new();
    for r in doc.representations.values() {
        let Subject::Conductor(c) = r.subject else { continue };
        if out.contains_key(&(c, r.page)) {
            continue;
        }
        let ends = r.items.first().and_then(|i| doc.items.get(i)).and_then(|i| match &i.kind {
            ItemKind::Primitive(p) => p.path_ends(),
            ItemKind::Symbol(_) => None,
        });
        if let Some(e) = ends {
            out.insert((c, r.page), e);
        }
    }
    out
}

/// The end points of `conductor`'s route on `page` (its first route item, as [`route_on_page`]
/// and [`route_ends_by_page`] pick it), found through the subject index.
pub fn route_ends_on(doc: &Document, conductor: ConductorId, page: PageId) -> Option<(Pt, Pt)> {
    doc.representations_of(Subject::Conductor(conductor)).filter(|(_, r)| r.page == page).find_map(|(_, r)| {
        r.items.first().and_then(|i| doc.items.get(i)).and_then(|i| match &i.kind {
            ItemKind::Primitive(p) => p.path_ends(),
            ItemKind::Symbol(_) => None,
        })
    })
}

/// Every cable reference in the document, in page order, then column, then ID (design 010 §6.2,
/// §7.4). Cost is one pass over the representations plus the referenced cables' members (their
/// routes are found through the subject index, brief 014); never panics on hostile values (they
/// read `/?`).
pub fn cable_references(doc: &Document) -> Vec<CableReference> {
    let refs: Vec<(RepresentationId, &Representation)> = doc
        .representations
        .iter()
        .filter(|(_, r)| r.role == REFERENCE_ROLE && matches!(r.subject, Subject::Cable(_)))
        .map(|(id, r)| (*id, r))
        .collect();
    references_from(doc, refs)
}

/// The references of `cables` only, exactly as [`cable_references`] derives them (same values,
/// same order): everything a reference says depends only on its own cable (its other references,
/// its members' routes) and on the page order and length unit. Cost is in proportion to those
/// cables' references and members, not to the document (brief 014).
pub fn cable_references_of(doc: &Document, cables: &BTreeSet<CableId>) -> Vec<CableReference> {
    let refs: Vec<(RepresentationId, &Representation)> = cables
        .iter()
        .flat_map(|k| doc.representations_of(Subject::Cable(*k)))
        .filter(|(_, r)| r.role == REFERENCE_ROLE && matches!(r.subject, Subject::Cable(_)))
        .collect();
    references_from(doc, refs)
}

/// [`cable_references`] of the reference representations `refs` (each listed once).
fn references_from(doc: &Document, refs: Vec<(RepresentationId, &Representation)>) -> Vec<CableReference> {
    if refs.is_empty() {
        return Vec::new();
    }
    let style = doc.settings.reference_style;
    let book = Book::of(doc);
    // Each page's grid and shown number, worked out once per page.
    let mut pages: BTreeMap<PageId, (Grid, String)> = BTreeMap::new();
    // Route ends per (conductor, page), looked up once each (a cable may list a conductor twice).
    let mut routes: BTreeMap<(ConductorId, PageId), Option<(Pt, Pt)>> = BTreeMap::new();
    // The member routes of each (cable, page) with a reference, built once per pair.
    let mut here: BTreeMap<(CableId, PageId), Vec<(Pt, Pt)>> = BTreeMap::new();
    for (_, r) in &refs {
        let Subject::Cable(cable) = r.subject else { continue };
        if here.contains_key(&(cable, r.page)) {
            continue;
        }
        let members = doc.cables.get(&cable).map(|c| c.members.as_slice()).unwrap_or_default();
        let ends: Vec<(Pt, Pt)> =
            members.iter().filter_map(|m| *routes.entry((m.conductor, r.page)).or_insert_with(|| route_ends_on(doc, m.conductor, r.page))).collect();
        here.insert((cable, r.page), ends);
    }
    let mut out: Vec<CableReference> = Vec::with_capacity(refs.len());
    for (rid, r) in &refs {
        let Subject::Cable(cable) = r.subject else { continue };
        let item = r.items.first().copied();
        let at = item.and_then(|i| doc.items.get(&i)).and_then(|i| match &i.kind {
            ItemKind::Primitive(Primitive::Point { at }) if at.is_finite() => Some(*at),
            _ => None,
        });
        let ends = here.get(&(cable, r.page)).map(Vec::as_slice).unwrap_or_default();
        let wires_here = ends.len();
        let (mut sx, mut sy) = (0.0_f64, 0.0_f64);
        for (a, b) in ends.iter().take(MAX_DIRECTION_SAMPLES) {
            if let Some(p) = at {
                let (near, far) = if a.dist(p) <= b.dist(p) { (*a, *b) } else { (*b, *a) };
                sx += far.x - near.x;
                sy += far.y - near.y;
            }
        }
        let len = sx.hypot(sy);
        let text_dir = if len.is_finite() && len > 0.0 { Pt::new(-sx / len, -sy / len) } else { Pt::new(1.0, 0.0) };
        let (grid, number) = pages.entry(r.page).or_insert_with(|| (Grid::of(doc, r.page), book.number(doc, r.page))).clone();
        let place = at.and_then(|p| place_on(&grid, &number, p));
        out.push(CableReference {
            reference: *rid,
            cable,
            page: r.page,
            position: book.position(r.page),
            page_number: number,
            item,
            at,
            column: place.as_ref().map(|p| p.column),
            row: place.as_ref().map(|p| p.row),
            column_label: place.as_ref().map(|p| p.column_label.clone()),
            row_label: place.as_ref().map(|p| p.row_label.clone()),
            style,
            partners: Vec::new(),
            partner_count: 0,
            text: String::new(),
            orphan: true,
            wires_here,
            text_dir,
        });
    }
    // Page order (a page in no order last), then column, then ID.
    out.sort_by_key(|r| (r.position.unwrap_or(usize::MAX), r.column.unwrap_or(u32::MAX), r.reference));
    // Partners: the cable's other references, already in that order. Each reference keeps at
    // most MAX_LISTED_PARTNERS of them and a count, so the work and memory are linear in the
    // number of references (a hostile file may hold many of one cable).
    let mut by_cable: BTreeMap<CableId, Vec<usize>> = BTreeMap::new();
    for (i, r) in out.iter().enumerate() {
        by_cable.entry(r.cable).or_default().push(i);
    }
    let locators: Vec<String> = out.iter().map(CableReference::locator).collect();
    let ids: Vec<RepresentationId> = out.iter().map(|r| r.reference).collect();
    for (i, r) in out.iter_mut().enumerate() {
        let same = by_cable.get(&r.cable).map(Vec::as_slice).unwrap_or_default();
        let count = same.len().saturating_sub(1);
        let shown: Vec<usize> = same.iter().copied().filter(|j| *j != i).take(MAX_LISTED_PARTNERS).collect();
        r.partners = shown.iter().filter_map(|j| ids.get(*j).copied()).collect();
        r.partner_count = count;
        r.orphan = count == 0;
        r.text = if r.orphan {
            unlocated(style).into()
        } else {
            let mut words: Vec<String> = shown.iter().filter_map(|j| locators.get(*j).cloned()).collect();
            if count > MAX_LISTED_PARTNERS {
                words.push(format!("+{}", count - MAX_LISTED_PARTNERS));
            }
            words.join(" ")
        };
    }
    out
}
