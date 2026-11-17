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
