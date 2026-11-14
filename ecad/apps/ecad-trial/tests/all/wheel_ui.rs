//! Brief 041 (U4b) in the trial at 1920 × 1080: the wheel on the page and on a page row, the
//! intent line in place of the command line, every key the brief names, the tool groups and
//! Ctrl+K over pages, tags, symbols and actions.

use cadcraft_engine::geom::Vec2;
use ecad_cadcraft::actions::ObjectKind;
use ecad_model::{ComponentId, ConductorId, PageId};
use ecad_trial::Trial;
use ecad_trial::shell::{Workspace, intent, tool_strip};
use eframe::egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use crate::project_panel::{doc, head, trial};

type H = Harness<'static, Trial>;

/// The wheel's slot labelled `label` (a page row's wheel shares labels with the Pages buttons:
/// the slot is the one inside the wheel's ring).
fn wheel_slot<'a>(h: &'a H, label: &'a str) -> egui_kittest::Node<'a> {
    let c = h.state().shell.tools.wheel.as_ref().map(|w| w.widget.center).expect("a wheel is open");
    let r = ecad_ui::tokens::wheel::OUTER_RADIUS;
    h.get_all_by_label(label).find(|n| (n.rect().center() - c).length() <= r).expect("the slot")
}

fn shell() -> H {
    shell_with(None)
}

fn shell_with(deltas: Option<crate::mouse_tester::Deltas>) -> H {
    let mut b = Harness::builder().with_size(vec2(1920.0, 1080.0)).with_step_dt(1.0 / 60.0);
    if let Some(d) = deltas {
        b = b.renderer(crate::mouse_tester::Collect(d));
    }
    let mut h = b.build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        trial(),
    );
    h.run_steps(5);
    h
}

fn running(h: &H) -> Option<String> {
    h.state().cad.session.running.as_ref().map(|r| r.id.clone())
}

fn cancel(h: &mut H) {
    h.state_mut().cad.session.cancel();
    h.run_steps(2);
}

fn keys(h: &mut H, m: Modifiers, k: Key) {
    h.key_press_modifiers(m, k);
    h.run_steps(3);
}

fn to_screen(h: &H, p: Vec2) -> Pos2 {
    h.state().cad.canvas.xf.expect("the page is drawn").to_screen(p)
}

fn pointer(h: &mut H, at: Pos2, button: PointerButton, pressed: bool) {
    h.event(Event::PointerMoved(at));
    h.event(Event::PointerButton { pos: at, button, pressed, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

struct F {
    h: H,
    k1: ComponentId,
    wire: ConductorId,
    /// On `-K01`'s A2 lead, away from the wire; on its A1 (the wire's start).
    coil_at: Vec2,
    a1: Vec2,
    page2: PageId,
}

/// `-K01`'s coil on page 1 with a wire from A1, page 2 "Field"; the page fitted at 1920 × 1080.
fn fixture() -> F {
    fixture_with(None)
}

fn fixture_with(deltas: Option<crate::mouse_tester::Deltas>) -> F {
    let mut h = shell_with(deltas);
    let t = h.state_mut();
    t.cad.session.viewport_px = (1280.0, 800.0);
    let k1 = t.cad.run("place", json!({"symbol": "coil", "at": [100, 100]})).unwrap()["component"].as_str().unwrap().to_string();
    let k1 = ComponentId(u128::from_str_radix(&k1, 16).unwrap());
    let d = doc(&mut h);
    let page1 = d.page_order[0];
    let pos = |label: &str| {
        let (t, _) = d.terminals_of(k1).find(|(_, x)| x.label.as_deref() == Some(label)).unwrap();
        let p = ecad_model::derive::terminal_positions(&d, page1).into_iter().find(|(x, _, _)| *x == t).unwrap().2;
        Vec2::new(p.x, p.y)
    };
    let (a1, a2) = (pos("A1"), pos("A2"));
    let w = h.state_mut().cad.run("wire", json!({"points": [[a1.x, a1.y], [a1.x + 60.0, a1.y]]})).unwrap();
    let wire = ConductorId(u128::from_str_radix(w["conductor"].as_str().unwrap(), 16).unwrap());
    let p2 = h.state_mut().cad.run("newpage", json!({"title": "Field"})).unwrap();
    let page2 = PageId(u128::from_str_radix(p2["page"].as_str().unwrap(), 16).unwrap());
    h.state_mut().show_page(page1).unwrap();
    h.run_steps(3);
    h.state_mut().zoom_to_drawing();
    h.run_steps(3);
    // A point on the A2 lead, toward the coil's body.
    let toward = Vec2::new(100.0, 100.0);
    let coil_at = Vec2::new(a2.x + (toward.x - a2.x) * 0.15, a2.y + (toward.y - a2.y) * 0.15);
    F { h, k1, wire, coil_at, a1, page2 }
}

fn exists(h: &mut H, k1: ComponentId) -> bool {
    doc(h).components.contains_key(&k1)
}

#[test]
fn right_click_on_the_coil_opens_its_wheel_and_both_gestures_delete_it_in_one_transaction() {
    let F { mut h, k1, coil_at, .. } = fixture();
    let at = to_screen(&h, coil_at);
    // Click, click: right press and release in the centre, then a click on Delete device.
    pointer(&mut h, at, PointerButton::Secondary, true);
    let t = h.state().wheel_target().cloned().expect("the wheel opened");
    assert_eq!((t.kind, t.name.as_str()), (ObjectKind::Device, "-K01"));
    h.get_by_label("-K01");
    pointer(&mut h, at, PointerButton::Secondary, false);
    assert!(h.state().wheel_target().is_some(), "released in the centre: still open");
    let before = head(&mut h);
    wheel_slot(&h, "Delete device").click();
    h.run_steps(3);
    assert!(h.state().wheel_target().is_none() && !exists(&mut h, k1));
    assert_eq!(head(&mut h), before + 1, "one transaction");
    // Ctrl+Z brings it back; press, move, release over the same slot deletes it again.
    keys(&mut h, Modifiers::COMMAND, Key::Z);
    assert!(exists(&mut h, k1), "Ctrl+Z undid it");
    pointer(&mut h, at, PointerButton::Secondary, true);
    let slot = ecad_ui::wheel::slot_center(h.state().shell.tools.wheel.as_ref().unwrap().widget.center, 4);
    pointer(&mut h, slot, PointerButton::Secondary, false);
    assert!(!exists(&mut h, k1), "press-move-release ran the same slot");
    // Ctrl+Y: redo after an undo.
    keys(&mut h, Modifiers::COMMAND, Key::Z);
    keys(&mut h, Modifiers::COMMAND, Key::Y);
    assert!(!exists(&mut h, k1), "Ctrl+Y redid the delete");
}

#[test]
fn a_greyed_slot_says_why_releasing_outside_cancels_and_no_wheel_over_hosted_views() {
    let F { mut h, coil_at, .. } = fixture();
    let at = to_screen(&h, coil_at);
    pointer(&mut h, at, PointerButton::Secondary, true);
    pointer(&mut h, at, PointerButton::Secondary, false);
    // -K01 has no symbol on another page: Go to contacts is greyed and says so.
    wheel_slot(&h, "Go to contacts").hover();
    h.run_steps(2);
    assert!(h.query_by_label("-K01 has no symbol on another page.").is_some());
    wheel_slot(&h, "Go to contacts").click();
    h.run_steps(2);
    assert!(h.state().wheel_target().is_some(), "a greyed slot runs nothing");
    // A click far outside cancels.
    pointer(&mut h, at + vec2(500.0, 0.0), PointerButton::Primary, true);
    pointer(&mut h, at + vec2(500.0, 0.0), PointerButton::Primary, false);
    assert!(h.state().wheel_target().is_none());
    // Releasing outside after a press cancels too.
    pointer(&mut h, at, PointerButton::Secondary, true);
    pointer(&mut h, at + vec2(400.0, 0.0), PointerButton::Secondary, false);
    assert!(h.state().wheel_target().is_none());
    // Lists, Library and Publish hide the page: it does not answer.
    for w in [Workspace::Lists, Workspace::Library, Workspace::Publish] {
        h.state_mut().set_workspace(w);
        h.run_steps(4);
        pointer(&mut h, at, PointerButton::Secondary, true);
        pointer(&mut h, at, PointerButton::Secondary, false);
        assert!(h.state().wheel_target().is_none(), "{w:?}");
    }
}

#[test]
fn while_wire_runs_right_click_finishes_the_segment_and_the_intent_line_replaces_the_command_line() {
    let F { mut h, a1, .. } = fixture();
    let main = h.state().shell.slots.unwrap().main.center();
    h.event(Event::PointerMoved(main));
    h.run_steps(2);
    // At rest: the intent line, no command line.
    assert!(!h.state().cad.ui.show_command_line);
    assert_eq!(h.state().intent_text(), "", "UI rev 3: nothing over the sheet at rest");
    keys(&mut h, Modifiers::NONE, Key::W);
    assert_eq!(running(&h).as_deref(), Some("wire"));
    assert!(!h.state().cad.ui.show_command_line, "no command line while prompting");
    assert!(h.state().intent_text().starts_with("WIRE:"), "{}", h.state().intent_text());
    // Hovering A1 of -K01 says what the click will join.
    let a1s = to_screen(&h, a1);
    h.event(Event::PointerMoved(a1s));
    h.run_steps(2);
    assert!(h.state().intent_text().contains("joins A1 of -K01"), "{}", h.state().intent_text());
    let reads = h.state().shell.tools.intent.reads;
    h.run_steps(5);
    assert_eq!(h.state().shell.tools.intent.reads, reads, "no reads while nothing moves");
    // First point, then a typed @10,0 shows in the line and Enter applies it.
    let page = h.state().shell.page_rect.unwrap();
    let start = page.left_top() + vec2(200.0, 200.0);
    pointer(&mut h, start, PointerButton::Primary, true);
    pointer(&mut h, start, PointerButton::Primary, false);
    h.event(Event::Text("@10,0".into()));
    h.run_steps(2);
    assert!(h.state().intent_text().contains("› @10,0"), "{}", h.state().intent_text());
    let before = head(&mut h);
    keys(&mut h, Modifiers::NONE, Key::Enter);
    assert!(h.state().cad.cmd.buffer.is_empty());
    // Right-click while Wire runs is Enter: the wire is written, and no wheel opens.
    assert_eq!(running(&h).as_deref(), Some("wire"), "the typed point kept Wire running");
    let rc = start + vec2(40.0, 0.0);
    pointer(&mut h, rc, PointerButton::Secondary, true);
    pointer(&mut h, rc, PointerButton::Secondary, false);
    assert!(h.state().wheel_target().is_none(), "no wheel while a tool runs");
    assert!(running(&h).is_none(), "right-click finished the wire: {:?}", h.state().cad.session.log.iter().rev().take(4).collect::<Vec<_>>());
    assert_eq!(head(&mut h), before + 1, "the wire with its typed segment, one transaction");
    // At rest again: nothing reads the project for the line.
    let reads = h.state().shell.tools.intent.reads;
    h.run_steps(5);
    assert_eq!(h.state().shell.tools.intent.reads, reads);
}

#[test]
fn a_refused_click_shows_what_why_and_show_it() {
    let F { mut h, .. } = fixture();
    let main = h.state().shell.slots.unwrap().main;
    h.event(Event::PointerMoved(main.center()));
    h.run_steps(2);
    // Connect asks for a wire near its end: a click on empty paper is refused.
    h.state_mut().cad.session.start("connect").unwrap();
    h.run_steps(2);
    let empty = main.left_top() + vec2(60.0, 60.0);
    pointer(&mut h, empty, PointerButton::Primary, true);
    pointer(&mut h, empty, PointerButton::Primary, false);
    let line = h.state().intent_text();
    assert!(line.starts_with("CONNECT: "), "what and why: {line}");
    h.get_by_label(intent::SHOW_IT).click();
    h.run_steps(2);
    cancel(&mut h);
}

#[test]
fn every_key_the_brief_names() {
    let F { mut h, k1, coil_at, .. } = fixture();
    let main = h.state().shell.slots.unwrap().main.center();
    h.event(Event::PointerMoved(main));
    h.run_steps(2);
    // V D T W C (UI rev 3 dropped P and B): each group's first tool; Select ends the running one.
    for (k, want) in [(Key::W, "wire"), (Key::C, "cableslash"), (Key::D, "line"), (Key::T, "text")] {
        cancel(&mut h);
        keys(&mut h, Modifiers::NONE, k);
        assert_eq!(running(&h).as_deref(), Some(want), "{k:?}");
    }
    for k in [Key::P, Key::B] {
        cancel(&mut h);
        keys(&mut h, Modifiers::NONE, k);
        assert!(running(&h).is_none(), "{k:?} starts nothing now");
    }
    // Esc ends the tool (back to selecting); V is Select.
    keys(&mut h, Modifiers::NONE, Key::Escape);
    assert!(running(&h).is_none(), "Esc");
    keys(&mut h, Modifiers::NONE, Key::V);
    assert!(running(&h).is_none(), "V: Select");
    // The coil selected: R, X, F2, Ctrl+D, Ctrl+C act on it; Ctrl+V pastes; Del deletes.
    let select = |h: &mut H| {
        let at = to_screen(h, coil_at);
        pointer(h, at, PointerButton::Primary, true);
        pointer(h, at, PointerButton::Primary, false);
        h.event(Event::PointerMoved(main));
        h.run_steps(2);
        assert!(!h.state().cad.session.selection().is_empty(), "the coil is selected");
    };
    for (m, k, want) in [
        (Modifiers::NONE, Key::R, "rotate"),
        (Modifiers::NONE, Key::X, "mirror"),
        (Modifiers::NONE, Key::F2, "deviceprops"),
        (Modifiers::COMMAND, Key::D, "copy"),
    ] {
        cancel(&mut h);
        select(&mut h);
        keys(&mut h, m, k);
        assert_eq!(running(&h).as_deref(), Some(want), "{m:?} {k:?}");
    }
    cancel(&mut h);
    select(&mut h);
    keys(&mut h, Modifiers::COMMAND, Key::C);
    assert!(running(&h).is_none_or(|r| r == "copyclip"), "Ctrl+C copied");
    cancel(&mut h);
    keys(&mut h, Modifiers::COMMAND, Key::V);
    assert_eq!(running(&h).as_deref(), Some("pasteclip"), "Ctrl+V");
    cancel(&mut h);
    select(&mut h);
    keys(&mut h, Modifiers::NONE, Key::Delete);
    assert!(!exists(&mut h, k1), "Del deleted -K01");
    keys(&mut h, Modifiers::COMMAND, Key::Z);
    assert!(exists(&mut h, k1), "Ctrl+Z");
    keys(&mut h, Modifiers::COMMAND, Key::Y);
    assert!(!exists(&mut h, k1), "Ctrl+Y");
}

#[test]
fn the_tool_strip_lists_only_tools_that_exist_and_steps_with_its_key() {
    let mut h = shell();
    let draw = tool_strip::GROUPS.iter().find(|g| g.letter() == Some('D')).unwrap();
    h.get_by_label(draw.label).hover();
    h.run_steps(2);
    for name in ["Line", "Polyline", "Rectangle", "Circle", "Arc"] {
        assert!(h.query_by_label(name).is_some(), "Draw lists {name}");
    }
    h.get_by_label("Polyline").click();
    h.run_steps(2);
    assert_eq!(running(&h).as_deref(), Some("pline"));
    // The key starts the last used, and steps.
    cancel(&mut h);
    let main = h.state().shell.slots.unwrap().main.center();
    h.event(Event::PointerMoved(main));
    h.run_steps(2);
    keys(&mut h, Modifiers::NONE, Key::D);
    assert_eq!(running(&h).as_deref(), Some("pline"), "the last used");
    keys(&mut h, Modifiers::NONE, Key::D);
    assert_eq!(running(&h).as_deref(), Some("rectang"), "stepped");
}

#[test]
fn ctrl_k_finds_a_page_a_tag_a_symbol_and_an_action_and_runs_the_action() {
    let F { mut h, wire, page2, .. } = fixture();
    let main = h.state().shell.slots.unwrap().main.center();
    h.event(Event::PointerMoved(main));
    h.run_steps(2);
    keys(&mut h, Modifiers::COMMAND, Key::K);
    assert!(h.state().search.open);
    let find = |h: &mut H, q: &str, want: &str| {
        h.state_mut().search.query = q.into();
        h.run_steps(3);
        assert!(h.query_all_by_label_contains(want).next().is_some(), "{q}: {want}");
    };
    find(&mut h, "Field", "(page)");
    find(&mut h, "-K01", "(device)");
    find(&mut h, "coil", "(symbol)");
    find(&mut h, "fix number", "(action)");
    // A page row shows the page.
    find(&mut h, "Field", "(page)");
    h.query_all_by_label_contains("(page)").next().unwrap().click();
    h.run_steps(3);
    assert_eq!(h.state().panel.active, Some(page2));
    // An action runs on the selection: Fix number on the selected wire.
    let page1 = doc(&mut h).page_order[0];
    h.state_mut().show_page(page1).unwrap();
    h.run_steps(2);
    let handles = {
        let t = h.state_mut();
        let p = t.project().unwrap().clone();
        p.read(&mut t.cad.session, |p, _| ecad_cadcraft::checks::handles_of(p, page1, ecad_cadcraft::checks::Object::Conductor(wire))).unwrap()
    };
    h.state_mut().cad.session.set_selection(handles);
    keys(&mut h, Modifiers::COMMAND, Key::K);
    h.state_mut().search.open = true;
    h.state_mut().search.query = "fix number".into();
    h.run_steps(3);
    h.query_all_by_label_contains("Fix number").next().unwrap().click();
    h.run_steps(3);
    assert_eq!(running(&h).as_deref(), Some("wirenumber"), "Fix number started on the wire");
    let prompt = h.state().cad.session.current_prompt().map(|p| p.message).unwrap_or_default();
    assert!(!prompt.to_lowercase().contains("click a wire"), "the wire was given: {prompt}");
}

#[test]
fn a_page_row_opens_its_wheel() {
    let F { mut h, .. } = fixture();
    h.get_by_label_contains("Page 1:").click_secondary();
    h.run_steps(3);
    let t = h.state().wheel_target().cloned().expect("the page row's wheel");
    assert_eq!(t.kind, ObjectKind::PageRow);
    wheel_slot(&h, "Rename").click();
    h.run_steps(3);
    assert!(h.state().name_box.is_some(), "Rename opens the name box");
}

/// The wheel, the intent line, a flyout and Ctrl+K at 1920 × 1080, for the owner's check (written
/// only when `ECAD_TESTER_SHOTS` names a folder and a wgpu adapter exists; nothing is asserted
/// about the pictures).
#[test]
fn screenshots_for_the_owner() {
    let Some(out) = crate::project_panel::shots() else { return };
    let deltas: crate::mouse_tester::Deltas = std::sync::Arc::default();
    let F { mut h, coil_at, a1, wire, .. } = fixture_with(Some(deltas.clone()));
    let shot = |h: &H, name: &str| {
        let _ = crate::mouse_tester::shot(h, &deltas, &out.join(name));
    };
    // The device wheel, hovering the greyed Go to contacts.
    let at = to_screen(&h, coil_at);
    pointer(&mut h, at, PointerButton::Secondary, true);
    pointer(&mut h, at, PointerButton::Secondary, false);
    wheel_slot(&h, "Go to contacts").hover();
    h.run_steps(3);
    shot(&h, "041-wheel-device.png");
    keys(&mut h, Modifiers::NONE, Key::Escape);
    // The wire's wheel with Set potential's list.
    let (a, b) = {
        let d = doc(&mut h);
        let page = d.page_order[0];
        let (_, prim) = ecad_model::derive::route_on_page(&d, wire, page).unwrap();
        prim.path_ends().unwrap()
    };
    let mid = to_screen(&h, Vec2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0));
    pointer(&mut h, mid, PointerButton::Secondary, true);
    pointer(&mut h, mid, PointerButton::Secondary, false);
    if h.state().wheel_target().is_some_and(|t| t.kind == ObjectKind::Wire) {
        wheel_slot(&h, "Set potential").click();
        h.run_steps(3);
    }
    shot(&h, "041-wheel-wire.png");
    keys(&mut h, Modifiers::NONE, Key::Escape);
    // Wire running, hovering A1: the intent line.
    h.event(Event::PointerMoved(h.state().shell.slots.unwrap().main.center()));
    h.run_steps(2);
    keys(&mut h, Modifiers::NONE, Key::W);
    h.event(Event::PointerMoved(to_screen(&h, a1)));
    h.run_steps(3);
    shot(&h, "041-intent-wire.png");
    cancel(&mut h);
    // The Draw flyout.
    let draw = tool_strip::GROUPS.iter().find(|g| g.letter() == Some('D')).unwrap();
    h.get_by_label(draw.label).hover();
    h.run_steps(3);
    shot(&h, "041-flyout.png");
    // Ctrl+K over actions.
    h.event(Event::PointerMoved(h.state().shell.slots.unwrap().main.center()));
    h.run_steps(2);
    keys(&mut h, Modifiers::COMMAND, Key::K);
    h.state_mut().search.query = "wire".into();
    h.run_steps(4);
    shot(&h, "041-search.png");
}

/// Review of #79: Ctrl+Shift+Z is redo, not undo; Ctrl+Z undoes and Ctrl+Y redoes. A chord with
/// an extra Shift is not taken by the plain key's action (Ctrl+Shift+C, Shift+R).
#[test]
fn ctrl_z_undoes_and_ctrl_shift_z_and_ctrl_y_redo() {
    let F { mut h, k1, coil_at, .. } = fixture();
    let main = h.state().shell.slots.unwrap().main.center();
    h.event(Event::PointerMoved(main));
    h.run_steps(2);
    let delete = |h: &mut H| {
        h.state_mut().cad.run("devicedelete", json!({"component": k1.hex()})).unwrap();
        h.run_steps(2);
    };
    delete(&mut h);
    let shift = Modifiers::COMMAND | Modifiers::SHIFT;
    for (redo, name) in [(shift, "Ctrl+Shift+Z"), (Modifiers::COMMAND, "Ctrl+Y")] {
        keys(&mut h, Modifiers::COMMAND, Key::Z);
        assert!(exists(&mut h, k1), "Ctrl+Z undid the delete");
        keys(&mut h, redo, if name == "Ctrl+Y" { Key::Y } else { Key::Z });
        assert!(!exists(&mut h, k1), "{name} redid it");
    }
    keys(&mut h, Modifiers::COMMAND, Key::Z);
    assert!(exists(&mut h, k1));
    // With the coil selected, Shift+R and Ctrl+Shift+C start neither Rotate nor Copy.
    let at = to_screen(&h, coil_at);
    pointer(&mut h, at, PointerButton::Primary, true);
    pointer(&mut h, at, PointerButton::Primary, false);
    h.event(Event::PointerMoved(main));
    h.run_steps(2);
    assert!(!h.state().cad.session.selection().is_empty());
    keys(&mut h, Modifiers::SHIFT, Key::R);
    assert_ne!(running(&h).as_deref(), Some("rotate"), "Shift+R is not R");
    cancel(&mut h);
    keys(&mut h, shift, Key::C);
    assert_ne!(running(&h).as_deref(), Some("copyclip"), "Ctrl+Shift+C is not Ctrl+C");
    cancel(&mut h);
}
