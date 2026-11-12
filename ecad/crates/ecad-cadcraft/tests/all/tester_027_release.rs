//! Tester 027 (release), from the brief and design 027 only: the release sequence with real
//! (not injected) failures, 026's finalise choice when the release then stops, `release.json`
//! hashes checked by an independent SHA-256 (the OS tool), `checks.csv` against `prerelease`,
//! FP1 through undo, redo, save and reopen, the existing-folder choices repeated, and Windows
//! paths (spaces, long paths, characters a file name cannot hold). A probe that found a bug is
//! `#[ignore = "tester 027: <bug>"]` and fails until it is fixed.

use std::path::{Path, PathBuf};

use cadcraft_engine::Session;
use ecad_cadcraft::release::{self, Step};
use ecad_cadcraft::{Persistence, ProjectHandle};
use serde_json::{Value, json};

use crate::common::H;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ecad-t027-{name}-{}", ecad_store::disk::unique()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Design 027 §14's fixture, saved for real (`Persistence::File`) as `<dir>/demo.ecad`: three
/// loose wires, cable C1 with an orphan reference end, drawing number `D-1001`, Rev A issued,
/// Rev B pending.
fn project_in(dir: &Path, drawing_number: &str) -> (H, String) {
    ecad_cadcraft::printbook::install_io();
    let mut h = H::new(0x0E27_0001);
    for y in [0.0, 5.0, 10.0] {
        h.run("wire", json!({"points": [[60.0, y], [120.0, y]]})).unwrap();
    }
    let c1 = h.run("cableslash", json!({"from": [88, -3], "to": [92, 18], "tag": "C1"})).unwrap()["cable"].clone();
    h.run("cableref", json!({"cable": c1, "at": [120, 5]})).unwrap();
    h.run("projectprops", json!({"title": "Pump panel", "drawing_number": drawing_number})).unwrap();
    h.run("revision", json!({"description": "First issue", "drawn": "CVA"})).unwrap();
    h.run("issuerevision", json!({"date": "2026-11-01"})).unwrap();
    h.run("revision", json!({"description": "Terminals added", "drawn": "CVA", "date": "2026-11-12"})).unwrap();
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join("demo.ecad").to_string_lossy().into_owned();
    h.cfg(|p| p.persistence = Persistence::File);
    h.run("saveas", json!({"path": path})).unwrap();
    (h, path)
}

fn rel(dir: &Path, rev: &str) -> PathBuf {
    dir.join("release").join(format!("Rev {rev}"))
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

fn manifest(dir: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(dir.join("release.json")).unwrap()).unwrap()
}

/// SHA-256 of a file by the operating system's own tool, not the crate's code. The tool gets a
/// copy at a short path (certutil cannot open paths past 260 characters).
fn os_sha256(long: &Path) -> String {
    let short = std::env::temp_dir().join(format!("ecad-t027-hash-{}", ecad_store::disk::unique()));
    std::fs::copy(long, &short).unwrap();
    let file = short.as_path();
    #[cfg(windows)]
    let out = std::process::Command::new("certutil").arg("-hashfile").arg(file).arg("SHA256").output().unwrap();
    #[cfg(not(windows))]
    let out = std::process::Command::new("sha256sum").arg(file).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let hex: Vec<String> = text
        .lines()
        .flat_map(|l| l.split_whitespace())
        .map(|w| w.to_ascii_lowercase())
        .filter(|w| w.len() == 64 && w.chars().all(|c| c.is_ascii_hexdigit()))
        .collect();
    let _ = std::fs::remove_file(&short);
    assert_eq!(hex.len(), 1, "{text}");
    hex[0].clone()
}

/// Every file of the manifest is in the folder with the OS tool's hash and its size, and every
/// file in the folder (bar the manifest) is in the manifest.
fn hashes_match(out: &Path) {
    let m = manifest(out);
    let listed: Vec<String> = m["files"].as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap().to_string()).collect();
    let mut on_disk = names(out);
    on_disk.retain(|n| n != "release.json");
    let mut l = listed.clone();
    l.sort();
    assert_eq!(l, on_disk, "manifest lists exactly the folder's files");
    for f in m["files"].as_array().unwrap() {
        let p = out.join(f["name"].as_str().unwrap());
        assert_eq!(f["sha256"].as_str().unwrap(), os_sha256(&p), "{f}");
        assert_eq!(f["bytes"].as_u64().unwrap(), std::fs::metadata(&p).unwrap().len(), "{f}");
    }
}

fn provisional(h: &mut H) {
    h.run("numbering", json!({"provisional": true})).unwrap();
    h.run("place", json!({"symbol": "coil", "at": [250, 150]})).unwrap();
    h.run("place", json!({"symbol": "coil", "at": [300, 150]})).unwrap();
}

fn labels_since(h: &mut H, head: u64) -> Vec<String> {
    h.p(|p| p.ledger.log().iter().skip(head as usize).map(|t| t.label.clone()).collect())
}

// ---------------------------------------------------------------- AC 4: folder, hashes, checks.csv

/// AC 4: the six files, every hash equal to the OS tool's; `checks.csv` has one row per
/// pre-release check row (all groups, no row limit) and the manifest's counts agree.
#[test]
fn hashes_match_an_independent_sha256_and_checks_csv_matches_prerelease() {
    let dir = tmp("hash");
    let (mut h, _) = project_in(&dir, "D-1001");
    let pre = h.run("prerelease", json!({"limit": 2000})).unwrap();
    let total: u64 = pre["groups"].as_array().unwrap().iter().map(|g| g["count"].as_u64().unwrap()).sum();
    let sum = pre["errors"].as_u64().unwrap() + pre["warnings"].as_u64().unwrap() + pre["info"].as_u64().unwrap();
    assert_eq!(total, sum, "{pre}");
    let rows_shown: usize = pre["groups"].as_array().unwrap().iter().map(|g| g["rows"].as_array().unwrap().len()).sum();
    assert_eq!(rows_shown as u64, total, "limit 2000 shows every row: {pre}");
    h.run("release", json!({})).unwrap();
    let out = rel(&dir, "B");
    assert_eq!(
        names(&out),
        ["D-1001 Rev B BOM.csv", "D-1001 Rev B cables.csv", "D-1001 Rev B checks.csv", "D-1001 Rev B wires.csv", "D-1001 Rev B.pdf", "release.json"]
    );
    hashes_match(&out);
    let csv = std::fs::read_to_string(out.join("D-1001 Rev B checks.csv")).unwrap();
    assert!(csv.starts_with('\u{feff}') && csv.contains("\r\n"), "UTF-8 BOM, CRLF");
    let lines = csv.trim_end_matches("\r\n").split("\r\n").count() as u64 - 1;
    assert_eq!(lines, total, "{csv}");
    let m = manifest(&out);
    assert_eq!(m["checks"]["errors"], pre["errors"]);
    assert_eq!(m["checks"]["warnings"], pre["warnings"]);
    assert_eq!(m["checks"]["info"], pre["info"]);
    let checks_file = m["files"].as_array().unwrap().iter().find(|f| f["kind"] == "checks").unwrap().clone();
    assert_eq!(checks_file["rows"].as_u64(), Some(total), "{checks_file}");
    // A saved v2 file: the manifest carries the issue's chain value.
    assert!(m["issued"]["chain"].as_str().is_some_and(|c| c.len() == 64), "{m}");
    drop(h);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------- AC 3: real failures

/// AC 3, design §6 "Folder not writable": `release` sits where a file is, so the folder cannot
/// be made. Refused before the issue; the project and the disk are as before.
#[test]
fn an_unwritable_folder_refuses_before_the_issue() {
    let dir = tmp("nowrite");
    let (mut h, _) = project_in(&dir, "D-1001");
    std::fs::write(dir.join("release"), b"a file, not a folder").unwrap();
    let head = h.head();
    let e = h.run("release", json!({})).unwrap_err();
    assert!(e.starts_with("Release stopped: ") && e.contains("Nothing was issued or written."), "{e}");
    assert_eq!(h.head(), head);
    assert!(!h.p(|p| p.unsaved()));
    let mut left = names(&dir);
    left.retain(|n| !n.ends_with(".lock"));
    assert_eq!(left, ["demo.ecad", "release"]);
    drop(h);
    let _ = std::fs::remove_dir_all(&dir);
}

/// AC 3: a `.partial` left by a crashed release (it carries the staging mark) is cleared by the
/// next release; nothing of it reaches the folder.
#[test]
fn a_partial_left_by_a_crash_is_cleared_by_the_next_release() {
    let dir = tmp("crash");
    let (mut h, _) = project_in(&dir, "D-1001");
    let out = rel(&dir, "B");
    let partial = release::partial_of(&out);
    std::fs::create_dir_all(&partial).unwrap();
    std::fs::write(partial.join(release::STAGING_MARK), b"").unwrap();
    std::fs::write(partial.join("D-1001 Rev B.pdf"), b"half a pdf").unwrap();
    std::fs::write(partial.join("stray.tmp"), b"x").unwrap();
    h.run("release", json!({})).unwrap();
    assert!(!partial.exists());
    assert!(!out.join("stray.tmp").exists());
    assert!(!out.join(release::STAGING_MARK).exists(), "the staging mark does not reach the folder");
    hashes_match(&out);
    drop(h);
    let _ = std::fs::remove_dir_all(&dir);
}

/// AC 3 step 5 with a real file: the project file has the read-only attribute. Either the release
/// is refused up front with nothing recorded, or (as on Windows today, where the save still
/// succeeds) the folder is finished; a failed save keeps the issue and says to save.
#[test]
fn a_real_save_failure_keeps_the_issue_and_finishes_the_folder() {
    let dir = tmp("ro");
    let (mut h, path) = project_in(&dir, "D-1001");
    let mut perm = std::fs::metadata(&path).unwrap().permissions();
    perm.set_readonly(true);
    std::fs::set_permissions(&path, perm.clone()).unwrap();
    let head = h.head();
    let r = h.run("release", json!({}));
    perm.set_readonly(false);
    #[allow(clippy::permissions_set_readonly_false)]
    std::fs::set_permissions(&path, perm).unwrap();
    let v = match r {
        Ok(v) => v,
        // A refusal up front (the file is read-only) is a way out too, if nothing was recorded.
        Err(e) => {
            eprintln!("read-only file: refused up front: {e}");
            assert_eq!(h.head(), head, "{e}");
            assert!(!dir.join("release").exists(), "{e}");
            drop(h);
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
    };
    eprintln!("read-only file: saved {} finished {}: {}", v["saved"], v["finished"], v["message"]);
    assert_eq!(h.head(), head + 1, "{v}");
    if v["saved"] == false {
        assert!(v["message"].as_str().unwrap().contains("Save it before closing, or Rev B is not kept."), "{v}");
        assert!(h.p(|p| p.unsaved()));
    }
    assert_eq!(v["finished"], true, "{v}");
    hashes_match(&rel(&dir, "B"));
    drop(h);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------- AC 2/3: 026's choice

/// AC 2: with provisional items, no choice is refused before anything is recorded; `"finalise"`
/// records Finalise then the issue, and the folder is built from the finalised state (no
/// provisional row in `checks.csv`, no `?` tag in the BOM).
#[test]
fn finalise_then_issue_and_the_folder_shows_the_finalised_tags() {
    let dir = tmp("fin");
    let (mut h, _) = project_in(&dir, "D-1001");
    provisional(&mut h);
    let head = h.head();
    let e = h.run("release", json!({})).unwrap_err();
    assert!(e.contains("provisional"), "{e}");
    assert_eq!(h.head(), head);
    assert!(!dir.join("release").exists());
    let v = h.s.execute("release", &json!({"provisional": "finalise"})).unwrap();
    assert_eq!(labels_since(&mut h, head), ["Finalise numbering", "Issue Rev B"], "{v}");
    let out = rel(&dir, "B");
    let checks = std::fs::read_to_string(out.join("D-1001 Rev B checks.csv")).unwrap();
    assert!(!checks.contains("Provisional"), "{checks}");
    let bom = std::fs::read_to_string(out.join("D-1001 Rev B BOM.csv")).unwrap();
    assert!(!bom.contains("-K?"), "{bom}");
    assert!(bom.contains("-K01"), "{bom}");
    hashes_match(&out);
    drop(h);
    let _ = std::fs::remove_dir_all(&dir);
}

/// AC 3: "A failure before the issue leaves the project and disk unchanged." With
/// `provisional: "finalise"`, a failure while staging must not leave `Finalise numbering`
/// recorded either.
#[test]
#[ignore = "tester 027: Finalise stays recorded when the release stops before the issue"]
fn a_staging_failure_after_finalise_leaves_the_project_unchanged() {
    let dir = tmp("finfail");
    let (mut h, _) = project_in(&dir, "D-1001");
    provisional(&mut h);
    let head = h.head();
    h.cfg(|p| p.faults.fail_release_at = Some(Step::Partial));
    let e = h.s.execute("release", &json!({"provisional": "finalise"})).unwrap_err().to_string();
    assert!(e.contains("Nothing was issued or written."), "{e}");
    assert_eq!(labels_since(&mut h, head), Vec::<String>::new(), "{e}");
    drop(h);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Same rule with a real failure: the folder cannot be made (`release` is a file).
#[test]
#[ignore = "tester 027: Finalise stays recorded when the release stops before the issue"]
fn an_unwritable_folder_after_finalise_leaves_the_project_unchanged() {
    let dir = tmp("finnowrite");
    let (mut h, _) = project_in(&dir, "D-1001");
    provisional(&mut h);
    std::fs::write(dir.join("release"), b"file").unwrap();
    let head = h.head();
    let e = h.s.execute("release", &json!({"provisional": "finalise"})).unwrap_err().to_string();
    assert!(e.contains("Nothing was issued or written."), "{e}");
    assert_eq!(labels_since(&mut h, head), Vec::<String>::new(), "{e}");
    drop(h);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Design §10: "an existing folder without `existing` refuses before any transaction", also
/// with `provisional: "finalise"`.
#[test]
#[ignore = "tester 027: Finalise stays recorded when the release stops before the issue"]
fn an_existing_folder_with_finalise_refuses_before_any_transaction() {
    let dir = tmp("finexists");
    let (mut h, _) = project_in(&dir, "D-1001");
    provisional(&mut h);
    std::fs::create_dir_all(rel(&dir, "B")).unwrap();
    let head = h.head();
    let e = h.s.execute("release", &json!({"provisional": "finalise"})).unwrap_err().to_string();
    assert!(e.contains("Give existing:"), "{e}");
    assert_eq!(labels_since(&mut h, head), Vec::<String>::new(), "{e}");
    drop(h);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------- FP1: undo, redo, reopen

/// AC 7, design §12: undo of a release removes FP1 with the issue, redo brings it back, the
/// folder stays on disk throughout; save and reopen keep FP1; releasing the pending revision
/// again after the undo asks about the existing folder before recording anything.
#[test]
fn fp1_follows_undo_redo_and_survives_save_and_reopen() {
    let dir = tmp("fp1");
    let (mut h, path) = project_in(&dir, "D-1001");
    h.run("release", json!({})).unwrap();
    let out = rel(&dir, "B");
    let key = "rev.2.released";
    assert_eq!(h.doc().properties.get(key).map(String::as_str), Some("release/Rev B"));
    h.run("undoissue", json!({})).unwrap();
    assert!(!h.doc().properties.contains_key(key));
    assert!(!h.doc().versions.contains_key("Rev B"));
    assert!(out.join("release.json").exists(), "undo never deletes files");
    // Released again while the folder is there: asked first, nothing recorded.
    let head = h.head();
    let e = h.run("release", json!({})).unwrap_err();
    assert!(e.contains("exists (") && e.contains("Give existing:"), "{e}");
    assert_eq!(h.head(), head);
    h.run("redo", Value::Null).unwrap();
    assert_eq!(h.doc().properties.get(key).map(String::as_str), Some("release/Rev B"));
    h.run("qsave", json!({})).unwrap();
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    drop(h);
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, &path, 271).unwrap();
    let mut h2 = H { s, project, trail: Vec::new() };
    let doc = h2.doc();
    assert_eq!(doc.properties.get(key).map(String::as_str), Some("release/Rev B"));
    let row = ecad_cadcraft::revisions::issued(&doc).into_iter().next().unwrap();
    assert_eq!((row.rev().to_string(), row.released.as_deref()), ("B".to_string(), Some("release/Rev B")));
    // Reopened: the issued revision's check reads its folder, and it is written again with
    // nothing recorded.
    let pre = h2.run("prerelease", json!({"rev": "B"})).unwrap();
    assert_eq!(pre["state"], "issued");
    assert_eq!(pre["exists"], true, "{pre}");
    let head = h2.head();
    let v = h2.run("release", json!({"rev": "B", "existing": "keep-both"})).unwrap();
    assert_eq!(h2.head(), head);
    assert!(v["folder"].as_str().unwrap().ends_with("Rev B (2)"), "{v}");
    drop(h2);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------- AC 5: re-release

/// AC 5: an issued revision is written from its issued state, whatever was drawn since; the
/// CSVs are byte-equal; Replace twice and Keep both twice never lose a folder.
#[test]
fn rewriting_ignores_later_edits_and_repeated_choices_lose_nothing() {
    let dir = tmp("again");
    let (mut h, _) = project_in(&dir, "D-1001");
    h.run("release", json!({})).unwrap();
    let out = rel(&dir, "B");
    let csvs = |d: &Path| -> Vec<(String, Vec<u8>)> {
        names(d).into_iter().filter(|n| n.ends_with(".csv")).map(|n| (n.clone(), std::fs::read(d.join(&n)).unwrap())).collect()
    };
    let first = csvs(&out);
    // Drawn since: another wire and a device. Rev B's folder is unchanged by them.
    h.run("wire", json!({"points": [[60, 40], [120, 40]]})).unwrap();
    h.run("place", json!({"symbol": "coil", "at": [250, 150]})).unwrap();
    let head = h.head();
    for i in 0..3 {
        let v = h.run("release", json!({"rev": "B", "existing": "replace"})).unwrap();
        assert_eq!(v["compare"]["different"], json!([]), "replace {i}: {v}");
        assert_eq!(csvs(&out), first, "replace {i}");
    }
    let replaced = names(&dir.join("release").join("replaced"));
    assert_eq!(replaced.len(), 3, "{replaced:?}");
    for r in &replaced {
        assert!(dir.join("release").join("replaced").join(r).join("release.json").exists(), "{r}");
    }
    for i in 2..=3 {
        let v = h.run("release", json!({"rev": "B", "existing": "keep-both"})).unwrap();
        let d = dir.join("release").join(format!("Rev B ({i})"));
        assert_eq!(v["folder"], json!(d.to_string_lossy()), "{v}");
        assert_eq!(csvs(&d), first);
        hashes_match(&d);
    }
    // A folder there that no release wrote: Replace moves it aside, its file kept.
    let c = rel(&dir, "A");
    std::fs::create_dir_all(&c).unwrap();
    std::fs::write(c.join("notes.txt"), "mine").unwrap();
    let e = h.run("release", json!({"rev": "A"})).unwrap_err();
    assert!(e.contains("This folder was not written by a release"), "{e}");
    let e = h.run("release", json!({"rev": "A", "existing": "add-missing"})).unwrap_err();
    assert!(e.contains("cannot only be completed"), "{e}");
    assert_eq!(names(&c), ["notes.txt"]);
    h.run("release", json!({"rev": "A", "existing": "replace"})).unwrap();
    let kept: Vec<PathBuf> = names(&dir.join("release").join("replaced"))
        .into_iter()
        .filter(|n| n.starts_with("Rev A"))
        .map(|n| dir.join("release").join("replaced").join(n))
        .collect();
    assert_eq!(kept.len(), 1);
    assert_eq!(std::fs::read_to_string(kept[0].join("notes.txt")).unwrap(), "mine");
    assert_eq!(h.head(), head, "re-writing records nothing");
    drop(h);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------- Windows paths

/// A project folder with spaces, nested deep enough that the files' full paths pass 260
/// characters (Windows' old MAX_PATH): the release, its hashes and a re-write all work.
#[test]
fn spaces_and_long_paths() {
    let root = tmp("long path");
    let mut dir = root.clone();
    while dir.to_string_lossy().len() < 220 {
        dir = dir.join("a folder with spaces");
    }
    let (mut h, _) = project_in(&dir, "D-1001 long drawing number with spaces for the stem");
    let v = h.run("release", json!({})).unwrap();
    let out = rel(&dir, "B");
    assert_eq!(v["folder"], json!(out.to_string_lossy()), "{v}");
    let longest = names(&out).iter().map(|n| out.join(n).to_string_lossy().len()).max().unwrap();
    assert!(longest > 260, "{longest}");
    hashes_match(&out);
    assert_eq!(h.doc().properties.get("rev.2.released").map(String::as_str), Some("release/Rev B"));
    let v = h.run("release", json!({"rev": "B", "existing": "keep-both"})).unwrap();
    assert!(v["folder"].as_str().unwrap().ends_with("Rev B (2)"), "{v}");
    drop(h);
    let _ = std::fs::remove_dir_all(&root);
}

/// Design §7.1: characters a Windows file name cannot hold become `-`; trailing dots go.
#[test]
fn a_drawing_number_with_reserved_characters_gives_clean_names() {
    let dir = tmp("chars");
    let (mut h, _) = project_in(&dir, "A/B\\C:D*E?F\"G<H>I|J.");
    h.run("release", json!({})).unwrap();
    let out = rel(&dir, "B");
    assert!(out.join("A-B-C-D-E-F-G-H-I-J Rev B.pdf").exists(), "{:?}", names(&out));
    hashes_match(&out);
    drop(h);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A Rev typed with a path separator or a reserved character: the release either refuses with
/// nothing recorded and nothing on disk, or writes one folder directly under `release/`.
#[test]
fn a_rev_with_a_separator_or_reserved_character_never_escapes_the_release_folder() {
    for rev in ["B/1", "B\\1", "B:1", "..", "B?"] {
        let dir = tmp("revchars");
        let (mut h, _) = project_in(&dir, "D-1001");
        if h.run("revision", json!({"rev": rev})).is_err() {
            // Refused where it is typed: nothing for the release to do.
            eprintln!("rev {rev:?}: refused by revision");
            drop(h);
            let _ = std::fs::remove_dir_all(&dir);
            continue;
        }
        let head = h.head();
        match h.run("release", json!({})) {
            Err(e) => {
                eprintln!("rev {rev:?}: release refused: {e}");
                assert_eq!(h.head(), head, "{rev}: {e}");
                let left: Vec<String> = if dir.join("release").exists() { names(&dir.join("release")) } else { vec![] };
                assert!(left.is_empty(), "{rev}: {e}: left {left:?}");
            }
            Ok(v) => {
                eprintln!("rev {rev:?}: released to {}", v["folder"]);
                let folder = PathBuf::from(v["folder"].as_str().unwrap());
                assert_eq!(folder.parent(), Some(dir.join("release").as_path()), "{rev}: {v}");
                hashes_match(&folder);
            }
        }
        assert_eq!(names(&dir).iter().filter(|n| *n != "release" && *n != "demo.ecad").count(), 0, "{rev}: {:?}", names(&dir));
        drop(h);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
