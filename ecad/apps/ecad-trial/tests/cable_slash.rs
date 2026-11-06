//! Brief 004, AC9 headless (design §10 checks A, D, E, N in a 1280×800 window, egui_kittest,
//! elements by name): fixture F typed, **Cable Slash** clicked by label, the first point clicked on
//! the canvas, the crossing preview before the second click, then the cable, its dots, the echo
//! and one-step undo. Reference screenshots are the tester's; how it feels is the owner's.

use cadcraft_engine::doc::EntityKind;
use cadcraft_engine::ext::MarkShape;
use cadcraft_engine::geom::Vec2;
use ecad_model::Document;
use ecad_trial::{Trial, tools};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

fn harness() -> Harness<'static, Trial> {
    ecad_cadcraft::install();
    let mut t = Trial::new(None).unwrap();
    t.hover_delay = 0.0;
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

fn doc(h: &mut Harness<'static, Trial>) -> Document {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.document().clone()).unwrap()
}

fn head(h: &mut Harness<'static, Trial>) -> u64 {
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.head()).unwrap()
}

fn screen(h: &Harness<'static, Trial>, x: f64, y: f64) -> Pos2 {
    h.state().cad.canvas.xf.expect("the canvas was laid out").to_screen(Vec2::new(x, y))
}

fn pointer_to(h: &mut Harness<'static, Trial>, at: Pos2) {
    h.event(Event::PointerMoved(at));
    h.run_steps(2);
}

fn click(h: &mut Harness<'static, Trial>, at: Pos2) {
    pointer_to(h, at);
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(2);
}

/// Type at CADCraft's command line (no text field has the keyboard), then Enter.
fn type_enter(h: &mut Harness<'static, Trial>, text: &str) {
    if !text.is_empty() {
        h.event(Event::Text(text.into()));
        h.run_steps(1);
    }
    h.key_press(Key::Enter);
    h.run_steps(2);
}

fn prompt(h: &Harness<'static, Trial>) -> String {
    h.state().cad.session.current_prompt().map(|p| p.display()).unwrap_or_default()
}

/// Fixture F, typed; the view shows -20..80 × -30..40.
fn fixture(h: &mut Harness<'static, Trial>) {
    for line in [
        r#"wire {"points":[[0,0],[60,0]]}"#,
        r#"wire {"points":[[0,5],[60,5]]}"#,
        r#"wire {"points":[[0,10],[60,10]]}"#,
        r#"line {"points":[[0,15],[60,15]]}"#,
    ] {
        assert!(h.state_mut().submit_command(line).is_none(), "{line}");
    }
    h.state_mut().cad.run("zoom", json!({"mode": "window", "p1": [-20, -30], "p2": [80, 40]})).unwrap();
    h.run_steps(3);
}

#[test]
fn a_cable_is_made_by_two_clicks_from_the_ecad_tab() {
    let mut h = harness();
    fixture(&mut h);
    let before = head(&mut h);
    h.get_by_label(tools::TAB).click();
    h.run_steps(3);
    h.get_by_label("Cable Slash").click();
    h.run_steps(3);
    assert_eq!(prompt(&h), "Specify first point of cable slash:");
    assert!(h.state().command.is_empty(), "the Command field stays empty");
    // First point by a click in empty space below the wires, second typed exactly.
    let below = screen(&h, 28.0, -3.0);
    click(&mut h, below);
    assert_eq!(prompt(&h), "Specify second point of cable slash or [Undo]:");
    let above = screen(&h, 32.0, 18.0);
    pointer_to(&mut h, above);
    let cursor = h.state().cad.canvas.cursor.unwrap();
    let preview = h.state().cad.session.preview(cursor);
    assert!(
        preview.iter().any(|e| matches!(&e.kind, EntityKind::Text(t) if t.value.starts_with("-> 3 wires"))),
        "the count shows before the second click"
    );
    assert_eq!(head(&mut h), before, "the preview writes nothing");
    type_enter(&mut h, "32,18");
    assert!(h.state().cad.session.running.is_none(), "an accepted slash ends the command");
    assert_eq!(head(&mut h), before + 1, "one transaction");
    let d = doc(&mut h);
    assert_eq!(d.cables.len(), 1);
    assert_eq!(d.cables.values().next().unwrap().members.len(), 3);
    let dots = h
        .state()
        .cad
        .session
        .owner_overlay()
        .into_iter()
        .filter(|m| m.shape == MarkShape::Dot && (m.color.0, m.color.1, m.color.2) == (0xff, 0x00, 0xff))
        .count();
    assert_eq!(dots, 3, "a magenta dot on each member");
    // `u` at the command line takes cable and slash in one step.
    type_enter(&mut h, "u");
    assert_eq!(head(&mut h), before + 2);
    let d = doc(&mut h);
    assert!(d.cables.is_empty() && d.representations.values().all(|r| r.role != "slash"));
}

#[test]
fn a_refused_slash_keeps_the_first_point_and_esc_writes_nothing() {
    let mut h = harness();
    fixture(&mut h);
    assert!(h.state_mut().submit_command(r#"cableslash {"from":[28,-3],"to":[32,18]}"#).is_none());
    let before = head(&mut h);
    h.get_by_label(tools::TAB).click();
    h.run_steps(3);
    h.get_by_label("Cable Slash").click();
    h.run_steps(3);
    type_enter(&mut h, "40,3");
    type_enter(&mut h, "40,7");
    assert!(h.state().cad.session.running.is_some(), "refused: the command goes on");
    assert_eq!(prompt(&h), "Specify second point of cable slash or [Undo]:");
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(h.state().cad.session.running.is_none());
    assert_eq!(head(&mut h), before, "nothing written");
}
