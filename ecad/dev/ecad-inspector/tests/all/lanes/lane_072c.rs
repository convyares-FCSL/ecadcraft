//! The inspector shows a PLC station's and module's
//! fields, and a `ModifyDevice` changing them, in its Readable and Raw views.

use std::time::Duration;

use ecad_inspector::source::{FileSource, Source, VerifyState};
use ecad_model::objects::plc::{ChannelGroup, ModuleFunction, PlcModule, PlcRack, Protocol, RackHead, Signal};
use ecad_model::{Author, Device, DeviceId, DeviceType, IdAllocator, Op};

use crate::common::*;

#[test]
fn plc_fields_and_their_change_are_shown_raw_and_readable() {
    let dir = TempDir::new("072c-plc");
    let mut l = history();
    let mut ids = IdAllocator::seeded(72);
    let (r, c): (DeviceId, DeviceId) = (ids.fresh(), ids.fresh());
    let station = Device {
        device_type: DeviceType::PlcRack(PlcRack { head: RackHead::Coupler { protocol: Protocol::Ethercat }, ..PlcRack::default() }),
        ..Device::default()
    };
    let mut m = PlcModule::new(ModuleFunction::AnalogIn(ChannelGroup::new(Signal::Rtd, 4)));
    m.rack = Some(r);
    let before = Device { device_type: DeviceType::PlcModule(m.clone()), ..Device::default() };
    m.attributes.intrinsically_safe = true;
    m.slot = Some(3);
    let after = Device { device_type: DeviceType::PlcModule(m), ..before.clone() };
    l.commit(Author::User, "add", vec![Op::CreateDevice { id: r, device: station }, Op::CreateDevice { id: c, device: before.clone() }]).unwrap();
    l.commit(Author::User, "slot", vec![Op::ModifyDevice { id: c, before, after }]).unwrap();
    let path = written(&dir, &l);

    let log = run(&[path.to_str().unwrap(), "log", "main", "--last", "2"]).unwrap();
    for want in ["ModifyDevice", "plc_module", "analog_in", "rtd", "intrinsically_safe", "plc_rack", "coupler", "ethercat"] {
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
    assert!(text.contains("ModifyDevice") && text.contains("intrinsically_safe") && text.contains("\"slot\""), "{text}");
    let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
    let rd = f.store().unwrap().reader().unwrap();
    let mut bytes = None;
    rd.raw_rows(|t, k, v| {
        if t == "transactions" && ecad_inspector::text::hex(k) == key {
            bytes = Some(v.to_vec());
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(raw["ok"]["hex"], ecad_inspector::text::hex(&bytes.unwrap()));
}
