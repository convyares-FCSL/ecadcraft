//! Tester 057, the owner's core rule in the app (item 3 of the overseer's re-check): a `core-open`
//! row's go-to lands on the cable's page with the cable selected and the ring at the cable (an
//! open core has no wire of its own, design 072 §4.2), and **Tick Not used** is one transaction
//! that one undo takes back.

use ecad_app::App;
use ecad_app::project_panel::Action;
use ecad_engine::checks::{Fix, Kind, Object};
use eframe::egui::{self, vec2};
use egui_kittest::Harness;

use crate::mouse_tester::{Collect, Deltas};
use crate::project_panel::{doc, head, ok, rev3_app as new_app};

type H = Harness<'static, App>;

fn harness() -> H {
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = Harness::builder().with_size(vec2(1920.0, 1080.0)).with_step_dt(1.0 / 60.0).renderer(Collect(deltas)).build_ui_state(
        |ui, t: &mut App| {
            let r = ui.ctx().content_rect();
            ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
                ui.set_clip_rect(r);
                t.logic(&ui.ctx().clone());
                t.ui(ui);
            });
        },
        new_app(),
    );
    h.run_steps(5);
    h
}

fn core_rows(h: &mut H) -> Vec<ecad_engine::checks::Check> {
    let d = doc(h);
    ecad_engine::checks::all(&d, 0.01).into_iter().filter(|c| c.kind == Kind::CoreOpen).collect()
}

#[test]
fn core_fixes_go_to_the_cable_and_tick_not_used_in_one_transaction() {
    let mut h = harness();
    let dir = std::env::temp_dir().join(format!("t057 core fixes {}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = crate::checks_ui::demo_file(&dir);
    h.state_mut().close_project();
    h.run_steps(2);
    h.state_mut().open_project(&path).unwrap();
    h.run_steps(3);
    let first = doc(&mut h).page_order[0];
    // A new page with a cable on a loose line (its core sits on the line, no wire yet): the
    // cable type offers more cores than are used, so open cores show.
    ok(&mut h, r#"newpage {}"#);
    let d = doc(&mut h);
    let page = *d.page_order.last().unwrap();
    h.state_mut().show_page(page).unwrap();
    h.run_steps(2);
    ok(&mut h, r#"wire {"points": [[300, 100], [300, 20]]}"#);
    ok(&mut h, r#"cableslash {"from": [295, 60], "to": [305, 60], "tag": "-W90"}"#);
    let d = doc(&mut h);
    let w90 = *d.cables_tagged("-W90").first().unwrap();
    ok(&mut h, &format!(r#"setprop {{"cable":"{}","key":"type","value":"H07RN-F 4G1.5"}}"#, w90.hex()));
    let d = doc(&mut h);
    let row = core_rows(&mut h).into_iter().find(|c| c.object == Object::Cable(w90)).expect("a core-open row for -W90");
    let fix = row.fix.unwrap();
    let Fix::CoreOpen(_, i) = fix else { panic!("{fix:?}") };
    let open = ecad_engine::styles::cable::open_cores(&d, w90).into_iter().nth(usize::from(i)).unwrap();
    let want_at = ecad_model::numbering::rep_anchor(&d, ecad_model::Subject::Cable(w90)).map(|(_, a)| a);
    // Go to the cable, from another page.
    h.state_mut().show_page(first).unwrap();
    h.run_steps(2);
    let before = head(&mut h);
    h.state_mut().panel_action(Action::CoreFix(fix, 1));
    h.run_steps(3);
    assert_eq!(head(&mut h), before, "going writes nothing");
    assert_eq!(h.state().panel.active, Some(page), "the cable's page");
    let ring = {
        let t = h.state_mut();
        let p = t.project().unwrap().clone();
        p.read(&mut t.cad.session, |p, _| p.goto_ring).unwrap()
    };
    assert!(ring.is_some_and(|(pg, at, _)| pg == page && Some(at) == want_at), "ring at the cable: {ring:?} vs {want_at:?}");
    let sel = h.state().cad.session.selection();
    let want = {
        let t = h.state_mut();
        let p = t.project().unwrap().clone();
        p.configure(&mut t.cad.session, |p| ecad_engine::checks::handles_of(p, page, Object::Cable(w90))).unwrap()
    };
    assert!(!sel.is_empty() && sel == want, "the cable is selected: {sel:?} vs {want:?}");
    // Tick Not used: one transaction; one undo takes it back.
    let n = core_rows(&mut h).len();
    let before = head(&mut h);
    h.state_mut().panel_action(Action::CoreFix(fix, 0));
    h.run_steps(3);
    assert_eq!(head(&mut h), before + 1, "one transaction");
    assert_eq!(core_rows(&mut h).len(), n - 1, "the row goes");
    let d = doc(&mut h);
    assert!(d.cables[&w90].cores.iter().any(|m| m.label_str() == Some(open.label.as_str()) && m.is_spare()), "the core is stored Spare");
    ok(&mut h, "u");
    assert_eq!(core_rows(&mut h).len(), n, "one undo brings it back");
    let _ = std::fs::remove_dir_all(&dir);
}
