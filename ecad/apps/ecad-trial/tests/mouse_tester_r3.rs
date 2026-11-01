//! Independent tester's round-3 UI checks for brief 008 (fix round 2): the six forms refuse to
//! start in a paper-space layout with the exact message; switching to a layout mid-form refuses
//! points and picks without a crash; Esc and undo stay clean afterwards.

use cadcraft_engine::geom::Vec2;
use ecad_model::Document;
use ecad_trial::{Trial, tools};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

const REFUSAL: &str = "Switch to the Model tab to draw ECAD objects.";

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
    h.state().cad.canvas.xf.expect("canvas laid out").to_screen(Vec2::new(x, y))
}

fn pointer_to(h: &mut Harness<'static, Trial>, at: Pos2) {
    h.event(Event::PointerMoved(at));
    h.run_steps(3);
}

fn click(h: &mut Harness<'static, Trial>, at: Pos2) {
    pointer_to(h, at);
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(2);
}

fn type_enter(h: &mut Harness<'static, Trial>, text: &str) {
    if !text.is_empty() {
        h.event(Event::Text(text.into()));
        h.run_steps(1);
    }
    h.key_press(Key::Enter);
    h.run_steps(2);
}

fn esc(h: &mut Harness<'static, Trial>) {
    h.key_press(Key::Escape);
    h.run_steps(2);
}

fn ok(h: &mut Harness<'static, Trial>, line: &str) {
    let e = h.state_mut().submit_command(line);
    assert!(e.is_none(), "{line}: {e:?}");
    h.run_steps(2);
}

fn to_layout(h: &mut Harness<'static, Trial>, name: &str) {
    h.get_by_label(name).click();
    h.run_steps(5);
}

fn button(h: &mut Harness<'static, Trial>, label: &str) {
    h.get_by_label(tools::TAB).click();
    h.run_steps(2);
    h.get_by_label(label).click();
    h.run_steps(3);
}

fn last_log(h: &Harness<'static, Trial>) -> String {
    h.state().cad.session.log.last().cloned().unwrap_or_default()
}

fn running(h: &Harness<'static, Trial>) -> Option<String> {
    h.state().cad.session.running.as_ref().map(|r| r.id.clone())
}

/// Rectangle component with terminal "1" at 40,10, rotated coil at 100,0, wire between them, and a
/// loose wire 0,30 -> 20,30; zoomed to the scene.
fn scene(h: &mut Harness<'static, Trial>) {
    ok(h, "line 0,0 40,0 40,20 0,20 c");
    let model: Vec<_> = h.state().cad.session.doc().unwrap().model.iter().cloned().collect();
    let hs: Vec<String> = model.iter().map(|e| e.handle.hex()).collect();
    ok(h, &format!("convertcomponent {}", json!({ "handles": hs })));
    let right = model
        .iter()
        .find(|e| matches!(&e.kind, cadcraft_engine::doc::EntityKind::Line(l) if l.a.x == 40.0 && l.b.x == 40.0))
        .map(|e| e.handle.hex())
        .unwrap();
    ok(h, &format!("addterminal {}", json!({"handle": right, "at": [40, 10], "label": "1"})));
    ok(h, r#"place {"at":[100,0],"rotation":90}"#);
    ok(h, r#"wire {"points":[[40,10],[70,10],[70,0],[90,0]]}"#);
    ok(h, r#"wire {"points":[[0,30],[20,30]]}"#);
    h.state_mut().cad.run("zoom", json!({"mode": "window", "p1": [-10, -20], "p2": [130, 60]})).unwrap();
    h.state_mut().cad.session.set_selection(Vec::new());
    h.run_steps(3);
}

const TOOLS: [(&str, &str); 6] = [
    ("Convert to Component", "convertcomponent"),
    ("Add Terminal", "addterminal"),
    ("Place Symbol", "place"),
    ("Wire", "wire"),
    ("Connect", "connect"),
    ("Disconnect", "disconnect"),
];

/// In Layout1 every tool, by button and by bare name, is refused before its first prompt with the
/// exact message, and nothing is recorded (Convert with a selection too).
#[test]
fn every_tool_is_refused_in_a_layout_with_the_exact_message() {
    let mut h = harness();
    scene(&mut h);
    to_layout(&mut h, "Layout1");
    let before = (head(&mut h), doc(&mut h));
    let mut wrong = Vec::new();
    for (label, id) in TOOLS {
        button(&mut h, label);
        if running(&h).is_some() || last_log(&h) != REFUSAL {
            wrong.push(format!("button {label}: running {:?}, said {:?}", running(&h), last_log(&h)));
        }
        esc(&mut h);
        let e = h.state_mut().submit_command(id);
        h.run_steps(2);
        if running(&h).is_some() || !(last_log(&h) == REFUSAL || e.as_deref() == Some(REFUSAL)) {
            wrong.push(format!("typed {id}: running {:?}, said {:?} / {e:?}", running(&h), last_log(&h)));
        }
        esc(&mut h);
    }
    let all: Vec<_> = h.state().cad.session.doc().unwrap().model.iter().map(|e| e.handle).collect();
    h.state_mut().cad.session.set_selection(all);
    button(&mut h, "Convert to Component");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    assert!(running(&h).is_none());
    assert!((head(&mut h), doc(&mut h)) == before, "something was recorded in the layout");
    to_layout(&mut h, "Model");
}

/// Each tool started in Model, then a switch to Layout1: clicks (points and window picks) and a
/// typed point on the sheet are refused with the message, nothing is recorded, nothing crashes;
/// back in Model, Esc leaves the project as it was.
#[test]
fn switching_to_a_layout_mid_form_refuses_points_and_picks() {
    let mut h = harness();
    scene(&mut h);
    let before = (head(&mut h), doc(&mut h));
    let mut wrong = Vec::new();
    for (label, id) in TOOLS {
        h.state_mut().cad.session.set_selection(Vec::new());
        button(&mut h, label);
        assert_eq!(running(&h).as_deref(), Some(id), "{label} started in Model");
        if id == "wire" {
            let p = screen(&h, 0.0, 50.0);
            click(&mut h, p);
        }
        to_layout(&mut h, "Layout1");
        for p in [egui::pos2(600.0, 450.0), egui::pos2(700.0, 500.0)] {
            click(&mut h, p);
        }
        type_enter(&mut h, "10,10");
        let said = last_log(&h);
        if running(&h).as_deref() != Some(id) || said != REFUSAL {
            wrong.push(format!("{label}: running {:?}, said {said:?}", running(&h)));
        }
        if head(&mut h) != before.0 {
            wrong.push(format!("{label}: recorded something in the layout"));
        }
        to_layout(&mut h, "Model");
        esc(&mut h);
        esc(&mut h);
        assert!(running(&h).is_none(), "{label}: Esc ended it");
        if (head(&mut h), doc(&mut h)) != before {
            wrong.push(format!("{label}: the project changed"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// A form started in Model survives a round trip to Layout1 and finishes normally back in Model;
/// one `u` then removes exactly what it committed.
#[test]
fn a_form_finishes_after_a_layout_round_trip_and_undo_is_one_step() {
    let mut h = harness();
    scene(&mut h);
    let before = (head(&mut h), doc(&mut h));
    button(&mut h, "Wire");
    let a = screen(&h, 0.0, 50.0);
    click(&mut h, a);
    to_layout(&mut h, "Layout1");
    click(&mut h, egui::pos2(600.0, 450.0));
    to_layout(&mut h, "Model");
    assert_eq!(running(&h).as_deref(), Some("wire"), "the wire is still waiting for its next point");
    let b = screen(&h, 30.0, 50.0);
    click(&mut h, b);
    type_enter(&mut h, "");
    assert!(running(&h).is_none());
    assert_eq!(head(&mut h), before.0 + 1, "one wire");
    let d = doc(&mut h);
    let page = d.page_order[0];
    let new_route = d
        .conductors
        .keys()
        .filter_map(|c| ecad_model::derive::route_on_page(&d, *c, page))
        .filter_map(|(_, r)| r.path_ends())
        .any(|(p, q)| p.y > 49.0 && q.y > 49.0 && q.x > 25.0);
    assert!(new_route, "the wire runs where it was clicked in Model, not through the sheet click");
    type_enter(&mut h, "u");
    assert!(doc(&mut h) == before.1, "one undo removes the wire");
}
