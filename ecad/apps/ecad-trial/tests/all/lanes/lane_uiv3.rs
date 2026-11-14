//! UI rev 3, board 1.01 ("Draw, both sides closed"): the frame. Region sizes at 1920 × 1080
//! against the build spec's table (`docs/discussions/ui-2026-11/rev3-build-spec.md`), the default
//! state, the strip's order and keys, the inspector rail, the bottom bar's groups, the sheet
//! fitted with a 24 px margin, and the screenshots for the side-by-side with the board.

use crate::project_panel::{doc, rev3_trial as trial};
use ecad_cadcraft::actions::ToolGroup;
use ecad_trial::Trial;
use ecad_trial::shell::state::{LeftTab, RightTab, ShellState, Workspace};
use ecad_trial::shell::{layout, right, status_bar, tool_strip, top_bar};
use eframe::egui::{Rect, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn at(state: &ShellState, w: f32, h: f32) -> layout::Slots {
    layout::layout(Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h)), state, false)
}

fn size(r: Rect) -> (f32, f32) {
    (r.width(), r.height())
}

#[test]
fn the_regions_at_1920_by_1080_are_the_specs() {
    let s = ShellState::default();
    let d = at(&s, 1920.0, 1080.0);
    assert_eq!(size(d.top_bar), (1920.0, 40.0), "top bar 40 px");
    assert_eq!(size(d.status), (1920.0, 32.0), "one bottom bar, 32 px");
    assert_eq!(d.tool_strip.map(size), Some((56.0, 1008.0)), "tool strip 56 px at the left edge");
    assert_eq!(d.tool_strip.map(|r| r.left()), Some(0.0));
    assert!(d.panel.is_none(), "both sides closed: no panel");
    assert!(d.right_rail && d.right.width() == 0.0, "the inspector is closed");
    assert_eq!((d.rail.left(), d.rail.width()), (1864.0, 56.0), "its rail is always at the right edge");
    assert_eq!(size(d.main), (1920.0 - 56.0 - 56.0, 1008.0), "the canvas is the rest");
    // A panel opened by the strip sits beside it: Book and Devices 248, History 280.
    for (tab, w) in [(LeftTab::Pages, 248.0), (LeftTab::Devices, 248.0), (LeftTab::History, 280.0)] {
        let mut o = s.clone();
        o.left.tab = tab;
        o.left.collapsed = false;
        let x = at(&o, 1920.0, 1080.0);
        assert_eq!(x.panel.map(|r| (r.left(), r.width())), Some((56.0, w)), "{tab:?}");
        assert_eq!(x.main.left(), 56.0 + w);
    }
    // The inspector open: 320 in Draw, 340 in the other workspaces, which have no strip or panel.
    let open = ShellState { right_collapsed: false, ..s.clone() };
    let o = at(&open, 1920.0, 1080.0);
    assert_eq!((o.right.left(), o.right.width(), o.rail.left()), (1864.0 - 320.0, 320.0, 1864.0), "beside the rail, as Book beside the strip");
    assert_eq!(o.main.right(), 1864.0 - 320.0);
    let project =
        ShellState { workspace: Workspace::Lists, left: ecad_trial::shell::state::LeftState { collapsed: false, ..Default::default() }, ..open };
    let p = at(&project, 1920.0, 1080.0);
    assert_eq!((p.right.width(), p.tool_strip, p.panel, p.main.left(), p.rail.width()), (340.0, None, None, 0.0, 56.0));
}

#[test]
fn both_sides_start_closed_and_the_workspace_is_called_project() {
    let s = ShellState::default();
    assert!(s.left.collapsed && s.right_collapsed);
    assert_eq!(s.right_tab, RightTab::Device);
    let labels: Vec<&str> = Workspace::ALL.iter().map(|w| w.label()).collect();
    assert_eq!(labels, ["Draw", "Project", "Library", "Publish"]);
    // A narrow window closes the panel after folding the inspector.
    let mut o = s.clone();
    o.left.collapsed = false;
    o.right_collapsed = false;
    let n = at(&o, 1100.0, 700.0);
    assert!(n.right_rail && n.panel.is_none(), "{:?}", n.panel);
}

#[test]
fn the_strip_is_in_the_specs_order_with_five_keys() {
    let names: Vec<&str> = tool_strip::GROUPS.iter().map(|g| g.group.name()).collect();
    assert_eq!(names, ["Select", "Draw", "Text", "Device", "Terminal", "PLC", "Wire", "Cable", "Location"]);
    let dividers: Vec<&str> = ToolGroup::ALL.iter().filter(|g| g.divider_after()).map(|g| g.name()).collect();
    assert_eq!(dividers, ["Text", "PLC", "Cable"]);
    let keys: String = tool_strip::GROUPS.iter().filter_map(|g| g.letter()).collect();
    assert_eq!(keys, "VDTWC", "P and B are dropped");
    let panels: Vec<&str> = tool_strip::PANELS.iter().map(|(t, _)| t.label()).collect();
    assert_eq!(panels, ["Book", "Devices", "History"]);
    let toggles: Vec<&str> = tool_strip::TOGGLES.iter().map(|t| t.0).collect();
    assert_eq!(toggles, ["Snap", "Grid", "Ortho"]);
    assert_eq!((ecad_ui::tokens::size::TOOL_BUTTON_W, ecad_ui::tokens::size::TOOL_BUTTON), (54.0, 44.0));
    assert_eq!((ecad_ui::tokens::size::TOOL_ICON, ecad_ui::tokens::size::TOOL_LABEL), (18.0, 10.0));
}

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

#[test]
fn the_frame_shows_its_buttons_and_they_open_what_they_name() {
    let mut h = harness();
    for label in [top_bar::PROJECT_MENU, top_bar::SEARCH, top_bar::AGENT_PILL, top_bar::HELP, status_bar::SAVE_STATE, status_bar::PAGE_WELL] {
        assert!(h.query_by_label_contains(label).is_some(), "{label}");
    }
    for g in tool_strip::GROUPS {
        assert!(h.query_by_label(g.label).is_some(), "{}", g.label);
    }
    // No branding, no revision pill in the top bar; Checks is in the bottom bar.
    assert!(h.query_by_label_contains(top_bar::CHECKS_PILL).is_some());
    // The strip's Book opens the panel beside it; pressed again it closes.
    h.get_by_label(&tool_strip::panel_label(LeftTab::Pages)).click();
    h.run_steps(3);
    assert!(!h.state().shell_state().left.collapsed && h.state().shell.slots.and_then(|s| s.panel).is_some());
    h.get_by_label(&tool_strip::panel_label(LeftTab::Pages)).click();
    h.run_steps(3);
    assert!(h.state().shell_state().left.collapsed);
    // The rail's Checks opens the inspector beside the rail on Checks; the rail's buttons are the
    // only tabs (no selector in the panel); pressed again it closes, as Book on the strip.
    h.get_by_label(&right::rail_label(RightTab::Checks)).click();
    h.run_steps(3);
    assert!(!h.state().shell_state().right_collapsed);
    assert_eq!(h.state().shell_state().right_tab, RightTab::Checks);
    assert_eq!(h.query_all_by_label(&right::rail_label(RightTab::Device)).count(), 1, "no tab selector in the panel");
    h.get_by_label(&right::rail_label(RightTab::Layout)).click();
    h.run_steps(3);
    assert_eq!(h.state().shell_state().right_tab, RightTab::Layout, "another rail button switches the tab");
    h.get_by_label(&right::rail_label(RightTab::Layout)).click();
    h.run_steps(3);
    assert!(h.state().shell_state().right_collapsed, "pressed again: closed");
    // Share is there and does nothing yet.
    assert!(h.query_by_label(top_bar::SHARE).is_some());
    // The bottom bar's checks counter opens the same tab.
    h.state_mut().shell.state.right_collapsed = true;
    h.run_steps(2);
    h.get_by_label_contains(top_bar::CHECKS_PILL).click();
    h.run_steps(3);
    assert!(!h.state().shell_state().right_collapsed && h.state().shell_state().right_tab == RightTab::Checks);
}

#[test]
fn a_page_first_opens_with_the_sheet_fitted_24_px_inside_the_canvas() {
    let mut h = harness();
    let pages = doc(&mut h).page_order.clone();
    let Some(&first) = pages.first() else { return };
    h.state_mut().show_page(first).unwrap();
    h.run_steps(6);
    let main = h.state().shell.slots.map(|s| s.main).unwrap();
    let xf = h.state().cad.canvas.xf.unwrap();
    let d = h.state().cad.session.doc().unwrap();
    let mut b = cadcraft_engine::geom::Bounds2::default();
    for e in d.model.iter().filter(|e| e.common.layer == ecad_cadcraft::frame::LAYER) {
        b = b.union(&cadcraft_engine::doc::entity_bounds(d, e, 0));
    }
    assert!(!b.is_empty(), "the page has a frame");
    let r = Rect::from_two_pos(xf.to_screen(b.min), xf.to_screen(b.max));
    let margins = [r.left() - main.left(), main.right() - r.right(), r.top() - main.top(), main.bottom() - r.bottom()];
    assert!(margins.iter().all(|m| *m >= 23.0), "{margins:?}");
    assert!(margins.iter().any(|m| (*m - 24.0).abs() < 1.5), "it fits one way: {margins:?}");
}

/// Give the open project nine pages in three folders, as the board's book: Power (3 pages),
/// Control (2) and Terminals (4). Returns the pages in book order.
fn book_in_folders(h: &mut Harness<'static, Trial>) -> Vec<ecad_model::PageId> {
    let run = |h: &mut Harness<'static, Trial>, cmd: &str, params: serde_json::Value| {
        let v = h.state_mut().cad.session.execute(cmd, &params).unwrap_or_else(|e| panic!("{cmd} {params}: {e}"));
        h.run_steps(2);
        v
    };
    while doc(h).page_order.len() < 9 {
        let n = doc(h).page_order.len() + 1;
        run(h, "newpage", serde_json::json!({"title": format!("Terminals {n}")}));
    }
    let order = doc(h).page_order.clone();
    let mut at = 0;
    for (name, count) in [("Power", 3), ("Control", 2), ("Terminals", 4)] {
        let f = run(h, "newfolder", serde_json::json!({"name": name}));
        let folder = f["folder"].as_str().unwrap().to_string();
        for p in order.iter().skip(at).take(count) {
            run(h, "movepage", serde_json::json!({"page": p.hex(), "folder": folder}));
        }
        at += count;
    }
    h.run_steps(4);
    order
}

#[test]
fn the_page_well_opens_the_shown_pages_group_and_folds_the_others() {
    let mut h = harness();
    let order = book_in_folders(&mut h);
    h.state_mut().show_page(order[1]).unwrap();
    h.run_steps(4);
    let groups: Vec<(String, usize)> = h.state().shell.well.groups.iter().map(|g| (g.name.clone(), g.pages.len())).collect();
    assert_eq!(groups, [("Power".to_string(), 3), ("Control".to_string(), 2), ("Terminals".to_string(), 4)]);
    // The shown page's group lists its pages; the others fold to a chip with their count.
    assert!(h.query_by_label("Page 02").is_some() || h.query_by_label("Page 2").is_some());
    assert!(h.query_by_label("Group Control").is_some() && h.query_by_label("Group Terminals").is_some());
    assert!(h.query_by_label("Group Power").is_none(), "the open group is not folded");
    // › on a folded group opens it as well; » goes to the next group's first page.
    h.get_by_label("Group Control").click();
    h.run_steps(3);
    assert!(h.query_by_label("Group Control").is_none());
    h.get_by_label(status_bar::NEXT_GROUP).click();
    h.run_steps(4);
    assert_eq!(h.state().panel.active, Some(order[3]));
    // The page shown sits at the well's centre.
    let well = h.get_by_label(status_bar::PAGE_WELL).rect();
    let prev = h.get_by_label(status_bar::PREV_PAGE).rect();
    let next = h.get_by_label(status_bar::NEXT_PAGE).rect();
    assert!(((prev.right() + next.left()) / 2.0 - well.center().x).abs() < 1.0, "centred between ‹ and ›");
    // A page button's right-click opens the page wheel, as a page row in Book does.
    let at = h.get_by_label("Page 5").rect().center();
    for pressed in [true, false] {
        h.event(eframe::egui::Event::PointerMoved(at));
        h.event(eframe::egui::Event::PointerButton {
            pos: at,
            button: eframe::egui::PointerButton::Secondary,
            pressed,
            modifiers: eframe::egui::Modifiers::NONE,
        });
        h.run_steps(2);
    }
    let t = h.state().wheel_target().cloned().expect("the page wheel");
    assert_eq!(t.kind, ecad_cadcraft::actions::ObjectKind::PageRow);
    assert_eq!(t.page, Some(order[4]));
}

/// Board 1.01 at 1920 × 1080 for the side-by-side (written only when `ECAD_TESTER_SHOTS` names a
/// folder and a wgpu adapter exists; nothing is asserted about the pictures): the start, then the
/// demo project with a page open in the well.
#[test]
fn screenshots_of_board_1_01() {
    let Some(out) = crate::project_panel::shots() else { return };
    ecad_trial::canvas::register_sheet_faces();
    let dir = std::env::temp_dir().join(format!("ecad-uiv3-shots-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = crate::checks_ui::demo_file(&dir);
    let deltas: crate::mouse_tester::Deltas = std::sync::Arc::default();
    // The whole window, as the app's root `Ui` (the harness keeps an 8 px margin round its own).
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
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("uiv3-1.01-start.png"));
    h.state_mut().close_project();
    h.run_steps(2);
    h.state_mut().open_project(&path).unwrap();
    h.run_steps(3);
    let order = book_in_folders(&mut h);
    if let Some(&p) = order.get(1).or(order.first()) {
        h.state_mut().show_page(p).unwrap();
    }
    h.run_steps(8);
    h.state_mut().fit_sheet();
    h.run_steps(4);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("uiv3-1.01-ours.png"));
    // Round 2: the inspector open beside its rail, on Device.
    h.state_mut().open_inspector(RightTab::Device);
    h.run_steps(4);
    h.state_mut().fit_sheet();
    h.run_steps(4);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("uiv3-1.01-r2-right.png"));
    let _ = std::fs::remove_dir_all(&dir);
}
