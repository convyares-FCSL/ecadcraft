//! Tester 055 in the trial, headless (egui_kittest) at 1920 × 1080: design 055 §4, §5 and §12.1
//! rows H and I. Library ▸ Title blocks lists the frames; a built-in row is read-only and offers
//! Duplicate for…; dragging "Drawing number" from the palette drops a field on whole millimetres
//! with Snap on; the right panel shows Value, Look and Box; Sample off shows the binding name;
//! Save is one transaction with the consequence line shown first; Discard asks only when the draft
//! changed; with writes off Save is disabled with the sentence. Written from the brief and design
//! before the implementation; only the element names of §5 and the words of §4 are used.

use ecad_trial::Trial;
use ecad_trial::shell::Workspace;
use eframe::egui::{Event, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

use crate::project_panel::{doc, head, trial};

type H = Harness<'static, Trial>;

fn shell() -> H {
    let mut h = Harness::builder().with_size(vec2(1920.0, 1080.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        trial(),
    );
    h.run_steps(5);
    h
}

fn run(h: &mut H, cmd: &str, params: Value) -> Value {
    let v = h.state_mut().cad.run(cmd, params).unwrap_or_else(|e| panic!("{cmd}: {e}"));
    h.run_steps(2);
    v
}

fn click(h: &mut H, name: &str) {
    h.get_by_label(name).click();
    h.run_steps(3);
}

fn has(h: &H, name: &str) -> bool {
    h.query_by_label(name).is_some()
}

fn has_text(h: &H, text: &str) -> bool {
    h.query_by_label_contains(text).is_some()
}

fn disabled(h: &H, name: &str) -> bool {
    h.get_by_label(name).accesskit_node().is_disabled()
}

/// Library ▸ Title blocks open.
fn title_blocks() -> H {
    let mut h = shell();
    h.state_mut().set_workspace(Workspace::Library);
    h.run_steps(3);
    click(&mut h, "library.tab.title_blocks");
    h
}

fn own_frame(h: &mut H, sheet: &str) -> String {
    let v = run(h, "titleblock", json!({"new": {"from": "sheet", "sheet": sheet}}));
    ["id", "style"].iter().filter_map(|k| v[*k].as_str()).find(|x| x.starts_with('#')).map(str::to_string).unwrap_or_else(|| panic!("{v}"))
}

fn rect_of(h: &H, name: &str) -> eframe::egui::Rect {
    h.get_by_label(name).rect()
}

fn drag(h: &mut H, from: Pos2, to: Pos2) {
    h.event(Event::PointerMoved(from));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    for i in 1..=8 {
        let t = i as f32 / 8.0;
        h.event(Event::PointerMoved(from + (to - from) * t));
        h.run_steps(1);
    }
    h.event(Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
}

/// §4.1, §5, §12.1 I: the list has the built-in first; a built-in row is read-only: no tool but
/// Select, the banner sentence, and Duplicate for…; the toolbar and palette are named.
#[test]
#[ignore = "tester 055: awaiting build"]
fn i_the_list_shows_the_built_in_read_only_with_duplicate() {
    let mut h = title_blocks();
    assert!(has(&h, "library.tb.list") && has(&h, "library.tb.list.new") && has(&h, "library.tb.list.import"));
    assert!(has(&h, "library.tb.list.row.sheet"), "the built-in row");
    for t in ["select", "line", "text", "field", "logo"] {
        assert!(has(&h, &format!("library.tb.tool.{t}")), "tool {t}");
    }
    for n in ["library.tb.sample", "library.tb.snap", "library.tb.frame", "library.tb.canvas", "library.tb.palette.project_drawing_number"] {
        assert!(has(&h, n), "{n}");
    }
    click(&mut h, "library.tb.list.row.sheet");
    assert!(has_text(&h, "Built-in. Duplicate it to make your own."), "§4.6 banner");
    assert!(has(&h, "library.tb.block.duplicate"), "Duplicate for…");
    for t in ["line", "text", "field", "logo"] {
        assert!(disabled(&h, &format!("library.tb.tool.{t}")), "§4.8: built-in: no tool but Select ({t})");
    }
    assert!(!has(&h, "library.tb.block.save") || disabled(&h, "library.tb.block.save"), "nothing to save on a built-in");
}

/// §4.3, §4.4, §4.5, §12.1 H: drag "Drawing number" onto the block; it lands on whole mm with Snap
/// on and is selected; the right panel shows Value, Look and Box; Sample off shows the binding
/// name; Save shows the consequence line first and writes one transaction; the saved record has the
/// new field on whole millimetres.
#[test]
#[ignore = "tester 055: awaiting build"]
fn h_drag_a_field_inspect_it_and_save_one_transaction() {
    let mut h = title_blocks();
    run(&mut h, "projectprops", json!({"drawing_number": "PS-04-E"}));
    let id = own_frame(&mut h, "A3");
    let before = run(&mut h, "titleblock", json!({"id": id}));
    let n_before = before.to_string().matches("drawing_number").count();
    click(&mut h, &format!("library.tb.list.row.{id}"));
    let from = rect_of(&h, "library.tb.palette.project_drawing_number").center();
    let canvas = rect_of(&h, "library.tb.canvas");
    let to = canvas.center() + vec2(13.3, 7.7);
    drag(&mut h, from, to);
    for section in
        ["library.tb.inspector.value.source", "library.tb.inspector.value.ifempty", "library.tb.inspector.look.size", "library.tb.inspector.box.x"]
    {
        assert!(has(&h, section), "§4.4: {section}");
    }
    assert!(has_text(&h, "PS-04-E"), "Sample on: the value is shown");
    click(&mut h, "library.tb.sample");
    assert!(has_text(&h, "Drawing number"), "Sample off: the binding name");
    // Nothing written while editing.
    let head0 = head(&mut h);
    assert!(has_text(&h, "Saving changes"), "§4.2: the consequence line is shown before Save");
    click(&mut h, "library.tb.block.save");
    assert_eq!(head(&mut h), head0 + 1, "Save writes one transaction");
    let after = run(&mut h, "titleblock", json!({"id": id}));
    assert_eq!(after.to_string().matches("drawing_number").count(), n_before + 1, "one more drawing number field");
    // Every coordinate of every field is on whole millimetres (Snap on, 1 mm; §4.5).
    let s = after.to_string();
    let whole = serde_json::from_str::<Value>(&s).unwrap();
    let mut off = Vec::new();
    fn walk(v: &Value, off: &mut Vec<f64>) {
        match v {
            Value::Object(m) => {
                if let Some(at) = m.get("at")
                    && let Some(o) = at.as_object()
                {
                    for k in ["x", "y"] {
                        if let Some(x) = o.get(k).and_then(Value::as_f64)
                            && (x - x.round()).abs() > 1e-6
                        {
                            off.push(x);
                        }
                    }
                }
                m.values().for_each(|x| walk(x, off));
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, off)),
            _ => {}
        }
    }
    walk(&whole, &mut off);
    assert!(off.is_empty(), "snapped to whole mm: {off:?}");
    let d = doc(&mut h);
    assert!(d.styles.get("frame").is_some_and(|m| m.values().any(|r| r.body.is_some())), "the body is stored on the frame record");
}

/// §4.2: Discard asks only when the draft changed ("Discard the changes to …?" Discard / Keep).
#[test]
#[ignore = "tester 055: awaiting build"]
fn h_discard_asks_only_when_changed() {
    let mut h = title_blocks();
    let id = own_frame(&mut h, "A3");
    click(&mut h, &format!("library.tb.list.row.{id}"));
    click(&mut h, "library.tb.block.discard");
    assert!(!has_text(&h, "Discard the changes"), "unchanged: no question");
    let from = rect_of(&h, "library.tb.palette.page_title").center();
    let to = rect_of(&h, "library.tb.canvas").center();
    drag(&mut h, from, to);
    let head0 = head(&mut h);
    click(&mut h, "library.tb.block.discard");
    assert!(has_text(&h, "Discard the changes to"), "changed: asks");
    assert_eq!(head(&mut h), head0, "Discard writes nothing");
}

/// §4.8, §12.1 I: writes off (the agent paused writes): tools work on the draft, Save is disabled
/// with "Writes are off for this project."
#[test]
#[ignore = "tester 055: awaiting build"]
fn i_writes_off_disables_save_with_the_sentence() {
    let mut h = title_blocks();
    let id = own_frame(&mut h, "A3");
    h.state_mut().agent.paused = true;
    h.run_steps(2);
    click(&mut h, &format!("library.tb.list.row.{id}"));
    let from = rect_of(&h, "library.tb.palette.page_title").center();
    let to = rect_of(&h, "library.tb.canvas").center();
    drag(&mut h, from, to);
    assert!(disabled(&h, "library.tb.block.save"), "Save disabled");
    assert!(has_text(&h, "Writes are off for this project."), "§4.8 sentence");
}
