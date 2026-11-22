//! Brief 040 (U4a): the canvas in the app. The paper sheet and CADCraft's view switches at
//! 1920 × 1080, `ui.sheet` switching to the dark sheet and remembered, the painter reading the
//! project only when its key moves, the related placements of a selected device, nothing painted
//! while the page is hidden, and the first-open zoom.

use cadcraft_engine::doc::Handle;
use ecad_app::canvas::{self, Sheet};
use ecad_app::shell::app::App;
use ecad_app::shell::state::{ShellState, Workspace};
use ecad_model::{DeviceId, PageId, RepresentationId, Subject};
use eframe::egui::vec2;
use egui_kittest::Harness;
use serde_json::{Value, json};

use crate::common::{self, Opts};
use crate::pages::project_panel::{click, new_app};

fn harness() -> Harness<'static, App> {
    common::harness(Opts::new().size(1920.0, 1080.0).chrome(false).app(new_app()))
}

fn exec(h: &mut Harness<'static, App>, cmd: &str, params: Value) -> Value {
    let v = h.state_mut().cad.session.execute(cmd, &params).unwrap_or_else(|e| panic!("{cmd} {params}: {e}"));
    h.run_steps(2);
    v
}

fn hex_id<T: From<u128>>(v: &Value, key: &str) -> T {
    let s = v[key].as_str().unwrap_or_else(|| panic!("{key} in {v}"));
    let hex = s.rsplit('-').next().unwrap_or(s);
    T::from(u128::from_str_radix(hex, 16).unwrap())
}

fn reads(h: &Harness<'static, App>) -> u64 {
    canvas::with_state(&h.ctx, |s| s.reads)
}

/// Whether any text painted in the last frame contains `needle`.
fn painted_text(h: &Harness<'static, App>, needle: &str) -> bool {
    fn walk(s: &eframe::egui::Shape, needle: &str) -> bool {
        match s {
            eframe::egui::Shape::Text(t) => t.galley.text().contains(needle),
            eframe::egui::Shape::Vec(v) => v.iter().any(|s| walk(s, needle)),
            _ => false,
        }
    }
    h.output().shapes.iter().any(|c| walk(&c.shape, needle))
}

#[test]
fn the_page_is_paper_without_viewcube_ucs_icon_label_or_grid_and_dark_is_one_command_away() {
    let mut h = harness();
    let paper = ecad_widgets::color(ecad_drawing_style::sheet::PAPER.sheet);
    {
        let t = h.state();
        assert_eq!(t.cad.options.model_background, Some(paper), "#F6F4EF");
        assert_eq!(t.cad.options.selection_color, Some(ecad_widgets::color(ecad_drawing_style::sheet::PAPER.selection)));
        assert!(!t.cad.ui.show_viewcube && !t.cad.ui.show_ucs_icon, "no ViewCube, no UCS icon");
        assert!(!t.cad.session.settings.gridmode, "CADCraft's grid off: ECAD paints the dots");
    }
    assert!(!painted_text(&h, "2D Wireframe"), "no viewport label");
    // `ui.sheet dark` in the Text panel's Commands tab: the dark set, no restart, remembered.
    h.state_mut().shell.state.text_panel = true;
    h.run_steps(2);
    h.state_mut().command = "ui.sheet dark".into();
    h.run_steps(1);
    click(&mut h, "Run");
    h.run_steps(2);
    assert_eq!(h.state().shell_state().sheet, Sheet::Dark);
    assert_eq!(h.state().cad.options.model_background, Some(ecad_widgets::color(ecad_drawing_style::sheet::DARK.sheet)));
    let stored = h.state().shell_state().to_stored();
    assert_eq!(ShellState::from_stored(Some(&stored)).0.sheet, Sheet::Dark, "kept with the shell state");
    assert_eq!(ShellState::from_stored(Some("{}")).0.sheet, Sheet::Paper, "paper by default");
    // The developer switch brings CADCraft's own view furniture back.
    h.state_mut().set_cad_chrome(true);
    h.run_steps(3);
    assert!(h.state().cad.ui.show_viewcube && h.state().cad.ui.show_ucs_icon);
}

/// The coil `-K01` and two contacts on page 1, a third contact on page 2; back on page 1.
fn device_on_two_pages(h: &mut Harness<'static, App>) -> (DeviceId, PageId, PageId, Vec<Handle>) {
    let k: DeviceId = hex_id(&exec(h, "place", json!({"symbol": "coil", "at": [60, 200]})), "component");
    exec(h, "place", json!({"symbol": "no", "at": [160, 200], "device": k.hex()}));
    exec(h, "place", json!({"symbol": "nc", "at": [220, 200], "device": k.hex()}));
    let p1 = h.state().panel.active.expect("a page");
    let p2: PageId = hex_id(&exec(h, "newpage", json!({"title": "P2"})), "page");
    exec(h, "place", json!({"symbol": "no", "at": [160, 200], "device": k.hex()}));
    h.state_mut().show_page(p1).unwrap();
    h.run_steps(3);
    let handle = h.state().project().cloned().unwrap();
    let coil = handle
        .read(&mut h.state_mut().cad.session, |p, _| {
            let doc = p.ledger.document();
            let (rid, _) = doc.representations_of(Subject::Device(k)).find(|(_, r)| r.page == p1).unwrap();
            ecad_engine::reports::device_list::symbol_target(p, rid).unwrap().2
        })
        .unwrap();
    (k, p1, p2, coil)
}

#[test]
fn selecting_a_placement_outlines_the_others_here_and_lists_the_rest() {
    let mut h = harness();
    let (k, p1, p2, coil) = device_on_two_pages(&mut h);
    assert!(!coil.is_empty());
    h.state_mut().cad.session.set_selection(coil.clone());
    h.run_steps(3);
    let related = canvas::with_state(&h.ctx, |s| s.page.related.clone());
    assert_eq!(related.devices, vec![k]);
    assert_eq!(related.here.len(), 2, "both contacts on page 1 get an outline: {related:?}");
    assert_eq!(related.elsewhere.len(), 1, "the contact on page 2 is listed");
    assert_eq!(related.elsewhere[0].0, p2);
    // The function itself, as 042's inspector calls it.
    let handle = h.state().project().cloned().unwrap();
    let again = handle.read(&mut h.state_mut().cad.session, |p, _| canvas::related(p, p1, &coil)).unwrap();
    assert_eq!(again, related);
    let _: Option<RepresentationId> = again.elsewhere.first().map(|e| e.1);
}

#[test]
fn the_painter_reads_the_project_only_when_its_key_moves() {
    let mut h = harness();
    let (_, _, _, coil) = device_on_two_pages(&mut h);
    h.run_steps(3);
    let before = reads(&h);
    h.run_steps(300);
    assert_eq!(reads(&h), before, "300 frames, same change count and selection: no read");
    h.state_mut().cad.session.set_selection(coil);
    h.run_steps(3);
    assert_eq!(reads(&h), before + 1, "a new selection: one read");
    h.run_steps(50);
    assert_eq!(reads(&h), before + 1);
}

#[test]
fn nothing_is_painted_while_lists_library_or_publish_hide_the_page() {
    let mut h = harness();
    for w in [Workspace::Library, Workspace::Publish] {
        h.state_mut().shell.state.workspace = w;
        h.run_steps(3);
        assert!(h.state().shell.page_rect.is_none());
        let before = reads(&h);
        exec(&mut h, "place", json!({"symbol": "coil", "at": [60, 120]}));
        h.run_steps(3);
        assert_eq!(reads(&h), before, "{w:?}: the hidden page is not read");
    }
}

#[test]
fn a_new_page_first_opens_with_its_sheet_fitted() {
    // UI rev 3 replaced 040's readable first zoom (a tag `TAG_PX` tall) with the sheet fitted at
    // a 24 px margin; `uiv3_frame` measures the margin.
    let mut h = harness();
    exec(&mut h, "newpage", json!({"title": "Fresh"}));
    h.run_steps(5);
    let (Some(rect), Some(xf)) = (h.state().cad.canvas.rect, h.state().cad.canvas.xf) else { panic!("drawn") };
    assert!(rect.height() > 0.0);
    // An A3 sheet (420 mm) fits inside the canvas less two margins.
    let sheet_px = 420.0 * xf.scale;
    assert!(sheet_px <= f64::from(rect.width()) - 47.0, "{sheet_px} in {}", rect.width());
    assert!(2.5 * xf.scale < canvas::TAG_PX, "smaller than 040's readable zoom");
}

/// The demo page on paper and on the dark sheet, with a GNYE wire, a BU wire, an unknown code, a
/// loose end that looks connected and the coil of `-K01` selected, for the owner's check (written
/// only when `ECAD_TESTER_SHOTS` names a folder and a wgpu adapter exists; nothing is asserted).
/// It registers the sheet font, as the app does at start, so run it on its own.
#[test]
fn screenshots_of_the_canvas() {
    let Some(out) = crate::pages::project_panel::shots() else { return };
    canvas::register_sheet_faces();
    let dir = std::env::temp_dir().join(format!("ecad-040-shots-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = crate::checks::ui::demo_file(&dir);
    let deltas: crate::canvas::mouse_acceptance::Deltas = std::sync::Arc::default();
    let mut h = Harness::builder().with_size(vec2(1920.0, 1080.0)).renderer(crate::canvas::mouse_acceptance::Collect(deltas.clone())).build_ui_state(
        |ui, t: &mut App| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        new_app(),
    );
    h.run_steps(3);
    h.state_mut().close_project();
    h.run_steps(2);
    h.state_mut().open_project(&path).unwrap();
    h.run_steps(3);
    let first = crate::pages::project_panel::doc(&mut h).page_order()[0];
    h.state_mut().show_page(first).unwrap();
    h.run_steps(3);
    for (y, code) in [(130.0, "GNYE"), (120.0, "BU"), (110.0, "PURPLISH")] {
        let w = exec(&mut h, "wire", json!({"points": [[120.0, y], [200.0, y]]}));
        let c = w["conductor"].as_str().unwrap().to_string();
        exec(&mut h, "setprop", json!({"conductor": c, "key": "colours", "value": code}));
    }
    exec(&mut h, "wire", json!({"points": [[160.0, 145.0], [160.0, 130.0]], "join": false}));
    h.state_mut().zoom_to_drawing();
    h.run_steps(5);
    let _ = crate::canvas::mouse_acceptance::shot(&h, &deltas, &out.join("040-paper.png"));
    h.state_mut().sheet_command("ui.sheet dark");
    h.run_steps(5);
    let _ = crate::canvas::mouse_acceptance::shot(&h, &deltas, &out.join("040-dark.png"));
    h.state_mut().sheet_command("ui.sheet paper");
    h.run_steps(3);
    // A device's coil selected: its contacts on the page get dashed outlines.
    let handle = h.state().project().cloned().unwrap();
    let pick = handle
        .read(&mut h.state_mut().cad.session, |p, _| {
            let doc = p.ledger.document();
            let page = p.active_page()?;
            doc.tagged().find_map(|(_, c)| {
                let reps: Vec<_> = doc.representations_of(Subject::Device(c)).filter(|(_, r)| r.page == page).map(|(id, _)| id).collect();
                (reps.len() > 1).then(|| ecad_engine::reports::device_list::symbol_target(p, reps[0]).map(|t| t.2).unwrap_or_default())
            })
        })
        .unwrap()
        .unwrap_or_default();
    h.state_mut().cad.session.set_selection(pick);
    h.run_steps(5);
    let _ = crate::canvas::mouse_acceptance::shot(&h, &deltas, &out.join("040-related.png"));
    // The page at its first-open zoom.
    exec(&mut h, "newpage", json!({"title": "Fresh"}));
    exec(&mut h, "place", json!({"symbol": "coil", "at": [60, 200]}));
    h.run_steps(5);
    let _ = crate::canvas::mouse_acceptance::shot(&h, &deltas, &out.join("040-first-open.png"));
}
