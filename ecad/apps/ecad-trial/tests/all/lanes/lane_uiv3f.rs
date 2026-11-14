//! UI rev 3, section F (boards 3.01–3.04, 4.01, 5.01, 5.02): the Library, Publish and Settings
//! inside the shared bars, and the screenshots for the side-by-side with the boards (written only
//! when `ECAD_TESTER_SHOTS` names a folder; nothing is asserted about the pictures).

use ecad_cadcraft::symbols;
use ecad_model::Subject;
use ecad_trial::Trial;
use ecad_trial::library::browser;
use ecad_trial::library::draft::Picked;
use ecad_trial::shell::Workspace;
use eframe::egui::vec2;
use egui_kittest::Harness;
use serde_json::{Value, json};

use crate::project_panel::{doc, rev3_trial};

type H = Harness<'static, Trial>;

/// The whole window as the app's root `Ui` (the harness keeps an 8 px margin round its own).
fn window(deltas: Option<crate::mouse_tester::Deltas>) -> H {
    let mut b = Harness::builder().with_size(vec2(1920.0, 1080.0)).with_step_dt(1.0 / 60.0);
    if let Some(d) = deltas {
        b = b.renderer(crate::mouse_tester::Collect(d));
    }
    let mut h = b.build_ui_state(
        |ui, t: &mut Trial| {
            let r = ui.ctx().content_rect();
            ui.scope_builder(eframe::egui::UiBuilder::new().max_rect(r), |ui| {
                ui.set_clip_rect(r);
                t.logic(&ui.ctx().clone());
                t.ui(ui);
            });
        },
        rev3_trial(),
    );
    h.run_steps(5);
    h
}

fn click(h: &mut H, label: &str) {
    use egui_kittest::kittest::Queryable;
    h.get_by_label(label).click();
    h.run_steps(3);
}

/// Settings (boards 5.01, 5.02) sits between the two bars, lists its sections in the board's
/// order, shows the project pill as active and no workspace, and a workspace closes it.
#[test]
fn settings_sit_between_the_bars_and_a_workspace_leaves_them() {
    use ecad_trial::settings::Section;
    use egui_kittest::kittest::{NodeT, Queryable};
    let mut h = window(None);
    h.state_mut().open_settings(Section::Project);
    h.run_steps(3);
    let labels: Vec<&str> = Section::ALL.iter().map(|s| s.label()).collect();
    assert_eq!(
        labels,
        ["Project info", "Standards", "Pages and sheets", "Libraries", "Agent access", "Versions and backups", "Appearance", "Shortcuts"]
    );
    for s in Section::ALL {
        assert!(h.query_by_label(s.name()).is_some(), "{}", s.name());
    }
    let slots = h.state().shell.slots.expect("laid out");
    let title = h.get_by_label(ecad_trial::settings::TITLE).rect();
    assert!(title.top() > slots.top_bar.bottom() && title.bottom() < slots.status.top(), "between the bars: {title:?}");
    assert!(h.query_by_label(ecad_trial::shell::top_bar::PROJECT_MENU).is_some(), "the top bar is still there");
    let draw = h.get_by_label("Draw").accesskit_node().is_selected();
    assert_ne!(draw, Some(true), "no workspace is lit while Settings is open");
    // Pages and sheets keeps 031's text styles on its second tab.
    h.state_mut().open_settings(Section::TextStyles);
    h.run_steps(2);
    assert_eq!(h.state().settings.section.item(), Section::Pages);
    // A workspace chosen in the top bar leaves Settings.
    click(&mut h, "Library");
    h.run_steps(2);
    assert!(!h.state().shell.settings_open, "Library closes Settings");
    assert_eq!(h.state().shell.state.workspace, Workspace::Library);
}

/// Library (board 3.01): five tabs, only Symbols with data; the Source filter; no inspector rail.
#[test]
fn the_library_has_its_tabs_its_source_filter_and_no_rail() {
    use egui_kittest::kittest::Queryable;
    let mut h = window(None);
    h.state_mut().set_workspace(Workspace::Library);
    h.run_steps(3);
    for t in browser::LibraryTab::ALL {
        assert!(h.query_by_label(&t.name()).is_some(), "{}", t.name());
    }
    let slots = h.state().shell.slots.expect("laid out");
    assert_eq!(slots.rail.width(), 0.0, "no inspector rail in the Library");
    assert_eq!(slots.right.width(), ecad_trial::shell::layout::RIGHT_W_WIDE);
    let builtin = h.state().library.listing.builtin_count();
    assert_eq!(h.state().library.listing.shown(&h.state().library.browser).len(), builtin, "no project symbols yet");
    h.state_mut().library.browser.source = browser::SourceFilter::Project;
    h.run_steps(2);
    assert!(h.state().library.listing.shown(&h.state().library.browser).is_empty(), "Source: Project shows none");
    h.state_mut().library.browser.source = browser::SourceFilter::All;
    click(&mut h, &browser::LibraryTab::Parts.name());
    assert_eq!(h.state().library.browser.tab, browser::LibraryTab::Parts);
    assert!(h.query_by_label(browser::NEW).is_some(), "New symbol stays on every tab");
}

/// Publish (board 4.01): its own right column, no rail; unticking a page leaves it out of Print
/// Book's pages and the count follows.
#[test]
fn publish_ticks_choose_the_pages_print_book_prints() {
    use egui_kittest::kittest::Queryable;
    let dir = std::env::temp_dir().join(format!("ecad-uiv3f-publish-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = window(None);
    demo(&mut h, &dir);
    h.state_mut().set_workspace(Workspace::Publish);
    h.run_steps(4);
    let slots = h.state().shell.slots.expect("laid out");
    assert_eq!(slots.rail.width(), 0.0, "no inspector rail in Publish");
    assert_eq!(slots.right.width(), ecad_trial::shell::layout::RIGHT_W_PUBLISH);
    assert!(h.query_by_label(ecad_trial::publish::SET_FULL).is_some());
    assert!(h.state().release.is_none(), "Release opens only from its button");
    let all = h.state().print_book.as_ref().map(|d| d.sheets).unwrap_or(0);
    assert!(all > 1, "{all} sheets");
    let first = doc(&mut h).page_order[0];
    click(&mut h, &format!("{}.{}.include", ecad_trial::publish::PAGE, first.hex()));
    h.run_steps(2);
    let d = h.state().print_book.clone().expect("Print Book's choices");
    assert_eq!(d.scope, ecad_trial::outputs_ui::ScopeKind::Pages);
    assert!(d.sheets < all, "{} of {all}", d.sheets);
    click(&mut h, ecad_trial::publish::SELECT_ALL);
    assert_eq!(h.state().print_book.as_ref().map(|d| d.scope), Some(ecad_trial::outputs_ui::ScopeKind::All));
    let _ = std::fs::remove_dir_all(&dir);
}

fn run(h: &mut H, cmd: &str, params: Value) -> Value {
    let v = h.state_mut().cad.run(cmd, params).unwrap_or_else(|e| panic!("{cmd}: {e}"));
    h.run_steps(2);
    v
}

/// The demo project open on its second page.
fn demo(h: &mut H, dir: &std::path::Path) {
    let path = crate::checks_ui::demo_file(dir);
    h.state_mut().close_project();
    h.run_steps(2);
    h.state_mut().open_project(&path).unwrap();
    h.run_steps(3);
    let order = doc(h).page_order;
    if let Some(&p) = order.get(1).or(order.first()) {
        h.state_mut().show_page(p).unwrap();
    }
    h.run_steps(4);
}

/// A float switch placed three times, the first wired on point 2.
fn three_copies(h: &mut H) -> Vec<ecad_model::RepresentationId> {
    run(
        h,
        "symbolnew",
        json!({
            "name": "Float switch", "key": "float1",
            "graphics": [{"kind": "polyline", "vertices": [[-2.5, -7.5], [2.5, -7.5], [2.5, 7.5], [-2.5, 7.5]], "closed": true}],
            "points": [{"key": "1", "label": "BN", "at": [0, 10]}, {"key": "2", "label": "BU", "at": [0, -10]}],
            "slots": [{"text": "tag", "at": [5, 0]}],
            "defaults": {"prefix": "B", "description": "Float switch", "join": "device"},
        }),
    );
    let mut comps = Vec::new();
    for x in [100.0, 150.0, 200.0] {
        let v = run(h, "place", json!({"symbol": "float1", "at": [x, 100.0]}));
        comps.push(ecad_model::ComponentId::from(u128::from_str_radix(v["component"].as_str().unwrap(), 16).unwrap()));
    }
    run(h, "wire", json!({"points": [[100.0, 90.0], [100.0, 60.0]]}));
    let d = doc(h);
    comps.iter().filter_map(|c| d.representations_of(Subject::Component(*c)).next().map(|r| r.0)).collect()
}

/// Boards 3.01–3.04, 4.01, 5.01 and 5.02 at 1920 × 1080, as `uiv3-F-<board>-ours.png`.
#[test]
fn screenshots_of_section_f() {
    let Some(out) = crate::project_panel::shots() else { return };
    ecad_trial::canvas::register_sheet_faces();
    let dir = std::env::temp_dir().join(format!("ecad-uiv3f-shots-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let deltas: crate::mouse_tester::Deltas = std::sync::Arc::default();
    let mut h = window(Some(deltas.clone()));
    let shot = |h: &H, board: &str| {
        let _ = crate::mouse_tester::shot(h, &deltas, &out.join(format!("uiv3-F-{board}-ours.png")));
    };
    demo(&mut h, &dir);

    // 3.01: Library ▸ Symbols, a tile selected.
    h.state_mut().set_workspace(Workspace::Library);
    h.run_steps(3);
    h.state_mut().library.browser.selected = Some(browser::Tile::Builtin("coil"));
    h.state_mut().open_inspector(ecad_trial::shell::RightTab::Device);
    h.run_steps(4);
    shot(&h, "3.01");

    // 3.02: New symbol, with a line drawn.
    h.state_mut().new_symbol();
    h.run_steps(4);
    shot(&h, "3.02");
    h.state_mut().close_builder(true);
    h.run_steps(2);

    // 3.03: Make symbol from lines and a text on the page.
    h.state_mut().set_workspace(Workspace::Draw);
    h.run_steps(2);
    let mut hs = Vec::new();
    for (a, b) in
        [((0.0, -10.0), (0.0, -5.0)), ((-5.0, -5.0), (5.0, -5.0)), ((5.0, -5.0), (5.0, 5.0)), ((5.0, 5.0), (-5.0, 5.0)), ((-5.0, 5.0), (-5.0, -5.0))]
    {
        let v = run(&mut h, "line", json!({"points": [[a.0 + 300.0, a.1 + 100.0], [b.0 + 300.0, b.1 + 100.0]]}));
        if let Some(x) = v["handles"][0].as_str().and_then(cadcraft_engine::doc::Handle::parse_hex) {
            hs.push(x);
        }
    }
    if let Ok(v) = h.state_mut().cad.run("text", json!({"at": [307.5, 97.5], "text": "-LS1", "height": 2.5})) {
        hs.extend(
            v["handles"]
                .as_array()
                .into_iter()
                .flatten()
                .chain(v.get("handle"))
                .filter_map(|x| x.as_str())
                .filter_map(cadcraft_engine::doc::Handle::parse_hex),
        );
    }
    h.state_mut().cad.session.set_selection(hs);
    if h.state_mut().make_symbol(None).is_ok() {
        h.run_steps(4);
        shot(&h, "3.03");
        h.state_mut().close_builder(true);
        h.run_steps(2);
    }

    // 3.04: Edit symbol with its review ("Before saving").
    h.state_mut().set_workspace(Workspace::Draw);
    h.run_steps(2);
    let reps = three_copies(&mut h);
    if let Some(&r) = reps.get(1) {
        h.state_mut().edit_placed(r).unwrap();
        h.run_steps(3);
        if let Some(b) = h.state_mut().library.builder.as_mut() {
            b.draft.delete(Picked::Point(1));
        }
        h.run_steps(2);
        h.state_mut().builder_save(false);
        h.run_steps(4);
        shot(&h, "3.04");
        h.state_mut().close_builder(true);
        h.run_steps(2);
    }
    let _ = symbols::STARTERS.len();

    // 4.01: Publish.
    h.state_mut().set_workspace(Workspace::Publish);
    h.run_steps(6);
    shot(&h, "4.01");

    // 5.01 and 5.02: Settings.
    h.state_mut().set_workspace(Workspace::Draw);
    h.run_steps(2);
    h.state_mut().open_settings(ecad_trial::settings::Section::Project);
    h.run_steps(4);
    shot(&h, "5.01");
    h.state_mut().open_settings(ecad_trial::settings::Section::Standards);
    h.run_steps(4);
    shot(&h, "5.02");
    let _ = std::fs::remove_dir_all(&dir);
}
