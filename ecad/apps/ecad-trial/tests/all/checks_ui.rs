//! Brief 022 headless in the trial (design 022 §15 checks F and I): the demo project's labels,
//! the Checks tab with its go-to, a T without a junction fixed by **Add junction**, and labels at
//! 0, 90, 45 and 135°. Screenshots are written when `ECAD_TESTER_SHOTS` names a folder and a wgpu
//! adapter exists; they are for the owner's approval, never committed.

use std::path::PathBuf;
use std::sync::Arc;

use ecad_cadcraft::checks::Kind;
use ecad_model::PageId;
use ecad_trial::{checks_panel, project_panel};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::mouse_tester::{Deltas, doc, harness_with, ok, pointer_to, screen, shot};

type H = Harness<'static, ecad_trial::Trial>;

fn out() -> Option<PathBuf> {
    let dir = std::env::var_os("ECAD_TESTER_SHOTS").map(PathBuf::from)?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn save(h: &H, deltas: &Deltas, name: &str) {
    if let Some(dir) = out()
        && shot(h, deltas, &dir.join(name)).is_none()
    {
        eprintln!("no wgpu adapter: {name} skipped");
    }
}

/// The demo project (`cargo xtask demo`), written to `dir` through the MCP `execute` tool.
fn demo_file(dir: &std::path::Path) -> String {
    let path = dir.join("demo.ecad").to_string_lossy().into_owned();
    let _ = std::fs::remove_file(&path);
    let mut mcp =
        ecad_mcp::EcadMcp::create(&path, ecad_mcp::Options { write: true, agent: Some(ecad_mcp::demo::AUTHOR.into()) })
            .unwrap();
    ecad_mcp::demo::run(&mut mcp, ecad_mcp::demo::COMMANDS).unwrap();
    let saved = mcp
        .handle(serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": "save", "arguments": {}}}))
        .unwrap();
    assert_ne!(saved["result"]["isError"], serde_json::json!(true), "{saved}");
    drop(mcp);
    path
}

fn page(h: &mut H, n: usize) -> PageId {
    doc(h).page_order[n]
}

fn show(h: &mut H, p: PageId) {
    h.state_mut().show_page(p).unwrap();
    h.run_steps(2);
    ok(h, "zoom e");
    h.run_steps(3);
}

fn click(h: &mut H, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(3);
}

#[test]
fn the_demo_labels_and_the_checks_tab_go_to_a_row() {
    let dir = std::env::temp_dir().join(format!("ecad-022-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = demo_file(&dir);
    let deltas: Deltas = Arc::default();
    let mut h = harness_with(Some(deltas.clone()));
    h.state_mut().cad.session.viewport_px = (1280.0, 800.0);
    h.state_mut().close_project();
    h.run_steps(2);
    h.state_mut().open_project(&path).unwrap();
    h.run_steps(3);

    // Pages 2 and 1: every wire labelled.
    let (p1, p2, p3) = (page(&mut h, 0), page(&mut h, 1), page(&mut h, 2));
    show(&mut h, p2);
    let labels = h
        .state()
        .cad
        .session
        .doc()
        .unwrap()
        .model
        .iter()
        .filter(|e| e.common.layer == ecad_cadcraft::wires::LABEL_LAYER)
        .count();
    assert!(labels >= 12, "{labels} labels on page 2");
    save(&h, &deltas, "022-demo-page2.png");
    show(&mut h, p1);
    save(&h, &deltas, "022-demo-page1.png");

    // The hover lens on the wire under the coil names its net (design 022 §15, step 2).
    show(&mut h, p2);
    let at = screen(&h, 47.5, 0.0);
    pointer_to(&mut h, at);
    h.run_steps(3);
    let lines = h.state().hover_lines().map(<[String]>::to_vec).unwrap_or_default();
    assert!(lines.iter().any(|l| l.starts_with("Net 0V-01")), "{lines:?}");
    assert!(lines.iter().any(|l| l == "Wire number 0V-01"), "{lines:?}");
    // Quick start 5I step 2, word for word.
    for want in [
        "End B: junction",
        "Potential 0V (continuation)",
        "Cross-section 0.5 (0V) \u{b7} Colour Blue (0V)",
        "Net 0V-01 \u{b7} 5 wires \u{b7} -G01:-, -H01:X2, -H02:X2, -K01:A2",
    ] {
        assert!(lines.iter().any(|l| l == want), "{want} missing from {lines:?}");
    }
    save(&h, &deltas, "022-hover-wire.png");

    // The Checks tab: -M01:PE, a click goes to page 3, centres it, selects the motor, rings PE.
    click(&mut h, checks_panel::CHECKS_TAB);
    assert_eq!(h.state().panel.tab, project_panel::Tab::Checks);
    let rows = h.state().panel.checks.rows().to_vec();
    assert_eq!(rows.len(), 4, "{rows:?}");
    let pe = rows.iter().find(|r| r.check.which == "-M01:PE").unwrap().clone();
    assert_eq!(pe.locator, "/3.4");
    click(&mut h, &checks_panel::row_label(&pe));
    assert_eq!(h.state().panel.active, Some(p3));
    assert!(!h.state().cad.session.selection().is_empty(), "the motor is selected");
    let ring = {
        let t = h.state_mut();
        let p = t.project().unwrap().clone();
        p.read(&mut t.cad.session, |p, _| p.goto_ring).unwrap()
    };
    assert!(ring.is_some_and(|(pg, at, _)| pg == p3 && Some(at) == pe.check.at), "{ring:?}");
    save(&h, &deltas, "022-checks.png");

    // A T without a junction on a new page: its row, then Add junction.
    ok(&mut h, r#"newpage {"title":"Tee"}"#);
    ok(&mut h, r#"wire {"points":[[0,0],[60,0]]}"#);
    ok(&mut h, r#"wire {"points":[[30,30],[30,0]],"junction":false}"#);
    ok(&mut h, "zoom e");
    h.run_steps(3);
    let rows = h.state().panel.checks.rows().to_vec();
    let tee = rows.iter().find(|r| r.check.kind == Kind::LooksConnected).expect("a looks-connected row").clone();
    click(&mut h, &checks_panel::row_label(&tee));
    save(&h, &deltas, "022-tee.png");
    click(&mut h, checks_panel::ADD_JUNCTION);
    let rows = h.state().panel.checks.rows().to_vec();
    assert!(rows.iter().all(|r| r.check.kind != Kind::LooksConnected), "{rows:?}");
    let d = doc(&mut h);
    assert!(d.components.values().any(|c| c.properties.get("kind").is_some_and(|k| k == "junction")));
    save(&h, &deltas, "022-tee-fixed.png");
    ok(&mut h, "u");
    let rows = h.state().panel.checks.rows().to_vec();
    assert!(rows.iter().any(|r| r.check.kind == Kind::LooksConnected), "u brings the row back");

    // Labels at 0, 90, 45 and 135°: never upside down.
    ok(&mut h, r#"newpage {"title":"Labels"}"#);
    ok(&mut h, r#"wire {"points":[[0,0],[40,0]],"potential":"24V"}"#);
    ok(&mut h, r#"wire {"points":[[60,0],[60,40]],"potential":"24V"}"#);
    ok(&mut h, r#"wire {"points":[[80,0],[110,30]],"potential":"L1"}"#);
    ok(&mut h, r#"wire {"points":[[150,0],[120,30]],"potential":"N"}"#);
    ok(&mut h, "zoom e");
    h.run_steps(3);
    save(&h, &deltas, "022-labels.png");
    let _ = std::fs::remove_dir_all(&dir);
}
