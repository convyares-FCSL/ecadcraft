//! Brief 051 in the trial (design 051 §10.1 J–M, P): Lists ▸ PLC shows the rack's tiles over the
//! channel table; a tile filters the table, a square selects its row, Make its page draws an
//! undrawn card, Description is edited in the table, and the strip follows its update map.
//! Screenshot `051-lists-plc.png` when `ECAD_TESTER_SHOTS` names a folder.

use ecad_model::ComponentId;
use ecad_trial::Trial;
use ecad_trial::lists::plc::{self, IoView};
use ecad_trial::shell::{ListsView, Workspace};
use ecad_ui::table::TableSource;
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::mouse_tester::{Collect, Deltas};
use crate::project_panel::{doc, head, ok, take, trial};

type H = Harness<'static, Trial>;

fn harness(deltas: Deltas) -> H {
    let mut h = Harness::builder().with_size(egui::vec2(1920.0, 1080.0)).with_step_dt(1.0 / 60.0).renderer(Collect(deltas)).build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        trial(),
    );
    h.run_steps(5);
    h
}

fn tagged(h: &mut H, tag: &str) -> ComponentId {
    doc(h).components_tagged(tag).first().copied().unwrap_or_else(|| panic!("{tag}"))
}

/// Fixture P51 in short: a CPU with six cards in slots 1–6 on the rack page; the DI card's first
/// eight channels fenced on wires; three DQ channels described (reserved) but no DQ cell drawn; the
/// AI card on its own page; DI channel 3 typed with channel 0's address (a check).
fn fixture(h: &mut H) {
    ok(h, r#"place {"symbol":"plccpu","at":[30,250]}"#);
    for (i, key) in ["plcdi", "plcdq", "plcai", "plcao", "plcrtd", "plciol"].iter().enumerate() {
        ok(h, &format!(r#"place {{"symbol":"{key}","at":[{},250]}}"#, 70 + 40 * i));
    }
    for i in 0..8 {
        let x = 60 + 20 * i;
        ok(h, &format!(r#"place {{"symbol":"lamp","at":[{x},160]}}"#));
        ok(h, &format!(r#"wire {{"points":[[{x},150],[{x},60]]}}"#));
    }
    ok(h, r#"plcchannels {"card":"-A02","from":[55,100],"to":[205,100]}"#);
    for (ch, text) in [(0, "Pump 1 run"), (1, "Pump 2 run"), (2, "Mixer run")] {
        ok(h, &format!(r#"plcchannel {{"card":"-A03","channel":{ch},"text":"{text}"}}"#));
    }
    ok(h, r#"plcpage {"card":"-A04"}"#);
    ok(h, r#"plcchannel {"card":"-A02","channel":3,"address":"I0.0"}"#);
}

fn show_plc(h: &mut H) {
    h.state_mut().set_workspace(Workspace::Lists);
    h.state_mut().shell.lists = ListsView::Plc;
    h.run_steps(4);
}

fn view(h: &H) -> &IoView {
    &h.state().reports.as_ref().expect("Lists ▸ PLC opens Reports' models").io
}

#[test]
fn j_k_l_m_lists_plc_tiles_filter_the_table_and_edit_it() {
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = harness(deltas.clone());
    fixture(&mut h);
    show_plc(&mut h);
    let (di, dq) = (tagged(&mut h, "-A02"), tagged(&mut h, "-A03"));
    let v = view(&h);
    assert!(v.chips.len() == 1, "one rack, no chips drawn: {:?}", v.chips);
    let names: Vec<String> = v.strip.iter().map(|t| t.name.clone()).collect();
    assert_eq!(names, ["rack", "1", "2", "3", "4", "5", "6", "free.7"].map(|s| format!("ecad.plc.tile.{s}")));
    assert_eq!(v.totals, "48 channels, 8 in use", "16 + 16 + 4 + 4 + 4 + 4 channels");
    // The first card is selected: its 16 rows, without Slot, Rack, Card and Type.
    assert_eq!(v.source.only(), Some(di));
    assert_eq!(v.source.keys().len(), 16);
    assert_eq!(v.source.columns().len(), 8);
    // Squares: in use, reserved, a check.
    let dq_tile = &v.strip[2];
    assert_eq!(dq_tile.marks.iter().filter(|m| **m == ecad_ui::table::strip::Mark::Soft).count(), 3);
    assert_eq!(v.strip[1].marks[3], ecad_ui::table::strip::Mark::Alert);
    assert!(h.query_by_label("ecad.plc.tile.2.makepage").is_some(), "DQ has no channel drawn");
    assert!(h.query_by_label("ecad.plc.tile.3.page.2").is_some(), "AI on page 2");

    // K: a tile filters the table; a square selects its row.
    h.get_by_label("ecad.plc.tile.2").click();
    h.run_steps(4);
    assert_eq!(view(&h).source.only(), Some(dq));
    take(&h, &deltas, "051-lists-plc.png");
    h.get_by_label("ecad.plc.tile.1.ch.5").click();
    h.run_steps(4);
    let v = view(&h);
    assert_eq!(v.source.only(), Some(di));
    assert_eq!(v.table.selection.keys().iter().copied().collect::<Vec<_>>(), [(di, 5)]);

    // M: Description edited in the table is one `plcchannel`; the tile is patched, not rebuilt.
    let (builds, patched) = (view(&h).strip_builds, view(&h).tiles_patched);
    let before = head(&mut h);
    {
        let t = h.state_mut();
        let io = &mut t.reports.as_mut().unwrap().io;
        io.table.cursor_col = Some(plc::TEXT);
        io.table.selection.click((di, 9));
    }
    h.state_mut().cad.session.cancel();
    edit(&mut h, "Edit Description", "Spare sensor");
    assert_eq!(head(&mut h), before + 1, "one transaction");
    let v = view(&h);
    assert_eq!(v.source.row((di, 9)).and_then(|r| r.text.clone()).as_deref(), Some("Spare sensor"));
    assert_eq!(v.strip_builds, builds, "no strip rebuild");
    assert!(v.tiles_patched > patched, "the DI tile patched");
    assert_eq!(v.strip[1].marks[9], ecad_ui::table::strip::Mark::Soft);
    // Too long: refused on the cell, nothing written.
    let before = head(&mut h);
    {
        let io = &mut h.state_mut().reports.as_mut().unwrap().io;
        io.table.cursor_col = Some(plc::TEXT);
        io.table.selection.click((di, 10));
    }
    edit(&mut h, "Edit Description", &"x".repeat(81));
    assert_eq!(head(&mut h), before);
    assert!(view(&h).table.refusal.is_some(), "the refusal shows on the cell");

    // P: a commit touching no PLC data changes neither the strip nor the rows.
    let (builds, patched, rows) = (view(&h).strip_builds, view(&h).tiles_patched, view(&h).patched);
    ok(&mut h, r#"wire {"points":[[300,0],[300,50]]}"#);
    show_plc(&mut h);
    let v = view(&h);
    assert_eq!((v.strip_builds, v.tiles_patched, v.patched), (builds, patched, rows));

    // L: Make its page draws the DQ card on a new page, one transaction; the tile then links it.
    let pages = doc(&mut h).page_order.len();
    let before = head(&mut h);
    h.get_by_label("ecad.plc.tile.2.makepage").click();
    h.run_steps(4);
    show_plc(&mut h);
    assert_eq!(head(&mut h), before + 1);
    assert_eq!(doc(&mut h).page_order.len(), pages + 1);
    assert!(h.query_by_label("ecad.plc.tile.2.makepage").is_none());
    assert!(view(&h).strip[2].links.iter().flatten().any(|l| l.name.starts_with("ecad.plc.tile.2.page.")));
}

#[test]
fn edit_lines_follow_the_column() {
    use ecad_cadcraft::plc::iolist::Row;
    let row = |pins: Vec<(&'static str, String)>| Row {
        card: ComponentId(1),
        card_tag: "-A02".into(),
        rack: None,
        slot: None,
        ty: "DI",
        channel: 3,
        index: 3,
        address: None,
        address_out: None,
        typed: false,
        pins,
        text: None,
        state: "spare",
        drawn: false,
        place: None,
        page: None,
        at: None,
        targets: Vec::new(),
        wire: None,
    };
    let one = row(vec![("in", "4".into())]);
    let two = row(vec![("+", "1".into()), ("-", "5".into())]);
    let cases: Vec<(&Row, ecad_ui::table::ColumnId, &str, Option<&str>)> = vec![
        (&one, plc::TEXT, "Pump", Some(r#""text":"Pump""#)),
        (&one, plc::TEXT, "  ", Some(r#""text":null"#)),
        (&one, plc::ADDRESS, "I9.9*", Some(r#""address":"I9.9""#)),
        (&one, plc::ADDRESS, "", Some(r#""address":null"#)),
        (&one, plc::PINS, "7", Some(r#""pins":{"in":"7"}"#)),
        (&two, plc::PINS, "+:2 -:6", Some(r#""pins":{"+":"2","-":"6"}"#)),
        (&two, plc::PINS, "2", None),
        (&two, plc::PINS, "x:2", None),
        (&one, plc::STATE, "used", None),
    ];
    for (r, col, value, want) in cases {
        let line = plc::edit_line(r, col, value);
        match want {
            Some(w) => assert!(line.as_deref().is_some_and(|l| l.starts_with("plcchannel ") && l.contains(w)), "{value}: {line:?}"),
            None => assert!(line.is_none(), "{value}: {line:?}"),
        }
    }
}

/// Edit the cell under the table's cursor: Enter, select all, type `text`, Enter (as 039's tests).
fn edit(h: &mut H, label: &str, text: &str) {
    use eframe::egui::{Event, Key, Modifiers};
    // The table has the keyboard (a click in it gives it; here the strip was clicked last).
    let id = view(h).table.id();
    h.ctx.memory_mut(|m| m.request_focus(id));
    h.run_steps(1);
    crate::project_panel::key(h, Key::Enter, Modifiers::NONE);
    let field = h.get_by_label(label);
    field.focus();
    h.run_steps(1);
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.event(Event::Text(text.into()));
    h.run_steps(1);
    crate::project_panel::key(h, Key::Enter, Modifiers::NONE);
    h.run_steps(2);
}

/// Screenshots of the two sheets for the owner (only when `ECAD_TESTER_SHOTS` names a folder and a
/// wgpu adapter exists; nothing is asserted about the pictures): `051-iolist-sheet.png`,
/// `051-rack-sheet.png`.
#[test]
fn screenshots_of_the_io_and_rack_sheets() {
    let Some(_) = crate::project_panel::shots() else { return };
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = harness(deltas.clone());
    fixture(&mut h);
    let di = tagged(&mut h, "-A02").hex();
    ok(&mut h, &format!(r#"setprop {{"component":"{di}","key":"part_number","value":"DI-16-24V"}}"#));
    for (name, kind) in [("051-rack-sheet.png", "plcrack"), ("051-iolist-sheet.png", "iolist")] {
        ok(&mut h, &format!(r#"newpage {{"generated":"{kind}"}}"#));
        h.state_mut().set_workspace(Workspace::Draw);
        h.state_mut().zoom_to_drawing();
        h.run_steps(6);
        take(&h, &deltas, name);
    }
    // The I/O sheet's first table, readable (the page on screen is the I/O list).
    let page = doc(&mut h).page_order.last().copied().unwrap();
    h.state_mut().goto_place(page, ecad_model::Pt::new(130.0, 240.0));
    if let Ok(st) = h.state_mut().cad.session.state_mut() {
        let _ = st.set_view(cadcraft_engine::View { center: cadcraft_engine::geom::Vec2::new(190.0, 205.0), height: 230.0, twist: 0.0 });
    }
    h.run_steps(6);
    take(&h, &deltas, "051-iolist-sheet-zoom.png");
}
