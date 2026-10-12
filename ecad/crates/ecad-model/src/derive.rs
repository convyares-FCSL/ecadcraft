//! Derived results: recomputed from the document, never stored (principle P5).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::doc::{Document, End, Form, ItemKind, Primitive, Subject};
use crate::geom::Pt;
use crate::id::{ConductorId, PageId, RepresentationId, TerminalId};

/// Where each terminal sits on one page, from drawn connection points and symbol placements.
///
/// # Semantics
/// Symbol terminal positions are computed from the stored placement and the embedded
/// definition's local terminal position (proposal §4.3). A terminal shown by several
/// representations on the same page appears once per representation.
pub fn terminal_positions(doc: &Document, page: PageId) -> Vec<(TerminalId, RepresentationId, Pt)> {
    let mut out = Vec::new();
    for (rid, r) in doc.representations.iter().filter(|(_, r)| r.page == page) {
        let Subject::Component(component) = r.subject else { continue };
        match r.form {
            Form::Drawn => out.extend(r.connection_points.iter().map(|(t, p)| (*t, *rid, *p))),
            Form::Symbol => {
                let Some(ItemKind::Symbol(inst)) = r.items.first().and_then(|i| doc.items.get(i)).map(|i| &i.kind)
                else {
                    continue;
                };
                let Some(def) = doc.definitions.get(&inst.definition) else { continue };
                for (tid, t) in doc.terminals_of(component) {
                    if let Some(td) = t.key.as_ref().and_then(|k| def.terminals.iter().find(|d| &d.key == k)) {
                        out.push((tid, *rid, inst.placement.apply(td.at)));
                    }
                }
            }
        }
    }
    out
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
