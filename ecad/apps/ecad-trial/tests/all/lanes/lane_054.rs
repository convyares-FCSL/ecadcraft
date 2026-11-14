//! Brief 054a in the trial (design 054 §4.1, §5; AC13, AC14): Settings ▸ Pages and sheets writes
//! one `setstyle` per control and lists the six types; the + Page picker adds a typed page after
//! the open one, Esc writes nothing, and Panel layout is disabled.

use crate::project_panel::{active, build, click, doc, head, ok, trial};

use ecad_cadcraft::styles::page as ptype;
use ecad_model::StyleRef;
use ecad_trial::settings::{Section, pages};
use ecad_trial::shell::new_page;
use eframe::egui::Key;
use egui_kittest::kittest::Queryable;

#[test]
fn pages_and_sheets_lists_the_types_and_each_control_writes_one_setstyle() {
    let mut h = build(trial(), None);
    h.set_size(eframe::egui::vec2(1920.0, 1080.0));
    h.state_mut().open_settings(Section::Pages);
    h.run_steps(4);
    for id in ["front", "schematic", "network", "singleline", "panel", "report"] {
        assert!(h.query_by_label(&format!("ecad.settings.pages.row.{id}")).is_some(), "{id}");
    }
    assert!(h.query_by_label(pages::TABLE).is_some());
    // Panel layout is selected on opening (board 5.02).
    assert_eq!(h.state().settings.pages.selected, Some(StyleRef::Builtin(ptype::PANEL.into())));
    let before = head(&mut h);
    click(&mut h, "ecad.settings.pages.zones.schematic");
    assert_eq!(head(&mut h), before + 1, "one transaction");
    let d = doc(&mut h);
    assert!(!ptype::look_of(&d, &StyleRef::Builtin("schematic".into())).zones);
    // The rows follow the commit.
    h.run_steps(3);
    let row = h.state().settings.pages.snapshot.as_ref().and_then(|s| s.rows.iter().find(|r| r.id.text() == "schematic").cloned());
    assert_eq!(row.map(|r| (r.zones, r.overridden)), Some((false, true)));
    // + Page type copies the selected row.
    click(&mut h, pages::ADD);
    assert_eq!(head(&mut h), before + 2);
    let own = h.state().settings.pages.selected.clone();
    assert!(matches!(own, Some(StyleRef::Own(_))), "the copy is selected: {own:?}");
    ok(&mut h, "u");
    ok(&mut h, "u");
}

#[test]
fn the_new_page_picker_adds_a_typed_page_after_the_open_one() {
    let mut h = build(trial(), None);
    ok(&mut h, r#"newpage {"title":"two"}"#);
    let d = doc(&mut h);
    let (p1, p2) = (d.page_order[0], d.page_order[1]);
    ok(&mut h, &format!(r#"openpage {{"page":"{}"}}"#, p1.hex()));
    // Opened from the page well's +: Schematic (the open page's type) is highlighted.
    let before = head(&mut h);
    click(&mut h, ecad_trial::shell::status_bar::ADD_PAGE);
    assert!(h.query_by_label(new_page::PICKER).is_some(), "the picker is open");
    let pk = h.state().shell.new_page.clone().unwrap();
    assert_eq!(pk.rows.get(pk.hi).map(|r| r.id.text()), Some("schematic".into()));
    assert!(pk.rows.iter().any(|r| r.id.text() == "panel" && !r.enabled), "Panel layout is disabled");
    // Esc writes nothing.
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(h.state().shell.new_page.is_none());
    assert_eq!(head(&mut h), before);
    // Network: one transaction, right after the open page.
    click(&mut h, ecad_trial::shell::status_bar::ADD_PAGE);
    click(&mut h, "ecad.newpage.type.network");
    assert_eq!(head(&mut h), before + 1);
    let d = doc(&mut h);
    let new = d.page_order.iter().copied().find(|p| *p != p1 && *p != p2).unwrap();
    let book = ecad_model::book::Book::of(&d);
    assert_eq!(book.order, vec![p1, new, p2], "after the open page");
    assert_eq!(ptype::type_of(&d, new).text(), "network");
    assert_eq!(active(&mut h), Some(new), "the new page is shown");
    // Front sheet opens its kinds; Cover adds a generated cover.
    click(&mut h, ecad_trial::shell::status_bar::ADD_PAGE);
    click(&mut h, "ecad.newpage.type.front");
    click(&mut h, "ecad.newpage.kind.cover");
    let d = doc(&mut h);
    assert!(d.pages.values().any(|p| p.properties.get("generated").map(String::as_str) == Some("cover")));
}
