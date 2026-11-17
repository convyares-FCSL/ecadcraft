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
    assert!(text.contains(r#""device_type":{"motor":{}}"#), "{text}");
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
