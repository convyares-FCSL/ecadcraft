//! Independent tester's round-2 contracts for brief 008: the Add Terminal prompt names the
//! default name the next terminal will actually get, while moving between components in one run,
//! back to the first one, for typed points, and after an undo.
#![cfg(not(target_arch = "wasm32"))]

mod common;

use cadcraft_engine::Input;
use cadcraft_engine::doc::{EntityKind, Handle};
use cadcraft_engine::geom::Vec2;
use common::H;
use ecad_model::ComponentId;
use serde_json::json;

fn new_h() -> H {
    let mut h = H::new(80_082);
    h.s.viewport_px = (1280.0, 800.0);
    h
}

fn prompt(h: &H) -> String {
    h.s.current_prompt().map(|p| p.display()).unwrap_or_default()
}

/// The Add Terminal prompt that promises no number.
const NUMBER_FREE: &str = "Specify point for a terminal on a component or [Name]:";

fn shown(h: &mut H, cursor: Vec2) -> String {
    h.s.cursor = cursor;
    let p = prompt(h);
    p.strip_prefix("Specify point for terminal ")
        .and_then(|r| r.strip_suffix(" on a component or [Name]:"))
        .unwrap_or(&p)
        .to_string()
}

fn edge(h: &H, x: f64) -> Handle {
    h.s.doc()
        .unwrap()
        .model
        .iter()
        .find(|e| matches!(&e.kind, EntityKind::Line(l) if l.a.x == x && l.b.x == x))
        .map(|e| e.handle)
        .unwrap()
}

fn rect(h: &mut H, x0: f64) -> ComponentId {
    let before: Vec<Handle> = h.s.doc().unwrap().model.iter().map(|e| e.handle).collect();
    h.cmdline(&format!("line {x0},0 {},0 {},20 {x0},20 c", x0 + 40.0, x0 + 40.0));
    let hs: Vec<String> =
        h.s.doc().unwrap().model.iter().map(|e| e.handle).filter(|e| !before.contains(e)).map(|e| e.hex()).collect();
    let r = h.run("convertcomponent", json!({ "handles": hs })).unwrap();
    common::id(&r, "component")
}

fn labels(h: &mut H, c: ComponentId) -> Vec<String> {
    let mut l: Vec<String> = h.doc().terminals_of(c).filter_map(|(_, t)| t.label.clone()).collect();
    l.sort();
    l
}

/// Clicks: the prompt with the cursor where the click goes names the label the click adds, while
/// moving from A to B and back to A in one run.
#[test]
fn clicked_names_match_the_prompt_across_components_in_one_run() {
    let mut h = new_h();
    let a = rect(&mut h, 0.0);
    let b = rect(&mut h, 100.0);
    let a_right = edge(&h, 40.0);
    h.run("addterminal", json!({"handle": a_right.hex(), "at": [40, 10], "label": "1"})).unwrap();
    h.s.start("addterminal").unwrap();
    let px = h.s.pixel_size();
    let mut mismatches = Vec::new();
    for (who, at) in [
        ("A left", Vec2::new(-px, 10.0)),
        ("B left", Vec2::new(100.0 - px, 10.0)),
        ("B right", Vec2::new(140.0 + px, 5.0)),
        ("A top", Vec2::new(20.0, 20.0 + px)),
        ("A bottom", Vec2::new(20.0, -px)),
    ] {
        let promised = shown(&mut h, at);
        let (la, lb) = (labels(&mut h, a), labels(&mut h, b));
        h.s.input(Input::Point(at)).unwrap();
        let (na, nb) = (labels(&mut h, a), labels(&mut h, b));
        let added: Vec<String> =
            na.iter().filter(|x| !la.contains(x)).chain(nb.iter().filter(|x| !lb.contains(x))).cloned().collect();
        if added != [promised.clone()] {
            mismatches.push(format!("{who}: prompt said {promised:?}, added {added:?}"));
        }
    }
    h.s.input(Input::Enter).unwrap();
    h.check();
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    assert_eq!(labels(&mut h, a), ["1", "2", "3", "4"]);
    assert_eq!(labels(&mut h, b), ["1", "2"]);
}

/// Typed points (the quick start's way): with the cursor resting in empty space, whenever the
/// prompt names a number it is the label the typed point then gets. A prompt with no number
/// ("Specify point for a terminal …") is accepted when the default cannot be known (overseer
/// decision, round 3).
#[test]
fn typed_names_match_the_prompt_with_the_cursor_in_empty_space() {
    let mut h = new_h();
    let a = rect(&mut h, 0.0);
    let b = rect(&mut h, 100.0);
    h.run("addterminal", json!({"handle": edge(&h, 40.0).hex(), "at": [40, 10], "label": "1"})).unwrap();
    h.s.start("addterminal").unwrap();
    let away = Vec2::new(500.0, 500.0);
    let mut mismatches = Vec::new();
    for (who, typed, c) in [("A", "0,10", a), ("B", "100,10", b), ("A again", "0,5", a)] {
        let promised = shown(&mut h, away);
        let before = labels(&mut h, c);
        h.s.cmdline(typed).unwrap();
        let added: Vec<String> = labels(&mut h, c).into_iter().filter(|x| !before.contains(x)).collect();
        assert_eq!(added.len(), 1, "{who} typed {typed}: one terminal added");
        let number_free = promised == NUMBER_FREE;
        if !number_free && added != [promised.clone()] {
            mismatches.push(format!("{who} typed {typed}: prompt said {promised:?}, added {added:?}"));
        }
    }
    h.s.input(Input::Enter).unwrap();
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

/// After an undo the prompt offers the freed number again, and the click uses it.
#[test]
fn after_undo_the_prompt_offers_the_freed_name() {
    let mut h = new_h();
    let a = rect(&mut h, 0.0);
    h.s.start("addterminal").unwrap();
    let px = h.s.pixel_size();
    // On the outline, away from the terminals already there.
    let on_a = Vec2::new(40.0 + px, 2.0);
    for y in [10.0, 5.0] {
        let at = Vec2::new(40.0 + px, y);
        h.s.cursor = at;
        h.s.input(Input::Point(at)).unwrap();
    }
    h.s.input(Input::Enter).unwrap();
    assert_eq!(labels(&mut h, a), ["1", "2"]);
    h.cmdline("u");
    assert_eq!(labels(&mut h, a), ["1"]);
    h.s.start("addterminal").unwrap();
    assert_eq!(shown(&mut h, on_a), "2");
    let at = Vec2::new(40.0 + px, 15.0);
    h.s.cursor = at;
    h.s.input(Input::Point(at)).unwrap();
    h.s.input(Input::Enter).unwrap();
    assert_eq!(labels(&mut h, a), ["1", "2"]);
    // Undo inside a run (U is not a keyword there): the prompt stays and nothing breaks.
    h.s.start("addterminal").unwrap();
    let _ = h.s.cmdline("u");
    let _ = h.s.cmdline("undo");
    h.s.cancel();
    h.check();
}

/// Hostile cursors while the Add Terminal prompt is drawn (it now looks under the cursor): no
/// panic, the plain default.
#[test]
fn the_prompt_survives_hostile_cursors() {
    let mut h = new_h();
    rect(&mut h, 0.0);
    h.s.start("addterminal").unwrap();
    for c in [
        Vec2::new(f64::NAN, 0.0),
        Vec2::new(f64::INFINITY, f64::NEG_INFINITY),
        Vec2::new(f64::MAX, f64::MAX),
        Vec2::new(1e13, 0.0),
    ] {
        assert_eq!(shown(&mut h, c), "1");
    }
    h.s.cancel();
}

/// Round 3 (fix N3): the prompt names a number only when it is known. With the cursor over a
/// component it names that component's default; in empty space it names one only when every
/// drawn component would get the same; a typed Name is always shown.
#[test]
fn numbered_and_number_free_prompts_follow_the_cursor() {
    let mut h = new_h();
    rect(&mut h, 0.0);
    h.s.start("addterminal").unwrap();
    let px = h.s.pixel_size();
    let empty = Vec2::new(500.0, 500.0);
    let over_a = Vec2::new(20.0, -px);
    assert_eq!(shown(&mut h, empty), "1", "one component, no terminals: 1 is known");
    assert_eq!(shown(&mut h, over_a), "1");
    h.s.cancel();
    rect(&mut h, 100.0);
    h.run("addterminal", json!({"handle": edge(&h, 40.0).hex(), "at": [40, 10], "label": "1"})).unwrap();
    h.s.start("addterminal").unwrap();
    let over_b = Vec2::new(120.0, -px);
    let cases = [
        ("empty", empty, NUMBER_FREE),
        ("over A", over_a, "2"),
        ("over B", over_b, "1"),
        ("empty again", empty, NUMBER_FREE),
    ];
    for (who, at, want) in cases {
        assert_eq!(shown(&mut h, at), want, "{who}");
    }
    // The Name prompt's default follows the same rule.
    h.s.cursor = empty;
    h.s.cmdline("N").unwrap();
    assert_eq!(prompt(&h), "Enter terminal name:", "no default promised in empty space");
    h.s.input(Input::Enter).unwrap();
    h.s.cursor = over_a;
    h.s.cmdline("N").unwrap();
    assert_eq!(prompt(&h), "Enter terminal name <2>:");
    h.s.cmdline("K7").unwrap();
    assert_eq!(shown(&mut h, empty), "K7", "a typed name is shown anywhere");
    assert_eq!(shown(&mut h, over_b), "K7");
    // Enter at a number-free Name prompt, then a typed point: the default of the component hit.
    h.s.cancel();
    h.s.start("addterminal").unwrap();
    h.s.cursor = empty;
    h.s.cmdline("N").unwrap();
    h.s.input(Input::Enter).unwrap();
    let d = h.doc();
    let b = d.components.keys().copied().find(|c| d.terminals_of(*c).count() == 0).unwrap();
    h.s.cmdline("100,10").unwrap();
    assert_eq!(labels(&mut h, b), ["1"]);
    h.s.cancel();
    h.check();
}
