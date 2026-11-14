//! Brief 047 (design 047 §6) in the trial, headless (egui_kittest): Reports ▸ BOM's Columns menu
//! writes the article column and the nested layout as project defaults; + Item runs `bomitem`'s
//! prompts on the command line and its row reads metres; an item row has no page and its menu
//! deletes it; the inspector lists a device's part set read only. Screenshots are written when
//! `ECAD_TESTER_SHOTS` names a folder and a wgpu adapter exists (never committed).

use std::path::PathBuf;
use std::sync::Arc;

use ecad_trial::outputs_ui;
use ecad_trial::project_panel;
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

/// Three contactors with an aux block line each, Reports open on the BOM.
fn fixture(deltas: Option<Deltas>) -> H {
    let mut h = harness_with(deltas);
    h.state_mut().set_cad_chrome(false);
    ok(&mut h, r#"projectprops {"location":"+CP1"}"#);
    for (x, tag) in [(60, "K03"), (90, "K04"), (120, "K05")] {
        ok(&mut h, &format!(r#"place {{"symbol":"coil","at":[{x},100],"tag":"{tag}"}}"#));
    }
    let ks: Vec<String> = ["-K03", "-K04", "-K05"].iter().map(|t| doc(&mut h).components_tagged(t)[0].hex()).collect();
    ok(
        &mut h,
        &format!(
            r#"partline {{"components":{},"add":{{"part_number":"AB-1NO","maker":"Generic","description":"Aux block 1NO"}}}}"#,
            serde_json::json!(ks)
        ),
    );
    h.state_mut().panel_action(project_panel::Action::Reports);
    h.run_steps(4);
    h
}

fn table(h: &H) -> ecad_cadcraft::outputs::Table {
    h.state().reports.as_ref().unwrap().table().clone()
}

#[test]
fn columns_menu_writes_the_article_column_and_nested_layout() {
    let deltas: Deltas = Arc::default();
    let mut h = fixture(Some(deltas.clone()));
    let t = table(&h);
    assert!(!t.headers.contains(&"Article number"));
    let aux = t.rows.iter().find(|r| r[5].text == "AB-1NO").expect("the aux block row");
    assert_eq!((aux[1].text.as_str(), aux[2].text.as_str()), ("3", "-K03, -K04, -K05"));
    // Columns ▸ Article number: one transaction, the column appears.
    let n = head(&mut h);
    click(&mut h, outputs_ui::COLUMNS);
    click(&mut h, outputs_ui::COLUMN_ARTICLE);
    h.run_steps(3);
    assert_eq!(head(&mut h), n + 1, "one transaction");
    ok(&mut h, r#"article {"part_number":"AB-1NO","article_number":"A-0110"}"#);
    h.run_steps(3);
    let t = table(&h);
    assert_eq!(t.headers.last(), Some(&"Article number"));
    assert_eq!(t.rows.iter().find(|r| r[5].text == "AB-1NO").unwrap()[6].text, "A-0110");
    save(&h, &deltas, "047-reports-article.png");
    // Columns ▸ Lines under devices: each device's line under it.
    click(&mut h, outputs_ui::COLUMNS);
    click(&mut h, outputs_ui::COLUMN_NESTED);
    h.run_steps(3);
    let t = table(&h);
    let lines = t.rows.iter().filter(|r| r[3].text.contains("\u{21b3} Aux block 1NO")).count();
    assert_eq!(lines, 3, "{:#?}", t.rows);
    save(&h, &deltas, "047-reports-nested.png");
}

#[test]
fn add_item_prompts_on_the_command_line_and_the_row_reads_metres() {
    let mut h = fixture(None);
    let n = head(&mut h);
    click(&mut h, outputs_ui::ADD_ITEM);
    for line in ["DIN rail 35x7.5", "2", "Metres", "", "", ""] {
        let e = h.state_mut().submit_command(line);
        assert!(e.is_none(), "{line}: {e:?}");
        h.run_steps(2);
    }
    assert_eq!(head(&mut h), n + 1, "one transaction");
    h.run_steps(3);
    let t = table(&h);
    let i = t.rows.iter().position(|r| r[3].text == "DIN rail 35x7.5").unwrap_or_else(|| panic!("{:#?}", t.rows));
    assert_eq!((t.rows[i][0].text.as_str(), t.rows[i][1].text.as_str(), t.rows[i][2].text.as_str()), ("+CP1", "2 m", ""));
    // Its row: no page; the CSV export has a Unit column and a plain Qty.
    // A row's label is its first two cells.
    click(&mut h, "+CP1: 2 m");
    assert_eq!(h.state().reports.as_ref().unwrap().note.as_deref(), Some(outputs_ui::ITEM_NO_PAGE));
    let csv = h.state().reports.as_ref().unwrap().shown_table();
    assert_eq!(&csv.headers[..4], ["Location", "Qty", "Unit", "Tags"]);
    let row = csv.rows.iter().find(|r| r[4].text == "DIN rail 35x7.5").unwrap();
    assert_eq!((row[1].text.as_str(), row[2].text.as_str()), ("2", "m"));
}

#[test]
fn the_inspector_lists_a_devices_part_set() {
    let deltas: Deltas = Arc::default();
    let mut h = fixture(Some(deltas.clone()));
    h.state_mut().reports = None;
    let d = doc(&mut h);
    let k03 = d.components_tagged("-K03")[0];
    ok(&mut h, &format!(r#"setprop {{"component":"{}","key":"part_number","value":"CT-9A"}}"#, k03.hex()));
    ok(
        &mut h,
        &format!(
            r#"partline {{"component":"{}","kit":"Starter set","add":[{{"part_number":"SUP-1","description":"Suppressor"}},{{"description":"Label"}}]}}"#,
            k03.hex()
        ),
    );
    let rep = doc(&mut h).representations.iter().find(|(_, r)| r.subject == ecad_model::Subject::Component(k03)).map(|(id, _)| *id).unwrap();
    // UI rev 3: the Project workspace's right panel shows its selected row; the inspector of the
    // page's selection is Draw's.
    h.state_mut().set_workspace(ecad_trial::shell::Workspace::Draw);
    h.state_mut().goto_symbols(&[rep]).unwrap();
    h.run_steps(4);
    let parts = h.state().settings.inspector.parts.clone();
    assert_eq!(parts.len(), 4, "the main part and three lines: {parts:#?}");
    assert_eq!(parts[0].part_number.as_deref(), Some("CT-9A"));
    assert_eq!(parts[2].note.as_deref(), Some("kit Starter set"));
    assert_eq!(parts[3].part_number, None, "shown as ?");
    assert!(h.query_by_label(ecad_trial::shell::right::PARTS).is_some());
    assert!(h.query_by_label("ecad.inspector.parts.row.3").is_some());
    save(&h, &deltas, "047-inspector-parts.png");
    // A device with no part lines has no Parts section.
    let k05 = doc(&mut h).components_tagged("-K05")[0];
    ok(&mut h, &format!(r#"partline {{"component":"{}","line":1,"remove":true}}"#, k05.hex()));
    let rep = doc(&mut h).representations.iter().find(|(_, r)| r.subject == ecad_model::Subject::Component(k05)).map(|(id, _)| *id).unwrap();
    h.state_mut().goto_symbols(&[rep]).unwrap();
    h.run_steps(4);
    assert!(h.state().settings.inspector.parts.is_empty());
    assert!(h.query_by_label(ecad_trial::shell::right::PARTS).is_none());
}
