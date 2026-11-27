//! A file of another format version is read-only and says so in those words when it reads to its head, and is
//! refused, saying how much could be read, when it does not; "damaged" is only real corruption at this build's
//! version.
//! The version is pinned with the golden head hash, so a changed format keeping its number fails.

use crate::common::*;
use crate::container::files::{TempDir, craft_head_record, deep};
use ecad_file::ReadOnly;
use ecad_file::SCHEMA_VERSION;
use ecad_file::disk::DiskProject;
use ecad_file::schema::META;
use redb::ReadableDatabase;

/// A saved project at `p` whose `schema_version` meta is `version`.
fn with_version(p: &std::path::Path, version: u64) {
    let mut f = fixture();
    let (d, _) = DiskProject::create(p, PROJECT, &mut f.ledger, None, None, WRITER).unwrap();
    drop(d);
    let db = redb::Database::open(p).unwrap();
    let w = db.begin_write().unwrap();
    w.open_table(META).unwrap().insert("schema_version", version.to_string().as_bytes()).unwrap();
    w.commit().unwrap();
}

fn meta_rows(p: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let db = redb::ReadOnlyDatabase::open(p).unwrap();
    let r = db.begin_read().unwrap();
    let t = r.open_table(META).unwrap();
    redb::ReadableTable::iter(&t).unwrap().map(|row| row.unwrap()).map(|(k, v)| (k.value().to_string(), v.value().to_vec())).collect()
}

#[test]
fn an_older_or_newer_file_opens_read_only_named_by_its_version_never_damaged() {
    for (file, want) in [
        (SCHEMA_VERSION - 1, format!("older format version (file {}, this build {SCHEMA_VERSION})", SCHEMA_VERSION - 1)),
        (SCHEMA_VERSION + 7, format!("newer format version (file {}, this build {SCHEMA_VERSION})", SCHEMA_VERSION + 7)),
    ] {
        let dir = TempDir::new("d2-format");
        let p = dir.path("p.ecad");
        with_version(&p, file);
        let before = meta_rows(&p);
        let mut o = DiskProject::open(&p, SEED ^ 1, WRITER).unwrap();
        assert_eq!(o.project.read_only(), Some(&ReadOnly::OtherFormat { file, build: SCHEMA_VERSION }), "{want}");
        let shown = o.project.read_only().unwrap().to_string();
        assert!(shown.contains(&want) && !shown.contains("damaged"), "{shown}");
        assert_eq!(o.project.project().read_only().map(|s| s.contains(&want)), Some(true), "the project refuses writes with the same reason");
        assert!(o.report.describe().contains(&want), "{}", o.report.describe());
        assert!(o.project.save(&mut o.ledger).is_err(), "a file of another format does not save");
        drop(o);
        assert_eq!(meta_rows(&p), before, "kept exactly as it is");
    }
}

#[test]
fn a_current_version_file_with_a_broken_record_is_damaged_not_another_format() {
    let dir = TempDir::new("d2-damage");
    let (p, reference) = deep(&dir, 20);
    craft_head_record(&p, reference.head(), |v| v.stack_cap = 2);
    let o = DiskProject::open(&p, SEED ^ 1, WRITER).unwrap();
    assert!(o.report.damage.is_some());
    assert_eq!(o.project.read_only(), Some(&ReadOnly::Damaged));
    assert!(!o.project.read_only().unwrap().to_string().contains("format version"));
}

#[test]
fn a_file_without_a_schema_version_is_refused() {
    let dir = TempDir::new("d2-noversion");
    let p = dir.path("p.ecad");
    with_version(&p, 0);
    assert!(DiskProject::open(&p, SEED ^ 1, WRITER).is_err());
}

/// A real file of format version 1 (`tests/golden/v1-cable-tag-in-bag.ecad`, written by a version 1
/// build, 18 transactions): one of its cables keeps its tag in the property bag, which this build reads as a
/// typed field only, so replay stops after transaction 4. It is refused, naming its format and how much could be
/// read: never shown as 4 transactions that look like the whole project, and never offered for Save As.
#[test]
fn a_version_1_file_that_does_not_read_to_its_head_is_refused_saying_how_much_could_be_read() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/v1-cable-tag-in-bag.ecad");
    let bytes = std::fs::read(path).expect("the version 1 fixture");
    let said = match ecad_file::Project::open(ecad_file::Store::from_bytes(bytes).unwrap(), SEED, WRITER) {
        Ok(o) => panic!("a part of an older file opens as if it were all of it: {}", o.report.describe()),
        Err(e) => e.to_string(),
    };
    let want = format!("older format version (file 1, this build {SCHEMA_VERSION}): only 4 of its 18 transactions could be read");
    assert!(said.contains(&want) && !said.contains("Save As"), "{said}");
}

/// The report says both facts: another format version never hides damage, and gets no Save As advice then.
#[test]
fn the_report_of_a_damaged_file_of_another_format_says_both() {
    let report = ecad_file::Report {
        head: 18,
        verified_seq: 18,
        snapshot_seq: 0,
        tail: 18,
        repaired: false,
        damage: Some(ecad_file::Damage { seq: Some(16), detail: "snapshot: a chunk is missing".into() }),
        schema_version: SCHEMA_VERSION - 1,
    };
    let said = report.describe();
    assert!(said.contains("older format version") && said.contains("It is also damaged: at transaction 16, snapshot: a chunk is missing"), "{said}");
    assert!(!said.contains("Save As"), "{said}");
    let current = ecad_file::Report { schema_version: SCHEMA_VERSION, ..report };
    assert!(current.describe().contains("Opened read-only: at transaction 16") && current.describe().contains("use Save As"));
}
