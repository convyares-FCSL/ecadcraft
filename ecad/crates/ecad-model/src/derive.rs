//! Derived results: recomputed from the document, never stored (principle P5).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::doc::{Document, End, Form, ItemKind, LengthUnit, Primitive, Representation, Subject};
use crate::geom::Pt;
use crate::id::{CableId, ConductorId, ItemId, PageId, RepresentationId, TerminalId};

/// Where each terminal sits on one page, from drawn connection points and symbol placements.
///
/// # Semantics
/// Symbol terminal positions are computed from the stored placement and the embedded
/// definition's local terminal position (proposal §4.3). A terminal shown by several
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
                let shown: Vec<Pt> = positions
                    .get(&r.page)
                    .into_iter()
                    .flatten()
                    .filter(|(pt, _, _)| *pt == t)
                    .map(|(_, _, p)| *p)
                    .collect();
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

/// Width of the fixed column grid, counted from x = 0, in millimetres (`ecad.column-width`, design
/// 010 §6.2), until drawing frames define their own columns.
pub const COLUMN_WIDTH_MM: f64 = 40.0;

/// The highest column a reference text names; larger values are clamped to it.
pub const MAX_COLUMN: u32 = 999;

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

/// The column grid's width in the project's length unit.
pub fn column_width(doc: &Document) -> f64 {
    match doc.settings.length_unit {
        LengthUnit::Millimetre => COLUMN_WIDTH_MM,
        LengthUnit::Inch => COLUMN_WIDTH_MM / 25.4,
    }
}

/// The column of `x` on a grid of `width` (1-based, clamped to 1..=[`MAX_COLUMN`]); `None` when it
/// cannot be located (`x` not finite, or a width that is not positive).
pub fn column_of(x: f64, width: f64) -> Option<u32> {
    if !(x.is_finite() && width.is_finite() && width > 0.0) {
        return None;
    }
    let c = ((x / width).floor() + 1.0).clamp(1.0, f64::from(MAX_COLUMN));
    // Finite and within 1..=999 here, so the conversion is exact.
    Some(c as u32)
}

/// One cable reference end, with everything it says derived from the document (design 010 §6).
///
/// # Semantics
/// Nothing here is stored: the page number is the reference page's 1-based position in
/// `page_order`, the column comes from the fixed grid ([`column_of`]), the partners are the
/// cable's other references, and the text names the partners. All of it changes when pages are
/// reordered or a reference point moves, with no transaction of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct CableReference {
    /// The reference representation.
    pub reference: RepresentationId,
    pub cable: CableId,
    pub page: PageId,
    /// The 1-based position of `page` in the page order (`None` for a page in no order: hostile).
    pub page_number: Option<usize>,
    /// The Point item, when the representation holds one.
    pub item: Option<ItemId>,
    /// Where the reference is, when it can be located (a finite Point item).
    pub at: Option<Pt>,
    /// The column of `at`.
    pub column: Option<u32>,
    /// The cable's other references, in page order, then column, then ID: at most
    /// [`MAX_LISTED_PARTNERS`] of them (the count is [`CableReference::partner_count`]).
    pub partners: Vec<RepresentationId>,
    /// How many other references the cable has.
    pub partner_count: usize,
    /// `/<page>.<column>` of each partner (at most [`MAX_LISTED_PARTNERS`], then `+N`), separated
    /// by a space; `/?` for an orphan, and for a partner that cannot be located.
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
    /// `/<page>.<column>` of this end (`/?` when it cannot be located), as its partners show it.
    pub fn locator(&self) -> String {
        match (self.page_number, self.column) {
            (Some(n), Some(c)) => format!("/{n}.{c}"),
            _ => "/?".into(),
        }
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

/// Every cable reference in the document, in page order, then column, then ID (design 010 §6.2,
/// §7.4). Cost is linear in the document plus the referenced cables' members; never panics on
/// hostile values (they read `/?`).
pub fn cable_references(doc: &Document) -> Vec<CableReference> {
    let refs: Vec<(RepresentationId, &Representation)> = doc
        .representations
        .iter()
        .filter(|(_, r)| r.role == REFERENCE_ROLE && matches!(r.subject, Subject::Cable(_)))
        .map(|(id, r)| (*id, r))
        .collect();
    if refs.is_empty() {
        return Vec::new();
    }
    let width = column_width(doc);
    let order: BTreeMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i + 1)).collect();
    let routes = route_ends_by_page(doc);
    // The member routes of each (cable, page) with a reference, built once per pair.
    let mut here: BTreeMap<(CableId, PageId), Vec<(Pt, Pt)>> = BTreeMap::new();
    for (_, r) in &refs {
        let Subject::Cable(cable) = r.subject else { continue };
        here.entry((cable, r.page)).or_insert_with(|| {
            let members = doc.cables.get(&cable).map(|c| c.members.as_slice()).unwrap_or_default();
            members.iter().filter_map(|m| routes.get(&(m.conductor, r.page)).copied()).collect()
        });
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
        out.push(CableReference {
            reference: *rid,
            cable,
            page: r.page,
            page_number: order.get(&r.page).copied(),
            item,
            at,
            column: at.and_then(|p| column_of(p.x, width)),
            partners: Vec::new(),
            partner_count: 0,
            text: String::new(),
            orphan: true,
            wires_here,
            text_dir,
        });
    }
    // Page order (a page in no order last), then column, then ID.
    out.sort_by_key(|r| (r.page_number.unwrap_or(usize::MAX), r.column.unwrap_or(u32::MAX), r.reference));
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
            "/?".into()
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
