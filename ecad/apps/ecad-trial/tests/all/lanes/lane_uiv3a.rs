//! UI rev 3, section A (boards 1.02, 1.03, 1.04 and 1.08): the left panels. Pin or let close
//! (a pinned panel docks, an unpinned one floats and closes when drawing starts), Book's
//! thumbnails, ticks and footer, Devices' grouping with "Not on a page yet" first, History's
//! sections, the page well's hover preview, and the screenshots for the side-by-side.

use crate::project_panel::{doc, rev3_trial as trial};
use ecad_trial::Trial;
use ecad_trial::shell::state::{LeftTab, ShellState};
use ecad_trial::shell::{layout, left, tool_strip};
use ecad_trial::{devices_panel, history_panel, project_panel};
use eframe::egui::{self, Rect, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn at(state: &ShellState) -> layout::Slots {
    layout::layout(Rect::from_min_size(pos2(0.0, 0.0), vec2(1920.0, 1080.0)), state, false)
}

#[test]
fn a_pinned_panel_docks_and_an_unpinned_one_floats_over_the_drawing() {
    let mut s = ShellState::default();
    assert_eq!(s.left.pinned, Some(LeftTab::Pages), "Book starts pinned");
    s.left.collapsed = false;
    let docked = at(&s);
    assert!(!docked.panel_floats);
    assert_eq!(docked.main.left(), 56.0 + 248.0, "the drawing moves over for a pinned panel");
    // Devices is not pinned: it floats over the drawing, which stays where it was.
    s.left.tab = LeftTab::Devices;
    let float = at(&s);
    assert!(float.panel_floats);
    assert_eq!(float.panel.map(|r| (r.left(), r.width())), Some((56.0, 248.0)));
    assert_eq!(float.main.left(), 56.0, "the drawing does not move");
    // One pinned at a time: pinning History unpins Book.
    s.left.tab = LeftTab::History;
    s.left.pinned = Some(LeftTab::History);
    let h = at(&s);
    assert!(!h.panel_floats && h.main.left() == 56.0 + 280.0);
    s.left.tab = LeftTab::Pages;
    assert!(at(&s).panel_floats, "Book floats once History is the pinned one");
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

fn press(h: &mut Harness<'static, Trial>, at: egui::Pos2) {
    for pressed in [true, false] {
        h.event(egui::Event::PointerMoved(at));
        h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        h.run_steps(2);
    }
}

#[test]
fn the_pin_docks_the_panel_and_an_unpinned_panel_closes_when_drawing_starts() {
    let mut h = harness();
    h.get_by_label(&tool_strip::panel_label(LeftTab::Devices)).click();
    h.run_steps(3);
    assert!(h.state().shell.slots.is_some_and(|s| s.panel_floats), "Devices opens floating");
    // A press on the drawing closes it.
    let main = h.state().shell.slots.map(|s| s.main).unwrap();
    press(&mut h, main.center() + vec2(200.0, 0.0));
    assert!(h.state().shell_state().left.collapsed, "closed when drawing starts");
    // Pinned, it docks and stays open.
    h.get_by_label(&tool_strip::panel_label(LeftTab::Devices)).click();
    h.run_steps(3);
    h.get_by_label(left::PIN).click();
    h.run_steps(3);
    assert_eq!(h.state().shell_state().left.pinned, Some(LeftTab::Devices));
    let s = h.state().shell.slots.unwrap();
    assert!(!s.panel_floats && s.panel.is_some_and(|p| s.main.left() == p.right()), "{s:?}");
    press(&mut h, s.main.center() + vec2(200.0, 0.0));
    assert!(!h.state().shell_state().left.collapsed, "a pinned panel stays");
    // The close button closes it.
    h.get_by_label(left::CLOSE).click();
    h.run_steps(3);
    assert!(h.state().shell_state().left.collapsed);
}

/// Nine pages in three groups, as board 1.02's book.
fn book(h: &mut Harness<'static, Trial>) -> Vec<ecad_model::PageId> {
    let run = |h: &mut Harness<'static, Trial>, cmd: &str, params: serde_json::Value| {
        let v = h.state_mut().cad.session.execute(cmd, &params).unwrap_or_else(|e| panic!("{cmd} {params}: {e}"));
        h.run_steps(2);
        v
    };
    let titles = [
        "Supply",
        "Motor control",
        "Field devices",
        "PLC inputs",
        "PLC outputs",
        "Terminal plan -X01",
        "Terminal plan -X02",
        "Cable list",
        "Parts list",
    ];
    while doc(h).page_order.len() < titles.len() {
        let n = doc(h).page_order.len();
        run(h, "newpage", serde_json::json!({"title": titles.get(n).copied().unwrap_or("Page")}));
    }
    let order = doc(h).page_order.clone();
    for (p, title) in order.iter().zip(titles) {
        run(h, "renamepage", serde_json::json!({"page": p.hex(), "title": title}));
    }
    let mut at = 0;
    for (name, count) in [("Power and control", 3), ("PLC", 2), ("Reports", 4)] {
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
fn book_ticks_pages_apart_from_the_open_one_and_moves_them_to_a_group() {
    let mut h = harness();
    let order = book(&mut h);
    h.state_mut().show_page(order[1]).unwrap();
    h.get_by_label(&tool_strip::panel_label(LeftTab::Pages)).click();
    h.run_steps(4);
    assert!(h.state().panel.thumbnails, "thumbnails by default");
    assert!(h.query_by_label(&project_panel::add_in_label("PLC")).is_some(), "each group has its +");
    // Ticking two pages leaves the open page as it was and shows the footer.
    h.get_by_label(&project_panel::pick_label("4")).click();
    h.run_steps(2);
    h.get_by_label(&project_panel::pick_label("5")).click();
    h.run_steps(3);
    assert_eq!(h.state().panel.picked.len(), 2);
    assert_eq!(h.state().panel.active, Some(order[1]), "ticking does not open a page");
    assert!(h.query_by_label_contains("2 pages selected").is_some() || h.query_by_label(project_panel::MOVE_TO_GROUP).is_some());
    assert!(h.query_by_label(project_panel::MOVE_TO_GROUP).is_some());
    // Move to group ▸ Reports moves both.
    h.get_by_label(project_panel::MOVE_TO_GROUP).click();
    h.run_steps(3);
    h.get_by_label("Reports").click();
    h.run_steps(4);
    let rows = h.state().panel.rows().to_vec();
    let folder_of = |p| rows.iter().find(|r| r.id == p).and_then(|r| r.folder);
    let reports = folder_of(order[8]);
    assert!(reports.is_some());
    for p in [order[3], order[4]] {
        assert_eq!(folder_of(p), reports, "moved into Reports");
    }
    // Clear empties the ticks; the list view switch works.
    let n = h.query_all_by_label(project_panel::CLEAR_PICKED).count();
    h.get_by_label(project_panel::CLEAR_PICKED).click();
    h.run_steps(2);
    assert!(h.state().panel.picked.is_empty(), "{n} {:?}", h.get_by_label(project_panel::CLEAR_PICKED).rect());
    h.get_by_label(left::BOOK_LIST).click();
    h.run_steps(2);
    assert!(!h.state().panel.thumbnails);
}

#[test]
fn devices_groups_by_location_with_not_on_a_page_yet_first() {
    let mut h = harness();
    crate::project_panel::ok(&mut h, r#"place {"symbol":"coil","at":[50,50],"tag":"-K1"}"#);
    h.get_by_label(&tool_strip::panel_label(LeftTab::Devices)).click();
    h.run_steps(4);
    for g in devices_panel::Grouping::ALL {
        assert!(h.query_by_label(&format!("Group by {}", g.label())).is_some(), "{g:?}");
    }
    assert_eq!(h.state().panel.devices.grouping, devices_panel::Grouping::Location);
    assert!(h.query_all_by_label_contains("Device -K").count() > 0, "the device is listed");
    h.get_by_label("Group by Type").click();
    h.run_steps(3);
    assert_eq!(h.state().panel.devices.grouping, devices_panel::Grouping::Type);
    assert!(h.query_by_label_contains("Group -K").is_some(), "grouped by its tag's letter");
}

#[test]
fn history_shows_the_revision_branches_changes_and_versions() {
    let mut h = harness();
    crate::project_panel::ok(&mut h, r#"wire {"points":[[0,150],[30,150]]}"#);
    h.get_by_label(&tool_strip::panel_label(LeftTab::History)).click();
    h.run_steps(4);
    for s in ["Section Branches", "Section Changes", "Section Versions"] {
        assert!(h.query_by_label(s).is_some(), "{s}");
    }
    assert!(h.query_by_label("Branch main").is_some());
    assert!(h.query_all_by_label_contains("Step ").count() > 0, "the changes are listed");
    assert!(h.query_by_label(history_panel::NAME_THIS).is_some());
    // Folding a section hides its rows.
    h.get_by_label("Section Changes").click();
    h.run_steps(3);
    assert_eq!(h.query_all_by_label_contains("Step ").count(), 0);
}

#[test]
fn hovering_a_page_in_the_well_shows_its_preview() {
    let mut h = harness();
    let order = book(&mut h);
    h.state_mut().show_page(order[1]).unwrap();
    h.run_steps(4);
    let at = h.get_by_label("Page 3").rect().center();
    h.event(egui::Event::PointerMoved(at));
    h.run_steps(3);
    let ctx = h.ctx.clone();
    let shown = ctx.memory(|m| m.areas().is_visible(&egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("ecad.well.preview"))));
    assert!(shown, "the preview shows above the bar");
}

/// Boards 1.02, 1.03, 1.04 and 1.08 at 1920 × 1080 (written only when `ECAD_TESTER_SHOTS` names a
/// folder and a wgpu adapter exists; nothing is asserted about the pictures).
#[test]
fn screenshots_of_boards_1_02_to_1_08() {
    let Some(out) = crate::project_panel::shots() else { return };
    ecad_trial::canvas::register_sheet_faces();
    let dir = std::env::temp_dir().join(format!("ecad-uiv3a-shots-{}", std::process::id()));
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
    let order = book(&mut h);
    h.state_mut().show_page(order[1]).unwrap();
    h.run_steps(6);
    let shot = |h: &mut Harness<'static, Trial>, name: &str| {
        h.state_mut().fit_sheet();
        h.run_steps(4);
        let _ = crate::mouse_tester::shot(h, &deltas, &out.join(name));
    };
    // 1.02: Book pinned, two pages ticked.
    h.state_mut().set_left_tab(LeftTab::Pages);
    h.state_mut().panel.picked.extend([order[3], order[4]]);
    h.run_steps(3);
    shot(&mut h, "uiv3-A-1.02-ours.png");
    h.state_mut().panel.picked.clear();
    // 1.03: Devices floating.
    h.state_mut().set_left_tab(LeftTab::Devices);
    h.run_steps(4);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("uiv3-A-1.03-ours.png"));
    // 1.04: History pinned.
    h.state_mut().set_left_tab(LeftTab::History);
    h.state_mut().toggle_pin();
    h.run_steps(4);
    shot(&mut h, "uiv3-A-1.04-ours.png");
    // 1.08: the well's hover preview, another group opened, Book closed.
    h.state_mut().shell.state.left.collapsed = true;
    h.state_mut().shell.state.left.pinned = Some(LeftTab::Pages);
    h.run_steps(3);
    if h.query_by_label("Group PLC").is_some() {
        h.get_by_label("Group PLC").click();
        h.run_steps(3);
    }
    h.state_mut().fit_sheet();
    h.run_steps(3);
    let at = h.get_by_label("Page 5").rect().center();
    h.event(egui::Event::PointerMoved(at));
    h.run_steps(4);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("uiv3-A-1.08-ours.png"));
    let _ = std::fs::remove_dir_all(&dir);
}
