//! ECAD-SEAM(owner): embedded drawing view. `Embedding::default()` draws CADCraft's whole window
//! as before; an embedder can turn off the chrome (only the drawing, in the rect it gives), idle
//! typing (letters typed while no command runs start nothing, values typed at a prompt still
//! reach it) and the idle canvas menus (a right-click is reported instead of opening a menu).

use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Embedding, Services};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

/// Where the embedder puts the drawing in [`harness`]'s 1600 × 1000 window.
const DRAWING: Rect = Rect { min: Pos2 { x: 300.0, y: 80.0 }, max: Pos2 { x: 1300.0, y: 900.0 } };

/// A sample drawing; `embedding` set, drawn through `ui_embedded` into [`DRAWING`] (or the whole
/// window, through `ui`, when `embedding` is the default).
fn harness(embedding: Embedding) -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.run("ui.sample", serde_json::json!({})).unwrap();
    app.ui.in_window_menu = false;
    app.integrated_titlebar = true;
    app.embedding = embedding;
    let embedded = embedding != Embedding::default();
    let mut h = Harness::builder().with_size(vec2(1600.0, 1000.0)).build_ui_state(
        move |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            if embedded {
                ui.scope_builder(egui::UiBuilder::new().max_rect(DRAWING), |ui| app.ui_embedded(ui));
            } else {
                app.ui(ui);
            }
        },
        app,
    );
    h.run_steps(5);
    h
}

fn embedded() -> Embedding {
    Embedding { chrome: false, idle_typing: false, canvas_menus: false, style: false }
}

/// Type `text` and press Enter, as at the keyboard.
fn type_line(h: &mut Harness<'static, CadApp>, text: &str) {
    h.event(Event::Text(text.into()));
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
}

fn right_click(h: &mut Harness<'static, CadApp>, at: Pos2) {
    h.event(Event::PointerMoved(at));
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos: at, button: PointerButton::Secondary, pressed, modifiers: Modifiers::NONE });
        h.run_steps(1);
    }
    h.run_steps(2);
}

#[test]
fn the_default_draws_cadcrafts_window_as_before() {
    assert_eq!(Embedding::default(), Embedding { chrome: true, idle_typing: true, canvas_menus: true, style: true });
    let mut h = harness(Embedding::default());
    assert!(h.query_by_label("Drafting").is_some(), "Tool Sets are shown");
    // Idle typing starts a command, as before.
    type_line(&mut h, "L");
    assert_eq!(h.state().session.running.as_ref().map(|r| r.id.to_string()).as_deref(), Some("line"));
}

#[test]
fn without_chrome_only_the_drawing_is_drawn_in_the_embedders_rect() {
    let h = harness(embedded());
    for chrome in ["Drafting", "Modeling", "Line", "Layers"] {
        assert!(h.query_by_label(chrome).is_none(), "{chrome} is CADCraft's chrome");
    }
    let rect = h.state().canvas.rect.expect("the drawing is drawn");
    assert_eq!(rect, DRAWING, "the drawing fills the rect it was given");
}

#[test]
fn without_idle_typing_letters_start_nothing_but_a_prompt_takes_values() {
    let mut h = harness(embedded());
    type_line(&mut h, "L");
    assert!(h.state().session.running.is_none(), "L + Enter while idle starts nothing");
    h.state_mut().start("line");
    h.run_steps(2);
    type_line(&mut h, "10,10");
    let prompt = h.state().session.prompt_text();
    assert!(prompt.contains("next point"), "the point was taken: {prompt}");
}

#[test]
fn without_canvas_menus_an_idle_right_click_is_reported_once() {
    let mut h = harness(embedded());
    let at = DRAWING.center() + vec2(40.0, 30.0);
    right_click(&mut h, at);
    assert!(h.query_by_label("Clipboard").is_none(), "no shortcut menu opens");
    assert_eq!(h.state_mut().take_secondary_click(), Some(at));
    assert_eq!(h.state_mut().take_secondary_click(), None, "each click is reported once");

    // While a command runs, right-click does what it does with canvas menus on, and is not
    // reported.
    let during = |h: &mut Harness<'static, CadApp>| {
        h.state_mut().cmdline("LINE 0,0 10,0");
        h.run_steps(2);
        right_click(h, at);
        (h.state().session.running.is_some(), h.state().session.prompt_text())
    };
    let off = during(&mut h);
    assert_eq!(h.state_mut().take_secondary_click(), None);
    let on = during(&mut harness(Embedding { canvas_menus: true, ..embedded() }));
    assert_eq!(off, on, "right-click during a command is unchanged");
}

#[test]
fn with_canvas_menus_an_idle_right_click_opens_the_menu_and_is_not_reported() {
    let mut h = harness(Embedding { canvas_menus: true, ..embedded() });
    let at = DRAWING.center() + pos2(40.0, 30.0).to_vec2();
    right_click(&mut h, at);
    assert!(h.query_by_label("Clipboard").is_some(), "the shortcut menu opens");
    assert_eq!(h.state_mut().take_secondary_click(), None);
}

/// Whether any text painted in the last frame contains `needle`.
fn painted_text(h: &Harness<'static, CadApp>, needle: &str) -> bool {
    fn walk(s: &egui::Shape, needle: &str) -> bool {
        match s {
            egui::Shape::Text(t) => t.galley.text().contains(needle),
            egui::Shape::Vec(v) => v.iter().any(|s| walk(s, needle)),
            _ => false,
        }
    }
    h.output().shapes.iter().any(|c| walk(&c.shape, needle))
}

#[test]
fn the_viewport_label_is_drawn_only_with_cadcrafts_chrome() {
    // ECAD-SEAM(owner): embedded drawing view (US1 line, ECAD brief 040).
    let mut on = harness(Embedding::default());
    on.run_steps(1);
    assert!(painted_text(&on, "2D Wireframe"), "the default draws the viewport label");
    let mut chrome = harness(Embedding { chrome: true, ..embedded() });
    chrome.run_steps(1);
    assert!(painted_text(&chrome, "2D Wireframe"), "the chrome switch keeps it");
    let mut off = harness(embedded());
    off.run_steps(1);
    assert!(!painted_text(&off, "2D Wireframe"), "an embedder without chrome labels its own view");
}
