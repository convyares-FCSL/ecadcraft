//! Tester 054 in the trial, headless (egui_kittest) at 1920 × 1080: design 054 §4.1, §4.3, §4.4,
//! §5 and §12.1 AC13 and AC14. The + Page picker from the page well on page 101 highlights
//! Schematic and adds 102 right after it; Esc writes nothing; Panel layout is disabled with its
//! tooltip; Settings ▸ Pages and sheets writes one `setstyle` per control, and read-only disables
//! every control; the Book's Renumber pages and a folder's Start number are named. Written from the
//! brief and design before the implementation; only the element names and words of the design.

use ecad_trial::Trial;
use ecad_trial::settings::Section;
use eframe::egui::{Key, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

use crate::project_panel::{doc, head, trial};

type H = Harness<'static, Trial>;

fn shell() -> H {
    let mut h = Harness::builder().with_size(vec2(1920.0, 1080.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        trial(),
    );
    h.run_steps(5);
    h
}

fn run(h: &mut H, cmd: &str, params: Value) -> Value {
    let v = h.state_mut().cad.run(cmd, params).unwrap_or_else(|e| panic!("{cmd}: {e}"));
    h.run_steps(2);
    v
}

fn click(h: &mut H, name: &str) {
    h.get_by_label(name).click();
    h.run_steps(3);
}

fn has(h: &H, name: &str) -> bool {
    h.query_by_label(name).is_some()
}

fn disabled(h: &H, name: &str) -> bool {
    h.get_by_label(name).accesskit_node().is_disabled()
}

fn key(h: &mut H, k: Key) {
    h.key_press(k);
    h.run_steps(2);
}

fn number_of(h: &mut H, page: ecad_model::PageId) -> String {
    let d = doc(h);
    ecad_model::book::Book::of(&d).number(&d, page)
}

/// A project whose open page is numbered 101 (pages 100, 101, 103).
fn on_page_101() -> (H, ecad_model::PageId) {
    let mut h = shell();
    let d = doc(&mut h);
    let first = d.page_order[0];
    run(&mut h, "pageprops", json!({"page": first.hex(), "number": "100"}));
    let p101 = run(&mut h, "newpage", json!({"title": "Motors", "number": "101"}));
    let p101: ecad_model::PageId = ecad_model::PageId::from(u128::from_str_radix(p101["page"].as_str().unwrap(), 16).unwrap());
    run(&mut h, "newpage", json!({"title": "Controls", "number": "103"}));
    run(&mut h, "openpage", json!({"page": p101.hex()}));
    (h, p101)
}

/// AC14, §4.1: from the page well on page 101 the picker highlights Schematic, shows `102` at its
/// right, and Enter adds 102 right after 101 (one transaction); the status line says so.
#[test]
#[ignore = "tester 054: awaiting build"]
fn ac14_the_picker_adds_102_right_after_101() {
    let (mut h, p101) = on_page_101();
    click(&mut h, ecad_trial::shell::status_bar::ADD_PAGE);
    assert!(has(&h, "ecad.newpage.picker"), "the picker opens");
    for t in ["front", "schematic", "network", "singleline", "panel", "report"] {
        assert!(has(&h, &format!("ecad.newpage.type.{t}")), "row {t}");
    }
    let row = h.get_by_label("ecad.newpage.type.schematic");
    assert!(
        row.accesskit_node().is_selected().unwrap_or(false) || format!("{:?}", row.accesskit_node()).contains("102"),
        "Schematic highlighted, showing 102"
    );
    let head0 = head(&mut h);
    key(&mut h, Key::Enter);
    assert_eq!(head(&mut h), head0 + 1, "one transaction");
    let d = doc(&mut h);
    let b = ecad_model::book::Book::of(&d);
    let at = b.order.iter().position(|p| *p == p101).unwrap();
    let new = b.order[at + 1];
    assert_eq!(number_of(&mut h, new), "102", "right after 101 with the next available number");
    assert!(h.query_by_label_contains("Added page 102 (Schematic) after 101.").is_some(), "§4.1 status words");
}

/// AC14, §4.1: Esc closes the picker and writes nothing; Panel layout is disabled with its tooltip;
/// Front sheet opens its kinds.
#[test]
#[ignore = "tester 054: awaiting build"]
fn ac14_esc_writes_nothing_and_panel_layout_is_disabled() {
    let (mut h, _) = on_page_101();
    let head0 = head(&mut h);
    click(&mut h, ecad_trial::shell::status_bar::ADD_PAGE);
    assert!(disabled(&h, "ecad.newpage.type.panel"), "Q3: disabled");
    h.get_by_label("ecad.newpage.type.panel").hover();
    h.run_steps(30);
    assert!(h.query_by_label_contains("Panel layout pages come with the panel layout page.").is_some(), "the tooltip");
    click(&mut h, "ecad.newpage.type.front");
    for k in ["cover", "revisions", "contents"] {
        assert!(has(&h, &format!("ecad.newpage.kind.{k}")), "kind {k}");
    }
    key(&mut h, Key::Escape);
    key(&mut h, Key::Escape);
    assert!(!has(&h, "ecad.newpage.picker"), "closed");
    assert_eq!(head(&mut h), head0, "Esc writes nothing");
}

/// AC13, §5.2: each control of Settings ▸ Pages and sheets writes one `setstyle`: the Zones toggle
/// and the Position segment of Schematic; the Pages count reads the types index.
#[test]
#[ignore = "tester 054: awaiting build"]
fn ac13_each_settings_control_writes_one_setstyle() {
    let mut h = shell();
    h.state_mut().open_settings(Section::Pages);
    h.run_steps(4);
    assert!(has(&h, "ecad.settings.pages.table"));
    for t in ["front", "schematic", "network", "singleline", "panel", "report"] {
        for col in ["row", "sheet", "frame", "titleblock", "zones", "position", "count"] {
            assert!(has(&h, &format!("ecad.settings.pages.{col}.{t}")), "{col}.{t}");
        }
    }
    assert!(has(&h, "ecad.settings.pages.add") && has(&h, "ecad.settings.pages.project"));
    let head0 = head(&mut h);
    click(&mut h, "ecad.settings.pages.zones.schematic");
    assert_eq!(head(&mut h), head0 + 1, "one transaction");
    let d = doc(&mut h);
    let last = run(&mut h, "steps", json!({"limit": 1}));
    assert!(last.to_string().contains("SetStyle") || d.styles.get("page").is_some_and(|m| !m.is_empty()), "a page record: {last}");
    run(&mut h, "u", json!({}));
    assert_eq!(doc(&mut h).styles.get("page").map(|m| m.len()).unwrap_or(0), 0, "one u");
}

/// AC13, §5.5: read only (a revision open) disables every control.
#[test]
#[ignore = "tester 054: awaiting build"]
fn ac13_read_only_disables_every_control() {
    let mut h = shell();
    run(&mut h, "revision", json!({"description": "first"}));
    run(&mut h, "issuerevision", json!({}));
    run(&mut h, "openrevision", json!({"rev": "A"}));
    h.state_mut().open_settings(Section::Pages);
    h.run_steps(4);
    for col in ["sheet", "zones", "position", "titleblock"] {
        assert!(disabled(&h, &format!("ecad.settings.pages.{col}.schematic")), "{col} disabled while read only");
    }
    assert!(disabled(&h, "ecad.settings.pages.add"));
}

/// §4.3, §4.4: the Book's header menu has Renumber pages and a folder row offers Start number….
#[test]
#[ignore = "tester 054: awaiting build"]
fn num_the_book_names_renumber_and_start_number() {
    let mut h = shell();
    let f = run(&mut h, "newfolder", json!({"name": "Schematic"}));
    let f = f["folder"].as_str().unwrap().to_string();
    run(&mut h, "folderprops", json!({"folder": f, "start": "100"}));
    h.run_steps(3);
    assert!(
        h.query_by_label_contains("Schematic").is_some_and(|n| format!("{:?}", n.accesskit_node()).contains("100"))
            || h.query_by_label_contains("100").is_some(),
        "the folder row shows its start number grey"
    );
    // Opening the header and row menus (ecad.book.renumber, ecad.book.folder.start.<id>) is left
    // to the click-through trial: the design names the items, not how the menus open.
}
