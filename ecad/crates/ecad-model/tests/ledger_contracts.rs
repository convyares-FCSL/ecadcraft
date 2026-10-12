//! Contract tests for the ledger: TX-01..TX-04, UNDO-01..UNDO-03, SAVE-01/02, DOC-05..DOC-07.

use ecad_model::{
    Author, CableMember, Color, Component, Conductor, Document, End, IdAllocator, Item, ItemKind, Layer, Ledger,
    ModelError, Op, Page, PageId, Primitive, Pt, Representation, Style, Subject, Terminal, TxKind,
};

struct Fixture {
    ids: IdAllocator,
    ledger: Ledger,
    page: PageId,
    layer: ecad_model::LayerId,
}

fn fixture() -> Fixture {
    let mut ids = IdAllocator::seeded(42);
    let mut ledger = Ledger::new(1);
    let page: PageId = ids.fresh();
    let layer = ids.fresh();
    ledger
        .commit(
            Author::User,
            "New project",
            vec![
                Op::AddPage { id: page, page: Page { title: "1".into() }, index: 0 },
                Op::AddLayer { id: layer, layer: Layer { name: "0".into() } },
            ],
        )
        .unwrap();
    Fixture { ids, ledger, page, layer }
}

fn line(f: &Fixture, a: (f64, f64), b: (f64, f64)) -> Item {
    Item {
        page: f.page,
        style: Style { layer: f.layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 },
        kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(a.0, a.1), b: Pt::new(b.0, b.1) }),
    }
}

#[test]
fn every_op_inverts_to_the_previous_state() {
    let mut f = fixture();
    let item = f.ids.fresh();
    let comp = f.ids.fresh();
    let term = f.ids.fresh();
    let cond = f.ids.fresh();
    let cable = f.ids.fresh();
    let rep = f.ids.fresh();
    let ops = vec![
        Op::AddItem { id: item, item: line(&f, (0.0, 0.0), (1.0, 0.0)) },
        Op::CreateComponent { id: comp, component: Component::default() },
        Op::AddTerminal { id: term, terminal: Terminal { component: comp, label: None, key: None } },
        Op::LinkRepresentation {
            id: rep,
            rep: Representation {
                subject: Subject::Component(comp),
                page: f.page,
                form: ecad_model::Form::Drawn,
                role: "main".into(),
                items: vec![item],
                connection_points: [(term, Pt::new(1.0, 0.0))].into(),
            },
        },
        Op::CreateConductor { id: cond, conductor: Conductor::default() },
        Op::AttachEnd { conductor: cond, end: End::B, before: None, after: Some(term) },
        Op::CreateCable { id: cable, cable: Default::default() },
        Op::AddCableMember { cable, index: 0, member: CableMember { conductor: cond, core: None } },
        Op::SetProperty {
            object: ecad_model::ObjectRef::Component(comp),
            key: "tag".into(),
            before: None,
            after: Some("-K1".into()),
        },
    ];
    let before = f.ledger.document().clone();
    let after = f.ledger.preview(&ops).unwrap();
    let mut back = after.clone();
    for op in ops.iter().rev() {
        op.inverse().apply(&mut back).unwrap();
    }
    assert_eq!(back, before);
    assert_ne!(after, before);
}

#[test]
fn a_failed_transaction_changes_nothing() {
    let mut f = fixture();
    let item = f.ids.fresh();
    let before = f.ledger.document().clone();
    let head = f.ledger.head();
    let cases: Vec<(&str, Vec<Op>)> = vec![
        ("empty", vec![]),
        ("precondition", vec![Op::RemoveItem { id: item, item: line(&f, (0.0, 0.0), (1.0, 1.0)) }]),
        (
            "second op fails",
            vec![
                Op::AddItem { id: item, item: line(&f, (0.0, 0.0), (1.0, 1.0)) },
                Op::AddItem { id: item, item: line(&f, (0.0, 0.0), (2.0, 2.0)) },
            ],
        ),
        (
            "dangling layer",
            vec![Op::AddItem {
                id: item,
                item: Item {
                    style: Style { layer: f.ids.fresh(), ..line(&f, (0.0, 0.0), (1.0, 1.0)).style },
                    ..line(&f, (0.0, 0.0), (1.0, 1.0))
                },
            }],
        ),
    ];
    for (name, ops) in cases {
        assert!(f.ledger.commit(Author::User, name, ops).is_err(), "{name} must fail");
        assert_eq!(f.ledger.document(), &before, "{name} changed the document");
        assert_eq!(f.ledger.head(), head, "{name} appended");
    }
}

#[test]
fn an_item_belongs_to_at_most_one_representation() {
    let mut f = fixture();
    let item = f.ids.fresh();
    let (c1, c2) = (f.ids.fresh(), f.ids.fresh());
    let rep = |subject| Representation {
        subject,
        page: f.page,
        form: ecad_model::Form::Drawn,
        role: "main".into(),
        items: vec![item],
        connection_points: Default::default(),
    };
    let ops = vec![
        Op::AddItem { id: item, item: line(&f, (0.0, 0.0), (1.0, 0.0)) },
        Op::CreateComponent { id: c1, component: Component::default() },
        Op::CreateComponent { id: c2, component: Component::default() },
        Op::LinkRepresentation { id: f.ids.fresh(), rep: rep(Subject::Component(c1)) },
        Op::LinkRepresentation { id: f.ids.fresh(), rep: rep(Subject::Component(c2)) },
    ];
    assert!(matches!(f.ledger.commit(Author::User, "x", ops), Err(ModelError::Invariant(_))));
}

#[test]
fn deleting_a_referenced_object_needs_explicit_detachment() {
    let mut f = fixture();
    let comp = f.ids.fresh();
    let term = f.ids.fresh();
    let cond = f.ids.fresh();
    let t = Terminal { component: comp, label: None, key: None };
    f.ledger
        .commit(
            Author::User,
            "setup",
            vec![
                Op::CreateComponent { id: comp, component: Component::default() },
                Op::AddTerminal { id: term, terminal: t.clone() },
                Op::CreateConductor { id: cond, conductor: Conductor::default() },
                Op::AttachEnd { conductor: cond, end: End::A, before: None, after: Some(term) },
            ],
        )
        .unwrap();
    let delete = vec![
        Op::RemoveTerminal { id: term, terminal: t.clone() },
        Op::DeleteComponent { id: comp, component: Component::default() },
    ];
    assert!(f.ledger.commit(Author::User, "delete without detaching", delete.clone()).is_err());
    let mut with_detach = vec![Op::AttachEnd { conductor: cond, end: End::A, before: Some(term), after: None }];
    with_detach.extend(delete);
    f.ledger.commit(Author::User, "delete component", with_detach).unwrap();
    assert!(f.ledger.document().conductors.contains_key(&cond), "the conductor survives with an open end");
}

#[test]
fn undo_and_redo_append_and_restore_state_with_the_same_ids() {
    let mut f = fixture();
    let mut states = vec![f.ledger.document().clone()];
    for i in 0..3 {
        let item = f.ids.fresh();
        f.ledger
            .commit(
                Author::User,
                "line",
                vec![Op::AddItem { id: item, item: line(&f, (0.0, i as f64), (1.0, i as f64)) }],
            )
            .unwrap();
        states.push(f.ledger.document().clone());
    }
    let head = f.ledger.head();
    for k in (0..3).rev() {
        let tx = f.ledger.undo(Author::User).unwrap();
        assert_eq!(f.ledger.document(), &states[k], "undo restores the recorded state");
        assert!(matches!(f.ledger.log().last().map(|t| t.kind), Some(TxKind::Undo { .. })));
        assert_eq!(f.ledger.log().last().map(|t| t.id), Some(tx));
    }
    for state in states.iter().skip(1) {
        f.ledger.redo(Author::User).unwrap();
        assert_eq!(f.ledger.document(), state, "redo restores the recorded state with the same IDs");
    }
    assert_eq!(f.ledger.head(), head + 6, "the log grows by one transaction per undo or redo");
    assert!(f.ledger.redo(Author::User).is_err());
}

#[test]
fn a_new_edit_after_undo_ends_redo_but_keeps_the_log() {
    let mut f = fixture();
    let (a, b) = (f.ids.fresh(), f.ids.fresh());
    f.ledger.commit(Author::User, "a", vec![Op::AddItem { id: a, item: line(&f, (0.0, 0.0), (1.0, 0.0)) }]).unwrap();
    f.ledger.undo(Author::User).unwrap();
    f.ledger.commit(Author::User, "b", vec![Op::AddItem { id: b, item: line(&f, (0.0, 1.0), (1.0, 1.0)) }]).unwrap();
    assert!(matches!(f.ledger.redo(Author::User), Err(ModelError::NothingTo(_))));
    assert_eq!(f.ledger.head(), 4);
    assert_eq!(f.ledger.undo_label(), Some("b"));
}

#[test]
fn undo_reaches_back_past_the_first_edit_only_to_the_empty_project() {
    let mut f = fixture();
    f.ledger.undo(Author::User).unwrap();
    assert_eq!(f.ledger.document(), &Document::default());
    assert!(f.ledger.undo(Author::User).is_err());
}

#[test]
fn unsaved_state_follows_head_and_persisted_position() {
    let mut f = fixture();
    assert!(f.ledger.is_unsaved());
    f.ledger.mark_persisted(f.ledger.head()).unwrap();
    assert!(!f.ledger.is_unsaved());
    // A non-graphical edit moves the head.
    let comp = f.ids.fresh();
    f.ledger
        .commit(Author::User, "component", vec![Op::CreateComponent { id: comp, component: Component::default() }])
        .unwrap();
    assert!(f.ledger.is_unsaved());
    f.ledger.mark_persisted(f.ledger.head()).unwrap();
    // Undo back to saved content is still unsaved (SAVE-02).
    f.ledger.undo(Author::User).unwrap();
    assert!(f.ledger.is_unsaved());
    assert!(f.ledger.mark_persisted(f.ledger.head() + 1).is_err());
}

#[test]
fn historical_state_is_reconstructed_without_touching_the_live_document() {
    let mut f = fixture();
    let item = f.ids.fresh();
    f.ledger.commit(Author::User, "a", vec![Op::AddItem { id: item, item: line(&f, (0.0, 0.0), (1.0, 0.0)) }]).unwrap();
    let live = f.ledger.document().clone();
    let at1 = f.ledger.state_at(1).unwrap();
    assert!(at1.items.is_empty());
    assert_eq!(f.ledger.document(), &live);
    assert!(f.ledger.replay_matches().unwrap());
    let replayed = Ledger::replay(f.ledger.log().to_vec(), 9).unwrap();
    assert_eq!(replayed.document(), &live);
    assert!(f.ledger.state_at(99).is_err());
}

#[test]
fn ids_from_one_allocator_never_repeat() {
    let mut ids = IdAllocator::seeded(3);
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..10_000 {
        let id: ecad_model::ItemId = ids.fresh();
        assert!(seen.insert(id));
    }
}

#[test]
fn ids_serialise_as_exact_hex_strings() {
    let id = ecad_model::ComponentId(u128::MAX - 5);
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, format!("\"{:032x}\"", u128::MAX - 5));
    assert_eq!(serde_json::from_str::<ecad_model::ComponentId>(&json).unwrap(), id);
    let mut doc = Document::default();
    doc.components.insert(id, Component::default());
    let back: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert_eq!(back, doc);
}
