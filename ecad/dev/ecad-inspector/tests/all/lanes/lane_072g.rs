//! Lane 072 G (design 072 §8, acceptance A15): a typed wire-type record and its `SetStyle` op are
//! shown in the inspector's Readable and Raw views.

use std::time::Duration;

use ecad_inspector::source::{FileSource, Source, VerifyState};
use ecad_model::style::wire::Insulation;
use ecad_model::style::{StyleBody, WireType};
use ecad_model::wire::{Colours, WireSize};
use ecad_model::{Author, Op, StyleCatalogue, StyleLayer, StyleRecord, StyleRef};

use crate::common::*;

#[test]
fn a_wire_type_record_is_shown_raw_and_readable() {
    let dir = TempDir::new("072g-wire-type");
    let mut l = history();
    let w = WireType { size: WireSize::parse("6"), colours: Colours::parse("GN/YE"), insulation: Some(Insulation::Xlpe), ..WireType::default() };
    let op = Op::SetStyle {
        layer: StyleLayer::Project,
        catalogue: StyleCatalogue::Wire,
        id: StyleRef::Own(0x72),
        before: None,
        after: Some(StyleRecord::new("Earth 6", StyleBody::Wire(w))),
    };
    l.commit(Author::User, "wire type", vec![op]).unwrap();
    let path = written(&dir, &l);

    // Readable: the CLI log names the op and the typed fields.
    let log = run(&[path.to_str().unwrap(), "log", "main", "--last", "1"]).unwrap();
    for want in ["SetStyle", "Earth 6", "GN/YE", "xlpe"] {
        assert!(log.contains(want), "{want} in the readable log: {log}");
    }
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);

    // Readable and Raw rows of the transaction.
    let all = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = all["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let mut found = None;
    for r in &rows {
        let key = r["key"].as_str().unwrap().to_string();
        let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
        let text = readable["ok"]["decoded"].to_string();
        if text.contains("SetStyle") && text.contains("xlpe") {
            found = Some(key);
        }
    }
    let key = found.expect("a transaction with the typed SetStyle");
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
