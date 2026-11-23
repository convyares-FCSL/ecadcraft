//! The one classifier of ops, `Effects::of`.

use ecad_model::document::effect::{Effect, EffectKind, Effects};
use ecad_model::{ObjectRef, Op};

use crate::document::incremental::{all_ops, applicable, base};

/// The kind of effect each op gives (design 064 §3.2).
fn expected(op: &Op) -> EffectKind {
    match op.name() {
        "AddPage" | "RemovePage" | "ModifyPage" => EffectKind::Page,
        "AddLayer" | "RemoveLayer" => EffectKind::Layer,
        "AddItem" | "ModifyItem" | "RemoveItem" => EffectKind::Item,
        "CreateDevice" | "DeleteDevice" | "ModifyDevice" => EffectKind::Device,
        "SetDeviceDefinition" => EffectKind::DeviceDefinition,
        "AddConnectionPoint" | "RemoveConnectionPoint" => EffectKind::ConnectionPoint,
        "LinkRepresentation" | "ModifyRepresentation" | "UnlinkRepresentation" => EffectKind::Rep,
        "CreateWire" | "DeleteWire" | "ModifyWire" => EffectKind::Wire,
        "AddRoute" | "RemoveRoute" | "ModifyRoute" => EffectKind::Route,
        "CreateAid" | "DeleteAid" | "ModifyAid" => EffectKind::Aid,
        "CreateLoosePart" | "DeleteLoosePart" | "ModifyLoosePart" => EffectKind::LoosePart,
        "CreateCable" | "DeleteCable" | "ModifyCable" => EffectKind::Cable,
        "SetProperty" => EffectKind::Prop,
        "EmbedDefinition" | "RemoveDefinition" => EffectKind::Definition,
        "SetProjectSettings" => EffectKind::Settings,
        "NameVersion" | "UnnameVersion" => EffectKind::Version,
        "AddFolder" | "RemoveFolder" | "ModifyFolder" => EffectKind::Folder,
        "SetStyle" => EffectKind::Style,
        "SetStandard" => EffectKind::Standard,
        "ModifyProject" => EffectKind::Project,
        "AddDeviceLink" | "RemoveDeviceLink" => EffectKind::DeviceLink,
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
    let set = |page| Op::SetProperty { object: ObjectRef::Page(page), key: "number".parse().unwrap(), before: None, after: Some("9".into()) };
    let page_pair =
        vec![Op::RemovePage { id: b.page, page: b.page_v.clone(), index: 0 }, Op::AddPage { id: b.page, page: renamed.clone(), index: 1 }];
    let point_pair = vec![
        Op::RemoveConnectionPoint { id: b.term, connection_point: b.point_v.clone() },
        Op::AddConnectionPoint { id: b.term, connection_point: b.point_v.clone() },
    ];
    let folder_pair = vec![
        Op::RemoveFolder { id: b.folder, folder: b.folder_v.clone(), index: 0 },
        Op::AddFolder { id: b.folder, folder: b.folder_v.clone(), index: 2 },
    ];
    let split = vec![page_pair[0].clone(), set(b.page), page_pair[1].clone()];
    let two_ids = vec![page_pair[0].clone(), Op::AddPage { id: other, page: renamed.clone(), index: 1 }];
    let before_other = vec![set(b.page), page_pair[0].clone(), page_pair[1].clone()];
    let cases: &[(&str, &[Op], usize)] = &[
        ("page rename or move", &page_pair, 1),
        ("connection point relabel", &point_pair, 1),
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
