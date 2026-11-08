//! Brief 018 at the model level: one device drawn by several symbols of different definitions
//! (I14 as relaxed, owner decision 2026-11-08) and the tag index.

use ecad_model::derive::terminal_positions;
use ecad_model::{
    Author, Color, Component, ComponentId, Definition, DefinitionId, IdAllocator, Item, ItemId, ItemKind, Layer,
    Ledger, ObjectRef, Op, Page, PageId, Placement, Primitive, Pt, Representation, Style, Subject, SymbolInstance,
    Terminal, TerminalDef, TerminalId,
};

struct Fx {
    ids: IdAllocator,
    l: Ledger,
    page: PageId,
    layer: ecad_model::LayerId,
    coil: DefinitionId,
    contact: DefinitionId,
    k1: ComponentId,
}

fn def(name: &str, keys: [&str; 2]) -> Definition {
    Definition {
        name: name.into(),
        origin: None,
        graphics: vec![Primitive::Line { a: Pt::new(0.0, 10.0), b: Pt::new(0.0, -10.0) }],
        terminals: vec![
            TerminalDef { key: keys[0].into(), label: Some(keys[0].into()), at: Pt::new(0.0, 10.0) },
            TerminalDef { key: keys[1].into(), label: Some(keys[1].into()), at: Pt::new(0.0, -10.0) },
        ],
    }
}

fn symbol(fx: &Fx, definition: DefinitionId, at: Pt) -> Item {
    Item {
        page: fx.page,
        style: Style { layer: fx.layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 },
        kind: ItemKind::Symbol(SymbolInstance {
            definition,
            placement: Placement { origin: at, rotation: 0.0, mirror: false, scale: 1.0 },
        }),
    }
}

fn fixture() -> Fx {
    let mut ids = IdAllocator::seeded(18);
    let mut l = Ledger::new(18);
    let (page, layer, coil, contact, k1) = (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
    let mut props = ecad_model::Properties::new();
    props.insert("tag".into(), "-K01".into());
    l.commit(
        Author::User,
        "setup",
        vec![
            Op::AddPage { id: page, page: Page { title: "1".into() }, index: 0 },
            Op::AddLayer { id: layer, layer: Layer { name: "0".into() } },
            Op::EmbedDefinition { id: coil, definition: def("coil", ["A1", "A2"]) },
            Op::EmbedDefinition { id: contact, definition: def("no", ["13", "14"]) },
            Op::CreateComponent { id: k1, component: Component { properties: props, definition: Some(coil) } },
        ],
    )
    .unwrap();
    Fx { ids, l, page, layer, coil, contact, k1 }
}

/// Ops adding a symbol of `definition` at `at` as a representation of K1 with two new terminals
/// labelled `labels`, with connection points (local frame) when `cps`.
fn contact_ops(
    fx: &mut Fx,
    definition: DefinitionId,
    at: Pt,
    labels: [&str; 2],
    cps: bool,
) -> (Vec<Op>, [TerminalId; 2]) {
    let (t1, t2, item, rep): (TerminalId, TerminalId, ItemId, ecad_model::RepresentationId) =
        (fx.ids.fresh(), fx.ids.fresh(), fx.ids.fresh(), fx.ids.fresh());
    let mut connection_points = std::collections::BTreeMap::new();
    if cps {
        connection_points.insert(t1, Pt::new(0.0, 10.0));
        connection_points.insert(t2, Pt::new(0.0, -10.0));
    }
    let k1 = fx.k1;
    let term = |l: &str| Terminal { component: k1, label: Some(l.into()), key: Some(l.into()) };
    let ops = vec![
        Op::AddTerminal { id: t1, terminal: term(labels[0]) },
        Op::AddTerminal { id: t2, terminal: term(labels[1]) },
        Op::AddItem { id: item, item: symbol(fx, definition, at) },
        Op::LinkRepresentation {
            id: rep,
            rep: Representation {
                subject: Subject::Component(fx.k1),
                page: fx.page,
                form: ecad_model::Form::Symbol,
                role: "no".into(),
                items: vec![item],
                connection_points,
            },
        },
    ];
    (ops, [t1, t2])
}

#[test]
fn i14_as_relaxed_table() {
    // (definition shown, connection points, accepted)
    let rows = [
        ("own definition, keys", true, false, true),
        ("other definition, no points", false, false, false),
        ("other definition, points", false, true, true),
    ];
    for (case, own, cps, ok) in rows {
        let mut fx = fixture();
        let d = if own { fx.coil } else { fx.contact };
        let labels = if own { ["A1", "A2"] } else { ["23", "24"] };
        let (ops, _) = contact_ops(&mut fx, d, Pt::new(50.0, 0.0), labels, cps);
        let r = fx.l.commit(Author::User, case, ops);
        assert_eq!(r.is_ok(), ok, "{case}: {r:?}");
        fx.l.document().check().unwrap();
    }
}

#[test]
fn connection_points_of_a_symbol_follow_its_placement() {
    let mut fx = fixture();
    let contact = fx.contact;
    let (ops, [t1, t2]) = contact_ops(&mut fx, contact, Pt::new(50.0, 0.0), ["23", "24"], true);
    fx.l.commit(Author::User, "join", ops).unwrap();
    let doc = fx.l.document();
    let shown = terminal_positions(doc, fx.page);
    assert!(shown.iter().any(|(t, _, p)| *t == t1 && p.near(Pt::new(50.0, 10.0), 1e-9)));
    assert!(shown.iter().any(|(t, _, p)| *t == t2 && p.near(Pt::new(50.0, -10.0), 1e-9)));
    // Moving the instance moves its terminals, with no change to the representation.
    let (item, before) = doc
        .items
        .iter()
        .find(|(_, i)| matches!(&i.kind, ItemKind::Symbol(s) if s.definition == fx.contact))
        .map(|(id, i)| (*id, i.clone()))
        .unwrap();
    let after = symbol(&fx, fx.contact, Pt::new(80.0, 5.0));
    fx.l.commit(Author::User, "move", vec![Op::ModifyItem { id: item, before, after }]).unwrap();
    let shown = terminal_positions(fx.l.document(), fx.page);
    assert!(shown.iter().any(|(t, _, p)| *t == t1 && p.near(Pt::new(80.0, 15.0), 1e-9)));
}

#[test]
fn connection_points_must_name_the_components_own_terminals() {
    let mut fx = fixture();
    let other: ComponentId = fx.ids.fresh();
    let foreign: TerminalId = fx.ids.fresh();
    fx.l.commit(
        Author::User,
        "other",
        vec![
            Op::CreateComponent { id: other, component: Component::default() },
            Op::AddTerminal { id: foreign, terminal: Terminal { component: other, label: None, key: None } },
        ],
    )
    .unwrap();
    let contact = fx.contact;
    let (mut ops, [t1, _]) = contact_ops(&mut fx, contact, Pt::new(50.0, 0.0), ["13", "14"], true);
    if let Some(Op::LinkRepresentation { rep, .. }) = ops.last_mut() {
        let p = rep.connection_points.remove(&t1).unwrap();
        rep.connection_points.insert(foreign, p);
    }
    assert!(fx.l.commit(Author::User, "foreign", ops).is_err());
}

#[test]
fn the_tag_index_finds_one_tag_however_it_is_written() {
    let mut fx = fixture();
    for written in ["-K01", "k1", "-K001", "K01"] {
        assert_eq!(fx.l.document().components_tagged(written), vec![fx.k1], "{written}");
    }
    assert!(fx.l.document().components_tagged("-K10").is_empty());
    let rename = |before: &str, after: &str| Op::SetProperty {
        object: ObjectRef::Component(fx.k1),
        key: "tag".into(),
        before: Some(before.into()),
        after: Some(after.into()),
    };
    fx.l.commit(Author::User, "Set tag", vec![rename("-K01", "-K05")]).unwrap();
    let doc = fx.l.document();
    assert!(doc.components_tagged("-K1").is_empty());
    assert_eq!(doc.components_tagged("K5"), vec![fx.k1]);
    doc.check_index().unwrap();
    fx.l.undo(Author::User).unwrap();
    assert_eq!(fx.l.document().components_tagged("-K01"), vec![fx.k1]);
    fx.l.document().check_index().unwrap();
    // Deleting the component takes its tag out of the index.
    let c = fx.l.document().components.get(&fx.k1).cloned().unwrap();
    fx.l.commit(Author::User, "Delete", vec![Op::DeleteComponent { id: fx.k1, component: c }]).unwrap();
    assert!(fx.l.document().components_tagged("-K01").is_empty());
    fx.l.document().check_index().unwrap();
}
