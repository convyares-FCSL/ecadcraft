//! CI on PR 88 (`history_ui`): a page rebuilt after a branch switch failed its check and became
//! unavailable when another thread switched the canvas sheet (`canvassheet`, process-wide)
//! between the projection and the check. This binary flips the sheet on purpose, so it runs apart
//! from the other tests (a process of its own).

#[path = "all/common/mod.rs"]
#[allow(unused_imports)]
mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use common::small_project;
use ecad_cadcraft::Persistence;
use ecad_inspect::EventKind;
use serde_json::{Value, json};

fn raw(h: &mut common::H, cmd: &str, params: Value) -> Value {
    h.s.execute(cmd, &params).unwrap_or_else(|e| panic!("{cmd} {params}: {e}"))
}

fn failures(h: &mut common::H) -> Vec<String> {
    h.p(|p| {
        p.events
            .events()
            .filter(|e| matches!(e.kind, EventKind::PageUnavailable | EventKind::RecoveryVerificationFailed))
            .map(|e| format!("{:?} {:?}: {}", e.kind, e.command, e.message))
            .collect()
    })
}

/// Branch, edit and switch back many times with project styles set (a standard, a red tag, a red
/// 24 V category line, a blue frame) while another thread keeps switching the sheet: every page
/// stays available, and both branches keep the standard and the styles.
#[test]
fn branching_and_switching_back_with_styles_survives_sheet_switches() {
    let dir = std::env::temp_dir().join(format!("ecad-050-sheet-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, ..) = small_project(0x50_5ee7);
    h.cfg(|p| p.persistence = Persistence::File);
    let path = dir.join("p.ecad").display().to_string();
    raw(&mut h, "saveas", json!({"path": path}));
    for (y, pot) in [(60.0, "24V"), (70.0, "L1"), (80.0, "PE")] {
        raw(&mut h, "wire", json!({"points": [[200.0, y], [260.0, y]], "potential": pot}));
    }
    raw(&mut h, "standard", json!({"new": "House"}));
    raw(&mut h, "setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"colour": "RD"}}));
    raw(&mut h, "setstyle", json!({"catalogue": "wire", "id": "dc24", "fields": {"line_colour": "RD"}}));
    raw(&mut h, "setstyle", json!({"catalogue": "frame", "id": "sheet", "fields": {"line_colour": "BU"}}));
    raw(&mut h, "qsave", Value::Null);
    let main_doc = h.doc();
    assert!(main_doc.standard.is_some() && !main_doc.styles.is_empty());

    let stop = Arc::new(AtomicBool::new(false));
    let flipper = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            let mut dark = false;
            while !stop.load(Ordering::Relaxed) {
                dark = !dark;
                ecad_cadcraft::look::set_dark_sheet(dark);
                std::thread::yield_now();
            }
        })
    };
    let mut bad = Vec::new();
    for i in 0..12 {
        raw(&mut h, "branch", json!({"name": format!("B{i}")}));
        raw(&mut h, "wire", json!({"points": [[200.0, 100.0 + f64::from(i)], [260.0, 100.0 + f64::from(i)]], "potential": "24V"}));
        let branch_doc = h.doc();
        assert_eq!(
            (branch_doc.standard.clone(), branch_doc.styles.clone()),
            (main_doc.standard.clone(), main_doc.styles.clone()),
            "the branch carries them"
        );
        raw(&mut h, "switchbranch", json!({"branch": "main", "save": true}));
        assert_eq!(h.doc(), main_doc, "main is as it was");
        bad.extend(failures(&mut h));
        if !bad.is_empty() {
            break;
        }
    }
    stop.store(true, Ordering::Relaxed);
    let _ = flipper.join();
    ecad_cadcraft::look::set_dark_sheet(false);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(bad.is_empty(), "pages became unavailable: {bad:#?}");
}
