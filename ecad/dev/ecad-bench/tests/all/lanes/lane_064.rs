//! Lane 064 (design 064 §10, test C2 on a recorded corpus): `Touched` over `Effects` equals the
//! old per-op classification for every transaction of a generated project's history (with its
//! churn), and for undoing and redoing its last transactions.
//!
//! The oracle is the pre-064 `Touched::add`, kept here as test code (ecad-model's
//! `lanes::lane_064` holds the same oracle for its op samples).
//!
//! Step 3: the Devices feed equals the old `device_list::devices_touched` (kept here as its
//! oracle) on every transaction of the same history.

use std::collections::BTreeSet;

use ecad_bench::generate::{Tier, Variant, generate, tier};
use ecad_cadcraft::device_list::DeviceTouch;
use ecad_cadcraft::symbols;
use ecad_cadcraft::title_block::{self, Reach};
use ecad_electrical::derived::{EngineInputs, Views};
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
