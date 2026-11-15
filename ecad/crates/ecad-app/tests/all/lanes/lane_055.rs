//! Brief 055 (design 055 §12.1 H, I) in the trial, headless (egui_kittest): Library ▸ Title
//! blocks lists the built-in read only; a frame made from it opens in the editor; a palette row
//! dropped on the block is selected on whole millimetres with Value, Look and Box on the right;
//! Sample off; Save is one transaction after the consequence line; Discard asks only when the
//! draft changed. Screenshots (board 3.05 side by side, a page with the house block) are written
//! when `ECAD_TESTER_SHOTS` names a folder and a wgpu adapter exists (never committed).

use std::path::PathBuf;
use std::sync::Arc;

use ecad_app::library::title_blocks::{self, inspector, list};
use ecad_model::title_block::BlockItem;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::mouse_tester::{Deltas, harness_with, head, ok, shot};

type H = Harness<'static, ecad_app::App>;

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

fn fixture(deltas: Option<Deltas>) -> H {
    let mut h = harness_with(deltas);
    h.state_mut().set_cad_chrome(false);
    ok(
        &mut h,
        r#"projectprops {"title":"Pump station PS-04","drawing_number":"PS04-E-002","customer":"Northfield Water","status":"WIP","function":"PUMP","location":"CAB1"}"#,
    );
    let page = crate::mouse_tester::doc(&mut h).page_order[0].hex();
    ok(&mut h, &format!(r#"renamepage {{"page":"{page}","title":"Motor control"}}"#));
    h
}

#[test]
fn the_editor_drops_a_field_saves_once_and_keeps_built_ins_read_only() {
    let deltas: Deltas = Arc::default();
    let mut h = fixture(Some(deltas.clone()));
    if let Some(dir) = std::env::var_os("ECAD_TESTER_SHOTS").map(PathBuf::from) {
        ok(&mut h, "zoom e");
        h.run_steps(4);
        save(&h, &deltas, "055-page-house.png");
        let pdf = dir.join("055-printed.pdf");
        let _ = h.state_mut().cad.run("printbook", serde_json::json!({"path": pdf.to_string_lossy(), "colour": "colour"}));
    }
    h.state_mut().open_title_blocks(None);
    h.run_steps(4);
    assert!(h.query_by_label(title_blocks::CANVAS).is_some(), "the canvas is named (§5)");
    // The built-in is listed first and read only (I).
    let first = h.state().library.tb.rows.first().map(|r| r.id.clone());
    assert_eq!(first.as_deref(), Some("sheet"));
    assert!(h.state().library.tb.draft.as_ref().is_some_and(|d| d.builtin));
    assert!(h.query_by_label(inspector::SAVE).is_none(), "a built-in offers no Save");
    assert!(h.query_by_label(&format!("{}.a3", inspector::DUPLICATE)).is_some(), "but Duplicate for…");
    // Duplicate for A3: a new own frame opens.
    let n = head(&mut h);
    click(&mut h, &format!("{}.a3", inspector::DUPLICATE));
    h.run_steps(3);
    assert_eq!(head(&mut h), n + 1, "one transaction");
    let d = h.state().library.tb.draft.clone().expect("the copy is open");
    assert!(!d.builtin);
    let items = d.now.layout.items.len();
    // Discard is disabled while nothing changed; no consequence line.
    assert!(h.query_by_label(inspector::CONSEQUENCE).is_none());
    // Drop "Drawing number" at the next free place (double-click), on whole millimetres.
    h.state_mut().library.tb.canvas.drop_free = list::PALETTE.iter().position(|r| r.label == "Drawing number");
    h.run_steps(4);
    let d = h.state().library.tb.draft.clone().unwrap();
    assert_eq!(d.now.layout.items.len(), items + 1);
    let i = *d.selected.iter().next().expect("the dropped field is selected");
    match d.now.layout.items.get(i) {
        Some(BlockItem::Field { at, bind, .. }) => {
            assert_eq!(bind.text(), "project:drawing_number");
            assert!([at.x, at.y, at.w, at.h].iter().all(|v| v.fract() == 0.0), "{at:?}");
        }
        other => panic!("{other:?}"),
    }
    // The right panel: Value, Look, Box.
    for name in [inspector::SOURCE, inspector::IFEMPTY, inspector::SIZE, inspector::ALIGN, inspector::COLOUR, inspector::BOX[0], inspector::BOX[3]] {
        assert!(h.query_by_label(name).is_some(), "{name}");
    }
    // For the board: the block's own Drawing no. field selected.
    if let Some(d) = h.state_mut().library.tb.draft.as_mut()
        && let Some(j) = d.now.layout.items.iter().position(|x| matches!(x, BlockItem::Field { bind, .. } if bind.text() == "project:drawing_number"))
    {
        d.selected = [j].into();
    }
    h.run_steps(3);
    save(&h, &deltas, "055-editor.png");
    // Sample off shows the binding names.
    click(&mut h, title_blocks::SAMPLE);
    assert!(!h.state().library.tb.sample);
    save(&h, &deltas, "055-editor-sample-off.png");
    click(&mut h, title_blocks::SAMPLE);
    // Nothing selected: the consequence line, then Save, one transaction.
    h.state_mut().library.tb.draft.as_mut().unwrap().selected.clear();
    h.run_steps(3);
    assert!(h.query_by_label(inspector::CONSEQUENCE).is_some(), "the consequence line before Save");
    let n = head(&mut h);
    click(&mut h, inspector::SAVE);
    h.run_steps(3);
    assert_eq!(head(&mut h), n + 1, "Save is one transaction");
    assert!(!h.state().library.tb.draft.as_ref().unwrap().changed());
    // Discard asks only when the draft changed.
    h.state_mut().library.tb.draft.as_mut().unwrap().now.name = "A3 Northfield".into();
    h.run_steps(3);
    click(&mut h, inspector::DISCARD);
    assert!(h.state().library.tb.ask_discard);
}
