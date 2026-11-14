//! Brief 027 headless in the trial (design 027 §14 A, D, F): on the demo, the Release dialog lists
//! the check by group, a row goes to its place with the dialog still open, Release writes the
//! folder and closes, the Revisions window's Folder cell reads Open, and writing an issued
//! revision again into its existing folder asks first (Keep both). Screenshots are written when
//! `ECAD_TESTER_SHOTS` names a folder and a wgpu adapter exists; they are for the owner's
//! approval, never committed.

use std::path::PathBuf;
use std::sync::Arc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::checks_ui::demo_file;
use crate::mouse_tester::{Deltas, harness_with, head, ok, shot};

type H = Harness<'static, ecad_trial::Trial>;

fn save(h: &H, deltas: &Deltas, name: &str) {
    let Some(dir) = std::env::var_os("ECAD_TESTER_SHOTS").map(PathBuf::from) else { return };
    let _ = std::fs::create_dir_all(&dir);
    if shot(h, deltas, &dir.join(name)).is_none() {
        eprintln!("no wgpu adapter: {name} skipped");
    }
}

fn click(h: &mut H, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(4);
}

#[test]
fn the_release_dialog_checks_releases_and_asks_before_replacing() {
    ecad_cadcraft::printbook::install_io();
    let dir = std::env::temp_dir().join(format!("ecad-027-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = demo_file(&dir);
    let deltas: Deltas = Arc::default();
    let mut h = harness_with(Some(deltas.clone()));
    h.state_mut().cad.session.viewport_px = (1280.0, 800.0);
    h.state_mut().close_project();
    h.run_steps(2);
    h.state_mut().open_project(&path).unwrap();
    h.run_steps(3);
    ok(&mut h, r#"projectprops {"drawing_number": "D-1001"}"#);

    // Publish ▸ Release opens the dialog (UI rev 3: the bottom bar's revision opens History, and
    // Publish shows the print choices, board 4.01): Rev A, its fields empty, Release disabled
    // until Description and Drawn are filled.
    h.state_mut().set_workspace(ecad_trial::shell::Workspace::Publish);
    h.run_steps(3);
    assert!(h.state().release.is_none(), "entering Publish opens no dialog");
    click(&mut h, ecad_trial::publish::RELEASE);
    h.run_steps(2);
    assert!(h.state().release.is_some(), "the dialog is open");
    let d = h.state().release.clone().unwrap();
    assert_eq!(d.view.as_ref().unwrap().rev, "A");
    assert_eq!(d.blocked().as_deref(), Some(ecad_cadcraft::revisions::FILL_FIRST));
    assert!(h.query_by_label("ecad.release.go").is_some());
    assert!(
        h.query_by_label("ecad.release.group.open-ends").is_some() || h.query_by_label("ecad.release.group.parts").is_some(),
        "the check is grouped"
    );
    // The fields write `revision` (one transaction each, as in Revisions).
    let before = head(&mut h);
    ok(&mut h, r#"revision {"description": "First issue", "drawn": "CVA"}"#);
    h.run_steps(3);
    assert_eq!(head(&mut h), before + 1);
    let d = h.state().release.clone().unwrap();
    assert_eq!(d.fields[2], "First issue", "the dialog follows the project");
    assert_eq!(d.blocked(), None);
    save(&h, &deltas, "027-release.png");
    // A row goes to its place; the dialog stays open.
    let row = ["open-ends", "parts", "orphans", "other"].iter().map(|g| format!("ecad.release.row.{g}.0")).find(|l| h.query_by_label(l).is_some());
    if let Some(l) = row {
        click(&mut h, &l);
        assert!(h.state().release.is_some(), "still open");
    }

    // Release: the folder is written beside the project, the dialog closes, the project is saved.
    let before = head(&mut h);
    click(&mut h, "ecad.release.go");
    assert!(h.state().release.is_none(), "closed after releasing");
    assert_eq!(head(&mut h), before + 1, "Issue Rev A");
    let out = dir.join("release").join("Rev A");
    assert!(out.join("release.json").exists(), "{out:?}");
    assert!(out.join("D-1001 Rev A.pdf").exists());

    // Revisions: Rev A's Folder cell reads Open.
    h.state_mut().open_revisions();
    h.run_steps(3);
    let r = h.state().revisions.clone().unwrap();
    assert!(r.issued[0].folder_exists, "{:?}", r.issued[0]);
    assert!(h.query_by_label("ecad.revisions.folder.A").is_some());
    assert!(h.query_by_label("ecad.revisions.release").is_some());
    save(&h, &deltas, "027-revisions.png");

    // Writing Rev A again into its folder asks first; Keep both writes `Rev A (2)`.
    h.state_mut().revisions = None;
    h.state_mut().open_release(Some("A".into()));
    h.run_steps(3);
    let before = head(&mut h);
    click(&mut h, "ecad.release.go");
    assert!(h.state().release.as_ref().and_then(|d| d.exists.clone()).is_some_and(|q| q.contains("exists (")), "the question");
    save(&h, &deltas, "027-exists.png");
    click(&mut h, "ecad.release.exists.keep-both");
    assert!(dir.join("release").join("Rev A (2)").join("release.json").exists());
    assert_eq!(head(&mut h), before, "nothing recorded");
    let _ = std::fs::remove_dir_all(&dir);
}
