//! The proposal's worked example (§7) as an executable scenario, at the model level.

use ecad_model::derive::{mismatches, nets};
use ecad_model::{
    Author, CableMember, Color, Component, Conductor, End, Form, IdAllocator, Item, ItemKind, Layer, Ledger, Op, Page,
    Primitive, Pt, Representation, Style, Subject, Terminal,
};

fn style(layer: ecad_model::LayerId) -> Style {
    Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 }
}

#[test]
fn worked_example_replays_and_undoes() {
    let mut ids = IdAllocator::seeded(11);
    let mut l = Ledger::new(5);
    let (p1, layer) = (ids.fresh(), ids.fresh());
    l.commit(
        Author::User,
        "New project",
        vec![
            Op::AddPage { id: p1, page: Page { title: "1".into() }, index: 0 },
            Op::AddLayer { id: layer, layer: Layer { name: "0".into() } },
        ],
    )
    .unwrap();
    let line = |a: (f64, f64), b: (f64, f64)| Item {
        page: p1,
        style: style(layer),
        kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(a.0, a.1), b: Pt::new(b.0, b.1) }),
    };
    let poly = |v: &[(f64, f64)]| Item {
        page: p1,
        style: style(layer),
        kind: ItemKind::Primitive(Primitive::Polyline {
            vertices: v.iter().map(|p| Pt::new(p.0, p.1)).collect(),
            closed: false,
        }),
    };

    // 1. Raw geometry.
    let rect: Vec<_> = (0..4).map(|_| ids.fresh()).collect();
    let corners = [
        ((0.0, 0.0), (40.0, 0.0)),
        ((40.0, 0.0), (40.0, 20.0)),
        ((40.0, 20.0), (0.0, 20.0)),
        ((0.0, 20.0), (0.0, 0.0)),
    ];
    l.commit(
        Author::User,
        "Line",
        rect.iter().zip(corners).map(|(id, (a, b))| Op::AddItem { id: *id, item: line(a, b) }).collect(),
    )
    .unwrap();

    // 2. Convert to component, add terminals.
    let (c1, r1) = (ids.fresh(), ids.fresh());
    let rep1 = Representation {
        subject: Subject::Component(c1),
        page: p1,
        form: Form::Drawn,
        role: "main".into(),
        items: rect.clone(),
        connection_points: Default::default(),
    };
    l.commit(
        Author::User,
        "Convert to component",
        vec![
            Op::CreateComponent { id: c1, component: Component::default() },
            Op::LinkRepresentation { id: r1, rep: rep1.clone() },
        ],
    )
    .unwrap();
    let ts: Vec<_> = (0..3).map(|_| ids.fresh()).collect();
    let mut rep1b = rep1.clone();
    for (t, y) in ts.iter().zip([15.0, 10.0, 5.0]) {
        rep1b.connection_points.insert(*t, Pt::new(40.0, y));
    }
    let mut ops: Vec<Op> = ts
        .iter()
        .map(|t| Op::AddTerminal { id: *t, terminal: Terminal { component: c1, label: None, key: None } })
        .collect();
    ops.push(Op::ModifyRepresentation { id: r1, before: rep1.clone(), after: rep1b.clone() });
    l.commit(Author::User, "Add terminals", ops).unwrap();

    // 3. Loose conductor, then connect; two more drawn attached.
    let mut wires = Vec::new();
    for (k, y) in [15.0, 10.0, 5.0].into_iter().enumerate() {
        let (w, i, r) = (ids.fresh(), ids.fresh(), ids.fresh());
        let x_end = if k == 0 { 55.0 } else { 40.0 };
        let mut ops = vec![
            Op::CreateConductor { id: w, conductor: Conductor::default() },
            Op::AddItem { id: i, item: poly(&[(90.0, y), (x_end, y)]) },
            Op::LinkRepresentation {
                id: r,
                rep: Representation {
                    subject: Subject::Conductor(w),
                    page: p1,
                    form: Form::Drawn,
                    role: "route".into(),
                    items: vec![i],
                    connection_points: Default::default(),
                },
            },
        ];
        if k > 0 {
            ops.push(Op::AttachEnd { conductor: w, end: End::B, before: None, after: Some(ts[k]) });
        }
        l.commit(Author::User, "Wire", ops).unwrap();
        wires.push((w, i));
    }
    let (w1, i5) = wires[0];
    l.commit(
        Author::User,
        "Connect",
        vec![
            Op::ModifyItem {
                id: i5,
                before: poly(&[(90.0, 15.0), (55.0, 15.0)]),
                after: poly(&[(90.0, 15.0), (40.0, 15.0)]),
            },
            Op::AttachEnd { conductor: w1, end: End::B, before: None, after: Some(ts[0]) },
        ],
    )
    .unwrap();
    assert!(mismatches(l.document(), 1e-9).is_empty());

    // 4. Cable.
    let k1 = ids.fresh();
    let mut ops = vec![Op::CreateCable { id: k1, cable: Default::default() }];
    for (n, (w, _)) in wires.iter().enumerate() {
        ops.push(Op::AddCableMember { cable: k1, index: n, member: CableMember { conductor: *w, core: None } });
    }
    l.commit(Author::User, "Group as cable", ops).unwrap();
    let conductors_before: Vec<_> = l.document().conductors.clone().into_iter().collect();

    // 5. Reroute w2.
    let i6 = wires[1].1;
    l.commit(
        Author::User,
        "Grip edit",
        vec![Op::ModifyItem {
            id: i6,
            before: poly(&[(90.0, 10.0), (40.0, 10.0)]),
            after: poly(&[(90.0, 10.0), (65.0, 25.0), (40.0, 10.0)]),
        }],
    )
    .unwrap();
    let nets_before = nets(l.document());

    // 6. Move the component; ends follow.
    let mut ops = Vec::new();
    for (id, (a, b)) in rect.iter().zip(corners) {
        ops.push(Op::ModifyItem { id: *id, before: line(a, b), after: line((a.0, a.1 - 5.0), (b.0, b.1 - 5.0)) });
    }
    let mut rep1c = rep1b.clone();
    for p in rep1c.connection_points.values_mut() {
        p.y -= 5.0;
    }
    ops.push(Op::ModifyRepresentation { id: r1, before: rep1b.clone(), after: rep1c });
    let before_items: Vec<Item> = wires.iter().map(|(_, i)| l.document().items[i].clone()).collect();
    for ((_, i), before) in wires.iter().zip(before_items) {
        let ItemKind::Primitive(p) = &before.kind else { unreachable!() };
        let (_, b) = p.path_ends().unwrap();
        let after = Item {
            kind: ItemKind::Primitive(p.with_path_end(End::B, Pt::new(b.x, b.y - 5.0)).unwrap()),
            ..before.clone()
        };
        ops.push(Op::ModifyItem { id: *i, before, after });
    }
    l.commit(Author::User, "Move", ops).unwrap();
    assert!(mismatches(l.document(), 1e-9).is_empty(), "drawing and intent agree after connected movement");
    assert_eq!(nets(l.document()), nets_before, "movement does not change connectivity");
    let after_move = l.document().clone();

    // 7. Undo ×3, redo.
    l.undo(Author::User).unwrap();
    l.undo(Author::User).unwrap();
    l.undo(Author::User).unwrap();
    assert!(l.document().cables.is_empty());
    assert_eq!(
        l.document().conductors.clone().into_iter().collect::<Vec<_>>(),
        conductors_before,
        "conductor identities untouched"
    );
    l.redo(Author::User).unwrap();
    assert!(l.document().cables.contains_key(&k1), "redo recreates the cable with the same ID");
    l.redo(Author::User).unwrap();
    l.redo(Author::User).unwrap();
    assert_eq!(l.document(), &after_move);

    // Replay equals live.
    assert!(l.replay_matches().unwrap());
}
