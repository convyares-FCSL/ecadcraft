//! Brief 054a in the app (design 054 §4.1, §5; AC13, AC14): Settings ▸ Pages and sheets writes
//! one `setstyle` per control and lists the six types; the + Page picker adds a typed page after
//! the open one, Esc writes nothing, and Panel layout is disabled.

use crate::pages::project_panel::{active, build, click, doc, head, new_app, ok};

use ecad_app::settings::{Section, pages};
use ecad_app::shell::new_page;
use ecad_electrical::styles::page as ptype;
use ecad_model::StyleRef;
use eframe::egui::Key;
use egui_kittest::kittest::Queryable;

#[test]
fn pages_and_sheets_lists_the_types_and_each_control_writes_one_setstyle() {
    let mut h = build(new_app(), None);
    h.set_size(eframe::egui::vec2(1920.0, 1080.0));
    h.state_mut().open_settings(Section::Pages);
    h.run_steps(4);
    for id in ["front", "schematic", "network", "singleline", "panel", "report", "plain"] {
        assert!(h.query_by_label(&format!("ecad.settings.pages.row.{id}")).is_some(), "{id}");
    }
    assert!(h.query_by_label(pages::TABLE).is_some());
    // Panel layout is selected on opening (board 5.02).
    assert_eq!(h.state().settings.pages.selected, Some(StyleRef::Builtin(ptype::PANEL.into())));
    let before = head(&mut h);
    click(&mut h, "ecad.settings.pages.zones.schematic");
    assert_eq!(head(&mut h), before + 1, "one transaction");
    let d = doc(&mut h);
    assert!(!ptype::draw_of(&d, &StyleRef::Builtin("schematic".into())).zones);
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
    let mut h = build(new_app(), None);
    ok(&mut h, r#"newpage {"title":"two"}"#);
    let d = doc(&mut h);
    let (p1, p2) = (d.page_order()[0], d.page_order()[1]);
    ok(&mut h, &format!(r#"openpage {{"page":"{}"}}"#, p1.hex()));
    // Opened from the page well's +: Schematic (the open page's type) is highlighted.
    let before = head(&mut h);
    click(&mut h, ecad_app::shell::status_bar::ADD_PAGE);
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
    click(&mut h, ecad_app::shell::status_bar::ADD_PAGE);
    click(&mut h, "ecad.newpage.type.network");
    assert_eq!(head(&mut h), before + 1);
    let d = doc(&mut h);
    let new = d.page_order().iter().copied().find(|p| *p != p1 && *p != p2).unwrap();
    let book = ecad_model::document::book::Book::of(&d);
    assert_eq!(book.order, vec![p1, new, p2], "after the open page");
    assert_eq!(ptype::type_of(&d, new).text(), "network");
    assert_eq!(active(&mut h), Some(new), "the new page is shown");
    // Cover sheet opens its kinds; Cover adds a generated cover.
    click(&mut h, ecad_app::shell::status_bar::ADD_PAGE);
    click(&mut h, "ecad.newpage.type.front");
    click(&mut h, "ecad.newpage.kind.cover");
    let d = doc(&mut h);
    assert!(d.pages().values().any(|p| p.kind.generated_kind() == Some(ecad_model::GenKind::Cover)));
}

/// Brief 054b: the board 1.16 project (a pump station's cover) and the screenshots of design 054
/// §12.2 when `$ECAD_TESTER_SHOTS` is set: `054-cover.png` beside board 1.16 (and
/// `054-cover-editor.png` with the notes in the table editor),
/// and `054-pages-and-sheets.png` (Panel layout selected) beside board 5.02.
#[test]
fn the_cover_and_its_page_tab_with_the_board_screenshots() {
    use crate::pages::project_panel::{Deltas, take};
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = build(new_app(), Some(deltas.clone()));
    h.set_size(eframe::egui::vec2(1920.0, 1080.0));
    h.state_mut().cad.session.viewport_px = (1920.0, 1080.0);
    ok(
        &mut h,
        r#"projectprops {"title":"Pump station PS-04","drawing_number":"PS04-E-001","customer":"Northfield Water","props":{"site":"Northfield treatment works, pump house 2","cover.notes":"All control wires are tri-rated unless stated.\nLine colours on the drawing are not the wire colours.\nA line through terminals means they are linked.\nCable core colours do not follow the wire colours."}}"#,
    );
    for (i, s) in ["coil", "no", "nc", "lamp", "motor3", "fuse", "terminal_block"].iter().enumerate() {
        ok(&mut h, &format!(r#"place {{"symbol":"{s}","at":[{},150]}}"#, 60 + 40 * i));
    }
    let mut wires = Vec::new();
    for (i, cat) in ["l1", "l2", "l3", "n", "dc24", "dc0", "pe"].iter().enumerate() {
        let y = 60 + 8 * i;
        crate::common::wire_pp(h.state_mut(), &[(40.0, y as f64), (340.0, y as f64)]);
        h.run_steps(2);
        let d = doc(&mut h);
        let c = d.wires().keys().copied().find(|c| !wires.contains(c)).unwrap();
        wires.push(c);
        ok(&mut h, &format!(r#"category {{"conductor":"{}","value":"{cat}"}}"#, c.hex()));
    }
    ok(&mut h, r#"cableslash {"from":[200,55],"to":[205,70],"tag":"W1"}"#);
    for (rev, what) in [("A", "First issue"), ("B", "Customer comments"), ("C", "Released for build")] {
        ok(&mut h, &format!(r#"issuerevision {{"rev":"{rev}","description":"{what}","drawn":"CvA","provisional":"issue"}}"#));
    }
    let first = doc(&mut h).page_order()[0];
    ok(&mut h, &format!(r#"newpage {{"type":"front","generated":"cover","after":"{}"}}"#, first.hex()));
    let cover = active(&mut h).unwrap();
    ok(&mut h, &format!(r#"movepage {{"page":"{}","index":1}}"#, cover.hex()));
    ok(&mut h, &format!(r#"newpage {{"type":"front","generated":"revisions","after":"{}"}}"#, cover.hex()));
    ok(&mut h, "renumberpages {}");
    ok(&mut h, &format!(r#"openpage {{"page":"{}"}}"#, cover.hex()));
    ok(&mut h, "zoom e");
    h.run_steps(6);
    // The Page tab: the block rows with Edit…, the generated pages and the sheet from the type.
    assert!(h.query_by_label("ecad.page.block.notes").is_some(), "the Page tab on the cover");
    assert!(h.query_by_label("ecad.page.front.contents").is_some());
    assert!(h.query_by_label(ecad_app::inspector::page::NOTES).is_none(), "no inline notes box (owner, fix round)");
    // The rail's first button names what the panel shows (owner, review of 054): a page here.
    assert_eq!(ecad_app::inspector::shows_word(h.state_mut()), "Page");
    take(&h, &deltas, "054-cover.png");
    // The font check at 100 % (96 px to 25.4 mm): the notes and the wire colours, top left.
    ok(&mut h, "zoom c 160,215 230");
    h.run_steps(4);
    take(&h, &deltas, "054-cover-100.png");
    ok(&mut h, "zoom e");
    h.run_steps(2);
    // The notes in the shared table editor: a row added, Save is one transaction; Cancel nothing.
    let before = head(&mut h);
    click(&mut h, "ecad.page.edit.notes");
    assert!(h.query_by_label(ecad_app::shell::table_editor::EDITOR).is_some(), "the editor opens");
    // A new window lays itself out over its first frames.
    h.run_steps(4);
    take(&h, &deltas, "054-cover-editor.png");
    click(&mut h, ecad_app::shell::table_editor::CANCEL);
    h.run_steps(2);
    assert!(h.state().settings.page_tab.editor.is_none());
    assert_eq!(head(&mut h), before, "Cancel writes nothing");
    click(&mut h, "ecad.page.edit.notes");
    if let Some(ed) = h.state_mut().settings.page_tab.editor.as_mut() {
        assert_eq!(ed.rows.len(), 4, "the four notes, one per row");
        ed.rows.swap(0, 1);
        ed.rows.push(ecad_app::shell::table_editor::Row { text: "Fifth note".into(), shown: true, ..Default::default() });
    }
    h.run_steps(2);
    click(&mut h, ecad_app::shell::table_editor::SAVE);
    assert_eq!(head(&mut h), before + 1, "Save is one transaction");
    let notes = ecad_electrical::pages::frames::project_text(&doc(&mut h), "cover.notes").unwrap_or_default();
    assert!(notes.starts_with("Line colours") && notes.ends_with("Fifth note"), "{notes}");
    ok(&mut h, "u");
    // The symbol legend's editor: generated rows, hide and relabel.
    click(&mut h, "ecad.page.edit.symbol_legend");
    if let Some(ed) = h.state_mut().settings.page_tab.editor.as_mut() {
        assert!(ed.rows.len() >= 5, "{}", ed.rows.len());
        if let Some(r) = ed.rows.first_mut() {
            r.text = "Our coil".into();
        }
        if let Some(r) = ed.rows.get_mut(1) {
            r.shown = false;
        }
    }
    h.run_steps(2);
    let before = head(&mut h);
    click(&mut h, ecad_app::shell::table_editor::SAVE);
    assert_eq!(head(&mut h), before + 1);
    ok(&mut h, "u");
    // Revisions: read only, with its link.
    click(&mut h, "ecad.page.edit.revisions");
    assert!(h.query_by_label(ecad_app::shell::table_editor::LINK).is_some());
    assert!(h.query_by_label(ecad_app::shell::table_editor::SAVE).is_none(), "read only");
    click(&mut h, ecad_app::shell::table_editor::CANCEL);
    // Contents on: one transaction, after the last front page; off again: one.
    let before = head(&mut h);
    click(&mut h, "ecad.page.front.contents");
    assert_eq!(head(&mut h), before + 1);
    assert!(doc(&mut h).pages().values().any(|p| p.kind.generated_kind() == Some(ecad_model::GenKind::Contents)));
    h.run_steps(3);
    click(&mut h, "ecad.page.front.contents");
    assert_eq!(head(&mut h), before + 2);
    assert!(!doc(&mut h).pages().values().any(|p| p.kind.generated_kind() == Some(ecad_model::GenKind::Contents)));
    // A block toggle: one setdefault.
    click(&mut h, "ecad.page.block.structure_codes");
    assert_eq!(head(&mut h), before + 3);
    ok(&mut h, "u");
    // Settings ▸ Pages and sheets with Panel layout selected (board 5.02).
    h.state_mut().open_settings(Section::Pages);
    h.run_steps(6);
    take(&h, &deltas, "054-pages-and-sheets.png");
}
