//! Tester 055 in the file layer (FP1): a `frame` style record's title block body through the op
//! record (`v1::StyleRecord`), a version 2 project file's log and its snapshot chunks, byte-equal
//! after canonical form; a record without a body writes exactly what 050a wrote; hostile bodies
//! (10 000 items, NaN and huge numbers, a 1 MB text, an unknown body tag, an unknown binding
//! source, a body on a `text` record) are read or refused in words, never by a panic. Written from
//! brief 055 and design 055 (§1.2, §1.3, §12.1 B and J, §14 FP1) before the implementation.

use std::path::PathBuf;

use crate::common::{PROJECT, SEED, WRITER};
use ecad_file::v2::{ProjectV2, Store};
use ecad_model::style::{self, Binding, IfEmpty, Rect, Source, StyleBody, StyleLayer, StyleRecord, StyleRef, TextLook, TitleBlockLayout};
use ecad_model::{Author, Ledger, Op};
use serde_json::{Value, json};

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad t055 {name} Schaltpl\u{e4}ne {}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn look(size: f64) -> TextLook {
    TextLook { size, bold: true, align: style::Align::Centre, colour: style::Ink::Red }
}

fn layout() -> TitleBlockLayout {
    let prim = ecad_model::Primitive::Line { a: ecad_model::Pt::new(0.0, 0.0), b: ecad_model::Pt::new(3.0, 1.5) };
    TitleBlockLayout {
        size: (400.0, 40.0),
        items: vec![
            style::Item::Line { a: (0.0, 25.0), b: (400.0, 25.0), weight: style::LineWeight::Thick },
            style::Item::Text { at: Rect { x: 1.0, y: 36.0, w: 20.0, h: 3.0 }, text: "Drawing no. \u{e4}\u{2013}\u{fffd}".into(), look: look(1.8) },
            style::Item::Field {
                at: Rect { x: 1.0, y: 27.0, w: 70.0, h: 8.0 },
                bind: Binding { source: Source::Project, key: "job_no".into() },
                label: None,
                look: look(3.5),
                if_empty: IfEmpty::Text("n/a".into()),
            },
            style::Item::Field {
                at: Rect { x: 75.0, y: 27.0, w: 20.0, h: 8.0 },
                bind: Binding { source: Source::Revision, key: "ecr".into() },
                label: Some("ECR".into()),
                look: look(2.5),
                if_empty: IfEmpty::Missing,
            },
            style::Item::RevTable { at: Rect { x: 200.0, y: 0.0, w: 200.0, h: 25.0 }, rows: 7, header: true, newest: style::Newest::Bottom },
            style::Item::Logo { at: Rect { x: 100.0, y: 26.0, w: 30.0, h: 13.0 }, name: "Logo (synthetic)".into(), prims: vec![prim; 120] },
        ],
    }
}

fn frame_rec(name: &str) -> StyleRecord {
    StyleRecord {
        name: name.into(),
        fields: [("sheet".to_string(), "A3".to_string())].into_iter().collect(),
        body: Some(StyleBody::TitleBlock(layout())),
    }
}

fn framed() -> Ledger {
    let mut l = Ledger::new(SEED);
    l.commit(
        Author::Agent("tester".into()),
        "New title block A3 Northfield",
        vec![Op::SetStyle {
            layer: StyleLayer::Project,
            catalogue: "frame".into(),
            id: StyleRef::Own(0x55),
            before: None,
            after: Some(frame_rec("A3 Northfield")),
        }],
    )
    .unwrap();
    let mut moved = frame_rec("A3 Northfield");
    if let Some(StyleBody::TitleBlock(b)) = &mut moved.body
        && let style::Item::Field { at, .. } = &mut b.items[2]
    {
        at.x += 10.0;
    }
    l.commit(
        Author::User,
        "Move a field",
        vec![Op::SetStyle {
            layer: StyleLayer::Project,
            catalogue: "frame".into(),
            id: StyleRef::Own(0x55),
            before: Some(frame_rec("A3 Northfield")),
            after: Some(moved),
        }],
    )
    .unwrap();
    l
}

/// §12.1 B: every transaction's record reads back as written, and its canonical bytes are stable
/// (written, read, written again: byte-equal).
#[test]
#[ignore = "tester 055: awaiting build"]
fn b_body_records_round_trip_byte_equal_after_canonical_form() {
    let l = framed();
    let prev = [0u8; 32];
    for tx in l.full_log().unwrap() {
        let v = ecad_file::record::tx_value(&tx, &prev).unwrap();
        let back = ecad_file::record::decode_tx(&v).unwrap();
        assert_eq!(back, tx, "{}", tx.label);
        let again = ecad_file::record::tx_value(&back, &prev).unwrap();
        assert_eq!(
            ecad_file::canonical::canonical(&v).unwrap(),
            ecad_file::canonical::canonical(&again).unwrap(),
            "{}: canonical bytes equal",
            tx.label
        );
        assert!(v.to_string().contains("job_no"), "the body is in the record");
    }
}

/// §12.1 B: a version 2 project file reopens with the same document and log, by snapshot and by
/// log replay; verify passes.
#[test]
#[ignore = "tester 055: awaiting build"]
fn b_body_round_trips_through_a_project_file_and_its_snapshots() {
    let dir = TempDir::new("roundtrip");
    let ledger = framed();
    for (name, every) in [("Projekt snap.ecad", 1), ("Projekt log.ecad", 1_000_000)] {
        let p = dir.path(name);
        let proj = ProjectV2::create(Store::create(&p).unwrap(), PROJECT, &ledger, None, None, Some(1), WRITER, every).unwrap().0;
        drop(proj);
        let o = ProjectV2::open(Store::open(&p, true).unwrap(), SEED ^ 1, WRITER).unwrap();
        assert_eq!(o.ledger.document(), ledger.document(), "{name}");
        assert_eq!(o.ledger.full_log().unwrap(), ledger.full_log().unwrap(), "{name}");
        let v = ecad_file::v2::verify::verify(o.project.store());
        assert!(v.ok, "{name}: {:#?}", v.lines);
    }
}

/// FP1: a record without a body writes the same JSON 050a wrote (no `body` key), so older files
/// and older readers are unaffected.
#[test]
#[ignore = "tester 055: awaiting build"]
fn fp1_a_record_without_a_body_writes_what_050a_wrote() {
    let rec = StyleRecord { name: "Heavy".into(), fields: [("border_weight".to_string(), "1".to_string())].into_iter().collect(), body: None };
    let v = serde_json::to_value(ecad_file::v1::StyleRecord::from(&rec)).unwrap();
    assert_eq!(v, json!({"name": "Heavy", "fields": {"border_weight": "1"}}));
    let back: ecad_file::v1::StyleRecord = serde_json::from_value(v).unwrap();
    assert_eq!(StyleRecord::from(back), rec);
}

/// The record JSON of the framed ledger's first transaction, and the path to its body.
fn body_record() -> Value {
    let tx = framed().full_log().unwrap().into_iter().next().unwrap();
    ecad_file::record::tx_value(&tx, &[0u8; 32]).unwrap()
}

fn body_mut(v: &mut Value) -> &mut Value {
    let rec = &mut v["ops"][0]["after"];
    assert!(!rec["body"].is_null(), "the record carries a body: {rec}");
    &mut rec["body"]
}

/// The items array inside a body JSON, whatever the tag's spelling.
fn items_mut(body: &mut Value) -> &mut Vec<Value> {
    if body["items"].is_array() {
        return body["items"].as_array_mut().unwrap();
    }
    let key = body.as_object().and_then(|m| m.keys().next().cloned()).expect("a tagged body");
    body[key]["items"].as_array_mut().expect("items")
}

/// §12.1 J, §1.3: hostile bodies are read (the drawing drops the excess and Checks counts it) or
/// refused in words; none panics, and anything read writes back and reads again.
#[test]
#[ignore = "tester 055: awaiting build"]
fn j_hostile_bodies_read_or_refuse_in_words_never_panic() {
    type Edit = fn(&mut Value);
    let cases: [(&str, Edit); 8] = [
        ("10 000 items", |b| {
            let items = items_mut(b);
            let one = items[0].clone();
            *items = std::iter::repeat_n(one, 10_000).collect();
        }),
        ("a 1 MB text", |b| {
            let s = serde_json::to_string(&items_mut(b)[1]).unwrap().replacen("Drawing no.", &"x".repeat(1 << 20), 1);
            items_mut(b)[1] = serde_json::from_str(&s).unwrap();
        }),
        ("huge numbers", |b| {
            let s = serde_json::to_string(b).unwrap().replace("400.0", "1e308").replace("40.0", "-1e308");
            *b = serde_json::from_str(&s).unwrap();
        }),
        ("strings for numbers (NaN)", |b| {
            let s = serde_json::to_string(b).unwrap().replacen("1.0", "\"NaN\"", 1);
            *b = serde_json::from_str(&s).unwrap();
        }),
        ("an unknown item kind", |b| items_mut(b).push(json!({"Hologram": {"at": {"x": 0, "y": 0, "w": 1, "h": 1}}}))),
        ("an unknown binding source", |b| {
            let s = serde_json::to_string(b).unwrap().replacen("\"Revision\"", "\"Weather\"", 1).replacen("\"revision\"", "\"weather\"", 1);
            *b = serde_json::from_str(&s).unwrap();
        }),
        ("an unknown body tag", |b| *b = json!({"pattern_sheet": {"stripes": 3}})),
        ("a deeply nested body", |b| {
            let mut v = json!(1);
            for _ in 0..10_000 {
                v = json!([v]);
            }
            items_mut(b).push(v);
        }),
    ];
    for (what, edit) in cases {
        let mut v = body_record();
        edit(body_mut(&mut v));
        let r = std::panic::catch_unwind(|| ecad_file::record::decode_tx(&v));
        let Ok(decoded) = r else { panic!("{what}: decoding panicked") };
        match decoded {
            Ok(tx) => {
                // Read: it writes back and reads again equal.
                let again = ecad_file::record::tx_value(&tx, &[0u8; 32]).unwrap_or_else(|e| panic!("{what}: re-encode: {e}"));
                assert_eq!(ecad_file::record::decode_tx(&again).unwrap(), tx, "{what}");
            }
            Err(e) => assert!(!format!("{e:?}").is_empty(), "{what}: refused in words"),
        }
    }
}

/// §12.1 J: a body on a `text` record (a hostile file) is refused in words or read and left for
/// the drawing to ignore; never a panic.
#[test]
#[ignore = "tester 055: awaiting build"]
fn j_a_body_on_a_text_record_never_panics() {
    let mut v = body_record();
    v["ops"][0]["catalogue"] = json!("text");
    v["ops"][0]["id"] = json!("tag");
    let r = std::panic::catch_unwind(|| ecad_file::record::decode_tx(&v));
    assert!(r.is_ok(), "decoding a body on a text record never panics");
}

/// The record limits (`check_limits`) cap a body before it is decoded: a 50 MB record is refused.
#[test]
#[ignore = "tester 055: awaiting build"]
fn j_an_oversized_record_is_refused_by_the_limits() {
    let mut v = body_record();
    items_mut(body_mut(&mut v)).push(json!({"Text": {"text": "x".repeat(50 << 20)}}));
    assert!(ecad_file::record::check_limits(&v).is_err() || ecad_file::record::decode_tx(&v).is_err(), "a 50 MB record is refused");
}
