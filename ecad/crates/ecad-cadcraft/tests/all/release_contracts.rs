//! Brief 027 acceptance (design 027 §14): the pre-release check, the orphan-reference Checks kind
//! and its update map, releasing the pending revision in order (with a failure injected at each
//! step), writing an issued revision's folder again, the existing-folder choices, and Reopen
//! Revision showing the revision it opens. Driven through the commands' JSON forms.

use std::path::{Path, PathBuf};

use ecad_cadcraft::checks::{Checks, Kind};
use ecad_cadcraft::release::{self, Step};
use ecad_model::{CableId, ConductorId, ObjectRef, Op, PageId};
use serde_json::{Value, json};

use crate::common::{H, id};

/// A fresh, empty folder for one test.
fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ecad-027-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

struct F {
    h: H,
    supply: PageId,
    c1: CableId,
    dir: PathBuf,
}

/// The fixture of design 027 §14: drawing number `D-1001`, three loose wires and cable `C1` with
/// one reference end on Supply (an orphan until continued), an issued Rev A, a pending Rev B
/// (`Terminals added`, drawn `CVA`), and a project file in a folder of its own.
fn fixture(name: &str) -> F {
    ecad_cadcraft::printbook::install_io();
    let mut h = H::new(0x0270);
    let supply = h.first_page();
    h.run("renamepage", json!({"page": supply.hex(), "title": "Supply"})).unwrap();
    for y in [0.0, 5.0, 10.0] {
        let _: ConductorId = id(&h.run("wire", json!({"points": [[60.0, y], [120.0, y]]})).unwrap(), "conductor");
    }
    let c1: CableId = id(&h.run("cableslash", json!({"from": [88, -3], "to": [92, 18], "tag": "C1"})).unwrap(), "cable");
    h.run("cableref", json!({"cable": c1.hex(), "at": [120, 5]})).unwrap();
    h.run("projectprops", json!({"title": "Pump panel", "drawing_number": "D-1001"})).unwrap();
    h.run("revision", json!({"description": "First issue", "drawn": "CVA"})).unwrap();
    h.run("issuerevision", json!({"date": "2026-11-01"})).unwrap();
    h.run("revision", json!({"description": "Terminals added", "drawn": "CVA", "date": "2026-11-12"})).unwrap();
    let dir = tmp(name);
    let path = dir.join("demo.ecad").to_string_lossy().into_owned();
    h.cfg(|p| p.path = Some(path));
    F { h, supply, c1, dir }
}

fn folder(f: &F, rev: &str) -> PathBuf {
    f.dir.join("release").join(format!("Rev {rev}"))
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

fn manifest(dir: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(dir.join("release.json")).unwrap()).unwrap()
}

/// The kept Checks equal a fresh build (the update map left nothing behind).
fn kept_is_built(h: &mut H) {
    let (kept, built) = h.cfg(|p| {
        p.checks();
        let doc = p.ledger.document();
        let kept: Vec<String> = p.checks.as_ref().unwrap().sorted(doc).iter().map(|c| format!("{c:?}")).collect();
        let built: Vec<String> = Checks::build(doc, p.tolerance).sorted(doc).iter().map(|c| format!("{c:?}")).collect();
        (kept, built)
    });
    assert_eq!(kept, built);
}

fn orphans(h: &mut H) -> Vec<String> {
    h.cfg(|p| p.checks().rows().filter(|c| c.kind == Kind::OrphanReference).map(|c| c.which.clone()).collect())
}

/// §14 A: an orphan reference end is a Checks row of its own, kept from each commit's ops: it
/// comes with the end, goes when the cable is continued, and comes back on undo.
#[test]
fn an_orphan_reference_is_a_check_kept_by_its_update_map() {
    let mut h = H::new(0x0271);
    let supply = h.first_page();
    for y in [0.0, 5.0] {
        h.run("wire", json!({"points": [[60.0, y], [120.0, y]]})).unwrap();
    }
    let c1: CableId = id(&h.run("cableslash", json!({"from": [88, -3], "to": [92, 12], "tag": "C1"})).unwrap(), "cable");
    // Kept from here on.
    assert!(orphans(&mut h).is_empty());
    h.run("cableref", json!({"cable": c1.hex(), "at": [120, 5]})).unwrap();
    assert_eq!(orphans(&mut h), ["-C01 /?"]);
    kept_is_built(&mut h);
    let row = h.cfg(|p| p.checks().rows().find(|c| c.kind == Kind::OrphanReference).cloned()).unwrap();
    assert_eq!(row.page, Some(supply));
    assert_eq!(row.severity(), ecad_cadcraft::checks::Severity::Warning);
    let v = h.run("checks", json!({"kinds": ["orphan-reference"]})).unwrap();
    assert_eq!(v["total"], 1, "{v}");
    assert_eq!(v["checks"][0]["which"], "-C01 /?");
    assert!(v["checks"][0]["object"]["reference"].is_string(), "{v}");
    // Continued on another page: the end has a partner.
    h.run("newpage", json!({"title": "Field"})).unwrap();
    h.run("cablecontinue", json!({"cable": c1.hex(), "at": [20, 5], "toward": [40, 5]})).unwrap();
    assert!(orphans(&mut h).is_empty());
    kept_is_built(&mut h);
    h.cmdline("u");
    assert_eq!(orphans(&mut h), ["-C01 /?"]);
    kept_is_built(&mut h);
    // The tag is part of the row's words.
    h.run("setprop", json!({"cable": c1.hex(), "key": "tag", "value": "W01"})).unwrap();
    assert_eq!(orphans(&mut h), ["-W01 /?"]);
    kept_is_built(&mut h);
}

/// §14 A, §10: `prerelease` groups the check, says what would be written and what `release` would
/// refuse, and records nothing.
#[test]
fn prerelease_groups_the_check_and_records_nothing() {
    let mut f = fixture("pre");
    let head = f.h.head();
    let v = f.h.run("prerelease", json!({})).unwrap();
    assert_eq!(f.h.head(), head);
    assert_eq!(v["rev"], "B");
    assert_eq!(v["state"], "pending");
    assert!(v.get("refusal").is_none(), "{v}");
    let group = |g: &str| v["groups"].as_array().unwrap().iter().find(|x| x["group"] == g).cloned().unwrap_or(Value::Null);
    assert_eq!(group("orphans")["rows"][0]["kind"], "orphan-reference", "{v}");
    assert_eq!(group("orphans")["rows"][0]["which"], "-C01 /?");
    assert!(group("open-ends")["count"].as_u64().unwrap() >= 3, "three loose wires: {v}");
    assert!(group("errors").is_null());
    assert_eq!(v["writes"][0]["file"], "D-1001 Rev B.pdf");
    assert!(v["writes"].as_array().unwrap().iter().any(|w| w["file"] == "D-1001 Rev B checks.csv"));
    assert_eq!(v["not_included"][0], json!({"slot": "terminal-plan", "why": release::NO_STRIPS}));
    assert_eq!(v["folder"], json!(folder(&f, "B").to_string_lossy()));
    assert_eq!(v["exists"], false);
    // Each refusal is said up front: never saved, a needed field empty, read-only.
    f.h.cfg(|p| p.path = None);
    assert_eq!(f.h.run("prerelease", json!({})).unwrap()["refusal"], release::SAVE_FIRST);
    f.h.cfg(|p| p.path = Some("x/demo.ecad".into()));
    f.h.run("revision", json!({"drawn": null})).unwrap();
    assert_eq!(f.h.run("prerelease", json!({})).unwrap()["refusal"], ecad_cadcraft::revisions::FILL_FIRST);
    let _ = std::fs::remove_dir_all(&f.dir);
}

/// §14 D: releasing writes the folder from the state as issued, issues with FP1, saves, and the
/// manifest's hashes match the files.
#[test]
fn release_issues_saves_and_writes_a_stamped_folder() {
    let mut f = fixture("go");
    let head = f.h.head();
    let v = f.h.run("release", json!({})).unwrap();
    assert_eq!(f.h.head(), head + 1, "one transaction: {v}");
    assert_eq!(v["transactions"], json!([head + 1]));
    assert_eq!(v["saved"], true);
    assert_eq!(v["finished"], true);
    let out = folder(&f, "B");
    assert_eq!(v["folder"], json!(out.to_string_lossy()));
    assert!(v["message"].as_str().unwrap().starts_with(&format!("Rev B released to {}: 1 sheet, 4 lists, ", out.to_string_lossy())), "{v}");
    assert_eq!(
        names(&out),
        [
            "D-1001 Rev B BOM.csv",
            "D-1001 Rev B cable schedule.csv",
            "D-1001 Rev B cables.csv",
            "D-1001 Rev B checks.csv",
            "D-1001 Rev B wires.csv",
            "D-1001 Rev B.pdf",
            "release.json"
        ]
    );
    assert!(!release::partial_of(&out).exists());
    // The issue transaction carries FP1.
    let (label, ops) = f.h.p(|p| {
        let t = p.ledger.log().last().unwrap();
        (t.label.clone(), t.ops.clone())
    });
    assert_eq!(label, "Issue Rev B");
    assert!(
        ops.iter().any(|o| matches!(o, Op::SetProperty { object: ObjectRef::Project, key, after: Some(a), .. }
        if key == "rev.2.released" && a == "release/Rev B")),
        "{ops:?}"
    );
    assert!(f.h.p(|p| !p.unsaved()), "saved");
    // The manifest.
    let m = manifest(&out);
    assert_eq!(m["ecad_release"], 1);
    assert_eq!(m["rev"], "B");
    assert_eq!(m["drawing_number"], "D-1001");
    assert_eq!(m["description"], "Terminals added");
    assert_eq!(m["issued"]["transaction"], head + 1);
    assert_eq!(m["issued"]["point"], head);
    assert_eq!(m["checks"]["accepted_errors"], false);
    assert!(m["written"].as_str().unwrap().ends_with('Z'));
    assert_eq!(m["not_included"].as_array().unwrap().len(), 1, "no strips: no terminal plan (brief 028)");
    for file in m["files"].as_array().unwrap() {
        let bytes = std::fs::read(out.join(file["name"].as_str().unwrap())).unwrap();
        assert_eq!(file["sha256"], release::sha256_hex(&bytes), "{file}");
        assert_eq!(file["bytes"], bytes.len());
    }
    let pdf = std::fs::read(out.join("D-1001 Rev B.pdf")).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    let checks = std::fs::read_to_string(out.join("D-1001 Rev B checks.csv")).unwrap();
    assert!(checks.starts_with("\u{feff}Severity,Group,Kind,Where,What,Accepted\r\n"), "{checks}");
    assert!(checks.contains("warning,Orphans,Cable reference with no other end,"), "{checks}");
    assert!(checks.contains("-C01 /?"), "{checks}");
    // What was printed is the state as issued: its title block and revision list read Rev B.
    let issued = f.h.p(|p| ecad_cadcraft::history::issued_state(p, "B").unwrap().2);
    assert_eq!(ecad_cadcraft::revisions::issued(&issued).first().map(|r| r.rev().to_string()).as_deref(), Some("B"));
    // The Revisions table knows where it went.
    let row = f.h.p(|p| ecad_cadcraft::revisions::issued(p.ledger.document()).into_iter().next().unwrap());
    assert_eq!(row.released.as_deref(), Some("release/Rev B"));
    // Undo asks first, saying the folder stays; it removes the issue with FP1's key, not the folder.
    let e = f.h.run("u", Value::Null).unwrap_err();
    assert!(e.contains("Undo the issue? This removes the release mark. Its release folder stays on disk."), "{e}");
    f.h.run("undoissue", json!({})).unwrap();
    assert!(!f.h.doc().properties.contains_key("rev.2.released"));
    assert!(out.join("release.json").exists());
    let _ = std::fs::remove_dir_all(&f.dir);
}

/// §14 B: errors stop a release until accepted, with the way out; accepted, the manifest and
/// `checks.csv` say so.
#[test]
fn errors_need_release_anyway() {
    let mut f = fixture("err");
    f.h.run("place", json!({"symbol": "continuation", "at": [300, 100], "name": "24V"})).unwrap();
    f.h.run("place", json!({"symbol": "continuation", "at": [300, 140], "rotation": 180, "name": "0V"})).unwrap();
    f.h.run("wire", json!({"points": [[300, 100], [300, 140]]})).unwrap();
    let head = f.h.head();
    let v = f.h.run("prerelease", json!({})).unwrap();
    assert_eq!(v["errors"], 1, "{v}");
    let e = f.h.run("release", json!({})).unwrap_err();
    assert!(e.starts_with("1 error is open (Net joins two potentials, "), "{e}");
    assert!(e.ends_with("Fix it, or give accept: \"errors\" to release with it; checks.csv lists it."), "{e}");
    assert_eq!(f.h.head(), head, "nothing issued");
    assert!(!f.dir.join("release").exists(), "nothing written");
    let v = f.h.run("release", json!({"accept": "errors"})).unwrap();
    assert!(v["message"].as_str().unwrap().contains("1 error and "), "{v}");
    let out = folder(&f, "B");
    assert_eq!(manifest(&out)["checks"]["accepted_errors"], true);
    let checks = std::fs::read_to_string(out.join("D-1001 Rev B checks.csv")).unwrap();
    assert!(checks.contains("error,Errors,Net joins two potentials,") && checks.contains(",yes\r\n"), "{checks}");
    let _ = std::fs::remove_dir_all(&f.dir);
}

/// §14 C (brief 026 in `dev`): provisional items refuse a release without a choice, with both
/// ways forward; `"finalise"` records Finalise numbering, then the issue; `"issue"` only the issue.
#[test]
fn provisional_items_follow_026s_choice() {
    for way in ["finalise", "issue"] {
        let mut f = fixture(&format!("prov-{way}"));
        f.h.run("numbering", json!({"provisional": true})).unwrap();
        f.h.run("place", json!({"symbol": "coil", "at": [250, 150]})).unwrap();
        let pre = f.h.run("prerelease", json!({})).unwrap();
        assert!(pre["provisional"].as_u64().unwrap() >= 1, "{pre}");
        let refusal = pre["refusal"].as_str().unwrap().to_string();
        assert!(refusal.contains("Give provisional: \"finalise\""), "{refusal}");
        let head = f.h.head();
        let e = f.h.run("release", json!({})).unwrap_err();
        assert_eq!(e, refusal);
        assert_eq!(f.h.head(), head, "nothing recorded");
        // `"finalise"` records two transactions (Finalise, then the issue): past the harness's
        // one-per-command check.
        let v = f.h.s.execute("release", &json!({"provisional": way})).unwrap();
        let log: Vec<String> = f.h.p(|p| p.ledger.log().iter().skip(head as usize).map(|t| t.label.clone()).collect());
        match way {
            "finalise" => {
                assert_eq!(log, ["Finalise numbering", "Issue Rev B"], "{v}");
                assert_eq!(v["transactions"], json!([head + 1, head + 2]));
            }
            _ => {
                assert_eq!(log, ["Issue Rev B"], "{v}");
                assert!(v["message"].as_str().unwrap().contains("issued)"), "{v}");
            }
        }
        let _ = std::fs::remove_dir_all(&f.dir);
    }
}

/// §5.2, AC 3: a failure injected at each step. Before the issue (staging, the issue itself):
/// nothing recorded, nothing on disk. After it (save, manifest, rename): the issue stays, the
/// message says how to finish, and releasing the issued revision again finishes the folder.
#[test]
fn a_failure_at_each_step_leaves_nothing_half_done() {
    for step in [Step::Partial, Step::Issue, Step::Save, Step::Manifest, Step::Rename] {
        let mut f = fixture(&format!("fail-{step:?}"));
        let out = folder(&f, "B");
        let head = f.h.head();
        f.h.cfg(|p| p.faults.fail_release_at = Some(step));
        let r = f.h.run("release", json!({}));
        match step {
            Step::Partial | Step::Issue => {
                let e = r.unwrap_err();
                assert!(e.contains("injected failure"), "{step:?}: {e}");
                if step == Step::Partial {
                    assert!(e.starts_with("Release stopped: ") && e.contains("Nothing was issued or written."), "{e}");
                }
                assert_eq!(f.h.head(), head, "{step:?}: nothing issued");
                assert!(!out.exists() && !release::partial_of(&out).exists(), "{step:?}: nothing on disk");
            }
            Step::Save => {
                let v = r.unwrap();
                assert_eq!(f.h.head(), head + 1, "issued");
                assert_eq!(v["saved"], false);
                assert_eq!(v["finished"], true, "the folder is still finished");
                assert!(v["message"].as_str().unwrap().contains("The project could not be saved: "), "{v}");
                assert!(v["message"].as_str().unwrap().contains("Save it before closing, or Rev B is not kept."), "{v}");
                assert!(f.h.p(|p| p.unsaved()));
                assert!(out.join("release.json").exists());
            }
            Step::Manifest | Step::Rename => {
                let v = r.unwrap();
                assert_eq!(f.h.head(), head + 1, "{step:?}: the issue stays");
                assert_eq!(v["saved"], true);
                assert_eq!(v["finished"], false);
                let m = v["message"].as_str().unwrap();
                assert!(m.contains("Rev B is issued, but its folder could not be finished (injected failure"), "{m}");
                assert!(m.contains("release Rev B again to write the folder."), "{m}");
                assert!(!out.exists());
                assert!(release::partial_of(&out).exists(), "the staged files are kept");
                // Releasing Rev B again writes it from its issued state, recording nothing.
                let head = f.h.head();
                let v = f.h.run("release", json!({"rev": "B"})).unwrap();
                assert_eq!(f.h.head(), head);
                assert_eq!(v["transactions"], json!([]));
                assert!(out.join("release.json").exists() && !release::partial_of(&out).exists());
            }
        }
        let _ = std::fs::remove_dir_all(&f.dir);
    }
}

/// A folder named like the staging folder but not made by a release is never cleared: the
/// release stops before the issue and says what to do.
#[test]
fn a_foreign_partial_folder_is_never_deleted() {
    let mut f = fixture("foreign");
    let partial = release::partial_of(&folder(&f, "B"));
    std::fs::create_dir_all(&partial).unwrap();
    std::fs::write(partial.join("mine.txt"), "keep me").unwrap();
    let head = f.h.head();
    let e = f.h.run("release", json!({})).unwrap_err();
    assert!(e.contains("exists and was not made by a release; move it, or choose another folder"), "{e}");
    assert!(e.contains("Nothing was issued or written."), "{e}");
    assert_eq!(f.h.head(), head);
    assert_eq!(std::fs::read_to_string(partial.join("mine.txt")).unwrap(), "keep me");
    let _ = std::fs::remove_dir_all(&f.dir);
}

/// §14 F, §8, §9: writing an issued revision's folder records nothing and gives the same files;
/// an existing folder is never overwritten silently.
#[test]
fn an_issued_revision_is_written_again_and_existing_folders_are_kept() {
    let mut f = fixture("again");
    // Rev A was issued without a folder: write it now, read-only.
    f.h.cfg(|p| p.persistence = ecad_cadcraft::Persistence::Unavailable);
    let e = f.h.run("release", json!({})).unwrap_err();
    assert!(e.starts_with("The project is read-only") && e.contains("give rev"), "{e}");
    let head = f.h.head();
    let v = f.h.run("release", json!({"rev": "A"})).unwrap();
    assert_eq!(f.h.head(), head, "nothing recorded");
    let a = folder(&f, "A");
    assert_eq!(v["folder"], json!(a.to_string_lossy()));
    assert!(v["message"].as_str().unwrap().ends_with("(written again; nothing recorded)."), "{v}");
    let before: Vec<(String, Vec<u8>)> =
        names(&a).into_iter().filter(|n| n != "release.json").map(|n| (n.clone(), std::fs::read(a.join(&n)).unwrap())).collect();
    assert!(before.iter().any(|(n, _)| n == "D-1001 Rev A.pdf"));
    // The same again: refused before anything is written, with the compare and the ways on.
    let e = f.h.run("release", json!({"rev": "A"})).unwrap_err();
    assert!(e.contains("exists (6 files the same)"), "{e}");
    assert!(e.contains("Give existing: \"add-missing\", \"replace\""), "{e}");
    assert!(!release::partial_of(&a).exists());
    // Keep both: `Rev A (2)`, byte-equal (the PDF too: CADCraft's writer embeds no time).
    let v = f.h.run("release", json!({"rev": "A", "existing": "keep-both"})).unwrap();
    let a2 = f.dir.join("release").join("Rev A (2)");
    assert_eq!(v["folder"], json!(a2.to_string_lossy()));
    assert_eq!(v["compare"]["same"].as_array().unwrap().len(), 6, "{v}");
    for (n, bytes) in &before {
        assert_eq!(&std::fs::read(a2.join(n)).unwrap(), bytes, "{n}");
    }
    // Replace: the old folder is moved to release/replaced, nothing deleted.
    let v = f.h.run("release", json!({"rev": "A", "existing": "replace"})).unwrap();
    assert_eq!(v["folder"], json!(a.to_string_lossy()));
    let replaced = names(&f.dir.join("release").join("replaced"));
    assert_eq!(replaced.len(), 1);
    assert!(replaced[0].starts_with("Rev A 20"), "{replaced:?}");
    // Add missing: a file lost from the folder (and its manifest row) is put back.
    let lost = "D-1001 Rev A wires.csv";
    std::fs::remove_file(a.join(lost)).unwrap();
    let mut m = manifest(&a);
    m["files"].as_array_mut().unwrap().retain(|x| x["name"] != lost);
    std::fs::write(a.join("release.json"), serde_json::to_vec(&m).unwrap()).unwrap();
    let v = f.h.run("release", json!({"rev": "A", "existing": "add-missing"})).unwrap();
    assert_eq!(v["compare"]["new"], json!([lost]), "{v}");
    assert!(a.join(lost).exists());
    assert!(manifest(&a)["files"].as_array().unwrap().iter().any(|x| x["name"] == lost));
    // The pending revision cannot be named as `rev`.
    let e = f.h.run("release", json!({"rev": "B"})).unwrap_err();
    assert_eq!(e, "Rev B is not issued yet; release without `rev` to issue it and write its folder.");
    assert_eq!(f.h.head(), head, "nothing recorded by any of it");
    let _ = std::fs::remove_dir_all(&f.dir);
}

/// Design 027 §6, Q11: nothing changed since the last issue is a warning, never a refusal.
#[test]
fn nothing_changed_since_the_last_issue_is_a_warning() {
    let mut f = fixture("same");
    let v = f.h.run("prerelease", json!({})).unwrap();
    assert_eq!(v["warning"], "Nothing has changed since Rev A.");
    f.h.run("wire", json!({"points": [[60, 40], [120, 40]]})).unwrap();
    assert!(f.h.run("prerelease", json!({})).unwrap().get("warning").is_none());
    let _ = std::fs::remove_dir_all(&f.dir);
}

/// Design 027 §16 (019 follow-up): Reopen Revision and the History point of a revision show it as
/// issued: the title block's Rev box reads that revision, not the one before.
#[test]
fn reopening_a_revision_shows_it_in_its_title_block() {
    let mut f = fixture("reopen");
    f.h.run("issuerevision", json!({})).unwrap();
    f.h.run("wire", json!({"points": [[60, 40], [120, 40]]})).unwrap();
    for (cmd, params) in [("openrevision", json!({"rev": "B"})), ("showpoint", json!({"revision": "B"}))] {
        f.h.run(cmd, params).unwrap();
        let doc = f.h.p(|p| p.revision_view.as_ref().unwrap().doc.clone());
        let shown = ecad_cadcraft::revisions::issued(&doc);
        assert_eq!(shown.iter().map(|r| r.rev().to_string()).collect::<Vec<_>>(), ["B", "A"], "{cmd}");
        assert_eq!(doc.conductors.len() + 1, f.h.doc().conductors.len(), "{cmd}: as it was");
        let frame = ecad_cadcraft::frame::FrameInput::of(&doc, f.supply, &ecad_model::book::Book::of(&doc));
        assert_eq!(frame.rev, "B", "{cmd}");
        assert_eq!(frame.revisions.first().map(|r| r[0].as_str()), Some("B"), "{cmd}");
        f.h.run("closerevision", json!({})).unwrap();
    }
    let _ = (f.c1, std::fs::remove_dir_all(&f.dir));
}

/// Review of PR #54, 1: a hostile or odd FP1 value (`""`, `"."`, `"release"`, `".."`, an
/// absolute folder that no release wrote) is never used as the folder: re-writing falls back to
/// `release/Rev A`, so Replace and Keep both can never move or write beside the project's own
/// folder or a foreign one. A `folder` that is the project's folder, holds it or lies above it is
/// refused before anything is written.
#[test]
fn a_release_never_lands_on_the_project_or_a_foreign_folder() {
    let mut f = fixture("land");
    let foreign = tmp("land-foreign");
    std::fs::write(foreign.join("keep.txt"), "mine").unwrap();
    let project_file = f.dir.join("demo.ecad");
    std::fs::write(&project_file, "stands in for the project file").unwrap();
    let foreign_text = foreign.to_string_lossy().into_owned();
    let head = f.h.head();
    for (i, v) in ["", ".", "release", "..", "release/replaced", foreign_text.as_str()].into_iter().enumerate() {
        // Write FP1 past the commands, as a hostile file could hold it.
        f.h.cfg(|p| {
            let doc = p.ledger.document();
            let before = doc.properties.get("rev.1.released").cloned();
            p.ledger
                .commit(
                    ecad_model::Author::User,
                    "hostile",
                    vec![Op::SetProperty { object: ObjectRef::Project, key: "rev.1.released".into(), before, after: Some(v.into()) }],
                )
                .unwrap();
        });
        let pre = f.h.run("prerelease", json!({"rev": "A"})).unwrap();
        assert_eq!(pre["folder"], json!(folder(&f, "A").to_string_lossy()), "FP1 {v:?}");
        let r = f.h.run("release", json!({"rev": "A", "existing": if i == 0 { "keep-both" } else { "replace" }}));
        let r = r.unwrap_or_else(|e| panic!("FP1 {v:?}: {e}"));
        assert_eq!(r["folder"].as_str().map(|s| s.contains("Rev A")), Some(true), "FP1 {v:?}: {r}");
        assert_eq!(std::fs::read_to_string(&project_file).unwrap(), "stands in for the project file", "FP1 {v:?}");
        assert_eq!(std::fs::read_to_string(foreign.join("keep.txt")).unwrap(), "mine", "FP1 {v:?}");
        assert!(!foreign.join("release.json").exists(), "FP1 {v:?}");
    }
    // `folder` naming the project's folder, a folder above it, or release/ itself: refused.
    let above = f.dir.parent().unwrap().to_string_lossy().into_owned();
    for (folder_arg, existing) in
        [(".", "replace"), ("", "replace"), (above.as_str(), "replace"), ("release", "keep-both"), ("release/replaced", "replace")]
    {
        if folder_arg.is_empty() {
            continue; // empty means the default folder
        }
        for rev in [Some("A"), None] {
            let mut params = json!({"folder": folder_arg, "existing": existing});
            if let Some(r) = rev {
                params["rev"] = json!(r);
            }
            let heads = f.h.head();
            let e = f.h.run("release", params).unwrap_err();
            assert!(e.contains("cannot be a release folder"), "{folder_arg:?}: {e}");
            assert_eq!(f.h.head(), heads, "nothing recorded");
        }
        assert_eq!(std::fs::read_to_string(&project_file).unwrap(), "stands in for the project file");
    }
    assert!(f.h.head() >= head);
    let _ = std::fs::remove_dir_all(&foreign);
    let _ = std::fs::remove_dir_all(&f.dir);
}

/// Review of PR #54, 2: with Finalise chosen, the Release anyway gate looks at the rows the
/// folder ships, those after Finalise. Two cables holding one provisional tag are an error before
/// Finalise and none after: the release goes on without `accept`, and `checks.csv` has no error.
#[test]
fn with_finalise_the_errors_gate_reads_the_finalised_rows() {
    let mut f = fixture("gate");
    for y in [30.0, 35.0] {
        f.h.run("wire", json!({"points": [[60.0, y], [120.0, y]]})).unwrap();
    }
    let c2: CableId = id(&f.h.run("cableslash", json!({"from": [88, 27], "to": [92, 38]})).unwrap(), "cable");
    let c1 = f.c1;
    // Both hold `-W?1`, as a hostile or older file could.
    f.h.cfg(|p| {
        let doc = p.ledger.document();
        let ops: Vec<Op> = [c1, c2]
            .iter()
            .map(|k| Op::SetProperty {
                object: ObjectRef::Cable(*k),
                key: "tag".into(),
                before: doc.cables[k].properties.get("tag").cloned(),
                after: Some("-W?1".into()),
            })
            .collect();
        p.ledger.commit(ecad_model::Author::User, "shared provisional tag", ops).unwrap();
    });
    let pre = f.h.run("prerelease", json!({})).unwrap();
    assert!(pre["errors"].as_u64().unwrap() >= 1, "an error before Finalise: {pre}");
    let head = f.h.head();
    let v = f.h.s.execute("release", &json!({"provisional": "finalise"})).unwrap();
    assert_eq!(v["transactions"], json!([head + 1, head + 2]), "{v}");
    let checks = std::fs::read_to_string(folder(&f, "B").join("D-1001 Rev B checks.csv")).unwrap();
    assert!(!checks.contains("\r\nerror,"), "{checks}");
    assert_eq!(manifest(&folder(&f, "B"))["checks"]["errors"], 0);
    let _ = std::fs::remove_dir_all(&f.dir);
}

/// Review of PR #54, 3: the badge's counts are kept by the update map, never recounted: after
/// each edit they equal a count of the rows; an edit that changes no row (a page renamed, the
/// project's title) moves neither `Checks::changed`, the report models' check key nor the
/// Release dialog's key, so nothing is recounted or rebuilt.
#[test]
fn the_badge_counts_are_kept_and_an_unrelated_edit_recounts_nothing() {
    let mut f = fixture("badge");
    let counted = |h: &mut H| {
        h.cfg(|p| {
            p.checks();
            p.outputs();
            let tally = |rows: &[ecad_cadcraft::checks::Check]| {
                let n = |s| rows.iter().filter(|c| c.severity() == s).count();
                use ecad_cadcraft::checks::Severity::*;
                (n(Error), n(Warning), n(Info))
            };
            let rows: Vec<_> = p.checks.as_ref().unwrap().rows().cloned().collect();
            let o = p.outputs.as_ref().unwrap();
            (p.checks.as_ref().unwrap().counts(), tally(&rows), o.check_counts(), tally(&o.checks()))
        })
    };
    let edits = [
        ("wire", json!({"points": [[60, 60], [120, 60]]})),
        ("place", json!({"symbol": "coil", "at": [250, 150]})),
        ("place", json!({"symbol": "continuation", "at": [300, 100], "name": "24V"})),
        ("place", json!({"symbol": "continuation", "at": [300, 140], "rotation": 180, "name": "0V"})),
        ("wire", json!({"points": [[300, 100], [300, 140]]})),
        ("projectprops", json!({"location": "+CP1"})),
    ];
    for (cmd, params) in edits {
        f.h.run(cmd, params.clone()).unwrap();
        let (kept, rows, okept, orows) = counted(&mut f.h);
        assert_eq!(kept, rows, "{cmd} {params}");
        assert_eq!(okept, orows, "{cmd} {params}");
    }
    f.h.cmdline("u");
    let (kept, rows, _, _) = counted(&mut f.h);
    assert_eq!(kept, rows, "after undo");
    assert!(kept.0 + kept.1 > 0, "something to count: {kept:?}");
    // Unrelated edits: nothing changes in the rows, so nothing is recounted or rebuilt.
    let key = |h: &mut H| h.p(|p| (p.checks.as_ref().unwrap().changed, p.outputs.as_ref().unwrap().checks_key(), release::dialog_key(p, false)));
    let before = key(&mut f.h);
    let page = f.supply;
    f.h.run("renamepage", json!({"page": page.hex(), "title": "Supply renamed"})).unwrap();
    f.h.run("projectprops", json!({"title": "Another title"})).unwrap();
    assert_eq!(key(&mut f.h), before, "an unrelated edit");
    // A related one does.
    f.h.run("wire", json!({"points": [[60, 80], [120, 80]]})).unwrap();
    assert_ne!(key(&mut f.h).0, before.0, "a loose wire is a new row");
    let _ = std::fs::remove_dir_all(&f.dir);
}

/// Brief 028 fills 027's two slots: with a terminal strip and a cable, the folder holds the
/// terminal plan and the cable schedule, their columns named by 028's JSON keys (design 028 §10);
/// a spare core has no `from`.
#[test]
fn a_release_writes_the_terminal_plan_and_cable_schedule() {
    let mut f = fixture("strips");
    f.h.run("terminals", json!({"from": [70, -2], "to": [70, 12]})).unwrap();
    f.h.run("setprop", json!({"cable": f.c1.hex(), "key": "cores", "value": "4G"})).unwrap();
    let pre = f.h.run("prerelease", json!({})).unwrap();
    assert!(pre.get("not_included").is_none_or(|n| n.as_array().is_some_and(Vec::is_empty)), "{pre}");
    let v = f.h.run("release", json!({})).unwrap();
    let out = folder(&f, "B");
    assert!(v["message"].as_str().unwrap().contains("5 lists"), "{v}");
    let read = |name: &str| std::fs::read_to_string(out.join(format!("D-1001 Rev B {name}.csv"))).unwrap();
    let plan = read("terminal plan");
    let mut lines = plan.lines();
    assert_eq!(
        lines.next().unwrap().trim_start_matches('\u{feff}'),
        "tag,location,offered,label,type,state,inside_target,inside_wire,bridge_group,outside_wire,cable,core,outside_target,place,kind,accessory,gap,marker,links"
    );
    assert_eq!(lines.count(), 3, "{plan}");
    let schedule = read("cable schedule");
    let rows: Vec<&str> = schedule.lines().collect();
    assert_eq!(rows[0].trim_start_matches('\u{feff}'), "cable,type,offer,label,state,wire,from,to");
    assert_eq!(rows.len(), 1 + 4, "{schedule}");
    let spare = rows.iter().find(|r| r.contains(",spare,")).unwrap();
    assert!(spare.ends_with("spare,,,"), "a spare core: no wire, from or to: {spare}");
}
