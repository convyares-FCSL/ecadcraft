//! Brief 042 (U4c) in the trial, headless (egui_kittest): Settings opens from the project menu,
//! the status bar's standard and an origin badge, and Esc returns to the same workspace, page and
//! selection; Settings ▸ Standards lists every default, says the consequence before a change and
//! resets; the inspector badges a symbol's values; the Standards table is patched, not rebuilt,
//! by a `textstyle` call and left alone by a commit that touches no default. Screenshots are
//! written when `ECAD_TESTER_SHOTS` names a folder and a wgpu adapter exists (never committed).

use std::path::PathBuf;
use std::sync::Arc;

use ecad_cadcraft::defaults::{self, Scope, Source};
use ecad_trial::settings::{Section, standards};
use ecad_trial::shell::{Workspace, status_bar, top_bar};
use eframe::egui::Key;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::mouse_tester::{Deltas, doc, harness_with, head, ok, shot};

type H = Harness<'static, ecad_trial::Trial>;

fn save(h: &H, deltas: &Deltas, name: &str) {
    let Some(dir) = std::env::var_os("ECAD_TESTER_SHOTS").map(PathBuf::from) else { return };
    if std::fs::create_dir_all(&dir).is_ok() && shot(h, deltas, &dir.join(name)).is_none() {
        eprintln!("no wgpu adapter: {name} skipped");
    }
}

fn click(h: &mut H, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(3);
}

/// The shell's own interface with a coil placed and its symbol selected.
fn fixture(deltas: Option<Deltas>) -> (H, ecad_model::RepresentationId) {
    let mut h = harness_with(deltas);
    h.state_mut().set_cad_chrome(false);
    ok(&mut h, r#"place {"symbol":"coil","at":[100,100]}"#);
    let d = doc(&mut h);
    let rep = *d.representations.keys().next().expect("the coil's symbol");
    h.state_mut().goto_symbols(&[rep]).unwrap();
    h.run_steps(3);
    (h, rep)
}

fn selection(h: &H) -> Vec<cadcraft_engine::doc::Handle> {
    h.state().cad.session.selection()
}

#[test]
fn settings_opens_from_the_menu_the_status_bar_and_a_badge_and_esc_returns() {
    let deltas: Deltas = Arc::default();
    let (mut h, _) = fixture(Some(deltas.clone()));
    let sel = selection(&h);
    assert!(!sel.is_empty(), "the coil is selected");
    let ws = h.state().shell.state.workspace;
    let page = crate::project_panel::active(&mut h);

    // Project menu ▸ Settings.
    click(&mut h, top_bar::PROJECT_MENU);
    click(&mut h, ecad_trial::shell::SETTINGS);
    assert!(h.state().shell.settings_open);
    for s in Section::ALL {
        assert!(h.query_by_label(s.name()).is_some(), "{}", s.name());
    }
    // Esc: back where you were.
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(!h.state().shell.settings_open, "Esc closes Settings");
    assert_eq!((h.state().shell.state.workspace, crate::project_panel::active(&mut h), selection(&h)), (ws, page, sel.clone()));

    // The status bar's standard opens Standards.
    h.state_mut().settings.section = Section::Developer;
    click(&mut h, status_bar::STANDARD);
    assert_eq!((h.state().shell.settings_open, h.state().settings.section), (true, Section::Standards));
    click(&mut h, ecad_trial::settings::CLOSE);
    assert!(!h.state().shell.settings_open);
    assert_eq!(selection(&h), sel, "Close keeps the selection");

    // The inspector: the symbol's tag prefix, read (grey) from the built-in, with its badge. (Brief
    // 050a: the tag height is its text style's now, chosen on the device's Tag style row.)
    let rows = &h.state().settings.inspector.rows;
    let tag = rows.iter().find(|r| r.key == "symbol.coil.prefix").expect("the coil prefix row");
    assert_eq!((tag.from, tag.value.as_str(), tag.here), (Source::Builtin, "-K", None), "read here, not set");
    assert!(h.query_by_label("ecad.inspector.symbol.coil.prefix").is_some());
    save(&h, &deltas, "042-inspector.png");
    // A badge opens Standards at that key (the Tag style row's, at the top of the rows).
    click(&mut h, "ecad.origin.text.tag.style");
    assert_eq!((h.state().shell.settings_open, h.state().settings.section), (true, Section::Standards));
    let v = &h.state().settings.standards;
    assert_eq!(v.focus.and_then(|k| v.defaults.entry(k)).map(|e| e.key), Some("text.tag.style"));
    h.run_steps(2);
    save(&h, &deltas, "042-standards-tag-height.png");
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert_eq!(selection(&h), sel, "still selected after Standards");
    assert_eq!(h.state().shell.state.workspace, Workspace::Draw);
}

#[test]
fn standards_lists_every_default_says_the_consequence_first_and_resets() {
    let deltas: Deltas = Arc::default();
    let (mut h, _) = fixture(Some(deltas.clone()));
    h.state_mut().open_standards(Some("symbol.coil.prefix"));
    h.run_steps(3);
    let v = &h.state().settings.standards;
    assert_eq!(ecad_ui::table::TableSource::keys(&v.defaults).len(), defaults::all().count(), "one row per default");
    assert!(h.query_by_label(standards::STANDARD_LINE).is_some());
    // The chain: built-in and the standard; the built-in in use.
    assert!(h.query_by_label("ecad.standards.chain.builtin").is_some());
    assert_eq!(h.query_all_by_label(standards::IN_USE).count(), 1, "one level in use");
    assert!(h.query_by_label("ecad.standards.chain.standard").is_some());
    assert!(h.query_by_label(standards::CONSEQUENCE).is_none(), "nothing typed, nothing said");
    // Type Q: the consequence is said before anything is written.
    let before = head(&mut h);
    h.state_mut().settings.standards.edit = "Q".into();
    h.run_steps(3);
    assert!(h.query_by_label(standards::CONSEQUENCE).is_some(), "the consequence line");
    assert_eq!(
        standards::consequence(defaults::find("symbol.coil.prefix").unwrap(), 1, "-K", "-Q"),
        "1 device uses -K; fixed and released tags keep their names; new devices get -Q."
    );
    assert_eq!(head(&mut h), before, "said, not applied");
    save(&h, &deltas, "042-standards-consequence.png");
    click(&mut h, standards::APPLY);
    assert_eq!(head(&mut h), before + 1, "one transaction");
    let d = doc(&mut h);
    let e = defaults::find("symbol.coil.prefix").unwrap();
    assert_eq!(defaults::resolve(&d, e, Scope::project()).from, Source::Project);
    h.run_steps(2);
    assert!(h.query_by_label("ecad.standards.chain.project").is_some(), "the project's level, above the built-in");
    assert_eq!(h.query_all_by_label(standards::IN_USE).count(), 1, "one level in use");
    let row = h.state().settings.standards.focus.unwrap();
    assert_eq!(h.state().settings.standards.defaults.from(row), Some(Source::Project), "the row's badge says Project");
    save(&h, &deltas, "042-standards-project.png");
    // Reset: back to the built-in.
    click(&mut h, standards::RESET);
    assert_eq!(defaults::resolve(&doc(&mut h), e, Scope::project()).from, Source::Builtin);
}

#[test]
fn the_standards_table_is_patched_by_textstyle_and_left_by_other_commits() {
    let (mut h, _) = fixture(None);
    h.state_mut().open_standards(None);
    h.run_steps(3);
    // Brief 050a: the text styles' table on screen (its rows are what textstyle changes).
    h.state_mut().settings.standards.catalogue = 1;
    h.run_steps(2);
    let (rebuilds, patches) = (h.state().settings.standards.rebuilds, h.state().settings.standards.patches);
    assert_eq!(rebuilds, 1, "built on first show");
    // A commit touching no default: nothing.
    ok(&mut h, r#"wire {"points":[[0,150],[40,150]]}"#);
    h.run_steps(2);
    assert_eq!((h.state().settings.standards.rebuilds, h.state().settings.standards.patches), (rebuilds, patches));
    // `textstyle` from the Commands field: the text styles' rows are patched and tinted, not
    // rebuilt (brief 050a: the tag's height is its text style's).
    ok(&mut h, r#"textstyle {"kind":"tag","height":3}"#);
    h.run_steps(2);
    let v = &h.state().settings.standards;
    assert_eq!((v.rebuilds, v.patches), (rebuilds, patches + 1));
    let (text, table) = &v.cats[0];
    let tag = text.row(text.key_of(&ecad_model::StyleRef::Builtin("tag".into())).unwrap()).unwrap();
    assert!(tag.values.iter().any(|(val, from)| val == "3 mm" && *from == Source::Project), "{:?}", tag.values);
    let now = h.ctx.input(|i| i.time);
    assert!(!table.tint.tinted(now).is_empty(), "the changed cells are tinted");
}

#[test]
fn the_inspector_sets_a_symbols_tag_shown_and_resets_it() {
    let (mut h, rep) = fixture(None);
    let before = head(&mut h);
    // The row's list sets it on this symbol (one `setdefault`); Reset takes it off.
    let line =
        ecad_cadcraft::default_cmds::line("text.tag.show", defaults::Level::Object, ecad_cadcraft::default_cmds::Target::Symbol(rep), Some("off"));
    h.state_mut().inspector_action(ecad_trial::shell::right::InspectorAction::Run("text.tag.show", line));
    h.run_steps(3);
    assert_eq!(head(&mut h), before + 1);
    let row = h.state().settings.inspector.rows.iter().find(|r| r.key == "text.tag.show").cloned().unwrap();
    assert_eq!((row.from, row.value.as_str()), (Source::Symbol, "Hidden"));
    h.get_by_label("ecad.inspector.reset.text.tag.show").scroll_to_me();
    h.run_steps(3);
    let r = h.get_by_label("ecad.inspector.reset.text.tag.show").rect();
    assert!(h.ctx.content_rect().contains_rect(r), "Reset is inside the window: {r:?}");
    click(&mut h, "ecad.inspector.reset.text.tag.show");
    let errors = h.state().settings.inspector.errors.clone();
    assert_eq!(head(&mut h), before + 2, "{errors:?} {:?}", h.state().cad.session.log.last());
    let row = h.state().settings.inspector.rows.iter().find(|r| r.key == "text.tag.show").cloned().unwrap();
    assert_eq!((row.from, row.value.as_str()), (Source::Builtin, "Shown on the page"));
}

/// Review of PR #78: with Settings open, Del and a tool key never act on the hidden page.
#[test]
fn keys_do_not_reach_the_page_under_settings() {
    let (mut h, _) = fixture(None);
    let sel = selection(&h);
    assert!(!sel.is_empty());
    let devices = doc(&mut h).components.len();
    let before = head(&mut h);
    let main = h.state().shell.slots.map(|s| s.main.center()).unwrap();
    h.state_mut().open_settings(Section::Developer);
    h.run_steps(2);
    // The pointer over where the page is, as the shell's single-letter keys need.
    h.event(eframe::egui::Event::PointerMoved(main));
    h.run_steps(1);
    h.key_press(Key::Delete);
    h.run_steps(3);
    h.key_press(Key::W);
    h.run_steps(3);
    assert!(h.state().shell.settings_open, "still in Settings");
    assert_eq!((head(&mut h), doc(&mut h).components.len()), (before, devices), "Del erased nothing");
    assert!(h.state().cad.session.running.is_none(), "W started no tool");
    assert_eq!(selection(&h), sel, "the selection is kept");
    // The same keys with Settings closed do reach the page (the test is not vacuous).
    h.key_press(Key::Escape);
    h.run_steps(3);
    h.event(eframe::egui::Event::PointerMoved(main));
    h.run_steps(1);
    h.key_press(Key::Delete);
    h.run_steps(3);
    assert_eq!(head(&mut h), before + 1, "Del erases the selection once Settings is closed");
}
