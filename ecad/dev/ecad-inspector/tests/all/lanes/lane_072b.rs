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
