//! Independent tester's round-2 UI checks for brief 008: the edges of the fixes for terminal hover
//! priority, paper-space marks and layout switching while a form runs.

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

fn ok(h: &mut Harness<'static, Trial>, line: &str) {
    let e = h.state_mut().submit_command(line);
    assert!(e.is_none(), "{line}: {e:?}");
    h.run_steps(2);
}

fn zoom_window(h: &mut Harness<'static, Trial>, p1: [f64; 2], p2: [f64; 2]) {
    h.state_mut().cad.run("zoom", json!({"mode": "window", "p1": p1, "p2": p2})).unwrap();
    h.run_steps(3);
}

fn first_hover_line(h: &Harness<'static, Trial>) -> Option<String> {
    h.state().hover_lines().and_then(|l| l.first().cloned())
}

fn cyan_rings(h: &Harness<'static, Trial>) -> Vec<Pos2> {
    fn walk(s: &egui::Shape, out: &mut Vec<Pos2>) {
        match s {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            egui::Shape::Circle(c) if c.stroke.color == egui::Color32::from_rgb(0, 255, 255) => out.push(c.center),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for c in &h.output().shapes {
        walk(&c.shape, &mut out);
    }
    out
}

/// Rectangle with terminals "1" (40,10, wired) and "2" (0,10, free); a coil rotated 90 at 100,0
/// (A1 90,0 wired; A2 110,0 free); a mirrored coil at 100,60; a wire from 1 to A1.
fn scene(h: &mut Harness<'static, Trial>) -> Vec<(String, f64, f64)> {
    ok(h, "line 0,0 40,0 40,20 0,20 c");
    let model: Vec<_> = h.state().cad.session.doc().unwrap().model.iter().cloned().collect();
    let hs: Vec<String> = model.iter().map(|e| e.handle.hex()).collect();
    ok(h, &format!("convertcomponent {}", json!({ "handles": hs })));
    let edge = |x: f64| {
        model
            .iter()
            .find(|e| matches!(&e.kind, cadcraft_engine::doc::EntityKind::Line(l) if l.a.x == x && l.b.x == x))
            .map(|e| e.handle.hex())
            .unwrap()
    };
    ok(h, &format!("addterminal {}", json!({"handle": edge(40.0), "at": [40, 10], "label": "1"})));
    ok(h, &format!("addterminal {}", json!({"handle": edge(0.0), "at": [0, 10], "label": "2"})));
    ok(h, r#"place {"at":[100,0],"rotation":90}"#);
    ok(h, r#"place {"at":[100,60],"mirror":true}"#);
    ok(h, r#"wire {"points":[[40,10],[70,10],[70,0],[90,0]]}"#);
    let d = doc(h);
    let page = d.page_order[0];
    ecad_model::derive::terminal_positions(&d, page)
        .into_iter()
        .map(|(t, _, p)| (d.terminals[&t].label.clone().unwrap_or_default(), p.x, p.y))
        .collect()
}

/// Hover priority: a terminal ring wins over the wire ending on it and the symbol around it, for
/// drawn and symbol terminals, wired and free, on the rotated and the mirrored coil, at three
/// zooms; the wire and the component still show away from the terminals.
#[test]
fn a_terminal_wins_the_hover_at_every_zoom_on_rotated_and_mirrored_coils() {
    let mut h = harness();
    let terms = scene(&mut h);
    assert_eq!(terms.len(), 6, "{terms:?}");
    h.state_mut().cad.session.set_selection(Vec::new());
    let mut wrong = Vec::new();
    for (zoom, p1, p2) in [
        ("near", [-10.0, -20.0], [130.0, 80.0]),
        ("far", [-400.0, -300.0], [500.0, 400.0]),
        ("deep", [85.0, -5.0], [95.0, 5.0]),
    ] {
        zoom_window(&mut h, p1, p2);
        for (label, x, y) in &terms {
            let at = screen(&h, *x, *y);
            if !h.state().cad.canvas.rect.unwrap().shrink(20.0).contains(at) {
                continue;
            }
            for off in [egui::vec2(0.0, 0.0), egui::vec2(3.0, -2.0)] {
                pointer_to(&mut h, at + off);
                let got = first_hover_line(&h).unwrap_or_default();
                // Far out, several terminals fall inside one aperture: any terminal is right.
                let good =
                    if zoom == "far" { got.starts_with("Terminal ") } else { got == format!("Terminal {label}") };
                if !good {
                    wrong.push(format!("{zoom}: {label} at {x},{y} +{off:?}: {got:?}"));
                }
            }
        }
    }
    // Away from terminals the wire and the component still answer.
    zoom_window(&mut h, [-10.0, -20.0], [130.0, 80.0]);
    let on_wire = screen(&h, 55.0, 10.0);
    pointer_to(&mut h, on_wire);
    let wire = first_hover_line(&h).unwrap_or_default();
    let on_rect = screen(&h, 20.0, 20.0);
    pointer_to(&mut h, on_rect);
    let rect = first_hover_line(&h).unwrap_or_default();
    assert!(wrong.is_empty(), "terminal hovers wrong:\n{}", wrong.join("\n"));
    assert_eq!(wire, "Wire (no tag)");
    assert_eq!(rect, "Component (no tag)");
}

fn to_layout(h: &mut Harness<'static, Trial>, name: &str) {
    h.get_by_label(name).click();
    h.run_steps(5);
}

fn in_model(h: &Harness<'static, Trial>) -> bool {
    h.state().cad.session.space() == cadcraft_engine::doc::Space::Model
}

/// In a paper-space layout: no marks, and no terminal tooltip at the screen spot where a model
/// terminal's coordinates fall on the sheet.
#[test]
fn paper_space_shows_no_marks_and_no_terminal_tooltip() {
    let mut h = harness();
    let terms = scene(&mut h);
    to_layout(&mut h, "Layout1");
    assert!(!in_model(&h), "switched to paper space");
    assert!(cyan_rings(&h).is_empty(), "rings painted in paper space");
    // Paper coordinates equal to model terminal positions, and a sweep over the sheet.
    let mut stray = Vec::new();
    let mut spots: Vec<Pos2> = terms.iter().map(|(_, x, y)| screen(&h, *x, *y)).collect();
    for i in 0..12 {
        for j in 0..8 {
            spots.push(egui::pos2(260.0 + 55.0 * i as f32, 210.0 + 60.0 * j as f32));
        }
    }
    for at in spots {
        if !h.state().cad.canvas.rect.is_some_and(|r| r.contains(at)) {
            continue;
        }
        pointer_to(&mut h, at);
        if let Some(l) = first_hover_line(&h) {
            stray.push(format!("{at:?}: {l}"));
        }
    }
    assert!(stray.is_empty(), "tooltips in paper space where nothing ECAD is drawn:\n{}", stray.join("\n"));
    to_layout(&mut h, "Model");
    assert!(in_model(&h));
    assert_eq!(cyan_rings(&h).len(), 6, "the six rings are back in model space");
}

/// Switching Model ↔ Layout1 while each form runs: no crash, no marks or previews of terminals in
/// paper space, marks back in model space, and Esc afterwards leaves the project as it was.
#[test]
fn switching_layouts_while_a_form_runs_is_safe() {
    let mut h = harness();
    scene(&mut h);
    zoom_window(&mut h, [-10.0, -20.0], [130.0, 80.0]);
    h.get_by_label(tools::TAB).click();
    h.run_steps(3);
    let before = (head(&mut h), doc(&mut h));
    for (button, setup) in [
        ("Wire", Some("0,40")),
        ("Place Symbol", Some("R")),
        ("Add Terminal", None),
        ("Connect", None),
        ("Disconnect", None),
        ("Convert to Component", None),
    ] {
        h.state_mut().cad.session.set_selection(Vec::new());
        h.get_by_label(tools::TAB).click();
        h.run_steps(2);
        h.get_by_label(button).click();
        h.run_steps(3);
        assert!(h.state().cad.session.running.is_some(), "{button} started");
        if let Some(s) = setup {
            type_enter(&mut h, s);
        }
        to_layout(&mut h, "Layout1");
        // Move over the sheet and click in it (as a user might) while in paper space.
        let p = egui::pos2(600.0, 450.0);
        pointer_to(&mut h, p);
        assert!(cyan_rings(&h).is_empty(), "{button}: rings in paper space");
        to_layout(&mut h, "Model");
        assert!(in_model(&h), "{button}: back in model space");
        let q = screen(&h, 60.0, 40.0);
        pointer_to(&mut h, q);
        assert!(cyan_rings(&h).len() >= 6, "{button}: rings back in model space");
        h.key_press(Key::Escape);
        h.run_steps(2);
        h.key_press(Key::Escape);
        h.run_steps(2);
        assert!(h.state().cad.session.running.is_none(), "{button}: Esc ended it");
        let after = (head(&mut h), doc(&mut h));
        assert!(after == before, "{button}: switching layouts and Esc changed the project");
    }
    // A form started in paper space is refused or harmless; Esc leaves nothing behind.
    to_layout(&mut h, "Layout1");
    h.get_by_label(tools::TAB).click();
    h.run_steps(2);
    h.get_by_label("Wire").click();
    h.run_steps(3);
    let p = egui::pos2(600.0, 450.0);
    click(&mut h, p);
    click(&mut h, p + egui::vec2(80.0, 0.0));
    type_enter(&mut h, "");
    h.key_press(Key::Escape);
    h.run_steps(2);
    let after = head(&mut h);
    to_layout(&mut h, "Model");
    // Where did a wire clicked on the paper sheet go? Report it, then check the project is sound.
    let d = doc(&mut h);
    let page = d.page_order[0];
    let routes: Vec<String> = d
        .conductors
        .keys()
        .filter_map(|c| ecad_model::derive::route_on_page(&d, *c, page))
        .map(|(_, r)| format!("{r:?}"))
        .collect();
    let paper: usize = h.state().cad.session.doc().unwrap().layouts.iter().map(|l| l.entities.len()).sum();
    eprintln!("wire in paper space: head {} -> {after}; routes {routes:?}; paper {paper}", before.0);
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    let sound = p.read(&mut t.cad.session, |p, _| p.ledger.replay_matches().unwrap_or(false)).unwrap();
    assert!(sound, "replay differs after a wire clicked in paper space");
    assert_eq!(after, before.0, "a wire was committed from clicks on the paper sheet (not on the page's model)");
}
