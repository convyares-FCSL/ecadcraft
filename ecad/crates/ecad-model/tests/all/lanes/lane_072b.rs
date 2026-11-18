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
