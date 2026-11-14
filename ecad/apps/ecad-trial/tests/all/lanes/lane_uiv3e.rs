//! UI rev 3, section E: the Project workspace (boards 2.01–2.06). The tabs in the tool strip's
//! order with their counts, each a full-width table under one head and over one foot; the right
//! panel shows the selected row; the terminal plan's bands and strips; the PLC cards at the
//! board's scale; Wires grouped by potential; Cables with the core table; Reports' list and its
//! preview. Screenshots `uiv3-E-2.0N-ours.png` when `ECAD_TESTER_SHOTS` names a folder.

use ecad_trial::Trial;
use ecad_trial::lists::workspace;
use ecad_trial::shell::state::{RightTab, Workspace};
use ecad_trial::shell::{ListsView, Slots};
use ecad_ui::table::TableSource;
use eframe::egui::{self, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::mouse_tester::{Collect, Deltas};
use crate::project_panel::{doc, ok, rev3_trial as trial};

type H = Harness<'static, Trial>;

/// The whole window as the app's root `Ui` (the harness keeps a margin round its own).
fn harness(deltas: Deltas) -> H {
    let mut h = Harness::builder().with_size(vec2(1920.0, 1080.0)).with_step_dt(1.0 / 60.0).renderer(Collect(deltas)).build_ui_state(
        |ui, t: &mut Trial| {
            let r = ui.ctx().content_rect();
            ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
                ui.set_clip_rect(r);
                t.logic(&ui.ctx().clone());
                t.ui(ui);
            });
        },
        trial(),
    );
    h.run_steps(5);
    h
}

/// The demo project, plus a PLC rack on a page of its own (051's fixture P51 in short).
fn project(h: &mut H, dir: &std::path::Path) {
    let path = crate::checks_ui::demo_file(dir);
    h.state_mut().close_project();
    h.run_steps(2);
    h.state_mut().open_project(&path).unwrap();
    h.run_steps(3);
    ok(h, r#"newpage {"title":"PLC"}"#);
    let pages = doc(h).page_order.clone();
    if let Some(p) = pages.last() {
        h.state_mut().show_page(*p).unwrap();
        h.run_steps(2);
    }
    ok(h, r#"place {"symbol":"plccpu","at":[30,250]}"#);
    for (i, key) in ["plcdi", "plcdq", "plcai", "plcao", "plcrtd"].iter().enumerate() {
        ok(h, &format!(r#"place {{"symbol":"{key}","at":[{},250]}}"#, 70 + 40 * i));
    }
    for i in 0..6 {
        let x = 60 + 20 * i;
        ok(h, &format!(r#"place {{"symbol":"lamp","at":[{x},160]}}"#));
        ok(h, &format!(r#"wire {{"points":[[{x},150],[{x},60]]}}"#));
    }
    ok(h, r#"plcchannels {"card":"-A02","from":[55,100],"to":[185,100]}"#);
    for (ch, text) in [(0, "Pump 1 run"), (1, "Pump 2 run"), (2, "Mixer run")] {
        ok(h, &format!(r#"plcchannel {{"card":"-A03","channel":{ch},"text":"{text}"}}"#));
    }
}

fn show(h: &mut H, v: ListsView) {
    h.state_mut().set_workspace(Workspace::Lists);
    h.state_mut().show_project_tab(v);
    h.run_steps(6);
}

fn slots(h: &H) -> Slots {
    h.state().shell.slots.expect("laid out")
}

#[test]
fn the_tabs_are_in_the_strips_order_with_one_head_and_foot() {
    assert_eq!(workspace::tab_labels(), ["Devices", "Terminals", "PLC", "Wires", "Cables", "Reports"]);
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = harness(deltas);
    let dir = std::env::temp_dir().join(format!("ecad-uiv3e-tabs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    project(&mut h, &dir);
    show(&mut h, ListsView::Devices);
    let main = slots(&h).main;
    // Each tab is a button in the head, and clicking it shows its view.
    for v in workspace::TABS {
        let tab = h.get_by_label(v.label());
        assert!(main.contains(tab.rect().center()), "{} in the head", v.label());
        tab.click();
        h.run_steps(4);
        assert_eq!(h.state().shell.lists, v);
    }
    // The head's actions: Export and Open beside the page on a table tab; Import greyed.
    show(&mut h, ListsView::Cables);
    assert!(h.query_by_label(workspace::EXPORT).is_some());
    assert!(h.query_by_label(workspace::OPEN_BESIDE).is_some());
    assert!(h.query_by_label(workspace::IMPORT).is_some());
    h.get_by_label(workspace::OPEN_BESIDE).click();
    h.run_steps(4);
    assert!(h.state().shell.state.companion.is_some(), "the page opens beside the table");
    assert!(slots(&h).companion.is_some());
    h.get_by_label(workspace::CLOSE_BESIDE).click();
    h.run_steps(4);
    assert!(h.state().shell.state.companion.is_none());
    // The counts come from the models: the demo's wires and cables.
    let r = h.state().reports.as_ref().unwrap();
    assert!(r.tab_counts[2] > 0 && r.tab_counts[3] > 0, "{:?}", r.tab_counts);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_selected_row_opens_the_right_panel_on_it() {
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = harness(deltas);
    let dir = std::env::temp_dir().join(format!("ecad-uiv3e-right-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    project(&mut h, &dir);
    show(&mut h, ListsView::Wires);
    assert!(h.state().shell.state.right_collapsed, "closed until a row is picked");
    let k = h.state().reports.as_ref().unwrap().wires.source.keys()[0];
    let number = h.state().reports.as_ref().unwrap().wires.source.rows.text(k, "Wire number");
    h.state_mut().reports.as_mut().unwrap().wires.table.selection.set([k]);
    h.run_steps(4);
    assert!(!h.state().shell.state.right_collapsed, "the panel opened");
    assert_eq!(h.state().shell.state.right_tab, RightTab::Device);
    assert!(h.query_by_label_contains(&format!("Number: {number}")).is_some(), "the wire's number in the panel");
    let ferrules = format!("Ferrules: {}", ecad_trial::lists::report::DASH);
    assert!(h.query_by_label(&ferrules).is_some(), "its ends, greyed until the cable manager");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wires_group_by_potential_and_the_terminal_plan_by_strip() {
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = harness(deltas);
    let dir = std::env::temp_dir().join(format!("ecad-uiv3e-group-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    project(&mut h, &dir);
    show(&mut h, ListsView::Wires);
    let w = &h.state().reports.as_ref().unwrap().wires;
    assert_eq!(w.table.view.group_by(), Some(ecad_trial::lists::wires::POTENTIAL));
    assert!(w.table.view.rows().iter().any(|r| matches!(r, ecad_ui::table::Row::Group { .. })), "potential groups");
    show(&mut h, ListsView::Terminals);
    let t = &h.state().reports.as_ref().unwrap().terminals;
    assert_eq!(t.table.view.group_by(), Some(ecad_trial::lists::terminals::STRIP));
    assert_eq!(t.table.bands.len(), 3, "inside | the strip | outside");
    assert!(!ecad_trial::lists::terminals::columns().iter().any(|c| c.id == ecad_trial::lists::terminals::STRIP), "the strip is a group");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_plc_cards_are_at_the_boards_scale() {
    use ecad_ui::table::strip;
    assert_eq!((strip::TILE_WIDTH, strip::MARK, strip::MARK_GAP), (196.0, 13.0, 4.0));
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = harness(deltas);
    let dir = std::env::temp_dir().join(format!("ecad-uiv3e-plc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    project(&mut h, &dir);
    show(&mut h, ListsView::Plc);
    let tile = h.get_by_label("ecad.plc.tile.1").rect();
    assert_eq!(tile.width(), strip::TILE_WIDTH);
    assert!(tile.height() >= strip::TILE_MIN_HEIGHT, "{tile:?}");
    let v = &h.state().reports.as_ref().unwrap().io;
    let card = &v.strip[1];
    assert!(!card.kind.is_empty() && !card.under.is_empty(), "type at the head's right, addresses under the squares: {card:?}");
    assert!(card.links.iter().flatten().any(|l| l.text.ends_with("in use")), "{:?}", card.links);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Boards 2.01–2.06 at 1920 × 1080 for the side-by-side (written only when `ECAD_TESTER_SHOTS`
/// names a folder and a wgpu adapter exists; nothing is asserted about the pictures).
#[test]
fn screenshots_of_boards_2_01_to_2_06() {
    let Some(out) = crate::project_panel::shots() else { return };
    ecad_trial::canvas::register_sheet_faces();
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = harness(deltas.clone());
    let dir = std::env::temp_dir().join(format!("ecad-uiv3e-shots-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    project(&mut h, &dir);
    let shot = |h: &H, name: &str| {
        let _ = crate::mouse_tester::shot(h, &deltas, &out.join(name));
    };
    // 2.01: a device with a part.
    show(&mut h, ListsView::Devices);
    let c = doc(&mut h).components_tagged("-K01").first().copied();
    if let Some(c) = c {
        h.state_mut().panel.devices.selected = Some(c);
        h.state_mut().panel.devices.table.selection.set([c]);
    }
    h.run_steps(8);
    shot(&h, "uiv3-E-2.01-ours.png");
    // 2.02: a terminal.
    show(&mut h, ListsView::Terminals);
    let k = h.state().reports.as_ref().and_then(|r| r.terminals.table.view.visible_keys().get(1).copied());
    if let Some(k) = k {
        h.state_mut().reports.as_mut().unwrap().terminals.table.selection.set([k]);
    }
    h.run_steps(8);
    shot(&h, "uiv3-E-2.02-ours.png");
    // 2.03: the output card.
    show(&mut h, ListsView::Plc);
    if let Some(n) = h.query_by_label("ecad.plc.tile.2") {
        n.click();
    }
    h.run_steps(8);
    shot(&h, "uiv3-E-2.03-ours.png");
    // 2.04: a wire.
    show(&mut h, ListsView::Wires);
    let k = h.state().reports.as_ref().and_then(|r| r.wires.table.view.visible_keys().get(1).copied());
    if let Some(k) = k {
        h.state_mut().reports.as_mut().unwrap().wires.table.selection.set([k]);
    }
    h.run_steps(8);
    shot(&h, "uiv3-E-2.04-ours.png");
    // 2.05: the first cable (selected on first show).
    show(&mut h, ListsView::Cables);
    h.run_steps(8);
    shot(&h, "uiv3-E-2.05-ours.png");
    // 2.06: the parts list.
    show(&mut h, ListsView::Reports);
    h.run_steps(10);
    shot(&h, "uiv3-E-2.06-ours.png");
    let _ = std::fs::remove_dir_all(&dir);
}
