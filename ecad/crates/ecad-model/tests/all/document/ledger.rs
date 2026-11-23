//! Contract tests for the ledger: transactions, undo and redo, the unsaved state and the document rules.

use ecad_model::{
    Author, Color, ConnectionPoint, Core, Device, Document, IdAllocator, Item, ItemKind, Layer, Ledger, ModelError, Op, Page, PageId, Primitive, Pt,
    Representation, Style, Subject, TxKind, Wire,
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
            vec![Op::AddPage { id: page, page: Page::titled("1").unwrap(), index: 0 }, Op::AddLayer { id: layer, layer: Layer { name: "0".into() } }],
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
    let term2 = f.ids.fresh();
    let cond = f.ids.fresh();
    let cable = f.ids.fresh();
    let rep = f.ids.fresh();
    let ops = vec![
        Op::AddItem { id: item, item: line(&f, (0.0, 0.0), (1.0, 0.0)) },
        Op::CreateDevice { id: comp, device: Device::default() },
        Op::AddConnectionPoint {
            id: term,
            connection_point: ConnectionPoint { owner: comp.into(), label: None, key: None, accepts: Default::default() },
        },
        Op::AddConnectionPoint {
            id: term2,
            connection_point: ConnectionPoint { owner: comp.into(), label: None, key: None, accepts: Default::default() },
        },
        Op::LinkRepresentation {
            id: rep,
            rep: Representation {
                subject: Subject::Device(comp),
                page: f.page,
                form: ecad_model::Form::Drawn,
                role: ecad_model::Role::Part(ecad_model::PartRole::Other),
                items: vec![item],
                connection_points: [(term, Pt::new(1.0, 0.0))].into(),
            },
        },
        Op::CreateWire { id: cond, wire: Wire::new(f.ids.fresh(), f.ids.fresh(), term2, term) },
        Op::CreateCable { id: cable, cable: Default::default() },
        Op::ModifyCable {
            id: cable,
            before: Default::default(),
            after: ecad_model::Cable { cores: vec![Core::on_wire(cond, None)], ..Default::default() },
        },
        Op::SetProperty { object: ecad_model::ObjectRef::Device(comp), key: "note".parse().unwrap(), before: None, after: Some("-K1".into()) },
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
            vec![Op::AddItem { id: item, item: line(&f, (0.0, 0.0), (1.0, 1.0)) }, Op::AddItem { id: item, item: line(&f, (0.0, 0.0), (2.0, 2.0)) }],
        ),
        (
            "dangling layer",
            vec![Op::AddItem {
                id: item,
                item: Item { style: Style { layer: f.ids.fresh(), ..line(&f, (0.0, 0.0), (1.0, 1.0)).style }, ..line(&f, (0.0, 0.0), (1.0, 1.0)) },
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
        role: ecad_model::Role::Part(ecad_model::PartRole::Other),
        items: vec![item],
        connection_points: Default::default(),
    };
    let ops = vec![
        Op::AddItem { id: item, item: line(&f, (0.0, 0.0), (1.0, 0.0)) },
        Op::CreateDevice { id: c1, device: Device::default() },
        Op::CreateDevice { id: c2, device: Device::default() },
        Op::LinkRepresentation { id: f.ids.fresh(), rep: rep(Subject::Device(c1)) },
        Op::LinkRepresentation { id: f.ids.fresh(), rep: rep(Subject::Device(c2)) },
    ];
    assert!(matches!(f.ledger.commit(Author::User, "x", ops), Err(ModelError::Invariant(_))));
}

#[test]
fn deleting_a_referenced_object_needs_explicit_detachment() {
    let mut f = fixture();
    let comp: ecad_model::DeviceId = f.ids.fresh();
    let (term, term2) = (f.ids.fresh(), f.ids.fresh());
    let cond = f.ids.fresh();
    let cable = f.ids.fresh();
    let t = ConnectionPoint { owner: comp.into(), label: None, key: None, accepts: Default::default() };
    let wire = Wire::new(f.ids.fresh(), f.ids.fresh(), term, term2);
    let with_member = ecad_model::Cable { cores: vec![Core::on_wire(cond, None)], ..Default::default() };
    f.ledger
        .commit(
            Author::User,
            "setup",
            vec![
                Op::CreateDevice { id: comp, device: Device::default() },
                Op::AddConnectionPoint { id: term, connection_point: t.clone() },
                Op::AddConnectionPoint { id: term2, connection_point: t.clone() },
                Op::CreateWire { id: cond, wire: wire.clone() },
                Op::CreateCable { id: cable, cable: with_member.clone() },
            ],
        )
        .unwrap();
    // A wire a cable holds: deleting it alone leaves the cable naming a missing wire.
    let delete = vec![Op::DeleteWire { id: cond, wire }];
    assert!(f.ledger.commit(Author::User, "delete without detaching", delete.clone()).is_err());
    // Removing a connection point takes the wires ending there with it (design 072 §4.4): the
    // rewrite removes the wire's place in its cable too, in the same transaction.
    let remove_point = vec![Op::RemoveConnectionPoint { id: term, connection_point: t.clone() }];
    f.ledger.commit(Author::User, "remove a point of a cabled wire", remove_point).unwrap();
    assert!(!f.ledger.document().wires().contains_key(&cond));
    assert!(f.ledger.document().cables()[&cable].cores.is_empty());
    f.ledger.undo(Author::User).unwrap();
    assert!(f.ledger.document().wires().contains_key(&cond));
    let mut with_detach = vec![Op::ModifyCable { id: cable, before: with_member, after: Default::default() }];
    with_detach.extend(delete);
    f.ledger.commit(Author::User, "delete wire", with_detach).unwrap();
    assert!(!f.ledger.document().wires().contains_key(&cond));
    assert!(f.ledger.document().cables()[&cable].cores.is_empty(), "the cable survives without it");
}

#[test]
fn undo_and_redo_append_and_restore_state_with_the_same_ids() {
    let mut f = fixture();
    let mut states = vec![f.ledger.document().clone()];
    for i in 0..3 {
        let item = f.ids.fresh();
        f.ledger.commit(Author::User, "line", vec![Op::AddItem { id: item, item: line(&f, (0.0, i as f64), (1.0, i as f64)) }]).unwrap();
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
    f.ledger.commit(Author::User, "component", vec![Op::CreateDevice { id: comp, device: Device::default() }]).unwrap();
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
    assert!(at1.items().is_empty());
    assert_eq!(f.ledger.document(), &live);
    assert!(f.ledger.replay_matches().unwrap());
    let replayed = Ledger::replay(f.ledger.full_log().unwrap(), 9).unwrap();
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
    let id = ecad_model::DeviceId(u128::MAX - 5);
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, format!("\"{:032x}\"", u128::MAX - 5));
    assert_eq!(serde_json::from_str::<ecad_model::DeviceId>(&json).unwrap(), id);
    let mut doc = Document::default();
    doc.edit_parts(|parts| parts.devices.insert(id, Device::default()));
    let back: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert_eq!(back, doc);
}

#[test]
fn definitions_with_non_finite_geometry_or_repeated_keys_are_refused() {
    let mut f = fixture();
    let def = |at: Pt, key2: &str| ecad_model::Definition {
        bridges: Vec::new(),
        name: "d".into(),
        origin: None,
        graphics: vec![Primitive::Line { a: Pt::new(0.0, 0.0), b: at }],
        connection_points: vec![
            ecad_model::ConnectionPointDef { key: "A1".parse().unwrap(), label: None, at, accepts: Default::default() },
            ecad_model::ConnectionPointDef { key: key2.parse().unwrap(), label: None, at: Pt::new(0.0, 1.0), accepts: Default::default() },
        ],
        ..Default::default()
    };
    for (name, d) in
        [("nan", def(Pt::new(f64::NAN, 0.0), "A2")), ("inf", def(Pt::new(f64::INFINITY, 0.0), "A2")), ("dup", def(Pt::new(1.0, 0.0), "A1"))]
    {
        let id = f.ids.fresh();
        assert!(
            matches!(f.ledger.commit(Author::User, name, vec![Op::EmbedDefinition { id, definition: d }]), Err(ModelError::Invariant(_))),
            "{name}"
        );
    }
}

#[test]
fn a_symbol_must_show_its_components_own_definition() {
    let mut f = fixture();
    let good = ecad_model::Definition {
        bridges: Vec::new(),
        name: "d".parse().unwrap(),
        origin: None,
        graphics: vec![],
        connection_points: vec![],
        ..Default::default()
    };
    let (d1, d2, c, i, r) = (f.ids.fresh(), f.ids.fresh(), f.ids.fresh(), f.ids.fresh(), f.ids.fresh());
    let item = Item {
        page: f.page,
        style: line(&f, (0.0, 0.0), (1.0, 0.0)).style,
        kind: ItemKind::Symbol(ecad_model::SymbolInstance { definition: d2, placement: Default::default() }),
    };
    let ops = vec![
        Op::EmbedDefinition { id: d1, definition: good.clone() },
        Op::EmbedDefinition { id: d2, definition: good },
        Op::CreateDevice { id: c, device: Device { definition: Some(d1), ..Default::default() } },
        Op::AddItem { id: i, item },
        Op::LinkRepresentation {
            id: r,
            rep: Representation {
                subject: Subject::Device(c),
                page: f.page,
                form: ecad_model::Form::Symbol,
                role: ecad_model::Role::Part(ecad_model::PartRole::Other),
                items: vec![i],
                connection_points: Default::default(),
            },
        },
    ];
    assert!(matches!(f.ledger.commit(Author::User, "mismatch", ops), Err(ModelError::Invariant(_))));
}

#[test]
fn a_named_version_must_name_existing_history() {
    let mut f = fixture();
    let head = f.ledger.head();
    let name = |at| vec![Op::NameVersion { name: ecad_model::PointName::version(&format!("v{at}")).unwrap(), at, description: None }];
    // The future, or the naming transaction itself, is refused and nothing is appended.
    for at in [head + 1, head + 5] {
        let r = f.ledger.commit(Author::User, "name", name(at));
        assert!(matches!(r, Err(ModelError::Invariant(_))), "at {at}: {r:?}");
        assert_eq!(f.ledger.head(), head);
    }
    // Existing history, including the empty project, is accepted and resolvable.
    for at in [0, head] {
        f.ledger.commit(Author::User, "name", name(at)).unwrap();
        assert!(f.ledger.state_at(at).is_ok());
    }
    // Undo and redo of a valid bookmark stay valid.
    f.ledger.undo(Author::User).unwrap();
    f.ledger.redo(Author::User).unwrap();
    // A log holding a bookmark to the future does not replay.
    let mut log = f.ledger.full_log().unwrap();
    let bad_seq = log.len() as u64 + 1;
    log.push(ecad_model::Transaction {
        id: f.ids.fresh(),
        seq: bad_seq,
        author: Author::User,
        label: "bad".into(),
        kind: TxKind::Edit,
        time_ms: None,
        ops: name(bad_seq),
    });
    assert!(matches!(Ledger::replay(log.clone(), 1), Err(ModelError::CorruptLog(_))));
    let (prefix, failure) = Ledger::replay_prefix(log, 1);
    assert_eq!(prefix.head(), bad_seq - 1);
    assert!(matches!(failure, Some((_, ModelError::Invariant(_)))), "{failure:?}");
}
