//! A connection point's protective earth: set from its symbol when the point is made (a PE
//! terminal block, a terminal keyed `PE`), stored on the point, and a Checks row when the symbol
//! showing it now disagrees.

use ecad_model::{ConnectionPointId, DeviceId};
use serde_json::json;

use crate::common::{H, id};

/// The points of device `c` by label, with whether each is a protective earth point.
fn earth(h: &mut H, c: DeviceId) -> Vec<(String, bool)> {
    let doc = h.doc();
    let mut v: Vec<(String, bool)> = doc.connection_points_of(c).map(|(_, p)| (p.label.clone().unwrap_or_default(), p.protective_earth)).collect();
    v.sort();
    v
}

fn mismatches(h: &mut H) -> Vec<String> {
    let all = h.run("checks", json!({"kinds": ["symbol-mismatch"], "limit": 100})).unwrap();
    all["checks"].as_array().cloned().unwrap_or_default().iter().map(|c| c["which"].as_str().unwrap_or_default().to_string()).collect()
}

#[test]
fn a_pe_point_is_stored_from_its_symbol_and_a_symbol_that_disagrees_is_a_check() {
    let mut h = H::new(0x00d4_0502);
    let m: DeviceId = id(&crate::common::place(&mut h, "motor3", (100.0, 100.0)), "component");
    assert_eq!(earth(&mut h, m), [("PE".into(), true), ("U1".into(), false), ("V1".into(), false), ("W1".into(), false)]);
    let x: DeviceId = id(&crate::common::place(&mut h, "terminal_block_pe", (200.0, 100.0)), "component");
    assert!(earth(&mut h, x).iter().all(|(_, pe)| *pe), "{:?}", earth(&mut h, x));
    let k: DeviceId = id(&crate::common::place(&mut h, "terminal_block", (300.0, 100.0)), "component");
    assert!(earth(&mut h, k).iter().all(|(_, pe)| !*pe), "{:?}", earth(&mut h, k));
    assert!(mismatches(&mut h).is_empty(), "{:?}", mismatches(&mut h));
    // Stored data that differs from the symbol (as a file or a merge may hold it) is flagged.
    let doc = h.doc();
    let (u1, before): (ConnectionPointId, _) =
        doc.connection_points_of(m).find(|(_, p)| p.label.as_deref() == Some("U1")).map(|(t, p)| (t, p.clone())).unwrap();
    let after = ecad_model::ConnectionPoint { protective_earth: true, ..before.clone() };
    let ops = vec![
        ecad_model::Op::RemoveConnectionPoint { id: u1, connection_point: before },
        ecad_model::Op::AddConnectionPoint { id: u1, connection_point: after },
    ];
    h.cfg(|p| p.ledger.commit(ecad_model::Author::User, "hand-made", ops)).unwrap();
    let rows = mismatches(&mut h);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].ends_with(":U1 is a protective earth point; its symbol draws none"), "{rows:?}");
}
