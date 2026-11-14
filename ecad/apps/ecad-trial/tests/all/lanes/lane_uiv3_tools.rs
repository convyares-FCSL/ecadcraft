//! UI rev 3, section C: the tool strip's flyouts (boards 1.09 Location and 1.10 Terminal), placing
//! a symbol with the picker folded and open (1.11, 1.12), the connector maker (1.13) and the
//! radial wheel (1.14). Behaviour is today's; these check the layout the boards ask for, and write
//! the screenshots for the side-by-side with the boards.

use crate::project_panel::{doc, rev3_trial as trial};
use ecad_trial::Trial;
use ecad_trial::shell::state::RightTab;
use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

type H = Harness<'static, Trial>;

fn pointer(h: &mut H, at: Pos2, button: PointerButton, pressed: bool) {
    h.event(Event::PointerMoved(at));
    h.event(Event::PointerButton { pos: at, button, pressed, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

fn hover(h: &mut H, at: Pos2) {
    h.event(Event::PointerMoved(at));
    h.run_steps(3);
}

/// The whole window at 1920 × 1080 as the app's root `Ui`, with a renderer for screenshots.
fn window(deltas: Option<crate::mouse_tester::Deltas>) -> H {
    let b = Harness::builder().with_size(vec2(1920.0, 1080.0));
    let app = |ui: &mut egui::Ui, t: &mut Trial| {
        let r = ui.ctx().content_rect();
        ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
            ui.set_clip_rect(r);
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        });
    };
    let mut h = match deltas {
        Some(d) => b.renderer(crate::mouse_tester::Collect(d)).build_ui_state(app, trial()),
        None => b.build_ui_state(app, trial()),
    };
    h.run_steps(5);
    h
}

/// The demo project open on its second page, fitted, with the inspector open on Device.
fn demo(h: &mut H, dir: &std::path::Path) {
    let path = crate::checks_ui::demo_file(dir);
    h.state_mut().close_project();
    h.run_steps(2);
    h.state_mut().open_project(&path).unwrap();
    h.run_steps(3);
    let order = doc(h).page_order.clone();
    if let Some(&p) = order.get(1).or(order.first()) {
        h.state_mut().show_page(p).unwrap();
    }
    h.state_mut().open_inspector(RightTab::Device);
    h.run_steps(6);
    h.state_mut().fit_sheet();
    h.run_steps(4);
}

/// A temporary folder for one test's demo project.
fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ecad-uiv3c-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Hover group `label`'s button so its flyout opens.
fn open_flyout(h: &mut H, label: &str) {
    let at = h.get_by_label(label).rect().center();
    hover(h, at);
    hover(h, at);
}

/// The running command, if any.
fn running(h: &H) -> Option<String> {
    h.state().cad.session.running.as_ref().map(|r| r.id.to_lowercase())
}

/// Where -H01 (the demo's lamp on page 2) is drawn at the fitted view.
const LAMP: Pos2 = Pos2::new(396.0, 813.0);

#[test]
fn every_group_has_a_flyout_with_the_boards_items_and_placeholders_stay_greyed() {
    use ecad_cadcraft::actions::{Enabled, ToolGroup};
    let labels = |g: ToolGroup| g.tools().iter().map(|a| a.label).collect::<Vec<_>>();
    assert_eq!(
        labels(ToolGroup::Terminal),
        ["Terminal", "Terminals across wires", "Draw a link (bridge)", "Swap top and bottom", "Reorder the strip…", "Renumber…"]
    );
    assert_eq!(ToolGroup::Terminal.link().map(|a| a.label), Some("Terminal manager"));
    assert_eq!(labels(ToolGroup::Plc), ["Rack overview", "Detail page…", "Card…", "Channel…"]);
    assert_eq!(labels(ToolGroup::Location), ["Location box", "Function box", "Function and location box", "Device or PLC outline"]);
    assert_eq!(labels(ToolGroup::Wire), ["Wire", "Bridge", "Continuation"]);
    let cx = ecad_cadcraft::actions::Context { target: None, writable: true };
    let greyed: Vec<&str> =
        ToolGroup::ALL.iter().flat_map(|g| g.tools()).filter(|a| matches!((a.enabled)(&cx), Enabled::Greyed { .. })).map(|a| a.id).collect();
    assert_eq!(greyed, ["terminal.swap", "terminal.reorder", "box.function", "box.both", "box.outline"]);

    let dir = temp("flyouts");
    let mut h = window(None);
    demo(&mut h, &dir);
    // Every group but Select opens a flyout on hover, with its items and its note.
    for g in ecad_trial::shell::tool_strip::GROUPS.iter().filter(|g| g.group != ToolGroup::Select) {
        open_flyout(&mut h, g.label);
        assert!(h.query_by_label(&ecad_trial::shell::tool_strip::flyout_label(g)).is_some(), "{} opens its flyout", g.label);
        for a in g.tools() {
            assert!(h.query_by_label(a.label).is_some(), "{} lists {}", g.label, a.label);
        }
        hover(&mut h, Pos2::new(900.0, 500.0));
    }
    // A placeholder does nothing when clicked; the Terminal manager link opens Project ▸ Terminals.
    open_flyout(&mut h, "Location group");
    h.get_by_label("Function box").click();
    h.run_steps(2);
    assert_eq!(running(&h), None, "a placeholder starts nothing");
    open_flyout(&mut h, "Terminal group");
    h.get_by_label("Terminal manager").click();
    h.run_steps(3);
    assert_eq!(h.state().shell.state.workspace, ecad_trial::shell::state::Workspace::Lists);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn placing_opens_the_picker_then_folds_it_to_a_chip_that_p_reopens() {
    use ecad_trial::shell::placing::{self, Shown};
    let dir = temp("picker");
    let mut h = window(None);
    demo(&mut h, &dir);
    // Device ▸ Symbol asks for a symbol: the picker is open, on Symbol.
    h.state_mut().start_tool(ecad_cadcraft::actions::find("place.symbol").unwrap());
    h.run_steps(3);
    assert_eq!(h.state().picker_shown(), Shown::Open);
    assert!(h.query_by_label(placing::PICKER).is_some());
    // A tile chooses the symbol: the picker folds to its chip, which names the next tag.
    h.get_all_by_label("Contactor / relay coil").next().expect("its tile").click();
    h.run_steps(3);
    assert_eq!(running(&h).as_deref(), Some("place"));
    assert_eq!(h.state().picker_shown(), Shown::Chip);
    let chip = h.state().shell.tools.chip_text.clone();
    assert!(chip.starts_with("Contactor / relay coil as -K"), "{chip}");
    // The intent line sits at the top of the canvas, clear of the title block, and names the symbol.
    let line = h.get_by_label(ecad_trial::shell::intent::LINE).rect();
    let canvas = h.state().shell.page_rect.unwrap();
    assert!(line.top() < canvas.top() + 30.0, "at the top of the canvas: {line:?}");
    // P opens the picker again; Change does too.
    hover(&mut h, Pos2::new(1100.0, 400.0));
    h.key_press(eframe::egui::Key::P);
    h.run_steps(3);
    assert_eq!(h.state().picker_shown(), Shown::Open);
    h.state_mut().cad.session.cancel();
    h.run_steps(3);
    assert_eq!(h.state().picker_shown(), Shown::Nothing);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_connector_maker_lays_out_044s_questions_and_its_chips_answer_them() {
    use ecad_trial::shell::connector;
    let dir = temp("maker");
    let mut h = window(None);
    demo(&mut h, &dir);
    h.state_mut().start_tool(ecad_cadcraft::actions::find("place.connector").unwrap());
    h.run_steps(3);
    assert!(h.query_by_label(connector::MAKER).is_some());
    for (title, _) in connector::SECTIONS {
        assert!(h.query_by_label(title).is_some(), "section {title}");
    }
    // New connector, then Plug and socket: the form moves on to its Type question.
    h.state_mut().cad.session.input(cadcraft_engine::Input::Keyword("New".into())).unwrap();
    h.run_steps(3);
    h.get_by_label("Plug and socket").click();
    h.run_steps(3);
    let p = h.state().cad.session.current_prompt().unwrap();
    assert_eq!(p.message.trim(), "Type");
    assert_eq!(h.state().shell.tools.maker.answers.get("Halves").map(String::as_str), Some("Both"));
    // A variant the library does not have yet is greyed and answers nothing.
    h.get_by_label("Han 3A").click();
    h.run_steps(2);
    assert_eq!(h.state().cad.session.current_prompt().unwrap().message.trim(), "Type");
    h.state_mut().cad.session.cancel();
    h.run_steps(2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_wheel_is_about_190_px_with_icon_slots_that_name_themselves() {
    let across = 2.0 * ecad_ui::tokens::wheel::OUTER_RADIUS;
    assert!((180.0..=200.0).contains(&across), "{across}");
    let dir = temp("wheel");
    let mut h = window(None);
    demo(&mut h, &dir);
    pointer(&mut h, LAMP, PointerButton::Secondary, true);
    pointer(&mut h, LAMP, PointerButton::Secondary, false);
    let t = h.state().wheel_target().cloned().expect("the device wheel");
    assert_eq!(t.kind, ecad_cadcraft::actions::ObjectKind::Device);
    // Fixed slots: go-to at the right, its row at the bottom right, delete at the bottom.
    let slot = |h: &H, i: usize| h.state().shell.tools.wheel.as_ref().and_then(|w| w.slots[i]).map(|a| a.id);
    assert_eq!(slot(&h, 2), Some("device.goto"));
    assert_eq!(slot(&h, 3), Some("device.row"));
    assert_eq!(slot(&h, 4), Some("device.delete"));
    // The slot under the cursor names itself in a tooltip; the slots carry no text of their own.
    let top = ecad_ui::wheel::slot_center(LAMP, 0);
    hover(&mut h, top);
    let label = h.state().shell.tools.wheel.as_ref().and_then(|w| w.specs[0].clone()).map(|s| s.label).unwrap();
    assert_eq!(h.get_all_by_label(&label).count(), 1, "named once: by its tooltip, which is not painted text on the ring");
    h.key_press(eframe::egui::Key::Escape);
    h.run_steps(2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_ortho_popout_sets_90_degrees_45_degree_steps_or_free() {
    use ecad_trial::shell::toggles::{self, Ortho};
    let mut h = window(None);
    let at = h.get_by_label("Ortho").rect().center();
    hover(&mut h, at);
    hover(&mut h, at);
    assert!(h.query_by_label(toggles::ORTHO).is_some(), "hovering Ortho opens its popout");
    h.get_by_label("45° steps").click();
    h.run_steps(2);
    assert_eq!(toggles::ortho(&h.state().cad.session.drafting_settings()), Ortho::Steps45);
    hover(&mut h, at);
    h.get_by_label("90° only").click();
    h.run_steps(2);
    assert_eq!(toggles::ortho(&h.state().cad.session.drafting_settings()), Ortho::Right);
    // A click on the toggle frees the angle again.
    h.get_by_label("Ortho").click();
    h.run_steps(2);
    assert_eq!(toggles::ortho(&h.state().cad.session.drafting_settings()), Ortho::Free);
}

/// Boards 1.09 to 1.14 at 1920 × 1080 for the side-by-side (written only when
/// `ECAD_TESTER_SHOTS` names a folder and a wgpu adapter exists; nothing is asserted about the
/// pictures).
#[test]
fn screenshots_of_boards_1_09_to_1_14() {
    let Some(out) = crate::project_panel::shots() else { return };
    ecad_trial::canvas::register_sheet_faces();
    let dir = temp("shots");
    let deltas: crate::mouse_tester::Deltas = std::sync::Arc::default();
    let mut h = window(Some(deltas.clone()));
    demo(&mut h, &dir);
    let shot = |h: &H, name: &str| {
        let _ = crate::mouse_tester::shot(h, &deltas, &out.join(name));
    };

    // 1.09 and 1.10: the Location and Terminal flyouts.
    for (label, name) in [("Location group", "uiv3-C-1.09-ours.png"), ("Terminal group", "uiv3-C-1.10-ours.png")] {
        open_flyout(&mut h, label);
        shot(&h, name);
        hover(&mut h, Pos2::new(900.0, 500.0));
    }

    // 1.11 and 1.12: placing a contactor coil, the picker folded and then open.
    h.state_mut().start_pick(ecad_trial::search::Pick::Symbol("coil")).unwrap();
    h.run_steps(3);
    hover(&mut h, Pos2::new(1200.0, 380.0));
    shot(&h, "uiv3-C-1.11-ours.png");
    h.state_mut().open_picker();
    h.run_steps(3);
    hover(&mut h, Pos2::new(1200.0, 380.0));
    shot(&h, "uiv3-C-1.12-ours.png");
    h.state_mut().cad.session.cancel();
    h.run_steps(3);

    // 1.13: the connector maker, at its Type question.
    h.state_mut().start_tool(ecad_cadcraft::actions::find("place.connector").unwrap());
    h.run_steps(3);
    for k in ["New", "Both"] {
        let _ = h.state_mut().cad.session.input(cadcraft_engine::Input::Keyword(k.into()));
        h.run_steps(3);
    }
    hover(&mut h, Pos2::new(700.0, 560.0));
    shot(&h, "uiv3-C-1.13-ours.png");
    h.state_mut().cad.session.cancel();
    h.run_steps(3);

    // 1.14: the wheel over a device, its top slot under the cursor.
    pointer(&mut h, LAMP, PointerButton::Secondary, true);
    pointer(&mut h, LAMP, PointerButton::Secondary, false);
    hover(&mut h, ecad_ui::wheel::slot_center(LAMP, 0));
    shot(&h, "uiv3-C-1.14-ours.png");
    // And the Ortho popout.
    h.key_press(eframe::egui::Key::Escape);
    h.run_steps(2);
    let at = h.get_by_label("Ortho").rect().center();
    hover(&mut h, at);
    hover(&mut h, at);
    shot(&h, "uiv3-C-popout-ortho.png");
    let _ = std::fs::remove_dir_all(&dir);
}
