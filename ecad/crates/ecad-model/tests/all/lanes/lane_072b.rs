//! Lane 072 B (design 072 §2, §7, §14): the device and its type.

use ecad_model::device::{DeviceGroup, DeviceType};
use ecad_model::{Author, Definition, Device, DeviceId, IdAllocator, Ledger, ModelError, Op, Word};

/// A2: every group's word, and every type with its data, round-trips; the words are the stored
/// ones.
#[test]
fn device_types_round_trip_as_their_words() -> Result<(), serde_json::Error> {
    let words: Vec<&str> = DeviceGroup::ALL.iter().map(|g| g.word()).collect();
    assert_eq!(
        words,
        [
            "power_supply",
            "protection",
            "switching",
            "sensor",
            "motor",
            "load",
            "indicator",
            "terminal_strip",
            "connector",
            "plc_rack",
            "plc_module",
            "general"
        ]
    );
    for g in DeviceGroup::ALL {
        let t = DeviceType::of_group(*g);
        assert_eq!(t.group(), *g);
        let text = serde_json::to_string(&t)?;
        assert!(text.contains(g.word()), "{g:?} is written as its word: {text}");
        assert_eq!(serde_json::from_str::<DeviceType>(&text)?, t);
    }
    Ok(())
}

/// A4 (model side): an unknown group, an unknown field in a group, or a device without a type
/// does not read.
#[test]
fn an_unknown_group_or_field_does_not_read() {
    for bad in [r#"{"heater":{}}"#, r#""relay""#, r#"{"protection":{"amps":10}}"#, r#"{"general":{}}"#] {
        assert!(serde_json::from_str::<DeviceType>(bad).is_err(), "{bad} must not read");
    }
    assert!(serde_json::from_str::<Device>(r#"{"properties":{},"definition":null}"#).is_err(), "a device states its type");
    assert!(
        serde_json::from_str::<Device>(r#"{"properties":{},"definition":null,"device_type":"general","kind":"x"}"#).is_err(),
        "a device has no unknown fields"
    );
}

/// F15: a symbol's type is left out of its record when it is `general`, and written otherwise.
#[test]
fn a_symbol_writes_its_type_unless_general() -> Result<(), serde_json::Error> {
    let general = Definition { name: "x".into(), ..Definition::default() };
    assert!(!serde_json::to_string(&general)?.contains("device_type"));
    let motor = Definition { device_type: DeviceType::of_group(DeviceGroup::Motor), ..general };
    let text = serde_json::to_string(&motor)?;
    assert!(text.contains(r#""device_type":{"motor":{"phases":"three"}}"#), "{text}");
    assert_eq!(serde_json::from_str::<Definition>(&text)?, motor);
    Ok(())
}

/// A8: a device placed from a symbol takes the type the symbol declares.
#[test]
fn a_device_takes_its_symbol_s_type() {
    let def = Definition { name: "psu".into(), device_type: DeviceType::of_group(DeviceGroup::PowerSupply), ..Definition::default() };
    let d = Device::of_definition(ecad_model::DefinitionId(7), &def, Default::default());
    assert_eq!(d.device_type.group(), DeviceGroup::PowerSupply);
    assert_eq!(d.definition, Some(ecad_model::DefinitionId(7)));
}

/// A9: `ModifyDevice` changes the whole device, undoes exactly, and refuses a `before` that is not
/// the device.
#[test]
fn modify_device_changes_the_type_and_undoes() {
    let mut ids = IdAllocator::seeded(9);
    let mut l = Ledger::new(1);
    let c: DeviceId = ids.fresh();
    let before = Device { device_type: DeviceType::General, ..Device::default() };
    l.commit(Author::User, "add", vec![Op::CreateDevice { id: c, device: before.clone() }]).unwrap();
    let after = Device { device_type: DeviceType::of_group(DeviceGroup::Protection), ..before.clone() };
    let op = Op::ModifyDevice { id: c, before: before.clone(), after: after.clone() };
    assert_eq!(op.name(), "ModifyDevice");
    assert_eq!(op.inverse(), Op::ModifyDevice { id: c, before: after.clone(), after: before.clone() });
    l.commit(Author::User, "retype", vec![op]).unwrap();
    assert_eq!(l.document().devices[&c], after);
    l.undo(Author::User).unwrap();
    assert_eq!(l.document().devices[&c], before);
    let wrong = Op::ModifyDevice { id: c, before: after.clone(), after: before };
    assert!(matches!(l.commit(Author::User, "stale", vec![wrong]), Err(ModelError::Precondition(_))));
}

/// Design 072 §2.1 (step 2a): the tag and its fixed flag are fields; a device's bag never holds
/// them, `SetProperty` refuses them on a device, and a file's tag is validated text.
#[test]
fn the_tag_is_a_field_not_a_property() {
    let mut ids = IdAllocator::seeded(11);
    let mut l = Ledger::new(11);
    let c: DeviceId = ids.fresh();
    let mut bagged = Device::default();
    bagged.properties.insert("tag".into(), "-K01".into());
    assert!(l.commit(Author::User, "bag", vec![Op::CreateDevice { id: c, device: bagged }]).is_err(), "a bag holding tag is refused");
    l.commit(Author::User, "add", vec![Op::CreateDevice { id: c, device: Device::default().with_tag(Some("-K01")) }]).unwrap();
    assert_eq!(l.document().components_tagged("K1"), vec![c], "the field is indexed");
    let set = ecad_model::ObjectRef::Device(c);
    for key in ["tag", "tag_fixed"] {
        let op = Op::SetProperty { object: set, key: key.into(), before: None, after: Some("yes".into()) };
        assert!(matches!(l.commit(Author::User, "set", vec![op]), Err(ModelError::Precondition(_))), "{key}");
    }
    let hidden = format!(r#"{{"tag":"-K{}01","tag_fixed":false,"properties":{{}},"definition":null,"device_type":"general"}}"#, '\u{202e}');
    assert!(serde_json::from_str::<Device>(&hidden).is_err(), "a bidi character in a tag does not read");
    assert_eq!(Device::default().with_tag(Some("-K02")).value("tag").as_deref(), Some("-K02"));
}

/// Design 072 §2.1 (step 2b): description, part number, maker, article number and type text are
/// fields; a device's bag never holds their keys, `SetProperty` refuses them, the values are
/// validated text, and `setprop`'s keys route to them.
#[test]
fn the_text_fields_are_fields_not_properties() {
    use ecad_model::TextField;
    let mut ids = IdAllocator::seeded(12);
    let mut l = Ledger::new(12);
    let c: DeviceId = ids.fresh();
    for f in TextField::ALL {
        let mut bagged = Device::default();
        bagged.properties.insert(f.key().into(), "x".into());
        assert!(l.commit(Author::User, "bag", vec![Op::CreateDevice { id: c, device: bagged }]).is_err(), "{}: a bag holding it is refused", f.key());
    }
    l.commit(Author::User, "add", vec![Op::CreateDevice { id: c, device: Device::default() }]).unwrap();
    for f in TextField::ALL {
        let op = Op::SetProperty { object: ecad_model::ObjectRef::Device(c), key: f.key().into(), before: None, after: Some("x".into()) };
        assert!(matches!(l.commit(Author::User, "set", vec![op]), Err(ModelError::Precondition(_))), "{}", f.key());
    }
    let mut d = Device::default();
    for (f, v) in TextField::ALL.into_iter().zip(["Main pump", "3RT2016", "Siemens", "A-0400", "UK 2,5"]) {
        d.set_value(f.key(), Some(v)).unwrap();
        assert_eq!(d.text(f), Some(v));
        assert_eq!(d.get(f.key()), Some(v));
        assert_eq!(d.value(f.key()).as_deref(), Some(v));
    }
    assert!(d.properties.is_empty(), "set_value routes the keys to the fields");
    assert_eq!(d.manufacturer.as_ref().map(|t| t.as_str()), Some("Siemens"), "`maker` is the manufacturer");
    assert_eq!(d.type_text.as_ref().map(|t| t.as_str()), Some("UK 2,5"), "`type` is the type text");
    // Too long, or a hidden character: refused; blank clears.
    assert!(d.with_text(TextField::PartNumber, Some(&"9".repeat(65))).is_err());
    assert!(d.with_text(TextField::Description, Some(&"d".repeat(256))).is_ok());
    assert!(d.with_text(TextField::Description, Some(&"d".repeat(257))).is_err());
    assert!(d.with_text(TextField::Manufacturer, Some("Sie\u{200b}mens")).is_err());
    assert_eq!(d.with_text(TextField::Description, Some("  ")).unwrap().description, None);
    let json = serde_json::to_value(&d).unwrap();
    assert_eq!(json["manufacturer"], "Siemens");
    let hidden = r#"{"tag":null,"tag_fixed":false,"description":"Pu\u202emp","properties":{},"definition":null,"device_type":"general"}"#;
    assert!(serde_json::from_str::<Device>(hidden).is_err(), "a bidi character in a description does not read");
}

/// Design 072 §6.3 (step 2c): a device's location and function are list ids with a `fixed` flag;
/// a device's bag never holds `location` or `function`, `SetProperty` refuses them and their flags,
/// and `setprop`'s keys read and set the fields.
#[test]
fn location_and_function_are_fields_with_fixed() {
    use ecad_model::{FunctionId, LocationId};
    let mut ids = IdAllocator::seeded(13);
    let mut l = Ledger::new(13);
    let c: DeviceId = ids.fresh();
    for key in ["location", "function"] {
        let mut bagged = Device::default();
        bagged.properties.insert(key.into(), "0".repeat(32));
        assert!(l.commit(Author::User, "bag", vec![Op::CreateDevice { id: c, device: bagged }]).is_err(), "{key}: a bag holding it is refused");
    }
    l.commit(Author::User, "add", vec![Op::CreateDevice { id: c, device: Device::default() }]).unwrap();
    for key in ["location", "location_fixed", "function", "function_fixed"] {
        let op = Op::SetProperty { object: ecad_model::ObjectRef::Device(c), key: key.into(), before: None, after: Some("x".into()) };
        assert!(matches!(l.commit(Author::User, "set", vec![op]), Err(ModelError::Precondition(_))), "{key}");
    }
    // `set_value` reads an entry's id; a value set that way is typed, so fixed.
    let loc = LocationId(0xabc);
    let mut d = Device::default();
    d.set_value("location", Some(&loc.hex())).unwrap();
    d.set_value("function", Some(&FunctionId(0xdef).hex())).unwrap();
    assert_eq!((d.location, d.location_fixed, d.function, d.function_fixed), (Some(loc), true, Some(FunctionId(0xdef)), true));
    assert!(d.properties.is_empty());
    assert_eq!(d.value("location"), Some(loc.hex()));
    assert_eq!(d.value("location_fixed").as_deref(), Some("yes"));
    let unfixed = Device { location_fixed: false, ..d.clone() };
    assert_eq!(unfixed.value("location_fixed"), None);
    assert!(d.only_place_differs(&Device::default()) && !d.only_text_differs(&Device::default()));
    let json = serde_json::to_value(&d).unwrap();
    assert_eq!(json["location_fixed"], true);
    let read: Device = serde_json::from_value(json).unwrap();
    assert_eq!(read, d, "the fields round-trip");
}

/// Step 3, Connector (design 072 §2.2): a connector's family, pins, pin names, half parts and
/// ignored rows are its type's data, strict on read and checked by the invariant; the keys are
/// never in a device's bag and `setprop`'s keys read them.
#[test]
fn a_connector_s_data_is_its_type_s() {
    use ecad_model::device::{Connector, ConnectorCheck, ConnectorFamily, PinCount, PinName};
    let mut x = Connector::of_family(ConnectorFamily::Dsub);
    x.pins = PinCount::new(9);
    x.pin_names = ["L", "N", "PE"].map(|n| PinName::new(n).unwrap()).to_vec();
    x.plug.part_number = Some(ecad_model::device::PartText::new("PLG-9").unwrap());
    x.ignored.insert(ConnectorCheck::Family);
    let d = Device { device_type: DeviceType::Connector(Box::new(x.clone())), ..Device::default() };
    let json = serde_json::to_value(&d.device_type).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"connector": {"family": "dsub", "pins": 9, "pin_names": ["L", "N", "PE"], "plug": {"part_number": "PLG-9"}, "ignored": ["connector-family"]}})
    );
    assert_eq!(serde_json::from_value::<DeviceType>(json).unwrap(), d.device_type, "round-trips");
    for bad in [
        serde_json::json!({"connector": {"family": "dsub9"}}),
        serde_json::json!({"connector": {"family": "plain", "colour": "red"}}),
        serde_json::json!({"connector": {"family": "plain", "ignored": ["wire-colour"]}}),
        serde_json::json!({"connector": {"family": "plain", "pins": 0}}),
    ] {
        assert!(serde_json::from_value::<DeviceType>(bad.clone()).is_err(), "{bad}");
    }
    assert_eq!(d.value("pins").as_deref(), Some("9"));
    assert_eq!(d.value("pin_names").as_deref(), Some("L,N,PE"));
    assert_eq!(d.value("half.p.part_number").as_deref(), Some("PLG-9"));
    assert_eq!(d.value("half.s.part_number"), None);
    assert_eq!(d.value("ignore.connector-family").as_deref(), Some("yes"));
    // The invariant: a name twice is refused; the keys never sit in a bag.
    let mut ids = IdAllocator::seeded(14);
    let mut l = Ledger::new(14);
    let c: DeviceId = ids.fresh();
    let mut twice = x.clone();
    twice.pin_names.push(PinName::new("pe").unwrap());
    let bad = Device { device_type: DeviceType::Connector(Box::new(twice)), ..Device::default() };
    assert!(l.commit(Author::User, "twice", vec![Op::CreateDevice { id: c, device: bad }]).is_err());
    for key in ["pins", "pin_names", "half.s.maker", "ignore.connector-size"] {
        let mut bagged = Device::default();
        bagged.properties.insert(key.into(), "1".into());
        assert!(l.commit(Author::User, "bag", vec![Op::CreateDevice { id: c, device: bagged }]).is_err(), "{key}");
    }
    l.commit(Author::User, "add", vec![Op::CreateDevice { id: c, device: d }]).unwrap();
}

/// Step 4 (design 072 §2.2 row 9): whether a connector has mating halves is its stored `mating`,
/// written only when set; a one-piece connector leaves it out.
#[test]
fn a_connector_s_mating_is_stored() {
    use ecad_model::device::{Connector, ConnectorFamily};
    let one_piece = DeviceType::Connector(Box::new(Connector::of_family(ConnectorFamily::Rj45)));
    assert_eq!(serde_json::to_value(&one_piece).unwrap(), serde_json::json!({"connector": {"family": "rj45"}}));
    let mating = DeviceType::Connector(Box::new(Connector { mating: true, ..Connector::of_family(ConnectorFamily::M12) }));
    let json = serde_json::to_value(&mating).unwrap();
    assert_eq!(json, serde_json::json!({"connector": {"family": "m12", "mating": true}}));
    assert_eq!(serde_json::from_value::<DeviceType>(json).unwrap(), mating, "round-trips");
    assert!(serde_json::from_value::<DeviceType>(serde_json::json!({"connector": {"family": "m12", "mating": "yes"}})).is_err());
}

/// Step 3, Protection (design 072 §2.2, "Ratings"): a protective device's rating is typed fields
/// per kind, shown from them; the note is separate; a bad value or a rating in a bag is refused.
#[test]
fn a_protective_device_s_rating_is_typed() {
    use ecad_model::device::{Breaker, Fuse, FuseBank, Protection, RatingNote, TripCurve};
    use ecad_model::units::{Amps, KiloAmps};
    let mcb = Protection::Mcb(Breaker { rated_current: Some(Amps(16.0)), curve: Some(TripCurve::B), breaking: Some(KiloAmps(6.0)), poles: None });
    let d = Device { device_type: DeviceType::Protection(mcb.clone()), ..Device::default() };
    let json = serde_json::to_value(&d.device_type).unwrap();
    assert_eq!(json, serde_json::json!({"protection": {"mcb": {"rated_current": 16.0, "curve": "b", "breaking": 6.0}}}));
    assert_eq!(serde_json::from_value::<DeviceType>(json).unwrap(), d.device_type, "round-trips");
    assert_eq!(d.value("rating").as_deref(), Some("curve B, 16 A, 6 kA"));
    assert!(!d.has_no_rating());
    for bad in [
        serde_json::json!({"protection": {"mcb": {"curve": "x"}}}),
        serde_json::json!({"protection": {"fuse": {"curve": "b"}}}),
        serde_json::json!({"protection": {"breaker": {}}}),
        serde_json::json!({"protection": {"rcd": {"residual": "30"}}}),
    ] {
        assert!(serde_json::from_value::<DeviceType>(bad.clone()).is_err(), "{bad}");
    }
    // A note is shown when nothing is typed, and is not a rating.
    let noted = Device {
        device_type: DeviceType::Protection(Protection::Mcb(Breaker::default())),
        rating_note: Some(RatingNote::new("see datasheet").unwrap()),
        ..Device::default()
    };
    assert_eq!(noted.value("rating").as_deref(), Some("see datasheet"));
    assert!(noted.has_no_rating());
    // A general device has no rating fields: its note is all it has, and no warning.
    let general = Device { rating_note: Some(RatingNote::new("24 V").unwrap()), ..Device::default() };
    assert!(!general.has_no_rating());
    // A fuse bank's output: `rating.<n>`.
    let mut bank = FuseBank::default();
    bank.outputs.insert(3, Fuse { rated_current: Some(Amps(4.0)), ..Fuse::default() });
    let b = Device { device_type: DeviceType::Protection(Protection::FuseBank(bank)), ..Device::default() };
    assert_eq!(b.value("rating.3").as_deref(), Some("4 A"));
    assert_eq!(b.value("rating.2"), None);
    // The invariant: out of range, or a rating in a bag, is refused.
    let mut ids = IdAllocator::seeded(15);
    let mut l = Ledger::new(15);
    let c: DeviceId = ids.fresh();
    let wild = Protection::Fuse(Fuse { rated_current: Some(Amps(-1.0)), ..Fuse::default() });
    let bad = Device { device_type: DeviceType::Protection(wild), ..Device::default() };
    assert!(l.commit(Author::User, "wild", vec![Op::CreateDevice { id: c, device: bad }]).is_err());
    for key in ["rating", "rating.3"] {
        let mut bagged = Device { device_type: DeviceType::Protection(Protection::default()), ..Device::default() };
        bagged.properties.insert(key.into(), "4 A".into());
        assert!(l.commit(Author::User, "bag", vec![Op::CreateDevice { id: c, device: bagged }]).is_err(), "{key}");
    }
    l.commit(Author::User, "add", vec![Op::CreateDevice { id: c, device: d }]).unwrap();
}

/// Step 3, Switching (design 072 §2.2, "Ratings"): a switching device's operator, control rating
/// and AC-3 rating are typed fields, shown from them; only a coil or an SSR without them has no
/// rating; a bad value is refused.
#[test]
fn a_switching_device_s_rating_is_typed() {
    use ecad_model::device::{Control, Operator, OperatorKind, RatingNote, Switching};
    use ecad_model::units::{Amps, Current, RatedVoltage, Volts, Watts};
    let k = Switching {
        operator: Operator::Coil(Control { voltage: Some(RatedVoltage { volts: Volts(24.0), current: Current::Dc }), power: Some(Watts(3.0)) }),
        ac3_current: Some(Amps(9.0)),
        ac3_power: Some(Watts(4000.0)),
    };
    let d = Device { device_type: DeviceType::Switching(k), ..Device::default() };
    let json = serde_json::to_value(&d.device_type).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"switching": {"operator": {"coil": {"voltage": {"volts": 24.0, "current": "dc"}, "power": 3.0}}, "ac3_current": 9.0, "ac3_power": 4000.0}})
    );
    assert_eq!(serde_json::from_value::<DeviceType>(json).unwrap(), d.device_type, "round-trips");
    assert_eq!(d.value("rating").as_deref(), Some("coil 24 V DC, 3 W, AC-3 9 A, 4 kW"));
    assert!(!d.has_no_rating());
    for bad in [
        serde_json::json!({"switching": {}}),
        serde_json::json!({"switching": {"operator": "foot"}}),
        serde_json::json!({"switching": {"operator": "manual", "coil_voltage": 24}}),
        serde_json::json!({"switching": {"operator": {"coil": {"voltage": {"volts": 24.0, "current": "ac/dc"}}}}}),
    ] {
        assert!(serde_json::from_value::<DeviceType>(bad.clone()).is_err(), "{bad}");
    }
    // An unrated coil has no rating, also with a note; an unrated push button is not warned about.
    let coil = Device {
        device_type: DeviceType::Switching(OperatorKind::Coil.empty()),
        rating_note: Some(RatingNote::new("see datasheet").unwrap()),
        ..Device::default()
    };
    assert!(coil.has_no_rating());
    assert_eq!(coil.value("rating").as_deref(), Some("see datasheet"));
    assert!(Device { device_type: DeviceType::Switching(OperatorKind::SolidState.empty()), ..Device::default() }.has_no_rating());
    let button = Device { device_type: DeviceType::Switching(OperatorKind::Manual.empty()), ..Device::default() };
    assert!(!button.has_no_rating());
    assert_eq!(serde_json::to_value(&button.device_type).unwrap(), serde_json::json!({"switching": {"operator": "manual"}}));
    // The invariant: out of range is refused.
    let mut ids = IdAllocator::seeded(16);
    let mut l = Ledger::new(16);
    let c: DeviceId = ids.fresh();
    let wild = Switching { ac3_current: Some(Amps(f64::NAN)), ..Switching::default() };
    let bad = Device { device_type: DeviceType::Switching(wild), ..Device::default() };
    assert!(l.commit(Author::User, "wild", vec![Op::CreateDevice { id: c, device: bad }]).is_err());
    l.commit(Author::User, "add", vec![Op::CreateDevice { id: c, device: d }]).unwrap();
}

/// Step 3, Sensor (design 072 §2.2 row 4, "Ratings"): a sensor's sensing, output and supply are
/// typed fields, shown from them; only a powered (PNP or NPN) sensor without a supply has no
/// rating; a supply on a contact, or out of range, is refused.
#[test]
fn a_sensor_s_rating_is_typed() {
    use ecad_model::device::{Output, RatingNote, Sensing, Sensor};
    use ecad_model::units::{Current, RatedVoltage, Volts};
    let v24 = RatedVoltage { volts: Volts(24.0), current: Current::Dc };
    let b = Sensor { sensing: Sensing::Proximity, output: Output::Pnp, supply: Some(v24) };
    let d = Device { device_type: DeviceType::Sensor(b), ..Device::default() };
    let json = serde_json::to_value(&d.device_type).unwrap();
    assert_eq!(json, serde_json::json!({"sensor": {"sensing": "proximity", "output": "pnp", "supply": {"volts": 24.0, "current": "dc"}}}));
    assert_eq!(serde_json::from_value::<DeviceType>(json).unwrap(), d.device_type, "round-trips");
    assert_eq!(d.value("rating").as_deref(), Some("24 V DC"));
    assert!(!d.has_no_rating());
    for bad in [
        serde_json::json!({"sensor": {}}),
        serde_json::json!({"sensor": {"sensing": "smell", "output": "pnp"}}),
        serde_json::json!({"sensor": {"sensing": "float", "output": "relay"}}),
        serde_json::json!({"sensor": {"sensing": "float", "output": "contact", "range": "4-20"}}),
    ] {
        assert!(serde_json::from_value::<DeviceType>(bad.clone()).is_err(), "{bad}");
    }
    // An unrated PNP sensor has no rating, also with a note; an unrated contact is not warned about.
    let prox = Device {
        device_type: DeviceType::Sensor(Sensor::empty(Sensing::Proximity, Output::Npn)),
        rating_note: Some(RatingNote::new("see datasheet").unwrap()),
        ..Device::default()
    };
    assert!(prox.has_no_rating());
    assert_eq!(prox.value("rating").as_deref(), Some("see datasheet"));
    let float = Device { device_type: DeviceType::Sensor(Sensor::empty(Sensing::Float, Output::Contact)), ..Device::default() };
    assert!(!float.has_no_rating());
    assert!(
        !Device { device_type: DeviceType::Sensor(Sensor::empty(Sensing::Temperature, Output::Thermocouple)), ..Device::default() }.has_no_rating()
    );
    // The invariant: a supply on a contact, or out of range, is refused.
    let mut ids = IdAllocator::seeded(17);
    let mut l = Ledger::new(17);
    for wild in [
        Sensor { supply: Some(v24), ..Sensor::empty(Sensing::Float, Output::Contact) },
        Sensor { supply: Some(RatedVoltage { volts: Volts(f64::NAN), current: Current::Dc }), ..Sensor::empty(Sensing::Proximity, Output::Pnp) },
    ] {
        let c: DeviceId = ids.fresh();
        let bad = Device { device_type: DeviceType::Sensor(wild), ..Device::default() };
        assert!(l.commit(Author::User, "wild", vec![Op::CreateDevice { id: c, device: bad }]).is_err());
    }
    let c: DeviceId = ids.fresh();
    l.commit(Author::User, "add", vec![Op::CreateDevice { id: c, device: d }]).unwrap();
}

/// Step 3, TerminalStrip (design 072 §2.5): a strip's fuse position ratings are typed fields by
/// position label, shown from them (`rating.<p>`); an unknown field, a fuse with no value, a value
/// out of range, or a `rating.<p>` left in the strip's properties is refused.
#[test]
fn a_strip_s_fuse_position_ratings_are_typed() {
    use ecad_model::device::{Fuse, FuseClass, PositionLabel, TerminalStrip};
    use ecad_model::units::Amps;
    let mut s = TerminalStrip::default();
    s.fuses.insert(PositionLabel::new("3").unwrap(), Fuse { rated_current: Some(Amps(6.3)), class: Some(FuseClass::T), breaking: None });
    let d = Device { device_type: DeviceType::TerminalStrip(s.clone()), ..Device::default() };
    let json = serde_json::to_value(&d.device_type).unwrap();
    assert_eq!(json, serde_json::json!({"terminal_strip": {"fuses": {"3": {"rated_current": 6.3, "class": "t"}}}}));
    assert_eq!(serde_json::from_value::<DeviceType>(json).unwrap(), d.device_type, "round-trips");
    assert_eq!(serde_json::to_value(DeviceType::TerminalStrip(TerminalStrip::default())).unwrap(), serde_json::json!({"terminal_strip": {}}));
    assert_eq!(d.value("rating.3").as_deref(), Some("6.3 A, T"));
    assert_eq!((d.value("rating.4"), d.value("rating")), (None, None));
    assert!(!d.has_no_rating(), "a strip itself is not warned about");
    for bad in [
        serde_json::json!({"terminal_strip": {"accepts": []}}),
        serde_json::json!({"terminal_strip": {"fuses": {"3": {"curve": "b"}}}}),
        serde_json::json!({"terminal_strip": {"fuses": {" 3": {"rated_current": 4.0}}}}),
    ] {
        assert!(serde_json::from_value::<DeviceType>(bad.clone()).is_err(), "{bad}");
    }
    // The invariant: a fuse with no value, a value out of range, or the old property is refused.
    let mut ids = IdAllocator::seeded(19);
    let mut l = Ledger::new(19);
    let label = PositionLabel::new("1").unwrap();
    let wild = |f: Fuse| Device { device_type: DeviceType::TerminalStrip(TerminalStrip { fuses: [(label.clone(), f)].into() }), ..Device::default() };
    let mut old = d.clone();
    old.properties.insert("rating.3".into(), "4 A".into());
    for bad in [wild(Fuse::default()), wild(Fuse { rated_current: Some(Amps(-1.0)), ..Fuse::default() }), old] {
        let c: DeviceId = ids.fresh();
        assert!(l.commit(Author::User, "wild", vec![Op::CreateDevice { id: c, device: bad }]).is_err());
    }
    let c: DeviceId = ids.fresh();
    l.commit(Author::User, "add", vec![Op::CreateDevice { id: c, device: d }]).unwrap();
}

/// Step 3, Motor (design 072 §2.2 row 5, "Ratings"): a motor's phases and nameplate are typed
/// fields, shown from them; a motor with none of them set has no rating; a value out of range is
/// refused.
#[test]
fn a_motor_s_rating_is_typed() {
    use ecad_model::device::{Motor, Phases, RatingNote, Starting};
    use ecad_model::units::{Amps, Percent, Watts};
    let m = Motor {
        rated_power: Some(Watts(7500.0)),
        rated_current: Some(Amps(15.2)),
        power_factor: Some(0.85),
        efficiency: Some(Percent(91.0)),
        starting: Some(Starting::StarDelta),
        ..Motor::empty(Phases::Three)
    };
    let d = Device { device_type: DeviceType::Motor(m), ..Device::default() };
    let json = serde_json::to_value(&d.device_type).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"motor": {"phases": "three", "rated_power": 7500.0, "rated_current": 15.2, "power_factor": 0.85, "efficiency": 91.0, "starting": "star_delta"}})
    );
    assert_eq!(serde_json::from_value::<DeviceType>(json).unwrap(), d.device_type, "round-trips");
    assert_eq!(d.value("rating").as_deref(), Some("7.5 kW, 15.2 A, cos φ 0.85, eff 91 %, star-delta"));
    assert!(!d.has_no_rating());
    for bad in [
        serde_json::json!({"motor": {}}),
        serde_json::json!({"motor": {"phases": "two"}}),
        serde_json::json!({"motor": {"phases": "three", "starting": "wye"}}),
        serde_json::json!({"motor": {"phases": "three", "voltage": 400.0}}),
    ] {
        assert!(serde_json::from_value::<DeviceType>(bad.clone()).is_err(), "{bad}");
    }
    // An unrated motor has no rating, also with a note.
    let unrated = Device {
        device_type: DeviceType::Motor(Motor::empty(Phases::Single)),
        rating_note: Some(RatingNote::new("IE3").unwrap()),
        ..Device::default()
    };
    assert!(unrated.has_no_rating());
    assert_eq!(unrated.value("rating").as_deref(), Some("IE3"));
    // The invariant: a value out of range or not a number is refused.
    let mut ids = IdAllocator::seeded(18);
    let mut l = Ledger::new(18);
    for wild in [
        Motor { power_factor: Some(1.5), ..Motor::empty(Phases::Three) },
        Motor { rated_power: Some(Watts(f64::NAN)), ..Motor::empty(Phases::Three) },
        Motor { efficiency: Some(Percent(0.0)), ..Motor::empty(Phases::Single) },
    ] {
        let c: DeviceId = ids.fresh();
        let bad = Device { device_type: DeviceType::Motor(wild), ..Device::default() };
        assert!(l.commit(Author::User, "wild", vec![Op::CreateDevice { id: c, device: bad }]).is_err());
    }
    let c: DeviceId = ids.fresh();
    l.commit(Author::User, "add", vec![Op::CreateDevice { id: c, device: d }]).unwrap();
}

/// Step 3, Load (design 072 §2.2 row 6, "Ratings"): a load's rated power is a typed field, shown
/// from it; a load without it has no rating; a power out of range is refused.
#[test]
fn a_load_s_rating_is_typed() {
    use ecad_model::device::{Load, RatingNote};
    use ecad_model::units::Watts;
    let d = Device { device_type: DeviceType::Load(Load { rated_power: Some(Watts(3000.0)) }), ..Device::default() };
    let json = serde_json::to_value(&d.device_type).unwrap();
    assert_eq!(json, serde_json::json!({"load": {"rated_power": 3000.0}}));
    assert_eq!(serde_json::from_value::<DeviceType>(json).unwrap(), d.device_type, "round-trips");
    assert_eq!(serde_json::to_value(DeviceType::Load(Load::default())).unwrap(), serde_json::json!({"load": {}}));
    assert_eq!(d.value("rating").as_deref(), Some("3 kW"));
    assert!(!d.has_no_rating());
    for bad in [serde_json::json!({"load": {"voltage": 24.0}}), serde_json::json!({"load": {"rated_power": "3 kW"}})] {
        assert!(serde_json::from_value::<DeviceType>(bad.clone()).is_err(), "{bad}");
    }
    // An unrated load has no rating, also with a note.
    let unrated =
        Device { device_type: DeviceType::Load(Load::default()), rating_note: Some(RatingNote::new("see datasheet").unwrap()), ..Device::default() };
    assert!(unrated.has_no_rating());
    assert_eq!(unrated.value("rating").as_deref(), Some("see datasheet"));
    // The invariant: a power out of range or not a number is refused.
    let mut ids = IdAllocator::seeded(17);
    let mut l = Ledger::new(17);
    for wild in [Load { rated_power: Some(Watts(0.0)) }, Load { rated_power: Some(Watts(f64::NAN)) }] {
        let c: DeviceId = ids.fresh();
        let bad = Device { device_type: DeviceType::Load(wild), ..Device::default() };
        assert!(l.commit(Author::User, "wild", vec![Op::CreateDevice { id: c, device: bad }]).is_err());
    }
    let c: DeviceId = ids.fresh();
    l.commit(Author::User, "add", vec![Op::CreateDevice { id: c, device: d }]).unwrap();
}

/// Step 3, Indicator (design 072 §2.2 row 7, "Ratings"): an indicator's signal is stored, and its
/// colour, rated voltage and power are typed fields, shown from them; an indicator with none of
/// them has no rating; a value out of range is refused.
#[test]
fn an_indicator_s_rating_is_typed() {
    use ecad_model::device::{Indicator, IndicatorColour, RatingNote, SignalType};
    use ecad_model::units::{Current, RatedVoltage, Volts, Watts};
    let i = Indicator {
        colour: Some(IndicatorColour::Red),
        rated_voltage: Some(RatedVoltage { volts: Volts(24.0), current: Current::Dc }),
        power: Some(Watts(2.0)),
        ..Indicator::empty(SignalType::Flashing)
    };
    let d = Device { device_type: DeviceType::Indicator(i), ..Device::default() };
    let json = serde_json::to_value(&d.device_type).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"indicator": {"signal": "flashing", "colour": "red", "rated_voltage": {"volts": 24.0, "current": "dc"}, "power": 2.0}})
    );
    assert_eq!(serde_json::from_value::<DeviceType>(json).unwrap(), d.device_type, "round-trips");
    assert_eq!(serde_json::to_value(DeviceType::of_group(DeviceGroup::Indicator)).unwrap(), serde_json::json!({"indicator": {"signal": "steady"}}));
    assert_eq!(d.value("rating").as_deref(), Some("red, flashing, 24 V DC, 2 W"));
    assert!(!d.has_no_rating());
    for bad in [
        serde_json::json!({"indicator": {}}),
        serde_json::json!({"indicator": {"signal": "blinking"}}),
        serde_json::json!({"indicator": {"signal": "steady", "colour": "black"}}),
        serde_json::json!({"indicator": {"signal": "steady", "rated_voltage": 24.0}}),
    ] {
        assert!(serde_json::from_value::<DeviceType>(bad.clone()).is_err(), "{bad}");
    }
    // An unrated indicator has no rating, also with a note.
    let unrated = Device {
        device_type: DeviceType::Indicator(Indicator::empty(SignalType::Steady)),
        rating_note: Some(RatingNote::new("LED").unwrap()),
        ..Device::default()
    };
    assert!(unrated.has_no_rating());
    assert_eq!(unrated.value("rating").as_deref(), Some("LED"));
    // The invariant: a value out of range or not a number is refused.
    let mut ids = IdAllocator::seeded(16);
    let mut l = Ledger::new(16);
    for wild in [
        Indicator { power: Some(Watts(0.0)), ..Indicator::empty(SignalType::Steady) },
        Indicator { rated_voltage: Some(RatedVoltage { volts: Volts(f64::NAN), current: Current::Ac }), ..Indicator::empty(SignalType::Steady) },
    ] {
        let c: DeviceId = ids.fresh();
        let bad = Device { device_type: DeviceType::Indicator(wild), ..Device::default() };
        assert!(l.commit(Author::User, "wild", vec![Op::CreateDevice { id: c, device: bad }]).is_err());
    }
    let c: DeviceId = ids.fresh();
    l.commit(Author::User, "add", vec![Op::CreateDevice { id: c, device: d }]).unwrap();
}

/// Step 3, PowerSupply (design 072 §2.2 row 1, "Ratings"): a power supply and a transformer each
/// carry their own typed fields, shown from them; a transformer cannot hold watts or a DC voltage;
/// one with none of them has no rating; a value out of range is refused.
#[test]
fn a_power_supply_s_rating_is_typed() {
    use ecad_model::device::{PowerSupply, Psu, RatingNote, Transformer};
    use ecad_model::units::{Amps, Current, RatedVoltage, VoltAmps, Volts, Watts};
    let v = |volts, current| Some(RatedVoltage { volts: Volts(volts), current });
    let psu = Psu { input: v(230.0, Current::Ac), output: v(24.0, Current::Dc), output_current: Some(Amps(5.0)), output_power: Some(Watts(120.0)) };
    let d = Device { device_type: DeviceType::PowerSupply(PowerSupply::Psu(psu)), ..Device::default() };
    let json = serde_json::to_value(&d.device_type).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"power_supply": {"psu": {"input": {"volts": 230.0, "current": "ac"}, "output": {"volts": 24.0, "current": "dc"}, "output_current": 5.0, "output_power": 120.0}}})
    );
    assert_eq!(serde_json::from_value::<DeviceType>(json).unwrap(), d.device_type, "round-trips");
    assert_eq!(serde_json::to_value(DeviceType::of_group(DeviceGroup::PowerSupply)).unwrap(), serde_json::json!({"power_supply": {"psu": {}}}));
    assert_eq!(d.value("rating").as_deref(), Some("230 V AC / 24 V DC, 5 A, 120 W"));
    assert!(!d.has_no_rating());
    let t = Transformer { input: v(400.0, Current::Ac), output: v(24.0, Current::Ac), rated_power: Some(VoltAmps(250.0)) };
    assert_eq!(t.ratio(), Some(400.0 / 24.0));
    let td = Device { device_type: DeviceType::PowerSupply(PowerSupply::Transformer(t)), ..Device::default() };
    assert_eq!(td.value("rating").as_deref(), Some("400 V AC / 24 V AC, 250 VA"));
    for bad in [
        serde_json::json!({"power_supply": {}}),
        serde_json::json!({"power_supply": {"battery": {}}}),
        serde_json::json!({"power_supply": {"transformer": {"output_power": 100.0}}}),
        serde_json::json!({"power_supply": {"psu": {"rated_power": 100.0}}}),
        serde_json::json!({"power_supply": {"psu": {"output": 24.0}}}),
    ] {
        assert!(serde_json::from_value::<DeviceType>(bad.clone()).is_err(), "{bad}");
    }
    // An unrated supply has no rating, also with a note.
    let unrated = Device {
        device_type: DeviceType::PowerSupply(PowerSupply::Transformer(Transformer::default())),
        rating_note: Some(RatingNote::new("see datasheet").unwrap()),
        ..Device::default()
    };
    assert!(unrated.has_no_rating());
    assert_eq!(unrated.value("rating").as_deref(), Some("see datasheet"));
    // The invariant: a value out of range or not a number, or a DC transformer, is refused.
    let mut ids = IdAllocator::seeded(17);
    let mut l = Ledger::new(17);
    for wild in [
        PowerSupply::Psu(Psu { output_current: Some(Amps(0.0)), ..Psu::default() }),
        PowerSupply::Psu(Psu { output: v(f64::NAN, Current::Dc), ..Psu::default() }),
        PowerSupply::Transformer(Transformer { output: v(24.0, Current::Dc), ..Transformer::default() }),
    ] {
        let c: DeviceId = ids.fresh();
        let bad = Device { device_type: DeviceType::PowerSupply(wild), ..Device::default() };
        assert!(l.commit(Author::User, "wild", vec![Op::CreateDevice { id: c, device: bad }]).is_err());
    }
    let c: DeviceId = ids.fresh();
    l.commit(Author::User, "add", vec![Op::CreateDevice { id: c, device: td }]).unwrap();
}
