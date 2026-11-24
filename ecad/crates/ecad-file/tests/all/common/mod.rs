//! A deterministic project history exercising every kind of record value: awkward numbers,
//! non-ASCII property keys, embedded definitions (two instances of one, a second version), one
//! device of every device type with each field set, conductors, cables, settings, a named version,
//! undo and redo.

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
    pub c1: DeviceId,
    pub c2: DeviceId,
    pub c3: DeviceId,
    pub t: Vec<ConnectionPointId>,
    pub time: u64,
}

pub fn style(layer: LayerId) -> Style {
    Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 }
}

pub fn coil(graphics_scale: f64) -> Definition {
    Definition {
        bridges: Vec::new(),
        name: "Coil".into(),
        origin: None,
        graphics: vec![
            Primitive::Polyline { vertices: vec![Pt::new(-5.0, -10.0), Pt::new(5.0, -10.0), Pt::new(5.0, 10.0), Pt::new(-5.0, 10.0)], closed: true },
            Primitive::Circle { center: Pt::new(0.0, 0.0), radius: graphics_scale },
        ],
        connection_points: vec![
            ConnectionPointDef { key: "A1".parse().unwrap(), label: Some("A1".into()), at: Pt::new(0.0, 10.0), accepts: Default::default() },
            ConnectionPointDef { key: "A2".parse().unwrap(), label: Some("A2".into()), at: Pt::new(0.0, -10.0), accepts: Default::default() },
        ],
        ..Default::default()
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
    pub fn place(&mut self, def: DefinitionId, at: Pt, rotation: f64) -> (DeviceId, [ConnectionPointId; 2]) {
        let (c, t1, t2, item, rep): (DeviceId, ConnectionPointId, ConnectionPointId, ItemId, RepresentationId) =
            (self.ids.fresh(), self.ids.fresh(), self.ids.fresh(), self.ids.fresh(), self.ids.fresh());
        let ops = vec![
            Op::CreateDevice { id: c, device: Device { definition: Some(def), ..Device::default() } },
            Op::AddConnectionPoint {
                id: t1,
                connection_point: ConnectionPoint {
                    owner: c.into(),
                    label: Some("A1".into()),
                    key: Some("A1".parse().unwrap()),
                    accepts: Default::default(),
                },
            },
            Op::AddConnectionPoint {
                id: t2,
                connection_point: ConnectionPoint {
                    owner: c.into(),
                    label: Some("A2".into()),
                    key: Some("A2".parse().unwrap()),
                    accepts: Default::default(),
                },
            },
            Op::AddItem {
                id: item,
                item: Item {
                    page: self.page,
                    style: style(self.layer),
                    kind: ItemKind::Symbol(SymbolInstance {
                        definition: def,
                        placement: Placement { origin: at, rotation: ecad_model::geometry::units::Radians(rotation), mirror: false, scale: 1.0 },
                    }),
                },
            },
            Op::LinkRepresentation {
                id: rep,
                rep: Representation {
                    subject: Subject::Device(c),
                    page: self.page,
                    form: Form::Symbol,
                    role: ecad_model::Role::Part(ecad_model::PartRole::Other),
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
    let (page, layer, d1, d2): (PageId, LayerId, DefinitionId, DefinitionId) = (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
    let mut f = Fixture {
        ledger: Ledger::new(SEED),
        ids,
        page,
        layer,
        d1,
        d2,
        c1: DeviceId(0),
        c2: DeviceId(0),
        c3: DeviceId(0),
        t: Vec::new(),
        time: 1_760_000_000_000,
    };
    f.commit(
        "New project",
        vec![
            Op::AddPage { id: page, page: Page::titled("1").unwrap(), index: 0 },
            Op::AddLayer { id: layer, layer: Layer { name: "0".into() } },
            Op::SetProjectSettings {
                before: ProjectSettings::default(),
                after: ProjectSettings { length_unit: LengthUnit::Millimetre, wiring: supplied(), ..ProjectSettings::default() },
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
                    rotation: ecad_model::geometry::units::Radians(std::f64::consts::FRAC_PI_2),
                    value: "K1 \"quoted\" \\ tab\t ünï 😀".into(),
                    halign: Default::default(),
                    valign: Default::default(),
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
    // A route between the two points (design 072 F31): the rewrite keeps the wire made with it.
    let (w, net, conductor, route, item, rep): (WireId, NetId, ConductorId, RouteId, ItemId, RepresentationId) =
        (f.ids.fresh(), f.ids.fresh(), f.ids.fresh(), f.ids.fresh(), f.ids.fresh(), f.ids.fresh());
    let points = vec![Pt::new(90.0, 0.0), Pt::new(120.0, 0.0), Pt::new(150.0, 10.0)];
    f.commit(
        "WIRE",
        vec![
            Op::AddRoute { id: route, route: Route::new(page, points.clone(), [RouteEnd::Point(t1[0]), RouteEnd::Point(t2[0])]) },
            Op::AddItem {
                id: item,
                item: Item { page, style: style(layer), kind: ItemKind::Primitive(Primitive::Polyline { vertices: points, closed: false }) },
            },
            Op::LinkRepresentation {
                id: rep,
                rep: Representation {
                    subject: Subject::Route(route),
                    page,
                    form: Form::Drawn,
                    role: ecad_model::Role::Route,
                    items: vec![item],
                    connection_points: Default::default(),
                },
            },
            Op::CreateWire { id: w, wire: Wire::new(net, conductor, t1[0], t2[0]) },
        ],
    );
    let cable: CableId = f.ids.fresh();
    f.commit(
        "Cable",
        vec![Op::CreateCable { id: cable, cable: Cable { cores: vec![Core::on_wire(w, ecad_model::CoreLabel::new("1").ok())], ..Cable::default() } }],
    );
    let ops = device_types(&mut f);
    f.commit("Device types", ops);
    // Keys whose UTF-8 and UTF-16 orders differ (U+FF5E sorts before U+1F600 in UTF-8 only).
    for (key, value) in [("note", "-K1"), ("\u{FF5E}wave", "w"), ("\u{1F600}smile", "s"), ("Bezeichnung-ä", "ä")] {
        f.commit("Set property", vec![Op::SetProperty { object: ObjectRef::Device(c1), key: key.into(), before: None, after: Some(value.into()) }]);
    }
    let at = f.ledger.head();
    f.commit("Name version", vec![Op::NameVersion { name: ecad_model::PointName::version("review-1").unwrap(), at, description: None }]);
    f.time += 1000;
    f.ledger.undo_at(Author::User, Some(f.time)).unwrap();
    f.time += 1000;
    f.ledger.redo_at(Author::Agent("assistant".into()), Some(f.time)).unwrap();
    f
}

/// One device of every device type (and of every protection and supply kind), each field set.
/// A device's type is the model's own `DeviceType`, stored as it is (no mirror in the records):
/// these literals list every field with no `..`, so a field added to a device type does not
/// compile until it is named here, and the pinned head hash then shows the format changed.
fn device_types(f: &mut Fixture) -> Vec<Op> {
    use ecad_model::geometry::units::{Amps, Current, KiloAmps, MilliAmps, Percent, RatedVoltage, VoltAmps, Volts, Watts};
    use ecad_model::naming::text::Text;
    use ecad_model::objects::devices::*;
    use ecad_model::objects::plc::*;
    let v = |volts: f64, current: Current| Some(RatedVoltage { volts: Volts(volts), current });
    let contacts = || {
        ContactSet::new(vec![
            ContactEntry { poles: ContactPoles::Count(std::num::NonZeroU8::new(3).unwrap()), kind: PoleKind::No, power: true },
            ContactEntry {
                poles: ContactPoles::Labels([ContactLabel::new("21").unwrap(), ContactLabel::new("22").unwrap()]),
                kind: PoleKind::Nc,
                power: false,
            },
        ])
    };
    let fuse =
        || Fuse { rated_current: Some(Amps(10.0)), class: Some(FuseClass::Gg), size: None, breaking: Some(KiloAmps(50.0)), rated_voltage: None };
    let rack: DeviceId = f.ids.fresh();
    let types = vec![
        (
            rack,
            DeviceType::PlcRack(PlcRack {
                network: Some(Text::new("plant").unwrap()),
                address: Some(NetAddress::Ip(core::net::Ipv4Addr::new(192, 168, 0, 10))),
                head: RackHead::Coupler { protocol: Protocol::Profinet },
                address_style: Some(AddressStyle::ByteBit),
                supply: None,
                backplane_budget: None,
                power: None,
            }),
        ),
        (
            f.ids.fresh(),
            DeviceType::PlcModule(PlcModule {
                rack: Some(rack),
                slot: Some(2),
                function: ModuleFunction::Drive(Drive { kind: DriveKind::Servo, axes: 2 }),
                attributes: ModuleAttributes { safety_rated: true, intrinsically_safe: true, calibration_certificate: true },
                address: AddressPin { start: Some(4), size: Some(8) },
                supply: v(24.0, Current::Dc),
                backplane_current: None,
                power_loss: None,
            }),
        ),
        (
            f.ids.fresh(),
            DeviceType::PowerSupply(PowerSupply::Psu(Psu {
                input: v(230.0, Current::Ac),
                output: v(24.0, Current::Dc),
                output_current: Some(Amps(5.0)),
                output_power: Some(Watts(120.0)),
                input_current: None,
                efficiency: None,
            })),
        ),
        (
            f.ids.fresh(),
            DeviceType::PowerSupply(PowerSupply::Transformer(Transformer {
                input: v(400.0, Current::Ac),
                output: v(230.0, Current::Ac),
                rated_power: Some(VoltAmps(630.0)),
                rated_frequency: None,
                impedance_voltage: None,
            })),
        ),
        (f.ids.fresh(), DeviceType::Protection(Protection::Fuse(fuse()))),
        (f.ids.fresh(), DeviceType::Protection(Protection::FuseTerminalBlock(fuse()))),
        (
            f.ids.fresh(),
            DeviceType::Protection(Protection::Mcb(Breaker {
                rated_current: Some(Amps(16.0)),
                curve: Some(TripCurve::C),
                breaking: Some(KiloAmps(6.0)),
                poles: Some(Poles::Three),
                rated_voltage: None,
                contacts: contacts(),
            })),
        ),
        (
            f.ids.fresh(),
            DeviceType::Protection(Protection::Rcd(Rcd {
                rated_current: Some(Amps(40.0)),
                residual: Some(MilliAmps(30.0)),
                residual_type: None,
                conditional_short_circuit: None,
                poles: Some(Poles::Four),
                rated_voltage: None,
                contacts: contacts(),
            })),
        ),
        (
            f.ids.fresh(),
            DeviceType::Protection(Protection::Rcbo(Rcbo {
                rated_current: Some(Amps(20.0)),
                curve: Some(TripCurve::B),
                residual: Some(MilliAmps(30.0)),
                residual_type: None,
                breaking: Some(KiloAmps(10.0)),
                poles: Some(Poles::OneN),
                rated_voltage: None,
                contacts: contacts(),
            })),
        ),
        (
            f.ids.fresh(),
            DeviceType::Protection(Protection::FuseBank(FuseBank { ways: None, outputs: [(1, fuse()), (2, fuse())].into_iter().collect() })),
        ),
        (
            f.ids.fresh(),
            DeviceType::Switching(Switching {
                operator: Operator::Coil(Control { voltage: v(24.0, Current::Dc), power: Some(Watts(3.5)) }),
                ac3_current: Some(Amps(12.0)),
                ac3_power: Some(Watts(5500.0)),
                operational_voltage: None,
                thermal_current: None,
                insulation_voltage: None,
                contacts: contacts(),
                contact_table: Some(ContactTable::At { dx: 12.5, dy: -4.0 }),
            }),
        ),
        (
            f.ids.fresh(),
            DeviceType::Sensor(Sensor {
                sensing: Sensing::Proximity,
                output: Output::Pnp,
                supply: v(24.0, Current::Dc),
                contacts: contacts(),
                output_current: None,
                no_load_current: None,
                ip_rating: None,
            }),
        ),
        (
            f.ids.fresh(),
            DeviceType::Motor(Motor {
                phases: Phases::Three,
                rated_power: Some(Watts(7500.0)),
                rated_voltage: Some(ecad_model::geometry::units::Volts(400.0)),
                rated_current: Some(Amps(15.2)),
                rated_frequency: Some(ecad_model::geometry::units::Hertz(50.0)),
                rated_speed: None,
                power_factor: Some(0.85),
                efficiency: Some(Percent(91.0)),
                efficiency_class: None,
                connection: None,
                duty: None,
                insulation_class: None,
                ip_rating: None,
                starting: Some(Starting::StarDelta),
            }),
        ),
        (f.ids.fresh(), DeviceType::Load(Load { rated_power: Some(Watts(2000.0)), rated_voltage: None, rated_current: None, phases: None })),
        (
            f.ids.fresh(),
            DeviceType::Indicator(Indicator {
                signal: SignalType::Flashing,
                colour: Some(IndicatorColour::Green),
                rated_voltage: v(24.0, Current::Dc),
                power: Some(Watts(1.2)),
                rated_current: None,
                ip_rating: None,
            }),
        ),
        (f.ids.fresh(), DeviceType::TerminalStrip(TerminalStrip { fuses: [(PositionLabel::new("3").unwrap(), fuse())].into_iter().collect() })),
        (
            f.ids.fresh(),
            DeviceType::Connector(Box::new(Connector {
                family: ConnectorFamily::Dsub,
                pins: Some(std::num::NonZeroU8::new(9).unwrap()),
                pin_names: vec![PinName::new("TX").unwrap(), PinName::new("RX").unwrap()],
                socket: HalfPart {
                    description: Some(Description::new("socket").unwrap()),
                    manufacturer: Some(PartText::new("maker").unwrap()),
                    part_number: Some(PartText::new("S-9").unwrap()),
                },
                plug: HalfPart {
                    description: Some(Description::new("plug").unwrap()),
                    manufacturer: Some(PartText::new("maker").unwrap()),
                    part_number: Some(PartText::new("P-9").unwrap()),
                },
                mating: true,
                ignored: [ConnectorCheck::Size].into_iter().collect(),
                rated_current: None,
                rated_voltage: None,
                ip_rating: None,
                cross_section_max: None,
            })),
        ),
        (f.ids.fresh(), DeviceType::General),
    ];
    types.into_iter().map(|(id, device_type)| Op::CreateDevice { id, device: Device { device_type, ..Device::default() } }).collect()
}

/// Write `ledger`'s whole history as a new project file in memory and open it again from its
/// bytes (the real format, no disk). The ledger is marked persisted.
pub fn reopened(ledger: &mut Ledger) -> ecad_file::Opened {
    ecad_file::Project::open(ecad_file::Store::from_bytes(written(ledger)).unwrap(), SEED, WRITER).unwrap()
}

/// The bytes of a new project file in memory holding `ledger`'s whole history.
pub fn written(ledger: &mut Ledger) -> Vec<u8> {
    let backend = ecad_store::MemoryBackend::new();
    let store = ecad_file::Store::with_backend(backend.clone()).unwrap();
    let (p, _) = ecad_file::Project::create(store, PROJECT, ledger, Some(1_760_000_000_000), WRITER, 16).unwrap();
    drop(p);
    ledger.mark_persisted(ledger.head()).unwrap();
    backend.bytes()
}

/// The default wiring with a supply on L1 (400/230 V 3~ 50 Hz) and on 24V (DC).
fn supplied() -> ecad_model::connectivity::wiring::WiringSettings {
    use ecad_model::connectivity::supply::Supply;
    use ecad_model::geometry::units::{Hertz, Volts};
    let mut w = ecad_model::connectivity::wiring::WiringSettings::default();
    for p in &mut w.potentials {
        p.supply = match p.name.as_str() {
            "L1" => Some(Supply::Ac3 { line_to_line: Volts(400.0), line_to_neutral: Some(Volts(230.0)), frequency: Some(Hertz(50.0)) }),
            "24V" => Some(Supply::Dc { nominal: Volts(24.0) }),
            _ => None,
        };
    }
    w
}
