//! `cablenew`, `cableedit` and `setprop cores` record one
//! `ModifyCable` (or the `CreateCable`) holding every core the cable offers, unplaced until a line
//! carries it; a slash places the stored core, never a second one of its label. The shield is the
//! cable's field, not a property.

use ecad_model::objects::styles::cable::Shield;
use ecad_model::{CableId, CorePosition, ObjectRef, Op, Purpose, WireId};
use serde_json::json;

use crate::common::{H, id};

fn last_ops(h: &mut H) -> Vec<Op> {
    h.p(|p| p.ledger.log().last().map(|t| t.ops.clone()).unwrap_or_default())
}

fn labels(h: &mut H, k: CableId) -> Vec<(String, CorePosition, Purpose)> {
    h.doc().cables()[&k].cores.iter().map(|m| (m.label_str().unwrap_or_default().to_string(), m.position, m.purpose)).collect()
}

fn unplaced(ls: &[&str]) -> Vec<(String, CorePosition, Purpose)> {
    ls.iter().map(|l| (l.to_string(), CorePosition::Unplaced, Purpose::InUse)).collect()
}

#[test]
fn the_offered_cores_are_stored_by_the_command_that_sets_the_offer() {
    let mut h = H::new(0x72_d2);
    let k: CableId = id(&h.run("cablenew", json!({"cores": "3"})).unwrap(), "cable");
    assert!(matches!(last_ops(&mut h).as_slice(), [Op::CreateCable { cable, .. }] if cable.cores.len() == 3));
    assert_eq!(labels(&mut h, k), unplaced(&["1", "2", "3"]));
    assert!(!ecad_electrical::cables::is_drawn(&h.doc(), k), "stored unplaced cores do not draw a cable");
    h.check();

    // A smaller offer takes away the unplaced core it no longer names: one ModifyCable.
    h.run("cableedit", json!({"cable": k.hex(), "set": {"cores": "2"}})).unwrap();
    let ops = last_ops(&mut h);
    assert!(matches!(ops.as_slice(), [Op::ModifyCable { id, .. }] if *id == k), "{ops:?}");
    assert_eq!(labels(&mut h, k), unplaced(&["1", "2"]));
    // `setprop cores` writes the same way, never a SetProperty of `cores`.
    h.run("setprop", json!({"cable": k.hex(), "key": "cores", "value": "BN,BU"})).unwrap();
    let ops = last_ops(&mut h);
    assert!(ops.iter().all(|o| matches!(o, Op::ModifyCable { .. })), "{ops:?}");
    assert_eq!(labels(&mut h, k), unplaced(&["BN", "BU"]));
    h.run("undo", json!({})).unwrap();
    assert_eq!(labels(&mut h, k), unplaced(&["1", "2"]));
    h.check();
}

#[test]
fn a_slash_places_the_stored_core_of_its_label() {
    let mut h = H::new(0x72_d3);
    let k: CableId = id(&h.run("cablenew", json!({"cores": "2"})).unwrap(), "cable");
    h.points_at(&[(0.0, 0.0), (0.0, 50.0)]);
    let w: WireId = id(&h.run("wire", json!({"points": [[0, 0], [0, 50]]})).unwrap(), "conductor");
    h.run("cableslash", json!({"from": [-5, 25], "to": [5, 25], "cable": k.hex()})).unwrap();
    let got = labels(&mut h, k);
    assert_eq!(got.len(), 2, "no second core 1: {got:?}");
    assert_eq!(got[0], ("1".to_string(), CorePosition::Wire(w), Purpose::InUse));
    assert_eq!(got[1], ("2".to_string(), CorePosition::Unplaced, Purpose::InUse));
    assert!(last_ops(&mut h).iter().any(|o| matches!(o, Op::ModifyCable { id, .. } if *id == k)));
    // Ticking core 2 Not used changes its purpose only; unticking keeps the offered core.
    h.run("setprop", json!({"cable": k.hex(), "key": "unused.2", "value": "yes"})).unwrap();
    assert_eq!(labels(&mut h, k)[1], ("2".to_string(), CorePosition::Unplaced, Purpose::Spare));
    h.run("setprop", json!({"cable": k.hex(), "key": "unused.2", "value": "no"})).unwrap();
    assert_eq!(labels(&mut h, k)[1], ("2".to_string(), CorePosition::Unplaced, Purpose::InUse));
    h.check();
}

#[test]
fn the_shield_is_the_cables_field() {
    let mut h = H::new(0x72_d4);
    let k: CableId = id(&h.run("cablenew", json!({"shield": "overall"})).unwrap(), "cable");
    let c = h.doc().cables()[&k].clone();
    assert_eq!(c.shield, Some(Shield::Overall));
    assert!(!c.properties.contains_key("shield"), "{:?}", c.properties);
    h.run("cableedit", json!({"cable": k.hex(), "set": {"shield": "pairs", "length": 12}})).unwrap();
    let ops = last_ops(&mut h);
    assert!(
        matches!(ops.as_slice(), [Op::ModifyCable { before, after, .. }]
        if before.shield == Some(Shield::Overall) && after.shield == Some(Shield::Pairs)),
        "{ops:?}"
    );
    assert!(!ops.iter().any(|o| matches!(o, Op::SetProperty { object: ObjectRef::Cable(_), .. })));
    // Another field leaves the shield as it is; null clears it.
    h.run("cableedit", json!({"cable": k.hex(), "set": {"length": 14}})).unwrap();
    assert_eq!(h.doc().cables()[&k].shield, Some(Shield::Pairs));
    h.run("cableedit", json!({"cable": k.hex(), "set": {"shield": null}})).unwrap();
    assert_eq!(h.doc().cables()[&k].shield, None);
    // Hostile words are refused and write nothing.
    let head = h.head();
    for bad in [json!("tartan"), json!("overall\u{202e}"), json!("x".repeat(5000))] {
        let e: String = h.run("cableedit", json!({"cable": k.hex(), "set": {"shield": bad}})).unwrap_err();
        assert!(e.contains("Shield is none, overall, pairs or overall+pairs") || e.contains("control or invisible"), "{e}");
        assert_eq!(h.head(), head);
    }
    // Lane D c3: `setprop shield` writes the field, in one ModifyCable; never a property.
    h.run("setprop", json!({"cable": k.hex(), "key": "shield", "value": "overall"})).unwrap();
    assert_eq!(h.doc().cables()[&k].shield, Some(Shield::Overall));
    assert!(!h.doc().cables()[&k].properties.contains_key("shield"));
    let head = h.head();
    assert!(h.run("setprop", json!({"cable": k.hex(), "key": "shield", "value": "tartan"})).is_err());
    assert_eq!(h.head(), head);
    h.check();
}

/// Lane 072 D e4 (design 072 §4.5, overseer ruling): a cable's type is its field
/// `Cable.cable_type`, set by `cablenew`, `cableedit` and `setprop`, never a property; an ID naming
/// no type is kept and is a check row; deleting a type in use is refused naming the cables.
#[test]
fn the_cable_type_is_the_cables_field() {
    let mut h = H::new(0x72_e4);
    let ty = h.run("setstyle", json!({"catalogue": "cable", "new": true, "name": "Synth four", "fields": {"cores": "4G", "size": "1.5"}})).unwrap();
    let t = ty["id"].as_str().unwrap().to_string();
    let k: CableId = id(&h.run("cablenew", json!({"cable_type": "Synth four"})).unwrap(), "cable");
    let c = h.doc().cables()[&k].clone();
    assert_eq!(c.cable_type.as_ref().map(ecad_model::StyleRef::text).as_deref(), Some(t.as_str()));
    assert!(!c.properties.contains_key("cable_type"), "{:?}", c.properties);
    assert_eq!(c.cores.len(), 4, "the type's offer is stored");
    h.check();

    // A second cable through setprop: one ModifyCable writing the field.
    let k2: CableId = id(&h.run("cablenew", json!({})).unwrap(), "cable");
    h.run("setprop", json!({"cable": k2.hex(), "key": "cable_type", "value": t})).unwrap();
    let ops = last_ops(&mut h);
    assert!(matches!(ops.as_slice(), [Op::ModifyCable { after, .. }] if after.cable_type.is_some() && after.cores.len() == 4), "{ops:?}");

    // Deleting the type in use is refused, naming both cables; nothing is written.
    let head = h.head();
    let e: String = h.run("deletestyle", json!({"catalogue": "cable", "id": t})).unwrap_err();
    assert!(e.contains("-W01") && e.contains("-W02"), "{e}");
    assert_eq!(h.head(), head);

    // Cleared by cableedit; another field leaves it.
    h.run("cableedit", json!({"cable": k.hex(), "set": {"length": 10}})).unwrap();
    assert!(h.doc().cables()[&k].cable_type.is_some());
    h.run("cableedit", json!({"cable": k.hex(), "set": {"cable_type": null}})).unwrap();
    assert_eq!(h.doc().cables()[&k].cable_type, None);

    // An ID naming no type is kept and shown as a check row on the cable.
    let gone = "#0123456789abcdef0123456789abcdef";
    h.run("setprop", json!({"cable": k2.hex(), "key": "cable_type", "value": gone})).unwrap();
    let rows = h.run("checks", json!({"kinds": ["style-unknown"], "limit": 100})).unwrap();
    let text = rows.to_string();
    assert!(text.contains("-W02") && text.contains("Cable type"), "{text}");
    // A name that is no type is refused.
    assert!(h.run("setprop", json!({"cable": k2.hex(), "key": "cable_type", "value": "No such"})).is_err());
    h.check();
}
