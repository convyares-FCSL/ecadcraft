//! Lane 064 (design 064 §10): the one classifier of ops, `Effects::of`, and `Touched` over it.

use ecad_model::effect::{Effect, EffectKind, Effects};
use ecad_model::{ObjectRef, Op, Touched};

use crate::incremental_contracts::{all_ops, applicable, base};

/// The kind of effect each op gives (design 064 §3.2).
fn expected(op: &Op) -> EffectKind {
    match op.name() {
        "AddPage" | "RemovePage" | "ModifyPage" => EffectKind::Page,
        "AddLayer" | "RemoveLayer" => EffectKind::Layer,
        "AddItem" | "ModifyItem" | "RemoveItem" => EffectKind::Item,
        "CreateDevice" | "DeleteDevice" | "ModifyDevice" => EffectKind::Device,
        "SetDeviceDefinition" => EffectKind::DeviceDefinition,
        "AddTerminal" | "RemoveTerminal" => EffectKind::Terminal,
        "LinkRepresentation" | "ModifyRepresentation" | "UnlinkRepresentation" => EffectKind::Rep,
        "CreateWire" | "DeleteWire" | "ModifyWire" => EffectKind::Wire,
        "AttachEnd" => EffectKind::End,
        "CreateCable" | "DeleteCable" => EffectKind::Cable,
        "AddCableMember" | "RemoveCableMember" => EffectKind::Member,
        "SetProperty" => EffectKind::Prop,
        "EmbedDefinition" | "RemoveDefinition" => EffectKind::Definition,
        "SetProjectSettings" => EffectKind::Settings,
        "NameVersion" | "UnnameVersion" => EffectKind::Version,
        "AddFolder" | "RemoveFolder" | "ModifyFolder" => EffectKind::Folder,
        "SetStyle" => EffectKind::Style,
        "SetStandard" => EffectKind::Standard,
        other => panic!("op {other} has no expected effect kind: add it here and to design 064 §3.2"),
    }
}

/// C1: one sample of every op gives exactly one effect, of its kind, and every kind is reached.
#[test]
fn every_op_has_an_effect() {
    let mut b = base();
    let ops = applicable(&mut b);
    let names: std::collections::BTreeSet<&str> = ops.iter().map(Op::name).collect();
    assert_eq!(names, all_ops(), "the samples cover every op");
    let mut reached = std::collections::BTreeSet::new();
    for op in &ops {
        let e = Effects::of(std::slice::from_ref(op));
        assert_eq!(e.len(), 1, "{}", op.name());
        let kind = e.iter().next().map(Effect::kind);
        assert_eq!(kind, Some(expected(op)), "{}", op.name());
        assert!(e.kinds().contains(expected(op)), "{}", op.name());
        reached.insert(expected(op));
    }
    assert_eq!(reached.len(), EffectKind::ALL.len(), "every effect kind has an op");
}

/// C1: a remove and an add of one page, connection point or folder fold into one effect with both
/// sides, where the remove stood; a pair split by an op naming the object, or of two ids, does not.
#[test]
fn pairs_fold() {
    let mut b = base();
    let other = b.ids.fresh();
    let mut renamed = b.page_v.clone();
    renamed.title = ecad_model::PageTitle::new("renamed").unwrap();
    let set = |page| Op::SetProperty { object: ObjectRef::Page(page), key: "number".into(), before: None, after: Some("9".into()) };
    let page_pair =
        vec![Op::RemovePage { id: b.page, page: b.page_v.clone(), index: 0 }, Op::AddPage { id: b.page, page: renamed.clone(), index: 1 }];
    let term_pair = vec![Op::RemoveTerminal { id: b.term, terminal: b.term_v.clone() }, Op::AddTerminal { id: b.term, terminal: b.term_v.clone() }];
    let folder_pair = vec![
        Op::RemoveFolder { id: b.folder, folder: b.folder_v.clone(), index: 0 },
        Op::AddFolder { id: b.folder, folder: b.folder_v.clone(), index: 2 },
    ];
    let split = vec![page_pair[0].clone(), set(b.page), page_pair[1].clone()];
    let two_ids = vec![page_pair[0].clone(), Op::AddPage { id: other, page: renamed.clone(), index: 1 }];
    let before_other = vec![set(b.page), page_pair[0].clone(), page_pair[1].clone()];
    let cases: &[(&str, &[Op], usize)] = &[
        ("page rename or move", &page_pair, 1),
        ("connection point relabel", &term_pair, 1),
        ("folder move", &folder_pair, 1),
        ("split by a property of the page", &split, 3),
        ("two pages", &two_ids, 2),
        ("a property before the pair", &before_other, 2),
    ];
    for (what, ops, want) in cases {
        assert_eq!(Effects::of(ops).len(), *want, "{what}");
    }
    let folded = Effects::of(&page_pair);
    match folded.iter().next() {
        Some(Effect::Page { id, before: Some(_), after: Some(af), index_before, index_after }) => {
            assert_eq!(*id, b.page);
            assert_eq!((*index_before, *index_after), (Some(0), Some(1)), "a move changes the index");
            assert_eq!(af.title.as_str(), "renamed");
        }
        other => panic!("page pair: {other:?}"),
    }
    match Effects::of(&folder_pair).iter().next() {
        Some(Effect::Folder { before: Some(_), after: Some(_), index_before: Some(0), index_after: Some(2), .. }) => {}
        other => panic!("folder pair: {other:?}"),
    }
    match Effects::of(&before_other).iter().nth(1) {
        Some(Effect::Page { before: Some(_), after: Some(_), .. }) => {}
        other => panic!("the pair stands where the remove stood: {other:?}"),
    }
}

/// `Touched::add` as it was before design 064 (one arm per op), kept here as the oracle.
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
        }
    }
    t
}

/// C2: `Touched` over effects equals the old per-op result: for every sample alone, for all of
/// them together, and for the folded pairs.
#[test]
fn touched_is_unchanged() {
    let mut b = base();
    let ops = applicable(&mut b);
    for op in &ops {
        let one = std::slice::from_ref(op);
        assert_eq!(Touched::of(one), old_touched(one), "{}", op.name());
        let mut added = Touched::default();
        added.add(op);
        assert_eq!(added, old_touched(one), "add {}", op.name());
    }
    assert_eq!(Touched::of(&ops), old_touched(&ops), "every sample together");
    let pairs = [
        vec![Op::RemovePage { id: b.page, page: b.page_v.clone(), index: 0 }, Op::AddPage { id: b.page, page: b.page_v.clone(), index: 1 }],
        vec![Op::RemoveTerminal { id: b.term, terminal: b.term_v.clone() }, Op::AddTerminal { id: b.term, terminal: b.term_v.clone() }],
        vec![
            Op::RemoveFolder { id: b.folder, folder: b.folder_v.clone(), index: 0 },
            Op::AddFolder { id: b.folder, folder: b.folder_v.clone(), index: 0 },
        ],
    ];
    for pair in &pairs {
        assert_eq!(Touched::of(pair), old_touched(pair), "{}", pair[0].name());
    }
}
