//! Translating a CADCraft commit into an ECAD transaction.
//!
//! The owner compares the engine's drawing with the last projection it wrote, classifies every
//! difference using the handle map and the engine's provenance records, applies the movement
//! policy (M1 reroute, M2 connected equipment, M3 deliberate reconnection) and the copy
//! policy (whole components are copied independently), and returns ops. Anything it cannot
//! classify is refused, never dropped.
//!
//! # Cost
//! Previews of the proposed ops run in place on the live document ([`Ledger::with_applied`]:
//! applied, checked where they touched it, read, rolled back), never on a copy, and the movement
//! policy goes only through the conductors the ops could have moved ([`Scope`]). Apart from the
//! per-page drawing comparison, a translation costs in proportion to what the edit touched.

pub(crate) mod oops;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use cadcraft_engine::doc::{Drawing, Entity, Handle};
use cadcraft_engine::ext::{Provenance, Source};
use ecad_model::connectivity::derive::{connection_point_position, connection_point_positions};
use ecad_model::{
    ConnectionPoint, ConnectionPointId, Definition, DefinitionId, Device, DeviceId, Document, End, Form, IdAllocator, Item, ItemId, ItemKind, Ledger,
    Op, PageId, Primitive, Pt, Representation, RepresentationId, Route, RouteEnd, RouteId, Subject,
};

use crate::convert::{ECAD_PREFIX, MARKER_LAYER, item_from, marker_entity, marker_position};
use crate::project::{ClipSnapshot, HandleMap, Mapped};
use {
    ecad_electrical::wires::ON_WIRE, ecad_electrical::wires::label_rule::ShownOnce, ecad_electrical::wires::label_rule::first_near,
    ecad_electrical::wires::labels::path_vertices, ecad_electrical::wires::placing::on_route_of, ecad_electrical::wires::placing::route_at,
    ecad_electrical::wires::routes::route_item,
};

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
    /// Tag numbers for copied devices, caught up with the ledger before the edit.
    pub tags: &'a mut ecad_electrical::devices::tags::TagHigh,
}

/// The result: ops plus the handles of objects the engine created.
#[derive(Debug, Default)]
pub(crate) struct Translation {
    pub ops: Vec<Op>,
    pub new_items: Vec<(ItemId, Handle)>,
    pub new_markers: Vec<((RepresentationId, ConnectionPointId), Handle)>,
    /// Handles whose geometry the owner changed beyond what the engine committed (M2, M3).
    pub amended: BTreeSet<Handle>,
    pub summary: String,
    /// Lines to echo after the commit (for example why a wire end was not connected).
    pub notes: Vec<String>,
}

/// Parts of the drawing outside model-space entities that ECAD pages do not record yet. A change
/// to any of them is refused.
fn table_changes(last: &Drawing, now: &Drawing) -> Vec<&'static str> {
    let mut out = Vec::new();
    let user_groups = |d: &Drawing| d.groups.iter().filter(|g| !g.name.starts_with(ECAD_PREFIX)).cloned().collect::<Vec<_>>();
    let checks: [(&'static str, bool); 15] = [
        ("layers", last.layers != now.layers),
        ("linetypes", last.linetypes != now.linetypes),
        ("text styles", last.text_styles != now.text_styles),
        ("dimension styles", last.dim_styles != now.dim_styles),
        ("multileader styles", last.mleader_styles != now.mleader_styles),
        ("table styles", last.table_styles != now.table_styles),
        ("multiline styles", last.mline_styles != now.mline_styles),
        ("block definitions", last.blocks != now.blocks),
        ("layouts", last.layouts.iter().map(|l| (&l.name, &l.entities)).ne(now.layouts.iter().map(|l| (&l.name, &l.entities)))),
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
///
/// The check is scoped to what `ops` touch (plus the full check in verify mode). On an already
/// invalid document the earlier full-check preview could refuse where this one accepts; a
/// committed document is always valid, so that cannot happen here.
fn preview<R>(ledger: &mut Ledger, ops: &[Op], read: impl FnOnce(&Document) -> R) -> Result<R, String> {
    ledger.with_applied(ops, read).map_err(|e| e.to_string())
}

/// The terminal shown at `at` on `page`, if any.
pub(crate) fn connection_point_at(doc: &Document, page: PageId, at: Pt, tol: f64) -> Option<ConnectionPointId> {
    first_near(&connection_point_positions(doc, page), at, tol)
}

/// What a set of proposed ops could move on a page, worked out on the document before them.
///
/// # Semantics
/// A route can get a movement change only if one of its ends lands on a connection point
/// whose shown position changed (M2), one of its ends moved (M3) or is fresh, or it lands on a
/// route the ops changed (a T whose route moved away). So:
/// - `connection_points`: every connection point whose position on the page the ops could change: those
///   of every component whose representation on the page the ops link, unlink or change (as
///   recorded in the ops, before and after), or whose page item they change or remove. Points the
///   ops create have no earlier position, so never count as moved.
/// - `routes`: every route the ops could change (its representation or its item changed, or the
///   route itself added, removed or changed).
///
/// Op kinds a translation does not produce make the scope the whole document, which is exactly
/// the earlier behaviour: `AddPage`, `RemovePage`, `AddLayer`, `RemoveLayer`, `DeleteDevice`,
/// `RemoveConnectionPoint`, the wire and cable ops, `SetProperty`, `RemoveDefinition`,
/// `SetProjectSettings`, `NameVersion`, `UnnameVersion`, and `CreateDevice`, `AddConnectionPoint` or
/// `EmbedDefinition` of an ID the document already has.
#[derive(Debug, Default)]
struct Scope {
    connection_points: BTreeSet<ConnectionPointId>,
    routes: BTreeSet<RouteId>,
}

impl Scope {
    fn of(doc: &Document, page: PageId, ops: &[Op]) -> Scope {
        let mut s = Scope::default();
        let mut devices: BTreeSet<DeviceId> = BTreeSet::new();
        let mut note = |r: &Representation, s: &mut Scope| {
            if r.page != page {
                return;
            }
            match r.subject {
                Subject::Device(c) => {
                    devices.insert(c);
                    s.connection_points.extend(r.connection_points.keys().copied());
                }
                Subject::Route(x) => {
                    s.routes.insert(x);
                }
                // A continuation's point moves with its arrow.
                Subject::Aid(_) => s.connection_points.extend(r.connection_points.keys().copied()),
                // A cable reference places its cores' named points.
                Subject::Cable(_) => s.connection_points.extend(r.connection_points.keys().copied()),
                Subject::Wire(_) => {}
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
                Op::AddRoute { id, .. } | Op::RemoveRoute { id, .. } | Op::ModifyRoute { id, .. } => {
                    s.routes.insert(*id);
                }
                // New objects; a definition the document lacks cannot be shown by anything in it.
                Op::CreateDevice { id, .. } if !doc.devices().contains_key(id) => {}
                Op::AddConnectionPoint { id, .. } if !doc.connection_points().contains_key(id) => {}
                Op::EmbedDefinition { id, .. } if !doc.definitions().contains_key(id) => {}
                _ => return Scope::everything(doc),
            }
        }
        for c in devices {
            s.connection_points.extend(doc.connection_points_of(c).map(|(t, _)| t));
        }
        s
    }

    fn everything(doc: &Document) -> Scope {
        Scope { connection_points: doc.connection_points().keys().copied().collect(), routes: doc.routes().keys().copied().collect() }
    }
}

/// What [`movement`] needs from the document before the ops: positions of the scope's connection
/// points and the end points of the scope's routes.
struct Earlier {
    positions: BTreeMap<ConnectionPointId, Option<Pt>>,
    /// A route's first and last point, `None` when there was no such route.
    routes: BTreeMap<RouteId, Option<[Pt; 2]>>,
}

/// A wire's line stays one open line.
pub(crate) const ROUTE_SHAPE: &str = "a wire's line stays one open line; draw a new wire instead";

/// What a route end at `at` lands on now: the connection point `found` there,
/// else the interior of another route on `page` (a T), else nothing.
pub(crate) fn landing(doc: &Document, page: PageId, r: RouteId, at: Pt, found: Option<ConnectionPointId>) -> RouteEnd {
    if let Some(t) = found {
        return RouteEnd::Point(t);
    }
    let interior = route_at(doc, page, at, ON_WIRE, Some(r)).filter(|on| {
        doc.routes().get(&on.route).and_then(Route::end_points).is_some_and(|[f, l]| !f.near(on.at, ON_WIRE) && !l.near(on.at, ON_WIRE))
    });
    match interior {
        Some(on) => RouteEnd::On { route: on.route, at },
        None => RouteEnd::Free,
    }
}

/// Path `p` through `points` instead, keeping its kind (a line stays a line).
pub(crate) fn drawn_as(p: &Primitive, points: &[Pt]) -> Primitive {
    match (p, points) {
        (Primitive::Line { .. }, [a, b]) => Primitive::Line { a: *a, b: *b },
        _ => Primitive::Polyline { vertices: points.to_vec(), closed: false },
    }
}

/// Route `r` becomes `after` in `ops`: folded into the op that already adds or changes it, else a
/// new `ModifyRoute` from `before` (the route as the ops found it).
pub(crate) fn set_route(ops: &mut Vec<Op>, r: RouteId, before: Route, after: Route) {
    for op in ops.iter_mut() {
        match op {
            Op::AddRoute { id, route } if *id == r => {
                *route = after;
                return;
            }
            Op::ModifyRoute { id, after: a, .. } if *id == r => {
                *a = after;
                return;
            }
            _ => {}
        }
    }
    if before != after {
        ops.push(Op::ModifyRoute { id: r, before, after });
    }
}

/// Several `ModifyRoute`s of one route, each worked out on the document before the ops (erase
/// follow-ups beside this translation's own), become one: the later ones' changes (points, each
/// end) are laid onto the first. A later one that follows on from the first replaces its result.
pub(crate) fn merge_route_edits(ops: &mut Vec<Op>) {
    let mut first: BTreeMap<RouteId, usize> = BTreeMap::new();
    let mut out: Vec<Op> = Vec::with_capacity(ops.len());
    for op in std::mem::take(ops) {
        if let Op::ModifyRoute { id, before, after } = &op
            && let Some(Op::ModifyRoute { after: a0, .. }) = first.get(id).and_then(|i| out.get_mut(*i))
        {
            if before == a0 {
                *a0 = after.clone();
            } else {
                if after.points != before.points {
                    a0.points = after.points.clone();
                }
                for (slot, (was, now)) in a0.ends.iter_mut().zip(before.ends.iter().zip(after.ends)) {
                    if *was != now {
                        *slot = now;
                    }
                }
            }
            continue;
        }
        if let Op::ModifyRoute { id, .. } = &op {
            first.insert(*id, out.len());
        }
        out.push(op);
    }
    *ops = out;
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
    let rep = |reps: &mut BTreeMap<RepresentationId, Representation>, id: RepresentationId, doc: &Document| -> Result<(), String> {
        if let std::collections::btree_map::Entry::Vacant(e) = reps.entry(id) {
            e.insert(doc.representations().get(&id).cloned().ok_or("representation missing")?);
        }
        Ok(())
    };
    let mut counts = [0usize; 4]; // added, modified, removed, copied components
    // Whether an object with a meaning (an item of a representation, a terminal marker) was removed.
    let mut linked_removed = false;
    // Whether such an object was changed in place (BREAK and TRIM keep the handle since CADCraft
    // 0.18.0).
    let mut linked_changed = false;

    // Removed and changed entities.
    let mut removed: Vec<(Handle, &Arc<Entity>)> = before.iter().filter(|(h, _)| !after.contains_key(h)).map(|(h, e)| (*h, *e)).collect();
    removed.sort_by_key(|(h, _)| *h);

    // Computed parts of a traditional-view cable (core labels at the slash, reference flags)
    // are left out of an edit that also touches other objects: a window selection
    // round a labelled cable moves, copies or erases the rest, and the parts are drawn again from
    // the document. An edit of nothing but such parts is refused.
    let prov: HashMap<Handle, (Source, u32)> = cx.provenance.iter().map(|p| (p.new, (p.source, p.batch))).collect();
    // Wire labels are left out the same way: drawn again from the document.
    // Device cross-references too.
    let drawn = |map: &HandleMap, h: Handle| {
        matches!(map.get(h), Some(Mapped::Fan(k)) if !map.is_fan(k))
            || matches!(map.get(h), Some(Mapped::Label(_) | Mapped::DeviceReference(_) | Mapped::Tee(_)))
    };
    // Device tag texts and terminal labels are left out the same way when changed or
    // copied with other objects; erasing one erases its symbol (below).
    let tag_text = |map: &HandleMap, h: Handle| matches!(map.get(h), Some(Mapped::Tag(_)));
    let copied_source = |h: &Handle, f: &dyn Fn(&HandleMap, Handle) -> bool| match prov.get(h).map(|(s, _)| s) {
        Some(Source::Entity(s)) => f(cx.map, *s),
        Some(Source::Clipboard { serial, handle }) => cx.clips.get(serial).is_some_and(|c| f(&c.map, *handle)),
        None => false,
    };
    let copied_drawn = |h: &Handle| copied_source(h, &|m, x| drawn(m, x) || tag_text(m, x));
    let changed_handles: Vec<Handle> =
        before.iter().filter(|(h, old)| after.get(h).is_some_and(|new| !Arc::ptr_eq(old, new) && ****old != ***new)).map(|(h, _)| *h).collect();
    let added_handles: Vec<Handle> = now_order.iter().copied().filter(|h| !before.contains_key(h)).collect();
    let skipped: BTreeSet<Handle> = removed
        .iter()
        .map(|(h, _)| *h)
        .chain(changed_handles.iter().copied())
        .filter(|h| drawn(cx.map, *h))
        .chain(changed_handles.iter().copied().filter(|h| tag_text(cx.map, *h)))
        // A jumper bar's line moved or copied with other objects is drawn again from its jumper bar.
        .chain(changed_handles.iter().copied().filter(|h| matches!(cx.map.get(*h), Some(Mapped::JumperBar(..)))))
        .chain(added_handles.iter().copied().filter(|h| copied_source(h, &|m, x| matches!(m.get(x), Some(Mapped::JumperBar(..))))))
        .chain(added_handles.iter().copied().filter(|h| copied_drawn(h)))
        .collect();
    // The sheet frame is left out of an edit of other objects too, and drawn again;
    // its copies are dropped. An edit of nothing but the frame is refused.
    let frame = |m: &HandleMap, h: Handle| matches!(m.get(h), Some(Mapped::Frame(_)));
    let framed: BTreeSet<Handle> = removed
        .iter()
        .map(|(h, _)| *h)
        .chain(changed_handles.iter().copied())
        .filter(|h| frame(cx.map, *h))
        .chain(added_handles.iter().copied().filter(|h| copied_source(h, &frame)))
        .collect();
    let touched = removed.len() + changed_handles.len() + added_handles.len();
    if !framed.is_empty() && framed.len() + skipped.len() >= touched {
        return Err(ecad_electrical::pages::frames::DERIVED.into());
    }
    let skipped: BTreeSet<Handle> = skipped.into_iter().chain(framed).collect();
    if !skipped.is_empty() && skipped.len() == touched {
        let tags_only = skipped.iter().all(|h| tag_text(cx.map, *h) || copied_source(h, &tag_text));
        let label = |m: &HandleMap, x: Handle| matches!(m.get(x), Some(Mapped::Label(_)));
        let labels_only = skipped.iter().all(|h| label(cx.map, *h) || copied_source(h, &label));
        let reference = |m: &HandleMap, x: Handle| matches!(m.get(x), Some(Mapped::DeviceReference(_)));
        let references_only = skipped.iter().all(|h| reference(cx.map, *h) || copied_source(h, &reference));
        let jumper_bar = |m: &HandleMap, x: Handle| matches!(m.get(x), Some(Mapped::JumperBar(..)));
        if skipped.iter().all(|h| jumper_bar(cx.map, *h) || copied_source(h, &jumper_bar)) {
            return Err(ecad_electrical::terminal_strips::jumper_bars::JUMPER_BAR_DERIVED.into());
        }
        // A T's dot alone.
        let tee = |m: &HandleMap, x: Handle| matches!(m.get(x), Some(Mapped::Tee(_)));
        if skipped.iter().all(|h| tee(cx.map, *h) || copied_source(h, &tee)) {
            return Err(ecad_electrical::wires::TEE_DERIVED.into());
        }
        return Err(if tags_only {
            ecad_electrical::devices::TAG_DERIVED
        } else if labels_only {
            ecad_electrical::wires::LABEL_DERIVED
        } else if references_only {
            {
                // A copy's handle is not mapped: its source says whether it is a table.
                let source = |h: &Handle| match prov.get(h).map(|(s, _)| s) {
                    Some(Source::Entity(s)) => cx.map.get(*s),
                    Some(Source::Clipboard { serial, handle }) => cx.clips.get(serial).and_then(|c| c.map.get(*handle)),
                    None => None,
                };
                crate::references::draw::devices::refusal(cx.ledger.document(), skipped.iter().find_map(|h| cx.map.get(*h).or_else(|| source(h))))
            }
        } else {
            crate::cables::views::DRAWN_REFUSED
        }
        .into());
    }
    // Reference texts erased: each erases its reference, after the loop.
    let mut erased_references: Vec<RepresentationId> = Vec::new();
    let mut erased_jumper_bars: BTreeSet<(ecad_model::DeviceId, u32)> = BTreeSet::new();
    for (h, _) in &removed {
        match cx.map.get(*h) {
            Some(Mapped::Item(id)) => {
                let item = cx.ledger.document().items().get(&id).cloned().ok_or("mapped item missing")?;
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
            // Left out (above); drawn again from the document.
            Some(Mapped::Fan(_) | Mapped::Label(_) | Mapped::DeviceReference(_) | Mapped::Tee(_)) if skipped.contains(h) => {}
            Some(Mapped::Label(_)) => return Err(ecad_electrical::wires::LABEL_DERIVED.into()),
            Some(Mapped::Tee(_)) => return Err(ecad_electrical::wires::TEE_DERIVED.into()),
            Some(m @ Mapped::DeviceReference(_)) => return Err(crate::references::draw::devices::refusal(cx.ledger.document(), Some(m)).into()),
            // A fan is a view, not an object; it cannot be erased.
            Some(Mapped::Fan(k)) => return Err(crate::cables::views::refusal(cx.map, k).into()),
            Some(Mapped::RefText(rid)) => erased_references.push(rid),
            // Erasing a jumper bar's line deletes the jumper bar (and its own part keys).
            Some(Mapped::JumperBar(c, n)) => {
                erased_jumper_bars.insert((c, n));
            }
            Some(Mapped::Frame(_)) => {}
            // Erasing a device's tag text or terminal label erases that symbol.
            Some(Mapped::Tag(rid)) => erased_references.push(rid),
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
    // Erasing a reference's text erases the reference: its point goes too, unless
    // it was erased with it, and the emptied representation is removed below.
    for (c, n) in erased_jumper_bars {
        let props = cx.ledger.document().devices().get(&c).map(|x| x.properties.clone()).unwrap_or_default();
        let key = ecad_model::connectivity::jumper_bars::key(n);
        if let Some(v) = props.get(&key) {
            t.notes.push(format!("Deleted jumper bar {} {v}.", ecad_electrical::devices::tags::tag_words(cx.ledger.document(), c)));
        }
        for k in std::iter::once(key.clone()).chain(ecad_electrical::terminal_strips::accessories::FIELDS.iter().map(|f| format!("{key}.{f}"))) {
            if let Some(before) = props.get(&k).cloned() {
                t.ops.push(Op::SetProperty { object: ecad_model::ObjectRef::Device(c), key: k, before: Some(before), after: None });
            }
        }
    }
    for rid in erased_references {
        linked_removed = true;
        rep(&mut reps, rid, cx.ledger.document())?;
        let items: Vec<ItemId> = reps.get(&rid).map(|r| r.items.clone()).unwrap_or_default();
        for id in items {
            let item = cx.ledger.document().items().get(&id).cloned().ok_or("mapped item missing")?;
            t.ops.push(Op::RemoveItem { id, item });
            counts[2] += 1;
            if let Some(h) = cx.map.item(id) {
                t.amended.insert(h);
            }
        }
        if let Some(r) = reps.get_mut(&rid) {
            r.items.clear();
        }
    }
    let mut changed: Vec<(Handle, &Arc<Entity>)> =
        before.iter().filter_map(|(h, old)| after.get(h).filter(|new| !Arc::ptr_eq(old, new) && **old != ***new).map(|new| (*h, *new))).collect();
    changed.sort_by_key(|(h, _)| *h);
    for (h, new) in &changed {
        match cx.map.get(*h) {
            Some(Mapped::Item(id)) => {
                let item = item_from(cx.ledger.document(), cx.page, new)?;
                let old = cx.ledger.document().items().get(&id).cloned().ok_or("mapped item missing")?;
                if std::mem::discriminant(&item.kind) != std::mem::discriminant(&old.kind) {
                    return Err("an object changed kind; this is not supported on ECAD pages yet".into());
                }
                if item != old {
                    // A location box stays a closed outline of three points or more.
                    if let Some(m) = ecad_electrical::locations::reshape_refusal(cx.ledger.document(), id, &item) {
                        return Err(m);
                    }
                    linked_changed |= cx.ledger.document().representation_of_item(id).is_some();
                    if let Some((rid, points)) = reference_points_follow(cx.ledger.document(), id, &old, &item) {
                        rep(&mut reps, rid, cx.ledger.document())?;
                        if let Some(r) = reps.get_mut(&rid) {
                            r.connection_points.extend(points);
                        }
                    }
                    // A route's points follow its line.
                    let route = route_edit(cx.ledger.document(), id, &item)?;
                    t.ops.push(Op::ModifyItem { id, before: old, after: item });
                    t.ops.extend(route);
                    counts[1] += 1;
                }
            }
            Some(Mapped::Fan(_) | Mapped::Label(_) | Mapped::DeviceReference(_) | Mapped::Tee(_)) if skipped.contains(h) => {}
            Some(Mapped::Label(_)) => return Err(ecad_electrical::wires::LABEL_DERIVED.into()),
            Some(Mapped::Tee(_)) => return Err(ecad_electrical::wires::TEE_DERIVED.into()),
            Some(m @ Mapped::DeviceReference(_)) => return Err(crate::references::draw::devices::refusal(cx.ledger.document(), Some(m)).into()),
            Some(Mapped::Fan(k)) => return Err(crate::cables::views::refusal(cx.map, k).into()),
            Some(Mapped::RefText(_)) => return Err(ecad_electrical::references::cables::TEXT_DERIVED.into()),
            Some(Mapped::Frame(_)) => {}
            Some(Mapped::JumperBar(..)) if skipped.contains(h) => {}
            Some(Mapped::JumperBar(..)) => return Err(ecad_electrical::terminal_strips::jumper_bars::JUMPER_BAR_DERIVED.into()),
            Some(Mapped::Tag(_)) if skipped.contains(h) => {}
            Some(Mapped::Tag(_)) => return Err(ecad_electrical::devices::TAG_DERIVED.into()),
            Some(Mapped::Marker(rid, term)) => {
                let at = marker_position(new).ok_or("connection point markers can only be moved")?;
                if marker_entity(at, *h) != ***new {
                    return Err("connection point markers can only be moved".into());
                }
                rep(&mut reps, rid, cx.ledger.document())?;
                if let Some(r) = reps.get_mut(&rid) {
                    r.connection_points.insert(term, at);
                }
            }
            None => return Err("an object this page does not know about changed".into()),
        }
    }

    // Added entities: with provenance (copies, pastes) or new drawing; copies of left-out parts
    // are dropped.
    let added: Vec<Handle> = added_handles.into_iter().filter(|h| !skipped.contains(h)).collect();
    let mut batches: BTreeMap<u32, Vec<(Handle, Source)>> = BTreeMap::new();
    let mut fresh_route_ends: Vec<(RouteId, End)> = Vec::new();
    // Removing a meaningful object while creating new ones without provenance is a replacement
    // (BREAK, JOIN, EXPLODE, TRIM that re-creates…): what the new objects mean is undecided (L1).
    // Changing one in place while creating new ones is the same split (BREAK and TRIM keep the
    // object's handle for one of the pieces since CADCraft 0.18.0).
    if (linked_removed || linked_changed) && added.iter().any(|h| !prov.contains_key(h)) {
        return Err("this command replaces objects that have an electrical meaning; that is not supported on ECAD pages yet".into());
    }
    for h in &added {
        match prov.get(h) {
            Some((src, batch)) => batches.entry(*batch).or_default().push((*h, *src)),
            None => {
                let e = after.get(h).ok_or("added entity missing")?;
                if e.common.layer == MARKER_LAYER {
                    return Err(format!("the {MARKER_LAYER} layer is reserved for connection point markers"));
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
        copy_batch(cx, &after, &batch, &mut t, &mut fresh_route_ends, &mut counts)?;
    }

    // Representation edits: an emptied representation is removed (its subject stays). A
    // symbol's connection points are in its definition's frame: with no symbol left
    // they place nothing. A cable reference's named points are placed by
    // its point: with the point erased, the reference goes with them.
    let mut unlinked: Vec<(RepresentationId, Representation)> = Vec::new();
    for (id, r) in reps {
        let old = cx.ledger.document().representations().get(&id).cloned().ok_or("representation missing")?;
        if r.items.is_empty() && (r.connection_points.is_empty() || r.form == Form::Symbol || matches!(r.subject, Subject::Cable(_))) {
            unlinked.push((id, old.clone()));
            t.ops.push(Op::UnlinkRepresentation { id, rep: old });
        } else if r != old {
            t.ops.push(Op::ModifyRepresentation { id, before: old, after: r });
        }
    }

    // Erased device symbols: the terminals only they showed go, and a device
    // whose last symbol went is deleted.
    let (more, notes) = ecad_electrical::devices::merge::erase_followups(cx.ledger.document(), &unlinked);
    t.ops.extend(more);
    t.notes.extend(notes);
    // One of a cable's several slashes erased takes its page's members along.
    t.ops.extend(ecad_electrical::references::cables::slash_erase_followups(cx.ledger.document(), &unlinked));
    // An erased line removes its route; routes landing on it end free.
    erase_routes(cx.ledger.document(), &unlinked, &mut t.ops);
    merge_route_edits(&mut t.ops);

    // Left-out parts beside objects that recorded nothing: nothing else was edited after all.
    if !skipped.is_empty() && t.ops.is_empty() {
        return Err(crate::cables::views::DRAWN_REFUSED.into());
    }

    movement(cx, &mut t, &fresh_route_ends)?;
    preview(cx.ledger, &t.ops, |_| ())?;
    t.summary = format!("{} added, {} modified, {} removed, {} components copied", counts[0], counts[1], counts[2], counts[3]);
    Ok(t)
}

/// The `ModifyRoute` that keeps route points equal to a changed item's vertices, when the item
/// draws a route (none when it draws something else or the points are the same).
///
/// # Errors
/// [`ROUTE_SHAPE`] when the item is no longer an open path.
pub(crate) fn route_edit(doc: &Document, id: ItemId, item: &Item) -> Result<Option<Op>, &'static str> {
    let Some((_, rep)) = doc.representation_of_item(id) else { return Ok(None) };
    let Subject::Route(r) = rep.subject else { return Ok(None) };
    let points = match &item.kind {
        ItemKind::Primitive(p) => path_vertices(p).ok_or(ROUTE_SHAPE)?,
        ItemKind::Symbol(_) => return Err(ROUTE_SHAPE),
    };
    let before = doc.routes().get(&r).cloned().ok_or("route missing")?;
    if before.points == points {
        return Ok(None);
    }
    let after = Route { points, ..before.clone() };
    Ok(Some(Op::ModifyRoute { id: r, before, after }))
}

/// A cable reference's named points follow its point: when item `id` is a
/// reference's point moved from `old` to `item`, that reference and its connection points moved
/// by the same distance (as the document has them, so a moved marker of one of them agrees).
pub(crate) fn reference_points_follow(
    doc: &Document, id: ItemId, old: &Item, item: &Item,
) -> Option<(RepresentationId, BTreeMap<ConnectionPointId, Pt>)> {
    let (rid, rep) = doc.representation_of_item(id)?;
    if !matches!(rep.subject, Subject::Cable(_)) || rep.connection_points.is_empty() {
        return None;
    }
    let (ItemKind::Primitive(Primitive::Point { at: a }), ItemKind::Primitive(Primitive::Point { at: b })) = (&old.kind, &item.kind) else {
        return None;
    };
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    Some((rid, rep.connection_points.iter().map(|(t, p)| (*t, Pt::new(p.x + dx, p.y + dy))).collect()))
}

/// The routes whose drawing `unlinked` removed go too (`RemoveRoute`), after the routes landing
/// on them end free where they are; a change planned for a removed route is dropped.
pub(crate) fn erase_routes(doc: &Document, unlinked: &[(RepresentationId, Representation)], ops: &mut Vec<Op>) {
    let gone: BTreeSet<RouteId> = unlinked.iter().filter_map(|(_, r)| if let Subject::Route(x) = r.subject { Some(x) } else { None }).collect();
    if gone.is_empty() {
        return;
    }
    ops.retain(|op| !matches!(op, Op::ModifyRoute { id, .. } if gone.contains(id)));
    for r in &gone {
        for tee in doc.tees_of(*r).filter(|x| !gone.contains(x)).collect::<Vec<_>>() {
            let Some(before) = doc.routes().get(&tee) else { continue };
            let mut after = ops
                .iter()
                .find_map(|op| match op {
                    Op::ModifyRoute { id, after, .. } if *id == tee => Some(after.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| before.clone());
            for e in after.ends.iter_mut().filter(|e| e.route() == Some(*r)) {
                *e = RouteEnd::Free;
            }
            set_route(ops, tee, before.clone(), after);
        }
    }
    for r in gone {
        if let Some(route) = doc.routes().get(&r) {
            ops.push(Op::RemoveRoute { id: r, route: route.clone() });
        }
    }
}

/// What a paste reads of the page objects were copied from, as it was when they were put on the clipboard: the
/// page's representations, the devices they show with their connection points and definitions, and the routes
/// they draw. A paste reads nothing else, so the rest of the document is not kept.
#[derive(Debug, Clone, Default)]
pub(crate) struct ClipPage {
    representations: BTreeMap<RepresentationId, Representation>,
    devices: BTreeMap<DeviceId, Device>,
    connection_points: BTreeMap<DeviceId, Vec<(ConnectionPointId, ConnectionPoint)>>,
    definitions: BTreeMap<DefinitionId, Definition>,
    routes: BTreeMap<RouteId, Route>,
}

impl ClipPage {
    /// The objects of `page` in `doc` a paste reads.
    pub(crate) fn of(doc: &Document, page: PageId) -> ClipPage {
        let mut c = ClipPage::default();
        for (id, r) in doc.representations_on(page) {
            match r.subject {
                Subject::Device(d) => {
                    if let Some(device) = doc.devices().get(&d) {
                        if let Some(def) = device.definition
                            && let Some(x) = doc.definitions().get(&def)
                        {
                            c.definitions.insert(def, x.clone());
                        }
                        c.connection_points.insert(d, doc.connection_points_of(d).map(|(t, x)| (t, x.clone())).collect());
                        c.devices.insert(d, device.clone());
                    }
                }
                Subject::Route(route) => {
                    if let Some(x) = doc.routes().get(&route) {
                        c.routes.insert(route, x.clone());
                    }
                }
                Subject::Wire(_) | Subject::Cable(_) | Subject::Aid(_) => {}
            }
            c.representations.insert(id, r.clone());
        }
        c
    }
}

/// Where copied objects come from: the live document, or the page kept when they were put on the clipboard.
#[derive(Clone, Copy)]
enum CopyFrom<'a> {
    Live(&'a Document),
    Clip(&'a ClipPage),
}

impl<'a> CopyFrom<'a> {
    /// The representations on `page`, in ID order.
    fn representations_on(self, page: PageId) -> Vec<(&'a RepresentationId, &'a Representation)> {
        let map = match self {
            CopyFrom::Live(d) => d.representations(),
            CopyFrom::Clip(c) => &c.representations,
        };
        map.iter().filter(|(_, r)| r.page == page).collect()
    }

    fn device(self, id: DeviceId) -> Option<&'a Device> {
        match self {
            CopyFrom::Live(d) => d.devices().get(&id),
            CopyFrom::Clip(c) => c.devices.get(&id),
        }
    }

    fn definition(self, id: DefinitionId) -> Option<&'a Definition> {
        match self {
            CopyFrom::Live(d) => d.definitions().get(&id),
            CopyFrom::Clip(c) => c.definitions.get(&id),
        }
    }

    fn connection_points_of(self, device: DeviceId) -> Vec<(ConnectionPointId, &'a ConnectionPoint)> {
        match self {
            CopyFrom::Live(d) => d.connection_points_of(device).collect(),
            CopyFrom::Clip(c) => c.connection_points.get(&device).map(|v| v.iter().map(|(t, x)| (*t, x)).collect()).unwrap_or_default(),
        }
    }

    fn route(self, id: RouteId) -> Option<&'a Route> {
        match self {
            CopyFrom::Live(d) => d.routes().get(&id),
            CopyFrom::Clip(c) => c.routes.get(&id),
        }
    }
}

/// One copy operation's new entities.
fn copy_batch(
    cx: &mut Ctx<'_>, after: &HashMap<Handle, &Arc<Entity>>, batch: &[(Handle, Source)], t: &mut Translation, fresh_ends: &mut Vec<(RouteId, End)>,
    counts: &mut [usize; 4],
) -> Result<(), String> {
    // Resolve sources against the live document or the clipboard snapshot.
    let mut src_doc: Option<CopyFrom<'_>> = None;
    let mut src_map: Option<&HandleMap> = None;
    let mut by_source: HashMap<Handle, Handle> = HashMap::new();
    for (new, src) in batch {
        let (doc, map, h) = match src {
            Source::Entity(h) => (CopyFrom::Live(cx.ledger.document()), cx.map, *h),
            Source::Clipboard { serial, handle } => {
                let snap = cx.clips.get(serial).ok_or("the clipboard was filled before this project recorded it; copy again")?;
                if snap.page != cx.page {
                    return Err("pasting onto another page is not supported yet; paste on the page you copied from".into());
                }
                (CopyFrom::Clip(&snap.source), &snap.map, *handle)
            }
        };
        match map.get(h) {
            Some(Mapped::Fan(k)) => return Err(crate::cables::views::refusal(map, k).into()),
            Some(Mapped::RefText(_)) => return Err(ecad_electrical::references::cables::TEXT_DERIVED.into()),
            Some(Mapped::Frame(_)) => return Err(ecad_electrical::pages::frames::DERIVED.into()),
            Some(Mapped::Tee(_)) => return Err(ecad_electrical::wires::TEE_DERIVED.into()),
            _ => {}
        }
        src_doc = Some(doc);
        src_map = Some(map);
        by_source.insert(h, *new);
    }
    let (Some(sdoc), Some(smap)) = (src_doc, src_map) else { return Ok(()) };
    let mut consumed: BTreeSet<Handle> = BTreeSet::new();
    let mut point_map: BTreeMap<ConnectionPointId, ConnectionPointId> = BTreeMap::new();

    // Whole component representations: independent copies.
    for (rid, r) in sdoc.representations_on(cx.page) {
        let Subject::Device(src_comp) = r.subject else { continue };
        let item_handles: Vec<Handle> = r.items.iter().filter_map(|i| smap.item(*i)).collect();
        let marker_handles: Vec<(ConnectionPointId, Handle)> =
            r.connection_points.keys().filter_map(|tm| smap.marker(*rid, *tm).map(|h| (*tm, h))).collect();
        let all_copied = item_handles.len() == r.items.len()
            // A symbol's connection points are in its definition's frame: it has no markers.
            && marker_handles.len() == if r.form == Form::Drawn { r.connection_points.len() } else { 0 }
            && item_handles.iter().chain(marker_handles.iter().map(|(_, h)| h)).all(|h| by_source.contains_key(h));
        if !all_copied || item_handles.is_empty() {
            continue;
        }
        let comp = sdoc.device(src_comp).cloned().ok_or("source component missing")?;
        if let Some(def) = comp.definition
            && !cx.ledger.document().definitions().contains_key(&def)
        {
            let d = sdoc.definition(def).cloned().ok_or("source definition missing")?;
            t.ops.push(Op::EmbedDefinition { id: def, definition: d });
        }
        let new_comp: DeviceId = cx.ids.fresh();
        // A copied device gets the next tag of its prefix.
        let mut comp = comp;
        if !comp.tag.is_none() {
            let tag = ecad_electrical::devices::merge::copy_tag(cx.ledger.document(), cx.tags, &comp);
            comp = match tag {
                Some(g) => comp.with_tag(Some(&g.text), g.state),
                None => comp.with_tag(None, ecad_model::naming::tag::TagState::Automatic),
            };
        }
        t.ops.push(Op::CreateDevice { id: new_comp, device: Device { ..comp } });
        for (src_t, term) in sdoc.connection_points_of(src_comp) {
            let nt: ConnectionPointId = cx.ids.fresh();
            point_map.insert(src_t, nt);
            t.ops.push(Op::AddConnectionPoint { id: nt, connection_point: ConnectionPoint { owner: new_comp.into(), ..term.clone() } });
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
        if r.form == Form::Symbol {
            for (src_t, at) in &r.connection_points {
                cps.insert(point_map.get(src_t).copied().ok_or("connection point not copied")?, *at);
            }
        }
        for (src_t, sh) in &marker_handles {
            let nh = by_source.get(sh).copied().ok_or("copy missing")?;
            let e = after.get(&nh).ok_or("copied marker missing")?;
            let at = marker_position(e).ok_or("copied marker is not a marker")?;
            let nt = point_map.get(src_t).copied().ok_or("connection point not copied")?;
            cps.insert(nt, at);
            t.new_markers.push(((new_rep, nt), nh));
            consumed.insert(nh);
        }
        // The copy keeps its symbol's own text keys, under the new representation.
        for op in t.ops.iter_mut().rev() {
            if let Op::CreateDevice { id, device: component } = op
                && *id == new_comp
            {
                ecad_electrical::devices::text_place::copy_symbol_keys(&mut component.properties, *rid, new_rep);
                break;
            }
        }
        t.ops.push(Op::LinkRepresentation {
            id: new_rep,
            rep: Representation { subject: Subject::Device(new_comp), page: cx.page, form: r.form, role: r.role, items, connection_points: cps },
        });
        counts[3] += 1;
    }

    // Whole routes: new routes; ends follow copied connection points, else are resolved later.
    for (_, r) in sdoc.representations_on(cx.page) {
        let Subject::Route(src_r) = r.subject else { continue };
        let handles: Vec<Handle> = r.items.iter().filter_map(|i| smap.item(*i)).collect();
        if handles.is_empty() || handles.len() != r.items.len() || !handles.iter().all(|h| by_source.contains_key(h)) {
            continue;
        }
        let src = sdoc.route(src_r).cloned().ok_or("source route missing")?;
        let new_r: RouteId = cx.ids.fresh();
        let new_rep: RepresentationId = cx.ids.fresh();
        let mut items = Vec::new();
        let mut added = Vec::new();
        let mut points: Option<Vec<Pt>> = None;
        for sh in &handles {
            let nh = by_source.get(sh).copied().ok_or("copy missing")?;
            let e = after.get(&nh).ok_or("copied entity missing")?;
            let item = item_from(cx.ledger.document(), cx.page, e)?;
            if points.is_none() {
                points = Some(match &item.kind {
                    ItemKind::Primitive(p) => path_vertices(p).ok_or(ROUTE_SHAPE)?,
                    ItemKind::Symbol(_) => return Err(ROUTE_SHAPE.into()),
                });
            }
            let id: ItemId = cx.ids.fresh();
            added.push(Op::AddItem { id, item });
            t.new_items.push((id, nh));
            items.push(id);
            consumed.insert(nh);
        }
        let points = points.ok_or(ROUTE_SHAPE)?;
        let mut ends = [RouteEnd::Free; 2];
        for ((end, slot), src_end) in End::BOTH.into_iter().zip(ends.iter_mut()).zip(src.ends) {
            match src_end.point().and_then(|src_t| point_map.get(&src_t)) {
                Some(nt) => *slot = RouteEnd::Point(*nt),
                None => fresh_ends.push((new_r, end)),
            }
        }
        t.ops.push(Op::AddRoute { id: new_r, route: Route::new(cx.page, points, ends) });
        t.ops.extend(added);
        t.ops.push(Op::LinkRepresentation {
            id: new_rep,
            rep: Representation {
                subject: Subject::Route(new_r),
                page: cx.page,
                form: Form::Drawn,
                role: r.role,
                items,
                connection_points: BTreeMap::new(),
            },
        });
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
            return Err("copying connection point markers without their whole component is not supported yet".into());
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

/// Movement policy. Adds route changes and their line amendments to `t`.
///
/// # Semantics
/// Goes through the routes of the proposed document in ID order, but only those [`Scope`] says
/// could have moved: routes landing on a connection point whose position the ops could change,
/// routes the ops change, routes landing on those, and routes with a fresh end. Any other route
/// has the same points, ends, landings and point positions before and after the ops, so the
/// policy gives it nothing to do. Positions are worked out per point, only for the scope's
/// points; the page's point positions, which a moved route end is matched against (M3), are
/// listed at most once per edit.
fn movement(cx: &mut Ctx<'_>, t: &mut Translation, fresh_ends: &[(RouteId, End)]) -> Result<(), String> {
    let (page, tol) = (cx.page, cx.tol);
    // Before the ops: what they could move, and where it is now.
    let doc = cx.ledger.document();
    let scope = Scope::of(doc, page, &t.ops);
    let earlier = Earlier {
        positions: scope.connection_points.iter().map(|term| (*term, connection_point_position(doc, page, *term))).collect(),
        routes: scope.routes.iter().map(|r| (*r, doc.routes().get(r).and_then(Route::end_points))).collect(),
    };
    // After the ops, on the live document, rolled back afterwards.
    let moves = preview(cx.ledger, &t.ops, |proposed| moved(proposed, page, tol, &scope, &earlier, fresh_ends))?;
    apply_moves(cx.map, t, moves);
    Ok(())
}

/// Fold the policy's changes into `t`: each line amendment into the op that already touches the
/// item, else a modification; each route change likewise ([`set_route`]).
pub(crate) fn apply_moves(map: &HandleMap, t: &mut Translation, (amend, routes): Moves) {
    for (id, (before, after)) in amend {
        let handle = map.item(id).or_else(|| t.new_items.iter().find(|(i, _)| *i == id).map(|(_, h)| *h));
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
    for (r, (before, after)) in routes {
        set_route(&mut t.ops, r, before, after);
    }
}

/// Route lines to amend (their item in `proposed`, and the amended item) and routes to change
/// (as in `proposed`, and changed), as the policy produces them.
pub(crate) type Moves = (BTreeMap<ItemId, (Item, Item)>, BTreeMap<RouteId, (Route, Route)>);

/// The movement policy on the proposed document, for the routes `scope` names.
fn moved(proposed: &Document, page: PageId, tol: f64, scope: &Scope, earlier: &Earlier, fresh_ends: &[(RouteId, End)]) -> Moves {
    let mut candidates: BTreeSet<RouteId> = scope.routes.clone();
    candidates.extend(fresh_ends.iter().map(|(r, _)| *r));
    for term in &scope.connection_points {
        candidates.extend(proposed.routes_at(*term));
    }
    for r in &scope.routes {
        candidates.extend(proposed.tees_of(*r));
    }
    let mut shown = ShownOnce::new(proposed, page);
    let mut found = |at: Pt| shown.connection_point_at(at, tol);
    let mut out: Moves = Default::default();
    for r in &candidates {
        let Some(route) = proposed.routes().get(r).filter(|x| x.page == page) else { continue };
        // A route outside the scope's routes kept its points.
        let old_ends = match earlier.routes.get(r) {
            Some(old) => *old,
            None => route.end_points(),
        };
        // A connection point outside the scope is where it was.
        let position = |term: ConnectionPointId| match earlier.positions.get(&term) {
            Some(op) => (*op, connection_point_position(proposed, page, term)),
            None => (None, None),
        };
        let after = route_moved(proposed, page, *r, route, old_ends, fresh_ends, position, &mut found);
        record(proposed, *r, route, after, &mut out);
    }
    out
}

/// One route under the movement policy: `old_ends` are its end points
/// before the ops (`None` for a route the ops made), `position` gives a connection point's
/// position before and after them, and `found` the connection point shown at a place.
///
/// - M2: an end on a connection point that moved follows it.
/// - M3: an end that moved (or is fresh) lands on what is there now ([`landing`]): a connection
///   point, another route's interior, or nothing.
/// - An end on another route that no longer passes there lands on what is there now the same way.
#[allow(clippy::too_many_arguments)]
pub(crate) fn route_moved(
    proposed: &Document, page: PageId, r: RouteId, route: &Route, old_ends: Option<[Pt; 2]>, fresh_ends: &[(RouteId, End)],
    position: impl Fn(ConnectionPointId) -> (Option<Pt>, Option<Pt>), found: &mut dyn FnMut(Pt) -> Option<ConnectionPointId>,
) -> Route {
    let eps = 1e-9;
    let mut after = route.clone();
    let Some(new_ends) = route.end_points() else { return after };
    for (i, end) in End::BOTH.into_iter().enumerate() {
        let (Some(new_end), Some(landed)) = (new_ends.get(i).copied(), route.ends.get(i).copied()) else { continue };
        let fresh = fresh_ends.contains(&(r, end));
        let end_moved = fresh || old_ends.and_then(|o| o.get(i).copied()).is_some_and(|o| !o.near(new_end, eps));
        let snapped = match landed {
            RouteEnd::Point(term) => match position(term) {
                (Some(a), Some(np)) if !a.near(np, eps) => {
                    // M2: the end follows its connection point.
                    if !new_end.near(np, eps) {
                        let slot = if end == End::A { after.points.first_mut() } else { after.points.last_mut() };
                        if let Some(p) = slot {
                            *p = np;
                        }
                    }
                    None
                }
                // M3: a deliberate reconnection, resolved once.
                _ if end_moved => Some(landing(proposed, page, r, new_end, found(new_end))),
                _ => None,
            },
            RouteEnd::On { route: x, .. } => {
                (end_moved || on_route_of(proposed, x, new_end).is_none()).then(|| landing(proposed, page, r, new_end, found(new_end)))
            }
            RouteEnd::Free => end_moved.then(|| landing(proposed, page, r, new_end, found(new_end))),
        };
        if let (Some(e), Some(slot)) = (snapped, after.ends.get_mut(i)) {
            *slot = e;
        }
    }
    after
}

/// Note route `r`'s change (`route` in the proposed document, `after` as the policy left it) in
/// `out`, with its line's amendment when its points changed.
pub(crate) fn record(proposed: &Document, r: RouteId, route: &Route, after: Route, out: &mut Moves) {
    if after == *route {
        return;
    }
    if after.points != route.points
        && let Some((_, id, item)) = route_item(proposed, r)
        && let ItemKind::Primitive(p) = &item.kind
    {
        out.0.insert(id, (item.clone(), Item { kind: ItemKind::Primitive(drawn_as(p, &after.points)), ..item.clone() }));
    }
    out.1.insert(r, (route.clone(), after));
}
