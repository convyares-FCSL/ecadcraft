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
//! The Properties feed (`PropsFeed`) equals the old `props::touched` (kept here) on every
//! transaction of the same history.
//!
//! Step 7: the report models' scope (`outputs::Scope::note`, over effects) equals the old per-op
//! `Scope::collect` notes (kept here) on every transaction of the same history.
//!
//! Step 7b: the PLC model's maps (`Plc::touched`, `Plc::rows_touched`), the rack sheet's places,
//! the strip pages' names, the report rows' after-state notes (`Scope::note_after`) and the
//! generated pages (`Outputs::placed_page`) over effects equal the old per-op code (kept here).
//!
//! Step 8a: the checks scope (`checks::Scope::note`, over effects, with `rating::Touch::note` and
//! the style, title block and page type rows' maps) equals the old per-op `Scope::collect` (kept
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

// ---------------------------------------------------------------- PropsFeed

/// `props::touched` as it was before design 064's `PropsFeed` (ecad-cadcraft), on the document
/// before `ops`. Its partner `Project::with_numbered` read only a transaction's extension past the
/// ops a command gave; a recorded transaction is replayed whole here, so it has none.
fn old_props_touched(doc: &Document, ops: &[Op]) -> Option<BTreeSet<ObjectRef>> {
    let mut out = BTreeSet::new();
    let subject = |s: Subject, out: &mut BTreeSet<ObjectRef>| {
        out.insert(match s {
            Subject::Device(c) => ObjectRef::Device(c),
            Subject::Wire(c) => ObjectRef::Wire(c),
            Subject::Cable(c) => ObjectRef::Cable(c),
            // A route has no properties (design 072 F31).
            Subject::Route(_) => return,
        });
    };
    for op in ops {
        match op {
            Op::ModifyPage { id, .. } => {
                out.insert(ObjectRef::Page(*id));
            }
            Op::SetProperty { object, .. } => {
                out.insert(*object);
                // A location box's own location is what its page's devices inherit (brief 024).
                if let ObjectRef::Device(c) = object
                    && ecad_electrical::locations::is_box(doc, *c)
                {
                    out.extend(doc.representations_of(Subject::Device(*c)).map(|(_, r)| ObjectRef::Page(r.page)));
                }
            }
            Op::CreateDevice { id, .. } | Op::DeleteDevice { id, .. } | Op::ModifyDevice { id, .. } | Op::SetDeviceDefinition { id, .. } => {
                out.insert(ObjectRef::Device(*id));
            }
            Op::AddTerminal { terminal, .. } | Op::RemoveTerminal { terminal, .. } => {
                out.insert(ObjectRef::Device(terminal.component));
            }
            Op::ModifyWire { id, .. } => {
                out.insert(ObjectRef::Wire(*id));
            }
            Op::CreateWire { id, wire } | Op::DeleteWire { id, wire } => {
                out.insert(ObjectRef::Wire(*id));
                // The devices at its ends: a wire's net (brief 022) is shown through them.
                out.extend(wire.ends.iter().filter_map(|t| doc.terminals.get(t)).map(|t| ObjectRef::Device(t.component)));
            }
            Op::CreateCable { id, cable } | Op::DeleteCable { id, cable } => {
                out.insert(ObjectRef::Cable(*id));
                out.extend(cable.cores.iter().filter_map(|m| m.wire()).map(ObjectRef::Wire));
            }
            // A route has no properties (design 072 F31); the devices it lands on show its net.
            Op::AddRoute { route, .. } | Op::RemoveRoute { route, .. } => {
                out.extend(route.points_landed().filter_map(|t| doc.terminals.get(&t)).map(|t| ObjectRef::Device(t.component)));
            }
            Op::ModifyRoute { before, after, .. } => {
                let landed = before.points_landed().chain(after.points_landed());
                out.extend(landed.filter_map(|t| doc.terminals.get(&t)).map(|t| ObjectRef::Device(t.component)));
            }
            Op::ModifyCable { id, before, after } => {
                out.insert(ObjectRef::Cable(*id));
                out.extend(before.cores.iter().chain(&after.cores).filter_map(|m| m.wire()).map(ObjectRef::Wire));
            }
            // A new or removed item is shown through its representation (linked or unlinked in the
            // same transaction); a moved one changes its subject's place.
            Op::AddItem { .. } => {}
            Op::ModifyItem { id, .. } | Op::RemoveItem { id, .. } => {
                if let Some((_, r)) = doc.representation_of_item(*id) {
                    subject(r.subject, &mut out);
                    if matches!(r.subject, Subject::Device(c) if ecad_electrical::locations::is_box(doc, c)) {
                        out.insert(ObjectRef::Page(r.page));
                    }
                }
            }
            Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => {
                subject(rep.subject, &mut out);
                // A location box (brief 024) changes the location its page's devices inherit.
                if matches!(rep.subject, Subject::Device(c) if ecad_electrical::locations::is_box(doc, c) || rep.role == ecad_model::Role::LocationBox) {
                    out.insert(ObjectRef::Page(rep.page));
                }
            }
            Op::ModifyRepresentation { before, after, .. } => {
                subject(before.subject, &mut out);
                subject(after.subject, &mut out);
            }
            Op::NameVersion { .. } | Op::UnnameVersion { .. } => {
                out.insert(ObjectRef::Project);
            }
            // Brief 050a: styles and the standard show in the property views of the project (the
            // inspector's style rows re-read through it).
            Op::SetStyle { .. } | Op::SetStandard { .. } => {
                out.insert(ObjectRef::Project);
            }
            // Brief 048 (design §8.4): a project symbol's new or removed version changes whether
            // the devices showing its lineage read "update available".
            Op::EmbedDefinition { definition, .. } | Op::RemoveDefinition { definition, .. } => {
                if let Some(o) = definition.origin.as_ref().filter(|o| o.library == symbols::PROJECT_SYMBOLS) {
                    for v in symbols::versions_of(doc, o.definition) {
                        for item in doc.items_using(v.id) {
                            if let Some((_, r)) = doc.representation_of_item(item) {
                                subject(r.subject, &mut out);
                            }
                        }
                    }
                }
            }
            Op::AddLayer { .. } | Op::RemoveLayer { .. } => {}
            Op::AddPage { .. }
            | Op::RemovePage { .. }
            | Op::AddFolder { .. }
            | Op::RemoveFolder { .. }
            | Op::ModifyFolder { .. }
            | Op::SetProjectSettings { .. }
            // Design 072 §5.3: a code renamed in the project's list shows on every object storing it.
            | Op::ModifyProject { .. }
            | Op::AddDeviceLink { .. }
            | Op::RemoveDeviceLink { .. } => return None,
        }
    }
    Some(out)
}

/// The Properties feed equals its old function on every transaction of `tier`'s churned history,
/// with five undos and five redos at its end. Returns how many transactions were compared.
fn props_feed_agrees(tier: Tier) -> usize {
    let mut g = generate(tier, Variant::Churned).unwrap();
    for undo in [true, true, true, true, true, false, false, false, false, false] {
        let r = if undo { g.ledger.undo_at(Author::User, None) } else { g.ledger.redo_at(Author::User, None) };
        r.unwrap();
    }
    assert_eq!(g.ledger.base(), 0, "the corpus replays from the empty document");
    let metrics = std::sync::Arc::new(ecad_electrical::text_style::fonts::FixedWidth);
    let mut views = Views::new(EngineInputs { metrics });
    let mut doc = Document::default();
    let (mut checked, mut some) = (0, 0);
    for tx in g.ledger.log() {
        let old = old_props_touched(&doc, &tx.ops);
        let pending = views.begin(&doc, &tx.ops);
        for op in &tx.ops {
            op.apply(&mut doc).unwrap();
        }
        let feed = views.finish(&doc, &tx.ops, pending).props;
        assert_eq!(feed, old, "{} transaction {} {:?}", tier.name, tx.seq, tx.label);
        some += usize::from(feed.is_some_and(|s| !s.is_empty()));
        checked += 1;
    }
    assert!(some > 0, "some transaction names objects");
    checked
}

/// `PropsFeed` on the small tier's recorded history.
#[test]
fn the_props_feed_matches_its_old_function_on_the_small_corpus() {
    let n = props_feed_agrees(tier("small").unwrap());
    assert!(n > 10, "{n} transactions");
}

/// `PropsFeed` on the medium tier's recorded history (design 064 §10).
#[test]
#[ignore = "slow: generates the medium bench project"]
fn the_props_feed_matches_its_old_function_on_the_medium_corpus() {
    let n = props_feed_agrees(tier("medium").unwrap());
    assert!(n > 10, "{n} transactions");
}

// ---------------------------------------------------------------- step 7: the report models' scope

/// `outputs::Scope::collect`'s per-op notes as they were before design 064 step 7 (its net mates
/// and renamed users, `Scope::close`, are unchanged), with the scope's fields in the same order.
#[derive(Debug, Default)]
struct OldScope {
    devices: BTreeSet<DeviceId>,
    pages: BTreeSet<PageId>,
    route_pages: BTreeSet<PageId>,
    wires: BTreeSet<ecad_model::WireId>,
    cables: BTreeSet<ecad_model::CableId>,
    project_location: bool,
    renamed: BTreeSet<String>,
    potentials: BTreeSet<String>,
    sheet: bool,
    styles: bool,
    strip_keys: BTreeSet<String>,
    connector_keys: BTreeSet<String>,
    bom_view: bool,
    all_wires: bool,
}

impl OldScope {
    fn subject(&mut self, doc: &Document, s: Subject, page: PageId) {
        match s {
            Subject::Device(c) if ecad_electrical::locations::is_box(doc, c) => {
                self.pages.insert(page);
            }
            Subject::Device(c) => {
                self.devices.insert(c);
            }
            Subject::Wire(w) => {
                self.wires.insert(w);
            }
            Subject::Cable(k) => {
                self.cables.insert(k);
            }
            // A route's drawing: the wires at the points it lands on (where their ends are drawn).
            Subject::Route(r) => {
                if let Some(route) = doc.routes.get(&r) {
                    for t in route.points_landed() {
                        self.wires.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
                    }
                }
            }
        }
    }

    fn notes(&mut self, doc: &Document, ops: &[Op]) {
        for op in ops {
            match op {
                Op::CreateDevice { id, device } | Op::DeleteDevice { id, device } | Op::ModifyDevice { id, after: device, .. } => {
                    if ecad_electrical::locations::is_box_component(device) {
                        self.pages.extend(doc.representations_of(Subject::Device(*id)).map(|(_, r)| r.page));
                    } else {
                        self.devices.insert(*id);
                    }
                }
                Op::SetDeviceDefinition { id, .. } => {
                    if ecad_electrical::locations::is_box(doc, *id) {
                        self.pages.extend(doc.representations_of(Subject::Device(*id)).map(|(_, r)| r.page));
                    } else {
                        self.devices.insert(*id);
                    }
                }
                Op::SetProperty { object: ObjectRef::Device(c), key, .. } => {
                    if ecad_electrical::locations::is_box(doc, *c) {
                        self.pages.extend(doc.representations_of(Subject::Device(*c)).map(|(_, r)| r.page));
                    } else {
                        self.devices.insert(*c);
                    }
                    if key == ecad_model::connectivity::NAME {
                        self.cables.extend(old_continuation_group_cables(doc, *c));
                    }
                }
                Op::SetProperty { object: ObjectRef::Wire(w), .. } | Op::ModifyWire { id: w, .. } => {
                    self.wires.insert(*w);
                }
                // A cable is written whole (`ModifyCable`); `SetProperty` on one is refused.
                Op::SetProperty { object: ObjectRef::Page(_) | ObjectRef::Cable(_), .. } => {}
                // No check or output reads a device link yet (design 072 §2.6).
                Op::AddDeviceLink { .. } | Op::RemoveDeviceLink { .. } => {}
                Op::ModifyProject { before, after } => {
                    use ecad_electrical::designations::{Aspect, changed_codes, default_key};
                    // Brief 047: an item without its own location shows the project's.
                    if default_key(before, Aspect::Location) != default_key(after, Aspect::Location) {
                        self.project_location = true;
                    }
                    // Brief 052: a rename in the project's list re-keys its users' rows.
                    for a in Aspect::ALL {
                        for k in changed_codes(before, after, a) {
                            // Brief 059: the cables planning an end there show its new code.
                            self.cables.extend(ecad_electrical::cables::planning(doc, &k));
                            self.renamed.insert(k);
                        }
                    }
                }
                // Brief 050a: a standard's value of a registry key reaches the same views.
                Op::SetProperty { object: ObjectRef::Project | ObjectRef::Standard, key, .. } => {
                    // Brief 050b: the project's or standard's wire category reaches every wire row.
                    if key == ecad_model::wiring::WIRE_CATEGORY {
                        self.all_wires = true;
                    }
                    // Brief 036: the registry's `terminal.*` defaults reach strips the same way.
                    if key.starts_with("strip_") || key.starts_with("terminal.") {
                        self.strip_keys.insert(key.clone());
                    }
                    // Brief 044: the registry's `connector.*` defaults reach connectors.
                    if key.starts_with("connector.") {
                        self.connector_keys.insert(key.clone());
                    }
                    // Brief 047 (design 047 §10): the BOM's report settings.
                    if key.starts_with("bom.") {
                        self.bom_view = true;
                    }
                }
                // Brief 050b (design 050 §10): a wire category reaches the rows of the conductors
                // resolving to it: its explicit users, the wires of the potentials it names (before
                // and after), and every wire when it is the project's or standard's choice.
                Op::SetStyle { catalogue, id, before, after, .. } if *catalogue == ecad_model::StyleCatalogue::Wire => {
                    for (_, o) in ecad_electrical::styles::explicit_users(doc, ecad_model::StyleCatalogue::Wire, id) {
                        if let ObjectRef::Wire(c) = o {
                            self.wires.insert(c);
                        }
                    }
                    for r in [before, after].into_iter().flatten() {
                        if let Some(pot) = r.body.wire().and_then(|w| w.potential.as_ref()) {
                            self.potentials.insert(pot.as_str().to_lowercase());
                        }
                    }
                    if let Some(r) = ecad_electrical::styles::field_of(doc, ecad_model::StyleCatalogue::Wire, id, "potential", None) {
                        self.potentials.insert(r.text().trim().to_lowercase());
                    }
                    if ecad_electrical::styles::chosen(
                        doc,
                        &ecad_electrical::defaults::styles::WIRE_CATEGORY,
                        ecad_electrical::defaults::Scope::project(),
                    ) == *id
                    {
                        self.all_wires = true;
                    }
                }
                // Brief 059 (design 059 §8): a cable type's cores, shield and gland reach its users' rows.
                Op::SetStyle { catalogue, id, .. } if *catalogue == ecad_model::StyleCatalogue::Cable => {
                    self.cables.extend(ecad_electrical::styles::cable::users(doc, id));
                }
                // Other style records and the standard header change no report row.
                Op::SetStyle { .. } | Op::SetStandard { .. } => {}
                Op::AddTerminal { id, terminal } | Op::RemoveTerminal { id, terminal } => {
                    self.devices.insert(terminal.component);
                    // Brief 028: a relabelled terminal (Remove + Add of one ID) renames its wires' ends.
                    self.wires.extend(doc.ends_at(*id).into_iter().map(|(w, _)| w));
                }
                Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => self.subject(doc, rep.subject, rep.page),
                Op::ModifyRepresentation { before, after, .. } => {
                    self.subject(doc, before.subject, before.page);
                    self.subject(doc, after.subject, after.page);
                }
                Op::AddItem { id, .. } | Op::ModifyItem { id, .. } | Op::RemoveItem { id, .. } => {
                    if let Some((_, r)) = doc.representation_of_item(*id) {
                        self.subject(doc, r.subject, r.page);
                    }
                }
                Op::CreateWire { id, .. } | Op::DeleteWire { id, .. } => {
                    self.wires.insert(*id);
                }
                // A cable is made or deleted with its cores (design 072 §4.5): their wires' rows.
                Op::CreateCable { id, cable } | Op::DeleteCable { id, cable } => {
                    self.cables.insert(*id);
                    self.wires.extend(cable.cores.iter().filter_map(|m| m.wire()));
                }
                // Design 072 §4.3: a route moved, added or removed moves where the wires at the
                // points it lands on are drawn.
                Op::AddRoute { route, .. } | Op::RemoveRoute { route, .. } => {
                    for t in route.points_landed() {
                        self.wires.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
                    }
                }
                Op::ModifyRoute { before, after, .. } => {
                    for t in before.points_landed().chain(after.points_landed()) {
                        self.wires.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
                    }
                }
                Op::ModifyCable { id, before, after } => {
                    self.cables.insert(*id);
                    self.wires.extend(before.cores.iter().chain(&after.cores).filter_map(|m| m.wire()));
                }
                Op::SetProjectSettings { before, after } => {
                    let rows = |w: &ecad_model::wiring::WiringSettings| -> std::collections::BTreeMap<String, ecad_model::wiring::PotentialDef> {
                        w.potentials.iter().map(|p| (p.name.trim().to_lowercase(), p.clone())).collect()
                    };
                    let (a, b) = (rows(&before.wiring), rows(&after.wiring));
                    for k in a.keys().chain(b.keys()) {
                        if a.get(k) != b.get(k) {
                            self.potentials.insert(k.clone());
                            if !(a.contains_key(k) && b.contains_key(k)) {
                                self.potentials.insert(String::new());
                            }
                        }
                    }
                    self.sheet |= before.sheet != after.sheet;
                    self.styles |= before.reference_style != after.reference_style || before.wiring.colour_style() != after.wiring.colour_style();
                }
                Op::AddPage { id, .. } | Op::RemovePage { id, .. } => {
                    self.pages.insert(*id);
                    self.route_pages.insert(*id);
                }
                // Design 072 §5.1: a page changed in place; its number and title are where rows
                // say they are.
                Op::ModifyPage { id, before, after } => {
                    if before.title != after.title || before.folder != after.folder {
                        self.pages.insert(*id);
                        self.route_pages.insert(*id);
                    }
                    if before.number != after.number {
                        self.route_pages.insert(*id);
                    }
                }
                Op::EmbedDefinition { .. }
                | Op::RemoveDefinition { .. }
                | Op::AddLayer { .. }
                | Op::RemoveLayer { .. }
                | Op::NameVersion { .. }
                | Op::UnnameVersion { .. }
                | Op::AddFolder { .. }
                | Op::RemoveFolder { .. }
                | Op::ModifyFolder { .. } => {}
            }
        }
    }
}

fn old_continuation_group_cables(doc: &Document, c: DeviceId) -> BTreeSet<ecad_model::CableId> {
    let Some(key) = doc.devices.get(&c).and_then(|x| ecad_model::connectivity::continuation_key(&x.properties)) else {
        return BTreeSet::new();
    };
    let mut group = doc.continuations_named(&key);
    if !group.contains(&c) {
        group.push(c);
    }
    let mut out = BTreeSet::new();
    for k in group {
        for (t, _) in doc.terminals_of(k) {
            for (w, _) in doc.ends_at(t) {
                out.extend(doc.cables_of(w));
            }
        }
    }
    out
}

/// `Outputs::moves_page_numbers` as it was before design 064 step 7 (per op).
fn old_moves_page_numbers(ops: &[Op]) -> bool {
    ops.iter().any(|op| match op {
        Op::AddPage { .. } | Op::RemovePage { .. } => true,
        // Brief 046 (review): a page's style or sheet changes its sheet count on the document
        // alone, so the count before cannot be compared.
        Op::ModifyPage { before, after, .. } => {
            before.number != after.number
                || before.sheet != after.sheet
                || before.display.get(ecad_electrical::defaults::strip_pages::STYLE.key)
                    != after.display.get(ecad_electrical::defaults::strip_pages::STYLE.key)
        }
        Op::SetProperty { object: ObjectRef::Page(_), key, .. } => key == ecad_electrical::defaults::strip_pages::STYLE.key,
        Op::SetProperty { object: ObjectRef::Project, key, .. } => key == ecad_electrical::defaults::strip_pages::STYLE.key,
        Op::ModifyProject { before, after } => ecad_electrical::revisions::issue_changed(before, after),
        Op::SetProperty { object: ObjectRef::Standard, key, .. } => key == ecad_electrical::defaults::strip_pages::STYLE.key,
        Op::SetProjectSettings { before, after } => before.sheet != after.sheet,
        _ => false,
    })
}

/// The report models' notes over effects (`Scope::note`, `Outputs::moves_page_number`) equal the
/// old per-op ones on every
/// transaction of `tier`'s churned history, on the document before and after it, with five undos
/// and five redos at its end. Returns how many transactions were compared.
fn outputs_scope_agrees(tier: Tier) -> usize {
    let mut g = generate(tier, Variant::Churned).unwrap();
    for undo in [true, true, true, true, true, false, false, false, false, false] {
        let r = if undo { g.ledger.undo_at(Author::User, None) } else { g.ledger.redo_at(Author::User, None) };
        r.unwrap();
    }
    assert_eq!(g.ledger.base(), 0, "the corpus replays from the empty document");
    let mut doc = Document::default();
    let (mut checked, mut some) = (0, 0);
    let compare = |doc: &Document, ops: &[Op], what: &str| -> bool {
        let mut old = OldScope::default();
        old.notes(doc, ops);
        let mut new = ecad_electrical::outputs::Scope::default();
        for e in &ecad_model::effect::Effects::of(ops) {
            new.note(doc, e);
        }
        assert_eq!(format!("{new:?}"), format!("{old:?}").replacen("OldScope", "Scope", 1), "{what}");
        let moves = ecad_model::effect::Effects::of(ops).iter().any(ecad_electrical::outputs::kept::Outputs::moves_page_number);
        assert_eq!(moves, old_moves_page_numbers(ops), "{what}: page numbers");
        !new.is_empty()
    };
    for tx in g.ledger.log() {
        let what = format!("{} transaction {} {:?}", tier.name, tx.seq, tx.label);
        some += usize::from(compare(&doc, &tx.ops, &format!("{what}: before")));
        for op in &tx.ops {
            op.apply(&mut doc).unwrap();
        }
        compare(&doc, &tx.ops, &format!("{what}: after"));
        checked += 1;
    }
    assert!(some > 0, "some transaction is in scope");
    checked
}

/// Step 7: the report models' notes on the small tier's recorded history.
#[test]
fn the_outputs_scope_matches_its_old_notes_on_the_small_corpus() {
    let n = outputs_scope_agrees(tier("small").unwrap());
    assert!(n > 10, "{n} transactions");
}

/// Step 7: the report models' notes on the medium tier's recorded history (design 064 §10).
#[test]
#[ignore = "slow: generates the medium bench project"]
fn the_outputs_scope_matches_its_old_notes_on_the_medium_corpus() {
    let n = outputs_scope_agrees(tier("medium").unwrap());
    assert!(n > 10, "{n} transactions");
}

// Step 7b: the rest of the report models' update over effects.

/// `Plc::touched` as it was before design 064 step 7b (per op).
fn old_plc_touched(plc: &plc::kept::Plc, doc: &Document, ops: &[Op]) -> BTreeSet<DeviceId> {
    let mut out = BTreeSet::new();
    let mut all = false;
    let mut styles = false;
    for op in ops {
        match op {
            Op::CreateDevice { id, .. } | Op::DeleteDevice { id, .. } => {
                out.insert(*id);
            }
            Op::ModifyDevice { id, .. } => {
                out.insert(*id);
                out.extend(plc.naming.get(id).into_iter().flatten().copied());
            }
            Op::SetProperty { object: ObjectRef::Device(c), key, .. } => {
                if key.starts_with("plc.") || key == ecad_model::tag::TAG_KEY {
                    if plc.cards.contains_key(c) || plc.racks.contains_key(c) || key.starts_with("plc.") {
                        out.insert(*c);
                    }
                    out.extend(plc.naming.get(c).into_iter().flatten().copied());
                }
            }
            Op::SetProperty { object: ObjectRef::Project, key, .. } if key.starts_with("plc.") => all = true,
            Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => {
                if let Subject::Device(c) = rep.subject {
                    out.insert(c);
                }
            }
            Op::AddTerminal { terminal, .. } | Op::RemoveTerminal { terminal, .. } => {
                out.insert(terminal.component);
            }
            Op::CreateWire { wire, .. } | Op::DeleteWire { wire, .. } | Op::ModifyWire { after: wire, .. } => {
                for t in &wire.ends {
                    if let Some(x) = doc.terminals.get(t) {
                        out.insert(x.component);
                    }
                }
                for w in doc.wires_of_net(wire.net) {
                    let ends = doc.wires.get(&w).map(|x| x.ends);
                    out.extend(ends.iter().flatten().filter_map(|t| doc.terminals.get(t)).map(|x| x.component).filter(|c| plc.cards.contains_key(c)));
                }
            }
            Op::ModifyItem { id, .. } => {
                if let Some(Subject::Device(c)) = doc.representation_of_item(*id).map(|(_, r)| r.subject)
                    && plc.cards.contains_key(&c)
                {
                    out.insert(c);
                }
            }
            Op::AddRoute { route, .. } | Op::RemoveRoute { route, .. } => {
                out.extend(route.points_landed().filter_map(|t| doc.terminals.get(&t)).map(|x| x.component).filter(|c| plc.cards.contains_key(c)));
            }
            Op::ModifyRoute { before, after, .. } => {
                let points = before.points_landed().chain(after.points_landed());
                out.extend(points.filter_map(|t| doc.terminals.get(&t)).map(|x| x.component).filter(|c| plc.cards.contains_key(c)));
            }
            Op::SetStyle { .. } | Op::SetStandard { .. } => styles = true,
            Op::SetProperty { object: ObjectRef::Project | ObjectRef::Standard, key, .. } if key.starts_with("text.") => styles = true,
            _ => {}
        }
    }
    if styles {
        out.extend(plc.cards.keys().copied());
    }
    if all {
        let keys: Vec<&str> = ops
            .iter()
            .filter_map(|op| match op {
                Op::SetProperty { object: ObjectRef::Project, key, .. } if key.starts_with("plc.") => Some(key.as_str()),
                _ => None,
            })
            .collect();
        let inherits = |c: &DeviceId| doc.devices.get(c).is_none_or(|x| keys.iter().any(|k| !x.properties.contains_key(*k)));
        out.extend(plc.racks.keys().filter(|c| inherits(c)).copied());
        out.extend(plc.cards.keys().filter(|c| inherits(c)).copied());
    }
    out.retain(|c| plc.cards.contains_key(c) || plc.racks.contains_key(c) || plc::card_type_of(doc, *c).is_some());
    let racks: Vec<DeviceId> = out.iter().filter(|c| plc.racks.contains_key(c)).copied().collect();
    for r in racks {
        out.extend(plc.naming.get(&r).into_iter().flatten().copied());
    }
    out
}

/// The `rows_changed` loop of `Plc::update_with` as it was before design 064 step 7b (per op).
fn old_plc_rows(plc: &plc::kept::Plc, doc: &Document, ops: &[Op]) -> BTreeSet<DeviceId> {
    use plc::iolist::{cards_on_nets_of, cards_on_nets_of_terminals};
    let mut out = BTreeSet::new();
    for op in ops {
        match op {
            Op::ModifyDevice { id: c, before, after } if before.tag != after.tag && !plc.cards.contains_key(c) => {
                out.extend(cards_on_nets_of(doc, plc, Some(*c), None));
            }
            Op::SetProperty { object: ObjectRef::Wire(w), .. } | Op::ModifyWire { id: w, .. } => {
                out.extend(cards_on_nets_of(doc, plc, None, Some(*w)));
            }
            Op::AddTerminal { id, .. } => {
                out.extend(cards_on_nets_of_terminals(doc, plc, &[*id]));
            }
            Op::SetProperty { object: ObjectRef::Device(c), key, before, after } if key == ecad_model::connectivity::NAME => {
                let mut ts: Vec<ecad_model::TerminalId> = doc.terminals_of(*c).map(|(t, _)| t).collect();
                for v in [before, after].into_iter().flatten() {
                    for k in doc.continuations_named(&ecad_model::connectivity::name_key(v)) {
                        ts.extend(doc.terminals_of(k).map(|(t, _)| t));
                    }
                }
                out.extend(cards_on_nets_of_terminals(doc, plc, &ts));
            }
            Op::SetProperty { object: ObjectRef::Device(c), key, .. } if ecad_model::links::link_number(key).is_some() => {
                out.extend(cards_on_nets_of(doc, plc, Some(*c), None));
            }
            _ => {}
        }
    }
    out
}

/// `plc::sheets::rack_places_touched` as it was before design 064 step 7b (per op).
fn old_rack_places_touched(doc: &Document, plc: &plc::kept::Plc, ops: &[Op]) -> bool {
    let ours = |c: DeviceId| plc.cards.contains_key(&c) || plc.racks.contains_key(&c);
    let page_holds = |p: PageId| doc.representations_on(p).any(|(_, r)| matches!(r.subject, Subject::Device(c) if ours(c)));
    for op in ops {
        match op {
            Op::ModifyItem { id, .. } => {
                if let Some((_, r)) = doc.representation_of_item(*id)
                    && let Subject::Device(c) = r.subject
                    && ours(c)
                {
                    return true;
                }
            }
            Op::ModifyPage { id: p, before, after } => {
                let ch = before.change(after);
                if (ch.number || ch.sheet || ch.folder) && page_holds(*p) {
                    return true;
                }
            }
            _ => {}
        }
    }
    ecad_model::book::moves_pages(ops) && !(plc.cards.is_empty() && plc.racks.is_empty())
}

/// What `StripPages::update`'s op loop named before design 064 step 7b (per op).
fn old_strip_pages_named(ops: &[Op]) -> ecad_electrical::strips::pages::Named {
    use ecad_electrical::defaults::strip_pages as entries;
    let mut n = ecad_electrical::strips::pages::Named::default();
    for op in ops {
        match op {
            Op::SetProperty { object: ObjectRef::Project, key, .. } if entries::is_layout_key(key) => n.layout = true,
            Op::SetProperty { object: ObjectRef::Project, key, .. } if key == entries::STYLE.key => n.style = true,
            Op::SetProjectSettings { before, after } if before.sheet != after.sheet => n.layout = true,
            Op::ModifyCable { id: k, .. } => {
                n.cables.insert(*k);
            }
            Op::SetProperty { object: ObjectRef::Device(d), .. } | Op::ModifyDevice { id: d, .. } => {
                n.devices.insert(*d);
            }
            _ => {}
        }
    }
    n
}

/// The two `SetProperty` loops of `Outputs::update_rows` as they were before design 064 step 7b
/// (per op, on the state after): the wires they add.
fn old_after_wires(doc: &Document, ops: &[Op]) -> BTreeSet<ecad_model::WireId> {
    let mut wires = BTreeSet::new();
    for op in ops {
        let Op::SetProperty { object: ObjectRef::Device(c), key, .. } = op else { continue };
        let position = key.strip_prefix(ecad_electrical::terminal_types::LABEL_PREFIX).and_then(|r| r.rsplit_once('.')).map(|(p, _)| p);
        if (position.is_none() && !ecad_electrical::defaults::terminals::is_label_key(key)) || ecad_electrical::strips::strip_of(doc, *c).is_none() {
            continue;
        }
        for (t, x) in doc.terminals_of(*c) {
            if position.is_none_or(|p| x.label.as_deref() == Some(p)) {
                wires.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
            }
        }
    }
    for op in ops {
        let Op::SetProperty { object: ObjectRef::Device(c), key, .. } = op else { continue };
        if ecad_electrical::defaults::connectors::is_naming_key(key) && ecad_electrical::connectors::connector_of(doc, *c).is_some_and(|x| x.mating) {
            for (t, _) in doc.terminals_of(*c) {
                wires.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
            }
        }
    }
    wires
}

/// The pages `Outputs::update_rows` read the generated set again for before design 064 step 7b.
fn old_placed_pages(ops: &[Op]) -> BTreeSet<PageId> {
    ops.iter()
        .filter_map(|op| match op {
            Op::AddPage { id, .. } | Op::RemovePage { id, .. } => Some(*id),
            _ => None,
        })
        .collect()
}

/// Step 7b: the PLC model's maps, the rack sheet's places, the strip pages' names, the report
/// rows' after-state notes and the generated pages over effects equal the old per-op ones on
/// every transaction of `tier`'s churned history (on the state after, with the PLC model kept
/// through the history as the outputs keep it), with five undos and five redos at its end.
/// Returns how many transactions were compared and how many named something.
fn outputs_update_agrees(tier: Tier) -> (usize, usize) {
    use ecad_model::effect::Effects;
    let mut g = generate(tier, Variant::Churned).unwrap();
    for undo in [true, true, true, true, true, false, false, false, false, false] {
        let r = if undo { g.ledger.undo_at(Author::User, None) } else { g.ledger.redo_at(Author::User, None) };
        r.unwrap();
    }
    assert_eq!(g.ledger.base(), 0, "the corpus replays from the empty document");
    let m = ecad_electrical::text_style::fonts::FixedWidth;
    let mut doc = Document::default();
    let mut model = plc::kept::Plc::build_with(&doc, &m);
    let (mut checked, mut some) = (0, 0);
    for tx in g.ledger.log() {
        for op in &tx.ops {
            op.apply(&mut doc).unwrap();
        }
        let what = format!("{} transaction {} {:?}", tier.name, tx.seq, tx.label);
        let ops = &tx.ops;
        let effects = Effects::of(ops);
        let touched = model.touched(&doc, &effects);
        assert_eq!(touched, old_plc_touched(&model, &doc, ops), "{what}: plc touched");
        let rows = model.rows_touched(&doc, &effects);
        assert_eq!(rows, old_plc_rows(&model, &doc, ops), "{what}: plc rows");
        let places = plc::sheets::rack_places_touched(&doc, &model, &effects);
        assert_eq!(places, old_rack_places_touched(&doc, &model, ops), "{what}: rack places");
        let named = ecad_electrical::strips::pages::Named::of(&effects);
        assert_eq!(named, old_strip_pages_named(ops), "{what}: strip pages");
        let mut new = ecad_electrical::outputs::Scope::default();
        for e in &effects {
            new.note_after(&doc, e);
        }
        let old = OldScope { wires: old_after_wires(&doc, ops), ..OldScope::default() };
        assert_eq!(format!("{new:?}"), format!("{old:?}").replacen("OldScope", "Scope", 1), "{what}: after notes");
        let placed: BTreeSet<PageId> = effects.iter().filter_map(ecad_electrical::outputs::kept::Outputs::placed_page).collect();
        assert_eq!(placed, old_placed_pages(ops), "{what}: placed pages");
        assert_eq!(ecad_model::book::positions_changed_by(&doc, &effects), ecad_model::book::positions_changed(&doc, ops), "{what}: book positions");
        let named_any = named != ecad_electrical::strips::pages::Named::default();
        some += usize::from(!touched.is_empty() || !rows.is_empty() || places || named_any || !new.is_empty() || !placed.is_empty());
        model.update_with(&doc, &effects, &m);
        checked += 1;
    }
    (checked, some)
}

/// Step 7b: the report models' update on the small tier's recorded history.
#[test]
fn the_outputs_update_matches_its_old_functions_on_the_small_corpus() {
    let (n, some) = outputs_update_agrees(tier("small").unwrap());
    assert!(n > 10, "{n} transactions");
    assert!(some > 0, "some transaction names something");
}

/// Step 7b: the report models' update on the medium tier's recorded history (design 064 §10).
#[test]
#[ignore = "slow: generates the medium bench project"]
fn the_outputs_update_matches_its_old_functions_on_the_medium_corpus() {
    let (n, some) = outputs_update_agrees(tier("medium").unwrap());
    assert!(n > 10, "{n} transactions");
    assert!(some > 0, "some transaction names something");
}

// ------------------------------------------------------------------- step 8a: the checks scope

/// `checks::Scope::collect` and `checks_pages`, `rating::Touch::collect`, `styles::checks::touches`,
/// `title_block::checks::touches` and `page_types::touches_checks` as they were before design 064
/// step 8a (one arm per op), with the scopes' fields in the same order and the same type names, so
/// their `Debug` text compares with the new ones'.
#[allow(clippy::all, dead_code)]
mod old_checks {
    use std::collections::BTreeSet;

    use ecad_electrical::rating::{RatedSubject, TEMPERATURE};
    use ecad_electrical::styles::checks::is_old_key;
    use ecad_electrical::{defaults, styles::wire};
    use ecad_model::connectivity::{NAME, name_key};
    use ecad_model::style::is_choice_key;
    use ecad_model::{CableId, DeviceId, Document, NetKey, ObjectRef, Op, PageId, RouteId, StyleRef, Subject, TerminalId, WireId};

    /// The connection points `route` lands on, and those of each route it ends on (a T, one level).
    fn route_points(doc: &Document, route: &ecad_model::Route) -> Vec<TerminalId> {
        let mut out: Vec<TerminalId> = route.points_landed().collect();
        for on in route.ends.iter().filter_map(|e| e.route()) {
            if let Some(r) = doc.routes.get(&on) {
                out.extend(r.points_landed());
            }
        }
        out
    }

    /// The subjects a transaction could change (see the module's update map).
    #[derive(Debug, Clone, Default)]
    pub struct Scope {
        all: bool,
        /// Only where rows are (page numbers, folders) changed: the revision moves, so copies of the
        /// rows are taken again.
        relocate: bool,
        nodes: BTreeSet<ecad_model::Node>,
        old_nets: BTreeSet<NetKey>,
        numbers: BTreeSet<String>,
        wires: BTreeSet<WireId>,
        routes: BTreeSet<RouteId>,
        terminals: BTreeSet<TerminalId>,
        devices: BTreeSet<DeviceId>,
        /// Cables whose reference ends to re-check (brief 027: orphans).
        cable_references: BTreeSet<CableId>,
        /// Cables to re-check (brief 026: provisional and shared tags).
        cables: BTreeSet<ecad_model::CableId>,
        /// Brief 050a: the style rows (`ecad_electrical::styles::checks::touches`).
        styles: bool,
        /// Brief 057: what the ops reach of the rating results.
        rating: Touch,
        /// Brief 052: the list's rows (an entry's code changed, added or removed).
        designations: bool,
        /// Brief 055: the title block rows (`ecad_electrical::title_block::checks::touches`).
        title_blocks: bool,
    }

    impl Scope {
        /// Brief 054: whether the set of pages with checks off may change (then everything is checked
        /// again).
        pub fn checks_pages(&mut self, doc: &Document, ops: &[Op]) {
            if ecad_model::effect::Effects::of(ops).iter().any(|e| ecad_electrical::page_types::checks_changed(doc, e)) {
                self.all = true;
            }
        }
    }

    impl Scope {
        /// Add what `ops` name, looked up in `doc` (called on the state before and after).
        pub fn collect(&mut self, doc: &Document, ops: &[Op]) {
            use ecad_model::Node;
            let conn = doc.connectivity();
            let node = |s: &mut Scope, n: Node| {
                if let Some(id) = conn.net_of(&n) {
                    s.old_nets.insert(id);
                    if let Some(net) = conn.net(id) {
                        s.numbers.extend(net.numbers.keys().cloned());
                    }
                }
                s.nodes.insert(n);
            };
            self.rating.collect(doc, ops);
            for op in ops {
                self.styles |= styles_touches(op);
                self.title_blocks |= title_block_touches(op);
                match op {
                    Op::CreateWire { id, wire } | Op::DeleteWire { id, wire } => {
                        // Brief 057: a member's ends decide its cable's open cores.
                        self.cables.extend(doc.cables_of(*id));
                        self.wires.insert(*id);
                        node(self, Node::Wire(*id));
                        for t in &wire.ends {
                            self.terminals.insert(*t);
                            node(self, Node::Terminal(*t));
                            self.component_of(doc, *t);
                        }
                    }
                    Op::AddTerminal { id, terminal } | Op::RemoveTerminal { id, terminal } => {
                        self.terminals.insert(*id);
                        self.devices.insert(terminal.component);
                        node(self, Node::Terminal(*id));
                    }
                    // Brief 048: another definition may bridge other points.
                    Op::SetDeviceDefinition { id, .. } => {
                        self.devices.insert(*id);
                        for (t, _) in doc.terminals_of(*id) {
                            node(self, Node::Terminal(t));
                        }
                    }
                    Op::CreateDevice { id, device } | Op::DeleteDevice { id, device } => {
                        self.tag_holders(doc, device.tag());
                        self.component(doc, *id, device.properties.get(NAME).map(String::as_str));
                        for (t, _) in doc.terminals_of(*id) {
                            node(self, Node::Terminal(t));
                        }
                    }
                    Op::ModifyDevice { id, before, after } => {
                        for device in [before, after] {
                            self.tag_holders(doc, device.tag());
                            self.component(doc, *id, device.properties.get(NAME).map(String::as_str));
                        }
                        for (t, _) in doc.terminals_of(*id) {
                            node(self, Node::Terminal(t));
                        }
                        // Design 072 §6.3: a device's location and function are fields; a box's are
                        // contexts for the devices on its pages, a device's are where the cables at its
                        // points are drawn (as the `SetProperty` arm below).
                        if before.location != after.location || before.function != after.function {
                            if ecad_electrical::locations::is_box(doc, *id) {
                                for p in doc.representations_of(Subject::Device(*id)).map(|(_, r)| r.page).collect::<Vec<_>>() {
                                    self.devices_on(doc, p);
                                }
                            }
                            for (t, _) in doc.terminals_of(*id) {
                                for (w, _) in doc.ends_at(t) {
                                    self.cables.extend(doc.cables_of(w));
                                }
                            }
                        }
                    }
                    Op::SetProperty { object: ObjectRef::Device(c), key, before, after } => {
                        // Brief 052 (design 052 §6): a box's value is a context for the devices on its
                        // pages.
                        if ecad_electrical::locations::is_box(doc, *c) {
                            for p in doc.representations_of(Subject::Device(*c)).map(|(_, r)| r.page).collect::<Vec<_>>() {
                                self.devices_on(doc, p);
                            }
                        }
                        if key == ecad_model::tag::TAG_KEY {
                            self.tag_holders(doc, [before, after].into_iter().flatten().map(String::as_str));
                        }
                        // Brief 059 (design 059 §8): a device's location or function is where the
                        // cables at its points are drawn (`cable-end-off-plan`).
                        if ecad_electrical::designations::Aspect::of_property(key).is_some() {
                            for (t, _) in doc.terminals_of(*c) {
                                for (w, _) in doc.ends_at(t) {
                                    self.cables.extend(doc.cables_of(w));
                                }
                            }
                        }
                        for n in [before, after].into_iter().flatten() {
                            self.component(doc, *c, Some(n));
                            node(self, Node::Name(name_key(n)));
                        }
                        self.component(doc, *c, None);
                        for (t, _) in doc.terminals_of(*c) {
                            node(self, Node::Terminal(t));
                        }
                    }
                    // A cable is written whole (`ModifyCable`); `SetProperty` on one is refused.
                    Op::SetProperty { object: ObjectRef::Cable(_), .. } => {}
                    Op::SetProperty { object: ObjectRef::Wire(c), .. } => {
                        node(self, Node::Wire(*c));
                        self.wires.insert(*c);
                    }
                    Op::ModifyWire { id: c, before, after } => {
                        node(self, Node::Wire(*c));
                        self.wires.insert(*c);
                        self.numbers.extend([before, after].into_iter().filter_map(|w| w.number_text().map(str::to_string)));
                    }
                    Op::CreateCable { id: k, .. } | Op::DeleteCable { id: k, .. } | Op::ModifyCable { id: k, .. } => {
                        // Brief 027: a new or deleted cable, or its tag (the orphan row's words).
                        self.cable_references.insert(*k);
                        // Brief 026: the cable's own rows, and every holder of the tags it had or has.
                        self.cables.insert(*k);
                        match op {
                            Op::CreateCable { cable, .. } | Op::DeleteCable { cable, .. } => {
                                self.tag_holders(doc, cable.properties.get(ecad_model::tag::TAG_KEY).map(String::as_str));
                            }
                            Op::ModifyCable { before, after, .. } => {
                                let tag = |c: &'_ ecad_model::Cable| c.properties.get(ecad_model::tag::TAG_KEY).cloned();
                                self.tag_holders(doc, [tag(before), tag(after)].iter().flatten().map(String::as_str));
                            }
                            _ => {}
                        }
                        // A cable's tag and cores are part of its members' words.
                        if let Some(cable) = doc.cables.get(k) {
                            self.wires.extend(cable.cores.iter().filter_map(|m| m.wire()));
                        }
                        match op {
                            Op::ModifyCable { before, after, .. } => {
                                self.wires.extend(before.cores.iter().chain(&after.cores).filter_map(|m| m.wire()));
                            }
                            Op::CreateCable { cable, .. } | Op::DeleteCable { cable, .. } => {
                                self.wires.extend(cable.cores.iter().filter_map(|m| m.wire()));
                            }
                            _ => {}
                        }
                    }
                    Op::AddItem { id, .. } | Op::RemoveItem { id, .. } => self.item(doc, *id),
                    Op::ModifyItem { id, .. } => self.item(doc, *id),
                    Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => self.rep(doc, rep),
                    Op::ModifyRepresentation { before, after, .. } => {
                        self.rep(doc, before);
                        self.rep(doc, after);
                    }
                    // Design 072 §4.3: the route's own rows, and the points it lands on (connected or
                    // not).
                    // The commit's rewrite pass makes and removes wires at those points (and at the
                    // points of a route it lands on), which the ops given to `scope` do not hold yet:
                    // their nets as they were lose their rows (design 072 §4.2).
                    Op::AddRoute { id, route } | Op::RemoveRoute { id, route } => {
                        self.routes.insert(*id);
                        // The cables with a core on it: their core rows (owner rule, 2026-11-18).
                        self.cables.extend(doc.cables_on_route(*id));
                        for t in route_points(doc, route) {
                            self.terminals.insert(t);
                            node(self, Node::Terminal(t));
                        }
                    }
                    Op::ModifyRoute { id, before, after } => {
                        self.routes.insert(*id);
                        self.cables.extend(doc.cables_on_route(*id));
                        for t in route_points(doc, before).into_iter().chain(route_points(doc, after)) {
                            self.terminals.insert(t);
                            node(self, Node::Terminal(t));
                        }
                    }
                    Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => {
                        for c in doc.components_using(*id) {
                            self.devices.insert(c);
                            for (t, _) in doc.terminals_of(c) {
                                self.terminals.insert(t);
                                node(self, Node::Terminal(t));
                            }
                        }
                    }
                    // Only the numbering and potential settings change what checks find; sheet, page
                    // numbering and reference style change where rows say they are; label looks
                    // (colour style, height), tag settings and units change nothing here.
                    Op::SetProjectSettings { before, after } => {
                        // Brief 033 (review of #64, 1): a table the user placed may enter or leave the
                        // title block when the project's sheet changes.
                        if before.sheet != after.sheet {
                            self.devices.extend(ecad_electrical::xref::placed_tables(doc));
                        }
                        let (b, a) = (&before.wiring, &after.wiring);
                        if b.scheme != a.scheme
                            || b.digits != a.digits
                            || b.unclassified_prefix != a.unclassified_prefix
                            || b.potentials != a.potentials
                        {
                            self.all = true;
                        } else if before.sheet != after.sheet
                            || before.page_number_step != after.page_number_step
                            || before.reference_style != after.reference_style
                        {
                            self.relocate = true;
                        }
                    }
                    // Brief 033 (review of #64, 1): a page's sheet moves its frame, so the devices on it
                    // are checked again for a table over the title block.
                    // Design 072 §5.1: a page changed in place; its sheet moves its frame (as below), its
                    // function or location is a context for the devices on it.
                    Op::ModifyPage { id: p, before, after } => {
                        let ch = before.change(after);
                        self.relocate = true;
                        if ch.sheet {
                            for (_, r) in doc.representations_on(*p) {
                                if let Subject::Device(c) = r.subject {
                                    self.devices.insert(c);
                                }
                            }
                        }
                        if ch.location || ch.function {
                            self.devices_on(doc, *p);
                        }
                    }
                    // Brief 052 (design 052 §6): a page's function or location is a context for the
                    // devices on it; the project's for every device (rare); the list's codes for the
                    // objects storing that entry, and the duplicate rows.
                    // No check or output reads a device link yet (design 072 §2.6).
                    Op::AddDeviceLink { .. } | Op::RemoveDeviceLink { .. } => {}
                    Op::ModifyProject { before, after } => {
                        use ecad_electrical::designations::{Aspect, changed_codes, default_key};
                        for a in Aspect::ALL {
                            if default_key(before, a) != default_key(after, a) {
                                self.relocate = true;
                                self.devices.extend(doc.devices.keys().copied());
                            }
                            let changed = changed_codes(before, after, a);
                            if !changed.is_empty() {
                                self.relocate = true;
                                self.designations = true;
                            }
                            for entry in &changed {
                                // Brief 059: the cables planning an end there (their rows name its code).
                                self.cables.extend(ecad_electrical::cables::planning(doc, entry));
                                for (c, x) in &doc.devices {
                                    if ecad_electrical::designations::device_keys(x).iter().any(|(_, v)| v == entry) {
                                        self.devices.insert(*c);
                                    }
                                }
                            }
                        }
                    }
                    // Page numbers, folders and the page order change where rows say they are (brief
                    // 019), not what they find: the rows are copied again.
                    Op::SetProperty { object: ObjectRef::Page(_) | ObjectRef::Project | ObjectRef::Standard, .. }
                    | Op::AddFolder { .. }
                    | Op::RemoveFolder { .. }
                    | Op::ModifyFolder { .. }
                    | Op::AddPage { .. } => self.relocate = true,
                    // Brief 027: a removed page's reference ends go with it.
                    Op::RemovePage { id, .. } => {
                        self.relocate = true;
                        for (_, r) in doc.representations_on(*id) {
                            if let Subject::Cable(k) = r.subject {
                                self.cable_references.insert(k);
                            }
                        }
                    }
                    Op::AddLayer { .. } | Op::RemoveLayer { .. } | Op::NameVersion { .. } | Op::UnnameVersion { .. } => {}
                    // Brief 059 (design 059 §8): a cable type's cores and gland reach its users' rows.
                    Op::SetStyle { catalogue, id, .. } if *catalogue == ecad_model::StyleCatalogue::Cable => {
                        self.cables.extend(ecad_electrical::styles::cable::users(doc, id));
                    }
                    // Brief 050a: style records and the standard reach only the style rows (above).
                    Op::SetStyle { .. } | Op::SetStandard { .. } => {}
                }
            }
        }

        /// Every device with a symbol on `page` (brief 052: their contexts may have changed).
        fn devices_on(&mut self, doc: &Document, page: PageId) {
            for (_, r) in doc.representations_on(page) {
                if let Subject::Device(c) = r.subject
                    && ecad_electrical::locations::is_device(doc, c)
                {
                    self.devices.insert(c);
                }
            }
        }

        /// Every device and cable holding one of `tags` (a shared tag's rows on both sides).
        fn tag_holders<'t>(&mut self, doc: &Document, tags: impl IntoIterator<Item = &'t str>) {
            for t in tags {
                self.devices.extend(doc.components_tagged(t));
                self.cables.extend(doc.cables_tagged(t));
            }
        }

        fn component_of(&mut self, doc: &Document, t: TerminalId) {
            if let Some(c) = doc.terminals.get(&t).map(|x| x.component) {
                self.devices.insert(c);
            }
        }

        /// A component and, for a continuation name, every continuation with that name.
        fn component(&mut self, doc: &Document, c: DeviceId, name: Option<&str>) {
            self.devices.insert(c);
            let name = name.map(str::to_string).or_else(|| doc.devices.get(&c).and_then(|x| x.properties.get(NAME).cloned()));
            if let Some(n) = name {
                self.devices.extend(doc.continuations_named(&name_key(&n)));
            }
            for (t, _) in doc.terminals_of(c) {
                self.terminals.insert(t);
            }
        }

        /// An item's representation (a new item's representation op names it instead).
        fn item(&mut self, doc: &Document, id: ecad_model::ItemId) {
            if let Some((_, r)) = doc.representation_of_item(id) {
                let r = r.clone();
                self.rep(doc, &r);
            }
        }

        fn rep(&mut self, doc: &Document, r: &ecad_model::Representation) {
            match r.subject {
                Subject::Wire(c) => {
                    self.wires.insert(c);
                }
                Subject::Route(x) => {
                    self.routes.insert(x);
                }
                Subject::Device(c) => {
                    // Brief 052: a box drawn, moved or erased changes the contexts on its page.
                    if ecad_electrical::locations::is_box(doc, c) {
                        self.devices_on(doc, r.page);
                    }
                    // Its points moved: the routes landing on them may now be drawn away; a
                    // continuation's place moves its partners' rows.
                    self.component(doc, c, None);
                    for (t, _) in doc.terminals_of(c) {
                        self.routes.extend(doc.routes_at(t));
                    }
                }
                // A cable's drawing moves its rows (brief 026); a reference end linked, changed or
                // unlinked (its point an item op) re-derives its orphans (brief 027).
                Subject::Cable(k) => {
                    self.cables.insert(k);
                    self.cable_references.insert(k);
                }
            }
        }
    }

    /// What an op reaches (collected on the state before and after, as `checks::Scope` does).
    #[derive(Debug, Clone, Default)]
    pub struct Touch {
        pub subjects: BTreeSet<RatedSubject>,
        /// Re-trace these too.
        pub retrace: BTreeSet<RatedSubject>,
        pub nets: BTreeSet<NetKey>,
        pub devices: BTreeSet<DeviceId>,
        pub styles: BTreeSet<(ecad_model::StyleCatalogue, StyleRef)>,
        /// Locations whose temperature changed: `at[location]` (D1).
        pub locations: BTreeSet<String>,
        /// Location boxes to read again.
        pub boxes: BTreeSet<DeviceId>,
        /// Devices whose location may have changed: their wires' results.
        pub moved: BTreeSet<DeviceId>,
        /// Pages a location box moved on: the wires of the devices drawn there.
        pub pages: BTreeSet<ecad_model::PageId>,
        /// Every result again (no re-trace).
        pub all_results: bool,
        /// The rating table's own rows (bad rows, odd pairs) again.
        pub table: bool,
    }

    impl Touch {
        /// Add what `ops` reach in `doc`.
        pub fn collect(&mut self, doc: &Document, ops: &[Op]) {
            let conn = doc.connectivity();
            let net_of_terminal = |t: ecad_model::TerminalId, s: &mut Touch| {
                if let Some(n) = conn.net_of(&ecad_model::Node::Terminal(t)) {
                    s.nets.insert(n);
                }
            };
            for op in ops {
                match op {
                    Op::SetProperty { object: ObjectRef::Wire(c), .. } => {
                        self.retrace.insert(RatedSubject::Wire(*c));
                        for k in doc.cables_of(*c) {
                            self.retrace.insert(RatedSubject::Cable(k));
                        }
                    }
                    Op::ModifyWire { id: c, before, after } => {
                        self.retrace.insert(RatedSubject::Wire(*c));
                        if (before.potential != after.potential || before.number != after.number)
                            && let Some(n) = conn.net_of_conductor(*c)
                        {
                            self.nets.insert(n);
                        }
                        for k in doc.cables_of(*c) {
                            self.retrace.insert(RatedSubject::Cable(k));
                        }
                    }
                    Op::SetProperty { object: ObjectRef::Device(c), key, .. } => {
                        let is_box = doc.devices.get(c).is_some_and(ecad_electrical::locations::is_box_component);
                        if is_box {
                            // A box's temperature or location: the wires at its locations (§10).
                            if key == TEMPERATURE {
                                self.boxes.insert(*c);
                                self.box_locations(doc, *c);
                            }
                        } else if ecad_electrical::symbols::is_rating_key(key) || key == ecad_model::tag::TAG_KEY {
                            self.devices.insert(*c);
                        } else if key.starts_with("link.") {
                            for (t, _) in doc.terminals_of(*c) {
                                net_of_terminal(t, self);
                            }
                        }
                    }
                    Op::SetProperty { object: ObjectRef::Project | ObjectRef::Standard, key, .. } => {
                        if key.starts_with("rating.") || key.starts_with("wire.") {
                            self.all_results = true;
                            self.table |= key == defaults::rating::TABLE.key;
                        }
                    }
                    // Brief 052: a location entry's code changed (or the entry came or went): the rows
                    // naming it are worded again.
                    Op::ModifyProject { before, after } => {
                        self.locations.extend(ecad_electrical::designations::changed_codes(
                            before,
                            after,
                            ecad_electrical::designations::Aspect::Location,
                        ));
                    }
                    Op::SetStyle { catalogue, id, .. } => {
                        if *catalogue == ecad_model::StyleCatalogue::RatingTable {
                            self.all_results = true;
                            self.table = true;
                        } else if *catalogue == wire::CATALOGUE.catalogue || *catalogue == ecad_model::StyleCatalogue::Cable {
                            self.styles.insert((*catalogue, id.clone()));
                            // A category naming a potential reaches every wire of it.
                            if *catalogue == wire::CATALOGUE.catalogue {
                                self.all_results = true;
                            }
                        }
                    }
                    Op::SetStandard { .. } => {
                        self.all_results = true;
                        self.table = true;
                    }
                    Op::SetProjectSettings { before, after } => {
                        if before.wiring.potentials != after.wiring.potentials {
                            self.all_results = true;
                        }
                    }
                    Op::CreateWire { id, wire } | Op::DeleteWire { id, wire } => {
                        self.retrace.insert(RatedSubject::Wire(*id));
                        if let Some(n) = conn.net_of_conductor(*id) {
                            self.nets.insert(n);
                        }
                        for t in &wire.ends {
                            net_of_terminal(*t, self);
                        }
                    }
                    Op::AddTerminal { id, .. } | Op::RemoveTerminal { id, .. } => net_of_terminal(*id, self),
                    Op::CreateDevice { id, device: component } | Op::DeleteDevice { id, device: component }
                        if ecad_electrical::locations::is_box_component(component) =>
                    {
                        self.boxes.insert(*id);
                        if let Some(l) = ecad_electrical::designations::device_key(component, ecad_electrical::designations::Aspect::Location) {
                            self.locations.insert(l);
                        }
                        self.box_pages(doc, *id);
                    }
                    // Lane 072 B: a device's description, part or type text is read by no rating row.
                    Op::ModifyDevice { before, after, .. } if before.only_text_differs(after) => {}
                    // Design 072 §2.2: a device's rating reaches only the subjects it protects.
                    Op::ModifyDevice { id, before, after } if before.only_rating_differs(after) => {
                        self.devices.insert(*id);
                    }
                    // Design 072 §6.3: a box's new location reaches the wires at both locations; a
                    // device's own location its wires only.
                    Op::ModifyDevice { id, before, after } if before.only_place_differs(after) => {
                        let loc =
                            |d: &ecad_model::Device| ecad_electrical::designations::device_key(d, ecad_electrical::designations::Aspect::Location);
                        if ecad_electrical::locations::is_box_component(after) {
                            self.boxes.insert(*id);
                            self.box_locations(doc, *id);
                            if before.location != after.location {
                                self.locations.extend([loc(before), loc(after)].into_iter().flatten());
                                self.box_pages(doc, *id);
                            }
                        } else if before.location != after.location {
                            self.moved.insert(*id);
                        }
                    }
                    Op::CreateDevice { id, .. } | Op::DeleteDevice { id, .. } | Op::ModifyDevice { id, .. } | Op::SetDeviceDefinition { id, .. } => {
                        self.devices.insert(*id);
                        for (t, _) in doc.terminals_of(*id) {
                            net_of_terminal(t, self);
                        }
                    }
                    Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => {
                        for c in doc.components_using(*id) {
                            for (t, _) in doc.terminals_of(c) {
                                net_of_terminal(t, self);
                            }
                        }
                    }
                    Op::CreateCable { id, cable } | Op::DeleteCable { id, cable } => {
                        self.retrace.insert(RatedSubject::Cable(*id));
                        for m in &cable.cores {
                            self.retrace.extend(m.wire().map(RatedSubject::Wire));
                        }
                    }
                    Op::ModifyCable { id, before, after } => {
                        self.retrace.insert(RatedSubject::Cable(*id));
                        self.retrace.extend(before.cores.iter().chain(&after.cores).filter_map(|m| m.wire()).map(RatedSubject::Wire));
                    }
                    // A device moved may enter another location box: its wires; a box moved: the wires at
                    // its location and those of the devices on its page.
                    Op::ModifyRepresentation { after, .. }
                    | Op::LinkRepresentation { rep: after, .. }
                    | Op::UnlinkRepresentation { rep: after, .. } => {
                        if let ecad_model::Subject::Device(c) = after.subject {
                            if doc.devices.get(&c).is_some_and(ecad_electrical::locations::is_box_component) {
                                self.boxes.insert(c);
                                self.box_locations(doc, c);
                                self.pages.insert(after.page);
                            } else {
                                self.moved.insert(c);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        fn box_locations(&mut self, doc: &Document, c: DeviceId) {
            if let Some(l) =
                doc.devices.get(&c).and_then(|x| ecad_electrical::designations::device_key(x, ecad_electrical::designations::Aspect::Location))
            {
                self.locations.insert(l);
            }
        }

        fn box_pages(&mut self, doc: &Document, c: DeviceId) {
            self.pages.extend(doc.representations_of(ecad_model::Subject::Device(c)).map(|(_, r)| r.page));
        }
    }
    /// Whether `op` can change the style rows.
    pub fn styles_touches(op: &Op) -> bool {
        match op {
            Op::SetStyle { .. } | Op::SetStandard { .. } => true,
            // 050b: a potentials table written again clears its older colours.
            Op::SetProjectSettings { before, after } => before.wiring.potentials != after.wiring.potentials,
            Op::SetProperty { object: ObjectRef::Project, key, .. } => is_choice_key(key) || is_old_key(key),
            Op::SetProperty { key, .. } => is_choice_key(key),
            // A deleted object takes its choices with it.
            Op::DeleteDevice { device: component, .. } => component.properties.keys().any(|k| is_choice_key(k)),
            Op::DeleteWire { wire, .. } => wire.wire_type.is_some() || wire.display.keys().any(|k| is_choice_key(k)),
            Op::ModifyWire { before, after, .. } => before.wire_type != after.wire_type || before.display != after.display,
            Op::RemovePage { page, .. } => page.page_type.is_some() || page.display.keys().any(|k| is_choice_key(k)),
            Op::ModifyPage { before, after, .. } => before.page_type != after.page_type,
            // Design 072 §4.5: a cable's type (a dangling one is a row).
            Op::CreateCable { cable, .. } | Op::DeleteCable { cable, .. } => cable.cable_type.is_some(),
            Op::ModifyCable { before, after, .. } => before.cable_type != after.cable_type,
            // Brief 054: the page type and folder rows.
            op if page_types_touches_checks(op) => true,
            _ => false,
        }
    }

    /// Whether `op` can change the title block rows.
    pub fn title_block_touches(op: &Op) -> bool {
        match op {
            Op::SetStyle { .. } | Op::SetStandard { .. } => true,
            Op::SetProjectSettings { before, after } => before.sheet != after.sheet || before.length_unit != after.length_unit,
            Op::SetProperty { object: ObjectRef::Project | ObjectRef::Standard | ObjectRef::Page(_), .. } => true,
            // Design 072 §5.3: the info, the revision rows and the lists the block shows.
            Op::ModifyProject { .. } => true,
            Op::AddPage { .. } | Op::RemovePage { .. } | Op::ModifyPage { .. } => true,
            Op::AddFolder { .. } | Op::RemoveFolder { .. } | Op::ModifyFolder { .. } => true,
            _ => false,
        }
    }

    /// Whether `op` can change [`check_rows`].
    pub fn page_types_touches_checks(op: &Op) -> bool {
        match op {
            Op::ModifyPage { before, after, .. } => {
                before.folder != after.folder || before.number != after.number || before.page_type != after.page_type
            }
            Op::AddPage { .. } | Op::RemovePage { .. } | Op::AddFolder { .. } | Op::RemoveFolder { .. } | Op::ModifyFolder { .. } => true,
            Op::SetProjectSettings { before, after } => before.page_number_step != after.page_number_step,
            Op::SetStyle { catalogue, .. } => *catalogue == ecad_model::StyleCatalogue::Page,
            _ => false,
        }
    }
}

/// The checks scope over effects (`checks::Scope::note`, with the rating touch, the style, title
/// block and page type rows' maps and the checks-off pages in it) equals the old per-op one on
/// every transaction of `tier`'s churned history, on the document before and after it, with five
/// undos and five redos at its end; each of the three row maps is compared on its own too.
/// Returns how many transactions were compared, and how many named something.
fn checks_scope_agrees(tier: Tier) -> (usize, usize) {
    use ecad_model::effect::Effects;
    let mut g = generate(tier, Variant::Churned).unwrap();
    for undo in [true, true, true, true, true, false, false, false, false, false] {
        let r = if undo { g.ledger.undo_at(Author::User, None) } else { g.ledger.redo_at(Author::User, None) };
        r.unwrap();
    }
    assert_eq!(g.ledger.base(), 0, "the corpus replays from the empty document");
    let mut doc = Document::default();
    let (mut checked, mut some) = (0, 0);
    let compare = |doc: &Document, ops: &[Op], what: &str| -> bool {
        let effects = Effects::of(ops);
        let mut old = old_checks::Scope::default();
        old.collect(doc, ops);
        old.checks_pages(doc, ops);
        let mut new = ecad_electrical::checks::Scope::default();
        for e in &effects {
            new.note(doc, e);
        }
        assert_eq!(format!("{new:?}"), format!("{old:?}"), "{what}");
        let any = |f: &dyn Fn(&ecad_model::effect::Effect<'_>) -> bool| effects.iter().any(f);
        let old_any = |f: fn(&Op) -> bool| ops.iter().any(f);
        assert_eq!(any(&ecad_electrical::styles::checks::touches), old_any(old_checks::styles_touches), "{what}: style rows");
        let tb = any(&ecad_electrical::title_block::checks::touches);
        assert_eq!(tb, old_any(old_checks::title_block_touches), "{what}: title block rows");
        let pt = any(&ecad_electrical::page_types::touches_checks);
        assert_eq!(pt, old_any(old_checks::page_types_touches_checks), "{what}: page type rows");
        format!("{new:?}") != format!("{:?}", ecad_electrical::checks::Scope::default())
    };
    for tx in g.ledger.log() {
        let what = format!("{} transaction {} {:?}", tier.name, tx.seq, tx.label);
        some += usize::from(compare(&doc, &tx.ops, &format!("{what}: before")));
        for op in &tx.ops {
            op.apply(&mut doc).unwrap();
        }
        compare(&doc, &tx.ops, &format!("{what}: after"));
        checked += 1;
    }
    (checked, some)
}

/// Step 8a: the checks scope on the small tier's recorded history.
#[test]
fn the_checks_scope_matches_its_old_function_on_the_small_corpus() {
    let (n, some) = checks_scope_agrees(tier("small").unwrap());
    assert!(n > 10, "{n} transactions");
    assert!(some > 0, "some transaction is in scope");
}

/// Step 8a: the checks scope on the medium tier's recorded history (design 064 §10).
#[test]
#[ignore = "slow: generates the medium bench project"]
fn the_checks_scope_matches_its_old_function_on_the_medium_corpus() {
    let (n, some) = checks_scope_agrees(tier("medium").unwrap());
    assert!(n > 10, "{n} transactions");
    assert!(some > 0, "some transaction is in scope");
}
