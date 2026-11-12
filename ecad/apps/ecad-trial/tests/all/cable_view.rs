//! Brief 005, AC5 headless (design 005 §9 checks A and C in a 1280×800 window, egui_kittest,
//! elements by name): fixture G typed, **Cable View** clicked by its label, the slash clicked on
//! the canvas, the fan drawn, then switched back by clicking the fan. Reference screenshots and
//! how it feels are for the tester and the owner.

use cadcraft_engine::geom::Vec2;
use ecad_cadcraft::views::{self, Settings};
use ecad_trial::{Trial, tools};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

fn harness() -> Harness<'static, Trial> {
    ecad_cadcraft::install();
    ecad_cadcraft::views::isolate_for_tests();
    let mut t = Trial::new(None).unwrap();
    t.hover_delay = 0.0;
    // Never the user's own settings folder: views last for this test only.
    let p = t.project().unwrap().clone();
    p.configure(&mut t.cad.session, |p| p.settings = Settings::none()).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        t,
    );
    h.run_steps(5);
    h
}

fn head(h: &mut Harness<'static, Trial>) -> u64 {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.head()).unwrap()
}

fn unsaved(h: &mut Harness<'static, Trial>) -> u64 {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.unsaved_transactions()).unwrap()
}

fn fan_lines(h: &mut Harness<'static, Trial>) -> usize {
    let t = h.state_mut();
    let d = t.cad.session.doc().unwrap();
    d.model.iter().filter(|e| e.common.layer != ecad_cadcraft::frame::LAYER).filter(|e| e.common.layer == views::LAYER).count()
}

fn views_of(h: &mut Harness<'static, Trial>) -> Value {
    h.state_mut().cad.run("cableview", json!({})).unwrap()["views"].clone()
}

fn screen(h: &Harness<'static, Trial>, x: f64, y: f64) -> Pos2 {
    h.state().cad.canvas.xf.expect("the canvas was laid out").to_screen(Vec2::new(x, y))
}

fn click(h: &mut Harness<'static, Trial>, at: Pos2) {
    h.event(Event::PointerMoved(at));
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(2);
}

fn prompt(h: &Harness<'static, Trial>) -> String {
    h.state().cad.session.current_prompt().map(|p| p.display()).unwrap_or_default()
}

/// One drawn component: a closed rectangle typed at the command line, converted, with terminals
/// at x on y = 0, 5, 10 (JSON forms, the same path as the Command field).
fn component(h: &mut Harness<'static, Trial>, rectangle: &str, x: f64) {
    let t = h.state_mut();
    let before: Vec<_> =
        t.cad.session.doc().unwrap().model.iter().filter(|e| e.common.layer != ecad_cadcraft::frame::LAYER).map(|e| e.handle).collect();
    t.cad.session.cmdline(rectangle).unwrap();
    let lines: Vec<String> = t
        .cad
        .session
        .doc()
        .unwrap()
        .model
        .iter()
        .filter(|e| e.common.layer != ecad_cadcraft::frame::LAYER)
        .map(|e| e.handle)
        .filter(|x| !before.contains(x))
        .map(|x| x.hex())
        .collect();
    assert_eq!(lines.len(), 4);
    t.cad.run("convertcomponent", json!({ "handles": lines })).unwrap();
    for y in [0.0, 5.0, 10.0] {
        t.cad.run("addterminal", json!({"handle": lines[0], "at": [x, y]})).unwrap();
    }
    h.run_steps(1);
}

/// Fixture G (design §9); the view shows -20..80 × -30..50.
fn fixture(h: &mut Harness<'static, Trial>) {
    component(h, "line -10,-5 0,-5 0,15 -10,15 c", 0.0);
    component(h, "line 60,-5 70,-5 70,15 60,15 c", 60.0);
    for line in [
        r#"wire {"points":[[0,0],[60,0]]}"#,
        r#"wire {"points":[[0,5],[60,5]]}"#,
        r#"wire {"points":[[0,10],[60,10]]}"#,
        r#"cableslash {"from":[28,-3],"to":[32,18],"tag":"C1"}"#,
    ] {
        assert!(h.state_mut().submit_command(line).is_none(), "{line}");
    }
    h.state_mut().cad.run("zoom", json!({"mode": "window", "p1": [-20, -30], "p2": [80, 50]})).unwrap();
    h.run_steps(3);
}

#[test]
fn the_cable_view_button_switches_a_cable_both_ways() {
    let mut h = harness();
    fixture(&mut h);
    let cable = views_of(&mut h).as_object().unwrap().keys().next().cloned().expect("C1");
    let (before, changes) = (head(&mut h), unsaved(&mut h));
    assert_eq!(fan_lines(&mut h), 0);
    h.get_by_label(tools::TAB).click();
    h.run_steps(3);
    h.get_by_label("Cable View").click();
    h.run_steps(3);
    assert_eq!(prompt(&h), "Select a cable to switch its view:");
    assert!(h.state().command.is_empty(), "the Command field stays empty");
    // Click the slash: C1 becomes a fan on the same frame.
    let slash = screen(&h, 30.0, 7.5);
    click(&mut h, slash);
    assert_eq!(views_of(&mut h)[&cable], "fan");
    // Brief 009: the restyled fan (label box, trunk, spines, bends, tails) is drawn.
    // Brief 009, joining the middle of each spine: 2 trunk pieces, the label box and its text
    // `C1`, 2 spines, 4 bends (the middle wire leaves straight), 6 tails.
    assert_eq!(fan_lines(&mut h), 16, "the fan is drawn");
    assert!(h.state().cad.session.log.iter().any(|l| l == "Cable -C01 now shown in fan view."));
    assert_eq!(prompt(&h), "Select a cable to switch its view:", "the prompt repeats");
    // Click the trunk: back to traditional.
    // Brief 009: the trunk joins the middle of the spine, at y = 5, left of the label box.
    let trunk = screen(&h, 24.0, 5.0);
    click(&mut h, trunk);
    assert_eq!(views_of(&mut h)[&cable], "traditional");
    assert_eq!(fan_lines(&mut h), 0);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.state().cad.session.running.is_none());
    assert_eq!(head(&mut h), before, "nothing recorded");
    assert_eq!(unsaved(&mut h), changes, "the change count is unchanged");
}
