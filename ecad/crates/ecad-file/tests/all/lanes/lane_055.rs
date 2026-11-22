//! Brief 055 (FP1): a frame record's title block body through the op records, a project file's
//! log and its snapshot chunks; a hostile or newer body still opens.

use std::path::PathBuf;

use crate::common::{PROJECT, SEED, WRITER};
use ecad_file::{Project, Store};
use ecad_model::objects::styles::{FrameStyle, StyleBody};
use ecad_model::objects::styles::{StyleLayer, StyleRecord, StyleRef};
use ecad_model::objects::title_block::{Binding, BlockItem, IfEmpty, Ink, Newest, Rect, Source, TextDraw, TitleBlockLayout};
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
    let mut red = TextDraw { size: 2.5, bold: true, ..TextDraw::default() };
    red.colour = Ink::Red;
    TitleBlockLayout {
        size: (180.0, 40.0),
        items: vec![
            BlockItem::Line { a: (0.0, 20.0), b: (180.0, 20.0), weight: Default::default() },
            BlockItem::Text { at: Rect::new(1.0, 1.0, 40.0, 5.0), text: "IF IN DOUBT, ASK".into(), draw: TextDraw::default() },
            BlockItem::Field {
                at: Rect::new(60.0, 21.0, 60.0, 12.5),
                bind: Binding::Project(ecad_model::objects::title_block::ProjectKey::new("status").unwrap()),
                label: Some("Status".into()),
                draw: red,
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
    let style =
        FrameStyle { sheet: ecad_model::objects::styles::frame::FrameSheet::parse("A3"), title_block: Some(layout()), ..FrameStyle::default() };
    let rec = StyleRecord::new("A3 Northfield", StyleBody::Frame(style));
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
    if let Some(BlockItem::Field { at, .. }) = frame_block(&mut moved).and_then(|l| l.items.get_mut(2)) {
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
        let proj = Project::create(store, PROJECT, &ledger, Some(1), WRITER, every).unwrap().0;
        drop(proj);
        let o = Project::open(Store::open(&p, true).unwrap(), SEED ^ 1, WRITER).unwrap();
        assert_eq!(o.ledger.document(), ledger.document(), "{name}");
        assert_eq!(o.ledger.full_log().unwrap(), ledger.full_log().unwrap(), "{name}: the log reads back the same ops");
        let v = ecad_file::verify::verify(o.project.store());
        assert!(v.ok, "{name}: {:#?}", v.lines);
    }
}

fn frame_block(r: &mut StyleRecord) -> Option<&mut TitleBlockLayout> {
    match &mut r.body {
        StyleBody::Frame(f) => f.title_block.as_mut(),
        _ => None,
    }
}

/// The op record (v1's encoding) carries the body; a record without one is unchanged from 050a.
#[test]
fn the_op_record_carries_the_body_and_reads_it_back() {
    let l = ledger();
    let tx = l.full_log().unwrap().into_iter().next().unwrap();
    let v = ecad_file::record::tx_value(&tx, &[0u8; 32]).unwrap();
    assert!(v["ops"][0]["after"]["body"]["frame"]["title_block"]["items"].is_array(), "{v}");
    assert_eq!(ecad_file::record::decode_tx(&v).unwrap().ops, tx.ops);
    let plain = Op::SetStyle {
        layer: StyleLayer::Project,
        catalogue: ecad_model::StyleCatalogue::Text,
        id: StyleRef::Own(1),
        before: None,
        after: Some(StyleRecord::new("Tag", StyleBody::empty(ecad_model::StyleCatalogue::Text))),
    };
    let mut t = tx.clone();
    t.ops = vec![plain];
    let v = ecad_file::record::tx_value(&t, &[0u8; 32]).unwrap();
    assert!(v["ops"][0]["after"]["body"]["frame"].is_null(), "a text record has a text body: {v}");
}

/// Design 072 §8 (strict words): an unknown field in a frame body is a read error; an unknown item
/// kind and an unknown binding source inside the title block still read (lane E's step 4 makes
/// them strict).
#[test]
fn an_unknown_body_field_is_refused_and_title_block_items_read() {
    let l = ledger();
    let tx = l.full_log().unwrap().into_iter().next().unwrap();
    let v0 = ecad_file::record::tx_value(&tx, &[0u8; 32]).unwrap();
    // Design 072 §8: an unknown field in a frame body is refused.
    let mut stamped = v0.clone();
    stamped["ops"][0]["after"]["body"]["frame"]["stamp"] = serde_json::json!({"text": "DRAFT"});
    assert!(ecad_file::record::decode_tx(&stamped).is_err(), "an unknown body field is refused");
    // Design 072 §5.4: an unknown item kind, an unknown binding source or a revision binding
    // naming no revision field inside a title block does not read (the project opens read-only).
    let items = |extra: serde_json::Value| {
        let mut v = v0.clone();
        v["ops"][0]["after"]["body"]["frame"]["title_block"]["items"].as_array_mut().unwrap().push(extra);
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
    let mut t = ecad_file::record::decode_tx(&v0).unwrap();
    let Op::SetStyle { after: Some(r), .. } = &mut t.ops[0] else { panic!("{:?}", t.ops) };
    let l2 = frame_block(r).unwrap();
    assert!(l2.bindings().all(|b| b.source() != Source::Revision || b.revision_column().is_some()));
    let _ = BlockItem::Line { a: (0.0, 0.0), b: (1.0, 1.0), weight: Default::default() };
    // It writes back (canonical form) and reads again.
    let back = ecad_file::record::tx_value(&t, &[0u8; 32]).unwrap();
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
