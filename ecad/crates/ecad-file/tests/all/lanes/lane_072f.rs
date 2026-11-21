//! Lane 072 F (design 072 §6.2): drawing aids are kept in their own `model:aid:` chunks and come
//! back the same when the project is opened, from the snapshot and from the log.

use crate::common::*;
use crate::container_v2::TempDir;
use ecad_file::{ProjectV2, Store};
use ecad_model::aid::{Caption, ContinuationName};
use ecad_model::*;

#[test]
fn aids_come_back_from_the_snapshot_and_the_log() {
    let dir = TempDir::new("072f-aids");
    let mut f = fixture();
    let (a, b): (AidId, AidId) = (f.ids.fresh(), f.ids.fresh());
    let arrow = Aid::Continuation { name: Some(ContinuationName::new("24V").unwrap()), caption: Some(Caption::new("To the PSU").unwrap()) };
    let boxed = Aid::Box(LocationBox { panel_temperature: Some(units::Celsius(35.0)), ..LocationBox::default() });
    f.ledger.commit(Author::User, "aids", vec![Op::CreateAid { id: a, aid: arrow }, Op::CreateAid { id: b, aid: boxed.clone() }]).unwrap();
    let after = Aid::Box(LocationBox { panel_temperature: Some(units::Celsius(40.0)), ..LocationBox::default() });
    f.ledger.commit(Author::User, "warmer", vec![Op::ModifyAid { id: b, before: boxed, after }]).unwrap();
    // Snapshot every transaction for one file, never for the other: both open the same aids.
    let (lo, hi) = ecad_file::schema::SNAPSHOT_EVERY_RANGE;
    for (name, every) in [("snap.ecad", lo), ("log.ecad", hi)] {
        let p = dir.path(name);
        ProjectV2::create(Store::create(&p).unwrap(), PROJECT, &f.ledger, Some(1), WRITER, every).unwrap();
        let opened = ProjectV2::open(Store::open(&p, true).unwrap(), SEED ^ 1, WRITER).unwrap();
        assert_eq!(opened.ledger.document().aids, f.ledger.document().aids, "{name}");
    }
}

/// F1b: a continuation is a drawing aid; a file holding a device of kind `continuation` (the old
/// form) is not read (pre-v1, no conversion).
#[test]
fn a_continuation_device_is_a_read_error() {
    let dir = TempDir::new("072f-old-continuation");
    let mut f = fixture();
    let c: DeviceId = f.ids.fresh();
    let mut old = Device::default();
    old.properties.insert("kind".into(), "continuation".into());
    old.properties.insert("name".into(), "24V".into());
    f.ledger.commit(Author::User, "old arrow", vec![Op::CreateDevice { id: c, device: old }]).unwrap();
    let (_, hi) = ecad_file::schema::SNAPSHOT_EVERY_RANGE;
    let p = dir.path("old.ecad");
    ProjectV2::create(Store::create(&p).unwrap(), PROJECT, &f.ledger, Some(1), WRITER, hi).unwrap();
    // A row or chunk that does not read is damage: the file opens read-only, naming it.
    let err = match ProjectV2::open(Store::open(&p, true).unwrap(), SEED ^ 1, WRITER) {
        Err(e) => e.to_string(),
        Ok(o) => o.project.read_only().unwrap_or_default().to_string(),
    };
    assert!(err.contains("continuation"), "a read error naming it: {err:?}");
}
