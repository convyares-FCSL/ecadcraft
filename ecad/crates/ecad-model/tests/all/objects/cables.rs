//! Lane 072 D, part c2 (design 072 §4.5): `ModifyCable` records a cable whole, before and after.
//! It changes the cable's fields and cores in one op, is its own inverse, and the indexes (its
//! wires, routes and tag) follow it. A core's purpose is kept apart from its position. A key that
//! became a cable field (its shield, a core's Not used) and the offer key `cores` are never
//! written into the cable's properties.

use ecad_model::objects::styles::cable::Shield;
use ecad_model::{
    Author, Cable, CableId, ConductorId, ConnectionPoint, ConnectionPointId, Core, CoreLabel, CorePosition, IdAllocator, Ledger, NetId, ObjectRef,
    Op, Purpose, Wire, WireId,
};

/// A ledger with two wires and a cable holding the first.
fn cable_on_a_wire() -> (Ledger, CableId, WireId, WireId) {
    let mut l = Ledger::new(0x72dc);
    let mut ids = IdAllocator::seeded(0x72dc);
    let d: ecad_model::DeviceId = ids.fresh();
    let t: Vec<ConnectionPointId> = (0..4).map(|_| ids.fresh()).collect();
    let (w1, w2, k): (WireId, WireId, CableId) = (ids.fresh(), ids.fresh(), ids.fresh());
    let mut ops = vec![Op::CreateDevice { id: d, device: ecad_model::Device::default() }];
    for id in &t {
        ops.push(Op::AddConnectionPoint {
            id: *id,
            connection_point: ConnectionPoint { owner: d.into(), label: None, key: None, accepts: Default::default() },
        });
    }
    ops.push(Op::CreateWire { id: w1, wire: Wire::new(NetId(901), ConductorId(902), t[0], t[1]) });
    ops.push(Op::CreateWire { id: w2, wire: Wire::new(NetId(903), ConductorId(904), t[2], t[3]) });
    let mut cable = Cable { cores: vec![Core::on_wire(w1, CoreLabel::new("1").ok())], ..Cable::default() };
    cable.tag = ecad_model::naming::tag::TagSlot::of_text(Some("-W1"), Default::default());
    ops.push(Op::CreateCable { id: k, cable });
    l.commit(Author::User, "setup", ops).unwrap();
    (l, k, w1, w2)
}

#[test]
fn modify_cable_changes_fields_and_cores_and_undoes() {
    let (mut l, k, w1, w2) = cable_on_a_wire();
    let before = l.document().cables.get(&k).cloned().unwrap();
    let mut after = before.clone();
    after.shield = Some(Shield::Overall);
    after.tag = ecad_model::naming::tag::TagSlot::of_text(Some("-W7"), Default::default());
    after.cores = vec![Core::on_wire(w2, CoreLabel::new("1").ok()), Core::unplaced(CoreLabel::new("2").ok())];
    let op = Op::modify_cable(k, &before, after.clone()).unwrap();
    assert!(Op::modify_cable(k, &before, before.clone()).is_none(), "no change, no op");
    assert_eq!(op.name(), "ModifyCable");
    l.commit(Author::User, "edit", vec![op]).unwrap();
    let doc = l.document();
    assert_eq!(doc.cables.get(&k), Some(&after));
    assert!(doc.cables_of(w1).is_empty() && doc.cables_of(w2) == vec![k], "the wire index follows");
    assert_eq!(doc.cables_tagged("-W7"), vec![k], "the tag index follows");
    assert!(doc.cables_tagged("-W1").is_empty());
    doc.check_index().unwrap();
    // A stale `before` is refused; nothing changes.
    let stale = Op::ModifyCable { id: k, before: before.clone(), after: Cable::default() };
    assert!(l.commit(Author::User, "stale", vec![stale]).is_err());
    assert!(l.commit(Author::User, "missing", vec![Op::ModifyCable { id: CableId(1), before: after.clone(), after: before.clone() }]).is_err());
    l.undo(Author::User).unwrap();
    let doc = l.document();
    assert_eq!(doc.cables.get(&k), Some(&before));
    assert_eq!(doc.cables_of(w1), vec![k]);
    assert_eq!(doc.cables_tagged("-W1"), vec![k]);
    doc.check_index().unwrap();
}

#[test]
fn a_cores_purpose_stays_when_its_position_changes() {
    let (mut l, k, _, w2) = cable_on_a_wire();
    let before = l.document().cables.get(&k).cloned().unwrap();
    let mut spare = before.clone();
    spare.cores.push(Core { purpose: Purpose::Spare, ..Core::unplaced(CoreLabel::new("2").ok()) });
    l.commit(Author::User, "unused", Op::modify_cable(k, &before, spare.clone()).into_iter().collect()).unwrap();
    let mut placed = spare.clone();
    if let Some(m) = placed.cores.get_mut(1) {
        m.position = CorePosition::Wire(w2);
    }
    l.commit(Author::User, "place", Op::modify_cable(k, &spare, placed).into_iter().collect()).unwrap();
    let core = l.document().cables.get(&k).and_then(|c| c.cores.get(1).cloned()).unwrap();
    assert_eq!((core.purpose, core.position), (Purpose::Spare, CorePosition::Wire(w2)), "purpose is not worked out from the position");
}

#[test]
fn a_cables_fields_are_never_in_its_properties() {
    let (mut l, k, _, _) = cable_on_a_wire();
    let object = ObjectRef::Cable(k);
    for (key, value) in [("shield", "overall"), ("unused.2", "yes"), ("cores", "4G")] {
        let op = Op::SetProperty { object, key: key.into(), before: None, after: Some(value.into()) };
        assert!(l.commit(Author::User, key, vec![op]).is_err(), "SetProperty {key} on a cable is refused");
    }
    // The invariant holds for a whole cable too: a bag holding `shield` is refused.
    let before = l.document().cables.get(&k).cloned().unwrap();
    let mut after = before.clone();
    after.properties.insert("shield".into(), "overall".into());
    assert!(l.commit(Author::User, "bag", vec![Op::ModifyCable { id: k, before, after }]).is_err());
}

/// S1 (#2): a cable's tag (with its state) and its description are fields, as a device's are: a bag
/// holding one is refused, and the typed field reads back through the same `get`/`value` route.
#[test]
fn a_cables_tag_and_description_are_fields_not_properties() {
    let (mut l, k, _, _) = cable_on_a_wire();
    for key in ["tag", "description"] {
        let before = l.document().cables.get(&k).cloned().unwrap();
        let mut after = before.clone();
        after.properties.insert(key.into(), "x".into());
        let err = l.commit(Author::User, key, vec![Op::ModifyCable { id: k, before, after }]).unwrap_err().to_string();
        assert!(err.contains(key) && err.contains("it is a field"), "{key}: {err}");
    }
    let mut c = Cable::default();
    c.set_value("tag", Some(" -W9 ")).unwrap();
    c.tag = ecad_model::naming::tag::TagSlot::of_text(c.tag(), ecad_model::naming::tag::TagState::Fixed);
    c.set_value("description", Some("Pump supply")).unwrap();
    c.set_value("length", Some("12 m")).unwrap();
    assert_eq!((c.tag(), c.is_fixed(), c.description()), (Some("-W9"), true, Some("Pump supply")));
    assert_eq!(
        (c.get("tag"), c.tag.state(), c.get("description")),
        (Some("-W9"), Some(ecad_model::naming::tag::TagState::Fixed), Some("Pump supply"))
    );
    assert_eq!(c.properties.keys().map(String::as_str).collect::<Vec<_>>(), ["length"], "only the free key is in the bag");
    c.set_value("description", Some("  ")).unwrap();
    assert_eq!(c.description(), None, "blank clears");
    assert!(c.set_value("tag", Some("a\u{202e}b")).is_err(), "a hidden character is refused");
}

#[test]
fn a_cable_reads_strictly() {
    let good = serde_json::json!({"tag": {"state": "none"}, "description": null, "properties": {}, "cores": [], "shield": "overall+pairs"});
    let c: Cable = serde_json::from_value(good).unwrap();
    assert_eq!(c.shield, Some(Shield::OverallAndPairs));
    assert!(serde_json::to_value(Cable::default()).unwrap().get("shield").is_none(), "no shield is left out");
    for hostile in [serde_json::json!("tartan"), serde_json::json!(3), serde_json::json!(""), serde_json::json!("overall\u{202e}")] {
        let v = serde_json::json!({"tag": {"state": "none"}, "description": null, "properties": {}, "cores": [], "shield": hostile});
        assert!(serde_json::from_value::<Cable>(v).is_err(), "{hostile} must not read");
    }
}
