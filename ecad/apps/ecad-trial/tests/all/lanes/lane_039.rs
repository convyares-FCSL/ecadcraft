//! Brief 039 (U3) in the trial: the Checks, Devices and Terminals column sets on the one table
//! (acceptance criteria 3, 4 and 5).

use ecad_trial::lists::{devices, terminals};
use ecad_trial::{Trial, outputs_ui};
use ecad_ui::table::{Align, Changed, TableSource};
use eframe::egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::project_panel::{Deltas, active, build, click, devices_fixture, doc, head, key, ok, take, trial};

type H = Harness<'static, Trial>;

/// Edit the cell under the table's cursor: Enter, select all, type `text`, Enter.
fn edit(h: &mut H, label: &str, text: &str) {
    key(h, Key::Enter, Modifiers::NONE);
    let field = h.get_by_label(label);
    field.focus();
    h.run_steps(1);
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.event(Event::Text(text.into()));
    h.run_steps(1);
    key(h, Key::Enter, Modifiers::NONE);
    h.run_steps(2);
}

/// Criteria 3 and 4: the Devices table shows the Devices list's rows; a row selects its symbol
/// on the page, a symbol selected on the page selects its row; a tag edited in the table is one
/// `setprop` transaction and patches one row (no rebuild), tinting the tag cell; a commit that
/// touches no device leaves the table untouched.
#[test]
fn devices_table_follows_the_page_and_edits_a_tag_in_one_transaction() {
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = build(trial(), Some(deltas.clone()));
    let (coil, page1, _page2) = devices_fixture(&mut h);
    let d = &h.state().panel.devices;
    let listed: Vec<_> = d.list.entries.iter().map(|e| e.id).collect();
    assert_eq!(d.table.view.visible_keys(), listed, "the same rows as the Devices list");
    assert_eq!(d.source.cell(coil, devices::PAGES).text(), "1-A-6, 2-A-6");

    // A row selects its first symbol on its page.
    click(&mut h, "Device -K01: Contactor / relay coil");
    assert_eq!(h.state().panel.devices.selected, Some(coil));
    assert_eq!(active(&mut h), Some(page1));
    let selected = h.state().cad.session.selection();
    assert!(!selected.is_empty(), "the coil is selected on the page");

    // The page's selection selects the row.
    ok(&mut h, r#"place {"symbol":"mcb1","at":[100,0]}"#);
    let q1 = doc(&mut h).components_tagged("-Q01")[0];
    h.state_mut().cad.session.set_selection(Vec::new());
    h.run_steps(2);
    h.state_mut().cad.session.set_selection(selected.clone());
    h.run_steps(3);
    assert_eq!(h.state().panel.devices.table.selection.keys().iter().copied().collect::<Vec<_>>(), [coil], "the symbol's row");

    // Edit the tag in the table: one transaction, one patched row, the tag cell tinted.
    let (builds, patches) = (h.state().panel.devices.table.view.builds, h.state().panel.devices.table.view.patches);
    let before = head(&mut h);
    h.state_mut().panel.devices.table.cursor_col = Some(devices::TAG);
    h.state_mut().panel.devices.table.selection.click(coil);
    h.state_mut().cad.session.cancel();
    edit(&mut h, "Edit Tag", "-K07");
    assert_eq!(head(&mut h), before + 1, "one transaction");
    assert_eq!(doc(&mut h).components_tagged("-K07"), [coil]);
    assert!(h.query_by_label("Device -K07: Contactor / relay coil").is_some(), "the row follows");
    let t = &h.state().panel.devices.table;
    assert_eq!(t.view.builds, builds, "no rebuild");
    assert_eq!(t.view.patches, patches + 1, "one row patched");
    let tinted = t.tint.tinted(h.ctx.input(|i| i.time));
    assert!(tinted.contains(&(coil, devices::TAG)), "{tinted:?}");
    assert!(tinted.iter().all(|(k, _)| *k == coil), "only the edited row: {tinted:?}");
    take(&h, &deltas, "039-devices-tint.png");

    // A commit that touches no device: nothing done.
    let patches = h.state().panel.devices.table.view.patches;
    ok(&mut h, r#"wire {"points":[[0,60],[40,60]]}"#);
    h.run_steps(3);
    let t = &h.state().panel.devices.table;
    assert_eq!((t.view.builds, t.view.patches), (builds, patches), "a wire touches no device row");
    let _ = q1;
}

/// Criterion 3: a refused tag edit shows its reason under the cell; the value stays.
#[test]
fn devices_table_shows_a_refusal_under_the_cell() {
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = build(trial(), Some(deltas.clone()));
    let (coil, ..) = devices_fixture(&mut h);
    ok(&mut h, r#"place {"symbol":"mcb1","at":[100,0]}"#);
    click(&mut h, "Device -K01: Contactor / relay coil");
    h.state_mut().panel.devices.table.cursor_col = Some(devices::TAG);
    h.state_mut().cad.session.cancel();
    let before = head(&mut h);
    edit(&mut h, "Edit Tag", "-Q01");
    assert_eq!(head(&mut h), before, "refused: nothing written");
    let refusal = h.state().panel.devices.table.refusal.clone().expect("the refusal is kept for the cell");
    assert_eq!((refusal.key, refusal.col), (coil, devices::TAG));
    assert!(h.query_all_by_label(&refusal.message).count() >= 1, "shown under the cell (and in the properties below): {}", refusal.message);
    assert!(h.query_by_label("Device -K01: Contactor / relay coil").is_some(), "the cell reverts");
    take(&h, &deltas, "039-devices-refusal.png");
}

/// Criterion 3: the Checks table shows the Checks tab's rows, in its order.
#[test]
fn checks_table_shows_the_checks_rows() {
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = build(trial(), Some(deltas.clone()));
    devices_fixture(&mut h);
    ok(&mut h, r#"wire {"points":[[0,60],[40,60]]}"#);
    click(&mut h, ecad_trial::checks_panel::CHECKS_TAB);
    h.run_steps(2);
    let c = &h.state().panel.checks;
    assert!(!c.rows().is_empty());
    let shown: Vec<String> = c.table.view.visible_keys().iter().map(|k| c.source.row_label(*k)).collect();
    let rows: Vec<String> = c.rows().iter().map(ecad_trial::checks_panel::row_label).collect();
    assert_eq!(shown, rows);
    // A commit that adds a check patches it in: no rebuild.
    let builds = c.table.view.builds;
    ok(&mut h, r#"wire {"points":[[0,90],[40,90]]}"#);
    h.run_steps(3);
    let c = &h.state().panel.checks;
    assert_eq!(c.table.view.builds, builds, "patched, not rebuilt");
    assert!(matches!(c.last_change, Some(Changed::Keys(_))), "{:?}", c.last_change);
    assert_eq!(c.table.view.visible_keys().len(), c.rows().len());
    take(&h, &deltas, "039-checks.png");
}

/// Criterion 5: Reports ▸ Terminals shows the strip's plan rows; only 036's editable fields are
/// inputs; inside columns right-aligned, outside left-aligned; a label edit is one `termlabel`
/// transaction, and patches only that strip's rows.
#[test]
fn terminals_table_edits_a_label_in_one_transaction() {
    let deltas: Deltas = std::sync::Arc::default();
    let mut h = build(trial(), Some(deltas.clone()));
    h.state_mut().cad.session.viewport_px = (1280.0, 800.0);
    ok(&mut h, r#"place {"symbol":"coil","at":[100,220]}"#);
    ok(&mut h, r#"place {"symbol":"coil","at":[150,220]}"#);
    ok(&mut h, r#"wire {"points":[[100,210],[100,65]]}"#);
    ok(&mut h, r#"wire {"points":[[150,210],[150,65]]}"#);
    ok(&mut h, r#"terminals {"from":[90,170],"to":[160,170]}"#);
    let x1 = doc(&mut h).components_tagged("-X01")[0];
    h.state_mut().show_plan(Some(x1));
    h.run_steps(4);
    let r = h.state().reports.as_ref().unwrap();
    assert_eq!(r.tab, outputs_ui::Tab::Terminals);
    let v = &r.terminals;
    let keys = v.table.view.visible_keys();
    assert_eq!(keys.len(), 2, "two drawn terminals, one row each");
    for c in terminals::columns() {
        let want = match c.id {
            terminals::IN_TARGET | terminals::IN_WIRE => Some(Align::Right),
            terminals::OUT_WIRE | terminals::OUT_TARGET | terminals::CABLE | terminals::CORE => Some(Align::Left),
            _ => None,
        };
        if let Some(a) = want {
            assert_eq!(c.align, a, "{}", c.title);
        }
        let set = matches!(c.id, terminals::POSITION | terminals::LABEL | terminals::RATING);
        assert_eq!(c.is_set(), set, "only 036's editable fields are inputs: {}", c.title);
    }
    let first = keys[0];
    let target = v.source.cell(first, terminals::IN_TARGET).text().into_owned();
    assert!(target.starts_with("-K0"), "the inside target is read from the wire: {target}");
    let label = format!("Terminal -X01:{}", v.source.cell(first, terminals::LABEL).text());
    let rebuilds = v.rebuilds;

    // Edit the first terminal's label: one transaction.
    h.get_by_label(&label).click();
    h.run_steps(2);
    h.state_mut().reports.as_mut().unwrap().terminals.table.cursor_col = Some(terminals::LABEL);
    let before = head(&mut h);
    edit(&mut h, "Edit Label", "L1");
    assert_eq!(head(&mut h), before + 1, "one transaction: {:?}", h.state().reports.as_ref().unwrap().terminals.table.refusal);
    let (label, _) = crate::project_panel::last_ops(&mut h);
    assert_eq!(label, "Terminal label", "termlabel's transaction");
    h.run_steps(3);
    let v = &h.state().reports.as_ref().unwrap().terminals;
    assert_eq!(v.rebuilds, rebuilds, "patched, not rebuilt");
    assert!(v.table.view.visible_keys().iter().any(|k| v.source.cell(*k, terminals::LABEL).text() == "L1"), "the row follows");
    take(&h, &deltas, "039-terminals.png");
}
