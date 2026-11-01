//! Brief 008, design check E, D and G headless: the quick-start example built by clicks and keys
//! in a 1280×800 window (egui_kittest, elements by name), Esc and undo from the keyboard, and the
//! hover tooltip. What these tests cannot show is how it looks on a display (screenshots are the
//! tester's), or whether it is easy enough (the owner's trial on Windows).

use cadcraft_engine::geom::Vec2;
use ecad_model::{Document, End, Form, Pt, Subject};
use ecad_trial::{HOVER_ID, Trial, tools};
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

/// Where drawing point (x, y) is on screen now.
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

fn button(h: &mut Harness<'static, Trial>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(3);
}

fn prompt(h: &Harness<'static, Trial>) -> String {
    h.state().cad.session.current_prompt().map(|p| p.display()).unwrap_or_default()
}

/// The example up to the placed coil: rectangle (typed), window selection, Convert, two
/// terminals, a rotated coil. The view shows -20..130 × -40..50.
fn component_and_coil(h: &mut Harness<'static, Trial>) {
    assert!(h.state_mut().submit_command("line 0,0 40,0 40,20 0,20 c").is_none());
    h.state_mut().cad.run("zoom", json!({"mode": "window", "p1": [-20, -40], "p2": [130, 50]})).unwrap();
    h.run_steps(3);
    // Window selection around the rectangle, from top left to bottom right.
    let (a, b) = (screen(h, -10.0, 30.0), screen(h, 50.0, -10.0));
    click(h, a);
    click(h, b);
    assert_eq!(h.state().cad.session.selection().len(), 4, "the four lines are selected");
    button(h, tools::TAB);
    button(h, "Convert to Component");
    assert_eq!(doc(h).components.len(), 1);
    button(h, "Add Terminal");
    assert_eq!(prompt(h), "Specify point for terminal 1 on a component or [Name]:");
    type_enter(h, "40,10");
    type_enter(h, "0,10");
    type_enter(h, "");
    assert!(h.state().cad.session.running.is_none(), "Enter finished Add Terminal");
    button(h, "Place Symbol");
    type_enter(h, "R");
    assert_eq!(prompt(h), "Specify insertion point (rotation 90, not mirrored) or [Rotate/Mirror]:");
    // The coil follows the cursor before the click.
    let at = screen(h, 100.0, 0.0);
    pointer_to(h, at);
    let preview = h.state().cad.session.preview(Vec2::new(100.0, 0.0));
    assert!(!preview.is_empty(), "the coil follows the cursor");
    type_enter(h, "100,0");
    type_enter(h, "");
    assert!(h.state().cad.session.running.is_none());
}

#[test]
fn the_quick_start_example_is_built_by_clicks_and_keys() {
    let mut h = harness();
    component_and_coil(&mut h);
    button(&mut h, "Wire");
    assert_eq!(prompt(&h), "Specify first point of wire:");
    let first = screen(&h, 40.0, 10.0) + egui::vec2(2.0, -2.0);
    click(&mut h, first);
    assert_eq!(prompt(&h), "Specify next point or [Undo]:");
    for (x, y) in [(70.0, 10.0), (70.0, 0.0)] {
        let p = screen(&h, x, y);
        click(&mut h, p);
    }
    // Within 5 px of the coil's A1: the attach ring and name show before the click.
    let near = screen(&h, 90.0, 0.0) + egui::vec2(-4.0, 3.0);
    pointer_to(&mut h, near);
    let cursor = h.state().cad.canvas.cursor.unwrap();
    let preview = h.state().cad.session.preview(cursor);
    assert!(
        preview.iter().any(|e| matches!(&e.kind, cadcraft_engine::doc::EntityKind::Text(t) if t.value == "-> A1")),
        "the capture shows before the click"
    );
    click(&mut h, near);
    assert!(h.state().cad.session.running.is_none(), "a click on a terminal finishes the wire");
    // The result: the quick-start's structure (IDs aside), drawn by clicks.
    let d = doc(&mut h);
    assert_eq!(d.components.len(), 2);
    assert_eq!(d.conductors.len(), 1);
    let page = d.page_order[0];
    let drawn = d
        .representations
        .values()
        .find_map(|r| match (r.form, r.subject) {
            (Form::Drawn, Subject::Component(c)) => Some((c, r)),
            _ => None,
        })
        .unwrap();
    let mut terminals: Vec<(String, Pt)> =
        drawn.1.connection_points.iter().map(|(t, p)| (d.terminals[t].label.clone().unwrap_or_default(), *p)).collect();
    terminals.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(terminals, vec![("1".into(), Pt::new(40.0, 10.0)), ("2".into(), Pt::new(0.0, 10.0))]);
    let (w, c) = d.conductors.iter().next().unwrap();
    let ends: Vec<String> =
        End::BOTH.iter().map(|e| c.end(*e).and_then(|t| d.terminals[&t].label.clone()).unwrap_or_default()).collect();
    assert_eq!(ends, ["1", "A1"], "both ends attached: to terminal 1 and to the coil's A1");
    let route = ecad_model::derive::route_on_page(&d, *w, page).unwrap().1;
    let ecad_model::Primitive::Polyline { vertices, .. } = route else { panic!("a polyline route") };
    let expected = [(40.0, 10.0), (70.0, 10.0), (70.0, 0.0), (90.0, 0.0)];
    assert_eq!(vertices.len(), 4);
    // Bends are where the clicks were (to screen precision); ends are the terminals' points.
    for (i, (v, (x, y))) in vertices.iter().zip(expected).enumerate() {
        let tol = if i == 0 || i == 3 { 1e-9 } else { 1e-3 };
        assert!(v.near(Pt::new(x, y), tol), "{v:?} is {x},{y}");
    }
    assert!(ecad_model::derive::mismatches(&d, 1e-9).is_empty());
    // The overlay: three terminal rings, two attached-end dots, no loose ends.
    let marks = h.state().cad.session.owner_overlay();
    let count = |s| marks.iter().filter(|m| m.shape == s).count();
    use cadcraft_engine::ext::MarkShape;
    assert_eq!((count(MarkShape::Ring), count(MarkShape::Dot), count(MarkShape::Square)), (4, 2, 0));
}

#[test]
fn esc_drops_a_wire_being_drawn_and_u_undoes_the_last_terminal() {
    let mut h = harness();
    component_and_coil(&mut h);
    let before = (head(&mut h), doc(&mut h));
    button(&mut h, "Wire");
    for (x, y) in [(60.0, 30.0), (80.0, 30.0)] {
        let p = screen(&h, x, y);
        click(&mut h, p);
    }
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(h.state().cad.session.running.is_none());
    assert_eq!((head(&mut h), doc(&mut h)), before, "Esc wrote nothing");
    // One undo takes back the last object only: the coil.
    type_enter(&mut h, "u");
    let d = doc(&mut h);
    assert_eq!(d.components.len(), 1, "the coil is undone");
    assert_eq!(d.terminals.len(), 2, "the terminals stay");
}

#[test]
fn hovering_names_the_object_and_its_connections_and_writes_nothing() {
    let mut h = harness();
    component_and_coil(&mut h);
    assert!(h.state_mut().submit_command(r#"wire {"points":[[40,10],[70,10],[70,0],[90,0]]}"#).is_none());
    // Nothing selected, nothing running: hovering shows the tooltip after the delay (0 here).
    let (head0, sel0) = (head(&mut h), h.state().cad.session.selection());
    let coil_edge = screen(&h, 110.0, 3.0);
    pointer_to(&mut h, coil_edge);
    h.run_steps(2);
    assert_eq!(h.state().hover_lines().map(|l| l[0].clone()).as_deref(), Some("Component (no tag)"));
    assert!(h.query_by_label("Component (no tag)").is_some(), "{HOVER_ID} shows it");
    assert!(h.query_by_label("Terminals: 2").is_some());
    let on_wire = screen(&h, 55.0, 10.0);
    pointer_to(&mut h, on_wire);
    h.run_steps(2);
    assert!(h.query_by_label("Wire (no tag)").is_some());
    assert!(h.query_by_label("End B: terminal A1 of ").is_none(), "lines are whole");
    let lines = h.state().hover_lines().unwrap().to_vec();
    assert!(lines.iter().any(|l| l.starts_with("End B: terminal A1 of ")), "{lines:?}");
    // Empty space: no tooltip.
    let empty = screen(&h, 60.0, 40.0);
    pointer_to(&mut h, empty);
    h.run_steps(2);
    assert!(h.state().hover_lines().is_none());
    assert!(h.query_by_label("Wire (no tag)").is_none());
    assert_eq!(head(&mut h), head0, "hovering writes nothing");
    assert_eq!(h.state().cad.session.selection(), sel0);
}
