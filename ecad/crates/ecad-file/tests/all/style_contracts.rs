//! Brief 050a (FP1–FP3, overseer ruling: the file arms with a round-trip test): `SetStyle`,
//! `SetStandard` and `SetProperty` on the standard through the op records, a project file's log
//! and its snapshot chunks; hostile records are refused in words, never by a panic.

use std::path::PathBuf;

use crate::common::{PROJECT, SEED, WRITER};
use ecad_file::{Project, Store};
use ecad_model::style::{StandardHeader, StyleLayer, StyleRecord, StyleRef};
use ecad_model::{Author, Ledger, ObjectRef, Op, StandardId};

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-050-{name}-{}", ecad_store::disk::unique()));
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

fn header(name: &str) -> StandardHeader {
    StandardHeader {
        id: StandardId(0x50a),
        name: ecad_model::style::StyleName::new(name).unwrap(),
        based_on: ecad_model::style::BasedOn::Iec,
        origin: None,
    }
}

fn rec(name: &str, fields: &[(&str, &str)]) -> StyleRecord {
    // A text style's fields, as a file holds them (numbers as numbers).
    let body: serde_json::Map<String, serde_json::Value> =
        fields.iter().map(|(k, v)| (k.to_string(), v.parse::<f64>().map_or_else(|_| serde_json::json!(v), |x| serde_json::json!(x)))).collect();
    StyleRecord::new(name, ecad_model::style::StyleBody::Text(serde_json::from_value(serde_json::Value::Object(body)).unwrap()))
}

/// A ledger with a standard, values, records in both layers, and one undo.
fn styled() -> Ledger {
    let mut l = Ledger::new(SEED);
    l.commit(
        Author::User,
        "Make standard House",
        vec![
            Op::SetStandard { before: None, after: Some(header("House")) },
            Op::SetProperty { object: ObjectRef::Standard, key: "tags.coil.prefix".into(), before: None, after: Some("K".into()) },
            Op::SetStyle {
                layer: StyleLayer::Standard,
                catalogue: ecad_model::StyleCatalogue::Text,
                id: StyleRef::Builtin("tag".into()),
                before: None,
                after: Some(rec("", &[("colour", "RD")])),
            },
        ],
    )
    .unwrap();
    l.commit(
        Author::Agent("tester".into()),
        "New text style Tag large",
        vec![Op::SetStyle {
            layer: StyleLayer::Project,
            catalogue: ecad_model::StyleCatalogue::Text,
            id: StyleRef::Own(0xabc),
            before: None,
            after: Some(rec("Tag large", &[("height", "5"), ("weight", "bold")])),
        }],
    )
    .unwrap();
    l.commit(Author::User, "Rename", vec![Op::SetStandard { before: Some(header("House")), after: Some(header("House 2026")) }]).unwrap();
    l.undo(Author::User).unwrap();
    l
}

#[test]
fn style_ops_round_trip_through_a_project_file_and_its_snapshots() {
    let dir = TempDir::new("roundtrip");
    let ledger = styled();
    // Snapshot every transaction, and almost never: both reopen paths (snapshot and log replay).
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

/// A record whose style ID or layer is not one this schema knows is refused in words.
#[test]
fn hostile_style_records_are_refused_in_words() {
    let l = styled();
    let tx = l.full_log().unwrap().into_iter().nth(1).unwrap();
    let prev = [0u8; 32];
    let good = ecad_file::record::tx_value(&tx, &prev).unwrap();
    assert!(ecad_file::record::decode_tx(&good).is_ok());
    for (path, bad) in [
        ("id", serde_json::json!("Not A Word")),
        ("id", serde_json::json!("#12")),
        ("layer", serde_json::json!("galaxy")),
        ("catalogue", serde_json::json!(7)),
        ("unknown_field", serde_json::json!(1)),
    ] {
        let mut v = good.clone();
        v["ops"][0][path] = bad.clone();
        let e = ecad_file::record::decode_tx(&v);
        assert!(e.is_err(), "{path} = {bad} decoded");
    }
}

/// Pre-v1 there is no compatibility: a potential row naming `colour` or `cross_section` (an older
/// build's) is refused by the strict reader.
#[test]
fn a_potential_row_with_a_colour_is_refused() {
    use ecad_file::chunks::{ModelMeta, PROJECT_KEY, assemble, build_full};
    let mut doc = ecad_model::Document::default();
    doc.settings.wiring.potentials = ecad_model::wiring::default_potentials();
    let (set, chunks) = build_full(&doc, &ModelMeta::default()).unwrap();
    let h = set.chunks[PROJECT_KEY];
    let text = String::from_utf8(chunks.iter().find(|c| c.hash == h).unwrap().body.clone()).unwrap();
    assert!(!text.contains(r#""colour":"#), "the table stores no colour: {text}");
    let old = text.replacen(r#""numbered":true"#, r#""numbered":true,"colour":"BN","cross_section":"2.5""#, 1);
    assert_ne!(old, text);
    let bodies: Vec<(String, Vec<u8>)> = set
        .chunks
        .iter()
        .map(|(k, h)| {
            let b = if k == PROJECT_KEY { old.clone().into_bytes() } else { chunks.iter().find(|c| c.hash == *h).unwrap().body.clone() };
            (k.clone(), b)
        })
        .collect();
    assert!(assemble(&set, bodies).is_err(), "an unknown `colour` key is a read error");
}
