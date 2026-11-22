//! Lane 072 B (design 072 §2, acceptance A15): the inspector shows a device's type and the
//! `ModifyDevice` op in its Readable and Raw views.

use std::time::Duration;

use ecad_inspector::source::{FileSource, Source, VerifyState};
use ecad_model::objects::devices::{DeviceGroup, DeviceType};
use ecad_model::{Author, Device, DeviceId, IdAllocator, Op};

use crate::common::*;

#[test]
fn a_device_s_type_and_modify_device_are_shown_raw_and_readable() {
    let dir = TempDir::new("072b-device");
    let mut l = history();
    let mut ids = IdAllocator::seeded(72);
    let c: DeviceId = ids.fresh();
    let before = Device { device_type: DeviceType::of_group(DeviceGroup::Switching), ..Device::default() };
    let after = Device { device_type: DeviceType::of_group(DeviceGroup::Protection), ..before.clone() };
    l.commit(Author::User, "add", vec![Op::CreateDevice { id: c, device: before.clone() }]).unwrap();
    l.commit(Author::User, "retype", vec![Op::ModifyDevice { id: c, before, after }]).unwrap();
    let path = written(&dir, &l);

    // Readable: the CLI log names the op, the field and both types.
    let log = run(&[path.to_str().unwrap(), "log", "main", "--last", "1"]).unwrap();
    for want in ["ModifyDevice", "device_type", "switching", "protection"] {
        assert!(log.contains(want), "{want} in the readable log: {log}");
    }
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let last = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = last["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let key = rows.last().and_then(|r| r["key"].as_str()).unwrap().to_string();
    let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
    let text = readable["ok"]["decoded"].to_string();
    assert!(text.contains("ModifyDevice") && text.contains("protection"), "{text}");
    // Raw: exactly the stored bytes of that row.
    let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
    let r = f.store().unwrap().reader().unwrap();
    let mut bytes = None;
    r.raw_rows(|t, k, v| {
        if t == "transactions" && ecad_inspector::text::hex(k) == key {
            bytes = Some(v.to_vec());
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(raw["ok"]["hex"], ecad_inspector::text::hex(&bytes.unwrap()));
}

#[test]
fn a_device_s_text_fields_are_shown_raw_and_readable() {
    use ecad_model::TextField;
    let dir = TempDir::new("072b-text");
    let mut l = history();
    let mut ids = IdAllocator::seeded(73);
    let c: DeviceId = ids.fresh();
    let before = Device::default();
    let mut after = before.with_text(TextField::Description, Some("Main pump")).unwrap();
    after.set_text(TextField::PartNumber, Some("3RT2016")).unwrap();
    after.set_text(TextField::Manufacturer, Some("Siemens")).unwrap();
    l.commit(Author::User, "add", vec![Op::CreateDevice { id: c, device: before.clone() }]).unwrap();
    l.commit(Author::User, "describe", vec![Op::ModifyDevice { id: c, before, after }]).unwrap();
    let path = written(&dir, &l);
    let log = run(&[path.to_str().unwrap(), "log", "main", "--last", "1"]).unwrap();
    for want in ["ModifyDevice", "description", "Main pump", "part_number", "3RT2016", "manufacturer", "Siemens"] {
        assert!(log.contains(want), "{want} in the readable log: {log}");
    }
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let rows = ask(s, serde_json::json!({"op": "devices", "detail": true}));
    let text = rows["ok"].to_string();
    assert!(text.contains("\"description\":\"Main pump\"") && text.contains("\"maker\":\"Siemens\""), "{text}");
}

/// Step 2c (design 072 §6.3, A15): a device's location and function, with their `fixed` flags,
/// show in the readable log and in the devices view as their codes.
#[test]
fn a_device_s_location_and_function_are_shown() {
    use {ecad_model::FunctionId, ecad_model::ListEntry, ecad_model::LocationId, ecad_model::Project, ecad_model::naming::text::Text};
    let dir = TempDir::new("072b-place");
    let mut l = history();
    let mut ids = IdAllocator::seeded(74);
    let c: DeviceId = ids.fresh();
    let (loc, fun): (LocationId, FunctionId) = (ids.fresh(), ids.fresh());
    let project = Project {
        locations: vec![ListEntry::new(loc, Text::new("+CP1").unwrap())],
        functions: vec![ListEntry::new(fun, Text::new("=F1").unwrap())],
        ..Project::default()
    };
    let before = Device::default();
    let after = Device { location: Some(loc), location_fixed: true, function: Some(fun), ..before.clone() };
    l.commit(Author::User, "list", vec![Op::ModifyProject { before: Project::default(), after: project }]).unwrap();
    l.commit(Author::User, "add", vec![Op::CreateDevice { id: c, device: before.clone() }]).unwrap();
    l.commit(Author::User, "place", vec![Op::ModifyDevice { id: c, before, after }]).unwrap();
    let path = written(&dir, &l);
    let log = run(&[path.to_str().unwrap(), "log", "main", "--last", "1"]).unwrap();
    for want in ["ModifyDevice", "location", "location_fixed", "function"] {
        assert!(log.contains(want), "{want} in the readable log: {log}");
    }
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let rows = ask(s, serde_json::json!({"op": "devices", "detail": true}));
    let text = rows["ok"].to_string();
    assert!(text.contains("\"location\":\"+CP1\"") && text.contains("\"location_fixed\":true") && text.contains("\"function\":\"=F1\""), "{text}");
}

/// Step 3, Connector (A15): a connector's family, pins and half parts show in the readable log.
#[test]
fn a_connector_s_data_is_shown() {
    use ecad_model::objects::devices::{Connector, ConnectorFamily, PartText, PinCount};
    let dir = TempDir::new("072b-conn");
    let mut l = history();
    let mut ids = IdAllocator::seeded(75);
    let c: DeviceId = ids.fresh();
    let before = Device { device_type: DeviceType::Connector(Box::new(Connector::of_family(ConnectorFamily::M12))), ..Device::default() };
    let mut x = Connector::of_family(ConnectorFamily::M12);
    x.pins = PinCount::new(5);
    x.socket.manufacturer = Some(PartText::new("Acme").unwrap());
    let after = Device { device_type: DeviceType::Connector(Box::new(x)), ..before.clone() };
    l.commit(Author::User, "add", vec![Op::CreateDevice { id: c, device: before.clone() }]).unwrap();
    l.commit(Author::User, "pins", vec![Op::ModifyDevice { id: c, before, after }]).unwrap();
    let path = written(&dir, &l);
    let log = run(&[path.to_str().unwrap(), "log", "main", "--last", "1"]).unwrap();
    for want in ["ModifyDevice", "connector", "m12", "pins", "Acme"] {
        assert!(log.contains(want), "{want} in the readable log: {log}");
    }
}

/// Step 4, Connector (A15): a connector's `mating` shows in the readable log and the Readable row
/// view, and the Raw view is the stored bytes.
#[test]
fn a_connector_s_mating_is_shown_raw_and_readable() {
    use ecad_model::objects::devices::{Connector, ConnectorFamily};
    let before = Device { device_type: DeviceType::Connector(Box::new(Connector::of_family(ConnectorFamily::M12))), ..Device::default() };
    let x = Connector { mating: true, ..Connector::of_family(ConnectorFamily::M12) };
    let after = Device { device_type: DeviceType::Connector(Box::new(x)), ..before.clone() };
    shown_raw_and_readable("072b-mating", 83, before, after, &["connector", "m12", "mating"], "mating");
}

/// Step 4, contacts (A15): a switching device's typed contacts and table placement show in the
/// readable log and the Readable row view, and the Raw view is the stored bytes.
#[test]
fn contacts_and_the_contact_table_are_shown_raw_and_readable() {
    use ecad_model::objects::devices::{ContactEntry, ContactPoles, ContactSet, ContactTable, OperatorKind, PoleKind};
    let before = Device { device_type: DeviceType::Switching(OperatorKind::Coil.empty()), ..Device::default() };
    let mut k = OperatorKind::Coil.empty();
    k.contacts =
        ContactSet::new(vec![ContactEntry { poles: ContactPoles::Count(std::num::NonZeroU8::new(3).unwrap()), kind: PoleKind::No, power: true }]);
    k.contact_table = Some(ContactTable::Below);
    let after = Device { device_type: DeviceType::Switching(k), ..before.clone() };
    shown_raw_and_readable(
        "072b-contacts",
        84,
        before,
        after,
        &["switching", "contacts", "count", "power", "contact_table", "below"],
        "contact_table",
    );
}

/// Step 3, Protection (A15): a protective device's typed rating and its note show in the readable
/// log.
#[test]
fn a_protective_rating_is_shown() {
    use ecad_model::objects::devices::{Breaker, Protection, RatingNote, TripCurve};
    use ecad_model::geometry::units::Amps;
    let dir = TempDir::new("072b-rating");
    let mut l = history();
    let mut ids = IdAllocator::seeded(76);
    let c: DeviceId = ids.fresh();
    let before = Device {
        device_type: DeviceType::Protection(Protection::Mcb(Breaker::default())),
        rating_note: Some(RatingNote::new("see datasheet").unwrap()),
        ..Device::default()
    };
    let b = Breaker { rated_current: Some(Amps(16.0)), curve: Some(TripCurve::C), ..Breaker::default() };
    let after = Device { device_type: DeviceType::Protection(Protection::Mcb(b)), rating_note: None, ..before.clone() };
    l.commit(Author::User, "add", vec![Op::CreateDevice { id: c, device: before.clone() }]).unwrap();
    l.commit(Author::User, "rating", vec![Op::ModifyDevice { id: c, before, after }]).unwrap();
    let path = written(&dir, &l);
    let log = run(&[path.to_str().unwrap(), "log", "main", "--last", "1"]).unwrap();
    for want in ["ModifyDevice", "protection", "mcb", "rated_current", "see datasheet"] {
        assert!(log.contains(want), "{want} in the readable log: {log}");
    }
}

/// Step 3, Switching (A15): a switching device's operator and typed rating show in the readable
/// log and the Readable row view, and the Raw view is the stored bytes.
#[test]
fn a_switching_rating_is_shown_raw_and_readable() {
    use ecad_model::objects::devices::{Control, Operator, OperatorKind, Switching};
    use ecad_model::geometry::units::{Amps, Current, RatedVoltage, Volts};
    let before = Device { device_type: DeviceType::Switching(OperatorKind::Coil.empty()), ..Device::default() };
    let k = Switching {
        operator: Operator::Coil(Control { voltage: Some(RatedVoltage { volts: Volts(24.0), current: Current::Dc }), power: None }),
        ac3_current: Some(Amps(9.0)),
        ac3_power: None,
        ..Switching::default()
    };
    let after = Device { device_type: DeviceType::Switching(k), ..before.clone() };
    shown_raw_and_readable("072b-switching", 77, before, after, &["switching", "operator", "coil", "voltage", "dc", "ac3_current"], "ac3_current");
}

/// Step 3, Sensor (A15): a sensor's kind and typed supply show in the readable log and the
/// Readable row view, and the Raw view is the stored bytes.
#[test]
fn a_sensor_rating_is_shown_raw_and_readable() {
    use ecad_model::objects::devices::{Output, Sensing, Sensor};
    use ecad_model::geometry::units::{Current, RatedVoltage, Volts};
    let before = Device { device_type: DeviceType::Sensor(Sensor::empty(Sensing::Proximity, Output::Pnp)), ..Device::default() };
    let b = Sensor { supply: Some(RatedVoltage { volts: Volts(24.0), current: Current::Dc }), ..Sensor::empty(Sensing::Proximity, Output::Pnp) };
    let after = Device { device_type: DeviceType::Sensor(b), ..before.clone() };
    shown_raw_and_readable("072b-sensor", 78, before, after, &["sensor", "sensing", "proximity", "pnp", "supply", "dc"], "supply");
}

/// Step 3, TerminalStrip (A15): a strip's typed fuse position rating shows in the readable log
/// and the Readable row view, and the Raw view is the stored bytes.
#[test]
fn a_strip_fuse_position_rating_is_shown_raw_and_readable() {
    use ecad_model::objects::devices::{Fuse, PositionLabel, TerminalStrip};
    use ecad_model::geometry::units::Amps;
    let before = Device { device_type: DeviceType::TerminalStrip(TerminalStrip::default()), ..Device::default() };
    let fuse = Fuse { rated_current: Some(Amps(4.0)), ..Fuse::default() };
    let s = TerminalStrip { fuses: [(PositionLabel::new("3").unwrap(), fuse)].into() };
    let after = Device { device_type: DeviceType::TerminalStrip(s), ..before.clone() };
    shown_raw_and_readable("072b-strip", 79, before, after, &["terminal_strip", "fuses", "rated_current"], "fuses");
}

/// Step 3, Motor (A15): a motor's phases and typed nameplate show in the readable log and the
/// Readable row view, and the Raw view is the stored bytes.
#[test]
fn a_motor_rating_is_shown_raw_and_readable() {
    use ecad_model::objects::devices::{Motor, Phases, Starting};
    use ecad_model::geometry::units::Watts;
    let before = Device { device_type: DeviceType::Motor(Motor::empty(Phases::Three)), ..Device::default() };
    let m = Motor { rated_power: Some(Watts(7500.0)), starting: Some(Starting::StarDelta), ..Motor::empty(Phases::Three) };
    let after = Device { device_type: DeviceType::Motor(m), ..before.clone() };
    shown_raw_and_readable("072b-motor", 79, before, after, &["motor", "phases", "three", "rated_power", "star_delta"], "rated_power");
}

/// Step 3, Load (A15): a load's typed power shows in the readable log and the Readable row view,
/// and the Raw view is the stored bytes.
#[test]
fn a_load_rating_is_shown_raw_and_readable() {
    use ecad_model::objects::devices::Load;
    use ecad_model::geometry::units::Watts;
    let before = Device { device_type: DeviceType::Load(Load::default()), ..Device::default() };
    let after = Device { device_type: DeviceType::Load(Load { rated_power: Some(Watts(3000.0)) }), ..before.clone() };
    shown_raw_and_readable("072b-load", 80, before, after, &["load", "rated_power"], "rated_power");
}

/// Step 3, Indicator (A15): an indicator's signal and typed colour and voltage show in the readable
/// log and the Readable row view, and the Raw view is the stored bytes.
#[test]
fn an_indicator_rating_is_shown_raw_and_readable() {
    use ecad_model::objects::devices::{Indicator, IndicatorColour, SignalType};
    use ecad_model::geometry::units::{Current, RatedVoltage, Volts};
    let before = Device { device_type: DeviceType::Indicator(Indicator::empty(SignalType::Steady)), ..Device::default() };
    let i = Indicator {
        colour: Some(IndicatorColour::Red),
        rated_voltage: Some(RatedVoltage { volts: Volts(24.0), current: Current::Dc }),
        ..Indicator::empty(SignalType::Steady)
    };
    let after = Device { device_type: DeviceType::Indicator(i), ..before.clone() };
    shown_raw_and_readable("072b-lamp", 81, before, after, &["indicator", "signal", "steady", "colour", "rated_voltage"], "rated_voltage");
}

/// Step 3, PowerSupply (A15): a supply's kind and typed voltages show in the readable log and the
/// Readable row view, and the Raw view is the stored bytes.
#[test]
fn a_power_supply_rating_is_shown_raw_and_readable() {
    use ecad_model::objects::devices::{PowerSupply, Transformer};
    use ecad_model::geometry::units::{Current, RatedVoltage, VoltAmps, Volts};
    let before = Device { device_type: DeviceType::PowerSupply(PowerSupply::Transformer(Transformer::default())), ..Device::default() };
    let ac = |v| Some(RatedVoltage { volts: Volts(v), current: Current::Ac });
    let t = Transformer { input: ac(400.0), output: ac(24.0), rated_power: Some(VoltAmps(250.0)) };
    let after = Device { device_type: DeviceType::PowerSupply(PowerSupply::Transformer(t)), ..before.clone() };
    shown_raw_and_readable("072b-trafo", 82, before, after, &["power_supply", "transformer", "input", "output", "rated_power"], "rated_power");
}

/// One device created as `before` and changed to `after`: the readable log names `log_words`,
/// the Readable row view names `readable_word`, and the Raw view is the stored bytes.
fn shown_raw_and_readable(name: &str, seed: u64, before: Device, after: Device, log_words: &[&str], readable_word: &str) {
    let mut ids = IdAllocator::seeded(seed);
    let c: DeviceId = ids.fresh();
    let add = vec![Op::CreateDevice { id: c, device: before.clone() }];
    shown(name, add, vec![Op::ModifyDevice { id: c, before, after }], "ModifyDevice", log_words, readable_word);
}

/// Commits `setup`, then `last`, and checks that the readable log of the last transaction names
/// `op` and `log_words`, that its Readable row view names `op` and `readable_word`, and that its
/// Raw view is exactly the stored bytes.
fn shown(name: &str, setup: Vec<Op>, last_ops: Vec<Op>, op: &str, log_words: &[&str], readable_word: &str) {
    let dir = TempDir::new(name);
    let mut l = history();
    l.commit(Author::User, "add", setup).unwrap();
    l.commit(Author::User, "last", last_ops).unwrap();
    let path = written(&dir, &l);
    let log = run(&[path.to_str().unwrap(), "log", "main", "--last", "1"]).unwrap();
    for want in std::iter::once(&op).chain(log_words) {
        assert!(log.contains(want), "{want} in the readable log: {log}");
    }
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let last = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = last["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let key = rows.last().and_then(|r| r["key"].as_str()).unwrap().to_string();
    let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
    let text = readable["ok"]["decoded"].to_string();
    assert!(text.contains(op) && text.contains(readable_word), "{text}");
    let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
    let r = f.store().unwrap().reader().unwrap();
    let mut bytes = None;
    r.raw_rows(|t, k, v| {
        if t == "transactions" && ecad_inspector::text::hex(k) == key {
            bytes = Some(v.to_vec());
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(raw["ok"]["hex"], ecad_inspector::text::hex(&bytes.unwrap()));
}

/// Step 4, roles (A15, design 072 §4.4, F3′): a definition's declared part and a representation's
/// stored role show as their words in the readable log and the Readable row view, and the Raw view
/// is the stored bytes.
#[test]
fn a_symbol_s_part_and_a_representation_s_role_are_shown_raw_and_readable() {
    use ecad_model::{Definition, DefinitionId, Form, PartRole, Representation, RepresentationId, Role, Subject};
    let mut ids = IdAllocator::seeded(85);
    let (c, d, r): (DeviceId, DefinitionId, RepresentationId) = (ids.fresh(), ids.fresh(), ids.fresh());
    let page = history().document().page_order[0];
    let definition = Definition { name: "coil".into(), part_role: PartRole::Head, ..Definition::default() };
    let rep = Representation {
        subject: Subject::Device(c),
        page,
        form: Form::Drawn,
        role: Role::Part(PartRole::Contact),
        items: Vec::new(),
        connection_points: Default::default(),
    };
    shown(
        "072b-roles",
        vec![Op::CreateDevice { id: c, device: Device::default() }],
        vec![Op::EmbedDefinition { id: d, definition }, Op::LinkRepresentation { id: r, rep }],
        "LinkRepresentation",
        &["part_role", "head", "contact"],
        "contact",
    );
}

/// Step 5, user fields (A15, design 072 §2.8): the project's field list (`ModifyProject`) and a
/// device's value (`ModifyDevice`) show in the readable log and the Readable row view, and the
/// Raw view is the stored bytes.
#[test]
fn user_fields_and_a_device_s_value_are_shown_raw_and_readable() {
    use ecad_model::naming::text::Text;
    use ecad_model::{Project, UserFieldDef, UserFieldId, UserFieldKind, UserValue};
    let mut ids = IdAllocator::seeded(86);
    let (c, f): (DeviceId, UserFieldId) = (ids.fresh(), ids.fresh());
    let def = UserFieldDef { id: f, name: Text::new("Panel row").unwrap(), kind: UserFieldKind::Number { unit: Some(Text::new("mm").unwrap()) } };
    let project = Project { user_fields: vec![def], ..Project::default() };
    let add_field = Op::ModifyProject { before: Project::default(), after: project.clone() };
    let device = vec![Op::CreateDevice { id: c, device: Device::default() }];
    shown("072b-user-field", device, vec![add_field], "ModifyProject", &["user_fields", "Panel row", "number", "mm"], "Panel row");
    let before = Device::default();
    let mut after = before.clone();
    after.user.insert(f, UserValue::Number(12.5));
    let setup = vec![Op::ModifyProject { before: Project::default(), after: project }, Op::CreateDevice { id: c, device: before.clone() }];
    shown("072b-user-value", setup, vec![Op::ModifyDevice { id: c, before, after }], "ModifyDevice", &["user", "12.5"], "12.5");
}

/// Step 5, device links (A15, design 072 §2.6): `AddDeviceLink` and `RemoveDeviceLink` show
/// with their kind and devices in the readable log and the Readable row view, and the Raw view
/// is the stored bytes.
#[test]
fn device_links_are_shown_raw_and_readable() {
    use ecad_model::{DeviceLink, LinkId, LinkKind};
    let mut ids = IdAllocator::seeded(87);
    let (a, b, l): (DeviceId, DeviceId, LinkId) = (ids.fresh(), ids.fresh(), ids.fresh());
    let devices = vec![Op::CreateDevice { id: a, device: Device::default() }, Op::CreateDevice { id: b, device: Device::default() }];
    let link = DeviceLink { kind: LinkKind::MechanicalInterlock, a, b };
    let add = Op::AddDeviceLink { id: l, link };
    shown(
        "072b-link-add",
        devices.clone(),
        vec![add.clone()],
        "AddDeviceLink",
        &["mechanical_interlock", &a.hex(), &b.hex()],
        "mechanical_interlock",
    );
    let mut setup = devices;
    setup.push(add);
    shown(
        "072b-link-remove",
        setup,
        vec![Op::RemoveDeviceLink { id: l, link }],
        "RemoveDeviceLink",
        &["mechanical_interlock"],
        "mechanical_interlock",
    );
}
