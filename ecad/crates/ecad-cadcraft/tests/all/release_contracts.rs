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
    assert_eq!(orphans(&mut h), ["C1 /?"]);
    kept_is_built(&mut h);
    let row = h.cfg(|p| p.checks().rows().find(|c| c.kind == Kind::OrphanReference).cloned()).unwrap();
    assert_eq!(row.page, Some(supply));
    assert_eq!(row.severity(), ecad_cadcraft::checks::Severity::Warning);
    let v = h.run("checks", json!({"kinds": ["orphan-reference"]})).unwrap();
    assert_eq!(v["total"], 1, "{v}");
    assert_eq!(v["checks"][0]["which"], "C1 /?");
    assert!(v["checks"][0]["object"]["reference"].is_string(), "{v}");
    // Continued on another page: the end has a partner.
    h.run("newpage", json!({"title": "Field"})).unwrap();
    h.run("cablecontinue", json!({"cable": c1.hex(), "at": [20, 5], "toward": [40, 5]})).unwrap();
    assert!(orphans(&mut h).is_empty());
    kept_is_built(&mut h);
    h.cmdline("u");
    assert_eq!(orphans(&mut h), ["C1 /?"]);
    kept_is_built(&mut h);
    // The tag is part of the row's words.
    h.run("setprop", json!({"cable": c1.hex(), "key": "tag", "value": "W01"})).unwrap();
    assert_eq!(orphans(&mut h), ["W01 /?"]);
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
    assert_eq!(group("orphans")["rows"][0]["which"], "C1 /?");
    assert!(group("open-ends")["count"].as_u64().unwrap() >= 3, "three loose wires: {v}");
    assert!(group("errors").is_null());
    assert_eq!(v["writes"][0]["file"], "D-1001 Rev B.pdf");
    assert!(v["writes"].as_array().unwrap().iter().any(|w| w["file"] == "D-1001 Rev B checks.csv"));
    assert_eq!(v["not_included"][0], json!({"slot": "terminal-plan", "why": release::COMES_WITH_028}));
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
    assert!(v["message"].as_str().unwrap().starts_with(&format!("Rev B released to {}: 1 sheet, 3 lists, ", out.to_string_lossy())), "{v}");
    assert_eq!(
        names(&out),
        ["D-1001 Rev B BOM.csv", "D-1001 Rev B cables.csv", "D-1001 Rev B checks.csv", "D-1001 Rev B wires.csv", "D-1001 Rev B.pdf", "release.json"]
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
    assert_eq!(m["not_included"].as_array().unwrap().len(), 2);
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
    assert!(checks.contains("C1 /?"), "{checks}");
    // What was printed is the state as issued: its title block and revision list read Rev B.
    let issued = f.h.p(|p| ecad_cadcraft::history::issued_state(p, "B").unwrap().2);
    assert_eq!(ecad_cadcraft::revisions::issued(&issued).first().map(|r| r.rev().to_string()).as_deref(), Some("B"));
    // The Revisions table knows where it went.
    let row = f.h.p(|p| ecad_cadcraft::revisions::issued(p.ledger.document()).into_iter().next().unwrap());
    assert_eq!(row.released.as_deref(), Some("release/Rev B"));
    // Undo removes the issue (with FP1's key); the folder stays on disk.
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
    assert!(e.contains("exists (5 files the same)"), "{e}");
    assert!(e.contains("Give existing: \"add-missing\", \"replace\""), "{e}");
    assert!(!release::partial_of(&a).exists());
    // Keep both: `Rev A (2)`, byte-equal (the PDF too: CADCraft's writer embeds no time).
    let v = f.h.run("release", json!({"rev": "A", "existing": "keep-both"})).unwrap();
    let a2 = f.dir.join("release").join("Rev A (2)");
    assert_eq!(v["folder"], json!(a2.to_string_lossy()));
    assert_eq!(v["compare"]["same"].as_array().unwrap().len(), 5, "{v}");
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
