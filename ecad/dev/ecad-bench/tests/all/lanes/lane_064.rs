//! Lane 064 (design 064 §10, test C2 on a recorded corpus): `Touched` over `Effects` equals the
//! old per-op classification for every transaction of a generated project's history (with its
//! churn), and for undoing and redoing its last transactions.
//!
//! The oracle is the pre-064 `Touched::add`, kept here as test code (ecad-model's
//! `lanes::lane_064` holds the same oracle for its op samples).
//!
//! Step 3: the Devices feed equals the old `device_list::devices_touched` (kept here as its
//! oracle) on every transaction of the same history.
//!
//! Step 4: the frame's effect map (`frames::touched_by`, `generated::touched_by`) equals the old
//! op-based `frames::touches` and `generated::touches` (kept here) for every page's frame.
//!
//! Step 6: the pages' feed (`PagesView`) equals the old `pages_touched` and `pages_named` (kept
//! here) on every transaction of the same history.
//!
//! Step 5: the references view (`RefsView`, its update map over effects) against the whole
//! document on every transaction of the same history: every page whose reference texts, tables,
//! connector parts, continuation arrows or cable reference texts differ is among its pages; its
//! orphaned-reference line is the whole document's; its index equals a build.
//!
//! # Frozen pre-R9 behaviour, test oracle only
//! Every `old_*` function here is a frozen copy of code R9 replaced, kept only as a test oracle.
//! It is not live legacy code and nothing outside this module calls it. Delete it when R9's
//! step 10 matrix covers it; step 10 decides whether it stays.

use std::collections::BTreeSet;

use ecad_bench::generate::{Tier, Variant, generate, tier};
use ecad_cadcraft::device_list::DeviceTouch;
use ecad_cadcraft::symbols;
use ecad_cadcraft::title_block::{self, Reach};
use ecad_electrical::derived::{EngineInputs, Views};
use ecad_electrical::{frames, generated};
use ecad_model::{Author, DeviceId, Document, ObjectRef, Op, Subject, Touched};

/// `Touched::add` as it was before design 064 (one arm per op).
fn old_touched(ops: &[Op]) -> Touched {
    let mut t = Touched::default();
    for op in ops {
        match op {
            Op::AddPage { id, .. } | Op::RemovePage { id, .. } | Op::ModifyPage { id, .. } => {
                t.page_order = true;
                t.pages.insert(*id);
            }
            Op::AddLayer { id, .. } | Op::RemoveLayer { id, .. } => {
                t.layers.insert(*id);
            }
            Op::AddItem { id, .. } | Op::ModifyItem { id, .. } | Op::RemoveItem { id, .. } => {
                t.items.insert(*id);
            }
            Op::CreateDevice { id, .. } | Op::DeleteDevice { id, .. } | Op::ModifyDevice { id, .. } | Op::SetDeviceDefinition { id, .. } => {
                t.devices.insert(*id);
            }
            Op::AddTerminal { id, .. } | Op::RemoveTerminal { id, .. } => {
                t.terminals.insert(*id);
            }
            Op::LinkRepresentation { id, .. } | Op::ModifyRepresentation { id, .. } | Op::UnlinkRepresentation { id, .. } => {
                t.representations.insert(*id);
            }
            Op::CreateWire { id, .. } | Op::DeleteWire { id, .. } | Op::ModifyWire { id, .. } => {
                t.wires.insert(*id);
            }
            Op::AddRoute { id, .. } | Op::RemoveRoute { id, .. } | Op::ModifyRoute { id, .. } => {
                t.routes.insert(*id);
            }
            Op::CreateCable { id, .. } | Op::DeleteCable { id, .. } | Op::ModifyCable { id, .. } => {
                t.cables.insert(*id);
            }
            Op::SetProperty { object, .. } => match object {
                ObjectRef::Device(id) => {
                    t.devices.insert(*id);
                }
                ObjectRef::Wire(id) => {
                    t.wires.insert(*id);
                }
                ObjectRef::Cable(id) => {
                    t.cables.insert(*id);
                }
                ObjectRef::Page(id) => {
                    t.page_properties.insert(*id);
                }
                ObjectRef::Project | ObjectRef::Standard => {}
            },
            Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => {
                t.definitions.insert(*id);
            }
            Op::SetProjectSettings { .. } | Op::NameVersion { .. } | Op::UnnameVersion { .. } => {}
            Op::AddFolder { id, .. } | Op::RemoveFolder { id, .. } => {
                t.folder_order = true;
                t.folders.insert(*id);
            }
            Op::ModifyFolder { id, .. } => {
                t.folders.insert(*id);
            }
            Op::SetStyle { .. } | Op::SetStandard { .. } => t.styles = true,
            Op::ModifyProject { .. } | Op::AddDeviceLink { .. } | Op::RemoveDeviceLink { .. } => t.project = true,
        }
    }
    t
}

/// Every transaction of `tier`'s churned history, then five undos and five redos, agree.
fn corpus_agrees(tier: Tier) -> usize {
    let mut g = generate(tier, Variant::Churned).unwrap();
    let mut checked = 0;
    for tx in g.ledger.log() {
        assert_eq!(Touched::of(&tx.ops), old_touched(&tx.ops), "{} transaction {:?}", tier.name, tx.label);
        checked += 1;
    }
    for undo in [true, true, true, true, true, false, false, false, false, false] {
        let r = if undo { g.ledger.undo_at(Author::User, None) } else { g.ledger.redo_at(Author::User, None) };
        r.unwrap();
        let tx = g.ledger.log().last().unwrap();
        assert_eq!(Touched::of(&tx.ops), old_touched(&tx.ops), "{} {}", tier.name, if undo { "undo" } else { "redo" });
        checked += 1;
    }
    checked
}

/// C2 on the small tier's recorded history.
#[test]
fn touched_is_unchanged_on_the_small_corpus() {
    let n = corpus_agrees(tier("small").unwrap());
    assert!(n > 10, "{n} transactions");
}

/// C2 on the medium tier's recorded history (design 064 §10).
#[test]
#[ignore = "slow: generates the medium bench project"]
fn touched_is_unchanged_on_the_medium_corpus() {
    let n = corpus_agrees(tier("medium").unwrap());
    assert!(n > 10, "{n} transactions");
}

/// `device_list::devices_touched` as it was before design 064 step 3.
fn old_devices_touched(doc: &Document, ops: &[Op]) -> DeviceTouch {
    let mut t = DeviceTouch::default();
    let mut component = |s: &Subject| {
        if let Subject::Device(c) = s {
            t.devices.insert(*c);
        }
    };
    let mut pages = false;
    let mut settings = false;
    let mut extra: BTreeSet<DeviceId> = BTreeSet::new();
    let mut moved = false;
    let mut moved_pages = |t: &mut BTreeSet<DeviceId>| {
        if std::mem::replace(&mut moved, true) {
            return;
        }
        for p in ecad_model::book::positions_changed(doc, ops) {
            for (_, r) in doc.representations_on(p) {
                if let Subject::Device(c) = r.subject {
                    t.insert(c);
                }
            }
        }
    };
    for op in ops {
        match title_block::frame_reach(doc, op) {
            Reach::Pages(ps) => {
                for p in ps {
                    for (_, r) in doc.representations_on(p) {
                        component(&r.subject);
                    }
                }
            }
            Reach::Wide => pages = true,
            Reach::NotFrame | Reach::Unchanged => {}
        }
        match op {
            Op::CreateDevice { id, .. } | Op::DeleteDevice { id, .. } | Op::ModifyDevice { id, .. } | Op::SetDeviceDefinition { id, .. } => {
                component(&Subject::Device(*id))
            }
            Op::SetProperty { object: ObjectRef::Device(c), .. } => component(&Subject::Device(*c)),
            Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => component(&rep.subject),
            Op::ModifyRepresentation { before, after, .. } => {
                component(&before.subject);
                component(&after.subject);
            }
            Op::ModifyItem { id, .. } => {
                if let Some((_, r)) = doc.representation_of_item(*id) {
                    component(&r.subject);
                }
            }
            Op::EmbedDefinition { definition, .. } | Op::RemoveDefinition { definition, .. } => {
                if let Some(o) = definition.origin.as_ref().filter(|o| o.library == symbols::PROJECT_SYMBOLS) {
                    for v in symbols::versions_of(doc, o.definition) {
                        for item in doc.items_using(v.id) {
                            if let Some((_, r)) = doc.representation_of_item(item) {
                                component(&r.subject);
                            }
                        }
                    }
                }
            }
            Op::AddPage { .. } | Op::RemovePage { .. } => moved_pages(&mut extra),
            Op::ModifyPage { id: pg, before, after } => {
                let ch = before.change(after);
                if ch.number || ch.sheet {
                    for (_, r) in doc.representations_on(*pg) {
                        component(&r.subject);
                    }
                }
                if ch.folder {
                    moved_pages(&mut extra);
                }
            }
            Op::AddFolder { .. } | Op::RemoveFolder { .. } | Op::ModifyFolder { .. } => moved_pages(&mut extra),
            Op::SetProjectSettings { before, after } => {
                settings = true;
                if before.sheet != after.sheet || before.length_unit != after.length_unit || before.reference_style != after.reference_style {
                    pages = true;
                }
            }
            _ => {}
        }
    }
    t.devices.extend(extra);
    t.pages = pages;
    t.settings = settings;
    t
}

/// The Devices feed equals its old function on every transaction of `tier`'s churned history,
/// with five undos and five redos at its end. Returns how many transactions were compared.
fn devices_feed_agrees(tier: Tier) -> usize {
    let mut g = generate(tier, Variant::Churned).unwrap();
    for undo in [true, true, true, true, true, false, false, false, false, false] {
        let r = if undo { g.ledger.undo_at(Author::User, None) } else { g.ledger.redo_at(Author::User, None) };
        r.unwrap();
    }
    assert_eq!(g.ledger.base(), 0, "the corpus replays from the empty document");
    let metrics = std::sync::Arc::new(ecad_electrical::text_style::fonts::FixedWidth);
    let mut views = Views::new(EngineInputs { metrics });
    let mut doc = Document::default();
    let mut checked = 0;
    for tx in g.ledger.log() {
        let old = old_devices_touched(&doc, &tx.ops);
        let pending = views.begin(&doc, &tx.ops);
        for op in &tx.ops {
            op.apply(&mut doc).unwrap();
        }
        let feed = views.finish(&doc, &tx.ops, pending).devices;
        assert_eq!(feed, old, "{} transaction {} {:?}", tier.name, tx.seq, tx.label);
        checked += 1;
    }
    checked
}

/// Step 3: `DevicesFeed` on the small tier's recorded history.
#[test]
fn the_devices_feed_matches_its_old_function_on_the_small_corpus() {
    let n = devices_feed_agrees(tier("small").unwrap());
    assert!(n > 10, "{n} transactions");
}

/// Step 3: `DevicesFeed` on the medium tier's recorded history (design 064 §10).
#[test]
#[ignore = "slow: generates the medium bench project"]
fn the_devices_feed_matches_its_old_function_on_the_medium_corpus() {
    let n = devices_feed_agrees(tier("medium").unwrap());
    assert!(n > 10, "{n} transactions");
}

/// `frames::touches` as it was before design 064 step 4.
fn old_frame_touches(ops: &[Op], page: ecad_model::PageId, shown: &frames::FrameInput) -> bool {
    use ecad_electrical::{designations, revisions};
    let removed: BTreeSet<ecad_model::PageId> = ops.iter().filter_map(|o| if let Op::RemovePage { id, .. } = o { Some(*id) } else { None }).collect();
    let added: BTreeSet<ecad_model::PageId> = ops.iter().filter_map(|o| if let Op::AddPage { id, .. } = o { Some(*id) } else { None }).collect();
    let pair = |id: &ecad_model::PageId| removed.contains(id) && added.contains(id);
    ops.iter().any(|op| match op {
        Op::SetProjectSettings { before, after } => before.sheet != after.sheet || before.length_unit != after.length_unit,
        Op::SetProperty { object: ObjectRef::Project | ObjectRef::Standard, key, .. } if ecad_electrical::text_style::is_reserved_key(key) => true,
        Op::SetProperty { object: ObjectRef::Project | ObjectRef::Standard, key, .. } if key == ecad_electrical::defaults::styles::FRAME.key => true,
        Op::SetStyle { catalogue, .. } => {
            *catalogue == ecad_model::StyleCatalogue::Frame
                || *catalogue == ecad_model::StyleCatalogue::Text
                || *catalogue == ecad_model::StyleCatalogue::Page
        }
        Op::SetStandard { before, after } => before != after,
        Op::ModifyProject { before, after } if revisions::issue_changed(before, after) || before.info != after.info => true,
        Op::ModifyProject { before, after } => designations::Aspect::ALL.into_iter().any(|a| {
            let field = match a {
                designations::Aspect::Function => &shown.function,
                designations::Aspect::Location => &shown.location,
            };
            designations::default_key(before, a) != designations::default_key(after, a)
                || designations::changed_codes(before, after, a).iter().any(|k| {
                    let old = designations::code_in_project(before, a, k);
                    old.as_deref().is_some_and(|b| b == field || shown.project.values().any(|v| v == b))
                })
        }),
        Op::SetProperty { object: ObjectRef::Project, key, .. } => {
            shown.project.contains_key(key.as_str()) || key == revisions::COLUMNS_KEY || key == revisions::PENDING_KEY
        }
        Op::SetProperty { object: ObjectRef::Page(p), .. } => *p == page,
        Op::ModifyPage { id, before, after } => {
            let ch = before.change(after);
            *id == page || ch.folder || (ch.number && shown.next_page == Some(*id))
        }
        Op::AddPage { id, index, .. } | Op::RemovePage { id, index, .. } if pair(id) => {
            let moved = ops.iter().any(|o| match o {
                Op::AddPage { id: a, index: i, .. } | Op::RemovePage { id: a, index: i, .. } => a == id && i != index,
                _ => false,
            });
            *id == page || moved
        }
        Op::AddPage { .. } | Op::RemovePage { .. } => true,
        Op::AddFolder { .. } | Op::RemoveFolder { .. } | Op::ModifyFolder { .. } => true,
        Op::NameVersion { .. } | Op::UnnameVersion { .. } => true,
        _ => false,
    })
}

/// `generated::touches` as it was before design 064 step 4.
fn old_generated_touches(ops: &[Op], kind: generated::GenKind) -> bool {
    use generated::GenKind;
    ops.iter().any(|op| match (kind, op) {
        (
            GenKind::Contents,
            Op::AddPage { .. } | Op::RemovePage { .. } | Op::AddFolder { .. } | Op::RemoveFolder { .. } | Op::ModifyFolder { .. },
        ) => true,
        (GenKind::Contents, Op::ModifyPage { before, after, .. }) => {
            before.title != after.title || before.number != after.number || before.folder != after.folder
        }
        (GenKind::Revisions, Op::SetProperty { object: ObjectRef::Project, .. }) => true,
        (GenKind::Revisions, Op::NameVersion { .. } | Op::UnnameVersion { .. }) => true,
        (GenKind::Revisions, Op::ModifyProject { before, after }) => before.revisions != after.revisions || before.info != after.info,
        _ => false,
    })
}

/// The frame's effect map against its old op-based functions on every transaction of `tier`'s
/// churned history (with five undos and five redos), for every page's frame and both generated
/// kinds. Returns how many (transaction, page) pairs were compared.
fn frame_map_agrees(tier: Tier) -> usize {
    let mut g = generate(tier, Variant::Churned).unwrap();
    for undo in [true, true, true, true, true, false, false, false, false, false] {
        let r = if undo { g.ledger.undo_at(Author::User, None) } else { g.ledger.redo_at(Author::User, None) };
        r.unwrap();
    }
    let metrics = ecad_electrical::text_style::fonts::FixedWidth;
    let mut doc = Document::default();
    let mut checked = 0;
    for tx in g.ledger.log() {
        let what = format!("{} transaction {} {:?}", tier.name, tx.seq, tx.label);
        for kind in [generated::GenKind::Contents, generated::GenKind::Revisions] {
            assert_eq!(generated::touches(&tx.ops, kind), old_generated_touches(&tx.ops, kind), "{what}: {kind:?}");
        }
        let book = ecad_model::book::Book::of(&doc);
        let text = frames::FrameText::with(&doc, &metrics);
        for page in doc.page_order.clone() {
            let shown = frames::FrameInput::with(&doc, page, &book, text);
            assert_eq!(frames::touches(&tx.ops, page, &shown), old_frame_touches(&tx.ops, page, &shown), "{what}: page {page}");
            checked += 1;
        }
        for op in &tx.ops {
            op.apply(&mut doc).unwrap();
        }
    }
    checked
}

/// Step 4: the frame's map on the small tier's recorded history.
#[test]
fn the_frame_map_matches_its_old_functions_on_the_small_corpus() {
    let n = frame_map_agrees(tier("small").unwrap());
    assert!(n > 10, "{n} frames");
}

/// Step 4: the frame's map on the medium tier's recorded history (design 064 §10).
#[test]
#[ignore = "slow: generates the medium bench project"]
fn the_frame_map_matches_its_old_functions_on_the_medium_corpus() {
    let n = frame_map_agrees(tier("medium").unwrap());
    assert!(n > 10, "{n} frames");
}

// ---------------------------------------------------------------- step 6: the pages' feed

use ecad_electrical::{derived, plc};
use ecad_model::PageId;
use ecad_model::view::PageSet;

/// `project::unused_after`, `project::pages_touched` and `project::pages_named` as they were before
/// design 064 step 6 (the kept PLC model left out: the walk gives the same cards).
fn old_unused_after(doc: &ecad_model::Document, id: ecad_model::DefinitionId, ops: &[Op]) -> bool {
    let items = doc.items_using(id);
    let comps = doc.components_using(id);
    let item_left = |i: &ecad_model::ItemId| {
        !ops.iter().any(|o| match o {
            Op::RemoveItem { id: x, .. } => x == i,
            Op::ModifyItem { id: x, after, .. } => x == i && !matches!(&after.kind, ecad_model::ItemKind::Symbol(sym) if sym.definition == id),
            _ => false,
        })
    };
    let comp_left = |c: &ecad_model::DeviceId| {
        !ops.iter().any(|o| match o {
            Op::DeleteDevice { id: x, .. } => x == c,
            Op::SetDeviceDefinition { id: x, after, .. } => x == c && *after != Some(id),
            _ => false,
        })
    };
    !items.iter().any(item_left) && !comps.iter().any(comp_left)
}

/// The pages whose drawing `ops` could change directly (their items and representations, and the
/// pages a conductor whose ends they attach is routed on), worked out on the document before them;
/// `None` when that could be any page (op kinds other than item, representation, attachment and
/// new-object ops). Reference texts on other pages are [`ecad_electrical::references::Snapshot`]'s part.
/// Brief 045: `plc`, the kept PLC model when built, gives a rack's cards without a walk.
fn old_pages_touched(doc: &ecad_model::Document, ops: &[Op]) -> Option<BTreeSet<PageId>> {
    let mut out = BTreeSet::new();
    for op in ops {
        match op {
            Op::AddItem { item, .. } | Op::RemoveItem { item, .. } => {
                out.insert(item.page);
            }
            Op::ModifyItem { before, after, .. } => {
                out.insert(before.page);
                out.insert(after.page);
            }
            Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => {
                out.insert(rep.page);
            }
            Op::ModifyRepresentation { before, after, .. } => {
                out.insert(before.page);
                out.insert(after.page);
            }
            // A route is drawn on its page; the wire labels its net's change move with the rewrite's
            // own ops (`Project::numbered_pages`).
            Op::AddRoute { route, .. } | Op::RemoveRoute { route, .. } => {
                out.insert(route.page);
            }
            Op::ModifyRoute { before, after, .. } => {
                out.insert(before.page);
                out.insert(after.page);
            }
            // Brief 049 (design 049 §8.2): a continuation's partners list it on their pages; those
            // pages come from the reference index's watch (`xref::Watch`), and its own pages from
            // its representation ops.
            Op::CreateDevice { device: component, .. } | Op::DeleteDevice { device: component, .. }
                if ecad_model::connectivity::continuation_key(&component.properties).is_some() => {}
            Op::CreateWire { id, .. } if !doc.wires.contains_key(id) => {}
            // A deleted wire's labels are on the pages of the routes at its ends.
            Op::DeleteWire { wire, .. } => out.extend(derived::points_pages(doc, &wire.ends)),
            Op::CreateDevice { id, .. } if !doc.devices.contains_key(id) => {}
            Op::AddTerminal { id, .. } if !doc.terminals.contains_key(id) => {}
            // Brief 028: a terminal relabelled (Remove + Add of one ID) is drawn by its device's
            // symbols only: their pages.
            Op::AddTerminal { terminal, .. } | Op::RemoveTerminal { terminal, .. } => {
                out.extend(doc.representations_of(Subject::Device(terminal.component)).map(|(_, r)| r.page));
            }
            Op::EmbedDefinition { id, .. } if !doc.definitions.contains_key(id) => {}
            // A page's own properties show in its frame only (brief 019); the references they
            // change on other pages are the reference snapshot's part.
            // A folder move changes the book order (Next boxes, unnumbered pages) anywhere.
            Op::ModifyPage { before, after, .. } if before.change(after).folder => {
                return None;
            }
            Op::ModifyPage { id: p, before, after } => {
                out.insert(*p);
                // The page before it in book order shows its number in its Next box.
                if before.change(after).number {
                    let book = ecad_model::book::Book::of(doc);
                    if let Some(prev) = book.position(*p).and_then(|i| i.checked_sub(2)).and_then(|i| book.order.get(i)) {
                        out.insert(*prev);
                    }
                }
            }
            Op::SetProperty { object: ecad_model::ObjectRef::Page(p), .. } => {
                out.insert(*p);
            }
            // A device's tag is drawn beside each of its symbols (brief 018): their pages. Brief
            // 049 (design 049 §8.2): a continuation's name and kind are drawn on its own pages; its
            // partners' pages come from the reference index's watch.
            Op::SetProperty { object: ecad_model::ObjectRef::Device(c), key, .. }
                if (key == ecad_model::connectivity::NAME || key == ecad_model::connectivity::KIND)
                    && ecad_electrical::continuations::is_continuation(doc, *c) =>
            {
                out.extend(doc.representations_of(Subject::Device(*c)).map(|(_, r)| r.page));
            }
            Op::SetProperty { object: ecad_model::ObjectRef::Device(_), key, .. }
                if key == ecad_model::connectivity::NAME || key == ecad_model::connectivity::KIND =>
            {
                return None;
            }
            Op::SetProperty { object: ecad_model::ObjectRef::Device(c), key, after, .. } => {
                out.extend(doc.representations_of(Subject::Device(*c)).map(|(_, r)| r.page));
                // Brief 045 (design 045 §11.1): a rack's tag shows in its cards' captions, and a
                // card's slot or size moves the addresses of the cards after it.
                out.extend(plc::pages_touched(doc, None, *c, key, after.as_deref()));
            }
            Op::SetDeviceDefinition { id: c, .. } => {
                out.extend(doc.representations_of(Subject::Device(*c)).map(|(_, r)| r.page));
            }
            // Design 072: a device's typed fields (its tag, its type) show beside its symbols; a
            // rack's tag also in its cards' captions. A PLC station's or module's fields (§3): its
            // station's cards' pages (their captions and addresses read it).
            Op::ModifyDevice { id: c, before, after } => {
                out.extend(doc.representations_of(Subject::Device(*c)).map(|(_, r)| r.page));
                if before.tag != after.tag {
                    out.extend(plc::pages_touched(doc, None, *c, ecad_model::tag::TAG_KEY, after.tag()));
                }
                if plc::is_plc(before) || plc::is_plc(after) {
                    out.extend(plc::pages_modified(doc, None, *c, before, after));
                }
            }
            // Brief 048 (FP8): a superseded version nothing shows after the transaction is drawn
            // nowhere. Its last copies move off it in the same transaction (review of PR #86,
            // finding 1), and their own ops name their pages; the document here is the one before.
            Op::RemoveDefinition { id, .. } if old_unused_after(doc, *id, ops) => {}
            // A conductor's label shows its properties (brief 022): its pages.
            Op::SetProperty { object: ecad_model::ObjectRef::Wire(c), .. } | Op::ModifyWire { id: c, .. } => {
                out.extend(ecad_electrical::wires::wire_pages(doc, *c));
            }
            // Design 072 §4.5: a cable's fields (its tag, brief 026) are drawn by its slashes and its
            // cores' lines (its references elsewhere are the reference snapshot's part); a change
            // of its cores redraws every page, as the core ops did.
            Op::ModifyCable { id, before, after } => {
                if before.cores != after.cores {
                    return None;
                }
                out.extend(doc.representations_of(Subject::Cable(*id)).map(|(_, r)| r.page));
                for m in &after.cores {
                    out.extend(ecad_electrical::cables::member_pages(doc, m));
                }
            }
            Op::SetProperty { object: ecad_model::ObjectRef::Project, key, .. } if key.starts_with(ecad_model::tag::HIGH_RESET_PREFIX) => {}
            // Brief 052 (design 052 §8.2a): a rename in the project's list redraws the pages of
            // the devices, boxes and pages storing that entry (every page when it is the project's
            // default); a description is drawn on no page.
            Op::ModifyProject { before, after } => {
                use ecad_electrical::designations::{Aspect, changed_codes, default_key, page_key};
                // Design 072 §5.3: the info and the revision rows are drawn in every page's frame,
                // as the project properties they were.
                if before.info != after.info || before.revisions != after.revisions {
                    return None;
                }
                for a in Aspect::ALL {
                    // A new default is drawn on every page that sets none.
                    if default_key(before, a) != default_key(after, a) {
                        return None;
                    }
                    for entry in changed_codes(before, after, a) {
                        if default_key(after, a).as_deref() == Some(entry.as_str()) {
                            return None;
                        }
                        for (c, x) in &doc.devices {
                            if ecad_electrical::designations::device_keys(x).iter().any(|(_, v)| *v == entry) {
                                out.extend(doc.representations_of(Subject::Device(*c)).map(|(_, r)| r.page));
                            }
                        }
                        out.extend(doc.pages.iter().filter(|(_, x)| page_key(x, a).as_deref() == Some(entry.as_str())).map(|(p, _)| *p));
                    }
                }
            }
            // Brief 044 (design 044 §11.1): a connector naming or description default is drawn by
            // the connectors that inherit it: their pages. The maker's answers are drawn nowhere.
            // Their pages come from the connector index view after the commit (`ConnectorsView`).
            Op::SetProperty { object: ecad_model::ObjectRef::Project, key, .. } if key.starts_with("connector.") => {}
            // Brief 030: strip accessory defaults are drawn nowhere (the plan page is a generated
            // page, kept by its own rule).
            Op::SetProperty { object: ecad_model::ObjectRef::Project, key, .. }
                if ecad_electrical::accessories::strip_key_of_project(key).is_some() => {}
            // Brief 050a (design 050 §10): a style record marks the pages of its users (every page
            // when the project's choice resolves to it); the standard's header none.
            Op::SetStyle { .. } | Op::SetStandard { .. } => out.extend(ecad_electrical::styles::pages_touched(doc, op)?),
            // A standard's value acts like a project-level write of its key, and changes nothing on
            // a page while the project holds its own value.
            Op::SetProperty { object: ecad_model::ObjectRef::Standard, key, before, after } => {
                let hidden = ecad_electrical::defaults::find(key).is_some_and(|e| {
                    ecad_electrical::defaults::resolve(doc, e, ecad_electrical::defaults::Scope::project()).from
                        == ecad_electrical::defaults::Source::Project
                });
                if !hidden {
                    let as_project =
                        Op::SetProperty { object: ecad_model::ObjectRef::Project, key: key.clone(), before: before.clone(), after: after.clone() };
                    out.extend(old_pages_touched(doc, std::slice::from_ref(&as_project))?);
                }
            }
            _ => return None,
        }
    }
    // Brief 052 (design 052 §8.2): a device's location and function are stored, so a box or a
    // page value changes the tag prefixes of that page only, and a moved symbol those of its own
    // page; both pages are named by the ops above. #102's widening is gone.
    Some(out)
}

/// The pages whose drawn objects `ops` add, change or remove (items and representations): where
/// an undo or redo of them shows. Other ops (subjects, attachments, cables, properties) are drawn
/// through those objects or nowhere, so they name no page.
fn old_pages_named(ops: &[Op]) -> BTreeSet<PageId> {
    let mut out = BTreeSet::new();
    for op in ops {
        match op {
            Op::AddItem { item, .. } | Op::RemoveItem { item, .. } => {
                out.insert(item.page);
            }
            Op::ModifyItem { before, after, .. } => {
                out.insert(before.page);
                out.insert(after.page);
            }
            Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => {
                out.insert(rep.page);
            }
            Op::ModifyRepresentation { before, after, .. } => {
                out.insert(before.page);
                out.insert(after.page);
            }
            _ => {}
        }
    }
    out
}

/// The pages' feed, on its own, equals its old functions on every transaction of `tier`'s churned
/// history, with five undos and five redos at its end. Returns how many transactions were compared.
fn pages_view_agrees(tier: Tier) -> usize {
    use derived::{Inputs, PagesView, note_view};
    use ecad_model::view::{DerivedView, NoteCx, Phase};
    let mut g = generate(tier, Variant::Churned).unwrap();
    for undo in [true, true, true, true, true, false, false, false, false, false] {
        let r = if undo { g.ledger.undo_at(Author::User, None) } else { g.ledger.redo_at(Author::User, None) };
        r.unwrap();
    }
    assert_eq!(g.ledger.base(), 0, "the corpus replays from the empty document");
    let metrics = std::sync::Arc::new(ecad_electrical::text_style::fonts::FixedWidth);
    let engine = EngineInputs { metrics };
    let mut view = PagesView::default();
    let mut doc = Document::default();
    let mut checked = 0;
    for tx in g.ledger.log() {
        let old = PageSet::from_option(old_pages_touched(&doc, &tx.ops));
        let old_named = old_pages_named(&tx.ops);
        let effects = ecad_model::effect::Effects::of(&tx.ops);
        let mut scope = Default::default();
        note_view::<_, Inputs<'_>>(&view, &NoteCx { doc: &doc, phase: Phase::Before, effects: &effects }, &mut scope);
        for op in &tx.ops {
            op.apply(&mut doc).unwrap();
        }
        note_view::<_, Inputs<'_>>(&view, &NoteCx { doc: &doc, phase: Phase::After, effects: &effects }, &mut scope);
        let pages = view.update(&doc, scope, &Inputs { engine: &engine, frame_page: None }).pages;
        let what = format!("{} transaction {} {:?}", tier.name, tx.seq, tx.label);
        assert_eq!(pages, old, "{what}: pages");
        assert_eq!(view.take_named(), old_named, "{what}: named");
        checked += 1;
    }
    checked
}

/// Step 6: `PagesView` on the small tier's recorded history.
#[test]
fn the_pages_view_matches_its_old_functions_on_the_small_corpus() {
    let n = pages_view_agrees(tier("small").unwrap());
    assert!(n > 10, "{n} transactions");
}

/// Step 6: `PagesView` on the medium tier's recorded history (design 064 §10).
#[test]
#[ignore = "slow: generates the medium bench project"]
fn the_pages_view_matches_its_old_functions_on_the_medium_corpus() {
    let n = pages_view_agrees(tier("medium").unwrap());
    assert!(n > 10, "{n} transactions");
}

/// What the references show on each page of `doc`, worked out over the whole document: the device
/// and connector parts, the continuation arrows, and the cable reference texts.
fn every_reference(doc: &Document, m: &dyn ecad_electrical::text_style::fonts::TextMetrics) -> std::collections::BTreeMap<PageId, String> {
    use ecad_electrical::{continuations, xref};
    let mut out: std::collections::BTreeMap<PageId, String> = std::collections::BTreeMap::new();
    for p in &doc.page_order {
        let parts = xref::parts_on_page_measured(doc, *p, m);
        if !parts.is_empty() {
            out.insert(*p, format!("{parts:?}"));
        }
    }
    let names: BTreeSet<String> = doc.devices.keys().filter_map(|c| continuations::name_of(doc, *c)).collect();
    let mut places = xref::Places::new(doc);
    for (p, parts) in continuations::shown_measured(doc, &mut places, &names, m) {
        if !parts.is_empty() {
            out.entry(p).or_default().push_str(&format!("{parts:?}"));
        }
    }
    for r in ecad_model::derive::cable_references(doc) {
        out.entry(r.page).or_default().push_str(&format!("{}:{};", r.reference.hex(), r.text));
    }
    out
}

/// The references view on every transaction of `tier`'s churned history with five undos and five
/// redos at its end, against the whole document. Returns how many transactions were compared.
fn refs_view_agrees(tier: Tier) -> usize {
    use derived::{Inputs, RefsView};
    let mut g = generate(tier, Variant::Churned).unwrap();
    for undo in [true, true, true, true, true, false, false, false, false, false] {
        let r = if undo { g.ledger.undo_at(Author::User, None) } else { g.ledger.redo_at(Author::User, None) };
        r.unwrap();
    }
    assert_eq!(g.ledger.base(), 0, "the corpus replays from the empty document");
    let metrics = std::sync::Arc::new(ecad_electrical::text_style::fonts::FixedWidth);
    let engine = EngineInputs { metrics: metrics.clone() };
    let inputs = Inputs { engine: &engine, frame_page: None };
    let mut view = RefsView::build(&Document::default());
    let mut doc = Document::default();
    let mut checked = 0;
    for tx in g.ledger.log() {
        let before = doc.clone();
        for op in &tx.ops {
            op.apply(&mut doc).unwrap();
        }
        let out = view.commit(&before, &doc, &tx.ops, &inputs);
        let what = format!("{} transaction {} {:?}", tier.name, tx.seq, tx.label);
        let (shown_before, shown_after) = (every_reference(&before, &*metrics), every_reference(&doc, &*metrics));
        if let PageSet::Only(pages) = &out.pages {
            for p in shown_before.keys().chain(shown_after.keys()) {
                if shown_before.get(p) != shown_after.get(p) {
                    assert!(pages.contains(p), "{what}: page {} changed its references but was not named", p.hex());
                }
            }
        }
        let whole = ecad_electrical::references::Snapshot::take(&before, None);
        let orphaned = whole.orphaned_line(&doc, &ecad_electrical::references::Snapshot::take(&doc, None));
        assert_eq!(out.echo, orphaned.into_iter().collect::<Vec<_>>(), "{what}: orphaned line");
        assert_eq!(view.index(), &ecad_electrical::xref::RefIndex::build(&doc), "{what}: index");
        checked += 1;
    }
    checked
}

/// Step 5: `RefsView` on the small tier's recorded history.
#[test]
fn the_refs_view_follows_the_whole_document_on_the_small_corpus() {
    let n = refs_view_agrees(tier("small").unwrap());
    assert!(n > 10, "{n} transactions");
}

/// Step 5: `RefsView` on the medium tier's recorded history (design 064 §10).
#[test]
#[ignore = "slow: generates the medium bench project"]
fn the_refs_view_follows_the_whole_document_on_the_medium_corpus() {
    let n = refs_view_agrees(tier("medium").unwrap());
    assert!(n > 10, "{n} transactions");
}
