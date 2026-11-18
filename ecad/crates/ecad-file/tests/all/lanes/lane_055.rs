//! Brief 055 (FP1): a frame record's title block body through the op records, a project file's
//! log and its snapshot chunks; a hostile or newer body still opens.

use std::path::PathBuf;

use crate::common::{PROJECT, SEED, WRITER};
use ecad_file::{ProjectV2, Store};
use ecad_model::style::{StyleLayer, StyleRecord, StyleRef};
use ecad_model::title_block::{Binding, BlockItem, IfEmpty, Ink, Newest, Rect, Source, StyleBody, TextLook, TitleBlockLayout};
use ecad_model::{Author, Ledger, Op, Primitive, Pt};

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-055-{name}-{}", ecad_store::disk::unique()));
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

fn layout() -> TitleBlockLayout {
    let mut red = TextLook { size: 2.5, bold: true, ..TextLook::default() };
    red.colour = Ink::Red;
    TitleBlockLayout {
        size: (180.0, 40.0),
        items: vec![
            BlockItem::Line { a: (0.0, 20.0), b: (180.0, 20.0), weight: Default::default() },
            BlockItem::Text { at: Rect::new(1.0, 1.0, 40.0, 5.0), text: "IF IN DOUBT, ASK".into(), look: TextLook::default() },
            BlockItem::Field {
                at: Rect::new(60.0, 21.0, 60.0, 12.5),
                bind: Binding::new(Source::Project, "status"),
                label: Some("Status".into()),
                look: red,
                if_empty: IfEmpty::Text("n/a".into()),
            },
            BlockItem::RevTable { at: Rect::new(0.0, 0.0, 60.0, 20.0), rows: 4, header: true, newest: Newest::Bottom },
            BlockItem::Logo {
                at: Rect::new(130.0, 22.0, 45.0, 15.0),
                name: "logo.dxf".into(),
                prims: vec![
                    Primitive::Circle { center: Pt::new(0.5, 0.25), radius: 3.125 },
                    Primitive::Line { a: Pt::new(0.0, 0.0), b: Pt::new(1.0, 1.0) },
                ],
            },
        ],
    }
}

fn ledger() -> Ledger {
    let mut l = Ledger::new(SEED);
    let mut rec = StyleRecord::new("A3 Northfield", [("sheet".to_string(), "A3".to_string())].into());
    rec.body = Some(StyleBody::title_block(layout()));
    l.commit(
        Author::Agent("tester".into()),
        "New title block",
        vec![Op::SetStyle {
            layer: StyleLayer::Project,
            catalogue: ecad_model::StyleCatalogue::Frame,
            id: StyleRef::Own(0x55),
            before: None,
            after: Some(rec.clone()),
        }],
    )
    .unwrap();
    let mut moved = rec.clone();
    if let Some(BlockItem::Field { at, .. }) = moved.body.as_mut().and_then(|b| b.title_block.as_mut()).and_then(|l| l.items.get_mut(2)) {
        at.x += 10.0;
    }
    l.commit(
        Author::User,
        "Title block A3 Northfield",
        vec![Op::SetStyle {
            layer: StyleLayer::Project,
            catalogue: ecad_model::StyleCatalogue::Frame,
            id: StyleRef::Own(0x55),
            before: Some(rec),
            after: Some(moved),
        }],
    )
    .unwrap();
    l
}

#[test]
fn a_title_block_round_trips_through_a_project_file_and_its_snapshots() {
    let dir = TempDir::new("roundtrip");
    let ledger = ledger();
    for (name, every) in [("snap.ecad", 16), ("log.ecad", 1_000_000)] {
        let p = dir.path(name);
        let store = Store::create(&p).unwrap();
        let proj = ProjectV2::create(store, PROJECT, &ledger, Some(1), WRITER, every).unwrap().0;
        drop(proj);
        let o = ProjectV2::open(Store::open(&p, true).unwrap(), SEED ^ 1, WRITER).unwrap();
        assert_eq!(o.ledger.document(), ledger.document(), "{name}");
        assert_eq!(o.ledger.full_log().unwrap(), ledger.full_log().unwrap(), "{name}: the log reads back the same ops");
        let v = ecad_file::verify::verify(o.project.store());
        assert!(v.ok, "{name}: {:#?}", v.lines);
    }
}

/// The op record (v1's encoding) carries the body; a record without one is unchanged from 050a.
#[test]
fn the_op_record_carries_the_body_and_reads_it_back() {
    let l = ledger();
    let tx = l.full_log().unwrap().into_iter().next().unwrap();
    let v = ecad_file::record::tx_value(&tx, &[0u8; 32]).unwrap();
    assert!(v["ops"][0]["after"]["body"]["title_block"]["items"].is_array(), "{v}");
    assert_eq!(ecad_file::record::decode_tx(&v).unwrap().ops, tx.ops);
    let plain = Op::SetStyle {
        layer: StyleLayer::Project,
        catalogue: ecad_model::StyleCatalogue::Text,
        id: StyleRef::Own(1),
        before: None,
        after: Some(StyleRecord::new("Tag", Default::default())),
    };
    let mut t = tx.clone();
    t.ops = vec![plain];
    let v = ecad_file::record::tx_value(&t, &[0u8; 32]).unwrap();
    assert!(v["ops"][0]["after"].get("body").is_none(), "absent bodies are left out: {v}");
}

/// A newer file: an unknown body kind is kept as read and written back; an unknown item kind and
/// an unknown binding source read; a huge integer in an unknown body still writes canonically.
#[test]
fn a_newer_body_opens_and_keeps_what_it_does_not_know() {
    let l = ledger();
    let tx = l.full_log().unwrap().into_iter().next().unwrap();
    let v0 = ecad_file::record::tx_value(&tx, &[0u8; 32]).unwrap();
    // Design 072 §5.4: an unknown item kind, an unknown binding source or a revision binding
    // naming no revision field inside a title block does not read (the project opens read-only).
    let items = |extra: serde_json::Value| {
        let mut v = v0.clone();
        v["ops"][0]["after"]["body"]["title_block"]["items"].as_array_mut().unwrap().push(extra);
        v
    };
    let field = |bind: &str| serde_json::json!({"kind": "field", "at": {"x": 1, "y": 1, "w": 10, "h": 5}, "bind": bind});
    for (bad, word) in [
        (items(serde_json::json!({"kind": "hologram", "glow": 3})), "hologram"),
        (items(field("weather:today")), "weather"),
        (items(field("revision:Not A Field")), "Not A Field"),
    ] {
        let e = ecad_file::record::decode_tx(&bad).unwrap_err();
        assert!(format!("{e:?}").contains(word), "{word}: {e:?}");
    }
    // A body kind this build does not know is still kept (lane G types the bodies).
    let mut v = v0.clone();
    v["ops"][0]["after"]["body"]["stamp"] = serde_json::json!({"text": "DRAFT", "angle": 45, "n": 9007199254740993u64, "deep": [[[1]]]});
    let t = ecad_file::record::decode_tx(&v).unwrap();
    let Op::SetStyle { after: Some(r), .. } = &t.ops[0] else { panic!("{:?}", t.ops) };
    let body = r.body.as_ref().unwrap();
    assert!(body.other.contains_key("stamp"), "the unknown kind is kept");
    assert!(body.title_block.as_ref().unwrap().bindings().all(|b| b.source != Source::Revision || b.revision_column().is_some()));
    let _ = BlockItem::Line { a: (0.0, 0.0), b: (1.0, 1.0), weight: Default::default() };
    // It writes back (canonical form) and reads again.
    let back = ecad_file::record::tx_value(&t, &[0u8; 32]).unwrap();
    assert_eq!(back["ops"][0]["after"]["body"]["stamp"]["text"], "DRAFT");
    assert!(ecad_file::record::decode_tx(&back).is_ok());
}

/// Hostile bodies are refused in words or read, never a panic: wrong shapes, deep nesting.
#[test]
fn hostile_bodies_are_refused_in_words() {
    let l = ledger();
    let tx = l.full_log().unwrap().into_iter().next().unwrap();
    let good = ecad_file::record::tx_value(&tx, &[0u8; 32]).unwrap();
    let mut deep = serde_json::json!(1);
    for _ in 0..100 {
        deep = serde_json::json!([deep]);
    }
    for bad in [serde_json::json!("x"), serde_json::json!({"title_block": {"size": "big"}}), serde_json::json!({"stamp": deep})] {
        let mut v = good.clone();
        v["ops"][0]["after"]["body"] = bad.clone();
        assert!(ecad_file::record::decode_tx(&v).is_err(), "{bad}");
    }
}
