//! Translating a CADCraft commit into an ECAD transaction (hypothesis H1, proposal §9).
//!
//! The owner compares the engine's drawing with the last projection it wrote, classifies every
//! difference using the handle map and the engine's provenance records, applies the movement
//! policy (§10: M1 reroute, M2 connected equipment, M3 deliberate reconnection) and the copy
//! policy (§11: whole components are copied independently), and returns ops. Anything it cannot
//! classify is refused, never dropped (DOC-08).
//!
//! # Cost (brief 014)
//! Previews of the proposed ops run in place on the live document ([`Ledger::with_applied`]:
//! applied, checked where they touched it, read, rolled back), never on a copy, and the movement
//! policy goes only through the conductors the ops could have moved ([`Scope`]). Apart from the
//! per-page drawing comparison, a translation costs in proportion to what the edit touched.
//! The ops are exactly those of the earlier whole-document code, which the `translate-oracle`
//! feature keeps as a test oracle (`translate_oracle.rs`).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use cadcraft_engine::doc::{Drawing, Entity, Handle};
use cadcraft_engine::ext::{Provenance, Source};
use ecad_model::derive::{route_on_page, terminal_positions};
use ecad_model::{
    Component, ComponentId, Conductor, ConductorId, Document, End, Form, IdAllocator, Item, ItemId, ItemKind, Ledger,
    Op, PageId, Pt, Representation, RepresentationId, Subject, Terminal, TerminalId,
};

use crate::convert::{ECAD_PREFIX, MARKER_LAYER, item_from, marker_entity, marker_position};
use crate::project::{ClipSnapshot, HandleMap, Mapped};

/// Everything the translation reads. The ledger is borrowed mutably only to preview proposed
/// ops in place ([`Ledger::with_applied`]); the translation never changes it.
pub(crate) struct Ctx<'a> {
    pub ledger: &'a mut Ledger,
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

/// Apply `ops` to the live document in place, check what they touched, run `read` on the result
/// and roll back. Errors are those a commit of `ops` would report.
fn preview<R>(ledger: &mut Ledger, ops: &[Op], read: impl FnOnce(&Document) -> R) -> Result<R, String> {
    ledger.with_applied(ops, read).map_err(|e| e.to_string())
}

/// The terminal shown at `at` on `page`, if any.
pub(crate) fn terminal_at(doc: &Document, page: PageId, at: Pt, tol: f64) -> Option<TerminalId> {
    terminal_positions(doc, page).into_iter().find(|(_, _, p)| p.near(at, tol)).map(|(t, _, _)| t)
}

/// Where `terminal` is shown on `page`: its first representation there, as
/// [`terminal_positions`] lists them (representations in ID order), without going through the
/// rest of the page.
///
/// # Semantics
/// The same answer as the first entry for `terminal` in `terminal_positions(doc, page)` on any
/// valid document: there a drawn representation's connection points are its own component's
/// terminals (invariant I16), so only the terminal's component's representations can show it.
fn position_of(doc: &Document, page: PageId, terminal: TerminalId) -> Option<Pt> {
    let t = doc.terminals.get(&terminal)?;
    for (_, r) in doc.representations_of(Subject::Component(t.component)).filter(|(_, r)| r.page == page) {
        match r.form {
            Form::Drawn => {
                if let Some(p) = r.connection_points.get(&terminal) {
                    return Some(*p);
                }
            }
            Form::Symbol => {
                let Some(ItemKind::Symbol(inst)) = r.items.first().and_then(|i| doc.items.get(i)).map(|i| &i.kind)
                else {
                    continue;
                };
                let Some(def) = doc.definitions.get(&inst.definition) else { continue };
                if let Some(td) = t.key.as_ref().and_then(|k| def.terminals.iter().find(|d| &d.key == k)) {
                    return Some(inst.placement.apply(td.at));
                }
            }
        }
    }
    None
}

/// What a set of proposed ops could move on a page, worked out on the document before them.
///
/// # Semantics
/// A conductor can get a movement op (§10) only if one of its ends is attached to a terminal
/// whose shown position changed (M2), or one of its route ends moved (M3) or is fresh. So:
/// - `terminals`: every terminal whose position on the page the ops could change: those of every
///   component whose representation on the page the ops link, unlink or change (as recorded in
///   the ops, before and after), or whose page item they change or remove. Terminals the ops
///   create have no earlier position, so never count as moved.
/// - `routes`: every conductor whose route on the page the ops could change (its representation
///   or its route item changed, or it is new).
/// - `attached`: conductors whose ends the ops attach or detach.
///
/// Ops a translation does not produce (pages, layers, cables, properties, definitions it does not
/// newly embed, deletions of components or terminals…) make the scope the whole document, which
/// is exactly the earlier behaviour.
#[derive(Debug, Default)]
struct Scope {
    terminals: BTreeSet<TerminalId>,
    routes: BTreeSet<ConductorId>,
    attached: BTreeSet<ConductorId>,
}

impl Scope {
    fn of(doc: &Document, page: PageId, ops: &[Op]) -> Scope {
        let mut s = Scope::default();
        let mut components: BTreeSet<ComponentId> = BTreeSet::new();
        let mut note = |r: &Representation, s: &mut Scope| {
            if r.page != page {
                return;
            }
            match r.subject {
                Subject::Component(c) => {
                    components.insert(c);
                    s.terminals.extend(r.connection_points.keys().copied());
                }
                Subject::Conductor(c) => {
                    s.routes.insert(c);
                }
                Subject::Cable(_) => {}
            }
        };
        for op in ops {
            match op {
                Op::ModifyItem { id, .. } | Op::RemoveItem { id, .. } => {
                    if let Some((_, r)) = doc.representation_of_item(*id) {
                        note(r, &mut s);
                    }
                }
                // A new item is shown only through a representation op, noted below.
                Op::AddItem { .. } => {}
                Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => note(rep, &mut s),
                Op::ModifyRepresentation { before, after, .. } => {
                    note(before, &mut s);
                    note(after, &mut s);
                }
                Op::CreateConductor { id, .. } => {
                    s.routes.insert(*id);
                }
                Op::AttachEnd { conductor, .. } => {
                    s.attached.insert(*conductor);
                }
                // New objects; a definition the document lacks cannot be shown by anything in it.
                Op::CreateComponent { id, .. } if !doc.components.contains_key(id) => {}
                Op::AddTerminal { id, .. } if !doc.terminals.contains_key(id) => {}
                Op::EmbedDefinition { id, .. } if !doc.definitions.contains_key(id) => {}
                _ => return Scope::everything(doc),
            }
        }
        for c in components {
            s.terminals.extend(doc.terminals_of(c).map(|(t, _)| t));
        }
        s
    }

    fn everything(doc: &Document) -> Scope {
        Scope {
            terminals: doc.terminals.keys().copied().collect(),
            routes: doc.conductors.keys().copied().collect(),
            attached: BTreeSet::new(),
        }
    }
}

/// What [`movement`] needs from the document before the ops: positions of the scope's terminals
/// and routes of the scope's conductors.
struct Earlier {
    positions: BTreeMap<TerminalId, Option<Pt>>,
    /// Route item and its path ends, `None` when the conductor has no route on the page.
    routes: BTreeMap<ConductorId, Option<(ItemId, Option<(Pt, Pt)>)>>,
}

pub(crate) fn translate(cx: &mut Ctx<'_>) -> Result<Translation, String> {
    #[cfg(feature = "translate-oracle")]
    let ids_before = cx.ids.clone();
    let result = translate_scoped(cx);
    #[cfg(feature = "translate-oracle")]
    crate::translate_oracle::compare(
        crate::translate_oracle::Inputs {
            doc: cx.ledger.document(),
            page: cx.page,
            map: cx.map,
            last: cx.last,
            now: cx.now,
            provenance: cx.provenance,
            clips: cx.clips,
            tol: cx.tol,
        },
        ids_before,
        cx.ids,
        &result,
    );
    result
}

fn translate_scoped(cx: &mut Ctx<'_>) -> Result<Translation, String> {
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
                let item = cx.ledger.document().items.get(&id).cloned().ok_or("mapped item missing")?;
                if let Some((rid, _)) = cx.ledger.document().representation_of_item(id) {
                    linked_removed = true;
                    rep(&mut reps, rid, cx.ledger.document())?;
                    if let Some(r) = reps.get_mut(&rid) {
                        r.items.retain(|i| *i != id);
                    }
                }
                t.ops.push(Op::RemoveItem { id, item });
                counts[2] += 1;
            }
            // Brief 005: a fan line is a view, not an object; it cannot be erased.
            Some(Mapped::Fan(_)) => return Err(crate::views::FAN_REFUSED.into()),
            Some(Mapped::Marker(rid, term)) => {
                linked_removed = true;
                rep(&mut reps, rid, cx.ledger.document())?;
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
                let item = item_from(cx.ledger.document(), cx.page, new)?;
                let old = cx.ledger.document().items.get(&id).cloned().ok_or("mapped item missing")?;
                if std::mem::discriminant(&item.kind) != std::mem::discriminant(&old.kind) {
                    return Err("an object changed kind; this is not supported on ECAD pages yet".into());
                }
                if item != old {
                    t.ops.push(Op::ModifyItem { id, before: old, after: item });
                    counts[1] += 1;
                }
            }
            Some(Mapped::Fan(_)) => return Err(crate::views::FAN_REFUSED.into()),
            Some(Mapped::Marker(rid, term)) => {
                let at = marker_position(new).ok_or("terminal markers can only be moved")?;
                if marker_entity(at, *h) != ***new {
                    return Err("terminal markers can only be moved".into());
                }
                rep(&mut reps, rid, cx.ledger.document())?;
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
                let item = item_from(cx.ledger.document(), cx.page, e)?;
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
        let old = cx.ledger.document().representations.get(&id).cloned().ok_or("representation missing")?;
        if r.items.is_empty() && r.connection_points.is_empty() {
            t.ops.push(Op::UnlinkRepresentation { id, rep: old });
        } else if r != old {
            t.ops.push(Op::ModifyRepresentation { id, before: old, after: r });
        }
    }

    movement(cx, &mut t, &fresh_conductor_ends)?;
    preview(cx.ledger, &t.ops, |_| ())?;
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
            Source::Entity(h) => (cx.ledger.document(), cx.map, *h),
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
        if matches!(map.get(h), Some(Mapped::Fan(_))) {
            return Err(crate::views::FAN_REFUSED.into());
        }
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
            && !cx.ledger.document().definitions.contains_key(&def)
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
            let item = item_from(cx.ledger.document(), cx.page, e)?;
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
            let item = item_from(cx.ledger.document(), cx.page, e)?;
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
        let item = item_from(cx.ledger.document(), cx.page, e)?;
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
///
/// # Semantics
/// Goes through the conductors of the proposed document in ID order, but only those [`Scope`]
/// says could have moved: conductors attached to a terminal whose position the ops could change,
/// conductors whose route the ops could change or whose ends they attach, and conductors with a
/// fresh end. Any other conductor has the same route, ends and terminal positions before and
/// after the ops, so the policy gives it nothing to do. Positions are worked out per terminal,
/// only for the scope's terminals.
fn movement(cx: &mut Ctx<'_>, t: &mut Translation, fresh_ends: &[(ConductorId, End)]) -> Result<(), String> {
    let (page, tol) = (cx.page, cx.tol);
    // Before the ops: what they could move, and where it is now.
    let doc = cx.ledger.document();
    let scope = Scope::of(doc, page, &t.ops);
    let earlier = Earlier {
        positions: scope.terminals.iter().map(|term| (*term, position_of(doc, page, *term))).collect(),
        routes: scope
            .routes
            .iter()
            .map(|cid| (*cid, route_on_page(doc, *cid, page).map(|(id, p)| (id, p.path_ends()))))
            .collect(),
    };
    // After the ops, on the live document, rolled back afterwards.
    let (amend, attach) =
        preview(cx.ledger, &t.ops, |proposed| moved(proposed, page, tol, &scope, &earlier, fresh_ends))?;
    for (id, (before, after)) in amend {
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
            t.ops.push(Op::ModifyItem { id, before, after });
        }
        if let Some(h) = handle {
            t.amended.insert(h);
        }
    }
    t.ops.extend(attach);
    Ok(())
}

/// Route items to amend (their item in `proposed`, and the amended item) and attachment ops, in
/// the order the policy produces them.
type Moves = (BTreeMap<ItemId, (Item, Item)>, Vec<Op>);

/// The movement policy on the proposed document, for the conductors `scope` names.
fn moved(
    proposed: &Document,
    page: PageId,
    tol: f64,
    scope: &Scope,
    earlier: &Earlier,
    fresh_ends: &[(ConductorId, End)],
) -> Moves {
    let eps = 1e-9;
    let mut candidates: BTreeSet<ConductorId> = scope.routes.union(&scope.attached).copied().collect();
    candidates.extend(fresh_ends.iter().map(|(c, _)| *c));
    for term in &scope.terminals {
        candidates.extend(proposed.ends_at(*term).into_iter().map(|(c, _)| c));
    }
    let mut amend: BTreeMap<ItemId, (Item, Item)> = BTreeMap::new();
    let mut attach: Vec<Op> = Vec::new();
    for cid in &candidates {
        let Some(c) = proposed.conductors.get(cid) else { continue };
        let Some((route_id, prim)) = route_on_page(proposed, *cid, page) else { continue };
        let Some((new_a, new_b)) = prim.path_ends() else { continue };
        // A conductor outside the scope's routes kept its route item and path.
        let old_ends = match earlier.routes.get(cid) {
            Some(old) => (*old).and_then(|(id, ends)| (id == route_id).then_some(ends).flatten()),
            None => Some((new_a, new_b)),
        };
        let mut route = prim.clone();
        for (end, new_end) in [(End::A, new_a), (End::B, new_b)] {
            let fresh = fresh_ends.contains(&(*cid, end));
            let end_moved =
                fresh || old_ends.is_some_and(|(a, b)| !(if end == End::A { a } else { b }).near(new_end, eps));
            match c.end(end) {
                Some(term) => {
                    // A terminal outside the scope is where it was.
                    let (op, np) = match earlier.positions.get(&term) {
                        Some(op) => (*op, position_of(proposed, page, term)),
                        None => (None, None),
                    };
                    let terminal_moved = matches!((op, np), (Some(a), Some(b)) if !a.near(b, eps));
                    if terminal_moved {
                        // M2: the end follows its terminal.
                        if let Some(np) = np
                            && !new_end.near(np, eps)
                            && let Some(r) = route.with_path_end(end, np)
                        {
                            route = r;
                        }
                    } else if end_moved {
                        // M3: a deliberate reconnection, resolved once.
                        let found = terminal_at(proposed, page, new_end, tol);
                        if found != Some(term) {
                            attach.push(Op::AttachEnd { conductor: *cid, end, before: Some(term), after: found });
                        }
                    }
                }
                None if end_moved => {
                    if let Some(found) = terminal_at(proposed, page, new_end, tol) {
                        attach.push(Op::AttachEnd { conductor: *cid, end, before: None, after: Some(found) });
                    }
                }
                None => {}
            }
        }
        if route != prim
            && let Some(item) = proposed.items.get(&route_id)
        {
            amend.insert(route_id, (item.clone(), Item { kind: ItemKind::Primitive(route), ..item.clone() }));
        }
    }
    (amend, attach)
}
