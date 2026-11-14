//! Brief 038 (U2): the shell. Layout sizes (design §5.2), the shell state and page views kept in
//! eframe storage (§5.4), the shell flows at 1920 × 1080 (workspaces, left tabs, the Text panel,
//! the developer switch, keys only over the page), nothing lost from the old panels, and the
//! update maps (a dot on a page changed elsewhere, no rebuild for it).

use std::collections::BTreeSet;

use crate::project_panel::{active, click, doc, head, ok, trial};
use ecad_trial::shell::state::{Companion, LeftTab, PageView, ShellState, Views, Workspace};
use ecad_trial::shell::{layout, layout::Slots, tool_strip, top_bar};
use ecad_trial::{Trial, developer, project_panel, tools};
use eframe::egui::{Event, Key, Modifiers, Rect, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn window(w: f32, h: f32) -> Rect {
    Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h))
}

fn at(state: &ShellState, w: f32, h: f32, agent: bool) -> Slots {
    layout::layout(window(w, h), state, agent)
}

// The region sizes and the narrow-window collapses are UI rev 3's now: `lane_uiv3`.

#[test]
fn the_companion_keeps_360_px_a_half_or_closes() {
    let s = ShellState { companion: Some(Companion { ratio: 0.9 }), ..ShellState::default() };
    let w = at(&s, 1920.0, 1080.0, false);
    let (main, comp) = (w.main, w.companion.expect("room for two"));
    assert!(main.width() >= layout::HALF_MIN && comp.width() >= layout::HALF_MIN);
    assert_eq!(main.width() + layout::SPLIT_HANDLE + comp.width(), 1920.0 - 56.0 - 56.0, "UI rev 3: strip and rail");
    // 1366: a 0.9 split is held at 360 px for the companion.
    let n = at(&s, 1366.0, 768.0, false);
    assert_eq!(n.companion.map(|r| r.width()), Some(layout::HALF_MIN));
    // Below the 900 px window minimum, the main view cannot hold two halves.
    let none = at(&s, 800.0, 560.0, false);
    assert!(none.companion.is_none() && none.handle.is_none(), "too narrow for two halves: no split");
}

#[test]
fn the_shell_state_round_trips_and_a_corrupt_value_starts_with_defaults() {
    let mut s = ShellState { workspace: Workspace::Publish, right_collapsed: false, cad_chrome: true, ..ShellState::default() };
    s.left.tab = LeftTab::Devices;
    s.left.width_per_tab[1] = 333.0;
    let (back, line) = ShellState::from_stored(Some(&s.to_stored()));
    assert_eq!((back, line), (s, None));
    assert_eq!(ShellState::from_stored(None), (ShellState::default(), None), "nothing stored: no line");
    for bad in ["{not json", "[1,2]", r#"{"workspace":"Nowhere"}"#] {
        let (state, line) = ShellState::from_stored(Some(bad));
        assert_eq!(state, ShellState::default(), "{bad}");
        assert!(line.is_some_and(|l| l.contains("starting with the default")), "{bad}");
    }
    // The line reaches the command history once.
    let mut t = trial();
    t.shell.load(Some("{oops"), None);
    let mut h = Harness::builder().with_size(vec2(1920.0, 1080.0)).build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        t,
    );
    h.run_steps(3);
    let lines = h.state().cad.session.log.iter().filter(|l| l.contains("The saved shell layout could not be read")).count();
    assert_eq!(lines, 1);
}

#[test]
fn page_views_are_kept_for_32_projects() {
    let mut v = Views::default();
    for i in 0..40 {
        let p = v.open(&format!("p{i:02}"));
        p.pages.insert("page".into(), PageView { cx: 1.0, cy: 2.0, height: 3.0 });
    }
    assert_eq!(v.projects.len(), 32);
    assert!(!v.projects.contains_key("p00") && v.projects.contains_key("p39"), "the least recently opened go");
    v.open("p10");
    v.open("p40");
    assert!(v.projects.contains_key("p10"), "opened again: kept");
    let (back, line) = Views::from_stored(Some(&v.to_stored()));
    assert_eq!((back, line), (v, None));
    let (bad, line) = Views::from_stored(Some("{\"projects\": 5}"));
    assert!(bad.projects.is_empty() && line.is_some());
}

fn shell_harness() -> Harness<'static, Trial> {
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

fn keys(h: &mut Harness<'static, Trial>, m: Modifiers, k: Key) {
    h.key_press_modifiers(m, k);
    h.run_steps(3);
}

/// The pointer over the middle of the main view.
fn over_page(h: &mut Harness<'static, Trial>) {
    let r = h.state().shell.slots.expect("laid out").main;
    h.event(Event::PointerMoved(r.center()));
    h.run_steps(2);
}

#[test]
fn workspaces_left_tabs_and_the_text_panel() {
    let mut h = shell_harness();
    assert_eq!(h.state().shell_state().workspace, Workspace::Draw);
    assert!(h.query_by_label(tool_strip::GROUPS[1].label).is_some(), "the tool strip in Draw");
    for (i, k) in [Key::Num2, Key::Num3, Key::Num4, Key::Num1].into_iter().enumerate() {
        keys(&mut h, Modifiers::COMMAND, k);
        let w = Workspace::ALL[(i + 1) % 4];
        assert_eq!(h.state().shell_state().workspace, w);
        assert_eq!(h.query_by_label(tool_strip::GROUPS[1].label).is_some(), w == Workspace::Draw, "{w:?}");
    }
    // Lists hosts Reports; Library its browser (brief 048); Publish Release and Print Book.
    keys(&mut h, Modifiers::COMMAND, Key::Num2);
    assert!(h.query_by_label(ecad_trial::devices_panel::FILTER).is_some(), "Lists ▸ Device list");
    keys(&mut h, Modifiers::COMMAND, Key::Num3);
    assert!(h.query_by_label(ecad_trial::library::browser::NEW).is_some(), "Library ▸ the browser");
    keys(&mut h, Modifiers::COMMAND, Key::Num4);
    assert!(h.state().print_book.is_some());
    keys(&mut h, Modifiers::COMMAND, Key::Num1);

    // Each left tab shows today's content.
    assert!(h.query_by_label("Page 1: 1").is_some(), "Pages");
    click(&mut h, project_panel::DEVICES_TAB);
    assert!(h.query_by_label(ecad_trial::devices_panel::FILTER).is_some(), "Devices");
    click(&mut h, ecad_trial::history_panel::HISTORY_TAB);
    assert_eq!(h.state().shell_state().left.tab, LeftTab::History);
    click(&mut h, project_panel::PAGES_TAB);

    // Ctrl+/ opens the Commands tab; a command typed there runs.
    assert!(h.query_by_label(developer::FIELD).is_none(), "closed by default");
    keys(&mut h, Modifiers::COMMAND, Key::Slash);
    assert!(h.query_by_label(developer::FIELD).is_some());
    let before = head(&mut h);
    h.get_by_label(developer::FIELD).click();
    h.event(Event::Text(r#"wire {"points":[[0,0],[40,0]]}"#.into()));
    h.run_steps(1);
    click(&mut h, "Run");
    assert_eq!(head(&mut h), before + 1, "one transaction from the Commands tab");
    keys(&mut h, Modifiers::COMMAND, Key::Slash);
    assert!(h.query_by_label(developer::FIELD).is_none(), "Ctrl+/ closes it");
}

#[test]
fn the_developer_switch_shows_cadcrafts_interface_and_idle_letters_start_nothing() {
    let mut h = shell_harness();
    assert!(h.query_by_label("Drafting").is_none(), "CADCraft's Tool Sets are hidden");
    // L + Enter over the page does not start LINE.
    over_page(&mut h);
    h.event(Event::Text("L".into()));
    h.run_steps(1);
    keys(&mut h, Modifiers::NONE, Key::Enter);
    assert!(h.state().cad.session.running.is_none(), "idle typing is the shell's");
    // The switch, as `--cad-ui` sets it, and as `ui.cadchrome` typed in the Commands tab.
    h.state_mut().set_cad_chrome(true);
    h.run_steps(3);
    assert!(h.query_by_label("Drafting").is_some(), "CADCraft's Tool Sets in the main view");
    keys(&mut h, Modifiers::COMMAND, Key::Slash);
    h.state_mut().command = "ui.cadchrome off".into();
    h.run_steps(1);
    click(&mut h, "Run");
    assert!(!h.state().shell_state().cad_chrome && h.query_by_label("Drafting").is_none());
    assert!(ecad_trial::parse_args(["--cad-ui".into()]).is_ok_and(|i| matches!(i, ecad_trial::Invocation::Run { cad_ui: true, .. })));
}

#[test]
fn group_keys_start_tools_over_the_page_but_not_while_a_field_has_focus() {
    let mut h = shell_harness();
    over_page(&mut h);
    keys(&mut h, Modifiers::NONE, Key::W);
    assert_eq!(h.state().cad.session.running.as_ref().map(|r| r.id.clone()).as_deref(), Some("wire"), "W: Wire");
    h.state_mut().cad.session.cancel();
    h.run_steps(2);
    // Brief 041: Draw's tools step (Wire is alone in its group until Branch and Jump exist).
    keys(&mut h, Modifiers::NONE, Key::D);
    assert_eq!(h.state().cad.session.running.as_ref().map(|r| r.id.clone()).as_deref(), Some("line"), "D: Line");
    // Pressed again while Line runs: the next tool of the group.
    keys(&mut h, Modifiers::NONE, Key::D);
    assert_eq!(h.state().cad.session.running.as_ref().map(|r| r.id.clone()).as_deref(), Some("pline"), "D again steps");
    // Later, with nothing running: the group's last-used tool.
    h.state_mut().cad.session.cancel();
    h.run_steps(2);
    keys(&mut h, Modifiers::NONE, Key::D);
    assert_eq!(h.state().cad.session.running.as_ref().map(|r| r.id.clone()).as_deref(), Some("pline"), "the last used");
    h.state_mut().cad.session.cancel();
    h.run_steps(2);
    // The search field has the keyboard: the letters are typed, no tool starts.
    h.get_by_label(top_bar::SEARCH).click();
    h.run_steps(2);
    for (letter, k) in tool_strip::GROUPS.iter().filter_map(|g| g.key) {
        h.event(Event::Text(letter.to_string()));
        h.key_press(k);
        h.run_steps(1);
    }
    h.run_steps(2);
    assert!(h.state().cad.session.running.is_none(), "no tool while typing");
    assert_eq!(h.state().search.query, "VDTWC");
}

#[test]
fn nothing_from_the_old_panels_is_lost() {
    // Brief 041 replaced "every ECAD Tool Sets button is in one flyout": every button's command
    // is reachable from an action (lane_041); the developer ones are in the Text panel.
    for t in tools::TOOLS.iter().filter(|t| t.group != tools::DEV_GROUP) {
        assert!(tools::covered(t), "{} ({})", t.label, t.id);
    }
    let mut h = shell_harness();
    // The Project panel header: file actions and popouts in the project menu.
    click(&mut h, top_bar::PROJECT_MENU);
    for label in [
        top_bar::NEW,
        top_bar::OPEN,
        top_bar::SAVE,
        top_bar::SAVE_AS,
        top_bar::CLOSE,
        project_panel::PROJECT_INFO,
        project_panel::PROJECT_SETTINGS,
        project_panel::REVISIONS,
        ecad_trial::shell::SETTINGS,
    ] {
        assert!(h.query_by_label(label).is_some(), "project menu: {label}");
    }
    h.key_press(Key::Escape);
    h.run_steps(2);
    // The page tools, the panels and the inspector's tabs (UI rev 3: the strip opens the panels;
    // Checks, the drawer before, is the inspector's tab).
    // UI rev 3: Reports and Print Book are the Project and Publish workspaces.
    for label in [project_panel::NEW_PAGE, project_panel::NEW_FOLDER, "ecad.project.newpage.kind"] {
        assert!(h.query_by_label(label).is_some(), "Pages: {label}");
    }
    for tab in [LeftTab::Pages, LeftTab::Devices, LeftTab::History] {
        let label = tool_strip::panel_label(tab);
        assert!(h.query_by_label(&label).is_some(), "panel: {label}");
    }
    let checks = ecad_trial::shell::right::rail_label(ecad_trial::shell::RightTab::Checks);
    assert!(h.query_by_label(&checks).is_some(), "the inspector's Checks tab");
    assert!(h.query_by_label_contains(top_bar::CHECKS_PILL).is_some() && h.query_by_label_contains(top_bar::AGENT_PILL).is_some());
    // The Developer fold: the Text panel's Commands tab.
    keys(&mut h, Modifiers::COMMAND, Key::Slash);
    // Agent access is in Settings since brief 042: the Commands tab no longer has it.
    assert!(h.query_by_label(developer::AGENT_MODE).is_none(), "Agent access is in Settings");
    for label in [developer::FIELD, developer::DEBUGGER, developer::INSPECT, developer::PRIMITIVES] {
        assert!(h.query_by_label(label).is_some(), "Commands: {label}");
    }
    keys(&mut h, Modifiers::COMMAND, Key::Slash);
    // The Properties section: drawn in the right panel.
    let frames = h.state().props.lock().map(|p| p.frames).unwrap_or(0);
    assert!(frames > 0, "the ECAD properties are drawn");
    // Every flyout opens on hover and lists its entries.
    for g in tool_strip::GROUPS {
        h.get_by_label(g.label).hover();
        h.run_steps(2);
        for a in g.tools() {
            assert!(h.query_by_label(a.label).is_some(), "{} lists {}", g.label, a.label);
        }
    }
}

#[test]
fn a_page_changed_elsewhere_gets_a_dot_without_a_rebuild_and_views_are_kept_per_page() {
    let mut h = shell_harness();
    ok(&mut h, r#"newpage {"title":"Two"}"#);
    ok(&mut h, r#"newpage {"title":"Three"}"#);
    let d = doc(&mut h);
    let (p1, p3) = (d.page_order[0], d.page_order[2]);
    // A device drawn on page 3 only.
    h.state_mut().show_page(p3).unwrap();
    h.run_steps(3);
    ok(&mut h, r#"place {"symbol":"coil","at":[100,100]}"#);
    let coil = doc(&mut h).components.keys().next().copied().expect("the coil");
    h.state_mut().show_page(p1).unwrap();
    h.run_steps(3);
    assert_eq!(active(&mut h), Some(p1));
    h.state_mut().panel.changed.clear();
    let (rebuilds, dots) = (h.state().panel.rebuilds, h.state().shell.counters.dots);
    // Its description changed by command while page 1 is shown: page 3's tag text changes.
    ok(&mut h, &format!(r#"setprop {{"component":"{}","key":"description","value":"Pump"}}"#, coil.hex()));
    h.run_steps(2);
    assert_eq!(active(&mut h), Some(p1), "page 1 stays shown");
    assert_eq!(h.state().panel.changed, BTreeSet::from([p3]), "a dot on page 3");
    assert_eq!(h.state().panel.rebuilds, rebuilds, "the Pages rows are not rebuilt");
    assert!(h.state().shell.counters.dots > dots);
    // Nothing changes: the dots are not worked out again.
    let dots = h.state().shell.counters.dots;
    h.run_steps(5);
    assert_eq!(h.state().shell.counters.dots, dots, "no work while nothing changes");
    // Showing page 3 clears its dot.
    h.state_mut().show_page(p3).unwrap();
    h.run_steps(3);
    assert!(h.state().panel.changed.is_empty());

    // Per-page zoom: page 1's view comes back when it is shown again, and is in the stored views.
    h.state_mut().show_page(p1).unwrap();
    h.run_steps(3);
    let mut v = h.state().cad.session.state().unwrap().view();
    v.height *= 0.25;
    h.state_mut().cad.session.state_mut().unwrap().set_view(v);
    h.run_steps(2);
    h.state_mut().show_page(p3).unwrap();
    h.run_steps(3);
    h.state_mut().show_page(p1).unwrap();
    h.run_steps(3);
    let back = h.state().cad.session.state().unwrap().view();
    assert!((back.height - v.height).abs() < 1e-9 && back.center == v.center, "{back:?} vs {v:?}");
    let (_, views) = h.state_mut().shell.to_save();
    let (restored, line) = Views::from_stored(views.as_deref());
    assert!(line.is_none());
    assert!(restored.projects.values().any(|p| p.pages.get(&p1.hex()).is_some_and(|pv| (pv.height - v.height).abs() < 1e-9)));
}

/// The four workspaces at 1920 × 1080 on the demo project, for the owner to compare with the
/// mock-up's screens (written only when `ECAD_TESTER_SHOTS` names a folder and a wgpu adapter
/// exists; nothing is asserted about the pictures).
#[test]
fn screenshots_of_the_four_workspaces() {
    let Some(out) = crate::project_panel::shots() else { return };
    let dir = std::env::temp_dir().join(format!("ecad-038-shots-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = crate::checks_ui::demo_file(&dir);
    let deltas: crate::mouse_tester::Deltas = std::sync::Arc::default();
    let mut h = Harness::builder().with_size(vec2(1920.0, 1080.0)).renderer(crate::mouse_tester::Collect(deltas.clone())).build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        trial(),
    );
    h.run_steps(3);
    h.state_mut().close_project();
    h.run_steps(2);
    h.state_mut().open_project(&path).unwrap();
    h.run_steps(3);
    let first = doc(&mut h).page_order[0];
    h.state_mut().show_page(first).unwrap();
    h.run_steps(3);
    h.state_mut().zoom_to_drawing();
    h.run_steps(5);
    for (w, name) in [(Workspace::Draw, "draw"), (Workspace::Lists, "lists"), (Workspace::Library, "library"), (Workspace::Publish, "publish")] {
        h.state_mut().set_workspace(w);
        h.run_steps(6);
        let _ = crate::mouse_tester::shot(&h, &deltas, &out.join(format!("038-{name}.png")));
    }
    // The flyout, the Text panel and the developer switch.
    h.state_mut().set_workspace(Workspace::Draw);
    h.run_steps(3);
    h.get_by_label(tool_strip::GROUPS[1].label).hover();
    h.run_steps(3);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("038-flyout.png"));
    let centre = h.state().shell.slots.map(|s| s.main.center()).unwrap_or_default();
    h.event(Event::PointerMoved(centre));
    h.run_steps(3);
    h.state_mut().toggle_text_panel();
    h.run_steps(3);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("038-text-panel.png"));
    h.state_mut().toggle_text_panel();
    h.state_mut().set_cad_chrome(true);
    h.run_steps(6);
    let _ = crate::mouse_tester::shot(&h, &deltas, &out.join("038-developer.png"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------- fix round (PR #74 review)

/// Review defect 1: Del erases the selection while no command runs (CADCraft's command line did
/// it, and hears no idle keys in the shell).
#[test]
fn delete_erases_the_selection() {
    let mut h = shell_harness();
    ok(&mut h, "line 0,0 40,0");
    // LINE asks for a next point: end it.
    h.state_mut().cad.session.cancel();
    h.run_steps(2);
    let drawn = |h: &Harness<'static, Trial>| {
        h.state()
            .cad
            .session
            .doc()
            .unwrap()
            .model
            .iter()
            .filter(|e| e.common.layer != ecad_cadcraft::frame::LAYER)
            .map(|e| e.handle)
            .collect::<Vec<_>>()
    };
    let lines = drawn(&h);
    assert_eq!(lines.len(), 1);
    h.state_mut().cad.session.set_selection(lines);
    h.run_steps(2);
    over_page(&mut h);
    keys(&mut h, Modifiers::NONE, Key::Delete);
    assert!(drawn(&h).is_empty(), "Del erased the selected line");
}

/// Review defect 2: a go-to onto a page seen before keeps the go-to's view; a plain page switch
/// gets the page's stored view back.
#[test]
fn a_go_to_keeps_its_view_on_a_page_seen_before() {
    let mut h = shell_harness();
    ok(&mut h, r#"newpage {"title":"Two"}"#);
    let d = doc(&mut h);
    let (p1, p2) = (d.page_order[0], d.page_order[1]);
    h.state_mut().show_page(p2).unwrap();
    h.run_steps(3);
    ok(&mut h, r#"place {"symbol":"coil","at":[100,100]}"#);
    let coil = doc(&mut h).components.keys().next().copied().unwrap();
    let rep = doc(&mut h).representations_of(ecad_model::Subject::Component(coil)).next().unwrap().0;
    // Page 2 seen far from the coil, then page 1.
    let mut far = h.state().cad.session.state().unwrap().view();
    far.center = cadcraft_engine::geom::Vec2::new(5000.0, 5000.0);
    h.state_mut().cad.session.state_mut().unwrap().set_view(far);
    h.run_steps(2);
    h.state_mut().panel_action(project_panel::Action::Open(p1));
    h.run_steps(3);
    assert_eq!(active(&mut h), Some(p1));
    // Go to the coil: page 2, centred on the coil, not where page 2 was left.
    h.state_mut().goto_symbol(rep).unwrap();
    h.run_steps(4);
    assert_eq!(active(&mut h), Some(p2));
    let v = h.state().cad.session.state().unwrap().view();
    assert!((v.center.x - 100.0).abs() < 50.0 && (v.center.y - 100.0).abs() < 50.0, "centred on the coil: {v:?}");
    // A plain switch away and back restores page 2's stored view (now the go-to's).
    h.state_mut().panel_action(project_panel::Action::Open(p1));
    h.run_steps(3);
    h.state_mut().panel_action(project_panel::Action::Open(p2));
    h.run_steps(3);
    let back = h.state().cad.session.state().unwrap().view();
    assert!((back.center.x - v.center.x).abs() < 1e-6 && (back.center.y - v.center.y).abs() < 1e-6, "{back:?} vs {v:?}");
}

/// Review defect 3: ECAD's hover (and the zone) react only over the page shown, not over a
/// hosted view laid over the hidden page.
#[test]
fn hover_tooltips_stay_off_hosted_views() {
    let mut h = shell_harness();
    crate::project_panel::fixture_p(&mut h);
    let supply = doc(&mut h).page_order[0];
    h.state_mut().show_page(supply).unwrap();
    h.run_steps(3);
    let reference_end = |h: &mut Harness<'static, Trial>| {
        let at = crate::mouse_tester::screen(h, 65.0, 5.0);
        crate::mouse_tester::pointer_to(h, at);
        h.run_steps(3);
        h.state().hover_lines().map(<[String]>::to_vec)
    };
    assert!(reference_end(&mut h).is_some(), "Draw: the reference end is described");
    keys(&mut h, Modifiers::COMMAND, Key::Num2);
    h.state_mut().shell.lists = ecad_trial::shell::ListsView::Reports;
    h.run_steps(3);
    assert!(reference_end(&mut h).is_none(), "Lists: no tooltip from the hidden page");
    let at = h.ctx.pointer_hover_pos().unwrap_or_default();
    let ctx = h.ctx.clone();
    assert!(h.state_mut().zone_under_cursor(&ctx).is_none(), "no zone from the hidden page at {at:?}");
}

/// The fix round's seam: Lists shows U3's Device list through `lists::show`, at the main view's
/// width; the left panel's Devices tab defers to it meanwhile.
#[test]
fn lists_shows_the_device_list_at_the_main_views_width() {
    let mut h = shell_harness();
    click(&mut h, project_panel::DEVICES_TAB);
    keys(&mut h, Modifiers::COMMAND, Key::Num2);
    assert_eq!(h.state().shell.lists, ecad_trial::shell::ListsView::Devices);
    let main = h.state().shell.slots.unwrap().main;
    let filters: Vec<Rect> = h.query_all_by_label(ecad_trial::devices_panel::FILTER).map(|n| n.rect()).collect();
    assert_eq!(filters.len(), 1, "one Devices view");
    assert!(filters.iter().all(|r| main.contains(r.center())), "in the main view");
    assert!(h.state().shell.slots.unwrap().panel.is_none(), "UI rev 3: Project has no panel slot");
    assert!(h.state().reports.as_ref().is_some_and(|r| r.hidden), "Reports is not drawn over it");
    h.get_by_label(ecad_trial::shell::LISTS_TERMINALS).click();
    h.run_steps(3);
    assert_eq!(h.state().reports.as_ref().map(|r| r.tab), Some(ecad_trial::outputs_ui::Tab::Terminals));
}
