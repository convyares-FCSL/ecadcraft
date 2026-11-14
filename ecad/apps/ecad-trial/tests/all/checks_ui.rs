//! Brief 022 headless in the trial (design 022 §15 checks F and I): the demo project's labels,
//! the Checks tab with its go-to, a T without a junction fixed by **Join**, and labels at
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
pub(crate) fn demo_file(dir: &std::path::Path) -> String {
    let path = dir.join("demo.ecad").to_string_lossy().into_owned();
    let _ = std::fs::remove_file(&path);
    let mut mcp = ecad_mcp::EcadMcp::create(&path, ecad_mcp::Options { write: true, agent: Some(ecad_mcp::demo::AUTHOR.into()) }).unwrap();
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

/// Show the box (x0, y0)-(x1, y1) in the part of the window the panels leave visible (about
/// 520 px wide, its centre 70 px right of the window's).
fn view(h: &mut H, x0: f64, y0: f64, x1: f64, y1: f64) {
    let height = (y1 - y0).max((x1 - x0) * 800.0 / 520.0);
    let per_px = height / 800.0;
    if let Ok(st) = h.state_mut().cad.session.state_mut() {
        let centre = cadcraft_engine::geom::Vec2::new((x0 + x1) / 2.0 - 70.0 * per_px, (y0 + y1) / 2.0);
        let _ = st.set_view(cadcraft_engine::View { center: centre, height, twist: 0.0 });
    }
    h.run_steps(3);
}

fn click(h: &mut H, label: &str) {
    // UI rev 3: the panels' tabs are reached through the strip and the inspector.
    crate::project_panel::click(h, label);
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

    // Pages 2 and 1: every net labelled (brief 032: once per net per page by default).
    let (p1, p2, p3) = (page(&mut h, 0), page(&mut h, 1), page(&mut h, 2));
    show(&mut h, p2);
    let labels = h.state().cad.session.doc().unwrap().model.iter().filter(|e| e.common.layer == ecad_cadcraft::wires::LABEL_LAYER).count();
    let d = doc(&mut h);
    let nets: std::collections::BTreeSet<_> = d
        .representations_on(p2)
        .filter_map(|(_, r)| if let ecad_model::Subject::Conductor(c) = r.subject { d.connectivity().net_of_conductor(c) } else { None })
        .collect();
    assert!(nets.len() > 1 && labels >= nets.len(), "{labels} labels for {} nets on page 2", nets.len());
    // The circuit, not the whole sheet: labels are small (owner, 2026-11-09).
    view(&mut h, 10.0, 35.0, 160.0, 195.0);
    save(&h, &deltas, "022-demo-page2.png");
    show(&mut h, p1);
    view(&mut h, 0.0, 35.0, 110.0, 165.0);
    save(&h, &deltas, "022-demo-page1.png");

    // The hover lens on the wire under the coil names its net (design 022 §15, step 2).
    show(&mut h, p2);
    view(&mut h, 10.0, 35.0, 160.0, 195.0);
    let at = screen(&h, 57.5, 52.5);
    pointer_to(&mut h, at);
    h.run_steps(3);
    let lines = h.state().hover_lines().map(<[String]>::to_vec).unwrap_or_default();
    assert!(lines.iter().any(|l| l.starts_with("Net 0V-01")), "{lines:?}");
    assert!(lines.iter().any(|l| l == "Wire number 0V-01"), "{lines:?}");
    // Quick start 5I step 2, word for word.
    for want in [
        "End B: junction",
        "Potential 0V (continuation)",
        "Cross-section 0.5 (0 V DC) \u{b7} Colour BU (0 V DC)",
        "Net 0V-01 \u{b7} 5 wires \u{b7} -G01:-, -H01:X2, -H02:X2, -K01:A2",
    ] {
        assert!(lines.iter().any(|l| l == want), "{want} missing from {lines:?}");
    }
    save(&h, &deltas, "022-hover-wire.png");

    // The same wire selected: the Properties palette's ECAD · WIRE section (brief 019's panel)
    // shows its number, potential, label values and the Net block (design 022 §9).
    let wire = {
        let t = h.state_mut();
        let p = t.project().unwrap().clone();
        p.read(&mut t.cad.session, |p, _| {
            let doc = p.ledger.document();
            let w = doc
                .conductors
                .iter()
                .find(|(c, _)| {
                    ecad_model::wiring::label(doc, **c).is_some_and(|l| l.text.starts_with("0V-01"))
                        && ecad_model::derive::route_on_page(doc, **c, p2).is_some()
                })
                .map(|(c, _)| *c)
                .unwrap();
            let (item, _) = ecad_model::derive::route_on_page(doc, w, p2).unwrap();
            p.pages[&p2].map.item(item).unwrap()
        })
        .unwrap()
    };
    h.state_mut().cad.session.set_selection(vec![wire]);
    h.run_steps(4);
    let rows: Vec<(String, String)> =
        h.state().props.lock().unwrap().view.sections.iter().flat_map(|s| s.rows.iter().map(|r| (r.label.clone(), r.value.clone()))).collect();
    assert!(rows.iter().any(|(l, v)| l == "Wire number" && v == "0V-01"), "{rows:?}");
    assert!(rows.iter().any(|(l, v)| l == "Potential" && v == "0V (continuation)"), "{rows:?}");
    save(&h, &deltas, "022-props-wire.png");
    h.state_mut().cad.session.set_selection(Vec::new());
    h.run_steps(2);

    // The Checks tab: -M01:PE, a click goes to page 3, centres it, selects the motor, rings PE.
    click(&mut h, checks_panel::CHECKS_TAB);
    // UI rev 3: Checks is the inspector's tab; the whole list is Project ▸ Check list.
    assert_eq!(h.state().shell_state().right_tab, ecad_trial::shell::RightTab::Checks);
    h.state_mut().set_workspace(ecad_trial::shell::Workspace::Lists);
    h.state_mut().shell.lists = ecad_trial::shell::ListsView::Checks;
    h.run_steps(3);
    let rows = h.state().panel.checks.rows().to_vec();
    // Four rows of brief 022, and brief 024's count of devices without a part number.
    assert_eq!(rows.len(), 5, "{rows:?}");
    assert!(rows.iter().any(|r| r.check.kind == Kind::NoPartNumber), "{rows:?}");
    let pe = rows.iter().find(|r| r.check.which == "-M01:PE").unwrap().clone();
    assert_eq!(pe.locator, "3-C-5");
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
    // The ring clears on Esc, and on a click on empty space (owner, 2026-11-09).
    let ring_of = |h: &mut H| {
        let t = h.state_mut();
        let p = t.project().unwrap().clone();
        p.read(&mut t.cad.session, |p, _| p.goto_ring).unwrap()
    };
    h.key_press(eframe::egui::Key::Escape);
    h.run_steps(3);
    assert_eq!(ring_of(&mut h), None, "Esc clears the ring");
    click(&mut h, &checks_panel::row_label(&pe));
    assert!(ring_of(&mut h).is_some());
    // Brief 038: the page is shown in Draw (Lists ▸ Check list went to the row).
    h.state_mut().set_workspace(ecad_trial::shell::Workspace::Draw);
    h.run_steps(3);
    let empty = screen(&h, pe.check.at.unwrap().x + 40.0, pe.check.at.unwrap().y + 40.0);
    pointer_to(&mut h, empty);
    assert!(h.state().cad.canvas.hover.is_none(), "nothing under the pointer");
    crate::mouse_tester::click(&mut h, empty);
    h.run_steps(3);
    assert_eq!(ring_of(&mut h), None, "a click on empty space clears the ring");
    // The click started a selection window; Esc ends it.
    h.key_press(eframe::egui::Key::Escape);
    h.run_steps(3);
    // Back to Lists ▸ Check list for the rows.
    h.state_mut().set_workspace(ecad_trial::shell::Workspace::Lists);
    h.run_steps(3);
    click(&mut h, &checks_panel::row_label(&pe));

    // A T without a junction on a new page: its row, then Join (brief 032: a junction there).
    ok(&mut h, r#"newpage {"title":"Tee"}"#);
    ok(&mut h, r#"wire {"points":[[0,0],[60,0]]}"#);
    ok(&mut h, r#"wire {"points":[[30,30],[30,0]],"junction":false}"#);
    ok(&mut h, "zoom e");
    h.run_steps(3);
    let rows = h.state().panel.checks.rows().to_vec();
    let tee = rows.iter().find(|r| r.check.kind == Kind::LooksConnected).expect("a looks-connected row").clone();
    click(&mut h, &checks_panel::row_label(&tee));
    save(&h, &deltas, "022-tee.png");
    // Brief 032: the row's Join (a menu item is called Join too: the Checks button comes first).
    let before = h.query_all_by_label(checks_panel::JOIN).count();
    assert!(before >= 1);
    if let Some(n) = h.query_all_by_label(checks_panel::JOIN).next() {
        n.click();
    }
    h.run_steps(3);
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

    // Project settings popout, Wires (owner, 2026-11-09): colours three-letter, label height 2.5;
    // one transaction, labels follow, `u` takes both back.
    crate::project_panel::project_menu(&mut h, project_panel::PROJECT_SETTINGS);
    {
        let d = h.state_mut().project_props.as_mut().unwrap();
        assert_eq!((d.colour_style.as_str(), d.label_height.as_str()), ("iec", "1.6"));
        d.label_height = "2.5".into();
    }
    h.run_steps(2);
    // Brief 031: the settings scroll since the Text section was added; bring the row into view.
    h.get_by_label("ecad.projectsettings.colours.three").scroll_to_me();
    h.run_steps(3);
    click(&mut h, "ecad.projectsettings.colours.three");
    save(&h, &deltas, "022-wire-settings.png");
    let before = crate::mouse_tester::head(&mut h);
    click(&mut h, "ecad.projectprops.ok");
    assert_eq!(crate::mouse_tester::head(&mut h), before + 1, "one transaction");
    let d = doc(&mut h);
    assert_eq!(d.settings.wiring.colour_style, ecad_model::wiring::ColourStyle::ThreeLetter);
    assert!((d.settings.wiring.label_height - 2.5).abs() < 1e-9);
    let labels = ecad_cadcraft::wires::labels_on_page(&d, p2);
    assert!(labels.iter().any(|l| l.text == "24V-04 0.5 BLU") && labels.iter().all(|l| (l.height - 2.5).abs() < 1e-9));
    ok(&mut h, "u");
    let d = doc(&mut h);
    assert_eq!(d.settings.wiring.colour_style, ecad_model::wiring::ColourStyle::Iec);

    // Brief 032 §12 item 20: the label rule rows by element name; Every wire and 10 mm run one
    // `wiresettings` line.
    crate::project_panel::project_menu(&mut h, project_panel::PROJECT_SETTINGS);
    h.get_by_label("ecad.projectsettings.label_rule.each").scroll_to_me();
    h.run_steps(3);
    assert!(h.query_by_label("ecad.projectsettings.label_rule.net").is_some());
    click(&mut h, "ecad.projectsettings.label_rule.each");
    h.state_mut().project_props.as_mut().unwrap().labels.as_mut().unwrap().min_length = "10".into();
    h.run_steps(2);
    assert!(h.query_by_label("ecad.projectsettings.label_min_length").is_some());
    let line = h.state().project_props.as_ref().unwrap().line().unwrap().unwrap();
    assert_eq!(line, r#"wiresettings {"label_min_length":10.0,"label_rule":"each"}"#);
    let before = crate::mouse_tester::head(&mut h);
    click(&mut h, "ecad.projectprops.ok");
    assert_eq!(crate::mouse_tester::head(&mut h), before + 1, "one transaction");
    let d = doc(&mut h);
    assert_eq!(d.settings.wiring.label_rule, "each");
    assert!((d.settings.wiring.label_min_length - 10.0).abs() < 1e-9);
    let _ = std::fs::remove_dir_all(&dir);
}
