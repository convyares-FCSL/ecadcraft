//! Brief 050a (FP1–FP3, overseer ruling: full arms with round-trip and merge tests): the standard,
//! style records and their ops through apply, inverse, undo and redo, replay, diff, merge, the
//! invariants and the style-choice index.

use std::collections::BTreeMap;

use ecad_model::diff::ops_between;
use ecad_model::merge::{self, Choice, Inputs, Scope};
use ecad_model::style::{StandardHeader, StyleLayer, StyleRecord, StyleRef};
use ecad_model::tag::HighWater;
use ecad_model::{Author, Component, ComponentId, Document, Ledger, ObjectRef, Op, StandardId};

fn header(name: &str) -> StandardHeader {
    StandardHeader { id: StandardId(0x50), name: name.into(), based_on: "iec".into(), origin: None }
}

fn rec(name: &str, fields: &[(&str, &str)]) -> StyleRecord {
    StyleRecord { name: name.into(), fields: fields.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() }
}

fn tag() -> StyleRef {
    StyleRef::Builtin("tag".into())
}

fn set_style(layer: StyleLayer, id: StyleRef, before: Option<StyleRecord>, after: Option<StyleRecord>) -> Op {
    Op::SetStyle { layer, catalogue: "text".into(), id, before, after }
}

/// Every new op applied and inverted gives back the document exactly.
#[test]
fn each_op_inverts_exactly() {
    let mut l = Ledger::new(50);
    let empty = l.document().clone();
    let ops = vec![
        Op::SetStandard { before: None, after: Some(header("House")) },
        Op::SetProperty { object: ObjectRef::Standard, key: "tags.coil.prefix".into(), before: None, after: Some("K".into()) },
        set_style(StyleLayer::Standard, tag(), None, Some(rec("", &[("height", "3")]))),
        set_style(StyleLayer::Project, StyleRef::Own(7), None, Some(rec("Tag large", &[("height", "5")]))),
    ];
    l.commit(Author::User, "make", ops).unwrap();
    let doc = l.document().clone();
    assert_eq!(doc.standard.as_ref().map(|s| s.header.name.as_str()), Some("House"));
    assert_eq!(doc.standard.as_ref().and_then(|s| s.values.get("tags.coil.prefix")).map(String::as_str), Some("K"));
    assert!(doc.styles.get("text").is_some_and(|m| m.contains_key(&StyleRef::Own(7))));
    l.undo(Author::User).unwrap();
    assert_eq!(*l.document(), empty, "undo restores the empty document");
    l.redo(Author::User).unwrap();
    assert_eq!(*l.document(), doc, "redo restores it");
    // Replay from the log gives the same document.
    let replayed = Ledger::replay(l.full_log().unwrap(), 50).unwrap();
    assert_eq!(*replayed.document(), doc);
    assert!(replayed.document().check().is_ok());
}

#[test]
fn a_standard_layer_write_needs_a_standard() {
    let mut l = Ledger::new(50);
    let e = l.commit(Author::User, "x", vec![set_style(StyleLayer::Standard, tag(), None, Some(rec("", &[("height", "3")])))]).unwrap_err();
    assert!(e.to_string().contains("no standard"), "{e}");
    let e = l
        .commit(Author::User, "x", vec![Op::SetProperty { object: ObjectRef::Standard, key: "a.b".into(), before: None, after: Some("1".into()) }])
        .unwrap_err();
    assert!(e.to_string().contains("Standard"), "{e}");
    assert_eq!(*l.document(), Document::default(), "nothing changed");
}

#[test]
fn removing_a_standard_needs_it_empty_first() {
    let mut l = Ledger::new(50);
    l.commit(Author::User, "make", vec![Op::SetStandard { before: None, after: Some(header("House")) }]).unwrap();
    l.commit(Author::User, "value", vec![Op::SetProperty { object: ObjectRef::Standard, key: "a.b".into(), before: None, after: Some("1".into()) }])
        .unwrap();
    assert!(l.commit(Author::User, "remove", vec![Op::SetStandard { before: Some(header("House")), after: None }]).is_err());
    // Cleared in the same transaction first: undoable as one step.
    let ops = vec![
        Op::SetProperty { object: ObjectRef::Standard, key: "a.b".into(), before: Some("1".into()), after: None },
        Op::SetStandard { before: Some(header("House")), after: None },
    ];
    l.commit(Author::User, "back to IEC", ops).unwrap();
    assert!(l.document().standard.is_none());
    l.undo(Author::User).unwrap();
    assert_eq!(l.document().standard.as_ref().map(|s| s.values.len()), Some(1));
}

#[test]
fn a_stale_before_is_refused() {
    let mut l = Ledger::new(50);
    l.commit(Author::User, "a", vec![set_style(StyleLayer::Project, tag(), None, Some(rec("", &[("height", "3")])))]).unwrap();
    let e = l.commit(Author::User, "b", vec![set_style(StyleLayer::Project, tag(), None, Some(rec("", &[("height", "4")])))]).unwrap_err();
    assert!(e.to_string().contains("differs"), "{e}");
}

#[test]
fn a_bad_catalogue_word_breaks_the_invariant() {
    let mut l = Ledger::new(50);
    let op = Op::SetStyle { layer: StyleLayer::Project, catalogue: "Bad Word".into(), id: tag(), before: None, after: Some(rec("", &[])) };
    assert!(l.commit(Author::User, "x", vec![op]).is_err());
}

#[test]
fn style_choices_are_indexed_and_kept() {
    let mut l = Ledger::new(50);
    let c = ComponentId(9);
    l.commit(Author::User, "c", vec![Op::CreateComponent { id: c, component: Component::default() }]).unwrap();
    let own = StyleRef::Own(7).text();
    let set = |before: Option<&str>, after: Option<&str>| Op::SetProperty {
        object: ObjectRef::Component(c),
        key: "text.tag.style".into(),
        before: before.map(str::to_string),
        after: after.map(str::to_string),
    };
    l.commit(Author::User, "choose", vec![set(None, Some(&own))]).unwrap();
    assert_eq!(l.document().style_users("text.tag.style", &own).collect::<Vec<_>>(), vec![ObjectRef::Component(c)]);
    assert!(l.document().check_index().is_ok());
    l.commit(Author::User, "other", vec![set(Some(&own), Some("tag"))]).unwrap();
    assert_eq!(l.document().style_users("text.tag.style", &own).count(), 0);
    assert_eq!(l.document().style_users("text.tag.style", "tag").count(), 1);
    l.undo(Author::User).unwrap();
    assert_eq!(l.document().style_users("text.tag.style", &own).count(), 1);
    assert!(l.document().check_index().is_ok());
    // A deleted device takes its choice with it.
    let comp = l.document().components.get(&c).cloned().unwrap();
    l.commit(Author::User, "delete", vec![Op::DeleteComponent { id: c, component: comp }]).unwrap();
    assert_eq!(l.document().style_users_of(&own).count(), 0);
    assert!(l.document().check_index().is_ok());
}

/// The diff from one state to another gives ops that reproduce it, in an order each holds
/// (standard first when it appears, its contents first when it goes).
#[test]
fn the_diff_round_trips_the_standard_and_styles() {
    let mut a = Ledger::new(50);
    a.commit(Author::User, "p", vec![set_style(StyleLayer::Project, tag(), None, Some(rec("", &[("colour", "RD")])))]).unwrap();
    let from = a.document().clone();
    let mut b = a.clone();
    b.commit(
        Author::User,
        "std",
        vec![
            Op::SetStandard { before: None, after: Some(header("House")) },
            Op::SetProperty { object: ObjectRef::Standard, key: "a.b".into(), before: None, after: Some("1".into()) },
            set_style(StyleLayer::Standard, StyleRef::Own(3), None, Some(rec("Big", &[("height", "6")]))),
            set_style(StyleLayer::Project, tag(), Some(rec("", &[("colour", "RD")])), None),
        ],
    )
    .unwrap();
    let to = b.document().clone();
    for (x, y) in [(&from, &to), (&to, &from)] {
        let ops = ops_between(x, y, &|_| true);
        let mut l = Ledger::new(51);
        let mut d = x.clone();
        for op in &ops {
            op.apply(&mut d).unwrap();
        }
        assert_eq!(d, *y);
        let _ = &mut l;
    }
}

fn run_merge(a: &Document, t: &Document, s: &Document, scope: &Scope, choices: &BTreeMap<String, Choice>) -> merge::Plan {
    let high = HighWater::default();
    let inp = Inputs {
        ancestor: a,
        target: t,
        source: s,
        scope,
        target_name: "main",
        source_name: "branch",
        keep_project_key: &|_| true,
        high: &high,
        edit_rule: &|_, _, v| Ok(v.to_string()),
        join: &|_, _, _| Err("no".into()),
    };
    merge::plan(&inp, choices)
}

/// A style record is merged whole: both branches changing it differently is one conflict, worded
/// with the style's name; one side's change merges; the standard's values merge per key.
#[test]
fn style_records_merge_as_units() {
    let mut base = Ledger::new(50);
    let big = StyleRef::Own(3);
    base.commit(
        Author::User,
        "base",
        vec![
            Op::SetStandard { before: None, after: Some(header("House")) },
            set_style(StyleLayer::Project, big.clone(), None, Some(rec("Tag large", &[("height", "5")]))),
        ],
    )
    .unwrap();
    let a = base.document().clone();
    let (mut t, mut s) = (base.clone(), base.clone());
    let t_ops = vec![
        set_style(StyleLayer::Project, big.clone(), Some(rec("Tag large", &[("height", "5")])), Some(rec("Tag large", &[("height", "6")]))),
        Op::SetProperty { object: ObjectRef::Standard, key: "x.one".into(), before: None, after: Some("1".into()) },
    ];
    let s_ops = vec![
        set_style(StyleLayer::Project, big.clone(), Some(rec("Tag large", &[("height", "5")])), Some(rec("Tag large", &[("height", "7")]))),
        Op::SetProperty { object: ObjectRef::Standard, key: "x.two".into(), before: None, after: Some("2".into()) },
        set_style(StyleLayer::Project, tag(), None, Some(rec("", &[("colour", "BU")]))),
    ];
    t.commit(Author::User, "t", t_ops.clone()).unwrap();
    s.commit(Author::User, "s", s_ops.clone()).unwrap();
    let mut scope = Scope::default();
    scope.add_ops(&t_ops);
    scope.add_ops(&s_ops);
    let plan = run_merge(&a, t.document(), s.document(), &scope, &BTreeMap::new());
    let style_conflicts: Vec<_> = plan.conflicts.iter().filter(|c| c.id.contains("style.")).collect();
    assert_eq!(style_conflicts.len(), 1, "{:?}", plan.conflicts);
    assert!(style_conflicts[0].what.contains("Text style \"Tag large\""), "{}", style_conflicts[0].what);
    let id = style_conflicts[0].id.clone();
    // Answered: the source's record, the other changes merged.
    let choices = BTreeMap::from([(id, Choice::Source)]);
    let plan = run_merge(&a, t.document(), s.document(), &scope, &choices);
    assert!(plan.unanswered.is_empty() && plan.problem.is_none(), "{:?} {:?}", plan.unanswered, plan.problem);
    let mut m = t.clone();
    m.commit(Author::User, "merge", plan.ops).unwrap();
    let d = m.document();
    assert_eq!(d.styles.get("text").and_then(|x| x.get(&big)).and_then(|r| r.fields.get("height")).map(String::as_str), Some("7"));
    assert_eq!(d.styles.get("text").and_then(|x| x.get(&tag())).and_then(|r| r.fields.get("colour")).map(String::as_str), Some("BU"));
    let values = &d.standard.as_ref().unwrap().values;
    assert_eq!((values.get("x.one").map(String::as_str), values.get("x.two").map(String::as_str)), (Some("1"), Some("2")));
    assert!(d.check().is_ok());
}

/// The standard's header is one unit: renamed differently on both sides is a conflict.
#[test]
fn the_standard_header_merges_as_one_unit() {
    let mut base = Ledger::new(50);
    base.commit(Author::User, "base", vec![Op::SetStandard { before: None, after: Some(header("House")) }]).unwrap();
    let a = base.document().clone();
    let (mut t, mut s) = (base.clone(), base.clone());
    let t_ops = vec![Op::SetStandard { before: Some(header("House")), after: Some(header("House A")) }];
    let s_ops = vec![Op::SetStandard { before: Some(header("House")), after: Some(header("House B")) }];
    t.commit(Author::User, "t", t_ops.clone()).unwrap();
    s.commit(Author::User, "s", s_ops.clone()).unwrap();
    let mut scope = Scope::default();
    scope.add_ops(&t_ops);
    scope.add_ops(&s_ops);
    let plan = run_merge(&a, t.document(), s.document(), &scope, &BTreeMap::new());
    assert!(plan.conflicts.iter().any(|c| c.id == "project/standard"), "{:?}", plan.conflicts);
}
