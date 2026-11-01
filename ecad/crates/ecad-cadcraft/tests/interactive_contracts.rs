//! Brief 008: the click-based forms of the ECAD commands.
//!
//! - Every interactive form records **exactly** the transaction its JSON form records for the same
//!   resolved values (AC1): the same scene is built twice from the same seed, once by prompts and
//!   once by JSON, and the two histories must be equal apart from wall-clock times.
//! - JSON forms and programmatic calls never prompt.
//! - Esc at any prompt leaves the document, head and selection as before that input; one undo
//!   removes the last committed object (AC2).
//! - Capture works within the aperture and not outside it, and the preview shows the difference
//!   before the click (AC4).
//! - Hostile input is refused at the prompt with a message; nothing panics or commits (AC7).

mod common;

use cadcraft_engine::Input;
use cadcraft_engine::doc::color::Color as CColor;
use cadcraft_engine::doc::{EntityKind, Handle};
use cadcraft_engine::ext::MarkShape;
use cadcraft_engine::geom::Vec2;
use common::{H, id};
use ecad_cadcraft::interact::{self, NOT_OWNED, OUT_OF_RANGE};
use ecad_model::{ComponentId, ConductorId, Document, End, PageId, TerminalId, Transaction};
use serde_json::{Value, json};

const SEED: u64 = 8;

fn new_h() -> H {
    let mut h = H::new(SEED);
    h.s.viewport_px = (1280.0, 800.0);
    h
}

/// The history without wall-clock times (the only thing allowed to differ).
fn history(h: &mut H) -> Vec<Transaction> {
    h.p(|p| p.ledger.log().iter().map(|t| Transaction { time_ms: None, ..t.clone() }).collect())
}

fn prompt(h: &H) -> String {
    h.s.current_prompt().map(|p| p.display()).unwrap_or_default()
}

/// A click: the cursor is there, and the point is given at it.
fn click(h: &mut H, at: Vec2) {
    h.s.cursor = at;
    h.s.input(Input::Point(at)).unwrap();
    h.check();
}

/// Typed at the command line (a point, a keyword, Enter as an empty line).
fn typed(h: &mut H, text: &str) {
    if text.is_empty() {
        h.s.input(Input::Enter).unwrap();
    } else {
        h.s.cmdline(text).unwrap();
    }
    h.check();
}

fn start(h: &mut H, id: &str) {
    h.s.start(id).unwrap();
    h.check();
}

fn px(h: &H) -> f64 {
    h.s.pixel_size()
}

fn line_at(h: &mut H, x0: f64, y0: f64, x1: f64, y1: f64) -> Handle {
    h.s.doc()
        .unwrap()
        .model
        .iter()
        .find(|e| {
            matches!(&e.kind, EntityKind::Line(l)
                if (l.a.x, l.a.y, l.b.x, l.b.y) == (x0, y0, x1, y1) || (l.b.x, l.b.y, l.a.x, l.a.y) == (x0, y0, x1, y1))
        })
        .map(|e| e.handle)
        .expect("line")
}

fn rectangle(h: &mut H) -> Vec<Handle> {
    h.cmdline("line 0,0 40,0 40,20 0,20 c");
    h.s.doc().unwrap().model.iter().map(|e| e.handle).collect()
}

fn page(h: &mut H) -> PageId {
    h.first_page()
}

fn terminal_at(d: &Document, page: PageId, x: f64, y: f64) -> TerminalId {
    ecad_model::derive::terminal_positions(d, page)
        .into_iter()
        .find(|(_, _, p)| p.near(ecad_model::Pt::new(x, y), 1e-9))
        .map(|(t, _, _)| t)
        .expect("terminal there")
}

/// The one conductor (IDs do not follow creation order, so scenes needing one have only one).
fn only_conductor(d: &Document) -> ConductorId {
    assert_eq!(d.conductors.len(), 1);
    *d.conductors.keys().next().expect("a conductor")
}

/// The quick-start scene with one more loose wire, connected and disconnected, built either by
/// prompts or by the JSON forms with the values the prompts resolve.
fn scene(h: &mut H, by_prompts: bool) {
    let lines = rectangle(h);
    let pg = page(h);
    // Convert to Component, by "Select objects" (nothing selected first).
    if by_prompts {
        start(h, "convertcomponent");
        assert_eq!(prompt(h), "Select objects to convert to a component:");
        h.s.input(Input::Pick(lines.clone())).unwrap();
        assert!(h.s.selection().is_empty(), "picks are not the selection");
        typed(h, "");
    } else {
        let hs: Vec<String> = lines.iter().map(|x| x.hex()).collect();
        h.run("convertcomponent", json!({ "handles": hs })).unwrap();
    }
    let right = line_at(h, 40.0, 0.0, 40.0, 20.0);
    let left = line_at(h, 0.0, 20.0, 0.0, 0.0);
    // Add Terminal: a click 2 pixels off the right edge lands on it; a typed point is literal.
    if by_prompts {
        start(h, "addterminal");
        assert_eq!(prompt(h), "Specify point for terminal 1 on a component or [Name]:");
        let p = px(h);
        click(h, Vec2::new(40.0 + 2.0 * p, 10.0));
        assert_eq!(prompt(h), "Specify point for terminal 2 on a component or [Name]:");
        typed(h, "0,10");
        typed(h, "N");
        assert_eq!(prompt(h), "Enter terminal name <3>:");
        h.s.cmdline("X1").unwrap();
        assert_eq!(prompt(h), "Specify point for terminal X1 on a component or [Name]:");
        typed(h, "0,5");
        typed(h, "");
        assert!(h.s.running.is_none());
    } else {
        h.run("addterminal", json!({"handle": right.hex(), "at": [40.0, 10.0], "label": "1"})).unwrap();
        h.run("addterminal", json!({"handle": left.hex(), "at": [0.0, 10.0], "label": "2"})).unwrap();
        h.run("addterminal", json!({"handle": left.hex(), "at": [0.0, 5.0], "label": "X1"})).unwrap();
    }
    // Place Symbol: rotated once, then a mirrored one.
    if by_prompts {
        start(h, "place");
        assert_eq!(prompt(h), "Specify insertion point (rotation 0, not mirrored) or [Rotate/Mirror]:");
        typed(h, "R");
        assert_eq!(prompt(h), "Specify insertion point (rotation 90, not mirrored) or [Rotate/Mirror]:");
        typed(h, "100,0");
        typed(h, "M");
        typed(h, "R");
        assert_eq!(prompt(h), "Specify insertion point (rotation 180, mirrored) or [Rotate/Mirror]:");
        typed(h, "100,60");
        typed(h, "");
    } else {
        h.run("place", json!({"at": [100.0, 0.0], "rotation": 90.0, "mirror": false})).unwrap();
        h.run("place", json!({"at": [100.0, 60.0], "rotation": 180.0, "mirror": true})).unwrap();
    }
    // Wire: captured first point, two bends typed, a click 3 pixels from the coil's A1 finishes.
    if by_prompts {
        start(h, "wire");
        assert_eq!(prompt(h), "Specify first point of wire:");
        let p = px(h);
        click(h, Vec2::new(40.0 + 3.0 * p, 10.0 - 2.0 * p));
        assert_eq!(prompt(h), "Specify next point or [Undo]:");
        typed(h, "70,10");
        typed(h, "70,0");
        click(h, Vec2::new(90.0, 3.0 * p));
        assert!(h.s.running.is_none(), "a click on a terminal finishes the wire");
    } else {
        // The coil's A1 is where its rotated placement puts it (90, 0 up to rounding).
        let d = h.doc();
        let a1 = ecad_model::derive::terminal_positions(&d, pg)
            .into_iter()
            .find(|(_, _, p)| p.near(ecad_model::Pt::new(90.0, 0.0), 1e-9))
            .map(|(_, _, p)| p)
            .unwrap();
        h.run("wire", json!({"points": [[40.0, 10.0], [70.0, 10.0], [70.0, 0.0], [a1.x, a1.y]]})).unwrap();
    }
    let d = h.doc();
    assert!(d.conductors[&only_conductor(&d)].ends.iter().all(Option::is_some), "both ends attached");
    // A loose wire, typed (both ways the same JSON), then Connect and Disconnect.
    h.run("wire", json!({"points": [[0.0, 30.0], [20.0, 30.0]]})).unwrap();
    let d = h.doc();
    let loose = *d.conductors.iter().find(|(_, c)| c.ends.iter().all(Option::is_none)).unwrap().0;
    let t2 = terminal_at(&d, pg, 0.0, 10.0);
    if by_prompts {
        start(h, "connect");
        assert_eq!(prompt(h), "Select wire near the end to connect:");
        click(h, Vec2::new(2.0, 30.0));
        assert_eq!(prompt(h), "Select terminal for end A:");
        let p = px(h);
        click(h, Vec2::new(-2.0 * p, 10.0 + 2.0 * p));
        assert_eq!(prompt(h), "Select wire near the end to connect:");
        typed(h, "");
        start(h, "disconnect");
        assert_eq!(prompt(h), "Select wire near the end to disconnect:");
        // End A now sits on the terminal at 0,10: the wire runs from there to 20,30.
        click(h, Vec2::new(18.0, 28.0));
        assert_eq!(h.s.log.last().map(String::as_str), Some("that end is not connected"), "end B never was");
        click(h, Vec2::new(2.0, 12.0));
        typed(h, "");
    } else {
        h.run("connect", json!({"conductor": loose.hex(), "end": "A", "terminal": t2.hex()})).unwrap();
        h.run("disconnect", json!({"conductor": loose.hex(), "end": "A"})).unwrap();
    }
}

#[test]
fn every_form_records_exactly_the_json_forms_transactions() {
    let mut by_prompts = new_h();
    scene(&mut by_prompts, true);
    let mut by_json = new_h();
    scene(&mut by_json, false);
    let (a, b) = (history(&mut by_prompts), history(&mut by_json));
    assert_eq!(a.len(), b.len(), "one transaction per placed object, as the JSON forms");
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x, y, "transaction #{} differs", x.seq);
    }
    assert_eq!(by_prompts.doc(), by_json.doc());
}

#[test]
fn the_pickfirst_selection_converts_at_once_like_the_json_form() {
    let mut a = new_h();
    let lines = rectangle(&mut a);
    a.s.set_selection(lines.clone());
    start(&mut a, "convertcomponent");
    assert!(a.s.running.is_none(), "no prompt with a selection");
    let mut b = new_h();
    let lines_b = rectangle(&mut b);
    b.s.set_selection(lines_b);
    b.run("convertcomponent", json!({})).unwrap();
    assert_eq!(history(&mut a), history(&mut b));
    assert_eq!(a.s.selection(), lines, "the selection is kept, as by the JSON form");
}

#[test]
fn json_forms_never_prompt() {
    let mut h = new_h();
    let lines = rectangle(&mut h);
    let calls: Vec<(&str, Value)> = vec![
        ("convertcomponent", json!({"handles": lines.iter().map(|x| x.hex()).collect::<Vec<_>>()})),
        ("addterminal", json!({"handle": lines[1].hex(), "at": [40, 10]})),
        ("place", json!({"at": [100, 0], "rotation": 90})),
        ("wire", json!({"points": [[40, 10], [90, 0]]})),
    ];
    for (cmd, params) in calls {
        h.run(cmd, params.clone()).unwrap();
        assert!(h.s.running.is_none(), "{cmd} JSON form left a prompt");
    }
    let w = only_conductor(&h.doc());
    h.run("disconnect", json!({"conductor": w.hex(), "end": "A"})).unwrap();
    assert!(h.s.running.is_none());
    let t = terminal_at(&h.doc(), page(&mut h), 40.0, 10.0);
    h.run("connect", json!({"conductor": w.hex(), "end": "A", "terminal": t.hex()})).unwrap();
    assert!(h.s.running.is_none());
    // `name {json}` at the command line runs the JSON form too.
    h.cmdline(r#"wire {"points":[[0,40],[10,40]]}"#);
    assert!(h.s.running.is_none());
    // And a bad JSON call is refused without a prompt.
    assert!(h.s.execute("wire", &json!({})).is_err());
    assert!(h.s.running.is_none());
}

#[test]
fn every_tool_names_its_first_prompt_and_refuses_off_an_ecad_page() {
    let mut h = new_h();
    let firsts = [
        ("convertcomponent", "Select objects to convert to a component:"),
        // No component on the page yet: the prompt promises no number (fix round 2, N3).
        ("addterminal", "Specify point for a terminal on a component or [Name]:"),
        ("place", "Specify insertion point (rotation 0, not mirrored) or [Rotate/Mirror]:"),
        ("wire", "Specify first point of wire:"),
        ("connect", "Select wire near the end to connect:"),
        ("disconnect", "Select wire near the end to disconnect:"),
    ];
    for (cmd, first) in firsts {
        start(&mut h, cmd);
        assert_eq!(prompt(&h), first, "{cmd}");
        h.s.cancel();
        assert!(h.s.running.is_none());
    }
    // A plain CADCraft drawing: nothing starts.
    h.s.execute("new", &Value::Null).unwrap();
    for (cmd, _) in firsts {
        let e = h.s.start(cmd).unwrap_err().to_string();
        assert!(e.contains(NOT_OWNED), "{cmd}: {e}");
        assert!(h.s.running.is_none());
    }
}

/// What can change, compared before and after an Esc.
fn state(h: &mut H) -> (u64, Document, Vec<Handle>) {
    (h.head(), h.doc(), h.s.selection())
}

#[test]
fn esc_at_every_prompt_drops_only_the_pending_input_and_undo_takes_one_object() {
    let mut h = new_h();
    let lines = rectangle(&mut h);
    let hs: Vec<String> = lines.iter().map(|x| x.hex()).collect();
    h.run("convertcomponent", json!({ "handles": hs })).unwrap();
    h.run("place", json!({"at": [100, 0], "rotation": 90})).unwrap();
    h.run("wire", json!({"points": [[0, 30], [20, 30]]})).unwrap();
    let p = px(&h);
    type Steps = fn(&mut H, f64);
    let cases: Vec<(&str, Steps)> = vec![
        ("convert after picks", |h, _| {
            start(h, "convertcomponent");
            let all: Vec<Handle> = h.s.doc().unwrap().model.iter().map(|e| e.handle).collect();
            h.s.input(Input::Pick(all)).unwrap();
        }),
        ("terminal name pending", |h, _| {
            start(h, "addterminal");
            typed(h, "N");
            h.s.cmdline("Q9").unwrap();
        }),
        ("place rotated", |h, _| {
            start(h, "place");
            typed(h, "R");
            typed(h, "M");
        }),
        ("wire with points", |h, p| {
            start(h, "wire");
            click(h, Vec2::new(40.0 + 50.0 * p, 50.0));
            typed(h, "60,50");
            typed(h, "60,70");
        }),
        ("connect with an end chosen", |h, _| {
            start(h, "connect");
            click(h, Vec2::new(2.0, 30.0));
        }),
        ("disconnect", |h, _| start(h, "disconnect")),
    ];
    for (what, steps) in cases {
        let before = state(&mut h);
        steps(&mut h, p);
        assert!(h.s.running.is_some(), "{what}: still prompting");
        h.s.cancel();
        assert!(h.s.running.is_none(), "{what}");
        assert!(state(&mut h) == before, "{what}: Esc changed something");
        h.check();
    }
    // Committed objects stay through Esc; one undo removes the last one only.
    start(&mut h, "addterminal");
    typed(&mut h, "40,5");
    typed(&mut h, "40,15");
    let two = state(&mut h);
    h.s.cancel();
    assert!(state(&mut h) == two, "terminals added before Esc stay");
    let c = only_component_drawn(&h.doc());
    assert_eq!(h.doc().terminals_of(c).count(), 2);
    h.s.undo().unwrap();
    h.check();
    assert_eq!(h.doc().terminals_of(c).count(), 1, "one undo, one terminal");
    assert_eq!(h.head(), two.0 + 1, "undo appends one transaction");
}

fn only_component_drawn(d: &Document) -> ComponentId {
    *d.representations
        .values()
        .find_map(|r| match (r.form, r.subject) {
            (ecad_model::Form::Drawn, ecad_model::Subject::Component(c)) => Some(c),
            _ => None,
        })
        .as_ref()
        .expect("a drawn component")
}

/// Quick-start objects: a component with a terminal at 40,10 and a coil whose A1 is at 90,0.
fn quick(h: &mut H) {
    let lines = rectangle(h);
    let hs: Vec<String> = lines.iter().map(|x| x.hex()).collect();
    h.run("convertcomponent", json!({ "handles": hs })).unwrap();
    let right = line_at(h, 40.0, 0.0, 40.0, 20.0);
    h.run("addterminal", json!({"handle": right.hex(), "at": [40, 10], "label": "1"})).unwrap();
    h.run("place", json!({"at": [100, 0], "rotation": 90})).unwrap();
}

fn rings(ents: &[cadcraft_engine::doc::Entity], aci: u8) -> usize {
    ents.iter().filter(|e| matches!(e.kind, EntityKind::Circle(_)) && e.common.color == CColor::Index(aci)).count()
}

fn label(ents: &[cadcraft_engine::doc::Entity]) -> Option<String> {
    ents.iter().find_map(|e| match &e.kind {
        EntityKind::Text(t) => Some(t.value.clone()),
        _ => None,
    })
}

#[test]
fn capture_is_within_the_aperture_only_and_the_preview_says_so_first() {
    for (pixels, attaches) in [(5.0, true), (20.0, false)] {
        let mut h = new_h();
        quick(&mut h);
        assert_eq!(h.s.settings.aperture, 10.0);
        let p = px(&h);
        start(&mut h, "wire");
        typed(&mut h, "60,-20");
        let near = Vec2::new(90.0 + pixels * p, 0.0);
        let preview = h.s.preview(near);
        let ring = rings(&preview, 3) > 0;
        assert_eq!(ring, attaches, "{pixels} px: the attach ring shows only when the click will attach");
        assert_eq!(label(&preview).is_some(), attaches);
        if attaches {
            assert_eq!(label(&preview).as_deref(), Some("-> A1"));
        }
        click(&mut h, near);
        if !attaches {
            typed(&mut h, "");
        }
        let d = h.doc();
        let w = only_conductor(&d);
        assert_eq!(d.conductors[&w].end(End::B).is_some(), attaches, "{pixels} px");
        let route = ecad_model::derive::route_on_page(&d, w, page(&mut h)).unwrap().1;
        let end = route.path_ends().unwrap().1;
        if attaches {
            let t = d.conductors[&w].end(End::B).unwrap();
            let at = ecad_model::derive::terminal_positions(&d, page(&mut h)).into_iter().find(|x| x.0 == t).unwrap().2;
            assert_eq!(end, at, "the end is the terminal's point exactly");
        }
        // After the commit the end shows a dot when attached, a square when loose.
        let uid = h.s.state().unwrap().uid;
        let marks = h.p(|p| interact::overlay_marks(p, uid));
        let at_end =
            marks.iter().filter(|m| m.at.near(Vec2::new(end.x, end.y), 1e-9)).map(|m| m.shape).collect::<Vec<_>>();
        assert!(at_end.contains(if attaches { &MarkShape::Dot } else { &MarkShape::Square }), "{at_end:?}");
    }
}

#[test]
fn typed_points_are_literal() {
    let mut h = new_h();
    quick(&mut h);
    let p = px(&h);
    // The cursor is on A1, but a typed point near (not on) it does not attach.
    h.s.cursor = Vec2::new(90.0, 0.0);
    start(&mut h, "wire");
    typed(&mut h, "60,-20");
    let near = format!("{},0", 90.0 + 2.0 * p);
    typed(&mut h, &near);
    typed(&mut h, "");
    let d = h.doc();
    assert!(d.conductors[&only_conductor(&d)].end(End::B).is_none());
}

#[test]
fn errors_are_shown_and_the_prompt_repeats() {
    let mut h = new_h();
    quick(&mut h);
    h.run("line", json!({"points": [[0, 60], [30, 60]]})).unwrap();
    h.run("wire", json!({"points": [[0, 80], [30, 80]]})).unwrap();
    let head = h.head();
    let cases: &[(&str, Vec2, &str)] = &[
        ("addterminal", Vec2::new(200.0, 200.0), "No component outline there. Click on a component's outline."),
        (
            "addterminal",
            Vec2::new(15.0, 60.0),
            "That object is not part of a component. Convert it to a component first.",
        ),
        ("addterminal", Vec2::new(105.0, 5.0), "terminals of placed symbols come from their definition"),
        ("addterminal", Vec2::new(15.0, 80.0), "That is not a component outline."),
        ("connect", Vec2::new(15.0, 60.0), "That is not a wire."),
        ("disconnect", Vec2::new(200.0, 200.0), "That is not a wire."),
    ];
    for (cmd, at, msg) in cases {
        start(&mut h, cmd);
        // The Add Terminal prompt names the default of the component under the cursor: compare
        // it with the cursor already where the click goes.
        h.s.cursor = *at;
        let first = prompt(&h);
        click(&mut h, *at);
        assert_eq!(h.s.log.last().map(String::as_str), Some(*msg), "{cmd} at {at:?}");
        assert_eq!(prompt(&h), first, "{cmd}: the prompt repeats");
        h.s.cancel();
    }
    // Connect: a click with no terminal captured.
    start(&mut h, "connect");
    click(&mut h, Vec2::new(2.0, 80.0));
    click(&mut h, Vec2::new(200.0, 200.0));
    assert_eq!(h.s.log.last().unwrap(), "No terminal there. Move onto a terminal until the green ring shows.");
    assert_eq!(prompt(&h), "Select terminal for end A:");
    h.s.cancel();
    // Wire: Enter with one point; Convert: Enter with nothing.
    start(&mut h, "wire");
    typed(&mut h, "0,90");
    typed(&mut h, "");
    assert_eq!(h.s.log.last().unwrap(), "A wire needs at least two points; nothing was drawn.");
    assert!(h.s.running.is_none());
    start(&mut h, "convertcomponent");
    typed(&mut h, "");
    assert_eq!(h.s.log.last().unwrap(), "Nothing selected; nothing converted.");
    // Convert: a refusal ends the command (the selection is wrong).
    start(&mut h, "convertcomponent");
    let wire_handle =
        h.s.doc().unwrap().model.iter().find(|e| matches!(e.kind, EntityKind::LwPolyline(_))).unwrap().handle;
    h.s.input(Input::Pick(vec![wire_handle])).unwrap();
    typed(&mut h, "");
    assert!(h.s.log.last().unwrap().contains("only plain geometry can be converted"));
    assert!(h.s.running.is_none());
    assert_eq!(h.head(), head, "no error committed anything");
}

#[test]
fn hostile_input_is_refused_without_a_commit() {
    let mut h = new_h();
    quick(&mut h);
    let head = h.head();
    for cmd in ["addterminal", "place", "wire", "connect", "disconnect"] {
        start(&mut h, cmd);
        for text in ["nan,0", "1e309,0", "@1e300<0", "2e12,0"] {
            h.s.cmdline(text).unwrap();
            assert_eq!(h.s.log.last().map(String::as_str), Some(OUT_OF_RANGE), "{cmd}: {text}");
            assert!(h.s.running.is_some(), "{cmd}: {text}: still prompting");
        }
        for bad in [Vec2::new(f64::NAN, 0.0), Vec2::new(f64::INFINITY, 1.0), Vec2::new(-1e13, 0.0)] {
            h.s.cursor = bad;
            assert!(h.s.preview(bad).is_empty(), "{cmd}: no preview at a non-finite cursor");
            h.s.input(Input::Point(bad)).unwrap();
            assert!(h.s.running.is_some());
        }
        h.s.cancel();
    }
    // A name of 10 000 characters, or with a control character.
    start(&mut h, "addterminal");
    typed(&mut h, "N");
    h.s.input(Input::Text("x".repeat(10_000))).unwrap();
    assert_eq!(h.s.log.last().unwrap(), interact::BAD_NAME);
    h.s.input(Input::Text("a\u{7}b".into())).unwrap();
    assert_eq!(h.s.log.last().unwrap(), interact::BAD_NAME);
    h.s.cancel();
    assert_eq!(h.head(), head);
    // 10 001 wire points: the last is refused; Esc commits nothing.
    start(&mut h, "wire");
    for i in 0..10_001u32 {
        h.s.input(Input::Point(Vec2::new(f64::from(i), 500.0 + f64::from(i % 2)))).unwrap();
    }
    assert_eq!(h.s.log.last().unwrap(), "A wire has at most 10000 points; press Enter to finish.");
    h.s.cancel();
    assert_eq!(h.head(), head);
    h.check();
}

#[test]
fn the_overlay_marks_terminals_and_wire_ends_and_hover_names_them() {
    let mut h = new_h();
    quick(&mut h);
    h.run("wire", json!({"points": [[40, 10], [70, 10], [70, 0], [90, 0]]})).unwrap();
    h.run("wire", json!({"points": [[0, 30], [20, 30]]})).unwrap();
    let uid = h.s.state().unwrap().uid;
    let marks = h.p(|p| interact::overlay_marks(p, uid));
    let count = |s: MarkShape| marks.iter().filter(|m| m.shape == s).count();
    assert_eq!(count(MarkShape::Ring), 3, "the drawn terminal and the coil's two");
    assert_eq!(count(MarkShape::Dot), 2, "both ends of the connected wire");
    assert_eq!(count(MarkShape::Square), 2, "both ends of the loose one");
    assert!(marks.iter().all(|m| m.size > 0.0 && m.at.is_finite()));
    // The session passes them to the UI unchanged.
    assert_eq!(h.s.owner_overlay(), marks);
    // Hover: the coil, the terminal marker and the wire.
    let pg = page(&mut h);
    let d = h.doc();
    let coil = d.components.iter().find(|(_, c)| c.definition.is_some()).map(|(c, _)| *c).unwrap();
    let item = h.symbol_item(coil);
    let coil_handle = h.handle_of(pg, item);
    let lines = h.p(|p| interact::hover_lines(p, uid, coil_handle)).unwrap();
    assert_eq!(lines[0], "Component (no tag)");
    assert_eq!(lines[1], format!("id {}", coil.hex()));
    assert_eq!(lines[2], "Terminals: 2");
    assert!(lines.iter().any(|l| l == "  A1 — 1 wire end"), "{lines:?}");
    let w = d.conductors.iter().find(|(_, c)| c.ends.iter().all(Option::is_some)).map(|(w, _)| *w).unwrap();
    let wh = h.route_handle(pg, w);
    let lines = h.p(|p| interact::hover_lines(p, uid, wh)).unwrap();
    assert_eq!(lines[0], "Wire (no tag)");
    assert!(lines[2].starts_with("End A: terminal 1 of "), "{lines:?}");
    assert!(lines[3].starts_with("End B: terminal A1 of "), "{lines:?}");
    let drawn = only_component_drawn(&d);
    let (rid, rep) = d.representations_of(ecad_model::Subject::Component(drawn)).next().unwrap();
    let t = *rep.connection_points.keys().next().unwrap();
    let marker = h.p(|p| p.pages[&pg].map.marker(rid, t)).unwrap();
    let lines = h.p(|p| interact::hover_lines(p, uid, marker)).unwrap();
    assert_eq!(lines[0], "Terminal 1");
    assert_eq!(lines[1], format!("of component {}", drawn.hex()));
    assert!(lines[4].starts_with("  wire ") && lines[4].ends_with("end A"), "{lines:?}");
    // Plain geometry: nothing.
    h.run("line", json!({"points": [[0, 60], [30, 60]]})).unwrap();
    let plain = h.s.doc().unwrap().model.iter().last().unwrap().handle;
    assert!(h.p(|p| interact::hover_lines(p, uid, plain)).is_none());
    // Reading them changed nothing.
    let before = h.head();
    let _ = h.p(|p| (interact::overlay_marks(p, uid), interact::hover_lines(p, uid, wh)));
    assert_eq!(h.head(), before);
}

#[test]
fn default_terminal_names_are_the_smallest_unused_number() {
    let mut h = new_h();
    let lines = rectangle(&mut h);
    let hs: Vec<String> = lines.iter().map(|x| x.hex()).collect();
    let c: ComponentId = id(&h.run("convertcomponent", json!({ "handles": hs })).unwrap(), "component");
    let right = line_at(&mut h, 40.0, 0.0, 40.0, 20.0);
    for (at, label) in [([40, 2], "2"), ([40, 4], "A")] {
        h.run("addterminal", json!({"handle": right.hex(), "at": at, "label": label})).unwrap();
    }
    assert_eq!(h.p(|p| interact::next_terminal_name(p.ledger.document(), c)), "1");
    h.run("addterminal", json!({"handle": right.hex(), "at": [40, 6], "label": "1"})).unwrap();
    assert_eq!(h.p(|p| interact::next_terminal_name(p.ledger.document(), c)), "3");
}

#[test]
fn a_click_lands_on_the_outline_nearest_the_cursor() {
    use ecad_model::{Primitive, Pt};
    let line = Primitive::Line { a: Pt::new(0.0, 0.0), b: Pt::new(10.0, 0.0) };
    assert_eq!(interact::nearest_on(&line, Pt::new(4.0, 1.0)), Some(Pt::new(4.0, 0.0)));
    assert_eq!(interact::nearest_on(&line, Pt::new(-4.0, 1.0)), Some(Pt::new(0.0, 0.0)));
    let square = Primitive::Polyline {
        vertices: vec![Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), Pt::new(10.0, 10.0), Pt::new(0.0, 10.0)],
        closed: true,
    };
    assert_eq!(interact::nearest_on(&square, Pt::new(-1.0, 5.0)), Some(Pt::new(0.0, 5.0)), "the closing side");
    let circle = Primitive::Circle { center: Pt::new(0.0, 0.0), radius: 5.0 };
    assert_eq!(interact::nearest_on(&circle, Pt::new(0.0, 7.0)), Some(Pt::new(0.0, 5.0)));
    assert_eq!(interact::nearest_on(&circle, Pt::new(0.0, 0.0)), None, "the centre has no nearest point");
    let arc = Primitive::Arc { center: Pt::new(0.0, 0.0), radius: 5.0, start: 0.0, end: std::f64::consts::FRAC_PI_2 };
    let q = interact::nearest_on(&arc, Pt::new(-6.0, -1.0)).unwrap();
    assert!(q.near(Pt::new(0.0, 5.0), 1e-9) || q.near(Pt::new(5.0, 0.0), 1e-9), "an end of the arc: {q:?}");
    let hostile = Primitive::Line { a: Pt::new(f64::MAX, 0.0), b: Pt::new(-f64::MAX, 0.0) };
    let _ = interact::nearest_on(&hostile, Pt::new(0.0, 1.0));
    assert_eq!(interact::valid_name("  K1 "), Some("K1".into()));
    assert_eq!(interact::valid_name(""), None);
    assert_eq!(interact::valid_name(&"x".repeat(65)), None);
}

/// Review fix 2: a panic in a form's prompt or preview (injected) gives a plain prompt and no
/// preview; the run goes on, and nothing it committed vanishes from the page (the engine's guard
/// would otherwise end the form at the next input by putting back the drawing from before it).
#[cfg(feature = "fault-injection")]
#[test]
fn a_panic_in_a_prompt_or_preview_keeps_what_the_run_committed() {
    let mut h = new_h();
    quick(&mut h);
    start(&mut h, "addterminal");
    typed(&mut h, "0,10");
    let one = h.head();
    h.cfg(|p| p.faults.panic_in_form_preview = true);
    let shown = prompt(&h);
    assert!(shown.starts_with("Internal error in this prompt"), "{shown}");
    assert!(h.s.preview(Vec2::new(0.0, 5.0)).is_empty(), "no preview, no panic");
    // The next input still works and adds to the run; the earlier terminal stays.
    typed(&mut h, "0,5");
    assert!(h.s.running.is_some(), "the form was not ended as broken");
    assert_eq!(h.head(), one + 1);
    h.cfg(|p| p.faults.panic_in_form_preview = false);
    typed(&mut h, "");
    assert!(h.s.running.is_none());
    let c = only_component_drawn(&h.doc());
    assert_eq!(h.doc().terminals_of(c).count(), 3, "40,10 from the scene, then 0,10 and 0,5");
    h.check();
}
