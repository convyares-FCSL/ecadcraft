//! Derived results: recomputed from the document, never stored (principle P5).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::document::book::Book;
use crate::document::doc::{Document, End, Form, ItemKind, Primitive, ReferenceStyle, Representation, Subject};
use crate::document::sheet::Grid;
use crate::geometry::geom::Pt;
use crate::naming::id::{CableId, ConnectionPointId, ItemId, PageId, RepresentationId, RouteId, WireId};

/// Where each terminal sits on one page, from drawn connection points and symbol placements.
///
/// # Semantics
/// Symbol terminal positions are computed from the stored placement and the embedded
/// definition's local terminal position (proposal §4.3), or the representation's connection
/// points in the definition's local frame when it has them (brief 018). A terminal shown by several
/// representations on the same page appears once per representation.
pub fn connection_point_positions(doc: &Document, page: PageId) -> Vec<(ConnectionPointId, RepresentationId, Pt)> {
    let mut out = Vec::new();
    for (rid, r) in doc.representations.iter().filter(|(_, r)| r.page == page) {
        out.extend(shown_by(doc, r).into_iter().map(|(t, p)| (t, *rid, p)));
    }
    out
}

/// Where `connection_point` is shown on `page`: the first entry for it in [`connection_point_positions`], found
/// through its component's representations only, so the cost does not grow with the page or
/// the project.
///
/// # Semantics
/// The same answer as `connection_point_positions(doc, page)`'s first entry for `connection_point` on any valid
/// document, where a representation shows only its own component's terminals (a drawn
/// representation's connection points are its component's terminals). Both read a
/// representation through the same helper, so they cannot drift apart.
pub fn connection_point_position(doc: &Document, page: PageId, connection_point: ConnectionPointId) -> Option<Pt> {
    let subject = match doc.connection_points.get(&connection_point) {
        Some(t) => t.owner.subject(),
        None => Subject::Cable(doc.cable_point(connection_point)?.0),
    };
    doc.representations_of(subject)
        .filter(|(_, r)| r.page == page)
        .find_map(|(_, r)| shown_by(doc, r).into_iter().find(|(t, _)| *t == connection_point).map(|(_, p)| p))
}

/// The terminals a component representation shows, with their positions, in the order
/// [`connection_point_positions`] lists them.
fn shown_by(doc: &Document, r: &Representation) -> Vec<(ConnectionPointId, Pt)> {
    // A cable reference shows its cores' named points (design 072 §4.4, F4).
    if let Subject::Cable(_) = r.subject {
        return r.connection_points.iter().map(|(t, p)| (*t, *p)).collect();
    }
    let owner: crate::document::doc::ConnectionPointOwner = match r.subject {
        Subject::Device(d) => d.into(),
        Subject::Aid(a) => a.into(),
        _ => return Vec::new(),
    };
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
            doc.connection_points_of(owner)
                .filter_map(|(tid, t)| {
                    let td = t.key.as_ref().and_then(|k| def.connection_points.iter().find(|d| &d.key == k))?;
                    Some((tid, inst.placement.apply(td.at)))
                })
                .collect()
        }
    }
}

/// The route of a conductor on a page: the first route there that carries it
/// ([`crate::connectivity::wire_order::wire_of_route`], design 072 §4: routes draw, wires run between their
/// connection points), found through the wire's points, and its drawing's first item path.
pub fn route_on_page(doc: &Document, conductor: WireId, page: PageId) -> Option<(crate::naming::id::ItemId, Primitive)> {
    let wire = doc.wires.get(&conductor)?;
    let routes: BTreeSet<crate::naming::id::RouteId> = wire
        .ends
        .iter()
        .flat_map(|t| doc.routes_at(*t))
        .filter(|r| doc.routes.get(r).is_some_and(|x| x.page == page) && crate::connectivity::wire_order::wire_of_route(doc, *r) == Some(conductor))
        .collect();
    routes.into_iter().find_map(|r| route_drawing(doc, r, page))
}

/// Where wire `w` starts: the end of a route at its first connection point (at its second when
/// no route reaches the first), on the earliest page in book order (design 072 §4.3: routes draw,
/// wires run between their connection points). `None` when no route reaches either point.
pub fn wire_start(doc: &Document, w: WireId) -> Option<(PageId, Pt)> {
    let wire = doc.wires.get(&w)?;
    wire.ends.iter().find_map(|t| {
        doc.routes_at(*t)
            .filter_map(|r| doc.routes.get(&r))
            .filter_map(|r| {
                let at = if r.ends[0].point() == Some(*t) { r.points.first() } else { r.points.last() }?;
                let i = doc.page_order.iter().position(|p| *p == r.page).unwrap_or(usize::MAX);
                Some((i, r.page, *at))
            })
            .min_by_key(|x| x.0)
            .map(|(_, page, at)| (page, at))
    })
}

/// The first item path of route `r`'s drawing on `page`.
fn route_drawing(doc: &Document, r: crate::naming::id::RouteId, page: PageId) -> Option<(crate::naming::id::ItemId, Primitive)> {
    doc.representations_of(Subject::Route(r)).filter(|(_, rep)| rep.page == page).find_map(|(_, rep)| {
        let id = *rep.items.first()?;
        match &doc.items.get(&id)?.kind {
            ItemKind::Primitive(p) if p.path_ends().is_some() => Some((id, p.clone())),
            _ => None,
        }
    })
}

/// An attached conductor end whose drawn route end is not at its terminal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mismatch {
    pub route: RouteId,
    /// End A is the route's first point, end B its last.
    pub end: End,
    pub connection_point: ConnectionPointId,
    pub page: PageId,
    pub route_end: Pt,
    pub connection_point_at: Pt,
}

/// Route ends on a connection point whose drawn end is not where that point is shown (DOC-03 check).
///
/// # Semantics
/// Only pages where both the route and the terminal are shown are compared; an attachment to a
/// terminal not shown on the route's page is not a mismatch. Positions compare within `tol`.
pub fn mismatches(doc: &Document, tol: f64) -> Vec<Mismatch> {
    let mut out = Vec::new();
    let mut positions: BTreeMap<PageId, Vec<(ConnectionPointId, RepresentationId, Pt)>> = BTreeMap::new();
    for page in &doc.page_order {
        positions.insert(*page, connection_point_positions(doc, *page));
    }
    for (rid, r) in &doc.routes {
        let Some([a, b]) = r.end_points() else { continue };
        for ((end, at), e) in [(End::A, a), (End::B, b)].into_iter().zip(r.ends) {
            let Some(t) = e.point() else { continue };
            let shown: Vec<Pt> = positions.get(&r.page).into_iter().flatten().filter(|(pt, _, _)| *pt == t).map(|(_, _, p)| *p).collect();
            if !shown.is_empty() && !shown.iter().any(|p| p.near(at, tol)) {
                let connection_point_at = shown.first().copied().unwrap_or_default();
                out.push(Mismatch { route: *rid, end, connection_point: t, page: r.page, route_end: at, connection_point_at });
            }
        }
    }
    out
}

/// A connected set of terminals and conductors (derived net; identity not stable, §15).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Net {
    pub connection_points: Vec<ConnectionPointId>,
    pub wires: Vec<WireId>,
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
    for (cid, c) in &doc.wires {
        let ci = id_of(Node::Wire(*cid), &mut nodes, &mut parent);
        for t in &c.ends {
            let ti = id_of(Node::ConnectionPoint(*t), &mut nodes, &mut parent);
            let (a, b) = (find(&mut parent, ci), find(&mut parent, ti));
            if let Some(slot) = parent.get_mut(a) {
                *slot = b;
            }
        }
    }
    let mut groups: BTreeMap<usize, Net> = BTreeMap::new();
    for (i, n) in nodes.iter().enumerate() {
        let root = find(&mut parent, i);
        let net = groups.entry(root).or_insert_with(|| Net { connection_points: Vec::new(), wires: Vec::new() });
        match n {
            Node::ConnectionPoint(t) => net.connection_points.push(*t),
            Node::Wire(c) => net.wires.push(*c),
        }
    }
    let mut out: Vec<Net> = groups.into_values().collect();
    for n in &mut out {
        n.connection_points.sort();
        n.wires.sort();
    }
    out.sort_by(|a, b| a.wires.cmp(&b.wires));
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Node {
    ConnectionPoint(ConnectionPointId),
    Wire(WireId),
}

// ---------------------------------------------------------------- cable references (brief 010)

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
/// ([`crate::document::book`]), the column and row come from the page's sheet ([`crate::document::sheet`]), the
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
pub fn route_ends_by_page(doc: &Document) -> BTreeMap<(WireId, PageId), (Pt, Pt)> {
    let mut out = BTreeMap::new();
    for (w, wire) in &doc.wires {
        let pages: BTreeSet<PageId> = wire.ends.iter().flat_map(|t| doc.routes_at(*t)).filter_map(|r| doc.routes.get(&r).map(|x| x.page)).collect();
        for page in pages {
            if let Some(e) = route_on_page(doc, *w, page).and_then(|(_, p)| p.path_ends()) {
                out.insert((*w, page), e);
            }
        }
    }
    out
}

/// The end points of `conductor`'s route on `page` (as [`route_on_page`] and
/// [`route_ends_by_page`] pick it), found through the wire's points.
pub fn route_ends_on(doc: &Document, conductor: WireId, page: PageId) -> Option<(Pt, Pt)> {
    route_on_page(doc, conductor, page).and_then(|(_, p)| p.path_ends())
}

/// Every cable reference in the document, in page order, then column, then ID (design 010 §6.2,
/// §7.4). Cost is one pass over the representations plus the referenced cables' members (their
/// routes are found through the subject index, brief 014); never panics on hostile values (they
/// read `/?`).
pub fn cable_references(doc: &Document) -> Vec<CableReference> {
    let references: Vec<(RepresentationId, &Representation)> = doc
        .representations
        .iter()
        .filter(|(_, r)| r.role == crate::Role::Reference && matches!(r.subject, Subject::Cable(_)))
        .map(|(id, r)| (*id, r))
        .collect();
    references_from(doc, references)
}

/// The references of `cables` only, exactly as [`cable_references`] derives them (same values,
/// same order): everything a reference says depends only on its own cable (its other references,
/// its members' routes) and on the page order and length unit. Cost is in proportion to those
/// cables' references and members, not to the document (brief 014).
pub fn cable_references_of(doc: &Document, cables: &BTreeSet<CableId>) -> Vec<CableReference> {
    let references: Vec<(RepresentationId, &Representation)> = cables
        .iter()
        .flat_map(|k| doc.representations_of(Subject::Cable(*k)))
        .filter(|(_, r)| r.role == crate::Role::Reference && matches!(r.subject, Subject::Cable(_)))
        .collect();
    references_from(doc, references)
}

/// [`cable_references`] of the reference representations `references` (each listed once).
fn references_from(doc: &Document, references: Vec<(RepresentationId, &Representation)>) -> Vec<CableReference> {
    if references.is_empty() {
        return Vec::new();
    }
    let style = doc.settings.reference_style();
    let book = Book::of(doc);
    // Each page's grid and shown number, worked out once per page.
    let mut pages: BTreeMap<PageId, (Grid, String)> = BTreeMap::new();
    // Route ends per (conductor, page), looked up once each (a cable may list a conductor twice).
    let mut routes: BTreeMap<(WireId, PageId), Option<(Pt, Pt)>> = BTreeMap::new();
    // The member routes of each (cable, page) with a reference, built once per pair.
    let mut here: BTreeMap<(CableId, PageId), Vec<(Pt, Pt)>> = BTreeMap::new();
    for (_, r) in &references {
        let Subject::Cable(cable) = r.subject else { continue };
        if here.contains_key(&(cable, r.page)) {
            continue;
        }
        let members = doc.cables.get(&cable).map(|c| c.cores.as_slice()).unwrap_or_default();
        let line = |route: &crate::objects::route::Route| route.end_points().filter(|_| route.page == r.page).map(|[a, b]| (a, b));
        let ends: Vec<(Pt, Pt)> = members
            .iter()
            .filter_map(|m| {
                // The line landing on the core's named point on this page (F4), else its wire's
                // route here, else the loose line it sits on.
                let through = m.through.iter().flat_map(|t| doc.routes_at(*t)).filter_map(|x| doc.routes.get(&x)).find_map(line);
                through.or_else(|| match m.position {
                    crate::document::doc::CorePosition::Wire(w) => *routes.entry((w, r.page)).or_insert_with(|| route_ends_on(doc, w, r.page)),
                    crate::document::doc::CorePosition::Route { route, .. } => doc.routes.get(&route).and_then(line),
                    crate::document::doc::CorePosition::Unplaced => None,
                })
            })
            .collect();
        here.insert((cable, r.page), ends);
    }
    let mut out: Vec<CableReference> = Vec::with_capacity(references.len());
    for (rid, r) in &references {
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
