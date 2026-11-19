//! Lane 072 D, parts c2 and c3 (design 072 §4.5): a `ModifyCable` (the cable before and after,
//! its shield, cores, rating inputs and design current) is shown in the Readable view of its transaction, the Raw view is the stored bytes,
//! and the cables table shows the shield.

use std::time::Duration;

use ecad_inspector::source::{FileSource, Source, VerifyState};
use ecad_model::style::cable::Shield;
use ecad_model::{Author, Cable, CableId, Core, CoreLabel, IdAllocator, Op, Purpose};

use crate::common::*;

#[test]
fn a_modify_cable_is_shown_raw_and_readable_and_the_shield_on_the_cable_row() {
    let dir = TempDir::new("072d-modify-cable");
    let mut l = history();
    let mut ids = IdAllocator::seeded(76);
    let k: CableId = ids.fresh();
    let made = Cable { cores: vec![Core::unplaced(CoreLabel::new("1").ok())], ..Cable::default() };
    l.commit(Author::User, "new", vec![Op::CreateCable { id: k, cable: made.clone() }]).unwrap();
    let mut after = made.clone();
    after.shield = Some(Shield::OverallAndPairs);
    after.rating.basis = Some(ecad_model::wire::Basis::Manual);
    after.load = Some(ecad_model::units::Amps(4.0));
    after.cores.push(Core { purpose: Purpose::Spare, ..Core::unplaced(CoreLabel::new("GNYE").ok()) });
    l.commit(Author::User, "edit", Op::modify_cable(k, &made, after).into_iter().collect()).unwrap();
    let path = written(&dir, &l);
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let rows = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = rows["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let key = rows.last().and_then(|r| r["key"].as_str().map(str::to_string)).unwrap();
    let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
    let text = readable["ok"]["decoded"].to_string();
    for want in ["ModifyCable", "before", "after", "shield", "overall+pairs", "GNYE", "spare", "unplaced", "rating", "manual", "load"] {
        assert!(text.contains(want), "{want} in the readable records: {text}");
    }
    let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
    assert!(raw["ok"]["hex"].as_str().is_some_and(|h| !h.is_empty()), "the raw row");
    let cables = ask(s, serde_json::json!({"op": "cables"}));
    assert_eq!(cables["ok"]["rows"][0]["shield"], "overall+pairs", "{cables}");
}

/// Lane 072 D step d: a cable's `protected_by` (a device and its position) is in the Readable view
/// of the `ModifyCable` that sets it, and on the cable row; one naming no device says so.
#[test]
fn a_protected_by_is_shown_readable_and_on_the_cable_row() {
    let dir = TempDir::new("072d-protected-by");
    let mut l = history();
    let mut ids = IdAllocator::seeded(77);
    let (k, gone): (CableId, ecad_model::DeviceId) = (ids.fresh(), ids.fresh());
    let made = Cable::default();
    l.commit(Author::User, "new", vec![Op::CreateCable { id: k, cable: made.clone() }]).unwrap();
    let after = Cable { protected_by: Some(ecad_model::wire::ProtectedBy { device: gone, position: Some(3) }), ..made.clone() };
    l.commit(Author::User, "protect", Op::modify_cable(k, &made, after).into_iter().collect()).unwrap();
    let path = written(&dir, &l);
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let rows = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = rows["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let key = rows.last().and_then(|r| r["key"].as_str().map(str::to_string)).unwrap();
    let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
    let text = readable["ok"]["decoded"].to_string();
    for want in ["ModifyCable", "protected_by", "position", gone.hex().as_str()] {
        assert!(text.contains(want), "{want} in the readable records: {text}");
    }
    let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
    assert!(raw["ok"]["hex"].as_str().is_some_and(|h| !h.is_empty()), "the raw row");
    let cables = ask(s, serde_json::json!({"op": "cables"}));
    let shown = cables["ok"]["rows"][0]["protected_by"].as_str().unwrap_or_default().to_string();
    assert!(shown.contains("not in the project") && shown.ends_with(":3"), "{cables}");
}

/// Lane 072 D e4/e5: a cable's `cable_type` (a style reference) is in the Readable view of the
/// `ModifyCable` that sets it, the Raw view is the stored bytes, and the cable row shows it (a
/// wire's `wire_type`: `lane_072d`'s ModifyWire test).
#[test]
fn a_cable_type_is_shown_raw_and_readable_and_on_the_cable_row() {
    let dir = TempDir::new("072d-types");
    let mut l = history();
    let mut ids = IdAllocator::seeded(78);
    let k: CableId = ids.fresh();
    let ty = "#0123456789abcdef0123456789abcdef";
    let made = Cable::default();
    l.commit(Author::User, "new", vec![Op::CreateCable { id: k, cable: made.clone() }]).unwrap();
    let after = Cable { cable_type: ecad_model::StyleRef::parse(ty), ..made.clone() };
    l.commit(Author::User, "type", Op::modify_cable(k, &made, after).into_iter().collect()).unwrap();
    let path = written(&dir, &l);
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let rows = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = rows["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let key = rows.last().and_then(|r| r["key"].as_str().map(str::to_string)).unwrap();
    let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
    let text = readable["ok"]["decoded"].to_string();
    for want in ["ModifyCable", "cable_type", &ty[1..]] {
        assert!(text.contains(want), "{want} in the readable records: {text}");
    }
    let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
    assert!(raw["ok"]["hex"].as_str().is_some_and(|h| !h.is_empty()), "the raw row");
    let cables = ask(s, serde_json::json!({"op": "cables"}));
    assert!(cables["ok"]["rows"][0]["cable_type"].as_str().is_some_and(|t| t.contains(&ty[1..])), "{cables}");
}
