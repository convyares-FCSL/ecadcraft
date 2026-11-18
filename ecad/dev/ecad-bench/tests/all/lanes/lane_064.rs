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
            Op::AttachEnd { conductor, .. } => {
                t.wires.insert(*conductor);
            }
            Op::CreateCable { id, .. } | Op::DeleteCable { id, .. } => {
                t.cables.insert(*id);
            }
            Op::AddCableMember { cable, .. } | Op::RemoveCableMember { cable, .. } => {
                t.cables.insert(*cable);
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
            Op::ModifyProject { .. } => t.project = true,
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
