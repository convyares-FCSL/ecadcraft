//! Recovery-path verification: does a rebuilt page show the authoritative document?
//!
//! Two checks guard projections, and they answer different questions:
//!
//! - **Normal path (fidelity).** After a CADCraft command commits, the page is reprojected with the
//!   engine's handles kept, and each entity is compared *by handle* with what the engine
//!   committed (`FidelityMismatch` events). Handles are the identity there.
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
use ecad_model::{CableId, Document, End, Form, ItemId, ItemKind, PageId, Pt, Subject};

use crate::convert::{ECAD_PREFIX, block_name, item_from, kind_for_primitive, marker_position};
use crate::project::{HandleMap, Mapped, part_entity};
use ecad_electrical::reports::kept::ReportModels;

/// What [`verify_page`] compared and what disagreed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Verification {
    pub items: usize,
    pub connection_points: usize,
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
            "{} items, {} connection points, {} groups, {} attached ends",
            self.items, self.connection_points, self.groups, self.attached_ends
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
fn block_connection_points(d: &Drawing, e: &Entity) -> Vec<Pt> {
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
/// Pages drawn from the report models are expected with `reports`
/// ([`crate::pages::generated::parts_of`]).
pub fn verify_page(doc: &Document, page: PageId, drawing: &Drawing, map: &HandleMap, reports: Option<&ReportModels>) -> Verification {
    verify_page_with(doc, page, drawing, map, &BTreeSet::new(), 0.0, reports)
}

/// [`verify_page`] for a page projected with cable views: the cables in `fan` whose
/// fan can be drawn on the page must show exactly their fan ([`crate::cables::views::fan_on_page`] with
/// tolerance `tol`) on the reserved layer, and the items the fan replaces must not be drawn.
pub(crate) fn verify_page_with(
    doc: &Document, page: PageId, drawing: &Drawing, map: &HandleMap, fan: &BTreeSet<CableId>, tol: f64, reports: Option<&ReportModels>,
) -> Verification {
    verify_page_shown(doc, page, drawing, map, fan, tol, false, reports)
}

/// [`verify_page_with`] for a page drawn with Show Hidden Text on or off:
/// with it on, the hidden derived texts are expected too, dimmed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn verify_page_shown(
    doc: &Document, page: PageId, drawing: &Drawing, map: &HandleMap, fan: &BTreeSet<CableId>, tol: f64, hidden_text: bool,
    reports: Option<&ReportModels>,
) -> Verification {
    verify_page_pending(doc, page, drawing, map, fan, tol, hidden_text, false, 0, reports)
}

/// [`verify_page_shown`] for a project whose title block marks a pending revision (the frame is
/// expected with the project's pending flag) and whose generated page shows sheet `sheet`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn verify_page_pending(
    doc: &Document, page: PageId, drawing: &Drawing, map: &HandleMap, fan: &BTreeSet<CableId>, tol: f64, hidden_text: bool, pending: bool,
    sheet: usize, reports: Option<&ReportModels>,
) -> Verification {
    let mut v = Verification::default();
    let fans = crate::cables::views::drawings_on_page(doc, page, fan, tol);
    let hidden: BTreeSet<ItemId> = fans.values().flat_map(|f| f.hidden.iter().copied()).collect();
    let mut fan_shown: BTreeMap<String, usize> = BTreeMap::new();
    let mut fan_expected: BTreeMap<String, usize> = BTreeMap::new();
    let mut tees_shown: Vec<(ecad_model::RouteId, Pt)> = Vec::new();
    let draw = crate::styles::draw::DrawStyles::new(doc);
    for f in fans.values() {
        for part in &f.parts {
            let e = part_entity(&draw, part, Handle(0));
            *fan_expected.entry(computed_key(&e)).or_default() += 1;
        }
    }
    // Device tags and terminal labels, derived like the cable parts.
    for t in crate::devices::draw::all_texts_on_page(doc, page) {
        if !t.shown && !hidden_text {
            continue;
        }
        let mut e = crate::devices::draw::text_entity(&draw, &t.text, Handle(0));
        if !t.shown {
            crate::styles::draw::dim_hidden(&mut e);
        }
        *fan_expected.entry(computed_key(&e)).or_default() += 1;
    }
    // Device cross-references, derived the same way.
    for part in crate::references::draw::devices::parts_on_page(doc, page) {
        let mut e = crate::project::device_reference_entity(&draw, &part, Handle(0));
        // A continuation's other-end text in Ink 2, as the page draws it.
        crate::continuations::draw::colour(doc, &part, &mut e);
        *fan_expected.entry(computed_key(&e)).or_default() += 1;
    }
    // PLC card frames, derived the same way.
    for part in crate::plc::draw::card::parts_on_page(doc, page) {
        *fan_expected.entry(computed_key(&crate::project::device_reference_entity(&draw, &part, Handle(0)))).or_default() += 1;
    }
    // Link lines, derived the same way.
    for part in ecad_electrical::terminal_strips::jumper_bars::parts_on_page(doc, page) {
        *fan_expected.entry(computed_key(&crate::project::jumper_bar_entity(&part, Handle(0)))).or_default() += 1;
    }
    // Location box labels, derived the same way.
    for (rep, value, at) in ecad_electrical::locations::labels_on_page(doc, page) {
        *fan_expected.entry(computed_key(&crate::locations::draw::styled_label_entity(doc, &draw, rep, &value, at, Handle(0)))).or_default() += 1;
    }
    // Wire labels, derived the same way.
    for l in crate::wires::draw::labels_on_page_with(doc, page, &hidden) {
        *fan_expected.entry(computed_key(&crate::wires::draw::label_entity(&draw, &l, Handle(0)))).or_default() += 1;
    }
    // The sheet frame, derived like the cable parts.
    // Generated pages and Total in sheets come from the same function.
    let frame_rgb = crate::pages::frame::line_colour(doc, page);
    for part in crate::pages::generated::shown_parts(doc, page, reports, pending, sheet) {
        *fan_expected.entry(computed_key(&crate::pages::frame::styled_entity(&part, Handle(0), frame_rgb))).or_default() += 1;
    }
    let mut problems: Vec<String> = Vec::new();
    let entities: HashMap<Handle, &Entity> = drawing.model.iter().map(|e| (e.handle, e.as_ref())).collect();
    // A two-colour wire's dashes, derived from its route like the labels.
    let budget = ecad_electrical::cables::words::dash_budget(doc, page);
    for r in crate::cables::draw::coloured_routes(doc, page, Some(map), crate::cables::draw::projection_sheet()) {
        if !budget.dashed.contains(&r.rep) {
            continue;
        }
        let Some(route) = r.handle.and_then(|h| entities.get(&h)) else { continue };
        for e in crate::wires::draw::route_dashes(&r, &route.common) {
            *fan_expected.entry(computed_key(&e)).or_default() += 1;
        }
    }
    // A route drawn in its wire colour reads back as its item in the item's own colour.
    let plain = |e: &Entity| -> Entity {
        let mut e = e.clone();
        if map.is_coloured(e.handle)
            && let Some(Mapped::Item(id)) = map.get(e.handle)
            && let Some(own) = doc.items().get(&id).and_then(|item| crate::convert::entity_for(doc, item, e.handle))
        {
            e.common.color = own.common.color;
        }
        // A location box outline drawn in its location style reads back as its item;
        // A wire route drawn in its category's line type and weight likewise.
        if let Some(Mapped::Item(id)) = map.get(e.handle)
            && doc
                .representation_of_item(id)
                .is_some_and(|(_, r)| r.role == ecad_model::Role::LocationBox || matches!(r.subject, ecad_model::Subject::Route(_)))
            && let Some(own) = doc.items().get(&id).and_then(|item| crate::convert::entity_for(doc, item, e.handle))
        {
            e.common.color = own.common.color;
            e.common.linetype = own.common.linetype;
            e.common.lineweight = own.common.lineweight;
        }
        e
    };

    // 1. Content: the drawing's entities, decoded, are exactly the page's items (as a multiset).
    let mut shown: BTreeMap<String, usize> = BTreeMap::new();
    let mut markers_shown = Vec::new();
    for e in entities.values() {
        if let Some(p) = marker_position(e) {
            markers_shown.push(p);
            continue;
        }
        if let Some(Mapped::Fan(_)) = map.get(e.handle) {
            if e.common.layer != crate::cables::views::LAYER {
                problems.push(format!("a fan line is not on layer {}", crate::cables::views::LAYER));
            }
            *fan_shown.entry(computed_key(e)).or_default() += 1;
            continue;
        }
        // A reference end's text (the live text in the marker): one of the computed
        // parts, mapped to its reference.
        if let Some(Mapped::RefText(r)) = map.get(e.handle) {
            if !doc.representations().contains_key(&r) {
                problems.push(format!("a reference text is drawn for {r}, which does not exist"));
            }
            *fan_shown.entry(computed_key(e)).or_default() += 1;
            continue;
        }
        if let Some(Mapped::Label(c)) = map.get(e.handle) {
            if !doc.wires().contains_key(&c) {
                problems.push(format!("a label is drawn for {c}, which does not exist"));
            }
            *fan_shown.entry(computed_key(e)).or_default() += 1;
            continue;
        }
        // A T's dot: a ring at a point where its route ends on another route.
        if let Some(Mapped::Tee(r)) = map.get(e.handle) {
            match &e.kind {
                EntityKind::Circle(c) => tees_shown.push((r, Pt::new(c.center.x, c.center.y))),
                _ => problems.push(format!("a T dot of {r} is not a circle")),
            }
            continue;
        }
        if let Some(Mapped::Frame(_) | Mapped::JumperBar(..)) = map.get(e.handle) {
            *fan_shown.entry(computed_key(e)).or_default() += 1;
            continue;
        }
        if let Some(Mapped::DeviceReference(r)) = map.get(e.handle) {
            if !doc.representations().contains_key(&r) {
                problems.push(format!("a cross-reference is drawn for {r}, which does not exist"));
            }
            *fan_shown.entry(computed_key(e)).or_default() += 1;
            continue;
        }
        if let Some(Mapped::Tag(r)) = map.get(e.handle) {
            if !doc.representations().contains_key(&r) {
                problems.push(format!("a tag text is drawn for {r}, which does not exist"));
            }
            *fan_shown.entry(computed_key(e)).or_default() += 1;
            continue;
        }
        match item_from(doc, page, &plain(e)) {
            Ok(item) => *shown.entry(serde_json::to_string(&item).unwrap_or_default()).or_default() += 1,
            Err(m) => problems.push(format!("an entity cannot be read back as an item: {m}")),
        }
    }
    let mut expected: BTreeMap<String, usize> = BTreeMap::new();
    for item in doc.items().iter().filter(|(id, i)| i.page == page && !hidden.contains(id)).map(|(_, i)| i) {
        *expected.entry(serde_json::to_string(item).unwrap_or_default()).or_default() += 1;
    }
    let items: usize = expected.values().sum();
    if shown != expected {
        let missing = expected.iter().filter(|(k, n)| shown.get(*k) != Some(n)).count();
        problems.push(format!("drawn items differ from the document's items ({missing} kinds of item differ)"));
    }
    // T dots: one dot of rings per route end on another route, where it lands.
    let tees: Vec<(ecad_model::RouteId, Pt)> = ecad_electrical::wires::routes::tees_on_page(doc, page)
        .into_iter()
        .filter(|(r, _, _)| ecad_electrical::wires::routes::route_item(doc, *r).is_some_and(|(_, i, _)| !hidden.contains(&i)))
        .map(|(r, _, at)| (r, at))
        .collect();
    let rings = ecad_electrical::wires::TEE_DOT_RADII.len();
    if tees_shown.len() != tees.len() * rings || !tees_shown.iter().all(|(r, at)| tees.iter().any(|(x, p)| x == r && near(*p, *at))) {
        problems.push("the T dots drawn differ from the T's of the document".into());
    }
    if fan_shown != fan_expected {
        problems.push("the cable fans drawn differ from the fans computed from the document".into());
    }

    // 2. Identity: each mapped handle shows the item (or marker) it is mapped to, and the map
    //    covers the drawing one-to-one.
    if !map.is_consistent() {
        problems.push("the handle map is not one-to-one".into());
    }
    if map.len() != entities.len() {
        problems.push(format!("{} entities but {} mapped handles", entities.len(), map.len()));
    }
    for (id, item) in doc.items().iter().filter(|(id, i)| i.page == page && !hidden.contains(id)) {
        match map.item(*id).and_then(|h| entities.get(&h)) {
            Some(e) if item_from(doc, page, &plain(e)).is_ok_and(|x| x == *item) => {}
            Some(_) => problems.push(format!("item {id} is mapped to an entity that shows something else")),
            None => problems.push(format!("item {id} is not drawn")),
        }
    }

    // 3. Terminal positions: drawn markers where the connection points are, and symbol terminals
    //    (from the block's markers and the insert) where the definition and placement put them.
    let mut expected_markers = Vec::new();
    for (rid, r) in doc.representations().iter().filter(|(_, r)| r.page == page) {
        match r.form {
            Form::Drawn => {
                for (t, at) in &r.connection_points {
                    expected_markers.push(*at);
                    v.connection_points += 1;
                    match map.marker(*rid, *t).and_then(|h| entities.get(&h)).and_then(|e| marker_position(e)) {
                        Some(p) if near(p, *at) => {}
                        _ => problems.push(format!("connection point {t} of representation {rid} is not marked at its position")),
                    }
                }
            }
            Form::Symbol => {
                let Some(item) = r.items.first() else { continue };
                let Some(e) = map.item(*item).and_then(|h| entities.get(&h)) else { continue };
                let Some(ItemKind::Symbol(inst)) = doc.items().get(item).map(|i| &i.kind) else { continue };
                if !matches!(&e.kind, EntityKind::Insert(i) if i.block == block_name(inst.definition)) {
                    problems.push(format!("symbol of representation {rid} is not an insert of its definition's block"));
                    continue;
                }
                let want: Vec<Pt> = ecad_model::connectivity::derive::connection_point_positions(doc, page)
                    .into_iter()
                    .filter(|(_, rep, _)| rep == rid)
                    .map(|(_, _, p)| p)
                    .collect();
                let got = block_connection_points(drawing, e);
                v.connection_points += want.len();
                // The block shows every terminal of the definition; the component may use a subset.
                if !want.iter().all(|w| got.iter().any(|g| near(*g, *w))) {
                    problems.push(format!("symbol of representation {rid} does not show its connection points where the document puts them"));
                }
            }
        }
    }
    if !same_points(markers_shown, expected_markers) {
        problems.push("connection point markers in the drawing differ from the document's connection points".into());
    }

    // 4. Blocks: one per definition used on the page, with the definition's graphics.
    for (name, b) in drawing.blocks.iter().filter(|(n, _)| n.starts_with(ECAD_PREFIX)) {
        let Some(def) = crate::convert::definition_of_block(name).and_then(|id| doc.definitions().get(&id)) else {
            problems.push(format!("block {name} has no definition in the document"));
            continue;
        };
        let graphics: Vec<EntityKind> = b.entities.iter().filter(|e| marker_position(e).is_none()).map(|e| e.kind.clone()).collect();
        let want: Vec<EntityKind> = def.graphics.iter().map(kind_for_primitive).collect();
        if graphics != want {
            problems.push(format!("block {name} does not show its definition's graphics"));
        }
        let terms: Vec<Pt> = b.entities.iter().filter_map(|e| marker_position(e)).collect();
        if !same_points(terms, def.connection_points.iter().map(|t| t.at).collect()) {
            problems.push(format!("block {name} does not mark its definition's connection points"));
        }
    }

    // 5. Groups: each drawn component with several parts is one group of exactly those parts.
    let groups: HashMap<&str, BTreeSet<Handle>> =
        drawing.groups.iter().filter(|g| g.name.starts_with(ECAD_PREFIX)).map(|g| (g.name.as_str(), g.members.iter().copied().collect())).collect();
    let mut wanted_groups = 0;
    for (rid, r) in doc.representations().iter().filter(|(_, r)| r.page == page && r.form == Form::Drawn) {
        let mut members: BTreeSet<Handle> = r.items.iter().filter_map(|i| map.item(*i)).collect();
        members.extend(r.connection_points.keys().filter_map(|t| map.marker(*rid, *t)));
        if !matches!(r.subject, Subject::Device(_)) || members.len() < 2 {
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

    // 6. Relationships shown on the page: a route end on a connection point drawn here ends on a
    //    terminal position read from the drawing (markers and symbol blocks), not from the document.
    let mut drawn_connection_points: Vec<Pt> = entities.values().filter_map(|e| marker_position(e)).collect();
    drawn_connection_points.extend(entities.values().flat_map(|e| block_connection_points(drawing, e)));
    let shown = ecad_model::connectivity::derive::connection_point_positions(doc, page);
    for rid in doc.routes_on(page) {
        let Some(c) = doc.routes().get(&rid) else { continue };
        let Some((item, _)) = doc.representations_of(ecad_model::Subject::Route(rid)).find_map(|(_, r)| r.items.first().map(|i| (*i, r))) else {
            continue;
        };
        let Some(e) = map.item(item).and_then(|h| entities.get(&h)) else { continue };
        let Some(path) = item_from(doc, page, e).ok().and_then(|i| match i.kind {
            ItemKind::Primitive(p) => p.path_ends(),
            ItemKind::Symbol(_) => None,
        }) else {
            problems.push(format!("route {rid} is not a path in the drawing"));
            continue;
        };
        for (end, landed) in End::BOTH.into_iter().zip(c.ends) {
            let Some(t) = landed.point() else { continue };
            if !shown.iter().any(|(x, _, _)| *x == t) {
                continue;
            }
            v.attached_ends += 1;
            let at = if end == End::A { path.0 } else { path.1 };
            if !drawn_connection_points.iter().any(|p| near(*p, at)) {
                problems.push(format!("route {rid} end {end:?} is on a connection point but not drawn on it"));
            }
        }
    }
    v.items = items;
    v.problems = problems;
    v
}

/// What a computed entity draws, for comparing multisets: its kind, lineweight and colour.
fn computed_key(e: &Entity) -> String {
    format!("{:?} {:?} {:?} {}", e.kind, e.common.lineweight, e.common.color, e.common.layer)
}
