//! Revisions and versions are data. A user version `B` and revision `B` are two points
//! that survive save, reopen, Restore and a branch merge; the revision scheme is the project's and
//! cannot change once something is issued; a rev outside the scheme is refused by name.

use super::acceptance::{TempDir, raw, saved, verify, wire};
use crate::common::reopen;

use ecad_model::{PointName, Rev};
use serde_json::{Value, json};

fn b_points() -> (PointName, PointName) {
    (PointName::version("B").unwrap(), PointName::Revision(Rev::Letter(2)))
}

/// The two points `B` the document holds, with where each is.
fn points(h: &mut crate::common::H) -> std::collections::BTreeMap<PointName, u64> {
    h.doc().versions().clone()
}

#[test]
fn a_user_version_b_and_revision_b_both_survive_save_reopen_restore_and_a_branch_merge() {
    let d = TempDir::new("typed-b");
    let (mut h, path) = saved(&d, 0x0004_0001);
    let (user_b, rev_b) = b_points();
    wire(&mut h, 10.0);
    h.run("version", json!({"name": "B"})).unwrap();
    wire(&mut h, 20.0);
    h.run("issuerevision", json!({"rev": "B", "description": "second", "drawn": "CV"})).unwrap();
    wire(&mut h, 30.0);
    let both = points(&mut h);
    assert_eq!(both.keys().cloned().collect::<Vec<_>>(), vec![user_b.clone(), rev_b.clone()], "two points named B");
    assert_ne!(both[&user_b], both[&rev_b]);

    // Save and reopen: both come back, each under its own kind, and the file verifies.
    h.run("qsave", Value::Null).unwrap();
    let reader_rows = |path: &str| {
        let v = ecad_file::Store::open(std::path::Path::new(path), false).unwrap();
        let r = v.reader().unwrap();
        let branch = r.branches().unwrap().into_iter().find(|(_, b)| b.name == "main").unwrap().0;
        let user = r.version(branch, ecad_file::schema::RowKind::VERSION, "B").unwrap().map(|v| v.point.seq);
        let rev = r.version(branch, ecad_file::schema::RowKind::REVISION, &Rev::Letter(2).key()).unwrap().map(|v| v.point.seq);
        (user, rev)
    };
    assert_eq!(reader_rows(&path), (Some(both[&user_b]), Some(both[&rev_b])), "one row per kind and name");
    verify(&path);
    h.close_all();
    let mut h = reopen(&path, 0x0004_0001);
    assert_eq!(points(&mut h), both, "reopened");

    // Restore to the user's version B leaves both names as they are.
    h.run("restore", json!({"version": "B"})).unwrap();
    assert_eq!(points(&mut h), both, "restored");
    // Restore addressed by the revision reads the rev in the project's scheme.
    h.run("restore", json!({"revision": "B"})).unwrap();
    assert_eq!(points(&mut h), both, "restored to revision B");
    h.run("qsave", Value::Null).unwrap();

    // A branch merge: work on a branch, merge it back; both points are still main's.
    raw(&mut h, "branch", json!({"name": "X"})).unwrap();
    wire(&mut h, 40.0);
    raw(&mut h, "switchbranch", json!({"branch": "main", "save": true})).unwrap();
    raw(&mut h, "merge", json!({"from": "X"})).unwrap();
    assert_eq!(points(&mut h), both, "merged");
    h.run("qsave", Value::Null).unwrap();
    assert_eq!(reader_rows(&path), (Some(both[&user_b]), Some(both[&rev_b])), "after the merge");
    verify(&path);
}

#[test]
fn a_rev_outside_the_scheme_is_refused_naming_the_problem_and_the_scheme_is_locked_once_issued() {
    let d = TempDir::new("typed-scheme");
    let (mut h, path) = saved(&d, 0x0004_0002);
    // The default scheme: dash, A, B ... A lower case rev is not one.
    let e = h.run("revision", json!({"rev": "a"})).unwrap_err();
    assert!(e.contains("not a rev of this project") && e.contains("A, B"), "{e}");
    // A numbered scheme before anything is issued.
    h.run("projectprops", json!({"rev_scheme": {"kind": "numbered", "prefix": "P", "width": 2, "start": 1}})).unwrap();
    let e = h.run("revision", json!({"rev": "A"})).unwrap_err();
    assert!(e.contains("not a rev of this project") && e.contains("P01"), "{e}");
    let e = h.run("revision", json!({"rev": "P1"})).unwrap_err();
    assert!(e.contains("P01"), "{e}");
    h.run("revision", json!({"description": "first", "drawn": "CV"})).unwrap();
    assert_eq!(h.run("projectprops", json!({})).unwrap()["pending_rev"], "P01", "the next rev comes from the scheme");
    wire(&mut h, 10.0);
    let r = h.run("issuerevision", json!({})).unwrap();
    assert_eq!(r["rev"], "P01");
    // Once issued the scheme is locked, and the refusal says so; nothing changes.
    let before = h.doc();
    let e = h.run("projectprops", json!({"rev_scheme": {"kind": "letters"}})).unwrap_err();
    assert!(e.contains("cannot change once a revision is issued"), "{e}");
    assert_eq!(h.doc(), before);
    // The same scheme again is no change, and not a refusal.
    h.run("projectprops", json!({"rev_scheme": {"kind": "numbered", "prefix": "P", "width": 2, "start": 1}})).unwrap();
    // A free-text date is refused naming the problem.
    let e = h.run("revision", json!({"date": "next week"})).unwrap_err();
    assert!(e.contains("not a date") && e.contains("YYYY-MM-DD"), "{e}");
    h.run("revision", json!({"description": "second"})).unwrap();
    h.run("revision", json!({"date": "2026-11-22"})).unwrap();
    h.run("qsave", Value::Null).unwrap();
    verify(&path);
}
