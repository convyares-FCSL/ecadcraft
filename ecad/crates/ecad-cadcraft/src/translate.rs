//! Translating a CADCraft commit into an ECAD transaction (hypothesis H1, proposal §9).
//!
//! The owner compares the engine's drawing with the last projection it wrote, classifies every
//! difference using the handle map and the engine's provenance records, applies the movement
//! policy (§10: M1 reroute, M2 connected equipment, M3 deliberate reconnection) and the copy
//! policy (§11: whole components are copied independently), and returns ops. Anything it cannot
//! classify is refused, never dropped (DOC-08).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use cadcraft_engine::doc::{Drawing, Entity, Handle};
use cadcraft_engine::ext::{Provenance, Source};
use ecad_model::derive::{route_on_page, terminal_positions};
use ecad_model::{
    Component, ComponentId, Conductor, ConductorId, Document, End, Form, IdAllocator, Item, ItemId, ItemKind, Op,
    PageId, Pt, Representation, RepresentationId, Subject, Terminal, TerminalId,
};

use crate::convert::{ECAD_PREFIX, MARKER_LAYER, item_from, marker_entity, marker_position};
use crate::project::{ClipSnapshot, HandleMap, Mapped};

/// Everything the translation reads.
pub(crate) struct Ctx<'a> {
    pub doc: &'a Document,
    pub page: PageId,
    pub map: &'a HandleMap,
    pub last: &'a Drawing,
    pub now: &'a Drawing,
    pub provenance: &'a [Provenance],
    pub clips: &'a BTreeMap<u64, ClipSnapshot>,
    pub ids: &'a mut IdAllocator,
    pub tol: f64,
}

/// The result: ops plus the handles of objects the engine created.
#[derive(Debug, Default)]
pub(crate) struct Translation {
    pub ops: Vec<Op>,
    pub new_items: Vec<(ItemId, Handle)>,
    pub new_markers: Vec<((RepresentationId, TerminalId), Handle)>,
    /// Handles whose geometry the owner changed beyond what the engine committed (M2, M3).
    pub amended: BTreeSet<Handle>,
    pub summary: String,
}

/// Parts of the drawing outside model-space entities that ECAD pages do not record yet. A change
/// to any of them is refused.
fn table_changes(last: &Drawing, now: &Drawing) -> Vec<&'static str> {
    let mut out = Vec::new();
    let user_groups =
        |d: &Drawing| d.groups.iter().filter(|g| !g.name.starts_with(ECAD_PREFIX)).cloned().collect::<Vec<_>>();
    let checks: [(&'static str, bool); 15] = [
        ("layers", last.layers != now.layers),
        ("linetypes", last.linetypes != now.linetypes),
        ("text styles", last.text_styles != now.text_styles),
        ("dimension styles", last.dim_styles != now.dim_styles),
        ("multileader styles", last.mleader_styles != now.mleader_styles),
        ("table styles", last.table_styles != now.table_styles),
        ("multiline styles", last.mline_styles != now.mline_styles),
        ("block definitions", last.blocks != now.blocks),
        (
            "layouts",
            last.layouts.iter().map(|l| (&l.name, &l.entities)).ne(now.layouts.iter().map(|l| (&l.name, &l.entities))),
        ),
        ("named views", last.views != now.views),
        ("UCSs", last.ucss != now.ucss),
        ("layer states", last.layer_states != now.layer_states),
        ("groups", user_groups(last) != user_groups(now)),
        ("constraints", last.constraints != now.constraints),
        ("plot style tables", last.plot_style_tables != now.plot_style_tables),
    ];
    for (name, changed) in checks {
        if changed {
            out.push(name);
        }
    }
    out
}

fn entities(d: &Drawing) -> (Vec<Handle>, HashMap<Handle, &Arc<Entity>>) {
    let order: Vec<Handle> = d.model.iter().map(|e| e.handle).collect();
    (order, d.model.iter().map(|e| (e.handle, e)).collect())
}

/// Apply `ops` to a copy of `doc` and check invariants.
fn preview(doc: &Document, ops: &[Op]) -> Result<Document, String> {
    let mut d = doc.clone();
    for op in ops {
        op.apply(&mut d).map_err(|e| e.to_string())?;
    }
    d.check().map_err(|e| e.to_string())?;
    Ok(d)
}

/// The terminal shown at `at` on `page`, if any.
pub(crate) fn terminal_at(doc: &Document, page: PageId, at: Pt, tol: f64) -> Option<TerminalId> {
    terminal_positions(doc, page).into_iter().find(|(_, _, p)| p.near(at, tol)).map(|(t, _, _)| t)
}

/// Where each terminal is shown on `page` (first representation).
fn positions(doc: &Document, page: PageId) -> BTreeMap<TerminalId, Pt> {
    let mut out = BTreeMap::new();
    for (t, _, p) in terminal_positions(doc, page) {
        out.entry(t).or_insert(p);
    }
    out
}

pub(crate) fn translate(cx: &mut Ctx<'_>) -> Result<Translation, String> {
    let changed_tables = table_changes(cx.last, cx.now);
    if !changed_tables.is_empty() {
        return Err(format!("changes to {} are not recorded on ECAD pages yet", changed_tables.join(", ")));
    }
    let (_, before) = entities(cx.last);
    let (now_order, after) = entities(cx.now);
    let mut t = Translation::default();
    let mut reps: BTreeMap<RepresentationId, Representation> = BTreeMap::new();
    let rep = |reps: &mut BTreeMap<RepresentationId, Representation>,
               id: RepresentationId,
               doc: &Document|
     -> Result<(), String> {
        if let std::collections::btree_map::Entry::Vacant(e) = reps.entry(id) {
            e.insert(doc.representations.get(&id).cloned().ok_or("representation missing")?);
        }
        Ok(())
    };
    let mut counts = [0usize; 4]; // added, modified, removed, copied components
    // Whether an object with a meaning (an item of a representation, a terminal marker) was removed.
    let mut linked_removed = false;

    // Removed and changed entities.
    let mut removed: Vec<(Handle, &Arc<Entity>)> =
        before.iter().filter(|(h, _)| !after.contains_key(h)).map(|(h, e)| (*h, *e)).collect();
    removed.sort_by_key(|(h, _)| *h);
    for (h, _) in &removed {
        match cx.map.get(*h) {
            Some(Mapped::Item(id)) => {
                let item = cx.doc.items.get(&id).cloned().ok_or("mapped item missing")?;
                if let Some((rid, _)) = cx.doc.representation_of_item(id) {
                    linked_removed = true;
                    rep(&mut reps, rid, cx.doc)?;
                    if let Some(r) = reps.get_mut(&rid) {
                        r.items.retain(|i| *i != id);
                    }
                }
                t.ops.push(Op::RemoveItem { id, item });
                counts[2] += 1;
            }
            Some(Mapped::Marker(rid, term)) => {
                linked_removed = true;
                rep(&mut reps, rid, cx.doc)?;
                if let Some(r) = reps.get_mut(&rid) {
                    r.connection_points.remove(&term);
                }
            }
            None => {}
        }
    }
    let mut changed: Vec<(Handle, &Arc<Entity>)> = before
        .iter()
        .filter_map(|(h, old)| {
            after.get(h).filter(|new| !Arc::ptr_eq(old, new) && **old != ***new).map(|new| (*h, *new))
        })
        .collect();
    changed.sort_by_key(|(h, _)| *h);
    for (h, new) in &changed {
        match cx.map.get(*h) {
            Some(Mapped::Item(id)) => {
                let item = item_from(cx.doc, cx.page, new)?;
                let old = cx.doc.items.get(&id).cloned().ok_or("mapped item missing")?;
                if std::mem::discriminant(&item.kind) != std::mem::discriminant(&old.kind) {
                    return Err("an object changed kind; this is not supported on ECAD pages yet".into());
                }
                if item != old {
                    t.ops.push(Op::ModifyItem { id, before: old, after: item });
                    counts[1] += 1;
                }
            }
            Some(Mapped::Marker(rid, term)) => {
                let at = marker_position(new).ok_or("terminal markers can only be moved")?;
                if marker_entity(at, *h) != ***new {
                    return Err("terminal markers can only be moved".into());
                }
                rep(&mut reps, rid, cx.doc)?;
                if let Some(r) = reps.get_mut(&rid) {
                    r.connection_points.insert(term, at);
                }
            }
            None => return Err("an object this page does not know about changed".into()),
        }
    }

    // Added entities: with provenance (copies, pastes) or new drawing.
    let prov: HashMap<Handle, (Source, u32)> = cx.provenance.iter().map(|p| (p.new, (p.source, p.batch))).collect();
    let added: Vec<Handle> = now_order.iter().copied().filter(|h| !before.contains_key(h)).collect();
    let mut batches: BTreeMap<u32, Vec<(Handle, Source)>> = BTreeMap::new();
    let mut fresh_conductor_ends: Vec<(ConductorId, End)> = Vec::new();
    // Removing a meaningful object while creating new ones without provenance is a replacement
    // (BREAK, JOIN, EXPLODE, TRIM that re-creates…): what the new objects mean is undecided (L1).
    if linked_removed && added.iter().any(|h| !prov.contains_key(h)) {
        return Err(
            "this command replaces objects that have an electrical meaning; that is not supported on ECAD pages yet"
                .into(),
        );
    }
    for h in &added {
        match prov.get(h) {
            Some((src, batch)) => batches.entry(*batch).or_default().push((*h, *src)),
            None => {
                let e = after.get(h).ok_or("added entity missing")?;
                if e.common.layer == MARKER_LAYER {
                    return Err(format!("the {MARKER_LAYER} layer is reserved for terminal markers"));
                }
                let item = item_from(cx.doc, cx.page, e)?;
                if matches!(item.kind, ItemKind::Symbol(_)) {
                    return Err("symbols are placed with PLACE, not inserted as blocks".into());
                }
                let id: ItemId = cx.ids.fresh();
                t.ops.push(Op::AddItem { id, item });
                t.new_items.push((id, *h));
                counts[0] += 1;
            }
        }
    }
    for (_, batch) in batches {
        copy_batch(cx, &after, &batch, &mut t, &mut fresh_conductor_ends, &mut counts)?;
    }

    // Representation edits: an emptied representation is removed (its subject stays, DOC-06).
    for (id, r) in reps {
        let old = cx.doc.representations.get(&id).cloned().ok_or("representation missing")?;
        if r.items.is_empty() && r.connection_points.is_empty() {
            t.ops.push(Op::UnlinkRepresentation { id, rep: old });
        } else if r != old {
            t.ops.push(Op::ModifyRepresentation { id, before: old, after: r });
        }
    }

    movement(cx, &mut t, &fresh_conductor_ends)?;
    preview(cx.doc, &t.ops)?;
    t.summary =
        format!("{} added, {} modified, {} removed, {} components copied", counts[0], counts[1], counts[2], counts[3]);
    Ok(t)
}

/// One copy operation's new entities (§11.1).
fn copy_batch(
    cx: &mut Ctx<'_>,
    after: &HashMap<Handle, &Arc<Entity>>,
    batch: &[(Handle, Source)],
    t: &mut Translation,
    fresh_ends: &mut Vec<(ConductorId, End)>,
    counts: &mut [usize; 4],
) -> Result<(), String> {
    // Resolve sources against the live document or the clipboard snapshot.
    let mut src_doc: Option<&Document> = None;
    let mut src_map: Option<&HandleMap> = None;
    let mut by_source: HashMap<Handle, Handle> = HashMap::new();
    for (new, src) in batch {
        let (doc, map, h) = match src {
            Source::Entity(h) => (cx.doc, cx.map, *h),
            Source::Clipboard { serial, handle } => {
                let snap = cx
                    .clips
                    .get(serial)
                    .ok_or("the clipboard was filled before this project recorded it; copy again")?;
                if snap.page != cx.page {
                    return Err("pasting onto another page is not supported in the experiment yet".into());
                }
                (&snap.doc, &snap.map, *handle)
            }
        };
        src_doc = Some(doc);
        src_map = Some(map);
        by_source.insert(h, *new);
    }
    let (Some(sdoc), Some(smap)) = (src_doc, src_map) else { return Ok(()) };
    let mut consumed: BTreeSet<Handle> = BTreeSet::new();
    let mut term_map: BTreeMap<TerminalId, TerminalId> = BTreeMap::new();

    // Whole component representations: independent copies.
    for (rid, r) in sdoc.representations.iter().filter(|(_, r)| r.page == cx.page) {
        let Subject::Component(src_comp) = r.subject else { continue };
        let item_handles: Vec<Handle> = r.items.iter().filter_map(|i| smap.item(*i)).collect();
        let marker_handles: Vec<(TerminalId, Handle)> =
            r.connection_points.keys().filter_map(|tm| smap.marker(*rid, *tm).map(|h| (*tm, h))).collect();
        let all_copied = item_handles.len() == r.items.len()
            && marker_handles.len() == r.connection_points.len()
            && item_handles.iter().chain(marker_handles.iter().map(|(_, h)| h)).all(|h| by_source.contains_key(h));
        if !all_copied || item_handles.is_empty() {
            continue;
        }
        let comp = sdoc.components.get(&src_comp).cloned().ok_or("source component missing")?;
        if let Some(def) = comp.definition
            && !cx.doc.definitions.contains_key(&def)
        {
            let d = sdoc.definitions.get(&def).cloned().ok_or("source definition missing")?;
            t.ops.push(Op::EmbedDefinition { id: def, definition: d });
        }
        let new_comp: ComponentId = cx.ids.fresh();
        t.ops.push(Op::CreateComponent { id: new_comp, component: Component { ..comp } });
        for (src_t, term) in sdoc.terminals_of(src_comp) {
            let nt: TerminalId = cx.ids.fresh();
            term_map.insert(src_t, nt);
            t.ops.push(Op::AddTerminal { id: nt, terminal: Terminal { component: new_comp, ..term.clone() } });
        }
        let new_rep: RepresentationId = cx.ids.fresh();
        let mut items = Vec::new();
        for (src_item, sh) in r.items.iter().zip(&item_handles) {
            let _ = src_item;
            let nh = by_source.get(sh).copied().ok_or("copy missing")?;
            let e = after.get(&nh).ok_or("copied entity missing")?;
            let item = item_from(cx.doc, cx.page, e)?;
            let id: ItemId = cx.ids.fresh();
            t.ops.push(Op::AddItem { id, item });
            t.new_items.push((id, nh));
            items.push(id);
            consumed.insert(nh);
        }
        let mut cps = BTreeMap::new();
        for (src_t, sh) in &marker_handles {
            let nh = by_source.get(sh).copied().ok_or("copy missing")?;
            let e = after.get(&nh).ok_or("copied marker missing")?;
            let at = marker_position(e).ok_or("copied marker is not a marker")?;
            let nt = term_map.get(src_t).copied().ok_or("terminal not copied")?;
            cps.insert(nt, at);
            t.new_markers.push(((new_rep, nt), nh));
            consumed.insert(nh);
        }
        t.ops.push(Op::LinkRepresentation {
            id: new_rep,
            rep: Representation {
                subject: Subject::Component(new_comp),
                page: cx.page,
                form: r.form,
                role: r.role.clone(),
                items,
                connection_points: cps,
            },
        });
        counts[3] += 1;
    }

    // Whole conductor routes: new conductors; ends follow copied terminals, else are resolved later.
    for (_, r) in sdoc.representations.iter().filter(|(_, r)| r.page == cx.page) {
        let Subject::Conductor(src_c) = r.subject else { continue };
        let handles: Vec<Handle> = r.items.iter().filter_map(|i| smap.item(*i)).collect();
        if handles.is_empty() || handles.len() != r.items.len() || !handles.iter().all(|h| by_source.contains_key(h)) {
            continue;
        }
        let c = sdoc.conductors.get(&src_c).cloned().ok_or("source conductor missing")?;
        let new_c: ConductorId = cx.ids.fresh();
        t.ops.push(Op::CreateConductor {
            id: new_c,
            conductor: Conductor { properties: c.properties.clone(), ends: [None, None] },
        });
        let new_rep: RepresentationId = cx.ids.fresh();
        let mut items = Vec::new();
        for sh in &handles {
            let nh = by_source.get(sh).copied().ok_or("copy missing")?;
            let e = after.get(&nh).ok_or("copied entity missing")?;
            let item = item_from(cx.doc, cx.page, e)?;
            let id: ItemId = cx.ids.fresh();
            t.ops.push(Op::AddItem { id, item });
            t.new_items.push((id, nh));
            items.push(id);
            consumed.insert(nh);
        }
        t.ops.push(Op::LinkRepresentation {
            id: new_rep,
            rep: Representation {
                subject: Subject::Conductor(new_c),
                page: cx.page,
                form: Form::Drawn,
                role: r.role.clone(),
                items,
                connection_points: BTreeMap::new(),
            },
        });
        for end in End::BOTH {
            match c.end(end).and_then(|src_t| term_map.get(&src_t)) {
                Some(nt) => t.ops.push(Op::AttachEnd { conductor: new_c, end, before: None, after: Some(*nt) }),
                None => fresh_ends.push((new_c, end)),
            }
        }
    }

    // Everything else in the batch: plain copies of plain or partly copied items.
    for (new, src) in batch {
        if consumed.contains(new) {
            continue;
        }
        let sh = match src {
            Source::Entity(h) | Source::Clipboard { handle: h, .. } => *h,
        };
        if matches!(smap.get(sh), Some(Mapped::Marker(..))) {
            return Err("copying terminal markers without their whole component is not supported yet".into());
        }
        let e = after.get(new).ok_or("copied entity missing")?;
        let item = item_from(cx.doc, cx.page, e)?;
        if matches!(item.kind, ItemKind::Symbol(_)) {
            return Err("a symbol copied without its component; this is not supported yet".into());
        }
        let id: ItemId = cx.ids.fresh();
        t.ops.push(Op::AddItem { id, item });
        t.new_items.push((id, *new));
    }
    Ok(())
}

/// Movement policy (§10). Adds attachment ops and route-end amendments to `t`.
fn movement(cx: &Ctx<'_>, t: &mut Translation, fresh_ends: &[(ConductorId, End)]) -> Result<(), String> {
    let eps = 1e-9;
    let proposed = preview(cx.doc, &t.ops)?;
    let old_pos = positions(cx.doc, cx.page);
    let new_pos = positions(&proposed, cx.page);
    let mut amend: BTreeMap<ItemId, Item> = BTreeMap::new();
    let mut attach: Vec<Op> = Vec::new();
    for (cid, c) in &proposed.conductors {
        let Some((route_id, prim)) = route_on_page(&proposed, *cid, cx.page) else { continue };
        let Some((new_a, new_b)) = prim.path_ends() else { continue };
        let old_ends =
            route_on_page(cx.doc, *cid, cx.page).and_then(|(id, p)| (id == route_id).then(|| p.path_ends()).flatten());
        let mut route = prim.clone();
        for (end, new_end) in [(End::A, new_a), (End::B, new_b)] {
            let fresh = fresh_ends.contains(&(*cid, end));
            let end_moved =
                fresh || old_ends.is_some_and(|(a, b)| !(if end == End::A { a } else { b }).near(new_end, eps));
            match c.end(end) {
                Some(term) => {
                    let (op, np) = (old_pos.get(&term), new_pos.get(&term));
                    let terminal_moved = matches!((op, np), (Some(a), Some(b)) if !a.near(*b, eps));
                    if terminal_moved {
                        // M2: the end follows its terminal.
                        if let Some(np) = np
                            && !new_end.near(*np, eps)
                            && let Some(r) = route.with_path_end(end, *np)
                        {
                            route = r;
                        }
                    } else if end_moved {
                        // M3: a deliberate reconnection, resolved once.
                        let found = terminal_at(&proposed, cx.page, new_end, cx.tol);
                        if found != Some(term) {
                            attach.push(Op::AttachEnd { conductor: *cid, end, before: Some(term), after: found });
                        }
                    }
                }
                None if end_moved => {
                    if let Some(found) = terminal_at(&proposed, cx.page, new_end, cx.tol) {
                        attach.push(Op::AttachEnd { conductor: *cid, end, before: None, after: Some(found) });
                    }
                }
                None => {}
            }
        }
        if route != prim
            && let Some(item) = proposed.items.get(&route_id)
        {
            amend.insert(route_id, Item { kind: ItemKind::Primitive(route), ..item.clone() });
        }
    }
    for (id, after) in amend {
        let handle = cx.map.item(id).or_else(|| t.new_items.iter().find(|(i, _)| *i == id).map(|(_, h)| *h));
        // Fold into the op that already touches the item, else add a modification.
        let mut folded = false;
        for op in t.ops.iter_mut() {
            match op {
                Op::ModifyItem { id: i, after: a, .. } if *i == id => {
                    *a = after.clone();
                    folded = true;
                }
                Op::AddItem { id: i, item } if *i == id => {
                    *item = after.clone();
                    folded = true;
                }
                _ => {}
            }
        }
        if !folded {
            let before = proposed.items.get(&id).cloned().ok_or("route item missing")?;
            t.ops.push(Op::ModifyItem { id, before, after });
        }
        if let Some(h) = handle {
            t.amended.insert(h);
        }
    }
    t.ops.extend(attach);
    Ok(())
}
