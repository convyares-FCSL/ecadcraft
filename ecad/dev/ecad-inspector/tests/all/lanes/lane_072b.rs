//! Lane 072 B (design 072 §2, acceptance A15): the inspector shows a device's type and the
//! `ModifyDevice` op in its Readable and Raw views.

use std::time::Duration;

use ecad_inspector::source::{FileSource, Source, VerifyState};
use ecad_model::device::{DeviceGroup, DeviceType};
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
    use ecad_model::{FunctionId, ListEntry, LocationId, Project, text::Text};
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
