//! Lane D2 (owner, 2026-11-22): a file of another format version is read-only and says so in
//! those words, whatever replay finds; "damaged" is only real corruption at this build's version.
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
