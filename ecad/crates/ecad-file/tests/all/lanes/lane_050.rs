//! Brief 050a (FP1–FP3, overseer ruling: the file arms with a round-trip test): `SetStyle`,
//! `SetStandard` and `SetProperty` on the standard through the op records, a project file's log
//! and its snapshot chunks; hostile records are refused in words, never by a panic.

use std::path::PathBuf;

use crate::common::{PROJECT, SEED, WRITER};
use ecad_file::v2::{ProjectV2, Store};
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
    StandardHeader { id: StandardId(0x50a), name: name.into(), based_on: "iec".into(), origin: None }
}

fn rec(name: &str, fields: &[(&str, &str)]) -> StyleRecord {
    StyleRecord { name: name.into(), fields: fields.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() }
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
                catalogue: "text".into(),
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
            catalogue: "text".into(),
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
        let proj = ProjectV2::create(store, PROJECT, &ledger, None, None, Some(1), WRITER, every).unwrap().0;
        drop(proj);
        let o = ProjectV2::open(Store::open(&p, true).unwrap(), SEED ^ 1, WRITER).unwrap();
        assert_eq!(o.ledger.document(), ledger.document(), "{name}");
        assert_eq!(o.ledger.full_log().unwrap(), ledger.full_log().unwrap(), "{name}: the log reads back the same ops");
        let v = ecad_file::v2::verify::verify(o.project.store());
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
