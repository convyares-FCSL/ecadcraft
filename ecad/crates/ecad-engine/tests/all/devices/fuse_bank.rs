//! A fuse bank's ways: stored from its symbol when placed, read from the device afterwards, and a
//! Checks row when the symbol drawn now disagrees.

use ecad_model::DeviceId;
use ecad_model::objects::devices::{DeviceType, Protection};
use serde_json::json;

use crate::common::{H, id};

fn ways(h: &mut H, c: DeviceId) -> Option<u8> {
    match &h.doc().devices()[&c].device_type {
        DeviceType::Protection(Protection::FuseBank(b)) => b.ways,
        _ => None,
    }
}

fn mismatches(h: &mut H) -> Vec<String> {
    let all = h.run("checks", json!({"kinds": ["symbol-mismatch"], "limit": 100})).unwrap();
    all["checks"].as_array().cloned().unwrap_or_default().iter().map(|c| c["which"].as_str().unwrap_or_default().to_string()).collect()
}

#[test]
fn a_fuse_bank_stores_its_ways_and_a_symbol_that_disagrees_is_a_check() {
    let mut h = H::new(0x00d4_0501);
    let f: DeviceId = id(&crate::common::place(&mut h, "fusebank4", (100.0, 100.0)), "component");
    assert_eq!(ways(&mut h, f), Some(4));
    assert!(mismatches(&mut h).is_empty());
    // Outputs are read from the stored ways.
    h.run("setprop", json!({"component": f.hex(), "key": "rating.4", "value": "4 A"})).unwrap();
    assert!(h.run("setprop", json!({"component": f.hex(), "key": "rating.5", "value": "4 A"})).is_err());
    // Stored data that differs from the symbol (as a file or a merge may hold it) is flagged, and
    // the stored ways are used, never re-read from the symbol.
    let before = h.doc().devices()[&f].clone();
    let mut after = before.clone();
    let DeviceType::Protection(Protection::FuseBank(b)) = &mut after.device_type else { panic!("not a fuse bank") };
    b.ways = Some(6);
    h.cfg(|p| p.ledger.commit(ecad_model::Author::User, "hand-made", vec![ecad_model::Op::ModifyDevice { id: f, before, after }])).unwrap();
    let rows = mismatches(&mut h);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].ends_with("has 6 ways; its symbol draws 4"), "{rows:?}");
    h.run("setprop", json!({"component": f.hex(), "key": "rating.6", "value": "4 A"})).unwrap();
    assert_eq!(ways(&mut h, f), Some(6));
}
