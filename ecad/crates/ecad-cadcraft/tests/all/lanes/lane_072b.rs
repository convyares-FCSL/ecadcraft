//! Lane 072 B (design 072 §2.3, acceptance A7 and A8): a device's type comes from its symbol and
//! agrees with what today's predicates say it is.

use ecad_cadcraft::symbols::{STARTERS, StarterKey};
use ecad_model::device::DeviceGroup;
use ecad_model::{DeviceId, Document, Word};
use serde_json::json;

use crate::common::{H, id};

/// The group today's predicates imply for device `c`, where one of them speaks (design 072 A7).
fn predicates_say(doc: &Document, c: DeviceId) -> Option<DeviceGroup> {
    use ecad_cadcraft::{bom, connectors, continuations, locations, plc, strips};
    if strips::is_fuse_device(doc, c) || bom::is_fuse_terminal(doc, c) {
        // The owner's answer (design 072 §2.2): a fuse terminal device is Protection; a fuse
        // position in a strip (`termfuse`) is the strip's.
        return Some(DeviceGroup::Protection);
    }
    if strips::is_strip(doc, c) {
        return Some(DeviceGroup::TerminalStrip);
    }
    if connectors::is_connector(doc, c) {
        return Some(DeviceGroup::Connector);
    }
    // Lane C (design 072 §3) removed the PLC predicates; the oracle reads the card body drawn.
    let body = doc
        .representations_of(ecad_model::Subject::Device(c))
        .find_map(|(_, r)| ecad_cadcraft::devices::rep_starter(doc, r).and_then(plc::rack::CardType::of_symbol));
    match body {
        Some(t) if t.is_rack() => return Some(DeviceGroup::PlcRack),
        Some(_) => return Some(DeviceGroup::PlcModule),
        None => {}
    }
    if continuations::is_continuation(doc, c) || locations::is_box(doc, c) || bom::is_item(doc, c) {
        return Some(DeviceGroup::General);
    }
    None
}

/// A7: one device per starter that places on its own; each has the group its starter declares,
/// and wherever a predicate speaks, the predicate's group.
#[test]
fn types_match_the_old_predicates() {
    let mut h = H::new(0x72_b7);
    let mut placed = Vec::new();
    for (n, s) in STARTERS.iter().enumerate() {
        let at = [20.0 + 30.0 * (n % 12) as f64, 30.0 + 40.0 * (n / 12) as f64];
        if let Ok(v) = h.run("place", json!({"symbol": s.key.word(), "at": at}))
            && v.get("component").is_some()
        {
            placed.push((s.key, id::<DeviceId>(&v, "component")));
        }
    }
    assert!(placed.len() >= 80, "most starters place on their own: {}", placed.len());
    let doc = h.doc();
    let mut spoke = 0;
    for (key, c) in placed {
        let group = doc.devices[&c].device_type.group();
        assert_eq!(group, key.device_group(), "{key}: the device has its starter's type");
        if let Some(want) = predicates_say(&doc, c) {
            spoke += 1;
            assert_eq!(group, want, "{key}: the predicates say {want:?}");
        }
    }
    assert!(spoke >= 40, "the predicates spoke for {spoke} devices");
}

/// A8: placing a symbol as a new device gives the symbol's type; adding another kind of symbol to
/// that device keeps it.
#[test]
fn the_symbol_declares_the_type_and_joining_keeps_it() {
    let mut h = H::new(0x72_b8);
    let k = id::<DeviceId>(&h.run("place", json!({"symbol": "coil", "at": [40, 40], "tag": "-K01"})).unwrap(), "component");
    let q = id::<DeviceId>(&h.run("place", json!({"symbol": "mcb1", "at": [80, 40]})).unwrap(), "component");
    let doc = h.doc();
    assert_eq!(doc.devices[&k].device_type.group(), DeviceGroup::Switching);
    assert_eq!(doc.devices[&q].device_type.group(), DeviceGroup::Protection);
    // A float switch's contact placed with -K01's tag joins -K01, which stays Switching.
    let joined = id::<DeviceId>(&h.run("place", json!({"symbol": "floatno", "at": [120, 40], "tag": "-K01"})).unwrap(), "component");
    assert_eq!(joined, k);
    assert_eq!(h.doc().devices[&k].device_type.group(), DeviceGroup::Switching);
    // The starter's definition declares it.
    let lamp = STARTERS.iter().find(|s| s.key == StarterKey::Lamp).unwrap();
    assert_eq!(lamp.definition().unwrap().device_type.group(), DeviceGroup::Indicator);
}
