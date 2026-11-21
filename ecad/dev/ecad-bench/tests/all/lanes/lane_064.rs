//! Lane 064 (design 064 §10, test C2 on a recorded corpus): `Touched` over `Effects` equals the
//! old per-op classification for every transaction of a generated project's history (with its
//! churn), and for undoing and redoing its last transactions.
//!
//! The oracle is the pre-064 `Touched::add`, kept here as test code (ecad-model's
//! `lanes::lane_064` holds the same oracle for its op samples).

use ecad_bench::generate::{Tier, Variant, generate, tier};
use ecad_model::{Author, ObjectRef, Op, Touched};

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
            Op::CreateAid { id, .. } | Op::DeleteAid { id, .. } | Op::ModifyAid { id, .. } => {
                t.aids.insert(*id);
            }
            Op::CreateLoosePart { id, .. } | Op::DeleteLoosePart { id, .. } | Op::ModifyLoosePart { id, .. } => {
                t.loose_parts.insert(*id);
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
