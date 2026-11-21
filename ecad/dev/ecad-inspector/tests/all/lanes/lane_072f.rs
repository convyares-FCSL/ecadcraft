//! Lane 072 F (design 072 §6.2, acceptance A15): drawing aids and the ops that change them are
//! shown in the Readable view of their transactions, and the Raw view is the stored bytes.

use std::time::Duration;

use ecad_inspector::source::{FileSource, Source, VerifyState};
use ecad_model::aid::{Caption, ContinuationName};
use ecad_model::units::Celsius;
use ecad_model::{Aid, AidId, Author, IdAllocator, LocationBox, Op};

use crate::common::*;

#[test]
fn aids_and_their_ops_are_shown_raw_and_readable() {
    let dir = TempDir::new("072f-aid");
    let mut l = history();
    let mut ids = IdAllocator::seeded(0x72f);
    let (a, b): (AidId, AidId) = (ids.fresh(), ids.fresh());
    let arrow = Aid::Continuation { name: Some(ContinuationName::new("24V").unwrap()), caption: None };
    let boxed = Aid::Box(LocationBox { panel_temperature: Some(Celsius(35.0)), ..LocationBox::default() });
    l.commit(Author::User, "aids", vec![Op::CreateAid { id: a, aid: arrow.clone() }, Op::CreateAid { id: b, aid: boxed }]).unwrap();
    let captioned = Aid::Continuation { name: Some(ContinuationName::new("24V").unwrap()), caption: Some(Caption::new("To the PSU").unwrap()) };
    l.commit(Author::User, "caption", vec![Op::ModifyAid { id: a, before: arrow, after: captioned }]).unwrap();
    let path = written(&dir, &l);

    let log = run(&[path.to_str().unwrap(), "log", "main", "--last", "2"]).unwrap();
    for want in ["CreateAid", "ModifyAid", "24V", "To the PSU"] {
        assert!(log.contains(want), "{want} in the readable log: {log}");
    }
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let rows = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = rows["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let keys: Vec<String> = rows.iter().rev().take(2).filter_map(|r| r["key"].as_str().map(str::to_string)).collect();
    let mut text = String::new();
    for key in &keys {
        let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
        text.push_str(&readable["ok"]["decoded"].to_string());
        let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
        assert!(raw["ok"]["hex"].as_str().is_some_and(|h| !h.is_empty()), "the raw row of {key}");
    }
    for want in ["CreateAid", "ModifyAid", "continuation", "box", "panel_temperature", "caption"] {
        assert!(text.contains(want), "{want} in the readable records: {text}");
    }
    // The snapshot (with the aids in their own `model:aid:` chunks) reads back.
    let summary = ask(s, serde_json::json!({"op": "summary"}));
    assert!(summary.get("ok").is_some(), "{summary}");
}

/// Lane F F1b: a connection point says what owns it, a device or a drawing aid, in both views.
#[test]
fn a_points_owner_is_shown_raw_and_readable() {
    use ecad_model::{Device, DeviceId, Terminal, TerminalId};
    let dir = TempDir::new("072f-owner");
    let mut l = history();
    let mut ids = IdAllocator::seeded(0x72f1);
    let (a, d): (AidId, DeviceId) = (ids.fresh(), ids.fresh());
    let (ta, td): (TerminalId, TerminalId) = (ids.fresh(), ids.fresh());
    let arrow = Aid::Continuation { name: Some(ContinuationName::new("24V").unwrap()), caption: None };
    let point = |owner: ecad_model::TerminalOwner| Terminal { owner, label: None, key: None, accepts: Default::default() };
    l.commit(
        Author::User,
        "points",
        vec![
            Op::CreateAid { id: a, aid: arrow },
            Op::AddTerminal { id: ta, terminal: point(a.into()) },
            Op::CreateDevice { id: d, device: Device::default() },
            Op::AddTerminal { id: td, terminal: point(d.into()) },
        ],
    )
    .unwrap();
    let path = written(&dir, &l);

    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let rows = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = rows["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let key = rows.last().and_then(|r| r["key"].as_str()).map(str::to_string).expect("the last transaction");
    let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
    let text = readable["ok"]["decoded"].to_string();
    for want in ["AddTerminal", "owner", "aid", "device", &a.hex(), &d.hex()] {
        assert!(text.contains(want), "{want} in the readable record: {text}");
    }
    let raw = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "raw"}));
    assert!(raw["ok"]["hex"].as_str().is_some_and(|h| !h.is_empty()), "the raw row");
    let summary = ask(s, serde_json::json!({"op": "summary"}));
    assert!(summary.get("ok").is_some(), "{summary}");
}
