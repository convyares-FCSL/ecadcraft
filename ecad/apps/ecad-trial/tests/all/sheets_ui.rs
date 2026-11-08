//! Brief 019 in the trial (design 019 §12, checks C–F and J by the UI), headless with
//! egui_kittest in a 1280×800 window: the Pages tree with folders, the Properties section drawn
//! through CADCraft's seam, the Project Properties and Revisions dialogs, opening Rev A
//! read-only, and the question a plain `u` across an issue stops at. The reference screenshots
//! of design §12 are written to `$ECAD_TESTER_SHOTS` when it is set and a wgpu adapter exists:
//! `019-sheet.png`, `019-titleblock.png`, `019-tree.png`, `019-props-page.png`,
//! `019-props-device.png`, `019-projectinfo.png`, `019-projectsettings.png`, `019-revisions.png`,
//! `019-rev-open.png`.

use crate::project_panel::{Deltas, active, build, click, doc, fixture_p, head, ok, take, trial};

use ecad_model::{FolderId, Op};
use ecad_trial::project_panel::{self, Action};
use ecad_trial::{Trial, sheets_ui};
use eframe::egui::{self, Key};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn enter(h: &mut Harness<'static, Trial>) {
    h.key_press(Key::Enter);
    h.run_steps(3);
}

/// Type into the field labelled `label` (replacing its text) and press Enter.
fn type_into(h: &mut Harness<'static, Trial>, label: &str, text: &str) {
    let n = h.get_by_label(label);
    n.focus();
    h.run_steps(1);
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.get_by_label(label).type_text(text);
    h.run_steps(1);
    enter(h);
}

/// Type into the field that has the keyboard (a field opened in place takes it), replacing its
/// text, and press Enter.
fn type_focused(h: &mut Harness<'static, Trial>, text: &str) {
    h.event(egui::Event::Key {
        key: Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    });
    h.event(egui::Event::Text(text.into()));
    h.run_steps(1);
    enter(h);
}

fn last_label(h: &mut Harness<'static, Trial>) -> String {
    crate::project_panel::last_ops(h).0
}

/// The Properties palette is shown with the ECAD section on top of it.
fn show_properties(h: &mut Harness<'static, Trial>) {
    if h.state().props.lock().unwrap().frames == 0 {
        ok(h, "properties");
        h.run_steps(3);
    }
}

/// Checks C–F through the UI, with the reference screenshots.
#[test]
fn sheets_folders_properties_and_revisions_by_the_ui() {
    let deltas: Deltas = Default::default();
    let mut h = build(trial(), Some(deltas.clone()));
    let (supply, field, spare) = fixture_p(&mut h);
    click(&mut h, "Page 1: Supply");
    h.state_mut().cad.run("zoom", serde_json::json!({"mode": "extents"})).unwrap();
    h.run_steps(3);
    take(&h, &deltas, "019-sheet.png");

    // The Properties section, nothing selected: the page and the project.
    show_properties(&mut h);
    assert!(h.state().props.lock().unwrap().frames > 0, "the section is drawn through the seam");
    let view = h.state().props.lock().unwrap().view.clone();
    let titles: Vec<&str> = view.sections.iter().map(|s| s.title.as_str()).collect();
    // Project-level things are popouts of the Project panel, not part of Properties.
    assert_eq!(titles, ["ECAD · PAGE 1"]);
    // Number 100 by the panel's field: one transaction; the other end reads 100-B-6.
    let before = head(&mut h);
    type_into(&mut h, "ecad.props.page.number", "100");
    assert_eq!(head(&mut h), before + 1, "one transaction");
    assert_eq!(last_label(&mut h), "Page properties");
    let refs = ecad_cadcraft::references::markers_on_page(&doc(&mut h), field);
    assert_eq!(refs.first().map(|m| m.text.as_str()), Some("100-B-6"));
    // A refusal keeps the field and says why; nothing recorded.
    let before = head(&mut h);
    type_into(&mut h, "ecad.props.page.number", "2");
    assert_eq!(head(&mut h), before);
    let err = h.state().props.lock().unwrap().errors.get("number").cloned();
    assert_eq!(err.as_deref(), Some("Page 2 already exists (\"Field\")."));
    take(&h, &deltas, "019-props-page.png");
    // The section is rebuilt only when what it shows changed: a wire elsewhere leaves it.
    let builds = h.state().props_rebuilds;
    ok(&mut h, r#"wire {"points":[[0,100],[30,100]]}"#);
    h.run_steps(2);
    assert_eq!(h.state().props_rebuilds, builds, "a wire touches neither the page's nor the project's rows");
    // Nor the Pages tree (design 019 §9): no rebuild on an unrelated commit.
    let tree_builds = h.state().panel.rebuilds;
    ok(&mut h, r#"line {"points":[[0,110],[30,110]]}"#);
    h.run_steps(5);
    assert_eq!(h.state().panel.rebuilds, tree_builds, "a line does not rebuild the tree");

    // Project Properties: one transaction; Cancel writes nothing.
    click(&mut h, project_panel::PROJECT_INFO);
    assert!(h.state().project_props.is_some());
    {
        let d = h.state_mut().project_props.as_mut().unwrap();
        d.fields.insert("title".into(), "Panel 7 extension".into());
        d.fields.insert("drawing_number".into(), "FCS-1708-E-001".into());
        d.fields.insert("customer".into(), "ACME".into());
        d.fields.insert("status".into(), "Draft".into());
    }
    h.run_steps(2);
    take(&h, &deltas, "019-projectinfo.png");
    let before = head(&mut h);
    click(&mut h, "ecad.projectprops.ok");
    assert_eq!(head(&mut h), before + 1, "OK is one transaction");
    assert!(h.state().project_props.is_none());
    assert_eq!(doc(&mut h).properties.get("customer").map(String::as_str), Some("ACME"));
    click(&mut h, project_panel::PROJECT_INFO);
    click(&mut h, "ecad.projectprops.cancel");
    assert_eq!(head(&mut h), before + 1, "Cancel writes nothing");

    // Project settings: the reference style and the tag digits in one transaction; `u` undoes both.
    click(&mut h, project_panel::PROJECT_SETTINGS);
    {
        let d = h.state_mut().project_props.as_mut().unwrap();
        assert_eq!(d.popout, sheets_ui::Popout::Settings);
        d.style = "page-column".into();
        d.padding = "3".into();
    }
    h.run_steps(2);
    take(&h, &deltas, "019-projectsettings.png");
    let before = head(&mut h);
    click(&mut h, "ecad.projectprops.ok");
    assert_eq!(head(&mut h), before + 1, "one transaction");
    let d = doc(&mut h);
    assert_eq!((d.settings.reference_style, d.settings.tag_padding), (ecad_model::ReferenceStyle::PageColumn, 3));
    let refs = ecad_cadcraft::references::markers_on_page(&d, field);
    assert_eq!(refs.first().map(|m| m.text.as_str()), Some("/100.B"));
    ok(&mut h, "u");
    assert_eq!(doc(&mut h).settings.reference_style, ecad_model::ReferenceStyle::PageColumnRow);

    // Folders: New folder (renamed in place), a page into it, fold; one transaction each.
    let before = head(&mut h);
    click(&mut h, project_panel::NEW_FOLDER);
    assert_eq!(head(&mut h), before + 1);
    type_focused(&mut h, "Schematic layouts");
    assert_eq!(head(&mut h), before + 2, "the rename is its own transaction");
    let folder: FolderId = doc(&mut h).folder_order[0];
    h.state_mut().panel_action(Action::MoveInto { page: field, folder: Some(folder), index: None });
    h.run_steps(3);
    assert_eq!(head(&mut h), before + 3);
    assert!(h.query_by_label("Folder Schematic layouts: 1 page").is_some());
    h.state_mut().panel_action(Action::NewFolder(None));
    h.run_steps(2);
    type_focused(&mut h, "Terminal strips");
    h.run_steps(2);
    take(&h, &deltas, "019-tree.png");
    click(&mut h, "Folder Schematic layouts: 1 page");
    assert!(h.state().panel.folded.contains(&folder), "a click folds (nothing recorded)");
    assert!(h.query_by_label("Page 2: Field").is_none(), "folded away");
    click(&mut h, "Folder Schematic layouts: 1 page");
    // The tree's order: pages first, then folders.
    let rows: Vec<String> = h.state().panel.rows().iter().map(|r| r.title.clone()).collect();
    assert_eq!(rows, ["Supply", "Spare", "Field"]);
    let _ = spare;

    // A device selected: its section.
    click(&mut h, "Page 100: Supply");
    ok(&mut h, r#"place {"symbol":"coil","at":[200,100]}"#);
    let sym = {
        let t = h.state_mut();
        t.cad
            .session
            .doc()
            .unwrap()
            .model
            .iter()
            .filter(|e| e.common.layer != ecad_cadcraft::frame::LAYER)
            .last()
            .unwrap()
            .handle
    };
    let _ = sym;
    let coil_items: Vec<_> = {
        let d = doc(&mut h);
        let c = d.components_tagged("-K01")[0];
        d.representations_of(ecad_model::Subject::Component(c)).flat_map(|(_, r)| r.items.clone()).collect()
    };
    let handles: Vec<_> = {
        let t = h.state_mut();
        let p = t.project().unwrap().clone();
        p.read(&mut t.cad.session, |p, _| {
            let page = p.active_page().unwrap();
            coil_items.iter().filter_map(|i| p.pages[&page].map.item(*i)).collect::<Vec<_>>()
        })
        .unwrap()
    };
    h.state_mut().cad.session.set_selection(handles);
    h.run_steps(3);
    let view = h.state().props.lock().unwrap().view.clone();
    assert_eq!(view.sections.first().map(|s| s.title.as_str()), Some("ECAD · DEVICE -K01"));
    let before = head(&mut h);
    type_into(&mut h, "ecad.props.device.description", "Main contactor");
    assert_eq!(head(&mut h), before + 1);
    take(&h, &deltas, "019-props-device.png");
    h.state_mut().cad.session.set_selection(Vec::new());
    h.run_steps(2);

    // Revisions (the panel's Revisions popout): the pending row, Issue Rev A (asks first), the
    // title block, Reopen Rev A.
    let p = h.state().project().unwrap().clone();
    click(&mut h, project_panel::REVISIONS);
    {
        let r = h.state_mut().revisions.as_mut().unwrap();
        assert_eq!(r.pending[0], "A");
        assert!(!r.can_issue(), "Rev, Description and Drawn first");
    }
    let before = head(&mut h);
    type_into(&mut h, "ecad.revisions.pending.description", "First issue");
    type_into(&mut h, "ecad.revisions.pending.drawn", "CVA");
    assert_eq!(head(&mut h), before + 2, "one transaction per field");
    take(&h, &deltas, "019-revisions.png");
    click(&mut h, "ecad.revisions.issue");
    click(&mut h, "Issue");
    assert_eq!(head(&mut h), before + 3);
    let (label, ops) = crate::project_panel::last_ops(&mut h);
    assert_eq!(label, "Issue Rev A");
    assert!(ops.iter().any(|o| matches!(o, Op::NameVersion { name, .. } if name == "Rev A")));
    h.state_mut().revisions = None;
    // The title block, zoomed.
    let _ = h.state_mut().cad.run("zoom", serde_json::json!({"mode": "window", "p1": [-5, -60], "p2": [405, 20]}));
    h.run_steps(3);
    take(&h, &deltas, "019-titleblock.png");

    // A wire after the issue; Open Rev A shows the page without it, under the revision bar.
    ok(&mut h, r#"wire {"points":[[0,150],[30,150]]}"#);
    h.state_mut().open_revisions();
    h.run_steps(2);
    click(&mut h, "ecad.revisions.open.A");
    h.state_mut().revisions = None;
    h.run_steps(3);
    assert!(h.state().panel.revision.as_deref().is_some_and(|b| b.starts_with("Rev A")), "the bar");
    click(&mut h, "Page 100: Supply");
    let title = h.state().preview.as_ref().map(|v| v.title.clone()).unwrap_or_default();
    assert!(title.starts_with("Rev A · page 100: Supply"), "{title}");
    take(&h, &deltas, "019-rev-open.png");
    assert_eq!(active(&mut h), Some(supply), "the live page stays");
    click(&mut h, project_panel::BACK_TO_CURRENT);
    assert!(h.state().panel.revision.is_none());
    let _ = p;

    // A plain `u` across the issue stops and asks; Undo the issue removes the release mark.
    ok(&mut h, "u"); // the wire
    let before = head(&mut h);
    let _ = h.state_mut().submit_command("u");
    h.run_steps(3);
    assert_eq!(head(&mut h), before, "stopped");
    assert!(h.query_by_label("Undo the issue").is_some(), "the question");
    click(&mut h, "Undo the issue");
    assert!(!doc(&mut h).versions.contains_key("Rev A"));
    let _ = sheets_ui::ISSUE_UNDO_ID;
}

/// The demo project (`cargo xtask demo <file>`) on its A3 sheets, for the owner's look: run with
/// `ECAD_DEMO_FILE` set to the file and `ECAD_TESTER_SHOTS` to a folder (`019-demo-p1.png` …
/// `019-demo-p3.png`). Does nothing without `ECAD_DEMO_FILE`.
#[test]
fn the_demo_on_its_sheets() {
    let Some(file) = std::env::var_os("ECAD_DEMO_FILE") else { return };
    ecad_cadcraft::install();
    ecad_cadcraft::views::isolate_for_tests();
    let mut t = Trial::with_file(None, &file.to_string_lossy()).unwrap();
    t.dismiss_notice();
    let deltas: Deltas = Default::default();
    let mut h = build(t, Some(deltas.clone()));
    let pages = doc(&mut h).page_order.clone();
    for (i, p) in pages.iter().enumerate() {
        h.state_mut().show_page(*p).unwrap();
        h.state_mut().cad.run("zoom", serde_json::json!({"mode": "extents"})).unwrap();
        h.run_steps(4);
        take(&h, &deltas, &format!("019-demo-p{}.png", i + 1));
    }
}
