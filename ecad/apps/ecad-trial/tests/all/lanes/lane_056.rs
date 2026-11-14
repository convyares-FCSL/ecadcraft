//! Brief 056 in the trial (design 056 §13.1 A–I, L, M; board 2.03; the owner's change of
//! 2026-11-14): Project ▸ PLC as the signal list: the rack heading, the tiles' kind words, the card
//! header, the signal columns with free channels folded, Import an I/O list, Export tags, the card
//! editor in the right panel, and the footer's overview page. Screenshots `056-*.png` when
//! `ECAD_TESTER_SHOTS` names a folder.

use ecad_model::ComponentId;
use ecad_trial::Trial;
use ecad_trial::lists::ListAction;
use ecad_trial::lists::plc::{self, IoView};
use ecad_trial::shell::{ListsView, Workspace};
use ecad_ui::table::TableSource;
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use crate::mouse_tester::{Collect, Deltas};
use crate::project_panel::{doc, head, ok, take, trial};

type H = Harness<'static, Trial>;

const TAGS: &str = include_str!("../../../../../tests/fixtures/plc/P56-tags.csv");

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

/// Fixture P56 (design 056 §13), as `ecad-cadcraft`'s lane test builds it, with the `plcrack`
/// page in the book.
fn p56(h: &mut H) {
    ok(h, r#"place {"symbol":"plccpu","at":[30,250]}"#);
    for (i, key) in ["plcdi", "plcdq", "plcai", "plcao", "plcrtd"].iter().enumerate() {
        ok(h, &format!(r#"place {{"symbol":"{key}","at":[{},250]}}"#, 70 + 40 * i));
    }
    ok(h, r#"plcaddress {"rack":"-A01","style":"percent","output_start":2}"#);
    for i in 0..9 {
        let x = 60 + 20 * i;
        ok(h, &format!(r#"place {{"symbol":"lamp","at":[{x},160]}}"#));
        ok(h, &format!(r#"wire {{"points":[[{x},150],[{x},60]]}}"#));
    }
    ok(h, r#"place {"symbol":"terminal","at":[100,130]}"#);
    ok(h, r#"plcchannels {"card":"-A03","from":[55,100],"to":[225,100]}"#);
    let a03 = [
        ("Fan_On", "Fan contactor on"),
        ("Heater_On", "Heater relay on"),
        ("Valve_Inlet", "Inlet valve open"),
        ("Valve_Drain", "Drain valve open"),
        ("Lamp_Fault", "Fault lamp"),
        ("Lamp_Auto", "Automatic mode lamp"),
        ("Horn", "Alarm horn"),
        ("Pump2_Run", "Standby pump contactor"),
        ("Dosing_On", "Dosing pump on"),
    ];
    for (ch, (tag, text)) in a03.iter().enumerate() {
        ok(h, &format!(r#"plcchannel {{"card":"-A03","channel":{ch},"tag":"{tag}","text":"{text}"}}"#));
    }
    let (a02, a03) = (tagged(h, "-A02").hex(), tagged(h, "-A03").hex());
    ok(h, &format!(r#"setprop {{"component":"{a03}","key":"description","value":"Outputs, pumps and valves"}}"#));
    ok(h, &format!(r#"setprop {{"component":"{a03}","key":"plc.kind","value":"relay"}}"#));
    ok(h, &format!(r#"setprop {{"component":"{a02}","key":"plc.kind","value":"24 V"}}"#));
    ok(h, r#"newpage {"generated":"plcrack"}"#);
}

fn show_plc(h: &mut H) {
    h.state_mut().set_workspace(Workspace::Lists);
    h.state_mut().shell.lists = ListsView::Plc;
    h.run_steps(4);
}

fn view(h: &H) -> &IoView {
    &h.state().reports.as_ref().expect("Project ▸ PLC opens Reports' models").io
}

fn act(h: &mut H, a: ListAction) {
    let out = h.state_mut().reports.as_mut().and_then(|r| r.io.handle(a));
    assert!(out.is_none(), "answered by the view: {out:?}");
    h.run_steps(4);
}

fn tagged(h: &mut H, tag: &str) -> ComponentId {
    doc(h).components_tagged(tag).first().copied().unwrap_or_else(|| panic!("{tag}"))
}

fn import(h: &mut H) {
    let line = format!("plcimport {}", json!({"text": TAGS, "name": "P56-tags.csv"}));
    ok(h, &line);
    h.run_steps(4);
}

#[test]
fn a_b_g_project_plc_is_the_signal_list_of_board_2_03() {
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = harness(deltas.clone());
    p56(&mut h);
    show_plc(&mut h);
    let a03 = tagged(&mut h, "-A03");

    // A: the rack heading and the tiles.
    let v = view(&h);
    assert_eq!(v.rack_title, "Rack 1");
    assert!(v.rack_line.starts_with("-A01 to -A06 \u{b7} 6 of 7 slots used \u{b7} "), "{}", v.rack_line);
    assert_eq!(v.strip[2].kind, "16 outputs, relay");
    assert_eq!(v.strip[2].lines, ["Outputs, pumps and valves"]);
    assert_eq!(v.strip[1].kind, "16 inputs, 24 V");
    assert_eq!(v.strip[6].lines, ["Slot 6 is free"], "the free slot");
    assert!(view(&h).book_page.is_some(), "the overview page is in the book");

    // B: -A03 selected: the card header, the columns, free channels folded.
    h.get_by_label("ecad.plc.tile.2").click();
    h.run_steps(4);
    let v = view(&h);
    assert_eq!(v.source.only(), Some(a03));
    let head = v.head.clone().unwrap();
    assert_eq!(head.title, "-A03 \u{b7} output card \u{b7} slot 2");
    assert!(head.words.starts_with("16 channels, 9 in use, drawn in detail on page "), "{}", head.words);
    let titles: Vec<&str> = v.source.columns().iter().map(|c| c.title).collect();
    assert_eq!(titles, ["Ch", "Address", "Tag", "Description", "Type", "Safe", "Drives", "Page", "Test"], "no PLC column (owner, 2026-11-14)");
    assert_eq!(v.source.folded, 6, "the first free channel shows, six fold");
    assert!(h.query_by_label("6 more free channels.").is_some());
    let r2 = v.source.row((a03, 2)).unwrap();
    assert!(plc::edit_line(r2, plc::TAG, "Valve_In").is_some_and(|l| l.contains(r#""tag":"Valve_In""#)));
    assert!(plc::edit_line(r2, plc::SAFE, "On").is_some_and(|l| l.contains(r#""safe":"On""#)));
    let r9 = v.source.row((a03, 9)).unwrap();
    assert_eq!(plc::edit_line(r9, plc::TAG, plc::FREE), None, "the placeholder is not a tag");
    assert!(h.query_by_label("ecad.plc.card.editor.tag").is_some(), "the right panel edits the card");

    // G: Import an I/O list fills tags and descriptions, one transaction.
    let before = head_of(&mut h);
    import(&mut h);
    assert_eq!(head_of(&mut h), before + 1, "one transaction");
    let v = view(&h);
    assert_eq!(v.source.row((a03, 2)).and_then(|r| r.tag.clone()).as_deref(), Some("Valve_In"));
    assert_eq!(v.source.row((a03, 12)).and_then(|r| r.tag.clone()).as_deref(), Some("Spare_Out4"));
    assert!(v.source.folded < 6, "a channel that took a tag is no longer free");
    take(&h, &deltas, "056-plc.png");

    // The light theme of the list.
    h.get_by_label(ecad_trial::shell::top_bar::THEME_LIGHT).click();
    h.run_steps(4);
    take(&h, &deltas, "056-plc-light.png");
}

fn head_of(h: &mut H) -> u64 {
    head(h)
}

#[test]
fn f_import_an_io_list_from_the_path_row_is_one_transaction_and_refusals_stay_in_the_row() {
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = harness(deltas);
    p56(&mut h);
    show_plc(&mut h);
    h.get_by_label("ecad.plc.tile.2").click();
    h.run_steps(4);
    let dir = std::env::temp_dir().join(format!("ecad-056-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("P56-tags.csv");
    std::fs::write(&file, TAGS).unwrap();
    // A file that is not a tag table is refused in the path row, in words, and writes nothing.
    std::fs::write(dir.join("x.csv"), "PK\u{3}\u{4}junk").unwrap();
    let before = head_of(&mut h);
    run_path(&mut h, ListAction::ImportTags, &dir.join("x.csv"));
    let err = view(&h).path_row.as_ref().and_then(|r| r.error.clone());
    assert_eq!(err.as_deref(), Some(ecad_cadcraft::plc::tagtable::UNKNOWN));
    assert_eq!(head_of(&mut h), before);
    // The table: one transaction, and the row closes.
    run_path(&mut h, ListAction::ImportTags, &file);
    assert_eq!(head_of(&mut h), before + 1);
    assert!(view(&h).path_row.is_none(), "done: the row closes");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Open the path row with `a`, give it `path`, press its OK, and run what it asks.
fn run_path(h: &mut H, a: ListAction, path: &std::path::Path) {
    act(h, a);
    h.state_mut().reports.as_mut().unwrap().io.path_row.as_mut().unwrap().path = path.to_string_lossy().into_owned();
    let run = h.state_mut().reports.as_mut().and_then(|r| r.io.handle(ListAction::PathOk));
    let Some(ListAction::Run(line)) = run else { panic!("{run:?}") };
    let refusal = h.state_mut().submit_command(&line);
    if let Some(r) = h.state_mut().reports.as_mut() {
        r.ran(refusal);
    }
    h.run_steps(4);
}

#[test]
fn the_card_editor_edits_type_id_rack_slot_parts_and_start() {
    use ecad_trial::lists::card_editor::{field_line, start_byte};
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = harness(deltas.clone());
    p56(&mut h);
    show_plc(&mut h);
    h.get_by_label("ecad.plc.tile.2").click();
    h.run_steps(4);
    let a03 = tagged(&mut h, "-A03");
    for label in ["tag", "description", "slot", "function", "location", "start", "maker", "part_number"] {
        assert!(h.query_by_label(&format!("ecad.plc.card.editor.{label}")).is_some(), "{label}");
    }
    // The commands the fields run, each one transaction.
    let before = head_of(&mut h);
    for (key, value) in [("description", "Outputs, pumps"), ("maker", "Generic"), ("part_number", "DQ16-R-2A"), ("slot", "7")] {
        let line = field_line(a03, key, value).unwrap();
        ok(&mut h, &line);
    }
    assert_eq!(head_of(&mut h), before + 4);
    let d = doc(&mut h);
    let p = &d.components[&a03].properties;
    assert_eq!(p.get("part_number").map(String::as_str), Some("DQ16-R-2A"));
    assert_eq!(p.get("plc.slot").map(String::as_str), Some("7"));
    // An accessory (the front connector) as a part line.
    ok(
        &mut h,
        &format!("partline {}", json!({"component": a03.hex(), "add": {"description": "Front connector, 20-pin, screw", "part_number": "FC-20-S"}})),
    );
    let parts = ecad_cadcraft::bom::part_set(&doc(&mut h), a03);
    assert_eq!(parts.len(), 2, "the main part and the accessory: {parts:?}");
    // The type: one transaction through the segmented control's command.
    h.get_by_label("ecad.plc.card.editor.type.ao").click();
    h.run_steps(4);
    assert_eq!(ecad_cadcraft::plc::card_type_of(&doc(&mut h), a03), Some(ecad_cadcraft::plc::rack::CardType::Ao), "the card is now an AO card");
    assert_eq!(start_byte("%Q4.0"), Some(Some(4)));
    assert_eq!(start_byte("IW64"), Some(Some(64)));
    assert_eq!(start_byte(""), Some(None), "empty unpins");
    assert!(field_line(a03, "slot", "x").is_none(), "a slot is a number");
    take(&h, &deltas, "056-card-editor.png");
}

#[test]
fn i_export_tags_writes_the_rack_and_changes_nothing() {
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = harness(deltas);
    p56(&mut h);
    show_plc(&mut h);
    h.get_by_label("ecad.plc.tile.2").click();
    h.run_steps(4);
    let dir = std::env::temp_dir().join(format!("ecad-056-x-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("out.csv");
    let before = head_of(&mut h);
    act(&mut h, ListAction::ExportTags);
    let row = view(&h).path_row.clone().unwrap();
    assert!(row.path.ends_with("PLC tags.csv"), "{}", row.path);
    h.state_mut().reports.as_mut().unwrap().io.path_row.as_mut().unwrap().path = file.to_string_lossy().into_owned();
    let run = h.state_mut().reports.as_mut().and_then(|r| r.io.handle(ListAction::PathOk));
    let Some(ListAction::Run(line)) = run else { panic!("{run:?}") };
    assert!(line.starts_with("plcexport "), "{line}");
    ok(&mut h, &line);
    assert_eq!(head_of(&mut h), before, "export records nothing");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("Valve_Inlet,%Q2.2,Bool,Inlet valve open,'-A03,2,Off,'-H03:X2,"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn m_the_rack_sheet_prints_the_new_tile_lines() {
    let Some(_) = crate::project_panel::shots() else { return };
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = harness(deltas.clone());
    p56(&mut h);
    h.state_mut().set_workspace(Workspace::Draw);
    h.state_mut().zoom_to_drawing();
    h.run_steps(6);
    take(&h, &deltas, "056-rack-sheet.png");
}
