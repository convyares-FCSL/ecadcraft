//! Strict `meta` names: a file whose `meta` table holds a name this build does not
//! read opens read-only in the app, and the inspector still shows it: the summary names the meta
//! value, the model views answer, and the row is shown Readable and Raw.

use std::time::Duration;

use ecad_file::schema::META;
use ecad_inspector::source::{FileSource, Source, VerifyState};

use crate::common::*;

#[test]
fn a_file_with_an_unknown_meta_name_is_shown_raw_and_readable() {
    let dir = TempDir::new("strict-meta");
    let path = written(&dir, &history());
    {
        let db = redb::Database::open(&path).unwrap();
        let w = db.begin_write().unwrap();
        w.open_table(META).unwrap().insert("future_name", br#"{"anything": 1}"#.as_slice()).unwrap();
        w.commit().unwrap();
    }
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);

    let summary = ask(s, serde_json::json!({"op": "summary"}));
    assert_eq!(summary["ok"]["meta"]["future_name"]["anything"], 1, "{summary:#}");
    assert!(ask(s, serde_json::json!({"op": "devices"})).get("ok").is_some(), "the model views still answer");

    // The meta row, Readable and Raw.
    let all = ask(s, serde_json::json!({"op": "rows", "table": "meta", "count": 100}));
    let rows = all["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let key = ecad_inspector::text::hex(b"future_name");
    assert!(rows.iter().any(|r| r["key"] == key.as_str()), "{all:#}");
    let readable = ask(s, serde_json::json!({"op": "row", "table": "meta", "key": key, "form": "readable"}));
    assert!(readable["ok"]["decoded"].to_string().contains("anything"), "{readable:#}");
    let raw = ask(s, serde_json::json!({"op": "row", "table": "meta", "key": key, "form": "raw"}));
    assert_eq!(raw["ok"]["hex"], ecad_inspector::text::hex(br#"{"anything": 1}"#), "{raw:#}");
}
