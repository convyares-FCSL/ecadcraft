//! Brief 048 (symbol builder): the new op `SetComponentDefinition` (FP5) through apply, inverse,
//! diff and merge, and invariant I7 for text slots, defaults and `derived_from` (FP1, FP2, FP4).

use std::collections::BTreeMap;

use ecad_model::merge::{self, Inputs, Scope};
use ecad_model::{
    Author, Component, ComponentId, ContentHash, Definition, DefinitionId, Document, Ledger, LibraryId, ObjectRef, Op, Origin, Pt, TerminalDef,
    TextSlot,
};

fn def(name: &str) -> Definition {
    Definition {
        name: name.into(),
        terminals: vec![TerminalDef { key: "1".into(), label: Some("1".into()), at: Pt::new(0.0, 0.0) }],
        ..Default::default()
    }
}

const A: DefinitionId = DefinitionId(10);
const B: DefinitionId = DefinitionId(11);
const C: DefinitionId = DefinitionId(12);
const K: ComponentId = ComponentId(20);

/// A ledger holding definitions A, B, C and component K made from A.
fn base() -> Ledger {
    let mut l = Ledger::new(48);
    let mut ops: Vec<Op> = [(A, "a"), (B, "b"), (C, "c")].into_iter().map(|(id, n)| Op::EmbedDefinition { id, definition: def(n) }).collect();
    ops.push(Op::CreateComponent { id: K, component: Component { definition: Some(A), ..Default::default() } });
    l.commit(Author::User, "base", ops).expect("base");
    l
}

fn set(before: Option<DefinitionId>, after: Option<DefinitionId>) -> Op {
    Op::SetComponentDefinition { id: K, before, after }
}

#[test]
fn set_component_definition_applies_and_undoes() {
    let mut l = base();
    l.commit(Author::User, "edit", vec![set(Some(A), Some(B))]).expect("set");
    assert_eq!(l.document().components[&K].definition, Some(B));
    assert_eq!(l.document().components_using(B), vec![K], "the index follows");
    assert!(l.document().components_using(A).is_empty());
    l.document().check_index().expect("index");
    l.undo(Author::User).expect("undo");
    assert_eq!(l.document().components[&K].definition, Some(A));
    assert_eq!(l.document().components_using(A), vec![K]);
    l.redo(Author::User).expect("redo");
    assert_eq!(l.document().components[&K].definition, Some(B));
}

#[test]
fn set_component_definition_keeps_properties() {
    let mut l = base();
    l.commit(
        Author::User,
        "prop",
        vec![Op::SetProperty { object: ObjectRef::Component(K), key: "tag".into(), before: None, after: Some("-K01".into()) }],
    )
    .expect("tag");
    l.commit(Author::User, "edit", vec![set(Some(A), Some(B))]).expect("set");
    assert_eq!(l.document().components[&K].properties.get("tag").map(String::as_str), Some("-K01"));
}

#[test]
fn set_component_definition_inverse_is_the_swap() {
    assert_eq!(set(Some(A), Some(B)).inverse(), set(Some(B), Some(A)));
    assert_eq!(set(None, Some(B)).inverse(), set(Some(B), None));
    assert_eq!(set(Some(A), Some(B)).name(), "SetComponentDefinition");
}

#[test]
fn set_component_definition_checks_its_precondition() {
    let mut l = base();
    let e = l.commit(Author::User, "stale", vec![set(Some(B), Some(C))]).unwrap_err().to_string();
    assert!(e.contains("differs"), "{e}");
    let e = l
        .commit(Author::User, "missing", vec![Op::SetComponentDefinition { id: ComponentId(99), before: None, after: Some(A) }])
        .unwrap_err()
        .to_string();
    assert!(e.contains("does not exist"), "{e}");
    assert_eq!(l.document().components[&K].definition, Some(A), "nothing changed");
    // A definition that is not embedded is refused by the invariants, not applied.
    assert!(l.commit(Author::User, "dangling", vec![set(Some(A), Some(DefinitionId(77)))]).is_err());
}

#[test]
fn diff_records_another_definition_as_one_op() {
    let l = base();
    let from = l.document().clone();
    let mut to = from.clone();
    set(Some(A), Some(B)).apply(&mut to).expect("apply");
    let ops = ecad_model::diff::ops_between(&from, &to, &|_| true);
    assert_eq!(ops, vec![set(Some(A), Some(B))], "no delete and create");
}

fn merged(ancestor: &Document, target: &Document, source: &Document, source_ops: &[Op]) -> merge::Plan {
    let mut scope = Scope::default();
    scope.add_ops(source_ops);
    let high = BTreeMap::new();
    let inp = Inputs {
        ancestor,
        target,
        source,
        scope: &scope,
        target_name: "main",
        source_name: "branch",
        keep_project_key: &|_| true,
        high: &high,
        edit_rule: &|_, _, v| Ok(v.to_string()),
        join: &|_, _, _| Err("no".into()),
    };
    merge::plan(&inp, &BTreeMap::new())
}

#[test]
fn merge_a_property_against_a_definition_change_does_not_conflict() {
    let a = base().document().clone();
    let tag = Op::SetProperty { object: ObjectRef::Component(K), key: "description".into(), before: None, after: Some("Pump".into()) };
    let mut t = a.clone();
    tag.apply(&mut t).expect("target");
    let mut s = a.clone();
    let s_ops = vec![set(Some(A), Some(B))];
    s_ops[0].apply(&mut s).expect("source");
    let plan = merged(&a, &t, &s, &s_ops);
    assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
    assert!(plan.ops.contains(&set(Some(A), Some(B))), "{:?}", plan.ops);
    assert!(!plan.ops.iter().any(|o| matches!(o, Op::DeleteComponent { .. })));
    let mut m = t.clone();
    for op in &plan.ops {
        op.apply(&mut m).expect("merged op");
    }
    assert_eq!(m.components[&K].definition, Some(B));
    assert_eq!(m.components[&K].properties.get("description").map(String::as_str), Some("Pump"));

    // The other way round: the source sets the property, the target the definition.
    let plan = merged(&a, &s, &t, std::slice::from_ref(&tag));
    assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
}

#[test]
fn merge_two_definition_changes_conflict() {
    let a = base().document().clone();
    let mut t = a.clone();
    set(Some(A), Some(B)).apply(&mut t).expect("target");
    let mut s = a.clone();
    let s_ops = vec![set(Some(A), Some(C))];
    s_ops[0].apply(&mut s).expect("source");
    let plan = merged(&a, &t, &s, &s_ops);
    assert_eq!(plan.conflicts.len(), 1, "{:?}", plan.conflicts);
    assert!(plan.conflicts[0].id.ends_with("/definition"), "{}", plan.conflicts[0].id);
    // The same change on both sides is not a conflict.
    let plan = merged(&a, &t, &t, &[set(Some(A), Some(B))]);
    assert!(plan.conflicts.is_empty());
}

fn origin(seq: u64) -> Origin {
    Origin { library: LibraryId(5), definition: DefinitionId(6), library_seq: seq, content_hash: ContentHash([0; 32]) }
}

fn embed(d: Definition) -> Result<(), String> {
    let mut l = Ledger::new(48);
    l.commit(Author::User, "embed", vec![Op::EmbedDefinition { id: DefinitionId(7), definition: d }]).map(|_| ()).map_err(|e| e.to_string())
}

fn slot(text: &str, x: f64) -> TextSlot {
    TextSlot { text: text.into(), at: Pt::new(x, 0.0), h: Default::default(), v: Default::default(), rotation: 0.0 }
}

#[test]
fn invariant_i7_covers_slots_defaults_and_derived_from() {
    type Case = (&'static str, fn(&mut Definition), Option<&'static str>);
    let cases: [Case; 11] = [
        ("plain", |_| {}, None),
        ("tag and rating slots", |d| d.texts = vec![slot("tag", 0.0), slot("rating", 2.0), slot("rating.A1", 4.0)], None),
        ("a NaN slot", |d| d.texts = vec![slot("tag", f64::NAN)], Some("non-finite")),
        ("an infinite rotation", |d| d.texts = vec![TextSlot { rotation: f64::INFINITY, ..slot("tag", 0.0) }], Some("non-finite")),
        ("two tag slots", |d| d.texts = vec![slot("tag", 0.0), slot("tag", 1.0)], Some("two slots")),
        ("an unknown text", |d| d.texts = vec![slot("colour", 0.0)], Some("not a device text")),
        ("an empty rating label", |d| d.texts = vec![slot("rating.", 0.0)], Some("not a device text")),
        (
            "known defaults",
            |d| d.defaults = [("key", "f1"), ("prefix", "B"), ("join", "device")].map(|(k, v)| (k.to_string(), v.to_string())).into(),
            None,
        ),
        ("an unknown default", |d| d.defaults = [("colour".to_string(), "red".to_string())].into(), Some("does not hold")),
        (
            "derived from another origin",
            |d| {
                d.origin = Some(origin(2));
                d.derived_from = Some(origin(1));
            },
            None,
        ),
        (
            "derived from itself",
            |d| {
                d.origin = Some(origin(1));
                d.derived_from = Some(origin(1));
            },
            Some("derived from itself"),
        ),
    ];
    for (name, change, refused) in cases {
        let mut d = def("x");
        change(&mut d);
        match (embed(d), refused) {
            (Ok(()), None) => {}
            (Err(e), Some(w)) => assert!(e.contains(w), "{name}: {e}"),
            (r, w) => panic!("{name}: {r:?}, expected refusal {w:?}"),
        }
    }
}
