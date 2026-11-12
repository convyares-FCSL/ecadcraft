//! Brief 026 headless in the trial (design 026 §14 check K): on the demo project, the Renumber
//! dialog by element name (preview, Apply, one step), the Devices tab's clash buttons (Swap first),
//! Project settings' provisional tags, the Checks rows, the Issue Revision prompt with Finalise
//! numbering now, and the merge dialog's Make them one device. Screenshots `026-renumber.png`,
//! `026-swap.png`, `026-provisional.png`, `026-checks.png`, `026-issue.png` and
//! `026-merge-one.png` are written when `ECAD_TESTER_SHOTS` names a folder and a wgpu adapter
//! exists.

use std::path::PathBuf;
use std::sync::Arc;

use ecad_model::ComponentId;
use ecad_trial::{checks_panel, project_panel};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use crate::checks_ui::demo_file;
use crate::mouse_tester::{Deltas, doc, harness_with, ok, shot};

type H = Harness<'static, ecad_trial::Trial>;

fn save_shot(h: &H, deltas: &Deltas, name: &str) {
    let Some(dir) = std::env::var_os("ECAD_TESTER_SHOTS").map(PathBuf::from) else { return };
    let _ = std::fs::create_dir_all(&dir);
    if shot(h, deltas, &dir.join(name)).is_none() {
        eprintln!("no wgpu adapter: {name} skipped");
    }
}

fn click(h: &mut H, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(3);
}

fn head(h: &mut H) -> u64 {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.head()).unwrap()
}

fn tagged(h: &mut H, t: &str) -> ComponentId {
    doc(h).components.iter().find(|(_, c)| c.properties.get("tag").is_some_and(|x| x == t)).map(|(k, _)| *k).unwrap()
}

fn open(dir_name: &str) -> (H, Deltas, PathBuf) {
    let dir = std::env::temp_dir().join(format!("{dir_name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = demo_file(&dir);
    let deltas: Deltas = Arc::default();
    let mut h = harness_with(Some(deltas.clone()));
    h.state_mut().cad.session.viewport_px = (1280.0, 800.0);
    h.state_mut().close_project();
    h.run_steps(2);
    h.state_mut().open_project(&path).unwrap();
    h.run_steps(3);
    (h, deltas, dir)
}

#[test]
fn renumber_dialog_swap_provisional_checks_and_issue_by_element_name() {
    let (mut h, deltas, dir) = open("ecad-026-ui");
    // Renumber… from the Numbering group: the dialog with its preview.
    h.state_mut().tool("ecad.renumber").unwrap();
    h.run_steps(3);
    let d = h.state().renumber.clone().expect("the dialog is open");
    assert!(d.preview["summary"].as_str().is_some_and(|s| s.contains("to change")), "{}", d.preview);
    for name in [
        "ecad.renumber.kind.devices",
        "ecad.renumber.scope.all",
        "ecad.renumber.start",
        "ecad.renumber.digits",
        "ecad.renumber.apply",
        "ecad.renumber.cancel",
    ] {
        assert!(h.query_by_label(name).is_some(), "{name}");
    }
    // A start that shifts every tag, so there is something to apply.
    h.state_mut().renumber.as_mut().unwrap().start = "2".into();
    let (c, v) = h.state().renumber.as_ref().unwrap().params(true).unwrap();
    let pv = h.state_mut().cad.session.execute(c, &v).unwrap();
    h.state_mut().renumber.as_mut().unwrap().preview = pv;
    h.run_steps(3);
    save_shot(&h, &deltas, "026-renumber.png");
    let before = head(&mut h);
    click(&mut h, "ecad.renumber.apply");
    assert!(h.state().renumber.is_none(), "Apply closes the dialog");
    assert_eq!(head(&mut h), before + 1, "one step");
    ok(&mut h, "u");
    assert_eq!(head(&mut h), before + 2);

    // The Devices tab: a taken tag offers Swap first; Swap is one step.
    ok(&mut h, r#"place {"symbol":"coil","at":[400,60],"tag":"-K02"}"#);
    click(&mut h, project_panel::DEVICES_TAB);
    let (k1, k2) = (tagged(&mut h, "-K01"), tagged(&mut h, "-K02"));
    h.state_mut().panel.devices.selected = Some(k2);
    h.state_mut().panel_action(project_panel::Action::SetDeviceProperty { component: k2, key: "tag", value: "-K01".into(), conflict: None });
    h.run_steps(3);
    assert!(h.query_by_label("ecad.tagclash.swap").is_some(), "Swap is offered");
    assert!(h.query_by_label("ecad.tagclash.merge").is_some() && h.query_by_label("ecad.tagclash.next").is_some());
    save_shot(&h, &deltas, "026-swap.png");
    let before = head(&mut h);
    click(&mut h, "ecad.tagclash.swap");
    assert_eq!(head(&mut h), before + 1, "{:?}", h.state().panel.devices.error);
    let d = doc(&mut h);
    assert_eq!((d.components[&k1].properties["tag"].as_str(), d.components[&k2].properties["tag"].as_str()), ("-K02", "-K01"));

    // Provisional tags (Project settings → Numbering, typed here), a new coil and a new net.
    ok(&mut h, r#"numbering {"provisional":true}"#);
    let page = doc(&mut h).page_order[0];
    h.state_mut().show_page(page).unwrap();
    h.run_steps(2);
    ok(&mut h, r#"place {"symbol":"coil","at":[150,60]}"#);
    ok(&mut h, r#"wire {"points":[[150,20],[190,20]],"potential":"24V"}"#);
    ok(&mut h, "zoom e");
    h.run_steps(3);
    assert!(doc(&mut h).components.values().any(|c| c.properties.get("tag").is_some_and(|t| t == "-K?1")));
    save_shot(&h, &deltas, "026-provisional.png");
    click(&mut h, checks_panel::CHECKS_TAB);
    h.run_steps(3);
    assert!(h.query_all_by_label(checks_panel::FINALISE).count() >= 2, "Finalise on the provisional rows");
    save_shot(&h, &deltas, "026-checks.png");

    // Issue Revision with provisional items: the prompt, then Finalise numbering now.
    for (f, v) in [("rev", "A"), ("description", "First issue"), ("drawn", "CV")] {
        ok(&mut h, &format!("revision {}", json!({f: v})));
    }
    h.state_mut().open_revisions();
    h.run_steps(3);
    click(&mut h, "ecad.revisions.issue");
    click(&mut h, "Issue");
    assert!(h.state().issue_prompt.is_some(), "the provisional question");
    for name in ["ecad.issue.finalise", "ecad.issue.anyway", "ecad.issue.cancel"] {
        assert!(h.query_by_label(name).is_some(), "{name}");
    }
    save_shot(&h, &deltas, "026-issue.png");
    let before = head(&mut h);
    click(&mut h, "ecad.issue.finalise");
    assert!(h.state().renumber.as_ref().is_some_and(|d| d.finalise), "Finalise numbering opens");
    click(&mut h, "ecad.renumber.apply");
    assert_eq!(head(&mut h), before + 2, "Finalise, then the issue");
    assert!(!doc(&mut h).components.values().any(|c| c.properties.get("tag").is_some_and(|t| t.contains('?'))));
    drop(h);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_merge_dialog_offers_make_them_one_device() {
    let (mut h, deltas, dir) = open("ecad-026-merge");
    ok(&mut h, r#"branch {"name":"Option E"}"#);
    ok(&mut h, r#"place {"symbol":"no","at":[300,300],"tag":"-K40"}"#);
    ok(&mut h, r#"switchbranch {"branch":"main","save":true}"#);
    ok(&mut h, r#"place {"symbol":"coil","at":[300,340],"tag":"-K40"}"#);
    h.run_steps(2);
    h.state_mut().ask_merge(json!({"from": "Option E"}));
    h.run_steps(3);
    let d = h.state().merge.clone().expect("the merge dialog is open");
    let k = d.preview["conflicts"].as_array().unwrap().iter().find(|k| k["kind"] == json!("tag")).cloned().expect("a tag conflict");
    let id = k["id"].as_str().unwrap().to_string();
    assert!(h.query_by_label(&format!("ecad.merge.conflict.{id}.one")).is_some(), "Make them one device");
    save_shot(&h, &deltas, "026-merge-one.png");
    click(&mut h, &format!("ecad.merge.conflict.{id}.one"));
    let before = head(&mut h);
    click(&mut h, "ecad.merge.ok");
    assert_eq!(head(&mut h), before + 1, "one merge step: {:?}", h.state().merge.as_ref().map(|d| d.error.clone()));
    let n = doc(&mut h).components.values().filter(|c| c.properties.get("tag").is_some_and(|t| t == "-K40")).count();
    assert_eq!(n, 1, "one device");
    drop(h);
    let _ = std::fs::remove_dir_all(&dir);
}
