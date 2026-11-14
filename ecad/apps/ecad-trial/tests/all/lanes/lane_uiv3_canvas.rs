//! UI rev 3, section D (boards 1.15 to 1.18): the light and dark interface, the sheet colour
//! menu (Paper, Follow, Dark), the zones numbered across and lettered down, and the screenshots
//! for the side-by-sides.

use ecad_trial::Trial;
use ecad_trial::canvas::Sheet;
use ecad_trial::shell::state::{RightTab, ShellState, Theme};
use ecad_trial::shell::{status_bar, top_bar};
use eframe::egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::project_panel::trial;

fn harness() -> Harness<'static, Trial> {
    let mut h = Harness::builder().with_size(vec2(1920.0, 1080.0)).build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        trial(),
    );
    h.run_steps(5);
    h
}

fn background(h: &Harness<'static, Trial>) -> Option<eframe::egui::Color32> {
    h.state().cad.options.model_background
}

#[test]
fn the_light_switch_turns_the_shell_light_and_is_remembered() {
    let mut h = harness();
    assert_eq!(h.state().shell_state().theme, Theme::Dark, "dark by default");
    assert_eq!(ecad_ui::theme(), ecad_ui::Theme::Dark);
    assert_eq!(ecad_ui::SHELL.panel, ecad_ui::tokens::GRAPHITE.panel);
    h.get_by_label(top_bar::THEME_LIGHT).click();
    h.run_steps(3);
    assert_eq!(h.state().shell_state().theme, Theme::Light);
    assert_eq!(ecad_ui::theme(), ecad_ui::Theme::Light);
    assert_eq!(ecad_ui::SHELL.panel, ecad_ui::tokens::LIGHT.panel, "the shell reads the light set");
    assert!(!h.ctx.style_of(eframe::egui::Theme::Dark).visuals.dark_mode, "egui's own widgets follow");
    assert_eq!(h.ctx.style_of(eframe::egui::Theme::Dark).visuals.panel_fill, ecad_ui::color(ecad_ui::tokens::LIGHT.panel));
    let stored = h.state().shell_state().to_stored();
    assert_eq!(ShellState::from_stored(Some(&stored)).0.theme, Theme::Light, "kept with the shell state");
    assert_eq!(ShellState::from_stored(Some("{}")).0.theme, Theme::Dark);
    h.get_by_label(top_bar::THEME_DARK).click();
    h.run_steps(3);
    assert_eq!(ecad_ui::SHELL.panel, ecad_ui::tokens::GRAPHITE.panel);
    assert!(h.ctx.style_of(eframe::egui::Theme::Dark).visuals.dark_mode);
}

#[test]
fn the_sheet_colour_menu_offers_paper_follow_and_dark_and_follow_follows_the_theme() {
    let mut h = harness();
    let paper = Some(ecad_ui::color(ecad_ui::tokens::sheet::PAPER.sheet));
    let dark = Some(ecad_ui::color(ecad_ui::tokens::sheet::DARK.sheet));
    assert_eq!(background(&h), paper, "paper by default");
    h.get_by_label(status_bar::SHEET_COLOUR).click();
    h.run_steps(2);
    assert!(h.query_by_label(status_bar::SHEET_MENU_TITLE).is_some());
    for name in [status_bar::PAPER, status_bar::FOLLOW, status_bar::DARK] {
        assert!(h.query_by_label(name).is_some(), "{name} in the menu");
    }
    h.get_by_label(status_bar::FOLLOW).click();
    h.run_steps(3);
    assert_eq!(h.state().shell_state().sheet, Sheet::Follow);
    assert_eq!(background(&h), dark, "Follow in the dark interface is the dark sheet");
    h.state_mut().shell.state.theme = Theme::Light;
    h.run_steps(3);
    assert_eq!(background(&h), paper, "Follow in the light interface is paper");
    // Paper and Dark keep their colour whatever the theme.
    h.state_mut().shell.state.sheet = Sheet::Dark;
    h.run_steps(3);
    assert_eq!(background(&h), dark);
    h.state_mut().shell.state.theme = Theme::Dark;
    h.state_mut().shell.state.sheet = Sheet::Paper;
    h.run_steps(3);
    assert_eq!(background(&h), paper);
    let stored = ShellState { sheet: Sheet::Follow, ..ShellState::default() }.to_stored();
    assert_eq!(ShellState::from_stored(Some(&stored)).0.sheet, Sheet::Follow, "kept with the shell state");
}

#[test]
fn ui_sheet_takes_follow() {
    let mut h = harness();
    h.state_mut().sheet_command("ui.sheet follow");
    assert_eq!(h.state().shell_state().sheet, Sheet::Follow);
    h.state_mut().sheet_command("ui.sheet");
    assert_eq!(h.state().shell_state().sheet, Sheet::Paper, "alone it switches away from the dark sheet shown");
    h.state_mut().sheet_command("ui.sheet");
    assert_eq!(h.state().shell_state().sheet, Sheet::Dark);
}

#[test]
fn a_new_projects_zones_are_numbered_across_and_lettered_down() {
    let mut h = harness();
    let p = h.state().project().cloned().expect("a project");
    let (col, row) = p
        .read(&mut h.state_mut().cad.session, |p, _| {
            let doc = p.ledger.document();
            let page = doc.page_order.first().copied().expect("a page");
            let g = ecad_model::sheet::Grid::of(doc, page);
            (g.column_label(5), g.row_label(2))
        })
        .expect("read");
    assert_eq!((col.as_str(), row.as_str()), ("5", "B"), "zones A–F down the sides, 1–8 across the top");
}

/// Boards 1.15, 1.16, 1.17 and 1.18 at 1920 × 1080 for the side-by-sides (written only when
/// `ECAD_TESTER_SHOTS` names a folder and a wgpu adapter exists; nothing is asserted about the
/// pictures).
#[test]
fn screenshots_of_boards_1_15_to_1_18() {
    let Some(out) = crate::project_panel::shots() else { return };
    ecad_trial::canvas::register_sheet_faces();
    let dir = std::env::temp_dir().join(format!("ecad-uiv3d-shots-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = crate::checks_ui::demo_file(&dir);
    let deltas: crate::mouse_tester::Deltas = std::sync::Arc::default();
    let mut h = Harness::builder().with_size(vec2(1920.0, 1080.0)).renderer(crate::mouse_tester::Collect(deltas.clone())).build_ui_state(
        |ui, t: &mut Trial| {
            let r = ui.ctx().content_rect();
            ui.scope_builder(eframe::egui::UiBuilder::new().max_rect(r), |ui| {
                ui.set_clip_rect(r);
                t.logic(&ui.ctx().clone());
                t.ui(ui);
            });
        },
        trial(),
    );
    h.run_steps(6);
    h.state_mut().close_project();
    h.run_steps(2);
    h.state_mut().open_project(&path).unwrap();
    h.run_steps(3);
    let project = h.state().project().cloned();
    let order = project.and_then(|p| p.read(&mut h.state_mut().cad.session, |p, _| p.ledger.document().page_order.clone()).ok()).unwrap_or_default();
    let page = order.get(1).or(order.first()).copied();
    let fit = |h: &mut Harness<'static, Trial>| {
        h.run_steps(6);
        h.state_mut().fit_sheet();
        h.run_steps(4);
    };
    // 1.17: the light interface on paper, Book and the inspector open.
    if let Some(p) = page {
        h.state_mut().show_page(p).unwrap();
    }
    h.state_mut().shell.state.theme = Theme::Light;
    h.state_mut().shell.state.sheet = Sheet::Paper;
    h.state_mut().shell.state.left.collapsed = false;
    h.state_mut().open_inspector(RightTab::Device);
    fit(&mut h);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("uiv3-D-1.17-ours.png"));
    // 1.18: full dark, the sheet following the theme, the sheet colour menu open.
    h.state_mut().shell.state.theme = Theme::Dark;
    h.state_mut().shell.state.sheet = Sheet::Follow;
    h.state_mut().shell.state.left.collapsed = true;
    fit(&mut h);
    h.get_by_label(status_bar::SHEET_COLOUR).click();
    h.run_steps(4);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("uiv3-D-1.18-ours.png"));
    h.key_press(eframe::egui::Key::Escape);
    h.run_steps(2);
    // 1.15: the parts list page, its lines listed under each device, Book open.
    h.state_mut().shell.state.sheet = Sheet::Paper;
    h.state_mut().shell.state.left.collapsed = false;
    let _ = h.state_mut().cad.session.execute("setdefault", &serde_json::json!({"key": "bom.lines", "level": "project", "value": "nested"}));
    if let Ok(v) = h.state_mut().cad.session.execute("newpage", &serde_json::json!({"generated": "bom-location", "title": "Parts list"})) {
        h.run_steps(4);
        let hex = v["page"].as_str().unwrap_or_default().rsplit('-').next().unwrap_or_default().to_string();
        if let Ok(n) = u128::from_str_radix(&hex, 16) {
            let _ = h.state_mut().show_page(ecad_model::PageId::from(n));
        }
    }
    fit(&mut h);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("uiv3-D-1.15-ours.png"));
    // 1.16: the Compare layout, opened from History.
    if let Some(p) = page {
        h.state_mut().show_page(p).unwrap();
    }
    h.state_mut().open_compare();
    fit(&mut h);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("uiv3-D-1.16-ours.png"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn compare_lays_out_a_header_over_the_canvas_and_a_list_under_it_and_closes() {
    let mut h = harness();
    let before = h.state().shell.slots.expect("slots").main;
    h.state_mut().open_compare();
    h.run_steps(3);
    let slots = h.state().shell.slots.expect("slots");
    let (head, list) = slots.compare.expect("compare's slots");
    assert_eq!((head.height(), list.height()), (ecad_trial::shell::layout::COMPARE_HEADER, ecad_trial::shell::layout::COMPARE_LIST));
    assert!(slots.main.top() >= head.bottom() && slots.main.bottom() <= list.top(), "the canvas sits between them");
    assert!(slots.main.height() < before.height());
    assert!(h.query_by_label(ecad_trial::shell::compare::NOT_YET).is_some(), "the list says what comes");
    h.get_by_label(ecad_trial::shell::compare::CLOSE).click();
    h.run_steps(3);
    assert!(!h.state().shell_state().compare);
    assert_eq!(h.state().shell.slots.expect("slots").main, before);
}
