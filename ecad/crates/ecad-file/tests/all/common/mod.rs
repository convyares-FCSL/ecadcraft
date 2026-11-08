//! A deterministic project history exercising every kind of record value: awkward numbers,
//! non-ASCII property keys, embedded definitions (two instances of one, a second version),
//! conductors, cables, settings, a named version, undo and redo.

#![allow(dead_code)]

use ecad_model::*;

pub const SEED: u64 = 0x5EED_0001;
pub const PROJECT: ProjectId = ProjectId(0x0123_4567_89ab_cdef_0011_2233_4455_6677);
pub const LIBRARY: LibraryId = LibraryId(0x00aa_00bb_00cc_00dd_00ee_00ff_0011_0022);
pub const WRITER: &str = "ecad-file tests";

pub struct Fixture {
    pub ledger: Ledger,
    pub ids: IdAllocator,
    pub page: PageId,
    pub layer: LayerId,
    pub d1: DefinitionId,
    pub d2: DefinitionId,
    pub c1: ComponentId,
    pub c2: ComponentId,
    pub c3: ComponentId,
    pub t: Vec<TerminalId>,
    pub time: u64,
}

pub fn style(layer: LayerId) -> Style {
    Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 }
}

pub fn coil(graphics_scale: f64) -> Definition {
    Definition {
        name: "Coil".into(),
        origin: None,
        graphics: vec![
            Primitive::Polyline {
                vertices: vec![Pt::new(-5.0, -10.0), Pt::new(5.0, -10.0), Pt::new(5.0, 10.0), Pt::new(-5.0, 10.0)],
                closed: true,
            },
            Primitive::Circle { center: Pt::new(0.0, 0.0), radius: graphics_scale },
        ],
        terminals: vec![
            TerminalDef { key: "A1".into(), label: Some("A1".into()), at: Pt::new(0.0, 10.0) },
            TerminalDef { key: "A2".into(), label: Some("A2".into()), at: Pt::new(0.0, -10.0) },
        ],
    }
}

/// A definition as embedded from library `LIBRARY`, version `seq`.
pub fn from_library(mut d: Definition, def: DefinitionId, seq: u64) -> Definition {
    let hash = ecad_file::definition_hash(&d).unwrap();
    d.origin = Some(Origin { library: LIBRARY, definition: def, library_seq: seq, content_hash: hash });
    d
}

impl Fixture {
    pub fn commit(&mut self, label: &str, ops: Vec<Op>) {
        self.time += 1000;
        self.ledger.commit_at(Author::User, label, ops, Some(self.time)).unwrap();
    }

    /// Place an instance of `def` as component with two terminals; returns (component, terminals).
    pub fn place(&mut self, def: DefinitionId, at: Pt, rotation: f64) -> (ComponentId, [TerminalId; 2]) {
        let (c, t1, t2, item, rep): (ComponentId, TerminalId, TerminalId, ItemId, RepresentationId) =
            (self.ids.fresh(), self.ids.fresh(), self.ids.fresh(), self.ids.fresh(), self.ids.fresh());
        let ops = vec![
            Op::CreateComponent {
                id: c,
                component: Component { properties: Default::default(), definition: Some(def) },
            },
            Op::AddTerminal {
                id: t1,
                terminal: Terminal { component: c, label: Some("A1".into()), key: Some("A1".into()) },
            },
            Op::AddTerminal {
                id: t2,
                terminal: Terminal { component: c, label: Some("A2".into()), key: Some("A2".into()) },
            },
            Op::AddItem {
                id: item,
                item: Item {
                    page: self.page,
                    style: style(self.layer),
                    kind: ItemKind::Symbol(SymbolInstance {
                        definition: def,
                        placement: Placement { origin: at, rotation, mirror: false, scale: 1.0 },
                    }),
                },
            },
            Op::LinkRepresentation {
                id: rep,
                rep: Representation {
                    subject: Subject::Component(c),
                    page: self.page,
                    form: Form::Symbol,
                    role: "main".into(),
                    items: vec![item],
                    connection_points: Default::default(),
                },
            },
        ];
        self.commit("Place symbol", ops);
        (c, [t1, t2])
    }
}

pub fn fixture() -> Fixture {
    let mut ids = IdAllocator::seeded(SEED ^ 0xA11C);
    let (page, layer, d1, d2): (PageId, LayerId, DefinitionId, DefinitionId) =
        (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
    let mut f = Fixture {
        ledger: Ledger::new(SEED),
        ids,
        page,
        layer,
        d1,
        d2,
        c1: ComponentId(0),
        c2: ComponentId(0),
        c3: ComponentId(0),
        t: Vec::new(),
        time: 1_760_000_000_000,
    };
    f.commit(
        "New project",
        vec![
            Op::AddPage { id: page, page: Page { title: "1".into() }, index: 0 },
            Op::AddLayer { id: layer, layer: Layer { name: "0".into() } },
            Op::SetProjectSettings {
                before: ProjectSettings::default(),
                after: ProjectSettings { length_unit: LengthUnit::Millimetre },
            },
        ],
    );
    // Awkward numbers: 0.1, -0.0, 1e-7, a large value, a negative.
    let line: ItemId = f.ids.fresh();
    f.commit(
        "LINE",
        vec![Op::AddItem {
            id: line,
            item: Item {
                page,
                style: Style { color: Color::Rgb(0xFF8800), lineweight: 25, ..style(layer) },
                kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(0.1, -0.0), b: Pt::new(1e-7, 123456.789) }),
            },
        }],
    );
    let text: ItemId = f.ids.fresh();
    f.commit(
        "TEXT",
        vec![Op::AddItem {
            id: text,
            item: Item {
                page,
                style: Style { color: Color::Index(3), ..style(layer) },
                kind: ItemKind::Primitive(Primitive::Text {
                    at: Pt::new(-2.5, 1e21),
                    height: 2.5,
                    rotation: std::f64::consts::FRAC_PI_2,
                    value: "K1 \"quoted\" \\ tab\t ünï 😀".into(),
                }),
            },
        }],
    );
    let embedded = from_library(coil(3.0), DefinitionId(0x77), 7);
    f.commit("Embed", vec![Op::EmbedDefinition { id: d1, definition: embedded }]);
    let (c1, t1) = f.place(d1, Pt::new(100.0, 0.0), std::f64::consts::FRAC_PI_2);
    let (c2, t2) = f.place(d1, Pt::new(150.0, 0.0), 0.0);
    let newer = from_library(coil(4.0), DefinitionId(0x77), 9);
    f.commit("Embed newer", vec![Op::EmbedDefinition { id: d2, definition: newer }]);
    let (c3, t3) = f.place(d2, Pt::new(200.0, 0.0), std::f64::consts::PI);
    f.c1 = c1;
    f.c2 = c2;
    f.c3 = c3;
    f.t = vec![t1[0], t1[1], t2[0], t2[1], t3[0], t3[1]];
    let (w, route, rep): (ConductorId, ItemId, RepresentationId) = (f.ids.fresh(), f.ids.fresh(), f.ids.fresh());
    f.commit(
        "WIRE",
        vec![
            Op::CreateConductor { id: w, conductor: Conductor::default() },
            Op::AddItem {
                id: route,
                item: Item {
                    page,
                    style: style(layer),
                    kind: ItemKind::Primitive(Primitive::Polyline {
                        vertices: vec![Pt::new(90.0, 0.0), Pt::new(120.0, 0.0), Pt::new(150.0, 10.0)],
                        closed: false,
                    }),
                },
            },
            Op::LinkRepresentation {
                id: rep,
                rep: Representation {
                    subject: Subject::Conductor(w),
                    page,
                    form: Form::Drawn,
                    role: "route".into(),
                    items: vec![route],
                    connection_points: Default::default(),
                },
            },
            Op::AttachEnd { conductor: w, end: End::A, before: None, after: Some(t1[0]) },
            Op::AttachEnd { conductor: w, end: End::B, before: None, after: Some(t2[0]) },
        ],
    );
    let cable: CableId = f.ids.fresh();
    f.commit(
        "Cable",
        vec![
            Op::CreateCable { id: cable, cable: Cable::default() },
            Op::AddCableMember { cable, index: 0, member: CableMember { conductor: w, core: Some("1".into()) } },
        ],
    );
    // Keys whose UTF-8 and UTF-16 orders differ (U+FF5E sorts before U+1F600 in UTF-8 only).
    for (key, value) in [("tag", "-K1"), ("\u{FF5E}wave", "w"), ("\u{1F600}smile", "s"), ("Bezeichnung-ä", "ä")] {
        f.commit(
            "Set property",
            vec![Op::SetProperty {
                object: ObjectRef::Component(c1),
                key: key.into(),
                before: None,
                after: Some(value.into()),
            }],
        );
    }
    let at = f.ledger.head();
    f.commit("Name version", vec![Op::NameVersion { name: "review-1".into(), at }]);
    f.time += 1000;
    f.ledger.undo_at(Author::User, Some(f.time)).unwrap();
    f.time += 1000;
    f.ledger.redo_at(Author::Agent("assistant".into()), Some(f.time)).unwrap();
    f
}
