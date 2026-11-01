//! Independent tester's contracts for brief 008 (ECAD tools by mouse), written from the brief and
//! design 008 only.
//!
//! - AC1: each interactive form, case by case, records exactly the transactions (and leaves exactly
//!   the document) its JSON form records for the resolved values (fresh projects, same seed).
//! - AC2: Esc at the first prompt of every tool, with and without a selection, changes nothing;
//!   undo after a finished interactive command undoes it in one step and redo brings it back.
//! - AC4: preview and click agree on capture, for clicks just inside and just outside the aperture.
//! - AC7: hostile prompt input (unicode, empty, CRLF, huge, NaN, LISP, wrong keywords) never panics,
//!   never commits from a non-finite value and leaves the project consistent.
//! - Design §3.4: refusals when the project is busy or the page is unavailable.
//! - Design §5 / owner answer Q2: the prompt names the default terminal name that is then used.
//! - Owner answer Q3: loose ends are not red.
//! - Save and reopen after building by prompts; the reopened project takes the forms again.
#![cfg(not(target_arch = "wasm32"))]

mod common;

use cadcraft_engine::Input;
use cadcraft_engine::doc::{EntityKind, Handle};
use cadcraft_engine::geom::Vec2;
use common::H;
use ecad_model::{ComponentId, ConductorId, Document, End, PageId, TerminalId, Transaction};
use serde_json::json;

const SEED: u64 = 80_08;

fn new_h() -> H {
    let mut h = H::new(SEED);
    h.s.viewport_px = (1280.0, 800.0);
    h
}

fn history(h: &mut H) -> Vec<Transaction> {
    h.p(|p| p.ledger.log().iter().map(|t| Transaction { time_ms: None, ..t.clone() }).collect())
}

fn prompt(h: &H) -> String {
    h.s.current_prompt().map(|p| p.display()).unwrap_or_default()
}

fn click(h: &mut H, at: Vec2) {
    h.s.cursor = at;
    let _ = h.s.input(Input::Point(at));
}

fn typed(h: &mut H, text: &str) {
    if text.is_empty() {
        h.s.input(Input::Enter).unwrap();
    } else {
        h.s.cmdline(text).unwrap();
    }
}

fn start(h: &mut H, id: &str) {
    h.s.start(id).unwrap_or_else(|e| panic!("{id} did not start: {e}"));
}

fn line_handle(h: &H, x0: f64, y0: f64, x1: f64, y1: f64) -> Handle {
    h.s.doc()
        .unwrap()
        .model
        .iter()
        .find(|e| {
            matches!(&e.kind, EntityKind::Line(l)
                if ((l.a.x, l.a.y, l.b.x, l.b.y) == (x0, y0, x1, y1)) || ((l.b.x, l.b.y, l.a.x, l.a.y) == (x0, y0, x1, y1)))
        })
        .map(|e| e.handle)
        .expect("line")
}

fn terminal_at(d: &Document, page: PageId, x: f64, y: f64) -> TerminalId {
    ecad_model::derive::terminal_positions(d, page)
        .into_iter()
        .find(|(_, _, p)| p.near(ecad_model::Pt::new(x, y), 1e-9))
        .map(|(t, _, _)| t)
        .unwrap_or_else(|| panic!("no terminal at {x},{y}"))
}

fn conductor_from(d: &Document, page: PageId, x: f64, y: f64) -> ConductorId {
    *d.conductors
        .keys()
        .find(|c| {
            ecad_model::derive::route_on_page(d, **c, page)
                .and_then(|(_, r)| r.path_ends())
                .is_some_and(|(a, _)| a.near(ecad_model::Pt::new(x, y), 1e-9))
        })
        .unwrap_or_else(|| panic!("no wire from {x},{y}"))
}

/// Base scene, by JSON: rectangle component (terminal "1" at 40,10), coil at 100,0 rotated 90
/// (A1 at 90,0), a loose wire 0,30 -> 20,30 and a wire 40,10 -> 90,0 attached at both ends.
fn base(h: &mut H) -> ComponentId {
    h.cmdline("line 0,0 40,0 40,20 0,20 c");
    let lines: Vec<String> = h.s.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
    let r = h.run("convertcomponent", json!({ "handles": lines })).unwrap();
    let c: ComponentId = common::id(&r, "component");
    let right = line_handle(h, 40.0, 0.0, 40.0, 20.0);
    h.run("addterminal", json!({"handle": right.hex(), "at": [40, 10], "label": "1"})).unwrap();
    h.run("place", json!({"at": [100, 0], "rotation": 90})).unwrap();
    h.run("wire", json!({"points": [[0, 30], [20, 30]]})).unwrap();
    h.run("wire", json!({"points": [[40, 10], [70, 10], [70, 0], [90, 0]]})).unwrap();
    c
}

type Step = fn(&mut H);

/// AC1, one case per behaviour: the interactive steps and the JSON form with the values they
/// resolve, each on a fresh base scene from the same seed.
#[test]
fn ac1_each_interactive_case_records_the_json_forms_transaction() {
    let cases: Vec<(&str, Step, Step)> = vec![
        (
            "addterminal: a click 3 px off a polyline outline lands on it",
            |h| {
                h.cmdline("pline 0,40 30,40 30,60 0,60 c");
                let pl = h.s.doc().unwrap().model.iter().last().unwrap().handle;
                h.run("convertcomponent", json!({"handles": [pl.hex()]})).unwrap();
                start(h, "addterminal");
                let px = h.s.pixel_size();
                click(h, Vec2::new(10.0, 40.0 - 3.0 * px));
                typed(h, "");
            },
            |h| {
                h.cmdline("pline 0,40 30,40 30,60 0,60 c");
                let pl = h.s.doc().unwrap().model.iter().last().unwrap().handle;
                h.run("convertcomponent", json!({"handles": [pl.hex()]})).unwrap();
                h.run("addterminal", json!({"handle": pl.hex(), "at": [10, 40], "label": "1"})).unwrap();
            },
        ),
        (
            "addterminal: Name option, trimmed, then the default resumes",
            |h| {
                start(h, "addterminal");
                typed(h, "N");
                h.s.input(Input::Text("  K1  ".into())).unwrap();
                typed(h, "40,15");
                typed(h, "40,5");
                typed(h, "");
            },
            |h| {
                let right = line_handle(h, 40.0, 0.0, 40.0, 20.0);
                h.run("addterminal", json!({"handle": right.hex(), "at": [40, 15], "label": "K1"})).unwrap();
                h.run("addterminal", json!({"handle": right.hex(), "at": [40, 5], "label": "2"})).unwrap();
            },
        ),
        (
            "place: every rotation, mirrored and not",
            |h| {
                start(h, "place");
                for k in 0..4 {
                    typed(h, &format!("200,{}", k * 40));
                    typed(h, "M");
                    typed(h, &format!("260,{}", k * 40));
                    typed(h, "M");
                    typed(h, "R");
                }
                typed(h, "");
            },
            |h| {
                for k in 0..4 {
                    let rot = f64::from(k * 90);
                    let y = f64::from(k * 40);
                    h.run("place", json!({"at": [200.0, y], "rotation": rot, "mirror": false})).unwrap();
                    h.run("place", json!({"at": [260.0, y], "rotation": rot, "mirror": true})).unwrap();
                }
            },
        ),
        (
            "wire: typed points exactly on terminals attach (Q4) and the typed terminal finishes",
            |h| {
                start(h, "wire");
                typed(h, "40,10");
                typed(h, "40,-30");
                typed(h, "90,-30");
                typed(h, "90,0");
                assert!(h.s.running.is_none(), "a point on a terminal finishes the wire");
            },
            |h| {
                h.run("wire", json!({"points": [[40, 10], [40, -30], [90, -30], [90, 0]]})).unwrap();
            },
        ),
        (
            "wire: loose, clicked, with a repeated click ignored, Enter commits",
            |h| {
                start(h, "wire");
                click(h, Vec2::new(0.0, 100.0));
                click(h, Vec2::new(25.0, 100.0));
                click(h, Vec2::new(25.0, 100.0));
                click(h, Vec2::new(25.0, 120.0));
                typed(h, "");
            },
            |h| {
                h.run("wire", json!({"points": [[0, 100], [25, 100], [25, 120]]})).unwrap();
            },
        ),
        (
            "wire: Undo removes the last point",
            |h| {
                start(h, "wire");
                typed(h, "0,100");
                typed(h, "30,100");
                typed(h, "U");
                typed(h, "0,130");
                typed(h, "");
            },
            |h| {
                h.run("wire", json!({"points": [[0, 100], [0, 130]]})).unwrap();
            },
        ),
        (
            "connect: end B of the loose wire to terminal 1 by a click 4 px away",
            |h| {
                start(h, "connect");
                click(h, Vec2::new(19.0, 30.0));
                assert_eq!(prompt(h), "Select terminal for end B:");
                let px = h.s.pixel_size();
                click(h, Vec2::new(40.0 + 4.0 * px, 10.0));
                typed(h, "");
            },
            |h| {
                let pg = h.first_page();
                let d = h.doc();
                let w = conductor_from(&d, pg, 0.0, 30.0);
                let t = terminal_at(&d, pg, 40.0, 10.0);
                h.run("connect", json!({"conductor": w.hex(), "end": "B", "terminal": t.hex()})).unwrap();
            },
        ),
        (
            "disconnect: end B of the attached wire",
            |h| {
                start(h, "disconnect");
                click(h, Vec2::new(85.0, 0.0));
                typed(h, "");
            },
            |h| {
                let pg = h.first_page();
                let w = conductor_from(&h.doc(), pg, 40.0, 10.0);
                h.run("disconnect", json!({"conductor": w.hex(), "end": "B"})).unwrap();
            },
        ),
        (
            "convertcomponent: picks, Remove and Add, duplicates ignored",
            |h| {
                h.run("line", json!({"points": [[200, 200], [220, 200]]})).unwrap();
                h.run("line", json!({"points": [[220, 200], [220, 220]]})).unwrap();
                h.run("line", json!({"points": [[220, 220], [200, 220]]})).unwrap();
                let all: Vec<Handle> = h.s.doc().unwrap().model.iter().map(|e| e.handle).collect();
                let (a, b, c) = (all[all.len() - 3], all[all.len() - 2], all[all.len() - 1]);
                start(h, "convertcomponent");
                h.s.input(Input::Pick(vec![a, b, a])).unwrap();
                typed(h, "R");
                h.s.input(Input::Pick(vec![b])).unwrap();
                typed(h, "A");
                h.s.input(Input::Pick(vec![b, c])).unwrap();
                typed(h, "");
            },
            |h| {
                h.run("line", json!({"points": [[200, 200], [220, 200]]})).unwrap();
                h.run("line", json!({"points": [[220, 200], [220, 220]]})).unwrap();
                h.run("line", json!({"points": [[220, 220], [200, 220]]})).unwrap();
                let all: Vec<String> = h.s.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
                h.run("convertcomponent", json!({"handles": &all[all.len() - 3..]})).unwrap();
            },
        ),
    ];
    let mut failures = Vec::new();
    for (what, by_prompts, by_json) in cases {
        let mut a = new_h();
        base(&mut a);
        let before = a.head();
        by_prompts(&mut a);
        assert!(a.s.running.is_none(), "{what}: the form is still running");
        a.check();
        let mut b = new_h();
        base(&mut b);
        by_json(&mut b);
        b.check();
        let (ha, hb) = (history(&mut a), history(&mut b));
        if a.head() == before {
            failures.push(format!("{what}: the interactive form committed nothing"));
        } else if ha != hb {
            failures.push(format!(
                "{what}: histories differ\n  prompts: {:?}\n  json:    {:?}",
                &ha[ha.len().min(7)..],
                &hb[hb.len().min(7)..]
            ));
        } else if a.doc() != b.doc() {
            failures.push(format!("{what}: documents differ"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// AC2: Esc at the first prompt of every tool changes nothing, with no selection and with a
/// selection that the tool does not consume (Convert with a selection runs at once, as designed).
#[test]
fn ac2_esc_at_every_first_prompt_changes_nothing_with_or_without_a_selection() {
    let mut h = new_h();
    base(&mut h);
    let some: Vec<Handle> = h.s.doc().unwrap().model.iter().take(2).map(|e| e.handle).collect();
    for selection in [Vec::new(), some] {
        for tool in ["convertcomponent", "addterminal", "place", "wire", "connect", "disconnect"] {
            if tool == "convertcomponent" && !selection.is_empty() {
                continue;
            }
            h.s.set_selection(selection.clone());
            let before = (h.head(), h.doc(), h.s.selection(), h.s.doc().unwrap().model.len());
            start(&mut h, tool);
            assert!(h.s.running.is_some(), "{tool} prompts");
            // The cursor moves over the drawing (previews are drawn) before Esc.
            for at in [Vec2::new(40.0, 10.0), Vec2::new(90.0, 0.0), Vec2::new(10.0, 30.0)] {
                h.s.cursor = at;
                let _ = h.s.preview(at);
            }
            h.s.cancel();
            let after = (h.head(), h.doc(), h.s.selection(), h.s.doc().unwrap().model.len());
            assert!(after == before, "{tool} with selection {selection:?}: Esc changed something");
            h.check();
        }
    }
}

/// AC2: one undo after a finished interactive command takes back what it committed (the last
/// object), and redo brings exactly that back.
#[test]
fn ac2_undo_after_each_finished_form_is_one_step_and_redo_restores_it() {
    let cases: Vec<(&str, Step)> = vec![
        ("addterminal", |h| {
            start(h, "addterminal");
            typed(h, "0,10");
            typed(h, "");
        }),
        ("place", |h| {
            start(h, "place");
            typed(h, "R");
            typed(h, "200,0");
            typed(h, "");
        }),
        ("wire", |h| {
            start(h, "wire");
            typed(h, "0,100");
            typed(h, "50,100");
            typed(h, "");
        }),
        ("connect", |h| {
            start(h, "connect");
            click(h, Vec2::new(1.0, 30.0));
            let px = h.s.pixel_size();
            click(h, Vec2::new(40.0, 10.0 + 2.0 * px));
            typed(h, "");
        }),
        ("disconnect", |h| {
            start(h, "disconnect");
            click(h, Vec2::new(41.0, 10.0));
            typed(h, "");
        }),
        ("convertcomponent", |h| {
            h.run("line", json!({"points": [[200, 200], [220, 200]]})).unwrap();
            let l = h.s.doc().unwrap().model.iter().last().unwrap().handle;
            start(h, "convertcomponent");
            h.s.input(Input::Pick(vec![l])).unwrap();
            typed(h, "");
        }),
    ];
    for (what, steps) in cases {
        let mut h = new_h();
        base(&mut h);
        let convert = what == "convertcomponent";
        let before_doc = if convert { None } else { Some(h.doc()) };
        // Convert's steps draw one plain line first (one transaction of its own).
        let head = h.head() + u64::from(convert);
        steps(&mut h);
        assert!(h.s.running.is_none(), "{what}");
        assert_eq!(h.head(), head + 1, "{what}: one transaction");
        let done = h.doc();
        h.cmdline("u");
        assert_eq!(h.head(), head + 2, "{what}: undo is one transaction");
        if let Some(b) = &before_doc {
            assert!(h.doc() == *b, "{what}: one undo restores the document before the command");
        } else {
            assert!(h.doc().components.len() == done.components.len() - 1, "{what}: the component is undone");
        }
        h.cmdline("redo");
        assert!(h.doc() == done, "{what}: redo restores it");
        h.check();
    }
}

/// AC4 through the session API, as the canvas calls it: the attach preview and the click agree,
/// on both sides of the aperture, for each prompt that takes a terminal.
#[test]
fn ac4_preview_and_click_agree_inside_and_outside_the_aperture() {
    for (pixels, inside) in [(9.0, true), (11.0, false), (0.0, true), (30.0, false)] {
        for tool in ["wire", "connect"] {
            let mut h = new_h();
            base(&mut h);
            let aperture = h.s.settings.aperture;
            assert_eq!(aperture, 10.0);
            start(&mut h, tool);
            if tool == "wire" {
                typed(&mut h, "60,-40");
            } else {
                click(&mut h, Vec2::new(19.0, 30.0));
            }
            let px = h.s.pixel_size();
            // Diagonal approach to the coil's A2 at 110,0 (free terminal).
            let off = pixels * px / std::f64::consts::SQRT_2;
            let at = Vec2::new(110.0 + off, -off);
            h.s.cursor = at;
            let preview = h.s.preview(at);
            let shows = preview.iter().any(|e| matches!(&e.kind, EntityKind::Text(t) if t.value.contains("A2")));
            assert_eq!(shows, inside, "{tool} at {pixels} px: preview");
            click(&mut h, at);
            if h.s.running.is_some() {
                typed(&mut h, "");
            }
            let pg = h.first_page();
            let d = h.doc();
            let a2 = terminal_at(&d, pg, 110.0, 0.0);
            let attached = d.conductors.values().any(|c| End::BOTH.iter().any(|e| c.end(*e) == Some(a2)));
            assert_eq!(attached, inside, "{tool} at {pixels} px: click");
            h.check();
        }
    }
}

/// AC7: hostile text at every prompt of every tool: no panic, no commit from a hostile value,
/// the project stays consistent.
#[test]
fn ac7_hostile_text_at_every_prompt_never_panics_or_commits() {
    let long = "9".repeat(100_000);
    let long_name = "\u{1F600}".repeat(10_000);
    let hostile: Vec<String> = vec![
        "nan,nan".into(),
        "NaN,0".into(),
        "inf,0".into(),
        "-inf,-inf".into(),
        "1e309,1e309".into(),
        "@1e308,1e308".into(),
        "@-1e308<45".into(),
        "@1e300<nan".into(),
        format!("{long},0"),
        format!("0,{long}"),
        "\u{FF11}\u{FF12},\u{FF13}".into(),
        "\u{1F600}".into(),
        long_name.clone(),
        "\0".into(),
        "40,10\r".into(),
        "\r\n".into(),
        "\u{202E}01,04".into(),
        "0x28,0xa".into(),
        "40;10".into(),
        "40,10,0,0".into(),
        ",".into(),
        "<".into(),
        "@".into(),
        "Rotate".into(),
        "Name".into(),
        "Undo".into(),
        "Mirror".into(),
        "(+ 1 2)".into(),
        "(command \"erase\" \"all\" \"\")".into(),
        "'zoom".into(),
    ];
    let tools = ["convertcomponent", "addterminal", "place", "wire", "connect", "disconnect"];
    for tool in tools {
        let mut h = new_h();
        base(&mut h);
        // Second prompt of each tool too: Name, a started wire, a chosen wire end.
        for second in [false, true] {
            start(&mut h, tool);
            if second {
                match tool {
                    "addterminal" => typed(&mut h, "N"),
                    "wire" => typed(&mut h, "0,200"),
                    "connect" => click(&mut h, Vec2::new(19.0, 30.0)),
                    "place" => typed(&mut h, "R"),
                    _ => {}
                }
            }
            for text in &hostile {
                let head = h.head();
                let model = h.s.doc().unwrap().model.len();
                let _ = h.s.cmdline(text);
                if h.s.running.is_none() {
                    // A keyword or Enter-like line may end the tool; start it again.
                    start(&mut h, tool);
                }
                let changed = h.head() != head;
                let _ = model;
                // Only a literal finite point may commit (Place commits a coil at `40,10\r`, for
                // example); nothing else may.
                let finite_point = matches!(text.trim(), "40,10" | "\u{FF11}\u{FF12},\u{FF13}" | "\u{202E}01,04");
                assert!(!changed || finite_point || tool == "place", "{tool} (second={second}): {text:?} committed");
            }
            h.s.cancel();
            h.check();
        }
        // Hostile raw inputs (as a UI or script could send).
        start(&mut h, tool);
        let head = h.head();
        let fake: Vec<Handle> = (1..=100_001u64).map(|i| Handle(0xdead_0000 + i)).collect();
        for i in [
            Input::Point(Vec2::new(f64::NAN, f64::NAN)),
            Input::Point(Vec2::new(f64::MAX, -f64::MAX)),
            Input::Point(Vec2::new(-0.0, -500.0)),
            Input::Keyword(String::new()),
            Input::Keyword("\u{0}".into()),
            Input::Text(String::new()),
            Input::Text("\u{0}\u{7}".into()),
            Input::Pick(Vec::new()),
            Input::Pick(vec![Handle(0), Handle(u64::MAX)]),
            Input::Pick(fake.clone()),
        ] {
            let _ = h.s.input(i);
            if h.s.running.is_none() {
                start(&mut h, tool);
            }
        }
        h.s.cancel();
        if tool != "place" {
            assert_eq!(h.head(), head, "{tool}: a hostile raw input committed");
        }
        h.check();
    }
}

/// Design §3.4: a busy project refuses every tool before its first prompt (never a deadlock), and
/// a busy project gives no preview.
#[test]
fn a_busy_project_refuses_to_start_any_tool_and_previews_nothing() {
    let mut h = new_h();
    base(&mut h);
    let uid = h.s.state().unwrap().uid;
    let owner = h.s.ext.owner_of(uid).expect("an ECAD page has an owner");
    for tool in ["convertcomponent", "addterminal", "place", "wire", "connect", "disconnect"] {
        let guard = owner.lock().unwrap();
        let r = h.s.start(tool);
        drop(guard);
        let e = r.expect_err("busy must refuse").to_string();
        assert!(e.contains("busy"), "{tool}: {e}");
        assert!(h.s.running.is_none(), "{tool}");
    }
    // Started first, then the project is busy while the cursor moves: no preview, no hang.
    start(&mut h, "wire");
    typed(&mut h, "40,10");
    let guard = owner.lock().unwrap();
    let _ = h.s.preview(Vec2::new(90.0, 0.0));
    assert!(h.s.owner_overlay().is_empty(), "a busy owner draws no marks");
    drop(guard);
    h.s.cancel();
    h.check();
}

/// Design §3.4: an unavailable page refuses every tool with the existing wording.
#[test]
fn an_unavailable_page_refuses_every_tool() {
    let mut h = new_h();
    base(&mut h);
    let pg = h.first_page();
    h.cfg(|p| p.pages.get_mut(&pg).unwrap().unavailable = true);
    for tool in ["convertcomponent", "addterminal", "place", "wire", "connect", "disconnect"] {
        let e = h.s.start(tool).expect_err("refused").to_string();
        assert!(e.contains("unavailable"), "{tool}: {e}");
        assert!(h.s.running.is_none());
    }
    h.cfg(|p| p.pages.get_mut(&pg).unwrap().unavailable = false);
}

/// Design §5 step 1 and owner answer Q2: the prompt shows the default name the next terminal will
/// get. With terminal "1" already on the component, the first prompt must not promise "1".
#[test]
fn the_prompt_names_the_default_terminal_name_that_is_used() {
    let mut h = new_h();
    base(&mut h);
    start(&mut h, "addterminal");
    let shown = prompt(&h);
    typed(&mut h, "0,10");
    let pg = h.first_page();
    let d = h.doc();
    let t = terminal_at(&d, pg, 0.0, 10.0);
    let label = d.terminals[&t].label.clone().unwrap_or_default();
    assert_eq!(label, "2", "smallest unused number on that component");
    assert_eq!(
        shown,
        format!("Specify point for terminal {label} on a component or [Name]:"),
        "the prompt promised a different name than the one used"
    );
    h.s.cancel();
}

/// Owner answer Q3: attached ends green, loose ends a neutral colour (not red); terminals cyan.
#[test]
fn loose_ends_are_neutral_not_red_and_marks_keep_token_sizes() {
    use cadcraft_engine::ext::MarkShape;
    let mut h = new_h();
    base(&mut h);
    let marks = h.s.owner_overlay();
    for m in &marks {
        match m.shape {
            MarkShape::Ring => assert_eq!((m.color.0, m.color.1, m.color.2, m.size), (0, 255, 255, 8.0)),
            MarkShape::Dot => assert_eq!((m.color.0, m.color.1, m.color.2, m.size), (0, 255, 0, 4.0)),
            MarkShape::Square => {
                assert_ne!((m.color.0, m.color.1, m.color.2), (255, 0, 0), "loose ends must not be red (Q3)");
                assert!(m.color.0.abs_diff(m.color.1) < 32 && m.color.1.abs_diff(m.color.2) < 32, "neutral: {m:?}");
                assert_eq!(m.size, 6.0);
            }
        }
    }
    assert_eq!(marks.iter().filter(|m| m.shape == MarkShape::Square).count(), 2);
    // Marks follow the page: a second page shows only its own.
    h.run("newpage", json!({"title": "2"})).unwrap();
    assert!(h.s.owner_overlay().is_empty(), "a new page has no marks");
}

/// Save and reopen after building by prompts: same document and history, and the reopened page
/// takes the interactive forms (capture works on the rebuilt page).
#[test]
fn built_by_prompts_saved_and_reopened_keeps_everything_and_still_captures() {
    let dir = std::env::temp_dir().join(format!("ecad-tester-008-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("by prompts.ecad").display().to_string();
    let mut h = new_h();
    h.cfg(|p| p.persistence = ecad_cadcraft::Persistence::File);
    base(&mut h);
    start(&mut h, "addterminal");
    typed(&mut h, "0,10");
    typed(&mut h, "");
    start(&mut h, "wire");
    let px = h.s.pixel_size();
    click(&mut h, Vec2::new(0.0 - 3.0 * px, 10.0));
    typed(&mut h, "-20,10");
    typed(&mut h, "-20,-20");
    typed(&mut h, "110,-20");
    click(&mut h, Vec2::new(110.0, 2.0 * px));
    assert!(h.s.running.is_none());
    h.run("saveas", json!({ "path": path })).unwrap();
    let live = h.doc();
    let log = h.p(|p| p.ledger.log().to_vec());
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    let mut s = cadcraft_engine::Session::empty();
    let (project, _) = ecad_cadcraft::ProjectHandle::open_file(&mut s, &path, 99).unwrap();
    let mut h2 = H { s, project, trail: Vec::new() };
    h2.s.viewport_px = (1280.0, 800.0);
    assert!(h2.doc() == live, "same document after reopening");
    assert_eq!(h2.p(|p| p.ledger.log().to_vec()), log, "same history");
    assert!(h2.p(|p| p.ledger.replay_matches().unwrap()), "replay gives the same state");
    h2.check();
    let pg = h2.first_page();
    let ends = h2.doc().conductors.len();
    start(&mut h2, "wire");
    typed(&mut h2, "0,200");
    let px = h2.s.pixel_size();
    let near = Vec2::new(40.0 + 2.0 * px, 10.0);
    h2.s.cursor = near;
    assert!(h2.s.preview(near).iter().any(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == "-> 1")));
    click(&mut h2, near);
    assert!(h2.s.running.is_none());
    let d = h2.doc();
    assert_eq!(d.conductors.len(), ends + 1);
    let t1 = terminal_at(&d, pg, 40.0, 10.0);
    assert!(d.conductors.values().filter(|c| c.end(End::B) == Some(t1)).count() >= 1);
    h2.check();
    while !h2.s.docs.is_empty() {
        h2.s.execute("close", &json!({"index": 0})).unwrap();
    }
    drop(h2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Brief 007 refusals still hold while an ECAD form is prompting: LISP typed at an ECAD prompt and
/// the Block Editor typed as a transparent or plain line change nothing.
#[test]
fn block_editor_and_lisp_stay_refused_at_ecad_prompts() {
    let mut h = new_h();
    base(&mut h);
    for tool in ["addterminal", "place", "wire", "connect", "disconnect", "convertcomponent"] {
        let before = (h.head(), h.doc(), h.s.doc().unwrap().model.len());
        start(&mut h, tool);
        for line in ["(command \"erase\" \"all\" \"\")", "(entdel (entlast))", "'bedit", "bedit", "lisp", "appload"] {
            let _ = h.s.cmdline(line);
            if h.s.running.is_none() {
                start(&mut h, tool);
            }
        }
        h.s.cancel();
        assert!(!h.s.lisp_running(), "{tool}: a LISP routine is left running");
        let after = (h.head(), h.doc(), h.s.doc().unwrap().model.len());
        assert!(after == before, "{tool}: a refused line changed something");
        h.check();
    }
}

/// Programmatic calls never prompt, even while the user's interactive form is running: the JSON
/// form waits (is refused) and the running form is left as it was.
#[test]
fn json_calls_during_a_running_form_do_not_prompt_or_corrupt_it() {
    let mut h = new_h();
    base(&mut h);
    start(&mut h, "wire");
    typed(&mut h, "0,300");
    let head = h.head();
    for (cmd, params) in [
        ("wire", json!({"points": [[0, 400], [10, 400]]})),
        ("place", json!({"at": [300, 300]})),
        ("addterminal", json!({"handle": "1", "at": [0, 0]})),
    ] {
        let _ = h.s.execute(cmd, &params);
    }
    assert_eq!(h.head(), head, "no JSON call committed while the form was running");
    let still = h.s.running.as_ref().map(|r| r.id.clone());
    if still.as_deref() == Some("wire") {
        assert_eq!(prompt(&h), "Specify next point or [Undo]:");
        typed(&mut h, "50,300");
        typed(&mut h, "");
    }
    assert!(h.s.running.is_none());
    h.check();
}

/// Never crash: a panic inside the JSON form an interactive form commits through (injected) is
/// contained; terminals committed earlier in the same run stay; the project stays consistent and
/// the next input works.
#[cfg(feature = "fault-injection")]
#[test]
fn a_panic_while_an_interactive_form_commits_is_contained() {
    let mut h = new_h();
    base(&mut h);
    start(&mut h, "addterminal");
    typed(&mut h, "0,10");
    let one = (h.head(), h.doc());
    h.cfg(|p| p.faults.panic_in_command = true);
    let _ = h.s.cmdline("0,15");
    assert_eq!(h.head(), one.0, "nothing appended by the panicking commit");
    assert!(h.doc() == one.1, "the terminal added before the panic stays, nothing else changed");
    if h.s.running.is_none() {
        start(&mut h, "addterminal");
    }
    typed(&mut h, "0,5");
    typed(&mut h, "");
    assert_eq!(h.head(), one.0 + 1, "the next input commits normally");
    h.check();
    // The same in Wire's single commit at Enter.
    start(&mut h, "wire");
    typed(&mut h, "0,300");
    typed(&mut h, "30,300");
    let before = (h.head(), h.doc());
    h.cfg(|p| p.faults.panic_in_command = true);
    let _ = h.s.input(cadcraft_engine::Input::Enter);
    h.s.cancel();
    assert_eq!((h.head(), h.doc() == before.1), (before.0, true));
    h.check();
}
