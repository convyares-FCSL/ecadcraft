//! Lane 072 A1 (design 072 §9.5, acceptance A15): the inspector still shows a project that ECAD
//! opens read-only for an unknown word, with the offending record readable.

use ecad_model::{Author, ObjectRef, Op};

use crate::common::*;

#[test]
fn a_project_with_an_unknown_word_is_still_shown() {
    let dir = TempDir::new("072a-words");
    let mut l = history();
    let op = Op::SetProperty { object: ObjectRef::Project, key: "text.tag.show".into(), before: None, after: Some("maybe".into()) };
    l.commit(Author::User, "a file made elsewhere", vec![op]).unwrap();
    let path = written(&dir, &l);
    let o = path.to_str().unwrap();
    let tables = run(&[o, "tables"]).unwrap();
    assert!(!tables.is_empty());
    let log = run(&[o, "log", "main", "--last", "1"]).unwrap();
    assert!(log.contains("text.tag.show") && log.contains("maybe"), "the record is readable: {log}");
    let verify = run(&[o, "verify"]).unwrap();
    assert!(!verify.is_empty());
}

/// Lane 072 A2 (design 072 F5): a project's tag prefixes, keyed by `starter:`/`project:`, and its
/// cable prefix are shown in the Readable view of their transaction and are the stored bytes in
/// the Raw view.
#[test]
fn tag_prefixes_by_symbol_and_the_cable_prefix_are_shown_raw_and_readable() {
    use std::time::Duration;

    use ecad_inspector::source::{FileSource, Source, VerifyState};
    use ecad_model::prefix::{PrefixOf, TagPrefix};

    let dir = TempDir::new("072a2-prefixes");
    let mut l = history();
    let before = l.document().settings.clone();
    let mut after = before.clone();
    let px = |t: &str| TagPrefix::typed(t).unwrap();
    after.tag_prefixes.insert(PrefixOf::Starter("coil".into()), px("KA"));
    after.tag_prefixes.insert(PrefixOf::Project("valve-x".into()), px("YV"));
    after.cable_prefix = Some(px("WC"));
    l.commit(Author::User, "prefixes", vec![Op::SetProjectSettings { before, after }]).unwrap();
    let path = written(&dir, &l);

    // Readable: the CLI log and the protocol's decoded record name the keys and values.
    let log = run(&[path.to_str().unwrap(), "log", "main", "--last", "1"]).unwrap();
    for want in ["starter:coil", "project:valve-x", "cable_prefix", "WC"] {
        assert!(log.contains(want), "{want} in the readable log: {log}");
    }
    let f = FileSource::from_path(&path);
    assert!(matches!(f.wait_verified(Duration::from_secs(60)), VerifyState::Done { .. }));
    let s = Some(&f as &dyn Source);
    let last = ask(s, serde_json::json!({"op": "rows", "table": "transactions", "count": 1000}));
    let rows = last["ok"]["rows"].as_array().cloned().unwrap_or_default();
    let key = rows.last().and_then(|r| r["key"].as_str()).unwrap().to_string();
    let readable = ask(s, serde_json::json!({"op": "row", "table": "transactions", "key": key, "form": "readable"}));
    let text = readable["ok"]["decoded"].to_string();
    assert!(text.contains("starter:coil") && text.contains("project:valve-x") && text.contains("cable_prefix"), "{text}");
    // Raw: exactly the stored bytes of that row.
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
