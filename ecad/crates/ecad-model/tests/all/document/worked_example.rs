//! A worked example as an executable scenario, at the model level.

use ecad_model::connectivity::derive::{mismatches, nets};
use ecad_model::{
    Author, Color, ConnectionPoint, ConnectionPointId, Core, Device, End, Form, IdAllocator, Item, ItemKind, Layer, Ledger, Op, Page, Primitive, Pt,
    Representation, Route, RouteEnd, Style, Subject, WireId,
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
        vec![Op::AddPage { id: p1, page: Page::titled("1").unwrap(), index: 0 }, Op::AddLayer { id: layer, layer: Layer { name: "0".into() } }],
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
        kind: ItemKind::Primitive(Primitive::Polyline { vertices: v.iter().map(|p| Pt::new(p.0, p.1)).collect(), closed: false }),
    };

    // 1. Raw geometry.
    let rect: Vec<_> = (0..4).map(|_| ids.fresh()).collect();
    let corners = [((0.0, 0.0), (40.0, 0.0)), ((40.0, 0.0), (40.0, 20.0)), ((40.0, 20.0), (0.0, 20.0)), ((0.0, 20.0), (0.0, 0.0))];
    l.commit(Author::User, "Line", rect.iter().zip(corners).map(|(id, (a, b))| Op::AddItem { id: *id, item: line(a, b) }).collect()).unwrap();

    // 2. Convert to component, add terminals.
    let (c1, r1) = (ids.fresh(), ids.fresh());
    let rep1 = Representation {
        subject: Subject::Device(c1),
        page: p1,
        form: Form::Drawn,
        role: ecad_model::Role::Part(ecad_model::PartRole::Other),
        items: rect.clone(),
        connection_points: Default::default(),
    };
    l.commit(
        Author::User,
        "Convert to component",
        vec![Op::CreateDevice { id: c1, device: Device::default() }, Op::LinkRepresentation { id: r1, rep: rep1.clone() }],
    )
    .unwrap();
    let ts: Vec<_> = (0..3).map(|_| ids.fresh()).collect();
    let mut rep1b = rep1.clone();
    for (t, y) in ts.iter().zip([15.0, 10.0, 5.0]) {
        rep1b.connection_points.insert(*t, Pt::new(40.0, y));
    }
    let mut ops: Vec<Op> = ts
        .iter()
        .map(|t| Op::AddConnectionPoint {
            id: *t,
            connection_point: ConnectionPoint { owner: c1.into(), label: None, key: None, accepts: Default::default() },
        })
        .collect();
    ops.push(Op::ModifyRepresentation { id: r1, before: rep1.clone(), after: rep1b.clone() });
    l.commit(Author::User, "Add terminals", ops).unwrap();

    // The far ends: a second device's connection points at x = 90.
    let (c2, r2, edge) = (ids.fresh(), ids.fresh(), ids.fresh());
    let us: Vec<ConnectionPointId> = (0..3).map(|_| ids.fresh()).collect();
    let mut ops = vec![Op::AddItem { id: edge, item: line((95.0, 0.0), (95.0, 20.0)) }, Op::CreateDevice { id: c2, device: Device::default() }];
    ops.extend(us.iter().map(|u| Op::AddConnectionPoint {
        id: *u,
        connection_point: ConnectionPoint { owner: c2.into(), label: None, key: None, accepts: Default::default() },
    }));
    ops.push(Op::LinkRepresentation {
        id: r2,
        rep: Representation {
            subject: Subject::Device(c2),
            page: p1,
            form: Form::Drawn,
            role: ecad_model::Role::Part(ecad_model::PartRole::Head),
            items: vec![edge],
            connection_points: us.iter().zip([15.0, 10.0, 5.0]).map(|(u, y)| (*u, Pt::new(90.0, y))).collect(),
        },
    });
    l.commit(Author::User, "Far ends", ops).unwrap();

    // 3. A loose route, then connected; two more drawn attached. The wires come from the routes
    //    (design 072 §4.4): a route between two points carries one, a loose one none.
    let pts = |v: &[(f64, f64)]| v.iter().map(|p| Pt::new(p.0, p.1)).collect::<Vec<_>>();
    let mut routes = Vec::new();
    for (k, y) in [15.0, 10.0, 5.0].into_iter().enumerate() {
        let (rt, i, r) = (ids.fresh(), ids.fresh(), ids.fresh());
        let x_end = if k == 0 { 55.0 } else { 40.0 };
        let to = if k == 0 { RouteEnd::Free } else { RouteEnd::Point(ts[k]) };
        let ops = vec![
            Op::AddItem { id: i, item: poly(&[(90.0, y), (x_end, y)]) },
            Op::AddRoute { id: rt, route: Route::new(p1, pts(&[(90.0, y), (x_end, y)]), [RouteEnd::Point(us[k]), to]) },
            Op::LinkRepresentation {
                id: r,
                rep: Representation {
                    subject: Subject::Route(rt),
                    page: p1,
                    form: Form::Drawn,
                    role: ecad_model::Role::Route,
                    items: vec![i],
                    connection_points: Default::default(),
                },
            },
        ];
        l.commit(Author::User, "Wire", ops).unwrap();
        routes.push((rt, i));
    }
    assert_eq!(l.document().wires().len(), 2, "the loose route carries no wire yet");
    let (r0, i5) = routes[0];
    let before = l.document().routes()[&r0].clone();
    let after = Route::new(p1, pts(&[(90.0, 15.0), (40.0, 15.0)]), [RouteEnd::Point(us[0]), RouteEnd::Point(ts[0])]);
    l.commit(
        Author::User,
        "Connect",
        vec![
            Op::ModifyItem { id: i5, before: poly(&[(90.0, 15.0), (55.0, 15.0)]), after: poly(&[(90.0, 15.0), (40.0, 15.0)]) },
            Op::ModifyRoute { id: r0, before, after },
        ],
    )
    .unwrap();
    assert!(mismatches(l.document(), 1e-9).is_empty());
    let wires: Vec<WireId> = l.document().wires().keys().copied().collect();
    assert_eq!(wires.len(), 3);
    for (u, t) in us.iter().zip(&ts) {
        assert!(l.document().wires().values().any(|w| w.ends.contains(u) && w.ends.contains(t)), "a wire from {u} to {t}");
    }

    // 4. Cable.
    let k1 = ids.fresh();
    let cores = wires.iter().map(|w| Core::on_wire(*w, None)).collect();
    let ops = vec![Op::CreateCable { id: k1, cable: ecad_model::Cable { cores, ..Default::default() } }];
    l.commit(Author::User, "Group as cable", ops).unwrap();
    let conductors_before: Vec<_> = l.document().wires().clone().into_iter().collect();

    // 5. Reroute the second route.
    let (r6, i6) = routes[1];
    let before = l.document().routes()[&r6].clone();
    let after = Route { points: pts(&[(90.0, 10.0), (65.0, 25.0), (40.0, 10.0)]), ..before.clone() };
    l.commit(
        Author::User,
        "Grip edit",
        vec![
            Op::ModifyItem { id: i6, before: poly(&[(90.0, 10.0), (40.0, 10.0)]), after: poly(&[(90.0, 10.0), (65.0, 25.0), (40.0, 10.0)]) },
            Op::ModifyRoute { id: r6, before, after },
        ],
    )
    .unwrap();
    assert_eq!(l.document().wires().clone().into_iter().collect::<Vec<_>>(), conductors_before, "a reroute keeps the wires");
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
    for (rt, i) in &routes {
        let before = l.document().items()[i].clone();
        let ItemKind::Primitive(p) = &before.kind else { unreachable!() };
        let (_, b) = p.path_ends().unwrap();
        let after = Item { kind: ItemKind::Primitive(p.with_path_end(End::B, Pt::new(b.x, b.y - 5.0)).unwrap()), ..before.clone() };
        ops.push(Op::ModifyItem { id: *i, before, after });
        let before = l.document().routes()[rt].clone();
        let mut after = before.clone();
        if let Some(last) = after.points.last_mut() {
            last.y -= 5.0;
        }
        ops.push(Op::ModifyRoute { id: *rt, before, after });
    }
    l.commit(Author::User, "Move", ops).unwrap();
    assert!(mismatches(l.document(), 1e-9).is_empty(), "drawing and intent agree after connected movement");
    assert_eq!(nets(l.document()), nets_before, "movement does not change connectivity");
    let after_move = l.document().clone();

    // 7. Undo ×3, redo.
    l.undo(Author::User).unwrap();
    l.undo(Author::User).unwrap();
    l.undo(Author::User).unwrap();
    assert!(l.document().cables().is_empty());
    assert_eq!(l.document().wires().clone().into_iter().collect::<Vec<_>>(), conductors_before, "conductor identities untouched");
    l.redo(Author::User).unwrap();
    assert!(l.document().cables().contains_key(&k1), "redo recreates the cable with the same ID");
    l.redo(Author::User).unwrap();
    l.redo(Author::User).unwrap();
    assert_eq!(l.document(), &after_move);

    // Replay equals live.
    assert!(l.replay_matches().unwrap());
}
