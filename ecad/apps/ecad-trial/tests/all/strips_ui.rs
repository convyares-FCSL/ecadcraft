//! Brief 028 headless in the trial (design 028 §12): fixture T, Place Terminals, a bridge, the
//! strip's offer, the Reports Terminals and Cables tabs, a terminal plan page and the Checks rows
//! with their fixes. Screenshots are written when `ECAD_TESTER_SHOTS` names a folder and a wgpu
//! adapter exists; they are for the owner's approval, never committed.

use std::path::PathBuf;
use std::sync::Arc;

use ecad_cadcraft::checks::Kind;
use ecad_model::Subject;
use ecad_trial::{outputs_ui, project_panel};
use egui_kittest::Harness;

use crate::mouse_tester::{Deltas, doc, harness_with, ok, shot};

type H = Harness<'static, ecad_trial::Trial>;

fn save(h: &H, deltas: &Deltas, name: &str) {
    if let Some(dir) = std::env::var_os("ECAD_TESTER_SHOTS").map(PathBuf::from)
        && std::fs::create_dir_all(&dir).is_ok()
        && shot(h, deltas, &dir.join(name)).is_none()
    {
        eprintln!("no wgpu adapter: {name} skipped");
    }
}

fn view(h: &mut H, x0: f64, y0: f64, x1: f64, y1: f64) {
    let height = (y1 - y0).max((x1 - x0) * 800.0 / 520.0);
    let per_px = height / 800.0;
    if let Ok(st) = h.state_mut().cad.session.state_mut() {
        let centre = cadcraft_engine::geom::Vec2::new((x0 + x1) / 2.0 - 70.0 * per_px, (y0 + y1) / 2.0);
        let _ = st.set_view(cadcraft_engine::View { center: centre, height, twist: 0.0 });
    }
    h.run_steps(3);
}

#[test]
fn terminal_strips_in_the_trial() {
    let deltas: Deltas = Arc::default();
    let mut h = harness_with(Some(deltas.clone()));
    h.state_mut().cad.session.viewport_px = (1280.0, 800.0);
    // Fixture T (design 028 §12).
    ok(&mut h, r#"locationbox {"location":"+CP1","corners":[[20,150],[400,280]]}"#);
    ok(&mut h, r#"locationbox {"location":"+FIELD","corners":[[20,20],[400,140]]}"#);
    ok(&mut h, r#"place {"symbol":"coil","at":[100,220]}"#);
    ok(&mut h, r#"place {"symbol":"coil","at":[150,220]}"#);
    let d = doc(&mut h);
    let (k1, k2) = (d.components_tagged("-K01")[0], d.components_tagged("-K02")[0]);
    ok(&mut h, &format!(r#"place {{"symbol":"no","at":[200,220],"device":"{}"}}"#, k1.hex()));
    ok(&mut h, &format!(r#"place {{"symbol":"no","at":[250,220],"device":"{}"}}"#, k2.hex()));
    ok(&mut h, r#"place {"symbol":"motor3","at":[110,50]}"#);
    ok(&mut h, r#"place {"symbol":"pbno","at":[250,60]}"#);
    ok(&mut h, r#"wire {"points":[[100,210],[100,65]]}"#);
    ok(&mut h, r#"wire {"points":[[150,210],[150,100],[110,100],[110,65]]}"#);
    ok(&mut h, r#"wire {"points":[[200,210],[200,95],[120,95],[120,65]]}"#);
    ok(&mut h, r#"wire {"points":[[250,210],[250,70]]}"#);
    ok(&mut h, r#"cableslash {"from":[90,120],"to":[260,120],"tag":"-W01"}"#);
    let cable = *doc(&mut h).cables.keys().next().unwrap();
    ok(&mut h, &format!(r#"setprop {{"cable":"{}","key":"type","value":"YSLY-JZ 7G1.5"}}"#, cable.hex()));
    // Place Terminals and a bridge.
    ok(&mut h, r#"terminals {"from":[90,170],"to":[260,170]}"#);
    let d = doc(&mut h);
    let x1 = d.components_tagged("-X01")[0];
    let mut terms: Vec<(String, ecad_model::TerminalId)> = d.terminals_of(x1).map(|(t, x)| (x.label.clone().unwrap(), t)).collect();
    terms.sort();
    assert_eq!(terms.len(), 4);
    ok(&mut h, &format!(r#"bridge {{"from":"{}","to":"{}"}}"#, terms[2].1.hex(), terms[3].1.hex()));
    ok(&mut h, &format!(r#"setprop {{"component":"{}","key":"terminals","value":"8+1PE"}}"#, x1.hex()));
    view(&mut h, 60.0, 30.0, 300.0, 250.0);
    save(&h, &deltas, "028-strip.png");
    // Properties of a terminal: the offer, the counts, Show plan.
    let rep = d.representations_of(Subject::Component(x1)).next().unwrap().0;
    h.state_mut().goto_symbol(rep).unwrap();
    h.run_steps(4);
    let sections = h.state().props.lock().unwrap().view.sections.clone();
    let rows: Vec<_> = sections.iter().flat_map(|s| s.rows.iter()).collect();
    assert!(rows.iter().any(|r| r.key == "terminals" && r.value == "8+1PE"), "{rows:?}");
    assert!(rows.iter().any(|r| r.key == "terminalcounts" && r.value == "4 drawn, 4 spare"), "{rows:?}");
    // Reports ▸ Terminals and ▸ Cables.
    h.state_mut().show_plan(Some(x1));
    h.run_steps(4);
    let r = h.state().reports.as_ref().unwrap();
    assert_eq!(r.tab, outputs_ui::Tab::Terminals);
    let t = r.table().clone();
    assert_eq!(t.rows.len(), 8, "four drawn, four spare");
    assert_eq!(t.rows[0][1].text, "-K01:A2");
    assert_eq!(t.rows[0][8].text, "-M01:U1");
    save(&h, &deltas, "028-reports-terminals.png");
    h.state_mut().reports.as_mut().unwrap().tab = outputs_ui::Tab::Cables;
    h.run_steps(4);
    let t = h.state().reports.as_ref().unwrap().table().clone();
    let row: Vec<String> = t.rows[0].iter().map(|c| c.text.clone()).collect();
    assert_eq!(row[..7], ["-W01", "YSLY-JZ 7G1.5", "7", "+CP1", "+FIELD", "4", "3 (5, 6, GNYE)"], "{row:?}");
    save(&h, &deltas, "028-reports-cables.png");
    h.state_mut().show_cores(cable);
    h.run_steps(4);
    let t = h.state().reports.as_ref().unwrap().table().clone();
    assert_eq!(t.rows.len(), 7, "one row per offered core");
    h.state_mut().reports = None;
    // A terminal plan page.
    ok(&mut h, &format!(r#"newpage {{"generated":"terminals","strip":"{}"}}"#, x1.hex()));
    h.run_steps(3);
    ok(&mut h, "zoom e");
    h.run_steps(3);
    save(&h, &deltas, "028-plan-page.png");
    // Checks: too many terminals, fixed by Offer drawn.
    let page1 = doc(&mut h).page_order[0];
    h.state_mut().show_page(page1).unwrap();
    h.run_steps(2);
    ok(&mut h, &format!(r#"setprop {{"component":"{}","key":"terminals","value":"2"}}"#, x1.hex()));
    h.state_mut().panel.tab = project_panel::Tab::Checks;
    h.run_steps(4);
    let rows = h.state().panel.checks.rows().to_vec();
    assert!(rows.iter().any(|r| r.check.kind == Kind::TooManyTerminals), "{rows:?}");
    save(&h, &deltas, "028-checks.png");
    h.state_mut().panel_action(project_panel::Action::OfferDrawn(x1));
    h.run_steps(3);
    assert_eq!(doc(&mut h).components[&x1].properties["terminals"], "4");
}
