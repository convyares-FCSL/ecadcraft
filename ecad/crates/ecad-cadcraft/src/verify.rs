//! Recovery-path verification: does a rebuilt page show the authoritative document?
//!
//! Two checks guard projections, and they answer different questions:
//!
//! - **Normal path (fidelity).** After a CADCraft command commits, the page is reprojected with the
//!   engine's handles kept, and each entity is compared *by handle* with what the engine
//!   committed (`FidelityMismatch` events, hypothesis H2). Handles are the identity there.
//! - **Recovery path (this module).** After a rebuild (fresh handles) or an owner recovery, handle
//!   numbers mean nothing: they were all reissued. [`verify_page`] decodes the drawing back into
//!   ECAD terms and compares it with the document by content: every item's geometry and style,
//!   each symbol's placement, every terminal position (drawn markers and the markers inside each
//!   symbol block, transformed by the placement), group membership, the handle map, and the
//!   relationships the page shows (attached conductor ends sit on their terminals). Handles are
//!   used only as the drawing's internal references (map entries, group members), never compared
//!   with earlier values.
//!
//! The domain relationships themselves (components, terminals, attachments, cables) live in the
//! ledger, which recovery never writes; tests compare the document and head before and after.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use cadcraft_engine::doc::{Drawing, Entity, EntityKind, Handle};
use ecad_model::{Document, End, Form, ItemKind, PageId, Pt, Subject};

use crate::convert::{ECAD_PREFIX, block_name, item_from, kind_for_primitive, marker_position};
use crate::project::{HandleMap, Mapped};

/// What [`verify_page`] compared and what disagreed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Verification {
    pub items: usize,
    pub terminals: usize,
    pub groups: usize,
    pub attached_ends: usize,
    /// Every disagreement, in words. Empty when the page shows the document.
    pub problems: Vec<String>,
}

impl Verification {
    pub fn ok(&self) -> bool {
        self.problems.is_empty()
    }

    /// One line for the event log.
    pub fn summary(&self) -> String {
        let counts = format!(
            "{} items, {} terminal positions, {} groups, {} attached ends",
            self.items, self.terminals, self.groups, self.attached_ends
        );
        match self.problems.first() {
            None => format!("rebuilt page agrees with the document: {counts}"),
            Some(p) => format!("{} problems ({counts}); first: {p}", self.problems.len()),
        }
    }
}

const TOL: f64 = 1e-9;

fn near(a: Pt, b: Pt) -> bool {
    (a.x - b.x).abs() <= TOL && (a.y - b.y).abs() <= TOL
}

/// Terminal positions of an insert of an ECAD block, read from the drawing: the block's marker
/// points mapped by the insert's own scale, rotation and position.
fn block_terminals(d: &Drawing, e: &Entity) -> Vec<Pt> {
    let EntityKind::Insert(i) = &e.kind else { return Vec::new() };
    let Some(b) = d.blocks.get(&i.block) else { return Vec::new() };
    let (s, c) = i.rotation.sin_cos();
    b.entities
        .iter()
        .filter_map(|m| marker_position(m))
        .map(|p| {
            let (x, y) = (p.x * i.scale.x, p.y * i.scale.y);
            Pt::new(i.insert.x + c * x - s * y, i.insert.y + s * x + c * y)
        })
        .collect()
}

/// Sorted multiset of points, for order-free comparison.
fn sorted(mut v: Vec<Pt>) -> Vec<Pt> {
    v.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    v
}

fn same_points(a: Vec<Pt>, b: Vec<Pt>) -> bool {
    let (a, b) = (sorted(a), sorted(b));
    a.len() == b.len() && a.iter().zip(&b).all(|(p, q)| near(*p, *q))
}

/// Check that `drawing` (with `map`) shows `page` of `doc`, without relying on handle numbers.
pub fn verify_page(doc: &Document, page: PageId, drawing: &Drawing, map: &HandleMap) -> Verification {
    let mut v = Verification::default();
    let mut problems: Vec<String> = Vec::new();
    let entities: HashMap<Handle, &Entity> = drawing.model.iter().map(|e| (e.handle, e.as_ref())).collect();

    // 1. Content: the drawing's entities, decoded, are exactly the page's items (as a multiset).
    let mut shown: BTreeMap<String, usize> = BTreeMap::new();
    let mut markers_shown = Vec::new();
    for e in entities.values() {
        if let Some(p) = marker_position(e) {
            markers_shown.push(p);
            continue;
        }
        match item_from(doc, page, e) {
            Ok(item) => *shown.entry(serde_json::to_string(&item).unwrap_or_default()).or_default() += 1,
            Err(m) => problems.push(format!("an entity cannot be read back as an item: {m}")),
        }
    }
    let mut expected: BTreeMap<String, usize> = BTreeMap::new();
    for item in doc.items.values().filter(|i| i.page == page) {
        *expected.entry(serde_json::to_string(item).unwrap_or_default()).or_default() += 1;
    }
    let items: usize = expected.values().sum();
    if shown != expected {
        let missing = expected.iter().filter(|(k, n)| shown.get(*k) != Some(n)).count();
        problems.push(format!("drawn items differ from the document's items ({missing} kinds of item differ)"));
    }

    // 2. Identity: each mapped handle shows the item (or marker) it is mapped to, and the map
    //    covers the drawing one-to-one.
    if !map.is_consistent() {
        problems.push("the handle map is not one-to-one".into());
    }
    if map.len() != entities.len() {
        problems.push(format!("{} entities but {} mapped handles", entities.len(), map.len()));
    }
    for (id, item) in doc.items.iter().filter(|(_, i)| i.page == page) {
        match map.item(*id).and_then(|h| entities.get(&h)) {
            Some(e) if item_from(doc, page, e).is_ok_and(|x| x == *item) => {}
            Some(_) => problems.push(format!("item {id} is mapped to an entity that shows something else")),
            None => problems.push(format!("item {id} is not drawn")),
        }
    }

    // 3. Terminal positions: drawn markers where the connection points are, and symbol terminals
    //    (from the block's markers and the insert) where the definition and placement put them.
    let mut expected_markers = Vec::new();
    for (rid, r) in doc.representations.iter().filter(|(_, r)| r.page == page) {
        match r.form {
            Form::Drawn => {
                for (t, at) in &r.connection_points {
                    expected_markers.push(*at);
                    v.terminals += 1;
                    match map.marker(*rid, *t).and_then(|h| entities.get(&h)).and_then(|e| marker_position(e)) {
                        Some(p) if near(p, *at) => {}
                        _ => {
                            problems.push(format!("terminal {t} of representation {rid} is not marked at its position"))
                        }
                    }
                }
            }
            Form::Symbol => {
                let Some(item) = r.items.first() else { continue };
                let Some(e) = map.item(*item).and_then(|h| entities.get(&h)) else { continue };
                let Some(ItemKind::Symbol(inst)) = doc.items.get(item).map(|i| &i.kind) else { continue };
                if !matches!(&e.kind, EntityKind::Insert(i) if i.block == block_name(inst.definition)) {
                    problems.push(format!("symbol of representation {rid} is not an insert of its definition's block"));
                    continue;
                }
                let want: Vec<Pt> = ecad_model::derive::terminal_positions(doc, page)
                    .into_iter()
                    .filter(|(_, rep, _)| rep == rid)
                    .map(|(_, _, p)| p)
                    .collect();
                let got = block_terminals(drawing, e);
                v.terminals += want.len();
                // The block shows every terminal of the definition; the component may use a subset.
                if !want.iter().all(|w| got.iter().any(|g| near(*g, *w))) {
                    problems.push(format!(
                        "symbol of representation {rid} does not show its terminals where the document puts them"
                    ));
                }
            }
        }
    }
    if !same_points(markers_shown, expected_markers) {
        problems.push("terminal markers in the drawing differ from the document's connection points".into());
    }

    // 4. Blocks: one per definition used on the page, with the definition's graphics.
    for (name, b) in drawing.blocks.iter().filter(|(n, _)| n.starts_with(ECAD_PREFIX)) {
        let Some(def) = crate::convert::definition_of_block(name).and_then(|id| doc.definitions.get(&id)) else {
            problems.push(format!("block {name} has no definition in the document"));
            continue;
        };
        let graphics: Vec<EntityKind> =
            b.entities.iter().filter(|e| marker_position(e).is_none()).map(|e| e.kind.clone()).collect();
        let want: Vec<EntityKind> = def.graphics.iter().map(kind_for_primitive).collect();
        if graphics != want {
            problems.push(format!("block {name} does not show its definition's graphics"));
        }
        let terms: Vec<Pt> = b.entities.iter().filter_map(|e| marker_position(e)).collect();
        if !same_points(terms, def.terminals.iter().map(|t| t.at).collect()) {
            problems.push(format!("block {name} does not mark its definition's terminals"));
        }
    }

    // 5. Groups: each drawn component with several parts is one group of exactly those parts.
    let groups: HashMap<&str, BTreeSet<Handle>> = drawing
        .groups
        .iter()
        .filter(|g| g.name.starts_with(ECAD_PREFIX))
        .map(|g| (g.name.as_str(), g.members.iter().copied().collect()))
        .collect();
    let mut wanted_groups = 0;
    for (rid, r) in doc.representations.iter().filter(|(_, r)| r.page == page && r.form == Form::Drawn) {
        let mut members: BTreeSet<Handle> = r.items.iter().filter_map(|i| map.item(*i)).collect();
        members.extend(r.connection_points.keys().filter_map(|t| map.marker(*rid, *t)));
        if !matches!(r.subject, Subject::Component(_)) || members.len() < 2 {
            continue;
        }
        wanted_groups += 1;
        let name = format!("{ECAD_PREFIX}{:032x}", rid.0);
        let ok = groups.get(name.as_str()).is_some_and(|g| *g == members);
        if !ok {
            problems.push(format!("component representation {rid} is not grouped with exactly its parts"));
        }
        // Every member is a part of this representation according to the map.
        for h in &members {
            match map.get(*h) {
                Some(Mapped::Item(i)) if r.items.contains(&i) => {}
                Some(Mapped::Marker(x, _)) if x == *rid => {}
                _ => problems.push(format!("group of {rid} holds a handle that is not one of its parts")),
            }
        }
    }
    if groups.len() != wanted_groups {
        problems.push(format!("{} ECAD groups in the drawing, {wanted_groups} expected", groups.len()));
    }
    v.groups = wanted_groups;

    // 6. Relationships shown on the page: an attached conductor end drawn here ends on a terminal
    //    position read from the drawing (markers and symbol blocks), not from the document.
    let mut drawn_terminals: Vec<Pt> = entities.values().filter_map(|e| marker_position(e)).collect();
    drawn_terminals.extend(entities.values().flat_map(|e| block_terminals(drawing, e)));
    for (cid, c) in &doc.conductors {
        let Some((item, _)) = ecad_model::derive::route_on_page(doc, *cid, page) else { continue };
        let Some(e) = map.item(item).and_then(|h| entities.get(&h)) else { continue };
        let Some(path) = item_from(doc, page, e).ok().and_then(|i| match i.kind {
            ItemKind::Primitive(p) => p.path_ends(),
            ItemKind::Symbol(_) => None,
        }) else {
            problems.push(format!("route of conductor {cid} is not a path in the drawing"));
            continue;
        };
        for end in End::BOTH {
            let Some(t) = c.end(end) else { continue };
            let shown_here = ecad_model::derive::terminal_positions(doc, page).iter().any(|(x, _, _)| *x == t);
            if !shown_here {
                continue;
            }
            v.attached_ends += 1;
            let at = if end == End::A { path.0 } else { path.1 };
            if !drawn_terminals.iter().any(|p| near(*p, at)) {
                problems.push(format!("conductor {cid} end {end:?} is attached but not drawn on a terminal"));
            }
        }
    }
    v.items = items;
    v.problems = problems;
    v
}
