//! Lane 072 D (design 072 §4.2, acceptance A15): a wire's typed values and the `ModifyWire` op that
//! changes them are shown in the Readable view of their transaction, and the Raw view is the stored
//! bytes.

use std::time::Duration;

use ecad_inspector::source::{FileSource, Source, VerifyState};
use ecad_model::text::Text;
use ecad_model::units::Amps;
use ecad_model::wire::{Basis, Colours, WireNumber, WireSize};
use ecad_model::{Author, IdAllocator, Op, Wire, WireId};

use crate::common::*;

#[test]
fn a_wires_values_and_its_modify_op_are_shown_raw_and_readable() {
    let dir = TempDir::new("072d-wire");
    let mut l = history();
    let mut ids = IdAllocator::seeded(72);
    let w: WireId = ids.fresh();
    let before =
        Wire { number: Some(WireNumber { text: Text::new("24V-07").unwrap(), fixed: true }), potential: Text::new("24V").ok(), ..Wire::default() };
    l.commit(Author::User, "wire", vec![Op::CreateWire { id: w, conductor: before.clone() }]).unwrap();
    let mut after = before.clone();
    after.size = WireSize::parse("2.5");
    after.colours = Colours::parse("GNYE");
    after.rating.basis = Some(Basis::Manual);
    after.rating.manual = Some(Amps(16.0));
    after.load = Some(Amps(4.0));
    l.commit(Author::User, "values", vec![Op::ModifyWire { id: w, before, after }]).unwrap();
    let path = written(&dir, &l);

    let log = run(&[path.to_str().unwrap(), "log", "main", "--last", "1"]).unwrap();
    for want in ["ModifyWire", "24V-07", "GN/YE", "2.5", "manual"] {
        assert!(log.contains(want), "{want} in the readable log: {log}");
    }
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let rows = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = rows["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let key = rows.last().and_then(|r| r["key"].as_str()).unwrap().to_string();
    let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
    let text = readable["ok"]["decoded"].to_string();
    for want in ["ModifyWire", "colours", "GN/YE", "size", "rating", "load"] {
        assert!(text.contains(want), "{want} in the readable record: {text}");
    }
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
