//! Strict `meta` names (owner, 0.2.3): a `meta` name this build does not read is an unknown word,
//! like every other. The project opens read-only, the reason names it, nothing writes to the file,
//! and the words check's own findings join it.

use crate::common::*;
use crate::container_v2::TempDir;
use ecad_file::ReadOnly;
use ecad_file::disk::DiskProject;
use ecad_file::schema::META;
use ecad_model::document::strict::{UnknownWord, WordPlace};
use redb::ReadableDatabase;

/// A saved project at `p` whose `meta` table also holds `future_name`.
fn with_unknown_meta(p: &std::path::Path) {
    let mut f = fixture();
    let (d, _) = DiskProject::create(p, PROJECT, &mut f.ledger, None, None, WRITER).unwrap();
    drop(d);
    let db = redb::Database::open(p).unwrap();
    let w = db.begin_write().unwrap();
    w.open_table(META).unwrap().insert("future_name", br#"{"anything": 1}"#.as_slice()).unwrap();
    w.commit().unwrap();
}

fn meta_rows(p: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let db = redb::ReadOnlyDatabase::open(p).unwrap();
    let r = db.begin_read().unwrap();
    let t = r.open_table(META).unwrap();
    redb::ReadableTable::iter(&t).unwrap().map(|row| row.unwrap()).map(|(k, v)| (k.value().to_string(), v.value().to_vec())).collect()
}

#[test]
fn an_unknown_meta_name_opens_read_only_and_names_it() {
    let dir = TempDir::new("strict-meta");
    let p = dir.path("p.ecad");
    with_unknown_meta(&p);
    let before = meta_rows(&p);

    let mut o = DiskProject::open(&p, SEED ^ 1, WRITER).unwrap();
    assert!(o.report.damage.is_none(), "the history is read as usual");
    let want = UnknownWord::MetaName { name: "future_name".into() };
    assert_eq!(want.place(), WordPlace::File);
    match o.project.read_only() {
        Some(ReadOnly::UnknownWords(words)) => assert_eq!(words, std::slice::from_ref(&want)),
        other => panic!("not read-only for the meta name: {other:?}"),
    }
    let shown = o.project.read_only().unwrap().to_string();
    assert!(shown.contains("unknown meta name `future_name`"), "{shown}");
    assert_eq!(o.project.project().read_only(), Some(shown.as_str()), "the project refuses writes with the same reason");

    // The words check's findings on open join the meta name, in one reason.
    let other = UnknownWord::ReservedKey { place: WordPlace::Project, key: "kind".into() };
    o.project.refuse_unknown_words(vec![other.clone()]);
    match o.project.read_only() {
        Some(ReadOnly::UnknownWords(words)) => assert_eq!(words, &[want, other]),
        other => panic!("{other:?}"),
    }

    // Nothing is written: not a save, not `meta.writer`.
    assert!(o.project.save(&mut o.ledger).is_err(), "a read-only project does not save");
    drop(o);
    assert_eq!(meta_rows(&p), before, "the file's meta table is kept exactly as it is");
}

#[test]
fn a_file_with_only_known_meta_names_opens_writable() {
    let dir = TempDir::new("strict-meta-ok");
    let p = dir.path("p.ecad");
    let mut f = fixture();
    let (d, _) = DiskProject::create(&p, PROJECT, &mut f.ledger, None, None, WRITER).unwrap();
    drop(d);
    let o = DiskProject::open(&p, SEED ^ 1, WRITER).unwrap();
    assert!(o.project.read_only().is_none(), "{:?}", o.project.read_only());
}
