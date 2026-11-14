//! Brief 056 in the trial (design 056 §13.1 A–I, L, M; board 2.03): Project ▸ PLC as the signal
//! list: the rack heading, the tiles' kind words, the card header with the PLC chip, the signal
//! columns with free channels folded, the import preview, Compare and its take, Export tags, and
//! the footer's overview page. Screenshots `056-*.png` when `ECAD_TESTER_SHOTS` names a folder.

use ecad_cadcraft::plc::sync::State;
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

fn import(h: &mut H, take: &str) {
    let line = format!("plcimport {}", json!({"text": TAGS, "name": "P56-tags.csv", "take": take}));
    ok(h, &line);
    h.run_steps(4);
}

#[test]
fn a_b_g_h_project_plc_is_the_signal_list_of_board_2_03() {
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
    let footer = view(&h).book_page.clone();
    assert!(footer.is_some(), "the overview page is in the book");

    // B: -A03 selected: the card header, the columns, free channels folded.
    h.get_by_label("ecad.plc.tile.2").click();
    h.run_steps(4);
    let v = view(&h);
    assert_eq!(v.source.only(), Some(a03));
    let head = v.head.clone().unwrap();
    assert_eq!(head.title, "-A03 \u{b7} output card \u{b7} slot 2");
    assert!(head.words.starts_with("16 channels, 9 in use, drawn in detail on page "), "{}", head.words);
    assert_eq!(head.chip, "PLC: not imported");
    let titles: Vec<&str> = v.source.columns().iter().map(|c| c.title).collect();
    assert_eq!(titles, ["Ch", "Address", "Tag", "Description", "Type", "Safe", "Drives", "Page", "PLC", "Test"]);
    assert_eq!(v.source.folded, 6, "the first free channel shows, six fold");
    assert!(h.query_by_label("6 more free channels.").is_some());
    let r2 = v.source.row((a03, 2)).unwrap();
    assert!(plc::edit_line(r2, plc::TAG, "Valve_In").is_some_and(|l| l.contains(r#""tag":"Valve_In""#)));
    assert!(plc::edit_line(r2, plc::SAFE, "On").is_some_and(|l| l.contains(r#""safe":"On""#)));
    let r9 = v.source.row((a03, 9)).unwrap();
    assert_eq!(plc::edit_line(r9, plc::TAG, plc::FREE), None, "the placeholder is not a tag");

    // G: import; the chip counts -A03's differences and the PLC column shows them.
    let before = head_of(&mut h);
    import(&mut h, "none");
    assert_eq!(head_of(&mut h), before + 1, "one transaction");
    let v = view(&h);
    assert_eq!(v.head.as_ref().unwrap().chip, "\u{26a0} PLC: 3 differences");
    assert_eq!(v.source.state((a03, 2)), State::Differs("Valve_In".into()));
    assert_eq!(v.source.state((a03, 8)), State::NotInPlc);
    assert_eq!(v.source.state((a03, 12)), State::OnlyInPlc("Spare_Out4".into()));
    assert!(v.source.folded < 6, "an only-in-PLC channel is never folded");
    take(&h, &deltas, "056-plc.png");

    // H: Compare, then Use PLC's on channel 2: one transaction.
    act(&mut h, ListAction::Compare);
    let c = view(&h).compare.as_ref().unwrap();
    assert!(c.title.starts_with("Compare -A03 with P56-tags.csv (csv), imported "), "{}", c.title);
    assert_eq!(c.source.rows.len(), 3);
    take(&h, &deltas, "056-compare.png");
    let before = head_of(&mut h);
    ok(&mut h, r#"plctake {"items":[{"card":"-A03","channel":2}]}"#);
    h.run_steps(4);
    assert_eq!(head_of(&mut h), before + 1);
    assert_eq!(view(&h).compare.as_ref().unwrap().source.rows.len(), 2, "Compare follows the take");
    assert_eq!(view(&h).head.as_ref().unwrap().chip, "\u{26a0} PLC: 2 differences");

    // The light theme of the list.
    act(&mut h, ListAction::Tile(2));
    h.state_mut().reports.as_mut().unwrap().io.compare = None;
    h.run_steps(2);
    h.get_by_label(ecad_trial::shell::top_bar::THEME_LIGHT).click();
    h.run_steps(4);
    take(&h, &deltas, "056-plc-light.png");
}

fn head_of(h: &mut H) -> u64 {
    head(h)
}

#[test]
fn f_the_import_preview_writes_nothing_until_import() {
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = harness(deltas.clone());
    p56(&mut h);
    show_plc(&mut h);
    h.get_by_label("ecad.plc.tile.2").click();
    h.run_steps(4);
    let dir = std::env::temp_dir().join(format!("ecad-056-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("P56-tags.csv");
    std::fs::write(&file, TAGS).unwrap();
    let before = head_of(&mut h);
    act(&mut h, ListAction::ImportTags);
    h.state_mut().reports.as_mut().unwrap().io.path_row.as_mut().unwrap().path = file.to_string_lossy().into_owned();
    act(&mut h, ListAction::PathOk);
    let c = view(&h).compare.as_ref().expect("the preview");
    let p = c.pending.as_ref().unwrap();
    assert!(p.summary.starts_with("310 tags read; 10 with an I/O address."), "{}", p.summary);
    assert!(p.summary.contains("3 differences on -A03, 1 not on any card"), "{}", p.summary);
    assert_eq!(head_of(&mut h), before, "the preview writes nothing");
    take(&h, &deltas, "056-preview.png");
    // Esc writes nothing either.
    h.state_mut().reports.as_mut().unwrap().io.compare = None;
    h.run_steps(2);
    assert_eq!(head_of(&mut h), before);
    // A file that is not a tag table is refused in the path row, in words.
    std::fs::write(dir.join("x.csv"), "PK\u{3}\u{4}junk").unwrap();
    act(&mut h, ListAction::ImportTags);
    h.state_mut().reports.as_mut().unwrap().io.path_row.as_mut().unwrap().path = dir.join("x.csv").to_string_lossy().into_owned();
    act(&mut h, ListAction::PathOk);
    let err = view(&h).path_row.as_ref().and_then(|r| r.error.clone());
    assert_eq!(err.as_deref(), Some(ecad_cadcraft::plc::tagtable::UNKNOWN));
    let _ = std::fs::remove_dir_all(&dir);
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
    assert!(text.contains("Valve_Inlet,%Q2.2,Bool,Inlet valve open,-A03,2,Off,-H03:X2,"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn l_with_writes_off_compare_offers_no_take() {
    use ecad_trial::lists::plc_compare::{CompareView, TAKE};
    let row = ecad_cadcraft::plc::sync::CompareRow {
        at: Some((ComponentId(1), 2)),
        card_tag: "-A03".into(),
        channel: Some(2),
        address: "%Q2.2".into(),
        tag: Some("Valve_Inlet".into()),
        text: None,
        ty: "DQ",
        plc: None,
        state: State::Differs("Valve_In".into()),
    };
    let mut v = CompareView::new(Some(ComponentId(1)));
    v.set_rows(vec![row]);
    v.source.writable = true;
    assert!(matches!(v.source.cell(0, TAKE), ecad_ui::table::Cell::Buttons(_)));
    v.source.writable = false;
    assert_eq!(v.source.cell(0, TAKE), ecad_ui::table::Cell::Empty, "writes off: no take");
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
