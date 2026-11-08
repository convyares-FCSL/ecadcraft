//! The translation as it was before brief 014, kept as a **test oracle** (feature
//! `translate-oracle`, never enabled in normal builds).
//!
//! Brief 014 made [`crate::translate::translate`] preview in place and go through only the
//! conductors an edit could move. With the feature on, every translation also runs this copy of
//! the old code on the same inputs and compares the two results byte for byte (their `Debug`
//! text: ops, new handles, amended handles, summary or refusal, and the ID allocator's state
//! afterwards). Any difference is printed and **aborts the process**, so no test, not even one
//! that expects a refusal or runs inside the engine's panic guard, can pass over it.
//!
//! Below the comparison, the code is the pre-014 `translate.rs` unchanged, except that its
//! context is named [`OldCtx`] and it uses the shared [`Translation`] type.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use cadcraft_engine::doc::{Drawing, Entity, Handle};
use cadcraft_engine::ext::{Provenance, Source};
use ecad_model::derive::{route_on_page, terminal_positions};
use ecad_model::{
    Component, ComponentId, Conductor, ConductorId, Document, End, Form, IdAllocator, Item, ItemId, ItemKind, Op,
    PageId, Pt, Representation, RepresentationId, Subject, Terminal, TerminalId,
};

use crate::convert::{ECAD_PREFIX, MARKER_LAYER, item_from, marker_entity, marker_position};
use crate::project::{ClipSnapshot, HandleMap, Mapped};
use crate::translate::Translation;

static COMPARED: AtomicU64 = AtomicU64::new(0);
static ACCEPTED: AtomicU64 = AtomicU64::new(0);

/// How many translations this process has compared with the oracle (all of them agreed: a
/// difference aborts the process).
pub fn comparisons() -> u64 {
    COMPARED.load(Ordering::Relaxed)
}

/// How many of [`comparisons`] were accepted with at least one op (not refused, not empty).
pub fn accepted() -> u64 {
    ACCEPTED.load(Ordering::Relaxed)
}

fn fail(what: &str) -> ! {
    eprintln!("translate-oracle: {what}");
    std::process::abort();
}

/// After a preview: the live document, its indexes included, must be exactly as before it.
pub(crate) fn unchanged(before: &Document, after: &Document) {
    if format!("{before:?}") != format!("{after:?}") || after.check_index().is_err() {
        fail(&format!("a preview left the document changed\n--- before\n{before:?}\n--- after\n{after:?}"));
    }
}

/// `terminal_position` must give the first `terminal_positions` entry of every terminal shown on
/// `page` (they share their per-representation code; this proves it on every translation).
fn positions_agree(doc: &Document, page: PageId) {
    let mut first: BTreeMap<TerminalId, Pt> = BTreeMap::new();
    for (t, _, p) in terminal_positions(doc, page) {
        first.entry(t).or_insert(p);
    }
    for (t, p) in &first {
        let one = ecad_model::derive::terminal_position(doc, page, *t);
        if one != Some(*p) {
            fail(&format!("terminal_position({t:?}) = {one:?}, terminal_positions gives {p:?}"));
        }
    }
}

/// Inputs of one translation, as the old code reads them.
pub(crate) struct Inputs<'a> {
    pub doc: &'a Document,
    pub page: PageId,
    pub map: &'a HandleMap,
    pub last: &'a Drawing,
    pub now: &'a Drawing,
    pub provenance: &'a [Provenance],
    pub clips: &'a BTreeMap<u64, ClipSnapshot>,
    pub tol: f64,
    pub tags: crate::devices::TagHigh,
}

/// Run the old translation on `inputs` with `ids_before` (the allocator as the new translation
/// found it) and compare with what the new one returned and left in `ids_after`.
pub(crate) fn compare(
    inputs: Inputs<'_>,
    ids_before: IdAllocator,
    ids_after: &IdAllocator,
    new: &Result<Translation, String>,
) {
    let mut ids = ids_before;
    let mut tags = inputs.tags;
    let old = translate(&mut OldCtx {
        doc: inputs.doc,
        page: inputs.page,
        map: inputs.map,
        last: inputs.last,
        now: inputs.now,
        provenance: inputs.provenance,
        clips: inputs.clips,
        ids: &mut ids,
        tol: inputs.tol,
        tags: &mut tags,
    });
    let (old_text, new_text) = (format!("{old:?}"), format!("{new:?}"));
    let (old_ids, new_ids) = (format!("{ids:?}"), format!("{ids_after:?}"));
    if old_text != new_text || old_ids != new_ids {
        eprintln!(
            "translate-oracle: the brief 014 translation differs from the old one\n--- old\n{old_text}\n{old_ids}\n--- \
             new\n{new_text}\n{new_ids}"
        );
        std::process::abort();
    }
    positions_agree(inputs.doc, inputs.page);
    COMPARED.fetch_add(1, Ordering::Relaxed);
    if new.as_ref().is_ok_and(|t| !t.ops.is_empty()) {
        ACCEPTED.fetch_add(1, Ordering::Relaxed);
    }
    log(new);
}

/// With `ECAD_TRANSLATE_ORACLE_LOG=<file>`, append one line per agreeing comparison (refused, or
/// the number of ops), so a whole test run's comparisons can be counted across test binaries.
fn log(new: &Result<Translation, String>) {
    use std::io::Write;
    let Some(path) = std::env::var_os("ECAD_TRANSLATE_ORACLE_LOG") else { return };
    let line = match new {
        Ok(t) => format!("ops {}\n", t.ops.len()),
        Err(_) => "refused\n".to_string(),
    };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Everything the translation reads.
pub(crate) struct OldCtx<'a> {
    pub doc: &'a Document,
    pub page: PageId,
    pub map: &'a HandleMap,
    pub last: &'a Drawing,
    pub now: &'a Drawing,
    pub provenance: &'a [Provenance],
    pub clips: &'a BTreeMap<u64, ClipSnapshot>,
    pub ids: &'a mut IdAllocator,
    pub tol: f64,
    pub tags: &'a mut crate::devices::TagHigh,
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
fn terminal_at(doc: &Document, page: PageId, at: Pt, tol: f64) -> Option<TerminalId> {
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

fn translate(cx: &mut OldCtx<'_>) -> Result<Translation, String> {
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

    // Computed parts of a traditional-view cable (core labels at the slash, reference flags;
    // design 009 §8) are left out of an edit that also touches other objects: a window selection
    // round a labelled cable moves, copies or erases the rest, and the parts are drawn again from
    // the document. An edit of nothing but such parts is refused (brief 009 review S2).
    let prov: HashMap<Handle, (Source, u32)> = cx.provenance.iter().map(|p| (p.new, (p.source, p.batch))).collect();
    let drawn = |map: &HandleMap, h: Handle| matches!(map.get(h), Some(Mapped::Fan(k)) if !map.is_fan(k));
    // Device tag texts and terminal labels (brief 018) are left out the same way when changed or
    // copied with other objects; erasing one erases its symbol (below).
    let tag_text = |map: &HandleMap, h: Handle| matches!(map.get(h), Some(Mapped::Tag(_)));
    let copied_source = |h: &Handle, f: &dyn Fn(&HandleMap, Handle) -> bool| match prov.get(h).map(|(s, _)| s) {
        Some(Source::Entity(s)) => f(cx.map, *s),
        Some(Source::Clipboard { serial, handle }) => cx.clips.get(serial).is_some_and(|c| f(&c.map, *handle)),
        None => false,
    };
    let copied_drawn = |h: &Handle| copied_source(h, &|m, x| drawn(m, x) || tag_text(m, x));
    let changed_handles: Vec<Handle> = before
        .iter()
        .filter(|(h, old)| after.get(h).is_some_and(|new| !Arc::ptr_eq(old, new) && ****old != ***new))
        .map(|(h, _)| *h)
        .collect();
    let added_handles: Vec<Handle> = now_order.iter().copied().filter(|h| !before.contains_key(h)).collect();
    let skipped: BTreeSet<Handle> = removed
        .iter()
        .map(|(h, _)| *h)
        .chain(changed_handles.iter().copied())
        .filter(|h| drawn(cx.map, *h))
        .chain(changed_handles.iter().copied().filter(|h| tag_text(cx.map, *h)))
        .chain(added_handles.iter().copied().filter(|h| copied_drawn(h)))
        .collect();
    let touched = removed.len() + changed_handles.len() + added_handles.len();
    if !skipped.is_empty() && skipped.len() == touched {
        let tags_only = skipped.iter().all(|h| tag_text(cx.map, *h) || copied_source(h, &tag_text));
        return Err(if tags_only { crate::devices::TAG_DERIVED } else { crate::views::DRAWN_REFUSED }.into());
    }
    // Reference texts erased (brief 010): each erases its reference, after the loop.
    let mut erased_refs: Vec<RepresentationId> = Vec::new();
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
            // Left out (above); drawn again from the document.
            Some(Mapped::Fan(_)) if skipped.contains(h) => {}
            // Briefs 005, 009: a fan is a view, not an object; it cannot be erased.
            Some(Mapped::Fan(k)) => return Err(crate::views::refusal(cx.map, k).into()),
            Some(Mapped::RefText(rid)) => erased_refs.push(rid),
            // Erasing a device's tag text or terminal label erases that symbol (design 018 §2.4).
            Some(Mapped::Tag(rid)) => erased_refs.push(rid),
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
    // Erasing a reference's text erases the reference (design 010 §9): its point goes too, unless
    // it was erased with it, and the emptied representation is removed below.
    for rid in erased_refs {
        linked_removed = true;
        rep(&mut reps, rid, cx.doc)?;
        let items: Vec<ItemId> = reps.get(&rid).map(|r| r.items.clone()).unwrap_or_default();
        for id in items {
            let item = cx.doc.items.get(&id).cloned().ok_or("mapped item missing")?;
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
            Some(Mapped::Fan(_)) if skipped.contains(h) => {}
            Some(Mapped::Fan(k)) => return Err(crate::views::refusal(cx.map, k).into()),
            Some(Mapped::RefText(_)) => return Err(crate::references::TEXT_DERIVED.into()),
            Some(Mapped::Tag(_)) if skipped.contains(h) => {}
            Some(Mapped::Tag(_)) => return Err(crate::devices::TAG_DERIVED.into()),
            Some(Mapped::Marker(rid, term)) => {
                let at = marker_position(new).ok_or("connection point markers can only be moved")?;
                if marker_entity(at, *h) != ***new {
                    return Err("connection point markers can only be moved".into());
                }
                rep(&mut reps, rid, cx.doc)?;
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
                    return Err(format!("the {MARKER_LAYER} layer is reserved for connection point markers"));
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

    // Representation edits: an emptied representation is removed (its subject stays, DOC-06). A
    // symbol's connection points are in its definition's frame (brief 018): with no symbol left
    // they place nothing.
    let mut unlinked: Vec<(RepresentationId, Representation)> = Vec::new();
    for (id, r) in reps {
        let old = cx.doc.representations.get(&id).cloned().ok_or("representation missing")?;
        if r.items.is_empty() && (r.connection_points.is_empty() || r.form == Form::Symbol) {
            unlinked.push((id, old.clone()));
            t.ops.push(Op::UnlinkRepresentation { id, rep: old });
        } else if r != old {
            t.ops.push(Op::ModifyRepresentation { id, before: old, after: r });
        }
    }

    // Erased device symbols (design 018 §3.3): the terminals only they showed go, and a device
    // whose last symbol went is deleted.
    let (more, notes) = crate::devices::erase_followups(cx.doc, &unlinked);
    t.ops.extend(more);
    t.notes.extend(notes);

    // Left-out parts beside objects that recorded nothing: nothing else was edited after all.
    if !skipped.is_empty() && t.ops.is_empty() {
        return Err(crate::views::DRAWN_REFUSED.into());
    }

    movement(cx, &mut t, &fresh_conductor_ends)?;
    preview(cx.doc, &t.ops)?;
    t.summary =
        format!("{} added, {} modified, {} removed, {} components copied", counts[0], counts[1], counts[2], counts[3]);
    Ok(t)
}

/// One copy operation's new entities (§11.1).
fn copy_batch(
    cx: &mut OldCtx<'_>,
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
        match map.get(h) {
            Some(Mapped::Fan(k)) => return Err(crate::views::refusal(map, k).into()),
            Some(Mapped::RefText(_)) => return Err(crate::references::TEXT_DERIVED.into()),
            _ => {}
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
            // A symbol's connection points are in its definition's frame: it has no markers.
            && marker_handles.len() == if r.form == Form::Drawn { r.connection_points.len() } else { 0 }
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
        // A copied device gets the next tag of its prefix (design 018 §3.3).
        let mut comp = comp;
        if comp.properties.contains_key(ecad_model::tag::TAG_KEY) {
            match crate::devices::copy_tag(cx.doc, cx.tags, &comp) {
                Some(tag) => comp.properties.insert(ecad_model::tag::TAG_KEY.into(), tag),
                None => comp.properties.remove(ecad_model::tag::TAG_KEY),
            };
        }
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
        if r.form == Form::Symbol {
            for (src_t, at) in &r.connection_points {
                cps.insert(term_map.get(src_t).copied().ok_or("connection point not copied")?, *at);
            }
        }
        for (src_t, sh) in &marker_handles {
            let nh = by_source.get(sh).copied().ok_or("copy missing")?;
            let e = after.get(&nh).ok_or("copied marker missing")?;
            let at = marker_position(e).ok_or("copied marker is not a marker")?;
            let nt = term_map.get(src_t).copied().ok_or("connection point not copied")?;
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
            return Err("copying connection point markers without their whole component is not supported yet".into());
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
fn movement(cx: &OldCtx<'_>, t: &mut Translation, fresh_ends: &[(ConductorId, End)]) -> Result<(), String> {
    let eps = 1e-9;
    let proposed = preview(cx.doc, &t.ops)?;
    let old_pos = positions(cx.doc, cx.page);
    let new_pos = positions(&proposed, cx.page);
    let mut amend: BTreeMap<ItemId, Item> = BTreeMap::new();
    let mut attach: Vec<Op> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
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
                    } else if end_moved && old_pos.contains_key(&term) {
                        // M3: a deliberate reconnection, resolved once. Only for a terminal shown
                        // on this page: an end attached on another page (a wire continued across
                        // pages, brief 010) is the end that leaves toward the reference here,
                        // and moving it never detaches it.
                        let found = terminal_at(&proposed, cx.page, new_end, cx.tol);
                        if found != Some(term) {
                            attach.push(Op::AttachEnd { conductor: *cid, end, before: Some(term), after: found });
                        }
                    } else if end_moved && terminal_at(&proposed, cx.page, new_end, cx.tol).is_some() {
                        // The end leaving toward a reference was dropped on a terminal here: it
                        // stays connected on the other page, and the user is told so.
                        notes.push(format!(
                            "End {end:?} of {} is connected on another page, so it was not connected here; \
disconnect it there first.",
                            crate::slash::wire_name(&proposed, *cid)
                        ));
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
    t.notes.extend(notes);
    Ok(())
}
