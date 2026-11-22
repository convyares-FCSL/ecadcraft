//! S6 (#3): a tag's state is stored, never read off its spelling. A provisional `-K3` and a real
//! `-K3` are different tags; a provisional tag is not counted in the high-water mark; a record
//! without the state does not read.

use ecad_model::naming::tag::{self, HighWater, Key, TagState};
use ecad_model::{Device, DeviceId, Document, ObjectRef, Op};

fn doc_with(devices: &[(u128, &str, TagState)]) -> Document {
    let mut d = Document::default();
    for (id, t, state) in devices {
        d.devices.insert(DeviceId(*id), Device::default().with_tag(Some(t), *state));
    }
    d.rebuild_index();
    d
}

#[test]
fn a_provisional_k3_and_a_real_k3_are_different_tags() {
    let d = doc_with(&[(1, "-K3", TagState::Provisional), (2, "-K3", TagState::Fixed)]);
    assert_eq!(d.components_tagged("-K3"), vec![DeviceId(2)], "the real tag is held by the real device only");
    assert_eq!(d.components_with(&Key::of("K03", TagState::Provisional)), vec![DeviceId(1)]);
    assert_eq!(d.devices[&DeviceId(1)].tag_shown().as_deref(), Some("-K?3"), "the ? is drawn, never stored");
    assert_eq!(d.devices[&DeviceId(2)].tag_shown().as_deref(), Some("-K3"));
    assert!(d.tag_taken("-K3"));
    let only_provisional = doc_with(&[(1, "-K3", TagState::Provisional)]);
    assert!(!only_provisional.tag_taken("-K3"), "a provisional tag does not take the real one");
    assert_eq!(only_provisional.provisional_keys("K").collect::<Vec<_>>(), ["K3"]);
}

#[test]
fn a_provisional_tag_does_not_raise_the_high_water_mark() {
    let d = doc_with(&[(1, "-K40", TagState::Provisional)]);
    let mut high = HighWater::new();
    let create = |device: &Device| Op::CreateDevice { id: DeviceId(1), device: device.clone() };
    tag::note_ops(&mut high, &[create(&d.devices[&DeviceId(1)])]);
    assert_eq!(high.get("K"), None, "a placeholder number is not a number used");
    let real = Device::default().with_tag(Some("-K07"), TagState::Fixed);
    tag::note_ops(&mut high, &[Op::ModifyDevice { id: DeviceId(1), before: d.devices[&DeviceId(1)].clone(), after: real }]);
    assert_eq!(high.get("K"), Some(&7));
    let _ = ObjectRef::Project;
}

#[test]
fn a_record_without_the_state_does_not_read() {
    let with = serde_json::to_value(Device::default().with_tag(Some("-K01"), TagState::Fixed)).unwrap();
    assert_eq!(with["tag_state"], "fixed");
    let back: Device = serde_json::from_value(with.clone()).unwrap();
    assert_eq!(back.tag_state, TagState::Fixed);
    let mut without = with;
    without.as_object_mut().unwrap().remove("tag_state");
    let err = serde_json::from_value::<Device>(without).unwrap_err().to_string();
    assert!(err.contains("tag_state"), "{err}");
    let mut place = serde_json::to_value(Device::default()).unwrap();
    place.as_object_mut().unwrap().remove("location_state");
    assert!(serde_json::from_value::<Device>(place).is_err(), "a record without location_state does not read either");
}
